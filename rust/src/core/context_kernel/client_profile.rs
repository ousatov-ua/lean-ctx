//! Client capability and efficiency profiles.

use super::coverage_class::CoverageClass;

const DEFAULT_CLIENT_ID: &str = "unknown";
const DEFAULT_CONTEXT_WINDOW: usize = 128_000;
const DEFAULT_MAX_TOOLS: usize = 64;
const DEFAULT_MAX_SCHEMA_TOKENS: usize = 16_384;
const DEFAULT_LATENCY_BUDGET_MS: u64 = 30_000;

/// MCP capabilities advertised by a client.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct McpFeatures {
    /// Maximum number of tools the client can expose; zero means unspecified.
    pub tool_limit: usize,
    /// Whether the client supports MCP elicitation.
    pub supports_elicitation: bool,
    /// Whether the client supports MCP sampling.
    pub supports_sampling: bool,
}

/// Limits applied to the tool catalog sent to a client.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ToolBudget {
    /// Maximum number of tools to expose.
    pub max_tools: usize,
    /// Maximum combined size of tool schemas, in tokens.
    pub max_schema_tokens: usize,
}

impl ToolBudget {
    /// Returns a practical tool budget for clients with no explicit limits.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_tools: DEFAULT_MAX_TOOLS,
            max_schema_tokens: DEFAULT_MAX_SCHEMA_TOKENS,
        }
    }
}

/// Client properties used to adapt context and tool delivery.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClientEfficiencyProfile {
    /// Stable client identifier.
    pub client_id: String,
    /// Degree of control available over the client's context.
    pub coverage: CoverageClass,
    /// MCP capabilities supported by the client.
    pub mcp_features: McpFeatures,
    /// Limits for tool metadata delivered to the client.
    pub tool_budget: ToolBudget,
    /// Model family reported by the client, when known.
    pub model_family: Option<String>,
    /// Maximum model context window, in tokens.
    pub context_window: usize,
    /// Whether the client supports streamed responses.
    pub supports_streaming: bool,
    /// Whether the client supports reusable cached context.
    pub supports_caching: bool,
    /// Target end-to-end latency budget, in milliseconds.
    pub latency_budget_ms: u64,
}

/// Fluent builder for [`ClientEfficiencyProfile`].
#[derive(Debug, Clone)]
pub struct ProfileBuilder {
    profile: ClientEfficiencyProfile,
}

impl ProfileBuilder {
    /// Creates a builder with conservative, production-ready defaults.
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            profile: ClientEfficiencyProfile {
                client_id: client_id.into(),
                coverage: CoverageClass::default(),
                mcp_features: McpFeatures::default(),
                tool_budget: ToolBudget::new(),
                model_family: None,
                context_window: DEFAULT_CONTEXT_WINDOW,
                supports_streaming: false,
                supports_caching: false,
                latency_budget_ms: DEFAULT_LATENCY_BUDGET_MS,
            },
        }
    }

    /// Sets the client's coverage class.
    #[must_use]
    pub fn coverage(mut self, coverage: CoverageClass) -> Self {
        self.profile.coverage = coverage;
        self
    }

    /// Sets the model family reported by the client.
    #[must_use]
    pub fn model_family(mut self, model_family: impl Into<String>) -> Self {
        self.profile.model_family = Some(model_family.into());
        self
    }

    /// Sets the model context window in tokens.
    #[must_use]
    pub fn context_window(mut self, context_window: usize) -> Self {
        self.profile.context_window = context_window;
        self
    }

    /// Sets the tool catalog budget.
    #[must_use]
    pub fn tool_budget(mut self, tool_budget: ToolBudget) -> Self {
        self.profile.tool_budget = tool_budget;
        self
    }

    /// Sets supported MCP features.
    #[must_use]
    pub fn mcp_features(mut self, mcp_features: McpFeatures) -> Self {
        self.profile.mcp_features = mcp_features;
        self
    }

    /// Sets whether response streaming is supported.
    #[must_use]
    pub fn streaming(mut self, supports_streaming: bool) -> Self {
        self.profile.supports_streaming = supports_streaming;
        self
    }

    /// Sets whether reusable context caching is supported.
    #[must_use]
    pub fn caching(mut self, supports_caching: bool) -> Self {
        self.profile.supports_caching = supports_caching;
        self
    }

    /// Sets the target end-to-end latency budget in milliseconds.
    #[must_use]
    pub fn latency_ms(mut self, latency_budget_ms: u64) -> Self {
        self.profile.latency_budget_ms = latency_budget_ms;
        self
    }

    /// Builds the client profile.
    #[must_use]
    pub fn build(self) -> ClientEfficiencyProfile {
        self.profile
    }
}

/// Detects a client profile from case-insensitive transport headers.
#[must_use]
pub fn detect_from_headers(headers: &[(String, String)]) -> ClientEfficiencyProfile {
    let mut builder = ProfileBuilder::new(DEFAULT_CLIENT_ID);
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("x-client-id") {
            builder.profile.client_id.clone_from(value);
        } else if name.eq_ignore_ascii_case("x-model-family") {
            builder.profile.model_family = Some(value.clone());
        } else if name.eq_ignore_ascii_case("x-context-window")
            && let Ok(context_window) = value.parse::<usize>()
        {
            builder.profile.context_window = context_window;
        }
    }
    builder.build()
}

/// Merges non-default override fields into a base profile.
#[must_use]
pub fn merge_profiles(
    base: &ClientEfficiencyProfile,
    override_: &ClientEfficiencyProfile,
) -> ClientEfficiencyProfile {
    let mut merged = base.clone();
    if !override_.client_id.is_empty() {
        merged.client_id.clone_from(&override_.client_id);
    }
    if override_.coverage != CoverageClass::default() {
        merged.coverage = override_.coverage;
    }
    if override_.mcp_features.tool_limit != 0 {
        merged.mcp_features.tool_limit = override_.mcp_features.tool_limit;
    }
    if override_.mcp_features.supports_elicitation {
        merged.mcp_features.supports_elicitation = true;
    }
    if override_.mcp_features.supports_sampling {
        merged.mcp_features.supports_sampling = true;
    }
    if override_.tool_budget.max_tools != 0 {
        merged.tool_budget.max_tools = override_.tool_budget.max_tools;
    }
    if override_.tool_budget.max_schema_tokens != 0 {
        merged.tool_budget.max_schema_tokens = override_.tool_budget.max_schema_tokens;
    }
    if let Some(model_family) = &override_.model_family {
        merged.model_family = Some(model_family.clone());
    }
    if override_.context_window != 0 {
        merged.context_window = override_.context_window;
    }
    if override_.supports_streaming {
        merged.supports_streaming = true;
    }
    if override_.supports_caching {
        merged.supports_caching = true;
    }
    if override_.latency_budget_ms != 0 {
        merged.latency_budget_ms = override_.latency_budget_ms;
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builder_defaults() {
        let profile = ProfileBuilder::new("test").build();
        assert_eq!(profile.client_id, "test");
        assert_eq!(profile.coverage, CoverageClass::default());
        assert!(profile.context_window > 0);
        assert!(profile.tool_budget.max_tools > 0);
    }
    #[test]
    fn builder_chain() {
        let features = McpFeatures {
            tool_limit: 12,
            supports_elicitation: true,
            supports_sampling: true,
        };
        let budget = ToolBudget {
            max_tools: 10,
            max_schema_tokens: 2_048,
        };
        let profile = ProfileBuilder::new("client")
            .coverage(CoverageClass::FullInline)
            .model_family("gpt")
            .context_window(32_000)
            .tool_budget(budget.clone())
            .mcp_features(features.clone())
            .streaming(true)
            .caching(true)
            .latency_ms(900)
            .build();
        assert_eq!(profile.coverage, CoverageClass::FullInline);
        assert_eq!(profile.model_family.as_deref(), Some("gpt"));
        assert_eq!(profile.context_window, 32_000);
        assert_eq!(profile.tool_budget, budget);
        assert_eq!(profile.mcp_features, features);
        assert!(profile.supports_streaming);
        assert!(profile.supports_caching);
        assert_eq!(profile.latency_budget_ms, 900);
    }
    #[test]
    fn detect_from_empty_headers() {
        assert_eq!(detect_from_headers(&[]).client_id, DEFAULT_CLIENT_ID);
    }
    #[test]
    fn detect_from_headers_with_client_id() {
        let headers = vec![("X-Client-Id".to_owned(), "codex".to_owned())];
        assert_eq!(detect_from_headers(&headers).client_id, "codex");
    }
    #[test]
    fn merge_overrides_non_default() {
        let base = ProfileBuilder::new("base")
            .model_family("base-model")
            .context_window(8_000)
            .latency_ms(2_000)
            .build();
        let override_ = ProfileBuilder::new("override")
            .coverage(CoverageClass::FullInline)
            .model_family("new-model")
            .context_window(16_000)
            .latency_ms(500)
            .build();
        let merged = merge_profiles(&base, &override_);
        assert_eq!(merged.client_id, "override");
        assert_eq!(merged.coverage, CoverageClass::FullInline);
        assert_eq!(merged.model_family.as_deref(), Some("new-model"));
        assert_eq!(merged.context_window, 16_000);
        assert_eq!(merged.latency_budget_ms, 500);
    }
    #[test]
    fn serde_roundtrip() {
        let profile = ProfileBuilder::new("serde")
            .coverage(CoverageClass::ObserveOnly)
            .model_family("family")
            .context_window(64_000)
            .streaming(false)
            .latency_ms(700)
            .build();
        let json = serde_json::to_string(&profile).expect("profile must serialize");
        let decoded: ClientEfficiencyProfile =
            serde_json::from_str(&json).expect("profile must deserialize");
        assert_eq!(decoded, profile);
    }
}
