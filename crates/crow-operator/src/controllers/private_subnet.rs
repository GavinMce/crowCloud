use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use kube::api::{Api, ListParams, ObjectMeta, Patch, PatchParams, PostParams};
use kube::{
    runtime::{
        controller::Action, finalizer, finalizer::Event as FinalizerEvent, watcher, Controller,
    },
    Client,
};
use kube::{Resource, ResourceExt};
use sqlx::PgPool;

use crow_core::{
    crd::networking::{
        IpClaim, IpPool, IpPoolSpec, IpPoolStatus, PrivateSubnet, PrivateSubnetStatus,
    },
    types::NetworkSpec,
};
use crow_provider_registry::{resolve_provider_by_name, VM_NAMESPACE};

const FINALIZER: &str = "privatesubnet.crow.cloud/finalizer";
const READY: &str = "Ready";
const FAILED: &str = "Failed";

#[derive(Debug, thiserror::Error)]
enum ReconcileError {
    #[error("subnet CIDR {0:?} has no /prefix suffix")]
    BadCidr(String),
    #[error("provider {0:?} could not create the bridge for this subnet's IpPool: still bound claims exist against it")]
    ClaimsStillBound(String),
    #[error("VNI {vni} is already used by PrivateSubnet {other:?}")]
    VniInUse { vni: u32, other: String },
    #[error(transparent)]
    Provider(#[from] crow_core::ProviderError),
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
}

pub async fn run(client: Client, db: PgPool) -> anyhow::Result<()> {
    let api: Api<PrivateSubnet> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx { client, db });

    Controller::new(api, watcher::Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            if let Err(e) = res {
                tracing::warn!(error = %e, "private subnet reconcile failed");
            }
        })
        .await;
    Ok(())
}

async fn reconcile(
    subnet: Arc<PrivateSubnet>,
    ctx: Arc<Ctx>,
) -> Result<Action, finalizer::Error<ReconcileError>> {
    let api: Api<PrivateSubnet> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    finalizer(&api, FINALIZER, subnet, |event| async {
        match event {
            FinalizerEvent::Apply(subnet) => apply(&subnet, &ctx).await,
            FinalizerEvent::Cleanup(subnet) => cleanup(&subnet, &ctx).await,
        }
    })
    .await
}

fn error_policy(
    _subnet: Arc<PrivateSubnet>,
    _err: &finalizer::Error<ReconcileError>,
    _ctx: Arc<Ctx>,
) -> Action {
    Action::requeue(Duration::from_secs(30))
}

fn ip_pool_cr_name(subnet_name: &str) -> String {
    format!("{subnet_name}-pool")
}

async fn apply(subnet: &PrivateSubnet, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let subnet_name = subnet.name_any();

    // Already provisioned — a bridge doesn't need re-creating on every
    // reconcile, and `create_network` isn't guaranteed idempotent against
    // a bridge that already exists under the same name.
    if subnet.status.as_ref().and_then(|s| s.phase.as_deref()) == Some(READY) {
        return Ok(Action::requeue(Duration::from_secs(300)));
    }

    // No central VNI allocator exists yet (mirrors `IpPoolSpec.cidr` being
    // explicit rather than drawn from a pool of pools) -- catch a
    // collision here instead of silently letting two subnets share a VNI
    // and fight over the same VXLAN/bridge interfaces on every node.
    let subnet_api: Api<PrivateSubnet> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let existing = subnet_api.list(&ListParams::default()).await?;
    if let Some(other) = vni_conflict(&existing.items, &subnet_name, subnet.spec.vni) {
        let status = PrivateSubnetStatus {
            phase: Some(FAILED.to_string()),
            bridge: None,
            ip_pool_ref: None,
            message: Some(format!(
                "VNI {} is already used by PrivateSubnet {other:?}",
                subnet.spec.vni
            )),
        };
        patch_status(ctx, &subnet_name, &status).await?;
        return Err(ReconcileError::VniInUse {
            vni: subnet.spec.vni,
            other: other.to_string(),
        });
    }

    let (_provider_id, provider) = resolve_provider_by_name(
        &ctx.db,
        &subnet.spec.infra_provider_ref.name,
        &subnet.spec.node,
    )
    .await?;

    let handle = provider
        .create_network(NetworkSpec {
            name: subnet_name.clone(),
            cidr: Some(subnet.spec.cidr.clone()),
            vni: subnet.spec.vni,
            gateway: Some(subnet.spec.gateway.clone()),
            snat: subnet.spec.snat,
        })
        .await?;

    let pool_name = ip_pool_cr_name(&subnet_name);
    ensure_ip_pool(ctx, subnet, &pool_name, &handle.provider_id).await?;

    let status = PrivateSubnetStatus {
        phase: Some(READY.to_string()),
        bridge: Some(handle.provider_id),
        ip_pool_ref: Some(pool_name),
        message: None,
    };
    patch_status(ctx, &subnet_name, &status).await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}

/// Creates the `IpPool` this subnet owns, if it doesn't already exist.
/// Owned via an owner reference so it cascades on subnet deletion instead
/// of being left pointing at a bridge that no longer exists.
async fn ensure_ip_pool(
    ctx: &Ctx,
    subnet: &PrivateSubnet,
    pool_name: &str,
    bridge: &str,
) -> Result<(), ReconcileError> {
    let pool_api: Api<IpPool> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    if pool_api.get_opt(pool_name).await?.is_some() {
        return Ok(());
    }

    let (range_start, range_end) = default_range(&subnet.spec.cidr, &subnet.spec.gateway)?;
    let owner_ref = subnet
        .controller_owner_ref(&())
        .expect("PrivateSubnet is a valid Kubernetes object with apiVersion/kind set");

    let pool = IpPool {
        metadata: ObjectMeta {
            name: Some(pool_name.to_string()),
            namespace: Some(VM_NAMESPACE.to_string()),
            owner_references: Some(vec![owner_ref]),
            ..Default::default()
        },
        spec: IpPoolSpec {
            cidr: subnet.spec.cidr.clone(),
            range_start,
            range_end,
            gateway: subnet.spec.gateway.clone(),
            dns: subnet.spec.dns.clone(),
            bridge: bridge.to_string(),
        },
        status: Some(IpPoolStatus::default()),
    };
    match pool_api.create(&PostParams::default(), &pool).await {
        Ok(_) => Ok(()),
        Err(kube::Error::Api(e)) if e.code == 409 => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Name of another `PrivateSubnet` in `existing` already claiming `vni`,
/// if any. Pure and self-contained so it's unit-testable without a real
/// kube API, same as `default_range` below.
fn vni_conflict<'a>(existing: &'a [PrivateSubnet], self_name: &str, vni: u32) -> Option<&'a str> {
    existing.iter().find_map(|other| {
        let other_name = other.metadata.name.as_deref()?;
        (other_name != self_name && other.spec.vni == vni).then_some(other_name)
    })
}

/// First and last usable host addresses in `cidr`, excluding the network
/// address, broadcast address, and `gateway`. IPv4 only, matching the
/// existing IPv4-only scope of `IpPool`/`IpClaim`.
fn default_range(cidr: &str, gateway: &str) -> Result<(String, String), ReconcileError> {
    use std::net::Ipv4Addr;

    let (addr, prefix) = cidr
        .split_once('/')
        .ok_or_else(|| ReconcileError::BadCidr(cidr.to_string()))?;
    let prefix: u32 = prefix
        .parse()
        .map_err(|_| ReconcileError::BadCidr(cidr.to_string()))?;
    let base: Ipv4Addr = addr
        .parse()
        .map_err(|_| ReconcileError::BadCidr(cidr.to_string()))?;
    let gateway: Ipv4Addr = gateway
        .parse()
        .map_err(|_| ReconcileError::BadCidr(gateway.to_string()))?;

    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    let network = u32::from(base) & mask;
    let broadcast = network | !mask;

    let first = (network + 1).max(u32::from(gateway) + 1).min(broadcast);
    let last = broadcast.saturating_sub(1);
    Ok((
        Ipv4Addr::from(first.min(last)).to_string(),
        Ipv4Addr::from(last).to_string(),
    ))
}

async fn cleanup(subnet: &PrivateSubnet, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let subnet_name = subnet.name_any();
    let Some(pool_name) = subnet
        .status
        .as_ref()
        .and_then(|s| s.ip_pool_ref.as_deref())
    else {
        // Never got far enough to create a pool or a bridge — nothing to
        // clean up on the provider side either.
        return Ok(Action::await_change());
    };

    if pool_has_bound_claims(ctx, pool_name).await? {
        // Refuse to tear down a bridge still serving live VM traffic —
        // same safety property `crow-api`'s IpPool delete route already
        // enforces at the API layer, applied here too since deletion can
        // also come in directly against the CR.
        return Err(ReconcileError::ClaimsStillBound(pool_name.to_string()));
    }

    if let Some(bridge) = subnet.status.as_ref().and_then(|s| s.bridge.clone()) {
        let (_provider_id, provider) = resolve_provider_by_name(
            &ctx.db,
            &subnet.spec.infra_provider_ref.name,
            &subnet.spec.node,
        )
        .await?;
        provider
            .delete_network(&crow_core::types::NetworkHandle {
                provider_type: provider.provider_type().to_string(),
                provider_id: bridge,
            })
            .await?;
    }

    let pool_api: Api<IpPool> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    match pool_api
        .delete(pool_name, &kube::api::DeleteParams::default())
        .await
    {
        Ok(_) => {}
        Err(kube::Error::Api(e)) if e.code == 404 => {}
        Err(e) => return Err(e.into()),
    }

    tracing::info!(subnet = %subnet_name, "private subnet and its bridge deleted");
    Ok(Action::await_change())
}

async fn pool_has_bound_claims(ctx: &Ctx, pool_name: &str) -> Result<bool, ReconcileError> {
    let claim_api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let claims = claim_api.list(&ListParams::default()).await?;
    Ok(claims.items.iter().any(|c| {
        c.spec.pool_ref.name == pool_name
            && c.status.as_ref().and_then(|s| s.phase.as_deref()) == Some("Bound")
    }))
}

async fn patch_status(
    ctx: &Ctx,
    subnet_name: &str,
    status: &PrivateSubnetStatus,
) -> Result<(), ReconcileError> {
    let api: Api<PrivateSubnet> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    api.patch_status(
        subnet_name,
        &PatchParams::default(),
        &Patch::Merge(serde_json::json!({ "status": status })),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subnet(name: &str, vni: u32) -> PrivateSubnet {
        PrivateSubnet {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: crow_core::crd::networking::PrivateSubnetSpec {
                infra_provider_ref: crow_core::crd::common::ResourceRef {
                    name: "pve".to_string(),
                    namespace: None,
                },
                node: "pve1".to_string(),
                cidr: "10.30.0.0/24".to_string(),
                vni,
                gateway: "10.30.0.1".to_string(),
                dns: vec![],
                snat: false,
            },
            status: None,
        }
    }

    #[test]
    fn vni_conflict_finds_another_subnet_claiming_the_same_vni() {
        let existing = vec![subnet("tenant-a", 100), subnet("tenant-b", 200)];
        assert_eq!(vni_conflict(&existing, "tenant-c", 200), Some("tenant-b"));
    }

    #[test]
    fn vni_conflict_ignores_the_subnet_being_reconciled_itself() {
        // A subnet reconciling repeatedly (e.g. requeued before reaching
        // Ready) must not conflict with its own prior listing.
        let existing = vec![subnet("tenant-a", 100)];
        assert_eq!(vni_conflict(&existing, "tenant-a", 100), None);
    }

    #[test]
    fn vni_conflict_is_none_when_every_vni_is_distinct() {
        let existing = vec![subnet("tenant-a", 100), subnet("tenant-b", 200)];
        assert_eq!(vni_conflict(&existing, "tenant-c", 300), None);
    }

    #[test]
    fn default_range_excludes_network_broadcast_and_gateway() {
        let (start, end) = default_range("10.30.0.0/24", "10.30.0.1").unwrap();
        assert_eq!(start, "10.30.0.2");
        assert_eq!(end, "10.30.0.254");
    }

    #[test]
    fn default_range_skips_past_a_gateway_thats_not_the_first_address() {
        // Some deployments put the gateway higher in the range rather than
        // at .1 -- the allocatable range must start after it, not before.
        let (start, _end) = default_range("10.30.0.0/24", "10.30.0.5").unwrap();
        assert_eq!(start, "10.30.0.6");
    }

    #[test]
    fn default_range_rejects_a_cidr_with_no_prefix() {
        assert!(matches!(
            default_range("10.30.0.0", "10.30.0.1"),
            Err(ReconcileError::BadCidr(_))
        ));
    }
}
