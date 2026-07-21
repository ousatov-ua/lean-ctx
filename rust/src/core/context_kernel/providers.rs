//! Built-in candidate providers for the context control kernel.

use std::collections::HashMap;

use crate::core::context_field::{ContextItemId, ContextState, Provenance, ViewCosts};
use crate::core::context_ledger::{ContextLedger, LedgerEntry};
use crate::core::episodic_memory::{EpisodicStore, Outcome};
use crate::core::knowledge::ProjectKnowledge;
use crate::core::procedural_memory::{ProceduralStore, Procedure};
use crate::core::session::SessionState;
use crate::core::tokens::count_tokens;

use super::types::{
    CandidateProvider, ContextObjectKind, ContextObjectV1, Freshness, RetrievalContext,
    SensitivityLevel, SideEffectPolicy,
};

const KNOWLEDGE_PROVIDER: &str = "knowledge.facts";
const SESSION_PROVIDER: &str = "session.state";
const EPISODIC_PROVIDER: &str = "memory.episodic";
const PROCEDURAL_PROVIDER: &str = "memory.procedural";
const LEDGER_PROVIDER: &str = "context.ledger";

/// Supplies persisted project knowledge facts as context candidates.
pub struct KnowledgeProvider {
    project_root: String,
}

impl KnowledgeProvider {
    /// Creates a provider scoped to `project_root`.
    pub fn new(project_root: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl CandidateProvider for KnowledgeProvider {
    fn provider_id(&self) -> &str {
        KNOWLEDGE_PROVIDER
    }

    fn candidates(&self, ctx: &RetrievalContext) -> Vec<ContextObjectV1> {
        let Some(mut knowledge) = ProjectKnowledge::load(&self.project_root) else {
            return Vec::new();
        };
        let (facts, _) = knowledge.recall_for_output(&ctx.query, ctx.max_candidates);

        facts
            .into_iter()
            .map(|fact| {
                let mut metadata = HashMap::new();
                metadata.insert("category".to_string(), fact.category.clone());
                metadata.insert("key".to_string(), fact.key.clone());
                metadata.insert("source_session".to_string(), fact.source_session.clone());
                context_object(
                    ContextItemId::from_knowledge(&fact.category, &fact.key),
                    ContextObjectKind::Fact,
                    KNOWLEDGE_PROVIDER,
                    format!("knowledge:{}:{}", fact.category, fact.key),
                    format!("{}: {}", fact.category, fact.key),
                    Some(fact.value.clone()),
                    freshness(fact.created_at.to_rfc3339(), false),
                    fact.confidence,
                    count_tokens(&fact.value),
                    ViewCosts::from_full_tokens(count_tokens(&fact.value)),
                    Provenance::default(),
                    metadata,
                )
            })
            .collect()
    }

    fn side_effect_policy(&self) -> SideEffectPolicy {
        SideEffectPolicy::MutatesStats
    }
}

/// Supplies the latest session's findings, decisions, and modified files.
pub struct SessionProvider {
    project_root: String,
}

impl SessionProvider {
    /// Creates a provider scoped to `project_root`.
    pub fn new(project_root: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl CandidateProvider for SessionProvider {
    fn provider_id(&self) -> &str {
        SESSION_PROVIDER
    }

    fn candidates(&self, ctx: &RetrievalContext) -> Vec<ContextObjectV1> {
        let Some(session) = SessionState::load_latest_for_project_root(&self.project_root) else {
            return Vec::new();
        };
        let mut candidates = Vec::new();

        for finding in &session.findings {
            let mut metadata = HashMap::new();
            if let Some(file) = &finding.file {
                metadata.insert("file".to_string(), file.clone());
            }
            if let Some(line) = finding.line {
                metadata.insert("line".to_string(), line.to_string());
            }
            let id = ContextItemId::from_provider(
                SESSION_PROVIDER,
                &format!(
                    "finding:{}",
                    finding.timestamp.timestamp_nanos_opt().unwrap_or_default()
                ),
            );
            candidates.push((
                finding.timestamp,
                context_object(
                    id,
                    ContextObjectKind::SessionItem,
                    SESSION_PROVIDER,
                    format!("session:{}:finding", session.id),
                    "Session finding".to_string(),
                    Some(finding.summary.clone()),
                    freshness(finding.timestamp.to_rfc3339(), false),
                    1.0,
                    count_tokens(&finding.summary),
                    ViewCosts::from_full_tokens(count_tokens(&finding.summary)),
                    Provenance::default(),
                    metadata,
                ),
            ));
        }

        for decision in &session.decisions {
            let mut metadata = HashMap::new();
            if let Some(rationale) = &decision.rationale {
                metadata.insert("rationale".to_string(), rationale.clone());
            }
            let id = ContextItemId::from_provider(
                SESSION_PROVIDER,
                &format!(
                    "decision:{}",
                    decision.timestamp.timestamp_nanos_opt().unwrap_or_default()
                ),
            );
            candidates.push((
                decision.timestamp,
                context_object(
                    id,
                    ContextObjectKind::SessionItem,
                    SESSION_PROVIDER,
                    format!("session:{}:decision", session.id),
                    "Session decision".to_string(),
                    Some(decision.summary.clone()),
                    freshness(decision.timestamp.to_rfc3339(), false),
                    1.0,
                    count_tokens(&decision.summary),
                    ViewCosts::from_full_tokens(count_tokens(&decision.summary)),
                    Provenance::default(),
                    metadata,
                ),
            ));
        }

        for file in session.files_touched.iter().filter(|file| file.modified) {
            let summary = file
                .summary
                .clone()
                .unwrap_or_else(|| format!("Modified file: {}", file.path));
            let mut metadata = HashMap::new();
            metadata.insert("path".to_string(), file.path.clone());
            metadata.insert("mode".to_string(), file.last_mode.clone());
            candidates.push((
                session.updated_at,
                context_object(
                    ContextItemId::from_file(&file.path),
                    ContextObjectKind::File,
                    SESSION_PROVIDER,
                    file.file_ref.clone().unwrap_or_else(|| file.path.clone()),
                    file.path.clone(),
                    Some(summary.clone()),
                    freshness(session.updated_at.to_rfc3339(), file.stale),
                    1.0,
                    file.tokens.max(count_tokens(&summary)),
                    ViewCosts::from_full_tokens(file.tokens.max(count_tokens(&summary))),
                    Provenance::default(),
                    metadata,
                ),
            ));
        }

        candidates.sort_by(|(left_time, left), (right_time, right)| {
            right_time
                .cmp(left_time)
                .then_with(|| left.id.as_str().cmp(right.id.as_str()))
        });
        candidates
            .into_iter()
            .take(ctx.max_candidates)
            .map(|(_, candidate)| candidate)
            .collect()
    }

    fn side_effect_policy(&self) -> SideEffectPolicy {
        SideEffectPolicy::ReadOnly
    }
}

/// Supplies query-matched episodes from persistent episodic memory.
pub struct EpisodicProvider {
    project_root: String,
}

impl EpisodicProvider {
    /// Creates a provider scoped to `project_root`.
    pub fn new(project_root: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl CandidateProvider for EpisodicProvider {
    fn provider_id(&self) -> &str {
        EPISODIC_PROVIDER
    }

    fn candidates(&self, ctx: &RetrievalContext) -> Vec<ContextObjectV1> {
        let project_hash = crate::core::project_hash::hash_project_root(&self.project_root);
        let Some(store) = EpisodicStore::load(&project_hash) else {
            return Vec::new();
        };

        store
            .search(&ctx.query)
            .into_iter()
            .take(ctx.max_candidates)
            .map(|episode| {
                let mut metadata = HashMap::new();
                metadata.insert("session_id".to_string(), episode.session_id.clone());
                metadata.insert("outcome".to_string(), episode.outcome.label().to_string());
                context_object(
                    ContextItemId::from_memory(&episode.id),
                    ContextObjectKind::Episode,
                    EPISODIC_PROVIDER,
                    episode.id.clone(),
                    episode.task_description.clone(),
                    Some(episode.summary.clone()),
                    freshness(episode.timestamp.to_rfc3339(), false),
                    outcome_confidence(&episode.outcome),
                    count_tokens(&episode.summary),
                    ViewCosts::from_full_tokens(count_tokens(&episode.summary)),
                    Provenance::default(),
                    metadata,
                )
            })
            .collect()
    }

    fn side_effect_policy(&self) -> SideEffectPolicy {
        SideEffectPolicy::ReadOnly
    }
}

/// Supplies task-matched procedures from persistent procedural memory.
pub struct ProceduralProvider {
    project_root: String,
}

impl ProceduralProvider {
    /// Creates a provider scoped to `project_root`.
    pub fn new(project_root: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl CandidateProvider for ProceduralProvider {
    fn provider_id(&self) -> &str {
        PROCEDURAL_PROVIDER
    }

    fn candidates(&self, ctx: &RetrievalContext) -> Vec<ContextObjectV1> {
        let project_hash = crate::core::project_hash::hash_project_root(&self.project_root);
        let Some(store) = ProceduralStore::load(&project_hash) else {
            return Vec::new();
        };
        let task = ctx.task.as_deref().unwrap_or(&ctx.query);

        store
            .suggest(task)
            .into_iter()
            .take(ctx.max_candidates)
            .map(|procedure| {
                let content = format_procedure_steps(procedure);
                let mut metadata = HashMap::new();
                metadata.insert("description".to_string(), procedure.description.clone());
                metadata.insert(
                    "activation_keywords".to_string(),
                    procedure.activation_keywords.join(","),
                );
                context_object(
                    ContextItemId::from_memory(&procedure.id),
                    ContextObjectKind::Procedure,
                    PROCEDURAL_PROVIDER,
                    procedure.id.clone(),
                    procedure.name.clone(),
                    Some(content),
                    freshness(procedure.created_at.to_rfc3339(), false),
                    procedure.confidence,
                    procedure.steps.len() * 30,
                    ViewCosts::from_full_tokens(procedure.steps.len() * 30),
                    Provenance::default(),
                    metadata,
                )
            })
            .collect()
    }

    fn side_effect_policy(&self) -> SideEffectPolicy {
        SideEffectPolicy::ReadOnly
    }
}

/// Supplies previously delivered ledger items as high-confidence candidates.
pub struct LedgerProvider {
    project_root: String,
}

impl LedgerProvider {
    /// Creates a provider scoped to `project_root`.
    pub fn new(project_root: impl Into<String>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }
}

impl CandidateProvider for LedgerProvider {
    fn provider_id(&self) -> &str {
        LEDGER_PROVIDER
    }

    fn candidates(&self, ctx: &RetrievalContext) -> Vec<ContextObjectV1> {
        let _ = &self.project_root;
        ledger_candidates(&ContextLedger::load().entries, ctx.max_candidates)
    }

    fn side_effect_policy(&self) -> SideEffectPolicy {
        SideEffectPolicy::ReadOnly
    }
}

/// Create all default providers for a project.
pub fn default_providers(project_root: &str) -> Vec<Box<dyn CandidateProvider>> {
    vec![
        Box::new(LedgerProvider::new(project_root)),
        Box::new(KnowledgeProvider::new(project_root)),
        Box::new(SessionProvider::new(project_root)),
        Box::new(EpisodicProvider::new(project_root)),
        Box::new(ProceduralProvider::new(project_root)),
    ]
}

#[allow(clippy::too_many_arguments)]
fn context_object(
    id: ContextItemId,
    kind: ContextObjectKind,
    source: &str,
    content_ref: String,
    title: String,
    content: Option<String>,
    freshness: Freshness,
    confidence: f32,
    token_estimate: usize,
    view_costs: ViewCosts,
    provenance: Provenance,
    metadata: HashMap<String, String>,
) -> ContextObjectV1 {
    ContextObjectV1 {
        id,
        kind,
        source: source.to_string(),
        content_ref,
        title,
        content,
        freshness,
        confidence,
        sensitivity: SensitivityLevel::Internal,
        token_estimate,
        view_costs,
        provenance,
        semantic_fingerprint: None,
        metadata,
    }
}

fn freshness(created_at: String, stale: bool) -> Freshness {
    Freshness {
        created_at,
        ttl_secs: None,
        stale,
    }
}

fn outcome_confidence(outcome: &Outcome) -> f32 {
    match outcome {
        Outcome::Success { .. } => 0.9,
        Outcome::Partial { .. } => 0.6,
        Outcome::Failure { .. } | Outcome::Unknown => 0.3,
    }
}

fn format_procedure_steps(procedure: &Procedure) -> String {
    procedure
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {}: {}", index + 1, step.tool, step.description))
        .collect::<Vec<_>>()
        .join("\n")
}

fn ledger_candidates(entries: &[LedgerEntry], max_candidates: usize) -> Vec<ContextObjectV1> {
    let mut candidates: Vec<(f64, ContextObjectV1)> = entries
        .iter()
        .filter(|entry| entry.state != Some(ContextState::Excluded))
        .map(|entry| {
            let mut metadata = HashMap::new();
            metadata.insert("mode".to_string(), entry.mode.clone());
            metadata.insert("path".to_string(), entry.path.clone());
            let tokens = entry.sent_tokens;
            (
                entry.phi.unwrap_or_default(),
                context_object(
                    entry
                        .id
                        .clone()
                        .unwrap_or_else(|| ContextItemId::from_file(&entry.path)),
                    ContextObjectKind::File,
                    LEDGER_PROVIDER,
                    entry
                        .source_hash
                        .clone()
                        .unwrap_or_else(|| entry.path.clone()),
                    entry.path.clone(),
                    None,
                    freshness(
                        entry.timestamp.to_string(),
                        entry.state == Some(ContextState::Stale),
                    ),
                    1.0,
                    tokens,
                    entry
                        .view_costs
                        .clone()
                        .unwrap_or_else(|| ViewCosts::from_full_tokens(tokens)),
                    entry.provenance.clone().unwrap_or_default(),
                    metadata,
                ),
            )
        })
        .collect();

    candidates.sort_by(|(left_phi, left), (right_phi, right)| {
        right_phi
            .total_cmp(left_phi)
            .then_with(|| left.id.as_str().cmp(right.id.as_str()))
    });
    candidates
        .into_iter()
        .take(max_candidates)
        .map(|(_, candidate)| candidate)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::context_field::TokenBudget;

    fn retrieval_context() -> RetrievalContext {
        RetrievalContext {
            query: "context kernel provider test".to_string(),
            task: None,
            project_root: "/__context_kernel_test_project__".to_string(),
            budget: TokenBudget {
                total: 1_000,
                used: 0,
            },
            max_candidates: 10,
        }
    }

    #[test]
    fn provider_ids_are_stable() {
        assert_eq!(
            KnowledgeProvider::new("/tmp").provider_id(),
            KNOWLEDGE_PROVIDER
        );
        assert_eq!(SessionProvider::new("/tmp").provider_id(), SESSION_PROVIDER);
        assert_eq!(
            EpisodicProvider::new("/tmp").provider_id(),
            EPISODIC_PROVIDER
        );
        assert_eq!(
            ProceduralProvider::new("/tmp").provider_id(),
            PROCEDURAL_PROVIDER
        );
        assert_eq!(LedgerProvider::new("/tmp").provider_id(), LEDGER_PROVIDER);
    }

    #[test]
    fn provider_side_effect_policies_are_declared() {
        assert_eq!(
            KnowledgeProvider::new("/tmp").side_effect_policy(),
            SideEffectPolicy::MutatesStats
        );
        for provider in [
            SessionProvider::new("/tmp").side_effect_policy(),
            EpisodicProvider::new("/tmp").side_effect_policy(),
            ProceduralProvider::new("/tmp").side_effect_policy(),
            LedgerProvider::new("/tmp").side_effect_policy(),
        ] {
            assert_eq!(provider, SideEffectPolicy::ReadOnly);
        }
    }

    #[test]
    fn default_providers_include_all_store_wrappers() {
        assert_eq!(default_providers("/tmp").len(), 5);
    }

    #[test]
    fn ledger_candidates_are_empty_for_an_empty_ledger() {
        assert!(ledger_candidates(&ContextLedger::new().entries, 10).is_empty());
    }

    #[test]
    fn knowledge_candidates_are_empty_for_a_missing_project() {
        let provider = KnowledgeProvider::new("/__context_kernel_missing_project__");
        assert!(provider.candidates(&retrieval_context()).is_empty());
    }
}
