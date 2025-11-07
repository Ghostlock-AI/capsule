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
  - `capsule-vm create myvm --cpus 2 --memory 1G --disk 8G`
- Basic lifecycle management
  - `capsule-vm ps` · `capsule-vm start myvm` · `capsule-vm stop myvm` · `capsule-vm delete myvm`
- Direct shell access for interactive work
  - `capsule-vm shell myvm` (as agent user) · `capsule-vm shell myvm --root`
- Cloud-init override support
  - `capsule-vm create myvm --template ./cloud-init.yaml`
- **eBPF Security Monitoring** via Tracee v0.23.2
  - Auto-installed and running as systemd service
  - Captures all process, network, and file operations by agent user
  - Raw events: `/var/log/tracee/events.jsonl`
  - Human-readable events: `/var/log/tracee/events-readable.jsonl` (auto-filtered)
  - View logs: `capsule-vm logs myvm` (pretty-printed with colors)
- **Comprehensive Event Tracking**
  - Process execution: `sched_process_exec`, `execve`
  - Process termination: `exit_group`, `exit`
  - Network connections: `net_tcp_connect` (TCP with IP:port), `connect` (fallback)
  - File operations: `openat` (open/create with flags), `close`
  - All events automatically transformed to human-readable format
- **Log Filtering System**
  - Background daemon (`capsule-log-filter`) runs inside VM
  - Reduces event size by 90% (1905 bytes → 199 bytes)
  - Clean descriptions: "Connected to: 23.220.75.245:443", "Executed: curl"

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
  - Streaming of Lima's serial console is enabled by default; append `--no-stream-logs` to silence it.
  - Then: `capsule-vm shell sandbox`
  - Defaults: `--cpus 2 --memory 1G --disk 8G`.
```

### Tool Bundles

- Discover supported installers: `capsule-vm tools`
- Provision with tools preinstalled: `capsule-vm create devbox --tools codex,python3`
- Capsule injects these bundles into the cloud-init script so snaps/npm installs finish during first boot (no extra post-create steps).
- Bundles map to snap/npm recipes (e.g. `codex` installs Node via snap then `@openai/codex`; `claude` installs the Anthropic CLI; `python3` uses `snap install python3-alt`; `rust` installs `rustup` via snap and runs `rustup default stable`).

---

### Secrets Management

Capsule VM provides secure environment variable management for API keys and other secrets needed by AI agents.

#### Creating VMs with Secrets

Provide a `.env` file during VM creation to inject secrets:

```bash
# Create .env file with your secrets
cat > .env << EOF
ANTHROPIC_API_KEY=sk-ant-api03-xxx
OPENAI_API_KEY=sk-xxx
CUSTOM_TOKEN=my-secret-token
EOF

# Create VM with secrets
capsule-vm create myagent --secrets .env --tools claude
```

The secrets are automatically available as environment variables to all processes running as the `agent` user, including AI coding agents like Claude Code and Codex.

#### Updating Secrets Post-Creation

Update secrets in an existing VM without recreating it:

```bash
# Update with new .env file
capsule-vm secrets myagent --file .env.production

# List current secret keys (values are hidden)
capsule-vm secrets myagent --list
```

**Note**: Existing shell sessions need to be restarted to see updated secrets. New shells automatically load the latest values.

#### Security Model

- **Storage location**: `/etc/capsule/secrets.env`
- **Owner**: `root:root`
- **Permissions**: `0644` (readable by all, writable only by root)
- **Agent access**: Can READ secrets via environment variables, cannot MODIFY the secrets file
- **Injection method**: Sourced automatically via `/etc/profile.d/capsule-secrets.sh` on shell initialization

Secrets are visible to all processes running as the agent user and will appear in Tracee event logs (for auditability). This is by design - the security boundary prevents modification, not visibility.

---

### Agent User & Tracee Logs

- Capsule shells drop you into the unprivileged `agent` account; the workspace lives under `/home/agent/workspace`.
- Create a fresh capsule with Python ready to go:
  - `capsule-vm create py-trace --cpus 2 --memory 1G --disk 8G`
- Open the guest, confirm instrumentation, and run code:
  - `capsule-vm shell py-trace`
  - `python3 --version` (preinstalled by cloud-init)
  - `python3 agent.py`
- Need to inspect root-owned artifacts during development? Use `capsule-vm shell py-trace --root` (provided by a temporary passwordless sudo rule at `/etc/sudoers.d/agent-dev`).
- Tracee scopes collection to the `agent` account and its primary group, so `--root` sessions stay outside the capture window unless you relax the unit file.
- Verify Tracee captured an `agent` session (JSONL only): `sudo tail -n 20 /var/log/tracee/events.jsonl | jq '.'`
  - Daemon diagnostics: `sudo tail -n 50 /var/log/tracee/tracee.log`
  - Full service history: `sudo journalctl -u tracee --no-pager`

Filtering and enrichment are preconfigured so the JSONL stream focuses on process, filesystem, network, credential, and signal activity, with file descriptors and peer addresses already expanded.

### Capsule Shell Workflow

- `capsule-vm shell <vm>` delegates to the active backend (Lima) to open an interactive session inside the guest.
- The backend executes `limactl shell <vm> sudo -iu <user> /bin/bash`, defaulting to the `agent` account unless you pass `--root`.
- Tracee continues running in the background but only observes the `agent` identity, so root sessions stay invisible unless you adjust the systemd unit.

### Tracee Telemetry

- Output sinks:
  - Tracee internal logs in `/var/log/tracee/tracee.log`
  - JSONL event stream in `/var/log/tracee/events.jsonl` (parse with `jq` or other tooling)
- Output options in `/etc/tracee/config.yaml`:
  - `parse-arguments` and `parse-arguments-fds` humanise syscall arguments and resolve file descriptors to paths.
  - `exec-hash: digest-inode` emits SHA-256 hashes tied to executable provenance for replayability.
- Scope filters:
  - Broad syscall sets `proc`, `fs`, `net`, and `signals` plus targeted events (`commit_creds`, `set*id`, `capset`, `keyctl`, `ptrace`, `kill*`, etc.) highlight privilege changes, credential misuse, and process control without the noisier `security_*` LSM hooks.
  - Runtime scope clamps collection to new processes while binding capture to the `agent` identity (`--scope uid=$(id -u agent) --scope pid=new --scope follow`), keeping host daemons and root escalations out of band unless you explicitly opt in.
  - SELinux/AppArmor enforcement audits are intentionally omitted here; rely on the host’s `auditd` stream for those decisions while Tracee focuses on syscall-level behaviour.
  - Containers are disabled (`containers.enrich=false`) so only host PIDs appear, reducing noise.
- Context helpers:
  - DNS cache enabled to resolve peer names in network logs.
  - Process tree cache retains ancestry so forks/background daemons remain tied to the originating agent session.

This telemetry mix answers questions like "which files left the sandbox" (inspect `send*` args), "what binaries executed" (hash + env), "did anything escalate" (credential syscalls), and "which peers received traffic" (socket addresses with DNS names).

---

### Tracee Service

Tracee ships as a systemd service that activates on first boot and restarts if it ever crashes. The unit runs `/bin/bash -lc "/usr/local/bin/tracee --config /etc/tracee/config.yaml --scope uid=$(id -u agent) --scope pid=new --scope follow --log file:/var/log/tracee/tracee.log"`, so you can manage it with:

```bash
sudo systemctl status tracee
sudo systemctl restart tracee
sudo tail -f /var/log/tracee/events.jsonl | jq '.'
# SELinux/AppArmor enforcement remains visible via auditd when enabled
sudo rm /etc/sudoers.d/agent-dev
```

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

---

## Implementation Status

### ✅ Completed Features

**Log Filtering System** (Nov 2025)
- Human-readable log filter daemon implemented in Rust (`capsule-log-filter`)
- Transforms verbose Tracee events (1905 bytes → 199 bytes, 90% reduction)
- Comprehensive event coverage:
  - Process lifecycle: `sched_process_exec`, `execve`, `exit_group`, `exit`
  - Network: `net_tcp_connect` (clean TCP with IP:port), `connect` (fallback for all sockets)
  - File operations: `openat` (with open flags), `close`
- Event transformations produce clean descriptions:
  - "Executed: curl"
  - "Connected to: 23.220.75.245:443"
  - "Opened file: /etc/hosts (read)"
  - "Closed fd: 3"
- Configured in `cloud-init.yaml` with 9 baseline + security events
- All transformation handlers tested with real Tracee event samples

### 🚧 In Progress

**Log Filter Deployment**
- Binary embedding into cloud-init (base64 encoding issue with inline content)
- Systemd service configuration complete (`/etc/systemd/system/capsule-log-filter.service`)
- Workaround: Binary can be manually copied for testing

**Pretty-Print CLI**
- `capsule-vm logs` command exists but shows raw Tracee events
- Need to implement pretty-printing with colors for human-readable events
- Planned format: `HH:MM:SS [user] description` with ANSI colors

### 📋 Planned

- Fix binary embedding for automatic deployment
- Complete `capsule-vm logs` pretty-printing implementation
- Add `--raw` and `--no-color` flags to logs command
- Test end-to-end with freshly provisioned VM
