//! Shared progress counters for index builds (BM25, semantic, graph).
//!
//! Build code reports here; CLI / status_json read snapshots. Decouples
//! builders from the orchestrator (no module cycles).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Which index component is reporting progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexComponent {
    Graph,
    Bm25,
    Semantic,
}

/// Snapshot of progress for one component.
///
/// `total == 0` means indeterminate (caller should show a bouncing indicator).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub done: u64,
    pub total: u64,
}

impl ProgressSnapshot {
    /// `true` when a percentage can be shown.
    pub fn is_determinate(&self) -> bool {
        self.total > 0
    }

    /// Percent complete in `0..=100`. Returns `None` when indeterminate.
    pub fn percent(&self) -> Option<u8> {
        if self.total == 0 {
            return None;
        }
        let pct = ((self.done as f64 / self.total as f64) * 100.0).round() as u64;
        Some(pct.min(100) as u8)
    }
}

fn registry() -> &'static Mutex<HashMap<(String, IndexComponent), ProgressSnapshot>> {
    static REG: OnceLock<Mutex<HashMap<(String, IndexComponent), ProgressSnapshot>>> =
        OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Report progress for `component` under `project_root`.
///
/// Pass `total = 0` for indeterminate phases (e.g. model download).
pub fn report(project_root: &str, component: IndexComponent, done: u64, total: u64) {
    let mut map = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.insert(
        (project_root.to_string(), component),
        ProgressSnapshot { done, total },
    );
}

/// Clear progress for one component (call when a phase finishes or is skipped).
pub fn clear(project_root: &str, component: IndexComponent) {
    let mut map = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.remove(&(project_root.to_string(), component));
}

/// Clear all progress for a project root.
pub fn clear_root(project_root: &str) {
    let mut map = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.retain(|(root, _), _| root != project_root);
}

/// Read current progress for a component (defaults to indeterminate zeros).
pub fn get(project_root: &str, component: IndexComponent) -> ProgressSnapshot {
    let map = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.get(&(project_root.to_string(), component))
        .copied()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_get_clear_roundtrip() {
        let root = "/tmp/lean-ctx-progress-test-root";
        clear_root(root);
        assert!(!get(root, IndexComponent::Bm25).is_determinate());

        report(root, IndexComponent::Bm25, 3, 10);
        let snap = get(root, IndexComponent::Bm25);
        assert_eq!(snap, ProgressSnapshot { done: 3, total: 10 });
        assert_eq!(snap.percent(), Some(30));

        report(root, IndexComponent::Semantic, 0, 0);
        assert!(!get(root, IndexComponent::Semantic).is_determinate());

        clear(root, IndexComponent::Bm25);
        assert_eq!(get(root, IndexComponent::Bm25).total, 0);
        clear_root(root);
    }

    #[test]
    fn percent_caps_at_100() {
        assert_eq!(
            ProgressSnapshot {
                done: 12,
                total: 10
            }
            .percent(),
            Some(100)
        );
    }
}
