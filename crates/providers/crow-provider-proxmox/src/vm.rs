use std::net::IpAddr;

use serde::Deserialize;
use tokio::time::{sleep, Duration};
use tracing::info;

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
    spec: &VmSpec,
) -> Result<VmHandle, ProxmoxError> {
    // Proxmox template VMID is passed as the image field.
    let template_vmid: u32 = spec.image.parse().map_err(|_| ProxmoxError::Api {
        status: 400,
        message: format!(
            "image must be a numeric Proxmox template VMID, got '{}'",
            spec.image
        ),
    })?;

    // Allocate the next available VMID from the cluster.
    let vmid: u32 = client.get("/cluster/nextid").await?;
    info!("creating VM '{}' as VMID {vmid} from template {template_vmid}", spec.name);

    // Full clone from template — creates independent disks on the target storage.
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
    client.wait_task(&upid, 300).await?;

    // Build the VM config update.
    let bridge = spec.network_ref.as_deref().unwrap_or("vmbr0");
    let mut cfg: Vec<(String, String)> = vec![
        ("cores".into(), spec.cpu.to_string()),
        ("memory".into(), spec.memory_mib.to_string()),
        ("net0".into(), format!("virtio,bridge={bridge}")),
    ];

    // Cloud-init: upload snippets when full user-data / network-config YAML is provided;
    // fall back to Proxmox's built-in ipconfig0 when only a static IP is known.
    let mut cicustom_parts: Vec<String> = Vec::new();

    if let Some(ci) = &spec.cloud_init {
        cfg.push(("citype".into(), "nocloud".into()));

        if let Some(user_data) = &ci.user_data {
            let filename = format!("vm-{vmid}-user.yaml");
            upload_snippet(client, default_storage, &filename, user_data).await?;
            cicustom_parts.push(format!("user={default_storage}:snippets/{filename}"));
        }

        if let Some(net_cfg) = &ci.network_config {
            let filename = format!("vm-{vmid}-network.yaml");
            upload_snippet(client, default_storage, &filename, net_cfg).await?;
            cicustom_parts.push(format!("network={default_storage}:snippets/{filename}"));
        } else if let Some(ip) = spec.ip {
            cfg.push(("ipconfig0".into(), build_ipconfig(ip)));
        } else {
            cfg.push(("ipconfig0".into(), "ip=dhcp".into()));
        }
    } else if let Some(ip) = spec.ip {
        cfg.push(("citype".into(), "nocloud".into()));
        cfg.push(("ipconfig0".into(), build_ipconfig(ip)));
    } else {
        cfg.push(("citype".into(), "nocloud".into()));
        cfg.push(("ipconfig0".into(), "ip=dhcp".into()));
    }

    if !cicustom_parts.is_empty() {
        cfg.push(("cicustom".into(), cicustom_parts.join(",")));
    }

    // POST /config returns a UPID or null depending on Proxmox version.
    let _upid: Option<String> = {
        let resp: serde_json::Value = client
            .post(
                &format!("/nodes/{}/qemu/{vmid}/config", client.node),
                &cfg,
            )
            .await?;
        resp.as_str().map(String::from)
    };

    // Grow the primary disk to the requested size.
    // Proxmox resize only accepts increases; ignore errors if the template is already larger.
    let _ = client
        .put(
            &format!("/nodes/{}/qemu/{vmid}/resize", client.node),
            &[("disk", "scsi0"), ("size", &format!("{}G", spec.disk_gib))],
        )
        .await;

    // Start the VM.
    let upid: String = client
        .post(
            &format!("/nodes/{}/qemu/{vmid}/status/start", client.node),
            &[] as &[(&str, &str)],
        )
        .await?;
    client.wait_task(&upid, 60).await?;

    Ok(VmHandle {
        provider_type: "proxmox".to_string(),
        provider_id: vmid.to_string(),
        ip: spec.ip,
        name: spec.name.clone(),
    })
}

pub async fn delete_vm(client: &ProxmoxClient, handle: &VmHandle) -> Result<(), ProxmoxError> {
    let vmid = &handle.provider_id;

    // Attempt a graceful stop first; ignore errors if already stopped.
    let _ = client
        .post::<_, serde_json::Value>(
            &format!("/nodes/{}/qemu/{vmid}/status/stop", client.node),
            &[] as &[(&str, &str)],
        )
        .await;
    sleep(Duration::from_secs(5)).await;

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
    client.wait_task(&upid, 60).await
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

/// Upload a cloud-init file to Proxmox storage snippets via multipart upload.
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

/// Build a Proxmox ipconfig0 string with a /24 mask and .1 gateway heuristic.
/// Callers that have full CIDR/gateway info should provide network_config YAML instead.
fn build_ipconfig(ip: IpAddr) -> String {
    let gw = match ip {
        IpAddr::V4(v4) => {
            let mut o = v4.octets();
            o[3] = 1;
            IpAddr::V4(std::net::Ipv4Addr::from(o))
        }
        IpAddr::V6(v6) => IpAddr::V6(v6),
    };
    format!("ip={ip}/24,gw={gw}")
}
