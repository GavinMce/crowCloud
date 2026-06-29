use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecretRef {
    pub name: String,
    pub key: String,
}

// --- Proxmox ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "ProxmoxProvider",
    namespaced,
    status = "ProxmoxProviderStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct ProxmoxProviderSpec {
    pub url: String,
    pub token_id: String,
    pub token_secret_ref: SecretRef,
    pub node: String,
    pub default_bridge: String,
    pub default_storage: String,
    pub insecure_skip_tls: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxmoxProviderStatus {
    pub connected: Option<bool>,
    pub version: Option<String>,
}

// --- Hetzner ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "HetznerProvider",
    namespaced,
    status = "HetznerProviderStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct HetznerProviderSpec {
    pub api_token_secret_ref: SecretRef,
    pub default_location: String,
    pub default_server_type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct HetznerProviderStatus {
    pub connected: Option<bool>,
}

// --- OPNsense ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "OPNsenseProvider",
    namespaced,
    status = "OPNsenseProviderStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct OPNsenseProviderSpec {
    pub url: String,
    pub api_key_secret_ref: SecretRef,
    pub public_ip: Option<String>,
    pub public_domain: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct OPNsenseProviderStatus {
    pub connected: Option<bool>,
    pub version: Option<String>,
}

// --- Cloudflare ---

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "CloudflareProvider",
    namespaced,
    status = "CloudflareProviderStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareProviderSpec {
    pub api_token_secret_ref: SecretRef,
    pub zone_id: String,
    pub domain: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareProviderStatus {
    pub connected: Option<bool>,
}
