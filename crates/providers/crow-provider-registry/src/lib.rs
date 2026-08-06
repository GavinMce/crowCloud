use std::sync::Arc;

use crow_core::traits::InfraProvider;
use crow_provider_proxmox::{ProxmoxProvider, ProxmoxProviderConfig};
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
    /// A host's connection config no longer requires a node — nodes are
    /// adopted individually via `provider_nodes` (see
    /// `resolved_node_defaults`). Still `Option` rather than removed
    /// outright so hosts created before that existed keep working through
    /// the legacy fallback.
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub default_storage: Option<String>,
    #[serde(default)]
    pub default_bridge: Option<String>,
    /// This node's own underlay-VLAN IP -- used as the VTEP source for
    /// `PrivateSubnet`'s VXLAN/EVPN dataplane (see
    /// `crow-provider-proxmox::network::create_network`). `Option` for the
    /// same reason as `default_storage`/`default_bridge`: unresolved
    /// connection-only config, or a node registered before this existed.
    #[serde(default)]
    pub underlay_ip: Option<String>,
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
        "proxmox" => Ok(Arc::new(build_proxmox_provider(config)?)),
        other => Err(RegistryError::UnknownType(other.to_string())),
    }
}

/// Concrete-typed sibling to `build_infra_provider`, for callers that need
/// Proxmox-specific capabilities (e.g. node listing) not on the shared
/// `InfraProvider` trait object.
///
/// Tolerates a missing `node`/`default_storage`/`default_bridge` (empty
/// string) — callers building a provider for an actual operation (VM
/// create/delete/...) must resolve and splice those in first via
/// `resolve_provider_by_id`/`resolve_provider_by_name`; this function alone
/// is also used for create-time validation, when they legitimately don't
/// exist yet.
pub fn build_proxmox_provider(config: &Value) -> Result<ProxmoxProvider, RegistryError> {
    let cfg: ProxmoxConfig = serde_json::from_value(config.clone())
        .map_err(|e| RegistryError::InvalidConfig(format!("invalid proxmox config: {e}")))?;
    ProxmoxProvider::new(ProxmoxProviderConfig {
        url: &cfg.url,
        token_id: &cfg.token_id,
        token_secret: &cfg.token_secret,
        node: cfg.node.as_deref().unwrap_or(""),
        default_storage: cfg.default_storage.as_deref().unwrap_or(""),
        default_bridge: cfg.default_bridge.as_deref().unwrap_or(""),
        underlay_ip: cfg.underlay_ip.as_deref(),
        tls_insecure: cfg.tls_insecure,
    })
    .map_err(|e| RegistryError::InvalidConfig(format!("failed to build proxmox provider: {e}")))
}

/// Resolves a specific node's default storage/bridge for a provider: first
/// checks `provider_nodes` (a node adopted via the Nodes tab), then falls
/// back to the host's own config *only* if that config's legacy `node`
/// matches — a host created before per-node config existed, whose sole
/// node is still baked into `providers.config`.
async fn resolved_node_defaults(
    pool: &PgPool,
    provider_id: Uuid,
    node_name: &str,
    config: &Value,
) -> Result<(String, String, Option<String>), RegistryError> {
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT default_storage, default_bridge, underlay_ip FROM provider_nodes
         WHERE provider_id = $1 AND node_name = $2",
    )
    .bind(provider_id)
    .bind(node_name)
    .fetch_optional(pool)
    .await?;
    if let Some((storage, bridge, underlay_ip)) = row {
        return Ok((storage, bridge, underlay_ip));
    }

    let legacy_node = config.get("node").and_then(|v| v.as_str());
    let legacy_storage = config.get("default_storage").and_then(|v| v.as_str());
    let legacy_bridge = config.get("default_bridge").and_then(|v| v.as_str());
    if legacy_node == Some(node_name) {
        if let (Some(storage), Some(bridge)) = (legacy_storage, legacy_bridge) {
            // Legacy config predates underlay_ip entirely (see 007/008) --
            // no VTEP source available for this node until it's re-adopted
            // via the Nodes tab or re-registers itself.
            return Ok((storage.to_string(), bridge.to_string(), None));
        }
    }

    Err(RegistryError::InvalidConfig(format!(
        "node {node_name:?} is not configured for this host — adopt it from the host's Nodes tab first"
    )))
}

/// Splices the resolved node/storage/bridge/underlay_ip into a copy of
/// `config`.
fn with_resolved_node(
    config: &Value,
    node_name: &str,
    storage: String,
    bridge: String,
    underlay_ip: Option<String>,
) -> Value {
    let mut config = config.clone();
    if let Some(obj) = config.as_object_mut() {
        obj.insert("node".to_string(), Value::String(node_name.to_string()));
        obj.insert("default_storage".to_string(), Value::String(storage));
        obj.insert("default_bridge".to_string(), Value::String(bridge));
        obj.insert(
            "underlay_ip".to_string(),
            underlay_ip.map_or(Value::Null, Value::String),
        );
    }
    config
}

/// Looks up a provider by its Postgres `providers.id`, resolves `node_name`
/// against its adopted nodes, and builds an `InfraProvider` targeting it.
pub async fn resolve_provider_by_id(
    pool: &PgPool,
    provider_id: Uuid,
    node_name: &str,
) -> Result<Arc<dyn InfraProvider>, RegistryError> {
    let row: Option<(String, Value)> =
        sqlx::query_as("SELECT provider_type, config FROM providers WHERE id = $1")
            .bind(provider_id)
            .fetch_optional(pool)
            .await?;
    let (provider_type, config) = row.ok_or(RegistryError::NotFound)?;
    let (storage, bridge, underlay_ip) =
        resolved_node_defaults(pool, provider_id, node_name, &config).await?;
    let config = with_resolved_node(&config, node_name, storage, bridge, underlay_ip);
    build_infra_provider(&provider_type, &config)
}

/// Looks up a provider by its Postgres `providers.name` (unique), resolves
/// `node_name` against its adopted nodes, and builds an `InfraProvider`
/// targeting it. This is what the operator uses to resolve
/// `VirtualMachineSpec.infra_provider_ref.name`/`.node` (see `crd::resources`
/// doc-comment for why `infra_provider_ref` holds a Postgres name, not a
/// Kubernetes object reference).
pub async fn resolve_provider_by_name(
    pool: &PgPool,
    provider_name: &str,
    node_name: &str,
) -> Result<(Uuid, Arc<dyn InfraProvider>), RegistryError> {
    let row: Option<(Uuid, String, Value)> =
        sqlx::query_as("SELECT id, provider_type, config FROM providers WHERE name = $1")
            .bind(provider_name)
            .fetch_optional(pool)
            .await?;
    let (id, provider_type, config) = row.ok_or(RegistryError::NotFound)?;
    let (storage, bridge, underlay_ip) =
        resolved_node_defaults(pool, id, node_name, &config).await?;
    let config = with_resolved_node(&config, node_name, storage, bridge, underlay_ip);
    Ok((id, build_infra_provider(&provider_type, &config)?))
}

/// Fixed namespace for all resource CRs in the VM/Proxmox vertical slice.
pub const VM_NAMESPACE: &str = "crow-resources";

/// Deterministic Kubernetes object name for the `VirtualMachine` CR backing a
/// `resources` row, so no extra DB column is needed to link the two.
pub fn vm_cr_name(resource_id: Uuid) -> String {
    format!("vm-{resource_id}")
}

/// Deterministic Kubernetes object name for the `IpClaim` CR a resource
/// creates when it has an `ip_pool_ref`. Derived from the same resource id
/// as [`vm_cr_name`] so the two are trivially correlated without an extra
/// lookup or DB column.
pub fn ip_claim_cr_name(resource_id: Uuid) -> String {
    format!("vm-{resource_id}-ip")
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
    fn ip_claim_cr_name_is_derived_from_the_vm_cr_name() {
        let id = Uuid::nil();
        assert_eq!(
            ip_claim_cr_name(id),
            "vm-00000000-0000-0000-0000-000000000000-ip"
        );
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

    #[test]
    fn build_proxmox_provider_tolerates_a_missing_node() {
        // Connection-only config (no node/default_storage/default_bridge)
        // must still build — this is the create-time validation path.
        let config = serde_json::json!({
            "url": "https://pve.example.com:8006",
            "token_id": "root@pam!crow",
            "token_secret": "secret",
        });
        assert!(build_proxmox_provider(&config).is_ok());
    }

    #[test]
    fn with_resolved_node_overwrites_the_config_fields() {
        let config = serde_json::json!({ "url": "https://pve.example.com:8006" });
        let resolved = with_resolved_node(
            &config,
            "pve2",
            "local-lvm".into(),
            "vmbr1".into(),
            Some("10.255.10.11".into()),
        );
        assert_eq!(resolved["node"], "pve2");
        assert_eq!(resolved["default_storage"], "local-lvm");
        assert_eq!(resolved["default_bridge"], "vmbr1");
        assert_eq!(resolved["underlay_ip"], "10.255.10.11");
        assert_eq!(resolved["url"], "https://pve.example.com:8006");
    }

    #[test]
    fn with_resolved_node_nulls_underlay_ip_when_the_node_has_none_yet() {
        // A node adopted before migration 008, or resolved via the legacy
        // fallback -- no VTEP source available, but this must not fail the
        // whole provider build (only PrivateSubnet creation needs it).
        let config = serde_json::json!({ "url": "https://pve.example.com:8006" });
        let resolved =
            with_resolved_node(&config, "pve2", "local-lvm".into(), "vmbr1".into(), None);
        assert!(resolved["underlay_ip"].is_null());
    }
}
