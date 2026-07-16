use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Uuid;

use crow_provider_registry::build_infra_provider;

use crate::{
    error::{ApiError, ApiResult},
    middleware::AuthUser,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).patch(update).delete(remove))
        .nest("/{id}/nodes", super::provider_nodes::router())
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

#[derive(sqlx::FromRow)]
struct ProviderDetailRow {
    id: Uuid,
    name: String,
    provider_type: String,
    config: Value,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct ProviderDetailResponse {
    id: Uuid,
    name: String,
    provider_type: String,
    config: Value,
    created_at: DateTime<Utc>,
}

/// Masks known secret-shaped keys before a config blob leaves the server.
/// Generic across provider types rather than proxmox-specific, since any
/// future provider's config could carry its own secret field under the same
/// convention.
fn redact_secrets(config: &mut Value) {
    if let Some(obj) = config.as_object_mut() {
        for key in obj.keys().cloned().collect::<Vec<_>>() {
            if key.ends_with("_secret") || key.ends_with("_password") {
                obj.insert(key, Value::String("••••••••".to_string()));
            }
        }
    }
}

async fn get_one(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProviderDetailResponse>> {
    let mut row = sqlx::query_as::<_, ProviderDetailRow>(
        "SELECT id, name, provider_type, config, created_at FROM providers WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::NotFound)?;

    redact_secrets(&mut row.config);

    Ok(Json(ProviderDetailResponse {
        id: row.id,
        name: row.name,
        provider_type: row.provider_type,
        config: row.config,
        created_at: row.created_at,
    }))
}

#[derive(Deserialize)]
struct UpdateProviderRequest {
    /// Shallow-merged into the existing config, so fields the caller doesn't
    /// mention (notably `token_secret`) are left untouched.
    config: Value,
}

async fn update(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProviderRequest>,
) -> ApiResult<Json<ProviderDetailResponse>> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let existing: Option<(String, Value)> =
        sqlx::query_as("SELECT provider_type, config FROM providers WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    let (provider_type, mut config) = existing.ok_or(ApiError::NotFound)?;

    if let (Some(existing_obj), Some(patch_obj)) = (config.as_object_mut(), req.config.as_object())
    {
        for (key, value) in patch_obj {
            existing_obj.insert(key.clone(), value.clone());
        }
    }

    // Validate the merged config actually builds before persisting it.
    build_infra_provider(&provider_type, &config)?;

    sqlx::query("UPDATE providers SET config = $1 WHERE id = $2")
        .bind(&config)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let mut row = sqlx::query_as::<_, ProviderDetailRow>(
        "SELECT id, name, provider_type, config, created_at FROM providers WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    redact_secrets(&mut row.config);

    Ok(Json(ProviderDetailResponse {
        id: row.id,
        name: row.name,
        provider_type: row.provider_type,
        config: row.config,
        created_at: row.created_at,
    }))
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
