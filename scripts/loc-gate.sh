#!/usr/bin/env bash
# LOC gate (#660 Maintainability-Wave): no Rust source file may exceed
# LIMIT lines. Files split in Wave A stay small; legacy files still over
# the limit are frozen via the allowlist below and must not grow past
# FROZEN_LIMIT. Shrink the list as files get split (Wave B), never extend it.
set -euo pipefail

LIMIT=1500
FROZEN_LIMIT=2000

# Legacy files awaiting their split. Paths relative to repo root.
# R12: all 8 files split below 1500 LOC — allowlist is now empty.
ALLOWLIST=()

cd "$(dirname "$0")/.."

is_allowed() {
  local f=$1
  local a
  for a in "${ALLOWLIST[@]+"${ALLOWLIST[@]}"}"; do
    [[ "$f" == "$a" ]] && return 0
  done
  return 1
}

fail=0
while IFS= read -r file; do
  lines=$(wc -l <"$file" | tr -d ' ')
  if is_allowed "$file"; then
    if ((lines > FROZEN_LIMIT)); then
      echo "FAIL: $file has $lines lines (> frozen limit $FROZEN_LIMIT — split it, do not grow it)"
      fail=1
    fi
  elif ((lines > LIMIT)); then
    echo "FAIL: $file has $lines lines (> $LIMIT — split into submodules or, for legacy files only, allowlist in scripts/loc-gate.sh)"
    fail=1
  fi
done < <(find rust/src -name '*.rs' -type f)

# Ratchet: allowlisted files that dropped under LIMIT must leave the list.
for a in "${ALLOWLIST[@]+"${ALLOWLIST[@]}"}"; do
  if [[ -f "$a" ]]; then
    lines=$(wc -l <"$a" | tr -d ' ')
    if ((lines <= LIMIT)); then
      echo "FAIL: $a is now $lines lines (<= $LIMIT) — remove it from the allowlist in scripts/loc-gate.sh"
      fail=1
    fi
  else
    echo "FAIL: allowlisted file $a no longer exists — remove it from scripts/loc-gate.sh"
    fail=1
  fi
done

if ((fail == 0)); then
  n=${#ALLOWLIST[@]}
  echo "LOC gate OK: all non-allowlisted Rust files <= $LIMIT lines ($n legacy files frozen <= $FROZEN_LIMIT)"
fi
exit "$fail"
