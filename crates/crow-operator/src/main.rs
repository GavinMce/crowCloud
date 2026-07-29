use k8s_openapi::api::core::v1::Namespace;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{Api, ObjectMeta, Patch, PatchParams, PostParams},
    Client, CustomResourceExt,
};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

mod controllers;

/// Installs the CRDs this operator reconciles. `VirtualMachine`, the IPAM
/// trio (`IpPool`/`IpClaim`/`PrivateSubnet`), and `ExposedEndpoint` (the
/// TCP/UDP shared-IP:port path only, see `controllers::exposed_endpoint`)
/// are installed — K8sCluster/Database/ObjectStore/TunnelEndpoint/
/// CustomDomain stay out of scope until their controllers are implemented.
async fn install_crds(client: &Client) -> anyhow::Result<()> {
    let crds: Api<CustomResourceDefinition> = Api::all(client.clone());
    let pp = PatchParams::apply("crow-operator").force();

    for crd in [
        crow_core::crd::resources::VirtualMachine::crd(),
        crow_core::crd::networking::IpPool::crd(),
        crow_core::crd::networking::IpClaim::crd(),
        crow_core::crd::networking::PrivateSubnet::crd(),
        crow_core::crd::networking::ExposedEndpoint::crd(),
    ] {
        let name = crd
            .metadata
            .name
            .clone()
            .expect("CustomResourceExt::crd() always sets metadata.name");
        crds.patch(&name, &pp, &Patch::Apply(&crd)).await?;
        tracing::info!(%name, "CRD applied");
    }
    Ok(())
}

/// Ensures the fixed namespace all resource CRs live in exists. Idempotent.
async fn ensure_namespace(client: &Client) -> anyhow::Result<()> {
    let ns_api: Api<Namespace> = Api::all(client.clone());
    let ns = Namespace {
        metadata: ObjectMeta {
            name: Some(crow_provider_registry::VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    match ns_api.create(&PostParams::default(), &ns).await {
        Ok(_) => tracing::info!(
            namespace = crow_provider_registry::VM_NAMESPACE,
            "created namespace"
        ),
        Err(kube::Error::Api(e)) if e.code == 409 => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

/// There's exactly one VyOS box per fabric today (no multi-provider
/// swapping the way `InfraProvider` resolution needs, since VMs can land
/// on different Proxmox nodes) -- so its SSH connection details are
/// operator-level config (env vars), not a per-CR reference or a Postgres
/// `providers` row.
fn build_vyos_network_provider() -> crow_provider_vyos::VyosNetworkProvider {
    crow_provider_vyos::VyosNetworkProvider {
        host: std::env::var("VYOS_HOST").expect("VYOS_HOST must be set for crow-operator"),
        port: std::env::var("VYOS_SSH_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(22),
        user: std::env::var("VYOS_SSH_USER").unwrap_or_else(|_| "vyos".to_string()),
        ssh_key: std::env::var("VYOS_SSH_KEY_PATH")
            .expect("VYOS_SSH_KEY_PATH must be set for crow-operator")
            .into(),
        uplink_interface: std::env::var("VYOS_UPLINK_INTERFACE")
            .expect("VYOS_UPLINK_INTERFACE must be set for crow-operator"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let client = Client::try_default().await?;
    tracing::info!("crow-operator starting");

    install_crds(&client).await?;
    ensure_namespace(&client).await?;

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for crow-operator");
    let db = crow_db::connect(&database_url).await?;

    let network = Arc::new(build_vyos_network_provider());

    controllers::run_all(client, db, network).await?;

    Ok(())
}
