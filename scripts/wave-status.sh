#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "wave-status.sh requires jq on PATH" >&2
  exit 64
fi

ROOT="${WAVE_REPO_ROOT:-$(pwd)}"
JSON="$(cargo run -q -p wave-cli --manifest-path "$ROOT/Cargo.toml" -- control status --json)"
SIGNAL="$(printf '%s\n' "$JSON" | jq -c '.control_status.signal')"

printf '%s\n' "$SIGNAL"
exit "$(printf '%s\n' "$SIGNAL" | jq -r '.exit_code')"
