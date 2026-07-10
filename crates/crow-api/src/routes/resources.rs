use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use crow_core::crd::{
    resource_group::ResourceRef,
    resources::{VirtualMachine, VirtualMachineSpec},
};
use crow_provider_registry::{vm_cr_name, VM_NAMESPACE};
use kube::api::{Api, DeleteParams, ObjectMeta, PostParams};
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
        .route("/", get(list).post(create))
        .route("/{name}", get(get_one).delete(remove))
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

/// `VirtualMachineSpec.memory_gib` has no sub-GiB precision, so a request whose
/// `memory_mib` isn't a whole number of GiB can't be represented faithfully —
/// reject it rather than silently rounding down.
async fn create_vm(
    state: AppState,
    project: String,
    rg: String,
    user_id: Option<Uuid>,
    req: CreateVmRequest,
) -> ApiResult<(StatusCode, Json<ResourceResponse>)> {
    if !req.memory_mib.is_multiple_of(1024) {
        return Err(ApiError::BadRequest(
            "memory_mib must be a whole number of GiB (a multiple of 1024)".to_string(),
        ));
    }

    let provider_name: String = sqlx::query_scalar("SELECT name FROM providers WHERE id = $1")
        .bind(req.provider_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    let spec_json = serde_json::json!({
        "name": req.name,
        "cpu": req.cpu,
        "memory_mib": req.memory_mib,
        "disk_gib": req.disk_gib,
        "image": req.image,
    });

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

    let vm_cr = VirtualMachine {
        metadata: ObjectMeta {
            name: Some(vm_cr_name(id)),
            namespace: Some(VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        spec: VirtualMachineSpec {
            infra_provider_ref: ResourceRef {
                name: provider_name,
                namespace: None,
            },
            ip_pool_ref: None,
            cpu: req.cpu,
            memory_gib: (req.memory_mib / 1024) as u32,
            disk_gib: req.disk_gib as u32,
            image: req.image.clone(),
        },
        status: None,
    };

    let vm_api: Api<VirtualMachine> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    vm_api
        .create(&PostParams::default(), &vm_cr)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let created_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM resources WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(ResourceResponse {
            id,
            name: req.name,
            resource_type: "vm".into(),
            phase: "Pending".into(),
            handle: None,
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

async fn remove(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((project, rg, name)): Path<(String, String, String)>,
) -> ApiResult<StatusCode> {
    let id: Uuid = sqlx::query_scalar(
        "SELECT id FROM resources WHERE project = $1 AND resource_group = $2 AND name = $3",
    )
    .bind(&project)
    .bind(&rg)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::NotFound)?;

    // Delete the CR; the operator's finalizer performs the provider cleanup and
    // the `DELETE FROM resources` (see crow-operator's virtual_machine::cleanup).
    let vm_api: Api<VirtualMachine> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    match vm_api
        .delete(&vm_cr_name(id), &DeleteParams::default())
        .await
    {
        Ok(_) => {}
        Err(kube::Error::Api(e)) if e.code == 404 => {
            // CR already gone (e.g. deleted out-of-band) — clean up the orphaned row.
            sqlx::query("DELETE FROM resources WHERE id = $1")
                .bind(id)
                .execute(&state.db)
                .await
                .map_err(|e| ApiError::Internal(e.into()))?;
        }
        Err(e) => return Err(ApiError::Internal(e.into())),
    }

    Ok(StatusCode::NO_CONTENT)
}
