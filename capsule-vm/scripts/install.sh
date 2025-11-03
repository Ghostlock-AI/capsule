#!/usr/bin/env bash
set -euo pipefail

PREFIX=/usr/local
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

echo "Building release binary (capsule-vm)..."
cargo build --release -p capsule-vm

mkdir -p "$BIN_DIR"

install_bin() {
  local mode="$1"
  local src="$2"
  local dest="$3"

  if [[ "$USER_MODE" -eq 1 ]]; then
    install -m "$mode" "$src" "$dest"
  else
    sudo install -m "$mode" "$src" "$dest"
  fi
}

install_bin 0755 target/release/capsule-vm "$BIN_DIR/capsule-vm"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "[note] Add $BIN_DIR to your PATH to use capsule-vm globally." ;;
esac

echo "Installed binaries:"
echo "  $BIN_DIR/capsule-vm"
echo "Use \"capsule-vm --help\" to confirm installation."
