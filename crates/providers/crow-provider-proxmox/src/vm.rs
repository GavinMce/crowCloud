use std::net::IpAddr;

use serde::Deserialize;
use tracing::{info, warn};

use crow_core::types::{VmHandle, VmSpec, VmStatus};

use crate::client::ProxmoxClient;
use crate::error::ProxmoxError;

#[derive(Deserialize)]
struct QemuStatus {
    status: String,
}

pub async fn create_vm(
    client: &ProxmoxClient,
    default_storage: &str,
    default_bridge: &str,
    spec: &VmSpec,
) -> Result<VmHandle, ProxmoxError> {
    let template_vmid: u32 = spec.image.parse().map_err(|_| ProxmoxError::Api {
        status: 400,
        message: format!(
            "image must be a numeric Proxmox template VMID, got '{}'",
            spec.image
        ),
    })?;

    // Proxmox returns the next VMID as a JSON string (e.g. `{"data":"100"}`), not a number.
    let vmid_str: String = client.get("/cluster/nextid").await?;
    let vmid: u32 = vmid_str.parse().map_err(|_| {
        ProxmoxError::Parse(format!("invalid VMID from /cluster/nextid: {vmid_str}"))
    })?;
    info!(
        "creating VM '{}' as VMID {vmid} from template {template_vmid}",
        spec.name
    );

    // Full clone — independent disks on the target storage.
    let upid: String = client
        .post(
            &format!("/nodes/{}/qemu/{template_vmid}/clone", client.node),
            &[
                ("newid", vmid.to_string()),
                ("name", spec.name.clone()),
                ("full", "1".to_string()),
                ("storage", default_storage.to_string()),
                ("target", client.node.clone()),
            ],
        )
        .await?;

    if let Err(e) = client.wait_task(&upid, 300).await {
        warn!("clone of VMID {vmid} timed out or failed ({e}), cleaning up");
        let _ = client
            .delete(
                &format!("/nodes/{}/qemu/{vmid}", client.node),
                &[("purge", "1"), ("destroy-unreferenced-disks", "1")],
            )
            .await;
        return Err(e);
    }

    // Build VM config.
    let bridge = spec.network_ref.as_deref().unwrap_or(default_bridge);
    let mut cfg: Vec<(String, String)> = vec![
        ("cores".into(), spec.cpu.to_string()),
        ("memory".into(), spec.memory_mib.to_string()),
        ("net0".into(), format!("virtio,bridge={bridge}")),
        ("citype".into(), "nocloud".into()),
    ];

    let mut cicustom_parts: Vec<String> = Vec::new();

    if let Some(ci) = &spec.cloud_init {
        if let Some(user_data) = &ci.user_data {
            let filename = format!("vm-{vmid}-user.yaml");
            upload_snippet(client, default_storage, &filename, user_data).await?;
            cicustom_parts.push(format!("user={default_storage}:snippets/{filename}"));
        }
        if let Some(net_cfg) = &ci.network_config {
            let filename = format!("vm-{vmid}-network.yaml");
            upload_snippet(client, default_storage, &filename, net_cfg).await?;
            cicustom_parts.push(format!("network={default_storage}:snippets/{filename}"));
        }
    }

    // Only use Proxmox built-in ipconfig when no cicustom network snippet was provided.
    if cicustom_parts.iter().all(|p| !p.starts_with("network=")) {
        let ipconfig = spec
            .ip
            .map(build_ipconfig)
            .unwrap_or_else(|| "ip=dhcp".into());
        cfg.push(("ipconfig0".into(), ipconfig));
    }

    if !cicustom_parts.is_empty() {
        cfg.push(("cicustom".into(), cicustom_parts.join(",")));
    }

    // POST /config returns null (sync) or a UPID (async) depending on Proxmox version.
    let config_upid: Option<String> = client
        .post_opt(&format!("/nodes/{}/qemu/{vmid}/config", client.node), &cfg)
        .await?;
    if let Some(upid) = config_upid {
        client.wait_task(&upid, 60).await?;
    }

    // Grow the primary disk if requested; skip if disk_gib is 0.
    if spec.disk_gib > 0 {
        client
            .put(
                &format!("/nodes/{}/qemu/{vmid}/resize", client.node),
                &[("disk", "scsi0"), ("size", &format!("{}G", spec.disk_gib))],
            )
            .await?;
    }

    // Start the VM.
    let start_upid: String = client
        .post(
            &format!("/nodes/{}/qemu/{vmid}/status/start", client.node),
            &[] as &[(&str, &str)],
        )
        .await?;

    if let Err(e) = client.wait_task(&start_upid, 120).await {
        warn!("VM {vmid} failed to start ({e}), cleaning up");
        let handle = VmHandle {
            provider_type: "proxmox".to_string(),
            provider_id: vmid.to_string(),
            ip: spec.ip,
            name: spec.name.clone(),
        };
        let _ = delete_vm(client, &handle).await;
        return Err(e);
    }

    Ok(VmHandle {
        provider_type: "proxmox".to_string(),
        provider_id: vmid.to_string(),
        ip: spec.ip,
        name: spec.name.clone(),
    })
}

pub async fn delete_vm(client: &ProxmoxClient, handle: &VmHandle) -> Result<(), ProxmoxError> {
    let vmid = &handle.provider_id;

    // Gracefully stop and wait for halt before deleting; ignore errors (VM may already be stopped).
    let _ = stop_vm(client, handle).await;

    let upid = client
        .delete(
            &format!("/nodes/{}/qemu/{vmid}", client.node),
            &[("purge", "1"), ("destroy-unreferenced-disks", "1")],
        )
        .await?;

    if let Some(upid) = upid {
        client.wait_task(&upid, 120).await?;
    }
    Ok(())
}

pub async fn vm_status(
    client: &ProxmoxClient,
    handle: &VmHandle,
) -> Result<VmStatus, ProxmoxError> {
    let vmid = &handle.provider_id;
    let s: QemuStatus = client
        .get(&format!(
            "/nodes/{}/qemu/{vmid}/status/current",
            client.node
        ))
        .await?;
    Ok(match s.status.as_str() {
        "running" => VmStatus::Running,
        "stopped" | "paused" => VmStatus::Stopped,
        _ => VmStatus::Unknown,
    })
}

pub async fn start_vm(client: &ProxmoxClient, handle: &VmHandle) -> Result<(), ProxmoxError> {
    let vmid = &handle.provider_id;
    let upid: String = client
        .post(
            &format!("/nodes/{}/qemu/{vmid}/status/start", client.node),
            &[] as &[(&str, &str)],
        )
        .await?;
    client.wait_task(&upid, 120).await
}

pub async fn stop_vm(client: &ProxmoxClient, handle: &VmHandle) -> Result<(), ProxmoxError> {
    let vmid = &handle.provider_id;
    let upid: String = client
        .post(
            &format!("/nodes/{}/qemu/{vmid}/status/stop", client.node),
            &[] as &[(&str, &str)],
        )
        .await?;
    client.wait_task(&upid, 120).await
}

// --- helpers ---

async fn upload_snippet(
    client: &ProxmoxClient,
    storage: &str,
    filename: &str,
    content: &str,
) -> Result<(), ProxmoxError> {
    let form = reqwest::multipart::Form::new()
        .text("content", "snippets")
        .text("filename", filename.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(content.as_bytes().to_vec())
                .file_name(filename.to_string())
                .mime_str("application/octet-stream")
                .map_err(|e| ProxmoxError::Parse(e.to_string()))?,
        );
    let _: serde_json::Value = client
        .post_multipart(
            &format!("/nodes/{}/storage/{storage}/upload", client.node),
            form,
        )
        .await?;
    Ok(())
}

/// Build a Proxmox ipconfig0 string.
///
/// IPv4: heuristic /24 + .1 gateway — use cloud_init.network_config for full control.
/// IPv6: ip6=<addr>/64 only (no gateway derivable without subnet info).
fn build_ipconfig(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => {
            let mut o = v4.octets();
            o[3] = 1;
            let gw = IpAddr::V4(std::net::Ipv4Addr::from(o));
            format!("ip={ip}/24,gw={gw}")
        }
        IpAddr::V6(_) => {
            // IPv6 gateway is not derivable from the address alone; configure the
            // IP only. Use cloud_init.network_config for a full dual-stack setup.
            format!("ip6={ip}/64")
        }
    }
}
