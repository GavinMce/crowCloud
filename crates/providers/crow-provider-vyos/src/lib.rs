/// `NetworkProvider` backed by a live VyOS box already sitting on both the
/// fabric and the upstream LAN -- no rented VPS, no WireGuard tunnel. VyOS
/// already has direct L3 reachability to every private subnet via the
/// fabric's own OSPF/BGP routing (see the ISO-building work in #66/#67), so
/// exposing something is just a NAT destination rule away, pushed over the
/// same SSH `configure`/`set`/`commit`/`save` session `crow-cli`'s
/// `iso vyos apply` already uses (shared via `crow-vyos-ssh`).
///
/// `expose_http` routes via Caddy (installed onto VyOS at first boot, see
/// `crow-cli`'s `vyos_flavor::INSTALL_CADDY`) -- a per-endpoint site file
/// pushed over plain SSH (not the `configure` session, see
/// `crow-vyos-ssh::write_remote_file`) and a graceful reload. No NAT rule
/// is needed for this path at all: Caddy terminates directly on VyOS's own
/// uplink IP and reverse-proxies over the fabric's existing routing, so
/// there's nothing to translate. `provision_cert`/`revoke_cert` stay
/// unimplemented -- Caddy issues/renews certs automatically as a side
/// effect of a site being configured, and nothing calls these two methods
/// yet regardless.
///
/// `reserve_ip`/`release_ip` (backing `crd::networking::PublicIp`) are a
/// third, different shape again: a *secondary* address bound directly on
/// the uplink interface (the primary one, used for everything above,
/// stays untouched), forwarding every port to one private target via a
/// single address-only NAT rule -- no port fields at all, unlike
/// `expose_tcp`'s one-port-at-a-time rules.
use async_trait::async_trait;
use crow_core::{traits::NetworkProvider, types::*, ProviderError};
use crow_vyos_ssh::VyosSshConfig;
use std::path::PathBuf;

mod caddy;

pub struct VyosNetworkProvider {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub ssh_key: PathBuf,
    /// VyOS's physical uplink NIC name -- DNAT rules are scoped to this
    /// interface so they only ever match traffic actually arriving from
    /// the upstream LAN, not fabric-internal traffic.
    pub uplink_interface: String,
}

impl VyosNetworkProvider {
    fn ssh_config(&self) -> VyosSshConfig {
        VyosSshConfig {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            ssh_key: self.ssh_key.clone(),
            // Trust-on-first-use, not a throwaway skip -- matches
            // `iso vyos apply`'s own default (`--insecure-skip-host-key-check`
            // is opt-in there, never the default), and this runs
            // unattended/repeatedly from the operator rather than once by
            // hand, so a host key that unexpectedly changes should still
            // be caught rather than silently trusted forever.
            strict_host_key_checking: true,
        }
    }
}

/// Deterministic NAT rule number derived from the public port alone --
/// `TcpExposeSpec`/`ExposeHandle` carry no endpoint name/identifier, but
/// `public_port` is already a stable, unique key by construction (two
/// different exposed services can never legitimately share one), so
/// `unexpose` can reconstruct the exact same rule number from the handle's
/// `public_port` alone with no separate state to keep in sync.
///
/// Offset by 1000 to stay well clear of rule 100 (the fabric's own egress
/// source-NAT rule, see `iso::vyos::render_configure_script`) and rules
/// 200/201 (the control-plane API/frontend forwarding rules, same
/// module) -- see `reserve_ip_rule_number` for the next range along.
fn nat_rule_number(public_port: u16) -> u32 {
    1000 + (public_port as u32 % 8000)
}

/// Deterministic NAT rule number for a `PublicIp`'s static 1:1 forward,
/// derived from the reserved address's own last octet -- same reasoning
/// as `nat_rule_number` (no separate state to keep in sync between
/// `reserve_ip` and `release_ip`). Offset by 9000, clear of every rule
/// range above (100, 200-201, 1000-8999).
fn reserve_ip_rule_number(address: &std::net::IpAddr) -> u32 {
    let last_octet = match address {
        std::net::IpAddr::V4(v4) => v4.octets()[3],
        std::net::IpAddr::V6(v6) => v6.octets()[15],
    };
    9000 + last_octet as u32
}

fn protocol_str(protocol: &Protocol) -> &'static str {
    match protocol {
        Protocol::Tcp => "tcp",
        Protocol::Udp => "udp",
        Protocol::TcpUdp => "tcp_udp",
    }
}

#[async_trait]
impl NetworkProvider for VyosNetworkProvider {
    fn provider_type(&self) -> &'static str {
        "vyos"
    }
    fn name(&self) -> &str {
        &self.host
    }

    async fn expose_http(&self, spec: HttpExposeSpec) -> Result<ExposeHandle, ProviderError> {
        caddy::validate_domain(&spec.domain).map_err(ProviderError::Other)?;

        let path = caddy::site_file_path(&spec.domain);
        let content =
            caddy::render_site_block(&spec.domain, &spec.target_ip, spec.target_port, spec.tls);

        crow_vyos_ssh::write_remote_file(&self.ssh_config(), &path, &content)
            .await
            .map_err(|e| ProviderError::Other(format!("writing Caddy site file to VyOS: {e:#}")))?;

        crow_vyos_ssh::run_remote_command(&self.ssh_config(), "sudo systemctl reload caddy")
            .await
            .map_err(|e| ProviderError::Other(format!("reloading Caddy on VyOS: {e:#}")))?;

        Ok(ExposeHandle {
            provider_id: path,
            domain: Some(spec.domain),
            public_port: None,
        })
    }

    async fn expose_tcp(&self, spec: TcpExposeSpec) -> Result<ExposeHandle, ProviderError> {
        let rule = nat_rule_number(spec.public_port);
        let protocol = protocol_str(&spec.protocol);
        let commands = vec![
            format!(
                "set nat destination rule {rule} description 'crowcloud-expose-{}'",
                spec.public_port
            ),
            format!(
                "set nat destination rule {rule} inbound-interface name '{}'",
                self.uplink_interface
            ),
            format!("set nat destination rule {rule} protocol '{protocol}'"),
            format!(
                "set nat destination rule {rule} destination port '{}'",
                spec.public_port
            ),
            format!(
                "set nat destination rule {rule} translation address '{}'",
                spec.target_ip
            ),
            format!(
                "set nat destination rule {rule} translation port '{}'",
                spec.target_port
            ),
        ];

        crow_vyos_ssh::apply_commands(&self.ssh_config(), &commands)
            .await
            .map_err(|e| ProviderError::Other(format!("pushing NAT rule to VyOS: {e:#}")))?;

        Ok(ExposeHandle {
            provider_id: format!("dnat-rule-{rule}"),
            domain: None,
            public_port: Some(spec.public_port),
        })
    }

    async fn unexpose(&self, handle: &ExposeHandle) -> Result<(), ProviderError> {
        if let Some(domain) = &handle.domain {
            let path = caddy::site_file_path(domain);
            crow_vyos_ssh::run_remote_command(&self.ssh_config(), &format!("sudo rm -f '{path}'"))
                .await
                .map_err(|e| {
                    ProviderError::Other(format!("removing Caddy site file from VyOS: {e:#}"))
                })?;
            return crow_vyos_ssh::run_remote_command(
                &self.ssh_config(),
                "sudo systemctl reload caddy",
            )
            .await
            .map(|_| ())
            .map_err(|e| ProviderError::Other(format!("reloading Caddy on VyOS: {e:#}")));
        }

        let Some(public_port) = handle.public_port else {
            return Err(ProviderError::Other(
                "ExposeHandle has neither domain nor public_port -- can't determine what to \
                 remove (this handle wasn't produced by VyosNetworkProvider)"
                    .to_string(),
            ));
        };
        let rule = nat_rule_number(public_port);
        let commands = vec![format!("delete nat destination rule {rule}")];

        crow_vyos_ssh::apply_commands(&self.ssh_config(), &commands)
            .await
            .map_err(|e| ProviderError::Other(format!("removing NAT rule from VyOS: {e:#}")))
    }

    /// Reserves `spec.address` on the uplink interface (a secondary
    /// address -- the interface keeps its primary one too) and forwards
    /// *all* traffic to it straight through to `spec.target_ip`, no
    /// ports involved. Both the address bind and the NAT rule go over
    /// one SSH session (same as `expose_tcp`).
    async fn reserve_ip(&self, spec: ReserveIpSpec) -> Result<ReserveIpHandle, ProviderError> {
        let rule = reserve_ip_rule_number(&spec.address);
        let commands = vec![
            format!(
                "set interfaces ethernet {} address '{}/{}'",
                self.uplink_interface, spec.address, spec.prefix
            ),
            format!(
                "set nat destination rule {rule} description 'crowcloud-reserved-ip-{}'",
                spec.address
            ),
            format!(
                "set nat destination rule {rule} inbound-interface name '{}'",
                self.uplink_interface
            ),
            format!(
                "set nat destination rule {rule} destination address '{}'",
                spec.address
            ),
            format!(
                "set nat destination rule {rule} translation address '{}'",
                spec.target_ip
            ),
        ];

        crow_vyos_ssh::apply_commands(&self.ssh_config(), &commands)
            .await
            .map_err(|e| ProviderError::Other(format!("reserving IP on VyOS: {e:#}")))?;

        Ok(ReserveIpHandle {
            provider_id: format!("static-nat-rule-{rule}"),
            address: spec.address,
            prefix: spec.prefix,
        })
    }

    async fn release_ip(&self, handle: &ReserveIpHandle) -> Result<(), ProviderError> {
        let rule = reserve_ip_rule_number(&handle.address);
        let commands = vec![
            format!("delete nat destination rule {rule}"),
            format!(
                "delete interfaces ethernet {} address '{}/{}'",
                self.uplink_interface, handle.address, handle.prefix
            ),
        ];

        crow_vyos_ssh::apply_commands(&self.ssh_config(), &commands)
            .await
            .map_err(|e| ProviderError::Other(format!("releasing reserved IP on VyOS: {e:#}")))
    }

    async fn provision_cert(&self, _domain: &str) -> Result<CertHandle, ProviderError> {
        Err(ProviderError::Other(
            "provision_cert (ACME via Caddy) isn't implemented yet -- \
             only the shared IP:port TCP/UDP path is built so far"
                .to_string(),
        ))
    }

    async fn revoke_cert(&self, _handle: &CertHandle) -> Result<(), ProviderError> {
        Err(ProviderError::Other(
            "revoke_cert (ACME via Caddy) isn't implemented yet -- \
             only the shared IP:port TCP/UDP path is built so far"
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nat_rule_number_stays_clear_of_the_egress_masquerade_rule() {
        assert_ne!(nat_rule_number(0), 100);
        assert!(nat_rule_number(8080) >= 1000);
    }

    #[test]
    fn nat_rule_number_is_deterministic_for_the_same_port() {
        assert_eq!(nat_rule_number(8080), nat_rule_number(8080));
    }

    #[test]
    fn protocol_str_matches_vyos_syntax() {
        assert_eq!(protocol_str(&Protocol::Tcp), "tcp");
        assert_eq!(protocol_str(&Protocol::Udp), "udp");
        assert_eq!(protocol_str(&Protocol::TcpUdp), "tcp_udp");
    }

    #[test]
    fn reserve_ip_rule_number_stays_clear_of_every_other_range() {
        let addr: std::net::IpAddr = "10.0.202.50".parse().unwrap();
        let rule = reserve_ip_rule_number(&addr);
        assert_eq!(rule, 9050);
        assert_ne!(rule, 100);
        assert!(!(200..=201).contains(&rule));
        assert!(!(1000..=8999).contains(&rule));
    }

    #[test]
    fn reserve_ip_rule_number_is_deterministic_for_the_same_address() {
        let addr: std::net::IpAddr = "10.0.202.50".parse().unwrap();
        assert_eq!(reserve_ip_rule_number(&addr), reserve_ip_rule_number(&addr));
    }
}
