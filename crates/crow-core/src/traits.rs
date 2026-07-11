use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::{
    error::{DriverError, ProviderError},
    types::*,
};

#[async_trait]
pub trait InfraProvider: Send + Sync {
    fn provider_type(&self) -> &'static str;
    fn name(&self) -> &str;

    async fn create_vm(&self, spec: VmSpec) -> Result<VmHandle, ProviderError>;
    async fn delete_vm(&self, handle: &VmHandle) -> Result<(), ProviderError>;
    async fn vm_status(&self, handle: &VmHandle) -> Result<VmStatus, ProviderError>;
    async fn start_vm(&self, handle: &VmHandle) -> Result<(), ProviderError>;
    async fn stop_vm(&self, handle: &VmHandle) -> Result<(), ProviderError>;
    /// Run a command inside the guest via the provider's remote-exec
    /// mechanism (e.g. the Proxmox QEMU guest agent) and return its stdout.
    async fn exec_in_vm(&self, handle: &VmHandle, command: &str) -> Result<String, ProviderError>;

    async fn create_volume(&self, spec: VolumeSpec) -> Result<VolumeHandle, ProviderError>;
    async fn delete_volume(&self, handle: &VolumeHandle) -> Result<(), ProviderError>;

    async fn create_network(&self, spec: NetworkSpec) -> Result<NetworkHandle, ProviderError>;
    async fn delete_network(&self, handle: &NetworkHandle) -> Result<(), ProviderError>;
}

#[async_trait]
pub trait NetworkProvider: Send + Sync {
    fn provider_type(&self) -> &'static str;
    fn name(&self) -> &str;

    async fn expose_http(&self, spec: HttpExposeSpec) -> Result<ExposeHandle, ProviderError>;
    async fn expose_tcp(&self, spec: TcpExposeSpec) -> Result<ExposeHandle, ProviderError>;
    async fn unexpose(&self, handle: &ExposeHandle) -> Result<(), ProviderError>;

    async fn provision_cert(&self, domain: &str) -> Result<CertHandle, ProviderError>;
    async fn revoke_cert(&self, handle: &CertHandle) -> Result<(), ProviderError>;
}

#[async_trait]
pub trait DnsProvider: Send + Sync {
    fn provider_type(&self) -> &'static str;
    fn name(&self) -> &str;

    async fn create_record(&self, spec: DnsRecordSpec) -> Result<DnsRecordHandle, ProviderError>;
    async fn delete_record(&self, handle: &DnsRecordHandle) -> Result<(), ProviderError>;
    async fn update_record(
        &self,
        handle: &DnsRecordHandle,
        spec: DnsRecordSpec,
    ) -> Result<(), ProviderError>;
}

/// IP address management — reserving a static IP (typically tied to a MAC
/// address via a DHCP static mapping) ahead of creating the VM that will
/// use it, so the IP is known deterministically before boot.
#[async_trait]
pub trait IpamProvider: Send + Sync {
    fn provider_type(&self) -> &'static str;
    fn name(&self) -> &str;

    async fn allocate_ip(&self, spec: IpAllocSpec) -> Result<IpAllocHandle, ProviderError>;
    async fn release_ip(&self, handle: &IpAllocHandle) -> Result<(), ProviderError>;
}

pub struct ProvisionCtx {
    pub infra: Arc<dyn InfraProvider>,
    pub network: Option<Arc<dyn NetworkProvider>>,
    pub dns: Option<Arc<dyn DnsProvider>>,
    pub ipam: Option<Arc<dyn IpamProvider>>,
    pub config: Value,
    pub project: String,
    pub resource_group: String,
    pub resource_name: String,
}

#[async_trait]
pub trait ResourceDriver: Send + Sync {
    fn resource_type(&self) -> &'static str;
    fn config_schema(&self) -> Value;

    async fn provision(&self, ctx: &ProvisionCtx) -> Result<ResourceHandle, DriverError>;
    async fn deprovision(
        &self,
        ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<(), DriverError>;
    async fn reconcile(
        &self,
        ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<ResourcePhase, DriverError>;
    async fn endpoints(&self, handle: &ResourceHandle) -> Result<Vec<Endpoint>, DriverError>;
    /// Takes `ctx` (not just `handle`) because some drivers need live
    /// provider access to retrieve credentials — e.g. K8sCluster fetches
    /// the kubeconfig via `ctx.infra.exec_in_vm` rather than storing it in
    /// the handle.
    async fn credentials(
        &self,
        ctx: &ProvisionCtx,
        handle: &ResourceHandle,
    ) -> Result<Value, DriverError>;
}
