use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "crow.cloud",
    version = "v1alpha1",
    kind = "Project",
    namespaced,
    status = "ProjectStatus",
    shortname = "proj",
    printcolumn = r#"{"name":"Owner","type":"string","jsonPath":".spec.owner"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSpec {
    pub display_name: String,
    pub description: Option<String>,
    pub owner: String,
    pub members: Vec<ProjectMember>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMember {
    pub user: String,
    pub role: ProjectRole,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, PartialEq)]
pub enum ProjectRole {
    Owner,
    Editor,
    Viewer,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
    pub phase: Option<String>,
    pub resource_count: Option<u32>,
}
