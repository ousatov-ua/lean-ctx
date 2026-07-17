use std::path::Path;

use crate::core::graph_provider::{self, FileInfo, GraphProvider, SymbolInfo};
use crate::core::protocol;
use crate::core::tokens::count_tokens;

pub fn handle(
    name: &str,
    file: Option<&str>,
    kind: Option<&str>,
    project_root: &str,
) -> (String, usize) {
    let Some(open) = graph_provider::open_or_build(project_root) else {
        return (
            format!(
                "Symbol '{name}' not found (no graph available). \
                 Try ctx_search(pattern=\"{name}\") for a broader search.",
            ),
            0,
        );
    };
    let gp = &open.provider;

    let matches = gp.find_symbols(name, file, kind);

    if matches.is_empty() {
        return (
            format!(
                "Symbol '{name}' not found in index ({} symbols indexed). \
                 Try ctx_search(pattern=\"{name}\") for a broader search.",
                gp.symbol_count()
            ),
            0,
        );
    }

    if matches.len() == 1 {
        return render_single(&matches[0], gp, project_root);
    }

    if matches.len() <= 5 {
        return render_multiple(&matches, gp, project_root);
    }

    let mut out = format!(
        "{} matches for '{name}'. Narrow with file= or kind=:\n",
        matches.len()
    );
    for m in matches.iter().take(20) {
        out.push_str(&format!(
            "  {}::{} ({}:L{}-{})\n",
            m.file, m.name, m.kind, m.start_line, m.end_line
        ));
    }
    if matches.len() > 20 {
        out.push_str(&format!("  ... and {} more\n", matches.len() - 20));
    }
    (out, 0)
}

/// Render the body of the single most relevant symbol named `name`, using the
/// surrounding task `keywords` to disambiguate same-named symbols across the
/// repo. Used by `ctx_compose` to inline the top symbol's definition. Returns
/// `(rendered_with_body, emitted_snippet_tokens)` or `None` when no graph/symbol.
/// The token count is the size of what is actually emitted (the snippet), not
/// the whole source file — `ctx_compose` charges it against a coverage budget,
/// and billing a 13-line method the tokens of its 440-line file would drop the
/// symbol from the budget entirely (why the "Top symbols" section vanished once
/// disambiguation started picking methods in large files).
///
/// Without task context a bare `find_symbols(name).next()` returns an arbitrary
/// match — e.g. a trivial `GetMaxCurrent` config accessor over the OCPP charger
/// method the task is actually about (issue #993). We rank candidates by how
/// well their file path reflects the *other* task keywords, preferring exported
/// symbols, and fall back to first-match only when nothing distinguishes them.
pub fn best_symbol_snippet(
    name: &str,
    keywords: &[String],
    project_root: &str,
) -> Option<(String, usize)> {
    let open = graph_provider::open_or_build(project_root)?;
    let gp = &open.provider;
    let candidates = gp.find_symbols(name, None, None);

    // Other task keywords (everything except the one that resolved this symbol),
    // lowercased once for case-insensitive path matching.
    let others: Vec<String> = keywords
        .iter()
        .filter(|k| !k.eq_ignore_ascii_case(name))
        .map(|k| k.to_ascii_lowercase())
        .collect();

    let sym = candidates
        .into_iter()
        .max_by_key(|s| symbol_task_score(s, &others))?;
    // Charge the coverage budget for the emitted snippet, not the whole file
    // (render_single's second value is full-file tokens, meant for ctx_symbol's
    // savings metric — the wrong unit for a per-snippet budget).
    let (rendered, _full_file_tokens) = render_single(&sym, gp, project_root);
    let emitted_tokens = count_tokens(&rendered);
    Some((rendered, emitted_tokens))
}

/// Relevance of a same-named symbol to the task, from its path and visibility.
/// Higher is better; ties keep `max_by_key`'s last-seen (i.e. `find_symbols`
/// order), so a zero-signal task degrades to the previous first-match behaviour.
fn symbol_task_score(sym: &SymbolInfo, other_keywords_lower: &[String]) -> i64 {
    let path_lower = sym.file.to_ascii_lowercase();
    let path_hits = other_keywords_lower
        .iter()
        .filter(|k| path_lower.contains(k.as_str()))
        .count() as i64;
    // Path relevance dominates; exported is a weak tiebreak below it.
    path_hits * 10 + i64::from(sym.is_exported)
}

/// Render one symbol resolved from a stable handle (`path#name@Lline`),
/// bypassing the fuzzy name lookup and the `>5 matches, narrow with file=/kind=`
/// disambiguation entirely. Returns `(rendered, full_file_tokens)`, or a clear,
/// actionable message (tokens = 0) when the handle is malformed, the graph is
/// unavailable, or nothing resolves.
pub fn render_by_handle(handle: &str, project_root: &str) -> (String, usize) {
    let Some(parsed) = crate::core::handle::SymbolHandle::parse(handle) else {
        return (
            format!(
                "Invalid handle '{handle}'. Expected path#name@Lline, \
                 e.g. src/lib.rs#Config::load@L22."
            ),
            0,
        );
    };
    let Some(open) = graph_provider::open_or_build(project_root) else {
        return (
            format!("Handle '{handle}' not resolvable (no graph available)."),
            0,
        );
    };
    let gp = &open.provider;
    match gp.find_symbol_by_handle(&parsed) {
        Some(sym) => render_single(&sym, gp, project_root),
        None => (
            format!(
                "No symbol for handle '{handle}'. \
                 Try ctx_search(action=\"symbol\", name=\"{}\").",
                parsed.name
            ),
            0,
        ),
    }
}

fn render_single(sym: &SymbolInfo, gp: &GraphProvider, project_root: &str) -> (String, usize) {
    let abs_path = resolve_file_path(&sym.file, project_root);

    if let Err(e) = crate::core::pathjail::jail_path(
        std::path::Path::new(&abs_path),
        std::path::Path::new(project_root),
    ) {
        return (
            format!("Symbol '{}': path blocked by jail: {e}", sym.name),
            0,
        );
    }

    let Ok(content) = std::fs::read_to_string(&abs_path) else {
        return (
            format!(
                "Symbol '{}' found at {}:L{}-{} but file unreadable",
                sym.name, sym.file, sym.start_line, sym.end_line
            ),
            0,
        );
    };

    let lines: Vec<&str> = content.lines().collect();
    let start = sym.start_line.saturating_sub(1);
    let end = sym.end_line.min(lines.len());
    let snippet: String = lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>4}|{}", start + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    let full_tokens = count_tokens(&content);
    let snippet_tokens = count_tokens(&snippet);

    let vis = if sym.is_exported { "+" } else { "-" };
    let cc_note = symbol_cc_note(&content, &sym.file, &sym.name, sym.start_line);
    // Lead with the stable handle (`path#name@Lline`) so the agent can re-target
    // this exact symbol next turn via ctx_search(action="symbol", handle=…).
    let handle = crate::core::handle::emit(&sym.file, &sym.name, sym.start_line);
    let header = format!(
        "{handle}  ({vis} {}, L{}-{}){cc_note}",
        sym.kind, sym.start_line, sym.end_line
    );

    let file_info: Option<FileInfo> = gp.get_file_entry(&sym.file);
    let ctx = if let Some(f) = file_info {
        format!(
            "File: {} ({} lines, {} tokens)",
            sym.file, f.line_count, f.token_count
        )
    } else {
        format!("File: {}", sym.file)
    };

    let savings = protocol::format_savings(full_tokens, snippet_tokens);

    (
        format!("{header}\n{ctx}\n\n{snippet}\n{savings}"),
        full_tokens,
    )
}

fn render_multiple(
    symbols: &[SymbolInfo],
    gp: &GraphProvider,
    project_root: &str,
) -> (String, usize) {
    let mut out = String::new();
    let mut total_original = 0usize;

    for (i, sym) in symbols.iter().enumerate() {
        if i > 0 {
            out.push_str("\n---\n\n");
        }
        let (rendered, orig) = render_single(sym, gp, project_root);
        out.push_str(&rendered);
        total_original = total_original.max(orig);
    }

    (out, total_original)
}

/// Optional ` · cc=NN` suffix for a symbol header — the code-health complexity
/// of the function being shown (#1084). Computed fresh from the already-read
/// file content, so it's exact for *any* symbol. Over-threshold functions are
/// flagged `(over)`. Honors the `code_health.annotate_reads` opt-out and is
/// empty for non-functions / when tree-sitter is off.
fn symbol_cc_note(content: &str, file: &str, name: &str, start_line: usize) -> String {
    let cfg = crate::core::config::Config::load();
    if !cfg.code_health.annotate_reads {
        return String::new();
    }
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match crate::core::code_health::annotate::cognitive_for_symbol(content, ext, name, start_line) {
        Some(cc) if cc > cfg.code_health.cognitive_threshold => format!(" · cc={cc} (over)"),
        Some(cc) => format!(" · cc={cc}"),
        None => String::new(),
    }
}

fn resolve_file_path(relative: &str, project_root: &str) -> String {
    let p = Path::new(relative);
    if p.is_absolute() && p.exists() {
        return relative.to_string();
    }
    let joined = Path::new(project_root).join(relative);
    if joined.exists() {
        return joined.to_string_lossy().to_string();
    }
    relative.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::graph_index::{ProjectIndex, SymbolEntry};

    fn test_provider() -> GraphProvider {
        let mut index = ProjectIndex::new("/tmp/test");
        index.symbols.insert(
            "src/main.rs::main".to_string(),
            SymbolEntry {
                file: "src/main.rs".to_string(),
                name: "main".to_string(),
                kind: "fn".to_string(),
                start_line: 1,
                end_line: 10,
                is_exported: false,
            },
        );
        index.symbols.insert(
            "src/lib.rs::Config".to_string(),
            SymbolEntry {
                file: "src/lib.rs".to_string(),
                name: "Config".to_string(),
                kind: "struct".to_string(),
                start_line: 5,
                end_line: 20,
                is_exported: true,
            },
        );
        index.symbols.insert(
            "src/lib.rs::Config::load".to_string(),
            SymbolEntry {
                file: "src/lib.rs".to_string(),
                name: "Config::load".to_string(),
                kind: "method".to_string(),
                start_line: 22,
                end_line: 35,
                is_exported: true,
            },
        );
        GraphProvider::GraphIndex(index)
    }

    #[test]
    fn find_exact_match() {
        let gp = test_provider();
        let results = gp.find_symbols("main", None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "main");
    }

    #[test]
    fn find_with_kind_filter() {
        let gp = test_provider();
        let results = gp.find_symbols("Config", None, Some("struct"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].kind, "struct");
    }

    #[test]
    fn find_with_file_filter() {
        let gp = test_provider();
        let results = gp.find_symbols("Config", Some("lib.rs"), None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn no_match_returns_empty() {
        let gp = test_provider();
        let results = gp.find_symbols("nonexistent", None, None);
        assert!(results.is_empty());
    }

    #[test]
    fn render_single_header_carries_handle() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src")).expect("mkdir");
        std::fs::write(
            tmp.path().join("src/lib.rs"),
            "struct Config;\nimpl Config { fn load() {} }\n",
        )
        .expect("write");
        let mut idx = ProjectIndex::new(tmp.path().to_str().unwrap());
        idx.symbols.insert(
            "src/lib.rs::Config".to_string(),
            SymbolEntry {
                file: "src/lib.rs".to_string(),
                name: "Config".to_string(),
                kind: "struct".to_string(),
                start_line: 1,
                end_line: 1,
                is_exported: true,
            },
        );
        let gp = GraphProvider::GraphIndex(idx);
        let sym = gp
            .find_symbols("Config", None, None)
            .into_iter()
            .next()
            .unwrap();
        let (out, _) = render_single(&sym, &gp, tmp.path().to_str().unwrap());
        assert!(
            out.contains("src/lib.rs#Config@L1"),
            "header must carry the stable handle, got: {out}"
        );
    }

    #[test]
    fn render_by_handle_rejects_malformed() {
        let (out, tok) = render_by_handle("not-a-handle", "/tmp/does-not-exist");
        assert!(out.contains("Invalid handle"), "got: {out}");
        assert_eq!(tok, 0);
    }
}
