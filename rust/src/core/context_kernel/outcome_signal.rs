//! Outcome quality signals inferred from observable LLM behavior.

use std::time::{SystemTime, UNIX_EPOCH};

use super::activation::{connect_feedback, record_real_outcome};
use super::types::{ContextReceiptV1, ReceiptOutcome};

/// The signal that determined the outcome classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutcomeSignal {
    /// LLM used the context on first try.
    FirstPass,
    /// LLM retried after receiving this context (indicates rejection).
    Retry,
    /// LLM produced zero response tokens (likely ignored the context).
    Ignored,
    /// Ambiguous — not enough signal to determine.
    Ambiguous,
}

/// An outcome inferred from LLM behavior heuristics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InferredOutcome {
    /// Accept/reject classification inferred from the observed behavior.
    pub outcome: ReceiptOutcome,
    /// Confidence in the inferred classification, from zero to one.
    pub confidence: f64,
    /// Behavioral signal used to determine the classification.
    pub signal: OutcomeSignal,
}

/// Infers a context outcome from request and response behavior.
pub fn infer_outcome(
    request_count: usize,
    was_retry: bool,
    response_tokens: usize,
) -> InferredOutcome {
    if was_retry && request_count > 1 {
        InferredOutcome {
            outcome: ReceiptOutcome::Rejected,
            confidence: 0.8,
            signal: OutcomeSignal::Retry,
        }
    } else if response_tokens == 0 {
        InferredOutcome {
            outcome: ReceiptOutcome::Rejected,
            confidence: 0.6,
            signal: OutcomeSignal::Ignored,
        }
    } else if request_count == 1 {
        InferredOutcome {
            outcome: ReceiptOutcome::Accepted,
            confidence: 0.9,
            signal: OutcomeSignal::FirstPass,
        }
    } else {
        InferredOutcome {
            outcome: ReceiptOutcome::Accepted,
            confidence: 0.5,
            signal: OutcomeSignal::Ambiguous,
        }
    }
}

/// Records an inferred outcome and feeds it into provider learning.
///
/// Feedback failures are contained so outcome tracking cannot disrupt delivery.
pub fn record_and_learn(outcome: &InferredOutcome, receipt: &ContextReceiptV1, project_root: &str) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let recorded = record_real_outcome(receipt, outcome.outcome == ReceiptOutcome::Accepted);
        connect_feedback(&recorded, project_root);
    }));
}

/// Tracks recent inferred outcomes for aggregate quality monitoring.
#[derive(Debug, Clone, Default)]
pub struct OutcomeTracker {
    outcomes: Vec<(OutcomeSignal, f64)>,
}

impl OutcomeTracker {
    /// Appends an inferred outcome with its observation timestamp.
    pub fn record(&mut self, outcome: &InferredOutcome) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |duration| duration.as_secs_f64());
        self.outcomes.push((outcome.signal, timestamp));
    }

    /// Returns the fraction of tracked outcomes classified as accepted.
    pub fn acceptance_rate(&self) -> f64 {
        if self.outcomes.is_empty() {
            return 0.0;
        }

        let accepted = self
            .outcomes
            .iter()
            .filter(|(signal, _)| is_accepted(*signal))
            .count();
        accepted as f64 / self.outcomes.len() as f64
    }

    /// Returns whether a complete recent window has below 50% acceptance.
    pub fn is_degrading(&self, window: usize) -> bool {
        if window == 0 || self.outcomes.len() < window {
            return false;
        }

        let accepted = self
            .outcomes
            .iter()
            .rev()
            .take(window)
            .filter(|(signal, _)| is_accepted(*signal))
            .count();
        accepted * 2 < window
    }

    /// Returns the number of tracked outcomes.
    pub fn len(&self) -> usize {
        self.outcomes.len()
    }

    /// Returns whether the tracker contains no outcomes.
    pub fn is_empty(&self) -> bool {
        self.outcomes.is_empty()
    }
}

fn is_accepted(signal: OutcomeSignal) -> bool {
    matches!(signal, OutcomeSignal::FirstPass | OutcomeSignal::Ambiguous)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{InferredOutcome, OutcomeSignal, OutcomeTracker, infer_outcome, record_and_learn};
    use crate::core::context_kernel::types::{ContextReceiptV1, ReceiptOutcome};

    fn receipt() -> ContextReceiptV1 {
        ContextReceiptV1 {
            receipt_id: "receipt-1".to_owned(),
            plan_id: "plan-1".to_owned(),
            delivered_tokens: 10,
            cache_hits: 0,
            cache_misses: 0,
            outcome: ReceiptOutcome::Unknown,
            quality_signals: Vec::new(),
            feedback_attribution: HashMap::new(),
        }
    }

    fn tracked(outcome: ReceiptOutcome, signal: OutcomeSignal) -> InferredOutcome {
        InferredOutcome {
            outcome,
            confidence: 1.0,
            signal,
        }
    }

    #[test]
    fn first_pass_accepted() {
        let inferred = infer_outcome(1, false, 20);
        assert_eq!(inferred.outcome, ReceiptOutcome::Accepted);
        assert_eq!(inferred.confidence, 0.9);
        assert_eq!(inferred.signal, OutcomeSignal::FirstPass);
    }

    #[test]
    fn retry_rejected() {
        let inferred = infer_outcome(2, true, 20);
        assert_eq!(inferred.outcome, ReceiptOutcome::Rejected);
        assert_eq!(inferred.confidence, 0.8);
        assert_eq!(inferred.signal, OutcomeSignal::Retry);
    }

    #[test]
    fn zero_tokens_ignored() {
        let inferred = infer_outcome(1, false, 0);
        assert_eq!(inferred.outcome, ReceiptOutcome::Rejected);
        assert_eq!(inferred.confidence, 0.6);
        assert_eq!(inferred.signal, OutcomeSignal::Ignored);
    }

    #[test]
    fn ambiguous_default_accepted() {
        let inferred = infer_outcome(2, false, 20);
        assert_eq!(inferred.outcome, ReceiptOutcome::Accepted);
        assert_eq!(inferred.confidence, 0.5);
        assert_eq!(inferred.signal, OutcomeSignal::Ambiguous);
    }

    #[test]
    fn tracker_acceptance_rate() {
        let mut tracker = OutcomeTracker::default();
        for _ in 0..7 {
            tracker.record(&tracked(ReceiptOutcome::Accepted, OutcomeSignal::FirstPass));
        }
        for _ in 0..3 {
            tracker.record(&tracked(ReceiptOutcome::Rejected, OutcomeSignal::Retry));
        }

        assert!((tracker.acceptance_rate() - 0.7).abs() < f64::EPSILON);
        assert_eq!(tracker.len(), 10);
    }

    #[test]
    fn tracker_detects_degradation() {
        let mut tracker = OutcomeTracker::default();
        for _ in 0..5 {
            tracker.record(&tracked(ReceiptOutcome::Rejected, OutcomeSignal::Retry));
        }

        assert!(tracker.is_degrading(5));
    }

    #[test]
    fn record_and_learn_no_panic() {
        let inferred = infer_outcome(2, true, 20);
        record_and_learn(&inferred, &receipt(), "/path/that/does/not/exist");
    }
}
