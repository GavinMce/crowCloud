use kube::Client;
use tracing_subscriber::EnvFilter;

mod controllers;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let client = Client::try_default().await?;
    tracing::info!("crow-operator starting");

    controllers::run_all(client).await?;

    Ok(())
}
