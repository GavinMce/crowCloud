use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use crow_core::crd::networking::{
    ExposeProtocol, ExposeType, ExposedEndpoint, ExposedEndpointSpec, ExposedTargetKind,
};
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
struct ExposedEndpointRow {
    name: String,
    target_kind: ExposedTargetKind,
    target_name: String,
    expose_type: ExposeType,
    port: u16,
    phase: Option<String>,
    public_url: Option<String>,
}

#[derive(Serialize)]
struct ExposedEndpointDetail {
    name: String,
    target_kind: ExposedTargetKind,
    target_name: String,
    expose_type: ExposeType,
    domain: Option<String>,
    port: u16,
    public_port: Option<u16>,
    protocol: Option<ExposeProtocol>,
    tls: bool,
    phase: Option<String>,
    public_url: Option<String>,
    cert_expiry: Option<String>,
}

impl From<ExposedEndpoint> for ExposedEndpointRow {
    fn from(endpoint: ExposedEndpoint) -> Self {
        let name = endpoint.name_any();
        let status = endpoint.status.unwrap_or_default();
        ExposedEndpointRow {
            name,
            target_kind: endpoint.spec.target_kind,
            target_name: endpoint.spec.target_name,
            expose_type: endpoint.spec.expose_type,
            port: endpoint.spec.port,
            phase: status.phase,
            public_url: status.public_url,
        }
    }
}

impl From<ExposedEndpoint> for ExposedEndpointDetail {
    fn from(endpoint: ExposedEndpoint) -> Self {
        let name = endpoint.name_any();
        let status = endpoint.status.unwrap_or_default();
        ExposedEndpointDetail {
            name,
            target_kind: endpoint.spec.target_kind,
            target_name: endpoint.spec.target_name,
            expose_type: endpoint.spec.expose_type,
            domain: endpoint.spec.domain,
            port: endpoint.spec.port,
            public_port: endpoint.spec.public_port,
            protocol: endpoint.spec.protocol,
            tls: endpoint.spec.tls,
            phase: status.phase,
            public_url: status.public_url,
            cert_expiry: status.cert_expiry,
        }
    }
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ExposedEndpointRow>>> {
    let api: Api<ExposedEndpoint> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let list = api
        .list(&ListParams::default())
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let mut rows: Vec<ExposedEndpointRow> = list.items.into_iter().map(Into::into).collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateExposeRequest {
    name: String,
    target_kind: ExposedTargetKind,
    target_name: String,
    expose_type: ExposeType,
    domain: Option<String>,
    port: u16,
    public_port: Option<u16>,
    protocol: Option<ExposeProtocol>,
    #[serde(default)]
    tls: bool,
}

async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateExposeRequest>,
) -> ApiResult<(StatusCode, Json<ExposedEndpointDetail>)> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    validate(&req)?;

    let endpoint = ExposedEndpoint {
        metadata: ObjectMeta {
            name: Some(req.name.clone()),
            namespace: Some(VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        spec: ExposedEndpointSpec {
            target_kind: req.target_kind.clone(),
            target_name: req.target_name.clone(),
            expose_type: req.expose_type.clone(),
            domain: req.domain.clone(),
            port: req.port,
            public_port: req.public_port,
            protocol: req.protocol.clone(),
            tls: req.tls,
        },
        status: None,
    };

    let api: Api<ExposedEndpoint> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let created = api
        .create(&PostParams::default(), &endpoint)
        .await
        .map_err(|e| match &e {
            kube::Error::Api(ae) if ae.code == 409 => {
                ApiError::Conflict(format!("exposed endpoint '{}' already exists", req.name))
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
) -> ApiResult<Json<ExposedEndpointDetail>> {
    let api: Api<ExposedEndpoint> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let endpoint = api
        .get_opt(&name)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(endpoint.into()))
}

/// No pre-check here for whether the target still exists / is reachable --
/// the operator's own finalizer calls `unexpose` on the provider (removing
/// the Caddy site file or NAT rule) before letting deletion complete, same
/// as `private_subnets::remove`.
async fn remove(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let api: Api<ExposedEndpoint> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    match api.delete(&name, &DeleteParams::default()).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(kube::Error::Api(e)) if e.code == 404 => Err(ApiError::NotFound),
        Err(e) => Err(ApiError::Internal(e.into())),
    }
}

/// Fails fast on two conditions the operator would otherwise only surface
/// asynchronously (or, for the target_kind case, never surface at all --
/// `resolve_target_ip` just hard-errors `UnsupportedTargetKind` and the
/// reconcile silently retries forever): a domain is required for `Http`
/// exposure, and only `VirtualMachine` targets actually resolve an IP
/// today, even though the CRD schema accepts all four `ExposedTargetKind`
/// variants for forward-compatibility.
fn validate(req: &CreateExposeRequest) -> ApiResult<()> {
    if matches!(req.expose_type, ExposeType::Http) && req.domain.is_none() {
        return Err(ApiError::BadRequest(
            "domain is required when expose_type is Http".to_string(),
        ));
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

    fn req(
        target_kind: ExposedTargetKind,
        expose_type: ExposeType,
        domain: Option<&str>,
    ) -> CreateExposeRequest {
        CreateExposeRequest {
            name: "test-endpoint".into(),
            target_kind,
            target_name: "vm-abc".into(),
            expose_type,
            domain: domain.map(String::from),
            port: 8080,
            public_port: None,
            protocol: None,
            tls: true,
        }
    }

    #[test]
    fn accepts_http_with_a_domain() {
        assert!(validate(&req(
            ExposedTargetKind::VirtualMachine,
            ExposeType::Http,
            Some("example.com")
        ))
        .is_ok());
    }

    #[test]
    fn rejects_http_without_a_domain() {
        assert!(validate(&req(
            ExposedTargetKind::VirtualMachine,
            ExposeType::Http,
            None
        ))
        .is_err());
    }

    #[test]
    fn accepts_tcp_without_a_domain() {
        assert!(validate(&req(
            ExposedTargetKind::VirtualMachine,
            ExposeType::Tcp,
            None
        ))
        .is_ok());
    }

    #[test]
    fn accepts_udp_without_a_domain() {
        assert!(validate(&req(
            ExposedTargetKind::VirtualMachine,
            ExposeType::Udp,
            None
        ))
        .is_ok());
    }

    #[test]
    fn rejects_non_vm_target_kinds() {
        for kind in [
            ExposedTargetKind::K8sCluster,
            ExposedTargetKind::ObjectStore,
            ExposedTargetKind::Database,
        ] {
            assert!(validate(&req(kind, ExposeType::Tcp, None)).is_err());
        }
    }
}
