//! Runs on the Hetzner VPS. Listens only on the WireGuard interface.
//! Manages nginx config, nftables rules, and Let's Encrypt certs on behalf
//! of the crow-operator in the management cluster.

use axum::Router;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

mod routes;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    // Only bind on WireGuard interface — never the public interface
    let addr: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "10.200.0.1:9090".into())
        .parse()?;

    tracing::info!(%addr, "crow-vps-agent starting");

    let app = Router::new().nest("/api", routes::router());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
