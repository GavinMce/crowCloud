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
    /// A specific address to allocate instead of the first free one in the
    /// pool's range. If it's outside the range, is the gateway, or is
    /// already allocated, the claim stays unbound (see
    /// `IpClaimStatus.message`) rather than silently substituting a
    /// different address.
    pub requested_ip: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct IpClaimStatus {
    pub allocated_ip: Option<String>,
    pub phase: Option<String>,
    /// Human-readable reason the claim is unbound — pool exhausted, or the
    /// requested address specifically was unavailable.
    pub message: Option<String>,
}

// --- PrivateSubnet ---

/// Owns the actual network segment an `IpPool` only ever references by
/// bridge name. `IpPool`/`IpClaim` are pure IP-address bookkeeping on top
/// of a bridge that's assumed to already exist (see `IpPoolSpec.bridge`,
/// a plain string); nothing previously created that bridge as a managed
/// resource -- `InfraProvider::create_network` existed but was never
/// called by any controller. `PrivateSubnet` closes that gap: reconciling
/// it provisions the bridge via `create_network`, then owns a matching
/// `IpPool` (created with an owner reference, so it cascades) pointing at
/// the resulting bridge name, so a subnet and its address pool come into
/// existence together instead of needing to be wired up by hand.
#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "PrivateSubnet",
    namespaced,
    status = "PrivateSubnetStatus",
    shortname = "psubnet",
    printcolumn = r#"{"name":"Bridge","type":"string","jsonPath":".status.bridge"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct PrivateSubnetSpec {
    /// Holds the Postgres `providers.name` value, not a Kubernetes object
    /// reference -- same convention as `VirtualMachineSpec.infra_provider_ref`
    /// (there is no `Provider` custom resource today).
    pub infra_provider_ref: ResourceRef,
    /// Which of the provider's adopted nodes to create the bridge on.
    pub node: String,
    pub cidr: String,
    pub vlan_id: Option<u16>,
    pub gateway: String,
    pub dns: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrivateSubnetStatus {
    pub phase: Option<String>,
    /// The provider-side bridge name backing this subnet, resolved from
    /// `NetworkHandle.provider_id` -- this is the value the owned
    /// `IpPool.spec.bridge` points at.
    pub bridge: Option<String>,
    /// Name of the `IpPool` this subnet owns, once created.
    pub ip_pool_ref: Option<String>,
    pub message: Option<String>,
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
