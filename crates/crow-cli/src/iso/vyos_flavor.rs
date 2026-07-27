/// Bakes fabric config directly into a custom VyOS image, so a
/// freshly-flashed box configures itself on boot with zero manual
/// `configure`/`set` interaction -- as opposed to `iso::vyos`'s
/// `render_configure_script`, which still needs `iso vyos apply` (or a
/// hand-run session) against an already-installed device.
///
/// Doesn't solve the install step itself -- VyOS has no unattended
/// install mode (see #63/#66 issue history) -- but removes the
/// post-install config step entirely.
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
/// (`/usr/local/bin/`, `/etc/cron.d/`) instead sidesteps that entirely,
/// since the installer copies the whole base OS image wholesale.
///
/// Uses `cron.d`'s `@reboot` instead of a systemd unit to trigger the
/// script -- deliberately avoids the systemd unit-enablement question
/// (whether a non-symlinked file placed directly under
/// `<target>.wants/` is honored the same as a real `systemctl enable`
/// symlink is not something this was able to confirm against a primary
/// source before shipping). `cron.d` files need no separate enable step
/// at all -- cron reads `/etc/cron.d/*` directly -- at the cost of
/// depending on cron already running by the time reboot fires, which
/// is standard on a Debian-based system but, like everything else
/// VyOS-specific in this session, worth confirming live once this is
/// actually built and booted.
///
/// Interface roles are detected live at boot rather than pre-specified
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

    for dev in /sys/class/net/*/; do
        iface="$(basename "$dev")"
        [ -e "${dev}device" ] || continue
        ip link set "$iface" up 2>/dev/null || true
        candidates+=("$iface")
    done

    # Give link state a moment to settle after bringing interfaces up.
    sleep 3

    for iface in "${candidates[@]}"; do
        if [ "$(cat "/sys/class/net/${iface}/carrier" 2>/dev/null || echo 0)" = "1" ]; then
            linked+=("$iface")
        fi
    done

    if [ "${#linked[@]}" -ne 2 ]; then
        echo "Expected exactly 2 cabled interfaces (trunk + uplink), found ${#linked[@]}: ${linked[*]:-none} -- refusing to guess" >&2
        return 1
    fi
    echo "Candidates with an active link: ${linked[*]}" >&2

    local uplink_if=""
    for iface in "${linked[@]}"; do
        echo "  Probing ${iface} for a DHCP offer..." >&2
        if timeout 10 dhclient -1 -timeout 8 "$iface" >/dev/null 2>&1; then
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
/// triggered on every boot via a `cron.d` `@reboot` entry (see
/// `render_cron_entry`). Detects the trunk/uplink interfaces live (see
/// module doc comment), then applies the exact same `set` commands
/// `iso vyos apply` would push over SSH -- reuses `render_configure_script`
/// directly rather than re-deriving the fabric config in bash, so the
/// two mechanisms can't drift apart.
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

{detect_trunk_and_uplink}

read -r TRUNK_IF UPLINK_IF < <(detect_trunk_and_uplink) || {{
    echo "Interface detection failed -- refusing to apply a partial fabric config" >&2
    exit 1
}}

vbash <<VBASH
source /opt/vyatta/etc/functions/script-template
configure
{set_commands}exit
VBASH
"#,
        uplink_gateway_probe = uplink_gateway_probe,
        detect_trunk_and_uplink = DETECT_TRUNK_AND_UPLINK,
        set_commands = set_commands,
    )
}

/// `/etc/cron.d/crowcloud-fabric-init` -- no `systemctl enable`
/// equivalent needed, cron.d files are read directly with no separate
/// enable step. Logs to a file since cron's own mail-based error
/// reporting isn't configured on a fresh box.
pub fn render_cron_entry() -> String {
    "@reboot root /usr/local/bin/crowcloud-fabric-init.sh >> /var/log/crowcloud-fabric-init.log 2>&1\n"
        .to_string()
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
/// = "iso"` matches the stock `generic.toml` flavor; the two
/// `[[includes_chroot]]` entries are the only customization.
pub fn render_flavor_toml(cfg: &VyosFlavorConfig) -> String {
    format!(
        r#"# Generated by `crow-cli iso vyos flavor` -- see #63.
image_format = "iso"

[[includes_chroot]]
path = "usr/local/bin/crowcloud-fabric-init.sh"
data = {script_data}

[[includes_chroot]]
path = "etc/cron.d/crowcloud-fabric-init"
data = {cron_data}
"#,
        script_data = toml_multiline_string(&render_fabric_init_script(cfg)),
        cron_data = toml_multiline_string(&render_cron_entry()),
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
                bgp_peer_password: "fabric-secret".into(),
                dns_servers: vec!["8.8.8.8".into(), "8.8.4.4".into()],
                allow_password_auth: false,
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
    fn cron_entry_needs_no_separate_enable_step() {
        let out = render_cron_entry();
        assert!(out.starts_with("@reboot root"));
        assert!(out.contains("crowcloud-fabric-init.sh"));
    }

    #[test]
    fn flavor_toml_embeds_both_files_as_inline_data_not_file_references() {
        // Confirmed against vyos-build's actual source
        // (build-vyos-image): `includes_chroot` entries are written via
        // `open(file_path, 'w').write(i["data"])` -- `data` must be the
        // literal content, not a path.
        let out = render_flavor_toml(&cfg());
        assert!(out.contains(r#"path = "usr/local/bin/crowcloud-fabric-init.sh""#));
        assert!(out.contains(r#"path = "etc/cron.d/crowcloud-fabric-init""#));
        assert!(out.contains("image_format = \"iso\""));
        assert!(out.contains("detect_trunk_and_uplink"));
        assert!(out.contains("@reboot root"));
    }
}
