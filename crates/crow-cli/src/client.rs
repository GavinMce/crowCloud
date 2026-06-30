#![allow(dead_code)]

use anyhow::Result;
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

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

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut req = self.inner.get(format!("{}{}", self.base, path));
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok);
        }
        Ok(req.send().await?.error_for_status()?.json().await?)
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
}
