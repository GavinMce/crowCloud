use std::{net::IpAddr, sync::Arc, time::Duration};

use chrono::Utc;
use futures::StreamExt;
use kube::api::{Api, DeleteParams, ObjectMeta, Patch, PatchParams, PostParams};
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
    crd::{
        common::ResourceRef,
        networking::{IpClaim, IpClaimSpec, IpPool},
        resources::{Condition, K8sCluster, K8sClusterStatus},
    },
    traits::{ProvisionCtx, ResourceDriver},
    types::{K8sClusterHandle, ResourcePhase},
};
use crow_provider_registry::{
    k8s_cluster_ip_claim_cr_name, k8s_cluster_worker_ip_claim_cr_name, resolve_provider_by_id,
    resolve_provider_by_name, VM_NAMESPACE,
};
use crow_resource_k8s::K8sClusterDriver;

const FINALIZER: &str = "k8scluster.crow.cloud/finalizer";

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReconcileError {
    #[error("resource row not found for id {0}")]
    RowMissing(Uuid),
    #[error("CR name {0:?} is not a valid `k8s-{{uuid}}` name")]
    BadCrName(String),
    #[error("ip pool {0:?} referenced by ip_pool_ref not found")]
    PoolMissing(String),
    #[error("malformed IPv4 address or CIDR {0:?} from IpClaim/IpPool")]
    BadAddress(String),
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
    /// crow-api's own reachable base URL (e.g. `https://crow-api.example`),
    /// so the control plane's cloud-init script knows where to POST its
    /// bootstrap-complete callback. Required — a cluster genuinely can't
    /// finish provisioning without it, so this crashes at startup rather
    /// than failing confusingly per-cluster later.
    crow_api_url: String,
}

pub async fn run(client: Client, db: PgPool) -> anyhow::Result<()> {
    let crow_api_url = std::env::var("CROW_API_URL")
        .expect("CROW_API_URL must be set for crow-operator (K8sCluster bootstrap callback)");
    let api: Api<K8sCluster> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx {
        client,
        db,
        driver: K8sClusterDriver,
        crow_api_url,
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
    phase: String,
    handle: Option<serde_json::Value>,
}

struct StaticAddress {
    ip: IpAddr,
    prefix_len: u8,
    gateway: IpAddr,
    dns: Vec<String>,
}

struct AllocatedNetwork {
    bridge: String,
    address: StaticAddress,
}

/// Ensures an `IpClaim` exists (for the control plane or one worker — same
/// mechanism, just a different name) and reports its allocation, if bound.
/// Every K8sCluster node requests a static address, no DHCP mode — DHCP on
/// the target network proved unreliable in practice (new MAC addresses
/// observed getting no lease at all), and crowCloud already owns the pool
/// capacity to just not depend on it.
async fn ensure_claim_bound(
    ctx: &Ctx,
    claim_name: &str,
    cluster_name: &str,
    pool_ref: &ResourceRef,
) -> Result<Option<AllocatedNetwork>, ReconcileError> {
    let claim_api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);

    if claim_api.get_opt(claim_name).await?.is_none() {
        let claim = IpClaim {
            metadata: ObjectMeta {
                name: Some(claim_name.to_string()),
                namespace: Some(VM_NAMESPACE.to_string()),
                ..Default::default()
            },
            spec: IpClaimSpec {
                pool_ref: pool_ref.clone(),
                resource_kind: "K8sCluster".to_string(),
                resource_name: cluster_name.to_string(),
                requested_ip: None,
            },
            status: None,
        };
        match claim_api.create(&PostParams::default(), &claim).await {
            Ok(_) => {}
            Err(kube::Error::Api(e)) if e.code == 409 => {}
            Err(e) => return Err(e.into()),
        }
    }

    let claim = claim_api.get(claim_name).await?;
    let Some(allocated_ip) = claim
        .status
        .as_ref()
        .filter(|s| s.phase.as_deref() == Some("Bound"))
        .and_then(|s| s.allocated_ip.as_deref())
    else {
        return Ok(None);
    };

    let pool_api: Api<IpPool> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let pool = pool_api
        .get_opt(&pool_ref.name)
        .await?
        .ok_or_else(|| ReconcileError::PoolMissing(pool_ref.name.clone()))?;

    let ip: IpAddr = allocated_ip
        .parse()
        .map_err(|_| ReconcileError::BadAddress(allocated_ip.to_string()))?;
    let gateway: IpAddr = pool
        .spec
        .gateway
        .parse()
        .map_err(|_| ReconcileError::BadAddress(pool.spec.gateway.clone()))?;
    let prefix_len: u8 = pool
        .spec
        .cidr
        .rsplit('/')
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ReconcileError::BadAddress(pool.spec.cidr.clone()))?;

    Ok(Some(AllocatedNetwork {
        bridge: pool.spec.bridge.clone(),
        address: StaticAddress {
            ip,
            prefix_len,
            gateway,
            dns: pool.spec.dns.clone(),
        },
    }))
}

async fn apply(cluster: &K8sCluster, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = cluster.name_any();
    let resource_id = resource_id_from_cr_name(&name)?;

    let (_provider_id, infra) = resolve_provider_by_name(
        &ctx.db,
        &cluster.spec.infra_provider_ref.name,
        &cluster.spec.node,
    )
    .await?;

    // Not part of `InfraProvider` (that's an opaque trait object) — fetched
    // separately so the driver can inject it into cloud-init for post-
    // failure debugging. Absent for most hosts; only worth the extra query
    // since it's cheap and this path doesn't run per-tick once Ready.
    let debug_ssh_public_key: Option<String> =
        sqlx::query_scalar("SELECT config->>'ssh_public_key' FROM providers WHERE name = $1")
            .bind(&cluster.spec.infra_provider_ref.name)
            .fetch_optional(&ctx.db)
            .await?
            .flatten();

    let row: Option<ResourceRow> =
        sqlx::query_as("SELECT project, phase, handle FROM resources WHERE id = $1")
            .bind(resource_id)
            .fetch_optional(&ctx.db)
            .await?;
    let row = row.ok_or(ReconcileError::RowMissing(resource_id))?;

    // Tracks whether `new_handle` is a freshly-produced value that actually
    // needs writing back. The reconcile-only branch never modifies the
    // handle — it only derives a phase from whatever's already there — but
    // `crow-api`'s bootstrap-callback route (`k8s_bootstrap::report`) writes
    // straight to `resources.handle` independently, at any time, outside
    // this reconcile loop entirely. Live-tested: writing the handle back
    // unconditionally here — even byte-for-byte unchanged — created a race
    // where an in-flight reconcile (holding the pre-callback handle in
    // memory while it made slow Proxmox/K8s-API calls) would finish *after*
    // the callback landed and clobber the just-written kubeconfig back to
    // null, hanging the cluster in `Bootstrapping` forever.
    let (new_phase, new_handle, handle_changed) = if let Some(handle_json) =
        row.handle.clone().filter(|_| row.phase != "Pending")
    {
        // Already provisioned (or provisioning failed after creating some
        // nodes) — reconcile() just checks status, it doesn't touch infra.
        let handle = serde_json::from_value(handle_json.clone())?;
        let provision_ctx = ProvisionCtx {
            infra,
            network: None,
            dns: None,
            config: serde_json::Value::Null,
            project: row.project.clone(),
            resource_name: name.clone(),
        };
        let phase = ctx.driver.reconcile(&provision_ctx, &handle).await?;
        (phase, handle_json, false)
    } else {
        let cp_claim_name = k8s_cluster_ip_claim_cr_name(resource_id);
        let net = match ensure_claim_bound(ctx, &cp_claim_name, &name, &cluster.spec.ip_pool_ref)
            .await?
        {
            Some(net) => net,
            // Claim exists but isn't bound yet — wait rather than create any
            // VMs, so the control plane's cloud-init gets its real address.
            None => return Ok(Action::requeue(Duration::from_secs(5))),
        };

        // Workers get static addresses too, same pool, same reasoning as
        // the control plane — see `ensure_claim_bound`'s doc comment.
        // All claims must be bound before creating *any* VM, so a
        // partway-exhausted pool fails fast instead of leaving a
        // half-built cluster with some DHCP-less, unaddressable workers.
        let mut worker_nets = Vec::with_capacity(cluster.spec.workers.count as usize);
        for i in 0..cluster.spec.workers.count {
            let claim_name = k8s_cluster_worker_ip_claim_cr_name(resource_id, i);
            match ensure_claim_bound(ctx, &claim_name, &name, &cluster.spec.ip_pool_ref).await? {
                Some(net) => worker_nets.push(net),
                None => return Ok(Action::requeue(Duration::from_secs(5))),
            }
        }

        let config = serde_json::json!({
            "image": cluster.spec.image,
            "k3s_version": cluster.spec.version,
            "control_plane": {
                "ip": net.address.ip,
                "prefix_len": net.address.prefix_len,
                "gateway": net.address.gateway,
                "dns": net.address.dns,
                "bridge": net.bridge,
            },
            "control_plane_cpu": cluster.spec.control_plane.cpu,
            "control_plane_memory_gib": cluster.spec.control_plane.memory_gib,
            "control_plane_disk_gib": cluster.spec.control_plane.disk_gib,
            "workers": worker_nets.iter().map(|w| serde_json::json!({
                "ip": w.address.ip,
                "prefix_len": w.address.prefix_len,
                "gateway": w.address.gateway,
                "dns": w.address.dns,
                "bridge": w.bridge,
            })).collect::<Vec<_>>(),
            "worker_cpu": cluster.spec.workers.cpu,
            "worker_memory_gib": cluster.spec.workers.memory_gib,
            "worker_disk_gib": cluster.spec.workers.disk_gib,
            "pod_cidr": cluster.spec.network.pod_cidr,
            "service_cidr": cluster.spec.network.service_cidr,
            "lb_pool_cidr": cluster.spec.network.lb_pool,
            "monitoring": cluster.spec.monitoring,
            "callback_url": format!(
                "{}/api/v1/internal/k8s-clusters/{resource_id}/report",
                ctx.crow_api_url.trim_end_matches('/')
            ),
            "cluster_token": cluster.spec.cluster_token,
            "debug_ssh_public_key": debug_ssh_public_key,
            "bootstrap_secret": cluster.spec.bootstrap_secret,
        });

        let provision_ctx = ProvisionCtx {
            infra,
            network: None,
            dns: None,
            config,
            project: row.project.clone(),
            resource_name: name.clone(),
        };
        let handle = ctx.driver.provision(&provision_ctx).await?;
        let handle_json = serde_json::to_value(&handle)?;
        // Not Ready — provisioning the VMs just means the cluster is now
        // bootstrapping, not that it's usable yet. reconcile() (next tick)
        // decides Ready/Failed based on whether the callback arrived.
        (ResourcePhase::Bootstrapping, handle_json, true)
    };

    // `new_handle` is the outer `ResourceHandle{resource_type, data}`
    // envelope, not the `K8sClusterHandle` itself — unwrap `.data` first.
    // Matches `virtual_machine.rs`'s identical `new_handle.get("data")...`
    // read; `resources.handle` always carries this envelope, since
    // `ResourceDriver::provision()`'s own return type is `ResourceHandle`.
    let cluster_handle: Option<K8sClusterHandle> = new_handle
        .get("data")
        .cloned()
        .and_then(|data| serde_json::from_value(data).ok());
    let endpoint = cluster_handle
        .as_ref()
        .and_then(|h| h.control_plane.ip)
        .map(|ip| format!("https://{ip}:6443"));
    let node_count = cluster_handle.as_ref().map(|h| 1 + h.workers.len() as u32);

    let status = K8sClusterStatus {
        phase: Some(new_phase.to_string()),
        endpoint,
        // v1 stores the kubeconfig directly in the resource handle
        // (Postgres), not as a separate Kubernetes Secret — this field is
        // unused for now; crow-api's kubeconfig download route reads the
        // handle directly instead.
        kubeconfig_secret: None,
        node_count,
        conditions: vec![Condition {
            condition_type: "Ready".to_string(),
            status: if matches!(new_phase, ResourcePhase::Ready) {
                "True".to_string()
            } else {
                "False".to_string()
            },
            reason: Some(new_phase.to_string()),
            message: None,
            last_transition_time: Some(Utc::now().to_rfc3339()),
        }],
    };
    let api: Api<K8sCluster> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    api.patch_status(
        &name,
        &PatchParams::default(),
        &Patch::Merge(serde_json::json!({ "status": status })),
    )
    .await?;

    if handle_changed {
        sqlx::query(
            "UPDATE resources SET phase = $1, handle = $2, updated_at = NOW() WHERE id = $3",
        )
        .bind(new_phase.to_string())
        .bind(&new_handle)
        .bind(resource_id)
        .execute(&ctx.db)
        .await?;
    } else {
        sqlx::query("UPDATE resources SET phase = $1, updated_at = NOW() WHERE id = $2")
            .bind(new_phase.to_string())
            .bind(resource_id)
            .execute(&ctx.db)
            .await?;
    }

    // Bootstrapping/Ready both want to keep checking in — Bootstrapping to
    // catch the callback (or time out), Ready to notice if it later fails.
    let requeue_secs = if matches!(new_phase, ResourcePhase::Bootstrapping) {
        20
    } else {
        120
    };
    Ok(Action::requeue(Duration::from_secs(requeue_secs)))
}

async fn cleanup(cluster: &K8sCluster, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = cluster.name_any();
    let resource_id = resource_id_from_cr_name(&name)?;

    let claim_api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let mut claim_names = vec![k8s_cluster_ip_claim_cr_name(resource_id)];
    claim_names.extend(
        (0..cluster.spec.workers.count)
            .map(|i| k8s_cluster_worker_ip_claim_cr_name(resource_id, i)),
    );
    for claim_name in claim_names {
        match claim_api
            .delete(&claim_name, &DeleteParams::default())
            .await
        {
            Ok(_) => {}
            Err(kube::Error::Api(e)) if e.code == 404 => {}
            Err(e) => return Err(e.into()),
        }
    }

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
        let infra = resolve_provider_by_id(&ctx.db, provider_id, &cluster.spec.node).await?;
        let handle = serde_json::from_value(handle_json)?;
        let provision_ctx = ProvisionCtx {
            infra,
            network: None,
            dns: None,
            config: serde_json::Value::Null,
            project: String::new(),
            resource_name: name.clone(),
        };
        ctx.driver.deprovision(&provision_ctx, &handle).await?;
    }

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
}
