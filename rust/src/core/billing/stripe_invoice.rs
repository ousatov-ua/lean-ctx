//! Minimal Stripe Invoicing client for the success-fee flow (GL #669).
//!
//! Scope is deliberately narrow: create one **invoice item** for the computed
//! success fee on an existing customer, and optionally draft an **invoice** that
//! pulls the pending items. This is the direct Invoicing API (not a
//! subscription / usage-based meter): the fee is a periodic, contract-negotiated
//! amount derived from the signed savings batch, not a per-unit price.
//!
//! ## Test mode is enforced
//! The client refuses any key that is not a Stripe **test** key
//! (`sk_test_…` / `rk_test_…`). Outcome-based billing is rolled out in test mode
//! first; a live key fails closed so a misconfiguration can never raise a real
//! charge from this code path.
//!
//! ## Key handling
//! The secret is read from the environment (`STRIPE_API_KEY`, or
//! `LEAN_CTX_STRIPE_API_KEY`) and never persisted to `config.toml`. A
//! [restricted key](https://docs.stripe.com/keys/restricted-api-keys) (`rk_test_…`)
//! scoped to write Invoices + Invoice Items is recommended over a full secret key.
//!
//! ## Idempotency
//! Every write carries an `Idempotency-Key`. The caller derives it from
//! `(customer, period, ledger head)` so re-running a period is a no-op on
//! Stripe's side instead of double-billing.

use std::time::Duration;

use serde::Serialize;

/// Pinned Stripe API version (see stripe-best-practices skill).
const STRIPE_API_VERSION: &str = "2026-05-27.dahlia";
const DEFAULT_BASE_URL: &str = "https://api.stripe.com";

/// A thin, blocking Stripe client bound to a single (test) API key.
pub struct StripeClient {
    secret: String,
    base_url: String,
}

/// Parameters for creating a single invoice item.
#[derive(Debug, Clone)]
pub struct InvoiceItemRequest {
    /// Stripe customer id (`cus_…`).
    pub customer: String,
    /// Amount in integer minor units (cents).
    pub amount_cents: i64,
    /// ISO currency code (e.g. `usd`).
    pub currency: String,
    /// Human-readable line description shown on the invoice.
    pub description: String,
    /// Flat metadata attached to the item (provenance: period, ledger head, …).
    pub metadata: Vec<(String, String)>,
}

/// A created Stripe object (the parts we surface), plus the raw JSON for `--json`.
#[derive(Debug, Clone, Serialize)]
pub struct StripeObject {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub raw: serde_json::Value,
}

impl StripeClient {
    /// Build a client from the environment, enforcing a **test** key.
    pub fn from_env() -> Result<Self, String> {
        let secret = std::env::var("STRIPE_API_KEY")
            .or_else(|_| std::env::var("LEAN_CTX_STRIPE_API_KEY"))
            .map_err(|_| {
                "STRIPE_API_KEY not set. Export a Stripe TEST key (sk_test_… or rk_test_…) \
                 to create the success-fee invoice."
                    .to_string()
            })?;
        let secret = secret.trim().to_string();
        if !is_test_key(&secret) {
            return Err(
                "refusing to use a non-test Stripe key: this command only operates in \
                 Stripe TEST mode (key must start with sk_test_ or rk_test_)."
                    .to_string(),
            );
        }
        let base_url = std::env::var("STRIPE_API_BASE")
            .ok()
            .map(|s| s.trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        Ok(Self { secret, base_url })
    }

    /// Create an invoice item for the success fee (idempotent per `idempotency_key`).
    pub fn create_invoice_item(
        &self,
        req: &InvoiceItemRequest,
        idempotency_key: &str,
    ) -> Result<StripeObject, String> {
        let mut form: Vec<(String, String)> = vec![
            ("customer".into(), req.customer.clone()),
            ("amount".into(), req.amount_cents.to_string()),
            ("currency".into(), req.currency.clone()),
            ("description".into(), req.description.clone()),
        ];
        for (k, v) in &req.metadata {
            form.push((format!("metadata[{k}]"), v.clone()));
        }
        self.post("/v1/invoiceitems", &form, idempotency_key)
    }

    /// Draft an invoice for `customer`, pulling its pending invoice items.
    /// `auto_advance` lets Stripe finalize + attempt collection automatically.
    pub fn create_invoice(
        &self,
        customer: &str,
        idempotency_key: &str,
        auto_advance: bool,
    ) -> Result<StripeObject, String> {
        let form: Vec<(String, String)> = vec![
            ("customer".into(), customer.to_string()),
            ("auto_advance".into(), auto_advance.to_string()),
            ("collection_method".into(), "charge_automatically".into()),
            (
                "pending_invoice_items_behavior".into(),
                "include".to_string(),
            ),
        ];
        self.post("/v1/invoices", &form, idempotency_key)
    }

    fn post(
        &self,
        path: &str,
        form: &[(String, String)],
        idempotency_key: &str,
    ) -> Result<StripeObject, String> {
        let url = format!("{}{}", self.base_url, path);
        let body = encode_form(form);
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(Duration::from_secs(30)))
                .http_status_as_error(false)
                .build(),
        );
        let resp = agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.secret))
            .header("Stripe-Version", STRIPE_API_VERSION)
            .header("Idempotency-Key", idempotency_key)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body.as_bytes())
            .map_err(|e| format!("Stripe unreachable: {e}"))?;

        let status = resp.status().as_u16();
        let text = resp.into_body().read_to_string().unwrap_or_default();
        let json: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);

        if (200..300).contains(&status) {
            let id = json
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let obj_status = json
                .get("status")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            Ok(StripeObject {
                id,
                status: obj_status,
                raw: json,
            })
        } else {
            let msg = json
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or(&text);
            Err(format!("Stripe rejected ({status}): {msg}"))
        }
    }
}

/// Whether `key` is a Stripe **test** secret or restricted key.
#[must_use]
pub fn is_test_key(key: &str) -> bool {
    let k = key.trim();
    k.starts_with("sk_test_") || k.starts_with("rk_test_")
}

/// Encode form pairs as `application/x-www-form-urlencoded`.
fn encode_form(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_test_keys_accepted() {
        assert!(is_test_key("sk_test_abc"));
        assert!(is_test_key("rk_test_abc"));
        assert!(!is_test_key("sk_live_abc"));
        assert!(!is_test_key("rk_live_abc"));
        assert!(!is_test_key(""));
        assert!(!is_test_key("pk_test_abc"));
    }

    #[test]
    fn form_encoding_escapes_and_joins() {
        let pairs = vec![
            ("customer".to_string(), "cus_123".to_string()),
            ("amount".to_string(), "2600".to_string()),
            ("description".to_string(), "fee (period all)".to_string()),
            ("metadata[period]".to_string(), "all".to_string()),
        ];
        let enc = encode_form(&pairs);
        assert!(enc.contains("customer=cus_123"));
        assert!(enc.contains("amount=2600"));
        assert!(enc.contains("description=fee%20%28period%20all%29"));
        assert!(enc.contains("metadata%5Bperiod%5D=all"));
        assert_eq!(enc.matches('&').count(), 3);
    }
}
