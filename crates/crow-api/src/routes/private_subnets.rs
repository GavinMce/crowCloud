use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use crow_core::crd::networking::{PrivateSubnet, PrivateSubnetSpec};
use crow_provider_registry::VM_NAMESPACE;
use kube::{
    api::{Api, DeleteParams, ListParams, ObjectMeta, PostParams},
    ResourceExt,
};
use serde::{Deserialize, Serialize};

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

#[derive(Serialize)]
struct PrivateSubnetRow {
    name: String,
    cidr: String,
    vni: u32,
    node: String,
    bridge: Option<String>,
    phase: Option<String>,
}

#[derive(Serialize)]
struct PrivateSubnetDetail {
    name: String,
    infra_provider_ref: String,
    node: String,
    cidr: String,
    vni: u32,
    gateway: String,
    dns: Vec<String>,
    bridge: Option<String>,
    ip_pool_ref: Option<String>,
    phase: Option<String>,
    message: Option<String>,
}

impl From<PrivateSubnet> for PrivateSubnetRow {
    fn from(subnet: PrivateSubnet) -> Self {
        let name = subnet.name_any();
        let status = subnet.status.unwrap_or_default();
        PrivateSubnetRow {
            name,
            cidr: subnet.spec.cidr,
            vni: subnet.spec.vni,
            node: subnet.spec.node,
            bridge: status.bridge,
            phase: status.phase,
        }
    }
}

impl From<PrivateSubnet> for PrivateSubnetDetail {
    fn from(subnet: PrivateSubnet) -> Self {
        let name = subnet.name_any();
        let status = subnet.status.unwrap_or_default();
        PrivateSubnetDetail {
            name,
            infra_provider_ref: subnet.spec.infra_provider_ref.name,
            node: subnet.spec.node,
            cidr: subnet.spec.cidr,
            vni: subnet.spec.vni,
            gateway: subnet.spec.gateway,
            dns: subnet.spec.dns,
            bridge: status.bridge,
            ip_pool_ref: status.ip_pool_ref,
            phase: status.phase,
            message: status.message,
        }
    }
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<PrivateSubnetRow>>> {
    let api: Api<PrivateSubnet> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let list = api
        .list(&ListParams::default())
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let mut rows: Vec<PrivateSubnetRow> = list.items.into_iter().map(Into::into).collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateSubnetRequest {
    name: String,
    infra_provider_ref: String,
    node: String,
    cidr: String,
    vni: u32,
    gateway: String,
    #[serde(default)]
    dns: Vec<String>,
}

async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateSubnetRequest>,
) -> ApiResult<(StatusCode, Json<PrivateSubnetDetail>)> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let subnet = PrivateSubnet {
        metadata: ObjectMeta {
            name: Some(req.name.clone()),
            namespace: Some(VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        spec: PrivateSubnetSpec {
            infra_provider_ref: crow_core::crd::common::ResourceRef {
                name: req.infra_provider_ref.clone(),
                namespace: None,
            },
            node: req.node.clone(),
            cidr: req.cidr.clone(),
            vni: req.vni,
            gateway: req.gateway.clone(),
            dns: req.dns.clone(),
        },
        status: None,
    };

    let api: Api<PrivateSubnet> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let created = api
        .create(&PostParams::default(), &subnet)
        .await
        .map_err(|e| match &e {
            kube::Error::Api(ae) if ae.code == 409 => {
                ApiError::Conflict(format!("private subnet '{}' already exists", req.name))
            }
            kube::Error::Api(ae) if (400..500).contains(&ae.code) => {
                ApiError::BadRequest(ae.message.clone())
            }
            _ => ApiError::Internal(e.into()),
        })?;

    Ok((StatusCode::CREATED, Json(created.into())))
}

async fn get_one(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<PrivateSubnetDetail>> {
    let api: Api<PrivateSubnet> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let subnet = api
        .get_opt(&name)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(subnet.into()))
}

/// No pre-check for bound `IpClaim`s here -- the operator's own finalizer
/// already does exactly that (returning `ClaimsStillBound`) and blocks
/// actual removal until they're released. Deleting the CR just starts
/// that process, same as VM deletion in `resources.rs`.
async fn remove(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let api: Api<PrivateSubnet> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    match api.delete(&name, &DeleteParams::default()).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(kube::Error::Api(e)) if e.code == 404 => Err(ApiError::NotFound),
        Err(e) => Err(ApiError::Internal(e.into())),
    }
}
