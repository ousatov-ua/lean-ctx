//! Pure line-model engine for anchored edits (epic #1008): split → validate
//! anchors against a single preimage → reject overlaps → splice bottom-up.
//!
//! Everything here is a pure function of `(content, ops)` so it is exhaustively
//! unit-testable without touching the filesystem; the I/O wrapper in
//! [`super`] handles the read/guard/atomic-write around it.

use crate::core::anchor;

use super::anchors::{AnchorMiss, AnchorOp};

/// Outcome of validating the ops against the preimage lines.
pub(crate) enum ResolveError {
    /// One or more anchors did not match the current file (staleness/drift).
    Conflict(Vec<AnchorMiss>),
    /// A structurally invalid op (out-of-range line, overlap, empty insert, …).
    Invalid(String),
}

/// A validated edit, normalized to a 0-based splice over the line vector.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedEdit {
    /// 0-based index where existing lines are replaced / new lines inserted.
    start_idx: usize,
    /// Number of existing lines removed (0 for a pure insert).
    remove_count: usize,
    /// Replacement lines (logical, no separators); empty = deletion.
    new_lines: Vec<String>,
    /// 1-based inclusive span the op depends on, for overlap detection.
    lo: usize,
    hi: usize,
}

/// Split `content` into logical lines plus the framing needed to rebuild it
/// byte-faithfully: the dominant line separator and whether a trailing newline
/// is present. Mirrors [`str::lines`] so line numbers match
/// `ctx_read(mode="anchored")`.
pub(crate) fn split_lines(content: &str) -> (Vec<String>, &'static str, bool) {
    let sep = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let trailing = content.ends_with('\n');
    let lines = content.lines().map(String::from).collect();
    (lines, sep, trailing)
}

/// Rebuild file content from logical `lines`, restoring the separator and the
/// original trailing-newline state.
pub(crate) fn join_lines(lines: &[String], sep: &str, trailing: bool) -> String {
    let mut out = lines.join(sep);
    if trailing && !lines.is_empty() {
        out.push_str(sep);
    }
    out
}

/// Split a `new_text` payload into logical replacement lines.
///
/// One trailing line separator is stripped (a habitual `"foo\n"` means the
/// single line `foo`, not `foo` + a blank). `""` → no lines (delete); `"\n"` →
/// one blank line.
fn split_new_text(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    let trimmed = s
        .strip_suffix("\r\n")
        .or_else(|| s.strip_suffix('\n'))
        .unwrap_or(s);
    trimmed
        .split('\n')
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect()
}

/// Validate every op's anchors against `lines` (the single preimage) and
/// normalize them to splices. Collects *all* stale anchors before failing so the
/// model gets the complete picture in one round-trip.
pub(crate) fn resolve_ops(
    lines: &[String],
    ops: &[AnchorOp],
) -> Result<Vec<ResolvedEdit>, ResolveError> {
    let len = lines.len();
    let mut misses: Vec<AnchorMiss> = Vec::new();
    let mut edits: Vec<ResolvedEdit> = Vec::new();

    let check = |line: usize, hash: &str, misses: &mut Vec<AnchorMiss>| {
        // 1-based; line is guaranteed ≥1 by the caller for anchored ops.
        match lines.get(line - 1) {
            Some(cur) if anchor::hash_matches(cur, hash) => {}
            Some(cur) => misses.push(AnchorMiss {
                line,
                expected: hash.to_string(),
                actual: anchor::line_hash(cur),
            }),
            None => misses.push(AnchorMiss {
                line,
                expected: hash.to_string(),
                actual: "<eof>".to_string(),
            }),
        }
    };

    for op in ops {
        match op {
            AnchorOp::SetLine {
                line,
                hash,
                new_text,
            } => {
                if *line > len {
                    return Err(ResolveError::Invalid(format!(
                        "line {line} is past end of file ({len} lines)"
                    )));
                }
                check(*line, hash, &mut misses);
                edits.push(ResolvedEdit {
                    start_idx: line - 1,
                    remove_count: 1,
                    new_lines: split_new_text(new_text),
                    lo: *line,
                    hi: *line,
                });
            }
            AnchorOp::ReplaceLines {
                start_line,
                start_hash,
                end_line,
                end_hash,
                new_text,
            } => {
                if let Err(e) = check_range(*start_line, *end_line, len) {
                    return Err(ResolveError::Invalid(e));
                }
                check(*start_line, start_hash, &mut misses);
                check(*end_line, end_hash, &mut misses);
                edits.push(ResolvedEdit {
                    start_idx: start_line - 1,
                    remove_count: end_line - start_line + 1,
                    new_lines: split_new_text(new_text),
                    lo: *start_line,
                    hi: *end_line,
                });
            }
            AnchorOp::Delete {
                start_line,
                start_hash,
                end_line,
                end_hash,
            } => {
                if let Err(e) = check_range(*start_line, *end_line, len) {
                    return Err(ResolveError::Invalid(e));
                }
                check(*start_line, start_hash, &mut misses);
                check(*end_line, end_hash, &mut misses);
                edits.push(ResolvedEdit {
                    start_idx: start_line - 1,
                    remove_count: end_line - start_line + 1,
                    new_lines: Vec::new(),
                    lo: *start_line,
                    hi: *end_line,
                });
            }
            AnchorOp::InsertAfter {
                line,
                hash,
                new_text,
            } => {
                if *line > len {
                    return Err(ResolveError::Invalid(format!(
                        "insert_after line {line} is past end of file ({len} lines); \
                         use line={len} to append"
                    )));
                }
                let new_lines = split_new_text(new_text);
                if new_lines.is_empty() {
                    return Err(ResolveError::Invalid(
                        "insert_after needs non-empty new_text (use delete to remove lines)"
                            .to_string(),
                    ));
                }
                if let Some(h) = hash {
                    check(*line, h, &mut misses);
                }
                edits.push(ResolvedEdit {
                    start_idx: *line,
                    remove_count: 0,
                    new_lines,
                    lo: *line,
                    hi: *line,
                });
            }
        }
    }

    if !misses.is_empty() {
        return Err(ResolveError::Conflict(misses));
    }
    if let Some(overlap) = first_overlap(&edits) {
        return Err(ResolveError::Invalid(overlap));
    }
    Ok(edits)
}

fn check_range(start: usize, end: usize, len: usize) -> Result<(), String> {
    if start > end {
        return Err(format!("start_line {start} is after end_line {end}"));
    }
    if end > len {
        return Err(format!("end_line {end} is past end of file ({len} lines)"));
    }
    Ok(())
}

/// Reject a batch where two edits touch overlapping line spans — their combined
/// result would be order-dependent. The model should split them or merge into a
/// single `replace_lines`. Returns a human-readable message for the first clash.
fn first_overlap(edits: &[ResolvedEdit]) -> Option<String> {
    for (i, a) in edits.iter().enumerate() {
        for b in &edits[i + 1..] {
            if a.lo <= b.hi && b.lo <= a.hi {
                return Some(format!(
                    "overlapping edits: lines {}-{} and {}-{} touch the same region — \
                     split into separate calls or merge into one replace_lines",
                    a.lo, a.hi, b.lo, b.hi
                ));
            }
        }
    }
    None
}

/// Apply validated `edits` to `lines`, bottom-up so earlier indices stay valid.
/// Non-overlap is guaranteed by [`resolve_ops`], so descending order is exact.
#[must_use]
pub(crate) fn apply_edits(mut lines: Vec<String>, mut edits: Vec<ResolvedEdit>) -> Vec<String> {
    edits.sort_by_key(|e| std::cmp::Reverse(e.start_idx));
    for e in edits {
        let end = e.start_idx + e.remove_count;
        lines.splice(e.start_idx..end, e.new_lines);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anc(_line: usize, content: &str) -> String {
        // The (line, hash) a model would have been shown for `content`; `_line`
        // is kept at call sites only to read like a real anchor reference.
        anchor::line_hash(content)
    }

    #[test]
    fn split_and_join_round_trip_lf() {
        let content = "a\nb\nc\n";
        let (lines, sep, trailing) = split_lines(content);
        assert_eq!(lines, vec!["a", "b", "c"]);
        assert_eq!(sep, "\n");
        assert!(trailing);
        assert_eq!(join_lines(&lines, sep, trailing), content);
    }

    #[test]
    fn split_and_join_round_trip_no_trailing() {
        let content = "a\nb";
        let (lines, sep, trailing) = split_lines(content);
        assert!(!trailing);
        assert_eq!(join_lines(&lines, sep, trailing), content);
    }

    #[test]
    fn split_and_join_round_trip_crlf() {
        let content = "a\r\nb\r\n";
        let (lines, sep, trailing) = split_lines(content);
        assert_eq!(sep, "\r\n");
        assert_eq!(join_lines(&lines, sep, trailing), content);
    }

    #[test]
    fn split_new_text_strips_one_trailing_newline() {
        assert_eq!(split_new_text("foo"), vec!["foo"]);
        assert_eq!(split_new_text("foo\n"), vec!["foo"]);
        assert_eq!(split_new_text("foo\nbar"), vec!["foo", "bar"]);
        assert_eq!(split_new_text("foo\nbar\n"), vec!["foo", "bar"]);
        assert_eq!(split_new_text(""), Vec::<String>::new());
        assert_eq!(split_new_text("\n"), vec![""]);
    }

    #[test]
    fn set_line_replaces_in_place() {
        let lines = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let ops = vec![AnchorOp::SetLine {
            line: 2,
            hash: anc(2, "b"),
            new_text: "B".to_string(),
        }];
        let edits = resolve_ops(&lines, &ops).ok().unwrap();
        let out = apply_edits(lines, edits);
        assert_eq!(out, vec!["a", "B", "c"]);
    }

    #[test]
    fn set_line_empty_text_deletes() {
        let lines = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let ops = vec![AnchorOp::SetLine {
            line: 2,
            hash: anc(2, "b"),
            new_text: String::new(),
        }];
        let edits = resolve_ops(&lines, &ops).ok().unwrap();
        assert_eq!(apply_edits(lines, edits), vec!["a", "c"]);
    }

    #[test]
    fn set_line_expands_to_multiple_lines() {
        let lines = vec!["a".to_string(), "b".to_string()];
        let ops = vec![AnchorOp::SetLine {
            line: 1,
            hash: anc(1, "a"),
            new_text: "x\ny\nz".to_string(),
        }];
        let edits = resolve_ops(&lines, &ops).ok().unwrap();
        assert_eq!(apply_edits(lines, edits), vec!["x", "y", "z", "b"]);
    }

    #[test]
    fn replace_lines_collapses_range() {
        let lines = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let ops = vec![AnchorOp::ReplaceLines {
            start_line: 2,
            start_hash: anc(2, "b"),
            end_line: 3,
            end_hash: anc(3, "c"),
            new_text: "X".to_string(),
        }];
        let edits = resolve_ops(&lines, &ops).ok().unwrap();
        assert_eq!(apply_edits(lines, edits), vec!["a", "X", "d"]);
    }

    #[test]
    fn insert_after_line_zero_prepends() {
        let lines = vec!["a".to_string(), "b".to_string()];
        let ops = vec![AnchorOp::InsertAfter {
            line: 0,
            hash: None,
            new_text: "// top".to_string(),
        }];
        let edits = resolve_ops(&lines, &ops).ok().unwrap();
        assert_eq!(apply_edits(lines, edits), vec!["// top", "a", "b"]);
    }

    #[test]
    fn insert_after_last_line_appends() {
        let lines = vec!["a".to_string(), "b".to_string()];
        let ops = vec![AnchorOp::InsertAfter {
            line: 2,
            hash: Some(anc(2, "b")),
            new_text: "c".to_string(),
        }];
        let edits = resolve_ops(&lines, &ops).ok().unwrap();
        assert_eq!(apply_edits(lines, edits), vec!["a", "b", "c"]);
    }

    #[test]
    fn stale_anchor_is_reported_as_conflict() {
        let lines = vec!["a".to_string(), "b".to_string()];
        let ops = vec![AnchorOp::SetLine {
            line: 2,
            hash: "ffff".to_string(), // wrong hash
            new_text: "B".to_string(),
        }];
        match resolve_ops(&lines, &ops) {
            Err(ResolveError::Conflict(misses)) => {
                assert_eq!(misses.len(), 1);
                assert_eq!(misses[0].line, 2);
                assert_eq!(misses[0].actual, anchor::line_hash("b"));
            }
            _ => panic!("expected a Conflict for the stale anchor"),
        }
    }

    #[test]
    fn all_stale_anchors_collected() {
        let lines = vec!["a".to_string(), "b".to_string()];
        let ops = vec![
            AnchorOp::SetLine {
                line: 1,
                hash: "0000".into(),
                new_text: "A".into(),
            },
            AnchorOp::SetLine {
                line: 2,
                hash: "1111".into(),
                new_text: "B".into(),
            },
        ];
        match resolve_ops(&lines, &ops) {
            Err(ResolveError::Conflict(misses)) => assert_eq!(misses.len(), 2),
            _ => panic!("expected both misses collected"),
        }
    }

    #[test]
    fn overlapping_edits_rejected() {
        let lines = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let ops = vec![
            AnchorOp::SetLine {
                line: 2,
                hash: anc(2, "b"),
                new_text: "B".into(),
            },
            AnchorOp::ReplaceLines {
                start_line: 1,
                start_hash: anc(1, "a"),
                end_line: 2,
                end_hash: anc(2, "b"),
                new_text: "X".into(),
            },
        ];
        match resolve_ops(&lines, &ops) {
            Err(ResolveError::Invalid(msg)) => assert!(msg.contains("overlapping")),
            _ => panic!("expected overlap rejection"),
        }
    }

    #[test]
    fn out_of_range_line_rejected() {
        let lines = vec!["a".to_string()];
        let ops = vec![AnchorOp::SetLine {
            line: 5,
            hash: "aa".into(),
            new_text: "x".into(),
        }];
        assert!(matches!(
            resolve_ops(&lines, &ops),
            Err(ResolveError::Invalid(_))
        ));
    }

    #[test]
    fn op_input_order_does_not_change_result() {
        // Determinism (#498): a batch validated against one preimage must be a
        // pure function of the *set* of ops — input order is irrelevant because
        // application is sorted bottom-up. Guards against accidental
        // order-sensitivity creeping into resolve/apply.
        let lines: Vec<String> = ["1", "2", "3", "4", "5"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let a = AnchorOp::SetLine {
            line: 1,
            hash: anc(1, "1"),
            new_text: "A\nA2".into(),
        };
        let b = AnchorOp::ReplaceLines {
            start_line: 3,
            start_hash: anc(3, "3"),
            end_line: 4,
            end_hash: anc(4, "4"),
            new_text: "B".into(),
        };
        let c = AnchorOp::InsertAfter {
            line: 5,
            hash: Some(anc(5, "5")),
            new_text: "C".into(),
        };

        let forward = apply_edits(
            lines.clone(),
            resolve_ops(&lines, &[a.clone(), b.clone(), c.clone()])
                .ok()
                .unwrap(),
        );
        let reversed = apply_edits(lines.clone(), resolve_ops(&lines, &[c, b, a]).ok().unwrap());
        assert_eq!(forward, reversed);
        assert_eq!(forward, vec!["A", "A2", "2", "B", "5", "C"]);
    }

    #[test]
    fn batch_bottom_up_keeps_line_numbers_valid() {
        // Two independent edits; applying top-down would shift the 2nd. Bottom-up
        // (handled by apply_edits) keeps both correct.
        let lines = vec![
            "1".to_string(),
            "2".to_string(),
            "3".to_string(),
            "4".to_string(),
            "5".to_string(),
        ];
        let ops = vec![
            AnchorOp::ReplaceLines {
                start_line: 1,
                start_hash: anc(1, "1"),
                end_line: 2,
                end_hash: anc(2, "2"),
                new_text: "A".into(), // 2 lines → 1 line (shifts later indices)
            },
            AnchorOp::SetLine {
                line: 5,
                hash: anc(5, "5"),
                new_text: "E".into(),
            },
        ];
        let edits = resolve_ops(&lines, &ops).ok().unwrap();
        assert_eq!(apply_edits(lines, edits), vec!["A", "3", "4", "E"]);
    }
}
