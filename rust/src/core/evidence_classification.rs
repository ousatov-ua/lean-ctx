//! Deterministic classification of savings evidence.

use crate::core::savings_ledger::event::{
    EvidenceClass, MeasurementMethod, MECHANISM_CACHING, MECHANISM_COMPRESSION,
    MECHANISM_ROUTING,
};

/// Classify the measurement method and evidence strength for a savings event.
pub fn classify(
    mechanism: &str,
    has_tokenizer_count: bool,
    is_holdout: bool,
) -> (MeasurementMethod, EvidenceClass) {
    if is_holdout {
        (MeasurementMethod::Holdout, EvidenceClass::Statistical)
    } else {
        match mechanism {
            MECHANISM_COMPRESSION if has_tokenizer_count => {
                (MeasurementMethod::DirectCount, EvidenceClass::Measured)
            }
            MECHANISM_COMPRESSION => {
                (MeasurementMethod::DirectCount, EvidenceClass::Approximated)
            }
            MECHANISM_ROUTING => (
                MeasurementMethod::BaselineEstimate,
                EvidenceClass::Approximated,
            ),
            MECHANISM_CACHING => (
                MeasurementMethod::ProviderReconciled,
                EvidenceClass::Measured,
            ),
            _ => (MeasurementMethod::Unknown, EvidenceClass::Unclassified),
        }
    }
}

/// Return the default confidence associated with an evidence class.
pub fn confidence_for_class(class: &EvidenceClass) -> f64 {
    match class {
        EvidenceClass::Measured => 1.0,
        EvidenceClass::Approximated => 0.85,
        EvidenceClass::Statistical => 0.7,
        EvidenceClass::Declared => 0.3,
        EvidenceClass::Unclassified => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_with_tokenizer_count_is_measured() {
        assert_eq!(
            classify(MECHANISM_COMPRESSION, true, false),
            (MeasurementMethod::DirectCount, EvidenceClass::Measured)
        );
    }

    #[test]
    fn compression_without_tokenizer_count_is_approximated() {
        assert_eq!(
            classify(MECHANISM_COMPRESSION, false, false),
            (MeasurementMethod::DirectCount, EvidenceClass::Approximated)
        );
    }

    #[test]
    fn routing_is_baseline_estimate_and_approximated() {
        assert_eq!(
            classify(MECHANISM_ROUTING, true, false),
            (
                MeasurementMethod::BaselineEstimate,
                EvidenceClass::Approximated
            )
        );
    }

    #[test]
    fn caching_is_provider_reconciled_and_measured() {
        assert_eq!(
            classify(MECHANISM_CACHING, false, false),
            (
                MeasurementMethod::ProviderReconciled,
                EvidenceClass::Measured
            )
        );
    }

    #[test]
    fn holdout_takes_precedence_and_is_statistical() {
        assert_eq!(
            classify(MECHANISM_COMPRESSION, true, true),
            (MeasurementMethod::Holdout, EvidenceClass::Statistical)
        );
    }

    #[test]
    fn unknown_mechanism_is_unclassified() {
        assert_eq!(
            classify("other", true, false),
            (MeasurementMethod::Unknown, EvidenceClass::Unclassified)
        );
    }

    #[test]
    fn confidence_covers_every_evidence_class() {
        assert_eq!(confidence_for_class(&EvidenceClass::Measured), 1.0);
        assert_eq!(confidence_for_class(&EvidenceClass::Approximated), 0.85);
        assert_eq!(confidence_for_class(&EvidenceClass::Statistical), 0.7);
        assert_eq!(confidence_for_class(&EvidenceClass::Declared), 0.3);
        assert_eq!(confidence_for_class(&EvidenceClass::Unclassified), 0.0);
    }
}
