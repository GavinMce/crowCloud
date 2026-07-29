/// Bakes Broadcom NetXtreme II (`bnx2`) firmware directly into VyOS flavor
/// images (#66-adjacent hardware-compat gap, found live: a box with both
/// trunk and uplink on the same `bnx2` NIC failed to load
/// `bnx2-mips-09-6.2.1b.fw`, since Debian/VyOS's base image doesn't bundle
/// non-free firmware by default).
///
/// Baked in rather than fetched at first boot for the same reason the
/// fabric config itself has to be baked in: if this NIC is the box's
/// *only* network hardware, there's no way to reach the internet to
/// `apt-get install firmware-bnx2` (the same way Caddy is installed) until
/// the firmware is already present -- chicken and egg.
///
/// Files sourced from Debian's `firmware-bnx2` package (`non-free-firmware`
/// component), itself sourced from the upstream `linux-firmware` project.
/// License is Broadcom's `binary-redist-firmware` (see
/// `firmware/LICENSE.firmware-bnx2`) -- the same terms every Linux
/// distribution (VyOS included) already redistributes these under.
///
/// Only the "09" generation (BCM5709/BCM5716) firmware is included, since
/// that's the specific chip generation confirmed live -- `bnx2-mips-06-*`
/// (BCM5706/5708) would need adding separately if that generation ever
/// comes up. Includes both `rv2p-09` variants (non-ax and ax) since the
/// specific sub-variant a given chip needs isn't distinguishable without
/// the exact chip ID on hand, and shipping both is cheap (a few KB).
///
/// `includes_chroot` writes files in text mode (confirmed against
/// vyos-build's source, see `vyos_flavor`'s own module doc comment) --
/// firmware blobs are binary, so they're base64-encoded for the trip
/// through the flavor TOML and decoded back to binary by the first-boot
/// script before `/lib/firmware` ever sees them.
use base64::Engine;

const BNX2_MIPS_09: &[u8] = include_bytes!("firmware/bnx2-mips-09-6.2.1b.fw");
const BNX2_RV2P_09: &[u8] = include_bytes!("firmware/bnx2-rv2p-09-6.0.17.fw");
const BNX2_RV2P_09AX: &[u8] = include_bytes!("firmware/bnx2-rv2p-09ax-6.0.17.fw");

const STAGING_DIR: &str = "usr/local/share/crowcloud-firmware";

struct FirmwareFile {
    name: &'static str,
    data: &'static [u8],
}

const FILES: &[FirmwareFile] = &[
    FirmwareFile {
        name: "bnx2-mips-09-6.2.1b.fw",
        data: BNX2_MIPS_09,
    },
    FirmwareFile {
        name: "bnx2-rv2p-09-6.0.17.fw",
        data: BNX2_RV2P_09,
    },
    FirmwareFile {
        name: "bnx2-rv2p-09ax-6.0.17.fw",
        data: BNX2_RV2P_09AX,
    },
];

/// `[[includes_chroot]]` TOML entries staging the base64-encoded firmware
/// into the image, one per file. Appended to the flavor TOML alongside
/// the fabric-init script/cron entries already written there.
pub fn render_includes_chroot_toml() -> String {
    let mut out = String::new();
    for file in FILES {
        let encoded = base64::engine::general_purpose::STANDARD.encode(file.data);
        out.push_str(&format!(
            "[[includes_chroot]]\npath = \"{STAGING_DIR}/{name}.b64\"\ndata = \"\"\"\n{encoded}\n\"\"\"\n\n",
            name = file.name,
        ));
    }
    out
}

/// Bash snippet decoding the staged firmware into `/lib/firmware/bnx2` and
/// reloading the driver -- must run before `detect_trunk_and_uplink`, since
/// a NIC missing its firmware may not show a carrier at all until the
/// driver's been reloaded with it present.
pub fn render_install_script() -> String {
    let mut out = String::from("echo \"==> Installing bnx2 NIC firmware (mips-09/rv2p-09 generation)\"\nmkdir -p /lib/firmware/bnx2\n");
    for file in FILES {
        out.push_str(&format!(
            "base64 -d /{STAGING_DIR}/{name}.b64 > /lib/firmware/bnx2/{name}\n",
            name = file.name,
        ));
    }
    out.push_str(
        "if lsmod | grep -q '^bnx2 '; then\n    modprobe -r bnx2 2>/dev/null || true\nfi\nmodprobe bnx2 2>/dev/null || true\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_chroot_stages_all_three_firmware_files_as_base64() {
        let out = render_includes_chroot_toml();
        assert!(out.contains("bnx2-mips-09-6.2.1b.fw.b64"));
        assert!(out.contains("bnx2-rv2p-09-6.0.17.fw.b64"));
        assert!(out.contains("bnx2-rv2p-09ax-6.0.17.fw.b64"));
        // Base64 of the real firmware bytes, not the raw binary directly --
        // includes_chroot writes files in text mode (see module doc), so
        // raw binary content would corrupt on the write.
        let expected = base64::engine::general_purpose::STANDARD.encode(BNX2_MIPS_09);
        assert!(out.contains(&expected));
    }

    #[test]
    fn install_script_decodes_before_reloading_the_driver() {
        let out = render_install_script();
        let decode_pos = out.find("base64 -d").expect("decode step present");
        let reload_pos = out.find("modprobe bnx2").expect("reload step present");
        assert!(
            decode_pos < reload_pos,
            "firmware must be decoded onto disk before the driver reload picks it up"
        );
    }

    #[test]
    fn install_script_reload_is_best_effort_not_fatal() {
        // Must never abort the whole first-boot script on hardware that
        // doesn't have this NIC at all -- `modprobe bnx2` on such a box
        // would otherwise fail and, under `set -euo pipefail`, take the
        // rest of the fabric-init script down with it.
        let out = render_install_script();
        assert!(out.contains("modprobe bnx2 2>/dev/null || true"));
    }
}
