#!/usr/bin/env bash
set -euo pipefail

# Resolve repo root relative to this script (scripts/..)
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

# Ensure virtual environment exists, create if missing
if [[ ! -d "$REPO_ROOT/.venv" ]]; then
  echo "Creating virtual environment at $REPO_ROOT/.venv ..."
  python3 -m venv "$REPO_ROOT/.venv"
fi

# Activate venv
source "$REPO_ROOT/.venv/bin/activate"

# Install/upgrade dependencies
echo "Installing requirements from requirements.txt ..."
python3 -m pip install -r "$REPO_ROOT/requirements.txt"

# Run the agent from repo root
cd "$REPO_ROOT"
exec python3 src/main.py
