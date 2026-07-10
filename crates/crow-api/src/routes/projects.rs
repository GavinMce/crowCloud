use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    middleware::AuthUser,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{project}", delete(remove))
}

#[derive(Serialize, sqlx::FromRow)]
struct ProjectRow {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ProjectRow>>> {
    let rows =
        sqlx::query_as::<_, ProjectRow>("SELECT id, name, created_at FROM projects ORDER BY name")
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateProjectRequest {
    name: String,
}

async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateProjectRequest>,
) -> ApiResult<(StatusCode, Json<ProjectRow>)> {
    let user_id = Uuid::parse_str(&claims.sub).ok();

    let row = sqlx::query_as::<_, ProjectRow>(
        "INSERT INTO projects (name, created_by) VALUES ($1, $2) RETURNING id, name, created_at",
    )
    .bind(&req.name)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.constraint() == Some("projects_name_key") => {
            ApiError::Conflict(format!("project '{}' already exists", req.name))
        }
        _ => ApiError::Internal(e.into()),
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

async fn remove(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query("DELETE FROM projects WHERE name = $1")
        .bind(&project)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
