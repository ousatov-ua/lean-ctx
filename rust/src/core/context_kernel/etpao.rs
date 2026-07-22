//! Effective tokens per accepted outcome metrics.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::{ContextReceiptV1, ReceiptOutcome};

/// Aggregated token efficiency metrics for a single scope (project/agent/model).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EtpaoMetrics {
    pub scope: String,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub tokens_reasoning: u64,
    pub tokens_schema: u64,
    pub tokens_retry: u64,
    pub tokens_cache_write: u64,
    pub tokens_handoff: u64,
    pub accepted_outcomes: u64,
    pub first_pass_successes: u64,
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl EtpaoMetrics {
    /// Returns total accounted tokens per accepted outcome.
    pub fn etpao(&self) -> f64 {
        if self.accepted_outcomes == 0 {
            return f64::INFINITY;
        }

        let total_tokens = self
            .tokens_input
            .saturating_add(self.tokens_output)
            .saturating_add(self.tokens_reasoning)
            .saturating_add(self.tokens_schema)
            .saturating_add(self.tokens_retry)
            .saturating_add(self.tokens_cache_write)
            .saturating_add(self.tokens_handoff);
        total_tokens as f64 / self.accepted_outcomes as f64
    }

    /// Returns the fraction of requests accepted on the first pass.
    pub fn first_pass_success_rate(&self) -> f64 {
        ratio(self.first_pass_successes, self.total_requests)
    }

    /// Returns cache hits as a fraction of total requests.
    pub fn cache_hit_rate(&self) -> f64 {
        ratio(
            self.cache_hits,
            self.cache_hits.saturating_add(self.cache_misses),
        )
    }

    /// Adds another scope's counters to this metric set.
    pub fn merge(&mut self, other: &EtpaoMetrics) {
        self.tokens_input = self.tokens_input.saturating_add(other.tokens_input);
        self.tokens_output = self.tokens_output.saturating_add(other.tokens_output);
        self.tokens_reasoning = self.tokens_reasoning.saturating_add(other.tokens_reasoning);
        self.tokens_schema = self.tokens_schema.saturating_add(other.tokens_schema);
        self.tokens_retry = self.tokens_retry.saturating_add(other.tokens_retry);
        self.tokens_cache_write = self
            .tokens_cache_write
            .saturating_add(other.tokens_cache_write);
        self.tokens_handoff = self.tokens_handoff.saturating_add(other.tokens_handoff);
        self.accepted_outcomes = self
            .accepted_outcomes
            .saturating_add(other.accepted_outcomes);
        self.first_pass_successes = self
            .first_pass_successes
            .saturating_add(other.first_pass_successes);
        self.total_requests = self.total_requests.saturating_add(other.total_requests);
        self.cache_hits = self.cache_hits.saturating_add(other.cache_hits);
        self.cache_misses = self.cache_misses.saturating_add(other.cache_misses);
    }
}

/// Tracks ETPAO metrics per scope, updated from context receipt outcomes.
#[derive(Debug, Default)]
pub struct EtpaoTracker {
    scopes: HashMap<String, EtpaoMetrics>,
}

impl EtpaoTracker {
    /// Records one delivered receipt and its evaluated outcome for a scope.
    pub fn record_receipt(
        &mut self,
        scope: &str,
        receipt: &ContextReceiptV1,
        outcome: ReceiptOutcome,
    ) {
        let metrics = self
            .scopes
            .entry(scope.to_owned())
            .or_insert_with(|| EtpaoMetrics {
                scope: scope.to_owned(),
                ..EtpaoMetrics::default()
            });
        metrics.tokens_input = metrics
            .tokens_input
            .saturating_add(receipt.delivered_tokens as u64);
        metrics.total_requests = metrics.total_requests.saturating_add(1);

        metrics.cache_hits = metrics.cache_hits.saturating_add(receipt.cache_hits as u64);
        metrics.cache_misses = metrics
            .cache_misses
            .saturating_add(receipt.cache_misses as u64);

        if outcome == ReceiptOutcome::Accepted {
            metrics.accepted_outcomes = metrics.accepted_outcomes.saturating_add(1);
            // Only count first-pass if no cache misses (proxy for no retries)
            if receipt.cache_misses == 0 {
                metrics.first_pass_successes = metrics.first_pass_successes.saturating_add(1);
            }
        }
    }

    /// Returns metrics for a tracked scope.
    pub fn get(&self, scope: &str) -> Option<&EtpaoMetrics> {
        self.scopes.get(scope)
    }

    /// Combines every tracked scope into a single aggregate.
    pub fn aggregate(&self) -> EtpaoMetrics {
        let mut aggregate = EtpaoMetrics {
            scope: "aggregate".to_owned(),
            ..EtpaoMetrics::default()
        };
        for metrics in self.scopes.values() {
            aggregate.merge(metrics);
        }
        aggregate
    }

    /// Lists tracked scope names in stable lexical order.
    pub fn all_scopes(&self) -> Vec<&str> {
        let mut scopes: Vec<_> = self.scopes.keys().map(String::as_str).collect();
        scopes.sort_unstable();
        scopes
    }

    /// Returns serializable scope metrics in stable lexical order.
    pub fn to_wire(&self) -> Vec<EtpaoMetrics> {
        let mut metrics: Vec<_> = self.scopes.values().cloned().collect();
        metrics.sort_unstable_by_key(|item| item.scope.clone());
        metrics
    }
}

/// Formats a privacy-safe numeric ETPAO summary.
pub fn format_etpao_summary(metrics: &EtpaoMetrics) -> String {
    format!(
        "ETPAO: {:.2}, first-pass rate: {:.2}%, requests: {}",
        metrics.etpao(),
        metrics.first_pass_success_rate() * 100.0,
        metrics.total_requests
    )
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::{EtpaoMetrics, EtpaoTracker, format_etpao_summary};
    use crate::core::context_kernel::types::{ContextReceiptV1, ReceiptOutcome};
    use std::collections::HashMap;

    fn receipt(tokens: usize) -> ContextReceiptV1 {
        ContextReceiptV1 {
            receipt_id: "receipt:test".to_owned(),
            plan_id: "plan:test".to_owned(),
            delivered_tokens: tokens,
            cache_hits: 0,
            cache_misses: 0,
            outcome: ReceiptOutcome::Unknown,
            quality_signals: Vec::new(),
            feedback_attribution: HashMap::new(),
        }
    }

    #[test]
    fn empty_tracker_returns_infinity_etpao() {
        assert!(EtpaoTracker::default().aggregate().etpao().is_infinite());
    }

    #[test]
    fn single_receipt_updates_metrics() {
        let mut tracker = EtpaoTracker::default();
        tracker.record_receipt("agent-1", &receipt(120), ReceiptOutcome::Accepted);

        let metrics = tracker.get("agent-1").expect("scope metrics");
        assert_eq!(metrics.tokens_input, 120);
        assert_eq!(metrics.total_requests, 1);
        assert_eq!(metrics.accepted_outcomes, 1);
        assert_eq!(metrics.first_pass_successes, 1);
        assert_eq!(metrics.etpao(), 120.0);
    }

    #[test]
    fn merge_sums_all_fields() {
        let mut left = EtpaoMetrics {
            tokens_input: 1,
            tokens_output: 2,
            tokens_reasoning: 3,
            tokens_schema: 4,
            tokens_retry: 5,
            tokens_cache_write: 6,
            tokens_handoff: 7,
            accepted_outcomes: 8,
            first_pass_successes: 9,
            total_requests: 10,
            ..EtpaoMetrics::default()
        };
        let right = left.clone();
        left.merge(&right);

        assert_eq!(left.tokens_input, 2);
        assert_eq!(left.tokens_output, 4);
        assert_eq!(left.tokens_reasoning, 6);
        assert_eq!(left.tokens_schema, 8);
        assert_eq!(left.tokens_retry, 10);
        assert_eq!(left.tokens_cache_write, 12);
        assert_eq!(left.tokens_handoff, 14);
        assert_eq!(left.accepted_outcomes, 16);
        assert_eq!(left.first_pass_successes, 18);
        assert_eq!(left.total_requests, 20);
    }

    #[test]
    fn aggregate_combines_scopes() {
        let mut tracker = EtpaoTracker::default();
        tracker.record_receipt("a", &receipt(10), ReceiptOutcome::Accepted);
        tracker.record_receipt("b", &receipt(20), ReceiptOutcome::Rejected);
        tracker.record_receipt("c", &receipt(30), ReceiptOutcome::Partial);

        let aggregate = tracker.aggregate();
        assert_eq!(aggregate.scope, "aggregate");
        assert_eq!(aggregate.tokens_input, 60);
        assert_eq!(aggregate.total_requests, 3);
        assert_eq!(aggregate.accepted_outcomes, 1);
    }

    #[test]
    fn first_pass_rate_calculation() {
        let metrics = EtpaoMetrics {
            first_pass_successes: 3,
            total_requests: 4,
            cache_hits: 2,
            cache_misses: 2,
            ..EtpaoMetrics::default()
        };

        assert_eq!(metrics.first_pass_success_rate(), 0.75);
        assert_eq!(metrics.cache_hit_rate(), 0.5);
    }

    #[test]
    fn format_summary_no_content_leak() {
        let metrics = EtpaoMetrics {
            scope: "/secret/project/private.rs".to_owned(),
            tokens_input: 25,
            accepted_outcomes: 1,
            first_pass_successes: 1,
            total_requests: 1,
            ..EtpaoMetrics::default()
        };
        let summary = format_etpao_summary(&metrics);

        assert!(summary.contains("25.00"));
        assert!(summary.contains("100.00%"));
        assert!(!summary.contains("secret"));
        assert!(!summary.contains("private.rs"));
    }

    #[test]
    fn scope_exports_are_sorted() {
        let mut tracker = EtpaoTracker::default();
        tracker.record_receipt("z", &receipt(1), ReceiptOutcome::Unknown);
        tracker.record_receipt("a", &receipt(1), ReceiptOutcome::Unknown);

        assert_eq!(tracker.all_scopes(), vec!["a", "z"]);
        assert_eq!(tracker.to_wire()[0].scope, "a");
    }
}
