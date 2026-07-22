use serde_json::Value;

#[allow(clippy::wildcard_imports)]
use super::super::shared::*;
use super::super::{WriteAction, WriteOptions, WriteResult};
use crate::core::editor_registry::types::EditorTarget;

// ---------------------------------------------------------------------------
// OpenClaw writer (GitHub #390)
//
// OpenClaw changed its config schema in 2026.6.1: MCP servers moved from a
// top-level `mcpServers` (camelCase) object to a nested `mcp.servers` object,
// and the new validator *rejects* unknown top-level keys — a re-injected
// `mcpServers` block makes every hot-reload fail ("Unrecognized key") and, if
// it wins on restart, takes the gateway down.
//
// Strategy:
//   - Detect the installed version via `meta.lastTouchedVersion`.
//   - >= 2026.6.1 (or unknown/missing): write nested `mcp.servers` and
//     migrate away our legacy `mcpServers.lean-ctx` entry (dropping the
//     `mcpServers` key entirely once it is empty).
//   - < 2026.6.1: keep writing the legacy camelCase schema.
//   - Idempotent: if the entry already matches and no stale legacy entry
//     exists, nothing is written (no watchdog reload-tick churn).
// ---------------------------------------------------------------------------

/// First OpenClaw version that requires the nested `mcp.servers` schema.
const OPENCLAW_NESTED_SCHEMA_VERSION: (u64, u64, u64) = (2026, 6, 1);

/// Parse an OpenClaw version string ("2026.6.1") into a comparable triple.
/// Tolerates missing components ("2026.6" -> (2026, 6, 0)) and pre-release
/// suffixes ("2026.6.1-beta.2" -> (2026, 6, 1)).
pub(crate) fn parse_openclaw_version(raw: &str) -> Option<(u64, u64, u64)> {
    let core = raw.trim().split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.trim().parse::<u64>().ok()?;
    let minor = parts
        .next()
        .and_then(|p| p.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|p| p.trim().parse::<u64>().ok())
        .unwrap_or(0);
    Some((major, minor, patch))
}

/// Whether this OpenClaw config requires the nested `mcp.servers` schema.
///
/// Defaults to nested when the version is unknown: current OpenClaw releases
/// are all >= 2026.6.1, the legacy key actively breaks them, and a fresh
/// install has no `meta` block at all. An existing `mcp.servers` object is
/// also treated as proof of the new schema (the user or OpenClaw itself
/// migrated already).
pub(crate) fn openclaw_uses_nested_schema(root: &serde_json::Map<String, Value>) -> bool {
    if root
        .get("mcp")
        .and_then(|m| m.get("servers"))
        .is_some_and(Value::is_object)
    {
        return true;
    }
    let version = root
        .get("meta")
        .and_then(|m| m.get("lastTouchedVersion"))
        .and_then(Value::as_str)
        .and_then(parse_openclaw_version);
    match version {
        Some(v) => v >= OPENCLAW_NESTED_SCHEMA_VERSION,
        None => true,
    }
}

/// Remove our legacy top-level `mcpServers.lean-ctx` entry. Drops the whole
/// `mcpServers` key when it becomes empty (OpenClaw >= 2026.6.1 rejects even
/// an empty unknown key). Foreign entries under `mcpServers` are preserved.
/// Returns true when the document was modified.
pub(crate) fn remove_legacy_openclaw_entry(root: &mut serde_json::Map<String, Value>) -> bool {
    let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) else {
        return false;
    };
    if servers.remove("lean-ctx").is_none() {
        return false;
    }
    if servers.is_empty() {
        root.remove("mcpServers");
    }
    true
}

pub(crate) fn write_openclaw_config(
    target: &EditorTarget,
    binary: &str,
    _opts: WriteOptions,
) -> Result<WriteResult, String> {
    let desired = serde_json::json!({
        "command": binary
    });

    if !target.config_path.exists() {
        let content = serde_json::to_string_pretty(&serde_json::json!({
            "mcp": { "servers": { "lean-ctx": desired } }
        }))
        .map_err(|e| e.to_string())?;
        crate::config_io::write_atomic_with_backup(&target.config_path, &content)?;
        return Ok(WriteResult {
            action: WriteAction::Created,
            note: None,
        });
    }

    let content = std::fs::read_to_string(&target.config_path).map_err(|e| e.to_string())?;
    let mut json = match crate::core::jsonc::parse_jsonc(&content) {
        Ok(v) => v,
        Err(_e) => {
            // Never text-inject into openclaw.json: the nested `mcp.servers`
            // shape cannot be patched safely with flat text injection, and a
            // malformed write would take the strict 2026.6.1 validator (and
            // with it the gateway) down. `allow_inject=false` keeps the
            // existing "already present? -> skip, else -> clear error" flow.
            return handle_invalid_json_write(
                &target.config_path,
                &content,
                "mcp",
                "lean-ctx",
                &desired,
                false,
            );
        }
    };
    let root = json
        .as_object_mut()
        .ok_or_else(|| "root JSON must be an object".to_string())?;

    if !openclaw_uses_nested_schema(root) {
        // Legacy OpenClaw (< 2026.6.1): keep the camelCase schema it expects.
        let servers = root
            .entry("mcpServers")
            .or_insert_with(|| serde_json::json!({}));
        let servers_obj = servers
            .as_object_mut()
            .ok_or_else(|| "\"mcpServers\" must be an object".to_string())?;
        if servers_obj.get("lean-ctx") == Some(&desired) {
            return Ok(WriteResult {
                action: WriteAction::Already,
                note: None,
            });
        }
        servers_obj.insert("lean-ctx".to_string(), desired);
        let formatted = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
        crate::config_io::write_atomic_with_backup(&target.config_path, &formatted)?;
        return Ok(WriteResult {
            action: WriteAction::Updated,
            note: Some("legacy mcpServers schema (OpenClaw < 2026.6.1)".to_string()),
        });
    }

    let migrated_legacy = remove_legacy_openclaw_entry(root);

    let mcp = root.entry("mcp").or_insert_with(|| serde_json::json!({}));
    let mcp_obj = mcp
        .as_object_mut()
        .ok_or_else(|| "\"mcp\" must be an object".to_string())?;
    let servers = mcp_obj
        .entry("servers")
        .or_insert_with(|| serde_json::json!({}));
    let servers_obj = servers
        .as_object_mut()
        .ok_or_else(|| "\"mcp.servers\" must be an object".to_string())?;

    let entry_current = servers_obj.get("lean-ctx") == Some(&desired);
    if entry_current && !migrated_legacy {
        return Ok(WriteResult {
            action: WriteAction::Already,
            note: None,
        });
    }
    servers_obj.insert("lean-ctx".to_string(), desired);

    let formatted = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    crate::config_io::write_atomic_with_backup(&target.config_path, &formatted)?;
    Ok(WriteResult {
        action: WriteAction::Updated,
        note: migrated_legacy
            .then(|| "migrated legacy mcpServers entry to mcp.servers".to_string()),
    })
}
