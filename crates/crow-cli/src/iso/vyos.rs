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
#[derive(Clone)]
pub struct VyosBuildConfig {
    pub hostname: String,
    pub trunk_interface: String,
    pub uplink_interface: String,
    pub trunk_mtu: u32,
    /// Pins the trunk to a fixed speed/duplex instead of auto-negotiation
    /// -- both fields are required together (VyOS rejects one without
    /// the other for a fixed-speed config). Confirmed against current
    /// VyOS docs: `speed` takes one of a fixed set of values (10/100/
    /// 1000/2500/5000/10000/25000/40000/50000/100000), `duplex` is
    /// 'full' or 'half'.
    pub trunk_speed: Option<String>,
    pub trunk_duplex: Option<String>,
    pub underlay_vlan: u16,
    pub underlay_ip: String,
    pub underlay_prefix: u8,
    pub mgmt_vlan: u16,
    pub mgmt_ip: String,
    pub mgmt_prefix: u8,
    pub mgmt_network: String,
    pub mgmt_network_prefix: u8,
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
    /// Recursive DNS forwarders the mgmt VLAN's hosts resolve through
    /// (this router acts as their forwarder -- see `mgmt_network`'s NAT
    /// rule below for why they need one at all). Confirmed live: not
    /// every public resolver is reachable from every network -- 9.9.9.9
    /// and 1.1.1.1 both timed out on the one this was first deployed on
    /// while 8.8.8.8 worked fine, so this is a configurable list with a
    /// sensible default rather than a single hardcoded address.
    pub dns_servers: Vec<String>,
    /// Defaults to `false` (key-only, matching the original design).
    /// Setting `true` skips `disable-password-authentication` -- an
    /// explicit escape hatch, not a silent fallback: added after a real
    /// incident where a malformed key commit left a box locked out over
    /// SSH (VyOS commits config sections somewhat independently, so
    /// `service ssh`'s disable-password-authentication can succeed even
    /// when the separate `system login` section containing the new key
    /// fails in the same commit). Keeping a password fallback available
    /// while validating key-based access on a given box is a reasonable
    /// choice, not a security regression to silently prevent.
    pub allow_password_auth: bool,
    /// crowCloud control plane's mgmt-VLAN address -- when both this and
    /// `crow_api_mgmt_port` are set, bakes in a NAT destination rule
    /// forwarding that same port on the uplink straight to it. This is
    /// specifically for reaching the control plane itself from the
    /// upstream LAN (e.g. during bootstrap, before it's up enough to
    /// configure an `ExposedEndpoint` for itself) -- a separate,
    /// bootstrap-level concern from `crow-provider-vyos`'s NAT rules,
    /// which the operator manages per-`ExposedEndpoint` at runtime.
    /// Optional: a fabric with no crowCloud instance yet has nothing to
    /// forward to.
    pub crow_api_mgmt_ip: Option<String>,
    pub crow_api_mgmt_port: Option<u16>,
}

pub fn render_configure_script(cfg: &VyosBuildConfig) -> String {
    let mut lines = Vec::new();

    lines.push(format!("set system host-name '{}'", cfg.hostname));

    // SSH-key-only access -- deliberately never emits a `set system
    // login user ... authentication plaintext-password` line. A build
    // with no `ssh_pubkey` is a caller bug, not something this function
    // silently tolerates by falling back to a password.
    //
    // Confirmed live (commit fails otherwise): VyOS wants `type` and
    // `key` as separate fields, not the key type left embedded as a
    // prefix on the `key` value the way the raw pubkey file has it.
    let mut parts = cfg.ssh_pubkey.split_whitespace();
    let key_type = parts.next().unwrap_or("ssh-ed25519");
    let key_data = parts.next().unwrap_or(&cfg.ssh_pubkey);
    lines.push(format!(
        "set system login user vyos authentication public-keys admin-key type '{key_type}'"
    ));
    lines.push(format!(
        "set system login user vyos authentication public-keys admin-key key '{key_data}'"
    ));
    lines.push("set service ssh port '22'".to_string());
    if !cfg.allow_password_auth {
        lines.push("set service ssh disable-password-authentication".to_string());
    }

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
    if let (Some(speed), Some(duplex)) = (&cfg.trunk_speed, &cfg.trunk_duplex) {
        lines.push(format!(
            "set interfaces ethernet {} speed '{}'",
            cfg.trunk_interface, speed
        ));
        lines.push(format!(
            "set interfaces ethernet {} duplex '{}'",
            cfg.trunk_interface, duplex
        ));
    }
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

    // Confirmed live: a fresh deployment had zero NAT rules configured
    // at all, so the mgmt VLAN (needed to fetch the seed image, install
    // packages, etc. during #67's seed election) had no internet egress
    // whatsoever despite this router itself having a working uplink.
    // The underlay VLAN deliberately does NOT get this -- it's pure
    // fabric transport, not meant to reach the internet.
    lines.push(format!(
        "set nat source rule 100 outbound-interface name '{}'",
        cfg.uplink_interface
    ));
    lines.push(format!(
        "set nat source rule 100 source address '{}/{}'",
        cfg.mgmt_network, cfg.mgmt_network_prefix
    ));
    lines.push("set nat source rule 100 translation address 'masquerade'".to_string());

    // Forwards the control plane's own port straight through from the
    // uplink -- rule 200, clear of both the egress rule above (100) and
    // the 1000+ range crow-provider-vyos's ExposedEndpoint reconciler
    // uses for its own dynamically-created rules, so the two mechanisms
    // can never collide on a rule number even though they both write to
    // the same `nat destination` tree.
    if let (Some(ip), Some(port)) = (&cfg.crow_api_mgmt_ip, cfg.crow_api_mgmt_port) {
        lines
            .push("set nat destination rule 200 description 'crowcloud-control-plane'".to_string());
        lines.push(format!(
            "set nat destination rule 200 inbound-interface name '{}'",
            cfg.uplink_interface
        ));
        lines.push("set nat destination rule 200 protocol 'tcp'".to_string());
        lines.push(format!(
            "set nat destination rule 200 destination port '{port}'"
        ));
        lines.push(format!(
            "set nat destination rule 200 translation address '{ip}'"
        ));
        lines.push(format!(
            "set nat destination rule 200 translation port '{port}'"
        ));
    }

    // Confirmed live: jumbo frames on the trunk (fabric) side meet a
    // standard 1500-MTU uplink -- without clamping, a remote server can
    // advertise a large MSS during the TCP handshake and then stall
    // silently sending oversized segments if any hop along the return
    // path drops the resulting ICMP "fragmentation needed" message
    // (a common NAT/PMTUD black hole). Clamping to the uplink's own
    // path MTU sidesteps needing PMTUD to work at all.
    lines.push(format!(
        "set interfaces ethernet {} ip adjust-mss 'clamp-mss-to-pmtu'",
        cfg.uplink_interface
    ));

    // Confirmed live: hosts on the mgmt VLAN had no working resolver --
    // this router forwards DNS for them rather than each host depending
    // on whatever (often stale or unreachable) resolver it inherited at
    // install time.
    lines.push(format!(
        "set service dns forwarding listen-address '{}'",
        cfg.mgmt_ip
    ));
    lines.push(format!(
        "set service dns forwarding allow-from '{}/{}'",
        cfg.mgmt_network, cfg.mgmt_network_prefix
    ));
    for server in &cfg.dns_servers {
        lines.push(format!("set service dns forwarding name-server '{server}'"));
    }

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
            trunk_speed: None,
            trunk_duplex: None,
            underlay_vlan: 10,
            underlay_ip: "10.255.10.1".into(),
            underlay_prefix: 24,
            mgmt_vlan: 20,
            mgmt_ip: "10.255.20.1".into(),
            mgmt_prefix: 24,
            mgmt_network: "10.255.20.0".into(),
            mgmt_network_prefix: 24,
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
            dns_servers: vec!["8.8.8.8".into(), "8.8.4.4".into()],
            allow_password_auth: false,
            crow_api_mgmt_ip: None,
            crow_api_mgmt_port: None,
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
    fn allow_password_auth_skips_the_disable_line() {
        let mut c = cfg();
        c.allow_password_auth = true;
        let out = render_configure_script(&c);
        assert!(!out.contains("disable-password-authentication"));
        // The key still gets configured either way -- this is "keep a
        // fallback available", not "don't bother with the key".
        assert!(out.contains("admin-key type"));
    }

    #[test]
    fn ssh_key_type_and_data_are_separate_fields() {
        // Confirmed live against a real VyOS commit: embedding the type
        // as a prefix on `key` (the naive reading of the pubkey file
        // format) fails with "Missing type for public-key" at commit
        // time -- `type` must be its own explicit field.
        let out = render_configure_script(&cfg());
        assert!(out.contains("admin-key type 'ssh-ed25519'"));
        assert!(out.contains("admin-key key 'AAAAtest'"));
        assert!(!out.contains("key 'ssh-ed25519 AAAAtest'"));
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
    fn pins_trunk_speed_and_duplex_when_both_given() {
        let mut c = cfg();
        c.trunk_speed = Some("1000".into());
        c.trunk_duplex = Some("full".into());
        let out = render_configure_script(&c);
        assert!(out.contains("set interfaces ethernet eth0 speed '1000'"));
        assert!(out.contains("set interfaces ethernet eth0 duplex 'full'"));
    }

    #[test]
    fn omits_speed_and_duplex_when_not_given() {
        let out = render_configure_script(&cfg());
        assert!(!out.contains(" speed "));
        assert!(!out.contains(" duplex "));
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
    fn nats_the_mgmt_subnet_out_the_uplink_but_not_the_underlay() {
        // Confirmed live: a fresh deployment had zero NAT rules at all,
        // so the mgmt VLAN (needed for #67's seed-image fetch, package
        // installs, self-registration) had no internet egress despite
        // the router's own uplink working fine.
        let out = render_configure_script(&cfg());
        assert!(out.contains("set nat source rule 100 outbound-interface name 'eth1'"));
        assert!(out.contains("set nat source rule 100 source address '10.255.20.0/24'"));
        assert!(out.contains("set nat source rule 100 translation address 'masquerade'"));
        assert!(!out.contains("10.255.10.0/24'\nset nat"));
    }

    #[test]
    fn omits_the_control_plane_nat_rule_when_not_configured() {
        // A fabric with no crowCloud instance yet has nothing to forward
        // to -- must not emit a broken/empty NAT rule.
        let out = render_configure_script(&cfg());
        assert!(!out.contains("nat destination rule 200"));
    }

    #[test]
    fn forwards_the_control_plane_port_from_the_uplink_when_configured() {
        // The whole point: reachable from the upstream LAN (e.g. during
        // bootstrap, before it's up enough to configure an
        // ExposedEndpoint for itself) -- separate from and never
        // colliding with crow-provider-vyos's own dynamically-numbered
        // (1000+) ExposedEndpoint rules.
        let cfg = VyosBuildConfig {
            crow_api_mgmt_ip: Some("10.255.20.50".to_string()),
            crow_api_mgmt_port: Some(8080),
            ..cfg()
        };
        let out = render_configure_script(&cfg);
        assert!(out.contains("set nat destination rule 200 inbound-interface name 'eth1'"));
        assert!(out.contains("set nat destination rule 200 protocol 'tcp'"));
        assert!(out.contains("set nat destination rule 200 destination port '8080'"));
        assert!(out.contains("set nat destination rule 200 translation address '10.255.20.50'"));
        assert!(out.contains("set nat destination rule 200 translation port '8080'"));
    }

    #[test]
    fn clamps_mss_on_the_uplink_to_avoid_a_pmtud_black_hole() {
        // Confirmed live: jumbo frames on the trunk meeting a standard
        // 1500-MTU uplink stalled TCP connections silently (TLS
        // handshake completed, then hung forever) once a remote server
        // advertised a large MSS and the resulting ICMP "frag needed"
        // never made it back through NAT.
        let out = render_configure_script(&cfg());
        assert!(out.contains("set interfaces ethernet eth1 ip adjust-mss 'clamp-mss-to-pmtu'"));
    }

    #[test]
    fn forwards_dns_for_the_mgmt_subnet_with_configurable_servers() {
        // Confirmed live: hosts on the mgmt VLAN had no working
        // resolver at all until this router started forwarding for
        // them. The server list is configurable (not a single
        // hardcoded address) because not every public resolver is
        // reachable from every network -- 9.9.9.9 and 1.1.1.1 both
        // timed out on the network this was first deployed on.
        let out = render_configure_script(&cfg());
        assert!(out.contains("set service dns forwarding listen-address '10.255.20.1'"));
        assert!(out.contains("set service dns forwarding allow-from '10.255.20.0/24'"));
        assert!(out.contains("set service dns forwarding name-server '8.8.8.8'"));
        assert!(out.contains("set service dns forwarding name-server '8.8.4.4'"));
    }

    #[test]
    fn ends_with_commit_and_save() {
        let out = render_configure_script(&cfg());
        let trimmed = out.trim_end();
        assert!(trimmed.ends_with("commit\nsave"));
    }
}
