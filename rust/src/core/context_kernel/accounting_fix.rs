//! Honest token accounting after all delivery-time additions.

/// Honest token accounting that includes all additions and subtractions.
///
/// Compression measured before kernel enrichment and server decorations can
/// overstate the savings visible to the LLM. This type records both views.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PostDeliveryAccounting {
    /// Tokens in the original raw content before any processing.
    pub original_tokens: usize,
    /// Tokens after compression but before kernel enrichment and decorations.
    pub compressed_tokens: usize,
    /// Tokens added by Context Kernel enrichment.
    pub kernel_overhead_tokens: usize,
    /// Tokens added by server decorations such as hints, headers, and footers.
    pub decoration_tokens: usize,
    /// Final tokens actually sent to the LLM.
    pub delivered_tokens: usize,
    /// True compression ratio: `(original - delivered) / original`.
    pub actual_compression_ratio: f64,
    /// Compression ratio reported before post-gate additions.
    pub reported_compression_ratio: f64,
    /// Reported minus actual compression, with negative values clamped to zero.
    pub phantom_savings_pct: f64,
}

/// Validation of a savings claim against actual delivery data.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SavingsValidation {
    /// Tokens the system claimed it saved.
    pub claimed_saved: usize,
    /// Tokens actually saved, or zero when delivery met or exceeded the input.
    pub actual_saved: usize,
    /// Claimed savings exceeding actual savings.
    pub phantom: usize,
    /// Whether phantom savings are less than five percent of the original.
    pub is_valid: bool,
}

/// Computes token accounting after kernel and server additions are included.
pub fn compute_honest_accounting(
    original: usize,
    compressed: usize,
    kernel_added: usize,
    decorations: usize,
) -> PostDeliveryAccounting {
    let delivered = compressed
        .saturating_add(kernel_added)
        .saturating_add(decorations);
    let (actual_ratio, reported_ratio) = if original == 0 {
        (0.0, 0.0)
    } else {
        let original = original as f64;
        (
            (1.0 - delivered as f64 / original).clamp(-1.0, 1.0),
            1.0 - compressed as f64 / original,
        )
    };

    PostDeliveryAccounting {
        original_tokens: original,
        compressed_tokens: compressed,
        kernel_overhead_tokens: kernel_added,
        decoration_tokens: decorations,
        delivered_tokens: delivered,
        actual_compression_ratio: actual_ratio,
        reported_compression_ratio: reported_ratio,
        phantom_savings_pct: (reported_ratio - actual_ratio).max(0.0),
    }
}

/// Validates claimed savings against the tokens actually sent to the LLM.
pub fn validate_savings(claimed_saved: usize, original: usize, sent: usize) -> SavingsValidation {
    let actual_saved = original.saturating_sub(sent);
    let phantom = claimed_saved.saturating_sub(actual_saved);
    let is_valid = phantom == 0 || (phantom as f64) < (original as f64 * 0.05);

    SavingsValidation {
        claimed_saved,
        actual_saved,
        phantom,
        is_valid,
    }
}

/// Formats a privacy-safe summary containing only token counts and ratios.
pub fn format_honest_summary(accounting: &PostDeliveryAccounting) -> String {
    format!(
        "Original: {} → Compressed: {} → +Kernel: {} → +Decorations: {} → Delivered: {}\n\
         Actual compression: {:.2}% (reported: {:.2}%, phantom: {:.2}%)",
        accounting.original_tokens,
        accounting.compressed_tokens,
        accounting.kernel_overhead_tokens,
        accounting.decoration_tokens,
        accounting.delivered_tokens,
        accounting.actual_compression_ratio * 100.0,
        accounting.reported_compression_ratio * 100.0,
        accounting.phantom_savings_pct * 100.0,
    )
}

/// Returns whether post-compression additions make delivery exceed the input.
pub fn detect_negative_savings(accounting: &PostDeliveryAccounting) -> bool {
    accounting.delivered_tokens > accounting.original_tokens
}

#[cfg(test)]
mod tests {
    use super::{
        compute_honest_accounting, detect_negative_savings, format_honest_summary, validate_savings,
    };

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn honest_accounting_basic() {
        let accounting = compute_honest_accounting(1_000, 300, 50, 20);

        assert_eq!(accounting.delivered_tokens, 370);
        assert_close(accounting.actual_compression_ratio, 0.63);
        assert_close(accounting.reported_compression_ratio, 0.70);
        assert_close(accounting.phantom_savings_pct, 0.07);
    }

    #[test]
    fn phantom_savings_detected() {
        let validation = validate_savings(500, 500, 500);

        assert_eq!(validation.actual_saved, 0);
        assert_eq!(validation.phantom, 500);
        assert!(!validation.is_valid);
    }

    #[test]
    fn negative_savings_when_kernel_dominates() {
        let accounting = compute_honest_accounting(100, 80, 200, 0);

        assert_eq!(accounting.delivered_tokens, 280);
        assert_close(accounting.actual_compression_ratio, -1.0);
        assert!(detect_negative_savings(&accounting));
    }

    #[test]
    fn zero_original_safe() {
        let accounting = compute_honest_accounting(0, 0, 10, 5);

        assert_close(accounting.actual_compression_ratio, 0.0);
        assert_close(accounting.reported_compression_ratio, 0.0);
    }

    #[test]
    fn no_phantom_when_honest() {
        let validation = validate_savings(500, 1_000, 500);

        assert_eq!(validation.phantom, 0);
        assert!(validation.is_valid);
    }

    #[test]
    fn format_summary_no_content() {
        let accounting = compute_honest_accounting(1_000, 300, 50, 20);
        let summary = format_honest_summary(&accounting);

        assert!(summary.contains("Original: 1000 → Compressed: 300"));
        assert!(summary.contains("Actual compression: 63.00%"));
        assert!(!summary.contains('/'));
        assert!(!summary.contains("content"));
    }

    #[test]
    fn kernel_overhead_visible() {
        let accounting = compute_honest_accounting(1_000, 300, 50, 20);
        let summary = format_honest_summary(&accounting);

        assert_eq!(accounting.kernel_overhead_tokens, 50);
        assert!(summary.contains("+Kernel: 50"));
    }

    #[test]
    fn five_percent_phantom_is_invalid() {
        let validation = validate_savings(55, 100, 50);

        assert_eq!(validation.phantom, 5);
        assert!(!validation.is_valid);
    }
}
