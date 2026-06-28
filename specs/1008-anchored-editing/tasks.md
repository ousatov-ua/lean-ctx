# Tasks: Anchored Editing (epic #1008)

Each task maps 1:1 to a GitLab subticket (label `epic::1008-anchored-editing`).

| Phase | Ticket | Deliverable | Status |
|------|--------|-------------|--------|
| 1a | #1009 | `ctx_read mode=anchored` + `core::anchor` primitive | in-progress |
| 1b | #1010 | `ctx_patch` tool (set_line/replace_lines/insert_after/delete) + `edit_io` reuse | ready |
| 1c | #1011 | batch-atomic `ops[]` + determinism regression test | ready |
| 2  | #1012 | compression-aware symbol anchors + escalation + `replace_symbol` | ready |
| 3a | #1013 | tree-sitter `parse_has_errors` post-edit gate | ready |
| 3b | #1014 | steering (tool descriptions + rules) + `edit_quality` high-signal | ready |
| 4  | #1015 | edit-reliability benchmark + dashboard | ready |

## Design decisions
- **Anchor format**: `N:hh|content`, `hh` = first 4 hex of `blake3(trim_end(line))`
  (whitespace-tolerant, 16-bit collision guard combined with the line number).
- **Hash source**: BLAKE3 (consistent with `expected_hash` in `ctx_refactor` /
  `edit_apply`), not the MD5 whole-file fingerprints.
- **Delete**: `new_text=""` (readseek convention).
- **Atomic write**: shared `tools::edit_io` primitives (extracted from `ctx_edit`,
  no behaviour change) — TOCTOU preimage guard + permission-preserving rename.
- **Determinism**: opt-in mode; default outputs byte-stable; `anchored:v1`.

## Phase boundaries
Phase 1 (1a–1c) is independently shippable and captures the bulk of the
benchmark win. Phases 2–4 are premium pillars layered on top.
