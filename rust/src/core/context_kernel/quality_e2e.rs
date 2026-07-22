//! Quality and integration end-to-end conformance tests.

#[cfg(test)]
mod tests {
    use super::super::accounting_fix;
    use super::super::hotpath_wiring;
    use super::super::outcome_signal;
    use super::super::types::ReceiptOutcome;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn full_pipeline_dedup_to_accounting() {
        let integration = hotpath_wiring::kernel_integrate("test query", "/tmp/test", 1_000, 300);

        assert!(integration.accounting.delivered_tokens > 0);
        assert!(integration.accounting.phantom_savings_pct >= 0.0);
    }

    #[test]
    fn outcome_signal_first_pass() {
        let inference = outcome_signal::infer_outcome(1, false, 500);

        assert_eq!(inference.outcome, ReceiptOutcome::Accepted);
        assert_eq!(format!("{:?}", inference.signal), "FirstPass");
        assert!(inference.confidence >= 0.8);
    }

    #[test]
    fn outcome_signal_retry_rejected() {
        let inference = outcome_signal::infer_outcome(3, true, 200);

        assert_eq!(inference.outcome, ReceiptOutcome::Rejected);
        assert_eq!(format!("{:?}", inference.signal), "Retry");
    }

    #[test]
    fn honest_accounting_vs_phantom() {
        let accounting = accounting_fix::compute_honest_accounting(1_000, 300, 100, 50);

        assert_eq!(accounting.delivered_tokens, 450);
        assert_close(accounting.actual_compression_ratio, 0.55);
        assert_close(accounting.reported_compression_ratio, 0.70);
        assert_close(accounting.phantom_savings_pct, 0.15);
    }

    #[test]
    fn outcome_tracker_quality_trend() {
        let mut tracker = outcome_signal::OutcomeTracker::new();
        for _ in 0..3 {
            tracker.record(ReceiptOutcome::Accepted);
        }
        for _ in 0..5 {
            tracker.record(ReceiptOutcome::Rejected);
        }

        assert_close(tracker.acceptance_rate(), 0.375);
        assert!(tracker.is_degrading(5));
    }

    #[test]
    fn integration_budget_never_exceeds_cap() {
        for (original, compressed) in [
            (0, 0),
            (100, 25),
            (1_000, 300),
            (10_000, 9_500),
            (usize::MAX, usize::MAX),
        ] {
            let integration = hotpath_wiring::kernel_integrate(
                "budget cap query",
                "/tmp/test",
                original,
                compressed,
            );

            assert!(integration.enrichment.verdict.budget_used <= 150);
        }
    }
}
