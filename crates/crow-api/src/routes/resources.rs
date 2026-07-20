use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use crow_core::crd::{
    common::ResourceRef,
    networking::{IpPool, IpPoolSpec},
    resources::{
        ControlPlaneSpec, IpMode, K8sCluster, K8sClusterSpec, K8sDistribution, K8sNetworkSpec,
        LbMode, VirtualMachine, VirtualMachineSpec, WorkerSpec,
    },
};
use crow_core::types::{K8sClusterHandle, ResourceHandle as CoreResourceHandle};
use crow_provider_registry::{k8s_cluster_cr_name, vm_cr_name, VM_NAMESPACE};
use kube::api::{Api, DeleteParams, ObjectMeta, PostParams};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Uuid;
use std::net::Ipv4Addr;

use crate::{
    error::{ApiError, ApiResult},
    middleware::AuthUser,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{name}", get(get_one).delete(remove))
        .route("/{name}/kubeconfig", get(download_kubeconfig))
        .route("/{name}/metrics", get(super::k8s_metrics::cluster_metrics))
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
    Path(project): Path<String>,
) -> ApiResult<Json<Vec<ResourceRow>>> {
    let rows = sqlx::query_as::<_, ResourceRow>(
        "SELECT id, name, resource_type, provider_id, phase, created_at
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
    K8sCluster(CreateK8sClusterRequest),
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
    /// `IpPool` name. Like `infra_provider_ref`, this is a lookup key
    /// resolved by the operator's `IpClaim` reconciler, not a Kubernetes
    /// object reference. `None` means DHCP on the node's default bridge.
    ip_pool: Option<String>,
    /// Only meaningful when `ip_pool` is set. Defaults to `Static`.
    #[serde(default)]
    ip_mode: IpMode,
    /// Only meaningful when `ip_pool` is set and `ip_mode` is `Static`.
    /// `None` auto-assigns the first free address in the pool's range.
    requested_ip: Option<String>,
}

#[derive(Deserialize)]
struct CreateK8sClusterRequest {
    name: String,
    provider_id: Uuid,
    node: String,
    /// Proxmox template VMID — same convention as `CreateVmRequest.image`.
    image: String,
    /// Required (unlike a VM's `ip_pool`) — the control plane needs a
    /// known-in-advance static address for worker join and the bootstrap
    /// callback to work at all.
    ip_pool: String,
    /// Empty/omitted installs K3s's current stable.
    #[serde(default)]
    k3s_version: String,
    control_plane_cpu: u32,
    control_plane_memory_gib: u32,
    control_plane_disk_gib: u32,
    #[serde(default)]
    worker_count: u32,
    #[serde(default = "default_worker_cpu")]
    worker_cpu: u32,
    #[serde(default = "default_worker_memory_gib")]
    worker_memory_gib: u32,
    #[serde(default = "default_worker_disk_gib")]
    worker_disk_gib: u32,
    #[serde(default = "default_pod_cidr")]
    pod_cidr: String,
    #[serde(default = "default_service_cidr")]
    service_cidr: String,
    /// Cilium LB-IPAM range for LoadBalancer services (L2 mode only — see
    /// `crow-resource-k8s`'s cloud-init generator).
    lb_pool_cidr: Option<String>,
    #[serde(default)]
    monitoring: bool,
}

fn default_worker_cpu() -> u32 {
    2
}
fn default_worker_memory_gib() -> u32 {
    4
}
fn default_worker_disk_gib() -> u32 {
    40
}
fn default_pod_cidr() -> String {
    "10.42.0.0/16".to_string()
}
fn default_service_cidr() -> String {
    "10.43.0.0/16".to_string()
}

fn default_memory_mib() -> u64 {
    2048
}
fn default_disk_gib() -> u64 {
    20
}

/// Format/range sanity check only — whether the address is actually free is
/// racy and belongs to the operator's `IpClaim` reconciler, which re-checks
/// it against live claim state at bind time.
fn validate_requested_ip(pool: &IpPoolSpec, requested_ip: &str) -> ApiResult<()> {
    let ip: Ipv4Addr = requested_ip
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid IPv4 address '{requested_ip}'")))?;
    let start: Ipv4Addr = pool
        .range_start
        .parse()
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("pool has a malformed range_start")))?;
    let end: Ipv4Addr = pool
        .range_end
        .parse()
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("pool has a malformed range_end")))?;
    let gateway: Ipv4Addr = pool
        .gateway
        .parse()
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("pool has a malformed gateway")))?;

    if !(u32::from(start)..=u32::from(end)).contains(&u32::from(ip)) {
        return Err(ApiError::BadRequest(format!(
            "requested_ip '{ip}' is outside the pool's range ({start} - {end})"
        )));
    }
    if ip == gateway {
        return Err(ApiError::BadRequest(format!(
            "requested_ip '{ip}' is the pool's gateway"
        )));
    }
    Ok(())
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
        CreateResourceRequest::K8sCluster(k8s) => {
            create_k8s_cluster(state, project, user_id, k8s).await
        }
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

    if req.requested_ip.is_some() && req.ip_pool.is_none() {
        return Err(ApiError::BadRequest(
            "requested_ip requires ip_pool to be set".to_string(),
        ));
    }
    if req.ip_mode == IpMode::Dhcp && req.requested_ip.is_some() {
        return Err(ApiError::BadRequest(
            "requested_ip is only used with ip_mode Static".to_string(),
        ));
    }
    if let Some(requested_ip) = &req.requested_ip {
        // Pool name is only resolved by the operator, not stored as a real
        // reference — re-fetch it here purely to validate the request early
        // rather than let a bad address surface as a silently-stuck claim.
        let pool_api: Api<IpPool> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
        let pool_name = req.ip_pool.as_deref().unwrap_or_default();
        let pool = pool_api
            .get_opt(pool_name)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?
            .ok_or_else(|| ApiError::BadRequest(format!("IP pool '{pool_name}' not found")))?;
        validate_requested_ip(&pool.spec, requested_ip)?;
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
            ip_mode: req.ip_mode.clone(),
            requested_ip: req.requested_ip.clone(),
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

async fn create_k8s_cluster(
    state: AppState,
    project: String,
    user_id: Option<Uuid>,
    req: CreateK8sClusterRequest,
) -> ApiResult<(StatusCode, Json<ResourceResponse>)> {
    let provider_name: String = sqlx::query_scalar("SELECT name FROM providers WHERE id = $1")
        .bind(req.provider_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    // Same "resolve early, fail fast" reasoning as CreateVmRequest's
    // requested_ip check — confirms the pool exists before committing.
    let pool_api: Api<IpPool> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    pool_api
        .get_opt(&req.ip_pool)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| ApiError::BadRequest(format!("IP pool '{}' not found", req.ip_pool)))?;

    // Generated once, here, and baked into the CR permanently — see
    // K8sClusterSpec.cluster_token/bootstrap_secret's doc comments.
    let cluster_token = Uuid::new_v4().simple().to_string();
    let bootstrap_secret = Uuid::new_v4().simple().to_string();

    let spec_json = serde_json::json!({
        "name": req.name,
        "image": req.image,
        "k3s_version": req.k3s_version,
        "control_plane_cpu": req.control_plane_cpu,
        "control_plane_memory_gib": req.control_plane_memory_gib,
        "control_plane_disk_gib": req.control_plane_disk_gib,
        "worker_count": req.worker_count,
        "worker_cpu": req.worker_cpu,
        "worker_memory_gib": req.worker_memory_gib,
        "worker_disk_gib": req.worker_disk_gib,
        "pod_cidr": req.pod_cidr,
        "service_cidr": req.service_cidr,
        "monitoring": req.monitoring,
    });

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO resources
           (project, name, resource_type, provider_id, spec, phase, created_by)
         VALUES ($1, $2, 'k8s_cluster', $3, $4, 'Pending', $5)
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
            node: req.node.clone(),
            image: req.image.clone(),
            ip_pool_ref: ResourceRef {
                name: req.ip_pool.clone(),
                namespace: None,
            },
            distribution: K8sDistribution::K3s,
            version: req.k3s_version.clone(),
            control_plane: ControlPlaneSpec {
                count: 1,
                cpu: req.control_plane_cpu,
                memory_gib: req.control_plane_memory_gib,
                disk_gib: req.control_plane_disk_gib,
                vip: None,
            },
            workers: WorkerSpec {
                count: req.worker_count,
                cpu: req.worker_cpu,
                memory_gib: req.worker_memory_gib,
                disk_gib: req.worker_disk_gib,
            },
            network: K8sNetworkSpec {
                pod_cidr: req.pod_cidr.clone(),
                service_cidr: req.service_cidr.clone(),
                lb_pool: req.lb_pool_cidr.clone(),
                lb_mode: req.lb_pool_cidr.as_ref().map(|_| LbMode::L2),
            },
            monitoring: req.monitoring,
            cluster_token,
            bootstrap_secret,
        },
        status: None,
    };

    let k8s_api: Api<K8sCluster> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    k8s_api
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

    // Delete the CR; the operator's finalizer performs the provider cleanup and
    // the `DELETE FROM resources` (see crow-operator's virtual_machine::cleanup
    // and k8s_cluster::cleanup).
    let result = if resource_type == "k8s_cluster" {
        let k8s_api: Api<K8sCluster> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
        k8s_api
            .delete(&k8s_cluster_cr_name(id), &DeleteParams::default())
            .await
            .map(|_| ())
    } else {
        let vm_api: Api<VirtualMachine> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
        vm_api
            .delete(&vm_cr_name(id), &DeleteParams::default())
            .await
            .map(|_| ())
    };

    match result {
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

#[derive(Serialize)]
struct KubeconfigResponse {
    kubeconfig: String,
}

async fn download_kubeconfig(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((project, name)): Path<(String, String)>,
) -> ApiResult<Json<KubeconfigResponse>> {
    let handle_json: Option<Value> = sqlx::query_scalar(
        "SELECT handle FROM resources WHERE project = $1 AND name = $2 AND resource_type = 'k8s_cluster'",
    )
    .bind(&project)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::NotFound)?;

    let Some(handle_json) = handle_json else {
        return Err(ApiError::Conflict(
            "cluster is still bootstrapping — no kubeconfig yet".to_string(),
        ));
    };
    let resource_handle: CoreResourceHandle = serde_json::from_value(handle_json)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("corrupt resource handle: {e}")))?;
    let cluster_handle: K8sClusterHandle = serde_json::from_value(resource_handle.data)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("corrupt K8sCluster handle: {e}")))?;

    let kubeconfig = cluster_handle.kubeconfig.ok_or_else(|| {
        ApiError::Conflict("cluster is still bootstrapping — no kubeconfig yet".to_string())
    })?;

    Ok(Json(KubeconfigResponse { kubeconfig }))
}
