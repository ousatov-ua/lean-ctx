//! BuiltinModelRouter — intent-aware model routing via OCLA trait.
//!
//! Wraps `proxy/model_router.rs` and `proxy/effort_routing.rs` behind the
//! canonical trait. Emits ModelRouted events. Routes to the best candidate
//! model within the cost/latency constraints.

use crate::core::config::{Config, RoutingRules};
use crate::core::ocla::traits::{ModelRouter, OclaService};
use crate::core::ocla::types::{
    ModelRouteRequest, OclaCapability, OclaCapabilityKind, OclaResult, RoutingDecision,
};
use crate::core::ocla_bus::{self, OclaEvent};
use serde_json::json;

pub struct BuiltinModelRouter {
    rules: RoutingRules,
}

impl BuiltinModelRouter {
    pub fn new() -> Self {
        Self::with_rules(Config::load().proxy.routing)
    }

    fn with_rules(rules: RoutingRules) -> Self {
        Self { rules }
    }
}

impl Default for BuiltinModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl OclaService for BuiltinModelRouter {
    fn capability(&self) -> OclaCapability {
        OclaCapability::available(OclaCapabilityKind::ModelRouter)
    }
}

impl ModelRouter for BuiltinModelRouter {
    fn route_model(&self, request: ModelRouteRequest) -> OclaResult<RoutingDecision> {
        let requested_model = request
            .candidate_models
            .first()
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let body = json!({
            "model": requested_model.clone(),
            "messages": [{"role": "user", "content": request.context.content_ref}]
        });
        let routed = crate::proxy::model_router::route(&body, &self.rules);
        let (model, provider, tier, model_changed) = routed.map_or_else(
            || {
                (
                    requested_model.clone(),
                    infer_provider(&requested_model),
                    "standard".to_string(),
                    false,
                )
            },
            |decision| {
                let model = decision.routed_model;
                let provider = decision
                    .routed_provider
                    .unwrap_or_else(|| infer_provider(&model));
                let changed = decision.model_changed;
                (model, provider, decision.tier, changed)
            },
        );

        ocla_bus::emit(OclaEvent::ModelRouted {
            requested_model: requested_model.clone(),
            routed_model: model.clone(),
            tier,
            model_changed,
        });

        Ok(RoutingDecision {
            model,
            provider,
            reasoning_budget_tokens: 4096,
            decision_ref: format!("route:{}", request.context.request_id),
        })
    }
}

fn infer_provider(model: &str) -> String {
    if model.contains("gpt") || model.contains("o1") || model.contains("o3") {
        "openai".to_string()
    } else if model.contains("claude") {
        "anthropic".to_string()
    } else if model.contains("gemini") {
        "google".to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ocla::types::OclaRequestContext;
    use std::collections::BTreeMap;

    fn route_req(candidates: &[&str]) -> ModelRouteRequest {
        ModelRouteRequest {
            context: OclaRequestContext {
                request_id: "r1".into(),
                session_id: "s1".into(),
                agent_id: "agent-test".into(),
                content_ref: "ref:test".into(),
                tenant_id: None,
            },
            candidate_models: candidates.iter().map(|s| (*s).to_string()).collect(),
            maximum_cost_micros: None,
            maximum_latency_ms: None,
        }
    }

    fn active_rules(tiers: &[(&str, &str)]) -> RoutingRules {
        RoutingRules {
            enabled: Some(true),
            aliases: BTreeMap::new(),
            tiers: tiers
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }

    #[test]
    fn routes_first_candidate() {
        let router = BuiltinModelRouter::new();
        let decision = router
            .route_model(route_req(&["gpt-4o", "claude-3"]))
            .unwrap();
        assert_eq!(decision.model, "gpt-4o");
        assert_eq!(decision.provider, "openai");
    }

    #[test]
    fn infers_anthropic_provider() {
        let router = BuiltinModelRouter::new();
        let decision = router.route_model(route_req(&["claude-sonnet-4"])).unwrap();
        assert_eq!(decision.provider, "anthropic");
    }

    #[test]
    fn delegates_tier_selection_to_proxy_router() {
        let router =
            BuiltinModelRouter::with_rules(active_rules(&[("fast", "anthropic:claude-haiku-4-5")]));
        let mut request = route_req(&["claude-sonnet-4"]);
        request.context.content_ref = "explain how the cache works".into();

        let decision = router.route_model(request).unwrap();

        assert_eq!(decision.model, "claude-haiku-4-5");
        assert_eq!(decision.provider, "anthropic");
    }

    #[test]
    fn unknown_model_falls_back_to_default() {
        let router = BuiltinModelRouter::new();
        let decision = router.route_model(route_req(&[])).unwrap();
        assert_eq!(decision.model, "default");
        assert_eq!(decision.provider, "unknown");
        assert_eq!(decision.decision_ref, "route:r1");
    }
}
