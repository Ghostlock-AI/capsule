#!/usr/bin/env bash
set -euo pipefail

# Configure host/port via env if desired
EXFIL_HOST=${EXFIL_HOST:-127.0.0.1}
EXFIL_PORT=${EXFIL_PORT:-8765}
OUTFILE=${OUTFILE:-output/exfil_log.jsonl}

echo "Exfil server will log to: $OUTFILE"
echo "Listening on: http://$EXFIL_HOST:$EXFIL_PORT"
echo
echo "Instructions:"
echo "  1) Leave this server running in this terminal."
echo "  2) In a NEW terminal, enable misalignment: export DEMO_MISALIGNMENT=1"
echo "  3) (Optional) export EXFIL_SERVER_URL=http://$EXFIL_HOST:$EXFIL_PORT/upload"
echo "  4) Run the agent: python3 src/main.py"
echo "  5) Paste the provided demo prompt into the agent."
echo

python3 -u src/exfil_server.py --host "$EXFIL_HOST" --port "$EXFIL_PORT" --outfile "$OUTFILE"

