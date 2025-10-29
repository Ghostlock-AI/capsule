#!/usr/bin/env bash
set -euo pipefail

VM_NAME="$1"
TARGET_PID="$2"

OUTPUT_DIR="$HOME/.capsule-vm/traces/$VM_NAME"
mkdir -p "$OUTPUT_DIR"
LOG="$OUTPUT_DIR/trace-$(date -u +%Y%m%dT%H%M%SZ).jsonl"

echo "[tracee] Capturing PID=$TARGET_PID in VM=$VM_NAME"
echo "[tracee] Output: $LOG"
echo "[tracee] Press Ctrl+C to stop tracing"
echo ""

# Run Tracee with PID filter (v0.23 syntax: -s for scope)
limactl shell "$VM_NAME" "sudo tracee -s pid=$TARGET_PID -s follow -e execve,open,openat,connect,socket --output json" \
  | tee "$LOG"
