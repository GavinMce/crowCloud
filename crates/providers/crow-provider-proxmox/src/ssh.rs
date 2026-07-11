use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Handler};
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::{PrivateKey, PublicKey};
use russh::ChannelMsg;

use crate::error::ProxmoxError;

/// A freshly generated Ed25519 keypair, in the formats Proxmox/OpenSSH expect:
/// `public_openssh` goes straight into a VM's `sshkeys` cloud-init field,
/// `private_openssh` is kept (e.g. in the provider's config) to authenticate
/// back into any VM that was given the matching public key.
pub struct SshKeyPair {
    pub private_openssh: String,
    pub public_openssh: String,
}

/// Generates a new Ed25519 SSH keypair. The seed comes from two random UUIDs
/// rather than pulling in a CSPRNG crate directly — `uuid`'s `v4` feature
/// already wraps the OS RNG, and the workspace depends on it everywhere else.
pub fn generate_keypair() -> Result<SshKeyPair, ProxmoxError> {
    let mut seed = [0u8; 32];
    seed[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    seed[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());

    let keypair = russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&seed);
    let private_key: PrivateKey = keypair.into();

    let private_openssh = private_key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .map_err(|e| ProxmoxError::Parse(format!("failed to encode ssh private key: {e}")))?
        .to_string();
    let public_openssh = private_key
        .public_key()
        .to_openssh()
        .map_err(|e| ProxmoxError::Parse(format!("failed to encode ssh public key: {e}")))?;

    Ok(SshKeyPair {
        private_openssh,
        public_openssh,
    })
}

/// No host-key pinning: every VM is freshly provisioned by us, so there is no
/// prior known_hosts entry to check against — equivalent in trust model to
/// the `tls_insecure` flag already accepted for the Proxmox HTTPS API itself.
struct TrustOnFirstUse;

impl Handler for TrustOnFirstUse {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Connects to `ip:22` as `user` using `private_key_openssh`, runs `command`,
/// and returns its combined stdout+stderr. Errors if the remote command exits
/// non-zero.
pub async fn exec(
    ip: IpAddr,
    user: &str,
    private_key_openssh: &str,
    command: &str,
    timeout_secs: u64,
) -> Result<String, ProxmoxError> {
    let key = PrivateKey::from_openssh(private_key_openssh)
        .map_err(|e| ProxmoxError::Parse(format!("invalid ssh private key: {e}")))?;

    let config = Arc::new(client::Config::default());
    let deadline = Duration::from_secs(timeout_secs);

    let mut handle = tokio::time::timeout(
        deadline,
        client::connect(config, (ip.to_string(), 22u16), TrustOnFirstUse),
    )
    .await
    .map_err(|_| ProxmoxError::TaskTimeout(timeout_secs))?
    .map_err(|e| ProxmoxError::Parse(format!("ssh connect failed: {e}")))?;

    let auth = handle
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), None))
        .await
        .map_err(|e| ProxmoxError::Parse(format!("ssh authentication failed: {e}")))?;
    if !auth.success() {
        return Err(ProxmoxError::Parse(
            "ssh authentication rejected by remote host".to_string(),
        ));
    }

    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| ProxmoxError::Parse(format!("ssh channel open failed: {e}")))?;
    channel
        .exec(true, command.as_bytes())
        .await
        .map_err(|e| ProxmoxError::Parse(format!("ssh exec failed: {e}")))?;

    let mut output = Vec::new();
    let mut exit_status: Option<u32> = None;
    while let Some(msg) = tokio::time::timeout(deadline, channel.wait())
        .await
        .map_err(|_| ProxmoxError::TaskTimeout(timeout_secs))?
    {
        match msg {
            ChannelMsg::Data { data } => output.extend_from_slice(&data),
            ChannelMsg::ExtendedData { data, .. } => output.extend_from_slice(&data),
            ChannelMsg::ExitStatus { exit_status: code } => exit_status = Some(code),
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }

    let stdout = String::from_utf8_lossy(&output).to_string();
    match exit_status {
        Some(0) | None => Ok(stdout),
        Some(code) => Err(ProxmoxError::Api {
            status: 0,
            message: format!("remote command exited with status {code}: {stdout}"),
        }),
    }
}
