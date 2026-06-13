use serde_json::{Map, Value};

use crate::server::tool_trait::{get_str, get_str_array, ToolContext};

#[derive(Debug)]
pub struct ResolvedPaths {
    pub roots: Vec<String>,
    pub is_multi: bool,
}

/// Resolve tool paths with multi-root support.
///
/// Priority:
/// 0. `repo` argument (multi-repo alias → specific root)
/// 1. `paths` array argument (explicit multi-root)
/// 2. `path` string argument (single root, pre-resolved by dispatch)
/// 3. Session `extra_roots` (default multi-root from config/MCP)
/// 4. Fallback to `"."` (project root)
///
/// Returns `Err` when an **explicit** `path`/`paths` argument was supplied but
/// could not be resolved (outside the project root, secret-screened, or
/// non-existent). Silently falling back to the project root in that case made
/// `ctx_tree path=/outside/repo` return the whole project tree (#401).
pub fn resolve_tool_paths(
    args: &Map<String, Value>,
    ctx: &ToolContext,
) -> Result<ResolvedPaths, String> {
    if let Some(repo) = get_str(args, "repo") {
        if let Some(root) = crate::core::multi_repo::resolve_repo_root(&repo) {
            return Ok(ResolvedPaths {
                roots: vec![root],
                is_multi: false,
            });
        }
    }

    if let Some(paths) = get_str_array(args, "paths") {
        if !paths.is_empty() {
            let resolved = resolve_paths_sync(ctx, &paths);
            if !resolved.is_empty() {
                return Ok(ResolvedPaths {
                    is_multi: resolved.len() > 1,
                    roots: resolved,
                });
            }
            // The caller explicitly listed paths but none resolved — surface
            // the failure instead of scanning the project root (#401).
            return Err(format!(
                "none of the requested paths could be resolved — they may not exist or are \
                 outside the project root: {}",
                paths.join(", ")
            ));
        }
    }

    if let Some(path) = ctx.resolved_path("path") {
        return Ok(ResolvedPaths {
            roots: vec![path.to_string()],
            is_multi: false,
        });
    }

    // An explicit `path` the dispatcher could not resolve lands in
    // `path_errors` (not `resolved_paths`). Do NOT fall back to the project
    // root — return the resolution error so the agent learns the path is out
    // of scope rather than silently receiving an unrelated tree (#401).
    if let Some(detail) = ctx.path_error("path") {
        return Err(detail.to_string());
    }

    if let Some(session_lock) = ctx.session.as_ref() {
        let (extra, jail_root) = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let session = session_lock.read().await;
                let root = session
                    .project_root
                    .clone()
                    .unwrap_or_else(|| ".".to_string());
                (session.extra_roots.clone(), root)
            })
        });
        if !extra.is_empty() {
            let jail = std::path::Path::new(&jail_root);
            let mut roots = vec![ctx.project_root.clone()];
            for r in &extra {
                let p = std::path::Path::new(r);
                if !p.is_dir() {
                    continue;
                }
                match crate::core::pathjail::jail_path(p, jail) {
                    Ok(_) => roots.push(r.clone()),
                    Err(e) => tracing::warn!("extra_root rejected by PathJail: {e}"),
                }
            }
            if roots.len() > 1 {
                return Ok(ResolvedPaths {
                    is_multi: true,
                    roots,
                });
            }
        }
    }

    Ok(ResolvedPaths {
        roots: vec![".".to_string()],
        is_multi: false,
    })
}

fn resolve_paths_sync(ctx: &ToolContext, raw: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(raw.len());
    for p in raw {
        match ctx.resolve_path_sync(p) {
            Ok(resolved) => out.push(resolved),
            Err(e) => {
                tracing::warn!("multi-path resolve failed for {p}: {e}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_ctx() -> ToolContext {
        ToolContext {
            project_root: "/test/project".to_string(),
            minimal: false,
            resolved_paths: std::collections::HashMap::new(),
            crp_mode: crate::tools::CrpMode::Off,
            cache: None,
            session: None,
            tool_calls: None,
            agent_id: None,
            workflow: None,
            ledger: None,
            client_name: None,
            pipeline_stats: None,
            call_count: None,
            autonomy: None,
            pressure_snapshot: None,
            path_errors: std::collections::HashMap::new(),
            bm25_cache: None,
            progress_sender: None,
        }
    }

    #[test]
    fn fallback_to_dot_when_nothing_set() {
        let args = Map::new();
        let ctx = test_ctx();
        let result = resolve_tool_paths(&args, &ctx).expect("no explicit path → default");
        assert_eq!(result.roots, vec!["."]);
        assert!(!result.is_multi);
    }

    #[test]
    fn uses_resolved_path_when_present() {
        let args = Map::new();
        let mut ctx = test_ctx();
        ctx.resolved_paths
            .insert("path".to_string(), "/resolved/dir".to_string());
        let result = resolve_tool_paths(&args, &ctx).expect("resolved path");
        assert_eq!(result.roots, vec!["/resolved/dir"]);
        assert!(!result.is_multi);
    }

    #[test]
    fn empty_paths_array_falls_back() {
        let mut args = Map::new();
        args.insert("paths".to_string(), json!([]));
        let mut ctx = test_ctx();
        ctx.resolved_paths
            .insert("path".to_string(), "/fallback".to_string());
        let result = resolve_tool_paths(&args, &ctx).expect("empty paths → fallback");
        assert_eq!(result.roots, vec!["/fallback"]);
        assert!(!result.is_multi);
    }

    // #401: an explicit `path` the dispatcher could not resolve (out of jail,
    // secret-screened, non-existent) must surface the error — NOT silently
    // fall back to the project root and return an unrelated tree.
    #[test]
    fn explicit_unresolvable_path_errors_instead_of_root_fallback() {
        let mut args = Map::new();
        args.insert(
            "path".to_string(),
            json!("/home/jules/.claude/skills/mpm-config"),
        );
        let mut ctx = test_ctx();
        // Dispatcher could not resolve it → recorded in path_errors, absent
        // from resolved_paths (exactly what the daemon does for out-of-jail).
        ctx.path_errors.insert(
            "path".to_string(),
            "path escapes project root: /home/jules/.claude/skills/mpm-config \
             (root: /test/project)"
                .to_string(),
        );
        let err = resolve_tool_paths(&args, &ctx)
            .expect_err("out-of-jail explicit path must be an error");
        assert!(
            err.contains("escapes project root"),
            "error must explain the path is out of scope: {err}"
        );
    }

    // #401: an explicit `paths` array where nothing resolves must error too.
    //
    // The candidate is an absolute path that cannot exist; its only existing
    // ancestor is the filesystem root `/`, which is never inside the project
    // root or any allow-listed directory. PathJail therefore rejects it
    // deterministically on every platform and regardless of allow-list env
    // state another test may have left behind. (An earlier version used sibling
    // temp dirs and flaked under `--test-threads=1`: a prior test had
    // allow-listed the temp directory via `LEAN_CTX_*` env vars, so the
    // out-of-jail sibling was accepted and the expected error never fired.)
    //
    // Gated on a live jail: `--all-features` (used by CI) enables `no-jail`,
    // which compiles PathJail out so every path resolves — there is no
    // out-of-scope path to reject. The same gate guards the PathJail unit
    // tests in `core::pathjail`.
    #[cfg(not(feature = "no-jail"))]
    #[test]
    fn explicit_unresolvable_paths_array_errors() {
        let ctx = test_ctx();
        let mut args = Map::new();
        args.insert(
            "paths".to_string(),
            json!(["/lean-ctx-nonexistent-path/never/here"]),
        );
        let err = resolve_tool_paths(&args, &ctx)
            .expect_err("a path outside the project root must be an error");
        assert!(
            err.contains("none of the requested paths"),
            "error must report the unresolved paths: {err}"
        );
    }
}
