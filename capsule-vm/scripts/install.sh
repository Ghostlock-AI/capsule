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

echo "Building release binary..."
cargo build --release

mkdir -p "$BIN_DIR"

if [[ "$USER_MODE" -eq 1 ]]; then
  install -m 0755 target/release/capsule-vm "$BIN_DIR/capsule-vm"
else
  sudo install -m 0755 target/release/capsule-vm "$BIN_DIR/capsule-vm"
fi

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "[note] Add $BIN_DIR to your PATH to use 'capsule-vm' globally." ;;
esac

echo "Installed: $BIN_DIR/capsule-vm"
echo "Try: capsule-vm --help"
