use serde::{Deserialize, Serialize};

use super::types::OclaResult;

/// Unified P5 savings event combining the legacy chain fields with
/// cross-capability attribution and analysis metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnifiedSavingsEventV2 {
    pub tool_name: String,
    pub mode: String,
    pub original_tokens: u64,
    pub compressed_tokens: u64,
    pub saved_tokens: u64,
    pub content_hash: String,
    pub timestamp_epoch_ms: u64,
    pub prev_hash: String,
    pub event_hash: String,
    pub intent: Option<String>,
    pub outcome: Option<String>,
    pub routing_decision: Option<String>,
    pub agent_id: Option<String>,
    pub efficiency_etpao: Option<u64>,
    pub attribution_id: String,
}

/// Unified ledger contract for P5 migration and eventual legacy replacement.
///
/// Migration plan:
/// - Phase 1: introduce this schema alongside the legacy schema (dual-write).
/// - Phase 2: migrate existing events into unified events.
/// - Phase 3: deactivate the legacy schema after migration verification.
pub trait UnifiedLedger: Send + Sync {
    fn record_unified(&self, event: UnifiedSavingsEventV2) -> OclaResult<String>;
    fn verify_chain(&self) -> OclaResult<bool>;
    fn query_by_attribution(&self, id: &str) -> OclaResult<Option<UnifiedSavingsEventV2>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_instantiates_legacy_and_p5_fields() {
        let event = UnifiedSavingsEventV2 {
            tool_name: "context_read".into(),
            mode: "compressed".into(),
            original_tokens: 1_000,
            compressed_tokens: 400,
            saved_tokens: 600,
            content_hash: "blake3:content".into(),
            timestamp_epoch_ms: 1_700_000_000_000,
            prev_hash: "blake3:previous".into(),
            event_hash: "blake3:event".into(),
            intent: Some("summarize".into()),
            outcome: Some("accepted".into()),
            routing_decision: Some("local".into()),
            agent_id: Some("agent-test".into()),
            efficiency_etpao: Some(750),
            attribution_id: "attribution:test".into(),
        };

        assert_eq!(event.saved_tokens, 600);
        assert_eq!(event.attribution_id, "attribution:test");
        assert_eq!(event.intent.as_deref(), Some("summarize"));
    }
}
