use tracing::info;

use crow_core::types::{NetworkHandle, NetworkSpec};

use crate::client::ProxmoxClient;
use crate::error::ProxmoxError;

/// Fleet-wide singletons, created lazily/idempotently on first use.
/// Proxmox SDN zone/controller ids have real length/charset constraints
/// (confirmed against the registered `pve-sdn-*-id` JSONSchema formats):
/// zone ids are alphanumeric only, max 8 characters; controller ids allow
/// `-`/`_` and up to 64.
const EVPN_CONTROLLER_ID: &str = "crowcloud";
const EVPN_ZONE_ID: &str = "crow";

/// Deterministic, short VNet id derived from the VNI. VNet ids share the
/// zone's alphanumeric-only, max-8-character constraint -- hex-encoding
/// the full 24-bit VNI range keeps this always exactly 7 characters
/// ("v" + 6 hex digits), where decimal would overflow it for large VNIs
/// (e.g. `"v16777215"` is 9 characters).
fn vnet_id(vni: u32) -> String {
    format!("v{vni:06x}")
}

/// Provisions this subnet's VXLAN/EVPN dataplane via Proxmox's SDN
/// subsystem (`/cluster/sdn/...`) -- the plain per-node
/// `/nodes/{node}/network` API (used for everything else in this crate)
/// has no VXLAN interface type at all (confirmed reading the real
/// `PVE::API2::Network` source: its `type` enum and parameter schema
/// have no `vxlan`/`vxlan-*` entries, and the endpoint rejects unknown
/// parameters outright), so SDN's own Controller/Zone/VNet objects are
/// the only real mechanism.
///
/// The Controller (a Proxmox SDN EVPN controller peering with VyOS as
/// external route-reflector via a plain `peers` IP -- confirmed it
/// doesn't need to be a Proxmox-managed fabric member) and Zone are
/// fleet-wide singletons, created once and shared by every
/// `PrivateSubnet`; only the VNet (carrying this subnet's own VNI as its
/// `tag`) is per-subnet. Creating the Controller makes Proxmox SDN start
/// generating `/etc/frr/frr.conf` itself (including `advertise-all-vni`
/// automatically) -- see `crow-cli iso proxmox build`'s post-install
/// hook for how the underlay OSPF config survives that.
///
/// No Subnet object is created -- that's Proxmox SDN's own IPAM/DHCP/
/// gateway feature, and this project already has independent IP
/// bookkeeping (`IpPool`/`IpClaim`); creating one would just be unused
/// surface area. `gateway`/`dns` stay purely informational, handed to
/// VMs via `IpPool`/`IpClaim`, same as before.
pub async fn create_network(
    client: &ProxmoxClient,
    bgp_asn: Option<u32>,
    bgp_route_reflector_ip: Option<&str>,
    spec: &NetworkSpec,
) -> Result<NetworkHandle, ProxmoxError> {
    let (bgp_asn, bgp_route_reflector_ip) = match (bgp_asn, bgp_route_reflector_ip) {
        (Some(asn), Some(ip)) => (asn, ip),
        _ => {
            return Err(ProxmoxError::Api {
                status: 400,
                message: "provider has no bgp_asn/bgp_route_reflector_ip configured -- both are \
                          required to create Proxmox SDN's EVPN controller for PrivateSubnet"
                    .to_string(),
            })
        }
    };

    ensure_controller(client, bgp_asn, bgp_route_reflector_ip).await?;
    ensure_zone(client).await?;

    let vnet = vnet_id(spec.vni);
    info!(
        "creating Proxmox SDN VNet '{vnet}' (VNI {}) in zone '{EVPN_ZONE_ID}'",
        spec.vni
    );
    create_sdn_object(
        client,
        "/cluster/sdn/vnets",
        &[
            ("vnet", vnet.clone()),
            ("zone", EVPN_ZONE_ID.to_string()),
            ("tag", spec.vni.to_string()),
        ],
    )
    .await?;

    reload_sdn(client).await?;

    // Once applied, a VNet becomes a real Linux bridge interface literally
    // named after its own id -- this plugs straight into the existing
    // `network_ref` -> `net0=virtio,bridge=<id>` flow in `vm.rs` unchanged.
    Ok(NetworkHandle {
        provider_type: "proxmox".to_string(),
        provider_id: vnet,
    })
}

/// Removes the VNet `create_network` created. The Controller/Zone are
/// never deleted here -- persistent shared fleet infrastructure, the
/// same relationship `vmbr0` itself has to individual VMs.
pub async fn delete_network(
    client: &ProxmoxClient,
    handle: &NetworkHandle,
) -> Result<(), ProxmoxError> {
    let vnet = &handle.provider_id;
    info!("deleting Proxmox SDN VNet '{vnet}'");
    client
        .delete(&format!("/cluster/sdn/vnets/{vnet}"), &[])
        .await?;
    reload_sdn(client).await
}

async fn ensure_controller(
    client: &ProxmoxClient,
    bgp_asn: u32,
    bgp_route_reflector_ip: &str,
) -> Result<(), ProxmoxError> {
    if exists(
        client,
        &format!("/cluster/sdn/controllers/{EVPN_CONTROLLER_ID}"),
    )
    .await?
    {
        return Ok(());
    }
    info!(
        "creating Proxmox SDN EVPN controller '{EVPN_CONTROLLER_ID}' \
         (asn {bgp_asn}, peers {bgp_route_reflector_ip})"
    );
    create_sdn_object(
        client,
        "/cluster/sdn/controllers",
        &[
            ("controller", EVPN_CONTROLLER_ID.to_string()),
            ("type", "evpn".to_string()),
            ("asn", bgp_asn.to_string()),
            ("peers", bgp_route_reflector_ip.to_string()),
        ],
    )
    .await
}

async fn ensure_zone(client: &ProxmoxClient) -> Result<(), ProxmoxError> {
    if exists(client, &format!("/cluster/sdn/zones/{EVPN_ZONE_ID}")).await? {
        return Ok(());
    }
    info!("creating Proxmox SDN EVPN zone '{EVPN_ZONE_ID}' (controller '{EVPN_CONTROLLER_ID}')");
    create_sdn_object(
        client,
        "/cluster/sdn/zones",
        &[
            ("zone", EVPN_ZONE_ID.to_string()),
            ("type", "evpn".to_string()),
            ("controller", EVPN_CONTROLLER_ID.to_string()),
        ],
    )
    .await
}

/// `true` if a GET against `path` succeeds, `false` on a 404, otherwise
/// propagates the error.
async fn exists(client: &ProxmoxClient, path: &str) -> Result<bool, ProxmoxError> {
    match client.get::<serde_json::Value>(path).await {
        Ok(_) => Ok(true),
        Err(ProxmoxError::Api { status: 404, .. }) => Ok(false),
        Err(e) => Err(e),
    }
}

/// POSTs a new SDN object, tolerating a concurrent create -- two
/// `PrivateSubnet`s reconciling for the first time simultaneously both
/// race to create the shared Controller/Zone. Proxmox's own handler
/// `die`s with "...already defined" on a duplicate id (confirmed reading
/// `PVE::API2::Network::SDN::{Controllers,Zones}.pm` directly); the exact
/// HTTP status for that isn't verified live, so this matches on the
/// message instead -- the same honest best-effort this codebase already
/// uses for anything not yet confirmed against real hardware.
async fn create_sdn_object(
    client: &ProxmoxClient,
    path: &str,
    params: &[(&str, String)],
) -> Result<(), ProxmoxError> {
    match client.post_opt::<_, serde_json::Value>(path, params).await {
        Ok(_) => Ok(()),
        Err(ProxmoxError::Api { message, .. }) if message.contains("already defined") => Ok(()),
        Err(e) => Err(e),
    }
}

/// Applies pending SDN config. Confirmed this is an async Proxmox task
/// (`PUT /cluster/sdn` returns a UPID via `$rpcenv->fork_worker(...)`),
/// unlike the synchronous per-node `PUT /nodes/{node}/network` apply
/// used elsewhere in this crate -- so it must be awaited via `wait_task`
/// rather than treated as complete on response.
async fn reload_sdn(client: &ProxmoxClient) -> Result<(), ProxmoxError> {
    let upid: String = client
        .put_returning("/cluster/sdn", &[] as &[(&str, &str)])
        .await?;
    client.wait_task(&upid, 60).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vnet_id_is_short_deterministic_and_alphanumeric() {
        assert_eq!(vnet_id(30), "v00001e");
        assert_eq!(vnet_id(30), vnet_id(30));
        assert!(vnet_id(30).chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn max_vni_vnet_id_still_fits_the_8_char_sdn_limit() {
        // Proxmox SDN vnet ids are alphanumeric only, max 8 characters
        // (confirmed against the registered `pve-sdn-vnet-id` format).
        let max_vni = 16_777_215u32; // VXLAN's 24-bit VNI ceiling
        let id = vnet_id(max_vni);
        assert_eq!(id, "vffffff");
        assert!(id.len() <= 8);
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn min_vni_vnet_id_is_still_valid() {
        let id = vnet_id(0);
        assert_eq!(id, "v000000");
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
