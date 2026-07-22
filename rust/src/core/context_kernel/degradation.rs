//! Kernel degradation levels and fallback planning.

use std::collections::HashMap;

use super::types::{ContextPlanV1, ExcludedEntry, PlanBudget, PlanEntry};

/// Operational capability remaining after provider failures.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum DegradationLevel {
    /// All configured providers are available.
    #[default]
    Full,
    /// At least half of the configured providers are available.
    Reduced,
    /// At least one provider is available.
    Minimal,
    /// No providers are available, so the kernel must be bypassed.
    Bypass,
}

/// Current availability information for one provider.
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub provider_id: String,
    pub available: bool,
    pub last_error: Option<String>,
}

/// Aggregate provider health used to select a degradation level.
#[derive(Debug, Clone)]
pub struct KernelHealth {
    providers: Vec<ProviderStatus>,
}

impl KernelHealth {
    /// Creates a health snapshot from the configured provider statuses.
    pub fn new(providers: Vec<ProviderStatus>) -> Self {
        Self { providers }
    }

    /// Returns the capability level supported by the available providers.
    pub fn degradation_level(&self) -> DegradationLevel {
        let total = self.providers.len();
        let available = self
            .providers
            .iter()
            .filter(|provider| provider.available)
            .count();

        match available {
            n if n == total => DegradationLevel::Full,
            n if n >= total / 2 => DegradationLevel::Reduced,
            n if n >= 1 => DegradationLevel::Minimal,
            _ => DegradationLevel::Bypass,
        }
    }

    /// Lists provider identifiers currently available for planning.
    pub fn available_providers(&self) -> Vec<&str> {
        self.providers
            .iter()
            .filter(|provider| provider.available)
            .map(|provider| provider.provider_id.as_str())
            .collect()
    }

    /// Lists provider identifiers currently unavailable for planning.
    pub fn unavailable_providers(&self) -> Vec<&str> {
        self.providers
            .iter()
            .filter(|provider| !provider.available)
            .map(|provider| provider.provider_id.as_str())
            .collect()
    }

    /// Summarizes provider availability and the resulting capability level.
    pub fn summary(&self) -> String {
        format!(
            "{}/{} providers available ({:?})",
            self.available_providers().len(),
            self.providers.len(),
            self.degradation_level()
        )
    }

    fn provider_is_available(&self, provider_id: &str) -> bool {
        self.providers
            .iter()
            .any(|status| status.provider_id == provider_id && status.available)
    }
}

/// Removes selections whose providers are unavailable and updates accounting.
pub fn degrade_plan(plan: &ContextPlanV1, health: &KernelHealth) -> ContextPlanV1 {
    let mut selected: Vec<PlanEntry> = Vec::new();
    let mut excluded = plan.excluded.clone();

    for entry in &plan.selected {
        if health.provider_is_available(&entry.provider) {
            selected.push(entry.clone());
        } else {
            excluded.push(ExcludedEntry {
                object_id: entry.object_id.clone(),
                provider: entry.provider.clone(),
                reason: "provider unavailable".to_owned(),
            });
        }
    }

    let used_tokens = selected.iter().map(|entry| entry.tokens).sum();

    ContextPlanV1 {
        plan_id: format!("{}-degraded", plan.plan_id),
        intent: plan.intent.clone(),
        budget: PlanBudget {
            total_tokens: plan.budget.total_tokens,
            used_tokens,
            remaining_tokens: plan.budget.total_tokens.saturating_sub(used_tokens),
        },
        selected,
        excluded,
        deferred: plan.deferred.clone(),
        provider_stats: plan.provider_stats.clone(),
    }
}

/// Creates an empty pass-through plan for bypass mode.
pub fn fallback_plan(intent: &str, budget_tokens: usize) -> ContextPlanV1 {
    ContextPlanV1 {
        plan_id: format!("plan:{intent}-fallback"),
        intent: intent.to_owned(),
        budget: PlanBudget {
            total_tokens: budget_tokens,
            used_tokens: 0,
            remaining_tokens: budget_tokens,
        },
        selected: Vec::new(),
        excluded: Vec::new(),
        deferred: Vec::new(),
        provider_stats: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(provider_id: &str, available: bool) -> ProviderStatus {
        ProviderStatus {
            provider_id: provider_id.to_owned(),
            available,
            last_error: (!available).then(|| "provider failed".to_owned()),
        }
    }

    fn entry(object_id: &str, provider: &str, tokens: usize) -> PlanEntry {
        PlanEntry {
            object_id: object_id.to_owned(),
            provider: provider.to_owned(),
            view: "full".to_owned(),
            tokens,
            phi: 1.0,
            reason: "selected".to_owned(),
        }
    }

    #[test]
    fn full_health_means_no_degradation() {
        let health = KernelHealth::new(vec![status("files", true), status("facts", true)]);

        assert_eq!(health.degradation_level(), DegradationLevel::Full);
        assert_eq!(health.summary(), "2/2 providers available (Full)");
    }

    #[test]
    fn partial_failure_degrades_to_reduced() {
        let health = KernelHealth::new(vec![
            status("files", true),
            status("facts", true),
            status("episodes", true),
            status("search", false),
            status("session", false),
        ]);

        assert_eq!(health.degradation_level(), DegradationLevel::Reduced);
        assert_eq!(health.unavailable_providers(), vec!["search", "session"]);
    }

    #[test]
    fn degrade_plan_removes_unavailable() {
        let health = KernelHealth::new(vec![status("files", true), status("facts", false)]);
        let plan = ContextPlanV1 {
            plan_id: "plan:test".to_owned(),
            intent: "test degradation".to_owned(),
            budget: PlanBudget {
                total_tokens: 100,
                used_tokens: 70,
                remaining_tokens: 30,
            },
            selected: vec![entry("file:1", "files", 40), entry("fact:1", "facts", 30)],
            excluded: Vec::new(),
            deferred: Vec::new(),
            provider_stats: HashMap::new(),
        };

        let degraded = degrade_plan(&plan, &health);

        assert_eq!(degraded.plan_id, "plan:test-degraded");
        assert_eq!(degraded.selected.len(), 1);
        assert_eq!(degraded.selected[0].object_id, "file:1");
        assert_eq!(degraded.budget.used_tokens, 40);
        assert_eq!(degraded.budget.remaining_tokens, 60);
        assert_eq!(degraded.excluded.len(), 1);
        assert_eq!(degraded.excluded[0].object_id, "fact:1");
        assert_eq!(degraded.excluded[0].reason, "provider unavailable");
    }

    #[test]
    fn fallback_plan_is_empty_with_budget() {
        let plan = fallback_plan("bypass kernel", 512);

        assert_eq!(plan.intent, "bypass kernel");
        assert_eq!(plan.budget.total_tokens, 512);
        assert_eq!(plan.budget.used_tokens, 0);
        assert_eq!(plan.budget.remaining_tokens, 512);
        assert!(plan.selected.is_empty());
        assert!(plan.excluded.is_empty());
        assert!(plan.deferred.is_empty());
        assert!(plan.provider_stats.is_empty());
    }
}
