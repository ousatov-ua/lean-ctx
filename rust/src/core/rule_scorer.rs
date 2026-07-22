//! Rule-relevance scoring (#1141): scores agent config rules against the
//! current working context to determine which rules to inject.
//!
//! Uses BM25 keyword matching between rule content and active context signals
//! (open files, recent tool calls, working directory). Rules below the relevance
//! threshold are moved to a dormant pool and only activated on context change.
//!
//! Determinism (#498): same rules + same context → same scores + same selection.
#![allow(dead_code)]

use std::collections::BTreeMap;

const CHARS_PER_TOKEN: usize = 4;
const DEFAULT_THRESHOLD: f64 = 0.15;

/// A parsed agent rule with metadata.
#[derive(Debug, Clone)]
pub struct AgentRule {
    pub id: String,
    pub source_path: String,
    pub content: String,
    pub path_globs: Vec<String>,
    pub tokens: usize,
    pub keywords: Vec<String>,
}

/// Scoring result for a rule.
#[derive(Debug, Clone)]
pub struct ScoredRule {
    pub rule: AgentRule,
    pub score: f64,
    pub injected: bool,
}

/// Context signals from the current agent session.
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    pub open_files: Vec<String>,
    pub recent_tool_calls: Vec<String>,
    pub working_directory: String,
    pub recent_content_keywords: Vec<String>,
}

/// Result of rule scoring and budget allocation.
#[derive(Debug)]
pub struct BudgetAllocation {
    pub injected: Vec<ScoredRule>,
    pub dormant: Vec<ScoredRule>,
    pub total_tokens_injected: usize,
    pub total_tokens_dormant: usize,
    pub budget_tokens: usize,
}

/// Score and allocate rules against a token budget.
pub fn allocate_rules(
    rules: &[AgentRule],
    context: &SessionContext,
    budget_tokens: usize,
) -> BudgetAllocation {
    let mut scored: Vec<ScoredRule> = rules
        .iter()
        .map(|rule| {
            let score = score_rule(rule, context);
            ScoredRule {
                rule: rule.clone(),
                score,
                injected: false,
            }
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut remaining_budget = budget_tokens;
    let mut injected = Vec::new();
    let mut dormant = Vec::new();

    for mut sr in scored {
        if sr.score >= DEFAULT_THRESHOLD && sr.rule.tokens <= remaining_budget {
            remaining_budget -= sr.rule.tokens;
            sr.injected = true;
            injected.push(sr);
        } else {
            sr.injected = false;
            dormant.push(sr);
        }
    }

    let total_injected: usize = injected.iter().map(|r| r.rule.tokens).sum();
    let total_dormant: usize = dormant.iter().map(|r| r.rule.tokens).sum();

    BudgetAllocation {
        injected,
        dormant,
        total_tokens_injected: total_injected,
        total_tokens_dormant: total_dormant,
        budget_tokens,
    }
}

/// Score a single rule against the current session context.
fn score_rule(rule: &AgentRule, context: &SessionContext) -> f64 {
    let path_score = score_path_match(rule, context);
    let keyword_score = score_keyword_match(rule, context);
    let glob_score = score_glob_match(rule, context);

    // Weighted combination — path match is strongest signal
    path_score * 0.45 + keyword_score * 0.35 + glob_score * 0.20
}

/// Score based on whether rule's path globs match currently open files.
fn score_path_match(rule: &AgentRule, context: &SessionContext) -> f64 {
    if rule.path_globs.is_empty() {
        return 0.3; // no path restriction → mildly relevant everywhere
    }
    if context.open_files.is_empty() {
        return 0.1;
    }

    let matches = context
        .open_files
        .iter()
        .filter(|file| {
            rule.path_globs
                .iter()
                .any(|glob| path_matches_glob(file, glob))
        })
        .count();

    if matches > 0 {
        (matches as f64 / context.open_files.len().max(1) as f64).min(1.0)
    } else {
        0.0
    }
}

/// Score based on keyword overlap between rule and current context.
fn score_keyword_match(rule: &AgentRule, context: &SessionContext) -> f64 {
    if rule.keywords.is_empty() || context.recent_content_keywords.is_empty() {
        return 0.0;
    }

    let rule_terms: BTreeMap<&str, usize> = {
        let mut tf = BTreeMap::new();
        for kw in &rule.keywords {
            *tf.entry(kw.as_str()).or_insert(0) += 1;
        }
        tf
    };

    let matching = context
        .recent_content_keywords
        .iter()
        .filter(|kw| rule_terms.contains_key(kw.as_str()))
        .count();

    let denominator = context
        .recent_content_keywords
        .len()
        .min(rule.keywords.len())
        .max(1);
    (matching as f64 / denominator as f64).min(1.0)
}

/// Score based on glob patterns matching working directory.
fn score_glob_match(rule: &AgentRule, context: &SessionContext) -> f64 {
    if rule.path_globs.is_empty() || context.working_directory.is_empty() {
        return 0.2; // neutral
    }
    if rule
        .path_globs
        .iter()
        .any(|g| context.working_directory.contains(g.trim_matches('*')))
    {
        return 1.0;
    }
    0.0
}

/// Simple glob matching: supports `*` and `**` patterns.
fn path_matches_glob(path: &str, glob: &str) -> bool {
    if glob == "**" || glob == "*" {
        return true;
    }
    // Handle **/*.ext patterns
    if let Some(ext) = glob.strip_prefix("**/") {
        if ext.starts_with("*.") {
            let suffix = &ext[1..]; // .ext
            return path.ends_with(suffix);
        }
        return path.contains(ext);
    }
    // Handle *.ext patterns
    if let Some(ext) = glob.strip_prefix("*.") {
        return path.ends_with(&format!(".{ext}"));
    }
    // Handle dir/* patterns
    if let Some(dir) = glob.strip_suffix("/*") {
        return path.starts_with(dir) || path.contains(&format!("/{dir}/"));
    }
    // Direct substring match
    path.contains(glob)
}

/// Parse an agent rule from raw content and source path.
pub fn parse_rule(id: &str, source_path: &str, content: &str) -> AgentRule {
    let path_globs = extract_path_globs(content);
    let keywords = extract_rule_keywords(content);
    let tokens = content.len() / CHARS_PER_TOKEN;

    AgentRule {
        id: id.to_string(),
        source_path: source_path.to_string(),
        content: content.to_string(),
        path_globs,
        tokens,
        keywords,
    }
}

/// Extract path globs from frontmatter or content patterns.
fn extract_path_globs(content: &str) -> Vec<String> {
    let mut globs = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        // YAML frontmatter: paths: ["*.rs", "src/**"]
        if trimmed.starts_with("paths:") || trimmed.starts_with("globs:") {
            let after_colon = trimmed.split_once(':').map_or("", |(_, v)| v.trim());
            for part in after_colon
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
            {
                let cleaned = part.trim().trim_matches('"').trim_matches('\'');
                if !cleaned.is_empty() {
                    globs.push(cleaned.to_string());
                }
            }
        }
        // Cursor rule frontmatter: path pattern after `---`
        if trimmed.starts_with("- ") && (trimmed.contains("*.") || trimmed.contains("**/")) {
            let glob = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim();
            globs.push(glob.to_string());
        }
    }

    globs
}

/// Extract meaningful keywords from rule content.
pub fn extract_rule_keywords(content: &str) -> Vec<String> {
    let mut tf: BTreeMap<String, usize> = BTreeMap::new();

    for word in content.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        let lower = word.to_lowercase();
        if lower.len() >= 3 && lower.len() <= 40 && !is_common_word(&lower) {
            *tf.entry(lower).or_insert(0) += 1;
        }
    }

    let mut terms: Vec<(String, usize)> = tf.into_iter().collect();
    terms.sort_by_key(|t| std::cmp::Reverse(t.1));
    terms.truncate(30);
    terms.into_iter().map(|(k, _)| k).collect()
}

fn is_common_word(word: &str) -> bool {
    const COMMON: &[&str] = &[
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "had", "her", "was", "one",
        "our", "out", "has", "his", "how", "its", "let", "may", "new", "now", "old", "see", "way",
        "who", "this", "that", "with", "will", "been", "each", "make", "like", "from", "have",
        "must", "should", "would", "could", "when", "then", "than", "also", "into", "only", "your",
        "what", "more", "use", "using", "used", "file", "files", "code", "rule", "rules",
    ];
    COMMON.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(id: &str, content: &str, globs: &[&str]) -> AgentRule {
        AgentRule {
            id: id.to_string(),
            source_path: format!(".cursor/rules/{id}.mdc"),
            content: content.to_string(),
            path_globs: globs.iter().map(ToString::to_string).collect(),
            tokens: content.len() / 4,
            keywords: extract_rule_keywords(content),
        }
    }

    fn make_context(files: &[&str], keywords: &[&str]) -> SessionContext {
        SessionContext {
            open_files: files.iter().map(ToString::to_string).collect(),
            recent_tool_calls: vec![],
            working_directory: "src/proxy".to_string(),
            recent_content_keywords: keywords.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn path_glob_matching_works() {
        assert!(path_matches_glob("src/proxy/forward.rs", "**/*.rs"));
        assert!(path_matches_glob("src/proxy/forward.rs", "src/proxy/*"));
        assert!(!path_matches_glob("src/proxy/forward.rs", "**/*.ts"));
        assert!(path_matches_glob("tests/unit.rs", "*.rs"));
    }

    #[test]
    fn relevant_rules_score_higher() {
        let rust_rule = make_rule(
            "rust-conv",
            "Use snake_case for Rust functions. Prefer Result over panic.",
            &["**/*.rs"],
        );
        let ts_rule = make_rule(
            "ts-conv",
            "Use camelCase for TypeScript. Prefer interfaces over types.",
            &["**/*.ts"],
        );

        let ctx = make_context(
            &["src/proxy/forward.rs", "src/core/mod.rs"],
            &["rust", "proxy", "forward"],
        );

        let rust_score = score_rule(&rust_rule, &ctx);
        let ts_score = score_rule(&ts_rule, &ctx);

        assert!(
            rust_score > ts_score,
            "Rust rule ({rust_score:.3}) should score higher than TS rule ({ts_score:.3})"
        );
    }

    #[test]
    fn budget_allocation_respects_limit() {
        let rules: Vec<AgentRule> = (0..20)
            .map(|i| {
                make_rule(
                    &format!("rule-{i}"),
                    &format!("Rule content for module {i} with proxy and forward patterns"),
                    &["**/*.rs"],
                )
            })
            .collect();

        let ctx = make_context(&["src/proxy/forward.rs"], &["proxy", "forward"]);
        let result = allocate_rules(&rules, &ctx, 100);

        let injected_tokens: usize = result.injected.iter().map(|r| r.rule.tokens).sum();
        assert!(
            injected_tokens <= 100,
            "Injected tokens ({injected_tokens}) should be within budget (100)"
        );
    }

    #[test]
    fn unscoped_rules_get_mild_score() {
        let unscoped = make_rule(
            "general",
            "Always respond in German. Keep responses concise.",
            &[],
        );
        let ctx = make_context(&["src/proxy/forward.rs"], &["proxy"]);

        let score = score_rule(&unscoped, &ctx);
        assert!(
            score > 0.0,
            "Unscoped rules should get a mild baseline score"
        );
        assert!(score < 0.5, "But not a high score");
    }

    #[test]
    fn scoring_is_deterministic() {
        let rule = make_rule("test", "Proxy forward authentication tokens", &["**/*.rs"]);
        let ctx = make_context(&["src/proxy/forward.rs"], &["proxy", "authentication"]);

        let s1 = score_rule(&rule, &ctx);
        let s2 = score_rule(&rule, &ctx);
        assert_eq!(s1, s2);
    }

    #[test]
    fn allocation_is_deterministic() {
        let rules = vec![
            make_rule("a", "Proxy rules for forward module", &["**/*.rs"]),
            make_rule("b", "TypeScript component patterns", &["**/*.ts"]),
            make_rule("c", "Database migration conventions", &["migrations/*"]),
        ];
        let ctx = make_context(&["src/proxy/forward.rs"], &["proxy", "forward"]);

        let r1 = allocate_rules(&rules, &ctx, 500);
        let r2 = allocate_rules(&rules, &ctx, 500);

        assert_eq!(r1.injected.len(), r2.injected.len());
        assert_eq!(r1.total_tokens_injected, r2.total_tokens_injected);
    }

    #[test]
    fn parse_rule_extracts_globs_from_frontmatter() {
        let content = "---\npaths: [\"**/*.rs\", \"src/proxy/*\"]\n---\nAlways use Result types.";
        let rule = parse_rule("test", ".cursor/rules/test.mdc", content);
        assert_eq!(rule.path_globs, vec!["**/*.rs", "src/proxy/*"]);
    }

    #[test]
    fn empty_context_still_works() {
        let rule = make_rule("test", "Some rule content here", &["**/*.rs"]);
        let ctx = SessionContext::default();
        let score = score_rule(&rule, &ctx);
        assert!(score >= 0.0);
    }
}
