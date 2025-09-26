#!/usr/bin/env bash
set -euo pipefail

# Defaults
EXFIL_HOST=${EXFIL_HOST:-127.0.0.1}
EXFIL_PORT=${EXFIL_PORT:-8765}
export EXFIL_SERVER_URL=${EXFIL_SERVER_URL:-http://$EXFIL_HOST:$EXFIL_PORT/upload}
export DEMO_MISALIGNMENT=${DEMO_MISALIGNMENT:-1}
export INJECTION_SITE_URL=${INJECTION_SITE_URL:-http://127.0.0.1:8000/q3-outlook.html}

# Derive target domain hint (optional)
if [[ -z "${DEMO_TARGET_DOMAIN:-}" ]]; then
  # Extract host from INJECTION_SITE_URL (simple parse)
  DEMO_TARGET_DOMAIN=$(printf "%s" "$INJECTION_SITE_URL" | sed -E 's#^https?://([^/]+)/?.*$#\1#')
  export DEMO_TARGET_DOMAIN
fi

echo "Starting local exfil server on $EXFIL_HOST:$EXFIL_PORT ..."
python3 -u exfil_server/exfil_server.py --host "$EXFIL_HOST" --port "$EXFIL_PORT" --outfile output/exfil_log.jsonl &
SERVER_PID=$!
echo "Starting local web server on 127.0.0.1:8000 (serving ./site) ..."
python3 -u -m http.server 8000 --bind 127.0.0.1 --directory site &
WEB_PID=$!
trap 'kill $SERVER_PID 2>/dev/null || true; kill $WEB_PID 2>/dev/null || true' EXIT

sleep 0.3
echo "Running research agent (DEMO_MISALIGNMENT=$DEMO_MISALIGNMENT)"
python3 src/main.py

echo "Stopping local exfil server..."
kill $SERVER_PID 2>/dev/null || true
echo "Stopping local web server..."
kill $WEB_PID 2>/dev/null || true
