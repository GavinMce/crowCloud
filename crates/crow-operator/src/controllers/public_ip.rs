use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::ResourceExt;
use kube::{
    runtime::{
        controller::Action, finalizer, finalizer::Event as FinalizerEvent, watcher, Controller,
    },
    Client,
};

use crow_core::{
    crd::networking::{PublicIp, PublicIpStatus},
    traits::NetworkProvider,
    types::{ReserveIpHandle, ReserveIpSpec},
};
use crow_provider_registry::VM_NAMESPACE;
use crow_provider_vyos::VyosNetworkProvider;

use super::target_resolution::{resolve_target_ip, TargetResolutionError};

const FINALIZER: &str = "publicip.crow.cloud/finalizer";
const READY: &str = "Ready";
const PENDING: &str = "Pending";
const FAILED: &str = "Failed";

#[derive(Debug, thiserror::Error)]
enum ReconcileError {
    #[error("address {0:?} is not a valid IP address")]
    InvalidAddress(String),
    #[error("address {0:?} is already claimed by PublicIp {1:?}")]
    AddressInUse(String, String),
    #[error(transparent)]
    TargetResolution(#[from] TargetResolutionError),
    #[error(transparent)]
    Provider(#[from] crow_core::ProviderError),
    #[error(transparent)]
    Kube(#[from] kube::Error),
}

struct Ctx {
    client: Client,
    network: Arc<VyosNetworkProvider>,
}

pub async fn run(client: Client, network: Option<Arc<VyosNetworkProvider>>) -> anyhow::Result<()> {
    let Some(network) = network else {
        // Unconfigured for this operator instance (see
        // `main.rs::build_vyos_network_provider`) -- stay a no-op stub,
        // same as `exposed_endpoint` and the other not-yet-configured
        // controllers, rather than crashing the whole operator over
        // optional functionality.
        tracing::info!("VyOS not configured -- public_ip controller is disabled");
        std::future::pending::<()>().await;
        return Ok(());
    };

    let api: Api<PublicIp> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx { client, network });

    Controller::new(api, watcher::Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            if let Err(e) = res {
                tracing::warn!(error = %e, "public ip reconcile failed");
            }
        })
        .await;
    Ok(())
}

async fn reconcile(
    public_ip: Arc<PublicIp>,
    ctx: Arc<Ctx>,
) -> Result<Action, finalizer::Error<ReconcileError>> {
    let api: Api<PublicIp> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    finalizer(&api, FINALIZER, public_ip, |event| async {
        match event {
            FinalizerEvent::Apply(public_ip) => apply(&public_ip, &ctx).await,
            FinalizerEvent::Cleanup(public_ip) => cleanup(&public_ip, &ctx).await,
        }
    })
    .await
}

fn error_policy(
    _public_ip: Arc<PublicIp>,
    _err: &finalizer::Error<ReconcileError>,
    _ctx: Arc<Ctx>,
) -> Action {
    Action::requeue(Duration::from_secs(30))
}

/// Name of another `PublicIp` in `existing` already claiming `address`,
/// if any. Pure and self-contained so it's unit-testable without a real
/// kube API, same as `private_subnet.rs`'s `vni_conflict`. Exact string
/// match is enough here -- unlike `ExposedEndpoint`'s port numbers,
/// there's no modulo-aliasing concern for a raw address.
fn address_conflict<'a>(
    existing: &'a [PublicIp],
    self_name: &str,
    address: &str,
) -> Option<&'a str> {
    existing.iter().find_map(|other| {
        let other_name = other.metadata.name.as_deref()?;
        (other_name != self_name && other.spec.address == address).then_some(other_name)
    })
}

async fn apply(public_ip: &PublicIp, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = public_ip.name_any();

    // Already reserved -- reserve_ip isn't guaranteed idempotent against
    // an address that's already bound, same reasoning as
    // `private_subnet.rs`'s own already-Ready short-circuit.
    if public_ip.status.as_ref().and_then(|s| s.phase.as_deref()) == Some(READY) {
        return Ok(Action::requeue(Duration::from_secs(300)));
    }

    let api: Api<PublicIp> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    let existing = api.list(&ListParams::default()).await?;
    if let Some(other) = address_conflict(&existing.items, &name, &public_ip.spec.address) {
        let status = PublicIpStatus {
            phase: Some(FAILED.to_string()),
            message: Some(format!(
                "address {:?} is already claimed by PublicIp {other:?}",
                public_ip.spec.address
            )),
        };
        patch_status(ctx, &name, &status).await?;
        return Err(ReconcileError::AddressInUse(
            public_ip.spec.address.clone(),
            other.to_string(),
        ));
    }

    let Some(target_ip) = resolve_target_ip(
        &ctx.client,
        &public_ip.spec.target_kind,
        &public_ip.spec.target_name,
    )
    .await?
    else {
        // Target doesn't exist yet, or exists but has no IP assigned yet
        // -- wait rather than error, same reasoning as
        // `exposed_endpoint.rs`'s own not-ready-yet case.
        patch_status(
            ctx,
            &name,
            &PublicIpStatus {
                phase: Some(PENDING.to_string()),
                message: Some("waiting for target to have an IP".to_string()),
            },
        )
        .await?;
        return Ok(Action::requeue(Duration::from_secs(15)));
    };

    let address = public_ip
        .spec
        .address
        .parse()
        .map_err(|_| ReconcileError::InvalidAddress(public_ip.spec.address.clone()))?;

    ctx.network
        .reserve_ip(ReserveIpSpec {
            address,
            prefix: public_ip.spec.prefix,
            target_ip,
        })
        .await?;

    patch_status(
        ctx,
        &name,
        &PublicIpStatus {
            phase: Some(READY.to_string()),
            message: None,
        },
    )
    .await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}

async fn cleanup(public_ip: &PublicIp, ctx: &Ctx) -> Result<Action, ReconcileError> {
    // Only release if reservation actually completed -- Pending/Failed
    // phases never called reserve_ip, so there's nothing on VyOS to
    // clean up (same reasoning as `private_subnet.rs`'s cleanup only
    // deleting a bridge when `status.bridge` was actually set).
    if public_ip.status.as_ref().and_then(|s| s.phase.as_deref()) == Some(READY) {
        let address = public_ip
            .spec
            .address
            .parse()
            .map_err(|_| ReconcileError::InvalidAddress(public_ip.spec.address.clone()))?;
        ctx.network
            .release_ip(&ReserveIpHandle {
                provider_id: String::new(),
                address,
                prefix: public_ip.spec.prefix,
            })
            .await?;
    }
    Ok(Action::await_change())
}

async fn patch_status(
    ctx: &Ctx,
    name: &str,
    status: &PublicIpStatus,
) -> Result<(), ReconcileError> {
    let api: Api<PublicIp> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    api.patch_status(
        name,
        &PatchParams::default(),
        &Patch::Merge(serde_json::json!({ "status": status })),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::api::ObjectMeta;

    fn public_ip(name: &str, address: &str) -> PublicIp {
        PublicIp {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                ..Default::default()
            },
            spec: crow_core::crd::networking::PublicIpSpec {
                address: address.to_string(),
                prefix: 24,
                target_kind: crow_core::crd::networking::ExposedTargetKind::VirtualMachine,
                target_name: "vm-abc".to_string(),
                label: None,
            },
            status: None,
        }
    }

    #[test]
    fn address_conflict_finds_another_public_ip_claiming_the_same_address() {
        let existing = vec![
            public_ip("wan-a", "10.0.202.50"),
            public_ip("wan-b", "10.0.202.51"),
        ];
        assert_eq!(
            address_conflict(&existing, "wan-c", "10.0.202.51"),
            Some("wan-b")
        );
    }

    #[test]
    fn address_conflict_ignores_the_public_ip_being_reconciled_itself() {
        let existing = vec![public_ip("wan-a", "10.0.202.50")];
        assert_eq!(address_conflict(&existing, "wan-a", "10.0.202.50"), None);
    }

    #[test]
    fn address_conflict_is_none_when_every_address_is_distinct() {
        let existing = vec![
            public_ip("wan-a", "10.0.202.50"),
            public_ip("wan-b", "10.0.202.51"),
        ];
        assert_eq!(address_conflict(&existing, "wan-c", "10.0.202.52"), None);
    }
}
