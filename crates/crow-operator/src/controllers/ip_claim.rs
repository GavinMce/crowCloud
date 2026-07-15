use std::{collections::HashSet, net::Ipv4Addr, sync::Arc, time::Duration};

use futures::StreamExt;
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::ResourceExt;
use kube::{
    runtime::{
        controller::Action, finalizer, finalizer::Event as FinalizerEvent, watcher, Controller,
    },
    Client,
};

use crow_core::crd::networking::{IpClaim, IpClaimStatus, IpPool, IpPoolSpec, IpPoolStatus};
use crow_provider_registry::VM_NAMESPACE;

const FINALIZER: &str = "ipclaim.crow.cloud/finalizer";
const BOUND: &str = "Bound";
const PENDING: &str = "Pending";

#[derive(Debug, thiserror::Error)]
enum ReconcileError {
    #[error("pool {0:?} referenced by claim not found")]
    PoolNotFound(String),
    #[error("invalid IPv4 address {0:?} in pool spec")]
    BadAddress(String),
    #[error(transparent)]
    Kube(#[from] kube::Error),
}

struct Ctx {
    client: Client,
}

pub async fn run(client: Client) -> anyhow::Result<()> {
    let api: Api<IpClaim> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx { client });

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

async fn reconcile(
    claim: Arc<IpClaim>,
    ctx: Arc<Ctx>,
) -> Result<Action, finalizer::Error<ReconcileError>> {
    let api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    finalizer(&api, FINALIZER, claim, |event| async {
        match event {
            FinalizerEvent::Apply(claim) => apply(&claim, &ctx).await,
            FinalizerEvent::Cleanup(claim) => cleanup(&claim, &ctx).await,
        }
    })
    .await
}

fn error_policy(
    _claim: Arc<IpClaim>,
    _err: &finalizer::Error<ReconcileError>,
    _ctx: Arc<Ctx>,
) -> Action {
    Action::requeue(Duration::from_secs(30))
}

async fn apply(claim: &IpClaim, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let claim_name = claim.name_any();
    let pool_name = claim.spec.pool_ref.name.clone();

    // Already allocated — keep the same address across operator restarts and
    // spurious re-applies instead of re-running the allocation scan.
    if claim.status.as_ref().and_then(|s| s.phase.as_deref()) == Some(BOUND) {
        return Ok(Action::requeue(Duration::from_secs(120)));
    }

    let pool_api: Api<IpPool> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let pool = pool_api
        .get_opt(&pool_name)
        .await?
        .ok_or_else(|| ReconcileError::PoolNotFound(pool_name.clone()))?;

    let used = bound_addresses(ctx, &pool_name, &claim_name).await?;
    let allocated = allocate_ip(&pool.spec, &used)?;

    let (phase, action) = match allocated {
        Some(_) => (BOUND, Action::requeue(Duration::from_secs(120))),
        // Pool exhausted — a recoverable condition (someone frees an address
        // or resizes the pool), not an error. Retry soonish.
        None => (PENDING, Action::requeue(Duration::from_secs(30))),
    };

    let status = IpClaimStatus {
        allocated_ip: allocated.map(|ip| ip.to_string()),
        phase: Some(phase.to_string()),
    };
    let claim_api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    claim_api
        .patch_status(
            &claim_name,
            &PatchParams::default(),
            &Patch::Merge(serde_json::json!({ "status": status })),
        )
        .await?;

    recompute_pool_status(ctx, &pool_name, None).await?;

    Ok(action)
}

async fn cleanup(claim: &IpClaim, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let claim_name = claim.name_any();
    // No provider-side address is actually "owned" by anything external, so
    // there is nothing to release beyond letting this claim disappear — just
    // bring the pool's counters back in sync immediately.
    recompute_pool_status(ctx, &claim.spec.pool_ref.name, Some(&claim_name)).await?;
    Ok(Action::await_change())
}

/// IPv4 addresses currently held by other `Bound` claims against `pool_name`,
/// used to build the pool's status counters — recomputed from a fresh scan
/// each time rather than incremented/decremented, since pool sizes for
/// self-hosted use (tens to low hundreds of addresses) make an O(n) scan
/// simple and correct with no separate bitmap to keep consistent.
async fn bound_addresses(
    ctx: &Ctx,
    pool_name: &str,
    exclude_claim: &str,
) -> Result<HashSet<Ipv4Addr>, ReconcileError> {
    let claim_api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let claims = claim_api.list(&ListParams::default()).await?;
    Ok(claims
        .items
        .into_iter()
        .filter(|c| c.spec.pool_ref.name == pool_name && c.name_any() != exclude_claim)
        .filter_map(|c| {
            c.status
                .filter(|s| s.phase.as_deref() == Some(BOUND))
                .and_then(|s| s.allocated_ip)
        })
        .filter_map(|ip| ip.parse::<Ipv4Addr>().ok())
        .collect())
}

async fn recompute_pool_status(
    ctx: &Ctx,
    pool_name: &str,
    exclude_claim: Option<&str>,
) -> Result<(), ReconcileError> {
    let pool_api: Api<IpPool> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let Some(pool) = pool_api.get_opt(pool_name).await? else {
        // Pool was deleted out from under its claims — nothing left to update.
        return Ok(());
    };

    let claim_api: Api<IpClaim> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let claims = claim_api.list(&ListParams::default()).await?;
    let allocated = claims
        .items
        .iter()
        .filter(|c| {
            c.spec.pool_ref.name == pool_name
                && exclude_claim != Some(c.name_any().as_str())
                && c.status.as_ref().and_then(|s| s.phase.as_deref()) == Some(BOUND)
        })
        .count() as u32;

    let total = pool_size(&pool.spec)?;
    let status = IpPoolStatus {
        allocated: Some(allocated),
        available: Some(total.saturating_sub(allocated)),
    };
    pool_api
        .patch_status(
            pool_name,
            &PatchParams::default(),
            &Patch::Merge(serde_json::json!({ "status": status })),
        )
        .await?;
    Ok(())
}

fn parse_v4(s: &str) -> Result<Ipv4Addr, ReconcileError> {
    s.parse()
        .map_err(|_| ReconcileError::BadAddress(s.to_string()))
}

fn pool_size(pool: &IpPoolSpec) -> Result<u32, ReconcileError> {
    let start = u32::from(parse_v4(&pool.range_start)?);
    let end = u32::from(parse_v4(&pool.range_end)?);
    Ok(end.saturating_sub(start) + 1)
}

/// First address in `pool`'s range that is neither the pool's gateway nor
/// already in `used`. IPv4 only for v1 (matches the existing IPv6 gap in
/// `crow-provider-proxmox`'s ipconfig heuristic).
fn allocate_ip(
    pool: &IpPoolSpec,
    used: &HashSet<Ipv4Addr>,
) -> Result<Option<Ipv4Addr>, ReconcileError> {
    let start = u32::from(parse_v4(&pool.range_start)?);
    let end = u32::from(parse_v4(&pool.range_end)?);
    let gateway = parse_v4(&pool.gateway)?;
    Ok((start..=end)
        .map(Ipv4Addr::from)
        .find(|ip| *ip != gateway && !used.contains(ip)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crow_core::crd::networking::IpPoolPurpose;

    fn pool(start: &str, end: &str, gateway: &str) -> IpPoolSpec {
        IpPoolSpec {
            cidr: "10.20.0.0/24".to_string(),
            range_start: start.to_string(),
            range_end: end.to_string(),
            gateway: gateway.to_string(),
            dns: vec!["1.1.1.1".to_string()],
            bridge: "vmbr0".to_string(),
            purpose: IpPoolPurpose::Vm,
        }
    }

    #[test]
    fn allocates_first_free_address_in_range() {
        let spec = pool("10.20.0.10", "10.20.0.12", "10.20.0.1");
        let used = HashSet::new();
        assert_eq!(
            allocate_ip(&spec, &used).unwrap(),
            Some("10.20.0.10".parse().unwrap())
        );
    }

    #[test]
    fn skips_addresses_already_in_use() {
        let spec = pool("10.20.0.10", "10.20.0.12", "10.20.0.1");
        let used: HashSet<Ipv4Addr> = ["10.20.0.10", "10.20.0.11"]
            .into_iter()
            .map(|ip| ip.parse().unwrap())
            .collect();
        assert_eq!(
            allocate_ip(&spec, &used).unwrap(),
            Some("10.20.0.12".parse().unwrap())
        );
    }

    #[test]
    fn skips_the_pool_gateway() {
        let spec = pool("10.20.0.1", "10.20.0.2", "10.20.0.1");
        let used = HashSet::new();
        assert_eq!(
            allocate_ip(&spec, &used).unwrap(),
            Some("10.20.0.2".parse().unwrap())
        );
    }

    #[test]
    fn returns_none_when_pool_is_exhausted() {
        let spec = pool("10.20.0.10", "10.20.0.11", "10.20.0.1");
        let used: HashSet<Ipv4Addr> = ["10.20.0.10", "10.20.0.11"]
            .into_iter()
            .map(|ip| ip.parse().unwrap())
            .collect();
        assert_eq!(allocate_ip(&spec, &used).unwrap(), None);
    }

    #[test]
    fn pool_size_is_inclusive_of_both_ends() {
        let spec = pool("10.20.0.10", "10.20.0.19", "10.20.0.1");
        assert_eq!(pool_size(&spec).unwrap(), 10);
    }

    #[test]
    fn rejects_malformed_addresses() {
        let spec = pool("not-an-ip", "10.20.0.19", "10.20.0.1");
        assert!(matches!(
            allocate_ip(&spec, &HashSet::new()),
            Err(ReconcileError::BadAddress(_))
        ));
    }
}
