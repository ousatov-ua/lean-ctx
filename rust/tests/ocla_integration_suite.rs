use std::collections::HashSet;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};

use chrono::Utc;
use ed25519_dalek::SigningKey;
use lean_ctx::core::a2a::dlq::{DeadLetter, DeadLetterQueue};
use lean_ctx::core::capsule_transport::LocalSignedCapsuleTransport;
use lean_ctx::core::context_capsule::{
    CONTEXT_CAPSULE_SCHEMA_VERSION, CapsuleReferenceKindV1, CapsuleSensitivityV1,
    ContextCapsuleBudgetV1, ContextCapsuleChainV1, ContextCapsuleReferenceV1, ContextCapsuleV1,
    SignedContextCapsuleV1,
};
use lean_ctx::core::ocla::budget::{BudgetLedger, BudgetLimit, BudgetScope};
use lean_ctx::core::ocla::health::{HealthStatus, check_system_health};
use lean_ctx::core::ocla::response_cache::{CachedResponse, ResponseCache, ResponseCacheKey};
use lean_ctx::core::ocla::routing_quality::{
    RoutingDecision, RoutingOutcome, RoutingQualityTracker,
};
use lean_ctx::core::ocla::tracing::{SpanStatus, spans_for_trace, start_span};
use lean_ctx::core::{agents::AgentRegistry, savings_ledger};

struct IsolatedDataDir {
    temp: tempfile::TempDir,
    _lock: MutexGuard<'static, ()>,
    previous_data_dir: Option<String>,
    previous_ledger_setting: Option<String>,
}

impl IsolatedDataDir {
    fn new() -> Self {
        let lock = lean_ctx::core::data_dir::test_env_lock();
        let temp = tempfile::tempdir().expect("isolated data directory");
        let previous_data_dir = std::env::var("LEAN_CTX_DATA_DIR").ok();
        let previous_ledger_setting = std::env::var("LEAN_CTX_SAVINGS_LEDGER").ok();
        // SAFETY: test_env_lock serializes all environment mutations in this suite.
        unsafe { std::env::set_var("LEAN_CTX_DATA_DIR", temp.path()) };
        // SAFETY: test_env_lock serializes all environment mutations in this suite.
        unsafe { std::env::set_var("LEAN_CTX_SAVINGS_LEDGER", "on") };
        Self {
            temp,
            _lock: lock,
            previous_data_dir,
            previous_ledger_setting,
        }
    }
}

impl Drop for IsolatedDataDir {
    fn drop(&mut self) {
        // SAFETY: test_env_lock remains held until after Drop restores the env.
        match &self.previous_data_dir {
            Some(value) => unsafe { std::env::set_var("LEAN_CTX_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("LEAN_CTX_DATA_DIR") },
        }
        // SAFETY: test_env_lock remains held until after Drop restores the env.
        match &self.previous_ledger_setting {
            Some(value) => unsafe { std::env::set_var("LEAN_CTX_SAVINGS_LEDGER", value) },
            None => unsafe { std::env::remove_var("LEAN_CTX_SAVINGS_LEDGER") },
        }
    }
}

fn capsule() -> ContextCapsuleV1 {
    let mut capsule = ContextCapsuleV1 {
        schema_version: CONTEXT_CAPSULE_SCHEMA_VERSION,
        capsule_id: "capsule:pending".into(),
        request_id: "request:integration".into(),
        session_id: "session:integration".into(),
        agent_id: "integration-parent".into(),
        intent_ref: "intent:integration".into(),
        task_ref: "task:integration".into(),
        expected_outcome_ref: "outcome:integration".into(),
        acceptance_criteria_refs: vec!["criteria:green".into()],
        references: vec![ContextCapsuleReferenceV1 {
            kind: CapsuleReferenceKindV1::File,
            content_ref: "blake3:integration".into(),
            freshness_ref: "freshness:integration".into(),
            recovery_ref: None,
        }],
        finding_refs: vec![],
        decision_refs: vec![],
        uncertainty_refs: vec![],
        negative_result_refs: vec![],
        source_ref: "source:integration".into(),
        policy_ref: "policy:integration".into(),
        contract_ref: "contract:integration".into(),
        freshness_ref: "freshness:integration".into(),
        sensitivity: CapsuleSensitivityV1::Internal,
        allowed_agent_ids: vec!["integration-child".into()],
        budget: ContextCapsuleBudgetV1 {
            tokens_used: 100,
            tokens_remaining: 900,
            cost_micros_used: 10,
            cost_micros_remaining: 90,
            latency_ms_used: 20,
            latency_ms_remaining: 80,
        },
        chain: ContextCapsuleChainV1 {
            chain_id: "chain:integration".into(),
            parent_capsule_ref: None,
            owner_agent_id: "integration-parent".into(),
            attribution_ref: "attribution:integration".into(),
            hop: 0,
        },
        quality_signal_refs: vec![],
        recovery_refs: vec![],
        delta_from: None,
    };
    capsule.assign_capsule_id().expect("capsule ID");
    capsule.validate().expect("valid capsule");
    capsule
}

fn signed(capsule: &ContextCapsuleV1) -> SignedContextCapsuleV1 {
    SignedContextCapsuleV1::sign(capsule, &SigningKey::from_bytes(&[7; 32]))
        .expect("signed capsule")
}

fn reconciled_event_count(data_dir: &std::path::Path) -> usize {
    let unified_path = data_dir.join("savings/unified_ledger.jsonl");
    let unified_hashes: HashSet<String> = std::fs::read_to_string(unified_path)
        .expect("unified ledger")
        .lines()
        .map(|line| {
            serde_json::from_str::<lean_ctx::core::ocla::UnifiedSavingsEventV2>(line)
                .expect("unified event")
                .event_hash
        })
        .collect();
    savings_ledger::all_events()
        .iter()
        .filter(|event| unified_hashes.contains(&event.entry_hash))
        .count()
}

#[test]
fn test_full_pipeline() {
    let data = IsolatedDataDir::new();
    let parent = capsule();
    let transport = LocalSignedCapsuleTransport::default();
    let delivery = transport
        .deliver(&signed(&parent), "integration-child")
        .expect("capsule registration");
    assert_eq!(delivery.capsule_ref, parent.capsule_id);

    let scope = BudgetScope::Org("integration-test".into());
    let mut budget = BudgetLedger::new();
    budget.set_limit(BudgetLimit {
        scope: scope.clone(),
        max_tokens_per_day: 1_000,
        max_usd_per_day: 10.0,
    });
    budget.check_budget(&scope, 100).expect("budget admission");
    budget.record_consumption(&scope, 40, 0.40);

    let mut routing = RoutingQualityTracker::new();
    routing.record(RoutingOutcome {
        decision: RoutingDecision {
            original_model: "expensive-model".into(),
            routed_model: "integration-model".into(),
            reason: "integration route".into(),
            timestamp: Utc::now().to_rfc3339(),
        },
        quality_score: Some(0.95),
        tokens_saved: 60,
        latency_delta_ms: -10,
    });
    assert!(!routing.should_fallback());

    let cache = ResponseCache::new(4, Duration::from_secs(30));
    let key = ResponseCacheKey::new("integration-model", 42, 0.0, 128);
    cache.put(
        key,
        CachedResponse {
            body: b"cached response".to_vec(),
            tokens: 40,
            created_at: Instant::now(),
            ttl: Duration::ZERO,
        },
    );
    assert_eq!(cache.get(&key).expect("cache hit").tokens, 40);

    savings_ledger::record_tool_event("ocla_integration", 100, 40);
    assert_eq!(savings_ledger::all_events().len(), 1);
    assert_eq!(reconciled_event_count(data.temp.path()), 1);
    assert!(savings_ledger::verify().valid);

    AgentRegistry::mutate_locked(|registry| registry.register("integration", Some("test"), "."))
        .expect("agent registration");
    assert_eq!(check_system_health().overall, HealthStatus::Healthy);
}

#[test]
fn test_budget_blocks_over_limit() {
    let scope = BudgetScope::Org("integration-budget-block".into());
    let mut budget = BudgetLedger::new();
    budget.set_limit(BudgetLimit {
        scope: scope.clone(),
        max_tokens_per_day: 10,
        max_usd_per_day: 1.0,
    });
    budget.record_consumption(&scope, 10, 0.10);
    assert!(budget.check_budget(&scope, 1).is_err());
}

#[test]
fn test_capsule_fork_and_resolve() {
    let parent = capsule();
    let transport = LocalSignedCapsuleTransport::default();
    transport
        .deliver(&signed(&parent), "integration-child")
        .expect("parent registration");

    let mut child = parent.clone();
    child.task_ref = "task:integration-child".into();
    child.budget.tokens_used += 50;
    child.budget.tokens_remaining -= 50;
    child.chain.parent_capsule_ref = Some(parent.capsule_id.clone());
    child.chain.hop = 1;
    child.assign_capsule_id().expect("child capsule ID");
    child.validate().expect("valid fork");
    transport
        .deliver(&signed(&child), "integration-child")
        .expect("child registration");

    let (_, parent_delivery) = transport.receive("integration-child").expect("parent");
    let (received_child, child_delivery) = transport.receive("integration-child").expect("child");
    assert_eq!(parent_delivery.capsule_ref, parent.capsule_id);
    assert_eq!(child_delivery.capsule_ref, child.capsule_id);
    assert_eq!(
        received_child.chain.parent_capsule_ref.as_deref(),
        Some(parent.capsule_id.as_str())
    );
    assert_eq!(received_child.budget.tokens_remaining, 850);
    assert_eq!(parent.task_ref, "task:integration");
}

#[test]
fn test_dlq_lifecycle() {
    let queue = DeadLetterQueue::new();
    queue.enqueue(DeadLetter {
        id: "integration-dead-letter".into(),
        original_message: "integration message".into(),
        target_agent: "integration-child".into(),
        error: "delivery failed".into(),
        attempts: 1,
        first_failed_at: Utc::now().to_rfc3339(),
        last_failed_at: Utc::now().to_rfc3339(),
    });
    assert_eq!(queue.peek_all().len(), 1);
    assert!(queue.dequeue("integration-dead-letter").is_some());
    assert!(queue.peek_all().is_empty());
}

#[test]
fn test_tracing_spans() {
    let trace_id = format!(
        "integration-trace-{}",
        Utc::now().timestamp_nanos_opt().unwrap()
    );
    let parent = start_span(&trace_id, "integration.parent");
    let child = start_span(&trace_id, "integration.child");
    child.set_status(SpanStatus::Ok);
    drop(child);
    drop(parent);

    let spans = spans_for_trace(&trace_id);
    assert_eq!(spans.len(), 2);
    assert!(spans.iter().all(|span| {
        span.end_ns.expect("span end") >= span.start_ns && span.trace_id == trace_id
    }));
    let child = spans
        .iter()
        .find(|span| span.operation == "integration.child")
        .expect("child span");
    let parent = spans
        .iter()
        .find(|span| span.operation == "integration.parent")
        .expect("parent span");
    assert_eq!(
        child.parent_span_id.as_deref(),
        Some(parent.span_id.as_str())
    );
}
