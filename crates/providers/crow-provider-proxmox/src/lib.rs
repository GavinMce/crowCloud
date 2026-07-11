mod client;
mod error;
mod network;
pub mod ssh;
mod vm;
mod volume;

use async_trait::async_trait;
use crow_core::{traits::InfraProvider, types::*, ProviderError};

use client::ProxmoxClient;
use error::ProxmoxError;

pub struct ProxmoxProvider {
    client: ProxmoxClient,
    default_storage: String,
    /// Storage for cloud-init snippet uploads. Must be a file/directory-based
    /// storage with the "Snippets" content type enabled — LVM-thin storages
    /// (typically used for `default_storage`, VM disks) can't hold them.
    snippets_storage: String,
    default_bridge: String,
    /// Login user injected via `ciuser` and used for SSH-based exec_in_vm.
    ssh_user: String,
    /// OpenSSH-format private key, matching `ssh_public_key`, used to
    /// authenticate back into VMs this provider creates.
    ssh_private_key: String,
    /// OpenSSH-format public key line injected into every VM via the native
    /// `sshkeys` cloud-init field.
    ssh_public_key: String,
}

impl ProxmoxProvider {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: &str,
        token_id: &str,
        token_secret: &str,
        node: &str,
        default_storage: &str,
        snippets_storage: &str,
        default_bridge: &str,
        ssh_user: &str,
        ssh_private_key: &str,
        ssh_public_key: &str,
        tls_insecure: bool,
    ) -> Result<Self, ProviderError> {
        let client = ProxmoxClient::new(url, token_id, token_secret, node, tls_insecure)
            .map_err(|e: ProxmoxError| ProviderError::from(e))?;
        Ok(Self {
            client,
            default_storage: default_storage.to_string(),
            snippets_storage: snippets_storage.to_string(),
            default_bridge: default_bridge.to_string(),
            ssh_user: ssh_user.to_string(),
            ssh_private_key: ssh_private_key.to_string(),
            ssh_public_key: ssh_public_key.to_string(),
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
        vm::create_vm(
            &self.client,
            &self.default_storage,
            &self.snippets_storage,
            &self.default_bridge,
            &self.ssh_user,
            &self.ssh_public_key,
            &spec,
        )
        .await
        .map_err(Into::into)
    }

    async fn delete_vm(&self, handle: &VmHandle) -> Result<(), ProviderError> {
        vm::delete_vm(&self.client, handle)
            .await
            .map_err(Into::into)
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

    async fn exec_in_vm(&self, handle: &VmHandle, command: &str) -> Result<String, ProviderError> {
        vm::exec_in_vm(&self.ssh_user, &self.ssh_private_key, handle, command)
            .await
            .map_err(Into::into)
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
