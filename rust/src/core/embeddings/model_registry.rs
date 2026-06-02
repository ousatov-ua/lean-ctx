//! Embedding model registry — model configs, selection, and metadata.
//!
//! Supports multiple ONNX embedding models with different dimensions,
//! tokenizers, and download sources. Models are selected via the
//! `LEAN_CTX_EMBEDDING_MODEL` env var or the `[embedding].model` key in `config.toml`
//! (env var wins) — see [`resolve_model`].

use std::fmt;

/// Supported embedding models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmbeddingModel {
    /// all-MiniLM-L6-v2 — generic sentence embeddings (384d, ~91MB).
    /// Default model for backward compatibility.
    AllMiniLmL6V2,
    /// jina-embeddings-v2-base-code — code-optimized, 30 languages (768d, ~642MB).
    /// Best for mixed code + natural language search.
    JinaCodeV2,
    /// nomic-embed-text-v1.5 — top MTEB general-purpose (768d, ~547MB).
    /// Matryoshka representation learning, supports dimension truncation.
    NomicEmbedV1_5,
}

impl EmbeddingModel {
    pub const DEFAULT: Self = Self::AllMiniLmL6V2;

    pub fn config(self) -> ModelConfig {
        match self {
            Self::AllMiniLmL6V2 => ModelConfig {
                model: self,
                name: "all-MiniLM-L6-v2",
                hf_repo: "sentence-transformers/all-MiniLM-L6-v2",
                onnx_path: "onnx/model.onnx",
                vocab_file: VocabSource::VocabTxt("vocab.txt"),
                dimensions: 384,
                max_seq_len: 256,
                model_min_bytes: 1_000_000,
                vocab_min_bytes: 100_000,
                query_prefix: None,
                document_prefix: None,
                needs_token_type_ids: true,
            },
            Self::JinaCodeV2 => ModelConfig {
                model: self,
                name: "jina-embeddings-v2-base-code",
                hf_repo: "jinaai/jina-embeddings-v2-base-code",
                onnx_path: "onnx/model.onnx",
                vocab_file: VocabSource::VocabTxt("vocab.txt"),
                dimensions: 768,
                max_seq_len: 512,
                model_min_bytes: 100_000_000,
                vocab_min_bytes: 100_000,
                query_prefix: None,
                document_prefix: None,
                needs_token_type_ids: true,
            },
            Self::NomicEmbedV1_5 => ModelConfig {
                model: self,
                name: "nomic-embed-text-v1.5",
                hf_repo: "nomic-ai/nomic-embed-text-v1.5",
                onnx_path: "onnx/model.onnx",
                vocab_file: VocabSource::VocabTxt("vocab.txt"),
                dimensions: 768,
                max_seq_len: 512,
                model_min_bytes: 100_000_000,
                vocab_min_bytes: 100_000,
                query_prefix: Some("search_query: "),
                document_prefix: Some("search_document: "),
                needs_token_type_ids: false,
            },
        }
    }

    /// Parse model name from string (env var / config file).
    pub fn from_str_name(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "all-minilm-l6-v2" | "minilm" | "default" => Some(Self::AllMiniLmL6V2),
            "jina-code-v2" | "jina-embeddings-v2-base-code" | "jina-code" | "jina" => {
                Some(Self::JinaCodeV2)
            }
            "nomic-embed-v1.5" | "nomic-embed-text-v1.5" | "nomic" | "nomic-embed" => {
                Some(Self::NomicEmbedV1_5)
            }
            _ => None,
        }
    }

    /// All available model variants.
    pub const ALL: &'static [Self] = &[Self::AllMiniLmL6V2, Self::JinaCodeV2, Self::NomicEmbedV1_5];

    /// Unique subdirectory name for model storage isolation.
    pub fn storage_dir_name(self) -> &'static str {
        match self {
            Self::AllMiniLmL6V2 => "all-minilm-l6-v2",
            Self::JinaCodeV2 => "jina-code-v2",
            Self::NomicEmbedV1_5 => "nomic-embed-v1.5",
        }
    }
}

impl fmt::Display for EmbeddingModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.config().name)
    }
}

/// Vocabulary/tokenizer source for a model.
#[derive(Debug, Clone, Copy)]
pub enum VocabSource {
    /// Standard BERT vocab.txt (one token per line, WordPiece).
    VocabTxt(&'static str),
    /// HuggingFace tokenizer.json (BPE/Unigram via JSON config).
    TokenizerJson(&'static str),
}

impl VocabSource {
    pub fn filename(&self) -> &'static str {
        match self {
            Self::VocabTxt(f) | Self::TokenizerJson(f) => f,
        }
    }

    pub fn is_wordpiece(&self) -> bool {
        matches!(self, Self::VocabTxt(_))
    }
}

/// Complete configuration for a single embedding model.
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub model: EmbeddingModel,
    pub name: &'static str,
    pub hf_repo: &'static str,
    pub onnx_path: &'static str,
    pub vocab_file: VocabSource,
    pub dimensions: usize,
    pub max_seq_len: usize,
    pub model_min_bytes: u64,
    pub vocab_min_bytes: u64,
    /// Optional prefix prepended to queries before embedding.
    pub query_prefix: Option<&'static str>,
    /// Optional prefix prepended to documents/code before embedding.
    pub document_prefix: Option<&'static str>,
    /// Whether the model expects token_type_ids input (BERT-style).
    /// Some models (e.g. nomic-embed) only use input_ids + attention_mask.
    pub needs_token_type_ids: bool,
}

impl ModelConfig {
    /// Full HuggingFace download URL for the ONNX model file.
    pub fn model_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.hf_repo, self.onnx_path
        )
    }

    /// Full HuggingFace download URL for the vocabulary/tokenizer file.
    pub fn vocab_url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.hf_repo,
            self.vocab_file.filename()
        )
    }
}

/// Resolve which embedding model to use.
///
/// Priority: `LEAN_CTX_EMBEDDING_MODEL` env var > `[embedding].model` in `config.toml` >
/// the default model. An unrecognized name is skipped (with a warning) so a typo in one
/// source never silently swaps the model — which would otherwise force a full re-index.
pub fn resolve_model() -> EmbeddingModel {
    let env_val = std::env::var("LEAN_CTX_EMBEDDING_MODEL").ok();
    let config_val = crate::core::config::Config::load().embedding.model;
    resolve_model_from(env_val.as_deref(), config_val.as_deref())
}

/// Pure model resolution used by [`resolve_model`]; kept separate so the env-var/config
/// precedence is unit-testable without touching the process environment or the on-disk
/// `config.toml`.
fn resolve_model_from(env_val: Option<&str>, config_val: Option<&str>) -> EmbeddingModel {
    for (source, raw) in [
        ("LEAN_CTX_EMBEDDING_MODEL", env_val),
        ("[embedding].model", config_val),
    ] {
        let Some(name) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        if let Some(model) = EmbeddingModel::from_str_name(name) {
            return model;
        }
        tracing::warn!(
            "Unknown embedding model {name:?} from {source}; using {} instead",
            EmbeddingModel::DEFAULT
        );
    }
    EmbeddingModel::DEFAULT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_minilm() {
        assert_eq!(EmbeddingModel::DEFAULT, EmbeddingModel::AllMiniLmL6V2);
    }

    #[test]
    fn from_str_name_variants() {
        assert_eq!(
            EmbeddingModel::from_str_name("minilm"),
            Some(EmbeddingModel::AllMiniLmL6V2)
        );
        assert_eq!(
            EmbeddingModel::from_str_name("jina-code-v2"),
            Some(EmbeddingModel::JinaCodeV2)
        );
        assert_eq!(
            EmbeddingModel::from_str_name("jina-code"),
            Some(EmbeddingModel::JinaCodeV2)
        );
        assert_eq!(
            EmbeddingModel::from_str_name("jina"),
            Some(EmbeddingModel::JinaCodeV2)
        );
        assert_eq!(
            EmbeddingModel::from_str_name("nomic-embed-v1.5"),
            Some(EmbeddingModel::NomicEmbedV1_5)
        );
        assert_eq!(
            EmbeddingModel::from_str_name("nomic"),
            Some(EmbeddingModel::NomicEmbedV1_5)
        );
        assert_eq!(
            EmbeddingModel::from_str_name("default"),
            Some(EmbeddingModel::AllMiniLmL6V2)
        );
        assert_eq!(EmbeddingModel::from_str_name("unknown"), None);
    }

    #[test]
    fn all_models_have_valid_configs() {
        for model in EmbeddingModel::ALL {
            let cfg = model.config();
            assert!(!cfg.name.is_empty());
            assert!(!cfg.hf_repo.is_empty());
            assert!(cfg.dimensions > 0);
            assert!(cfg.max_seq_len > 0);
            assert!(cfg.model_min_bytes > 0);
            assert!(cfg.vocab_min_bytes > 0);
        }
    }

    #[test]
    fn model_urls_are_valid() {
        for model in EmbeddingModel::ALL {
            let cfg = model.config();
            let model_url = cfg.model_url();
            let vocab_url = cfg.vocab_url();
            assert!(model_url.starts_with("https://huggingface.co/"));
            assert!(vocab_url.starts_with("https://huggingface.co/"));
            assert!(model_url.contains("resolve/main"));
        }
    }

    #[test]
    fn storage_dir_names_are_unique() {
        let names: Vec<_> = EmbeddingModel::ALL
            .iter()
            .map(|m| m.storage_dir_name())
            .collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len());
    }

    #[test]
    fn display_uses_model_name() {
        assert_eq!(
            format!("{}", EmbeddingModel::AllMiniLmL6V2),
            "all-MiniLM-L6-v2"
        );
        assert_eq!(
            format!("{}", EmbeddingModel::JinaCodeV2),
            "jina-embeddings-v2-base-code"
        );
    }

    #[test]
    fn resolve_defaults_when_nothing_set() {
        assert_eq!(resolve_model_from(None, None), EmbeddingModel::DEFAULT);
        assert_eq!(
            resolve_model_from(Some(""), Some("   ")),
            EmbeddingModel::DEFAULT
        );
    }

    #[test]
    fn config_selects_model_when_env_unset() {
        assert_eq!(
            resolve_model_from(None, Some("jina-code-v2")),
            EmbeddingModel::JinaCodeV2
        );
        assert_eq!(
            resolve_model_from(None, Some("nomic")),
            EmbeddingModel::NomicEmbedV1_5
        );
    }

    #[test]
    fn env_var_overrides_config() {
        assert_eq!(
            resolve_model_from(Some("minilm"), Some("nomic")),
            EmbeddingModel::AllMiniLmL6V2
        );
    }

    #[test]
    fn unknown_name_falls_through_then_defaults() {
        // Bad env value → valid config value wins.
        assert_eq!(
            resolve_model_from(Some("bogus"), Some("nomic")),
            EmbeddingModel::NomicEmbedV1_5
        );
        // Bad everywhere → default (never silently breaks the index).
        assert_eq!(
            resolve_model_from(Some("bogus"), Some("nope")),
            EmbeddingModel::DEFAULT
        );
        // Empty/whitespace in the higher-priority source is skipped, not treated as a match.
        assert_eq!(
            resolve_model_from(Some("   "), Some("jina")),
            EmbeddingModel::JinaCodeV2
        );
    }

    #[test]
    fn jina_code_v2_config_details() {
        let cfg = EmbeddingModel::JinaCodeV2.config();
        assert_eq!(cfg.dimensions, 768);
        assert!(cfg.needs_token_type_ids);
        assert!(cfg.query_prefix.is_none());
    }

    #[test]
    fn nomic_has_prefixes() {
        let cfg = EmbeddingModel::NomicEmbedV1_5.config();
        assert!(cfg.query_prefix.is_some());
        assert!(cfg.document_prefix.is_some());
        assert!(!cfg.needs_token_type_ids);
    }

    #[test]
    fn minilm_is_wordpiece() {
        let cfg = EmbeddingModel::AllMiniLmL6V2.config();
        assert!(cfg.vocab_file.is_wordpiece());
    }

    #[test]
    fn all_current_models_use_wordpiece() {
        for model in EmbeddingModel::ALL {
            assert!(
                model.config().vocab_file.is_wordpiece(),
                "{model} should use WordPiece vocab.txt"
            );
        }
    }
}
