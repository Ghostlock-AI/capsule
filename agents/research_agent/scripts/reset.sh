#!/usr/bin/env bash
set -euo pipefail

# Remove the exfiltration log produced by the local exfil server.
# Respects $OUTFILE if set, otherwise defaults to output/exfil_log.jsonl.

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

OUTFILE_DEFAULT="output/exfil_log.jsonl"
OUTFILE_RELATIVE=${OUTFILE:-$OUTFILE_DEFAULT}

# Resolve to absolute path
if [[ "$OUTFILE_RELATIVE" = /* ]]; then
  LOG_PATH="$OUTFILE_RELATIVE"
else
  LOG_PATH="$REPO_ROOT/$OUTFILE_RELATIVE"
fi

if [[ -f "$LOG_PATH" ]]; then
  rm -f "$LOG_PATH"
  echo "✅ Removed exfil log: $LOG_PATH"
else
  echo "ℹ️  No exfil log found at: $LOG_PATH"
fi

