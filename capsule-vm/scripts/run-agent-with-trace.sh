#!/usr/bin/env bash
set -euo pipefail

VM_NAME="$1"; shift
CMD=("$@")

if [ ${#CMD[@]} -eq 0 ]; then
  echo "Usage: $0 <vm-name> <command> [args...]"
  exit 1
fi

BASENAME=$(basename "${CMD[0]}")
OUTPUT_DIR="$HOME/.capsule-vm/traces/$VM_NAME"
mkdir -p "$OUTPUT_DIR"
LOG="$OUTPUT_DIR/run-$(date -u +%Y%m%dT%H%M%SZ).jsonl"

echo "[tracee] Tracing command: ${CMD[*]}"
echo "[tracee] Filtering by comm=$BASENAME"
echo "[tracee] Output: $LOG"
echo ""

# Start Tracee in background (v0.23 syntax: -s for scope, -e for events)
limactl shell "$VM_NAME" "sudo tracee -s comm=$BASENAME -s follow -e execve,open,openat,connect,socket --output json" > "$LOG" 2>&1 &
TRACE_PID=$!

# Give Tracee time to attach
echo "[tracee] Waiting for Tracee to attach..."
sleep 2

# Run the command
echo "[tracee] Executing command..."
limactl shell "$VM_NAME" "${CMD[@]}"

# Stop Tracee
echo ""
echo "[tracee] Stopping Tracee..."
kill $TRACE_PID 2>/dev/null || true
wait $TRACE_PID 2>/dev/null || true

echo "[tracee] ✅ Trace saved to: $LOG"
echo "[tracee] View events: cat $LOG | jq -r '.eventName' | sort | uniq"
