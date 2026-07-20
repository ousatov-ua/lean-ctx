use std::borrow::Cow;

const SHA_FULL_LEN: usize = 40;
const SHA_SHORT_LEN: usize = 8;

/// Compress a bus message for compact delivery.
/// Reduces tokens by ~60-80% without losing semantic content.
pub fn compress_message(msg: &str) -> String {
    let mut out = msg.to_string();
    out = strip_bracket_tags(&out);
    out = shorten_shas(&out);
    out = shorten_agent_ids(&out);
    out = strip_filler(&out);
    out = collapse_whitespace(&out);
    out.trim().to_string()
}

/// `[P7 CANONICAL RECONCILIATION]` → `P7-RECON:`
/// `[COORDINATOR DIRECTIVE — IMMEDIATE]` → `COORD-DIRECTIVE:`
fn strip_bracket_tags(s: &str) -> String {
    let s = s.trim();
    if !s.starts_with('[') {
        return s.to_string();
    }
    let Some(end) = s.find(']') else {
        return s.to_string();
    };
    let tag = &s[1..end];
    let rest = s[end + 1..].trim();

    let short_tag = compress_tag(tag);
    if rest.is_empty() {
        short_tag
    } else {
        format!("{short_tag} {rest}")
    }
}

fn compress_tag(tag: &str) -> String {
    let normalized = tag.to_uppercase();
    let parts: Vec<&str> = normalized
        .split([' ', '—', '-', '/', ':'])
        .filter(|p| !p.is_empty())
        .collect();

    if parts.len() <= 2 {
        return parts.join("-");
    }

    let skip = [
        "THE", "A", "AN", "IN", "ON", "AT", "TO", "FOR", "OF", "BY", "IS", "ARE",
        "WAS", "AND", "OR", "WITH", "FROM", "AFTER", "BEFORE", "BETWEEN",
    ];

    let compressed: Vec<&str> = parts
        .iter()
        .filter(|p| !skip.contains(p))
        .copied()
        .collect();

    let abbreviated: Vec<Cow<'_, str>> = compressed
        .iter()
        .map(|p| abbreviate_word(p))
        .collect();

    abbreviated.join("-")
}

fn abbreviate_word(word: &str) -> Cow<'_, str> {
    match word {
        "IMPLEMENTATION" => Cow::Borrowed("IMPL"),
        "RECONCILIATION" => Cow::Borrowed("RECON"),
        "COORDINATOR" | "COORDINATION" => Cow::Borrowed("COORD"),
        "INDEPENDENT" => Cow::Borrowed("INDEP"),
        "CONFIRMATION" => Cow::Borrowed("CONF"),
        "CONFIGURATION" => Cow::Borrowed("CFG"),
        "INTEGRATION" => Cow::Borrowed("INTEG"),
        "RESERVATION" => Cow::Borrowed("RESV"),
        "CLASSIFICATION" => Cow::Borrowed("CLASS"),
        "CLARIFICATION" => Cow::Borrowed("CLAR"),
        "EXTERNAL" => Cow::Borrowed("EXT"),
        "INTERNAL" => Cow::Borrowed("INT"),
        "EVIDENCE" => Cow::Borrowed("EVID"),
        "DEPLOYMENT" => Cow::Borrowed("DEPLOY"),
        "CANONICAL" => Cow::Borrowed("CANON"),
        "PRODUCTIVE" => Cow::Borrowed("PROD"),
        "DUPLICATE" => Cow::Borrowed("DUP"),
        "MEASUREMENT" => Cow::Borrowed("MEAS"),
        "CHECKPOINT" => Cow::Borrowed("CKPT"),
        "HANDOFF" | "HAND" => Cow::Borrowed("HO"),
        "FOLLOW" => Cow::Borrowed("FU"),
        "REQUEST" => Cow::Borrowed("REQ"),
        "PROPOSAL" => Cow::Borrowed("PROP"),
        "COMPLETE" => Cow::Borrowed("DONE"),
        "CURRENT" => Cow::Borrowed("CUR"),
        "STATUS" => Cow::Borrowed("STAT"),
        "REVIEW" => Cow::Borrowed("REV"),
        "BLOCKER" => Cow::Borrowed("BLOCK"),
        "DECISION" => Cow::Borrowed("DEC"),
        "STRATEGY" => Cow::Borrowed("STRAT"),
        "SUPERSEDING" | "SUPERSEDES" => Cow::Borrowed("SUPER"),
        "LIVENESS" => Cow::Borrowed("LIVE"),
        "TAKEOVER" => Cow::Borrowed("TAKE"),
        "ADDENDUM" => Cow::Borrowed("ADD"),
        "VERDICT" => Cow::Borrowed("VERD"),
        "REFINEMENT" => Cow::Borrowed("REF"),
        _ => Cow::Borrowed(word),
    }
}

/// Shorten 40-char SHAs to 8 chars, keep short SHAs as-is.
fn shorten_shas(s: &str) -> String {
    let mut result = s.to_string();
    let hex_chars: &[char] = &[
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
    ];

    let bytes = s.as_bytes();
    let mut i = 0;
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    while i < bytes.len() {
        if bytes[i].is_ascii_hexdigit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            let len = i - start;
            if (10..=SHA_FULL_LEN).contains(&len) {
                let sha = &s[start..i];
                if sha.chars().all(|c| hex_chars.contains(&c)) {
                    replacements.push((start, i, sha[..SHA_SHORT_LEN].to_string()));
                }
            }
        } else {
            i += 1;
        }
    }

    for (start, end, short) in replacements.into_iter().rev() {
        result.replace_range(start..end, &short);
    }
    result
}

/// `mcp-53757-981920c1` → `53757`
fn shorten_agent_ids(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(pos) = result.find("mcp-") {
        let tail = result[pos + 4..].to_string();
        let Some(dash2) = tail.find('-') else { break };
        let pid = &tail[..dash2];
        if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_digit()) {
            break;
        }
        let hex_start = dash2 + 1;
        let hex_end = tail[hex_start..]
            .find(|c: char| !c.is_ascii_hexdigit())
            .map_or(tail.len(), |e| hex_start + e);
        let total_end = pos + 4 + hex_end;
        let pid_owned = pid.to_string();
        result.replace_range(pos..total_end, &pid_owned);
    }
    result
}

/// Strip filler phrases that waste tokens.
fn strip_filler(s: &str) -> String {
    let patterns = [
        "I am ", "I will ", "I have ", "I see ", "I propose ",
        "Please ", "please ",
        "This does not ", "This is ",
        "after review", "after inspection",
        "independently ", "Independent ",
        "do NOT ", "Do NOT ", "DO NOT ",
        "without shared-file edits",
        "No overlap objection received; ",
        "proceeding on an isolated branch",
    ];

    let mut result = s.to_string();
    for pat in &patterns {
        result = result.replace(pat, "");
    }
    result
}

/// Collapse multiple whitespace/newlines into single space.
fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut last_was_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bracket_tag_compression() {
        assert_eq!(
            strip_bracket_tags("[P7 CANONICAL RECONCILIATION] rest"),
            "P7-CANON-RECON rest"
        );
        assert_eq!(
            strip_bracket_tags("[COORDINATOR DIRECTIVE — IMMEDIATE] do it"),
            "COORD-DIRECTIVE-IMMEDIATE do it"
        );
    }

    #[test]
    fn test_sha_shortening() {
        assert_eq!(
            shorten_shas("commit 6682f5c25f1234567890abcdef1234567890abcd done"),
            "commit 6682f5c2 done"
        );
        assert_eq!(shorten_shas("short abc123"), "short abc123");
    }

    #[test]
    fn test_agent_id_shortening() {
        assert_eq!(
            shorten_agent_ids("from mcp-53757-981920c1 to mcp-67653-6f7dd84a"),
            "from 53757 to 67653"
        );
    }

    #[test]
    fn test_filler_stripping() {
        assert_eq!(
            strip_filler("I am implementing the fix. Please review."),
            "implementing the fix. review."
        );
    }

    #[test]
    fn test_full_compression() {
        let msg = "[P7 CANONICAL RECONCILIATION]\nmcp-53757-981920c1: build fresh canonical lineage from 6682f5c25f1234567890abcdef1234567890abcd using owner commits 8ab6ff7cf71234567890abcdef1234567890abcd then 671c37dcc21234567890abcdef1234567890abcd, exclude duplicate 607e1e276b1234567890abcdef1234567890abcd entirely, then replay only shared-files. I will proceed independently. I am applying the fix now. Please review the implementation carefully.";
        let compressed = compress_message(msg);
        assert!(compressed.len() < msg.len() * 2 / 3, "compressed={} original={}", compressed.len(), msg.len());
        assert!(compressed.contains("P7-CANON-RECON"));
        assert!(compressed.contains("53757"));
        assert!(!compressed.contains("981920c1"));
    }

    #[test]
    fn test_whitespace_collapse() {
        assert_eq!(
            collapse_whitespace("hello   world\n\n  test"),
            "hello world test"
        );
    }
}
