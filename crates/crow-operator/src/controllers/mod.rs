use std::sync::Arc;

use kube::Client;
use sqlx::PgPool;

use crow_provider_vyos::VyosNetworkProvider;

pub mod database;
pub mod exposed_endpoint;
pub mod ip_claim;
pub mod ip_pool;
pub mod k8s_cluster;
pub mod object_store;
pub mod private_subnet;
pub mod tunnel;
pub mod virtual_machine;

pub async fn run_all(
    client: Client,
    db: PgPool,
    network: Option<Arc<VyosNetworkProvider>>,
) -> anyhow::Result<()> {
    tokio::try_join!(
        virtual_machine::run(client.clone(), db.clone()),
        k8s_cluster::run(client.clone()),
        object_store::run(client.clone()),
        database::run(client.clone()),
        ip_claim::run(client.clone()),
        ip_pool::run(client.clone()),
        private_subnet::run(client.clone(), db),
        tunnel::run(client.clone()),
        exposed_endpoint::run(client, network),
    )?;
    Ok(())
}
