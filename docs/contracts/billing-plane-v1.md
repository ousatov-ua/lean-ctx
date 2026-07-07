# Contract: Billing Plane v1 (`billing-plane-v1`)

Status: **frozen baseline** · Plane: commercial (Team/Cloud) · Source: `rust/src/core/billing/`

> **Note (2026-07-07):** This document is the frozen v1 baseline. The current
> plan catalog has evolved: **Pro** now includes 1000 MB hosted index (not 0),
> and the **Business** plan ($149/mo flat, OIDC SSO) was added in
> [`billing-plane-v3`](billing-plane-v3.md). For the authoritative tier ladder
> and entitlements, see [`billing-plane-v1-catalog.json`](billing-plane-v1-catalog.json)
> (golden fixture, test-pinned) and [`docs/business/product-architecture.md`](../business/product-architecture.md).
> The six-plan ladder is: `free ⊂ supporter ⊂ pro ⊂ team ⊂ business ⊂ enterprise`.

The commercial-plane billing substrate (EPIC 13.6). It turns the existing
plan-upgrade flow into **real plans + entitlements** and **usage-based metering**
derived from the signed savings ledger — without ever touching the local
experience.

> Local-Free Invariant (RFC §4/§6): the Personal (local) plane is free, ungated,
> best-in-class — forever. Billing only describes/meters; it never gates local.

## Plans & entitlements

Five plans, strictly additive: `free` ⊂ `supporter` ⊂ `pro` ⊂ `team` ⊂ `enterprise`.

| Entitlement | free | supporter | pro | team | enterprise |
|-------------|------|-----------|-----|------|------------|
| seats | 1 | 1 | 1 | 25 | unlimited |
| hosted_index_mb | 0 (none) | 0 (none) | 0 (none) | 5000 | unlimited |
| managed_connectors | 0 (none) | 0 (none) | 0 (none) | 5 | unlimited |
| private_registry | no | no | no | yes | yes |
| sso_scim | no | no | no | no | yes |
| audit_retention_days | 0 | 0 | 0 | 90 | 3650 |
| revenue_share | no | no | no | yes | yes |
| supporter | no | yes | yes | yes | yes |
| cloud_sync | no | no | yes | yes | yes |

`supporter` is the **voluntary** Supporter subscription (`sponsor` is an accepted
alias for its top tier): an individual funds development and gets account-level
recognition (a supporter badge) and convenience perks. It is commercially
identical to `free` for every Team/Cloud capability — it can never gate a local
feature and grants none of the coordination entitlements; it only sets the
account-level `supporter` flag (also `true` for `pro`/`team`/`enterprise`, since
every paid plan is at minimum a supporter). Self-serve checkout for it never
triggers team-server provisioning (only `team` does).

`pro` is the **paid** "Personal Cloud" subscription — its **own** plan (`pro`
parses to `Plan::Pro`; it is no longer an alias of `supporter`). It adds exactly
one capability over `supporter` — `cloud_sync` — and nothing else: the same single
seat, none of the Team/Cloud coordination entitlements, so the ladder stays
additive (`supporter ⊂ pro ⊂ team`). `cloud_sync` is the hosted **Personal
Cloud**: cross-device sync + backup of the user's *own* context (knowledge, learned
shell patterns, CEP scores, gotchas, savings history) via the `/api/sync/*`
endpoints. It is a *hosted* service, **not** a local capability — the local engine
is fully usable without it. Like `team`, `pro` is account-bound self-serve
checkout; unlike `team` it provisions no team server.

A quota of `0` means **none**; the `UNBOUNDED` sentinel (`u32::MAX`) means
**unlimited / negotiated**. The two are never conflated (so Free's "no hosted
index" is never shown as "unlimited"). Every entitlement describes a Team/Cloud
capability; none can restrict a local feature.

### `entitlement_allows(plan, feature)`

- Any feature in `LOCAL_ALWAYS_ON_FEATURES` (or the local compile-optional set)
  returns `true` on **every** plan — the local plane is never gated.
- Commercial keys (`private_registry`, `sso_scim`, `revenue_share`,
  `supporter`, `cloud_sync`, `managed_connectors`, `hosted_index`,
  `audit_retention`) resolve from the plan's entitlements.
- Unknown features default to **allowed** (fail-open for the user — never
  fail-closed against the local experience).
- Self-hosting `team_server`/`cloud_server` stays free: those are compile-time
  capabilities, not entitlement keys. The commercial plane is the *hosted*
  version.

## Metering

`Usage` is derived **read-only** from `RoiReport` (EPIC 12.20), which is itself
derived from the Ed25519-`SignedSavingsBatchV1`. It carries only counts, sums,
and provenance hashes — never paths, prompts, or content.

```json
{
  "schema_version": 1,
  "period": "all",
  "created_at": "…",
  "agent_id": "…",
  "metered_events": 1234,
  "net_saved_tokens": 9876543,
  "saved_usd": 19.75,
  "last_entry_hash": "…",
  "chain_valid": true,
  "signed": true
}
```

- `is_billable()` = `signed && chain_valid`. Unsigned or broken chains are
  observable locally but are **not** billable (fail-closed for *billing* only,
  never for the user).
- Producing a usage record never mutates the ledger or the local experience.

## CLI surface (informational, never gating)

```bash
lean-ctx billing plans [--json]              # plan catalog + entitlements
lean-ctx billing entitlements <plan> [--json] # one plan's entitlements
lean-ctx billing usage [--json]              # metered usage from the local ledger
```

All three are read-only reporting. There are **no entitlement checks** in the
local binary; enforcement (checkout, plan gating) lives only on the hosted
control plane, the single place an account/plan is consulted.

## Production wiring (out of scope for the local engine)

Self-serve checkout and plan provisioning are a **hosted control-plane**
concern, built on the existing `cloud_server` backend (`rust/src/cloud_server/`)
and `lean-ctx upgrade` flow (`rust/src/cli/cloud.rs`):

1. A payment processor (e.g. Stripe) handles checkout + subscription lifecycle;
   its webhooks update the account's plan in the cloud Postgres
   (`cloud_server/db.rs`, `models.rs`).
2. The hosted `/v1` endpoint maps an authenticated account → `Plan`, then uses
   `entitlement_allows` to gate **hosted** capabilities only.
3. Usage is reported by clients submitting signed savings batches
   (`lean-ctx savings push`); the control plane aggregates `Usage` for
   usage-based billing.

The local engine never participates in (1)–(3); it only *describes* plans and
*reports* its own usage.

## Invariants (test-enforced)

1. `entitlement_allows` returns `true` for every local feature on every plan
   (`core::billing` unit tests + `tests/local_free_invariant.rs`).
2. Free grants no commercial entitlements; higher plans only add.
3. `Usage` is privacy-preserving (no path/prompt/content fields).
4. Only signed + intact chains are billable.

## Versioning

Adding a plan or entitlement field is additive (stays `v1`). Removing/renaming a
field, or changing the local-free semantics of `entitlement_allows`, bumps to
`billing-plane-v2`. The `pro` plan and the `cloud_sync` entitlement were added
under this rule — purely additive, so still `v1`. (`cloud_sync` gates only a
*hosted* sync service, never a local feature, so the local-free semantics are
unchanged.)

The metered **hosted-index storage-overage** add-on is documented separately in
[`billing-plane-v2`](billing-plane-v2.md): a new metered surface layered on top
of these plans, additive and Local-Free-preserving.

