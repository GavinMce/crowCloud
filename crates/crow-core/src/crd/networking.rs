use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::ResourceRef;

// --- IpPool / IpClaim ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "IpPool",
    namespaced,
    status = "IpPoolStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct IpPoolSpec {
    pub cidr: String,
    pub range_start: String,
    pub range_end: String,
    pub gateway: String,
    pub dns: Vec<String>,
    pub bridge: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct IpPoolStatus {
    pub allocated: Option<u32>,
    pub available: Option<u32>,
}

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "IpClaim",
    namespaced,
    status = "IpClaimStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct IpClaimSpec {
    pub pool_ref: ResourceRef,
    pub resource_kind: String,
    pub resource_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct IpClaimStatus {
    pub allocated_ip: Option<String>,
    pub phase: Option<String>,
}

// --- TunnelEndpoint ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "TunnelEndpoint",
    namespaced,
    status = "TunnelEndpointStatus",
    shortname = "ctun",
    printcolumn = r#"{"name":"Public IP","type":"string","jsonPath":".status.publicIp"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct TunnelEndpointSpec {
    pub vps_provider_ref: ResourceRef,
    pub server_type: String,
    pub location: String,
    pub wireguard_subnet: String,
    pub base_domain: String,
    pub acme_email: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct TunnelEndpointStatus {
    pub phase: Option<String>,
    pub public_ip: Option<String>,
    pub vps_resource_id: Option<String>,
    pub wireguard_status: Option<String>,
}

// --- ExposedEndpoint ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "ExposedEndpoint",
    namespaced,
    status = "ExposedEndpointStatus",
    printcolumn = r#"{"name":"URL","type":"string","jsonPath":".status.publicUrl"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct ExposedEndpointSpec {
    pub target_kind: ExposedTargetKind,
    pub target_name: String,
    pub expose_type: ExposeType,
    pub domain: Option<String>,
    pub port: u16,
    pub public_port: Option<u16>,
    pub protocol: Option<ExposeProtocol>,
    pub tls: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub enum ExposedTargetKind {
    VirtualMachine,
    K8sCluster,
    ObjectStore,
    Database,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub enum ExposeType {
    Http,
    Tcp,
    Udp,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub enum ExposeProtocol {
    Tcp,
    Udp,
    TcpUdp,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExposedEndpointStatus {
    pub phase: Option<String>,
    pub public_url: Option<String>,
    pub cert_expiry: Option<String>,
}

// --- CustomDomain ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "CustomDomain",
    namespaced,
    status = "CustomDomainStatus",
    printcolumn = r#"{"name":"Domain","type":"string","jsonPath":".spec.domain"}"#,
    printcolumn = r#"{"name":"Verified","type":"boolean","jsonPath":".status.verified"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct CustomDomainSpec {
    pub domain: String,
    pub target_kind: ExposedTargetKind,
    pub target_name: String,
    pub tls: bool,
    pub dns_provider_ref: Option<ResourceRef>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct CustomDomainStatus {
    pub phase: Option<String>,
    pub verified: Option<bool>,
    pub verified_at: Option<String>,
    pub cert_expiry: Option<String>,
}
