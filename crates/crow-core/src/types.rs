use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

// --- VM ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSpec {
    pub name: String,
    pub cpu: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    pub image: String,
    pub ip: Option<IpAddr>,
    /// Pin the NIC's MAC address (e.g. so a pre-reserved DHCP/static-mapping
    /// IP is guaranteed to land on this VM). `None` lets the provider
    /// auto-generate one.
    pub mac: Option<String>,
    pub cloud_init: Option<CloudInitConfig>,
    pub network_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudInitConfig {
    pub hostname: String,
    pub user_data: Option<String>,
    pub network_config: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmHandle {
    pub provider_type: String,
    pub provider_id: String,
    pub ip: Option<IpAddr>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VmStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Error(String),
    Unknown,
}

// --- Volume ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSpec {
    pub name: String,
    pub size_gib: u64,
    pub storage_pool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeHandle {
    pub provider_type: String,
    pub provider_id: String,
}

// --- Network ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSpec {
    pub name: String,
    pub cidr: Option<String>,
    pub vlan_id: Option<u16>,
    pub bridge: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHandle {
    pub provider_type: String,
    pub provider_id: String,
}

// --- Expose ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpExposeSpec {
    pub domain: String,
    pub target_ip: IpAddr,
    pub target_port: u16,
    pub tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpExposeSpec {
    pub target_ip: IpAddr,
    pub target_port: u16,
    pub public_port: u16,
    pub protocol: Protocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Protocol {
    Tcp,
    Udp,
    TcpUdp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposeHandle {
    pub provider_id: String,
    pub domain: Option<String>,
    pub public_port: Option<u16>,
}

// --- TLS ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertHandle {
    pub domain: String,
    pub provider_id: String,
    pub expiry: DateTime<Utc>,
}

// --- DNS ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecordSpec {
    pub name: String,
    pub record_type: DnsRecordType,
    pub value: String,
    pub ttl: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DnsRecordType {
    A,
    Aaaa,
    Cname,
    Txt,
    Mx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecordHandle {
    pub provider_id: String,
    pub zone_id: Option<String>,
    pub name: String,
    pub record_type: DnsRecordType,
}

// --- Resource ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHandle {
    pub resource_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourcePhase {
    Pending,
    ProvisioningInfra,
    Bootstrapping,
    HealthChecking,
    Ready,
    Degraded(String),
    Scaling,
    Upgrading,
    Deleting,
    Deleted,
    Failed(String),
}

impl std::fmt::Display for ResourcePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::ProvisioningInfra => write!(f, "ProvisioningInfra"),
            Self::Bootstrapping => write!(f, "Bootstrapping"),
            Self::HealthChecking => write!(f, "HealthChecking"),
            Self::Ready => write!(f, "Ready"),
            Self::Degraded(msg) => write!(f, "Degraded: {msg}"),
            Self::Scaling => write!(f, "Scaling"),
            Self::Upgrading => write!(f, "Upgrading"),
            Self::Deleting => write!(f, "Deleting"),
            Self::Deleted => write!(f, "Deleted"),
            Self::Failed(msg) => write!(f, "Failed: {msg}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub name: String,
    pub url: String,
    pub description: Option<String>,
}

// --- IPAM ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAllocSpec {
    pub hostname: String,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAllocHandle {
    pub ip: IpAddr,
    pub mac: String,
    pub provider_id: String,
}
