use async_trait::async_trait;
use crow_core::{traits::InfraProvider, types::*, ProviderError};

pub struct ProxmoxProvider {
    pub url: String,
    pub token_id: String,
    pub token_secret: String,
    pub node: String,
    pub default_bridge: String,
    pub default_storage: String,
}

#[async_trait]
impl InfraProvider for ProxmoxProvider {
    fn provider_type(&self) -> &'static str {
        "proxmox"
    }
    fn name(&self) -> &str {
        &self.url
    }

    async fn create_vm(&self, _spec: VmSpec) -> Result<VmHandle, ProviderError> {
        todo!("proxmox create_vm")
    }
    async fn delete_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
        todo!("proxmox delete_vm")
    }
    async fn vm_status(&self, _handle: &VmHandle) -> Result<VmStatus, ProviderError> {
        todo!("proxmox vm_status")
    }
    async fn start_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
        todo!("proxmox start_vm")
    }
    async fn stop_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
        todo!("proxmox stop_vm")
    }
    async fn create_volume(&self, _spec: VolumeSpec) -> Result<VolumeHandle, ProviderError> {
        todo!("proxmox create_volume")
    }
    async fn delete_volume(&self, _handle: &VolumeHandle) -> Result<(), ProviderError> {
        todo!("proxmox delete_volume")
    }
    async fn create_network(&self, _spec: NetworkSpec) -> Result<NetworkHandle, ProviderError> {
        todo!("proxmox create_network")
    }
    async fn delete_network(&self, _handle: &NetworkHandle) -> Result<(), ProviderError> {
        todo!("proxmox delete_network")
    }
}
