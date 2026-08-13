use std::net::IpAddr;

use kube::api::Api;
use kube::Client;

use crow_core::crd::{networking::ExposedTargetKind, resources::VirtualMachine};
use crow_provider_registry::VM_NAMESPACE;

#[derive(Debug, thiserror::Error)]
pub enum TargetResolutionError {
    #[error("target kind {0:?} isn't supported yet -- only VirtualMachine is wired up so far")]
    UnsupportedTargetKind(String),
    #[error(transparent)]
    Kube(#[from] kube::Error),
}

/// Resolves `target_kind`/`target_name` to the target's private IP.
/// Shared between `exposed_endpoint` and `public_ip` -- both resolve
/// targets the exact same way. `VirtualMachine` is the only kind wired up
/// so far -- `K8sCluster`/`ObjectStore`/`Database` all have no-op stub
/// controllers today (see `main.rs`'s own `install_crds` comment), so
/// there's no status field to resolve an IP from for them yet regardless.
///
/// Returns `Ok(None)` (not an error) when the target doesn't exist yet or
/// exists but has no IP assigned yet -- callers should wait/requeue
/// rather than treat this as failed.
pub async fn resolve_target_ip(
    client: &Client,
    kind: &ExposedTargetKind,
    name: &str,
) -> Result<Option<IpAddr>, TargetResolutionError> {
    match kind {
        ExposedTargetKind::VirtualMachine => {
            let api: Api<VirtualMachine> = Api::namespaced(client.clone(), VM_NAMESPACE);
            let Some(vm) = api.get_opt(name).await? else {
                return Ok(None);
            };
            Ok(vm.status.and_then(|s| s.ip).and_then(|ip| ip.parse().ok()))
        }
        other => Err(TargetResolutionError::UnsupportedTargetKind(format!(
            "{other:?}"
        ))),
    }
}
