use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use crow_core::crd::{
    common::ResourceRef,
    resources::{Disk, DiskSpec, VirtualMachine, VirtualMachineSpec},
};
use crow_provider_registry::{disk_cr_name, vm_cr_name, VM_NAMESPACE};
use kube::api::{Api, DeleteParams, ObjectMeta, Patch, PatchParams, PostParams};
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
        .route("/{name}", get(get_one).patch(update).delete(remove))
}

#[derive(Serialize, sqlx::FromRow)]
struct ResourceRow {
    id: Uuid,
    name: String,
    resource_type: String,
    provider_id: Option<Uuid>,
    phase: String,
    // Included so the frontend can, e.g., tell which disks are already
    // attached without an N+1 detail fetch per row.
    handle: Option<Value>,
    created_at: DateTime<Utc>,
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path(project): Path<String>,
) -> ApiResult<Json<Vec<ResourceRow>>> {
    let rows = sqlx::query_as::<_, ResourceRow>(
        "SELECT id, name, resource_type, provider_id, phase, handle, created_at
         FROM resources WHERE project = $1
         ORDER BY name",
    )
    .bind(&project)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(rows))
}

#[derive(Deserialize)]
#[serde(tag = "resource_type", rename_all = "snake_case")]
enum CreateResourceRequest {
    Vm(CreateVmRequest),
    Disk(CreateDiskRequest),
}

#[derive(Deserialize)]
struct CreateVmRequest {
    name: String,
    provider_id: Uuid,
    /// Which of the host's adopted nodes to provision on.
    node: String,
    cpu: u32,
    #[serde(default = "default_memory_mib")]
    memory_mib: u64,
    #[serde(default = "default_disk_gib")]
    disk_gib: u64,
    image: String,
    /// `IpPool` name to request a static address from. Like
    /// `infra_provider_ref`, this is a lookup key resolved by the operator's
    /// `IpClaim` reconciler, not a Kubernetes object reference.
    ip_pool: Option<String>,
}

fn default_memory_mib() -> u64 {
    2048
}
fn default_disk_gib() -> u64 {
    20
}

#[derive(Deserialize)]
struct CreateDiskRequest {
    name: String,
    provider_id: Uuid,
    /// Must match the target VM's node when `vm_name` is set.
    node: String,
    size_gib: u32,
    /// Attach immediately on creation — the `resources` name of a VM in the
    /// same project. Omit to create an unattached disk to assign later.
    vm_name: Option<String>,
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
    Path(project): Path<String>,
    Json(req): Json<CreateResourceRequest>,
) -> ApiResult<(StatusCode, Json<ResourceResponse>)> {
    let user_id = Uuid::parse_str(&claims.sub).ok();

    match req {
        CreateResourceRequest::Vm(vm) => create_vm(state, project, user_id, vm).await,
        CreateResourceRequest::Disk(disk) => create_disk(state, project, user_id, disk).await,
    }
}

/// `VirtualMachineSpec.memory_gib` has no sub-GiB precision, so a request whose
/// `memory_mib` isn't a whole number of GiB can't be represented faithfully —
/// reject it rather than silently rounding down.
async fn create_vm(
    state: AppState,
    project: String,
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
           (project, name, resource_type, provider_id, spec, phase, created_by)
         VALUES ($1, $2, 'vm', $3, $4, 'Pending', $5)
         RETURNING id",
    )
    .bind(&project)
    .bind(&req.name)
    .bind(req.provider_id)
    .bind(&spec_json)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.constraint() == Some("resources_project_name_key") => {
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
            node: req.node.clone(),
            ip_pool_ref: req.ip_pool.as_ref().map(|name| ResourceRef {
                name: name.clone(),
                namespace: None,
            }),
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

async fn create_disk(
    state: AppState,
    project: String,
    user_id: Option<Uuid>,
    req: CreateDiskRequest,
) -> ApiResult<(StatusCode, Json<ResourceResponse>)> {
    let provider_name: String = sqlx::query_scalar("SELECT name FROM providers WHERE id = $1")
        .bind(req.provider_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    if let Some(vm_name) = &req.vm_name {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM resources WHERE project = $1 AND name = $2 AND resource_type = 'vm')",
        )
        .bind(&project)
        .bind(vm_name)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
        if !exists {
            return Err(ApiError::BadRequest(format!(
                "no VM named '{vm_name}' in this project"
            )));
        }
    }

    let spec_json = serde_json::json!({
        "name": req.name,
        "size_gib": req.size_gib,
        "vm_name": req.vm_name,
    });

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO resources
           (project, name, resource_type, provider_id, spec, phase, created_by)
         VALUES ($1, $2, 'disk', $3, $4, 'Pending', $5)
         RETURNING id",
    )
    .bind(&project)
    .bind(&req.name)
    .bind(req.provider_id)
    .bind(&spec_json)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.constraint() == Some("resources_project_name_key") => {
            ApiError::Conflict(format!("resource '{}' already exists", req.name))
        }
        _ => ApiError::Internal(e.into()),
    })?;

    let disk_cr = Disk {
        metadata: ObjectMeta {
            name: Some(disk_cr_name(id)),
            namespace: Some(VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        spec: DiskSpec {
            infra_provider_ref: ResourceRef {
                name: provider_name,
                namespace: None,
            },
            node: req.node.clone(),
            size_gib: req.size_gib,
            vm_ref: req.vm_name.as_ref().map(|name| ResourceRef {
                name: name.clone(),
                namespace: None,
            }),
        },
        status: None,
    };

    let disk_api: Api<Disk> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    disk_api
        .create(&PostParams::default(), &disk_cr)
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
            resource_type: "disk".into(),
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
    Path((project, name)): Path<(String, String)>,
) -> ApiResult<Json<ResourceResponse>> {
    let row = sqlx::query_as::<_, ResourceDetailRow>(
        "SELECT id, name, resource_type, phase, handle, created_at
         FROM resources WHERE project = $1 AND name = $2",
    )
    .bind(&project)
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

/// `detach: true` clears `vm_ref` (mutually exclusive with `vm_name`).
/// `vm_name` attaches to that VM — rejected if already attached elsewhere.
/// `size_gib` grows the disk — rejected if it would shrink it. Each PATCH
/// call is expected to represent one action (attach, detach, or resize),
/// not a general partial update, so there's no "leave field unchanged"
/// ambiguity to resolve for `vm_name` being merely absent vs. explicitly null.
#[derive(Deserialize)]
struct UpdateDiskRequest {
    #[serde(default)]
    detach: bool,
    vm_name: Option<String>,
    size_gib: Option<u32>,
}

async fn update(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((project, name)): Path<(String, String)>,
    Json(req): Json<UpdateDiskRequest>,
) -> ApiResult<Json<ResourceResponse>> {
    if req.detach && req.vm_name.is_some() {
        return Err(ApiError::BadRequest(
            "detach and vm_name are mutually exclusive".to_string(),
        ));
    }

    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, resource_type FROM resources WHERE project = $1 AND name = $2")
            .bind(&project)
            .bind(&name)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    let (id, resource_type) = row.ok_or(ApiError::NotFound)?;
    if resource_type != "disk" {
        return Err(ApiError::BadRequest(
            "updates are only supported for disk resources".to_string(),
        ));
    }

    let disk_api: Api<Disk> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let disk_name = disk_cr_name(id);
    let disk = disk_api
        .get(&disk_name)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let mut spec_patch = serde_json::Map::new();

    if req.detach {
        spec_patch.insert("vmRef".to_string(), Value::Null);
    } else if let Some(vm_name) = &req.vm_name {
        if let Some(attached) = disk
            .status
            .as_ref()
            .and_then(|s| s.attached_vm_ref.as_ref())
        {
            if attached.name != *vm_name {
                return Err(ApiError::Conflict(format!(
                    "disk is already attached to '{}' — detach it before attaching elsewhere",
                    attached.name
                )));
            }
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM resources WHERE project = $1 AND name = $2 AND resource_type = 'vm')",
        )
        .bind(&project)
        .bind(vm_name)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
        if !exists {
            return Err(ApiError::BadRequest(format!(
                "no VM named '{vm_name}' in this project"
            )));
        }
        spec_patch.insert("vmRef".to_string(), serde_json::json!({ "name": vm_name }));
    }

    if let Some(size_gib) = req.size_gib {
        if size_gib < disk.spec.size_gib {
            return Err(ApiError::BadRequest("disks cannot be shrunk".to_string()));
        }
        spec_patch.insert("sizeGib".to_string(), serde_json::json!(size_gib));
    }

    if spec_patch.is_empty() {
        return Err(ApiError::BadRequest("no changes requested".to_string()));
    }

    disk_api
        .patch(
            &disk_name,
            &PatchParams::default(),
            &Patch::Merge(serde_json::json!({ "spec": spec_patch })),
        )
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let row = sqlx::query_as::<_, ResourceDetailRow>(
        "SELECT id, name, resource_type, phase, handle, created_at FROM resources WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

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
    Path((project, name)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, resource_type FROM resources WHERE project = $1 AND name = $2")
            .bind(&project)
            .bind(&name)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    let (id, resource_type) = row.ok_or(ApiError::NotFound)?;

    if resource_type == "disk" {
        return remove_disk(&state, id).await;
    }

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

async fn remove_disk(state: &AppState, id: Uuid) -> ApiResult<StatusCode> {
    let disk_api: Api<Disk> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let disk_name = disk_cr_name(id);

    if let Some(disk) = disk_api
        .get_opt(&disk_name)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
    {
        if disk
            .status
            .as_ref()
            .and_then(|s| s.attached_vm_ref.as_ref())
            .is_some()
        {
            return Err(ApiError::Conflict(
                "disk is still attached to a VM — detach it before deleting".to_string(),
            ));
        }
    }

    // Delete the CR; the operator's finalizer destroys any real backing
    // storage and performs the `DELETE FROM resources` (see
    // crow-operator's disk::cleanup).
    match disk_api.delete(&disk_name, &DeleteParams::default()).await {
        Ok(_) => {}
        Err(kube::Error::Api(e)) if e.code == 404 => {
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
