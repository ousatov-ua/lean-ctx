//! Edge client to the private commercial control-plane (`lean-ctx-cloud`).
//!
//! This is the *only* place the open community backend learns an account's paid
//! plan. It calls the private billing service's `/api/billing/entitlements`
//! endpoint with the shared internal key. If the billing service is not
//! configured or unreachable, every account resolves to
//! [`Plan::Free`](crate::core::billing::Plan) — so the open backend runs fully
//! standalone and **no local capability is ever gated** (Local-Free Invariant).

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::core::billing::Plan;

use super::auth::{auth_user, AppState};
use super::config::Config;

/// Resolve a user's effective plan via the private billing service. Any failure
/// (unconfigured, network error, bad response) degrades gracefully to
/// [`Plan::Free`] — the safe default that grants no commercial entitlements.
pub(super) async fn resolve_plan(cfg: &Config, user_id: Uuid) -> Plan {
    let (Some(base), Some(key)) = (
        cfg.billing_base_url.clone(),
        cfg.billing_internal_key.clone(),
    ) else {
        return Plan::Free;
    };

    let url = format!("{base}/api/billing/entitlements/{user_id}");
    let body = tokio::task::spawn_blocking(move || {
        ureq::get(&url)
            .header("X-Internal-Key", &key)
            .call()
            .ok()?
            .into_body()
            .read_to_string()
            .ok()
    })
    .await
    .ok()
    .flatten();

    let Some(body) = body else { return Plan::Free };
    serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| v.get("plan").and_then(Value::as_str).map(Plan::parse))
        .unwrap_or(Plan::Free)
}

/// `GET /api/account/entitlements` — the logged-in user's plan and the
/// additive Team/Cloud entitlements it grants.
pub(super) async fn get_account_entitlements(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, String)> {
    let (user_id, _email) = auth_user(&state, &headers).await?;
    let plan = resolve_plan(&state.cfg, user_id).await;
    Ok(Json(json!({
        "plan": plan.as_str(),
        "entitlements": plan.entitlements(),
    })))
}
