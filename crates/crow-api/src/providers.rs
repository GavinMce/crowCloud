use std::sync::Arc;

use crow_core::traits::InfraProvider;
use crow_provider_proxmox::ProxmoxProvider;
use serde::Deserialize;
use serde_json::Value;

use crate::error::ApiError;

#[derive(Debug, Deserialize)]
pub struct ProxmoxConfig {
    pub url: String,
    pub token_id: String,
    pub token_secret: String,
    pub node: String,
    pub default_storage: String,
    pub default_bridge: String,
    #[serde(default)]
    pub tls_insecure: bool,
}

pub fn build_infra_provider(
    provider_type: &str,
    config: &Value,
) -> Result<Arc<dyn InfraProvider>, ApiError> {
    match provider_type {
        "proxmox" => {
            let cfg: ProxmoxConfig = serde_json::from_value(config.clone())
                .map_err(|e| ApiError::BadRequest(format!("invalid proxmox config: {e}")))?;
            let p = ProxmoxProvider::new(
                &cfg.url,
                &cfg.token_id,
                &cfg.token_secret,
                &cfg.node,
                &cfg.default_storage,
                &cfg.default_bridge,
                cfg.tls_insecure,
            )
            .map_err(|e| ApiError::BadRequest(format!("failed to build proxmox provider: {e}")))?;
            Ok(Arc::new(p))
        }
        other => Err(ApiError::BadRequest(format!(
            "unknown provider type: {other}"
        ))),
    }
}
