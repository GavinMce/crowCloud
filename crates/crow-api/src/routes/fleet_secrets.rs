use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
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
        .route("/{id}", axum::routing::delete(revoke))
}

#[derive(Serialize, sqlx::FromRow)]
struct FleetSecretRow {
    id: Uuid,
    // Deliberately not `secret` -- once minted, a fleet secret is only
    // ever shown again in the `create` response (see below), matching
    // how a Proxmox API token is shown once at creation and never again.
    label: Option<String>,
    created_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<FleetSecretRow>>> {
    let rows: Vec<FleetSecretRow> = sqlx::query_as(
        "SELECT id, label, created_at, revoked_at FROM fleet_secrets ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateFleetSecretRequest {
    label: Option<String>,
}

#[derive(Serialize)]
struct CreateFleetSecretResponse {
    #[serde(flatten)]
    row: FleetSecretRow,
    secret: String,
}

/// Mints a new fleet secret for `crow-cli iso proxmox build` to bake into
/// images (#66). Not tied to any specific `provider_id` or host -- one
/// secret is meant to be shared across every image built with it, unlike
/// the per-MAC pre-registration model this replaced.
async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateFleetSecretRequest>,
) -> ApiResult<Json<CreateFleetSecretResponse>> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let secret = Uuid::new_v4().simple().to_string();
    let created_by = Uuid::parse_str(&claims.sub).ok();

    let row: FleetSecretRow = sqlx::query_as(
        "INSERT INTO fleet_secrets (secret, label, created_by)
         VALUES ($1, $2, $3)
         RETURNING id, label, created_at, revoked_at",
    )
    .bind(&secret)
    .bind(&req.label)
    .bind(created_by)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(CreateFleetSecretResponse { row, secret }))
}

async fn revoke(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let result = sqlx::query(
        "UPDATE fleet_secrets SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
