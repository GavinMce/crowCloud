use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    middleware::AuthUser,
    providers::build_infra_provider,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", delete(remove))
}

#[derive(Serialize, sqlx::FromRow)]
struct ProviderRow {
    id: Uuid,
    name: String,
    provider_type: String,
    created_at: DateTime<Utc>,
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ProviderRow>>> {
    let rows = sqlx::query_as::<_, ProviderRow>(
        "SELECT id, name, provider_type, created_at FROM providers ORDER BY name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateProviderRequest {
    name: String,
    provider_type: String,
    config: Value,
}

#[derive(Serialize, sqlx::FromRow)]
struct CreateProviderResponse {
    id: Uuid,
    name: String,
    provider_type: String,
}

async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateProviderRequest>,
) -> ApiResult<(StatusCode, Json<CreateProviderResponse>)> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    // Validate config by attempting to build the provider now.
    build_infra_provider(&req.provider_type, &req.config)?;

    let user_id = Uuid::parse_str(&claims.sub).ok();

    let row = sqlx::query_as::<_, CreateProviderResponse>(
        "INSERT INTO providers (name, provider_type, config, created_by)
         VALUES ($1, $2, $3, $4)
         RETURNING id, name, provider_type",
    )
    .bind(&req.name)
    .bind(&req.provider_type)
    .bind(&req.config)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.constraint() == Some("providers_name_key") => {
            ApiError::Conflict(format!("provider '{}' already exists", req.name))
        }
        _ => ApiError::Internal(e.into()),
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

async fn remove(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let result = sqlx::query("DELETE FROM providers WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
