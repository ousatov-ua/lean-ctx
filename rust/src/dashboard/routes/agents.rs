use std::collections::HashMap;

pub(super) fn handle(
    path: &str,
    _query_str: &str,
    _method: &str,
    _body: &str,
) -> Option<(&'static str, &'static str, String)> {
    match path {
        "/api/mcp" => {
            let json = build_mcp_tools_json();
            Some(("200 OK", "application/json", json))
        }
        "/api/agents" => {
            let json = build_agents_json();
            Some(("200 OK", "application/json", json))
        }
        "/api/events" => {
            let evs = crate::core::events::load_events_from_file(200);
            let json = serde_json::to_string(&evs).unwrap_or_else(|_| "[]".to_string());
            Some(("200 OK", "application/json", json))
        }
        p if p.starts_with("/api/events/") => {
            let id_str = &p["/api/events/".len()..];
            if let Ok(id) = id_str.parse::<u64>() {
                let evs = crate::core::events::load_events_from_file(500);
                if let Some(ev) = evs.iter().find(|e| e.id == id) {
                    let json = serde_json::to_string(ev).unwrap_or_else(|_| "{}".to_string());
                    Some(("200 OK", "application/json", json))
                } else {
                    Some((
                        "404 Not Found",
                        "application/json",
                        "{\"error\":\"event not found\"}".to_string(),
                    ))
                }
            } else {
                Some((
                    "400 Bad Request",
                    "application/json",
                    "{\"error\":\"invalid event id\"}".to_string(),
                ))
            }
        }
        _ => None,
    }
}

fn build_agents_json() -> String {
    let mut registry = crate::core::agents::AgentRegistry::load_or_create();
    registry.cleanup_stale(24);
    let _ = registry.save();

    let mut agents: Vec<serde_json::Value> = registry
        .agents
        .iter()
        .filter(|a| {
            a.status != crate::core::agents::AgentStatus::Finished
                && crate::core::agents::is_process_alive(a.pid)
        })
        .map(|a| {
            let age_min = (chrono::Utc::now() - a.last_active).num_minutes();
            serde_json::json!({
                "id": a.agent_id,
                "type": a.agent_type,
                "role": a.role,
                "status": format!("{}", a.status),
                "status_message": a.status_message,
                "last_active_minutes_ago": age_min,
                "pid": a.pid
            })
        })
        .collect();

    if agents.is_empty() {
        agents = infer_agents_from_events();
    }

    let pending_msgs = registry.scratchpad.len();

    let shared_dir = crate::core::data_dir::lean_ctx_data_dir()
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".lean-ctx"))
        .join("agents")
        .join("shared");
    let shared_count = if shared_dir.exists() {
        std::fs::read_dir(&shared_dir).map_or(0, std::iter::Iterator::count)
    } else {
        0
    };

    serde_json::json!({
        "agents": agents,
        "total_active": agents.len(),
        "pending_messages": pending_msgs,
        "shared_contexts": shared_count
    })
    .to_string()
}

/// Event timestamps are written by `events.rs` with `chrono::Local::now()` and
/// carry no offset, so they MUST be interpreted as *local* time. Reading them
/// as UTC made agents appear "active 2h in the future" on UTC+2 machines
/// (`last_active_minutes_ago = -119`, GL #479 D3).
fn local_event_ts_to_utc(ts: chrono::NaiveDateTime) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone as _;
    match chrono::Local.from_local_datetime(&ts) {
        chrono::LocalResult::Single(t) | chrono::LocalResult::Ambiguous(t, _) => {
            t.with_timezone(&chrono::Utc)
        }
        // A nonexistent local time (DST spring-forward gap): fall back to UTC,
        // which is at most one DST shift off — never negative by hours.
        chrono::LocalResult::None => ts.and_utc(),
    }
}

fn infer_agents_from_events() -> Vec<serde_json::Value> {
    let evts = crate::core::events::load_events_from_file(200);
    let now = chrono::Utc::now();
    let cutoff = now - chrono::Duration::minutes(30);

    let mut recent_tool_count: u64 = 0;
    let mut latest_ts: Option<chrono::NaiveDateTime> = None;

    for ev in &evts {
        let ts_str = &ev.timestamp;
        if let Ok(ts) = chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H:%M:%S%.f") {
            let aware = local_event_ts_to_utc(ts);
            if aware >= cutoff {
                if matches!(&ev.kind, crate::core::events::EventKind::ToolCall { .. }) {
                    recent_tool_count += 1;
                }
                if latest_ts.is_none_or(|prev| ts > prev) {
                    latest_ts = Some(ts);
                }
            }
        }
    }

    if recent_tool_count == 0 {
        return Vec::new();
    }

    // Clamp at zero: clock skew must never render a negative age.
    let age_min = latest_ts.map_or(0, |ts| {
        (now - local_event_ts_to_utc(ts)).num_minutes().max(0)
    });

    // #717: same freshness threshold as /api/workspaces — the two panels
    // used to disagree ("active" here, "idle" there) for the same session.
    let status = if age_min < 10 { "active" } else { "idle" };

    vec![serde_json::json!({
        "id": "lean-ctx-session",
        "type": "lean-ctx",
        "role": "context-engine",
        "status": status,
        "status_message": format!("{} tool calls in last 30min", recent_tool_count),
        "last_active_minutes_ago": age_min,
        "pid": std::process::id(),
        "inferred": true
    })]
}

fn build_mcp_tools_json() -> String {
    // All-time per-tool aggregates from stats.json — the same source Home and
    // ROI use. The event log only holds the last N events, which silently
    // understated these counters as a pseudo all-time view (#492).
    let store = crate::core::stats::load_for_display();

    let mut tool_stats: HashMap<String, ToolAgg> = HashMap::new();

    for (name, cmd) in &store.commands {
        let entry = tool_stats.entry(name.clone()).or_default();
        entry.calls += cmd.count;
        entry.tokens_saved += cmd.input_tokens.saturating_sub(cmd.output_tokens);
        entry.tokens_original += cmd.input_tokens;
    }

    let known_tools: &[(&str, &str)] = &[
        ("ctx_read", "Read files with 10 compression modes"),
        ("ctx_search", "Search code with compact results"),
        ("ctx_shell", "Shell commands with pattern compression"),
        ("ctx_tree", "Compact directory maps"),
        ("ctx_overview", "Project overview with dependency graph"),
        ("ctx_session", "Session management and state tracking"),
        ("ctx_compress", "Compress context when budget is tight"),
        ("ctx_metrics", "Token savings and performance metrics"),
        ("ctx_control", "Context overlays: pin, exclude, priority"),
        ("ctx_plan", "Context-aware planning with budget estimation"),
    ];

    let mut tools: Vec<serde_json::Value> = Vec::new();

    for &(name, description) in known_tools {
        let stats = tool_stats.remove(name);
        let (calls, saved, original) =
            stats.map_or((0, 0, 0), |s| (s.calls, s.tokens_saved, s.tokens_original));
        tools.push(serde_json::json!({
            "name": name,
            "description": description,
            "call_count": calls,
            "tokens_saved": saved,
            "tokens_original": original
        }));
    }

    for (name, stats) in &tool_stats {
        tools.push(serde_json::json!({
            "name": name,
            "description": "",
            "call_count": stats.calls,
            "tokens_saved": stats.tokens_saved,
            "tokens_original": stats.tokens_original
        }));
    }

    serde_json::json!({ "tools": tools }).to_string()
}

#[derive(Default)]
struct ToolAgg {
    calls: u64,
    tokens_saved: u64,
    tokens_original: u64,
}

#[cfg(test)]
mod tests {
    use super::local_event_ts_to_utc;

    /// GL #479 D3: event timestamps are local wall-clock strings; interpreting
    /// "now" as local must yield an age of ~0 — not a negative UTC-offset age.
    #[test]
    fn local_event_ts_age_is_never_hours_in_the_future() {
        let now_local = chrono::Local::now().naive_local();
        let aware = local_event_ts_to_utc(now_local);
        let age_min = (chrono::Utc::now() - aware).num_minutes();
        assert!(
            (-1..=1).contains(&age_min),
            "a just-written event must be ~0 minutes old, got {age_min}"
        );
    }
}
