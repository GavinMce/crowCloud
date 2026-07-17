use crow_core::types::{VmHandle, VolumeHandle, VolumeSpec};

use crate::client::ProxmoxClient;
use crate::error::ProxmoxError;

/// Highest `scsiN` index this project ever assigns. `scsi0` is always the
/// VM's own OS disk (set at clone time in `vm::create_vm`), so attached
/// disks start at `scsi1`.
const MAX_SCSI_SLOTS: u32 = 30;

/// Finds the lowest unused `scsiN` (N >= 1) slot in a VM's live config.
fn next_free_scsi_slot(config: &serde_json::Value) -> Option<String> {
    (1..=MAX_SCSI_SLOTS)
        .map(|i| format!("scsi{i}"))
        .find(|key| config.get(key).is_none())
}

/// Finds which `scsiN` key currently holds `volid` (its value looks like
/// `"<volid>,size=32G"`, so a prefix match on the string form is enough).
fn find_scsi_slot_for_volid(config: &serde_json::Value, volid: &str) -> Option<String> {
    let obj = config.as_object()?;
    obj.iter()
        .find(|(key, value)| {
            key.starts_with("scsi")
                && value
                    .as_str()
                    .is_some_and(|v| v == volid || v.starts_with(&format!("{volid},")))
        })
        .map(|(key, _)| key.clone())
}

/// The real volid Proxmox allocated for `disk_key`, read back from the VM's
/// config after a config PUT that set a bare `"<storage>:<size>"` value —
/// Proxmox rewrites that into `"<storage>:<real-name>,size=<size>G"` once
/// the disk actually exists.
fn volid_from_config(config: &serde_json::Value, disk_key: &str) -> Option<String> {
    config
        .get(disk_key)?
        .as_str()?
        .split(',')
        .next()
        .map(String::from)
}

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

/// Allocates a new disk directly on `vm_handle`'s VM by setting a fresh
/// `scsiN` config key to a bare `"<storage>:<size>"` value — Proxmox
/// interprets that as "allocate a new disk image of this size on this
/// storage and attach it here" (the same mechanism `vm::create_vm` already
/// relies on for the OS disk, just on a new slot instead of `scsi0`).
pub async fn attach_volume(
    client: &ProxmoxClient,
    default_storage: &str,
    vm_handle: &VmHandle,
    spec: &VolumeSpec,
) -> Result<VolumeHandle, ProxmoxError> {
    let vmid = &vm_handle.provider_id;
    let storage = spec.storage_pool.as_deref().unwrap_or(default_storage);

    let config: serde_json::Value = client
        .get(&format!("/nodes/{}/qemu/{vmid}/config", client.node))
        .await?;
    let disk_key = next_free_scsi_slot(&config).ok_or_else(|| ProxmoxError::Api {
        status: 409,
        message: format!("VM {vmid} has no free scsi disk slots"),
    })?;

    let config_upid: Option<String> = client
        .post_opt(
            &format!("/nodes/{}/qemu/{vmid}/config", client.node),
            &[(disk_key.as_str(), format!("{storage}:{}", spec.size_gib))],
        )
        .await?;
    if let Some(upid) = config_upid {
        client.wait_task(&upid, 60).await?;
    }

    // Proxmox rewrites the bare "storage:size" value into the real volid
    // once the disk exists — re-read config to learn it.
    let config: serde_json::Value = client
        .get(&format!("/nodes/{}/qemu/{vmid}/config", client.node))
        .await?;
    let volid = volid_from_config(&config, &disk_key).ok_or_else(|| {
        ProxmoxError::Parse(format!(
            "VM {vmid} config has no resolved volid for {disk_key} after attach"
        ))
    })?;

    Ok(VolumeHandle {
        provider_type: "proxmox".to_string(),
        provider_id: volid,
    })
}

/// Removes the config reference to `handle`'s disk from `vm_handle`'s VM.
/// Deliberately does not call the storage-content delete endpoint — the
/// disk survives, just detached, so it can be re-attached elsewhere later.
pub async fn detach_volume(
    client: &ProxmoxClient,
    vm_handle: &VmHandle,
    handle: &VolumeHandle,
) -> Result<(), ProxmoxError> {
    let vmid = &vm_handle.provider_id;
    let volid = &handle.provider_id;

    let config: serde_json::Value = client
        .get(&format!("/nodes/{}/qemu/{vmid}/config", client.node))
        .await?;
    let Some(disk_key) = find_scsi_slot_for_volid(&config, volid) else {
        // Already detached (or never was attached to this VM) — nothing to do.
        return Ok(());
    };

    let config_upid: Option<String> = client
        .post_opt(
            &format!("/nodes/{}/qemu/{vmid}/config", client.node),
            &[("delete", disk_key.as_str())],
        )
        .await?;
    if let Some(upid) = config_upid {
        client.wait_task(&upid, 60).await?;
    }
    Ok(())
}

/// Grows an attached disk to `new_size_gib`. Mirrors the absolute-size
/// resize call `vm::create_vm` already makes for the OS disk — Proxmox's
/// resize endpoint rejects a size smaller than the disk's current size, so
/// this is inherently grow-only; callers should still check before calling
/// to surface a clearer error than Proxmox's own.
pub async fn resize_volume(
    client: &ProxmoxClient,
    vm_handle: &VmHandle,
    handle: &VolumeHandle,
    new_size_gib: u64,
) -> Result<(), ProxmoxError> {
    let vmid = &vm_handle.provider_id;
    let volid = &handle.provider_id;

    let config: serde_json::Value = client
        .get(&format!("/nodes/{}/qemu/{vmid}/config", client.node))
        .await?;
    let disk_key = find_scsi_slot_for_volid(&config, volid).ok_or_else(|| ProxmoxError::Api {
        status: 404,
        message: format!("volid '{volid}' is not attached to VM {vmid}"),
    })?;

    client
        .put(
            &format!("/nodes/{}/qemu/{vmid}/resize", client.node),
            &[
                ("disk", disk_key.as_str()),
                ("size", &format!("{new_size_gib}G")),
            ],
        )
        .await
}
