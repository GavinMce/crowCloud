use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use crow_core::crd::networking::{ExposedTargetKind, PublicIp, PublicIpSpec};
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
struct PublicIpRow {
    name: String,
    address: String,
    target_kind: ExposedTargetKind,
    target_name: String,
    phase: Option<String>,
}

#[derive(Serialize)]
struct PublicIpDetail {
    name: String,
    address: String,
    prefix: u8,
    target_kind: ExposedTargetKind,
    target_name: String,
    label: Option<String>,
    phase: Option<String>,
    message: Option<String>,
}

impl From<PublicIp> for PublicIpRow {
    fn from(public_ip: PublicIp) -> Self {
        let name = public_ip.name_any();
        let status = public_ip.status.unwrap_or_default();
        PublicIpRow {
            name,
            address: public_ip.spec.address,
            target_kind: public_ip.spec.target_kind,
            target_name: public_ip.spec.target_name,
            phase: status.phase,
        }
    }
}

impl From<PublicIp> for PublicIpDetail {
    fn from(public_ip: PublicIp) -> Self {
        let name = public_ip.name_any();
        let status = public_ip.status.unwrap_or_default();
        PublicIpDetail {
            name,
            address: public_ip.spec.address,
            prefix: public_ip.spec.prefix,
            target_kind: public_ip.spec.target_kind,
            target_name: public_ip.spec.target_name,
            label: public_ip.spec.label,
            phase: status.phase,
            message: status.message,
        }
    }
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<PublicIpRow>>> {
    let api: Api<PublicIp> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let list = api
        .list(&ListParams::default())
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let mut rows: Vec<PublicIpRow> = list.items.into_iter().map(Into::into).collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreatePublicIpRequest {
    name: String,
    address: String,
    prefix: u8,
    target_kind: ExposedTargetKind,
    target_name: String,
    label: Option<String>,
}

async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreatePublicIpRequest>,
) -> ApiResult<(StatusCode, Json<PublicIpDetail>)> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    validate(&req)?;

    let public_ip = PublicIp {
        metadata: ObjectMeta {
            name: Some(req.name.clone()),
            namespace: Some(VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        spec: PublicIpSpec {
            address: req.address.clone(),
            prefix: req.prefix,
            target_kind: req.target_kind.clone(),
            target_name: req.target_name.clone(),
            label: req.label.clone(),
        },
        status: None,
    };

    let api: Api<PublicIp> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let created = api
        .create(&PostParams::default(), &public_ip)
        .await
        .map_err(|e| match &e {
            kube::Error::Api(ae) if ae.code == 409 => {
                ApiError::Conflict(format!("public IP '{}' already exists", req.name))
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
) -> ApiResult<Json<PublicIpDetail>> {
    let api: Api<PublicIp> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let public_ip = api
        .get_opt(&name)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(public_ip.into()))
}

/// No pre-check here for whether the reservation actually released on
/// VyOS -- the operator's own finalizer calls `release_ip` on the
/// provider before letting deletion complete, same as
/// `expose::remove`/`private_subnets::remove`.
async fn remove(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let api: Api<PublicIp> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    match api.delete(&name, &DeleteParams::default()).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(kube::Error::Api(e)) if e.code == 404 => Err(ApiError::NotFound),
        Err(e) => Err(ApiError::Internal(e.into())),
    }
}

/// Fails fast on two conditions the operator would otherwise only surface
/// asynchronously: `address` has to actually parse as an IP (the operator
/// hard-errors `InvalidAddress` and the reconcile silently retries
/// forever otherwise), and only `VirtualMachine` targets resolve an IP
/// today, same limitation `expose::validate` already enforces.
fn validate(req: &CreatePublicIpRequest) -> ApiResult<()> {
    if req.address.parse::<std::net::IpAddr>().is_err() {
        return Err(ApiError::BadRequest(format!(
            "'{}' is not a valid IP address",
            req.address
        )));
    }

    if !matches!(req.target_kind, ExposedTargetKind::VirtualMachine) {
        return Err(ApiError::BadRequest(
            "only target_kind=VirtualMachine is supported today".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(address: &str, target_kind: ExposedTargetKind) -> CreatePublicIpRequest {
        CreatePublicIpRequest {
            name: "wan-1".into(),
            address: address.into(),
            prefix: 24,
            target_kind,
            target_name: "vm-abc".into(),
            label: None,
        }
    }

    #[test]
    fn accepts_a_valid_address_and_vm_target() {
        assert!(validate(&req("10.0.202.50", ExposedTargetKind::VirtualMachine)).is_ok());
    }

    #[test]
    fn rejects_an_unparseable_address() {
        assert!(validate(&req("not-an-ip", ExposedTargetKind::VirtualMachine)).is_err());
    }

    #[test]
    fn rejects_non_vm_target_kinds() {
        for kind in [
            ExposedTargetKind::K8sCluster,
            ExposedTargetKind::ObjectStore,
            ExposedTargetKind::Database,
        ] {
            assert!(validate(&req("10.0.202.50", kind)).is_err());
        }
    }
}
