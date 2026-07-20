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
    /// Which of the host's adopted nodes every VM in this cluster (control
    /// plane and workers alike) is provisioned on — v1 keeps a whole
    /// cluster on one node, matching the single-control-plane scope below.
    pub node: String,
    /// Proxmox template VMID every node VM (control plane and workers) is
    /// cloned from — same base-OS convention as `VirtualMachineSpec.image`.
    /// Needs a Linux cloud-init-ready template; nothing distribution-aware
    /// happens here beyond that.
    pub image: String,
    /// Unlike a plain VM's `ip_pool_ref`, this is required, not optional —
    /// worker join and the bootstrap callback both need a known-in-advance
    /// control plane address, so the control plane can't be DHCP-only the
    /// way a bare VM can.
    pub ip_pool_ref: ResourceRef,
    pub distribution: K8sDistribution,
    pub version: String,
    /// v1 only actually implements `count: 1` — kube-vip HA (`count: 3`)
    /// is modeled here for later but not yet built.
    pub control_plane: ControlPlaneSpec,
    pub workers: WorkerSpec,
    pub network: K8sNetworkSpec,
    /// Installs kube-prometheus-stack when true. Opt-in, not default —
    /// Prometheus is memory-hungry and this platform targets modest
    /// self-hosted hardware.
    #[serde(default)]
    pub monitoring: bool,
    /// Pre-shared K3s join token, generated once at creation time (not left
    /// to K3s's own auto-generation) so worker cloud-init never needs to
    /// read anything back from the control plane first.
    pub cluster_token: String,
    /// Per-cluster secret the control plane's cloud-init presents when it
    /// calls back to crow-api to report the cluster is up — see
    /// `crow-api`'s `routes::k8s_bootstrap`.
    pub bootstrap_secret: String,
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
    /// IP range handed to Cilium's native LB-IPAM for LoadBalancer services
    /// (Cilium replaces MetalLB — no separate load balancer controller).
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
