#!/usr/bin/env bash
# lean-ctx auto-updater — macOS / Linux
# Checks GitHub API first; only calls `lean-ctx update` when a newer
# version exists, avoiding unnecessary daemon restarts.
#
# Install (macOS): see install-macos.sh
# Install (Linux): add to crontab — `0 */6 * * * bash ~/.lean-ctx/autoupdate.sh`

LEAN_CTX=$(command -v lean-ctx 2>/dev/null) || { echo "lean-ctx not in PATH"; exit 1; }
LOG="$HOME/.lean-ctx/autoupdate.log"
API="https://api.github.com/repos/yvgude/lean-ctx/releases/latest"

log()    { printf '[%s] %s\n' "$(date '+%F %T')" "$1" >> "$LOG"; }
notify() {
    case "$(uname)" in
        Darwin) osascript -e "display notification \"$1\" with title \"lean-ctx\" sound name \"Glass\"" 2>/dev/null ;;
        Linux)  command -v notify-send &>/dev/null && notify-send "lean-ctx" "$1" ;;
    esac
}
ver() { "$LEAN_CTX" status --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['version'])" 2>/dev/null; }

# Rotate log at 500 lines
[[ -f "$LOG" ]] && (( $(wc -l < "$LOG") > 500 )) && tail -500 "$LOG" > "$LOG.tmp" && mv "$LOG.tmp" "$LOG"

CURRENT=$(ver)
LATEST=$(curl -sf --max-time 10 "$API" | python3 -c "import sys,json; print(json.load(sys.stdin)['tag_name'].lstrip('v'))" 2>/dev/null)

[[ -z "$CURRENT" || -z "$LATEST" ]] && { log "WARN: version check failed (current='$CURRENT' latest='$LATEST')"; exit 0; }
log "current=v$CURRENT latest=v$LATEST"
[[ "$CURRENT" == "$LATEST" ]] && exit 0

log "Updating v$CURRENT → v$LATEST"
"$LEAN_CTX" update >> "$LOG" 2>&1 || { log "ERROR: lean-ctx update failed"; notify "Update failed — check $LOG"; exit 1; }

NEW=$(ver)
SAVINGS=$("$LEAN_CTX" stats 2>/dev/null | grep -oE 'Saved:.*' | head -1)
notify "v$CURRENT → v$NEW${SAVINGS:+ · $SAVINGS} · Restart IDE to reconnect MCP"
log "Done: v$NEW${SAVINGS:+ · $SAVINGS}"
