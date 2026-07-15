use crate::AppState;
use axum::Router;

mod auth;
mod domains;
mod expose;
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
        .nest("/expose", expose::router())
        .nest("/domains", domains::router())
}
