# LoCoMo Memory Benchmark — lean-ctx

Suite: `reference-suite` · samples: 2 · questions: 13 · top_k: 5

Retrieval-recall benchmark: each conversation turn is stored as a memory, then for every question the top-k memories are recalled and scored against the gold answers. Model-free and deterministic.

## Overall

| metric | value |
|---|---|
| answer containment (recall@5) | 100.0% |
| mean best-memory token-F1 | 0.229 |
| exact-match rate | 0.0% |
| mean recalled-context tokens | 82 |
| mean full-transcript tokens | 116 |
| token reduction vs. full transcript | 29.4% |

## By category

| category | questions | containment | mean F1 | recall tokens |
|---|---|---|---|---|
| single-hop | 11 | 100.0% | 0.232 | 80 |
| temporal | 2 | 100.0% | 0.213 | 95 |

_Generated 2026-06-09T06:50:06.284618+00:00._
