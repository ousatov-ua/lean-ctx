# lean-ctx Chrome Extension

**Token compression for web-based AI chat tools.** Automatically compresses code when pasting into ChatGPT, Claude.ai, Gemini, or GitHub Copilot Chat.

## Features

- **Auto-compress pastes** — Code pasted into AI chat inputs is automatically compressed
- **Token counter badge** — Shows savings in real-time after each compression
- **Native messaging** — Connects to local lean-ctx binary for full compression (95+ patterns)
- **Fallback compression** — Basic comment/whitespace removal when native host unavailable
- **Popup dashboard** — Token savings stats + toggle controls

## Supported Sites

- [ChatGPT](https://chatgpt.com) / [chat.openai.com](https://chat.openai.com)
- [Claude.ai](https://claude.ai)
- [Gemini](https://gemini.google.com)
- [GitHub Copilot Chat](https://github.com/copilot)
- [Lovable](https://lovable.dev)
- [Bolt.new](https://bolt.new)
- [v0.dev](https://v0.dev)
- [Poe](https://poe.com)
- [Google AI Studio](https://aistudio.google.com)
- [Perplexity Labs](https://labs.perplexity.ai)

## Installation

### 1. Load Extension (Developer Mode)

1. Open `chrome://extensions`
2. Enable "Developer mode"
3. Click "Load unpacked" and select this directory
4. Note the Extension ID

### 2. Install Native Messaging Host (optional, for full compression)

```bash
cd native-host
chmod +x install.sh bridge.sh
./install.sh
```

Then edit the native messaging manifest to include your Extension ID.

### 3. Requirements

- [lean-ctx](https://leanctx.com) binary for native messaging (`cargo install lean-ctx`)
- Python 3 for the native messaging bridge

## How It Works

1. When you paste text (>200 chars) into a supported AI chat input
2. The extension intercepts the paste event
3. Text is sent to the background service worker
4. If native messaging is available, lean-ctx compresses it
5. Otherwise, fallback compression removes comments and whitespace
6. Compressed text replaces the paste, badge shows savings

## Settings

Toggle via the popup (click extension icon):

- **Auto-compress pastes** — Enable/disable automatic compression
- **Native messaging** — Use lean-ctx binary for advanced compression

## Enterprise-Managed Deployment

The extension supports Chrome Enterprise policies via `chrome.storage.managed`
(schema: `managed_schema.json`). Managed values override user choices; the
popup shows a "Managed by your organization" banner and locks those controls.

Available policy keys:

| Key | Type | Effect |
|---|---|---|
| `enabled` | boolean | Force compress-on-send on/off fleet-wide |
| `autoCompressPaste` | boolean | Force clipboard auto-compress on/off |
| `threshold` | integer | Minimum token estimate before compressing |
| `gatewayBaseUrl` | string | Org AI-gateway endpoint, surfaced in the popup for base-URL-capable tools |

Example policy (macOS: managed preferences plist, Windows: registry under
`Software\Policies\Google\Chrome\3rdparty\extensions\<extension-id>\policy`,
Linux: `/etc/opt/chrome/policies/managed/*.json`):

```json
{
  "3rdparty": {
    "extensions": {
      "<extension-id>": {
        "enabled": true,
        "autoCompressPaste": true,
        "gatewayBaseUrl": "https://ai-gateway.example.com"
      }
    }
  }
}
```

### Honest boundary — what the extension can and cannot do

- **Can**: compress prompts you type/paste into supported web chats
  (client-side token reduction), and display your org's gateway endpoint so
  engineers configure their CLI/IDE tools correctly.
- **Cannot**: re-route first-party AI web/desktop apps (claude.ai, ChatGPT
  web, Claude Desktop) through your gateway. Manifest V3 grants no
  `webRequest`/`declarativeNetRequest` here, and those apps pin their own
  backends. Org-side metering/routing covers only traffic that reaches the
  gateway via a base URL: Claude Code CLI, Cursor, IDE API clients, SDKs.
  `gatewayBaseUrl` is informational for the user — not a traffic redirect.
