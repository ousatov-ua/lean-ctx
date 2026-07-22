#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::capsule_wire::{self, ContextCapsuleV1};
    use super::super::etpao::{EtpaoMetrics, EtpaoTracker};
    use super::super::knowledge_health;
    use super::super::result_fusion::{self, ChildResult, FusionStrategy};
    use super::super::types::{ContextReceiptV1, ReceiptOutcome};

    fn receipt(id: &str, delivered_tokens: usize, cache_hits: usize) -> ContextReceiptV1 {
        ContextReceiptV1 {
            receipt_id: format!("receipt:{id}"),
            plan_id: format!("plan:{id}"),
            delivered_tokens,
            cache_hits,
            cache_misses: usize::from(cache_hits == 0),
            outcome: ReceiptOutcome::Accepted,
            quality_signals: Vec::new(),
            feedback_attribution: HashMap::new(),
        }
    }

    fn capsule(owner: &str, refs: Vec<String>, budget: u64) -> ContextCapsuleV1 {
        ContextCapsuleV1::new("multi-agent-e2e", refs, budget, owner)
    }

    fn child_result(
        agent_id: &str,
        evidence: &[&str],
        confidence: f64,
        quality_score: f64,
        contradicts: &[&str],
    ) -> ChildResult {
        ChildResult {
            agent_id: agent_id.to_owned(),
            receipt_ref: format!("receipt:{agent_id}"),
            evidence: evidence.iter().map(|item| (*item).to_owned()).collect(),
            confidence,
            quality_score,
            contradicts: contradicts
                .iter()
                .map(|agent| (*agent).to_owned())
                .collect(),
        }
    }

    #[test]
    fn three_deep_chain_budget_cascade() {
        let mut tracker = EtpaoTracker::default();
        let mut parent = capsule("parent", vec!["ref:root".to_owned()], 10_000);

        let mut child = capsule("child", vec!["ref:child".to_owned()], 5_000);
        child.parent_capsule = Some(parent.capsule_id.clone());
        parent.budget_remaining = parent.budget_remaining.saturating_sub(child.budget_tokens);
        tracker.record_receipt(
            "parent",
            &receipt("parent-to-child", 5_000, 0),
            ReceiptOutcome::Accepted,
        );

        let mut grandchild = capsule("grandchild", vec!["ref:grandchild".to_owned()], 2_500);
        grandchild.parent_capsule = Some(child.capsule_id.clone());
        child.budget_remaining = child
            .budget_remaining
            .saturating_sub(grandchild.budget_tokens);
        tracker.record_receipt(
            "child",
            &receipt("child-to-grandchild", 2_500, 0),
            ReceiptOutcome::Accepted,
        );

        assert_eq!(parent.budget_remaining, 5_000);
        assert_eq!(child.budget_remaining, 2_500);
        assert_eq!(grandchild.budget_remaining, 2_500);
        assert!(grandchild.budget_remaining <= child.budget_remaining);
        assert!(child.budget_remaining <= parent.budget_remaining);
        assert_eq!(
            tracker.get("parent").map(|metrics| metrics.tokens_input),
            Some(5_000)
        );
        assert_eq!(
            tracker.get("child").map(|metrics| metrics.tokens_input),
            Some(2_500)
        );
    }

    #[test]
    fn sibling_context_dedup() {
        let shared = ["ref:A", "ref:B"];
        let siblings: Vec<_> = ["ref:one", "ref:two", "ref:three"]
            .into_iter()
            .enumerate()
            .map(|(index, unique)| {
                capsule(
                    &format!("sibling-{index}"),
                    shared
                        .into_iter()
                        .chain([unique])
                        .map(str::to_owned)
                        .collect(),
                    1_000,
                )
            })
            .collect();

        let report = capsule_wire::dedup_siblings(&siblings);

        assert_eq!(report.shared_refs, vec!["ref:A", "ref:B"]);
        assert!(report.dedup_ratio > 0.0);
        assert!(report.unique_per_capsule.iter().all(|refs| refs.len() == 1));
    }

    #[test]
    fn delta_transfer_smaller_than_full() {
        let original_refs: Vec<_> = (0..10)
            .map(|index| format!("ref:original-context-object-{index:02}-with-stable-metadata"))
            .collect();
        let base = capsule("parent", original_refs.clone(), 5_000);
        let mut updated_refs = original_refs[..8].to_vec();
        updated_refs.extend([
            "ref:new-context-object-10-with-stable-metadata".to_owned(),
            "ref:new-context-object-11-with-stable-metadata".to_owned(),
        ]);
        let mut updated = capsule("parent", updated_refs, 5_000);
        updated.created_at_epoch = base.created_at_epoch;

        let delta = capsule_wire::compute_delta(&base, &updated);
        let delta_size = serde_json::to_vec(&delta).expect("delta serializes").len();
        let full_size = serde_json::to_vec(&updated)
            .expect("capsule serializes")
            .len();
        let applied = capsule_wire::apply_delta(&base, &delta);

        assert!(delta_size * 2 < full_size);
        assert_eq!(applied, updated);
    }

    #[test]
    fn result_fusion_resolves_contradiction() {
        let results = [
            child_result("agent-a", &["finding:accepted"], 0.95, 0.95, &["agent-b"]),
            child_result("agent-b", &["finding:rejected"], 0.80, 0.90, &["agent-a"]),
            child_result("agent-c", &["finding:accepted"], 0.85, 0.90, &[]),
        ];

        let report = result_fusion::fuse_results(&results, FusionStrategy::WeightedMerge);

        assert!(report.conflicts.iter().any(|conflict| {
            (conflict.agent_a == "agent-a" && conflict.agent_b == "agent-b")
                || (conflict.agent_a == "agent-b" && conflict.agent_b == "agent-a")
        }));
        assert_eq!(report.winning_agent, "agent-a");
        assert!(
            report
                .merged_evidence
                .contains(&"finding:accepted".to_owned())
        );
    }

    #[test]
    fn etpao_improves_with_context_reuse() {
        let mut tracker = EtpaoTracker::default();
        for index in 0..5 {
            tracker.record_receipt(
                "chain",
                &receipt(&format!("uncached-{index}"), 1_000, 0),
                ReceiptOutcome::Accepted,
            );
        }
        let first: EtpaoMetrics = tracker.aggregate();
        let first_etpao = first.etpao();

        for index in 0..5 {
            tracker.record_receipt(
                "chain",
                &receipt(&format!("cached-{index}"), 100, 1),
                ReceiptOutcome::Accepted,
            );
        }
        let second_etpao = tracker.aggregate().etpao();

        assert!(second_etpao < first_etpao);
    }

    #[test]
    fn knowledge_health_detects_stale_facts() {
        let mut facts = vec![(true, false); 3];
        facts.extend([(false, false); 5]);
        facts.extend([(false, true); 2]);

        let report = knowledge_health::assess_health(&facts, 0, 0, 0, 0);

        assert!((report.freshness_score - 0.3).abs() < f64::EPSILON);
        assert!((report.stale_ratio - 0.5).abs() < f64::EPSILON);
        assert!((report.contradiction_rate - 0.2).abs() < f64::EPSILON);
    }
}
