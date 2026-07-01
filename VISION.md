# LeanCTX Vision

> **Control what your AI can see.**
>
> Stack overview (LeanCTX · CTXPKG · ctxpkg.com · CTXFabric): [`ECOSYSTEM.md`](ECOSYSTEM.md)

## The Cognitive Context Layer

High performance with LLMs isn't about bigger context windows — it's about
**information density**. LeanCTX is the cognitive context layer between your
AI and your code: every token reaching the LLM carries maximum signal, and
every byte of noise stripped away is a byte of reasoning gained.

> The winners won't be those who can afford 1M-token contexts.
> They'll be those who achieve the same result with 10K.

## The four dimensions

1. **Compression layer (input efficiency)** — AST-based signatures, delta
   loading, session caching (re-reads ~13 tokens), entropy filtering, 95+ CLI
   compression patterns, 26 tree-sitter languages, 10 read modes.
2. **Semantic router (model selection)** — intent detection, mode prediction
   learned per file type, LITM-aware positioning per model family.
3. **Context manager (memory architecture)** — Context Continuity Protocol
   (~400 tokens instead of ~50K cold start), context ledger, multi-agent
   coordination, temporal knowledge system, property graph with hybrid
   search fusion.
4. **Quality guardrail (output verification)** — compression safety levels,
   deterministic anchoring, 19 versioned contracts with CI drift gates,
   policy packs, tamper-evident audit trails, Ed25519-signed evidence bundles.

Technical depth: [`docs/cognition-interface.md`](docs/cognition-interface.md) ·
[`CONTRACTS.md`](CONTRACTS.md)

## Two halves of context, one pipeline

Getting the right knowledge into the window is really *two* problems, and most
tools only solve one:

- **Compress what fits.** A file, a diff, a shell log, a handful of docs — the
  right move is to fit it into the window *losslessly* (read modes, structural
  crushing, cached re-reads). Embedding-and-retrieving here throws away
  information you already had room for.
- **Retrieve what doesn't.** A large or dynamic knowledge base has to be
  retrieved — and lean-ctx does it *hybrid* (BM25 + dense vectors + Reciprocal
  Rank Fusion + rerank), never a single cosine signal.

The failure mode of naive RAG is applying *retrieve* to everything, including
material that never needed it — more chunks, less signal, quiet drift. lean-ctx
runs both halves under **one pipeline** and picks the right one for the material.

**The moat is structure.** A codebase is not a bag of paragraphs: functions call
functions, changes have a blast radius, symbols have definitions and references.
lean-ctx is structure-aware (tree-sitter AST + code graph across 26 languages),
so retrieval is *precise on code* in a way pure text-embedding search cannot be.

**Knowledge stays yours.** What the engine learns is portable, not harvested:
export it as open, git-diffable **OKF** Markdown (no lock-in), or as a signed,
versioned **`.ctxpkg`** for distribution through the registry. One knowledge
model, many renderings — the same source of truth whether you open it up or ship
it out.

## Principles

- **Local-first, zero telemetry.** Nothing leaves your machine automatically —
  ever. The engine learns locally (read modes, compression thresholds,
  bandits); what it learns belongs to you.
- **Learned optimization is portable, not harvested.** Tuned profiles can be
  exported as signed `.ctxpkg` packages and shared through the registry — a
  deliberate, inspectable file, not a background upload.
- **Evidence over claims.** Policy decides what an agent may see; signed
  evidence proves what it saw. Compliance reports (EU AI Act, ISO/IEC 42001,
  SOC 2) are generated from real session data, offline-verifiable.
- **One binary, 30+ tools.** Cursor, Claude Code, CodeBuddy, Windsurf, Copilot, Codex,
  Gemini, JetBrains and more — the same engine everywhere.

## Direction

- **Context Time Machine** — the layer state (what the model saw, why, and at
  what token ROI) is now a git-anchored, signed, navigable artifact: rewind to
  any commit, reproduce it, resume from it, or share it. The temporal axis
  through everything lean-ctx does — it *decides, remembers, guards, proves, and
  now replays*. **Shipped:** the snapshot engine (`snapshot
  create/list/show/verify`), dashboard replay, `restore [--git]`, and signed
  file-based `publish`/`import`. **Next:** a `ctxpkg.com` registry for hosted,
  versioned history and a side-by-side model-view ｜ git-diff replay. See
  [`docs/concepts/context-time-machine.md`](docs/concepts/context-time-machine.md).
- **Context as Code** — declarative pipelines, profiles and policies in TOML,
  version-controlled like infrastructure.
- **Cognition interface** — constraints-aware instruction compilation,
  attention-aware layout, budget/SLO enforcement, proof-carrying context.
- **Unified context graph** — code, tests, commits, CI runs and knowledge in
  one semantic graph with graph-aware reads.
- **Provider framework** — issues, tickets, CI and logs flowing through the
  same consolidation pipeline as code.
- **Fabric primitives** — agent handoffs, cross-session memory and org
  accounts as the substrate for fleet-level context (see `ECOSYSTEM.md`).

The end state: an AI that sees only what matters, remembers what's relevant,
and reasons at maximum capacity — governed by policies you define.

**Tokens are the new gold. Context is the new infrastructure. Spend both wisely.**
