use axum::{http::HeaderMap, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::types::Uuid;

use crow_provider_registry::build_infra_provider;

use crate::{
    error::{ApiError, ApiResult},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/hosts/register", post(register))
}

#[derive(Deserialize)]
struct RegisterRequest {
    mac_address: String,
    node_name: String,
    default_storage: String,
    default_bridge: String,
    management_ip: String,
    /// Only consulted when no Proxmox provider exists yet (the seed
    /// host's own bootstrap, calling back in after standing crowCloud up
    /// on itself) -- lets it become the fleet's first provider
    /// automatically instead of that being a manual `crow provider
    /// add-proxmox` step after the fact. Ignored (a provider already
    /// exists) for every other host self-registering as an additional
    /// node.
    proxmox_url: Option<String>,
    proxmox_token_id: Option<String>,
    proxmox_token_secret: Option<String>,
    /// Fleet-wide BGP facts needed for `PrivateSubnet`'s VXLAN/EVPN
    /// dataplane -- specifically, to create the one-time Proxmox SDN EVPN
    /// controller pointed at VyOS (`crow-provider-proxmox::network`).
    /// Provider-wide, not per-node (unlike e.g. `management_ip`), so
    /// these only matter -- and are only consulted -- alongside the
    /// other `proxmox_*` fields above, on first-provider creation.
    bgp_asn: Option<u32>,
    bgp_route_reflector_ip: Option<String>,
}

/// Tells the post-install hook (#66/#67) what to do with `pvecm` --
/// resolved by crowCloud, not baked into the image at build time, so it
/// never points at a target that's since gone stale (down, rebuilt,
/// decommissioned).
#[derive(Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ClusterAction {
    /// No other node with a known `management_ip` exists yet for this
    /// provider -- this is the first real node, run `pvecm create`.
    Create,
    /// `pvecm add <join_host>` against the most recently registered
    /// other node. "Most recently registered" is a weak proxy for
    /// "healthy" -- there's no heartbeat/liveness tracking on
    /// `provider_nodes` yet, so this is a known limitation, not a
    /// guarantee the target is actually up.
    Join { join_host: String },
}

#[derive(Serialize)]
struct RegisterResponse {
    node_name: String,
    cluster_action: ClusterAction,
}

/// The self-registration callback -- called once by a host's post-install
/// hook after it's applied its underlay config (loopback, VLANs, FRR),
/// before it decides how to join the Proxmox cluster. Not behind
/// `AuthUser`, since the caller is unprovisioned hardware, not a
/// logged-in user.
///
/// Authenticated instead by `X-Fleet-Secret`, a secret baked into the
/// image at build time by `crow-cli iso proxmox build` (#66) -- not
/// minted or looked up per-host. Trust is "this image came from our own
/// tooling", not "an admin declared this specific MAC in advance" (the
/// per-MAC `pending_hosts` design this replaced) and not "merely present
/// on the network" (the alternative that was explicitly rejected).
///
/// Unlike a single-use bootstrap secret, a fleet secret is meant to
/// authenticate many hosts -- every image built with it, for the fleet's
/// lifetime or until rotated/revoked via `fleet_secrets.rs`. Re-running
/// registration for an already-known `(provider_id, node_name)` is
/// idempotent (`ON CONFLICT ... DO UPDATE`), so a reinstalled host
/// re-registering isn't an error.
async fn register(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<Json<RegisterResponse>> {
    let secret = headers
        .get("X-Fleet-Secret")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;

    let valid: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM fleet_secrets WHERE secret = $1 AND revoked_at IS NULL")
            .bind(secret)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    valid.ok_or(ApiError::Unauthorized)?;

    // v1 assumes a single-fleet deployment: exactly one Proxmox provider
    // to join. A fleet spanning multiple independent Proxmox providers
    // isn't supported yet -- ambiguous which one a new host should join,
    // and not a real requirement at the scale this is being built for.
    let provider_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM providers WHERE provider_type = 'proxmox'")
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.into()))?;
    let provider_id =
        match provider_ids.as_slice() {
            [id] => *id,
            [] => {
                // The seed host's own bootstrap calling back in after
                // standing crowCloud up on itself -- becomes the
                // fleet's first provider automatically if it brought
                // credentials for itself, same shape `providers::create`
                // builds and validates.
                let (Some(url), Some(token_id), Some(token_secret)) = (
                    &req.proxmox_url,
                    &req.proxmox_token_id,
                    &req.proxmox_token_secret,
                ) else {
                    return Err(ApiError::Conflict(
                        "no Proxmox provider exists yet to register against".to_string(),
                    ));
                };
                let config = json!({
                    "url": url,
                    "token_id": token_id,
                    "token_secret": token_secret,
                    "node": req.node_name,
                    "default_storage": req.default_storage,
                    "default_bridge": req.default_bridge,
                    "bgp_asn": req.bgp_asn,
                    "bgp_route_reflector_ip": req.bgp_route_reflector_ip,
                });
                build_infra_provider("proxmox", &config)?;
                let name = format!("proxmox-{}", req.node_name);
                sqlx::query_scalar::<_, Uuid>(
                    "INSERT INTO providers (name, provider_type, config)
                     VALUES ($1, 'proxmox', $2)
                     RETURNING id",
                )
                .bind(&name)
                .bind(&config)
                .fetch_one(&state.db)
                .await
                .map_err(|e| ApiError::Internal(e.into()))?
            }
            _ => return Err(ApiError::Conflict(
                "multiple Proxmox providers exist; automatic fleet registration needs exactly one"
                    .to_string(),
            )),
        };

    // Resolved *before* the upsert below, so this node never sees
    // itself as its own join target.
    let join_host: Option<String> = sqlx::query_scalar(
        "SELECT management_ip FROM provider_nodes
         WHERE provider_id = $1 AND node_name != $2 AND management_ip IS NOT NULL
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(provider_id)
    .bind(&req.node_name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    sqlx::query(
        "INSERT INTO provider_nodes (provider_id, node_name, default_storage, default_bridge, management_ip)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider_id, node_name)
         DO UPDATE SET default_storage = EXCLUDED.default_storage,
                        default_bridge = EXCLUDED.default_bridge,
                        management_ip = EXCLUDED.management_ip,
                        updated_at = NOW()",
    )
    .bind(provider_id)
    .bind(&req.node_name)
    .bind(&req.default_storage)
    .bind(&req.default_bridge)
    .bind(&req.management_ip)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    tracing::info!(
        mac = %req.mac_address,
        node_name = %req.node_name,
        "host self-registered"
    );

    let cluster_action = match join_host {
        Some(join_host) => ClusterAction::Join { join_host },
        None => ClusterAction::Create,
    };

    Ok(Json(RegisterResponse {
        node_name: req.node_name,
        cluster_action,
    }))
}
