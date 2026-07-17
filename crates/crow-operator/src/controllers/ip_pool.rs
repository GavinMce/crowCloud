use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use kube::{
    api::Api,
    runtime::{controller::Action, watcher, Controller},
    Client, ResourceExt,
};

use crow_core::crd::networking::IpPool;
use crow_provider_registry::VM_NAMESPACE;

use super::ip_claim::{recompute_pool_status, ReconcileError};

/// Reconciles `IpPool` directly (as opposed to only recomputing a pool's
/// status as a side effect of an `IpClaim` event) so a pool with zero claims
/// still gets `status.allocated`/`available` populated instead of sitting at
/// `null` — matching the allocator's own "recompute from a fresh scan"
/// approach in `ip_claim`. No finalizer: deletion has nothing to clean up
/// (already guarded at the API layer by rejecting deletes while claims are
/// bound), so a plain reconcile-on-apply is enough.
struct Ctx {
    client: Client,
}

pub async fn run(client: Client) -> anyhow::Result<()> {
    let api: Api<IpPool> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx { client });

    Controller::new(api, watcher::Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            if let Err(e) = res {
                tracing::warn!(error = %e, "ip pool reconcile failed");
            }
        })
        .await;
    Ok(())
}

async fn reconcile(pool: Arc<IpPool>, ctx: Arc<Ctx>) -> Result<Action, ReconcileError> {
    recompute_pool_status(&ctx.client, &pool.name_any(), None).await?;
    Ok(Action::requeue(Duration::from_secs(300)))
}

fn error_policy(_pool: Arc<IpPool>, _err: &ReconcileError, _ctx: Arc<Ctx>) -> Action {
    Action::requeue(Duration::from_secs(30))
}
