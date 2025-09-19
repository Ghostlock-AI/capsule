# Capsule Runtime V2 - Process Monitoring & Analysis

## Overview

Capsule Runtime provides real-time process monitoring and analysis through kernel-level syscall tracing. Features live TUI monitoring, process state tracking, and comprehensive syscall analysis.

## Quick Start

If you are in the capsule-integration container...

```bash
capsule run claude
capsule run codex
capsule run python3 agent.py
```

### Installation

```bash
# installs globally as capsule
cargo install --path cli --force
```

### Usage

If a program is running under capsule trace...

```bash
# Monitor live processes in TUI
capsule monitor
```

### Pipeline & Session Logs

- Every `capsule run …` spawns the trace → parse → track pipeline and stores artifacts under `~/.capsule/runs/<session>/`.
- Session directories are timestamp-prefixed (e.g., `2024-01-15T14:30:00Z-1a2b3c`) and include:
  - `metadata.json` – session info and original command
  - `syscalls.jsonl` – raw `strace` lines (plain text today)
  - `events.jsonl` – human-readable rollups with structured `ts/pid/kind/extra` fields
- Remove the directory when you no longer need the capture; the TUI reads live state from the same pipeline.
