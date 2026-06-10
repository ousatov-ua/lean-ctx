# Device Overview v1 (GL #387)

Per-account list of machines that sync to the Personal Cloud, keyed by the
client's hostname. Pure display metadata: a device row never participates in
authentication, entitlements, quota, or billing.

## Client behavior

Every authenticated sync push attaches one header:

```
X-Device-Label: <hostname>
```

- Source: `gethostname()` on the pushing machine (`cloud_client::device_label`).
- Attached to: `POST /api/stats`, `POST /api/sync/{knowledge,commands,cep,gotchas,buddy,feedback,gain}`, `PUT /api/sync/index/{project_hash}`.
- An empty or missing header is valid — the push succeeds, it is just not
  tracked.

## Server behavior

`devices` table, one row per `(user_id, device_label)`:

| column | meaning |
|---|---|
| `first_seen` | first push carrying this label |
| `last_seen` | most recent push |
| `last_surface` | which sync surface pushed last (`stats`, `knowledge`, …) |
| `sync_count` | total tracked pushes |

Tracking is fire-and-forget (`devices::track`, spawned task): a database
failure can never fail or slow the sync request itself. Labels are sanitized
server-side (trimmed, control characters rejected, capped at 64 chars);
unusable labels are silently skipped.

## API

### GET /api/account/devices

Auth: account bearer. Returns up to 50 devices, most recently active first.

```json
{
  "devices": [
    {
      "label": "mbp-yves",
      "first_seen": "2026-06-01T09:14:00+00:00",
      "last_seen": "2026-06-10T07:02:11+00:00",
      "last_surface": "knowledge",
      "sync_count": 184
    }
  ]
}
```

### DELETE /api/account/devices/{label}

Auth: account bearer. Forgets one row; idempotent (`{"forgotten": false}` for
an unknown label, still HTTP 200 — the end state is identical). Forgetting a
device deletes nothing else and signs nothing out; the row simply reappears on
that machine's next sync.

`400 bad_label` for labels that fail sanitation (empty / control chars).

## UI

`/account/cloud` renders the Devices card below usage: hostname, relative
last-sync time, last surface, push count, and a Forget button per row. The
card never blocks the dashboard — fetch errors leave it hidden.

## Privacy

- Hostnames stay within the account that pushed them; no cross-account
  surface, no analytics use.
- The label is the only machine identifier collected — no MACs, IPs, serials,
  or fingerprints.
- `DELETE /api/account/devices/{label}` is the user-facing forget control;
  account deletion cascades the whole table (`ON DELETE CASCADE`).
