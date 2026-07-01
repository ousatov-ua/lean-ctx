//! Deterministic Markdown/documentation compaction for LLM-agent reads.
//!
//! Keeps heading topology intact and selects high-signal body lines with a small
//! IDF-style scorer. This is intentionally std-only and byte-stable (#498).

use std::collections::{HashMap, HashSet};

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "into", "your", "you", "are", "can",
    "will", "not", "all", "use", "using", "used", "lean", "ctx", "context", "agent", "agents",
];

/// Compact Markdown while preserving all headings and high-signal details.
pub fn compact_markdown(content: &str) -> Option<String> {
    if content.trim().is_empty() || !looks_like_markdown(content) {
        return None;
    }

    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 24 {
        return None;
    }

    let docs = token_sets(&lines);
    let df = document_frequency(&docs);
    let mut scored: Vec<(usize, f64)> = lines
        .iter()
        .enumerate()
        .map(|(idx, line)| (idx, score_line(idx, line, &docs[idx], lines.len(), &df)))
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    // ~45% line budget from the measured prototype, with all headings forced in.
    let target = ((lines.len() as f64) * 0.45).ceil() as usize;
    let mut keep: HashSet<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| is_heading(line).then_some(idx))
        .collect();
    for (idx, _) in scored.into_iter().take(target) {
        keep.insert(idx);
    }

    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        if keep.contains(&idx) {
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }

    let kept_lines = out.lines().count();
    if kept_lines >= lines.len() || out.len() >= content.len() {
        None
    } else {
        Some(out)
    }
}

fn looks_like_markdown(content: &str) -> bool {
    content.lines().any(is_heading)
        || content.lines().any(|l| l.trim_start().starts_with("- "))
        || content.lines().any(|l| l.trim_start().starts_with("* "))
        || content.lines().any(|l| l.trim_start().starts_with("| "))
}

fn is_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('#') && trimmed.chars().take_while(|c| *c == '#').count() <= 6
}

fn token_sets(lines: &[&str]) -> Vec<HashSet<String>> {
    lines
        .iter()
        .map(|line| tokens(line).into_iter().collect())
        .collect()
}

fn document_frequency(docs: &[HashSet<String>]) -> HashMap<String, usize> {
    let mut df = HashMap::new();
    for doc in docs {
        for token in doc {
            *df.entry(token.clone()).or_insert(0) += 1;
        }
    }
    df
}

fn score_line(
    idx: usize,
    line: &str,
    tokens: &HashSet<String>,
    line_count: usize,
    df: &HashMap<String, usize>,
) -> f64 {
    let mut score = 0.0;
    for token in tokens {
        let freq = *df.get(token).unwrap_or(&1) as f64;
        score += ((line_count as f64 + 1.0) / (freq + 1.0)).ln();
    }

    let trimmed = line.trim_start();
    if is_heading(line) {
        score += 20.0;
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("| ") {
        score += 2.0;
    }
    if line.contains('`')
        || line.contains("MUST")
        || line.contains("SHOULD")
        || line.contains("BLOCKING")
        || line.contains("WARNING")
        || line.contains("ctx_")
        || line.contains("lean-ctx")
    {
        score += 10.0;
    }
    score + (1.0 / (idx + 1) as f64)
}

fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in line.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.' | ':') {
            cur.push(ch);
        } else if !cur.is_empty() {
            push_token(&mut out, &cur);
            cur.clear();
        }
    }
    if !cur.is_empty() {
        push_token(&mut out, &cur);
    }
    out
}

fn push_token(out: &mut Vec<String>, token: &str) {
    let t = token
        .trim_matches(|c: char| matches!(c, '.' | ',' | ':' | ';' | '(' | ')' | '[' | ']'))
        .to_ascii_lowercase();
    if t.len() < 3 || STOP_WORDS.contains(&t.as_str()) {
        return;
    }
    if t.len() >= 8 || t.chars().any(|c| matches!(c, '_' | '-' | '/' | '.' | ':')) {
        out.push(t);
    }
}

#[cfg(test)]
mod tests {
    use super::compact_markdown;

    #[test]
    fn keeps_all_headings_and_shrinks() {
        let input = "# Title\n\nIntro paragraph with ordinary words.\n\n## Setup\n\n";
        let body = "- `ctx_read` keeps important details for agents.\n";
        let repeated = "This sentence is useful once but repeated many times for filler.\n";
        let mut doc = input.to_string();
        for _ in 0..20 {
            doc.push_str(repeated);
        }
        doc.push_str(body);
        doc.push_str("## Safety\n\nMUST preserve warnings and exact commands.\n");
        for _ in 0..20 {
            doc.push_str(repeated);
        }

        let compacted = compact_markdown(&doc).expect("markdown should compact");
        assert!(compacted.len() < doc.len());
        assert!(compacted.contains("# Title"));
        assert!(compacted.contains("## Setup"));
        assert!(compacted.contains("## Safety"));
        assert!(compacted.contains("ctx_read"));
        assert!(compacted.contains("MUST preserve"));
    }

    #[test]
    fn short_docs_pass_through() {
        assert!(compact_markdown("# Title\n\nSmall.\n").is_none());
    }
}
