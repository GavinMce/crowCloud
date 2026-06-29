mod client;
mod error;
mod network;
mod vm;
mod volume;

use async_trait::async_trait;
use crow_core::{traits::InfraProvider, types::*, ProviderError};

use client::ProxmoxClient;
use error::ProxmoxError;

pub struct ProxmoxProvider {
    client: ProxmoxClient,
    default_storage: String,
}

impl ProxmoxProvider {
    pub fn new(
        url: &str,
        token_id: &str,
        token_secret: &str,
        node: &str,
        default_storage: &str,
        tls_insecure: bool,
    ) -> Result<Self, ProviderError> {
        let client = ProxmoxClient::new(url, token_id, token_secret, node, tls_insecure)
            .map_err(|e: ProxmoxError| ProviderError::from(e))?;
        Ok(Self {
            client,
            default_storage: default_storage.to_string(),
        })
    }
}

#[async_trait]
impl InfraProvider for ProxmoxProvider {
    fn provider_type(&self) -> &'static str {
        "proxmox"
    }

    fn name(&self) -> &str {
        &self.client.base
    }

    async fn create_vm(&self, spec: VmSpec) -> Result<VmHandle, ProviderError> {
        vm::create_vm(&self.client, &self.default_storage, &spec)
            .await
            .map_err(Into::into)
    }

    async fn delete_vm(&self, handle: &VmHandle) -> Result<(), ProviderError> {
        vm::delete_vm(&self.client, handle).await.map_err(Into::into)
    }

    async fn vm_status(&self, handle: &VmHandle) -> Result<VmStatus, ProviderError> {
        vm::vm_status(&self.client, handle)
            .await
            .map_err(Into::into)
    }

    async fn start_vm(&self, handle: &VmHandle) -> Result<(), ProviderError> {
        vm::start_vm(&self.client, handle).await.map_err(Into::into)
    }

    async fn stop_vm(&self, handle: &VmHandle) -> Result<(), ProviderError> {
        vm::stop_vm(&self.client, handle).await.map_err(Into::into)
    }

    async fn create_volume(&self, spec: VolumeSpec) -> Result<VolumeHandle, ProviderError> {
        volume::create_volume(&self.client, &self.default_storage, &spec)
            .await
            .map_err(Into::into)
    }

    async fn delete_volume(&self, handle: &VolumeHandle) -> Result<(), ProviderError> {
        volume::delete_volume(&self.client, handle)
            .await
            .map_err(Into::into)
    }

    async fn create_network(&self, spec: NetworkSpec) -> Result<NetworkHandle, ProviderError> {
        network::create_network(&self.client, &spec)
            .await
            .map_err(Into::into)
    }

    async fn delete_network(&self, handle: &NetworkHandle) -> Result<(), ProviderError> {
        network::delete_network(&self.client, handle)
            .await
            .map_err(Into::into)
    }
}
