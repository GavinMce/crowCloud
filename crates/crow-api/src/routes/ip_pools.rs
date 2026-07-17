use std::net::Ipv4Addr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use crow_core::crd::networking::{IpPool, IpPoolSpec};
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
struct IpPoolRow {
    name: String,
    cidr: String,
    range_start: String,
    range_end: String,
    bridge: String,
    allocated: Option<u32>,
    available: Option<u32>,
}

#[derive(Serialize)]
struct IpPoolDetail {
    name: String,
    cidr: String,
    range_start: String,
    range_end: String,
    gateway: String,
    dns: Vec<String>,
    bridge: String,
    allocated: Option<u32>,
    available: Option<u32>,
}

impl From<IpPool> for IpPoolRow {
    fn from(pool: IpPool) -> Self {
        let name = pool.name_any();
        let status = pool.status.unwrap_or_default();
        IpPoolRow {
            name,
            cidr: pool.spec.cidr,
            range_start: pool.spec.range_start,
            range_end: pool.spec.range_end,
            bridge: pool.spec.bridge,
            allocated: status.allocated,
            available: status.available,
        }
    }
}

impl From<IpPool> for IpPoolDetail {
    fn from(pool: IpPool) -> Self {
        let name = pool.name_any();
        let status = pool.status.unwrap_or_default();
        IpPoolDetail {
            name,
            cidr: pool.spec.cidr,
            range_start: pool.spec.range_start,
            range_end: pool.spec.range_end,
            gateway: pool.spec.gateway,
            dns: pool.spec.dns,
            bridge: pool.spec.bridge,
            allocated: status.allocated,
            available: status.available,
        }
    }
}

async fn list(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<IpPoolRow>>> {
    let api: Api<IpPool> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let list = api
        .list(&ListParams::default())
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let mut rows: Vec<IpPoolRow> = list.items.into_iter().map(Into::into).collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateIpPoolRequest {
    name: String,
    cidr: String,
    range_start: String,
    range_end: String,
    gateway: String,
    #[serde(default)]
    dns: Vec<String>,
    bridge: String,
}

async fn create(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateIpPoolRequest>,
) -> ApiResult<(StatusCode, Json<IpPoolDetail>)> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    validate(&req)?;

    let pool = IpPool {
        metadata: ObjectMeta {
            name: Some(req.name.clone()),
            namespace: Some(VM_NAMESPACE.to_string()),
            ..Default::default()
        },
        spec: IpPoolSpec {
            cidr: req.cidr.clone(),
            range_start: req.range_start.clone(),
            range_end: req.range_end.clone(),
            gateway: req.gateway.clone(),
            dns: req.dns.clone(),
            bridge: req.bridge.clone(),
        },
        status: None,
    };

    let api: Api<IpPool> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let created = api
        .create(&PostParams::default(), &pool)
        .await
        .map_err(|e| match &e {
            kube::Error::Api(ae) if ae.code == 409 => {
                ApiError::Conflict(format!("IP pool '{}' already exists", req.name))
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
) -> ApiResult<Json<IpPoolDetail>> {
    let api: Api<IpPool> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let pool = api
        .get_opt(&name)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(pool.into()))
}

async fn remove(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    if !claims.is_admin {
        return Err(ApiError::Forbidden);
    }

    let api: Api<IpPool> = Api::namespaced(state.kube.clone(), VM_NAMESPACE);
    let pool = api
        .get_opt(&name)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or(ApiError::NotFound)?;

    if pool.status.as_ref().and_then(|s| s.allocated).unwrap_or(0) > 0 {
        return Err(ApiError::Conflict(format!(
            "IP pool '{name}' still has allocated addresses — release them before deleting"
        )));
    }

    match api.delete(&name, &DeleteParams::default()).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(kube::Error::Api(e)) if e.code == 404 => Err(ApiError::NotFound),
        Err(e) => Err(ApiError::Internal(e.into())),
    }
}

/// Cheap sanity checks before a broken pool ever reaches the allocator —
/// mirrors the IPv4-only, std-parsing approach the `ip_claim` controller
/// already uses rather than pulling in a CIDR crate for this alone.
fn validate(req: &CreateIpPoolRequest) -> ApiResult<()> {
    let (network, prefix) = parse_cidr(&req.cidr)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid CIDR '{}'", req.cidr)))?;

    let start = parse_v4(&req.range_start, "range_start")?;
    let end = parse_v4(&req.range_end, "range_end")?;
    let gateway = parse_v4(&req.gateway, "gateway")?;

    if u32::from(start) > u32::from(end) {
        return Err(ApiError::BadRequest(
            "range_start must not be after range_end".into(),
        ));
    }

    for (label, ip) in [
        ("range_start", start),
        ("range_end", end),
        ("gateway", gateway),
    ] {
        if !in_cidr(ip, network, prefix) {
            return Err(ApiError::BadRequest(format!(
                "{label} '{ip}' is not within {}",
                req.cidr
            )));
        }
    }

    for dns in &req.dns {
        dns.parse::<Ipv4Addr>()
            .map_err(|_| ApiError::BadRequest(format!("invalid DNS address '{dns}'")))?;
    }

    Ok(())
}

fn parse_v4(s: &str, field: &str) -> ApiResult<Ipv4Addr> {
    s.parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid IPv4 address for {field}: '{s}'")))
}

fn parse_cidr(cidr: &str) -> Option<(Ipv4Addr, u32)> {
    let (addr, prefix) = cidr.split_once('/')?;
    let addr: Ipv4Addr = addr.parse().ok()?;
    let prefix: u32 = prefix.parse().ok()?;
    if prefix > 32 {
        return None;
    }
    Some((addr, prefix))
}

fn in_cidr(ip: Ipv4Addr, network: Ipv4Addr, prefix: u32) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        !0u32 << (32 - prefix)
    };
    (u32::from(ip) & mask) == (u32::from(network) & mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(cidr: &str, start: &str, end: &str, gateway: &str) -> CreateIpPoolRequest {
        CreateIpPoolRequest {
            name: "test-pool".into(),
            cidr: cidr.into(),
            range_start: start.into(),
            range_end: end.into(),
            gateway: gateway.into(),
            dns: vec!["1.1.1.1".into()],
            bridge: "vmbr0".into(),
        }
    }

    #[test]
    fn accepts_a_well_formed_pool() {
        assert!(validate(&req(
            "10.20.0.0/24",
            "10.20.0.10",
            "10.20.0.20",
            "10.20.0.1"
        ))
        .is_ok());
    }

    #[test]
    fn rejects_a_range_outside_the_cidr() {
        assert!(validate(&req(
            "10.20.0.0/24",
            "10.30.0.10",
            "10.30.0.20",
            "10.20.0.1"
        ))
        .is_err());
    }

    #[test]
    fn rejects_a_reversed_range() {
        assert!(validate(&req(
            "10.20.0.0/24",
            "10.20.0.20",
            "10.20.0.10",
            "10.20.0.1"
        ))
        .is_err());
    }

    #[test]
    fn rejects_a_malformed_cidr() {
        assert!(validate(&req("not-a-cidr", "10.20.0.10", "10.20.0.20", "10.20.0.1")).is_err());
    }

    #[test]
    fn rejects_a_gateway_outside_the_cidr() {
        assert!(validate(&req(
            "10.20.0.0/24",
            "10.20.0.10",
            "10.20.0.20",
            "10.30.0.1"
        ))
        .is_err());
    }
}
