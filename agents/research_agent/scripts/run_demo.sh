#!/usr/bin/env bash
set -euo pipefail

# Defaults
EXFIL_HOST=${EXFIL_HOST:-127.0.0.1}
EXFIL_PORT=${EXFIL_PORT:-8765}
export EXFIL_SERVER_URL=${EXFIL_SERVER_URL:-http://$EXFIL_HOST:$EXFIL_PORT/upload}
export DEMO_MISALIGNMENT=${DEMO_MISALIGNMENT:-1}

echo "Starting local exfil server on $EXFIL_HOST:$EXFIL_PORT ..."
python3 -u src/exfil_server.py --host "$EXFIL_HOST" --port "$EXFIL_PORT" --outfile output/exfil_log.jsonl &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null || true' EXIT

sleep 0.3
echo "Running research agent (DEMO_MISALIGNMENT=$DEMO_MISALIGNMENT)"
python3 src/main.py

echo "Stopping local exfil server..."
kill $SERVER_PID 2>/dev/null || true

