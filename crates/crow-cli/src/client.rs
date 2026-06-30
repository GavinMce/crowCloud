use anyhow::Result;
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

use crate::config::Config;

pub struct CrowClient {
    base: String,
    token: Option<String>,
    inner: Client,
}

impl CrowClient {
    pub fn new(base: String, token: Option<String>) -> Self {
        Self {
            base,
            token,
            inner: Client::new(),
        }
    }

    /// Build from saved config, requiring a server URL and token to be present.
    pub fn from_config(server_override: Option<String>) -> Result<Self> {
        let cfg = Config::load()?;
        let base = server_override
            .or(cfg.server)
            .map(|s| s.trim_end_matches('/').to_string())
            .ok_or_else(|| {
                anyhow::anyhow!("No server configured. Run `crow login --server <url>` first.")
            })?;
        let token = cfg.token;
        Ok(Self::new(base, token))
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut req = self.inner.get(format!("{}{}", self.base, path));
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok);
        }
        let res = req.send().await?.error_for_status()?;
        Ok(res.json().await?)
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let mut req = self.inner.post(format!("{}{}", self.base, path)).json(body);
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok);
        }
        Ok(req.send().await?.error_for_status()?.json().await?)
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let mut req = self.inner.delete(format!("{}{}", self.base, path));
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok);
        }
        req.send().await?.error_for_status()?;
        Ok(())
    }

    /// Resolve a provider name-or-UUID to a UUID string.
    pub async fn resolve_provider_id(&self, name_or_id: &str) -> Result<String> {
        // If it already looks like a UUID, use it directly.
        if uuid::Uuid::parse_str(name_or_id).is_ok() {
            return Ok(name_or_id.to_string());
        }
        #[derive(serde::Deserialize)]
        struct ProviderItem {
            id: String,
            name: String,
        }
        let providers: Vec<ProviderItem> = self.get("/api/v1/providers").await?;
        providers
            .into_iter()
            .find(|p| p.name == name_or_id)
            .map(|p| p.id)
            .ok_or_else(|| anyhow::anyhow!("provider '{}' not found", name_or_id))
    }
}

/// Require a project from flag or config.
pub fn require_project(flag: Option<String>) -> Result<String> {
    if let Some(p) = flag {
        return Ok(p);
    }
    let cfg = Config::load()?;
    cfg.current_project.ok_or_else(|| {
        anyhow::anyhow!("No project set. Use --project or `crow context set --project <name>`")
    })
}

/// Require an rg from flag or config.
pub fn require_rg(flag: Option<String>) -> Result<String> {
    if let Some(r) = flag {
        return Ok(r);
    }
    let cfg = Config::load()?;
    cfg.current_rg.ok_or_else(|| {
        anyhow::anyhow!("No resource group set. Use --rg or `crow context set --rg <name>`")
    })
}
