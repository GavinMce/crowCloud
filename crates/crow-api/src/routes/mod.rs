use crate::AppState;
use axum::Router;

mod auth;
mod domains;
mod expose;
mod fleet_secrets;
mod host_bootstrap;
mod ip_pools;
mod private_subnets;
mod projects;
mod provider_nodes;
mod providers;
mod public_ips;
mod resources;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/projects", projects::router())
        .nest("/projects/{project}/resources", resources::router())
        .nest("/providers", providers::router())
        .nest("/ip-pools", ip_pools::router())
        .nest("/private-subnets", private_subnets::router())
        .nest("/expose", expose::router())
        .nest("/public-ips", public_ips::router())
        .nest("/domains", domains::router())
        .nest("/fleet-secrets", fleet_secrets::router())
        .nest("/internal", host_bootstrap::router())
}
