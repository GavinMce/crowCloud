use crate::AppState;
use axum::Router;

mod auth;
mod domains;
mod expose;
mod projects;
mod providers;
mod resource_groups;
mod resources;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/projects", projects::router())
        .nest(
            "/projects/{project}/resource-groups",
            resource_groups::router(),
        )
        .nest(
            "/projects/{project}/resource-groups/{rg}/resources",
            resources::router(),
        )
        .nest("/providers", providers::router())
        .nest("/expose", expose::router())
        .nest("/domains", domains::router())
}
