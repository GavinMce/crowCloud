use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::ResourceRef;

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
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
    /// Which of the host's adopted Proxmox nodes to provision on — the
    /// host's connection config alone no longer implies a node (a host can
    /// have zero or several adopted nodes, see `provider_nodes`).
    pub node: String,
    pub ip_pool_ref: Option<ResourceRef>,
    /// Only meaningful when `ip_pool_ref` is set — ignored otherwise (no
    /// pool means DHCP on the node's default bridge, unconditionally).
    #[serde(default)]
    pub ip_mode: IpMode,
    /// Only meaningful when `ip_pool_ref` is set and `ip_mode` is `Static`.
    /// `None` means auto-assign the first free address in the pool's range.
    pub requested_ip: Option<String>,
    pub cpu: u32,
    pub memory_gib: u32,
    pub disk_gib: u32,
    pub image: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default, PartialEq)]
pub enum IpMode {
    /// Allocate a static address from the pool via `IpClaim` — the default,
    /// matching this field's pre-existing behavior before `ip_mode` existed.
    #[default]
    Static,
    /// Attach to the pool's bridge (so the VM lands on the right network
    /// segment) without allocating an address from the pool — the VM's own
    /// DHCP client handles addressing, e.g. via a router/firewall already
    /// serving that segment.
    Dhcp,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
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
    pub infra_provider_ref: ResourceRef,
    pub ip_pool_ref: Option<ResourceRef>,
    pub distribution: K8sDistribution,
    pub version: String,
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
    pub lb_pool: Option<String>,
    pub lb_mode: Option<LbMode>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub enum LbMode {
    L2,
    Bgp,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
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
