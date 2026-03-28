#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "wave-watch.sh requires jq on PATH" >&2
  exit 64
fi

TARGET="${1:-ready}"
INTERVAL="${WAVE_WATCH_INTERVAL:-5}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

while true; do
  set +e
  OUTPUT="$("$SCRIPT_DIR/wave-status.sh")"
  STATUS=$?
  set -e

  QUEUE_STATE="$(printf '%s\n' "$OUTPUT" | jq -r '.queue_state')"
  SOFT_STATE="$(printf '%s\n' "$OUTPUT" | jq -r '.delivery_soft_state')"

  case "$TARGET" in
    ready)
      if [[ "$QUEUE_STATE" == "ready" || "$QUEUE_STATE" == "completed" ]]; then
        printf '%s\n' "$OUTPUT"
        exit "$STATUS"
      fi
      ;;
    settled)
      if [[ "$QUEUE_STATE" != "active" ]]; then
        printf '%s\n' "$OUTPUT"
        exit "$STATUS"
      fi
      ;;
    clear)
      if [[ "$QUEUE_STATE" != "active" && "$SOFT_STATE" == "clear" ]]; then
        printf '%s\n' "$OUTPUT"
        exit "$STATUS"
      fi
      ;;
    blocked)
      if [[ "$QUEUE_STATE" == "blocked" ]]; then
        printf '%s\n' "$OUTPUT"
        exit "$STATUS"
      fi
      ;;
    *)
      echo "usage: wave-watch.sh [ready|settled|clear|blocked]" >&2
      exit 64
      ;;
  esac

  sleep "$INTERVAL"
done
