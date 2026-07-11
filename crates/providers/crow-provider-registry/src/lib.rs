use std::net::Ipv4Addr;
use std::sync::Arc;

use crow_core::traits::{InfraProvider, IpamProvider};
use crow_provider_opnsense::OPNsenseProvider;
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
    /// Storage for cloud-init snippet uploads — must have "Snippets" enabled
    /// in its content types (a file/directory storage, e.g. "local"; LVM-thin
    /// storages used for `default_storage` can't hold snippets).
    #[serde(default = "default_snippets_storage")]
    pub snippets_storage: String,
    pub default_bridge: String,
    /// Login user for SSH-based `exec_in_vm`, injected into every VM via the
    /// native `ciuser` cloud-init field.
    #[serde(default = "default_ssh_user")]
    pub ssh_user: String,
    /// OpenSSH-format keypair injected into every VM via the native
    /// `sshkeys` cloud-init field (public) and used to authenticate back
    /// into it (private). Generate with
    /// `crow_provider_proxmox::ssh::generate_keypair`.
    pub ssh_private_key: String,
    pub ssh_public_key: String,
    #[serde(default)]
    pub tls_insecure: bool,
}

fn default_snippets_storage() -> String {
    "local".to_string()
}

fn default_ssh_user() -> String {
    "root".to_string()
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
                &cfg.snippets_storage,
                &cfg.default_bridge,
                &cfg.ssh_user,
                &cfg.ssh_private_key,
                &cfg.ssh_public_key,
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

#[derive(Debug, Deserialize)]
pub struct OPNsenseConfig {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    /// CIDR of an existing Kea DHCPv4 subnet (Services > Kea DHCPv4 in the
    /// OPNsense UI) to attach reservations to, e.g. "192.168.100.0/24".
    pub subnet_cidr: String,
    pub range_start: Ipv4Addr,
    pub range_end: Ipv4Addr,
    #[serde(default)]
    pub tls_insecure: bool,
}

/// Pure, sync factory for IPAM providers, mirroring `build_infra_provider`.
pub fn build_ipam_provider(
    provider_type: &str,
    config: &Value,
) -> Result<Arc<dyn IpamProvider>, RegistryError> {
    match provider_type {
        "opnsense" => {
            let cfg: OPNsenseConfig = serde_json::from_value(config.clone()).map_err(|e| {
                RegistryError::InvalidConfig(format!("invalid opnsense config: {e}"))
            })?;
            let p = OPNsenseProvider::new(
                &cfg.url,
                &cfg.api_key,
                &cfg.api_secret,
                &cfg.subnet_cidr,
                cfg.range_start,
                cfg.range_end,
                cfg.tls_insecure,
            )
            .map_err(|e| {
                RegistryError::InvalidConfig(format!("failed to build opnsense provider: {e}"))
            })?;
            Ok(Arc::new(p))
        }
        other => Err(RegistryError::UnknownType(other.to_string())),
    }
}

/// Validates that `provider_type`/`config` can build *some* provider
/// (`InfraProvider` or `IpamProvider`) — used by crow-api's provider
/// registration endpoint, which accepts any provider type generically and
/// doesn't know ahead of time which kind a given type is.
pub fn validate_provider_config(provider_type: &str, config: &Value) -> Result<(), RegistryError> {
    match build_infra_provider(provider_type, config) {
        Ok(_) => return Ok(()),
        Err(RegistryError::UnknownType(_)) => {}
        Err(e) => return Err(e),
    }
    build_ipam_provider(provider_type, config).map(|_| ())
}

/// Looks up a provider by its Postgres `providers.name` and builds an
/// `IpamProvider`. Mirrors `resolve_provider_by_name` for `InfraProvider`.
pub async fn resolve_ipam_provider_by_name(
    pool: &PgPool,
    provider_name: &str,
) -> Result<Arc<dyn IpamProvider>, RegistryError> {
    let row: Option<(String, Value)> =
        sqlx::query_as("SELECT provider_type, config FROM providers WHERE name = $1")
            .bind(provider_name)
            .fetch_optional(pool)
            .await?;
    let (provider_type, config) = row.ok_or(RegistryError::NotFound)?;
    build_ipam_provider(&provider_type, &config)
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

/// Deterministic Kubernetes object name for the `K8sCluster` CR backing a
/// `resources` row. Shares `VM_NAMESPACE` — the namespace is generic despite
/// its name (it's just "crow-resources").
pub fn k8s_cluster_cr_name(resource_id: Uuid) -> String {
    format!("k8s-{resource_id}")
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
