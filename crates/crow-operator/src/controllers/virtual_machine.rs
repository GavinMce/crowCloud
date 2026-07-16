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
        resources::{Condition, VirtualMachine, VirtualMachineStatus},
    },
    traits::{ProvisionCtx, ResourceDriver},
    types::ResourcePhase,
};
use crow_provider_registry::{
    ip_claim_cr_name, resolve_provider_by_id, resolve_provider_by_name, VM_NAMESPACE,
};
use crow_resource_vm::VirtualMachineDriver;

const FINALIZER: &str = "vm.crow.cloud/finalizer";

#[derive(Debug, thiserror::Error)]
enum ReconcileError {
    #[error("resource row not found for id {0}")]
    RowMissing(Uuid),
    #[error("CR name {0:?} is not a valid `vm-{{uuid}}` name")]
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
    driver: VirtualMachineDriver,
}

pub async fn run(client: Client, db: PgPool) -> anyhow::Result<()> {
    let api: Api<VirtualMachine> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx {
        client,
        db,
        driver: VirtualMachineDriver,
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
        .strip_prefix("vm-")
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ReconcileError::BadCrName(cr_name.to_string()))
}

async fn reconcile(
    vm: Arc<VirtualMachine>,
    ctx: Arc<Ctx>,
) -> Result<Action, finalizer::Error<ReconcileError>> {
    let api: Api<VirtualMachine> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    finalizer(&api, FINALIZER, vm, |event| async {
        match event {
            FinalizerEvent::Apply(vm) => apply(&vm, &ctx).await,
            FinalizerEvent::Cleanup(vm) => cleanup(&vm, &ctx).await,
        }
    })
    .await
}

fn error_policy(
    _vm: Arc<VirtualMachine>,
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

struct AllocatedNetwork {
    ip: IpAddr,
    prefix_len: u8,
    gateway: IpAddr,
    dns: Vec<String>,
}

/// Ensures an `IpClaim` exists for this VM and reports its allocation, if
/// bound. Returns `Ok(None)` when the claim exists but the `ip_claim`
/// controller hasn't allocated an address yet — the caller should wait
/// rather than provision, so cloud-init gets the real address on first boot.
async fn ensure_ip_claim_bound(
    ctx: &Ctx,
    resource_id: Uuid,
    vm_name: &str,
    pool_ref: &ResourceRef,
) -> Result<Option<AllocatedNetwork>, ReconcileError> {
    let claim_name = ip_claim_cr_name(resource_id);
    let claim_api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);

    if claim_api.get_opt(&claim_name).await?.is_none() {
        let claim = IpClaim {
            metadata: ObjectMeta {
                name: Some(claim_name.clone()),
                namespace: Some(VM_NAMESPACE.to_string()),
                ..Default::default()
            },
            spec: IpClaimSpec {
                pool_ref: pool_ref.clone(),
                resource_kind: "VirtualMachine".to_string(),
                resource_name: vm_name.to_string(),
            },
            status: None,
        };
        match claim_api.create(&PostParams::default(), &claim).await {
            Ok(_) => {}
            Err(kube::Error::Api(e)) if e.code == 409 => {}
            Err(e) => return Err(e.into()),
        }
    }

    let claim = claim_api.get(&claim_name).await?;
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
        ip,
        prefix_len,
        gateway,
        dns: pool.spec.dns.clone(),
    }))
}

async fn apply(vm: &VirtualMachine, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = vm.name_any();
    let resource_id = resource_id_from_cr_name(&name)?;

    let (provider_id, infra) =
        resolve_provider_by_name(&ctx.db, &vm.spec.infra_provider_ref.name, &vm.spec.node).await?;

    let row: Option<ResourceRow> =
        sqlx::query_as("SELECT project, phase, handle FROM resources WHERE id = $1")
            .bind(resource_id)
            .fetch_optional(&ctx.db)
            .await?;
    let row = row.ok_or(ReconcileError::RowMissing(resource_id))?;

    let network = match &vm.spec.ip_pool_ref {
        Some(pool_ref) => {
            match ensure_ip_claim_bound(ctx, resource_id, &name, pool_ref).await? {
                Some(net) => Some(net),
                // Claim exists but isn't bound yet — wait rather than
                // provision, so cloud-init gets the real address on first boot.
                None => return Ok(Action::requeue(Duration::from_secs(5))),
            }
        }
        None => None,
    };

    let mut config = serde_json::json!({
        "cpu": vm.spec.cpu,
        "memory_mib": (vm.spec.memory_gib as u64) * 1024,
        "disk_gib": vm.spec.disk_gib,
        "image": vm.spec.image,
    });
    if let Some(net) = &network {
        config["ip"] = serde_json::json!(net.ip);
        config["prefix_len"] = serde_json::json!(net.prefix_len);
        config["gateway"] = serde_json::json!(net.gateway);
        config["dns"] = serde_json::json!(net.dns);
    }

    let provision_ctx = ProvisionCtx {
        infra,
        network: None,
        dns: None,
        config,
        project: row.project,
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

    let vm_ip = new_handle
        .get("data")
        .and_then(|d| d.get("ip"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let status = VirtualMachineStatus {
        phase: Some(new_phase.to_string()),
        ip: vm_ip,
        provider_id: Some(provider_id.to_string()),
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
    let api: Api<VirtualMachine> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    api.patch_status(
        &name,
        &PatchParams::default(),
        &Patch::Merge(serde_json::json!({ "status": status })),
    )
    .await?;

    sqlx::query("UPDATE resources SET phase = $1, handle = $2, updated_at = NOW() WHERE id = $3")
        .bind(new_phase.to_string())
        .bind(&new_handle)
        .bind(resource_id)
        .execute(&ctx.db)
        .await?;

    Ok(Action::requeue(Duration::from_secs(120)))
}

async fn cleanup(vm: &VirtualMachine, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = vm.name_any();
    let resource_id = resource_id_from_cr_name(&name)?;

    if vm.spec.ip_pool_ref.is_some() {
        let claim_api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
        match claim_api
            .delete(&ip_claim_cr_name(resource_id), &DeleteParams::default())
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
        let infra = resolve_provider_by_id(&ctx.db, provider_id, &vm.spec.node).await?;
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
        let name = format!("vm-{id}");
        assert_eq!(resource_id_from_cr_name(&name).unwrap(), id);
    }

    #[test]
    fn rejects_missing_prefix() {
        assert!(resource_id_from_cr_name(&Uuid::new_v4().to_string()).is_err());
    }

    #[test]
    fn rejects_malformed_uuid_suffix() {
        assert!(resource_id_from_cr_name("vm-not-a-uuid").is_err());
    }
}
