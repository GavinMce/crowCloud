use serde::{Deserialize, Serialize};

use crate::client::ProxmoxClient;
use crate::error::ProxmoxError;

/// Proxmox's cluster-wide `/nodes` endpoint already returns per-node status
/// (cpu/mem/uptime) in one call — no need for a second per-node `/status`
/// request for what this needs. Numeric fields are optional because
/// Proxmox omits them for offline nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxmoxNodeSummary {
    pub node: String,
    pub status: String,
    pub cpu: Option<f64>,
    pub maxcpu: Option<u32>,
    pub mem: Option<u64>,
    pub maxmem: Option<u64>,
    pub uptime: Option<u64>,
}

pub async fn list_nodes(client: &ProxmoxClient) -> Result<Vec<ProxmoxNodeSummary>, ProxmoxError> {
    client.get("/nodes").await
}
