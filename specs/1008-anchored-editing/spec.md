# Spec: Anchored Editing — Hash-Anchored Read↔Edit  (refs #1008)

> SDD spec-anchored: this file is the source of truth for **intent**.
> Code + tests enforce it. When requirements change, update the spec first.

## Problem / Why
`ctx_edit` is a `str_replace` tool, so it inherits the best-documented,
model-independent failure class in agentic coding: the **exact-recall tax** — the
model must reproduce `old_string` byte-for-byte (indentation, whitespace,
near-duplicate lines). When it cannot, the edit fails or, worse, hits the wrong
occurrence. The Hashline benchmark (16 models) shows up to **10×** edit-reliability
improvement from anchor editing alone, strongest on weaker models because their
edit errors are *mechanical*, not cognitive.

lean-ctx is a model-agnostic proxy/MCP in front of every model, so it is the
ideal layer to remove this tax for all of them at once.

## Goal
Ship hash-anchored editing as the second pillar next to compression: every code
read can return verifiable `N:hh|line` anchors, and a new `ctx_patch` tool edits
by anchor (line + content hash) so the model never reproduces old text.

## Acceptance Criteria (EARS)
- WHEN `ctx_read` is called with `mode="anchored"`, THE read pipeline SHALL return each line as `N:hh|content` (N=1-based line, hh=first 4 hex of `blake3(trim_end(line))`).
- THE `ctx_read` default `full` output SHALL remain byte-identical to before this feature (determinism #498).
- WHEN identical `mode="anchored"` reads are issued for an unchanged file, THE output SHALL be byte-stable (no timestamps/counters), preserving provider prompt caching.
- WHEN `ctx_patch` applies `set_line`/`replace_lines`/`insert_after`/delete and every anchor hash matches the current file, THE tool SHALL apply the edit atomically.
- WHEN any anchor hash does not match the current line, THE tool SHALL reject the edit without any partial write and return fresh anchors for the affected region.
- WHEN `ctx_patch` is given `ops:[...]`, THE tool SHALL validate all anchors against the *same* preimage, apply bottom-up, and be all-or-nothing.
- WHEN an edit would introduce a new syntax error into a file that parsed cleanly before, THE tree-sitter gate SHALL reject it (unsupported language → skip the gate).
- THE existing `ctx_edit` (str_replace) tool SHALL remain available and unchanged as a fallback.

## Determinism Invariant (#498)
Anchors are a pure function of line bytes. `mode=anchored` adds **no** wall-clock,
counter or random element to the output body. Default `full`/`raw`/`map`/… outputs
are untouched (anchored is an opt-in mode + versioned behaviour `anchored:v1`).
Regression guard: `annotate_is_deterministic` (core::anchor) +
`anchored_output_is_byte_stable_across_calls` (ctx_read render).

## Blast Radius (Impact-First, ctx_impact intent)
New, additive — no existing tool output changes shape:
- `rust/src/core/anchor.rs` (new SSOT hash+render primitive).
- `rust/src/core/mod.rs` (+`pub mod anchor`).
- `rust/src/tools/ctx_read/mode.rs` (+`ReadMode::Anchored`, 3 classifier arms, tests).
- `rust/src/tools/ctx_read/render.rs` (+`format_anchored_output`, +`anchored` match arm).
- `rust/src/tools/registered/ctx_read.rs` (description only).
- `rust/src/tools/edit_io.rs` (new: file-I/O primitives extracted from `ctx_edit` for reuse — `ctx_edit` behaviour unchanged).
- `rust/src/tools/ctx_patch/**` (new tool + modules).
- `rust/src/server/registry.rs` + `registered/mod.rs` + `permission_inheritance.rs` (register `ctx_patch`).
Reverse-deps of touched read APIs: `tools/registered/ctx_read.rs`,
`cli/read_cmd.rs`, `core/conformance.rs` — all consume by string mode, so the
additive `anchored` mode cannot break them.

## Out of Scope
- Removing or rewriting `ctx_edit` (kept as fallback).
- Changing the default read mode or any non-anchored output bytes.
- Neural "fast apply" merge models (anchors are deterministic, not learned).

## Verification (deterministic first)
- `cargo test -q -p lean-ctx anchor` (hash + annotate)
- `cargo test -q -p lean-ctx ctx_patch` (apply / staleness / batch atomic)
- `cargo test -q -p lean-ctx anchored` (read determinism)
- `cargo clippy -- -W clippy::all` (zero warnings)
- Phase 4: edit-reliability benchmark `ctx_edit` vs `ctx_patch` across languages.

## Links
- Tracking epic: #1008 · Subtickets: #1009 #1010 #1011 #1012 #1013 #1014 #1015
- Plan: `~/.cursor/plans/anchored_editing_13a381e4.plan.md` · Tasks: ./tasks.md
