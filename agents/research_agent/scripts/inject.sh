#!/usr/bin/env bash
set -euo pipefail

# Simple static web server hosting the ./site directory (injector).

# Resolve repo root relative to this script (scripts/..)
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

# Activate venv if present
if [[ -d "$REPO_ROOT/.venv" ]]; then
  source "$REPO_ROOT/.venv/bin/activate"
fi

WEB_DIR=${WEB_DIR:-site}
WEB_HOST=${WEB_HOST:-127.0.0.1}
WEB_PORT=${WEB_PORT:-8080}

echo "Serving injection site: $WEB_DIR on http://$WEB_HOST:$WEB_PORT"
echo "Try: curl -s http://$WEB_HOST:$WEB_PORT/q3-outlook.html | head -n 20"
echo
cat <<'BANNER'
  __  __ _    __  ____  ___  ____  __  ____ 
 (  )(  ( \ _(  )(  __)/ __)(_  _)/  \(  _ \
  )( /    // \) \ ) _)( (__   )( (  O ))   /
 (__)")__)(\____/(____)\___) (__) \__/(__\_)
BANNER

# Extra space between the banner and server logs
echo

# Start a small HTTP server that logs injection page hits in green
python3 - << 'PY'
import os
import sys
from datetime import datetime
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler

WEB_DIR = os.environ.get('WEB_DIR', 'site')
WEB_HOST = os.environ.get('WEB_HOST', '127.0.0.1')
WEB_PORT = int(os.environ.get('WEB_PORT', '8080'))

GREEN = "\033[32m"
RESET = "\033[0m"

class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=WEB_DIR, **kwargs)

    def log_message(self, format, *args):
        # Keep standard HTTP log
        sys.stdout.write("%s - - [%s] %s\n" % (self.client_address[0], self.log_date_time_string(), format % args))

    def do_GET(self):
        super().do_GET()
        path = self.path.split('?', 1)[0]
        if path in ('/', '/q3-outlook.html'):
            ts = datetime.utcnow().isoformat() + 'Z'
            print(f"{GREEN}[INJECTED]{RESET} {ts} GET {path} from {self.client_address[0]}")
            sys.stdout.flush()

httpd = ThreadingHTTPServer((WEB_HOST, WEB_PORT), Handler)
print(f"Listening on http://{WEB_HOST}:{WEB_PORT} (serving {WEB_DIR})")
try:
    httpd.serve_forever()
except KeyboardInterrupt:
    print("\nInjector server stopped.")
PY
