//! Thin integration layer connecting response shaping + conversation
//! compression into the proxy forward path.
//!
//! Keeps `forward.rs` below LOC gate while providing the full pipeline:
//! 1. Response shaping (strip preambles/confirmations from LLM output)
//! 2. Conversation compression (score+tier messages on long conversations)
//!
//! Both are gated on `Config::response_shaping_mode` / `Config::conversation_compression`.

use super::conversation;
use super::response_shaper::{self, ShapingMode, ShapingResult};

/// Apply response shaping to non-streaming response bytes.
/// Returns shaped bytes + tokens saved, or `None` if shaping didn't apply.
pub(crate) fn shape_response(resp_bytes: &[u8], mode: &str) -> Option<ShapingResult> {
    let shaping_mode = ShapingMode::from_str_config(mode);
    response_shaper::shape_response(resp_bytes, shaping_mode)
}

/// Compress conversation messages if the request exceeds thresholds.
/// Returns compressed messages array + savings stats, or `None` if not applicable.
pub(crate) fn compress_conversation(body_bytes: &[u8]) -> Option<(Vec<u8>, usize, usize, usize)> {
    let mut parsed: serde_json::Value = serde_json::from_slice(body_bytes).ok()?;

    let messages = parsed.get("messages")?.as_array()?.clone();
    let result = conversation::compress_messages(&messages)?;

    parsed["messages"] = serde_json::Value::Array(result.messages);
    let output = serde_json::to_vec(&parsed).ok()?;

    Some((
        output,
        result.tokens_saved,
        result.messages_summarized,
        result.messages_dropped,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_response_off_mode_returns_none() {
        let json = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "Great, I'll do that.\n\nDone.\n\nLet me know if you need help!"}}]
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        assert!(shape_response(&bytes, "off").is_none());
    }

    #[test]
    fn shape_response_gentle_strips_ceremony() {
        let json = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "Sure, let me check that.\n\nThe answer is 42.\n\nLet me know if you need anything else!"}}]
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let result = shape_response(&bytes, "gentle");
        assert!(result.is_some());
        let shaped: serde_json::Value = serde_json::from_slice(&result.unwrap().bytes).unwrap();
        let content = shaped["choices"][0]["message"]["content"].as_str().unwrap();
        assert!(!content.contains("Sure, let me"));
        assert!(!content.contains("Let me know"));
        assert!(content.contains("42"));
    }

    #[test]
    fn compress_conversation_below_threshold_returns_none() {
        let body = serde_json::json!({
            "model": "claude-4",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello"},
                {"role": "assistant", "content": "Hi!"}
            ]
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        assert!(compress_conversation(&bytes).is_none());
    }
}
