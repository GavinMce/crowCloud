/// Fabric-wide network config shared between `iso proxmox build` and
/// `iso vyos build`/`iso vyos flavor` (#66).
///
/// Several values have to be typed identically across every one of
/// these commands for the fabric to actually work -- `bgp_peer_password`
/// mismatched between VyOS and a Proxmox host fails BGP peering
/// silently, and there was previously nothing enforcing they matched
/// beyond the operator remembering to type the same thing twice (or
/// more, for `bgp_asn`/`underlay_vlan`/`mgmt_vlan`/`trunk_mtu`/
/// `ospf_area`). `mgmt_gateway` is the same value under two different
/// names on each side already -- it's VyOS's own `mgmt_ip` (the
/// interface every Proxmox host's `--mgmt-gateway` points at).
///
/// Configured once via `crow iso fabric-configure`, persisted in the
/// same local config file as the cached fleet secret (`Config`), then
/// read automatically by the build commands -- any of these fields
/// still given explicitly as a flag on a build command takes priority
/// over the stored fabric config, which in turn takes priority over
/// prompting.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricConfig {
    pub underlay_vlan: u16,
    pub underlay_network: String,
    pub underlay_network_prefix: u8,
    pub mgmt_vlan: u16,
    pub mgmt_network: String,
    pub mgmt_network_prefix: u8,
    /// VyOS's own IP on the mgmt VLAN -- also every Proxmox host's
    /// default gateway on that same VLAN.
    pub mgmt_gateway: String,
    pub trunk_mtu: u32,
    pub ospf_area: String,
    pub bgp_asn: u32,
    pub bgp_peer_password: String,
    /// Only consumed by VyOS today (it forwards DNS for mgmt-VLAN
    /// hosts; Proxmox hosts just point resolv.conf at `mgmt_gateway`),
    /// kept here anyway since it's a fabric-wide network design
    /// decision, not a per-host one.
    pub dns_servers: Vec<String>,
}
