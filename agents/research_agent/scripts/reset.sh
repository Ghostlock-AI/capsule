#!/usr/bin/env bash
set -euo pipefail

# Reset script: clears all files in the output directory.
# Expected to be run from the scripts directory (but works from anywhere).
# It prefers ../.output and falls back to ../output.

# Resolve repo root relative to this script (scripts/..)
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

PREFERRED_DIR="$REPO_ROOT/.output"
FALLBACK_DIR="$REPO_ROOT/output"

if [[ -d "$PREFERRED_DIR" ]]; then
  TARGET_DIR="$PREFERRED_DIR"
elif [[ -d "$FALLBACK_DIR" ]]; then
  TARGET_DIR="$FALLBACK_DIR"
else
  echo "Error: Neither '.output' nor 'output' directory exists under $REPO_ROOT" >&2
  exit 1
fi

# Safety guard: ensure TARGET_DIR is inside the repo root
case "$TARGET_DIR" in
  "$REPO_ROOT"/*) ;;
  *) echo "Error: Refusing to operate outside repo root ($REPO_ROOT)" >&2; exit 1 ;;
esac

echo "Clearing contents of: $TARGET_DIR"

# Count items to remove (includes hidden entries, excludes . and ..)
COUNT=$(find "$TARGET_DIR" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l | awk '{print $1}')

if [[ "$COUNT" -eq 0 ]]; then
  echo "Nothing to remove."
  exit 0
fi

# Remove everything inside, but keep the directory itself
find "$TARGET_DIR" -mindepth 1 -maxdepth 1 -exec rm -rf {} +

echo "Removed $COUNT item(s) from $TARGET_DIR"

