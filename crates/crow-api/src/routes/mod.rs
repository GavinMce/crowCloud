use crate::AppState;
use axum::Router;

mod auth;
mod domains;
mod expose;
mod ip_pools;
mod k8s_bootstrap;
mod k8s_metrics;
mod projects;
mod provider_nodes;
mod providers;
mod resources;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/projects", projects::router())
        .nest("/projects/{project}/resources", resources::router())
        .nest("/providers", providers::router())
        .nest("/ip-pools", ip_pools::router())
        .nest("/expose", expose::router())
        .nest("/domains", domains::router())
        .nest("/internal", k8s_bootstrap::router())
}
