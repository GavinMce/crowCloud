/// Inputs for rendering a VyOS first-boot configuration script (#66).
///
/// Rendered as a sequence of `set` commands, not VyOS's native
/// curly-brace `config.boot` format. `set` command syntax is stable and
/// well-documented across VyOS versions; the raw `config.boot` grammar
/// has historically shifted between releases and isn't something worth
/// hand-generating without a live instance to validate against. This
/// script is meant to be applied once, on first boot, via `configure <
/// script` (or embedded into a `vyos-build` flavor's config-commands
/// list, if that mechanism accepts the same syntax -- unverified in this
/// environment, no `vyos-build` toolchain available to test against).
pub struct VyosBuildConfig {
    pub hostname: String,
    pub trunk_interface: String,
    pub uplink_interface: String,
    pub trunk_mtu: u32,
    pub underlay_vlan: u16,
    pub underlay_ip: String,
    pub underlay_prefix: u8,
    pub mgmt_vlan: u16,
    pub mgmt_ip: String,
    pub mgmt_prefix: u8,
    pub loopback_ip: String,
    pub uplink_dhcp: bool,
    pub uplink_ip: Option<String>,
    pub uplink_prefix: Option<u8>,
    pub uplink_gateway: Option<String>,
    pub ospf_area: String,
    pub underlay_network: String,
    pub underlay_network_prefix: u8,
    pub ssh_pubkey: String,
    pub bgp_asn: u32,
    pub bgp_peer_password: String,
}

pub fn render_configure_script(cfg: &VyosBuildConfig) -> String {
    let mut lines = Vec::new();

    lines.push(format!("set system host-name '{}'", cfg.hostname));

    // SSH-key-only access -- deliberately never emits a `set system
    // login user ... authentication plaintext-password` line. A build
    // with no `ssh_pubkey` is a caller bug, not something this function
    // silently tolerates by falling back to a password.
    lines.push(
        "set system login user vyos authentication public-keys admin-key key \
        '{}'"
            .replace("{}", &cfg.ssh_pubkey),
    );
    lines.push("set service ssh port '22'".to_string());
    lines.push("set service ssh disable-password-authentication".to_string());

    // Uplink
    lines.push(format!(
        "set interfaces ethernet {} description 'uplink'",
        cfg.uplink_interface
    ));
    if cfg.uplink_dhcp {
        lines.push(format!(
            "set interfaces ethernet {} address dhcp",
            cfg.uplink_interface
        ));
    } else {
        let ip = cfg
            .uplink_ip
            .as_deref()
            .expect("uplink_ip required when uplink_dhcp is false");
        let prefix = cfg
            .uplink_prefix
            .expect("uplink_prefix required when uplink_dhcp is false");
        lines.push(format!(
            "set interfaces ethernet {} address '{ip}/{prefix}'",
            cfg.uplink_interface
        ));
        if let Some(gw) = &cfg.uplink_gateway {
            lines.push(format!(
                "set protocols static route 0.0.0.0/0 next-hop '{gw}'"
            ));
        }
    }

    // Trunk: underlay + management VLANs
    lines.push(format!(
        "set interfaces ethernet {} description 'fabric trunk'",
        cfg.trunk_interface
    ));
    lines.push(format!(
        "set interfaces ethernet {} mtu '{}'",
        cfg.trunk_interface, cfg.trunk_mtu
    ));
    lines.push(format!(
        "set interfaces ethernet {} vif {} address '{}/{}'",
        cfg.trunk_interface, cfg.underlay_vlan, cfg.underlay_ip, cfg.underlay_prefix
    ));
    lines.push(format!(
        "set interfaces ethernet {} vif {} mtu '{}'",
        cfg.trunk_interface, cfg.underlay_vlan, cfg.trunk_mtu
    ));
    lines.push(format!(
        "set interfaces ethernet {} vif {} address '{}/{}'",
        cfg.trunk_interface, cfg.mgmt_vlan, cfg.mgmt_ip, cfg.mgmt_prefix
    ));

    // Loopback -- VTEP source / BGP router-id
    lines.push(format!(
        "set interfaces loopback lo address '{}/32'",
        cfg.loopback_ip
    ));

    // Underlay OSPF
    lines.push(format!(
        "set protocols ospf area {} network '{}/{}'",
        cfg.ospf_area, cfg.underlay_network, cfg.underlay_network_prefix
    ));

    // BGP EVPN route reflector, dynamic listen-range neighbors (not a
    // static per-host list -- see #66's design note on why), with
    // peer-group authentication so fabric membership requires the same
    // shared secret baked into every Proxmox host's FRR config (#66's
    // `--bgp-peer-password`), not merely L2 reachability to this VLAN.
    lines.push(format!("set protocols bgp system-as '{}'", cfg.bgp_asn));
    lines.push("set protocols bgp peer-group FABRIC remote-as 'internal'".to_string());
    lines.push(
        "set protocols bgp peer-group FABRIC address-family l2vpn-evpn nexthop-self".to_string(),
    );
    lines.push(
        "set protocols bgp peer-group FABRIC address-family l2vpn-evpn route-reflector-client"
            .to_string(),
    );
    lines.push(format!(
        "set protocols bgp peer-group FABRIC password '{}'",
        cfg.bgp_peer_password
    ));
    lines.push(format!(
        "set protocols bgp listen range '{}/{}' peer-group 'FABRIC'",
        cfg.underlay_network, cfg.underlay_network_prefix
    ));

    lines.push("commit".to_string());
    lines.push("save".to_string());

    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> VyosBuildConfig {
        VyosBuildConfig {
            hostname: "vyos-rr".into(),
            trunk_interface: "eth0".into(),
            uplink_interface: "eth1".into(),
            trunk_mtu: 9000,
            underlay_vlan: 10,
            underlay_ip: "10.255.10.1".into(),
            underlay_prefix: 24,
            mgmt_vlan: 20,
            mgmt_ip: "10.255.20.1".into(),
            mgmt_prefix: 24,
            loopback_ip: "10.255.0.1".into(),
            uplink_dhcp: false,
            uplink_ip: Some("192.168.1.50".into()),
            uplink_prefix: Some(24),
            uplink_gateway: Some("192.168.1.1".into()),
            ospf_area: "0".into(),
            underlay_network: "10.255.10.0".into(),
            underlay_network_prefix: 24,
            ssh_pubkey: "ssh-ed25519 AAAAtest".into(),
            bgp_asn: 65000,
            bgp_peer_password: "fabric-secret".into(),
        }
    }

    #[test]
    fn never_bakes_a_password_only_an_ssh_key() {
        let out = render_configure_script(&cfg());
        assert!(out.contains("authentication public-keys"));
        assert!(!out.contains("authentication plaintext-password"));
        assert!(out.contains("disable-password-authentication"));
    }

    #[test]
    fn uses_dynamic_bgp_listen_range_not_a_static_neighbor() {
        let out = render_configure_script(&cfg());
        assert!(out.contains("set protocols bgp listen range '10.255.10.0/24' peer-group 'FABRIC'"));
        assert!(!out.contains("set protocols bgp neighbor"));
    }

    #[test]
    fn requires_a_peer_group_password() {
        let out = render_configure_script(&cfg());
        assert!(out.contains("set protocols bgp peer-group FABRIC password 'fabric-secret'"));
    }

    #[test]
    fn sets_trunk_mtu_on_both_parent_and_vif() {
        let out = render_configure_script(&cfg());
        assert!(out.contains("set interfaces ethernet eth0 mtu '9000'"));
        assert!(out.contains("set interfaces ethernet eth0 vif 10 mtu '9000'"));
    }

    #[test]
    fn dhcp_uplink_skips_static_address_and_gateway() {
        let mut c = cfg();
        c.uplink_dhcp = true;
        c.uplink_ip = None;
        c.uplink_prefix = None;
        c.uplink_gateway = None;
        let out = render_configure_script(&c);
        assert!(out.contains("set interfaces ethernet eth1 address dhcp"));
        assert!(!out.contains("set protocols static route"));
    }

    #[test]
    fn ends_with_commit_and_save() {
        let out = render_configure_script(&cfg());
        let trimmed = out.trim_end();
        assert!(trimmed.ends_with("commit\nsave"));
    }
}
