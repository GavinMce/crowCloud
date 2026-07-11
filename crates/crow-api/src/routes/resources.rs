use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use crow_core::crd::{
    resource_group::ResourceRef,
    resources::{
        ControlPlaneSpec, K8sCluster, K8sClusterSpec, K8sDistribution, K8sNetworkSpec,
        VirtualMachine, VirtualMachineSpec, WorkerSpec,
    },
};
use crow_provider_registry::{k8s_cluster_cr_name, vm_cr_name, VM_NAMESPACE};
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
    K8sCluster(CreateK8sClusterRequest),
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

#[derive(Deserialize)]
struct CreateK8sClusterRequest {
    name: String,
    provider_id: Uuid,
    /// Provider row for IP allocation (e.g. an OPNsense provider). Optional —
    /// omitting it leaves node IPs to provider-assigned DHCP.
    ipam_provider_id: Option<Uuid>,
    #[serde(default)]
    version: String,
    image: String,
    control_plane: ControlPlaneRequest,
    workers: WorkerRequest,
    #[serde(default = "default_pod_cidr")]
    pod_cidr: String,
    #[serde(default = "default_service_cidr")]
    service_cidr: String,
}

#[derive(Deserialize)]
struct ControlPlaneRequest {
    count: u32,
    cpu: u32,
    memory_gib: u32,
    disk_gib: u32,
    vip: Option<String>,
}

#[derive(Deserialize)]
struct WorkerRequest {
    count: u32,
    cpu: u32,
    memory_gib: u32,
    disk_gib: u32,
}

fn default_pod_cidr() -> String {
    "10.42.0.0/16".to_string()
}
fn default_service_cidr() -> String {
    "10.43.0.0/16".to_string()
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
        CreateResourceRequest::K8sCluster(k8s) => {
            create_k8s_cluster(state, project, rg, user_id, k8s).await
        }
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

/// `control_plane.vip` must be set when `control_plane.count > 1` — the
/// operator/driver enforce this too, but failing fast here avoids creating a
/// CR that will just sit in `Failed` after burning a create call.
async fn create_k8s_cluster(
    state: AppState,
    project: String,
    rg: String,
    user_id: Option<Uuid>,
    req: CreateK8sClusterRequest,
) -> ApiResult<(StatusCode, Json<ResourceResponse>)> {
    if req.control_plane.count > 1 && req.control_plane.vip.is_none() {
        return Err(ApiError::BadRequest(
            "control_plane.vip is required when control_plane.count > 1".to_string(),
        ));
    }

    let provider_name: String = sqlx::query_scalar("SELECT name FROM providers WHERE id = $1")
        .bind(req.provider_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    let ipam_provider_name: Option<String> = match req.ipam_provider_id {
        Some(id) => Some(
            sqlx::query_scalar("SELECT name FROM providers WHERE id = $1")
                .bind(id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| ApiError::Internal(e.into()))?
                .ok_or(ApiError::NotFound)?,
        ),
        None => None,
    };

    let spec_json = serde_json::json!({
        "name": req.name,
        "distribution": "K3s",
        "version": req.version,
        "image": req.image,
        "control_plane": {
            "count": req.control_plane.count,
            "cpu": req.control_plane.cpu,
            "memory_gib": req.control_plane.memory_gib,
            "disk_gib": req.control_plane.disk_gib,
            "vip": req.control_plane.vip,
        },
        "workers": {
            "count": req.workers.count,
            "cpu": req.workers.cpu,
            "memory_gib": req.workers.memory_gib,
            "disk_gib": req.workers.disk_gib,
        },
    });

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO resources
           (project, resource_group, name, resource_type, provider_id, spec, phase, created_by)
         VALUES ($1, $2, $3, 'k8s_cluster', $4, $5, 'Pending', $6)
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

    let cluster_cr = K8sCluster {
        metadata: ObjectMeta {
            name: Some(k8s_cluster_cr_name(id)),
            namespace: Some(VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        spec: K8sClusterSpec {
            infra_provider_ref: ResourceRef {
                name: provider_name,
                namespace: None,
            },
            ip_pool_ref: ipam_provider_name.map(|name| ResourceRef {
                name,
                namespace: None,
            }),
            distribution: K8sDistribution::K3s,
            version: req.version,
            image: req.image,
            control_plane: ControlPlaneSpec {
                count: req.control_plane.count,
                cpu: req.control_plane.cpu,
                memory_gib: req.control_plane.memory_gib,
                disk_gib: req.control_plane.disk_gib,
                vip: req.control_plane.vip,
            },
            workers: WorkerSpec {
                count: req.workers.count,
                cpu: req.workers.cpu,
                memory_gib: req.workers.memory_gib,
                disk_gib: req.workers.disk_gib,
            },
            network: K8sNetworkSpec {
                pod_cidr: req.pod_cidr,
                service_cidr: req.service_cidr,
                lb_pool: None,
                lb_mode: None,
            },
        },
        status: None,
    };

    let cluster_api: Api<K8sCluster> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    cluster_api
        .create(&PostParams::default(), &cluster_cr)
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
            resource_type: "k8s_cluster".into(),
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
    let (id, resource_type): (Uuid, String) = sqlx::query_as(
        "SELECT id, resource_type FROM resources WHERE project = $1 AND resource_group = $2 AND name = $3",
    )
    .bind(&project)
    .bind(&rg)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::NotFound)?;

    // Delete the CR; the operator's finalizer performs the provider cleanup and
    // the `DELETE FROM resources` (see crow-operator's virtual_machine/k8s_cluster::cleanup).
    let delete_result = match resource_type.as_str() {
        "vm" => {
            let vm_api: Api<VirtualMachine> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
            vm_api
                .delete(&vm_cr_name(id), &DeleteParams::default())
                .await
                .map(|_| ())
        }
        "k8s_cluster" => {
            let cluster_api: Api<K8sCluster> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
            cluster_api
                .delete(&k8s_cluster_cr_name(id), &DeleteParams::default())
                .await
                .map(|_| ())
        }
        other => {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "unknown resource_type '{other}' in resources row {id}"
            )))
        }
    };

    match delete_result {
        Ok(()) => {}
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
