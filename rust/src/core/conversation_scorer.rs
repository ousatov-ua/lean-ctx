//! Pure message-importance scoring for proxy conversation compression (#1123).
//!
//! Scoring depends only on the supplied message array and positions. It does
//! not read configuration, the filesystem, clocks, or process-global state, so
//! identical requests always produce identical decisions.

use serde_json::Value;

const CHARS_PER_TOKEN: usize = 4;
const RECENCY_HALF_LIFE: f64 = 12.0;

/// Importance dimensions used by the tiered conversation compressor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MessageScore {
    pub recency: f64,
    pub reference_count: f64,
    pub action_density: f64,
    pub error_signal: f64,
    pub file_relevance: f64,
    pub decision_weight: f64,
}

impl MessageScore {
    /// Weighted score used by the compression tiers.
    #[must_use]
    pub(crate) fn total(self) -> f64 {
        self.recency * 0.30
            + self.reference_count * 0.10
            + self.action_density * 0.10
            + self.error_signal * 0.25
            + self.file_relevance * 0.05
            + self.decision_weight * 0.20
    }
}

/// Score one message against the complete conversation.
#[must_use]
pub(crate) fn score_message(msg: &Value, index: usize, messages: &[Value]) -> MessageScore {
    MessageScore {
        recency: score_recency(index, messages.len()),
        reference_count: score_reference_count(msg, index, messages),
        action_density: score_action_density(msg),
        error_signal: score_error_signal(msg),
        file_relevance: score_file_relevance(msg, messages),
        decision_weight: score_decision_weight(msg),
    }
}

/// Token estimate shared by the message threshold and savings accounting.
#[must_use]
pub(crate) fn message_token_estimate(msg: &Value) -> usize {
    count_tokens(&extract_text_content(msg)).saturating_add(4)
}

/// Text extraction supports OpenAI/Anthropic string and block-array content.
#[must_use]
pub(crate) fn extract_text_content(msg: &Value) -> String {
    let Some(content) = msg.get("content") else {
        return String::new();
    };
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    content
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| {
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .or_else(|| block.get("content").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// True for messages that must remain verbatim regardless of score.
#[must_use]
pub(crate) fn is_error_message(msg: &Value) -> bool {
    if msg.get("is_error").and_then(Value::as_bool) == Some(true) || msg.get("error").is_some() {
        return true;
    }
    let content = extract_text_content(msg).to_ascii_lowercase();
    [
        "error",
        "failed",
        "failure",
        "panic",
        "exception",
        "traceback",
        "stack trace",
        "segfault",
        "fatal",
    ]
    .iter()
    .any(|needle| content.contains(needle))
}

fn count_tokens(text: &str) -> usize {
    // The project tokenizer is deterministic and already used by context
    // accounting; retain a small fallback for empty content.
    crate::core::tokens::count_tokens(text).max(text.len() / CHARS_PER_TOKEN)
}

fn score_recency(index: usize, total: usize) -> f64 {
    if total == 0 || index >= total {
        return 0.0;
    }
    let age = (total - 1 - index) as f64;
    (-age / RECENCY_HALF_LIFE).exp()
}

fn score_error_signal(msg: &Value) -> f64 {
    if is_error_message(msg) { 1.0 } else { 0.0 }
}

fn score_decision_weight(msg: &Value) -> f64 {
    let content = extract_text_content(msg).to_ascii_lowercase();
    let indicators = [
        "i decided",
        "i chose",
        "the approach",
        "architecture",
        "trade-off",
        "instead of",
        "because",
        "the reason",
        "design decision",
        "we should",
        "the plan is",
    ];
    let matches = indicators
        .iter()
        .filter(|indicator| content.contains(*indicator))
        .count();
    (matches as f64 / 2.0).min(1.0)
}

fn score_file_relevance(msg: &Value, messages: &[Value]) -> f64 {
    let current_files = file_mentions(msg);
    if current_files.is_empty() {
        return 0.0;
    }
    let recent_start = messages.len().saturating_sub(10);
    let recent_files: Vec<String> = messages[recent_start..]
        .iter()
        .flat_map(file_mentions)
        .collect();
    if recent_files.is_empty() {
        return 0.25;
    }
    let relevant = current_files
        .iter()
        .filter(|file| recent_files.iter().any(|recent| recent == *file))
        .count();
    (relevant as f64 / current_files.len() as f64).min(1.0)
}

fn score_reference_count(msg: &Value, index: usize, messages: &[Value]) -> f64 {
    let content = extract_text_content(msg);
    if content.is_empty() || index + 1 >= messages.len() {
        return 0.0;
    }
    let anchors = reference_anchors(&content);
    if anchors.is_empty() {
        return 0.0;
    }
    let references = messages[index + 1..]
        .iter()
        .filter(|later| {
            let later_text = extract_text_content(later);
            anchors.iter().any(|anchor| later_text.contains(anchor))
        })
        .count();
    (references as f64 / 3.0).min(1.0)
}

fn score_action_density(msg: &Value) -> f64 {
    let role = msg.get("role").and_then(Value::as_str).unwrap_or_default();
    if role == "tool" {
        return 0.6;
    }
    let content = extract_text_content(msg);
    if content.is_empty() {
        return 0.0;
    }
    let code_blocks = content.matches("```").count() / 2;
    let commands = content.matches("cargo ").count()
        + content.matches("git ").count()
        + content.matches("apply_patch").count();
    let paths = file_mentions(msg).len();
    (code_blocks as f64 * 0.25 + commands as f64 * 0.15 + paths as f64 * 0.10).min(1.0)
}

fn reference_anchors(content: &str) -> Vec<String> {
    let mut anchors = file_mentions_from_text(content);
    let first_line = content.lines().next().unwrap_or_default().trim();
    if first_line.len() >= 16 {
        anchors.push(first_line.chars().take(80).collect());
    }
    anchors.sort();
    anchors.dedup();
    anchors
}

fn file_mentions(msg: &Value) -> Vec<String> {
    file_mentions_from_text(&extract_text_content(msg))
}

fn file_mentions_from_text(content: &str) -> Vec<String> {
    let mut files = content
        .split_whitespace()
        .filter_map(|word| {
            let cleaned = word.trim_matches(|c: char| {
                !c.is_alphanumeric() && !matches!(c, '/' | '\\' | '.' | '_' | '-')
            });
            let ext = std::path::Path::new(cleaned)
                .extension()
                .and_then(|ext| ext.to_str())?;
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "rs" | "ts" | "py" | "js" | "go" | "toml"
            )
            .then(|| cleaned.to_owned())
        })
        .filter(|file| file.len() > 3)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(role: &str, content: &str) -> Value {
        json!({"role": role, "content": content})
    }

    #[test]
    fn score_is_deterministic_and_has_all_dimensions() {
        let messages = vec![
            message(
                "user",
                "Inspect src/proxy/forward.rs and decide the approach.",
            ),
            message(
                "assistant",
                "The approach is to preserve src/proxy/forward.rs.",
            ),
        ];
        let score = score_message(&messages[0], 0, &messages);
        assert_eq!(score, score_message(&messages[0], 0, &messages));
        assert!(score.file_relevance >= 0.0);
        assert!(score.reference_count > 0.0);
    }

    #[test]
    fn errors_are_always_signalled() {
        let msg = message("tool", "command failed with error: permission denied");
        let score = score_message(&msg, 0, &[msg.clone()]);
        assert_eq!(score.error_signal, 1.0);
        assert!(is_error_message(&msg));
    }

    #[test]
    fn recent_messages_score_above_old_messages() {
        let messages = (0..20)
            .map(|i| message("user", &format!("message {i}")))
            .collect::<Vec<_>>();
        let old = score_message(&messages[0], 0, &messages).total();
        let recent = score_message(&messages[19], 19, &messages).total();
        assert!(recent > old);
    }
}
