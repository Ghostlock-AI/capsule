#!/usr/bin/env bash
set -euo pipefail

# Get the directory where this script is located
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# Create virtual environment if it doesn't exist
if [[ ! -d "$SCRIPT_DIR/.venv" ]]; then
  echo "Creating virtual environment..."
  python3 -m venv "$SCRIPT_DIR/.venv"
fi

# Activate venv (only in this script's subshell)
source "$SCRIPT_DIR/.venv/bin/activate"

# Install/upgrade dependencies
echo "Installing requirements..."
python3 -m pip install --upgrade pip -q
python3 -m pip install -r "$SCRIPT_DIR/requirements.txt" -q

echo ""
echo "Next: run source .venv/bin/activate"
echo ""
