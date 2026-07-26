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
/// Confirmed live: VyOS's CLI wrapper rejects `configure`/`set`/etc. on
/// a plain non-interactive `ssh host "configure < file"` exec --
/// `Invalid command: [configure]`, exit 127 -- even with `-t`, because
/// stdin isn't a real terminal in that mode. `ssh -tt` (force a PTY on
/// the *remote* side) with piped local stdin/stdout works fine without
/// needing a local PTY at all; ssh itself handles the terminal relay.
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::time::Instant;

pub struct VyosApplyConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub ssh_key: std::path::PathBuf,
    pub commands: Vec<String>,
    pub strict_host_key_checking: bool,
}

/// True once VyOS reports a hard login failure -- distinct from a
/// prompt timeout, which might just mean the device is slow.
fn login_failed(output: &str) -> bool {
    let lower = output.to_lowercase();
    lower.contains("permission denied") || lower.contains("denied")
}

/// Confirmed live (see the SSH lockout incident this was written
/// after): VyOS commits config sections somewhat independently, so a
/// `commit` reporting failure does NOT mean nothing was applied. Stop
/// immediately rather than proceeding to `save`, and never report the
/// device as untouched just because `commit` failed.
fn commit_failed(output: &str) -> bool {
    output.contains("failed") || output.contains("Missing") || output.contains("Error")
}

fn session_dropped(output: &str) -> bool {
    let lower = output.to_lowercase();
    lower.contains("denied") || lower.contains("lost connection") || lower.contains("closed")
}

async fn read_until(
    stdout: &mut (impl tokio::io::AsyncRead + Unpin),
    buf: &mut Vec<u8>,
    markers: &[&str],
    timeout: Duration,
) -> Result<String> {
    let deadline = Instant::now() + timeout;
    let mut chunk = [0u8; 4096];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!(
                "timed out waiting for one of {:?} in device output",
                markers
            );
        }
        match tokio::time::timeout(remaining, stdout.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(buf);
                if markers.iter().any(|m| text.contains(m)) {
                    return Ok(text.into_owned());
                }
            }
            Ok(Err(e)) => return Err(e).context("reading SSH session output"),
            Err(_) => bail!(
                "timed out waiting for one of {:?} in device output",
                markers
            ),
        }
    }
    Ok(String::from_utf8_lossy(buf).into_owned())
}

async fn send(stdin: &mut (impl tokio::io::AsyncWrite + Unpin), line: &str) -> Result<()> {
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

fn spawn_ssh(cfg: &VyosApplyConfig) -> Result<Child> {
    let host_key_checking = if cfg.strict_host_key_checking {
        "accept-new"
    } else {
        "no"
    };
    Command::new("ssh")
        .arg("-tt")
        .arg("-i")
        .arg(&cfg.ssh_key)
        .arg("-p")
        .arg(cfg.port.to_string())
        .arg("-o")
        .arg(format!("StrictHostKeyChecking={host_key_checking}"))
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(format!("{}@{}", cfg.user, cfg.host))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Inherited, not piped-and-left-unread: a piped stream nobody
        // drains can fill its OS buffer and block the child mid-write,
        // stalling the whole session with no timeout catching it (this
        // is suspected of causing a real hang seen in live testing).
        .stderr(Stdio::inherit())
        .spawn()
        .context("spawning ssh")
}

/// Applies every line in `cfg.commands` inside a single `configure`
/// session, then `commit` and `save`. Aborts immediately -- without
/// running `save` -- the moment anything looks wrong, since a partial
/// apply is a real possibility (see `commit_failed`'s doc comment).
pub async fn apply(cfg: &VyosApplyConfig) -> Result<()> {
    if cfg.commands.is_empty() {
        bail!("no commands to apply");
    }

    let mut child = spawn_ssh(cfg)?;
    let mut stdin = child.stdin.take().context("ssh stdin")?;
    let mut stdout = child.stdout.take().context("ssh stdout")?;

    let mut buf = Vec::new();
    let login = read_until(&mut stdout, &mut buf, &["$", "#"], Duration::from_secs(30)).await?;
    if login_failed(&login) {
        bail!("SSH login failed -- nothing was sent to the device");
    }

    buf.clear();
    send(&mut stdin, "configure").await?;
    let out = read_until(&mut stdout, &mut buf, &["#"], Duration::from_secs(20)).await?;
    if session_dropped(&out) {
        bail!("session dropped entering configure mode -- nothing was applied");
    }

    for cmd in &cfg.commands {
        buf.clear();
        send(&mut stdin, cmd).await?;
        let out = read_until(&mut stdout, &mut buf, &["#"], Duration::from_secs(20)).await?;
        if session_dropped(&out) {
            bail!(
                "session dropped mid-apply after '{cmd}' -- some sections may have \
                 already committed regardless. Do NOT assume the device is untouched."
            );
        }
    }

    buf.clear();
    send(&mut stdin, "commit").await?;
    let out = read_until(&mut stdout, &mut buf, &["#"], Duration::from_secs(60)).await?;
    if commit_failed(&out) {
        bail!(
            "commit failed. Some sections may have partially applied regardless \
             -- VyOS commits config sections somewhat independently. Not running \
             'save'. Manually inspect the device via console/SSH.\n\n{out}"
        );
    }

    buf.clear();
    send(&mut stdin, "save").await?;
    read_until(&mut stdout, &mut buf, &["#"], Duration::from_secs(60)).await?;

    send(&mut stdin, "exit").await?;
    drop(stdin);
    // The config was already committed and saved by this point -- a
    // clean session teardown is best-effort, not something worth
    // hanging the whole command over if `exit` doesn't cleanly close
    // the session for some reason.
    if tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_login_failure() {
        assert!(login_failed("Permission denied (publickey)."));
        assert!(!login_failed("vyos@vyos-rr:~$"));
    }

    #[test]
    fn detects_commit_failure_from_real_vyos_wording() {
        // Confirmed live: the SSH lockout incident this whole apply
        // path was written after started with a commit reporting
        // "Missing type for public-key" while other sections in the
        // same commit still landed.
        assert!(commit_failed("Missing type for public-key \"admin-key\"!"));
        assert!(commit_failed("Commit failed"));
        assert!(!commit_failed("vyos@vyos-rr#"));
    }

    #[test]
    fn detects_session_drop_distinctly_from_a_slow_prompt() {
        assert!(session_dropped("Connection closed by remote host"));
        assert!(session_dropped("Permission denied, please try again."));
        assert!(!session_dropped("vyos@vyos-rr#"));
    }
}
