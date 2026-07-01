# Portable Knowledge with OKF

The **Open Knowledge Format (OKF)** is a vendor-neutral way to carry a project's
knowledge as plain Markdown. lean-ctx can export its knowledge base to an OKF
bundle and read one back — so your accumulated project facts are never locked
inside lean-ctx's private store.

An OKF bundle is a **directory of Markdown files**:

```
kb-okf/
├── index.md                 # overview (categories + counts)
├── log.md                   # consolidated-insight history (if any)
├── architecture/
│   ├── auth.md              # one concept per file
│   └── db.md
└── patterns/
    └── naming.md
```

Each concept file has a small YAML frontmatter (only `type` is required by OKF)
and a Markdown body. lean-ctx-specific fields ride along as producer-owned
`leanctx_*` keys so an export → import round-trip is lossless.

## Export

```bash
lean-ctx knowledge export --format okf --output ./kb-okf
```

Or from an MCP agent:

```jsonc
ctx_knowledge(action="export", format="okf", path="./kb-okf")
```

A single concept file looks like this:

```markdown
---
type: "architecture"
title: "auth"
description: "Auth uses JWT RS256 tokens verified against Redis sessions."
tags:
  - "architecture"
timestamp: "2026-06-24T10:00:00+00:00"
leanctx_archetype: "architecture"
leanctx_category: "architecture"
leanctx_confidence: 0.9
leanctx_key: "auth"
leanctx_source_session: "s1"
---

Auth uses JWT RS256 tokens verified against Redis sessions.

## Relations

- depends_on: [architecture/db](db.md)
```

The `## Relations` section renders the knowledge graph: each edge becomes a
Markdown link to the target concept, labelled with the relation
(`depends_on`, `related_to`, `supports`, `contradicts`, `supersedes`).

## Edit by hand, review in git

Because a bundle is just Markdown, you can:

- **fix a fact** by editing its body,
- **add a relation** by adding a `- depends_on: [category/key](path.md)` line,
- **review changes in a pull request** like any other docs change.

Exports are **deterministic** — the same knowledge always produces byte-identical
files (fixed frontmatter key order, stable filenames, sorted relations). Diffs
show only what actually changed, and re-exporting never churns your history.

## Import

```bash
lean-ctx knowledge import ./kb-okf --merge append
```

```jsonc
ctx_knowledge(action="import", path="./kb-okf", merge="append")
```

Merge strategies match the JSON importer: `append`, `replace`, `skip-existing`
(default). Import reconstructs facts first, then relations — an edge is only
created when **both endpoints are current facts**, so a bundle can never leave
dangling links in your graph.

## Importing a foreign bundle

OKF only mandates `type`. A bundle written by another tool imports cleanly even
with nothing but a type and a body:

```markdown
---
type: architecture
---

We run everything on Kubernetes.
```

lean-ctx maps the OKF `type` to its nearest archetype (unknown types become
plain `fact`), derives the category from `tags` (or `imported`), and uses the
body as the value. Unknown frontmatter keys are tolerated, never fatal.

## Check a bundle before importing

Import surfaces non-fatal **lint warnings** (missing `type`, empty body,
unreadable file). They never block the import — a partially malformed bundle
still imports everything it can, and the warnings tell you what to fix.

## When to use OKF vs other formats

OKF is the *portable, hand-editable* format. For a lossless machine backup use
native JSON; to ship or sell a signed pack use ctxpkg. See
[Knowledge Formats — which one, when](knowledge-formats.md) for the full map.

## Notes

- OKF export/import is a **local feature — free on every plan.**
- The bundle is rendered from the same `KnowledgeSnapshot` as a ctxpkg export, so
  the two never disagree on the project's knowledge.
