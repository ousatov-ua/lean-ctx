#!/usr/bin/env bash
set -euo pipefail

COMBINED_DIR="/tmp/codex-combined-tickets"
WORKTREE_BASE="/tmp/lean-ctx-tickets"
LOG_DIR="/tmp/codex-ticket-logs"
mkdir -p "$LOG_DIR"

TICKETS=(1101 1105 1110 1117 1122 1123 1124 1125 1135 1142 1143 1147)

echo "=== lean-ctx Ticket Agents ==="
echo "Tickets: ${#TICKETS[@]}"
echo "Worktrees: $WORKTREE_BASE"
echo "Logs: $LOG_DIR"
echo ""

STARTED=0

for i in "${!TICKETS[@]}"; do
  NUM=$(printf "%02d" $((i + 1)))
  TICKET="${TICKETS[$i]}"
  COMBINED="$COMBINED_DIR/agent-${NUM}.md"
  WTDIR="$WORKTREE_BASE/agent-${NUM}"
  LOG="$LOG_DIR/agent-${NUM}.log"

  if [ ! -f "$COMBINED" ]; then
    echo "SKIP agent-${NUM}: no goal file"
    continue
  fi
  if [ ! -d "$WTDIR" ]; then
    echo "SKIP agent-${NUM}: no worktree at $WTDIR"
    continue
  fi

  echo "Starting agent-${NUM} (#${TICKET}) in $WTDIR ..."

  osascript -e "
    tell application \"Terminal\"
      do script \"cd $WTDIR && lean-ctx agent register --id ticket-${NUM}-\$\$ --role developer --owner yves@lean-ctx 2>/dev/null; cat $COMBINED | codex exec -s workspace-write - 2>&1 | tee $LOG\"
    end tell
  "

  STARTED=$((STARTED + 1))
  sleep 2
done

echo ""
echo "=== $STARTED / ${#TICKETS[@]} Agents gestartet ==="
echo ""
echo "Monitor:  bash scripts/monitor-agents-tickets.sh"
echo "Logs:     ls $LOG_DIR/"
echo "Bus:      lean-ctx agent list"
