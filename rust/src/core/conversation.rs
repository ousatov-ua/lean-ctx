//! Conversation identity for read-cache scoping.
//!
//! The `[unchanged]` re-read stub means *"you already have this in context"* —
//! which is only true within the **same conversation / context window**. The
//! read [`SessionCache`](crate::core::cache::SessionCache) is shared across all
//! chats served by one daemon, so without scoping a file delivered in chat A
//! could be stubbed for a re-read in chat B (which never received it).
//!
//! Cursor's hooks write the live conversation id to `active_transcript.json`
//! (2h TTL); we read it via [`crate::hook_handlers::load_active_transcript`] but
//! cache it behind a short TTL so the read hot path never stats+parses a file on
//! every call. The last-known-good value is retained across a transient refresh
//! miss, so a momentary read failure never spuriously invalidates valid stubs.
//!
//! Disabled with `LEAN_CTX_CONVERSATION_SCOPE=0` (falls back to the legacy
//! process-scoped behavior).

use std::sync::OnceLock;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// How long a resolved conversation id stays fresh before we re-read the file.
const REFRESH_TTL: Duration = Duration::from_secs(3);

struct Cached {
    value: Option<String>,
    refreshed_at: Instant,
}

fn store() -> &'static RwLock<Option<Cached>> {
    static STORE: OnceLock<RwLock<Option<Cached>>> = OnceLock::new();
    STORE.get_or_init(|| RwLock::new(None))
}

fn scope_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("LEAN_CTX_CONVERSATION_SCOPE")
                .ok()
                .as_deref()
                .map(str::trim),
            Some("0" | "false" | "off")
        )
    })
}

/// The current conversation id, or `None` when no conversation context is
/// available (hooks not installed, TTL expired with no prior value, or scoping
/// disabled). `None` preserves the legacy process-scoped cache behavior.
pub fn current_conversation_id() -> Option<String> {
    if !scope_enabled() {
        return None;
    }
    if let Ok(guard) = store().read()
        && let Some(cached) = guard.as_ref()
        && cached.refreshed_at.elapsed() < REFRESH_TTL
    {
        return cached.value.clone();
    }
    refresh()
}

fn refresh() -> Option<String> {
    let fresh = crate::hook_handlers::load_active_transcript().and_then(|(_, conv)| conv);
    if let Ok(mut guard) = store().write() {
        // Retain last-known-good: a transient miss (file briefly absent or
        // expired) must not flip a stable conversation to `None` and force
        // needless cold re-reads.
        if fresh.is_none()
            && let Some(existing) = guard.as_ref()
            && existing.value.is_some()
        {
            let kept = existing.value.clone();
            *guard = Some(Cached {
                value: kept.clone(),
                refreshed_at: Instant::now(),
            });
            return kept;
        }
        *guard = Some(Cached {
            value: fresh.clone(),
            refreshed_at: Instant::now(),
        });
    }
    fresh
}

/// Whether a `[unchanged]` stub may be served for an entry that was delivered to
/// `delivered`, given the `current` conversation.
///
/// `current == None` (no conversation context) preserves the legacy
/// process-scoped behavior — stub allowed. Otherwise the stub is only allowed
/// when the current conversation *is* the one that received the full content.
pub fn conversation_allows_stub(current: Option<&str>, delivered: Option<&str>) -> bool {
    match current {
        None => true,
        Some(cur) => delivered == Some(cur),
    }
}

/// Whether a *cold* `[unchanged]` stub may be served — i.e. one backed only by
/// the persisted index ([`crate::core::read_stub_index`]) after a daemon
/// restart, with no live in-memory entry.
///
/// Stricter than [`conversation_allows_stub`]: a cold stub crosses a process
/// boundary, so we serve it **only** when both sides name the *same, known*
/// conversation. Unlike the warm path there is no "no context → legacy" escape,
/// because without a current conversation id we cannot prove the content is in
/// the new process's context, and a wrong cold stub would resurrect exactly the
/// cross-chat hazard #954 closed.
pub fn conversation_allows_cold_stub(current: Option<&str>, delivered: Option<&str>) -> bool {
    matches!((current, delivered), (Some(c), Some(d)) if c == d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_current_context_allows_stub_legacy() {
        // Without conversation context we must behave exactly as before.
        assert!(conversation_allows_stub(None, None));
        assert!(conversation_allows_stub(None, Some("conv-a")));
    }

    #[test]
    fn same_conversation_allows_stub() {
        assert!(conversation_allows_stub(Some("conv-a"), Some("conv-a")));
    }

    #[test]
    fn different_conversation_blocks_stub() {
        assert!(!conversation_allows_stub(Some("conv-b"), Some("conv-a")));
    }

    #[test]
    fn unknown_delivering_conversation_blocks_stub() {
        // Entry delivered before scoping existed → cannot prove it is in context.
        assert!(!conversation_allows_stub(Some("conv-a"), None));
    }

    #[test]
    fn cold_stub_requires_both_known_and_matching() {
        assert!(conversation_allows_cold_stub(Some("c"), Some("c")));
        assert!(!conversation_allows_cold_stub(Some("c"), Some("d")));
        // No "legacy" escape for the cold path: unknown either side → blocked.
        assert!(!conversation_allows_cold_stub(None, Some("c")));
        assert!(!conversation_allows_cold_stub(Some("c"), None));
        assert!(!conversation_allows_cold_stub(None, None));
    }
}
