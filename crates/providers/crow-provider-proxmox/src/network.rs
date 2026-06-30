use tracing::info;

use crow_core::types::{NetworkHandle, NetworkSpec};

use crate::client::ProxmoxClient;
use crate::error::ProxmoxError;

/// Creates a Linux bridge on the Proxmox node.
///
/// Proxmox network changes are written to `/etc/network/interfaces` and take effect
/// on next boot or after `ifreload -a`. The PUT /network call below applies pending
/// changes immediately (equivalent to `ifreload`). Callers should be aware that this
/// touches live node networking.
pub async fn create_network(
    client: &ProxmoxClient,
    spec: &NetworkSpec,
) -> Result<NetworkHandle, ProxmoxError> {
    let iface = spec.bridge.as_deref().unwrap_or(&spec.name);
    info!("creating Proxmox bridge '{iface}' on node {}", client.node);

    let mut params: Vec<(&str, String)> = vec![
        ("iface", iface.to_string()),
        ("type", "bridge".to_string()),
        ("autostart", "1".to_string()),
        ("bridge_stp", "off".to_string()),
        ("bridge_fd", "0".to_string()),
        ("comments", spec.name.clone()),
    ];

    if let Some(cidr) = &spec.cidr {
        // Parse "192.168.2.0/24" into address + netmask for Proxmox config.
        if let Some((addr, prefix)) = cidr.split_once('/') {
            params.push(("address", addr.to_string()));
            params.push(("netmask", prefix_to_mask(prefix)));
        }
    }

    if let Some(vlan) = spec.vlan_id {
        params.push(("bridge_vids", vlan.to_string()));
    }

    client
        .post::<_, serde_json::Value>(&format!("/nodes/{}/network", client.node), &params)
        .await?;

    // Apply the pending network configuration.
    client
        .put(
            &format!("/nodes/{}/network", client.node),
            &[] as &[(&str, &str)],
        )
        .await?;

    Ok(NetworkHandle {
        provider_type: "proxmox".to_string(),
        provider_id: iface.to_string(),
    })
}

/// Removes a Linux bridge from the Proxmox node and applies the change.
pub async fn delete_network(
    client: &ProxmoxClient,
    handle: &NetworkHandle,
) -> Result<(), ProxmoxError> {
    let iface = &handle.provider_id;
    info!("deleting Proxmox bridge '{iface}' on node {}", client.node);

    client
        .delete(&format!("/nodes/{}/network/{iface}", client.node), &[])
        .await?;

    client
        .put(
            &format!("/nodes/{}/network", client.node),
            &[] as &[(&str, &str)],
        )
        .await?;

    Ok(())
}

fn prefix_to_mask(prefix: &str) -> String {
    let bits: u32 = prefix.parse().unwrap_or(24).min(32);
    let mask = if bits == 0 {
        0u32
    } else {
        !0u32 << (32 - bits)
    };
    let [a, b, c, d] = mask.to_be_bytes();
    format!("{a}.{b}.{c}.{d}")
}
