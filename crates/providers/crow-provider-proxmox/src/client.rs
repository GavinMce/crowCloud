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

pub struct ProxmoxClient {
    http: Client,
    pub base: String,
    auth: String,
    pub node: String,
}

impl ProxmoxClient {
    pub fn new(
        url: &str,
        token_id: &str,
        token_secret: &str,
        node: &str,
        tls_insecure: bool,
    ) -> Result<Self, ProxmoxError> {
        let http = Client::builder()
            .danger_accept_invalid_certs(tls_insecure)
            .build()?;
        Ok(Self {
            http,
            base: url.trim_end_matches('/').to_string(),
            auth: format!("PVEAPIToken={token_id}={token_secret}"),
            node: node.to_string(),
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

    pub async fn post_multipart<T: DeserializeOwned>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<T, ProxmoxError> {
        let resp = self
            .http
            .post(self.url(path))
            .header("Authorization", &self.auth)
            .multipart(form)
            .send()
            .await?;
        let env: PveEnvelope<T> = self.parse_raw(resp).await?;
        env.data
            .ok_or_else(|| ProxmoxError::Parse("empty data field".into()))
    }

    /// PUT with form body; returns nothing on success.
    pub async fn put<B: Serialize + ?Sized>(&self, path: &str, body: &B) -> Result<(), ProxmoxError> {
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
        // DELETE can return a UPID (string) or null
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
        serde_json::from_str::<T>(&text)
            .map_err(|e| ProxmoxError::Parse(format!("{e}: {text}")))
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
                return match status.exit_status.as_deref() {
                    Some("OK") | None => Ok(()),
                    Some(msg) => Err(ProxmoxError::TaskFailed(msg.to_string())),
                };
            }
            if Instant::now() > deadline {
                return Err(ProxmoxError::TaskTimeout(timeout_secs));
            }
            sleep(Duration::from_secs(2)).await;
        }
    }
}
