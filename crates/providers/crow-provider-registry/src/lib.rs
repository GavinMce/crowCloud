use std::sync::Arc;

use crow_core::traits::InfraProvider;
use crow_provider_proxmox::ProxmoxProvider;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("provider not found")]
    NotFound,
    #[error("invalid provider config: {0}")]
    InvalidConfig(String),
    #[error("unknown provider type: {0}")]
    UnknownType(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

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

/// Pure, sync factory. Relocated from crow-api so both crow-api and
/// crow-operator can build an `InfraProvider` from a `providers` row without
/// crow-api depending directly on concrete provider crates.
pub fn build_infra_provider(
    provider_type: &str,
    config: &Value,
) -> Result<Arc<dyn InfraProvider>, RegistryError> {
    match provider_type {
        "proxmox" => {
            let cfg: ProxmoxConfig = serde_json::from_value(config.clone()).map_err(|e| {
                RegistryError::InvalidConfig(format!("invalid proxmox config: {e}"))
            })?;
            let p = ProxmoxProvider::new(
                &cfg.url,
                &cfg.token_id,
                &cfg.token_secret,
                &cfg.node,
                &cfg.default_storage,
                &cfg.default_bridge,
                cfg.tls_insecure,
            )
            .map_err(|e| {
                RegistryError::InvalidConfig(format!("failed to build proxmox provider: {e}"))
            })?;
            Ok(Arc::new(p))
        }
        other => Err(RegistryError::UnknownType(other.to_string())),
    }
}

/// Looks up a provider by its Postgres `providers.id` and builds an `InfraProvider`.
pub async fn resolve_provider_by_id(
    pool: &PgPool,
    provider_id: Uuid,
) -> Result<Arc<dyn InfraProvider>, RegistryError> {
    let row: Option<(String, Value)> =
        sqlx::query_as("SELECT provider_type, config FROM providers WHERE id = $1")
            .bind(provider_id)
            .fetch_optional(pool)
            .await?;
    let (provider_type, config) = row.ok_or(RegistryError::NotFound)?;
    build_infra_provider(&provider_type, &config)
}

/// Looks up a provider by its Postgres `providers.name` (unique) and builds an
/// `InfraProvider`. This is what the operator uses to resolve
/// `VirtualMachineSpec.infra_provider_ref.name` (see `crd::resources` doc-comment
/// for why that field holds a Postgres name, not a Kubernetes object reference).
pub async fn resolve_provider_by_name(
    pool: &PgPool,
    provider_name: &str,
) -> Result<(Uuid, Arc<dyn InfraProvider>), RegistryError> {
    let row: Option<(Uuid, String, Value)> =
        sqlx::query_as("SELECT id, provider_type, config FROM providers WHERE name = $1")
            .bind(provider_name)
            .fetch_optional(pool)
            .await?;
    let (id, provider_type, config) = row.ok_or(RegistryError::NotFound)?;
    Ok((id, build_infra_provider(&provider_type, &config)?))
}

/// Fixed namespace for all resource CRs in the VM/Proxmox vertical slice.
pub const VM_NAMESPACE: &str = "crow-resources";

/// Deterministic Kubernetes object name for the `VirtualMachine` CR backing a
/// `resources` row, so no extra DB column is needed to link the two.
pub fn vm_cr_name(resource_id: Uuid) -> String {
    format!("vm-{resource_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_cr_name_is_prefixed_with_resource_id() {
        let id = Uuid::nil();
        assert_eq!(vm_cr_name(id), "vm-00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn build_infra_provider_rejects_unknown_type() {
        match build_infra_provider("nope", &Value::Null) {
            Err(RegistryError::UnknownType(t)) => assert_eq!(t, "nope"),
            Ok(_) => panic!("expected an error for an unknown provider type"),
            Err(other) => panic!("expected UnknownType, got {other:?}"),
        }
    }

    #[test]
    fn build_infra_provider_rejects_malformed_proxmox_config() {
        match build_infra_provider("proxmox", &serde_json::json!({})) {
            Err(RegistryError::InvalidConfig(_)) => {}
            Ok(_) => panic!("expected an error for a malformed proxmox config"),
            Err(other) => panic!("expected InvalidConfig, got {other:?}"),
        }
    }
}
