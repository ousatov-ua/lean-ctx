//! Shadow logging for Context Kernel plans and receipts.

use std::fs;
use std::path::PathBuf;

use super::types::{ContextPlanV1, ContextReceiptV1};

/// Persists kernel artifacts for debugging and observability.
pub struct ShadowLogger {
    log_dir: PathBuf,
    max_entries: usize,
}

impl ShadowLogger {
    /// Creates a shadow logger with the supplied storage limit.
    pub fn new(log_dir: PathBuf, max_entries: usize) -> Self {
        Self {
            log_dir,
            max_entries,
        }
    }

    /// Creates the default per-user kernel shadow logger.
    pub fn default_for_project(_project_root: &str) -> Self {
        let dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("lean-ctx")
            .join("kernel");
        Self::new(dir, 100)
    }

    /// Writes a context plan as pretty-printed JSON.
    pub fn log_plan(&self, plan: &ContextPlanV1) {
        if fs::create_dir_all(&self.log_dir).is_err() {
            return;
        }
        if let Ok(json) = serde_json::to_string_pretty(plan) {
            let path = self.log_dir.join(format!("{}.plan.json", plan.plan_id));
            if fs::write(path, json).is_ok() {
                self.rotate();
            }
        }
    }

    /// Writes a context receipt as pretty-printed JSON.
    pub fn log_receipt(&self, receipt: &ContextReceiptV1) {
        if fs::create_dir_all(&self.log_dir).is_err() {
            return;
        }
        if let Ok(json) = serde_json::to_string_pretty(receipt) {
            let path = self
                .log_dir
                .join(format!("{}.receipt.json", receipt.plan_id));
            if fs::write(path, json).is_ok() {
                self.rotate();
            }
        }
    }

    fn rotate(&self) {
        let Ok(entries) = fs::read_dir(&self.log_dir) else {
            return;
        };
        let mut json_files: Vec<(std::time::SystemTime, PathBuf)> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                    return None;
                }
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, path))
            })
            .collect();
        json_files.sort_by_key(|(modified, path)| (*modified, path.clone()));

        let remove_count = json_files.len().saturating_sub(self.max_entries);
        for (_, path) in json_files.into_iter().take(remove_count) {
            let _ = fs::remove_file(path);
        }
    }
}

/// Logs a plan using the default project logger.
pub fn log_kernel_event(project_root: &str, plan: &ContextPlanV1) {
    let logger = ShadowLogger::default_for_project(project_root);
    logger.log_plan(plan);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::super::types::ContextPlanV1;
    use super::ShadowLogger;
    use crate::core::context_field::TokenBudget;

    fn plan_with_id(plan_id: &str) -> ContextPlanV1 {
        let mut plan = ContextPlanV1::empty(
            "shadow logging",
            TokenBudget {
                total: 100,
                used: 0,
            },
        );
        plan.plan_id = plan_id.to_owned();
        plan
    }

    #[test]
    fn log_plan_creates_json_file() {
        let temp_dir = TempDir::new().expect("create temporary directory");
        let logger = ShadowLogger::new(temp_dir.path().to_path_buf(), 10);
        let plan = plan_with_id("test-plan");

        logger.log_plan(&plan);

        let path = temp_dir.path().join("test-plan.plan.json");
        assert!(path.exists());
        let contents = fs::read_to_string(path).expect("read shadow plan");
        let parsed: ContextPlanV1 =
            serde_json::from_str(&contents).expect("parse shadow plan JSON");
        assert_eq!(parsed.plan_id, plan.plan_id);
    }

    #[test]
    fn rotation_enforces_max_entries() {
        let temp_dir = TempDir::new().expect("create temporary directory");
        let logger = ShadowLogger::new(temp_dir.path().to_path_buf(), 3);

        for index in 0..5 {
            logger.log_plan(&plan_with_id(&format!("plan-{index}")));
        }

        let json_count = fs::read_dir(temp_dir.path())
            .expect("list shadow logs")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .count();
        assert_eq!(json_count, 3);
    }
}
