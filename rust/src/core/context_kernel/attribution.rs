//! Receipt-based savings attribution for Context Kernel providers.

use std::collections::HashMap;

use super::types::{ContextPlanV1, ContextReceiptV1, PlanEntry, ReceiptOutcome};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttributionEntry {
    pub provider: String,
    pub tokens_contributed: usize,
    pub tokens_saved: usize,
    pub efficiency: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttributionReport {
    pub plan_id: String,
    pub receipt_id: String,
    pub total_tokens_delivered: usize,
    pub total_tokens_saved: usize,
    pub entries: Vec<AttributionEntry>,
}

fn entry_savings(entry: &PlanEntry, compression_ratio: f64) -> usize {
    let delivered_share = (entry.tokens as f64 * compression_ratio) as usize;
    entry.tokens.saturating_sub(delivered_share)
}

pub fn compute_attribution(plan: &ContextPlanV1, receipt: &ContextReceiptV1) -> AttributionReport {
    let compression_ratio = if plan.budget.total_tokens == 0 {
        0.0
    } else {
        receipt.delivered_tokens as f64 / plan.budget.total_tokens as f64
    };
    let _outcome: ReceiptOutcome = receipt.outcome;
    let mut provider_totals: HashMap<String, (usize, usize)> = HashMap::new();

    for entry in &plan.selected {
        let totals = provider_totals.entry(entry.provider.clone()).or_default();
        totals.0 = totals.0.saturating_add(entry.tokens);
        totals.1 = totals
            .1
            .saturating_add(entry_savings(entry, compression_ratio));
    }

    let mut entries: Vec<AttributionEntry> = provider_totals
        .into_iter()
        .map(
            |(provider, (tokens_contributed, tokens_saved))| AttributionEntry {
                provider,
                tokens_contributed,
                tokens_saved,
                efficiency: tokens_saved as f64 / tokens_contributed.max(1) as f64,
            },
        )
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.tokens_saved));

    let total_tokens_saved = entries.iter().map(|entry| entry.tokens_saved).sum();

    AttributionReport {
        plan_id: plan.plan_id.clone(),
        receipt_id: receipt.receipt_id.clone(),
        total_tokens_delivered: receipt.delivered_tokens,
        total_tokens_saved,
        entries,
    }
}

pub fn format_attribution_summary(report: &AttributionReport) -> String {
    let top_provider = report
        .entries
        .first()
        .map_or_else(|| "none", |entry| entry.provider.as_str());

    format!(
        "Attribution: {} providers, {} tokens saved, top: {}",
        report.entries.len(),
        report.total_tokens_saved,
        top_provider
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{compute_attribution, format_attribution_summary};
    use crate::core::context_kernel::types::{
        ContextPlanV1, ContextReceiptV1, PlanBudget, PlanEntry, ReceiptOutcome,
    };

    fn sample_plan() -> ContextPlanV1 {
        ContextPlanV1 {
            plan_id: "plan:test".to_owned(),
            intent: "test attribution".to_owned(),
            budget: PlanBudget {
                total_tokens: 1_000,
                used_tokens: 800,
                remaining_tokens: 200,
            },
            selected: vec![
                PlanEntry {
                    object_id: "file:a".to_owned(),
                    provider: "files".to_owned(),
                    view: "full".to_owned(),
                    tokens: 600,
                    phi: 1.0,
                    reason: "relevant".to_owned(),
                },
                PlanEntry {
                    object_id: "fact:b".to_owned(),
                    provider: "knowledge".to_owned(),
                    view: "summary".to_owned(),
                    tokens: 200,
                    phi: 0.8,
                    reason: "supporting".to_owned(),
                },
            ],
            excluded: Vec::new(),
            deferred: Vec::new(),
            provider_stats: HashMap::new(),
        }
    }

    fn sample_receipt() -> ContextReceiptV1 {
        ContextReceiptV1 {
            receipt_id: "receipt:test".to_owned(),
            plan_id: "plan:test".to_owned(),
            delivered_tokens: 500,
            cache_hits: 0,
            cache_misses: 0,
            outcome: ReceiptOutcome::Accepted,
            quality_signals: Vec::new(),
            feedback_attribution: HashMap::new(),
        }
    }

    #[test]
    fn attribution_computes_savings() {
        let report = compute_attribution(&sample_plan(), &sample_receipt());

        assert_eq!(report.total_tokens_delivered, 500);
        assert_eq!(report.total_tokens_saved, 400);
        assert_eq!(report.entries[0].tokens_contributed, 600);
        assert!((report.entries[0].efficiency - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn attribution_sorted_by_savings() {
        let report = compute_attribution(&sample_plan(), &sample_receipt());

        assert_eq!(report.entries[0].provider, "files");
        assert_eq!(report.entries[0].tokens_saved, 300);
        assert_eq!(report.entries[1].provider, "knowledge");
        assert_eq!(report.entries[1].tokens_saved, 100);
    }

    #[test]
    fn format_summary_contains_top_provider() {
        let report = compute_attribution(&sample_plan(), &sample_receipt());
        let summary = format_attribution_summary(&report);

        assert_eq!(
            summary,
            "Attribution: 2 providers, 400 tokens saved, top: files"
        );
    }
}
