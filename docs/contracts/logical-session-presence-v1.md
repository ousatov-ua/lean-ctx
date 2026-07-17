# Logical Session Presence Contract v1

Status: additive local-dashboard contract.

## Purpose

MCP transport processes and editor/chat sessions are different resources. A single
long-lived MCP process may multiplex several logical sessions. lean-ctx therefore
never derives session presence from tool calls, transcripts, file mtimes, or a
transport PID.

## Producer requirements

An integration that owns the editor session lifecycle sends:

```http
POST /api/agents/sessions
Content-Type: application/json

{
  "event": "open",
  "source": "vscode",
  "workspace": "/absolute/workspace",
  "session_id": "editor-owned-stable-id"
}
```

`event` is one of `open`, `heartbeat`, or `close`.

- Send `open` when the logical session becomes live.
- Send `heartbeat` at least once every 60 seconds while it remains live.
- Send `close` when it closes.
- Identity key is `(source, workspace, session_id)`.
- `open` and `heartbeat` are idempotent upserts.
- A heartbeat preserves `opened_at`.
- Presence expires 180 seconds after the last heartbeat, even if tool activity
  continues.

Fields must be non-empty, contain no control characters, and stay within these
UTF-8 byte limits: source 64, workspace 4096, session ID 256.

## Consumer response

`GET /api/agents` exposes two independent collections:

- `transports` and `transport_count`: live registered MCP processes.
- `logical_sessions` and `logical_session_count`: explicit logical presence.

`logical_session_count` is `null` and
`logical_session_presence_available` is `false` until an integration has sent
presence telemetry. This distinguishes “zero live sessions” from “session
lifecycle unavailable.”

Legacy `agents` and `total_active` fields remain aliases for transports.
Recent tool activity is reported separately and never changes either presence
count.
