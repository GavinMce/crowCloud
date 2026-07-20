use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use crow_core::types::{K8sClusterHandle, ResourceHandle};
use serde::Deserialize;
use serde_json::Value;
use sqlx::types::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/k8s-clusters/{id}/report", post(report))
}

#[derive(Deserialize)]
struct ReportRequest {
    kubeconfig: String,
}

/// Called by a `K8sCluster` control plane VM's own cloud-init script once it
/// confirms the cluster is actually up — deliberately not behind `AuthUser`,
/// since the caller is a VM, not a logged-in user. Authenticated instead by
/// a random per-cluster secret crowCloud generates and bakes into that VM's
/// cloud-init at creation time (see `crow-resource-k8s`'s
/// `ControlPlaneScriptInput`), presented here as `X-Bootstrap-Secret`.
///
/// Only writes to Postgres (`resources.handle`) — leaves `phase` and the CR
/// status alone. The operator's own reconcile loop already checks
/// `handle.kubeconfig.is_some()` on every tick and converges to `Ready`
/// on its own; duplicating that transition here would just be two owners
/// racing to decide the same thing.
async fn report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<ReportRequest>,
) -> ApiResult<StatusCode> {
    let secret = headers
        .get("X-Bootstrap-Secret")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;

    let handle_json: Option<Value> =
        sqlx::query_scalar("SELECT handle FROM resources WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    let handle_json = handle_json.ok_or(ApiError::NotFound)?;

    // `resources.handle` is always the outer `ResourceHandle{resource_type,
    // data}` envelope, never the driver's handle type directly — matches
    // every operator controller (see e.g. `virtual_machine.rs`'s identical
    // `new_handle.get("data")...` read).
    let resource_handle: ResourceHandle = serde_json::from_value(handle_json)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("corrupt resource handle: {e}")))?;
    let mut cluster_handle: K8sClusterHandle = serde_json::from_value(resource_handle.data)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("corrupt K8sCluster handle: {e}")))?;

    if cluster_handle.bootstrap_secret != secret {
        return Err(ApiError::Unauthorized);
    }

    cluster_handle.kubeconfig = Some(req.kubeconfig);

    let new_handle = ResourceHandle {
        resource_type: resource_handle.resource_type,
        data: serde_json::to_value(&cluster_handle).map_err(|e| ApiError::Internal(e.into()))?,
    };
    let new_handle_json =
        serde_json::to_value(&new_handle).map_err(|e| ApiError::Internal(e.into()))?;

    sqlx::query("UPDATE resources SET handle = $1, updated_at = NOW() WHERE id = $2")
        .bind(&new_handle_json)
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(StatusCode::NO_CONTENT)
}
