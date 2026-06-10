//! Shared SDK conformance kit (EPIC 12.5, industrialized in GL #395).
//!
//! A client-side check that proves this Rust client + a live server
//! interoperate over the **entire** frozen `/v1` contract. It is the exact
//! mirror of the Python SDK's `run_conformance` and the TypeScript SDK's
//! `runConformance`, so every first-party SDK proves the same contract and
//! they stay in lockstep.
//!
//! Two checks make this a drift gate (GL #395):
//!
//! * `route_coverage` — every path the server's OpenAPI document advertises
//!   must be covered by a client method ([`COVERED_ROUTES`]). A new server
//!   route without SDK support fails conformance in the next CI run.
//! * `engine_compat` — the server's `http_mcp` contract version must be one
//!   this SDK release supports ([`SUPPORTED_HTTP_CONTRACT_VERSIONS`]).

use serde_json::Value;

use crate::client::LeanCtxClient;
use crate::error::LeanCtxError;

/// `METHOD path` → client method. The conformance kit fails when the live
/// server's OpenAPI document lists a route missing here.
pub const COVERED_ROUTES: &[(&str, &str)] = &[
    ("GET /health", "health"),
    ("GET /v1/manifest", "manifest"),
    ("GET /v1/capabilities", "capabilities"),
    ("GET /v1/openapi.json", "openapi"),
    ("GET /v1/tools", "list_tools"),
    ("POST /v1/tools/call", "call_tool"),
    ("GET /v1/events", "subscribe_events"),
    ("GET /v1/context/summary", "context_summary"),
    ("GET /v1/events/search", "search_events"),
    ("GET /v1/events/lineage", "event_lineage"),
    ("GET /v1/metrics", "metrics"),
];

/// `http_mcp` contract versions this SDK release speaks (SemVer coupling:
/// the SDK major follows the engine contract major).
pub const SUPPORTED_HTTP_CONTRACT_VERSIONS: &[u64] = &[1];

/// One named probe result.
#[derive(Debug, Clone)]
pub struct ConformanceCheck {
    /// Stable check identifier, identical across the three SDK kits.
    pub name: &'static str,
    /// Whether the probe held against the live server.
    pub passed: bool,
    /// Failure context (empty when passed).
    pub detail: String,
}

/// The complete, comparable result of one conformance run.
#[derive(Debug, Clone, Default)]
pub struct ConformanceScorecard {
    /// All probe results, in execution order.
    pub checks: Vec<ConformanceCheck>,
}

impl ConformanceScorecard {
    /// Number of passed checks.
    #[must_use]
    pub fn passed(&self) -> usize {
        self.checks.iter().filter(|c| c.passed).count()
    }

    /// Total number of checks executed.
    #[must_use]
    pub fn total(&self) -> usize {
        self.checks.len()
    }

    /// `true` when every check passed.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    fn add(&mut self, name: &'static str, probe: impl FnOnce() -> (bool, String)) {
        let (passed, detail) = probe();
        self.checks.push(ConformanceCheck {
            name,
            passed,
            detail,
        });
    }
}

fn ok(passed: bool) -> (bool, String) {
    (passed, String::new())
}

/// Run the conformance kit against a live client.
///
/// Network/contract failures become failed checks rather than errors, so the
/// returned scorecard is always complete and comparable across SDKs.
#[must_use]
pub fn run_conformance(client: &LeanCtxClient) -> ConformanceScorecard {
    let mut card = ConformanceScorecard::default();

    card.add("health", || match client.health() {
        Ok(_) => ok(true),
        Err(e) => (false, e.to_string()),
    });

    card.add("manifest_shape", || match client.manifest() {
        Ok(m) => ok(m.is_object()),
        Err(e) => (false, e.to_string()),
    });

    card.add("capabilities_shape", || match client.capabilities() {
        Ok(caps) => ok(caps["contract_version"].is_u64()
            && caps["server"]["version"].is_string()
            && caps["plane"].is_string()
            && caps["transports"].is_array()
            && caps["features"].is_object()
            && caps["contracts"].is_object()),
        Err(e) => (false, e.to_string()),
    });

    card.add("contract_status_map", || match client.capabilities() {
        // GL #394: stability per contract is part of the discovery document.
        Ok(caps) => {
            let status = &caps["contract_status"];
            let http_mcp = status["http-mcp"].as_str().unwrap_or_default();
            let passed = status.is_object() && matches!(http_mcp, "frozen" | "stable");
            (
                passed,
                if passed {
                    String::new()
                } else {
                    format!("contract_status={status}")
                },
            )
        }
        Err(e) => (false, e.to_string()),
    });

    card.add("engine_compat", || match client.capabilities() {
        Ok(caps) => {
            let version = caps["contracts"]["leanctx.contract.http_mcp.contract_version"].as_u64();
            let passed = version.is_some_and(|v| SUPPORTED_HTTP_CONTRACT_VERSIONS.contains(&v));
            (
                passed,
                if passed {
                    String::new()
                } else {
                    format!("server http_mcp contract {version:?} unsupported")
                },
            )
        }
        Err(e) => (false, e.to_string()),
    });

    card.add("openapi_shape", || match client.openapi() {
        Ok(doc) => ok(doc["openapi"].as_str().is_some_and(|v| v.starts_with("3."))
            && doc["paths"].is_object()),
        Err(e) => (false, e.to_string()),
    });

    card.add("route_coverage", || match client.openapi() {
        // The drift gate: every advertised route needs a client method.
        Ok(doc) => {
            let uncovered = uncovered_routes(&doc);
            (uncovered.is_empty(), uncovered.join(", "))
        }
        Err(e) => (false, e.to_string()),
    });

    card.add("tools_list", || match client.list_tools(None, Some(1)) {
        Ok(listing) => ok(listing.tools.len() <= 1),
        Err(e) => (false, e.to_string()),
    });

    card.add("tool_call_error_contract", || {
        // Typed-error semantics: an unknown tool must produce a structured
        // 4xx with a machine-readable error_code, not a 5xx or free text.
        match client.call_tool("definitely_not_a_tool_conformance_probe", None, None) {
            Ok(_) => (false, "unknown tool call unexpectedly succeeded".into()),
            Err(LeanCtxError::Http(e)) => {
                let passed = (400..500).contains(&e.status) && e.error_code.is_some();
                (
                    passed,
                    if passed {
                        String::new()
                    } else {
                        format!("status={} error_code={:?}", e.status, e.error_code)
                    },
                )
            }
            Err(e) => (false, e.to_string()),
        }
    });

    card.add("events_stream", || match client.events_probe() {
        Ok(content_type) => {
            let passed = content_type.starts_with("text/event-stream");
            (
                passed,
                if passed {
                    String::new()
                } else {
                    format!("content-type={content_type}")
                },
            )
        }
        Err(e) => (false, e.to_string()),
    });

    card.add("context_summary_shape", || {
        match client.context_summary(None, None, Some(1)) {
            Ok(summary) => ok(summary["workspaceId"].is_string()
                && summary["totalEvents"].is_u64()
                && summary["eventCountsByKind"].is_object()),
            Err(e) => (false, e.to_string()),
        }
    });

    card.add("events_search_shape", || {
        match client.search_events("conformance-probe", None, None, Some(1)) {
            Ok(res) => ok(res["results"].is_array() && res["count"].is_u64()),
            Err(e) => (false, e.to_string()),
        }
    });

    card.add("event_lineage_shape", || {
        match client.event_lineage(1, Some(1), None) {
            Ok(res) => ok(!res["eventId"].is_null() && res["chain"].is_array()),
            Err(e) => (false, e.to_string()),
        }
    });

    card.add("metrics_shape", || match client.metrics() {
        Ok(m) => ok(m.is_object()),
        Err(e) => (false, e.to_string()),
    });

    card
}

fn uncovered_routes(openapi_doc: &Value) -> Vec<String> {
    let Some(paths) = openapi_doc["paths"].as_object() else {
        return vec!["openapi document has no paths object".to_string()];
    };
    let mut uncovered = Vec::new();
    for (path, ops) in paths {
        let Some(ops) = ops.as_object() else { continue };
        for method in ops.keys() {
            let route = format!("{} {}", method.to_uppercase(), path);
            if !COVERED_ROUTES.iter().any(|(r, _)| *r == route) {
                uncovered.push(route);
            }
        }
    }
    uncovered.sort();
    uncovered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_routes_are_unique() {
        let mut routes: Vec<_> = COVERED_ROUTES.iter().map(|(r, _)| *r).collect();
        routes.sort_unstable();
        let len = routes.len();
        routes.dedup();
        assert_eq!(len, routes.len());
    }

    #[test]
    fn uncovered_routes_flags_unknown_paths() {
        let doc = serde_json::json!({
            "paths": {
                "/health": { "get": {} },
                "/v1/brand-new-route": { "get": {} },
            }
        });
        let uncovered = uncovered_routes(&doc);
        assert_eq!(uncovered, vec!["GET /v1/brand-new-route".to_string()]);
    }
}
