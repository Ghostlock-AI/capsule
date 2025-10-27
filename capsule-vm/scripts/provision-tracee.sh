#!/usr/bin/env bash
set -euo pipefail

VM_NAME="$1"

echo "[provision-tracee] Detecting guest architecture..."
ARCH=$(limactl shell "$VM_NAME" uname -m 2>/dev/null | grep -v "═" | grep -v "CAPSULE" | grep -v "Workspace" | grep -v "Address" | grep -v "Resources" | grep -v "📂" | grep -v "🌐" | grep -v "💻" | tr -d '\n\r ' | tail -c 20)

VERSION="v0.23.2"
case "$ARCH" in
  x86_64)  TAR="tracee-x86_64.${VERSION}.tar.gz" ;;
  aarch64) TAR="tracee-aarch64.${VERSION}.tar.gz" ;;
  *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
esac

CACHE="$HOME/.capsule-vm/cache"
mkdir -p "$CACHE"

echo "[provision-tracee] Checking cache for $TAR..."
if [ ! -f "$CACHE/$TAR" ]; then
  echo "[provision-tracee] Downloading Tracee ${VERSION}..."
  curl -fsSL "https://github.com/aquasecurity/tracee/releases/download/${VERSION}/$TAR" -o "$CACHE/$TAR"
  echo "[provision-tracee] Downloaded to cache"
else
  echo "[provision-tracee] Using cached $TAR"
fi

echo "[provision-tracee] Transferring to VM..."
limactl copy "$CACHE/$TAR" "$VM_NAME":/tmp/$TAR

echo "[provision-tracee] Installing in VM..."
limactl shell "$VM_NAME" sudo mkdir -p /usr/local/bin
limactl shell "$VM_NAME" sudo tar -xzf /tmp/$TAR -C /tmp
limactl shell "$VM_NAME" sudo cp /tmp/dist/tracee /usr/local/bin/tracee
limactl shell "$VM_NAME" sudo cp -r /tmp/dist/signatures /usr/local/bin/signatures
limactl shell "$VM_NAME" sudo chmod +x /usr/local/bin/tracee
limactl shell "$VM_NAME" sudo rm -rf /tmp/$TAR /tmp/dist /tmp/LICENSE

echo "[provision-tracee] Verifying installation..."
limactl shell "$VM_NAME" tracee --help | head -n 5

echo "[provision-tracee] ✅ Tracee installed successfully!"
