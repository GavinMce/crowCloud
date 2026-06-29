use kube::Client;

pub mod database;
pub mod ip_claim;
pub mod k8s_cluster;
pub mod object_store;
pub mod tunnel;
pub mod virtual_machine;

pub async fn run_all(client: Client) -> anyhow::Result<()> {
    tokio::try_join!(
        virtual_machine::run(client.clone()),
        k8s_cluster::run(client.clone()),
        object_store::run(client.clone()),
        database::run(client.clone()),
        ip_claim::run(client.clone()),
        tunnel::run(client.clone()),
    )?;
    Ok(())
}
