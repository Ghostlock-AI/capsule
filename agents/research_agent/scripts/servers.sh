#!/usr/bin/env bash
set -euo pipefail

# Run both the exfil server and inject server in a single terminal
# Logs are interleaved and both servers shut down together with Ctrl+C

# Resolve repo root relative to this script (scripts/..)
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

# Activate venv if present
if [[ -d "$REPO_ROOT/.venv" ]]; then
  source "$REPO_ROOT/.venv/bin/activate"
fi

# Configuration
EXFIL_HOST=${EXFIL_HOST:-127.0.0.1}
EXFIL_PORT=${EXFIL_PORT:-8765}
OUTFILE=${OUTFILE:-output/exfil_log.jsonl}

WEB_DIR=${WEB_DIR:-site}
WEB_HOST=${WEB_HOST:-127.0.0.1}
WEB_PORT=${WEB_PORT:-8080}

# Ensure output directory exists
mkdir -p "$(dirname "$OUTFILE")"

echo "=========================================="
echo "Starting Prompt Injection Demo Servers"
echo "=========================================="
echo ""
echo "📡 Exfil Server: http://$EXFIL_HOST:$EXFIL_PORT"
echo "   Logging to: $OUTFILE"
echo ""
echo "🌐 Inject Server: http://$WEB_HOST:$WEB_PORT"
echo "   Serving: $WEB_DIR/"
echo ""
echo "Press Ctrl+C to stop both servers"
echo "=========================================="
echo ""

# Trap to kill all background jobs on exit
trap 'echo ""; echo "Shutting down servers..."; kill $(jobs -p) 2>/dev/null; wait; echo "✅ All servers stopped"; exit' SIGINT SIGTERM EXIT

# Start exfil server in background with prefixed output
python3 -u exfil_server/exfil_server.py \
  --host "$EXFIL_HOST" \
  --port "$EXFIL_PORT" \
  --outfile "$OUTFILE" 2>&1 | sed 's/^/[EXFIL] /' &

# Give exfil server a moment to start
sleep 0.5

# Start inject server in background with prefixed output
python3 - << 'PY' 2>&1 | sed 's/^/[INJECT] /' &
import os
import sys
from datetime import datetime
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler

WEB_DIR = os.environ.get('WEB_DIR', 'site')
WEB_HOST = os.environ.get('WEB_HOST', '127.0.0.1')
WEB_PORT = int(os.environ.get('WEB_PORT', '8080'))

GREEN = "\033[32m"
YELLOW = "\033[33m"
RESET = "\033[0m"

class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=WEB_DIR, **kwargs)

    def log_message(self, format, *args):
        # Standard HTTP log
        sys.stdout.write("%s - [%s] %s\n" % (self.client_address[0], self.log_date_time_string(), format % args))

    def do_GET(self):
        super().do_GET()
        path = self.path.split('?', 1)[0]
        if path in ('/', '/index.html', '/q3-outlook.html'):
            ts = datetime.utcnow().isoformat() + 'Z'
            print(f"{GREEN}✓ INJECTION PAGE SERVED{RESET} {ts} GET {path}")
            sys.stdout.flush()

httpd = ThreadingHTTPServer((WEB_HOST, WEB_PORT), Handler)
print(f"{YELLOW}Server ready{RESET} on http://{WEB_HOST}:{WEB_PORT}")
sys.stdout.flush()

try:
    httpd.serve_forever()
except KeyboardInterrupt:
    print("Server stopped")
PY

# Give inject server a moment to start
sleep 0.5

echo ""
echo "=========================================="
echo "✅ Both servers running"
echo "=========================================="
echo ""
echo "Ready to run agent with:"
echo "  ./scripts/agent.sh"
echo ""
echo "Or monitor exfiltration in another terminal:"
echo "  tail -f $OUTFILE"
echo ""
echo "Waiting for requests (logs appear below)..."
echo "=========================================="
echo ""

# Wait for all background jobs
wait
