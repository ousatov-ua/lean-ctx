//! Live model prices from the OpenRouter models API (#1179).
//!
//! The embedded table in [`super::model_pricing`] can only ever know the
//! models that existed at release time; anything newer used to fall into a
//! family heuristic and could be priced an order of magnitude off (a tester's
//! DeepSeek V4 Flash was billed at 2025 V3 list prices — ~15× too high).
//!
//! This module fetches `GET https://openrouter.ai/api/v1/models` (public, no
//! key, ~340 models covering every major vendor), converts the USD-per-token
//! strings to per-MTok [`ModelCost`] rows, and caches the result on disk. The
//! table is loaded into a process-wide snapshot **only when a run-mode opts
//! in** (`ensure_loaded` / `spawn_background_refresh` from the proxy, gateway
//! or spend CLI): plain CLI tools and the test suite keep the deterministic
//! embedded table.
//!
//! Precedence inside [`super::model_pricing::ModelPricing::quote`]:
//! embedded exact > **live** > heuristic > blended fallback. Live hits are
//! [`super::model_pricing::PricingMatchKind::Live`] — current market data,
//! not an estimate.
//!
//! Kill switch: `LEAN_CTX_LIVE_PRICING=off|0|false`.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use super::model_pricing::ModelCost;

/// Refresh cadence for the background task. Provider price changes are rare
/// events; half a day keeps drift negligible without hammering the API.
const REFRESH_INTERVAL_SECS: u64 = 12 * 60 * 60;

/// On-disk cache file, under the lean-ctx cache directory.
const CACHE_FILE: &str = "model-prices.json";

const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// A fetched-and-indexed price table. `models` is keyed by canonicalized
/// lookup keys (see `canon`) — several keys may point at the same cost row.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LivePriceTable {
    /// Unix seconds of the successful fetch that produced this table.
    pub fetched_at: u64,
    pub models: HashMap<String, ModelCost>,
}

impl LivePriceTable {
    /// Number of distinct lookup keys (not models) in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

fn snapshot() -> &'static RwLock<Option<Arc<LivePriceTable>>> {
    static SNAP: OnceLock<RwLock<Option<Arc<LivePriceTable>>>> = OnceLock::new();
    SNAP.get_or_init(|| RwLock::new(None))
}

/// True unless the operator disabled live pricing.
fn enabled() -> bool {
    let v = std::env::var("LEAN_CTX_LIVE_PRICING").unwrap_or_default();
    !matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "off" | "0" | "false" | "no"
    )
}

/// Looks a model up in the current live snapshot. `None` when the snapshot
/// was never loaded (CLI tools, tests), the kill switch is set, or the model
/// is genuinely unknown to the provider list.
#[must_use]
pub fn lookup(model: &str) -> Option<(String, ModelCost)> {
    if !enabled() {
        return None;
    }
    let guard = snapshot()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let table = guard.as_ref()?;
    for key in lookup_candidates(model) {
        if let Some(cost) = table.models.get(&key) {
            return Some((key, *cost));
        }
    }
    None
}

/// Loads the disk cache into the process snapshot (idempotent, no network).
/// Returns the number of lookup keys now available. Call sites are the
/// run-modes that *want* live prices: proxy, gateway server, spend CLI.
pub fn ensure_loaded() -> usize {
    if !enabled() {
        return 0;
    }
    {
        let guard = snapshot()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(t) = guard.as_ref() {
            return t.len();
        }
    }
    let Some(table) = load_cache_file() else {
        return 0;
    };
    let len = table.len();
    install(table);
    len
}

/// Installs a table as the process snapshot (also used by tests).
pub fn install(table: LivePriceTable) {
    let mut guard = snapshot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = Some(Arc::new(table));
}

/// `(fetched_at_unix, lookup_keys)` of the active snapshot, for status
/// surfaces. `None` when live pricing is off or never loaded.
#[must_use]
pub fn status() -> Option<(u64, usize)> {
    if !enabled() {
        return None;
    }
    let guard = snapshot()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.as_ref().map(|t| (t.fetched_at, t.len()))
}

/// Test-only: clears the process snapshot so other tests see the embedded table.
#[cfg(test)]
pub fn clear_for_tests() {
    let mut guard = snapshot()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = None;
}

fn cache_path() -> Option<std::path::PathBuf> {
    crate::core::paths::cache_dir()
        .ok()
        .map(|d| d.join(CACHE_FILE))
}

fn load_cache_file() -> Option<LivePriceTable> {
    let path = cache_path()?;
    let raw = std::fs::read(path).ok()?;
    let table: LivePriceTable = serde_json::from_slice(&raw).ok()?;
    if table.is_empty() { None } else { Some(table) }
}

/// Atomic write (tmp + rename) so a crashed refresh never truncates the cache.
fn store_cache_file(table: &LivePriceTable) {
    let Some(path) = cache_path() else { return };
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return;
    }
    let Ok(json) = serde_json::to_vec(table) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Fetches the OpenRouter model list and swaps the snapshot + disk cache.
/// Returns the number of lookup keys on success.
///
/// # Errors
/// Network / decode errors propagate; the caller decides whether they matter
/// (the background task just logs and keeps the previous table — fail-open).
pub async fn refresh_now(client: &reqwest::Client) -> anyhow::Result<usize> {
    let body = client
        .get(MODELS_URL)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    let models = parse_openrouter_models(&json);
    anyhow::ensure!(!models.is_empty(), "OpenRouter model list parsed empty");
    let table = LivePriceTable {
        fetched_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        models,
    };
    let len = table.len();
    store_cache_file(&table);
    install(table);
    Ok(len)
}

/// Loads the disk cache immediately, then keeps the table fresh in the
/// background (first fetch runs at once when the cache is missing or older
/// than the refresh interval). Never blocks or fails the caller; idempotent —
/// a process embedding both proxy and gateway spawns exactly one refresher.
pub fn spawn_background_refresh() {
    static SPAWNED: OnceLock<()> = OnceLock::new();
    if !enabled() {
        return;
    }
    ensure_loaded();
    if SPAWNED.set(()).is_err() {
        return;
    }
    tokio::spawn(async {
        let client = reqwest::Client::new();
        loop {
            let stale = {
                let guard = snapshot()
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.as_ref().is_none_or(|t| {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs());
                    now.saturating_sub(t.fetched_at) >= REFRESH_INTERVAL_SECS
                })
            };
            if stale {
                match refresh_now(&client).await {
                    Ok(n) => tracing::info!("live model pricing refreshed ({n} lookup keys)"),
                    Err(e) => tracing::warn!(
                        "live model pricing refresh failed (keeping previous table): {e:#}"
                    ),
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(REFRESH_INTERVAL_SECS / 12)).await;
        }
    });
}

/// Canonical lookup form: lowercase, `.`→`-`, whitespace→`-`. Applied to both
/// index keys and queries so `claude-opus-4.5` and `claude-opus-4-5` unify.
fn canon(s: &str) -> String {
    s.trim().to_lowercase().replace([' ', '.'], "-")
}

/// Strips a trailing `-YYYYMMDD` date stamp (`deepseek-v4-flash-20260423`).
fn strip_date_suffix(s: &str) -> Option<&str> {
    let (base, tail) = s.rsplit_once('-')?;
    if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) {
        Some(base)
    } else {
        None
    }
}

/// Ordered candidate keys for a query: exact canon first, then progressively
/// vendor-stripped / variant-stripped / date-stripped forms.
fn lookup_candidates(model: &str) -> Vec<String> {
    let full = canon(model);
    if full.is_empty() {
        return Vec::new();
    }
    let mut out = vec![full.clone()];
    let mut push = |s: String| {
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    };
    let no_vendor = full.split_once('/').map(|(_, m)| m.to_string());
    if let Some(nv) = &no_vendor {
        push(nv.clone());
    }
    for base in [Some(full.as_str()), no_vendor.as_deref()]
        .into_iter()
        .flatten()
    {
        let no_variant = base.split(':').next().unwrap_or(base);
        push(no_variant.to_string());
        if let Some(no_date) = strip_date_suffix(no_variant) {
            push(no_date.to_string());
        }
    }
    out
}

/// Index keys for one provider model id/slug. Mirrors [`lookup_candidates`]
/// so every form a client might send resolves. `with_variant` ids (`:free`)
/// only claim the variant-less keys when nothing else owns them.
fn index_keys(id: &str) -> (Vec<String>, bool) {
    let full = canon(id);
    let has_variant = full.contains(':');
    let mut keys = vec![full.clone()];
    let mut push = |s: String| {
        if !s.is_empty() && !keys.contains(&s) {
            keys.push(s);
        }
    };
    let no_vendor = full.split_once('/').map(|(_, m)| m.to_string());
    if let Some(nv) = &no_vendor {
        push(nv.clone());
    }
    for base in [Some(full.as_str()), no_vendor.as_deref()]
        .into_iter()
        .flatten()
    {
        let no_variant = base.split(':').next().unwrap_or(base);
        push(no_variant.to_string());
        if let Some(no_date) = strip_date_suffix(no_variant) {
            push(no_date.to_string());
        }
    }
    (keys, has_variant)
}

/// USD-per-token decimal string → USD per MTok. `None` for absent/invalid.
fn per_mtok(pricing: &serde_json::Value, field: &str) -> Option<f64> {
    let v = pricing.get(field)?;
    let n = v
        .as_str()
        .map_or_else(|| v.as_f64(), |s| s.trim().parse::<f64>().ok())?;
    if n.is_finite() && n >= 0.0 {
        Some(n * 1_000_000.0)
    } else {
        None
    }
}

/// Parses the OpenRouter `GET /api/v1/models` payload into the lookup map.
///
/// Pricing fields are USD-per-token strings; absent cache fields mean "no
/// separate cache pricing" — those tokens bill at the input rate (the same
/// convention the embedded table uses for Gemini/Foundry). `web_search` and
/// other per-request fees are not token prices and are ignored.
fn parse_openrouter_models(json: &serde_json::Value) -> HashMap<String, ModelCost> {
    let mut map: HashMap<String, ModelCost> = HashMap::new();
    let Some(data) = json.get("data").and_then(serde_json::Value::as_array) else {
        return map;
    };

    // Two passes: variant-less ids first so `model:free` never hijacks the
    // canonical `model` key; variants still resolve under their full name.
    let mut deferred: Vec<(&serde_json::Value, &str)> = Vec::new();
    let absorb = |map: &mut HashMap<String, ModelCost>, m: &serde_json::Value, id: &str| {
        let Some(pricing) = m.get("pricing") else {
            return;
        };
        let (Some(input), Some(output)) =
            (per_mtok(pricing, "prompt"), per_mtok(pricing, "completion"))
        else {
            return;
        };
        let cost = ModelCost {
            input_per_m: input,
            output_per_m: output,
            cache_write_per_m: per_mtok(pricing, "input_cache_write").unwrap_or(input),
            cache_read_per_m: per_mtok(pricing, "input_cache_read").unwrap_or(input),
        };
        let (keys, _) = index_keys(id);
        for key in keys {
            map.entry(key).or_insert(cost);
        }
        // The dated canonical slug resolves date-stamped client model names
        // (`deepseek-v4-flash-20260423`) even when the id carries no date.
        if let Some(slug) = m.get("canonical_slug").and_then(serde_json::Value::as_str) {
            let (slug_keys, _) = index_keys(slug);
            for key in slug_keys {
                map.entry(key).or_insert(cost);
            }
        }
    };

    for m in data {
        let Some(id) = m.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if canon(id).contains(':') {
            deferred.push((m, id));
        } else {
            absorb(&mut map, m, id);
        }
    }
    for (m, id) in deferred {
        absorb(&mut map, m, id);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> serde_json::Value {
        serde_json::json!({
            "data": [
                {
                    "id": "deepseek/deepseek-v4-flash",
                    "canonical_slug": "deepseek/deepseek-v4-flash-20260423",
                    "pricing": {"prompt": "0.00000007", "completion": "0.00000028",
                                 "input_cache_read": "0.000000007"}
                },
                {
                    "id": "anthropic/claude-sonnet-5",
                    "canonical_slug": "anthropic/claude-sonnet-5-20260630",
                    "pricing": {"prompt": "0.000002", "completion": "0.00001",
                                 "web_search": "0.01",
                                 "input_cache_read": "0.0000002",
                                 "input_cache_write": "0.0000025"}
                },
                {
                    "id": "poolside/laguna-xs-2.1:free",
                    "canonical_slug": "poolside/laguna-xs-2.1-20260625",
                    "pricing": {"prompt": "0", "completion": "0"}
                },
                {
                    "id": "poolside/laguna-xs-2.1",
                    "canonical_slug": "poolside/laguna-xs-2.1-20260625",
                    "pricing": {"prompt": "0.00000006", "completion": "0.00000012"}
                },
                {"id": "broken/no-pricing"}
            ]
        })
    }

    #[test]
    fn parses_usd_per_token_strings_into_per_mtok() {
        let map = parse_openrouter_models(&fixture());
        let flash = map.get("deepseek/deepseek-v4-flash").expect("indexed");
        assert!((flash.input_per_m - 0.07).abs() < 1e-9);
        assert!((flash.output_per_m - 0.28).abs() < 1e-9);
        assert!((flash.cache_read_per_m - 0.007).abs() < 1e-9);
        // No explicit cache write price → bills at the input rate.
        assert!((flash.cache_write_per_m - 0.07).abs() < 1e-9);
        assert!(!map.contains_key("broken/no-pricing"));
    }

    #[test]
    fn date_stamped_and_vendor_prefixed_names_resolve() {
        let map = parse_openrouter_models(&fixture());
        // The exact name Nicolas' client sent (#1179):
        for name in [
            "deepseek/deepseek-v4-flash-20260423",
            "deepseek-v4-flash-20260423",
            "deepseek-v4-flash",
        ] {
            let mut found = false;
            for key in lookup_candidates(name) {
                if map.contains_key(&key) {
                    found = true;
                    break;
                }
            }
            assert!(found, "{name} must resolve against the live table");
        }
    }

    #[test]
    fn free_variant_never_hijacks_the_paid_model_key() {
        let map = parse_openrouter_models(&fixture());
        let paid = map
            .get("poolside/laguna-xs-2-1")
            .expect("paid model indexed");
        assert!(
            paid.input_per_m > 0.0,
            "canonical key must carry the paid price"
        );
        let free = map
            .get("poolside/laguna-xs-2-1:free")
            .expect("variant indexed");
        assert_eq!(
            free.input_per_m, 0.0,
            "the :free variant stays free under its full name"
        );
    }

    #[test]
    fn dot_dash_and_case_unify() {
        assert_eq!(canon("Claude-Opus-4.5"), "claude-opus-4-5");
        assert_eq!(
            strip_date_suffix("deepseek-v4-flash-20260423"),
            Some("deepseek-v4-flash")
        );
        assert_eq!(
            strip_date_suffix("claude-opus-4-5"),
            None,
            "short numeric tails are versions"
        );
        assert_eq!(strip_date_suffix("no-date"), None);
    }

    #[test]
    fn snapshot_lookup_respects_kill_switch_and_install() {
        let _lock = crate::core::data_dir::test_env_lock();
        clear_for_tests();
        assert!(
            lookup("deepseek/deepseek-v4-flash").is_none(),
            "empty snapshot"
        );

        install(LivePriceTable {
            fetched_at: 1,
            models: parse_openrouter_models(&fixture()),
        });
        let (_, cost) = lookup("deepseek/deepseek-v4-flash-20260423").expect("live hit");
        assert!((cost.input_per_m - 0.07).abs() < 1e-9);

        crate::test_env::set_var("LEAN_CTX_LIVE_PRICING", "off");
        assert!(
            lookup("deepseek/deepseek-v4-flash").is_none(),
            "kill switch"
        );
        crate::test_env::remove_var("LEAN_CTX_LIVE_PRICING");
        clear_for_tests();
    }
}
