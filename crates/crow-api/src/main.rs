use axum::Router;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

mod error;
mod middleware;
mod routes;
mod state;

pub use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    let state = AppState::init().await?;

    let app = Router::new()
        .nest("/api/v1", routes::router())
        .with_state(state);

    let addr: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;

    tracing::info!(%addr, "crow-api starting");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
