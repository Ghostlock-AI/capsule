#!/usr/bin/env bash
set -euo pipefail

# Minimal launcher for the local exfiltration server.
# Env:
#   EXFIL_HOST (default 127.0.0.1)
#   EXFIL_PORT (default 8765)
#   OUTFILE    (default output/exfil_log.jsonl)

# Resolve repo root relative to this script (scripts/..)
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

# Ensure virtual environment exists, create if missing
if [[ ! -d "$REPO_ROOT/.venv" ]]; then
  echo "Creating virtual environment at $REPO_ROOT/.venv ..."
  python3 -m venv "$REPO_ROOT/.venv"
fi

# Activate venv
source "$REPO_ROOT/.venv/bin/activate"

EXFIL_HOST=${EXFIL_HOST:-127.0.0.1}
EXFIL_PORT=${EXFIL_PORT:-8765}
OUTFILE=${OUTFILE:-output/exfil_log.jsonl}

echo "Exfil server will log to: $OUTFILE"
echo "Listening on: http://$EXFIL_HOST:$EXFIL_PORT"
echo

python3 -u exfil_server/exfil_server.py \
  --host "$EXFIL_HOST" \
  --port "$EXFIL_PORT" \
  --outfile "$OUTFILE"
