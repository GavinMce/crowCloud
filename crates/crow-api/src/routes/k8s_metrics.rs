use axum::{
    extract::{Path, State},
    http::Method,
    Json,
};
use crow_core::types::{K8sClusterHandle, ResourceHandle as CoreResourceHandle};
use k8s_openapi::api::core::v1::Node;
use kube::{
    api::{Api, ListParams},
    config::{KubeConfigOptions, Kubeconfig},
    Client, Config,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{
    error::{ApiError, ApiResult},
    middleware::AuthUser,
    AppState,
};

#[derive(Serialize)]
pub struct NodeMetric {
    name: String,
    ready: bool,
    cpu_usage_millicores: Option<u64>,
    cpu_capacity_millicores: Option<u64>,
    memory_usage_bytes: Option<u64>,
    memory_capacity_bytes: Option<u64>,
}

#[derive(Serialize)]
pub struct ClusterMetricsResponse {
    nodes: Vec<NodeMetric>,
}

#[derive(Deserialize)]
struct NodeMetricsList {
    #[serde(default)]
    items: Vec<NodeMetricsItem>,
}

#[derive(Deserialize)]
struct NodeMetricsItem {
    metadata: NodeMetricsMeta,
    usage: NodeMetricsUsage,
}

#[derive(Deserialize)]
struct NodeMetricsMeta {
    name: String,
}

#[derive(Deserialize)]
struct NodeMetricsUsage {
    cpu: String,
    memory: String,
}

/// Parses a Kubernetes CPU quantity ("500m", "2", "1500u") into millicores.
fn parse_cpu_millicores(raw: &str) -> Option<u64> {
    if let Some(n) = raw.strip_suffix('m') {
        n.parse::<f64>().ok().map(|v| v.round() as u64)
    } else if let Some(n) = raw.strip_suffix('u') {
        n.parse::<f64>().ok().map(|v| (v / 1000.0).round() as u64)
    } else if let Some(n) = raw.strip_suffix('n') {
        n.parse::<f64>()
            .ok()
            .map(|v| (v / 1_000_000.0).round() as u64)
    } else {
        raw.parse::<f64>().ok().map(|v| (v * 1000.0).round() as u64)
    }
}

/// Parses a Kubernetes memory quantity ("512Ki", "2Gi", "1024") into bytes.
/// Binary (Ki/Mi/Gi/Ti) and decimal (K/M/G/T) suffixes both appear in the
/// wild depending on where the quantity originated.
fn parse_memory_bytes(raw: &str) -> Option<u64> {
    const UNITS: &[(&str, f64)] = &[
        ("Ki", 1024.0),
        ("Mi", 1024.0 * 1024.0),
        ("Gi", 1024.0 * 1024.0 * 1024.0),
        ("Ti", 1024.0 * 1024.0 * 1024.0 * 1024.0),
        ("K", 1000.0),
        ("M", 1_000_000.0),
        ("G", 1_000_000_000.0),
        ("T", 1_000_000_000_000.0),
    ];
    for (suffix, multiplier) in UNITS {
        if let Some(n) = raw.strip_suffix(suffix) {
            return n
                .parse::<f64>()
                .ok()
                .map(|v| (v * multiplier).round() as u64);
        }
    }
    raw.parse::<f64>().ok().map(|v| v.round() as u64)
}

async fn build_client(kubeconfig_yaml: &str) -> ApiResult<Client> {
    let kubeconfig = Kubeconfig::from_yaml(kubeconfig_yaml)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("invalid stored kubeconfig: {e}")))?;
    let config = Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to build cluster config: {e}")))?;
    Client::try_from(config)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to build cluster client: {e}")))
}

async fn fetch_node_metrics(client: &Client) -> ApiResult<Vec<NodeMetricsItem>> {
    let req = http::Request::builder()
        .method(Method::GET)
        .uri("/apis/metrics.k8s.io/v1beta1/nodes")
        .body(Vec::new())
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
    // metrics-server can be a beat behind on a freshly-Ready cluster — an
    // empty/missing metrics list isn't a real error, just "no data yet",
    // so usage is left `None` per-node rather than failing the whole route.
    let list: NodeMetricsList = client
        .request(req)
        .await
        .unwrap_or(NodeMetricsList { items: Vec::new() });
    Ok(list.items)
}

pub async fn cluster_metrics(
    AuthUser(_): AuthUser,
    State(state): State<AppState>,
    Path((project, name)): Path<(String, String)>,
) -> ApiResult<Json<ClusterMetricsResponse>> {
    let handle_json: Option<Value> = sqlx::query_scalar(
        "SELECT handle FROM resources WHERE project = $1 AND name = $2 AND resource_type = 'k8s_cluster'",
    )
    .bind(&project)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::NotFound)?;

    let Some(handle_json) = handle_json else {
        return Err(ApiError::Conflict(
            "cluster is still bootstrapping — no metrics yet".to_string(),
        ));
    };
    let resource_handle: CoreResourceHandle = serde_json::from_value(handle_json)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("corrupt resource handle: {e}")))?;
    let cluster_handle: K8sClusterHandle = serde_json::from_value(resource_handle.data)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("corrupt K8sCluster handle: {e}")))?;
    let kubeconfig = cluster_handle.kubeconfig.ok_or_else(|| {
        ApiError::Conflict("cluster is still bootstrapping — no metrics yet".to_string())
    })?;

    let client = build_client(&kubeconfig).await?;

    let nodes: Api<Node> = Api::all(client.clone());
    let node_list = nodes
        .list(&ListParams::default())
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to list cluster nodes: {e}")))?;

    let usage_by_name: HashMap<String, (String, String)> = fetch_node_metrics(&client)
        .await?
        .into_iter()
        .map(|item| (item.metadata.name, (item.usage.cpu, item.usage.memory)))
        .collect();

    let node_metrics = node_list
        .items
        .into_iter()
        .map(|node| {
            let node_name = node.metadata.name.clone().unwrap_or_default();
            let ready = node
                .status
                .as_ref()
                .and_then(|s| s.conditions.as_ref())
                .and_then(|conds| conds.iter().find(|c| c.type_ == "Ready"))
                .map(|c| c.status == "True")
                .unwrap_or(false);

            let capacity = node.status.as_ref().and_then(|s| s.capacity.as_ref());
            let cpu_capacity_millicores = capacity
                .and_then(|c| c.get("cpu"))
                .and_then(|q| parse_cpu_millicores(&q.0));
            let memory_capacity_bytes = capacity
                .and_then(|c| c.get("memory"))
                .and_then(|q| parse_memory_bytes(&q.0));

            let (cpu_usage_millicores, memory_usage_bytes) = usage_by_name
                .get(&node_name)
                .map(|(cpu, mem)| (parse_cpu_millicores(cpu), parse_memory_bytes(mem)))
                .unwrap_or((None, None));

            NodeMetric {
                name: node_name,
                ready,
                cpu_usage_millicores,
                cpu_capacity_millicores,
                memory_usage_bytes,
                memory_capacity_bytes,
            }
        })
        .collect();

    Ok(Json(ClusterMetricsResponse {
        nodes: node_metrics,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_quantity_parses_millicore_suffix() {
        assert_eq!(parse_cpu_millicores("500m"), Some(500));
    }

    #[test]
    fn cpu_quantity_parses_bare_cores_as_millicores() {
        assert_eq!(parse_cpu_millicores("2"), Some(2000));
        assert_eq!(parse_cpu_millicores("0.5"), Some(500));
    }

    #[test]
    fn cpu_quantity_parses_nano_and_micro_suffixes() {
        assert_eq!(parse_cpu_millicores("1500000n"), Some(2));
        assert_eq!(parse_cpu_millicores("1500u"), Some(2));
    }

    #[test]
    fn memory_quantity_parses_binary_suffixes() {
        assert_eq!(parse_memory_bytes("1Ki"), Some(1024));
        assert_eq!(parse_memory_bytes("1Mi"), Some(1024 * 1024));
        assert_eq!(parse_memory_bytes("2Gi"), Some(2 * 1024 * 1024 * 1024));
    }

    #[test]
    fn memory_quantity_parses_decimal_suffixes_and_bare_bytes() {
        assert_eq!(parse_memory_bytes("1K"), Some(1000));
        assert_eq!(parse_memory_bytes("512"), Some(512));
    }
}
