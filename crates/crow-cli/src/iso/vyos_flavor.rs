/// Bakes fabric-init tooling directly into a custom VyOS image, so a
/// freshly-flashed box has everything it needs (bnx2 firmware, Caddy,
/// the interface-detecting fabric-config script) on disk with no network
/// access required -- as opposed to `iso::vyos`'s `render_configure_script`,
/// which still needs `iso vyos apply` (or a hand-run session) against an
/// already-installed device.
///
/// Doesn't solve the install step itself -- VyOS has no unattended
/// install mode (see #63/#66 issue history) -- and, as of this revision,
/// doesn't trigger the fabric-init script automatically either: an
/// earlier version fired it via a `cron.d` `@reboot` entry, but that
/// didn't reliably trigger in practice. The script is baked into the
/// image at `/usr/local/bin/crowcloud-fabric-init.sh` (see
/// `render_fabric_init_script`) and meant to be run by hand over SSH
/// once the box is up, the same way `crow-cli iso proxmox build`'s
/// post-install hook now is.
///
/// Built via VyOS's own `vyos-build` toolchain (Docker, `--privileged`
/// required). Confirmed against `vyos-build`'s actual source
/// (`scripts/image-build/build-vyos-image`, function reading
/// `build_config["includes_chroot"]`): a flavor's `[[includes_chroot]]`
/// entries are `{ path, data }` pairs written verbatim as regular files
/// into the chroot at build time -- `data` is literal inline file
/// content, not a reference to copy an external file, and there's no
/// mechanism to inject a `config/hooks/live/*.hook.chroot` build-time
/// hook script via the flavor schema. Confirmed against the installer
/// source (`vyos-1x`'s `image_installer.py`, `install_image()`): on a
/// *fresh* install the installer copies only `config.boot` into the
/// new `/config` partition, never the live environment's `/config`
/// tree wholesale -- so baking a file at `/config/scripts/
/// vyos-postconfig-bootup.script` via `includes_chroot` would silently
/// never reach the installed system. Baking into a normal squashfs path
/// (`/usr/local/bin/`) instead sidesteps that entirely, since the
/// installer copies the whole base OS image wholesale.
///
/// Interface roles are detected live at boot (well, at whenever the
/// script is actually run) rather than pre-specified
/// at build time (no PCI address, MAC, or interface name baked in at
/// all): every physical interface with an active link is a candidate,
/// and whichever one answers DHCP (or, if the uplink is static, ARPs
/// successfully for the configured uplink gateway) is the uplink -- the
/// other candidate is the trunk, by elimination. This trades a
/// deterministic pre-specified identity for zero advance hardware
/// knowledge, at the cost of depending on there being exactly one
/// other DHCP-or-gateway-reachable candidate at boot time -- untested
/// against real hardware as of writing, needs live verification like
/// everything else VyOS-specific this session.
use crate::iso::vyos::{render_configure_script, VyosBuildConfig};

pub struct VyosFlavorConfig {
    /// `trunk_interface`/`uplink_interface` are ignored by this
    /// module -- the generated script detects the real interface
    /// names live at boot instead (see module doc comment).
    pub base: VyosBuildConfig,
}

const DETECT_TRUNK_AND_UPLINK: &str = r#"detect_trunk_and_uplink() {
    local dev iface candidates=() linked=()

    # dhclient expects this to already exist (normally created by
    # ifupdown/tmpfiles before it's invoked the usual way) -- confirmed
    # live: calling dhclient directly on a fresh boot without it fails
    # immediately with "run/dhclient/dhclient_<iface>.lease: No such
    # file or directory", not a timeout, regardless of -timeout value.
    mkdir -p /run/dhclient

    for dev in /sys/class/net/*/; do
        iface="$(basename "$dev")"
        [ -e "${dev}device" ] || continue
        ip link set "$iface" up 2>/dev/null || true
        candidates+=("$iface")
    done

    # Poll for link state to settle rather than a single fixed sleep --
    # confirmed live: a flat 10s sleep was enough for a manual re-run on
    # an already-idle system, but not right after a cold boot, where
    # kernel init/other services/disk I/O all compete for the same
    # window. bnx2 (Broadcom NetXtreme II) autonegotiation is
    # also slower than more modern chips, and the driver's just been
    # reloaded (to pick up firmware, see bnx2_firmware) immediately
    # before this runs, adding further to the settle time needed -- a
    # bounded poll adapts to however long that actually takes instead of
    # guessing a single magic number that keeps needing to be bumped
    # under different boot conditions.
    local waited=0
    local max_wait=60
    while [ "$waited" -lt "$max_wait" ]; do
        linked=()
        for iface in "${candidates[@]}"; do
            if [ "$(cat "/sys/class/net/${iface}/carrier" 2>/dev/null || echo 0)" = "1" ]; then
                linked+=("$iface")
            fi
        done
        [ "${#linked[@]}" -ge 2 ] && break
        sleep 2
        waited=$((waited + 2))
    done

    if [ "${#linked[@]}" -ne 2 ]; then
        echo "Expected exactly 2 cabled interfaces (trunk + uplink), found ${#linked[@]}: ${linked[*]:-none} -- refusing to guess" >&2
        return 1
    fi
    echo "Candidates with an active link: ${linked[*]}" >&2

    local uplink_if=""
    for iface in "${linked[@]}"; do
        echo "  Probing ${iface} for a DHCP offer..." >&2
        # `-timeout` is not a real dhclient CLI flag (confirmed live --
        # ISC dhclient only supports `timeout` as a dhclient.conf
        # directive; passing it here made dhclient mis-parse the
        # argument list entirely, failing instantly with `Cannot find
        # device "timeout"` regardless of the value used). The outer
        # coreutils `timeout` is the only thing actually bounding how
        # long this waits -- 65s gives dhclient's own default 60s
        # internal timeout room to fire cleanly before being killed.
        if timeout 65 dhclient -1 "$iface" >/dev/null 2>&1; then
            uplink_if="$iface"
            dhclient -r "$iface" >/dev/null 2>&1 || true
            break
        fi
    done

    if [ -z "$uplink_if" ] && [ -n "${UPLINK_GATEWAY_PROBE}" ]; then
        echo "  No interface answered DHCP -- falling back to ARP-probing the configured uplink gateway (${UPLINK_GATEWAY_PROBE})" >&2
        apt-get install -y arping >/dev/null 2>&1 || true
        for iface in "${linked[@]}"; do
            if arping -c 2 -w 3 -I "$iface" "${UPLINK_GATEWAY_PROBE}" >/dev/null 2>&1; then
                uplink_if="$iface"
                break
            fi
        done
    fi

    if [ -z "$uplink_if" ]; then
        echo "Could not determine which interface is the uplink (no DHCP offer, no ARP reply from the configured uplink gateway) -- refusing to apply a partial fabric config" >&2
        return 1
    fi

    local trunk_if=""
    for iface in "${linked[@]}"; do
        [ "$iface" != "$uplink_if" ] && trunk_if="$iface"
    done

    echo "Resolved trunk=${trunk_if}, uplink=${uplink_if}" >&2
    echo "${trunk_if} ${uplink_if}"
}"#;

/// The script baked into the image at `/usr/local/bin/crowcloud-fabric-init.sh`,
/// meant to be run by hand over SSH once the box is up (see module doc
/// comment). Detects the trunk/uplink interfaces live (see module doc
/// comment), then applies the exact same `set` commands `iso vyos apply`
/// would push over SSH -- reuses `render_configure_script` directly
/// rather than re-deriving the fabric config in bash, so the two
/// mechanisms can't drift apart. Uses the shared `step`/`fail` framework
/// (see `step_output`) so a session watching this run over SSH gets
/// numbered progress and, on failure, an exact list of what completed
/// beforehand.
pub fn render_fabric_init_script(cfg: &VyosFlavorConfig) -> String {
    let shell_cfg = VyosBuildConfig {
        trunk_interface: "${TRUNK_IF}".to_string(),
        uplink_interface: "${UPLINK_IF}".to_string(),
        ..cfg.base.clone()
    };
    let set_commands = render_configure_script(&shell_cfg);
    let uplink_gateway_probe = if !cfg.base.uplink_dhcp {
        cfg.base.uplink_gateway.clone().unwrap_or_default()
    } else {
        String::new()
    };

    format!(
        r#"#!/usr/bin/env bash
# Generated by `crow-cli iso vyos flavor` -- see #63.
set -euo pipefail

UPLINK_GATEWAY_PROBE="{uplink_gateway_probe}"

{step_framework}
{install_bnx2_firmware}

{detect_trunk_and_uplink}

step "Detecting trunk/uplink interfaces"
read -r TRUNK_IF UPLINK_IF < <(detect_trunk_and_uplink) || {{
    fail "Interface detection failed -- refusing to apply a partial fabric config"
}}

step "Applying VyOS fabric configuration"
vbash <<VBASH
source /opt/vyatta/etc/functions/script-template
configure
{set_commands}exit
VBASH

{install_caddy}

{on_success}"#,
        uplink_gateway_probe = uplink_gateway_probe,
        step_framework = crate::iso::step_output::render_step_framework(),
        install_bnx2_firmware = crate::iso::bnx2_firmware::render_install_script(),
        detect_trunk_and_uplink = DETECT_TRUNK_AND_UPLINK,
        set_commands = set_commands,
        install_caddy = render_install_caddy(),
        on_success = crate::iso::step_output::render_on_success_call(),
    )
}

/// Installs Caddy for the HTTP/subdomain-routing exposure path
/// (`NetworkProvider::expose_http`, pushed later over SSH per
/// `ExposedEndpoint`). Baked in directly from a pre-fetched `.deb` (see
/// `caddy_package`) rather than installed via apt at first boot --
/// confirmed live: the apt-based install (Caddy's own official Cloudsmith
/// instructions) hit real breakage (unavailable keyring packages, then a
/// DNS/connectivity gap at exactly the moment first boot needed to reach
/// Cloudsmith). Idempotent (`dpkg -i` no-ops if already installed, and the base
/// Caddyfile write never touches `sites/*.caddy`, which is only ever
/// managed by the operator over SSH afterward).
fn render_install_caddy() -> String {
    format!(
        r#"step "Installing Caddy (HTTP exposure path)"
{install_caddy_package}

mkdir -p /etc/caddy/sites
if [ ! -f /etc/caddy/Caddyfile ] || ! grep -qF 'import sites/*.caddy' /etc/caddy/Caddyfile; then
    cat > /etc/caddy/Caddyfile <<'CADDYFILE'
import sites/*.caddy
CADDYFILE
fi

systemctl enable caddy >/dev/null 2>&1 || true
systemctl restart caddy"#,
        install_caddy_package = crate::iso::caddy_package::render_install_script(),
    )
}

fn toml_multiline_string(s: &str) -> String {
    // TOML triple-quoted strings can't contain the delimiter itself;
    // none of our generated content does, but guard against a future
    // change introducing one silently producing invalid TOML.
    assert!(
        !s.contains(r#"""""#),
        "generated content contains a TOML triple-quote delimiter"
    );
    format!("\"\"\"\n{s}\"\"\"")
}

/// The `vyos-build` flavor TOML consumed by `build-vyos-image`. `image_format
/// = "iso"` matches the stock `generic.toml` flavor; the `[[includes_chroot]]`
/// entry bakes the fabric-init script onto the image at
/// `/usr/local/bin/crowcloud-fabric-init.sh` -- nothing triggers it
/// automatically (no `cron.d` entry, no systemd unit; see module doc
/// comment), it's meant to be run by hand over SSH after install.
/// `includes_chroot` writes files with no execute bit (confirmed against
/// `vyos-build`'s source -- a plain `open(file_path, 'w')`, no `chmod`),
/// so it must be invoked as `bash crowcloud-fabric-init.sh`, not
/// `./crowcloud-fabric-init.sh`.
pub fn render_flavor_toml(cfg: &VyosFlavorConfig) -> String {
    format!(
        r#"# Generated by `crow-cli iso vyos flavor` -- see #63.
image_format = "iso"

[[includes_chroot]]
path = "usr/local/bin/crowcloud-fabric-init.sh"
data = {script_data}

{bnx2_firmware}
{caddy_package}"#,
        script_data = toml_multiline_string(&render_fabric_init_script(cfg)),
        bnx2_firmware = crate::iso::bnx2_firmware::render_includes_chroot_toml(),
        caddy_package = crate::iso::caddy_package::render_includes_chroot_toml(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> VyosFlavorConfig {
        VyosFlavorConfig {
            base: VyosBuildConfig {
                hostname: "vyos-rr".into(),
                trunk_interface: "eth1".into(),
                uplink_interface: "eth2".into(),
                trunk_mtu: 9000,
                trunk_speed: Some("1000".into()),
                trunk_duplex: Some("full".into()),
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
                uplink_ip: Some("10.0.202.220".into()),
                uplink_prefix: Some(24),
                uplink_gateway: Some("10.0.202.1".into()),
                ospf_area: "0".into(),
                underlay_network: "10.255.10.0".into(),
                underlay_network_prefix: 24,
                ssh_pubkey: "ssh-ed25519 AAAAtest".into(),
                bgp_asn: 65000,
                bgp_peer_password: Some("fabric-secret".into()),
                dns_servers: vec!["8.8.8.8".into(), "8.8.4.4".into()],
                allow_password_auth: false,
                crow_api_mgmt_ip: None,
                crow_api_mgmt_port: None,
                crow_frontend_mgmt_port: None,
                wireguard_port: None,
                wireguard_address: None,
                wireguard_address_prefix: None,
                wireguard_private_key: None,
            },
        }
    }

    #[test]
    fn detects_interfaces_live_instead_of_using_a_pre_baked_identity() {
        let out = render_fabric_init_script(&cfg());
        assert!(out.contains("detect_trunk_and_uplink"));
        assert!(out.contains("carrier"));
        // The config's own trunk_interface/uplink_interface (kernel
        // names) must never leak in -- only the dynamically-resolved
        // shell variables should, since real hardware might not even
        // use those names.
        assert!(!out.contains("\"eth1\""));
        assert!(!out.contains("\"eth2\""));
        assert!(out.contains("${TRUNK_IF}"));
        assert!(out.contains("${UPLINK_IF}"));
    }

    #[test]
    fn probes_dhcp_first_then_falls_back_to_arping_the_static_gateway() {
        let out = render_fabric_init_script(&cfg());
        assert!(out.contains("dhclient -1"));
        assert!(out.contains("arping"));
        assert!(out.contains(r#"UPLINK_GATEWAY_PROBE="10.0.202.1""#));
    }

    #[test]
    fn omits_the_arp_fallback_gateway_when_uplink_is_dhcp() {
        // No static gateway exists to probe in DHCP mode -- if DHCP
        // itself doesn't identify the uplink, there's no fallback
        // signal available at all, so the baked-in probe target must
        // be empty (skips the fallback loop entirely) rather than some
        // stale/wrong value.
        let mut c = cfg();
        c.base.uplink_dhcp = true;
        c.base.uplink_gateway = None;
        let out = render_fabric_init_script(&c);
        assert!(out.contains(r#"UPLINK_GATEWAY_PROBE="""#));
    }

    #[test]
    fn refuses_to_guess_if_interface_detection_fails() {
        let out = render_fabric_init_script(&cfg());
        assert!(out.contains("refusing to apply a partial fabric config"));
        assert!(out.contains("refusing to guess"));
    }

    #[test]
    fn polls_for_link_state_instead_of_a_single_fixed_sleep() {
        // Confirmed live: a flat sleep long enough for a manual re-run on
        // an idle system still wasn't enough right after a cold boot,
        // where kernel init/other services/disk I/O all compete for the
        // same window -- a bounded poll adapts to however long that
        // actually takes instead of a magic number that keeps needing to
        // be bumped under different boot conditions.
        let out = render_fabric_init_script(&cfg());
        assert!(!out.contains("sleep 10"));
        assert!(out.contains("max_wait=60"));
        assert!(out.contains("while [ \"$waited\" -lt \"$max_wait\" ]"));
    }

    #[test]
    fn reuses_the_real_configure_script_renderer_not_a_reimplementation() {
        // Confirmed by construction: both `iso vyos apply` and this
        // baked-in script must apply identical fabric config, so they
        // can't silently drift apart from separately-maintained logic.
        let out = render_fabric_init_script(&cfg());
        assert!(out.contains("set protocols bgp listen range '10.255.10.0/24' peer-group 'FABRIC'"));
        assert!(out.contains("set nat source rule 100 translation address 'masquerade'"));
        assert!(out.contains("set interfaces ethernet ${TRUNK_IF} speed '1000'"));
    }

    #[test]
    fn fabric_init_script_installs_caddy_for_the_http_exposure_path() {
        let out = render_fabric_init_script(&cfg());
        // Baked-in .deb via dpkg, not apt -- confirmed live, the apt-based
        // install hit unavailable keyring packages and a network gap at
        // exactly the moment first boot needed to reach Cloudsmith.
        assert!(out.contains("dpkg -i"));
        assert!(!out.contains("apt-get install -y caddy"));
        assert!(out.contains("import sites/*.caddy"));
        assert!(out.contains("systemctl enable caddy"));
        // Idempotent across every re-run, not just the first one -- must
        // not reinstall/reconfigure once already present.
        assert!(out.contains("if ! command -v caddy"));
    }

    #[test]
    fn flavor_toml_embeds_the_script_as_inline_data_not_a_file_reference() {
        // Confirmed against vyos-build's actual source
        // (build-vyos-image): `includes_chroot` entries are written via
        // `open(file_path, 'w').write(i["data"])` -- `data` must be the
        // literal content, not a path.
        let out = render_flavor_toml(&cfg());
        assert!(out.contains(r#"path = "usr/local/bin/crowcloud-fabric-init.sh""#));
        assert!(out.contains("image_format = \"iso\""));
        assert!(out.contains("detect_trunk_and_uplink"));
    }

    #[test]
    fn flavor_toml_bakes_in_no_automatic_trigger() {
        // Neither a cron.d @reboot entry nor a systemd unit -- the script
        // is meant to be run by hand over SSH (see module doc comment),
        // not fired automatically at boot.
        let out = render_flavor_toml(&cfg());
        assert!(!out.contains("cron.d"));
        assert!(!out.contains("@reboot"));
    }

    #[test]
    fn fabric_init_script_is_syntactically_valid_bash() {
        // `bash -n` parses without executing -- catches quoting/brace
        // mistakes from the step-framework interpolation that plain
        // substring assertions elsewhere in this file wouldn't. Note the
        // embedded `vbash <<VBASH ... VBASH` heredoc is opaque to `-n`
        // (it's just a string literal as far as the outer script's own
        // syntax is concerned), so this doesn't validate the VyOS `set`
        // commands themselves -- only the surrounding bash.
        use std::io::Write;
        use std::process::{Command, Stdio};
        let out = render_fabric_init_script(&cfg());
        let mut child = Command::new("bash")
            .arg("-n")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("bash must be on PATH to run this test");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(out.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "bash -n reported a syntax error:\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn fabric_init_script_reports_numbered_steps_and_a_final_summary() {
        let out = render_fabric_init_script(&cfg());
        assert!(out.contains("step \"Detecting trunk/uplink interfaces\""));
        assert!(out.contains("step \"Applying VyOS fabric configuration\""));
        assert!(out.contains("step \"Installing Caddy (HTTP exposure path)\""));
        assert!(out.trim_end().ends_with("on_success"));
    }
}
