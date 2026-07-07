# Hosted Personal Index — Contract v1 (GL #392)

Pro ("Personal Cloud") accounts get a hosted copy of their **own** semantic
index: the locally built BM25 + embedding artifacts, pushed as one encrypted
bundle per project and pullable from any logged-in device. Cross-device
retrieval without a local re-index — and never a local capability gate
(`tests/local_free_invariant.rs` stays green; hosted is purely additive).

## Privacy decision (binding for v1)

**Client-side encryption, end to end.** The server stores an opaque blob.

- Bundles are encrypted on the device with **XChaCha20-Poly1305**.
- The 32-byte key is derived per account via **HKDF-SHA256** from the
  account's API key: `HKDF(ikm = api_key, salt = "leanctx", info =
  "index-bundle-v1")`.
- The community backend stores API keys **only as SHA-256 hashes**
  (`api_keys.api_key_sha256`), so the server can authenticate the caller but
  can never derive the encryption key. Operators see ciphertext only.
- Zero-content logging: handlers log sizes and project hashes, never payloads.

Consequences, deliberately accepted:

- Every logged-in device derives the same key with **zero extra setup** —
  the pull-on-new-device flow needs nothing beyond `lean-ctx login`.
- Rotating the API key makes old bundles undecryptable. That is fine: the
  bundle is a *cache* of state that is always rebuildable locally — the fix
  is one `lean-ctx sync index push`.

## Bundle format (`LCIB1`)

```
"LCIB1\n" | u32 LE manifest_len | manifest JSON | zstd(files payload)
```

The manifest lists `{name, size, sha256}` per file plus `project_hash`,
`created_at` and the engine version. v1 carries the two retrieval artifacts
from the project's vector namespace (`vectors/{namespace_hash}/`):

| File | Content |
|---|---|
| `bm25_index.bin.zst` | BM25 chunk index (already zstd) |
| `embeddings.json` | embedding vectors + chunk metadata |

The whole container is encrypted (24-byte random nonce prepended) before
upload. Decrypt → verify per-file SHA-256 → write atomically into the local
namespace. HNSW is rebuilt lazily from the embeddings on first search (no
serialization needed in v1).

## API (community backend, `/api/sync` family)

| Method | Path | Notes |
|---|---|---|
| `PUT` | `/api/sync/index/{project_hash}` | body = encrypted bundle (`application/octet-stream`, ≤ 64 MB per bundle) |
| `GET` | `/api/sync/index/{project_hash}` | returns the encrypted bundle |
| `GET` | `/api/sync/index` | per-project listing + quota usage |
| `DELETE` | `/api/sync/index/{project_hash}` | frees the bucket |

Auth: same bearer auth as every `/api/sync/*` route, gated by
`require_cloud_sync` (Pro/Team/Enterprise; open deployments stay open —
Local-Free Invariant).

## Quota (display-first, never billed)

The account-wide cap is `entitlements.hosted_index_mb` (Pro: **1000 MB**,
Team: 5000 MB, Business: 20000 MB, Enterprise: unbounded; open deployments
without a billing plane: 1000 MB default). A push that would exceed the cap returns
`413 quota_exceeded` with current usage in the body — it **warns and blocks,
it never bills** (consistent with the billing-plane-v2 display-first rollout).

The listing (and the `413` body) carries a `storage` block whose threshold
semantics mirror billing-plane-v2's `StorageMetering` exactly — one story on
every surface:

```json
"storage": {
  "used_bytes": 612000000,
  "quota_bytes": 1000000000,
  "overage_bytes": 0,
  "percent": 61.2,
  "state": "warn"
}
```

`state`: `none` (no entitlement) | `ok` | `warn` (≥ 50 %) | `critical`
(≥ 80 %) | `over` (≥ 100 %). `lean-ctx sync index status` renders this as a
coloured state line so users see headroom before a push bounces.

## Background auto-push (opt-in)

`lean-ctx cloud autoindex on` sets `[cloud] auto_index`; the daily background
task then pushes the project's bundle at most once per project per day
(per-project debounce in `[cloud] last_index_push`). Separate flag from
`autosync` because bundles are megabytes, not kilobytes. Pro-gate and quota
rejections consume the day's slot (one quiet attempt, no error spam); network
failures leave it open for the next cycle.

## Consistency

One bundle per `(account, project_hash)`, last-writer-wins; the server keeps
`updated_at` + `sha256` so clients can skip no-op pushes and detect drift
(`lean-ctx sync index status`). Conflicts are impossible by construction —
the bundle is device-generated derived state, not a merged document.

