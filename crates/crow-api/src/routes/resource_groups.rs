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
        .route("/:rg", delete(remove))
}

#[derive(Serialize, sqlx::FromRow)]
struct RgRow {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> ApiResult<Json<Vec<RgRow>>> {
    let rows = sqlx::query_as::<_, RgRow>(
        "SELECT id, name, created_at FROM resource_groups WHERE project = $1 ORDER BY name",
    )
    .bind(&project)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateRgRequest {
    name: String,
}

async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(project): Path<String>,
    Json(req): Json<CreateRgRequest>,
) -> ApiResult<(StatusCode, Json<RgRow>)> {
    // Verify project exists.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE name = $1)")
        .bind(&project)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    if !exists {
        return Err(ApiError::NotFound);
    }

    let _user_id = Uuid::parse_str(&claims.sub).ok();

    let row = sqlx::query_as::<_, RgRow>(
        "INSERT INTO resource_groups (project, name) VALUES ($1, $2) RETURNING id, name, created_at",
    )
    .bind(&project)
    .bind(&req.name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db)
            if db.constraint() == Some("resource_groups_project_name_key") =>
        {
            ApiError::Conflict(format!("resource group '{}' already exists", req.name))
        }
        _ => ApiError::Internal(e.into()),
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

async fn remove(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((project, rg)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let result = sqlx::query("DELETE FROM resource_groups WHERE project = $1 AND name = $2")
        .bind(&project)
        .bind(&rg)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
