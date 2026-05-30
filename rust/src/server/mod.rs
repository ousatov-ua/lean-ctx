pub mod bounded_lock;
pub mod bypass_hint;
pub mod compaction_sync;
pub mod context_gate;
mod dispatch;
pub mod dynamic_tools;
pub mod elicitation;
pub(crate) mod execute;
pub mod helpers;
pub mod multi_path;
pub mod notifications;
pub mod progress;
pub mod prompts;
pub mod reference_store;
pub mod registry;
pub mod resources;
pub mod role_guard;
pub mod roots;
use roots::has_project_marker;
pub mod tool_trait;

use futures::FutureExt;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, InitializeRequestParams,
    InitializeResult, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ErrorData;

use crate::tools::{CrpMode, LeanCtxServer};

impl ServerHandler for LeanCtxServer {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_prompts()
            .build();

        let config = crate::core::config::Config::load();
        let level = crate::core::config::CompressionLevel::effective(&config);
        let _ = crate::core::terse::rules_inject::inject(&level);

        let instructions = crate::instructions::build_instructions(CrpMode::effective());

        InitializeResult::new(capabilities)
            .with_server_info(Implementation::new("lean-ctx", env!("CARGO_PKG_VERSION")))
            .with_instructions(instructions)
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        let name = request.client_info.name.clone();
        tracing::info!("MCP client connected: {:?}", name);
        *self.client_name.write().await = name.clone();
        *self.peer.write().await = Some(context.peer.clone());

        if self.session_mode != crate::tools::SessionMode::Shared {
            crate::core::budget_tracker::BudgetTracker::global().reset();
            if let Ok(data_dir) = crate::core::data_dir::lean_ctx_data_dir() {
                let radar = data_dir.join("context_radar.jsonl");
                if radar.exists() {
                    let prev = data_dir.join("context_radar.prev.jsonl");
                    let _ = std::fs::rename(&radar, &prev);
                }
            }
        }

        let has_roots = request.capabilities.roots.is_some();
        self.has_client_roots
            .store(has_roots, std::sync::atomic::Ordering::Relaxed);
        if has_roots {
            tracing::info!("Client supports MCP roots/list — will resolve on first tool call");
        }

        let env_root = roots::root_from_env();
        let derived_root = derive_project_root_from_cwd();
        let effective_root = env_root.or(derived_root);

        let cwd_str = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        {
            let mut session = self.session.write().await;
            if !cwd_str.is_empty() {
                session.shell_cwd = Some(cwd_str.clone());
            }
            if let Some(ref root) = effective_root {
                session.project_root = Some(root.clone());
                tracing::info!("Project root set to: {root}");
            } else if let Some(ref root) = session.project_root {
                // A previously persisted session may carry a contaminated root
                // (e.g. HOME from an older build or a client that reported HOME
                // as its workspace). Drop it unless it is a real, safe project
                // dir — otherwise PROJECT MEMORY leaks across projects.
                let root_path = std::path::Path::new(root);
                let root_has_marker = has_project_marker(root_path);
                let root_str = root_path.to_string_lossy();
                let root_suspicious = crate::core::pathutil::is_broad_or_unsafe_root(root_path)
                    || root_str.contains("/var/folders/")
                    || root_str.contains("/tmp/")
                    || root_str.contains("\\AppData\\Local\\Temp")
                    || root_str.contains("\\Temp\\");
                if root_suspicious && !root_has_marker {
                    tracing::info!("Dropping suspicious persisted project root: {root}");
                    session.project_root = None;
                }
            }
            let cfg_extra = crate::core::config::Config::load().extra_roots;
            if !cfg_extra.is_empty() {
                let existing: std::collections::HashSet<_> =
                    session.extra_roots.iter().cloned().collect();
                for r in cfg_extra {
                    if !existing.contains(&r) {
                        session.extra_roots.push(r);
                    }
                }
            }
            if self.session_mode == crate::tools::SessionMode::Shared {
                if let Some(ref root) = session.project_root {
                    if let Some(ref rt) = self.context_os {
                        rt.shared_sessions.persist_best_effort(
                            root,
                            &self.workspace_id,
                            &self.channel_id,
                            &session,
                        );
                        rt.metrics.record_session_persisted();
                    }
                }
            } else if let Err(e) = session.save() {
                tracing::warn!("lean-ctx: failed to persist session state: {e}");
            }
        }

        if let Some(ref root) = effective_root {
            crate::core::index_orchestrator::ensure_all_background(root);
        }

        let agent_name = name.clone();
        let agent_root = effective_root.clone().unwrap_or_default();
        let agent_id_handle = self.agent_id.clone();
        tokio::task::spawn_blocking(move || {
            if std::env::var("LEAN_CTX_HEADLESS").is_ok() {
                return;
            }

            // Avoid startup stampedes when multiple agent sessions initialize at once.
            // These are best-effort maintenance tasks; it's fine to skip if another
            // lean-ctx instance is already doing them.
            let maintenance = crate::core::startup_guard::try_acquire_lock(
                "startup-maintenance",
                std::time::Duration::from_secs(2),
                std::time::Duration::from_mins(2),
            );
            if maintenance.is_some() {
                if let Some(home) = dirs::home_dir() {
                    let _ = crate::rules_inject::inject_all_rules(&home);
                }
                crate::hooks::refresh_installed_hooks();
                crate::core::version_check::check_background();
                // Enforce the on-disk budget: prune accumulated quarantined BM25
                // indexes and cap the archive FTS DB (#2364). Silent (tracing
                // only) so it never corrupts the MCP stdio protocol.
                let _ = crate::core::storage_maintenance::run_quiet();
            }
            drop(maintenance);

            if !agent_root.is_empty() {
                let heuristic_role = match agent_name.to_lowercase().as_str() {
                    n if n.contains("cursor") => Some("coder"),
                    n if n.contains("claude") => Some("coder"),
                    n if n.contains("codex") => Some("coder"),
                    n if n.contains("antigravity") || n.contains("gemini") => Some("coder"),
                    n if n.contains("review") => Some("reviewer"),
                    n if n.contains("test") => Some("debugger"),
                    _ => None,
                };
                let env_role = std::env::var("LEAN_CTX_ROLE")
                    .or_else(|_| std::env::var("LEAN_CTX_AGENT_ROLE"))
                    .ok();
                let effective_role = env_role.as_deref().or(heuristic_role).unwrap_or("coder");

                let _ = crate::core::roles::set_active_role_with_source(effective_role, true);

                let mut registry = crate::core::agents::AgentRegistry::load_or_create();
                registry.cleanup_stale(24);
                let id = registry.register("mcp", Some(effective_role), &agent_root);
                let _ = registry.save();
                if let Ok(mut guard) = agent_id_handle.try_write() {
                    *guard = Some(id);
                }
            }
        });

        let client_caps = crate::core::client_capabilities::ClientMcpCapabilities::detect(&name);
        tracing::info!("Client capabilities: {}", client_caps.format_summary());

        {
            let cfg = crate::core::config::Config::load();
            let cats = cfg.default_tool_categories_effective();
            dynamic_tools::init_from_config(&cats);
        }

        if client_caps.dynamic_tools {
            if let Ok(mut dt) = dynamic_tools::global().lock() {
                dt.set_supports_list_changed(true);
            }
        }
        if let Some(max) = client_caps.max_tools {
            if let Ok(mut dt) = dynamic_tools::global().lock() {
                dt.set_supports_list_changed(true);
                if max < 100 {
                    dt.unload_category(dynamic_tools::ToolCategory::Debug);
                    dt.unload_category(dynamic_tools::ToolCategory::Memory);
                }
            }
        }

        crate::core::client_capabilities::set_detected(&client_caps);

        let instructions =
            crate::instructions::build_instructions_with_client(CrpMode::effective(), &name);

        let capabilities = match (client_caps.resources, client_caps.prompts) {
            (true, true) => ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_resources_subscribe()
                .enable_prompts()
                .build(),
            (true, false) => ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_resources_subscribe()
                .build(),
            (false, true) => ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
            (false, false) => ServerCapabilities::builder().enable_tools().build(),
        };

        Ok(InitializeResult::new(capabilities)
            .with_server_info(Implementation::new("lean-ctx", env!("CARGO_PKG_VERSION")))
            .with_instructions(instructions))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let all_tools = if crate::tool_defs::is_full_mode() {
            if let Some(ref reg) = self.registry {
                reg.tool_defs()
            } else {
                crate::tool_defs::granular_tool_defs()
            }
        } else if std::env::var("LEAN_CTX_UNIFIED").is_ok() {
            crate::tool_defs::unified_tool_defs()
        } else if let Some(ref reg) = self.registry {
            let core_names = crate::tool_defs::core_tool_names();
            reg.tool_defs()
                .into_iter()
                .filter(|t| core_names.contains(&t.name.as_ref()))
                .collect()
        } else {
            crate::tool_defs::lazy_tool_defs()
        };

        let disabled = crate::core::config::Config::load().disabled_tools_effective();
        let client = self.client_name.read().await.clone();
        let is_zed = !client.is_empty() && client.to_lowercase().contains("zed");

        let active_role = crate::core::roles::active_role();
        let tools: Vec<_> = all_tools
            .into_iter()
            .filter(|t| {
                let name = t.name.as_ref();
                if !disabled.is_empty() && disabled.iter().any(|d| d.as_str() == name) {
                    return false;
                }
                if is_zed && name == "ctx_edit" {
                    return false;
                }
                if !active_role.is_tool_allowed(name) {
                    return false;
                }
                true
            })
            .collect();

        let tools = {
            let Ok(dyn_state) = dynamic_tools::global().lock() else {
                tracing::warn!("dynamic_tools mutex poisoned in list_tools; returning unfiltered");
                return Ok(ListToolsResult {
                    tools,
                    ..Default::default()
                });
            };
            if dyn_state.supports_list_changed() {
                tools
                    .into_iter()
                    .filter(|t| dyn_state.is_tool_active(t.name.as_ref()))
                    .collect()
            } else {
                tools
            }
        };

        let tools = {
            let active = self.workflow.read().await.clone();
            if let Some(run) = active {
                if run.current == "done" || is_workflow_stale(&run) {
                    let mut wf = self.workflow.write().await;
                    *wf = None;
                    let _ = crate::core::workflow::clear_active();
                } else if let Some(state) = run.spec.state(&run.current) {
                    if let Some(allowed) = &state.allowed_tools {
                        let mut allow: std::collections::HashSet<&str> =
                            allowed.iter().map(std::string::String::as_str).collect();
                        for passthrough in WORKFLOW_PASSTHROUGH_TOOLS {
                            allow.insert(passthrough);
                        }
                        return Ok(ListToolsResult {
                            tools: tools
                                .into_iter()
                                .filter(|t| allow.contains(t.name.as_ref()))
                                .collect(),
                            ..Default::default()
                        });
                    }
                }
            }
            tools
        };

        let tools = {
            let cfg = crate::core::config::Config::load();
            let level = crate::core::config::CompressionLevel::effective(&cfg);
            let mode =
                crate::core::terse::mcp_compress::DescriptionMode::from_compression_level(&level);
            if mode == crate::core::terse::mcp_compress::DescriptionMode::Full {
                tools
            } else {
                tools
                    .into_iter()
                    .map(|mut t| {
                        let compressed = crate::core::terse::mcp_compress::compress_description(
                            t.name.as_ref(),
                            t.description.as_deref().unwrap_or(""),
                            mode,
                        );
                        t.description = Some(compressed.into());
                        t
                    })
                    .collect()
            }
        };

        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListPromptsResult, ErrorData> {
        Ok(rmcp::model::ListPromptsResult::with_all_items(
            prompts::list_prompts(),
        ))
    }

    async fn get_prompt(
        &self,
        request: rmcp::model::GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResult, ErrorData> {
        let ledger = self.ledger.read().await;
        match prompts::get_prompt(&request, &ledger) {
            Some(result) => Ok(result),
            None => Err(ErrorData::invalid_params(
                format!("Unknown prompt: {}", request.name),
                None,
            )),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListResourcesResult::with_all_items(
            resources::list_resources(),
        ))
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
        let ledger = self.ledger.read().await;
        match resources::read_resource(&request.uri, &ledger) {
            Some(contents) => Ok(rmcp::model::ReadResourceResult::new(contents)),
            None => Err(rmcp::ErrorData::resource_not_found(
                format!("Unknown resource: {}", request.uri),
                None,
            )),
        }
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        use std::panic::AssertUnwindSafe;

        let progress_token = request
            .meta
            .as_ref()
            .and_then(rmcp::model::Meta::get_progress_token);
        if let Some(ref token) = progress_token {
            let sender =
                crate::server::progress::ProgressSender::new(context.peer.clone(), token.clone());
            *self
                .progress_sender
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
        }

        let tool_name_for_panic = request.name.as_ref().to_string();
        let args_fp_for_panic = request
            .arguments
            .as_ref()
            .map(|a| {
                crate::core::loop_detection::LoopDetector::fingerprint(&serde_json::Value::Object(
                    a.clone(),
                ))
            })
            .unwrap_or_default();

        let loop_detector = self.loop_detector.clone();

        match AssertUnwindSafe(self.call_tool_guarded(request))
            .catch_unwind()
            .await
        {
            Ok(result) => result,
            Err(panic_payload) => {
                let detail = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown".to_string()
                };
                tracing::error!("call_tool panicked: {detail}");

                if let Ok(mut detector) =
                    tokio::time::timeout(std::time::Duration::from_secs(1), loop_detector.write())
                        .await
                {
                    detector.record_error_outcome(&tool_name_for_panic, &args_fp_for_panic);
                }

                Ok(CallToolResult::error(vec![Content::text(
                    "ERROR: lean-ctx internal error. The MCP server is still running. \
                     Please retry or use a different approach."
                        .to_string(),
                )]))
            }
        }
    }

    async fn on_roots_list_changed(
        &self,
        _context: rmcp::service::NotificationContext<RoleServer>,
    ) {
        tracing::info!("Received roots/list_changed — will re-resolve on next tool call");
        self.roots_resolved
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl LeanCtxServer {
    async fn call_tool_guarded(
        &self,
        request: CallToolRequestParams,
    ) -> Result<CallToolResult, ErrorData> {
        self.check_idle_expiry().await;
        self.resolve_roots_once().await;
        elicitation::increment_call();

        let original_name = request.name.as_ref().to_string();
        let (resolved_name, resolved_args) = if original_name == "ctx" {
            let sub = request
                .arguments
                .as_ref()
                .and_then(|a| a.get("tool"))
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string)
                .ok_or_else(|| {
                    ErrorData::invalid_params("'tool' is required for ctx meta-tool", None)
                })?;
            let tool_name = if sub.starts_with("ctx_") {
                sub
            } else {
                format!("ctx_{sub}")
            };
            let mut args = request.arguments.unwrap_or_default();
            args.remove("tool");
            (tool_name, Some(args))
        } else {
            (original_name, request.arguments)
        };
        let name = resolved_name.as_str();
        let args = resolved_args.as_ref();

        let role_check = role_guard::check_tool_access(name);
        if let Some(denied) = role_guard::into_call_tool_result(&role_check) {
            tracing::warn!(
                tool = name,
                role = %role_check.role_name,
                "Tool blocked by role policy"
            );
            return Ok(denied);
        }

        if name != "ctx_workflow" {
            let active = self.workflow.read().await.clone();
            if let Some(run) = active {
                if run.current == "done" || is_workflow_stale(&run) {
                    let mut wf = self.workflow.write().await;
                    *wf = None;
                    let _ = crate::core::workflow::clear_active();
                } else if !WORKFLOW_PASSTHROUGH_TOOLS.contains(&name) {
                    if let Some(state) = run.spec.state(&run.current) {
                        if let Some(allowed) = &state.allowed_tools {
                            let allowed_ok = allowed.iter().any(|t| t == name);
                            if !allowed_ok {
                                let mut shown = allowed.clone();
                                shown.sort();
                                shown.truncate(30);
                                return Ok(CallToolResult::success(vec![Content::text(format!(
                                    "Tool '{name}' blocked by workflow '{}' (state: {}). Allowed: {}. Use ctx_workflow(action=\"stop\") to exit.",
                                    run.spec.name,
                                    run.current,
                                    shown.join(", ")
                                ))]));
                            }
                        }
                    }
                }
            }
        }

        let auto_context = {
            let task = {
                let session = self.session.read().await;
                session.task.as_ref().map(|t| t.description.clone())
            };
            let project_root = {
                let session = self.session.read().await;
                session.project_root.clone()
            };
            let cache_timeout =
                tokio::time::timeout(std::time::Duration::from_secs(5), self.cache.write()).await;
            if let Ok(mut cache) = cache_timeout {
                crate::tools::autonomy::session_lifecycle_pre_hook(
                    &self.autonomy,
                    name,
                    &mut cache,
                    task.as_deref(),
                    project_root.as_deref(),
                    CrpMode::effective(),
                )
            } else {
                tracing::warn!("pre-dispatch: cache write-lock timeout (5s), skipping autonomy");
                None
            }
        };

        let args_fp = args
            .map(|a| {
                crate::core::loop_detection::LoopDetector::fingerprint(&serde_json::Value::Object(
                    a.clone(),
                ))
            })
            .unwrap_or_default();
        let throttle_result = {
            let fp = &args_fp;
            let detector_timeout = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                self.loop_detector.write(),
            )
            .await;
            if let Ok(mut detector) = detector_timeout {
                let is_search = crate::core::loop_detection::LoopDetector::is_search_tool(name);
                let is_search_shell = name == "ctx_shell" && {
                    let cmd = args
                        .as_ref()
                        .and_then(|a| a.get("command"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    crate::core::loop_detection::LoopDetector::is_search_shell_command(cmd)
                };

                if is_search || is_search_shell {
                    let search_pattern = args.and_then(|a| {
                        a.get("pattern")
                            .or_else(|| a.get("query"))
                            .and_then(|v| v.as_str())
                    });
                    let shell_pattern = if is_search_shell {
                        args.and_then(|a| a.get("command"))
                            .and_then(|v| v.as_str())
                            .and_then(helpers::extract_search_pattern_from_command)
                    } else {
                        None
                    };
                    let pat = search_pattern.or(shell_pattern.as_deref());
                    detector.record_search(name, fp, pat)
                } else {
                    detector.record_call(name, fp)
                }
            } else {
                tracing::warn!("pre-dispatch: loop_detector write-lock timeout (3s), skipping");
                crate::core::loop_detection::ThrottleResult::default()
            }
        };

        if throttle_result.level == crate::core::loop_detection::ThrottleLevel::Blocked {
            let msg = throttle_result.message.unwrap_or_default();
            return Ok(CallToolResult::success(vec![Content::text(msg)]));
        }

        let throttle_warning =
            if throttle_result.level == crate::core::loop_detection::ThrottleLevel::Reduced {
                throttle_result.message.clone()
            } else {
                None
            };

        let config = crate::core::config::Config::load();
        let minimal = config.minimal_overhead_effective();

        {
            use crate::core::budget_tracker::{BudgetLevel, BudgetTracker};
            let snap = BudgetTracker::global().check();
            if *snap.worst_level() == BudgetLevel::Exhausted
                && name != "ctx_session"
                && name != "ctx_cost"
                && name != "ctx_metrics"
            {
                for (dim, lvl, used, limit) in [
                    (
                        "tokens",
                        &snap.tokens.level,
                        format!("{}", snap.tokens.used),
                        format!("{}", snap.tokens.limit),
                    ),
                    (
                        "shell",
                        &snap.shell.level,
                        format!("{}", snap.shell.used),
                        format!("{}", snap.shell.limit),
                    ),
                    (
                        "cost",
                        &snap.cost.level,
                        format!("${:.2}", snap.cost.used_usd),
                        format!("${:.2}", snap.cost.limit_usd),
                    ),
                ] {
                    if *lvl == BudgetLevel::Exhausted {
                        crate::core::events::emit_budget_exhausted(&snap.role, dim, &used, &limit);
                    }
                }
                let msg = format!(
                    "[BUDGET EXHAUSTED] {}\n\
                     Use `ctx_session action=role` to check/switch roles, \
                     or `ctx_session action=reset` to start fresh.",
                    snap.format_compact()
                );
                tracing::warn!(tool = name, "{msg}");
                return Ok(CallToolResult::success(vec![Content::text(msg)]));
            }
        }

        if is_shell_tool_name(name) {
            crate::core::budget_tracker::BudgetTracker::global().record_shell();
        }

        let tool_start = std::time::Instant::now();
        let (mut result_text, tool_saved_tokens) =
            match self.dispatch_tool(name, args, minimal).await {
                Ok(pair) => pair,
                Err(e) => {
                    if let Ok(mut detector) = tokio::time::timeout(
                        std::time::Duration::from_secs(1),
                        self.loop_detector.write(),
                    )
                    .await
                    {
                        detector.record_error_outcome(name, &args_fp);
                    }
                    return Err(e);
                }
            };

        let is_raw_shell = name == "ctx_shell" && {
            let arg_raw = helpers::get_bool(args, "raw").unwrap_or(false);
            let arg_bypass = helpers::get_bool(args, "bypass").unwrap_or(false);
            arg_raw
                || arg_bypass
                || std::env::var("LEAN_CTX_DISABLED").is_ok()
                || std::env::var("LEAN_CTX_RAW").is_ok()
        };

        let pre_terse_len = result_text.len();
        let output_tokens = {
            let tokens = crate::core::tokens::count_tokens(&result_text) as u64;
            crate::core::budget_tracker::BudgetTracker::global().record_tokens(tokens);
            tokens
        };

        crate::core::anomaly::record_metric("tokens_per_call", output_tokens as f64);

        // Context IR: record lineage for every tool call.
        if let Some(ref ir) = self.context_ir {
            let tool_duration = tool_start.elapsed();
            let source_kind = match name {
                n if n.contains("read") || n.contains("multi_read") || n.contains("smart_read") => {
                    crate::core::context_ir::ContextIrSourceKindV1::Read
                }
                "ctx_shell" => crate::core::context_ir::ContextIrSourceKindV1::Shell,
                "ctx_search" | "ctx_semantic_search" => {
                    crate::core::context_ir::ContextIrSourceKindV1::Search
                }
                "ctx_provider" => crate::core::context_ir::ContextIrSourceKindV1::Provider,
                _ => crate::core::context_ir::ContextIrSourceKindV1::Other,
            };
            let ir_path = helpers::get_str(args, "path");
            let ir_command = helpers::get_str(args, "command");
            let ir_mode = helpers::get_str(args, "mode");
            let excerpt = if result_text.len() > 200 {
                let mut end = 200;
                while !result_text.is_char_boundary(end) && end > 0 {
                    end -= 1;
                }
                &result_text[..end]
            } else {
                &result_text
            };
            let input = crate::core::context_ir::RecordIrInput {
                kind: source_kind,
                tool: name,
                client_name: None,
                agent_id: None,
                path: ir_path.as_deref(),
                command: ir_command.as_deref(),
                pattern: ir_mode.as_deref(),
                input_tokens: pre_terse_len / 4,
                output_tokens: output_tokens as usize,
                duration: tool_duration,
                content_excerpt: excerpt,
            };
            ir.write().await.record(input);
        }

        // Correction-loop detection: track re-reads and re-runs as quality signals.
        {
            let mut detector = self.loop_detector.write().await;
            if name == "ctx_read" {
                let path = helpers::get_str(args, "path").unwrap_or_default();
                let mode = helpers::get_str(args, "mode").unwrap_or_else(|| "auto".into());
                let fresh = helpers::get_bool(args, "fresh").unwrap_or(false);
                detector.record_read_for_correction(&path, &mode, fresh);
            } else if name == "ctx_shell" {
                let cmd = helpers::get_str(args, "command").unwrap_or_default();
                detector.record_shell_for_correction(&cmd);
            }
            let correction_count = detector.correction_count();
            if correction_count > 0 {
                crate::core::anomaly::record_metric(
                    "correction_loop_rate",
                    f64::from(correction_count),
                );
            }
            // Auto-degrade: reduce compression when correction rate is high
            use crate::core::config::CompressionLevel;
            if correction_count >= 5 {
                CompressionLevel::set_session_degrade(&CompressionLevel::Off);
            } else if correction_count >= 3 {
                CompressionLevel::set_session_degrade(&CompressionLevel::Lite);
            } else if correction_count == 0 {
                CompressionLevel::clear_session_degrade();
            }
            detector.prune_corrections();
        }

        // Persist anomaly detector — debounced to reduce I/O in burst sequences.
        crate::core::anomaly::save_debounced();

        let budget_warning = {
            use crate::core::budget_tracker::{BudgetLevel, BudgetTracker};
            let snap = BudgetTracker::global().check();
            if *snap.worst_level() == BudgetLevel::Warning {
                for (dim, lvl, used, limit, pct) in [
                    (
                        "tokens",
                        &snap.tokens.level,
                        format!("{}", snap.tokens.used),
                        format!("{}", snap.tokens.limit),
                        snap.tokens.percent,
                    ),
                    (
                        "shell",
                        &snap.shell.level,
                        format!("{}", snap.shell.used),
                        format!("{}", snap.shell.limit),
                        snap.shell.percent,
                    ),
                    (
                        "cost",
                        &snap.cost.level,
                        format!("${:.2}", snap.cost.used_usd),
                        format!("${:.2}", snap.cost.limit_usd),
                        snap.cost.percent,
                    ),
                ] {
                    if *lvl == BudgetLevel::Warning {
                        crate::core::events::emit_budget_warning(
                            &snap.role, dim, &used, &limit, pct,
                        );
                    }
                }
                if crate::core::protocol::meta_visible() {
                    Some(format!("[BUDGET WARNING] {}", snap.format_compact()))
                } else {
                    None
                }
            } else {
                None
            }
        };

        let archive_hint = if minimal || is_raw_shell {
            None
        } else {
            use crate::core::archive;
            let archivable = matches!(
                name,
                "ctx_shell"
                    | "ctx_read"
                    | "ctx_multi_read"
                    | "ctx_smart_read"
                    | "ctx_execute"
                    | "ctx_search"
                    | "ctx_tree"
            );
            if archivable && archive::should_archive(&result_text) {
                let cmd = helpers::get_str(args, "command")
                    .or_else(|| helpers::get_str(args, "path"))
                    .unwrap_or_default();
                let session_id = self.session.read().await.id.clone();
                let to_store = crate::core::redaction::redact_text_if_enabled(&result_text);
                let tokens = crate::core::tokens::count_tokens(&to_store);
                archive::store(name, &cmd, &to_store, Some(&session_id))
                    .map(|id| archive::format_hint(&id, to_store.len(), tokens))
            } else {
                None
            }
        };

        let pre_compression = result_text.clone();
        let deeply_compressed = matches!(
            name,
            "ctx_read" | "ctx_multi_read" | "ctx_smart_read" | "ctx_compress" | "ctx_overview"
        );
        let skip_terse = is_raw_shell
            || (tool_saved_tokens > 0 && deeply_compressed)
            || (name == "ctx_shell"
                && helpers::get_str(args, "command")
                    .is_some_and(|c| crate::shell::compress::has_structural_output(&c)));
        let compression = crate::core::config::CompressionLevel::effective(&config);
        if compression.is_active() && !skip_terse {
            let terse_result =
                crate::core::terse::pipeline::compress(&result_text, &compression, None);
            if terse_result.quality_passed && terse_result.savings_pct >= 3.0 {
                result_text = terse_result.output;
            }
        }

        let profile_hints = crate::core::profiles::active_profile().output_hints;

        if !is_raw_shell && profile_hints.verify_footer() {
            let verify_cfg = crate::core::profiles::active_profile().verification;
            let vr = crate::core::output_verification::verify_output(
                &pre_compression,
                &result_text,
                &verify_cfg,
            );
            if !vr.warnings.is_empty() {
                let msg = format!("[VERIFY] {}", vr.format_compact());
                result_text = format!("{result_text}\n\n{msg}");
            }
        }

        if profile_hints.archive_hint() {
            if let Some(hint) = archive_hint {
                result_text = format!("{result_text}\n{hint}");
            }
        }

        if !is_raw_shell {
            if let Some(ctx) = auto_context {
                let ctx_tokens = crate::core::tokens::count_tokens(&ctx);
                if ctx_tokens <= 400 {
                    result_text = format!("{ctx}\n\n{result_text}");
                }
            }
        }

        if let Some(warning) = throttle_warning {
            result_text = format!("{result_text}\n\n{warning}");
        }

        if let Some(bw) = budget_warning {
            result_text = format!("{result_text}\n\n{bw}");
        }

        if !self
            .rules_stale_checked
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            let client = self.client_name.read().await.clone();
            if !client.is_empty() {
                if let Some(stale_msg) = crate::rules_inject::check_rules_freshness(&client) {
                    result_text = format!("{result_text}\n\n{stale_msg}");
                }
            }
        }

        {
            // Evaluate SLOs for observability (watch/dashboard), but keep tool outputs clean.
            let _ = crate::core::slo::evaluate();
        }

        if name == "ctx_read" {
            if minimal {
                let cache_clone = self.cache.clone();
                let autonomy_clone = self.autonomy.clone();
                let name_owned = name.to_string();
                tokio::spawn(async move {
                    let result = std::panic::AssertUnwindSafe(async {
                        let mut cache = cache_clone.write().await;
                        crate::tools::autonomy::maybe_auto_dedup(
                            &autonomy_clone,
                            &mut cache,
                            &name_owned,
                        );
                    })
                    .catch_unwind()
                    .await;
                    if let Err(e) = result {
                        let msg = e
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| e.downcast_ref::<&str>().copied())
                            .unwrap_or("unknown");
                        tracing::error!("background auto_dedup panicked: {msg}");
                    }
                });
            } else {
                let read_path = self
                    .resolve_path_or_passthrough(
                        &helpers::get_str(args, "path").unwrap_or_default(),
                    )
                    .await;
                let project_root = {
                    let session = self.session.read().await;
                    session.project_root.clone()
                };

                // Bounded cache lock for enrichment — degrade gracefully under contention
                let enrich_timeout =
                    tokio::time::timeout(std::time::Duration::from_secs(3), self.cache.write())
                        .await;
                if let Ok(mut cache) = enrich_timeout {
                    let enrich = crate::tools::autonomy::enrich_after_read(
                        &self.autonomy,
                        &mut cache,
                        &read_path,
                        project_root.as_deref(),
                        None,
                        crate::tools::CrpMode::effective(),
                        false,
                    );
                    if profile_hints.related_hint() {
                        if let Some(hint) = enrich.related_hint {
                            result_text = format!("{result_text}\n{hint}");
                        }
                    }
                    crate::tools::autonomy::maybe_auto_dedup(&self.autonomy, &mut cache, name);
                } else {
                    tracing::warn!(
                        "post-dispatch cache lock timeout (3s) for {read_path}, skipping enrichment"
                    );
                }

                // Ledger update — fire-and-forget to avoid blocking concurrent reads
                let ledger_clone = self.ledger.clone();
                let session_clone = self.session.clone();
                let peer_clone = self.peer.clone();
                let read_path_owned = read_path.clone();
                let project_root_owned = project_root.clone();
                let mode_used =
                    helpers::get_str(args, "mode").unwrap_or_else(|| "auto".to_string());
                let out_tok = output_tokens as usize;
                let sent_tok = crate::core::tokens::count_tokens(&result_text);
                let wants_eviction = true;
                let wants_elicitation = profile_hints.elicitation_hint();
                tokio::spawn(async move {
                    let result = std::panic::AssertUnwindSafe(async {
                        let active_task = {
                            let session = session_clone.read().await;
                            session.task.as_ref().map(|t| t.description.clone())
                        };
                        let mut ledger = ledger_clone.write().await;
                        let overlay = crate::core::context_overlay::OverlayStore::load_project(
                            &std::path::PathBuf::from(project_root_owned.as_deref().unwrap_or(".")),
                        );
                        let gate_result = context_gate::post_dispatch_record_with_task(
                            &read_path_owned,
                            &mode_used,
                            out_tok,
                            sent_tok,
                            &mut ledger,
                            &overlay,
                            active_task.as_deref(),
                        );
                        drop(ledger);
                        if wants_eviction {
                            if let Some(hint) = &gate_result.eviction_hint {
                                tracing::debug!("deferred eviction hint: {hint}");
                            }
                        }
                        if wants_elicitation {
                            if let Some(hint) = &gate_result.elicitation_hint {
                                tracing::debug!("deferred elicitation hint: {hint}");
                            }
                        }
                        if gate_result.resource_changed {
                            if let Some(peer) = peer_clone.read().await.as_ref() {
                                notifications::send_resource_updated(
                                    peer,
                                    notifications::RESOURCE_URI_SUMMARY,
                                )
                                .await;
                            }
                        }
                    })
                    .catch_unwind()
                    .await;
                    if let Err(e) = result {
                        let msg = e
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| e.downcast_ref::<&str>().copied())
                            .unwrap_or("unknown");
                        tracing::error!("background post_dispatch panicked: {msg}");
                    }
                });
            }
        }

        if !minimal && !is_raw_shell && name == "ctx_shell" {
            let cmd = helpers::get_str(args, "command").unwrap_or_default();

            if let Some(file_path) = extract_file_read_from_shell(&cmd) {
                if let Ok(mut bt) = crate::core::bounce_tracker::global().lock() {
                    bt.next_seq();
                    bt.record_shell_file_access(&file_path);
                }
            }

            if profile_hints.efficiency_hint() {
                let calls = self.tool_calls.read().await;
                let last_original = calls.last().map_or(0, |c| c.original_tokens);
                drop(calls);
                let pre_hint_tokens = crate::core::tokens::count_tokens(&result_text);
                if let Some(hint) = crate::tools::autonomy::shell_efficiency_hint(
                    &self.autonomy,
                    &cmd,
                    last_original,
                    pre_hint_tokens,
                ) {
                    result_text = format!("{result_text}\n{hint}");
                }
            }
        }

        if !minimal && !is_raw_shell {
            if let Ok(data_dir) = crate::core::data_dir::lean_ctx_data_dir() {
                let session = self.session.read().await;
                bypass_hint::set_session_id(&session.id);
                drop(session);
                if let Some(hint) = bypass_hint::check(&data_dir) {
                    result_text = format!("{result_text}\n{hint}");
                }
            }
            bypass_hint::record_lctx_call();
        }

        if let Some(finding) = crate::core::auto_findings::extract(name, &result_text) {
            let mut session = self.session.write().await;
            session.add_finding(finding.file.as_deref(), None, &finding.summary);
            let project_root = session.project_root.clone();
            drop(session);
            if let Some(ref root) = project_root {
                let f = finding.clone();
                let r = root.clone();
                std::thread::spawn(move || {
                    crate::core::auto_capture::capture_finding(&r, &f);
                });
            }
        }
        if let Some(extra) = crate::core::auto_capture::extract_extra(name, &result_text) {
            let session = self.session.read().await;
            let project_root = session.project_root.clone();
            drop(session);
            if let Some(ref root) = project_root {
                let e = extra.clone();
                let r = root.clone();
                std::thread::spawn(move || {
                    crate::core::auto_capture::capture_finding(&r, &e);
                });
            }
        }

        {
            let tool_name = name.to_string();
            let summary = result_text.lines().next().unwrap_or("").to_string();
            std::thread::spawn(move || {
                crate::core::journal::maybe_day_separator();
                crate::core::journal::log_tool_call(&tool_name, &summary);
            });
        }

        #[allow(clippy::cast_possible_truncation)]
        let output_token_count = if result_text.len() == pre_terse_len {
            output_tokens as usize
        } else {
            crate::core::tokens::count_tokens(&result_text)
        };

        // OPT-4: Correct stats with post-processing token counts.
        // dispatch/mod.rs records savings before terse/hints; adjust here
        // so persistent stats reflect what the model actually receives.
        if result_text.len() != pre_terse_len && tool_saved_tokens > 0 {
            let pre_savings = tool_saved_tokens;
            let actual_sent = output_token_count;
            let original = actual_sent + pre_savings;
            let actual_savings = original.saturating_sub(actual_sent);
            if actual_savings != pre_savings {
                let delta = pre_savings as i64 - actual_savings as i64;
                if delta != 0 {
                    crate::core::stats::adjust_savings(name, delta);
                }
            }
        }

        let action = helpers::get_str(args, "action");

        // K-bounded staleness guard: warn if shared context has diverged.
        const K_STALENESS_BOUND: i64 = 10;
        if self.session_mode == crate::tools::SessionMode::Shared {
            if let Some(ref rt) = self.context_os {
                let latest = rt.bus.latest_id(&self.workspace_id, &self.channel_id);
                let cursor = self
                    .last_seen_event_id
                    .load(std::sync::atomic::Ordering::Relaxed);
                if cursor > 0 && latest - cursor > K_STALENESS_BOUND {
                    let gap = latest - cursor;
                    result_text = format!(
                        "[CONTEXT STALE] {gap} events happened since your last read. \
                         Use ctx_session(action=\"status\") to sync.\n\n{result_text}"
                    );
                }
                self.last_seen_event_id
                    .store(latest, std::sync::atomic::Ordering::Relaxed);
            }
        }

        {
            let input = helpers::canonical_args_string(args);
            let input_md5 = helpers::hash_fast(&input);
            let output_md5 = helpers::hash_fast(&result_text);
            let agent_id = self.agent_id.read().await.clone();
            let client_name = self.client_name.read().await.clone();
            let mut explicit_intent: Option<(
                crate::core::intent_protocol::IntentRecord,
                Option<String>,
                String,
            )> = None;

            let pending_session_save = {
                let empty_args = serde_json::Map::new();
                let args_map = args.unwrap_or(&empty_args);
                let mut session = self.session.write().await;
                session.record_tool_receipt(
                    name,
                    action.as_deref(),
                    &input_md5,
                    &output_md5,
                    agent_id.as_deref(),
                    Some(&client_name),
                );

                if let Some(intent) = crate::core::intent_protocol::infer_from_tool_call(
                    name,
                    action.as_deref(),
                    args_map,
                    session.project_root.as_deref(),
                ) {
                    let is_explicit =
                        intent.source == crate::core::intent_protocol::IntentSource::Explicit;
                    let root = session.project_root.clone();
                    let sid = session.id.clone();
                    session.record_intent(intent.clone());
                    if is_explicit {
                        explicit_intent = Some((intent, root, sid));
                    }
                }
                if session.should_save() {
                    session.prepare_save().ok()
                } else {
                    None
                }
            };

            if let Some(prepared) = pending_session_save {
                let ir_clone = self.context_ir.clone();
                tokio::task::spawn_blocking(move || {
                    let _ = prepared.write_to_disk();
                    if let Some(ir) = ir_clone {
                        if let Ok(ir_guard) = ir.try_read() {
                            ir_guard.save();
                        }
                    }
                });
            }

            if let Some((intent, root, session_id)) = explicit_intent {
                let _ = crate::core::intent_protocol::apply_side_effects(
                    &intent,
                    root.as_deref(),
                    &session_id,
                );
            }

            if self.autonomy.is_enabled() {
                let (calls, project_root) = {
                    let session = self.session.read().await;
                    (session.stats.total_tool_calls, session.project_root.clone())
                };

                if let Some(root) = project_root {
                    if crate::tools::autonomy::should_auto_consolidate(&self.autonomy, calls) {
                        let root_clone = root.clone();
                        tokio::task::spawn_blocking(move || {
                            let _ = crate::core::consolidation_engine::consolidate_latest(
                                &root_clone,
                                crate::core::consolidation_engine::ConsolidationBudgets::default(),
                            );
                        });
                    }
                }
            }

            let agent_key = agent_id.unwrap_or_else(|| "unknown".to_string());
            let input_token_count = crate::core::tokens::count_tokens(&input) as u64;
            let output_token_count_u64 = output_token_count as u64;
            let name_owned = name.to_string();
            tokio::task::spawn_blocking(move || {
                let pricing = crate::core::gain::model_pricing::ModelPricing::load();
                let quote = pricing.quote_from_env_or_agent_type(&client_name);
                let cost_usd =
                    quote
                        .cost
                        .estimate_usd(input_token_count, output_token_count_u64, 0, 0);
                crate::core::budget_tracker::BudgetTracker::global().record_cost_usd(cost_usd);

                let mut store = crate::core::a2a::cost_attribution::CostStore::load();
                store.record_tool_call(
                    &agent_key,
                    &client_name,
                    &name_owned,
                    input_token_count,
                    output_token_count_u64,
                    0,
                );
                if let Err(e) = store.save() {
                    tracing::warn!("lean-ctx: failed to persist cost attribution: {e}");
                }
            });
        }

        // Context Bus: conflict detection for knowledge writes in shared mode.
        if self.session_mode == crate::tools::SessionMode::Shared
            && name == "ctx_knowledge"
            && action.as_deref() == Some("remember")
        {
            if let Some(ref rt) = self.context_os {
                let my_agent = self.agent_id.read().await.clone();
                let category = helpers::get_str(args, "category");
                let key = helpers::get_str(args, "key");
                if let (Some(ref cat), Some(ref k)) = (&category, &key) {
                    let recent = rt.bus.recent_by_kind(
                        &self.workspace_id,
                        &self.channel_id,
                        "knowledge_remembered",
                        20,
                    );
                    for ev in &recent {
                        let p = &ev.payload;
                        let ev_cat = p.get("category").and_then(|v| v.as_str());
                        let ev_key = p.get("key").and_then(|v| v.as_str());
                        let ev_actor = ev.actor.as_deref();
                        if ev_cat == Some(cat.as_str())
                            && ev_key == Some(k.as_str())
                            && ev_actor != my_agent.as_deref()
                        {
                            let other = ev_actor.unwrap_or("unknown");
                            result_text = format!(
                                "[CONFLICT] Agent '{other}' recently wrote to the same knowledge key \
                                 '{cat}/{k}'. Review before proceeding.\n\n{result_text}"
                            );
                            break;
                        }
                    }
                }
            }
        }

        // Context OS: persist shared session + publish events.
        if self.session_mode == crate::tools::SessionMode::Shared {
            let ws = self.workspace_id.clone();
            let ch = self.channel_id.clone();
            let rt = self.context_os.clone();
            let agent = self.agent_id.read().await.clone();
            let tool = name.to_string();
            let tool_action = action.clone();
            let tool_path = helpers::get_str(args, "path");
            let tool_category = helpers::get_str(args, "category");
            let tool_key = helpers::get_str(args, "key");
            let session_snapshot = self.session.read().await.clone();
            let session_task = session_snapshot.task.clone();
            tokio::task::spawn_blocking(move || {
                let Some(rt) = rt else {
                    return;
                };
                let Some(root) = session_snapshot.project_root.as_deref() else {
                    return;
                };
                rt.shared_sessions
                    .persist_best_effort(root, &ws, &ch, &session_snapshot);
                rt.metrics.record_session_persisted();

                let mut base_payload = serde_json::json!({
                    "tool": tool,
                    "action": tool_action,
                });
                if let Some(ref p) = tool_path {
                    base_payload["path"] = serde_json::Value::String(p.clone());
                }
                if let Some(ref c) = tool_category {
                    base_payload["category"] = serde_json::Value::String(c.clone());
                }
                if let Some(ref k) = tool_key {
                    base_payload["key"] = serde_json::Value::String(k.clone());
                }
                if let Some(ref t) = session_task {
                    base_payload["reasoning"] = serde_json::Value::String(t.description.clone());
                }

                if rt
                    .bus
                    .append(
                        &ws,
                        &ch,
                        &crate::core::context_os::ContextEventKindV1::ToolCallRecorded,
                        agent.as_deref(),
                        base_payload.clone(),
                    )
                    .is_some()
                {
                    rt.metrics.record_event_appended();
                    rt.metrics.record_event_broadcast();
                }

                if let Some(secondary) =
                    crate::core::context_os::secondary_event_kind(&tool, tool_action.as_deref())
                {
                    if rt
                        .bus
                        .append(&ws, &ch, &secondary, agent.as_deref(), base_payload)
                        .is_some()
                    {
                        rt.metrics.record_event_appended();
                        rt.metrics.record_event_broadcast();
                    }
                }
            });
        }

        let skip_checkpoint = minimal
            || matches!(
                name,
                "ctx_compress"
                    | "ctx_metrics"
                    | "ctx_benchmark"
                    | "ctx_analyze"
                    | "ctx_cache"
                    | "ctx_discover"
                    | "ctx_dedup"
                    | "ctx_session"
                    | "ctx_knowledge"
                    | "ctx_agent"
                    | "ctx_share"
                    | "ctx_gain"
                    | "ctx_overview"
                    | "ctx_preload"
                    | "ctx_cost"
                    | "ctx_heatmap"
                    | "ctx_task"
                    | "ctx_impact"
                    | "ctx_architecture"
                    | "ctx_smells"
                    | "ctx_workflow"
            );

        if !skip_checkpoint && self.increment_and_check() {
            if let Some(checkpoint) = self.auto_checkpoint().await {
                let interval = LeanCtxServer::checkpoint_interval_effective();
                let hints = crate::core::profiles::active_profile().output_hints;
                if hints.checkpoint_in_output() && crate::core::protocol::meta_visible() {
                    let combined = format!(
                        "{result_text}\n\n--- AUTO CHECKPOINT (every {interval} calls) ---\n{checkpoint}"
                    );
                    return Ok(CallToolResult::success(vec![Content::text(combined)]));
                }
            }
        }

        let tool_duration_ms = tool_start.elapsed().as_millis() as u64;
        if tool_duration_ms > 100 {
            LeanCtxServer::append_tool_call_log(
                name,
                tool_duration_ms,
                0,
                0,
                None,
                &chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            );
        }

        let current_count = self.call_count.load(std::sync::atomic::Ordering::Relaxed);
        if current_count > 0 && current_count.is_multiple_of(100) {
            std::thread::spawn(crate::cloud_sync::cloud_background_tasks);
        }

        Ok(CallToolResult::success(vec![Content::text(result_text)]))
    }

    /// Resolve project root from MCP client roots (once per session).
    /// Called on the first tool call. If the client supports `roots/list`,
    /// we query it and pick the best root with project markers.
    async fn resolve_roots_once(&self) {
        use std::sync::atomic::Ordering;
        if !self.has_client_roots.load(Ordering::Relaxed) {
            return;
        }
        if self.roots_resolved.swap(true, Ordering::Relaxed) {
            return;
        }
        let peer_guard = self.peer.read().await;
        let Some(peer) = peer_guard.as_ref() else {
            return;
        };
        let list_result = match peer.list_roots().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("roots/list failed: {e}");
                return;
            }
        };
        drop(peer_guard);

        let uris: Vec<String> = list_result.roots.iter().map(|r| r.uri.clone()).collect();
        let validated_paths = roots::valid_dir_paths_from_uris(&uris);
        let Some(new_root) = roots::best_root_from_uris(&uris) else {
            return;
        };
        // Defense-in-depth: never adopt a broad/unsafe root (HOME, `/`, agent
        // sandbox dirs) even if the client reports it — it would pollute the
        // session and resolve relative paths against the wrong tree.
        if crate::core::pathutil::is_broad_or_unsafe_root(std::path::Path::new(&new_root)) {
            tracing::warn!("MCP roots: ignoring unsafe project root {new_root}");
            return;
        }

        let mut session = self.session.write().await;
        let old_root = session.project_root.clone();

        let other_roots: Vec<String> = validated_paths
            .iter()
            .filter(|p| p.as_str() != new_root)
            .cloned()
            .collect();
        if !other_roots.is_empty() {
            session.extra_roots = other_roots;
            tracing::info!(
                "MCP roots: {} extra root(s) registered",
                session.extra_roots.len()
            );
        }

        if old_root.as_deref() == Some(&new_root) {
            let _ = session.save();
            return;
        }
        tracing::info!(
            "MCP roots: switching project root from {:?} to {new_root}",
            old_root
        );
        if let Some(existing) =
            crate::core::session::SessionState::load_latest_for_project_root(&new_root)
        {
            *session = existing;
            session.extra_roots = validated_paths
                .iter()
                .filter(|p| p.as_str() != new_root)
                .cloned()
                .collect();
        }
        session.project_root = Some(new_root);
        let _ = session.save();
    }
}

pub fn build_instructions_for_test(crp_mode: CrpMode) -> String {
    crate::instructions::build_instructions_for_test(crp_mode)
}

pub fn build_claude_code_instructions_for_test() -> String {
    crate::instructions::claude_code_instructions()
}

fn is_home_or_agent_dir(dir: &std::path::Path) -> bool {
    if let Some(home) = dirs::home_dir() {
        if dir == home {
            return true;
        }
    }
    let dir_str = dir.to_string_lossy();
    dir_str.ends_with("/.claude")
        || dir_str.ends_with("/.codex")
        || dir_str.contains("/.claude/")
        || dir_str.contains("/.codex/")
}

fn git_toplevel_from(dir: &std::path::Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

pub fn derive_project_root_from_cwd() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let canonical = crate::core::pathutil::safe_canonicalize_or_self(&cwd);

    if is_home_or_agent_dir(&canonical) {
        return git_toplevel_from(&canonical);
    }

    if has_project_marker(&canonical) {
        return Some(canonical.to_string_lossy().to_string());
    }

    if let Some(git_root) = git_toplevel_from(&canonical) {
        return Some(git_root);
    }

    if let Some(root) = detect_multi_root_workspace(&canonical) {
        return Some(root);
    }

    // Fallback: use CWD as project root if it's a specific, safe directory.
    // This ensures bare directories (no .git, no markers) still work.
    // Guard: reject home dir, filesystem root, and agent sandbox dirs.
    if !crate::core::pathutil::is_broad_or_unsafe_root(&canonical) {
        tracing::info!(
            "No project markers found — using CWD as project root: {}",
            canonical.display()
        );
        return Some(canonical.to_string_lossy().to_string());
    }

    None
}

// Delegated to crate::core::pathutil::is_broad_or_unsafe_root
#[cfg(test)]
use crate::core::pathutil::is_broad_or_unsafe_root;

/// Detect a multi-root workspace: a directory that has no project markers
/// itself, but contains child directories that do. In this case, use the
/// parent as jail root and auto-allow all child projects via LEAN_CTX_ALLOW_PATH.
fn detect_multi_root_workspace(dir: &std::path::Path) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut child_projects: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && has_project_marker(&path) {
            let canonical = crate::core::pathutil::safe_canonicalize_or_self(&path);
            child_projects.push(canonical.to_string_lossy().to_string());
        }
    }

    if child_projects.len() >= 2 {
        let existing = std::env::var("LEAN_CTX_ALLOW_PATH").unwrap_or_default();
        let sep = if cfg!(windows) { ";" } else { ":" };
        let merged = if existing.is_empty() {
            child_projects.join(sep)
        } else {
            format!("{existing}{sep}{}", child_projects.join(sep))
        };
        std::env::set_var("LEAN_CTX_ALLOW_PATH", &merged);
        tracing::info!(
            "Multi-root workspace detected at {}: auto-allowing {} child projects",
            dir.display(),
            child_projects.len()
        );
        return Some(dir.to_string_lossy().to_string());
    }

    None
}

pub fn tool_descriptions_for_test() -> Vec<(&'static str, &'static str)> {
    crate::tool_defs::list_all_tool_defs()
        .into_iter()
        .map(|(name, desc, _)| (name, desc))
        .collect()
}

pub fn tool_schemas_json_for_test() -> String {
    crate::tool_defs::list_all_tool_defs()
        .iter()
        .map(|(name, _, schema)| format!("{name}: {schema}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tools that always pass through the workflow gate regardless of state.
/// Read-only tools should never be blocked — agents need them for context
/// recovery after crashes or session transitions.
pub const WORKFLOW_PASSTHROUGH_TOOLS: &[&str] = &[
    "ctx",
    "ctx_workflow",
    "ctx_read",
    "ctx_multi_read",
    "ctx_smart_read",
    "ctx_search",
    "ctx_tree",
    "ctx_session",
    "ctx_ledger",
];

/// A workflow is stale if it hasn't been updated in 30 minutes.
/// This prevents dead workflows from blocking tools across sessions.
pub fn is_workflow_stale(run: &crate::core::workflow::types::WorkflowRun) -> bool {
    let elapsed = chrono::Utc::now()
        .signed_duration_since(run.updated_at)
        .num_minutes();
    elapsed > 30
}

fn is_shell_tool_name(name: &str) -> bool {
    matches!(name, "ctx_shell" | "ctx_execute")
}

fn extract_file_read_from_shell(cmd: &str) -> Option<String> {
    let trimmed = cmd.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let bin = parts[0].rsplit('/').next().unwrap_or(parts[0]);
    match bin {
        "cat" | "head" | "tail" | "less" | "more" | "bat" | "batcat" => {
            let file_arg = parts.iter().skip(1).find(|a| !a.starts_with('-'))?;
            Some(file_arg.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_markers_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("myproject");
        std::fs::create_dir_all(&root).unwrap();
        assert!(!has_project_marker(&root));

        std::fs::create_dir(root.join(".git")).unwrap();
        assert!(has_project_marker(&root));
    }

    #[test]
    fn home_dir_detected_as_agent_dir() {
        if let Some(home) = dirs::home_dir() {
            assert!(is_home_or_agent_dir(&home));
        }
    }

    #[test]
    fn agent_dirs_detected() {
        let claude = std::path::PathBuf::from("/home/user/.claude");
        assert!(is_home_or_agent_dir(&claude));
        let codex = std::path::PathBuf::from("/home/user/.codex");
        assert!(is_home_or_agent_dir(&codex));
        let project = std::path::PathBuf::from("/home/user/projects/myapp");
        assert!(!is_home_or_agent_dir(&project));
    }

    #[test]
    fn test_unified_tool_count() {
        let tools = crate::tool_defs::unified_tool_defs();
        assert_eq!(tools.len(), 5, "Expected 5 unified tools");
    }

    #[test]
    fn test_granular_tool_count() {
        let tools = crate::tool_defs::granular_tool_defs();
        assert!(tools.len() >= 25, "Expected at least 25 granular tools");
    }

    #[test]
    fn test_registry_tool_count_ssot() {
        let registry = crate::server::registry::build_registry();
        assert_eq!(
            registry.len(),
            63,
            "Registry tool count drift! Update this test AND all docs when adding/removing tools."
        );
    }

    #[test]
    fn disabled_tools_filters_list() {
        let all = crate::tool_defs::granular_tool_defs();
        let total = all.len();
        let disabled = ["ctx_graph".to_string(), "ctx_agent".to_string()];
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|t| !disabled.iter().any(|d| t.name.as_ref() == d.as_str()))
            .collect();
        assert_eq!(filtered.len(), total - 2);
        assert!(!filtered.iter().any(|t| t.name.as_ref() == "ctx_graph"));
        assert!(!filtered.iter().any(|t| t.name.as_ref() == "ctx_agent"));
    }

    #[test]
    fn empty_disabled_tools_returns_all() {
        let all = crate::tool_defs::granular_tool_defs();
        let total = all.len();
        let disabled: Vec<String> = vec![];
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|t| !disabled.iter().any(|d| t.name.as_ref() == d.as_str()))
            .collect();
        assert_eq!(filtered.len(), total);
    }

    #[test]
    fn misspelled_disabled_tool_is_silently_ignored() {
        let all = crate::tool_defs::granular_tool_defs();
        let total = all.len();
        let disabled = ["ctx_nonexistent_tool".to_string()];
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|t| !disabled.iter().any(|d| t.name.as_ref() == d.as_str()))
            .collect();
        assert_eq!(filtered.len(), total);
    }

    #[test]
    fn detect_multi_root_workspace_with_child_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        let proj_a = workspace.join("project-a");
        let proj_b = workspace.join("project-b");
        std::fs::create_dir_all(proj_a.join(".git")).unwrap();
        std::fs::create_dir_all(&proj_b).unwrap();
        std::fs::write(proj_b.join("package.json"), "{}").unwrap();

        let result = detect_multi_root_workspace(&workspace);
        assert!(
            result.is_some(),
            "should detect workspace with 2 child projects"
        );

        std::env::remove_var("LEAN_CTX_ALLOW_PATH");
    }

    #[test]
    fn detect_multi_root_workspace_returns_none_for_single_project() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        let proj_a = workspace.join("project-a");
        std::fs::create_dir_all(proj_a.join(".git")).unwrap();

        let result = detect_multi_root_workspace(&workspace);
        assert!(
            result.is_none(),
            "should not detect workspace with only 1 child project"
        );
    }

    #[test]
    fn is_broad_or_unsafe_root_rejects_home() {
        if let Some(home) = dirs::home_dir() {
            assert!(is_broad_or_unsafe_root(&home));
        }
    }

    #[test]
    fn is_broad_or_unsafe_root_rejects_filesystem_root() {
        assert!(is_broad_or_unsafe_root(std::path::Path::new("/")));
    }

    #[test]
    fn is_broad_or_unsafe_root_rejects_agent_dirs() {
        assert!(is_broad_or_unsafe_root(std::path::Path::new(
            "/home/user/.claude"
        )));
        assert!(is_broad_or_unsafe_root(std::path::Path::new(
            "/home/user/.codex"
        )));
    }

    #[test]
    fn is_broad_or_unsafe_root_allows_project_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let subdir = tmp.path().join("my-project");
        std::fs::create_dir_all(&subdir).unwrap();
        assert!(!is_broad_or_unsafe_root(&subdir));
    }

    #[test]
    fn is_broad_or_unsafe_root_allows_tmp_subdirs() {
        assert!(!is_broad_or_unsafe_root(std::path::Path::new(
            "/tmp/leanctx-test"
        )));
        assert!(!is_broad_or_unsafe_root(std::path::Path::new(
            "/tmp/my-project"
        )));
    }

    #[test]
    fn is_broad_or_unsafe_root_allows_home_subdirs() {
        if let Some(home) = dirs::home_dir() {
            let subdir = home.join("projects").join("my-app");
            assert!(!is_broad_or_unsafe_root(&subdir));
        }
    }

    #[test]
    fn derive_project_root_falls_back_to_bare_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = tmp.path().join("bare-dir");
        std::fs::create_dir_all(&bare).unwrap();

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&bare).unwrap();
        let result = derive_project_root_from_cwd();
        std::env::set_current_dir(original).unwrap();

        assert!(result.is_some(), "bare dir should produce a project root");
        let root = result.unwrap();
        assert!(
            root.contains("bare-dir"),
            "fallback should use the bare dir path"
        );
    }
}
