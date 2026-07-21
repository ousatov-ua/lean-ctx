#!/usr/bin/env bash
set -euo pipefail

WORKTREE_BASE="/tmp/lean-ctx-tickets"
LOG_DIR="/tmp/codex-ticket-logs"
REPO="/Users/yvesgugger/Documents/Privat/Projects/lean-ctx"

TICKETS=(1101 1105 1110 1117 1122 1123 1124 1125 1135 1142 1143 1147)

printf "\n=== Ticket Agent Monitor === %s\n\n" "$(date '+%H:%M:%S')"
printf "%-6s %-8s %-6s %-6s %-7s %s\n" "Agent" "Status" "+LOC" "Files" "Ticket" "Last Commit"
printf "%-6s %-8s %-6s %-6s %-7s %s\n" "-----" "------" "----" "-----" "------" "-----------"

DONE=0
WORK=0
IDLE=0
FAIL=0

for i in "${!TICKETS[@]}"; do
  NUM=$(printf "%02d" $((i + 1)))
  TICKET="${TICKETS[$i]}"
  WTDIR="$WORKTREE_BASE/agent-${NUM}"
  LOG="$LOG_DIR/agent-${NUM}.log"
  BRANCH="ticket/agent-${NUM}-issue-${TICKET}"

  if [ ! -d "$WTDIR" ]; then
    printf "%-6s %-8s\n" "$NUM" "MISSING"
    continue
  fi

  cd "$WTDIR"

  COMMIT_MSG=$(git log -1 --format="%s" 2>/dev/null || echo "")
  CHANGES=$(git diff --shortstat 2>/dev/null || echo "")
  UNTRACKED=$(git ls-files --others --exclude-standard 2>/dev/null | wc -l | tr -d ' ')

  # Check if agent has committed on its branch
  HAS_COMMIT=$(git log main.."$BRANCH" --oneline 2>/dev/null | head -1)

  # Check if log contains error/panic
  HAS_ERROR=""
  if [ -f "$LOG" ]; then
    HAS_ERROR=$(grep -l "panic\|FATAL\|Error:" "$LOG" 2>/dev/null || true)
  fi

  if [ -n "$HAS_COMMIT" ]; then
    STATUS="DONE"
    DONE=$((DONE + 1))
    ADDED_LINES=$(git diff --numstat main.."$BRANCH" 2>/dev/null | awk '{sum+=$1}END{print sum+0}')
    CHANGED_FILES=$(git diff --name-only main.."$BRANCH" 2>/dev/null | wc -l | tr -d ' ')
  elif [ -n "$HAS_ERROR" ]; then
    STATUS="FAIL"
    FAIL=$((FAIL + 1))
    ADDED_LINES="0"
    CHANGED_FILES="0"
  elif [ -n "$CHANGES" ] || [ "$UNTRACKED" -gt 0 ]; then
    STATUS="WORK"
    WORK=$((WORK + 1))
    ADDED_LINES=$(git diff --numstat 2>/dev/null | awk '{sum+=$1}END{print sum+0}')
    CHANGED_FILES=$(git diff --name-only 2>/dev/null | wc -l | tr -d ' ')
  else
    STATUS="IDLE"
    IDLE=$((IDLE + 1))
    ADDED_LINES="0"
    CHANGED_FILES="0"
  fi

  printf "%-6s %-8s %-6s %-6s %-7s %s\n" "$NUM" "$STATUS" "+$ADDED_LINES" "$CHANGED_FILES" "#$TICKET" "${COMMIT_MSG:0:50}"
done

cd "$REPO"
echo ""
echo "Summary: $DONE DONE, $WORK WORK, $IDLE IDLE, $FAIL FAIL"
echo ""

if [ $DONE -gt 0 ]; then
  echo "Completed agents — merge candidates:"
  for i in "${!TICKETS[@]}"; do
    NUM=$(printf "%02d" $((i + 1)))
    TICKET="${TICKETS[$i]}"
    WTDIR="$WORKTREE_BASE/agent-${NUM}"
    BRANCH="ticket/agent-${NUM}-issue-${TICKET}"
    [ -d "$WTDIR" ] || continue
    cd "$WTDIR"
    HAS_COMMIT=$(git log main.."$BRANCH" --oneline 2>/dev/null | head -1)
    if [ -n "$HAS_COMMIT" ]; then
      echo "  git merge $BRANCH  # agent-${NUM} → #${TICKET}"
    fi
    cd "$REPO"
  done
fi

echo ""
echo "Quality check (after all DONE):"
echo "  cd $REPO/rust && cargo test --lib 2>&1 | tail -5"
