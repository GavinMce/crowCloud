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

/// L3VNI for the zone's own VRF. Confirmed live against a real Proxmox VE
/// 9.2.6 install: `vrf-vxlan` is listed `optional => 1` in the zone
/// property *schema*, but zone creation is actually rejected outright
/// without it ("missing value for required option 'vrf-vxlan'") -- the
/// schema's `optional` flag doesn't reflect this type's real
/// requiredness. Fixed at the top of the VNI range specifically so it's
/// never a value a real subnet would pick; `crow-operator`'s VNI
/// collision check also rejects a `PrivateSubnet` using this exact VNI
/// for the same reason. No `PrivateSubnet` L3 routing feature actually
/// depends on this VRF today -- it's created because Proxmox requires
/// it, not because anything here uses it (v1 stays a pure L2 segment).
const EVPN_ZONE_VRF_VNI: u32 = 16_777_214;

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
///
/// Confirmed live against a real Proxmox VE 9.2.6 install and a real
/// installed VyOS 1.5 route-reflector: Controller/Zone/VNet creation,
/// idempotent re-detection of an already-created object, and the reload
/// task all behave as this code expects -- see `EVPN_ZONE_VRF_VNI` and
/// `exists`'s own doc comments for two real bugs that testing caught and
/// fixed (a required-in-practice `vrf-vxlan` param the schema claimed
/// was optional, and `exists` never actually working since Proxmox
/// returns HTTP 500, not 404, for "doesn't exist"). Real BGP EVPN
/// session establishment was also confirmed: after clearing VyOS's
/// (now-optional, see `VyosBuildConfig::bgp_peer_password`) peer-group
/// password, `show bgp summary` showed `Established` with the real
/// Proxmox host within seconds, and creating a VNet produced a genuine
/// EVPN Type-3 route on VyOS's side (`RT:<asn>:<vni>`, next-hop the
/// Proxmox node's underlay IP) -- confirming the control plane a second
/// node would use to learn where to tunnel VNI traffic. Only literal
/// cross-node VM-to-VM reachability remains unverified, since this test
/// fleet has just the one Proxmox node.
pub async fn create_network(
    client: &ProxmoxClient,
    bgp_asn: Option<u32>,
    bgp_route_reflector_ip: Option<&str>,
    spec: &NetworkSpec,
) -> Result<NetworkHandle, ProxmoxError> {
    if spec.vni == EVPN_ZONE_VRF_VNI {
        return Err(ProxmoxError::Api {
            status: 400,
            message: format!(
                "VNI {EVPN_ZONE_VRF_VNI} is reserved for the fleet's own EVPN zone VRF -- choose a different VNI"
            ),
        });
    }

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
            ("vrf-vxlan", EVPN_ZONE_VRF_VNI.to_string()),
        ],
    )
    .await
}

/// `true` if a GET against `path` succeeds, `false` if the object doesn't
/// exist, otherwise propagates the error.
///
/// Confirmed live against a real Proxmox VE 9.2.6 install: a GET for a
/// nonexistent SDN controller/zone/vnet returns HTTP **500**, not 404,
/// with a message like `"sdn zone 'X' does not exist\n"` -- matching on
/// status code alone (the first version of this function did) never
/// actually detects "doesn't exist" at all, since Proxmox never sends
/// 404 for this. Matching on the message instead, the same pragmatic
/// approach `create_sdn_object` already uses for "already defined".
async fn exists(client: &ProxmoxClient, path: &str) -> Result<bool, ProxmoxError> {
    match client.get::<serde_json::Value>(path).await {
        Ok(_) => Ok(true),
        Err(ProxmoxError::Api { message, .. }) if message.contains("does not exist") => Ok(false),
        Err(e) => Err(e),
    }
}

/// POSTs a new SDN object, tolerating a concurrent create -- two
/// `PrivateSubnet`s reconciling for the first time simultaneously both
/// race to create the shared Controller/Zone. Confirmed live against a
/// real Proxmox VE 9.2.6 install: a duplicate id fails with HTTP 500 and
/// a message ending "...already defined" -- matched on message content
/// since Proxmox doesn't use a distinct status (e.g. 409) for this.
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

    #[tokio::test]
    async fn create_network_rejects_the_reserved_zone_vrf_vni() {
        // Confirmed live against a real Proxmox VE 9.2.6 install: the zone
        // itself requires a vrf-vxlan VNI (see EVPN_ZONE_VRF_VNI's own doc
        // comment) -- a PrivateSubnet claiming that exact VNI would either
        // collide with it or be silently confusing. Never reaches the
        // network (returns before any client call), so a throwaway client
        // pointed at a bogus URL is safe here.
        let client = ProxmoxClient::new("https://example.invalid", "u", "s", "pve", true).unwrap();
        let spec = NetworkSpec {
            name: "test".to_string(),
            cidr: None,
            vni: EVPN_ZONE_VRF_VNI,
        };
        let err = create_network(&client, Some(65000), Some("10.10.0.1"), &spec)
            .await
            .unwrap_err();
        assert!(matches!(err, ProxmoxError::Api { status: 400, .. }));
    }
}
