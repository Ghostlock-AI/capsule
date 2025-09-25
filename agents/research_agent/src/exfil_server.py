#!/usr/bin/env python3
"""
Local HTTP server to receive simulated exfiltration.

Usage:
  python3 src/exfil_server.py --host 127.0.0.1 --port 8765 --outfile output/exfil_log.jsonl

Writes each POST body to the JSONL log with a timestamp.
"""

import argparse
import json
import os
from datetime import datetime
from http.server import BaseHTTPRequestHandler, HTTPServer


class ExfilHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get('content-length', 0))
        body = self.rfile.read(length) if length else b""
        # Ensure output directory exists
        os.makedirs(os.path.dirname(self.server.outfile), exist_ok=True)

        # Write JSONL record
        record = {
            "time": datetime.utcnow().isoformat() + "Z",
            "path": self.path,
            "remote": self.client_address[0],
            "bytes": len(body),
            "body": body.decode('utf-8', errors='replace'),
        }
        with open(self.server.outfile, 'a', encoding='utf-8') as f:
            f.write(json.dumps(record) + "\n")

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"status":"ok"}')

    # Silence default logging to keep the console clean
    def log_message(self, format, *args):
        return


def main():
    parser = argparse.ArgumentParser(description="Local exfiltration server")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", default=8765, type=int)
    parser.add_argument("--outfile", default="output/exfil_log.jsonl")
    args = parser.parse_args()

    httpd = HTTPServer((args.host, args.port), ExfilHandler)
    httpd.outfile = args.outfile
    print(f"Exfil server listening on http://{args.host}:{args.port} -> {args.outfile}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nExfil server stopped.")


if __name__ == "__main__":
    main()

