use kube::Client;

pub async fn run(_client: Client) -> anyhow::Result<()> {
    // TODO: implement controller reconcile loop
    std::future::pending::<()>().await;
    Ok(())
}
