use std::net::IpAddr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use crow_core::types::{CloudInitConfig, VmHandle, VmSpec};
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
        .route("/:name", get(get_one).delete(remove))
}

#[derive(Serialize, sqlx::FromRow)]
struct ResourceRow {
    id: Uuid,
    name: String,
    resource_type: String,
    provider_id: Option<Uuid>,
    phase: String,
    created_at: DateTime<Utc>,
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((project, rg)): Path<(String, String)>,
) -> ApiResult<Json<Vec<ResourceRow>>> {
    let rows = sqlx::query_as::<_, ResourceRow>(
        "SELECT id, name, resource_type, provider_id, phase, created_at
         FROM resources WHERE project = $1 AND resource_group = $2
         ORDER BY name",
    )
    .bind(&project)
    .bind(&rg)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
#[serde(tag = "resource_type", rename_all = "snake_case")]
enum CreateResourceRequest {
    Vm(CreateVmRequest),
}

#[derive(Deserialize)]
struct CreateVmRequest {
    name: String,
    provider_id: Uuid,
    cpu: u32,
    #[serde(default = "default_memory_mib")]
    memory_mib: u64,
    #[serde(default = "default_disk_gib")]
    disk_gib: u64,
    image: String,
    ip: Option<IpAddr>,
    hostname: Option<String>,
    user_data: Option<String>,
}

fn default_memory_mib() -> u64 {
    2048
}
fn default_disk_gib() -> u64 {
    20
}

#[derive(Serialize)]
struct ResourceResponse {
    id: Uuid,
    name: String,
    resource_type: String,
    phase: String,
    handle: Option<Value>,
    created_at: DateTime<Utc>,
}

async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path((project, rg)): Path<(String, String)>,
    Json(req): Json<CreateResourceRequest>,
) -> ApiResult<(StatusCode, Json<ResourceResponse>)> {
    let user_id = Uuid::parse_str(&claims.sub).ok();

    match req {
        CreateResourceRequest::Vm(vm) => create_vm(state, project, rg, user_id, vm).await,
    }
}

async fn create_vm(
    state: AppState,
    project: String,
    rg: String,
    user_id: Option<Uuid>,
    req: CreateVmRequest,
) -> ApiResult<(StatusCode, Json<ResourceResponse>)> {
    let (provider_type, provider_config): (String, Value) =
        sqlx::query_as("SELECT provider_type, config FROM providers WHERE id = $1")
            .bind(req.provider_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?
            .ok_or(ApiError::NotFound)?;

    let provider = build_infra_provider(&provider_type, &provider_config)?;

    let cloud_init = req.hostname.as_ref().map(|h| CloudInitConfig {
        hostname: h.clone(),
        user_data: req.user_data.clone(),
        network_config: None,
    });

    let spec = VmSpec {
        name: req.name.clone(),
        cpu: req.cpu,
        memory_mib: req.memory_mib,
        disk_gib: req.disk_gib,
        image: req.image.clone(),
        ip: req.ip,
        cloud_init,
        network_ref: None,
    };

    let spec_json =
        serde_json::to_value(&spec).map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO resources
           (project, resource_group, name, resource_type, provider_id, spec, phase, created_by)
         VALUES ($1, $2, $3, 'vm', $4, $5, 'Pending', $6)
         RETURNING id",
    )
    .bind(&project)
    .bind(&rg)
    .bind(&req.name)
    .bind(req.provider_id)
    .bind(&spec_json)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db)
            if db.constraint() == Some("resources_project_resource_group_name_key") =>
        {
            ApiError::Conflict(format!("resource '{}' already exists", req.name))
        }
        _ => ApiError::Internal(e.into()),
    })?;

    let (phase, handle_json) = match provider.create_vm(spec).await {
        Ok(handle) => {
            let j = serde_json::to_value(&handle)
                .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
            ("Running".to_string(), Some(j))
        }
        Err(e) => {
            tracing::error!(resource_id = %id, "create_vm failed: {e}");
            (format!("Failed: {e}"), None)
        }
    };

    sqlx::query("UPDATE resources SET phase = $1, handle = $2, updated_at = NOW() WHERE id = $3")
        .bind(&phase)
        .bind(&handle_json)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let created_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM resources WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

    Ok((
        StatusCode::CREATED,
        Json(ResourceResponse {
            id,
            name: req.name,
            resource_type: "vm".into(),
            phase,
            handle: handle_json,
            created_at,
        }),
    ))
}

#[derive(sqlx::FromRow)]
struct ResourceDetailRow {
    id: Uuid,
    name: String,
    resource_type: String,
    phase: String,
    handle: Option<Value>,
    created_at: DateTime<Utc>,
}

async fn get_one(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((project, rg, name)): Path<(String, String, String)>,
) -> ApiResult<Json<ResourceResponse>> {
    let row = sqlx::query_as::<_, ResourceDetailRow>(
        "SELECT id, name, resource_type, phase, handle, created_at
         FROM resources WHERE project = $1 AND resource_group = $2 AND name = $3",
    )
    .bind(&project)
    .bind(&rg)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::NotFound)?;

    Ok(Json(ResourceResponse {
        id: row.id,
        name: row.name,
        resource_type: row.resource_type,
        phase: row.phase,
        handle: row.handle,
        created_at: row.created_at,
    }))
}

#[derive(sqlx::FromRow)]
struct ResourceProviderRow {
    provider_id: Option<Uuid>,
    handle: Option<Value>,
}

async fn remove(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((project, rg, name)): Path<(String, String, String)>,
) -> ApiResult<StatusCode> {
    let row = sqlx::query_as::<_, ResourceProviderRow>(
        "SELECT provider_id, handle FROM resources
         WHERE project = $1 AND resource_group = $2 AND name = $3",
    )
    .bind(&project)
    .bind(&rg)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::NotFound)?;

    // Best-effort provider cleanup.
    if let (Some(pid), Some(handle_val)) = (row.provider_id, row.handle) {
        let provider_row: Option<(String, Value)> =
            sqlx::query_as("SELECT provider_type, config FROM providers WHERE id = $1")
                .bind(pid)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| ApiError::Internal(e.into()))?;

        if let Some((ptype, cfg)) = provider_row {
            if let Ok(provider) = build_infra_provider(&ptype, &cfg) {
                if let Ok(handle) = serde_json::from_value::<VmHandle>(handle_val) {
                    if let Err(e) = provider.delete_vm(&handle).await {
                        tracing::warn!("delete_vm failed during resource removal: {e}");
                    }
                }
            }
        }
    }

    sqlx::query("DELETE FROM resources WHERE project = $1 AND resource_group = $2 AND name = $3")
        .bind(&project)
        .bind(&rg)
        .bind(&name)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(StatusCode::NO_CONTENT)
}
