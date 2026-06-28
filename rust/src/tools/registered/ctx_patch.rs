use rmcp::ErrorData;
use rmcp::model::Tool;
use serde_json::{Map, Value, json};

use crate::server::tool_trait::{
    McpTool, ToolContext, ToolOutput, get_bool, get_int, get_str, require_resolved_path,
};
use crate::tool_defs::tool_def;

pub struct CtxPatchTool;

impl McpTool for CtxPatchTool {
    fn name(&self) -> &'static str {
        "ctx_patch"
    }

    fn tool_def(&self) -> Tool {
        tool_def(
            "ctx_patch",
            "Hash-anchored edit — edit by line ANCHOR, not by reproducing old text.\n\
             First read with ctx_read(mode=\"anchored\") to get N:hh|line anchors, then patch by (line, hash).\n\
             op=set_line replaces one line; replace_lines a range; insert_after adds after a line (line 0 = top); delete removes.\n\
             op=replace_symbol rewrites a whole symbol body by name (or path+line) via ctx_refactor — pass new_body.\n\
             new_text=\"\" deletes the line. Batch many line edits via ops:[…] — all validated against the same file, applied all-or-nothing.\n\
             A stale anchor is REJECTED with fresh anchors to retry — no partial writes. Prefer this over native str_replace/Edit for reliability.",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to edit" },
                    "op": { "type": "string", "description": "set_line | replace_lines | insert_after | delete | replace_symbol" },
                    "line": { "type": "integer", "description": "1-based line (set_line/insert_after/delete; line 0 = top for insert_after)" },
                    "hash": { "type": "string", "description": "Anchor hash hh from ctx_read(mode=anchored) for `line`" },
                    "start_line": { "type": "integer", "description": "Range start (replace_lines/delete)" },
                    "start_hash": { "type": "string", "description": "Anchor hash for start_line" },
                    "end_line": { "type": "integer", "description": "Range end, inclusive (replace_lines/delete)" },
                    "end_hash": { "type": "string", "description": "Anchor hash for end_line" },
                    "new_text": { "type": "string", "description": "Replacement text; \"\" deletes (set_line/replace_lines)" },
                    "name": { "type": "string", "description": "Symbol path for replace_symbol (qualified or bare)" },
                    "new_body": { "type": "string", "description": "Full replacement declaration for replace_symbol" },
                    "ops": {
                        "type": "array",
                        "description": "Batch-atomic edits; each item is {op, line/start_line…, hash…, new_text}",
                        "items": { "type": "object" }
                    },
                    "expected_md5": { "type": "string", "description": "Optional whole-file BLAKE3 guard (postimage md5 from a prior edit)" },
                    "backup": { "type": "boolean", "description": "Write a .bak before editing", "default": false },
                    "validate_syntax": { "type": "boolean", "description": "Reject edits that break a cleanly-parsing file (tree-sitter)", "default": true },
                    "evidence": { "type": "boolean", "description": "Append a redacted, bounded diff", "default": true }
                },
                "required": ["path"]
            }),
        )
    }

    fn handle(
        &self,
        args: &Map<String, Value>,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ErrorData> {
        // replace_symbol is a whole-symbol rewrite — delegate to the LSP/IDE-aware
        // ctx_refactor so there is one symbol-edit implementation (epic #1008).
        if crate::tools::ctx_patch::is_replace_symbol(args) {
            return delegate_replace_symbol(args, ctx);
        }

        let path = require_resolved_path(ctx, args, "path")?;

        let ops = crate::tools::ctx_patch::parse_ops(args)
            .map_err(|e| ErrorData::invalid_params(e, None))?;

        let expected_md5 = get_str(args, "expected_md5");
        let backup = get_bool(args, "backup").unwrap_or(false);
        let backup_path = get_str(args, "backup_path")
            .map(|p| ctx.resolved_paths.get("backup_path").cloned().unwrap_or(p));
        let evidence = get_bool(args, "evidence").unwrap_or(true);
        let diff_max_lines = get_int(args, "diff_max_lines")
            .and_then(|v| usize::try_from(v.max(0)).ok())
            .unwrap_or(200);
        let allow_lossy_utf8 = get_bool(args, "allow_lossy_utf8").unwrap_or(false);
        let validate_syntax = get_bool(args, "validate_syntax").unwrap_or(true);

        let patch_params = crate::tools::ctx_patch::PatchParams {
            path: path.clone(),
            ops,
            expected_md5,
            backup,
            backup_path,
            evidence,
            diff_max_lines,
            allow_lossy_utf8,
            validate_syntax,
        };

        tokio::task::block_in_place(|| {
            let cache_lock = ctx
                .cache
                .as_ref()
                .ok_or_else(|| ErrorData::internal_error("cache not available", None))?;
            let rt = tokio::runtime::Handle::current();

            // Serialize edits to the SAME file via the shared per-file lock (the
            // same registry ctx_edit/ctx_read use), so anchored and str_replace
            // edits of one file never interleave (issue #320). Correctness across
            // processes still rests on the TOCTOU preimage guard + atomic rename.
            let file_lock = crate::core::path_locks::per_file_lock(&path);
            let _file_guard = {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                loop {
                    if let Ok(guard) = file_lock.try_lock() {
                        break guard;
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(ErrorData::internal_error(
                            format!(
                                "per-file edit lock contention for {path} — another edit to the same file is in progress, retry in a moment"
                            ),
                            None,
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            };

            let last_mode = match rt.block_on(tokio::time::timeout(
                std::time::Duration::from_secs(5),
                cache_lock.read(),
            )) {
                Ok(cache) => cache
                    .get(&path)
                    .map(|e| e.last_mode.clone())
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };

            // Heavy disk I/O — no global cache lock held here.
            let (output, effect) = crate::tools::ctx_patch::run_io(&patch_params, &last_mode);

            crate::tools::ctx_patch::record_outcome(&patch_params, &last_mode, &output, &effect);

            if !matches!(effect, crate::tools::ctx_edit::CacheEffect::None) {
                match rt.block_on(tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    cache_lock.write(),
                )) {
                    Ok(mut cache) => {
                        crate::tools::ctx_edit::apply_cache_effect(&mut cache, &path, effect);
                    }
                    Err(_) => {
                        tracing::warn!(
                            "ctx_patch: cache write-lock timeout (5s) applying post-edit cache effect for {path}"
                        );
                    }
                }
            }

            if let Some(session_lock) = ctx.session.as_ref() {
                let guard = rt.block_on(tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    session_lock.write(),
                ));
                if let Ok(mut session) = guard {
                    session.mark_modified(&path);
                }
            }

            Ok(ToolOutput {
                text: output,
                original_tokens: 0,
                saved_tokens: 0,
                mode: None,
                path: Some(path),
                changed: false,
                shell_outcome: None,
            })
        })
    }
}

/// Handle `op="replace_symbol"` by translating to `ctx_refactor`'s
/// `replace_symbol_body` and dispatching through it. The symbol-resolution,
/// CONFLICT guard and atomic write all live in ctx_refactor — this is a thin,
/// pure-mapping adapter (mapping logic lives in `ctx_patch::symbol`).
fn delegate_replace_symbol(
    args: &Map<String, Value>,
    ctx: &ToolContext,
) -> Result<ToolOutput, ErrorData> {
    let refactor_args = crate::tools::ctx_patch::build_refactor_args(args)
        .map_err(|e| ErrorData::invalid_params(e, None))?;

    // Resolve `path` at the boundary when given (the name route resolves its own
    // path inside ctx_refactor). abs_path is unused by the symbol-edit branch but
    // we mirror ctx_refactor's wrapper to keep jail behaviour identical.
    let has_path = args.get("path").and_then(Value::as_str).is_some();
    let abs_path = if has_path {
        require_resolved_path(ctx, args, "path")?
    } else {
        String::new()
    };

    let args_value = Value::Object(refactor_args);
    let result = crate::tools::ctx_refactor::handle(&args_value, &ctx.project_root, &abs_path);
    let changed = !result.starts_with("ERROR") && !result.starts_with("CONFLICT");

    Ok(ToolOutput {
        text: result,
        original_tokens: 0,
        saved_tokens: 0,
        mode: Some("replace_symbol".to_string()),
        path: get_str(args, "path"),
        changed,
        shell_outcome: None,
    })
}
