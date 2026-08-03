/// Bakes Caddy's `.deb` package directly into VyOS flavor images, rather
/// than fetching it via apt at first boot.
///
/// Confirmed live: the apt-based install (Caddy's own official Cloudsmith
/// repo instructions) hit real, sequential breakage on a fresh box --
/// `debian-keyring`/`debian-archive-keyring` unavailable on VyOS's apt
/// sources (unnecessary anyway, since the repo's key is written directly
/// into its own keyring file, never touching Debian's archive trust
/// chain), then a DNS/connectivity gap at exactly the moment first boot
/// needs to reach Cloudsmith. Baking the package in sidesteps needing
/// working internet access at boot for Caddy specifically, at the cost of
/// a real, permanent size tradeoff: ~17MB added to this repo and to every
/// compiled `crow-cli` binary via `include_bytes!`, not just when actually
/// building a VyOS image. Confirmed worth it here: `apt-cache depends
/// caddy` shows zero external dependencies beyond base libc, so `dpkg -i`
/// alone (no `apt-get install -f` follow-up) is enough -- no partial
/// install possible from a missing-dependency gap either.
///
/// `includes_chroot` writes files in text mode (see `bnx2_firmware`'s
/// module doc comment on this, confirmed against vyos-build's source) --
/// same base64-encode-for-the-TOML-trip, decode-back-to-binary-at-first-
/// boot approach as the bnx2 firmware.
use base64::Engine;

const CADDY_DEB: &[u8] = include_bytes!("packages/caddy.deb");
const STAGING_PATH: &str = "usr/local/share/crowcloud-firmware/caddy.deb.b64";

/// `[[includes_chroot]]` TOML entry staging the base64-encoded `.deb`.
pub fn render_includes_chroot_toml() -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(CADDY_DEB);
    format!("[[includes_chroot]]\npath = \"{STAGING_PATH}\"\ndata = \"\"\"\n{encoded}\n\"\"\"\n\n")
}

/// Bash snippet decoding the staged `.deb` and installing it with `dpkg`
/// directly -- no `apt-get`/network involved at all for the package
/// itself. Safe to run on every `@reboot`: `dpkg -i` on an
/// already-installed version at the same package/version is a no-op.
pub fn render_install_script() -> String {
    format!(
        "if ! command -v caddy >/dev/null 2>&1; then\n    \
         base64 -d /{STAGING_PATH} > /tmp/caddy.deb\n    \
         dpkg -i /tmp/caddy.deb\n    \
         rm -f /tmp/caddy.deb\nfi\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_chroot_stages_the_deb_as_base64_not_raw_binary() {
        let out = render_includes_chroot_toml();
        assert!(out.contains("caddy.deb.b64"));
        // Base64 of the real .deb bytes, not the raw binary directly --
        // includes_chroot writes files in text mode, so raw binary
        // content would corrupt on the write.
        let expected = base64::engine::general_purpose::STANDARD.encode(CADDY_DEB);
        assert!(out.contains(&expected));
    }

    #[test]
    fn install_script_uses_dpkg_directly_not_apt() {
        // The whole point: no network/apt dependency for this package.
        let out = render_install_script();
        assert!(out.contains("dpkg -i"));
        assert!(!out.contains("apt-get"));
    }

    #[test]
    fn install_script_skips_reinstalling_if_caddy_already_present() {
        let out = render_install_script();
        assert!(out.contains("if ! command -v caddy"));
    }
}
