#!/usr/bin/env bash
set -euo pipefail

# Reuse install.sh semantics to rebuild and reinstall
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$SCRIPT_DIR/install.sh" "$@"

