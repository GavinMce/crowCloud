use async_trait::async_trait;
use crow_core::{traits::InfraProvider, types::*, ProviderError};

pub struct HetznerProvider {
    pub api_token: String,
    pub default_location: String,
    pub default_server_type: String,
}

#[async_trait]
impl InfraProvider for HetznerProvider {
    fn provider_type(&self) -> &'static str {
        "hetzner"
    }
    fn name(&self) -> &str {
        &self.default_location
    }

    async fn create_vm(&self, _spec: VmSpec) -> Result<VmHandle, ProviderError> {
        todo!()
    }
    async fn delete_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
        todo!()
    }
    async fn vm_status(&self, _handle: &VmHandle) -> Result<VmStatus, ProviderError> {
        todo!()
    }
    async fn start_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
        todo!()
    }
    async fn stop_vm(&self, _handle: &VmHandle) -> Result<(), ProviderError> {
        todo!()
    }
    async fn exec_in_vm(
        &self,
        _handle: &VmHandle,
        _command: &str,
    ) -> Result<String, ProviderError> {
        todo!()
    }
    async fn create_volume(&self, _spec: VolumeSpec) -> Result<VolumeHandle, ProviderError> {
        todo!()
    }
    async fn delete_volume(&self, _handle: &VolumeHandle) -> Result<(), ProviderError> {
        todo!()
    }
    async fn create_network(&self, _spec: NetworkSpec) -> Result<NetworkHandle, ProviderError> {
        todo!()
    }
    async fn delete_network(&self, _handle: &NetworkHandle) -> Result<(), ProviderError> {
        todo!()
    }
}
