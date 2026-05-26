//! Auto-generated config schema from `Config` struct metadata.
//!
//! Used by `lean-ctx config schema` to emit JSON and by
//! `lean-ctx config validate` to check user config.toml files.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSchema {
    pub version: u32,
    pub sections: BTreeMap<String, SectionSchema>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionSchema {
    pub description: String,
    pub keys: BTreeMap<String, KeySchema>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeySchema {
    #[serde(rename = "type")]
    pub ty: String,
    pub default: serde_json::Value,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_override: Option<String>,
}

fn clean_f32(v: f32) -> serde_json::Value {
    let clean: f64 = format!("{v}").parse().unwrap_or(v as f64);
    serde_json::json!(clean)
}

fn key(ty: &str, default: serde_json::Value, desc: &str) -> KeySchema {
    KeySchema {
        ty: ty.to_string(),
        default,
        description: desc.to_string(),
        values: None,
        env_override: None,
    }
}

fn key_enum(values: &[&str], default: &str, desc: &str) -> KeySchema {
    KeySchema {
        ty: "enum".to_string(),
        default: serde_json::Value::String(default.to_string()),
        description: desc.to_string(),
        values: Some(values.iter().map(ToString::to_string).collect()),
        env_override: None,
    }
}

fn key_with_env(ty: &str, default: serde_json::Value, desc: &str, env: &str) -> KeySchema {
    KeySchema {
        ty: ty.to_string(),
        default,
        description: desc.to_string(),
        values: None,
        env_override: Some(env.to_string()),
    }
}

fn key_enum_with_env(values: &[&str], default: &str, desc: &str, env: &str) -> KeySchema {
    KeySchema {
        ty: "enum".to_string(),
        default: serde_json::Value::String(default.to_string()),
        description: desc.to_string(),
        values: Some(values.iter().map(ToString::to_string).collect()),
        env_override: Some(env.to_string()),
    }
}

impl ConfigSchema {
    pub fn generate() -> Self {
        let cfg = super::Config::default();
        let mut sections = BTreeMap::new();

        let mut root = BTreeMap::new();
        root.insert(
            "ultra_compact".into(),
            key(
                "bool",
                serde_json::json!(false),
                "Legacy flag for maximum compression (use compression_level instead)",
            ),
        );
        root.insert(
            "tee_mode".into(),
            key_enum(
                &["never", "failures", "always"],
                "failures",
                "Controls when shell output is tee'd to disk for later retrieval",
            ),
        );
        root.insert(
            "output_density".into(),
            key_enum_with_env(
                &["normal", "terse", "ultra"],
                "normal",
                "Controls how dense/compact MCP tool output is formatted",
                "LEAN_CTX_OUTPUT_DENSITY",
            ),
        );
        root.insert(
            "checkpoint_interval".into(),
            key(
                "u32",
                serde_json::json!(cfg.checkpoint_interval),
                "Session checkpoint interval in minutes",
            ),
        );
        root.insert(
            "excluded_commands".into(),
            key(
                "string[]",
                serde_json::json!(cfg.excluded_commands),
                "Commands to exclude from shell hook interception",
            ),
        );
        root.insert(
            "passthrough_urls".into(),
            key(
                "string[]",
                serde_json::json!(cfg.passthrough_urls),
                "URLs to pass through without proxy interception",
            ),
        );
        root.insert("slow_command_threshold_ms".into(), key("u64", serde_json::json!(cfg.slow_command_threshold_ms), "Commands taking longer than this (ms) are recorded in the slow log. Set to 0 to disable"));
        root.insert(
            "theme".into(),
            key(
                "string",
                serde_json::json!(cfg.theme),
                "Dashboard color theme",
            ),
        );
        root.insert(
            "buddy_enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.buddy_enabled),
                "Enable the buddy system for multi-agent coordination",
            ),
        );
        root.insert(
            "enable_wakeup_ctx".into(),
            key(
                "bool",
                serde_json::json!(cfg.enable_wakeup_ctx),
                "Append wakeup briefing (facts, session summary) to ctx_overview output. Set false to reduce context bloat when calling ctx_overview frequently.",
            ),
        );
        root.insert(
            "redirect_exclude".into(),
            key(
                "string[]",
                serde_json::json!(cfg.redirect_exclude),
                "URL patterns to exclude from proxy redirection",
            ),
        );
        root.insert(
            "disabled_tools".into(),
            key(
                "string[]",
                serde_json::json!(cfg.disabled_tools),
                "Tools to exclude from the MCP tool list",
            ),
        );
        root.insert(
            "default_tool_categories".into(),
            key(
                "string[]",
                serde_json::json!(cfg.default_tool_categories),
                "Tool categories active by default (core, arch, debug, memory, metrics, session). Override via LCTX_DEFAULT_CATEGORIES",
            ),
        );
        root.insert(
            "no_degrade".into(),
            key(
                "boolean",
                serde_json::json!(cfg.no_degrade),
                "Disable all automatic read-mode degradation. Override via LCTX_NO_DEGRADE=1",
            ),
        );
        root.insert(
            "profile".into(),
            key(
                "string",
                serde_json::json!(cfg.profile.as_deref().unwrap_or("")),
                "Persistent profile name. Checked after LEAN_CTX_PROFILE env var. Set via: lean-ctx config set profile passthrough",
            ),
        );
        root.insert(
            "rules_scope".into(),
            key_enum(
                &["both", "global", "project"],
                "both",
                "Where agent rule files are installed. Override via LEAN_CTX_RULES_SCOPE",
            ),
        );
        root.insert(
            "extra_ignore_patterns".into(),
            key(
                "string[]",
                serde_json::json!(cfg.extra_ignore_patterns),
                "Extra glob patterns to ignore in graph/overview/preload",
            ),
        );
        root.insert(
            "terse_agent".into(),
            key_enum_with_env(
                &["off", "lite", "full", "ultra"],
                "off",
                "Controls agent output verbosity via instructions injection",
                "LEAN_CTX_TERSE_AGENT",
            ),
        );
        root.insert(
            "compression_level".into(),
            key_enum_with_env(
                &["off", "lite", "standard", "max"],
                "off",
                "Unified compression level for all output",
                "LEAN_CTX_COMPRESSION",
            ),
        );
        root.insert(
            "allow_paths".into(),
            key_with_env(
                "string[]",
                serde_json::json!(cfg.allow_paths),
                "Additional paths allowed by PathJail (absolute)",
                "LEAN_CTX_ALLOW_PATH",
            ),
        );
        root.insert(
            "extra_roots".into(),
            key_with_env(
                "string[]",
                serde_json::json!(cfg.extra_roots),
                "Extra project roots for multi-root workspaces (auto-added to PathJail allow-list)",
                "LEAN_CTX_EXTRA_ROOTS",
            ),
        );
        root.insert(
            "content_defined_chunking".into(),
            key(
                "bool",
                serde_json::json!(false),
                "Enable Rabin-Karp chunking for cache-optimal output ordering",
            ),
        );
        root.insert(
            "minimal_overhead".into(),
            key_with_env(
                "bool",
                serde_json::json!(false),
                "Skip session/knowledge/gotcha blocks in MCP instructions",
                "LEAN_CTX_MINIMAL",
            ),
        );
        root.insert(
            "shell_hook_disabled".into(),
            key_with_env(
                "bool",
                serde_json::json!(false),
                "Disable shell hook injection",
                "LEAN_CTX_NO_HOOK",
            ),
        );
        root.insert(
            "shell_activation".into(),
            key_enum_with_env(
                &["always", "agents-only", "off"],
                "always",
                "Controls when the shell hook auto-activates aliases",
                "LEAN_CTX_SHELL_ACTIVATION",
            ),
        );
        root.insert(
            "update_check_disabled".into(),
            key_with_env(
                "bool",
                serde_json::json!(false),
                "Disable the daily version check",
                "LEAN_CTX_NO_UPDATE_CHECK",
            ),
        );
        root.insert(
            "bm25_max_cache_mb".into(),
            key_with_env(
                "u64",
                serde_json::json!(cfg.bm25_max_cache_mb),
                "Maximum BM25 cache file size in MB",
                "LEAN_CTX_BM25_MAX_CACHE_MB",
            ),
        );
        root.insert(
            "graph_index_max_files".into(),
            key(
                "u64",
                serde_json::json!(cfg.graph_index_max_files),
                "Maximum files in graph index. 0 = unlimited (default). Set >0 to cap for constrained systems",
            ),
        );
        root.insert(
            "memory_profile".into(),
            key_enum_with_env(
                &["low", "balanced", "performance"],
                "balanced",
                "Controls RAM vs feature trade-off",
                "LEAN_CTX_MEMORY_PROFILE",
            ),
        );
        root.insert(
            "memory_cleanup".into(),
            key_enum_with_env(
                &["aggressive", "shared"],
                "aggressive",
                "Controls how aggressively memory is freed when idle",
                "LEAN_CTX_MEMORY_CLEANUP",
            ),
        );
        root.insert(
            "savings_footer".into(),
            key_enum_with_env(
                &["auto", "always", "never"],
                "never",
                "Controls visibility of token savings footers: never (default, suppress everywhere), always, auto (context-dependent)",
                "LEAN_CTX_SAVINGS_FOOTER",
            ),
        );
        root.insert(
            "max_ram_percent".into(),
            key_with_env(
                "u8",
                serde_json::json!(cfg.max_ram_percent),
                "Maximum percentage of system RAM that lean-ctx may use (1-50, default 5)",
                "LEAN_CTX_MAX_RAM_PERCENT",
            ),
        );
        root.insert(
            "max_disk_mb".into(),
            key_with_env(
                "u64",
                serde_json::json!(cfg.max_disk_mb),
                "Simplified disk budget in MB (0 = disabled). Distributes: archive ~25%, BM25 ~10%",
                "LEAN_CTX_MAX_DISK_MB",
            ),
        );
        root.insert(
            "max_staleness_days".into(),
            key_with_env(
                "u32",
                serde_json::json!(cfg.max_staleness_days),
                "Auto-purge data older than N days (0 = disabled). Flows into archive.max_age_hours",
                "LEAN_CTX_MAX_STALENESS_DAYS",
            ),
        );
        root.insert(
            "project_root".into(),
            key_with_env(
                "string?",
                serde_json::json!(null),
                "Explicit project root directory. Prevents accidental home-directory scans",
                "LEAN_CTX_PROJECT_ROOT",
            ),
        );
        root.insert(
            "proxy_enabled".into(),
            key(
                "bool?",
                serde_json::json!(null),
                "Enable/disable the proxy layer. null = auto-detect, true = force on, false = force off",
            ),
        );
        root.insert(
            "proxy_port".into(),
            key(
                "u16?",
                serde_json::json!(null),
                "Custom proxy port (default: 4444). Useful for multi-user systems. Env: LEAN_CTX_PROXY_PORT",
            ),
        );
        root.insert(
            "proxy_timeout_ms".into(),
            key(
                "u64?",
                serde_json::json!(null),
                "Proxy reachability timeout in ms (default: 200). Override via LEAN_CTX_PROXY_TIMEOUT_MS",
            ),
        );
        root.insert(
            "response_verbosity".into(),
            key_enum_with_env(
                &["normal", "compact", "minimal"],
                "normal",
                "Controls how verbose tool responses are",
                "LEAN_CTX_RESPONSE_VERBOSITY",
            ),
        );
        root.insert(
            "allow_auto_reroot".into(),
            key_with_env(
                "bool",
                serde_json::json!(false),
                "Allow automatic project-root re-rooting when absolute paths outside the jail are seen",
                "LEAN_CTX_ALLOW_REROOT",
            ),
        );
        root.insert(
            "sandbox_level".into(),
            key_with_env(
                "u8",
                serde_json::json!(0),
                "Sandbox strictness level (0=default, 1=strict, 2=paranoid)",
                "LEAN_CTX_SANDBOX_LEVEL",
            ),
        );
        root.insert(
            "reference_results".into(),
            key_with_env(
                "bool",
                serde_json::json!(false),
                "Store large tool outputs as references instead of inline content",
                "LEAN_CTX_REFERENCE_RESULTS",
            ),
        );
        root.insert(
            "agent_token_budget".into(),
            key(
                "usize",
                serde_json::json!(0),
                "Default per-agent token budget. 0 = unlimited",
            ),
        );
        root.insert(
            "shell_allowlist".into(),
            key_with_env(
                "array",
                serde_json::json!([]),
                "Optional shell command allowlist. When non-empty, only listed binaries are permitted",
                "LEAN_CTX_SHELL_ALLOWLIST",
            ),
        );

        sections.insert(
            "root".into(),
            SectionSchema {
                description: "Top-level configuration keys".into(),
                keys: root,
            },
        );

        sections.insert(
            "ide_paths".into(),
            SectionSchema {
                description: "Per-IDE allowed paths. Keys are agent names (cursor, codex, opencode, antigravity, etc.), values are arrays of paths to index for that agent".into(),
                keys: BTreeMap::new(),
            },
        );

        let mut lsp_keys = BTreeMap::new();
        lsp_keys.insert(
            "rust".into(),
            key(
                "string?",
                serde_json::json!(null),
                "Custom path to rust-analyzer binary",
            ),
        );
        lsp_keys.insert(
            "typescript".into(),
            key(
                "string?",
                serde_json::json!(null),
                "Custom path to typescript-language-server binary",
            ),
        );
        lsp_keys.insert(
            "python".into(),
            key(
                "string?",
                serde_json::json!(null),
                "Custom path to pylsp binary",
            ),
        );
        lsp_keys.insert(
            "go".into(),
            key(
                "string?",
                serde_json::json!(null),
                "Custom path to gopls binary",
            ),
        );
        sections.insert(
            "lsp".into(),
            SectionSchema {
                description: "LSP server binary overrides. Map language name to custom binary path"
                    .into(),
                keys: lsp_keys,
            },
        );

        let mut archive = BTreeMap::new();
        archive.insert(
            "enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.archive.enabled),
                "Enable zero-loss compression archive",
            ),
        );
        archive.insert(
            "threshold_chars".into(),
            key(
                "usize",
                serde_json::json!(cfg.archive.threshold_chars),
                "Minimum output size (chars) to trigger archiving",
            ),
        );
        archive.insert(
            "max_age_hours".into(),
            key(
                "u64",
                serde_json::json!(cfg.archive.max_age_hours),
                "Maximum age of archived entries before cleanup",
            ),
        );
        archive.insert(
            "max_disk_mb".into(),
            key(
                "u64",
                serde_json::json!(cfg.archive.max_disk_mb),
                "Maximum total disk usage for the archive",
            ),
        );
        sections.insert("archive".into(), SectionSchema {
            description: "Settings for the zero-loss compression archive (large tool outputs saved to disk)".into(),
            keys: archive,
        });

        let mut autonomy = BTreeMap::new();
        autonomy.insert(
            "enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.autonomy.enabled),
                "Enable autonomous background behaviors",
            ),
        );
        autonomy.insert(
            "auto_preload".into(),
            key(
                "bool",
                serde_json::json!(cfg.autonomy.auto_preload),
                "Auto-preload related files on first read",
            ),
        );
        autonomy.insert(
            "auto_dedup".into(),
            key(
                "bool",
                serde_json::json!(cfg.autonomy.auto_dedup),
                "Auto-deduplicate repeated reads",
            ),
        );
        autonomy.insert(
            "auto_related".into(),
            key(
                "bool",
                serde_json::json!(cfg.autonomy.auto_related),
                "Auto-load graph-related files",
            ),
        );
        autonomy.insert(
            "auto_consolidate".into(),
            key(
                "bool",
                serde_json::json!(cfg.autonomy.auto_consolidate),
                "Auto-consolidate knowledge periodically",
            ),
        );
        autonomy.insert(
            "silent_preload".into(),
            key(
                "bool",
                serde_json::json!(cfg.autonomy.silent_preload),
                "Suppress preload notifications in output",
            ),
        );
        autonomy.insert(
            "dedup_threshold".into(),
            key(
                "usize",
                serde_json::json!(cfg.autonomy.dedup_threshold),
                "Number of repeated reads before dedup triggers",
            ),
        );
        autonomy.insert(
            "consolidate_every_calls".into(),
            key(
                "u32",
                serde_json::json!(cfg.autonomy.consolidate_every_calls),
                "Consolidate knowledge every N tool calls",
            ),
        );
        autonomy.insert(
            "consolidate_cooldown_secs".into(),
            key(
                "u64",
                serde_json::json!(cfg.autonomy.consolidate_cooldown_secs),
                "Minimum seconds between consolidation runs",
            ),
        );
        sections.insert(
            "autonomy".into(),
            SectionSchema {
                description:
                    "Controls autonomous background behaviors (preload, dedup, consolidation)"
                        .into(),
                keys: autonomy,
            },
        );

        let mut providers = BTreeMap::new();
        providers.insert(
            "enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.providers.enabled),
                "Master switch for the provider subsystem (GitHub, GitLab, etc.)",
            ),
        );
        providers.insert(
            "auto_index".into(),
            key(
                "bool",
                serde_json::json!(cfg.providers.auto_index),
                "Auto-ingest provider results into BM25/embedding indexes",
            ),
        );
        providers.insert(
            "cache_ttl_secs".into(),
            key(
                "u64",
                serde_json::json!(cfg.providers.cache_ttl_secs),
                "Default cache TTL for provider results (seconds)",
            ),
        );
        providers.insert(
            "github.enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.providers.github.enabled),
                "Enable/disable GitHub provider",
            ),
        );
        providers.insert(
            "github.api_url".into(),
            key(
                "string",
                serde_json::json!(cfg.providers.github.api_url),
                "GitHub API base URL (for GitHub Enterprise)",
            ),
        );
        providers.insert(
            "gitlab.enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.providers.gitlab.enabled),
                "Enable/disable GitLab provider",
            ),
        );
        providers.insert(
            "gitlab.api_url".into(),
            key(
                "string",
                serde_json::json!(cfg.providers.gitlab.api_url),
                "GitLab API base URL (for self-hosted instances)",
            ),
        );
        providers.insert(
            "mcp_bridges.<name>.url".into(),
            key(
                "string",
                serde_json::json!(null),
                "HTTP/SSE URL for a remote MCP server",
            ),
        );
        providers.insert(
            "mcp_bridges.<name>.command".into(),
            key(
                "string",
                serde_json::json!(null),
                "Command to spawn a local MCP server (stdio transport)",
            ),
        );
        providers.insert(
            "mcp_bridges.<name>.args".into(),
            key(
                "array",
                serde_json::json!([]),
                "Arguments for the MCP server command",
            ),
        );
        providers.insert(
            "mcp_bridges.<name>.auth_env".into(),
            key(
                "string",
                serde_json::json!(null),
                "Environment variable name containing auth token for MCP server",
            ),
        );
        sections.insert(
            "providers".into(),
            SectionSchema {
                description:
                    "External context providers (GitHub, GitLab, Jira, MCP bridges, etc.). Set tokens via env vars (GITHUB_TOKEN, GITLAB_TOKEN). MCP bridges connect external MCP servers as context sources."
                        .into(),
                keys: providers,
            },
        );

        let mut loop_det = BTreeMap::new();
        loop_det.insert(
            "normal_threshold".into(),
            key(
                "u32",
                serde_json::json!(cfg.loop_detection.normal_threshold),
                "Repetitions before reducing output",
            ),
        );
        loop_det.insert(
            "reduced_threshold".into(),
            key(
                "u32",
                serde_json::json!(cfg.loop_detection.reduced_threshold),
                "Repetitions before further reducing output",
            ),
        );
        loop_det.insert(
            "blocked_threshold".into(),
            key(
                "u32",
                serde_json::json!(cfg.loop_detection.blocked_threshold),
                "Repetitions before blocking. 0 = disabled",
            ),
        );
        loop_det.insert(
            "window_secs".into(),
            key(
                "u64",
                serde_json::json!(cfg.loop_detection.window_secs),
                "Time window in seconds for loop detection",
            ),
        );
        loop_det.insert(
            "search_group_limit".into(),
            key(
                "u32",
                serde_json::json!(cfg.loop_detection.search_group_limit),
                "Maximum unique searches within a loop window",
            ),
        );
        loop_det.insert(
            "tool_total_limits".into(),
            key(
                "table",
                serde_json::json!({"ctx_read": 100, "ctx_search": 80, "ctx_shell": 50, "ctx_semantic_search": 60}),
                "Per-tool total call limits within a session. Keys are tool names, values are max calls",
            ),
        );
        sections.insert(
            "loop_detection".into(),
            SectionSchema {
                description: "Loop detection settings for preventing repeated identical tool calls"
                    .into(),
                keys: loop_det,
            },
        );

        let mut updates = BTreeMap::new();
        updates.insert(
            "auto_update".into(),
            key(
                "bool",
                serde_json::json!(cfg.updates.auto_update),
                "Enable automatic updates (requires explicit opt-in)",
            ),
        );
        updates.insert(
            "check_interval_hours".into(),
            key(
                "u64",
                serde_json::json!(cfg.updates.check_interval_hours),
                "How often to check for updates (hours)",
            ),
        );
        updates.insert(
            "notify_only".into(),
            key(
                "bool",
                serde_json::json!(cfg.updates.notify_only),
                "Only notify about updates, don't install automatically",
            ),
        );
        sections.insert(
            "updates".into(),
            SectionSchema {
                description: "Automatic update configuration".into(),
                keys: updates,
            },
        );

        let mut boundary = BTreeMap::new();
        boundary.insert(
            "cross_project_search".into(),
            key(
                "bool",
                serde_json::json!(cfg.boundary_policy.cross_project_search),
                "Allow searching across project boundaries",
            ),
        );
        boundary.insert(
            "cross_project_import".into(),
            key(
                "bool",
                serde_json::json!(cfg.boundary_policy.cross_project_import),
                "Allow importing knowledge from other projects",
            ),
        );
        boundary.insert(
            "audit_cross_access".into(),
            key(
                "bool",
                serde_json::json!(cfg.boundary_policy.audit_cross_access),
                "Log audit events when cross-project access occurs",
            ),
        );
        boundary.insert(
            "universal_gotchas_enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.boundary_policy.universal_gotchas_enabled),
                "Load universal (cross-project) gotchas",
            ),
        );
        sections.insert(
            "boundary_policy".into(),
            SectionSchema {
                description: "Cross-project boundary and access control policies".into(),
                keys: boundary,
            },
        );

        let mut secret_det = BTreeMap::new();
        secret_det.insert(
            "enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.secret_detection.enabled),
                "Enable secret/credential detection in tool outputs",
            ),
        );
        secret_det.insert(
            "redact".into(),
            key(
                "bool",
                serde_json::json!(cfg.secret_detection.redact),
                "Redact detected secrets from output",
            ),
        );
        secret_det.insert(
            "custom_patterns".into(),
            key(
                "array",
                serde_json::json!(cfg.secret_detection.custom_patterns),
                "Additional regex patterns to detect as secrets",
            ),
        );
        sections.insert(
            "secret_detection".into(),
            SectionSchema {
                description: "Secret/credential detection and redaction settings".into(),
                keys: secret_det,
            },
        );

        let mut cloud = BTreeMap::new();
        cloud.insert(
            "contribute_enabled".into(),
            key(
                "bool",
                serde_json::json!(cfg.cloud.contribute_enabled),
                "Enable contributing anonymized stats to lean-ctx cloud",
            ),
        );
        sections.insert(
            "cloud".into(),
            SectionSchema {
                description: "Cloud feature settings".into(),
                keys: cloud,
            },
        );

        let mut proxy = BTreeMap::new();
        proxy.insert(
            "anthropic_upstream".into(),
            key(
                "string?",
                serde_json::json!(cfg.proxy.anthropic_upstream),
                "Custom upstream URL for Anthropic API proxy",
            ),
        );
        proxy.insert(
            "openai_upstream".into(),
            key(
                "string?",
                serde_json::json!(cfg.proxy.openai_upstream),
                "Custom upstream URL for OpenAI API proxy",
            ),
        );
        proxy.insert(
            "gemini_upstream".into(),
            key(
                "string?",
                serde_json::json!(cfg.proxy.gemini_upstream),
                "Custom upstream URL for Gemini API proxy",
            ),
        );
        sections.insert(
            "proxy".into(),
            SectionSchema {
                description: "Proxy upstream configuration for API routing".into(),
                keys: proxy,
            },
        );

        let mem = &cfg.memory;
        let mut mem_knowledge = BTreeMap::new();
        mem_knowledge.insert(
            "max_facts".into(),
            key(
                "usize",
                serde_json::json!(mem.knowledge.max_facts),
                "Maximum number of knowledge facts stored per project",
            ),
        );
        mem_knowledge.insert(
            "max_patterns".into(),
            key(
                "usize",
                serde_json::json!(mem.knowledge.max_patterns),
                "Maximum number of patterns stored",
            ),
        );
        mem_knowledge.insert(
            "max_history".into(),
            key(
                "usize",
                serde_json::json!(mem.knowledge.max_history),
                "Maximum history entries retained",
            ),
        );
        mem_knowledge.insert(
            "contradiction_threshold".into(),
            key(
                "f32",
                clean_f32(mem.knowledge.contradiction_threshold),
                "Confidence threshold for contradiction detection",
            ),
        );
        mem_knowledge.insert(
            "recall_facts_limit".into(),
            key(
                "usize",
                serde_json::json!(mem.knowledge.recall_facts_limit),
                "Maximum facts returned per recall query",
            ),
        );
        mem_knowledge.insert(
            "rooms_limit".into(),
            key(
                "usize",
                serde_json::json!(mem.knowledge.rooms_limit),
                "Maximum number of rooms returned",
            ),
        );
        mem_knowledge.insert(
            "timeline_limit".into(),
            key(
                "usize",
                serde_json::json!(mem.knowledge.timeline_limit),
                "Maximum number of timeline entries returned",
            ),
        );
        mem_knowledge.insert(
            "relations_limit".into(),
            key(
                "usize",
                serde_json::json!(mem.knowledge.relations_limit),
                "Maximum number of relations returned",
            ),
        );
        sections.insert(
            "memory.knowledge".into(),
            SectionSchema {
                description: "Knowledge memory budgets (facts, patterns, gotchas)".into(),
                keys: mem_knowledge,
            },
        );

        let mut mem_episodic = BTreeMap::new();
        mem_episodic.insert(
            "max_episodes".into(),
            key(
                "usize",
                serde_json::json!(mem.episodic.max_episodes),
                "Maximum number of episodes retained",
            ),
        );
        mem_episodic.insert(
            "max_actions_per_episode".into(),
            key(
                "usize",
                serde_json::json!(mem.episodic.max_actions_per_episode),
                "Maximum actions tracked per episode",
            ),
        );
        mem_episodic.insert(
            "summary_max_chars".into(),
            key(
                "usize",
                serde_json::json!(mem.episodic.summary_max_chars),
                "Maximum characters in episode summary",
            ),
        );
        sections.insert(
            "memory.episodic".into(),
            SectionSchema {
                description: "Episodic memory budgets (session episodes)".into(),
                keys: mem_episodic,
            },
        );

        let mut mem_procedural = BTreeMap::new();
        mem_procedural.insert(
            "max_procedures".into(),
            key(
                "usize",
                serde_json::json!(mem.procedural.max_procedures),
                "Maximum number of learned procedures stored",
            ),
        );
        mem_procedural.insert(
            "min_repetitions".into(),
            key(
                "usize",
                serde_json::json!(mem.procedural.min_repetitions),
                "Minimum repetitions before a pattern is stored",
            ),
        );
        mem_procedural.insert(
            "min_sequence_len".into(),
            key(
                "usize",
                serde_json::json!(mem.procedural.min_sequence_len),
                "Minimum sequence length for procedure detection",
            ),
        );
        mem_procedural.insert(
            "max_window_size".into(),
            key(
                "usize",
                serde_json::json!(mem.procedural.max_window_size),
                "Maximum window size for pattern analysis",
            ),
        );
        sections.insert(
            "memory.procedural".into(),
            SectionSchema {
                description: "Procedural memory budgets (learned patterns)".into(),
                keys: mem_procedural,
            },
        );

        let mut mem_lifecycle = BTreeMap::new();
        mem_lifecycle.insert(
            "decay_rate".into(),
            key(
                "f32",
                clean_f32(mem.lifecycle.decay_rate),
                "Rate at which knowledge confidence decays over time",
            ),
        );
        mem_lifecycle.insert(
            "low_confidence_threshold".into(),
            key(
                "f32",
                clean_f32(mem.lifecycle.low_confidence_threshold),
                "Threshold below which facts are considered low-confidence",
            ),
        );
        mem_lifecycle.insert(
            "stale_days".into(),
            key(
                "i64",
                serde_json::json!(mem.lifecycle.stale_days),
                "Days after which unused facts are considered stale",
            ),
        );
        mem_lifecycle.insert(
            "similarity_threshold".into(),
            key(
                "f32",
                clean_f32(mem.lifecycle.similarity_threshold),
                "Similarity threshold for deduplication",
            ),
        );
        sections.insert(
            "memory.lifecycle".into(),
            SectionSchema {
                description: "Knowledge lifecycle policy (decay, staleness, dedup)".into(),
                keys: mem_lifecycle,
            },
        );

        let mut mem_gotcha = BTreeMap::new();
        mem_gotcha.insert(
            "max_gotchas_per_project".into(),
            key(
                "usize",
                serde_json::json!(mem.gotcha.max_gotchas_per_project),
                "Maximum gotchas stored per project",
            ),
        );
        mem_gotcha.insert(
            "retrieval_budget_per_room".into(),
            key(
                "usize",
                serde_json::json!(mem.gotcha.retrieval_budget_per_room),
                "Maximum gotchas retrieved per room per query",
            ),
        );
        mem_gotcha.insert(
            "default_decay_rate".into(),
            key(
                "f32",
                clean_f32(mem.gotcha.default_decay_rate),
                "Default decay rate for gotcha importance",
            ),
        );
        sections.insert(
            "memory.gotcha".into(),
            SectionSchema {
                description: "Gotcha memory settings (project-specific warnings and pitfalls)"
                    .into(),
                keys: mem_gotcha,
            },
        );

        let mut mem_embeddings = BTreeMap::new();
        mem_embeddings.insert(
            "max_facts".into(),
            key(
                "usize",
                serde_json::json!(mem.embeddings.max_facts),
                "Maximum number of embedding facts stored",
            ),
        );
        sections.insert(
            "memory.embeddings".into(),
            SectionSchema {
                description: "Embeddings memory settings for semantic search".into(),
                keys: mem_embeddings,
            },
        );

        let mut aliases = BTreeMap::new();
        aliases.insert(
            "command".into(),
            key(
                "string",
                serde_json::json!(""),
                "The command pattern to match (e.g. 'deploy')",
            ),
        );
        aliases.insert(
            "alias".into(),
            key(
                "string",
                serde_json::json!(""),
                "The alias definition to execute",
            ),
        );
        sections.insert("custom_aliases".into(), SectionSchema {
            description: "Custom command aliases (array of {command, alias} entries). Note: field names are 'command' and 'alias' (not 'name')".into(),
            keys: aliases,
        });

        if let Some(root_section) = sections.get_mut("root") {
            root_section.keys.insert(
                "custom_aliases".into(),
                key(
                    "array",
                    serde_json::json!([]),
                    "Custom command aliases (array of {command, alias} entries)",
                ),
            );
        }

        ConfigSchema {
            version: 1,
            sections,
        }
    }

    /// All known TOML keys (dot-separated) for validation.
    pub fn known_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for (section, schema) in &self.sections {
            if section == "root" {
                for key_name in schema.keys.keys() {
                    keys.push(key_name.clone());
                }
            } else {
                if schema.keys.is_empty() {
                    keys.push(section.clone());
                }
                for key_name in schema.keys.keys() {
                    keys.push(format!("{section}.{key_name}"));
                }
            }
        }
        keys
    }
}
