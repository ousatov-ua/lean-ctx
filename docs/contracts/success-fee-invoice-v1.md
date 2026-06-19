# Contract: Success-Fee Invoice v1 (`success-fee-invoice-v1`)

Status: stable · Plane: commercial (outcome-based pricing) · Mode: **Stripe TEST only**
Source: `rust/src/core/billing/success_fee.rs`, `rust/src/core/billing/stripe_invoice.rs`,
CLI `rust/src/cli/dispatch/analytics/billing.rs` (`lean-ctx billing invoice`)
Epic: [Outcome-Based Pricing #671] · builds on [`billing-plane-v2`](billing-plane-v2.md)

Turns a **verified** savings figure into a Stripe **invoice item** for the agreed
enterprise success fee. The data source is the Ed25519-signed savings batch via
[`Usage`](billing-plane-v2.md) (`is_billable = signed && chain_valid`); the
billing plane never meters raw activity and never gates the local plane.

> Local-Free Invariant: unchanged. This is a *hosted/commercial* action driven by
> the operator; it reads the local signed aggregate read-only and gates nothing.

## Fee formula

```text
base = fixed_floor + take_rate * (saved_usd * cache_haircut)
cap  = invoice_cap_pct * provider_delta_usd
fee  = min(base, cap)
amount_cents = round(max(fee, 0) * 100)
```

- **`saved_usd`** — verified savings for the period, from the signed batch.
- **`cache_haircut`** — discounts savings that came from cache hits (cheaper than
  fresh reads) before the take rate is applied.
- **`provider_delta_usd`** — the customer's *actual* provider-bill change for the
  period. Only the customer knows it, so it is a **required CLI input**
  (`--provider-delta-usd`); the cap guarantees the fee never exceeds an agreed
  fraction of real spend (the over-billing guard).

## Configuration (no defaults)

Section `[success_fee]` in `config.toml`. The four fee terms are commercial
inputs with **no defaults** — `lean-ctx` never invents a price; the invoice
command fails closed naming every missing key.

| Key | Type | Meaning |
|-----|------|---------|
| `success_fee.take_rate` | f64 `0..=1` | share of haircut-adjusted savings |
| `success_fee.fixed_floor` | f64 `>=0` | fixed USD component (before cap) |
| `success_fee.cache_haircut` | f64 `0..=1` | cache-savings discount multiplier |
| `success_fee.invoice_cap_pct` | f64 `0..=1` | cap as fraction of provider delta |
| `success_fee.currency` | string? | invoice currency (default `usd` at invoice time) |
| `success_fee.stripe_customer` | string? | default `cus_…` (overridable via `--customer`) |

```bash
lean-ctx config set success_fee.take_rate 0.2
lean-ctx config set success_fee.fixed_floor 60000
lean-ctx config set success_fee.cache_haircut 0.8
lean-ctx config set success_fee.invoice_cap_pct 0.5
```

## Stripe key & test-mode enforcement

The Stripe secret is **never stored in config**; it is read from the environment
(`STRIPE_API_KEY`, or `LEAN_CTX_STRIPE_API_KEY`). A
[restricted key](https://docs.stripe.com/keys/restricted-api-keys) (`rk_test_…`)
scoped to write Invoices + Invoice Items is recommended.

This command operates in **Stripe TEST mode only**: a key that is not
`sk_test_…` / `rk_test_…` is **refused** (fail-closed), so a misconfiguration can
never raise a real charge from this path. Pinned API version: `2026-05-27.dahlia`.

## Idempotency

Every write carries an `Idempotency-Key` derived from `(customer, period)`
(`leanctx-fee-<customer>-<period>`, sanitized). Re-running the same period is a
no-op on Stripe's side instead of double-billing — the period's fee is locked
once created. The ledger head, the raw inputs and the full breakdown are attached
as invoice-item **metadata** for audit (they do not affect idempotency).

## Fail-closed for billing (never for the user)

- Unsigned or broken chain → **no invoice** (`exit 2`), printed as a billing
  refusal. The local experience is never affected.
- Any fee term missing / out of range → **no invoice** (`exit 1`), every
  offending key named.
- Computed fee rounds to `$0.00` → **no invoice** (nothing to bill).
- `--dry-run` computes + prints the full breakdown and makes **no** Stripe call
  (no key required), for review and CI.

## CLI

```bash
# Preview the fee (no Stripe call):
lean-ctx billing invoice --provider-delta-usd=200000 --dry-run

# Create the invoice item on a customer (TEST key in env):
STRIPE_API_KEY=rk_test_… lean-ctx billing invoice \
  --provider-delta-usd=200000 --customer=cus_123 --period=2026-06

# Also draft (and with --finalize, advance) an invoice pulling the pending item:
… lean-ctx billing invoice … --create-invoice --finalize
```

`--json` emits the machine payload (billable flag, breakdown, idempotency key,
created Stripe object) for automation.

## Invariants (test-enforced)

1. Fee math: base formula, cap binding, zero/negative inputs, cents rounding
   (`core/billing/success_fee.rs` tests).
2. No defaults: missing terms fail closed and name every key.
3. Test-mode guard: only `sk_test_`/`rk_test_` keys accepted
   (`core/billing/stripe_invoice.rs` tests).
4. Privacy: only the signed aggregate is read; no path/prompt/content leaves the
   machine (inherited from `Usage` / `billing-plane-v2`).

## Versioning

Additive changes (extra metadata, optional flags) keep v1. Changing the formula,
the idempotency derivation, or the test-mode enforcement requires a new major
contract version.
