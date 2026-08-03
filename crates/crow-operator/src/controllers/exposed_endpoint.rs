use std::{net::IpAddr, sync::Arc, time::Duration};

use futures::StreamExt;
use kube::api::{Api, Patch, PatchParams};
use kube::ResourceExt;
use kube::{
    runtime::{
        controller::Action, finalizer, finalizer::Event as FinalizerEvent, watcher, Controller,
    },
    Client,
};

use crow_core::{
    crd::{
        networking::{
            ExposeProtocol, ExposeType, ExposedEndpoint, ExposedEndpointStatus, ExposedTargetKind,
        },
        resources::VirtualMachine,
    },
    traits::NetworkProvider,
    types::{Protocol, TcpExposeSpec},
};
use crow_provider_registry::VM_NAMESPACE;
use crow_provider_vyos::VyosNetworkProvider;

const FINALIZER: &str = "exposedendpoint.crow.cloud/finalizer";
const READY: &str = "Ready";
const PENDING: &str = "Pending";

#[derive(Debug, thiserror::Error)]
enum ReconcileError {
    #[error("target kind {0:?} isn't supported yet -- only VirtualMachine is wired up so far")]
    UnsupportedTargetKind(String),
    #[error("ExposeType::Http requires spec.domain to be set -- subdomain routing has no meaning without one")]
    HttpRequiresDomain,
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
        // same as the other not-yet-configured/implemented controllers,
        // rather than crashing the whole operator over optional
        // functionality.
        tracing::info!("VyOS not configured -- exposed_endpoint controller is disabled");
        std::future::pending::<()>().await;
        return Ok(());
    };

    let api: Api<ExposedEndpoint> = Api::namespaced(client.clone(), VM_NAMESPACE);
    let ctx = Arc::new(Ctx { client, network });

    Controller::new(api, watcher::Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            if let Err(e) = res {
                tracing::warn!(error = %e, "exposed endpoint reconcile failed");
            }
        })
        .await;
    Ok(())
}

async fn reconcile(
    endpoint: Arc<ExposedEndpoint>,
    ctx: Arc<Ctx>,
) -> Result<Action, finalizer::Error<ReconcileError>> {
    let api: Api<ExposedEndpoint> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
    finalizer(&api, FINALIZER, endpoint, |event| async {
        match event {
            FinalizerEvent::Apply(endpoint) => apply(&endpoint, &ctx).await,
            FinalizerEvent::Cleanup(endpoint) => cleanup(&endpoint, &ctx).await,
        }
    })
    .await
}

fn error_policy(
    _endpoint: Arc<ExposedEndpoint>,
    _err: &finalizer::Error<ReconcileError>,
    _ctx: Arc<Ctx>,
) -> Action {
    Action::requeue(Duration::from_secs(30))
}

/// Resolves `target_kind`/`target_name` to the target's private IP.
/// `VirtualMachine` is the only kind wired up so far -- `K8sCluster`/
/// `ObjectStore`/`Database` all have no-op stub controllers today (see
/// `main.rs`'s own `install_crds` comment), so there's no status field to
/// resolve an IP from for them yet regardless.
async fn resolve_target_ip(
    ctx: &Ctx,
    kind: &ExposedTargetKind,
    name: &str,
) -> Result<Option<IpAddr>, ReconcileError> {
    match kind {
        ExposedTargetKind::VirtualMachine => {
            let api: Api<VirtualMachine> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
            let Some(vm) = api.get_opt(name).await? else {
                return Ok(None);
            };
            Ok(vm.status.and_then(|s| s.ip).and_then(|ip| ip.parse().ok()))
        }
        other => Err(ReconcileError::UnsupportedTargetKind(format!("{other:?}"))),
    }
}

fn to_protocol(expose_type: &ExposeType, protocol: &Option<ExposeProtocol>) -> Protocol {
    match protocol {
        Some(ExposeProtocol::Tcp) => Protocol::Tcp,
        Some(ExposeProtocol::Udp) => Protocol::Udp,
        Some(ExposeProtocol::TcpUdp) => Protocol::TcpUdp,
        // No explicit override -- derive straight from expose_type rather
        // than defaulting to some third value, so an omitted `protocol`
        // behaves exactly as its `expose_type` name implies.
        None => match expose_type {
            ExposeType::Udp => Protocol::Udp,
            _ => Protocol::Tcp,
        },
    }
}

async fn apply(endpoint: &ExposedEndpoint, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let name = endpoint.name_any();

    if matches!(endpoint.spec.expose_type, ExposeType::Http) && endpoint.spec.domain.is_none() {
        return Err(ReconcileError::HttpRequiresDomain);
    }

    let Some(target_ip) =
        resolve_target_ip(ctx, &endpoint.spec.target_kind, &endpoint.spec.target_name).await?
    else {
        // Target doesn't exist yet, or exists but has no IP assigned yet
        // (e.g. its IpClaim hasn't bound) -- wait rather than error, same
        // reasoning as `virtual_machine.rs`'s own IpClaim-not-bound-yet case.
        patch_status(
            ctx,
            &name,
            &ExposedEndpointStatus {
                phase: Some(PENDING.to_string()),
                public_url: None,
                cert_expiry: None,
            },
        )
        .await?;
        return Ok(Action::requeue(Duration::from_secs(15)));
    };

    let public_url = if matches!(endpoint.spec.expose_type, ExposeType::Http) {
        // Checked above -- Http always has a domain by this point.
        let domain = endpoint.spec.domain.clone().expect("checked above");
        ctx.network
            .expose_http(crow_core::types::HttpExposeSpec {
                domain: domain.clone(),
                target_ip,
                target_port: endpoint.spec.port,
                tls: endpoint.spec.tls,
            })
            .await?;
        let scheme = if endpoint.spec.tls { "https" } else { "http" };
        format!("{scheme}://{domain}")
    } else {
        let public_port = endpoint.spec.public_port.unwrap_or(endpoint.spec.port);
        let protocol = to_protocol(&endpoint.spec.expose_type, &endpoint.spec.protocol);
        ctx.network
            .expose_tcp(TcpExposeSpec {
                target_ip,
                target_port: endpoint.spec.port,
                public_port,
                protocol,
            })
            .await?;
        format!("{}:{public_port}", ctx.network.host)
    };

    patch_status(
        ctx,
        &name,
        &ExposedEndpointStatus {
            phase: Some(READY.to_string()),
            public_url: Some(public_url),
            cert_expiry: None,
        },
    )
    .await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}

async fn cleanup(endpoint: &ExposedEndpoint, ctx: &Ctx) -> Result<Action, ReconcileError> {
    let handle = if matches!(endpoint.spec.expose_type, ExposeType::Http) {
        let Some(domain) = endpoint.spec.domain.clone() else {
            // Never got far enough to create anything -- HttpRequiresDomain
            // would have stopped `apply` before any Caddy file existed.
            return Ok(Action::await_change());
        };
        crow_core::types::ExposeHandle {
            provider_id: String::new(),
            domain: Some(domain),
            public_port: None,
        }
    } else {
        let public_port = endpoint.spec.public_port.unwrap_or(endpoint.spec.port);
        crow_core::types::ExposeHandle {
            provider_id: String::new(),
            domain: None,
            public_port: Some(public_port),
        }
    };

    ctx.network.unexpose(&handle).await?;
    Ok(Action::await_change())
}

async fn patch_status(
    ctx: &Ctx,
    name: &str,
    status: &ExposedEndpointStatus,
) -> Result<(), ReconcileError> {
    let api: Api<ExposedEndpoint> = Api::namespaced(ctx.client.clone(), VM_NAMESPACE);
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

    #[test]
    fn to_protocol_derives_from_expose_type_when_unset() {
        assert!(matches!(
            to_protocol(&ExposeType::Tcp, &None),
            Protocol::Tcp
        ));
        assert!(matches!(
            to_protocol(&ExposeType::Udp, &None),
            Protocol::Udp
        ));
    }

    #[test]
    fn to_protocol_prefers_an_explicit_override() {
        assert!(matches!(
            to_protocol(&ExposeType::Tcp, &Some(ExposeProtocol::TcpUdp)),
            Protocol::TcpUdp
        ));
    }
}
