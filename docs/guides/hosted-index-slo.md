# Hosted Index — SLO Gate & Operations Runbook

> GL #391 · The reliability gate that must hold before the hosted semantic
> index is sold at a premium price point. (Note: the "$29" figure referenced
> in GL #374 is a **planned future add-on price** for hosted-index storage,
> not yet mapped to any shipped plan. Current hosted-index quotas are included
> in Pro/Team/Business/Enterprise plans per `billing-plane-v1-catalog.json`.)

## The three objectives

| SLO | Metric | Target | Direction |
|---|---|---|---|
| Query latency | `team_query_p95_ms` | < 500 ms | max |
| Availability | `team_availability_pct` | ≥ 99.5 % | min |
| Index freshness | `team_index_lag_seconds` | < 300 s | max |

Definitions live in [`docs/examples/team-slos.toml`](../examples/team-slos.toml).
Copy that file to the team server host as `~/.lean-ctx/slos.toml` (or the
configured data dir) and the SLO engine evaluates it after every tool call.

## How the signals are measured

The team server instruments every `/v1/*` and `/api/v1/*` request in an
outermost middleware (`team_slo_middleware`), so the recorded latency matches
what clients observe — auth, rate limiting and handler time included.

- **Latency percentiles** — nearest-rank p50/p95/p99 over a rolling window of
  the last 4096 requests.
- **Availability** — share of requests in the window that did *not* return a
  5xx. Client errors (4xx: bad arguments, scope denials, rate limits) do not
  count against availability; they are caller-side failures.
- **Index freshness** — seconds since the last *successful* tool call that
  required the `Index` scope (e.g. `ctx_graph index-build*`,
  `ctx_semantic_search action=reindex`). This is a staleness indicator from
  the server's view. End-to-end push→query lag is measured externally by the
  control-plane probe (push marker → query marker → time delta), because only
  an outside observer can see the full pipeline.

## Reading the numbers

```bash
# JSON (runtime metrics + slo block)
curl -s -H "Authorization: Bearer $TOKEN" https://team-host:8484/v1/metrics | jq .slo

# Prometheus text exposition (for Datadog/Prometheus/Grafana scrape agents)
curl -s -H "Authorization: Bearer $TOKEN" "https://team-host:8484/v1/metrics?format=prometheus"
```

Exported series (all `leanctx_team_*`):

```
leanctx_team_request_duration_p50_ms
leanctx_team_request_duration_p95_ms
leanctx_team_request_duration_p99_ms
leanctx_team_availability_pct
leanctx_team_index_lag_seconds   (absent until the first index write)
leanctx_team_uptime_seconds
leanctx_team_requests_total
leanctx_team_errors_total
```

## Incident response

### p95 > 500 ms

1. Check `leanctx_team_requests_total` rate — saturation? Raise
   `max_concurrency` / `max_rps` in the team config only if host CPU < 70 %.
2. Check whether a large `index-build-full` ran concurrently (audit log:
   `ctx_graph` entries). Background builds are preferred:
   `action=index-build-background`.
3. Host-level: disk latency on the index volume is the most common culprit.

### Availability < 99.5 %

1. `journalctl`/server logs for panics or `tool_error` storms — note that
   tool errors are 4xx and do **not** lower availability; a real drop means
   5xx (timeouts → 504, internal failures → 500).
2. Timeouts (`request_timeout` / 504) count against availability. If they
   correlate with large workspaces, raise `request_timeout_ms` deliberately —
   do not mask capacity problems with longer timeouts.
3. Restart only after capturing `/v1/metrics` output; the rolling window
   resets on restart and you lose the evidence.

### Index lag > 5 min

1. Confirm pushes are arriving: audit log should show successful
   Index-scoped calls. No entries → client-side scheduling problem.
2. Entries present but failing → inspect the `tool_error` payloads;
   the freshness baseline only resets on *success*.
3. The control-plane probe alerts on end-to-end lag even when the
   server-side indicator looks healthy (e.g. queries served from a stale
   replica). Treat probe alerts as the source of truth.

## GA gate (pricing move)

The $29 hosted-index price move requires **30 consecutive days** with all
three SLOs green, measured by the control-plane probe (external) and
cross-checked against the server-side `/v1/metrics` series. Evidence is the
probe's monthly SLO report — keep it with the release notes of the GA tag.

