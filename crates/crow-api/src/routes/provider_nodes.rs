use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use crow_provider_proxmox::ProxmoxNodeSummary;
use crow_provider_registry::{build_proxmox_provider, ProxmoxConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    middleware::AuthUser,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{name}", get(get_one).put(configure).delete(remove))
}

#[derive(Serialize)]
struct ProviderNodeResponse {
    name: String,
    status: String,
    cpu: Option<f64>,
    max_cpu: Option<u32>,
    mem: Option<u64>,
    max_mem: Option<u64>,
    uptime: Option<u64>,
    configured: bool,
    default_storage: Option<String>,
    default_bridge: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ProviderRow {
    provider_type: String,
    config: Value,
}

#[derive(sqlx::FromRow)]
struct NodeConfigRow {
    node_name: String,
    default_storage: String,
    default_bridge: String,
}

/// Loads the provider row and confirms it's a Proxmox host — node
/// discovery is Proxmox-specific, unlike the rest of the `providers` CRUD.
async fn load_proxmox_provider(
    state: &AppState,
    id: Uuid,
) -> ApiResult<crow_provider_proxmox::ProxmoxProvider> {
    let row: Option<ProviderRow> =
        sqlx::query_as("SELECT provider_type, config FROM providers WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    let row = row.ok_or(ApiError::NotFound)?;

    if row.provider_type != "proxmox" {
        return Err(ApiError::BadRequest("not a Proxmox host".to_string()));
    }

    Ok(build_proxmox_provider(&row.config)?)
}

/// Same config lookup as above, but only the parts needed for the legacy
/// primary-node fallback (kept separate so `configure`/`remove` — which
/// don't need a live Proxmox connection — can check existence without
/// building a provider).
async fn provider_config(state: &AppState, id: Uuid) -> ApiResult<ProxmoxConfig> {
    let row: Option<ProviderRow> =
        sqlx::query_as("SELECT provider_type, config FROM providers WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    let row = row.ok_or(ApiError::NotFound)?;
    serde_json::from_value(row.config)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("corrupt provider config: {e}")))
}

async fn configured_nodes(
    state: &AppState,
    provider_id: Uuid,
) -> ApiResult<HashMap<String, (String, String)>> {
    let rows: Vec<NodeConfigRow> = sqlx::query_as(
        "SELECT node_name, default_storage, default_bridge FROM provider_nodes WHERE provider_id = $1",
    )
    .bind(provider_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(rows
        .into_iter()
        .map(|r| (r.node_name, (r.default_storage, r.default_bridge)))
        .collect())
}

fn merge(
    summary: &ProxmoxNodeSummary,
    configured: &HashMap<String, (String, String)>,
    legacy: &ProxmoxConfig,
) -> ProviderNodeResponse {
    let (configured_flag, default_storage, default_bridge) = if let Some((storage, bridge)) =
        configured.get(&summary.node)
    {
        (true, Some(storage.clone()), Some(bridge.clone()))
    } else if legacy.node.as_deref() == Some(summary.node.as_str()) {
        // Legacy primary node: configured via providers.config even
        // though it has no provider_nodes row yet.
        match (&legacy.default_storage, &legacy.default_bridge) {
            (Some(storage), Some(bridge)) => (true, Some(storage.clone()), Some(bridge.clone())),
            _ => (false, None, None),
        }
    } else {
        (false, None, None)
    };

    ProviderNodeResponse {
        name: summary.node.clone(),
        status: summary.status.clone(),
        cpu: summary.cpu,
        max_cpu: summary.maxcpu,
        mem: summary.mem,
        max_mem: summary.maxmem,
        uptime: summary.uptime,
        configured: configured_flag,
        default_storage,
        default_bridge,
    }
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ProviderNodeResponse>>> {
    let provider = load_proxmox_provider(&state, id).await?;
    let legacy = provider_config(&state, id).await?;
    let configured = configured_nodes(&state, id).await?;

    let nodes = provider.list_nodes().await?;
    Ok(Json(
        nodes
            .iter()
            .map(|n| merge(n, &configured, &legacy))
            .collect(),
    ))
}

async fn get_one(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
) -> ApiResult<Json<ProviderNodeResponse>> {
    let provider = load_proxmox_provider(&state, id).await?;
    let legacy = provider_config(&state, id).await?;
    let configured = configured_nodes(&state, id).await?;

    let nodes = provider.list_nodes().await?;
    let summary = nodes
        .into_iter()
        .find(|n| n.node == name)
        .ok_or(ApiError::NotFound)?;
    Ok(Json(merge(&summary, &configured, &legacy)))
}

#[derive(Deserialize)]
struct ConfigureNodeRequest {
    default_storage: String,
    default_bridge: String,
}

async fn configure(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(req): Json<ConfigureNodeRequest>,
) -> ApiResult<Json<ProviderNodeResponse>> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    // Confirms the provider exists before writing config for it — a
    // friendlier 404 than the FK constraint would give.
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM providers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    exists.ok_or(ApiError::NotFound)?;

    sqlx::query(
        "INSERT INTO provider_nodes (provider_id, node_name, default_storage, default_bridge)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (provider_id, node_name)
         DO UPDATE SET default_storage = EXCLUDED.default_storage,
                        default_bridge = EXCLUDED.default_bridge,
                        updated_at = NOW()",
    )
    .bind(id)
    .bind(&name)
    .bind(&req.default_storage)
    .bind(&req.default_bridge)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    get_one(AuthUser(claims), State(state), Path((id, name))).await
}

async fn remove(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
) -> ApiResult<StatusCode> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let result =
        sqlx::query("DELETE FROM provider_nodes WHERE provider_id = $1 AND node_name = $2")
            .bind(id)
            .bind(&name)
            .execute(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}
