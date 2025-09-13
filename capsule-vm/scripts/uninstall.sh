#!/usr/bin/env bash
set -euo pipefail

PREFIX="/usr/local"
BIN_DIR="$PREFIX/bin"
USER_MODE=0

usage() {
  echo "Usage: $0 [--prefix /path/to/prefix | --user]" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      shift
      [[ $# -gt 0 ]] || usage
      PREFIX="$1"
      BIN_DIR="$PREFIX/bin"
      ;;
    --user)
      USER_MODE=1
      PREFIX="$HOME/.local"
      BIN_DIR="$PREFIX/bin"
      ;;
    *)
      usage
      ;;
  esac
  shift
done

TARGETS=(
  "$BIN_DIR/capsule-vm"
  "/usr/local/bin/capsule-vm"
  "$HOME/.local/bin/capsule-vm"
)

echo "Removing capsule-vm binaries (best effort)..."
for t in "${TARGETS[@]}"; do
  if [[ -e "$t" ]]; then
    if [[ -w "$t" ]]; then
      rm -f "$t" && echo "Removed $t" || echo "Could not remove $t"
    else
      sudo rm -f "$t" && echo "Removed $t (sudo)" || echo "Could not remove $t"
    fi
  fi
done

echo "Removing local config/state..."
rm -rf "$HOME/.capsule-vm" || true

echo "Removing temp files..."
rm -f /tmp/capsule-vm-cloud-init.yaml || true
rm -f /tmp/capsule-vm-setup-*.sh || true

echo "Uninstall complete. Some system paths may require sudo."

