//! OpenRouter usage accounting — the request-side half of measured cost
//! (#1179).
//!
//! OpenRouter only reports the billed charge (`usage.cost`, and
//! `cost_details.upstream_inference_cost` for BYOK) when the request opts in
//! with `"usage": {"include": true}`. This module decides *when* the proxy may
//! inject that opt-in and performs the injection.
//!
//! The gate is deliberately narrow: `usage` is not an OpenAI Chat Completions
//! parameter — api.openai.com rejects unknown top-level fields with a 400, and
//! other OpenAI-compatible upstreams (Azure, Groq, vLLM…) are not guaranteed
//! to tolerate it either. Injection therefore only happens when the *effective*
//! upstream host (post-routing) is openrouter.ai.

use serde_json::Value;

/// True when `base_url` points at OpenRouter — the only upstream documented to
/// accept (and reward) the `usage.include` opt-in.
pub(super) fn upstream_is_openrouter(base_url: &str) -> bool {
    let rest = base_url
        .strip_prefix("https://")
        .or_else(|| base_url.strip_prefix("http://"))
        .unwrap_or(base_url);
    let host_port = rest.split(['/', '?']).next().unwrap_or(rest);
    let host = host_port.split(':').next().unwrap_or(host_port);
    host.eq_ignore_ascii_case("openrouter.ai")
        || host.to_ascii_lowercase().ends_with(".openrouter.ai")
}

/// Injects `"usage": {"include": true}` into an OpenAI-shaped Chat Completions
/// body so OpenRouter's final usage payload carries the billed `cost`.
///
/// Respects the caller: an existing `usage.include` (even `false`) is never
/// overwritten, and a non-object `usage` value is left untouched.
pub(super) fn inject_usage_include(doc: &mut Value) {
    let Some(obj) = doc.as_object_mut() else {
        return;
    };
    let usage = obj
        .entry("usage")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(usage_obj) = usage.as_object_mut() {
        usage_obj.entry("include").or_insert(Value::Bool(true));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_hosts_are_recognized() {
        for url in [
            "https://openrouter.ai/api",
            "https://openrouter.ai",
            "http://openrouter.ai:443/api",
            "https://gateway.openrouter.ai/api",
            "https://OPENROUTER.AI/api",
        ] {
            assert!(
                upstream_is_openrouter(url),
                "{url} must count as OpenRouter"
            );
        }
    }

    #[test]
    fn other_upstreams_are_not_openrouter() {
        for url in [
            "https://api.openai.com",
            "https://my-resource.services.ai.azure.com",
            "https://api.groq.com/openai",
            "http://127.0.0.1:11434",
            "https://evil-openrouter.ai.example.com",
            "https://notopenrouter.ai",
        ] {
            assert!(
                !upstream_is_openrouter(url),
                "{url} must NOT count as OpenRouter"
            );
        }
    }

    #[test]
    fn injects_usage_include_when_absent() {
        let mut doc = serde_json::json!({"model": "deepseek/deepseek-v4-flash", "messages": []});
        inject_usage_include(&mut doc);
        assert_eq!(doc["usage"]["include"], Value::Bool(true));
    }

    #[test]
    fn existing_opt_out_is_respected() {
        let mut doc = serde_json::json!({"model": "m", "usage": {"include": false}});
        inject_usage_include(&mut doc);
        assert_eq!(doc["usage"]["include"], Value::Bool(false));
    }

    #[test]
    fn non_object_usage_is_left_untouched() {
        let mut doc = serde_json::json!({"model": "m", "usage": true});
        inject_usage_include(&mut doc);
        assert_eq!(doc["usage"], Value::Bool(true));
    }
}
