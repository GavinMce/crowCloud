use std::{sync::Arc, time::Duration};

use chrono::Utc;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Secret;
use kube::api::{Api, ObjectMeta, Patch, PatchParams};
use kube::ResourceExt;
use kube::{
    runtime::{
        controller::Action, finalizer, finalizer::Event as FinalizerEvent, watcher, Controller,
    },
    Client,
};
use sqlx::PgPool;
use uuid::Uuid;

use crow_core::{
    crd::resources::{Condition, K8sCluster, K8sClusterStatus},
    traits::{ProvisionCtx, ResourceDriver},
    types::ResourcePhase,
};
use crow_provider_registry::{
    resolve_ipam_provider_by_name, resolve_provider_by_id, resolve_provider_by_name, VM_NAMESPACE,
};
use crow_resource_k8s::K8sClusterDriver;

const FINALIZER: &str = "k8scluster.crow.cloud/finalizer";

#[derive(Debug, thiserror::Error)]
enum ReconcileError {
    #[error("resource row not found for id {0}")]
    RowMissing(Uuid),
    #[error("CR name {0:?} is not a valid `k8s-{{uuid}}` name")]
    BadCrName(String),
    #[error(transparent)]
    Driver(#[from] crow_core::DriverError),
    #[error(transparent)]
    Registry(#[from] crow_provider_registry::RegistryError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Kube(#[from] kube::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

struct Ctx {
    client: Client,
    db: PgPool,
    driver: K8sClusterDriver,
}

pub async fn run(client: Client, db: PgPool) -> anyhow::Result<()> {
    let api: Api<K8sCluster> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx {
        client,
        db,
        driver: K8sClusterDriver,
    });

    Controller::new(api, watcher::Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            match res {
                Ok(o) => tracing::debug!(?o, "reconciled"),
                Err(e) => {
                    let mut chain = e.to_string();
                    let mut source = std::error::Error::source(&e);
                    while let Some(s) = source {
                        chain.push_str(&format!(": {s}"));
                        source = s.source();
                    }
                    tracing::warn!(error = %chain, "reconcile failed");
                }
            }
        })
        .await;
    Ok(())
}

fn resource_id_from_cr_name(cr_name: &str) -> Result<Uuid, ReconcileError> {
    cr_name
        .strip_prefix("k8s-")
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ReconcileError::BadCrName(cr_name.to_string()))
}

async fn reconcile(
    cluster: Arc<K8sCluster>,
    ctx: Arc<Ctx>,
) -> Result<Action, finalizer::Error<ReconcileError>> {
    let api: Api<K8sCluster> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    finalizer(&api, FINALIZER, cluster, |event| async {
        match event {
            FinalizerEvent::Apply(cluster) => apply(&cluster, &ctx).await,
            FinalizerEvent::Cleanup(cluster) => cleanup(&cluster, &ctx).await,
        }
    })
    .await
}

fn error_policy(
    _cluster: Arc<K8sCluster>,
    _err: &finalizer::Error<ReconcileError>,
    _ctx: Arc<Ctx>,
) -> Action {
    Action::requeue(Duration::from_secs(30))
}

#[derive(sqlx::FromRow)]
struct ResourceRow {
    project: String,
    resource_group: String,
    phase: String,
    handle: Option<serde_json::Value>,
}

fn kubeconfig_secret_name(cr_name: &str) -> String {
    format!("{cr_name}-kubeconfig")
}

async fn apply(cluster: &K8sCluster, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = cluster.name_any();
    let resource_id = resource_id_from_cr_name(&name)?;

    let (_provider_id, infra) =
        resolve_provider_by_name(&ctx.db, &cluster.spec.infra_provider_ref.name).await?;

    let ipam = match &cluster.spec.ip_pool_ref {
        Some(ip_pool_ref) => Some(resolve_ipam_provider_by_name(&ctx.db, &ip_pool_ref.name).await?),
        None => None,
    };

    let row: Option<ResourceRow> = sqlx::query_as(
        "SELECT project, resource_group, phase, handle FROM resources WHERE id = $1",
    )
    .bind(resource_id)
    .fetch_optional(&ctx.db)
    .await?;
    let row = row.ok_or(ReconcileError::RowMissing(resource_id))?;

    let provision_ctx = ProvisionCtx {
        infra,
        network: None,
        dns: None,
        ipam,
        config: serde_json::json!({
            "distribution": cluster.spec.distribution,
            "version": cluster.spec.version,
            "image": cluster.spec.image,
            "vip": cluster.spec.control_plane.vip,
            "control_plane": {
                "count": cluster.spec.control_plane.count,
                "cpu": cluster.spec.control_plane.cpu,
                "memory_mib": (cluster.spec.control_plane.memory_gib as u64) * 1024,
                "disk_gib": cluster.spec.control_plane.disk_gib,
            },
            "workers": {
                "count": cluster.spec.workers.count,
                "cpu": cluster.spec.workers.cpu,
                "memory_mib": (cluster.spec.workers.memory_gib as u64) * 1024,
                "disk_gib": cluster.spec.workers.disk_gib,
            },
        }),
        project: row.project,
        resource_group: row.resource_group,
        resource_name: name.clone(),
    };

    let (new_phase, new_handle) =
        if let Some(handle_json) = row.handle.filter(|_| row.phase != "Pending") {
            let handle = serde_json::from_value(handle_json.clone())?;
            let phase = ctx.driver.reconcile(&provision_ctx, &handle).await?;
            (phase, handle_json)
        } else {
            let handle = ctx.driver.provision(&provision_ctx).await?;
            let handle_json = serde_json::to_value(&handle)?;
            (ResourcePhase::Ready, handle_json)
        };

    let handle = serde_json::from_value(new_handle.clone())?;
    let endpoint = ctx
        .driver
        .endpoints(&handle)
        .await?
        .into_iter()
        .next()
        .map(|e| e.url);

    let kubeconfig_secret = if matches!(new_phase, ResourcePhase::Ready) {
        match ctx.driver.credentials(&provision_ctx, &handle).await {
            Ok(creds) => {
                let secret_name = kubeconfig_secret_name(&name);
                write_kubeconfig_secret(ctx, &secret_name, &creds).await?;
                Some(secret_name)
            }
            Err(e) => {
                tracing::warn!(cluster = %name, "failed to fetch kubeconfig: {e}");
                None
            }
        }
    } else {
        None
    };

    let ready_status = if matches!(new_phase, ResourcePhase::Ready) {
        "True".to_string()
    } else {
        "False".to_string()
    };
    // Only bump `last_transition_time` on an actual transition — otherwise
    // every reconcile "changes" the status (even when nothing meaningful
    // did), which re-triggers the watch that drives reconciliation and
    // causes a self-sustaining reconcile storm (each pass re-runs live
    // provider calls, which starved a real k3s node's CPU during bring-up).
    let previous_ready_status = cluster
        .status
        .as_ref()
        .and_then(|s| s.conditions.iter().find(|c| c.condition_type == "Ready"));
    let last_transition_time = match previous_ready_status {
        Some(c) if c.status == ready_status => c.last_transition_time.clone(),
        _ => Some(Utc::now().to_rfc3339()),
    };

    let status = K8sClusterStatus {
        phase: Some(new_phase.to_string()),
        endpoint,
        kubeconfig_secret,
        node_count: Some(count_nodes(&new_handle)),
        conditions: vec![Condition {
            condition_type: "Ready".to_string(),
            status: ready_status,
            reason: Some(new_phase.to_string()),
            message: None,
            last_transition_time,
        }],
    };
    if cluster.status.as_ref() != Some(&status) {
        let api: Api<K8sCluster> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
        api.patch_status(
            &name,
            &PatchParams::default(),
            &Patch::Merge(serde_json::json!({ "status": status })),
        )
        .await?;
    }

    sqlx::query("UPDATE resources SET phase = $1, handle = $2, updated_at = NOW() WHERE id = $3")
        .bind(new_phase.to_string())
        .bind(&new_handle)
        .bind(resource_id)
        .execute(&ctx.db)
        .await?;

    Ok(Action::requeue(Duration::from_secs(120)))
}

fn count_nodes(handle_json: &serde_json::Value) -> u32 {
    let cp = handle_json
        .get("control_plane")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let workers = handle_json
        .get("workers")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    (cp + workers) as u32
}

async fn write_kubeconfig_secret(
    ctx: &Ctx,
    secret_name: &str,
    creds: &serde_json::Value,
) -> Result<(), ReconcileError> {
    let kubeconfig = creds
        .get("kubeconfig")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let secret = Secret {
        metadata: ObjectMeta {
            name: Some(secret_name.to_string()),
            namespace: Some(VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        string_data: Some(std::collections::BTreeMap::from([(
            "kubeconfig".to_string(),
            kubeconfig.to_string(),
        )])),
        ..Default::default()
    };

    let api: Api<Secret> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let pp = PatchParams::apply("crow-operator").force();
    api.patch(secret_name, &pp, &Patch::Apply(&secret)).await?;
    Ok(())
}

async fn cleanup(cluster: &K8sCluster, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = cluster.name_any();
    let resource_id = resource_id_from_cr_name(&name)?;

    #[derive(sqlx::FromRow)]
    struct CleanupRow {
        provider_id: Option<Uuid>,
        handle: Option<serde_json::Value>,
    }

    let row: Option<CleanupRow> =
        sqlx::query_as("SELECT provider_id, handle FROM resources WHERE id = $1")
            .bind(resource_id)
            .fetch_optional(&ctx.db)
            .await?;

    if let Some(CleanupRow {
        provider_id: Some(provider_id),
        handle: Some(handle_json),
    }) = row
    {
        let infra = resolve_provider_by_id(&ctx.db, provider_id).await?;
        let ipam = match &cluster.spec.ip_pool_ref {
            Some(ip_pool_ref) => {
                Some(resolve_ipam_provider_by_name(&ctx.db, &ip_pool_ref.name).await?)
            }
            None => None,
        };
        let handle = serde_json::from_value(handle_json)?;
        let provision_ctx = ProvisionCtx {
            infra,
            network: None,
            dns: None,
            ipam,
            config: serde_json::Value::Null,
            project: String::new(),
            resource_group: String::new(),
            resource_name: name.clone(),
        };
        ctx.driver.deprovision(&provision_ctx, &handle).await?;
    }

    let secret_api: Api<Secret> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let _ = secret_api
        .delete(&kubeconfig_secret_name(&name), &Default::default())
        .await;

    sqlx::query("DELETE FROM resources WHERE id = $1")
        .bind(resource_id)
        .execute(&ctx.db)
        .await?;

    Ok(Action::await_change())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_cr_name() {
        let id = Uuid::new_v4();
        let name = format!("k8s-{id}");
        assert_eq!(resource_id_from_cr_name(&name).unwrap(), id);
    }

    #[test]
    fn rejects_missing_prefix() {
        assert!(resource_id_from_cr_name(&Uuid::new_v4().to_string()).is_err());
    }

    #[test]
    fn rejects_malformed_uuid_suffix() {
        assert!(resource_id_from_cr_name("k8s-not-a-uuid").is_err());
    }

    #[test]
    fn counts_nodes_from_handle_json() {
        let handle = serde_json::json!({
            "control_plane": [{}, {}, {}],
            "workers": [{}],
        });
        assert_eq!(count_nodes(&handle), 4);
    }
}
