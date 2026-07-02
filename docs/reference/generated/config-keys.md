# Appendix — Configuration Keys (generated)

<!-- GENERATED FILE — do not edit by hand. Run: `cargo run --example gen_docs --features dev-tools` -->

Source of truth: `rust/src/core/config/schema.rs`.

lean-ctx reads `~/.lean-ctx/config.toml` (and a project `.lean-ctx.toml` overlay). Below is every recognized key with its type, default, and environment-variable override where one exists.

## Top-level keys

Top-level configuration keys

- `agent_token_budget` (usize, default `0`) — Default per-agent token budget. 0 = unlimited
- `allow_auto_reroot` (bool, default `false` — env `LEAN_CTX_ALLOW_REROOT`) — Allow automatic project-root re-rooting when absolute paths outside the jail are seen
- `allow_ide_config_dirs` (bool, default `null` — env `LEAN_CTX_ALLOW_IDE_DIRS`) — Allow jailed ctx_* tools to read home-level IDE config dirs (registry-derived; covers all editors). Off by default — exposes other agents' sessions/credentials
- `allow_paths` (string[], default `[]` — env `LEAN_CTX_ALLOW_PATH`) — Additional paths allowed by PathJail (absolute)
- `allow_symlink_roots` (string[], default `[]` — env `LEAN_CTX_ALLOW_SYMLINK_ROOTS`) — Trusted roots OUTSIDE $HOME lean-ctx may follow when an agent config is symlinked there (#596). Empty = strict $HOME-only
- `auto_capture` (bool, default `true`) — Automatic knowledge capture from tool findings
- `auto_mode_learning` (bool, default `false` — env `LEAN_CTX_AUTO_MODE_LEARNING`) — Opt-in: let adaptive learning signals (predictor, bandit, heatmap, adaptive policy, bounce/path memory) influence `auto` mode. Off by default for a deterministic, I/O-light cascade (capability guards + size/task heuristic only) that keeps output byte-stable for prompt caching. Override via LEAN_CTX_AUTO_MODE_LEARNING
- `bm25_max_cache_mb` (u64, default `128` — env `LEAN_CTX_BM25_MAX_CACHE_MB`) — Maximum BM25 cache file size in MB
- `buddy_enabled` (bool, default `true`) — Enable the buddy system for multi-agent coordination
- `bypass_hints` (enum: on | off | aggressive, default `on` — env `LEAN_CTX_BYPASS_HINTS`) — Bypass-hint mode: when agents use native Read/Grep instead of lean-ctx tools, a hint is appended to the next tool response. on (default), off, aggressive (hint on every call, no cooldown). Override via LEAN_CTX_BYPASS_HINTS
- `cache_max_tokens` (usize, default `0` — env `LEAN_CTX_CACHE_MAX_TOKENS`) — Token budget for the in-memory ctx_read cache (0 = built-in default 500k). When exceeded, least-valuable entries are evicted immediately via RRF (recency x frequency x size) so reads never block; eviction is not deferred to the staleness TTL
- `cache_policy` (enum(aggressive|safe|off), default `aggressive` — env `LEAN_CTX_CACHE_POLICY`) — Cache policy for ctx_read: aggressive (13-tok stubs), safe (map on hit), off (always disk)
- `checkpoint_interval` (u32, default `15`) — Session checkpoint interval in minutes
- `compression_aggressiveness` (f64, default `null` — env `LEAN_CTX_AGGRESSIVENESS`) — Global compression intensity 0.0 (lossless) – 1.0 (max), mapped onto read modes/entropy/IB. Empty = per-mode defaults
- `compression_level` (enum: off | lite | standard | max, default `lite` — env `LEAN_CTX_COMPRESSION`) — Unified output-style level for the model's prose (not tool-output compression). lite=plain concise (default), standard/max=denser symbolic 'power modes'
- `content_defined_chunking` (bool, default `false`) — Enable Rabin-Karp chunking for cache-optimal output ordering
- `crush_verbatim_json` (bool, default `false` — env `LEAN_CTX_CRUSH_VERBATIM_JSON`) — Opt-in: losslessly crush array-heavy JSON from verbatim data commands (gh api, jq, kubectl get -o json, curl). Off by default keeps them verbatim. Reshapes only when it at least halves the payload; fully reconstructible
- `custom_aliases` (array, default `[]`) — Custom command aliases (array of {command, alias} entries)
- `dashboard_auth` (bool, default `true`) — Require Bearer-token auth for the dashboard (default true). Set false for no-auth mode protected by Sec-Fetch-Site/Origin/Host checks. Override per-run with --no-auth or LEAN_CTX_DASHBOARD_AUTH
- `debug_log` (bool, default `false` — env `LEAN_CTX_DEBUG_LOG`) — Opt-in (default off): write a human-readable debug log of intercepted MCP tool calls and hook routing decisions (lean-ctx vs native, with the reason) to <state_dir>/logs/debug.log. View with `lean-ctx debug-log`
- `default_tool_categories` (string[], default `[]`) — Tool categories active by default (core, arch, debug, memory, metrics, session). Override via LCTX_DEFAULT_CATEGORIES
- `delta_explicit` (boolean, default `false`) — Serve explicit full/lines re-reads of changed cached files as diffs (opt-in). Override via LCTX_DELTA_EXPLICIT=1
- `disabled_tools` (string[], default `[]`) — Tools to exclude from the MCP tool list
- `enable_wakeup_ctx` (bool, default `true`) — Append wakeup briefing (facts, session summary) to ctx_overview output. Set false to reduce context bloat when calling ctx_overview frequently.
- `excluded_commands` (string[], default `[]`) — Commands to exclude from shell hook interception
- `extra_ignore_patterns` (string[], default `[]`) — Extra glob patterns to ignore in graph/overview/preload
- `extra_roots` (string[], default `[]` — env `LEAN_CTX_EXTRA_ROOTS`) — Extra project roots for multi-root workspaces (auto-added to PathJail allow-list)
- `graph_index_max_files` (u64, default `0`) — Maximum files in graph index. 0 = unlimited (default). Set >0 to cap for constrained systems
- `journal_enabled` (bool, default `true`) — Write human-readable activity journal to ~/.lean-ctx/journal.md
- `max_disk_mb` (u64, default `0` — env `LEAN_CTX_MAX_DISK_MB`) — Simplified disk budget in MB (0 = disabled). Distributes: archive ~25%, BM25 ~10%
- `max_index_threads` (usize, default `0` — env `LEANCTX_INDEX_THREADS`) — Cap rayon threads for the CPU-heavy index build (0 = all cores). Bounds per-instance CPU so concurrent sessions don't saturate the host on startup
- `max_ram_percent` (u8, default `5` — env `LEAN_CTX_MAX_RAM_PERCENT`) — Maximum percentage of system RAM that lean-ctx may use (1-50, default 5)
- `max_staleness_days` (u32, default `0` — env `LEAN_CTX_MAX_STALENESS_DAYS`) — Auto-purge data older than N days (0 = disabled). Flows into archive.max_age_hours
- `memory_cleanup` (enum: aggressive | shared, default `aggressive` — env `LEAN_CTX_MEMORY_CLEANUP`) — Controls how aggressively memory is freed when idle
- `memory_profile` (enum: low | balanced | performance, default `performance` — env `LEAN_CTX_MEMORY_PROFILE`) — Controls RAM vs feature trade-off (performance = max quality)
- `minimal_overhead` (bool, default `true` — env `LEAN_CTX_MINIMAL`) — Skip session/knowledge/gotcha blocks in MCP instructions
- `no_degrade` (boolean, default `false`) — Disable all automatic read-mode degradation. Override via LCTX_NO_DEGRADE=1
- `output_density` (enum: normal | terse | ultra, default `normal` — env `LEAN_CTX_OUTPUT_DENSITY`) — Controls how dense/compact MCP tool output is formatted
- `passthrough_urls` (string[], default `[]`) — URLs to pass through without proxy interception
- `path_jail` (bool?, default `null`) — Filesystem path jail. null/true = enforced (tools confined to the project root + allow_paths). false = the blanket "any path" opt-out — every tool path is allowed (for containers/sandboxes where the boundary is external). Compression and secret redaction are unaffected. Flip both planes at once with `lean-ctx yolo` / `lean-ctx secure`
- `permission_inheritance` (enum: off | on, default `off`) — Mirror the host IDE's permission rules onto lean-ctx tools (v1: OpenCode). When on, ctx_shell honors your bash/rm * rules instead of bypassing them. Override via LEAN_CTX_PERMISSION_INHERITANCE
- `persona` (string, default `coding` — env `LEAN_CTX_PERSONA`) — Active context persona (persona-spec-v1): selects the domain bundle — tool surface, read-mode/compressor/chunker defaults, intent taxonomy, sensitivity floor. Built-ins: coding (default), research, lead-gen, support, data-analysis; or a custom <name>.toml from the personas dir. Override via LEAN_CTX_PERSONA
- `prefer_native_editor` (bool, default `false`) — Disable lean-ctx edit tools (ctx_edit) so the host's native editor handles edits (#454)
- `preserve_compact_formats` (string[], default `["toon"]`) — Already-compact output formats preserved verbatim instead of recompressed (e.g. ["toon"]). Set to [] to disable
- `profile` (string, default `""`) — Persistent profile name. Checked after LEAN_CTX_PROFILE env var. Set via: lean-ctx config set profile passthrough
- `project_root` (string?, default `null` — env `LEAN_CTX_PROJECT_ROOT`) — Explicit project root directory. Prevents accidental home-directory scans
- `proxy_enabled` (bool?, default `null`) — Enable/disable the proxy layer. null = auto-detect, true = force on, false = force off
- `proxy_port` (u16?, default `null`) — Custom proxy port (default: 4444). Useful for multi-user systems. Env: LEAN_CTX_PROXY_PORT
- `proxy_require_token` (bool, default `false`) — Require lean-ctx Bearer token authentication and disable provider API key fallback
- `proxy_timeout_ms` (u64?, default `null`) — Proxy reachability timeout in ms (default: 200). Override via LEAN_CTX_PROXY_TIMEOUT_MS
- `read_dedup` (enum: auto | on | off, default `auto` — env `LEAN_CTX_READ_DEDUP`) — Controls the PostToolUse native-Read re-read dedup. auto (default): replace only re-reads of unchanged files with the compact stub, and only on guard hosts (Claude Code / CodeBuddy) where the PreToolUse redirect is off — first reads stay byte-identical and the read-before-write guard is untouched. on: dedup wherever the hook fires. off: never replace a Read result
- `read_only_roots` (string[], default `[]` — env `LEAN_CTX_READ_ONLY_ROOTS`) — Read-only sibling roots: reads allowed, writes always denied (edit/refactor/export)
- `read_redirect` (enum: auto | on | off, default `auto` — env `LEAN_CTX_READ_REDIRECT`) — Controls the native-Read → ctx_read redirect hook. auto (default): redirect everywhere except hosts with a native read-before-write guard (Claude Code / CodeBuddy), where rewriting Read to a temp copy breaks native Write/Edit (#637). on: always redirect. off: never redirect native Read (ctx_read MCP tool + Grep/Glob redirect stay active)
- `recovery_hints` (enum: off | minimal | full, default `minimal`) — Verbosity of the reactive recovery footer on compressed output (path-first, MCP-optional)
- `redirect_exclude` (string[], default `[]`) — URL patterns to exclude from proxy redirection
- `reference_results` (bool, default `false` — env `LEAN_CTX_REFERENCE_RESULTS`) — Store large tool outputs as references instead of inline content
- `response_verbosity` (enum: normal | compact | minimal, default `normal` — env `LEAN_CTX_RESPONSE_VERBOSITY`) — Controls how verbose tool responses are
- `rules_injection` (enum: shared | dedicated | off, default `shared`) — How rules load for CLAUDE.md/AGENTS.md/GEMINI.md agents: shared block, dedicated (no shared-file edits; SessionStart hook / instructions[] / context.fileName), or off (write no rules file — for hosts that supply their own steering or phase-isolated/non-caching harnesses). Override via LEAN_CTX_RULES_INJECTION
- `rules_scope` (enum: both | global | project, default `both`) — Where agent rule files are installed. Override via LEAN_CTX_RULES_SCOPE
- `sandbox_level` (u8, default `0` — env `LEAN_CTX_SANDBOX_LEVEL`) — Sandbox strictness level (0=default, 1=strict, 2=paranoid)
- `savings_footer` (enum: auto | always | never, default `always` — env `LEAN_CTX_SAVINGS_FOOTER`) — Controls visibility of token savings footers: always (default, show on every response), never, auto (context-dependent). Also: LEAN_CTX_SHOW_SAVINGS=1|0
- `shadow_mode` (bool, default `false` — env `LEAN_CTX_SHADOW_MODE`) — Opt-in (default off): transparently route native Read/Grep/Edit/Shell through lean-ctx — via hooks for hook-based agents, via the interception plugin for OpenCode
- `shell_activation` (enum: always | agents-only | off, default `always` — env `LEAN_CTX_SHELL_ACTIVATION`) — Controls when the shell hook auto-activates aliases
- `shell_allow_writes` (bool, default `false` — env `LEAN_CTX_SHELL_ALLOW_WRITES`) — Allow ctx_shell file-write redirects (>, >>, tee, heredoc-to-file, curl -o, wget default mode). Default false — prefer the native Write/Edit tool. The real command gating (allowlist, dangerous-pattern, interpreter-eval) still applies
- `shell_allowlist` (array, default `[]` — env `LEAN_CTX_SHELL_ALLOWLIST`) — Optional shell command allowlist. When non-empty, only listed binaries are permitted
- `shell_allowlist_extra` (array, default `[]`) — Commands merged on top of shell_allowlist without replacing the defaults. Managed via `lean-ctx allow <cmd>`
- `shell_heavy_timeout_secs` (u64?, default `null` — env `LEAN_CTX_SHELL_HEAVY_TIMEOUT_SECS`) — Shell command timeout (seconds) for heavy commands (cargo build/test, make, docker build, git commit/push). null = built-in 10-minute ceiling
- `shell_hook_disabled` (bool, default `false` — env `LEAN_CTX_NO_HOOK`) — Disable shell hook injection
- `shell_security` (string, default `enforce` — env `LEAN_CTX_SHELL_SECURITY`) — Shell command gating: enforce (default, secure), warn (log only, never block) or off (skip allowlist + hard blocks; compression stays active)
- `shell_strict_mode` (bool, default `false`) — Block $(), backticks, <() in shell arguments. Default false = warn only.
- `shell_timeout_secs` (u64?, default `null` — env `LEAN_CTX_SHELL_TIMEOUT_SECS`) — Shell command timeout (seconds) for normal commands. null = built-in 2-minute default. LEAN_CTX_SHELL_TIMEOUT_MS overrides both tiers (in ms)
- `slow_command_threshold_ms` (u64, default `5000`) — Commands taking longer than this (ms) are recorded in the slow log. Set to 0 to disable
- `structure_first` (bool, default `false` — env `LEAN_CTX_STRUCTURE_FIRST`) — Opt-in: bias `auto` toward structure-first reads (map) for medium code files on a cold read. Off by default — for phase-isolated harnesses with no warm-session cache payback. Override via LEAN_CTX_STRUCTURE_FIRST
- `symbol_map_auto` (bool, default `false`) — Opt-in: α-code identifier substitution in aggressive reads (>50-file projects). Off by default — abbreviated symbols hinder editing/refactoring
- `team_auto_push` (bool, default `false`) — Opt-in: daemon periodically pushes your signed savings batch to team_url (off by default; requires team_url + team_token)
- `team_token` (string?, default `null`) — Bearer token for the team server (push needs a member token; pull/auto-push needs the configured team token)
- `team_url` (string?, default `null`) — Team server base URL for the opt-in savings roll-up (push/pull)
- `tee_mode` (enum: never | failures | highcompression | always, default `highcompression`) — Controls when shell output is tee'd to disk for later retrieval
- `terse_agent` (enum: off | lite | full | ultra, default `off` — env `LEAN_CTX_TERSE_AGENT`) — Controls agent output verbosity via instructions injection
- `theme` (string, default `default`) — Dashboard color theme
- `tool_profile` (enum: minimal | standard | power, default `""`) — Tool visibility profile: minimal (5 tools), standard (15), power (all). Override via LEAN_CTX_TOOL_PROFILE
- `tools_enabled` (string[], default `[]`) — Explicit list of enabled tool names. Used only when no tool_profile is pinned (tool_profile takes precedence); leave tool_profile unset to apply this list. The universal invoker ctx_call stays advertised so unlisted tools remain reachable — add it to disabled_tools (disabled_tools = ["ctx_call"]) to make this allowlist authoritative.
- `ultra_compact` (bool, default `false`) — Legacy flag for maximum compression (use compression_level instead)
- `update_check_disabled` (bool, default `false` — env `LEAN_CTX_NO_UPDATE_CHECK`) — Disable the daily version check

## `[addons]`

Addon ecosystem security floor: install policy, signature requirement, per-addon capability sandbox (#863, P1). Global-only.

- `allow_bootstrap` (bool, default `true`) — Allow `addon add` to install an addon's upstream package via a pinned manager (uv/pip/cargo/npm/brew/dotnet). Off = refuse bootstrap installs
- `allowlist` (array, default `[]`) — Addon slugs permitted when policy = allowlist
- `block_risky` (bool, default `false`) — Refuse to install an addon that has a high-risk (Danger) capability
- `enforce_capabilities` (bool, default `false`) — Fail closed when an addon declares restricted [capabilities] but no OS sandbox launcher is available to enforce them
- `metering` (bool, default `true`) — Record per-addon / per-tool gateway usage to <data_dir>/addons/usage.json (analytics + billing base)
- `policy` (enum: open | verified_only | allowlist | locked, default `open`) — Addon install policy: open (any) | verified_only | allowlist | locked
- `require_signature` (bool, default `false`) — Honour a user-override registry only if signed by a trusted org key
- `sandbox` (enum: off | auto | strict, default `off`) — Sandbox spawned addon stdio servers: off | auto (block network) | strict (read-only fs + refuse if no launcher)

## `[archive]`

Settings for the zero-loss compression archive (large tool outputs saved to disk)

- `enabled` (bool, default `true`) — Enable zero-loss compression archive
- `ephemeral` (bool, default `true`) — Replace large results with summary+ref (ctx_expand to retrieve). Env: LEAN_CTX_EPHEMERAL
- `ephemeral_min_tokens` (usize, default `2000`) — Minimum output tokens before the ephemeral firewall replaces inline body with summary+ref. Env: LEAN_CTX_EPHEMERAL_MIN_TOKENS
- `max_age_hours` (u64, default `48`) — Maximum age of archived entries before cleanup
- `max_disk_mb` (u64, default `500`) — Maximum total disk usage for the archive
- `threshold_chars` (usize, default `800`) — Minimum output size (chars) to trigger archiving

## `[autonomy]`

Controls autonomous background behaviors (preload, dedup, consolidation)

- `auto_consolidate` (bool, default `true`) — Auto-consolidate knowledge periodically
- `auto_dedup` (bool, default `true`) — Auto-deduplicate repeated reads
- `auto_preload` (bool, default `true`) — Auto-preload related files on first read
- `auto_related` (bool, default `true`) — Auto-load graph-related files
- `cognition_loop_enabled` (bool, default `true` — env `LEAN_CTX_COGNITION_LOOP_ENABLED`) — Enable the background cognition loop (periodic knowledge consolidation)
- `cognition_loop_interval_secs` (u64, default `3600` — env `LEAN_CTX_COGNITION_LOOP_INTERVAL_SECS`) — Seconds between cognition loop iterations
- `cognition_loop_max_steps` (u8, default `9` — env `LEAN_CTX_COGNITION_LOOP_MAX_STEPS`) — Maximum steps per cognition loop iteration (>= 9 enables observation synthesis)
- `cognition_synthesis_min_cluster` (usize, default `3` — env `LEAN_CTX_COGNITION_SYNTHESIS_MIN_CLUSTER`) — Minimum facts per entity before observation synthesis writes a summary (needs cognition_loop_max_steps >= 9)
- `consolidate_cooldown_secs` (u64, default `120`) — Minimum seconds between consolidation runs
- `consolidate_every_calls` (u32, default `25`) — Consolidate knowledge every N tool calls
- `dedup_threshold` (usize, default `8`) — Number of repeated reads before dedup triggers
- `enabled` (bool, default `true`) — Enable autonomous background behaviors
- `silent_preload` (bool, default `true`) — Suppress preload notifications in output

## `[boundary_policy]`

Cross-project boundary and access control policies

- `audit_cross_access` (bool, default `true`) — Log audit events when cross-project access occurs
- `cross_project_import` (bool, default `false`) — Allow importing knowledge from other projects
- `cross_project_search` (bool, default `false`) — Allow searching across project boundaries
- `universal_gotchas_enabled` (bool, default `true`) — Load universal (cross-project) gotchas

## `[cloud]`

Cloud feature settings

- `auto_sync` (bool, default `false`) — Push the Personal Cloud (knowledge, commands, CEP, gotchas, buddy, feedback) silently once per day at session end (Pro; toggle: `lean-ctx cloud autosync on|off`)
- `contribute_enabled` (bool, default `false`) — Enable contributing anonymized stats to lean-ctx cloud

## `[context]`

Fixed-context budget accounting (#964)

- `budget_tokens` (usize, default `8000` — env `LEAN_CTX_CONTEXT_BUDGET_TOKENS`) — Fixed per-session context budget (tool schemas + MCP instructions + auto-loaded rules + wakeup briefing). `doctor overhead` warns past this; `doctor overhead --gate` exits non-zero for CI. 0 disables the warning

## `[cost]`

Model declaration for measured-vs-estimated cost reporting

- `default_model` (string?, default `null`) — Fallback pricing model for MCP-only IDEs whose real model lean-ctx cannot observe (Cursor, Copilot, Windsurf, …). Unset → blended heuristic. Per-IDE overrides live in [cost.models]

## `[custom_aliases]`

Custom command aliases (array of {command, alias} entries). Note: field names are 'command' and 'alias' (not 'name')

- `alias` (string, default `""`) — The alias definition to execute
- `command` (string, default `""`) — The command pattern to match (e.g. 'deploy')

## `[embedding]`

Semantic-embedding engine settings (model selection for ctx_semantic_search)

- `auto_download` (bool, default `null` — env `LEAN_CTX_EMBEDDINGS_AUTO_DOWNLOAD`) — Download the embedding model in the background on first semantic need (default: allowed). Set false for air-gapped machines; semantic features then stay off until a model is provided manually.
- `deterministic` (bool, default `null` — env `LEAN_CTX_EMBEDDING_DETERMINISTIC`) — Pin embedding inference to a single CPU thread with no GPU provider so vectors are bit-identical across machines (default: off, multi-threaded GPU-capable path). Extractive prose ranking is already deterministic via score quantization; enable this only for cross-machine reproducibility, at a throughput cost.
- `dimensions` (integer, default `null`) — Declared embedding width for hf: custom models (fallback only — the real width is probed from the ONNX graph at load time). Built-in models ignore this key.
- `model` (string, default `minilm` — env `LEAN_CTX_EMBEDDING_MODEL`) — Local ONNX embedding model for ctx_semantic_search. One of: minilm (all-MiniLM-L6-v2, 384d, default), nomic (768d) — or any HuggingFace repo with an ONNX export via hf:org/repo[@revision] (e.g. hf:jinaai/jina-embeddings-v2-base-code for code). Switching models re-indexes once on the next search.

## `[gain]`

Token-savings recap publishing (gain --publish / auto-publish)

- `auto_publish` (bool, default `false`) — Automatically (re)publish your Wrapped recap when you run `lean-ctx gain` (opt-in, off by default; throttled and sends only an aggregate payload)
- `auto_publish_interval_hours` (u64, default `24`) — Minimum hours between automatic publishes (throttle; default 24)
- `display_name` (string?, default `null`) — Optional display name shown on your published card / leaderboard entry
- `last_auto_publish` (string?, default `null`) — Timestamp of the last automatic publish (written by lean-ctx for throttling — not meant to be edited)
- `leaderboard` (bool, default `true`) — When auto-publishing, also list the card on the public opt-in leaderboard

## `[gateway]`

MCP Tool-Catalog Gateway: aggregate + query-route downstream MCP servers (#210). Global-only.

- `cache_ttl_secs` (integer, default `300`) — Aggregated-catalog cache lifetime in seconds
- `call_timeout_secs` (integer, default `30`) — Per-operation timeout for downstream connect/list/call (seconds)
- `enabled` (bool, default `false`) — Enable the MCP Tool-Catalog Gateway (no-op when false)
- `top_n` (integer, default `5`) — How many tools `ctx_tools find` returns per query (clamped 1..=50)

## `[gateway.servers]`

Downstream MCP servers (array of tables: `[[gateway.servers]]`)

- `args` (array, default `[]`) — Arguments for the spawned command (stdio transport)
- `command` (string, default `""`) — Executable to spawn (stdio transport)
- `enabled` (bool, default `true`) — Per-server switch (default true)
- `env` (table, default `{}`) — Extra environment variables for the child process (stdio transport)
- `headers` (table, default `{}`) — Extra request headers, e.g. Authorization (http transport)
- `name` (string, default `""`) — Stable server id; becomes the catalog namespace (`name::tool`)
- `transport` (string, default `stdio`) — Transport: stdio (spawn command) or http (connect to url)
- `url` (string, default `""`) — Streamable-HTTP endpoint (http transport)

## `[graph]`

Code-graph settings, including traversal (co-access) edges learned from sessions

- `traversal_edges` (bool, default `true`) — Learn co-access edges from real sessions (files surfaced together), surface them as decaying `co_access` graph edges, and boost recall by them. Set false for a purely static AST-only graph.

## `[ide_paths]`

Per-IDE allowed paths. Keys are agent names (cursor, codex, opencode, antigravity, etc.), values are arrays of paths to index for that agent

_No sub-keys (presence of the section toggles the feature)._

## `[llm]`

Optional LLM enhancement settings (query expansion, contradiction explanation). Deterministic fallback when disabled or unreachable.

- `api_key` (string, default `""`) — API key for OpenRouter or Anthropic backends
- `backend` (enum: ollama | openrouter | anthropic, default `ollama`) — LLM backend provider
- `enabled` (bool, default `false`) — Enable optional LLM enhancements (query expansion, contradiction explanation)
- `model` (string, default `llama3.2`) — Model name for the selected backend
- `timeout_secs` (u64, default `10`) — HTTP timeout for LLM requests

## `[loop_detection]`

Loop detection settings for preventing repeated identical tool calls

- `blocked_threshold` (u32, default `0`) — Repetitions before blocking. 0 = disabled
- `normal_threshold` (u32, default `2`) — Repetitions before reducing output
- `reduced_threshold` (u32, default `4`) — Repetitions before further reducing output
- `search_group_limit` (u32, default `10`) — Maximum unique searches within a loop window
- `tool_total_limits` (table, default `{"ctx_read":100,"ctx_search":80,"ctx_semantic_search":60,"ctx_shell":50}`) — Per-tool total call limits within a session. Keys are tool names, values are max calls
- `window_secs` (u64, default `300`) — Time window in seconds for loop detection

## `[lsp]`

LSP server binary overrides. Map language name to custom binary path

- `go` (string?, default `null`) — Custom path to gopls binary
- `python` (string?, default `null`) — Custom path to pylsp binary
- `rust` (string?, default `null`) — Custom path to rust-analyzer binary
- `typescript` (string?, default `null`) — Custom path to typescript-language-server binary

## `[memory.embeddings]`

Embeddings memory settings for semantic search

- `max_facts` (usize, default `2000`) — Maximum number of embedding facts stored

## `[memory.episodic]`

Episodic memory budgets (session episodes)

- `max_actions_per_episode` (usize, default `50`) — Maximum actions tracked per episode
- `max_episodes` (usize, default `500`) — Maximum number of episodes retained
- `summary_max_chars` (usize, default `200`) — Maximum characters in episode summary

## `[memory.gotcha]`

Gotcha memory settings (project-specific warnings and pitfalls)

- `default_decay_rate` (f32, default `0.03`) — Default decay rate for gotcha importance
- `max_gotchas_per_project` (usize, default `100`) — Maximum gotchas stored per project
- `retrieval_budget_per_room` (usize, default `10`) — Maximum gotchas retrieved per room per query

## `[memory.knowledge]`

Knowledge memory budgets (facts, patterns, gotchas)

- `contradiction_threshold` (f32, default `0.5`) — Confidence threshold for contradiction detection
- `max_facts` (usize, default `200`) — Maximum number of knowledge facts stored per project
- `max_history` (usize, default `100`) — Maximum history entries retained
- `max_patterns` (usize, default `50`) — Maximum number of patterns stored
- `recall_facts_limit` (usize, default `10`) — Maximum facts returned per recall query
- `relations_limit` (usize, default `40`) — Maximum number of relations returned
- `rooms_limit` (usize, default `25`) — Maximum number of rooms returned
- `timeline_limit` (usize, default `25`) — Maximum number of timeline entries returned

## `[memory.lifecycle]`

Knowledge lifecycle policy (decay, staleness, dedup)

- `archetype_aware_decay` (bool, default `false`) — Scale Ebbinghaus stability by fact archetype so structural evidence decays slower than inference (default false)
- `base_stability_days` (f32, default `90.0`) — Characteristic memory stability (days) for the Ebbinghaus curve
- `decay_rate` (f32, default `0.01`) — Rate at which knowledge confidence decays over time
- `forgetting_model` (string, default `ebbinghaus`) — Forgetting curve: ebbinghaus (default, exponential + spacing) or linear
- `low_confidence_threshold` (f32, default `0.3`) — Threshold below which facts are considered low-confidence
- `reclaim_enabled` (bool, default `true` — env `LEAN_CTX_LIFECYCLE_RECLAIM_ENABLED`) — Master switch for the proactive capacity reclaim (#995). false trims only the overflow (escape hatch, no headroom); eviction stays lossless either way
- `reclaim_headroom_pct` (f32, default `0.25` — env `LEAN_CTX_LIFECYCLE_RECLAIM_HEADROOM_PCT`) — Proactive headroom on a capacity reclaim: settle a full store at 1 - this fraction (0.25 = 75%) instead of churning at the cap. Lossless — the reclaimed tail is archived and restorable
- `similarity_threshold` (f32, default `0.85`) — Similarity threshold for deduplication
- `stale_days` (i64, default `30`) — Days after which unused facts are considered stale

## `[memory.procedural]`

Procedural memory budgets (learned patterns)

- `max_procedures` (usize, default `100`) — Maximum number of learned procedures stored
- `max_window_size` (usize, default `10`) — Maximum window size for pattern analysis
- `min_repetitions` (usize, default `3`) — Minimum repetitions before a pattern is stored
- `min_sequence_len` (usize, default `2`) — Minimum sequence length for procedure detection

## `[providers]`

External context providers (GitHub, GitLab, Jira, MCP bridges, etc.). Set tokens via env vars (GITHUB_TOKEN, GITLAB_TOKEN). MCP bridges connect external MCP servers as context sources.

- `auto_index` (bool, default `true`) — Auto-ingest provider results into BM25/embedding indexes
- `cache_ttl_secs` (u64, default `120`) — Default cache TTL for provider results (seconds)
- `enabled` (bool, default `true`) — Master switch for the provider subsystem (GitHub, GitLab, etc.)
- `github.api_url` (string, default `null`) — GitHub API base URL (for GitHub Enterprise)
- `github.enabled` (bool, default `true`) — Enable/disable GitHub provider
- `gitlab.api_url` (string, default `null`) — GitLab API base URL (for self-hosted instances)
- `gitlab.enabled` (bool, default `true`) — Enable/disable GitLab provider
- `mcp_bridges.<name>.args` (array, default `[]`) — Arguments for the MCP server command
- `mcp_bridges.<name>.auth_env` (string, default `null`) — Environment variable name containing auth token for MCP server
- `mcp_bridges.<name>.command` (string, default `null`) — Command to spawn a local MCP server (stdio transport)
- `mcp_bridges.<name>.url` (string, default `null`) — HTTP/SSE URL for a remote MCP server

## `[proxy]`

Proxy upstream configuration for API routing

- `allow_custom_upstream` (bool, default `false` — env `LEAN_CTX_ALLOW_CUSTOM_UPSTREAM`) — Allow a custom (non-allowlisted) HTTPS upstream host, e.g. a corporate gateway in front of the provider API. Opt-in; default false. Unlike the env var, this config flag reaches the managed (service-spawned) proxy started by `proxy enable`/`restart` (#590)
- `allow_insecure_http_upstream` (bool, default `false` — env `LEAN_CTX_ALLOW_INSECURE_HTTP_UPSTREAM`) — Allow a non-loopback plaintext http:// upstream (trusted local network only, e.g. http://host.docker.internal:2455 in front of codex-lb). Opt-in; default false
- `anthropic_upstream` (string?, default `null`) — Custom upstream URL for Anthropic API proxy
- `cache_align_relocate` (bool, default `false` — env `LEAN_CTX_PROXY_CACHE_ALIGN_RELOCATE`) — Opt-in active cache-aligner relocate (#974). When on, the proxy rewrites an unanchored Anthropic system prompt into a stable block (volatile values - ISO dates/datetimes, UUIDs, git SHAs - replaced by constant placeholders) carrying the cache_control breakpoint, plus an uncached trailing block that re-states the relocated values. The cacheable prefix then stays byte-stable turn-to-turn and finally caches; only the small tail is reprocessed. Anthropic-only, Treatment-arm, gated on a client that anchored nothing and on Anthropic's minimum cacheable size. Deterministic (#498) and idempotent. The cache_aligner telemetry is the precursor that quantifies the saving. Default false
- `cache_aligner` (bool, default `true` — env `LEAN_CTX_PROXY_CACHE_ALIGNER`) — Cache-aligner volatile-field telemetry (#940), on by default. The proxy scans each unanchored Anthropic system prompt for volatile, cache-busting fields (ISO dates/datetimes, UUIDs, git SHAs) and reports how many it found on /status cache_safety (volatile_system_requests, volatile_fields_detected) - purely to quantify how much prompt-cache the client leaks. Measurement only: the request body is never mutated, so it is strictly cache-safe, which is why it ships on for every proxy (#986 premium defaults). The deterministic scan is the precursor to the opt-in tail-relocate below. Set false to opt out of the per-request scan. Default true
- `cache_breakpoint` (bool, default `false` — env `LEAN_CTX_PROXY_CACHE_BREAKPOINT`) — Opt-in active prompt-cache breakpoint injection for Anthropic (#939). When on and the client set no cache_control of its own, the proxy adds one cache_control: {type:ephemeral} marker to the system field so an otherwise-uncached, stable system prompt bills later turns at the cached rate (the win a raw API client leaves on the table). Anthropic-only: OpenAI/Gemini cache prefixes automatically and ignore the marker, so those paths stay byte-unchanged. Deterministic, never adds a second breakpoint, and skipped below Anthropic's minimum cacheable size. Default false
- `cache_policy` (bool, default `true` — env `LEAN_CTX_PROXY_CACHE_POLICY`) — Cache-economics (#986), on by default. Enables prompt-cache miss attribution telemetry (per turn, classify the outcome as cold start / warm reuse / TTL lapse / prefix change and report cumulative gauges on /status cache_attribution) plus a net-cost gate on the cold-prefix repack that skips re-seeding prefixes too small to be cached (below Anthropic's ~1024-token minimum). The telemetry never mutates the body and the gate only makes repacking more conservative, so it can never bust a cache that would otherwise have been kept - both halves are strictly safe, so every proxy gets them out of the box (#986 premium defaults). Set false to opt out (drops the /status attribution gauges and the per-request prefix hash). Default true
- `ccr_inband` (bool, default `false` — env `LEAN_CTX_PROXY_CCR_INBAND`) — Opt-in in-band CCR retrieval for a remote proxy with no shared filesystem (#493). When on, a lossy stub advertises a compact <lc_expand:HASH> marker instead of a local tee path; when the model echoes that marker, the proxy splices the verbatim original (from its local tee store) back inline next turn — one turn of latency, no MCP/filesystem on the agent host. The splice is a strict no-op on marker-less turns, so it never perturbs the provider cache prefix unless the model asked to expand. Default false
- `chatgpt_upstream` (string?, default `null`) — Custom upstream URL for ChatGPT/Codex subscription API proxy
- `codex_chatgpt_proxy` (bool, default `false` — env `LEAN_CTX_CODEX_CHATGPT_PROXY`) — Opt-in routing of a Codex ChatGPT-subscription login through the proxy for model-turn compression (#603/#616). Default false leaves Codex native (history visible, cloud/remote intact, no #597). When true, setup pins model_provider = leanctx-chatgpt + chatgpt_base_url + a [model_providers.leanctx-chatgpt] block, so model turns route through /backend-api/codex/responses (the proxy strips the responses-lite marker so every model incl. gpt-5.5 works); pinning a provider scopes Codex history to it (#597), hence opt-in. Toggle durably with `lean-ctx proxy codex-chatgpt on|off|status`. Default false
- `cold_prefix_repack` (bool, default `false` — env `LEAN_CTX_PROXY_COLD_PREFIX_REPACK`) — Opt-in big-gap cold-prefix repack (#480): on a session-resume request the proxy may predict (from idle time vs the provider cache TTL) that the client-cached prefix has already expired, then prune that now-cold prefix to re-seed a leaner cache and keep applying the same deterministic compression on later turns so warm follow-ups hit it (sticky; baselines persist across restarts, #499). A wrong guess re-bills cache reads as writes (~12x), so default false
- `compress_protect` (string[], default `[]`) — File-path globs whose reads are never compressed (#1150): a matching path is returned verbatim (full) by the read tools, for files where exact bytes matter more than token savings (golden snapshots, byte-asserted fixtures, security-sensitive configs). Globs (*/**/?) match the path and its file name, so *.snap, **/golden/**, tests/fixtures/* all work. Empty (default) protects nothing — the lossless crushers and beneficial gate already keep compression safe; this is an explicit escape hatch
- `effort` (enum: off | minimal | low | medium | high, default `off` — env `LEAN_CTX_PROXY_EFFORT`) — Cache-safe cross-provider reasoning-effort control (#834). off (default) = no-op. minimal|low|medium|high pins the model's reasoning depth across providers: lean-ctx translates it to OpenAI reasoning_effort / reasoning.effort, Anthropic output_config.effort, and Gemini thinkingConfig (thinkingLevel on 3.x, thinkingBudget on 2.5 pro/flash), only on models that accept it and only when the client didn't set its own value. The level is a constant, so it never breaks the provider prompt cache (unlike per-turn effort routing). Anthropic is dialed only when the client already requested adaptive thinking
- `gemini_upstream` (string?, default `null`) — Custom upstream URL for Gemini API proxy
- `history_mode` (enum: cache-aware | rolling | off, default `cache-aware` — env `LEAN_CTX_PROXY_HISTORY_MODE`) — History pruning strategy. cache-aware: frozen boundaries that keep provider prompt caches valid (default). rolling: legacy moving window (max raw savings, breaks prompt caching). off: never prune
- `live_compress` (bool, default `true` — env `LEAN_CTX_PROXY_LIVE_COMPRESS`) — Live-compress non-protected tool_result content on the wire (#481). Default true. Set false for a meter-only proxy — real billed/cache token metering with zero request rewriting (combine with history_mode = "off" and no role_aggressiveness for a byte-unchanged body)
- `live_compress_exclude` (string[], default `["serena"]`) — Tool-name patterns (case-insensitive substring) whose tool_result is never live-compressed — treated as protected, like a file read (#481). Unset protects Serena's code-reading tools; set an explicit list to narrow it, or [] to disable
- `meter_openai_usage` (bool, default `true`) — Inject stream_options.include_usage into streamed OpenAI Chat Completions so the final chunk reports real token usage for the measured spend meter. Default true
- `openai_upstream` (string?, default `null`) — Custom upstream URL for OpenAI API proxy
- `output_holdout` (f64, default `0.0` — env `LEAN_CTX_PROXY_OUTPUT_HOLDOUT`) — Fraction 0.0-1.0 of conversations placed in the output-savings control arm (#895). 0 (default) = no holdout (every conversation is output-shaped). When > 0, a deterministic cohort = blake3(system + first user message) puts ~this fraction in a control arm that skips output-shaping (effort control + verbosity steer) but is still metered, yielding an honest measured output-token reduction (lean-ctx output-savings). The cohort is a pure function of conversation identity, so a conversation keeps one arm across all turns - cache-safe
- `prose_ranker` (enum: auto | extractive | truncate, default `auto` — env `LEAN_CTX_PROXY_PROSE_RANKER`) — How the proxy squeezes prose it must shrink (#895). auto (default) and extractive use embedding-based extractive ranking — keeping the most central sentences instead of just the prefix — when the local embedding engine is available, else fall back to truncation; truncate keeps the original deterministic FIFO squeeze and never loads the engine. Wire rewrites are memoized per content so the engine's cold→warm transition never changes an already-emitted frozen-region rewrite (cache-safe, #448/#498)
- `verbosity_steer` (bool, default `false` — env `LEAN_CTX_PROXY_VERBOSITY_STEER`) — Opt-in cache-safe wire verbosity steer (#895). When true, the proxy appends a single constant 'be concise' instruction to the last user turn of each request - output-shaping for raw API clients that do not load lean-ctx rules. The suffix is constant and appended strictly after the last cache_control breakpoint (a new trailing text block, never modifying a cache-anchored block), so the provider prompt-cache prefix stays byte-stable. Under an output_holdout the control arm skips it so its effect is measured. Default false

## `[proxy.role_aggressiveness]`

Opt-in per-role prose compression for the proxy's frozen request region (#710). Assistant turns are always passed through verbatim

- `system` (f64, default `null` — env `LEAN_CTX_PROXY_SYSTEM_AGGR`) — Opt-in prose compression intensity (0.0–1.0) for system prompts in the proxy's frozen request region. Unset = leave untouched. Higher = more aggressive. Cache-safe (deterministic, never touches the client-cached prefix)
- `user` (f64, default `null` — env `LEAN_CTX_PROXY_USER_AGGR`) — Opt-in prose compression intensity (0.0–1.0) for free-text user turns (never tool results) in the proxy's frozen request region. Unset = leave untouched

## `[search]`

Hybrid search weights for ctx_semantic_search (BM25 + dense vector + SPLADE + graph proximity)

- `bm25_candidates` (usize, default `75`) — Number of BM25 candidates to retrieve before fusion
- `bm25_weight` (f64, default `1.0`) — BM25 lexical search weight in RRF fusion
- `dense_candidates` (usize, default `75`) — Number of dense candidates to retrieve before fusion
- `dense_enabled` (bool, default `true`) — Enable the dense (embedding) retrieval path. false → hybrid search ranks with BM25 + graph + rerank (+ SPLADE) only, skipping the embedding engine and the persistent embeddings.json (lighter footprint, no embed latency). An explicit mode=dense query still forces dense.
- `dense_weight` (f64, default `1.0`) — Dense vector search weight in RRF fusion
- `splade_weight` (f64, default `0.5`) — SPLADE expansion weight (0.0 to disable)

## `[secret_detection]`

Secret/credential detection and redaction settings

- `custom_patterns` (array, default `[]`) — Additional regex patterns to detect as secrets
- `enabled` (bool, default `true`) — Enable secret/credential detection in tool outputs
- `redact` (bool, default `true`) — Redact detected secrets from output

## `[sensitivity]`

Per-item sensitivity model with a uniform policy floor (#212)

- `action` (string, default `redact`) — How to enforce the floor: redact (mask spans) or drop (withhold item)
- `enabled` (bool, default `false`) — Enable the per-item sensitivity policy floor (no-op when false)
- `policy_floor` (string, default `secret`) — Block items at/above this level: public|internal|confidential|secret

## `[setup]`

Controls what lean-ctx injects during setup and updates. Fresh installs default to non-invasive (rules/skills off, MCP on).

- `auto_inject_rules` (bool?, default `null`) — Inject agent rule files during setup/update. null=auto (inject if already present), true=always, false=never
- `auto_inject_skills` (bool?, default `null`) — Install SKILL.md files during setup/update. null=auto (install if rules present), true=always, false=never
- `auto_update_mcp` (bool, default `true`) — Register lean-ctx MCP server in editor configs during setup/update

## `[skillify]`

Skillify miner: distill recurring session diary + knowledge patterns into rules

- `enabled` (bool, default `true`) — Master switch for the skillify miner (codify recurring session patterns into .cursor/rules). Only acts when explicitly invoked.
- `min_confidence` (f32, default `0.699999988079071`) — Minimum confidence for a single curated knowledge fact to be codified without repetition (0.0..=1.0).
- `min_recurrence` (u32, default `2`) — Minimum reinforcements (confirmations / repeated mentions) before a sub-threshold-confidence pattern is codified.
- `scope` (enum: project | global, default `project`) — Where generated rules are written: project (<repo>/.cursor/rules, git-committable) or global (~/.cursor/rules).

## `[summaries]`

AI session summaries: periodic, semantically-recallable session digests

- `enabled` (bool, default `true`) — Record periodic, semantically-recallable AI session summaries (what was done, files, decisions).
- `every_n_turns` (u32, default `25`) — Tool calls between automatic session summaries (gated by the auto-checkpoint cadence).
- `max_kept` (u32, default `100`) — Maximum session summaries kept per project (oldest pruned first).

## `[updates]`

Automatic update configuration

- `auto_update` (bool, default `false`) — Enable automatic updates (requires explicit opt-in)
- `check_interval_hours` (u64, default `6`) — How often to check for updates (hours)
- `notify_only` (bool, default `false`) — Only notify about updates, don't install automatically

