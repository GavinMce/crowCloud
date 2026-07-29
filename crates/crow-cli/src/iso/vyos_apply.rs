/// Applies a rendered VyOS `configure.txt` (see `iso::vyos`) to a live
/// device over SSH (#66).
///
/// VyOS has no unattended/answer-file install mode -- confirmed against
/// current VyOS docs, there is no equivalent to Proxmox's `answer.toml`
/// for the installer itself, so a router still needs one interactive
/// `install image` session per physical box. This closes the *next*
/// gap instead: applying the fabric config afterwards without hand-
/// running an ad-hoc script over SSH each time.
///
/// The actual SSH session handling (VyOS's `-tt`/PTY quirks, expect-style
/// login/commit/session-drop detection) lives in `crow-vyos-ssh`, shared
/// with `crow-operator`'s `NetworkProvider` implementation, which pushes
/// NAT rule changes on every `ExposedEndpoint` reconcile rather than once.
use anyhow::{Context, Result};
use crow_vyos_ssh::VyosSshConfig;
use std::path::Path;

pub struct VyosApplyConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub ssh_key: std::path::PathBuf,
    pub commands: Vec<String>,
    pub strict_host_key_checking: bool,
}

pub async fn apply(cfg: &VyosApplyConfig) -> Result<()> {
    let ssh_cfg = VyosSshConfig {
        host: cfg.host.clone(),
        port: cfg.port,
        user: cfg.user.clone(),
        ssh_key: cfg.ssh_key.clone(),
        strict_host_key_checking: cfg.strict_host_key_checking,
    };
    crow_vyos_ssh::apply_commands(&ssh_cfg, &cfg.commands).await
}

pub fn read_commands_from_script(path: &Path) -> Result<Vec<String>> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("reading configure script at {}", path.display()))?;
    Ok(contents
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}
