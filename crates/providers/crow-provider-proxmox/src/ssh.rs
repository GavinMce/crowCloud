use std::os::unix::fs::PermissionsExt;

use openssh::{KnownHosts, SessionBuilder, Stdio};
use tokio::io::AsyncWriteExt;

use crate::error::ProxmoxError;

pub struct SshTarget<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    pub private_key_pem: &'a str,
}

/// Writes `content` to `{remote_dir}/{filename}` on the Proxmox node over
/// SSH — the only way to place a file in a storage's `snippets/` directory,
/// since Proxmox's REST API has no upload endpoint for content type
/// "snippets" (confirmed against a live PVE 9.2 host: both `/upload` and
/// `/download-url` reject it with "does not have a value in the
/// enumeration 'iso, vztmpl, import'"). Requires the connecting user's key
/// to already be trusted via `authorized_keys` on the node — crowCloud has
/// no way to bootstrap that itself.
pub async fn write_remote_file(
    target: &SshTarget<'_>,
    remote_dir: &str,
    filename: &str,
    content: &[u8],
) -> Result<(), ProxmoxError> {
    let key_path = std::env::temp_dir().join(format!("crowcloud-ssh-{}.pem", uuid::Uuid::new_v4()));
    write_key_file(&key_path, target.private_key_pem)?;

    let result = upload_via_session(target, remote_dir, filename, content, &key_path).await;

    let _ = std::fs::remove_file(&key_path);
    result
}

fn write_key_file(path: &std::path::Path, pem: &str) -> Result<(), ProxmoxError> {
    // A missing trailing newline makes OpenSSH's key loader fail with an
    // opaque "error in libcrypto" — easy to lose in transit (e.g. shell
    // `$(...)` substitution strips all trailing newlines), so restore it
    // unconditionally rather than trusting the stored value's exact bytes.
    let pem = if pem.ends_with('\n') {
        pem.to_string()
    } else {
        format!("{pem}\n")
    };
    std::fs::write(path, &pem).map_err(|e| ProxmoxError::Ssh(format!("write key file: {e}")))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| ProxmoxError::Ssh(format!("chmod key file: {e}")))?;
    Ok(())
}

async fn upload_via_session(
    target: &SshTarget<'_>,
    remote_dir: &str,
    filename: &str,
    content: &[u8],
    key_path: &std::path::Path,
) -> Result<(), ProxmoxError> {
    let session = SessionBuilder::default()
        .user(target.user.to_string())
        .port(target.port)
        .keyfile(key_path)
        .known_hosts_check(KnownHosts::Accept)
        .connect(target.host)
        .await
        // `{e:?}` alongside Display — openssh's Display often just says
        // "failed to connect to the remote host" while Debug carries the
        // ssh subprocess's actual stderr (auth failure reason, key parse
        // error, etc.), which is the part that's actually actionable.
        .map_err(|e| ProxmoxError::Ssh(format!("connect to {}: {e} ({e:?})", target.host)))?;

    let remote_path = format!("{remote_dir}/{filename}");
    let mut cmd = session.command("sh");
    cmd.arg("-c")
        .arg(format!("mkdir -p {remote_dir} && cat > {remote_path}"))
        .stdin(Stdio::piped());

    let mut child = cmd
        .spawn()
        .await
        .map_err(|e| ProxmoxError::Ssh(format!("spawn remote command: {e}")))?;
    {
        let stdin = child.stdin().as_mut().expect("stdin was piped");
        stdin
            .write_all(content)
            .await
            .map_err(|e| ProxmoxError::Ssh(format!("write to remote stdin: {e}")))?;
        stdin
            .shutdown()
            .await
            .map_err(|e| ProxmoxError::Ssh(format!("close remote stdin: {e}")))?;
    }

    let status = child
        .wait()
        .await
        .map_err(|e| ProxmoxError::Ssh(format!("wait for remote command: {e}")))?;
    if !status.success() {
        return Err(ProxmoxError::Ssh(format!(
            "remote write to {remote_path} exited with {status:?}"
        )));
    }

    session
        .close()
        .await
        .map_err(|e| ProxmoxError::Ssh(format!("close ssh session: {e}")))?;
    Ok(())
}
