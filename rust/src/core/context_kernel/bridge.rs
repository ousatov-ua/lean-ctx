//! Runtime integration helpers for the Context Control Kernel.

use super::orchestrator::ContextKernel;
use super::types::{ContextPlanV1, ContextReceiptV1, PlanEntry, ReceiptOutcome, RetrievalContext};

/// Result of kernel enrichment for compose integration.
#[derive(Debug, Clone)]
pub struct KernelEnrichment {
    /// The selection plan that produced the injected blocks.
    pub plan: ContextPlanV1,
    /// Human-readable blocks suitable for compose output injection.
    pub blocks: String,
}

/// Enrich a compose response with kernel-selected context.
///
/// Returns additional context blocks from Knowledge, Episodic, and Procedural
/// stores that the standard compose pipeline misses. Returns `None` if no
/// additional context is worth including.
pub fn kernel_enrich(
    task: &str,
    project_root: &str,
    budget_tokens: usize,
) -> Option<KernelEnrichment> {
    let kernel = ContextKernel::for_project(project_root);
    let ctx = RetrievalContext {
        query: task.to_owned(),
        task: Some(task.to_owned()),
        project_root: project_root.to_owned(),
        budget: crate::core::context_field::TokenBudget {
            total: budget_tokens,
            used: 0,
        },
        max_candidates: 20,
    };
    let plan = kernel.plan(&ctx);
    let enrichments: Vec<&PlanEntry> = plan
        .selected
        .iter()
        .filter(|entry| entry.provider != "context.ledger")
        .collect();

    if enrichments.is_empty() {
        return None;
    }
    let blocks = format_enrichment_blocks(&enrichments);
    Some(KernelEnrichment { plan, blocks })
}

fn format_enrichment_blocks(entries: &[&PlanEntry]) -> String {
    let mut out = String::new();
    append_provider_block(&mut out, entries, "knowledge.facts", "Relevant Knowledge");
    append_provider_block(&mut out, entries, "memory.episodic", "Relevant Episodes");
    append_provider_block(
        &mut out,
        entries,
        "memory.procedural",
        "Relevant Procedures",
    );
    append_provider_block(&mut out, entries, "session.state", "Relevant Session State");
    out
}

fn append_provider_block(
    output: &mut String,
    entries: &[&PlanEntry],
    provider: &str,
    heading: &str,
) {
    let mut found = false;
    for entry in entries
        .iter()
        .copied()
        .filter(|entry| entry.provider == provider)
    {
        if !found {
            output.push_str("\n## ");
            output.push_str(heading);
            output.push('\n');
            found = true;
        }
        output.push_str("- ");
        output.push_str(&entry.reason);
        output.push_str(" (phi=");
        output.push_str(&format!("{:.2}", entry.phi));
        output.push_str(")\n");
    }
}

/// Emit a plan-created event on the OclaBus.
pub fn emit_plan_event(plan: &ContextPlanV1) {
    use crate::core::ocla_bus::{self, OclaEvent};

    ocla_bus::emit(OclaEvent::AgentChainEvent {
        agent_id: format!("kernel:{}", plan.plan_id),
        action: format!(
            "plan_created:selected={},excluded={},budget={}/{}",
            plan.selected.len(),
            plan.excluded.len(),
            plan.budget.used_tokens,
            plan.budget.total_tokens,
        ),
        parent_agent: None,
    });
}

/// Emit a receipt-recorded event on the OclaBus.
pub fn emit_receipt_event(receipt: &ContextReceiptV1) {
    use crate::core::ocla_bus::{self, OclaEvent};

    ocla_bus::emit(OclaEvent::AgentChainEvent {
        agent_id: format!("kernel:{}", receipt.receipt_id),
        action: format!(
            "receipt_recorded:tokens={},outcome={:?}",
            receipt.delivered_tokens, receipt.outcome,
        ),
        parent_agent: Some(receipt.plan_id.clone()),
    });
}

/// Update the bandit-learned FieldWeights based on a receipt outcome.
///
/// Accepted outcomes reinforce the balanced arm, rejected outcomes penalize
/// the aggressive arm, and partial outcomes inform the conservative arm.
pub fn apply_feedback(receipt: &ContextReceiptV1) {
    use crate::core::context_field::{FieldWeights, set_active_weights};

    let arm_name = match receipt.outcome {
        ReceiptOutcome::Accepted => "balanced",
        ReceiptOutcome::Rejected => "aggressive",
        ReceiptOutcome::Partial => "conservative",
        ReceiptOutcome::Unknown => return,
    };
    let mut bandit = crate::core::bandit::ThresholdBandit::default();
    bandit.update(arm_name, receipt.outcome == ReceiptOutcome::Accepted);

    let best_idx = bandit.best_arm_idx_by_mean();
    if let Some(best_arm) = bandit.arms.get(best_idx) {
        set_active_weights(FieldWeights::from_arm(best_arm));
    }
}

/// Format a plan as a compact human-readable summary.
pub fn format_plan_summary(plan: &ContextPlanV1) -> String {
    let mut out = String::new();
    let plan_prefix = &plan.plan_id[..plan.plan_id.len().min(8)];
    out.push_str(&format!(
        "[kernel] plan={plan_prefix} intent=\"{}\" budget={}/{}\n",
        plan.intent, plan.budget.used_tokens, plan.budget.total_tokens,
    ));
    out.push_str(&format!(
        "  selected={} excluded={} deferred={}\n",
        plan.selected.len(),
        plan.excluded.len(),
        plan.deferred.len(),
    ));

    let mut providers: Vec<_> = plan.provider_stats.iter().collect();
    providers.sort_unstable_by_key(|(k, _)| *k);
    for (provider, stat) in providers {
        out.push_str(&format!(
            "  {provider}: {}/{} candidates, {} tokens\n",
            stat.candidates_selected, stat.candidates_offered, stat.tokens_used,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::types::{PlanBudget, ProviderStat};
    use super::*;

    fn plan() -> ContextPlanV1 {
        ContextPlanV1 {
            plan_id: "1234567890abcdef".to_owned(),
            intent: "test kernel output".to_owned(),
            budget: PlanBudget {
                total_tokens: 100,
                used_tokens: 25,
                remaining_tokens: 75,
            },
            selected: vec![PlanEntry {
                object_id: "fact-1".to_owned(),
                provider: "knowledge.facts".to_owned(),
                view: "summary".to_owned(),
                tokens: 25,
                phi: 0.8,
                reason: "Known project constraint".to_owned(),
            }],
            excluded: Vec::new(),
            deferred: Vec::new(),
            provider_stats: HashMap::from([(
                "knowledge.facts".to_owned(),
                ProviderStat {
                    candidates_offered: 1,
                    candidates_selected: 1,
                    tokens_used: 25,
                },
            )]),
        }
    }

    fn receipt(outcome: ReceiptOutcome) -> ContextReceiptV1 {
        ContextReceiptV1 {
            receipt_id: "receipt-1".to_owned(),
            plan_id: "plan-1".to_owned(),
            delivered_tokens: 25,
            cache_hits: 0,
            cache_misses: 1,
            outcome,
            quality_signals: Vec::new(),
            feedback_attribution: HashMap::new(),
        }
    }

    #[test]
    fn nonexistent_project_does_not_enrich() {
        assert!(kernel_enrich("test task", "/nonexistent/context-kernel-project", 100).is_none());
    }

    #[test]
    fn empty_entries_produce_no_blocks() {
        assert!(format_enrichment_blocks(&[]).is_empty());
    }

    #[test]
    fn plan_summary_is_readable() {
        let summary = format_plan_summary(&plan());
        assert!(summary.contains("plan=12345678"));
        assert!(summary.contains("selected=1"));
        assert!(summary.contains("knowledge.facts: 1/1 candidates, 25 tokens"));
    }

    #[test]
    fn accepted_feedback_does_not_panic() {
        apply_feedback(&receipt(ReceiptOutcome::Accepted));
    }

    #[test]
    fn unknown_feedback_is_a_no_op() {
        apply_feedback(&receipt(ReceiptOutcome::Unknown));
    }

    #[test]
    fn plan_event_does_not_require_a_enabled_bus() {
        emit_plan_event(&plan());
    }

    #[test]
    fn receipt_event_does_not_require_a_enabled_bus() {
        emit_receipt_event(&receipt(ReceiptOutcome::Accepted));
    }
}
