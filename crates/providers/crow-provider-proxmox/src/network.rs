use tracing::info;

use crow_core::types::{NetworkHandle, NetworkSpec};

use crate::client::ProxmoxClient;
use crate::error::ProxmoxError;

/// Deterministic, short interface names derived from the VNI -- not the
/// subnet's own (arbitrary-length) Kubernetes CR name, which the previous
/// plain-bridge implementation used and could silently exceed Linux's
/// 15-byte `IFNAMSIZ` limit. The longest real VNI (`16777215`, VXLAN's
/// 24-bit ceiling) still fits comfortably either way.
fn vxlan_iface_name(vni: u32) -> String {
    format!("vxlan{vni}")
}

fn bridge_iface_name(vni: u32) -> String {
    format!("vxbr{vni}")
}

/// Provisions this subnet's VXLAN/EVPN dataplane on the node: a VXLAN
/// interface sourced from the node's own underlay-VLAN IP (its VTEP
/// address) carrying `spec.vni`, enslaved to a dedicated bridge that VMs
/// attach to via `network_ref`. No static remote-peer list
/// (`vxlan-remoteip`/`vxlan-svcnodeip`) is set -- FRR's zebra learns
/// remote VTEPs/MACs dynamically from BGP EVPN routes over the fabric's
/// existing FABRIC peering (see `crow-cli iso proxmox build`'s
/// post-install hook, `advertise-all-vni`) once this interface exists for
/// it to auto-discover and advertise.
///
/// Deliberately a pure L2 segment: no host IP/gateway is set on the
/// bridge, unlike the old plain-bridge implementation. `gateway`/`dns`
/// stay informational, handed to VMs via `IpPool`/`IpClaim` -- there's no
/// real routing/NAT path for private subnets yet regardless
/// (`ExposedEndpoint`'s CLI/API are still unimplemented stubs), and a
/// shared subnet spanning multiple nodes can't each put the same host IP
/// on their own local bridge without a real distributed-anycast-gateway
/// setup, which is out of scope here.
pub async fn create_network(
    client: &ProxmoxClient,
    underlay_ip: Option<&str>,
    spec: &NetworkSpec,
) -> Result<NetworkHandle, ProxmoxError> {
    let underlay_ip = underlay_ip.ok_or_else(|| ProxmoxError::Api {
        status: 400,
        message: format!(
            "node {:?} has no known underlay IP -- it must (re-)register with crowCloud \
             (or be re-adopted via the Nodes tab) before it can host a PrivateSubnet's VTEP",
            client.node
        ),
    })?;

    let vxlan_iface = vxlan_iface_name(spec.vni);
    let bridge_iface = bridge_iface_name(spec.vni);
    info!(
        "creating VXLAN VNI {} ('{vxlan_iface}' -> '{bridge_iface}') on node {}",
        spec.vni, client.node
    );

    client
        .post::<_, serde_json::Value>(
            &format!("/nodes/{}/network", client.node),
            &[
                ("iface", vxlan_iface.clone()),
                ("type", "vxlan".to_string()),
                ("vxlan-id", spec.vni.to_string()),
                ("vxlan-local-tunnelip", underlay_ip.to_string()),
                ("autostart", "1".to_string()),
                ("comments", spec.name.clone()),
            ],
        )
        .await?;

    client
        .post::<_, serde_json::Value>(
            &format!("/nodes/{}/network", client.node),
            &[
                ("iface", bridge_iface.clone()),
                ("type", "bridge".to_string()),
                ("bridge_ports", vxlan_iface),
                ("autostart", "1".to_string()),
                ("bridge_stp", "off".to_string()),
                ("bridge_fd", "0".to_string()),
                ("comments", spec.name.clone()),
            ],
        )
        .await?;

    // Applies both pending interfaces at once (equivalent to `ifreload`).
    client
        .put(
            &format!("/nodes/{}/network", client.node),
            &[] as &[(&str, &str)],
        )
        .await?;

    Ok(NetworkHandle {
        provider_type: "proxmox".to_string(),
        provider_id: bridge_iface,
    })
}

/// Removes both interfaces `create_network` created. The vxlan interface's
/// name is derived back from the bridge name (`handle.provider_id`) via
/// the same naming convention rather than stored separately.
pub async fn delete_network(
    client: &ProxmoxClient,
    handle: &NetworkHandle,
) -> Result<(), ProxmoxError> {
    let bridge_iface = &handle.provider_id;
    let vni_suffix = bridge_iface.strip_prefix("vxbr").ok_or_else(|| ProxmoxError::Parse(
        format!("network handle {bridge_iface:?} doesn't match the expected vxbr<vni> naming convention"),
    ))?;
    let vxlan_iface = format!("vxlan{vni_suffix}");
    info!(
        "deleting VXLAN network '{bridge_iface}' / '{vxlan_iface}' on node {}",
        client.node
    );

    client
        .delete(
            &format!("/nodes/{}/network/{bridge_iface}", client.node),
            &[],
        )
        .await?;
    client
        .delete(
            &format!("/nodes/{}/network/{vxlan_iface}", client.node),
            &[],
        )
        .await?;

    client
        .put(
            &format!("/nodes/{}/network", client.node),
            &[] as &[(&str, &str)],
        )
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vxlan_and_bridge_names_are_short_and_deterministic() {
        assert_eq!(vxlan_iface_name(30), "vxlan30");
        assert_eq!(bridge_iface_name(30), "vxbr30");
    }

    #[test]
    fn max_vni_names_still_fit_ifnamsiz() {
        // Linux interface names are capped at 15 bytes (IFNAMSIZ),
        // including the null terminator -- 14 usable characters.
        let max_vni = 16_777_215u32; // VXLAN's 24-bit VNI ceiling
        assert!(vxlan_iface_name(max_vni).len() <= 14);
        assert!(bridge_iface_name(max_vni).len() <= 14);
    }
}
