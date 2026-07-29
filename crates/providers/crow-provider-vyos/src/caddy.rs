/// Renders per-`ExposedEndpoint` Caddy site files. Each exposed HTTP
/// service gets its own file under `/etc/caddy/sites/`, imported by the
/// base Caddyfile's `import sites/*.caddy` (written once at first boot,
/// see `crow-cli`'s `vyos_flavor::INSTALL_CADDY`) -- one file per endpoint
/// rather than a single shared Caddyfile the operator would need to
/// read-modify-write on every change, avoiding any risk of two concurrent
/// reconciles racing on the same file.
///
/// `tls: true` (the common case) needs no explicit ACME wiring at all --
/// Caddy issues and renews certs automatically for any site address that
/// isn't `http://`-prefixed, the moment it's configured and reachable.
/// `tls: false` uses the `http://` scheme prefix specifically to disable
/// that automatic behavior for a site that should stay plain HTTP.
use std::net::IpAddr;

pub fn site_file_path(domain: &str) -> String {
    format!("/etc/caddy/sites/{domain}.caddy")
}

/// Domain names should never legitimately contain a path separator or
/// `..` -- reject rather than silently sanitize, since this value ends up
/// directly in a remote file path (`site_file_path`) and a request to
/// write "../../etc/passwd.caddy" should fail loudly, not get "cleaned up"
/// into something unexpected.
pub fn validate_domain(domain: &str) -> Result<(), String> {
    if domain.is_empty() {
        return Err("domain is empty".to_string());
    }
    if domain.contains('/') || domain.contains("..") {
        return Err(format!(
            "domain {domain:?} contains a path separator or '..' -- refusing to use it in a file path"
        ));
    }
    Ok(())
}

pub fn render_site_block(domain: &str, target_ip: &IpAddr, target_port: u16, tls: bool) -> String {
    let address = if tls {
        domain.to_string()
    } else {
        format!("http://{domain}")
    };
    format!("{address} {{\n    reverse_proxy {target_ip}:{target_port}\n}}\n",)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_plain_domain_block_for_tls() {
        let out = render_site_block(
            "app.example.com",
            &"10.10.20.5".parse().unwrap(),
            8080,
            true,
        );
        assert!(out.starts_with("app.example.com {"));
        assert!(out.contains("reverse_proxy 10.10.20.5:8080"));
    }

    #[test]
    fn renders_an_http_scheme_prefix_when_tls_is_disabled() {
        let out = render_site_block(
            "app.example.com",
            &"10.10.20.5".parse().unwrap(),
            8080,
            false,
        );
        assert!(out.starts_with("http://app.example.com {"));
    }

    #[test]
    fn rejects_a_domain_with_a_path_separator() {
        assert!(validate_domain("../../etc/passwd").is_err());
        assert!(validate_domain("foo/bar.com").is_err());
        assert!(validate_domain("app.example.com").is_ok());
    }

    #[test]
    fn rejects_an_empty_domain() {
        assert!(validate_domain("").is_err());
    }
}
