use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use super::types::{OclaError, OclaResult};

static NEXT_FORK_ID: AtomicU64 = AtomicU64::new(1);

static GLOBAL_CAPSULE_STORE: OnceLock<CapsuleStore> = OnceLock::new();

#[must_use]
pub fn global_capsule_store() -> &'static CapsuleStore {
    GLOBAL_CAPSULE_STORE.get_or_init(CapsuleStore::new)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delta {
    pub offset: usize,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct CapsuleEntry {
    pub parent_ref: Option<String>,
    pub data: Vec<u8>,
    pub deltas: Vec<Delta>,
    pub budget_tokens: u64,
    pub created_at: Instant,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapsuleStats {
    pub total_entries: usize,
    pub total_bytes: usize,
    pub max_depth: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CapsuleStore {
    entries: Arc<RwLock<HashMap<String, CapsuleEntry>>>,
}

impl CapsuleStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, data: &[u8]) -> String {
        let capsule_ref = format!("capsule:{}", blake3::hash(data).to_hex());
        let entry = CapsuleEntry {
            parent_ref: None,
            data: data.to_vec(),
            deltas: Vec::new(),
            budget_tokens: 0,
            created_at: Instant::now(),
        };
        if let Ok(mut entries) = self.entries.write() {
            entries.insert(capsule_ref.clone(), entry);
        }
        capsule_ref
    }

    pub fn fork(&self, parent_ref: &str, budget_tokens: u64) -> OclaResult<String> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| invalid("capsule store lock poisoned"))?;
        if !entries.contains_key(parent_ref) {
            return Err(invalid(format!("unknown parent capsule: {parent_ref}")));
        }

        let fork_id = NEXT_FORK_ID.fetch_add(1, Ordering::Relaxed);
        let identity = format!("{parent_ref}\0{budget_tokens}\0{fork_id}");
        let capsule_ref = format!("capsule:{}", blake3::hash(identity.as_bytes()).to_hex());
        entries.insert(
            capsule_ref.clone(),
            CapsuleEntry {
                parent_ref: Some(parent_ref.to_string()),
                data: Vec::new(),
                deltas: Vec::new(),
                budget_tokens,
                created_at: Instant::now(),
            },
        );
        Ok(capsule_ref)
    }

    pub fn resolve(&self, capsule_ref: &str) -> OclaResult<Vec<u8>> {
        let entries = self
            .entries
            .read()
            .map_err(|_| invalid("capsule store lock poisoned"))?;
        resolve_entries(&entries, capsule_ref)
    }

    pub(crate) fn budget_tokens(&self, capsule_ref: &str) -> OclaResult<u64> {
        let entries = self
            .entries
            .read()
            .map_err(|_| invalid("capsule store lock poisoned"))?;
        entries
            .get(capsule_ref)
            .map(|entry| entry.budget_tokens)
            .ok_or_else(|| invalid(format!("unknown capsule: {capsule_ref}")))
    }

    pub fn apply_delta(&self, capsule_ref: &str, delta: Delta) -> OclaResult<()> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| invalid("capsule store lock poisoned"))?;
        let current = resolve_entries(&entries, capsule_ref)?;
        let entry = entries
            .get_mut(capsule_ref)
            .ok_or_else(|| invalid(format!("unknown capsule: {capsule_ref}")))?;
        if entry.parent_ref.is_none() {
            return Err(invalid("deltas can only be applied to forked capsules"));
        }
        if delta.offset > current.len() {
            return Err(invalid("capsule delta starts beyond materialized content"));
        }
        entry.deltas.push(delta);
        Ok(())
    }

    pub fn merge_back(&self, child_ref: &str) -> OclaResult<()> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| invalid("capsule store lock poisoned"))?;
        resolve_entries(&entries, child_ref)?;
        let (parent_ref, deltas) = {
            let child = entries
                .get(child_ref)
                .ok_or_else(|| invalid(format!("unknown capsule: {child_ref}")))?;
            (
                child
                    .parent_ref
                    .clone()
                    .ok_or_else(|| invalid("root capsules cannot merge back"))?,
                child.deltas.clone(),
            )
        };
        let parent = entries
            .get_mut(&parent_ref)
            .ok_or_else(|| invalid(format!("unknown parent capsule: {parent_ref}")))?;
        parent.deltas.extend(deltas);
        entries
            .get_mut(child_ref)
            .ok_or_else(|| invalid(format!("unknown capsule: {child_ref}")))?
            .deltas
            .clear();
        Ok(())
    }

    #[must_use]
    pub fn stats(&self) -> CapsuleStats {
        let Ok(entries) = self.entries.read() else {
            return CapsuleStats::default();
        };
        let total_bytes = entries.values().fold(0_usize, |total, entry| {
            total.saturating_add(entry.data.len()).saturating_add(
                entry
                    .deltas
                    .iter()
                    .map(|delta| delta.data.len())
                    .sum::<usize>(),
            )
        });
        let max_depth = entries
            .keys()
            .map(|capsule_ref| depth_of(&entries, capsule_ref))
            .max()
            .unwrap_or(0);
        CapsuleStats {
            total_entries: entries.len(),
            total_bytes,
            max_depth,
        }
    }
}

fn resolve_entries(
    entries: &HashMap<String, CapsuleEntry>,
    capsule_ref: &str,
) -> OclaResult<Vec<u8>> {
    let mut current = capsule_ref;
    let mut visited = HashSet::new();
    let mut deltas = Vec::new();
    loop {
        if !visited.insert(current) {
            return Err(invalid("capsule parent cycle detected"));
        }
        let entry = entries
            .get(current)
            .ok_or_else(|| invalid(format!("unknown capsule: {capsule_ref}")))?;
        deltas.extend(entry.deltas.iter().cloned());
        if let Some(parent_ref) = entry.parent_ref.as_deref() {
            current = parent_ref;
        } else {
            let mut data = entry.data.clone();
            for delta in deltas.iter().rev() {
                apply_patch(&mut data, delta)?;
            }
            return Ok(data);
        }
    }
}

fn apply_patch(data: &mut Vec<u8>, delta: &Delta) -> OclaResult<()> {
    let end = delta
        .offset
        .checked_add(delta.data.len())
        .ok_or_else(|| invalid("capsule delta range overflow"))?;
    if delta.offset > data.len() {
        return Err(invalid("capsule delta starts beyond materialized content"));
    }
    if end > data.len() {
        data.resize(end, 0);
    }
    data[delta.offset..end].copy_from_slice(&delta.data);
    Ok(())
}

fn depth_of(entries: &HashMap<String, CapsuleEntry>, capsule_ref: &str) -> usize {
    let mut depth = 0;
    let mut current = capsule_ref;
    let mut visited = HashSet::new();
    while visited.insert(current) {
        let Some(entry) = entries.get(current) else {
            break;
        };
        let Some(parent_ref) = entry.parent_ref.as_deref() else {
            break;
        };
        depth += 1;
        current = parent_ref;
    }
    depth
}

fn invalid(message: impl Into<String>) -> OclaError {
    OclaError::InvalidRequest(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_delta() -> Delta {
        Delta {
            offset: 1,
            data: vec![b'a'],
        }
    }

    #[test]
    fn global_store_registers_capsule() {
        let capsule_ref = global_capsule_store().register(b"global capsule");
        assert_eq!(
            global_capsule_store()
                .resolve(&capsule_ref)
                .expect("global resolves"),
            b"global capsule"
        );
    }

    #[test]
    fn register_resolves_original_data() {
        let store = CapsuleStore::new();
        let capsule_ref = store.register(b"hello");
        assert_eq!(
            store.resolve(&capsule_ref).expect("root resolves"),
            b"hello"
        );
    }
    #[test]
    fn fork_resolves_parent_data() {
        let store = CapsuleStore::new();
        let parent_ref = store.register(b"hello");
        let child_ref = store.fork(&parent_ref, 100).expect("fork succeeds");
        assert_eq!(store.resolve(&child_ref).expect("child resolves"), b"hello");
        assert_eq!(store.budget_tokens(&child_ref).expect("budget exists"), 100);
    }
    #[test]
    fn fork_delta_resolves_patched_data() {
        let store = CapsuleStore::new();
        let parent_ref = store.register(b"hello");
        let child_ref = store.fork(&parent_ref, 100).expect("fork succeeds");
        store
            .apply_delta(&child_ref, test_delta())
            .expect("delta applies");
        assert_eq!(store.resolve(&child_ref).expect("child resolves"), b"hallo");
    }
    #[test]
    fn merge_back_projects_deltas_to_parent() {
        let store = CapsuleStore::new();
        let parent_ref = store.register(b"hello");
        let child_ref = store.fork(&parent_ref, 100).expect("fork succeeds");
        store
            .apply_delta(&child_ref, test_delta())
            .expect("delta applies");
        store.merge_back(&child_ref).expect("merge succeeds");
        assert_eq!(
            store.resolve(&parent_ref).expect("parent resolves"),
            b"hallo"
        );
        assert_eq!(store.resolve(&child_ref).expect("child resolves"), b"hallo");
    }
    #[test]
    fn stats_report_entries_storage_and_depth() {
        let store = CapsuleStore::new();
        let parent_ref = store.register(b"hello");
        let child_ref = store.fork(&parent_ref, 100).expect("fork succeeds");
        store
            .apply_delta(&child_ref, test_delta())
            .expect("delta applies");
        let stats = store.stats();
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.total_bytes, 6);
        assert_eq!(stats.max_depth, 1);
    }
}
