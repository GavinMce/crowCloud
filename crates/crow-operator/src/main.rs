use k8s_openapi::api::core::v1::Namespace;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{Api, ObjectMeta, Patch, PatchParams, PostParams},
    Client, CustomResourceExt,
};
use tracing_subscriber::EnvFilter;

mod controllers;

/// Installs the CRDs this operator reconciles. Only `VirtualMachine` is
/// installed for now — K8sCluster/Database/ObjectStore stay out of scope until
/// their controllers are implemented.
async fn install_crds(client: &Client) -> anyhow::Result<()> {
    let crds: Api<CustomResourceDefinition> = Api::all(client.clone());
    let pp = PatchParams::apply("crow-operator").force();

    for crd in [
        crow_core::crd::resources::VirtualMachine::crd(),
        crow_core::crd::resources::K8sCluster::crd(),
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

    controllers::run_all(client, db).await?;

    Ok(())
}
