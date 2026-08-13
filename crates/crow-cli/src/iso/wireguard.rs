/// Admin WireGuard VPN into the mgmt VLAN, terminating directly on VyOS
/// -- a different, unrelated feature from `crd::networking::TunnelEndpoint`/
/// `crow-vps-agent`'s WireGuard tunnel (that one is for *public* exposure
/// via a rented VPS when a fleet has no public IP; this one is
/// admin-only, and only ever routes into the fabric's own mgmt/underlay
/// VLANs, never out to the internet).
///
/// Key generation shells out to `wg genkey`/`wg pubkey` (the
/// `wireguard-tools` package) rather than pulling in a Rust crypto crate
/// -- matches this crate's existing pattern of shelling out to
/// established system tools for crypto operations (see `hash_password`'s
/// use of `openssl passwd -6`).
///
/// Server setup (`crates/crow-cli/src/iso/vyos.rs`'s `wireguard_*`
/// fields, baked into `iso vyos build`) and peer management
/// (`render_add_peer`/`render_remove_peer` below, pushed live over SSH
/// via `iso vyos wireguard add-peer`/`remove-peer`) are deliberately
/// separate: the server exists once per fabric and rarely changes, but
/// admins get added/removed on an ongoing basis and shouldn't need a
/// full VyOS rebuild+reflash each time.
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// Generates a fresh WireGuard private key. Never touches disk or the
/// network on its own -- callers decide whether/where to persist it
/// (`Config::wireguard_server_key_or_generate` caches the server's;
/// `iso vyos wireguard add-peer` writes a per-admin key to its own file).
pub fn genkey() -> Result<String> {
    let output = Command::new("wg")
        .arg("genkey")
        .output()
        .context("running `wg genkey` -- is wireguard-tools installed?")?;
    if !output.status.success() {
        bail!("wg genkey exited with {}", output.status);
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Derives the public key matching `private_key`, via `wg pubkey`
/// (reads the private key from stdin, same as how `hash_password` pipes
/// the plaintext password to `openssl passwd -6` rather than passing it
/// as a CLI argument, which would leak it into the process list).
pub fn pubkey(private_key: &str) -> Result<String> {
    let output = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(private_key.as_bytes())?;
            }
            child.wait_with_output()
        })
        .context("deriving public key via `wg pubkey`")?;

    if !output.status.success() {
        bail!("wg pubkey exited with {}", output.status);
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Renders the `set` commands adding one admin as a WireGuard peer --
/// pushed live over SSH by `iso vyos wireguard add-peer`
/// (`crow_vyos_ssh::apply_commands`, the same primitive `iso vyos apply`
/// already uses for the fabric configure script). `client_address` is a
/// bare IP (no prefix) -- `allowed-ips` always scopes a peer to exactly
/// its own `/32`, never a wider range, so one admin's tunnel traffic is
/// never routed toward another's.
pub fn render_add_peer(name: &str, client_pubkey: &str, client_address: &str) -> Vec<String> {
    vec![
        format!("set interfaces wireguard wg0 peer {name} public-key '{client_pubkey}'"),
        format!("set interfaces wireguard wg0 peer {name} allowed-ips '{client_address}/32'"),
    ]
}

/// Reverses `render_add_peer`. Only the peer's `set`-tree config is
/// removed -- any local private-key file for this admin is left alone
/// (see `iso vyos wireguard remove-peer`'s own doc comment for why).
pub fn render_remove_peer(name: &str) -> Vec<String> {
    vec![format!("delete interfaces wireguard wg0 peer {name}")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genkey_and_pubkey_round_trip_if_wireguard_tools_is_installed() {
        // Skips rather than fails when `wg` isn't on PATH (e.g. most CI
        // runners) -- this is a smoke test for the round trip, not a
        // reimplementation of WireGuard's own key-format validation.
        let Ok(private_key) = genkey() else {
            eprintln!("skipping: wireguard-tools not installed");
            return;
        };
        // A valid WireGuard key is 32 raw bytes, base64-encoded -- 44
        // characters, padded with one trailing '='.
        assert_eq!(private_key.len(), 44);
        assert!(private_key.ends_with('='));

        let public_key = pubkey(&private_key).expect("wg pubkey must succeed if wg genkey did");
        assert_eq!(public_key.len(), 44);
        assert_ne!(public_key, private_key);
    }

    #[test]
    fn render_add_peer_scopes_allowed_ips_to_exactly_this_peers_slash_32() {
        let out = render_add_peer("alice", "clientPubKeyBase64==", "10.255.30.2");
        assert!(out
            .iter()
            .any(|l| l
                == "set interfaces wireguard wg0 peer alice public-key 'clientPubKeyBase64=='"));
        assert!(out
            .iter()
            .any(|l| l == "set interfaces wireguard wg0 peer alice allowed-ips '10.255.30.2/32'"));
    }

    #[test]
    fn render_remove_peer_deletes_the_named_peer_only() {
        let out = render_remove_peer("alice");
        assert_eq!(out, vec!["delete interfaces wireguard wg0 peer alice"]);
    }
}
