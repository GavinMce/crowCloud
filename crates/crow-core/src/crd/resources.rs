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
    pub cpu: u32,
    pub memory_gib: u32,
    pub disk_gib: u32,
    pub image: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct VirtualMachineStatus {
    pub phase: Option<String>,
    pub ip: Option<String>,
    pub provider_id: Option<String>,
    pub conditions: Vec<Condition>,
}

// --- Disk ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "Disk",
    namespaced,
    status = "DiskStatus",
    shortname = "cdisk",
    printcolumn = r#"{"name":"Size","type":"integer","jsonPath":".spec.sizeGib"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct DiskSpec {
    /// Must match the target VM's host+node when `vm_ref` is set — a disk
    /// can only attach to a VM on the same Proxmox node its storage lives on.
    pub infra_provider_ref: ResourceRef,
    pub node: String,
    pub size_gib: u32,
    /// The VM (by its `resources` table name, within the same project) this
    /// disk should be attached to. `None` means unattached — a disk can
    /// exist with no real backing storage yet, since Proxmox ties every
    /// disk image to an owning VMID (see `crow-provider-proxmox`'s
    /// `attach_volume`). Once set, only clearing it back to `None` is
    /// supported — moving an attached disk directly to a different VM is
    /// not (detach first).
    pub vm_ref: Option<ResourceRef>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiskStatus {
    pub phase: Option<String>,
    /// The real Proxmox volid once attached (e.g. `local-lvm:vm-100-disk-1`).
    pub volid: Option<String>,
    /// The size actually applied to real storage — lags `spec.size_gib`
    /// while unattached (nothing real exists yet) or mid-resize.
    pub attached_size_gib: Option<u32>,
    /// Which VM the real storage is *actually* attached to right now —
    /// tracks applied state, distinct from `spec.vm_ref` (desired state),
    /// so a detach (`spec.vm_ref` cleared to `None`) still knows where to
    /// detach from, and so `virtual_machine.rs`'s cleanup can find every
    /// disk attached to a VM being deleted.
    pub attached_vm_ref: Option<ResourceRef>,
    pub message: Option<String>,
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
