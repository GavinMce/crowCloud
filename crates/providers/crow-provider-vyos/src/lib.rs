/// `NetworkProvider` backed by a live VyOS box already sitting on both the
/// fabric and the upstream LAN -- no rented VPS, no WireGuard tunnel. VyOS
/// already has direct L3 reachability to every private subnet via the
/// fabric's own OSPF/BGP routing (see the ISO-building work in #66/#67), so
/// exposing something is just a NAT destination rule away, pushed over the
/// same SSH `configure`/`set`/`commit`/`save` session `crow-cli`'s
/// `iso vyos apply` already uses (shared via `crow-vyos-ssh`).
///
/// `expose_http`/`provision_cert`/`revoke_cert` (the subdomain/Caddy-routing
/// path) are a separate follow-up -- this pass only covers the "shared
/// IP:port" TCP/UDP path, since VyOS itself has no HTTP-layer routing
/// capability without a reverse proxy installed on it first.
use async_trait::async_trait;
use crow_core::{traits::NetworkProvider, types::*, ProviderError};
use crow_vyos_ssh::VyosSshConfig;
use std::path::PathBuf;

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
/// Offset by 1000 to stay well clear of rule 100, already used by the
/// fabric's own egress source-NAT rule (see `iso::vyos::render_configure_script`).
fn nat_rule_number(public_port: u16) -> u32 {
    1000 + (public_port as u32 % 8000)
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

    async fn expose_http(&self, _spec: HttpExposeSpec) -> Result<ExposeHandle, ProviderError> {
        Err(ProviderError::Other(
            "expose_http (subdomain/Caddy routing) isn't implemented yet -- \
             only the shared IP:port TCP/UDP path is built so far"
                .to_string(),
        ))
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
        let Some(public_port) = handle.public_port else {
            return Err(ProviderError::Other(
                "ExposeHandle has no public_port -- can't determine which NAT rule to remove \
                 (this handle wasn't produced by VyosNetworkProvider::expose_tcp)"
                    .to_string(),
            ));
        };
        let rule = nat_rule_number(public_port);
        let commands = vec![format!("delete nat destination rule {rule}")];

        crow_vyos_ssh::apply_commands(&self.ssh_config(), &commands)
            .await
            .map_err(|e| ProviderError::Other(format!("removing NAT rule from VyOS: {e:#}")))
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
}
