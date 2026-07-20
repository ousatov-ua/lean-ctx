//! BuiltinObservationHook — emits structured observations to OclaBus.
//!
//! Wraps the proxy observation path. Each `observe` call appends to a
//! bounded per-session ring buffer and emits a CompressionApplied event
//! (the closest existing event type for observation signals).

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use crate::core::ocla::traits::{ObservationHook, OclaService};
use crate::core::ocla::types::{Observation, OclaCapability, OclaCapabilityKind, OclaResult};
use crate::core::ocla_bus::{self, OclaEvent};

const MAX_OBSERVATIONS: usize = 512;
const ORIGINAL_TOKENS: &str = "original_tokens";
const SAVED_TOKENS: &str = "saved_tokens";
const DELIVERED_TOKENS: &str = "delivered_tokens";
const COMPRESSION_RATIO_MILLI: &str = "compression_ratio_milli";

pub struct BuiltinObservationHook {
    state: Mutex<ObservationState>,
}

#[derive(Default)]
struct ObservationState {
    ring: HashMap<String, VecDeque<Observation>>,
}

impl BuiltinObservationHook {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ObservationState::default()),
        }
    }

    pub fn recent(&self, session_id: &str, limit: usize) -> Vec<Observation> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(ring) = state.ring.get(session_id) else {
            return Vec::new();
        };
        let start = ring.len().saturating_sub(limit);
        ring.iter().skip(start).cloned().collect()
    }

    fn enrich(observation: &mut Observation) -> (u64, u64) {
        let original = observation
            .attributes
            .get(ORIGINAL_TOKENS)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let saved = observation
            .attributes
            .get(SAVED_TOKENS)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
            .min(original);
        let delivered = original.saturating_sub(saved);

        let ratio = saved
            .saturating_mul(1000)
            .checked_div(original)
            .unwrap_or(0);

        observation
            .attributes
            .insert(DELIVERED_TOKENS.into(), delivered.to_string());
        observation
            .attributes
            .insert(COMPRESSION_RATIO_MILLI.into(), ratio.to_string());
        (original, saved)
    }

    fn project_heatmap(observation: &Observation, original: u64, saved: u64) {
        let Some(path) = observation.context.content_ref.strip_prefix("file:") else {
            return;
        };
        if path.is_empty() || original == 0 {
            return;
        }
        let original = usize::try_from(original).unwrap_or(usize::MAX);
        let saved = usize::try_from(saved).unwrap_or(usize::MAX);
        crate::core::heatmap::record_file_access_with_agent(
            path,
            original,
            saved.min(original),
            Some(&observation.context.agent_id),
        );
    }
}

impl Default for BuiltinObservationHook {
    fn default() -> Self {
        Self::new()
    }
}

impl OclaService for BuiltinObservationHook {
    fn capability(&self) -> OclaCapability {
        OclaCapability::available(OclaCapabilityKind::ObservationHook)
    }
}

impl ObservationHook for BuiltinObservationHook {
    fn observe(&self, mut observation: Observation) -> OclaResult<()> {
        let (original, saved) = Self::enrich(&mut observation);
        let session_id = observation.context.session_id.clone();
        let name = observation.name.clone();
        let path = observation
            .context
            .content_ref
            .strip_prefix("file:")
            .map(str::to_string);
        Self::project_heatmap(&observation, original, saved);

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let ring = state
            .ring
            .entry(session_id.clone())
            .or_insert_with(|| VecDeque::with_capacity(MAX_OBSERVATIONS));

        if ring.len() >= MAX_OBSERVATIONS {
            ring.pop_front();
        }
        ring.push_back(observation);

        ocla_bus::emit(OclaEvent::CompressionApplied {
            path,
            before_tokens: original,
            after_tokens: original.saturating_sub(saved),
            strategy: format!("observation:{name}"),
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ocla::types::OclaRequestContext;
    use std::collections::BTreeMap;

    fn ctx(session: &str) -> OclaRequestContext {
        OclaRequestContext {
            request_id: "r1".into(),
            session_id: session.to_string(),
            agent_id: "agent-test".into(),
            content_ref: "ref:test".into(),
            tenant_id: None,
        }
    }

    #[test]
    fn observe_stores_and_bounds() {
        let hook = BuiltinObservationHook::new();
        for i in 0..600 {
            let obs = Observation {
                context: ctx("s1"),
                name: format!("obs-{i}"),
                attributes: BTreeMap::new(),
            };
            hook.observe(obs).unwrap();
        }
        let state = hook.state.lock().unwrap();
        assert_eq!(state.ring.get("s1").unwrap().len(), MAX_OBSERVATIONS);
    }

    #[test]
    fn observe_enriches_tokens_and_projects_file_access() {
        let hook = BuiltinObservationHook::new();
        let mut context = ctx("s1");
        context.content_ref = "file:src/observed.rs".into();
        let observation = Observation {
            context,
            name: "tool_call:ctx_read".into(),
            attributes: BTreeMap::from([
                (ORIGINAL_TOKENS.into(), "100".into()),
                (SAVED_TOKENS.into(), "40".into()),
            ]),
        };

        hook.observe(observation).unwrap();

        let state = hook.state.lock().unwrap();
        let stored = state.ring.get("s1").unwrap().back().unwrap();
        assert_eq!(stored.attributes[DELIVERED_TOKENS], "60");
        assert_eq!(stored.attributes[COMPRESSION_RATIO_MILLI], "400");
        assert_eq!(stored.context.content_ref, "file:src/observed.rs");
    }

    #[test]
    fn observe_valid_input_is_returned_by_recent() {
        let hook = BuiltinObservationHook::new();
        let observation = Observation {
            context: ctx("session-valid"),
            name: "compression".into(),
            attributes: BTreeMap::from([
                (ORIGINAL_TOKENS.into(), "80".into()),
                (SAVED_TOKENS.into(), "20".into()),
            ]),
        };

        hook.observe(observation).unwrap();

        let recent = hook.recent("session-valid", 1);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "compression");
        assert_eq!(recent[0].attributes[DELIVERED_TOKENS], "60");
        assert_eq!(recent[0].attributes[COMPRESSION_RATIO_MILLI], "250");
    }

    #[test]
    fn observe_zero_tokens_reports_zero_ratio() {
        let hook = BuiltinObservationHook::new();
        let observation = Observation {
            context: ctx("session-empty"),
            name: "empty".into(),
            attributes: BTreeMap::from([
                (ORIGINAL_TOKENS.into(), "0".into()),
                (SAVED_TOKENS.into(), "0".into()),
            ]),
        };

        hook.observe(observation).unwrap();

        let stored = hook.recent("session-empty", 1);
        assert_eq!(stored[0].attributes[DELIVERED_TOKENS], "0");
        assert_eq!(stored[0].attributes[COMPRESSION_RATIO_MILLI], "0");
    }

    #[test]
    fn observe_clamps_invalid_savings() {
        let hook = BuiltinObservationHook::new();
        let observation = Observation {
            context: ctx("s1"),
            name: "tool_call:ctx_read".into(),
            attributes: BTreeMap::from([
                (ORIGINAL_TOKENS.into(), "10".into()),
                (SAVED_TOKENS.into(), "99".into()),
            ]),
        };

        hook.observe(observation).unwrap();

        let state = hook.state.lock().unwrap();
        let stored = state.ring.get("s1").unwrap().back().unwrap();
        assert_eq!(stored.attributes[DELIVERED_TOKENS], "0");
        assert_eq!(stored.attributes[COMPRESSION_RATIO_MILLI], "1000");
    }
}
