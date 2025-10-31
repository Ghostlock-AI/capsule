# Capsule VM

A lightweight VM orchestrator for running AI agents securely on your local machine via CLI.

---

For developers who want to spin up a safe, isolated VM for coding or general-purpose agents (Codex, Claude Code, custom tools) without exposing their host OS. It is not intended for production servers.

Think of Capsule VM like instant ramen for sandboxed agent interactions.
One step to a sandboxed Linux VM with tracing, basic tooling, and a user controlled workspace.

Unlike containers, Capsule VM always runs agents in a full VM with hardened defaults and built in tracing of every action the agent takes, so all sessions are fully auditable, replayable, and secure to a limited degree (though far more than running the agent without the VM).

---

### Features

- Create Ubuntu 24.04 capsules via Lima
  - `capsule-vm create myvm . --cpus 2 --memory 1G --disk 8G`
- Basic lifecycle management
  - `capsule-vm ps` · `capsule-vm start myvm` · `capsule-vm stop myvm` · `capsule-vm delete myvm`
- Direct shell access for interactive work
  - `capsule-vm shell myvm`
- Cloud-init override support
  - `capsule-vm create myvm . --template ./cloud-init.yaml`
- Tracee v0.23.2 installed inside every VM (available via `tracee --version`)

---

### Install (system-wide as `capsule-vm`)

```bash
# build + install (system-wide)
./scripts/install.sh
# or user install (no sudo)
./scripts/install.sh --user
# verify
capsule-vm --help && capsule-vm --version
```

Linux (user install)

```bash
./scripts/install.sh --user
```

Windows Alternative

```bash
# build in a dev shell
cargo build --release
# build then copy `target\release\capsule-vm.exe` to a directory on PATH
capsule-vm --help
```

---

### Example Usage

```bash
# create a VM with defaults (uses ./cloud-init.yaml)
capsule-vm create myagent --cpus 2 --memory 1G

# enter and work
capsule-vm shell myagent

# list active sandboxes
capsule-vm ps

# stop / start / delete
capsule-vm stop myagent
capsule-vm start myagent
capsule-vm delete myagent

### Simple start

- Create a basic VM from the current directory with defaults:
- `capsule-vm create sandbox`
  - Then: `capsule-vm shell sandbox`
  - Defaults: `--cpus 2 --memory 1G --disk 8G`.
```

---

### Agent User & Tracee Logs

- Capsule shells drop you into the unprivileged `agent` account; the workspace lives under `/home/agent/workspace`.
- Create a fresh capsule (ensure cloud-init has Python if needed):
  - `capsule-vm create py-trace --cpus 2 --memory 1G --disk 8G`
- Open the guest and run your workload:
  - `capsule-vm shell py-trace`
  - `python3 my_script.py`
- Inspect Tracee output written by the systemd service (root-owned files):
  - Event stream: `sudo tail -n 20 /var/log/tracee/events.log`
  - Service diagnostics: `sudo journalctl -u tracee --no-pager` or `sudo tail -n 50 /var/log/tracee/tracee.log`

All Tracee filters from `cloud-init.yaml` load automatically, so the log file contains only process, file I/O, network, credential, and signal syscalls related to your script.

---

### Tracee Binary

Every capsule boots with Tracee v0.23.2 installed via `cloud-init`. The binary is not started automatically; it is simply ready for you to run when needed.

```bash
capsule-vm create demo
capsule-vm shell demo
tracee --version
```

If you want Tracee running continuously, enable it manually inside the VM just as you would on any Ubuntu system.

---

### Troubleshooting

- Boot race: If `limactl shell <name>` fails right after create, wait a few seconds and try again (cloud-init finishing up).
- Cloud-init: Edit `./cloud-init.yaml` to customize packages and setup. Use `--template <path>` to point at another file.
- Clean state: remove `~/.capsule-vm` metadata manually if you need to start fresh.

### Development Checks

Run the baseline validation suite before committing:

```bash
./scripts/check.sh
```

The script ensures formatting (`cargo fmt`), lint cleanliness (`cargo clippy -- -D warnings`), and a unit-test pass (`cargo test`).

---

### Update Without Cached State

- Rebuild + reinstall: `./scripts/update.sh`
- Ensure fresh VM image: `limactl delete <name> --force && limactl prune`
- Ensure fresh provisioning: edit `./cloud-init.yaml` or pass `--template <path>` explicitly on create
