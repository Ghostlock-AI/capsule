#!/usr/bin/env bash
set -euo pipefail

# Simple static web server hosting the ./site directory.

WEB_DIR=${WEB_DIR:-site}
WEB_HOST=${WEB_HOST:-127.0.0.1}
WEB_PORT=${WEB_PORT:-8000}

echo "Serving $WEB_DIR on http://$WEB_HOST:$WEB_PORT"
echo "Try: curl -s http://$WEB_HOST:$WEB_PORT/q3-outlook.html | head -n 20"
echo
cat <<'BANNER'
  __  __ _    __  ____  ___  ____  __  ____ 
 (  )(  ( \ _(  )(  __)/ __)(_  _)/  \(  _ \
  )( /    // \) \ ) _)( (__   )( (  O ))   /
 (__)")__)(\____/(____)\___) (__) \__/(__\_)
BANNER
python3 -u -m http.server "$WEB_PORT" --bind "$WEB_HOST" --directory "$WEB_DIR"
