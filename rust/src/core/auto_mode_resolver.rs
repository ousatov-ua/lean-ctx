use std::collections::HashMap;
use std::sync::Mutex;

use crate::core::cache::SessionCache;
use crate::core::context_ledger::PressureAction;
use crate::core::mode_predictor::{FileSignature, ModePredictor};

/// Per-process counters of which signal decided each auto-mode resolution.
/// Surfaced by `ctx_metrics` so the learning loops are observable (#496).
static SOURCE_COUNTS: Mutex<Option<HashMap<&'static str, u64>>> = Mutex::new(None);

fn count_source(source: &'static str) {
    if let Ok(mut guard) = SOURCE_COUNTS.lock() {
        *guard
            .get_or_insert_with(HashMap::new)
            .entry(source)
            .or_insert(0) += 1;
    }
}

/// Snapshot of auto-mode decision sources, sorted by count descending.
pub fn source_counts() -> Vec<(&'static str, u64)> {
    let Ok(guard) = SOURCE_COUNTS.lock() else {
        return Vec::new();
    };
    let mut items: Vec<(&'static str, u64)> = guard
        .as_ref()
        .map(|m| m.iter().map(|(k, v)| (*k, *v)).collect())
        .unwrap_or_default();
    items.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    items
}

fn sources_path() -> Option<std::path::PathBuf> {
    crate::core::data_dir::lean_ctx_data_dir()
        .ok()
        .map(|d| d.join("auto_mode_sources.json"))
}

/// Persist the in-process counters by *adding* them into the cumulative
/// on-disk file, then reset the process counters. The counters live in the
/// MCP/CLI process — the dashboard is a separate process and can only see
/// them through this file (#505).
pub fn flush_sources() {
    let drained: Vec<(String, u64)> = {
        let Ok(mut guard) = SOURCE_COUNTS.lock() else {
            return;
        };
        match guard.take() {
            Some(m) if !m.is_empty() => m.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            _ => return,
        }
    };
    let Some(path) = sources_path() else {
        return;
    };
    let mut on_disk: HashMap<String, u64> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    for (k, v) in drained {
        *on_disk.entry(k).or_insert(0) += v;
    }
    let Ok(json) = serde_json::to_string_pretty(&on_disk) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Cumulative auto-mode decision sources from disk (all processes, all time),
/// sorted by count descending. Used by the dashboard's Live Signals panel.
pub fn persisted_source_counts() -> Vec<(String, u64)> {
    let Some(path) = sources_path() else {
        return Vec::new();
    };
    let map: HashMap<String, u64> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let mut items: Vec<(String, u64)> = map.into_iter().collect();
    items.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    items
}

pub struct AutoModeContext<'a> {
    pub path: &'a str,
    pub token_count: usize,
    pub task: Option<&'a str>,
    pub cache: Option<&'a SessionCache>,
}

pub struct ResolvedMode {
    pub mode: String,
    pub source: &'static str,
}

/// Single entry point for auto-mode resolution.
/// Merges Pipeline A (select_mode_with_task) and Pipeline B (resolve_auto_mode).
pub fn resolve(ctx: &AutoModeContext) -> ResolvedMode {
    // Quality loop (#494), signal 1: an edit on this file just failed after a
    // compressed read — the agent needs the real body now, one-shot.
    if crate::core::edit_quality::take_pending_escalation(ctx.path) {
        return resolved("full", "edit_fail_escalation");
    }

    let r = resolve_inner(ctx);

    // Quality loop (#494), signal 2: this mode keeps producing edit failures
    // for this file type — compression here is a proven net loss, use full.
    if r.mode != "full" && crate::core::edit_quality::is_risky_mode(ctx.path, &r.mode) {
        return resolved("full", "edit_quality_penalty");
    }
    r
}

fn resolve_inner(ctx: &AutoModeContext) -> ResolvedMode {
    if crate::tools::ctx_read::is_instruction_file(ctx.path) {
        return resolved("full", "instruction_file");
    }

    if crate::core::binary_detect::is_binary_file(ctx.path) {
        return resolved("full", "binary");
    }

    if let Some(cache) = ctx.cache {
        if let Some(cached) = cache.get(ctx.path) {
            if file_unchanged(ctx.path, cached) {
                return resolved("full", "cache_hit");
            }
            return resolved("diff", "cache_changed");
        }
    }

    if ctx.token_count <= 200 {
        return resolved("full", "small_file");
    }

    let ext = std::path::Path::new(ctx.path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if is_config_or_data(ext, ctx.path) {
        return resolved("full", "config_data");
    }

    if let Ok(bt) = crate::core::bounce_tracker::global().lock() {
        if bt.should_force_full(ctx.path) {
            return resolved("full", "bounce_tracker");
        }
    }

    // Per-path long-term memory (#496): a file that historically bounced in
    // the majority of its reads will bounce again — compression is a proven
    // net loss for it, across process restarts.
    if crate::core::path_mode_memory::should_force_full(ctx.path) {
        return resolved("full", "path_bounce_memory");
    }

    // Active compiler error (#499): the agent reads this file to fix the
    // build — compressed modes would hide the error region.
    if crate::core::diagnostics_store::has_error(ctx.path) {
        return resolved("full", "active_diagnostic");
    }

    if let Some(mode) = intent_recommended_mode(ctx.task) {
        return resolved(&mode, "intent");
    }

    let sig = FileSignature::from_path(ctx.path, ctx.token_count);
    let predictor = ModePredictor::new();
    let mut predicted = predictor
        .predict_best_mode(&sig)
        .unwrap_or_else(|| "full".to_string());
    if predicted == "auto" {
        predicted = "full".to_string();
    }

    if predicted != "full" {
        if let Some(bandit_override) = bandit_explore(ctx.path, ctx.token_count) {
            predicted = bandit_override;
        }
    }

    // Heatmap signal (#496): a frequently-read file where compression barely
    // saves anything will likely trigger a follow-up read — step one mode more
    // conservative. avg_compression_ratio is the historical fraction saved.
    if predicted != "full" {
        if let Some((access_count, avg_ratio)) = crate::core::heatmap::entry_stats(ctx.path) {
            if access_count >= 5 && avg_ratio < 0.30 {
                let conservative = match predicted.as_str() {
                    "signatures" | "aggressive" | "entropy" => "map".to_string(),
                    "map" if ctx.token_count <= 6000 => "full".to_string(),
                    other => other.to_string(),
                };
                if conservative != predicted {
                    return resolved(&conservative, "heatmap_conservative");
                }
            }
        }
    }

    let policy = crate::core::adaptive_mode_policy::AdaptiveModePolicyStore::load();
    let chosen = policy.choose_auto_mode(ctx.task, &predicted);

    if ctx.token_count > 2000 {
        if (predicted == "map" || predicted == "signatures")
            && chosen != "map"
            && chosen != "signatures"
        {
            return resolved(&predicted, "predictor_guard");
        }
        if chosen == "full" && predicted != "full" {
            return resolved(&predicted, "predictor_override");
        }
    }

    if chosen != predicted {
        return resolved(&chosen, "adaptive_policy");
    }

    if predicted != "full" {
        return resolved(&predicted, "predictor");
    }

    let heuristic = heuristic_mode(ext, ctx.token_count);
    resolved(&heuristic, "heuristic")
}

/// Unified pressure downgrade table.
/// Used by both context_gate and intent_router pressure paths.
pub fn pressure_downgrade(requested_mode: &str, action: &PressureAction) -> Option<String> {
    match action {
        PressureAction::SuggestCompression => match requested_mode {
            "auto" | "full" => Some("map".to_string()),
            _ => None,
        },
        PressureAction::ForceCompression => match requested_mode {
            "full" => Some("map".to_string()),
            "auto" | "map" => Some("signatures".to_string()),
            _ => None,
        },
        PressureAction::EvictLeastRelevant => match requested_mode {
            "full" => Some("map".to_string()),
            "auto" | "map" => Some("signatures".to_string()),
            "signatures" => Some("reference".to_string()),
            _ => None,
        },
        PressureAction::NoAction => None,
    }
}

fn intent_recommended_mode(task: Option<&str>) -> Option<String> {
    let task_desc = task?;
    let classification = crate::core::intent_engine::classify(task_desc);
    if classification.confidence < 0.4 {
        return None;
    }
    let route = crate::core::intent_engine::route_intent(task_desc, &classification);
    let mode =
        crate::core::intent_router::read_mode_for_tier(route.model_tier, classification.task_type);
    if mode == "auto" {
        return None;
    }
    Some(mode)
}

fn bandit_explore(file_path: &str, token_count: usize) -> Option<String> {
    let project_root =
        crate::core::session::SessionState::load_latest().and_then(|s| s.project_root)?;
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let bucket = match token_count {
        0..=2000 => "sm",
        2001..=10000 => "md",
        10001..=50000 => "lg",
        _ => "xl",
    };
    let bandit_key = format!("{ext}_{bucket}");
    let mut store = crate::core::bandit::BanditStore::load(&project_root);
    let bandit = store.get_or_create(&bandit_key);
    let arm = bandit.select_arm();
    if arm.budget_ratio < 0.25 && token_count > 2000 {
        Some("aggressive".to_string())
    } else {
        None
    }
}

fn heuristic_mode(ext: &str, token_count: usize) -> String {
    if token_count > 8000 {
        if is_code(ext) {
            return "map".to_string();
        }
        return "aggressive".to_string();
    }
    // Raised from 3000 → 6000: at 3-6k tokens, returning only signatures forces
    // the agent into a follow-up full/lines read for the body it actually
    // needs. Keeping `full` here trades a few hundred tokens per call for
    // fewer round-trips — the right call per the total-task-token principle.
    if token_count > 6000 && is_code(ext) {
        return "map".to_string();
    }
    "full".to_string()
}

/// Fast O(1) staleness check: if the file's mtime still matches what was
/// stored when the cache entry was created, the content is unchanged — no need
/// to read the file or compute any hash. Falls back to "changed" when metadata
/// is unavailable (e.g. file deleted) or when the cache entry predates mtime
/// tracking (legacy entries with `stored_mtime = None`).
///
/// mtime comparison is sufficient for correctness on all major filesystems:
/// every `write(2)` / `truncate(2)` updates mtime (POSIX guarantee).
fn file_unchanged(path: &str, cached: &crate::core::cache::CacheEntry) -> bool {
    let Some(stored_mtime) = cached.stored_mtime else {
        return false;
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(current_mtime) = meta.modified() else {
        return false;
    };
    current_mtime == stored_mtime
}

fn is_code(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "go"
            | "java"
            | "c"
            | "cpp"
            | "cc"
            | "h"
            | "hpp"
            | "rb"
            | "cs"
            | "kt"
            | "swift"
            | "php"
            | "zig"
            | "ex"
            | "exs"
            | "scala"
            | "sc"
            | "dart"
            | "sh"
            | "bash"
            | "svelte"
            | "vue"
    )
}

fn is_config_or_data(ext: &str, path: &str) -> bool {
    if matches!(ext, "xml" | "ini" | "cfg" | "env") {
        return true;
    }
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    matches!(
        name,
        "Cargo.toml"
            | "package.json"
            | "tsconfig.json"
            | "Makefile"
            | "Dockerfile"
            | "docker-compose.yml"
            | ".gitignore"
            | ".env"
            | "pyproject.toml"
            | "go.mod"
            | "build.gradle"
            | "pom.xml"
    )
}

fn resolved(mode: &str, source: &'static str) -> ResolvedMode {
    count_source(source);
    ResolvedMode {
        mode: mode.to_string(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_suggest_full_to_map() {
        assert_eq!(
            pressure_downgrade("full", &PressureAction::SuggestCompression),
            Some("map".to_string())
        );
    }

    #[test]
    fn pressure_suggest_auto_to_map() {
        assert_eq!(
            pressure_downgrade("auto", &PressureAction::SuggestCompression),
            Some("map".to_string())
        );
    }

    #[test]
    fn pressure_suggest_does_not_touch_signatures() {
        assert!(pressure_downgrade("signatures", &PressureAction::SuggestCompression).is_none());
    }

    #[test]
    fn pressure_force_full_to_map() {
        assert_eq!(
            pressure_downgrade("full", &PressureAction::ForceCompression),
            Some("map".to_string())
        );
    }

    #[test]
    fn pressure_force_map_to_signatures() {
        assert_eq!(
            pressure_downgrade("map", &PressureAction::ForceCompression),
            Some("signatures".to_string())
        );
    }

    #[test]
    fn pressure_evict_signatures_to_reference() {
        assert_eq!(
            pressure_downgrade("signatures", &PressureAction::EvictLeastRelevant),
            Some("reference".to_string())
        );
    }

    #[test]
    fn pressure_noaction_returns_none() {
        assert!(pressure_downgrade("full", &PressureAction::NoAction).is_none());
    }

    #[test]
    fn flush_sources_merges_additively_into_disk_file() {
        let _lock = crate::core::data_dir::test_env_lock();
        let dir = std::env::temp_dir().join(format!("lctx-amr-flush-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::env::set_var("LEAN_CTX_DATA_DIR", dir.to_str().unwrap());
        let _ = std::fs::remove_file(dir.join("auto_mode_sources.json"));

        // Unique test-only keys: parallel resolve() tests count real sources
        // into the same process-global map, so shared keys would be flaky.
        count_source("test_flush_alpha");
        count_source("test_flush_alpha");
        count_source("test_flush_beta");
        flush_sources();

        count_source("test_flush_alpha");
        flush_sources();

        let persisted = persisted_source_counts();
        let get = |k: &str| {
            persisted
                .iter()
                .find(|(s, _)| s == k)
                .map_or(0, |(_, n)| *n)
        };
        assert_eq!(
            get("test_flush_alpha"),
            3,
            "two flushes must merge additively"
        );
        assert_eq!(get("test_flush_beta"), 1);

        std::env::remove_var("LEAN_CTX_DATA_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn small_file_always_full() {
        let ctx = AutoModeContext {
            path: "test.rs",
            token_count: 100,
            task: None,
            cache: None,
        };
        let result = resolve(&ctx);
        assert_eq!(result.mode, "full");
        assert_eq!(result.source, "small_file");
    }

    #[test]
    fn config_file_returns_full() {
        let ctx = AutoModeContext {
            path: "config.ini",
            token_count: 500,
            task: None,
            cache: None,
        };
        let result = resolve(&ctx);
        assert_eq!(result.mode, "full");
        assert_eq!(result.source, "config_data");
    }

    #[test]
    fn intent_explore_returns_map() {
        let ctx = AutoModeContext {
            path: "large.rs",
            token_count: 5000,
            task: Some("how does the cache work?"),
            cache: None,
        };
        let result = resolve(&ctx);
        assert_eq!(result.mode, "map");
        assert_eq!(result.source, "intent");
    }
}
