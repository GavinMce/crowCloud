use reqwest::{Client, Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::time::{sleep, Duration, Instant};

use crate::error::ProxmoxError;

#[derive(Deserialize)]
struct PveEnvelope<T> {
    data: Option<T>,
}

#[derive(Deserialize)]
pub struct TaskStatus {
    pub status: String,
    #[serde(rename = "exitstatus")]
    pub exit_status: Option<String>,
}

/// SSH is the only way to place a file under a Proxmox storage's
/// `snippets/` directory — the REST API has no upload endpoint for that
/// content type (see `crate::ssh`). `None` when a host has no SSH
/// credentials configured; snippet-dependent operations (custom cloud-init
/// `user_data`/`network_config`) then fail with a clear "SSH not
/// configured" error instead of the opaque 400 Proxmox itself returns.
pub struct SshCredentials {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub private_key_pem: String,
}

pub struct ProxmoxClient {
    http: Client,
    pub base: String,
    auth: String,
    pub node: String,
    pub ssh: Option<SshCredentials>,
    /// `false` disables hardware-accelerated virtualization on VMs created
    /// against this host (Proxmox's `kvm=0`, falling back to slow QEMU/TCG
    /// software emulation) — only needed when the host itself has no
    /// VT-x/AMD-V available to it (e.g. a nested/virtualized Proxmox
    /// install without nested-virt passed through). Defaults to `true`
    /// (normal hardware acceleration) so this never silently slows down a
    /// host that doesn't need it.
    pub kvm: bool,
}

/// Strips scheme and port/path from a Proxmox API URL to get the bare
/// hostname/IP SSH should target — the same host, just a different port.
fn host_from_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    let without_scheme = trimmed
        .rsplit_once("://")
        .map(|(_scheme, rest)| rest)
        .unwrap_or(trimmed);
    let host_and_maybe_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    host_and_maybe_port
        .rsplit_once(':')
        .map(|(host, _port)| host)
        .unwrap_or(host_and_maybe_port)
        .to_string()
}

impl ProxmoxClient {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: &str,
        token_id: &str,
        token_secret: &str,
        node: &str,
        tls_insecure: bool,
        ssh_user: Option<&str>,
        ssh_port: Option<u16>,
        ssh_private_key: Option<&str>,
        kvm: bool,
    ) -> Result<Self, ProxmoxError> {
        let http = Client::builder()
            .danger_accept_invalid_certs(tls_insecure)
            .timeout(Duration::from_secs(30)) // fix: per-request timeout so task-level deadlines actually fire
            .build()?;
        let ssh = ssh_private_key.map(|key| SshCredentials {
            host: host_from_url(url),
            port: ssh_port.unwrap_or(22),
            user: ssh_user.unwrap_or("root").to_string(),
            private_key_pem: key.to_string(),
        });
        Ok(Self {
            http,
            base: url.trim_end_matches('/').to_string(),
            auth: format!("PVEAPIToken={token_id}={token_secret}"),
            node: node.to_string(),
            ssh,
            kvm,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api2/json{}", self.base, path)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ProxmoxError> {
        let resp = self
            .http
            .get(self.url(path))
            .header("Authorization", &self.auth)
            .send()
            .await?;
        let env: PveEnvelope<T> = self.parse_raw(resp).await?;
        env.data
            .ok_or_else(|| ProxmoxError::Parse("empty data field".into()))
    }

    /// POST that expects a non-null data field in the response.
    pub async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ProxmoxError> {
        let resp = self
            .http
            .post(self.url(path))
            .header("Authorization", &self.auth)
            .form(body)
            .send()
            .await?;
        let env: PveEnvelope<T> = self.parse_raw(resp).await?;
        env.data
            .ok_or_else(|| ProxmoxError::Parse("empty data field".into()))
    }

    /// POST where the response data may be null (sync Proxmox operations return {"data":null}).
    pub async fn post_opt<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<Option<T>, ProxmoxError> {
        let resp = self
            .http
            .post(self.url(path))
            .header("Authorization", &self.auth)
            .form(body)
            .send()
            .await?;
        let env: PveEnvelope<T> = self.parse_raw(resp).await?;
        Ok(env.data)
    }

    /// PUT with form body; returns nothing on success.
    pub async fn put<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), ProxmoxError> {
        let resp = self
            .http
            .put(self.url(path))
            .header("Authorization", &self.auth)
            .form(body)
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(());
        }
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        Err(ProxmoxError::Api { status, message })
    }

    /// DELETE; returns the UPID task string if the operation is async, or None.
    pub async fn delete(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<Option<String>, ProxmoxError> {
        let resp = self
            .http
            .delete(self.url(path))
            .header("Authorization", &self.auth)
            .query(params)
            .send()
            .await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let env: PveEnvelope<Option<String>> = self.parse_raw(resp).await?;
        Ok(env.data.flatten())
    }

    async fn parse_raw<T: DeserializeOwned>(&self, resp: Response) -> Result<T, ProxmoxError> {
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("errors")
                        .map(|e| e.to_string())
                        .or_else(|| v.get("message").and_then(|m| m.as_str()).map(String::from))
                })
                .unwrap_or(text);
            return Err(ProxmoxError::Api {
                status: status.as_u16(),
                message,
            });
        }
        serde_json::from_str::<T>(&text).map_err(|e| ProxmoxError::Parse(format!("{e}: {text}")))
    }

    /// Block until a Proxmox task UPID finishes or the timeout is reached.
    pub async fn wait_task(&self, upid: &str, timeout_secs: u64) -> Result<(), ProxmoxError> {
        // UPID format: UPID:<node>:<pid>:<pstart>:<starttime>:<type>:<id>:<user>:
        let node = upid.split(':').nth(1).unwrap_or(&self.node);
        let encoded = urlencoding::encode(upid);
        let path = format!("/nodes/{node}/tasks/{encoded}/status");

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            let status: TaskStatus = self.get(&path).await?;
            if status.status == "stopped" {
                // exitstatus can be transiently absent if the task runner hasn't
                // flushed its state yet — treat None as "not settled, poll again".
                match status.exit_status.as_deref() {
                    Some("OK") => return Ok(()),
                    Some(msg) => return Err(ProxmoxError::TaskFailed(msg.to_string())),
                    None => {}
                }
            }
            if Instant::now() > deadline {
                return Err(ProxmoxError::TaskTimeout(timeout_secs));
            }
            sleep(Duration::from_secs(2)).await;
        }
    }
}
