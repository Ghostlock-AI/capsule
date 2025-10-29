# Capsule Tracee Integration Spec

## What Was Added

- **Tracee runner service (`capsule-tracee.service`)**
  - Boots Tracee v0.23.2 automatically.
  - Emits JSON events to `/var/log/capsule-vm/tracee/events.jsonl` covering process, file, network, signal, and credential syscalls.
  - Relies on Tracee’s eBPF probes so kernel telemetry works without user intervention.

- **Scope watcher service (`capsule-tracee-watcher.service`)**
  - Monitors `/var/lib/capsule-vm/sessions/*.env` for active workloads.
  - Reads `/etc/capsule-tracee/allowlist.comm` and `/etc/capsule-tracee/mode` (`session` vs `global`).
  - Rebuilds Tracee `--scope` arguments to follow only registered session PID trees while excluding allowlisted daemons; stops Tracee entirely when no sessions are active in `session` mode.

- **Session registry (`capsule-session`)**
  - `capsule-session adopt` auto-runs at shell login so interactive users (and their descendants) are tracked.
  - `capsule-session run` wraps non-interactive commands, writing metadata (PID, command, timestamps) to `/var/lib/capsule-vm/sessions` and logging to `/var/log/capsule-vm/tracee/session.log`.
  - `capsule-session cleanup` removes stale entries.

- **CLI controls**
  - `capsule-vm ps` shows a `Tracee` health column (`running`, `starting`, `stopped`, `failed`, `unknown`).
  - `capsule-vm exec <vm> -- <cmd>` launches commands inside the guest via `capsule-session run`, guaranteeing they are traced.
  - `capsule-vm trace mode <vm> session|global` flips tracing behavior by updating `/etc/capsule-tracee/mode` and restarting the watcher.

- **Documentation**
  - README now covers auto-tracing, session handling, mode toggles, allowlist editing, and manual verification steps.

## Why This Design Works

### Local Sandbox LLM Use Case

- **Focused visibility:** Session-scoped tracing ensures the sandbox captures exactly what the local LLM or tool executes, without drowning in background noise from the VM’s own services.
- **Zero-touch operator experience:** Interactive shells auto-register via profile scripts, so developers don’t need to remember extra commands. When the session ends, Tracee winds down automatically, keeping resource usage low.
- **Human-friendly feedback:** `capsule-vm ps` immediately shows whether Tracee is healthy, so operators know the audit trail is live before letting an agent run unattended.

### Firecracker / Fleet Evaluation Use Case

- **Consistent telemetry contract:** Session files encode PID, command, mode, and timestamps, giving an orchestrator (or collector) the context it needs to stream traces off-box and correlate them with jobs.
- **Scoped yet configurable:** Default `session` mode keeps per-VM volume predictable; switching to `global` is a single CLI/API call if a broader forensic sweep is needed. Allowlist files give fine-grained control per image.
- **Security posture:** Because Tracee is eBPF-based and runs alongside AppArmor confinement, it surfaces suspicious syscalls (network, credential, file exfil) without having to inject agents into the workload itself—ideal for spotting compromised or misaligned behavior in batch agents.

