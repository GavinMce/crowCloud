use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRef {
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "ResourceGroup",
    namespaced,
    status = "ResourceGroupStatus",
    shortname = "rg",
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct ResourceGroupSpec {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub infra_provider_ref: ResourceRef,
    pub network_provider_ref: Option<ResourceRef>,
    pub dns_provider_ref: Option<ResourceRef>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResourceGroupStatus {
    pub phase: Option<String>,
    pub resource_count: Option<u32>,
}
