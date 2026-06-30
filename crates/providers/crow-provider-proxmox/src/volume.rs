use crow_core::types::{VolumeHandle, VolumeSpec};

use crate::client::ProxmoxClient;
use crate::error::ProxmoxError;

/// Creates a volume record for later attachment to a VM.
///
/// Proxmox does not expose a REST endpoint for creating standalone (unattached) disk images.
/// Disks are always associated with a VMID in Proxmox storage. crowCloud resource drivers
/// are expected to allocate actual disk space during VM creation (via the resize/disk
/// parameters on qemu config). This function records the intent as a handle so drivers
/// can refer to the allocation when they later provision a VM.
pub async fn create_volume(
    _client: &ProxmoxClient,
    default_storage: &str,
    spec: &VolumeSpec,
) -> Result<VolumeHandle, ProxmoxError> {
    let storage = spec.storage_pool.as_deref().unwrap_or(default_storage);
    Ok(VolumeHandle {
        provider_type: "proxmox".to_string(),
        // Encode intent as "<storage>:<name>:<size>G" so the driver can reconstruct it.
        provider_id: format!("{storage}:{}:{}G", spec.name, spec.size_gib),
    })
}

/// Deletes a Proxmox storage volume by its volid.
///
/// The `handle.provider_id` must be the raw Proxmox volid (e.g. `local-lvm:vm-100-disk-0`),
/// not the intent string produced by `create_volume`. Resource drivers that attach real
/// disks to VMs should store the resulting volid in the handle they persist.
pub async fn delete_volume(
    client: &ProxmoxClient,
    handle: &VolumeHandle,
) -> Result<(), ProxmoxError> {
    // volid format: <storage>:<content-type>/<name>  e.g. local-lvm:vm-100-disk-0
    let volid = &handle.provider_id;

    // Extract storage name from the volid prefix.
    let storage = volid.split(':').next().ok_or_else(|| ProxmoxError::Api {
        status: 400,
        message: format!("invalid volid '{volid}': expected '<storage>:<name>'"),
    })?;

    let encoded_volid = urlencoding::encode(volid);
    let upid = client
        .delete(
            &format!(
                "/nodes/{}/storage/{storage}/content/{encoded_volid}",
                client.node
            ),
            &[],
        )
        .await?;

    if let Some(upid) = upid {
        client.wait_task(&upid, 120).await?;
    }
    Ok(())
}
