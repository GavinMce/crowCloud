use reqwest::{Client, Response};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::error::OPNsenseError;

pub struct OPNsenseClient {
    http: Client,
    base: String,
    api_key: String,
    api_secret: String,
}

impl OPNsenseClient {
    pub fn new(
        url: &str,
        api_key: &str,
        api_secret: &str,
        tls_insecure: bool,
    ) -> Result<Self, OPNsenseError> {
        let http = Client::builder()
            .danger_accept_invalid_certs(tls_insecure)
            .build()?;
        Ok(Self {
            http,
            base: url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, OPNsenseError> {
        let resp = self
            .http
            .get(self.url(path))
            .basic_auth(&self.api_key, Some(&self.api_secret))
            .send()
            .await?;
        self.parse(resp).await
    }

    pub async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, OPNsenseError> {
        let resp = self
            .http
            .post(self.url(path))
            .basic_auth(&self.api_key, Some(&self.api_secret))
            .json(body)
            .send()
            .await?;
        self.parse(resp).await
    }

    /// POST with no body (used for the `/service/reconfigure` style commands).
    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, OPNsenseError> {
        let resp = self
            .http
            .post(self.url(path))
            .basic_auth(&self.api_key, Some(&self.api_secret))
            .json(&Value::Object(Default::default()))
            .send()
            .await?;
        self.parse(resp).await
    }

    async fn parse<T: DeserializeOwned>(&self, resp: Response) -> Result<T, OPNsenseError> {
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(OPNsenseError::Api {
                status: status.as_u16(),
                message: text,
            });
        }
        serde_json::from_str::<T>(&text).map_err(|e| OPNsenseError::Parse(format!("{e}: {text}")))
    }
}
