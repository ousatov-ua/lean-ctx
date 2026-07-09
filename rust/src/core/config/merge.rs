//! Project-local `.lean-ctx.toml` merge logic, extracted from `mod.rs` (#660).
//!
//! `merge_local` overlays a per-project config onto the global config, respecting
//! workspace trust boundaries: untrusted repos cannot weaken security-sensitive
//! settings (path-jail, shell allowlist, proxy upstream, etc.).

#[allow(clippy::wildcard_imports)]
use super::*;

impl Config {
    /// Merge a project-local `.lean-ctx.toml` onto `self`.
    ///
    /// `trusted` reflects [`crate::core::workspace_trust`]: when `false`, the
    /// security-sensitive overrides (shell allowlist, path-jail widening, proxy
    /// upstream, command aliases, rules scope, ...) are withheld and a warning is
    /// emitted — comfort-only overrides (compression, theme, memory tuning) still
    /// apply. This stops a cloned, untrusted repo from silently weakening
    /// lean-ctx's own boundaries through its bundled config (security audit #4).
    pub(super) fn merge_local(&mut self, local_toml: &str, trusted: bool) {
        let mut local: Config = match toml::from_str(local_toml) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("local config parse error: {e}");
                eprintln!(
                    "\x1b[33m[lean-ctx] WARNING: local .lean-ctx.toml parse error: {e}\n  \
                     Local overrides skipped.\x1b[0m"
                );
                return;
            }
        };
        if !trusted {
            let withheld = strip_sensitive_overrides(&mut local);
            if !withheld.is_empty() {
                tracing::warn!(
                    "[SECURITY] untrusted workspace: ignoring {} security-sensitive \
                     .lean-ctx.toml override(s): {} — run `lean-ctx trust` to apply them",
                    withheld.len(),
                    withheld.join(", ")
                );
            }
        }
        if local.ultra_compact {
            self.ultra_compact = true;
        }
        if local.tee_mode != TeeMode::default() {
            self.tee_mode = local.tee_mode;
        }
        if local.recovery_hints != RecoveryHints::default() {
            self.recovery_hints = local.recovery_hints;
        }
        if local.output_density != OutputDensity::default() {
            self.output_density = local.output_density;
        }
        if local.checkpoint_interval != 15 {
            self.checkpoint_interval = local.checkpoint_interval;
        }
        if !local.excluded_commands.is_empty() {
            self.excluded_commands.extend(local.excluded_commands);
        }
        if !local.passthrough_urls.is_empty() {
            self.passthrough_urls.extend(local.passthrough_urls);
        }
        if !local.custom_aliases.is_empty() {
            self.custom_aliases.extend(local.custom_aliases);
        }
        // Additive merge with dedup: project-local config can add formats on top
        // of the global default (`["toon"]`) without re-listing it.
        for fmt in local.preserve_compact_formats {
            if !self
                .preserve_compact_formats
                .iter()
                .any(|f| f.eq_ignore_ascii_case(&fmt))
            {
                self.preserve_compact_formats.push(fmt);
            }
        }
        if local.slow_command_threshold_ms != 5000 {
            self.slow_command_threshold_ms = local.slow_command_threshold_ms;
        }
        if local.theme != "default" {
            self.theme = local.theme;
        }
        if !local.buddy_enabled {
            self.buddy_enabled = false;
        }
        if !local.enable_wakeup_ctx {
            self.enable_wakeup_ctx = false;
        }
        if !local.redirect_exclude.is_empty() {
            self.redirect_exclude.extend(local.redirect_exclude);
        }
        if !local.disabled_tools.is_empty() {
            self.disabled_tools.extend(local.disabled_tools);
        }
        if local.prefer_native_editor {
            self.prefer_native_editor = true;
        }
        if !local.extra_ignore_patterns.is_empty() {
            self.extra_ignore_patterns
                .extend(local.extra_ignore_patterns);
        }
        // Index filters (#735): repo-local excludes extend the global list; a
        // repo-local include set (the stricter, corpus-defining axis) replaces
        // the global one; gitignore handling can only be switched off locally
        // (same only-tighten pattern as the bool flags above).
        if !local.index.exclude.is_empty() {
            self.index.exclude.extend(local.index.exclude);
        }
        if !local.index.include.is_empty() {
            self.index.include = local.index.include;
        }
        if !local.index.respect_gitignore {
            self.index.respect_gitignore = false;
        }
        if local.rules_scope.is_some() {
            self.rules_scope = local.rules_scope;
        }
        if local.rules_injection.is_some() {
            self.rules_injection = local.rules_injection;
        }
        if local.permission_inheritance.is_some() {
            self.permission_inheritance = local.permission_inheritance;
        }
        if local.proxy.anthropic_upstream.is_some() {
            self.proxy.anthropic_upstream = local.proxy.anthropic_upstream;
        }
        if local.proxy.openai_upstream.is_some() {
            self.proxy.openai_upstream = local.proxy.openai_upstream;
        }
        if local.proxy.chatgpt_upstream.is_some() {
            self.proxy.chatgpt_upstream = local.proxy.chatgpt_upstream;
        }
        if local.proxy.gemini_upstream.is_some() {
            self.proxy.gemini_upstream = local.proxy.gemini_upstream;
        }
        if !local.autonomy.enabled {
            self.autonomy.enabled = false;
        }
        if !local.autonomy.auto_preload {
            self.autonomy.auto_preload = false;
        }
        if !local.autonomy.auto_dedup {
            self.autonomy.auto_dedup = false;
        }
        if !local.autonomy.auto_related {
            self.autonomy.auto_related = false;
        }
        if !local.autonomy.auto_consolidate {
            self.autonomy.auto_consolidate = false;
        }
        if local.autonomy.silent_preload {
            self.autonomy.silent_preload = true;
        }
        if !local.autonomy.silent_preload && self.autonomy.silent_preload {
            self.autonomy.silent_preload = false;
        }
        if local.autonomy.dedup_threshold != AutonomyConfig::default().dedup_threshold {
            self.autonomy.dedup_threshold = local.autonomy.dedup_threshold;
        }
        if local.autonomy.consolidate_every_calls
            != AutonomyConfig::default().consolidate_every_calls
        {
            self.autonomy.consolidate_every_calls = local.autonomy.consolidate_every_calls;
        }
        if local.autonomy.consolidate_cooldown_secs
            != AutonomyConfig::default().consolidate_cooldown_secs
        {
            self.autonomy.consolidate_cooldown_secs = local.autonomy.consolidate_cooldown_secs;
        }
        if !local.autonomy.cognition_loop_enabled {
            self.autonomy.cognition_loop_enabled = false;
        }
        if local.autonomy.cognition_loop_interval_secs
            != AutonomyConfig::default().cognition_loop_interval_secs
        {
            self.autonomy.cognition_loop_interval_secs =
                local.autonomy.cognition_loop_interval_secs;
        }
        if local.autonomy.cognition_loop_max_steps
            != AutonomyConfig::default().cognition_loop_max_steps
        {
            self.autonomy.cognition_loop_max_steps = local.autonomy.cognition_loop_max_steps;
        }
        if local_toml.contains("compression_level") {
            self.compression_level = local.compression_level;
        }
        if local_toml.contains("compression_aggressiveness") {
            self.compression_aggressiveness = local.compression_aggressiveness;
        }
        if local_toml.contains("terse_agent") {
            self.terse_agent = local.terse_agent;
        }
        if !local.archive.enabled {
            self.archive.enabled = false;
        }
        if local.archive.threshold_chars != ArchiveConfig::default().threshold_chars {
            self.archive.threshold_chars = local.archive.threshold_chars;
        }
        if local.archive.max_age_hours != ArchiveConfig::default().max_age_hours {
            self.archive.max_age_hours = local.archive.max_age_hours;
        }
        if local.archive.max_disk_mb != ArchiveConfig::default().max_disk_mb {
            self.archive.max_disk_mb = local.archive.max_disk_mb;
        }
        if !local.archive.ephemeral {
            self.archive.ephemeral = false;
        }
        if local.archive.ephemeral_min_tokens != ArchiveConfig::default().ephemeral_min_tokens {
            self.archive.ephemeral_min_tokens = local.archive.ephemeral_min_tokens;
        }
        let mem_def = MemoryPolicy::default();
        if local.memory.knowledge.max_facts != mem_def.knowledge.max_facts {
            self.memory.knowledge.max_facts = local.memory.knowledge.max_facts;
        }
        if local.memory.knowledge.max_patterns != mem_def.knowledge.max_patterns {
            self.memory.knowledge.max_patterns = local.memory.knowledge.max_patterns;
        }
        if local.memory.knowledge.max_history != mem_def.knowledge.max_history {
            self.memory.knowledge.max_history = local.memory.knowledge.max_history;
        }
        if local.memory.knowledge.contradiction_threshold
            != mem_def.knowledge.contradiction_threshold
        {
            self.memory.knowledge.contradiction_threshold =
                local.memory.knowledge.contradiction_threshold;
        }

        if local.memory.episodic.max_episodes != mem_def.episodic.max_episodes {
            self.memory.episodic.max_episodes = local.memory.episodic.max_episodes;
        }
        if local.memory.episodic.max_actions_per_episode != mem_def.episodic.max_actions_per_episode
        {
            self.memory.episodic.max_actions_per_episode =
                local.memory.episodic.max_actions_per_episode;
        }
        if local.memory.episodic.summary_max_chars != mem_def.episodic.summary_max_chars {
            self.memory.episodic.summary_max_chars = local.memory.episodic.summary_max_chars;
        }

        if local.memory.procedural.min_repetitions != mem_def.procedural.min_repetitions {
            self.memory.procedural.min_repetitions = local.memory.procedural.min_repetitions;
        }
        if local.memory.procedural.min_sequence_len != mem_def.procedural.min_sequence_len {
            self.memory.procedural.min_sequence_len = local.memory.procedural.min_sequence_len;
        }
        if local.memory.procedural.max_procedures != mem_def.procedural.max_procedures {
            self.memory.procedural.max_procedures = local.memory.procedural.max_procedures;
        }
        if local.memory.procedural.max_window_size != mem_def.procedural.max_window_size {
            self.memory.procedural.max_window_size = local.memory.procedural.max_window_size;
        }

        if local.memory.lifecycle.decay_rate != mem_def.lifecycle.decay_rate {
            self.memory.lifecycle.decay_rate = local.memory.lifecycle.decay_rate;
        }
        if local.memory.lifecycle.low_confidence_threshold
            != mem_def.lifecycle.low_confidence_threshold
        {
            self.memory.lifecycle.low_confidence_threshold =
                local.memory.lifecycle.low_confidence_threshold;
        }
        if local.memory.lifecycle.stale_days != mem_def.lifecycle.stale_days {
            self.memory.lifecycle.stale_days = local.memory.lifecycle.stale_days;
        }
        if local.memory.lifecycle.similarity_threshold != mem_def.lifecycle.similarity_threshold {
            self.memory.lifecycle.similarity_threshold =
                local.memory.lifecycle.similarity_threshold;
        }
        if local.memory.lifecycle.reclaim_headroom_pct != mem_def.lifecycle.reclaim_headroom_pct {
            self.memory.lifecycle.reclaim_headroom_pct =
                local.memory.lifecycle.reclaim_headroom_pct;
        }
        if local.memory.lifecycle.reclaim_enabled != mem_def.lifecycle.reclaim_enabled {
            self.memory.lifecycle.reclaim_enabled = local.memory.lifecycle.reclaim_enabled;
        }

        if local.memory.embeddings.max_facts != mem_def.embeddings.max_facts {
            self.memory.embeddings.max_facts = local.memory.embeddings.max_facts;
        }
        if !local.allow_paths.is_empty() {
            self.allow_paths.extend(local.allow_paths);
        }
        if !local.extra_roots.is_empty() {
            self.extra_roots.extend(local.extra_roots);
        }
        // Project-local config may only ADD read-only roots (tighten the write
        // boundary), never remove them — merge mirrors extra_roots (#475).
        if !local.read_only_roots.is_empty() {
            self.read_only_roots.extend(local.read_only_roots);
        }
        // Symlink write-through roots (#596) follow extra_roots: a *trusted*
        // workspace may add roots, an untrusted one is stripped above.
        if !local.allow_symlink_roots.is_empty() {
            self.allow_symlink_roots.extend(local.allow_symlink_roots);
        }
        if local.minimal_overhead {
            self.minimal_overhead = true;
        }
        if local.shell_hook_disabled {
            self.shell_hook_disabled = true;
        }
        if local.skip_agent_aliases {
            self.skip_agent_aliases = true;
        }
        if local.shell_activation != ShellActivation::default() {
            self.shell_activation = local.shell_activation.clone();
        }
        if local.read_redirect != ReadRedirect::default() {
            self.read_redirect = local.read_redirect;
        }
        if local.read_dedup != ReadDedup::default() {
            self.read_dedup = local.read_dedup;
        }
        if local.bm25_max_cache_mb != default_bm25_max_cache_mb() {
            self.bm25_max_cache_mb = local.bm25_max_cache_mb;
        }
        if local.memory_profile != MemoryProfile::default() {
            self.memory_profile = local.memory_profile;
        }
        if local.memory_cleanup != MemoryCleanup::default() {
            self.memory_cleanup = local.memory_cleanup;
        }
        // Only override when the local file actually defines `shell_allowlist`.
        // The field carries `#[serde(default = "default_shell_allowlist")]`, so a
        // local `.lean-ctx.toml` that omits the key still deserializes to the full
        // 201-entry built-in list — an `is_empty()` guard would then silently clobber
        // a deliberately shorter global allowlist with the defaults. Comparing against
        // the default (the same pattern used for every other merged field) treats
        // "omitted" as "no override".
        if local.shell_allowlist != default_shell_allowlist() {
            self.shell_allowlist = local.shell_allowlist;
        }
        if !local.shell_allowlist_extra.is_empty() {
            self.shell_allowlist_extra
                .extend(local.shell_allowlist_extra);
        }
        if !local.default_tool_categories.is_empty() {
            self.default_tool_categories = local.default_tool_categories;
        }
        if local.tool_profile.is_some() {
            self.tool_profile = local.tool_profile;
        }
        if !local.tools_enabled.is_empty() {
            self.tools_enabled = local.tools_enabled;
        }
        if local.no_degrade {
            self.no_degrade = true;
        }
        if local.delta_explicit {
            self.delta_explicit = true;
        }
        if local.profile.is_some() {
            self.profile = local.profile;
        }
        if local.proxy_timeout_ms.is_some() {
            self.proxy_timeout_ms = local.proxy_timeout_ms;
        }
    }
}
