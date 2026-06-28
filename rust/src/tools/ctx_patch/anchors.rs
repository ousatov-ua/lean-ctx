//! Anchored-edit operations: the `AnchorOp` vocabulary, JSON parsing and the
//! per-anchor staleness check (epic #1008).
//!
//! An *anchor* is `(line, hash)` where `hash` is [`crate::core::anchor::line_hash`]
//! of the line the model was shown by `ctx_read(mode="anchored")`. The edit side
//! re-derives the hash from the *current* file and rejects the op if it drifted —
//! so the model never has to reproduce the old text, only reference it.

use serde_json::{Map, Value};

/// A single anchored edit. `new_text=""` deletes (readseek convention); a
/// multi-line `new_text` expands one anchor into several lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnchorOp {
    /// Replace (or delete, if `new_text==""`) a single line.
    SetLine {
        line: usize,
        hash: String,
        new_text: String,
    },
    /// Replace (or delete) the inclusive line range `start..=end`.
    ReplaceLines {
        start_line: usize,
        start_hash: String,
        end_line: usize,
        end_hash: String,
        new_text: String,
    },
    /// Insert after `line` (line 0 = top of file, needs no hash).
    InsertAfter {
        line: usize,
        hash: Option<String>,
        new_text: String,
    },
    /// Delete the inclusive line range `start..=end` (sugar for an empty
    /// `ReplaceLines`; a single-line delete uses `start==end`).
    Delete {
        start_line: usize,
        start_hash: String,
        end_line: usize,
        end_hash: String,
    },
}

/// A stale anchor: the line the model referenced no longer hashes to the value
/// it was given (the file drifted, or the model copied the wrong anchor).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnchorMiss {
    /// 1-based line the anchor pointed at.
    pub line: usize,
    /// The hash the model supplied.
    pub expected: String,
    /// The hash of the line currently on disk (`<eof>` when out of range).
    pub actual: String,
}

/// Parse the tool arguments into one or more [`AnchorOp`]s.
///
/// Two shapes are accepted: a batch `ops:[{…}, …]`, or a single op described by
/// the top-level fields. Every op must name its `op` explicitly so error
/// messages and model steering stay unambiguous (the "two tools" pitfall).
pub(crate) fn parse_ops(args: &Map<String, Value>) -> Result<Vec<AnchorOp>, String> {
    if let Some(ops) = args.get("ops") {
        let arr = ops
            .as_array()
            .ok_or_else(|| "ops must be an array of edit objects".to_string())?;
        if arr.is_empty() {
            return Err("ops[] is empty — provide at least one edit".to_string());
        }
        return arr
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let obj = v
                    .as_object()
                    .ok_or_else(|| format!("ops[{i}] must be an object"))?;
                parse_one(obj).map_err(|e| format!("ops[{i}]: {e}"))
            })
            .collect();
    }
    Ok(vec![parse_one(args)?])
}

fn parse_one(obj: &Map<String, Value>) -> Result<AnchorOp, String> {
    let op = get_str(obj, "op").ok_or_else(|| {
        "missing 'op' (one of: set_line, replace_lines, insert_after, delete)".to_string()
    })?;
    match op.as_str() {
        "set_line" => Ok(AnchorOp::SetLine {
            line: req_line(obj, "line")?,
            hash: req_str(obj, "hash")?,
            new_text: req_new_text(obj)?,
        }),
        "replace_lines" => Ok(AnchorOp::ReplaceLines {
            start_line: req_line(obj, "start_line")?,
            start_hash: req_str(obj, "start_hash")?,
            end_line: req_line(obj, "end_line")?,
            end_hash: req_str(obj, "end_hash")?,
            new_text: req_new_text(obj)?,
        }),
        "insert_after" => {
            // Line 0 means "insert at the top"; it has no preceding line to hash.
            let line = req_line_allow_zero(obj, "line")?;
            let hash = if line == 0 {
                None
            } else {
                Some(req_str(obj, "hash")?)
            };
            Ok(AnchorOp::InsertAfter {
                line,
                hash,
                new_text: req_new_text(obj)?,
            })
        }
        "delete" => {
            // Single-line delete ({line,hash}) or a range ({start,end}).
            if obj.contains_key("start_line") || obj.contains_key("end_line") {
                Ok(AnchorOp::Delete {
                    start_line: req_line(obj, "start_line")?,
                    start_hash: req_str(obj, "start_hash")?,
                    end_line: req_line(obj, "end_line")?,
                    end_hash: req_str(obj, "end_hash")?,
                })
            } else {
                let line = req_line(obj, "line")?;
                let hash = req_str(obj, "hash")?;
                Ok(AnchorOp::Delete {
                    start_line: line,
                    start_hash: hash.clone(),
                    end_line: line,
                    end_hash: hash,
                })
            }
        }
        "replace_symbol" => Err(
            "replace_symbol cannot be batched in ops[] — it is a different (symbol \
             resolution) write path; send it as a single top-level op"
                .to_string(),
        ),
        other => Err(format!(
            "unknown op '{other}' (one of: set_line, replace_lines, insert_after, delete, replace_symbol)"
        )),
    }
}

fn get_str(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn req_str(obj: &Map<String, Value>, key: &str) -> Result<String, String> {
    get_str(obj, key).ok_or_else(|| format!("missing '{key}'"))
}

/// `new_text` must be *present* but may be empty (`""` = delete).
fn req_new_text(obj: &Map<String, Value>) -> Result<String, String> {
    obj.get("new_text")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "missing 'new_text' (use \"\" to delete)".to_string())
}

/// A 1-based line number ≥ 1.
fn req_line(obj: &Map<String, Value>, key: &str) -> Result<usize, String> {
    let n = req_line_allow_zero(obj, key)?;
    if n == 0 {
        return Err(format!("'{key}' must be ≥ 1 (lines are 1-based)"));
    }
    Ok(n)
}

/// A line number ≥ 0 (0 only meaningful as `insert_after` "top of file").
fn req_line_allow_zero(obj: &Map<String, Value>, key: &str) -> Result<usize, String> {
    let v = obj
        .get(key)
        .ok_or_else(|| format!("missing '{key}'"))?
        .as_u64()
        .ok_or_else(|| format!("'{key}' must be a non-negative integer"))?;
    usize::try_from(v).map_err(|_| format!("'{key}' is out of range"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => panic!("expected a JSON object"),
        }
    }

    #[test]
    fn parses_single_set_line() {
        let ops = parse_ops(&obj(
            json!({"op": "set_line", "line": 3, "hash": "ab12", "new_text": "x"}),
        ))
        .unwrap();
        assert_eq!(
            ops,
            vec![AnchorOp::SetLine {
                line: 3,
                hash: "ab12".into(),
                new_text: "x".into()
            }]
        );
    }

    #[test]
    fn empty_new_text_is_allowed_for_delete() {
        let ops = parse_ops(&obj(
            json!({"op": "set_line", "line": 1, "hash": "aa", "new_text": ""}),
        ))
        .unwrap();
        assert!(matches!(&ops[0], AnchorOp::SetLine { new_text, .. } if new_text.is_empty()));
    }

    #[test]
    fn insert_after_top_needs_no_hash() {
        let ops = parse_ops(&obj(
            json!({"op": "insert_after", "line": 0, "new_text": "// header"}),
        ))
        .unwrap();
        assert_eq!(
            ops,
            vec![AnchorOp::InsertAfter {
                line: 0,
                hash: None,
                new_text: "// header".into()
            }]
        );
    }

    #[test]
    fn insert_after_nonzero_requires_hash() {
        let err = parse_ops(&obj(
            json!({"op": "insert_after", "line": 5, "new_text": "x"}),
        ))
        .unwrap_err();
        assert!(err.contains("hash"), "got: {err}");
    }

    #[test]
    fn delete_single_and_range() {
        let single = parse_ops(&obj(json!({"op": "delete", "line": 4, "hash": "cc"}))).unwrap();
        assert_eq!(
            single[0],
            AnchorOp::Delete {
                start_line: 4,
                start_hash: "cc".into(),
                end_line: 4,
                end_hash: "cc".into()
            }
        );
        let range = parse_ops(&obj(json!({
            "op": "delete", "start_line": 2, "start_hash": "aa", "end_line": 5, "end_hash": "bb"
        })))
        .unwrap();
        assert_eq!(
            range[0],
            AnchorOp::Delete {
                start_line: 2,
                start_hash: "aa".into(),
                end_line: 5,
                end_hash: "bb".into()
            }
        );
    }

    #[test]
    fn parses_batch_ops() {
        let ops = parse_ops(&obj(json!({
            "ops": [
                {"op": "set_line", "line": 1, "hash": "aa", "new_text": "A"},
                {"op": "insert_after", "line": 3, "hash": "bb", "new_text": "B"}
            ]
        })))
        .unwrap();
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn empty_ops_array_is_rejected() {
        let err = parse_ops(&obj(json!({"ops": []}))).unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn line_zero_rejected_for_set_line() {
        let err = parse_ops(&obj(
            json!({"op": "set_line", "line": 0, "hash": "aa", "new_text": "x"}),
        ))
        .unwrap_err();
        assert!(err.contains("1-based"), "got: {err}");
    }

    #[test]
    fn unknown_op_is_rejected() {
        let err = parse_ops(&obj(json!({"op": "frobnicate", "line": 1}))).unwrap_err();
        assert!(err.contains("unknown op"), "got: {err}");
    }

    #[test]
    fn missing_op_is_rejected() {
        let err = parse_ops(&obj(json!({"line": 1, "hash": "aa", "new_text": "x"}))).unwrap_err();
        assert!(err.contains("missing 'op'"), "got: {err}");
    }
}
