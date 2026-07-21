//! Configuration for proxy response shaping (#1125).

use serde::{Deserialize, Serialize};

/// Top-level response-shaping configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ResponseShapingConfig {
    /// Enable proxy response shaping. Defaults to the conservative gentle mode.
    pub enabled: bool,
    /// `off`, `gentle`, or `aggressive`.
    pub mode: String,
    pub preamble: PreambleConfig,
    pub code_repetition: CodeRepetitionConfig,
    pub narration: NarrationConfig,
    pub confirmation: ConfirmationConfig,
}

impl Default for ResponseShapingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "gentle".into(),
            preamble: PreambleConfig::default(),
            code_repetition: CodeRepetitionConfig::default(),
            narration: NarrationConfig::default(),
            confirmation: ConfirmationConfig::default(),
        }
    }
}

/// Preamble stripping controls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PreambleConfig {
    pub strip: bool,
    /// Maximum preamble length in approximate tokens.
    pub max_pattern_length: usize,
}

impl Default for PreambleConfig {
    fn default() -> Self {
        Self {
            strip: true,
            max_pattern_length: 50,
        }
    }
}

/// Reserved for explicit aggressive code-repetition shaping. It remains off by
/// default because a read-cache baseline is required to prove repetition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CodeRepetitionConfig {
    pub enabled: bool,
    pub similarity_threshold: f64,
    pub min_block_lines: usize,
}

impl Default for CodeRepetitionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            similarity_threshold: 0.8,
            min_block_lines: 10,
        }
    }
}

/// Post-action narration controls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NarrationConfig {
    pub compress_post_action: bool,
    pub max_summary_tokens: usize,
}

impl Default for NarrationConfig {
    fn default() -> Self {
        Self {
            compress_post_action: true,
            max_summary_tokens: 30,
        }
    }
}

/// Trailing confirmation controls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ConfirmationConfig {
    pub strip_trailing: bool,
}

impl Default for ConfirmationConfig {
    fn default() -> Self {
        Self {
            strip_trailing: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_enabled_and_gentle() {
        let cfg = ResponseShapingConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.mode, "gentle");
        assert!(cfg.preamble.strip);
        assert!(cfg.confirmation.strip_trailing);
    }

    #[test]
    fn proposed_toml_shape_round_trips() {
        let cfg: ResponseShapingConfig = toml::from_str(
            r#"
            enabled = true
            mode = "aggressive"
            [preamble]
            max_pattern_length = 42
            [code_repetition]
            enabled = true
            similarity_threshold = 0.9
            min_block_lines = 12
            [narration]
            max_summary_tokens = 24
            [confirmation]
            strip_trailing = false
            "#,
        )
        .expect("response-shaping TOML");
        assert_eq!(cfg.mode, "aggressive");
        assert_eq!(cfg.preamble.max_pattern_length, 42);
        assert!(cfg.code_repetition.enabled);
        assert_eq!(cfg.narration.max_summary_tokens, 24);
        assert!(!cfg.confirmation.strip_trailing);
    }

    #[test]
    fn omitted_nested_values_use_safe_defaults() {
        let cfg: ResponseShapingConfig = toml::from_str("mode = \"off\"").unwrap();
        assert!(!cfg.enabled || cfg.mode == "off");
        assert_eq!(cfg.code_repetition, CodeRepetitionConfig::default());
        assert_eq!(cfg.narration, NarrationConfig::default());
    }
}
