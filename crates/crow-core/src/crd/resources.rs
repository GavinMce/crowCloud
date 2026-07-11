use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::resource_group::ResourceRef;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub condition_type: String,
    pub status: String,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub last_transition_time: Option<String>,
}

// --- VirtualMachine ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "VirtualMachine",
    namespaced,
    status = "VirtualMachineStatus",
    shortname = "cvm",
    printcolumn = r#"{"name":"IP","type":"string","jsonPath":".status.ip"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct VirtualMachineSpec {
    /// `name` holds the Postgres `providers.name` value, not a Kubernetes object
    /// reference — there is no `Provider` custom resource today, so `namespace`
    /// is unused. The operator resolves this by querying Postgres directly.
    pub infra_provider_ref: ResourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_pool_ref: Option<ResourceRef>,
    pub cpu: u32,
    pub memory_gib: u32,
    pub disk_gib: u32,
    pub image: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct VirtualMachineStatus {
    pub phase: Option<String>,
    pub ip: Option<String>,
    pub provider_id: Option<String>,
    pub conditions: Vec<Condition>,
}

// --- K8sCluster ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "K8sCluster",
    namespaced,
    status = "K8sClusterStatus",
    shortname = "ck8s",
    printcolumn = r#"{"name":"Endpoint","type":"string","jsonPath":".status.endpoint"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct K8sClusterSpec {
    /// `name` holds the Postgres `providers.name` value (same convention as
    /// `VirtualMachineSpec.infra_provider_ref`).
    pub infra_provider_ref: ResourceRef,
    /// `name` holds the Postgres `providers.name` of an IPAM provider (e.g.
    /// an OPNsense provider row) used to reserve static IPs for cluster
    /// nodes — not a `Provider`/`IpPool` CRD reference (no `IpPool`
    /// controller exists). `None` falls back to provider-assigned DHCP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_pool_ref: Option<ResourceRef>,
    /// Only `K3s` is implemented today; `Rke2` is accepted by the schema but
    /// rejected at provision time.
    pub distribution: K8sDistribution,
    /// k3s version to pin via `INSTALL_K3S_VERSION` (empty string installs
    /// whatever `get.k3s.io` currently serves as latest).
    pub version: String,
    /// Numeric Proxmox template VMID to clone control-plane/worker nodes
    /// from (same convention as `VirtualMachineSpec.image`).
    pub image: String,
    pub control_plane: ControlPlaneSpec,
    pub workers: WorkerSpec,
    pub network: K8sNetworkSpec,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub enum K8sDistribution {
    K3s,
    Rke2,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ControlPlaneSpec {
    /// 1 = single node, 3 = HA with kube-vip
    pub count: u32,
    pub cpu: u32,
    pub memory_gib: u32,
    pub disk_gib: u32,
    /// Required when count > 1; kube-vip will hold this VIP
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vip: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerSpec {
    pub count: u32,
    pub cpu: u32,
    pub memory_gib: u32,
    pub disk_gib: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct K8sNetworkSpec {
    pub pod_cidr: String,
    pub service_cidr: String,
    /// IP range handed to MetalLB for LoadBalancer services
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lb_pool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lb_mode: Option<LbMode>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub enum LbMode {
    L2,
    Bgp,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct K8sClusterStatus {
    pub phase: Option<String>,
    pub endpoint: Option<String>,
    pub kubeconfig_secret: Option<String>,
    pub node_count: Option<u32>,
    pub conditions: Vec<Condition>,
}

// --- ObjectStore ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "ObjectStore",
    namespaced,
    status = "ObjectStoreStatus",
    shortname = "cos",
    printcolumn = r#"{"name":"Endpoint","type":"string","jsonPath":".status.endpoint"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct ObjectStoreSpec {
    pub infra_provider_ref: ResourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_pool_ref: Option<ResourceRef>,
    pub cpu: u32,
    pub memory_gib: u32,
    pub storage_gib: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ObjectStoreStatus {
    pub phase: Option<String>,
    pub endpoint: Option<String>,
    pub credentials_secret: Option<String>,
    pub conditions: Vec<Condition>,
}

// --- Database ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "Database",
    namespaced,
    status = "DatabaseStatus",
    shortname = "cdb",
    printcolumn = r#"{"name":"Engine","type":"string","jsonPath":".spec.engine"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSpec {
    pub infra_provider_ref: ResourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_pool_ref: Option<ResourceRef>,
    pub engine: DatabaseEngine,
    pub version: String,
    pub cpu: u32,
    pub memory_gib: u32,
    pub storage_gib: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub enum DatabaseEngine {
    Postgres,
    Mysql,
    Mariadb,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatus {
    pub phase: Option<String>,
    pub connection_string_secret: Option<String>,
    pub ip: Option<String>,
    pub conditions: Vec<Condition>,
}
