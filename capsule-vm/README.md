# Capsule VM

A lightweight VM orchestrator for running AI agents securely on your local machine via CLI.

---

For developers who want to spin up a safe, isolated VM for coding or general-purpose agents (Codex, Claude Code, custom tools) without exposing their host OS. It is not intended for production servers.

Think of Capsule VM like instant ramen for sandboxed agent interactions.
One step to a sandboxed Linux VM with tracing, basic tooling, and a user controlled workspace.

Unlike containers, Capsule VM always runs agents in a full VM with hardened defaults and built in tracing of every action the agent takes, so all sessions are fully auditable, replayable, and secure to a limited degree (though far more than running the agent without the VM).

---

### Features

- Create Ubuntu 24.04 capsules
  - `capsule-vm create myvm . --cpus 2 --memory 1G --disk 8G`
- Post-boot tool install (idempotent, dependency-aware)
  - `capsule-vm create myvm . --tools "python,rust,git,build"`
  - Existing VM: `capsule-vm tools install myvm --tools "web"` (installs node,npm,bun)
- Quick status and lifecycle
  - `capsule-vm ps` · `capsule-vm start myvm` · `capsule-vm stop myvm` · `capsule-vm delete myvm`
- Session-aware tracing with allowlists and mode toggles
  - `capsule-vm exec myvm -- python script.py`
  - `capsule-vm trace mode myvm session|global`
- Shell with visual banner (MOTD)
  - `capsule-vm shell myvm` (shows Capsule VM ASCII banner on login)
- Template override (cloud-init)
  - `capsule-vm create myvm . --template ./cloud-init.yaml`
- Clean metadata / uninstall
  - `capsule-vm clean` · `capsule-vm uninstall`

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
capsule-vm create myagent . --cpus 2 --memory 1G --tools "python,rust,git,build"

# copy your current dir into the VM when it's ready
limactl cp -r . myagent:/home/ubuntu/work

# enter and work
limactl shell myagent

# list active sandboxes
capsule-vm ps

# stop / start / delete
capsule-vm stop myagent
capsule-vm start myagent
capsule-vm delete myagent

# clear cached templates and temp files
capsule-vm clean

### Simple start

- Create a basic VM from the current directory with defaults:
  - `capsule-vm create sandbox .`
  - Then: `limactl cp -r . sandbox:/home/ubuntu/work && limactl shell sandbox`
  - Defaults: `--cpus 2 --memory 1G --disk 8G`.
```

---

### Tracing & Session Scoping

Every capsule boots with Tracee v0.23.2 managed by systemd services:

- `capsule-tracee.service` runs Tracee with JSON output at `/var/log/capsule-vm/tracee/events.jsonl`.
- `capsule-tracee-watcher.service` monitors `/var/lib/capsule-vm/sessions/*.env`, rebuilds `--scope` filters, and restarts Tracee when sessions change. It honors `/etc/capsule-tracee/allowlist.comm` and `/etc/capsule-tracee/mode` (`session` by default).

`capsule-vm ps` now reports a `Tracee` column (`running`, `starting`, `stopped`, `failed`, `unknown`) so you can catch unhealthy sandboxes quickly.

#### Capturing interactive shells

`capsule-vm shell <name>` installs a login profile that runs `capsule-session adopt interactive`. The shell PID tree is registered automatically, so Tracee follows the user workflow (and descendants) while ignoring background daemons. Session metadata is appended to `/var/log/capsule-vm/tracee/session.log`.

#### Running one-off commands

Use `capsule-vm exec <name> -- <command...>` for non-interactive runs. The CLI wraps the command with `capsule-session run`, writes a session file, and Tracee traces the full process tree. Example:

```bash
capsule-vm exec demo -- python -c "print('hello from capsule')"
```

Logs land in `/var/log/capsule-vm/tracee/events.jsonl`; session files disappear shortly after the process exits.

#### Adjusting the trace scope

- `capsule-vm trace mode <name> session` (default) traces only active sessions; Tracee stops when no sessions remain.
- `capsule-vm trace mode <name> global` keeps Tracee running for the whole VM (still excluding allowlisted daemons).

Both commands update `/etc/capsule-tracee/mode` and restart the watcher. Extend the allowlist by editing `/etc/capsule-tracee/allowlist.comm` on the VM (one command name per line).

#### Manual verification

1. `capsule-vm create demo .` → after provisioning, `capsule-vm ps` shows Tracee `stopped` until a session exists.
2. `capsule-vm shell demo` → in another terminal, `capsule-vm ps` reports Tracee `running`; inside the VM, list `/var/lib/capsule-vm/sessions` to see the interactive session file.
3. `capsule-vm exec demo -- bash -lc 'touch /tmp/trace-demo && ls /tmp'` → review `/var/log/capsule-vm/tracee/events.jsonl` to confirm exec/file syscalls were captured; the session file is removed after the command ends.
4. `capsule-vm trace mode demo global` → after a few seconds, Tracee remains `running` even without sessions; watcher mode changes are logged to `/var/log/capsule-vm/tracee/watcher.log`.

---

### AppArmor Security Profiles

AppArmor provides mandatory access control to restrict what programs can do on the system. Capsule VM provisions a `capsule-agent` user with an AppArmor profile that confines agent code to a restricted workspace. The profile allows network access and execution of system binaries but denies access to sensitive files like `/etc/shadow`, `/root`, and other users' home directories. Agents can only read/write within `/home/capsule-agent/workspace` and `/tmp`.

```bash
# Verify AppArmor profile is loaded
sudo aa-status | grep capsule-agent
# Should show: capsule-agent (enforce)

# Run command as restricted user
capsule-run whoami
# Returns: capsule-agent

# Test restrictions - these should fail
capsule-run cat /etc/shadow       # Denied by AppArmor
capsule-run ls /root              # Denied by AppArmor
capsule-run cat /home/ubuntu/.bashrc  # Denied by AppArmor

# Test allowed operations
capsule-run touch /home/capsule-agent/workspace/test.txt  # Works
capsule-run python3 -c "print('hello')"  # Works
```

---

### Troubleshooting

- Boot race: If `limactl shell <name>` fails right after create, wait a few seconds and try again (cloud-init finishing up).
- Cloud-init: Edit `./cloud-init.yaml` to customize packages and setup. Use `--template <path>` to point at another file.
- Clean state: `capsule-vm clean` removes `~/.capsule-vm` metadata. For Lima instances, run `limactl delete <name>` (and `limactl prune` to clear caches).

### Development Checks

Run the baseline validation suite before committing:

```bash
./scripts/check.sh
```

The script ensures formatting (`cargo fmt`), lint cleanliness (`cargo clippy -- -D warnings`), and a unit-test pass (`cargo test`).

---

### Uninstall and Full Wipe

```bash
# best-effort uninstall (removes configs and common install locations)
capsule-vm uninstall

# or manual removal
sudo rm -f /usr/local/bin/capsule-vm
rm -rf ~/.capsule-vm

# remove all Lima VMs and cached images (affects ALL VMs)
limactl delete --all --force && limactl prune
```

### Update Without Cached State

- Rebuild + reinstall: `./scripts/update.sh`
- Ensure fresh VM image: `limactl delete <name> --force && limactl prune`
- Ensure fresh metadata: `capsule-vm clean`
- Ensure fresh provisioning: edit `./cloud-init.yaml` or pass `--template <path>` explicitly on create

---

### Behavior Model: Capsule wrapper over Lima

- Capsule orchestrates Lima under the hood and aims for a cohesive UX.
- Visual entry: on `capsule-vm shell <name>`, Capsule installs a login banner (MOTD) so you immediately see you’re inside the capsule.
- Convention over config: sane defaults (Ubuntu 24.04, non-root user, workspace at `~/work`).
- Extensible tooling: `--tools` installs via a generated script, idempotent with markers in `/var/lib/capsule-vm/tools/`.
- Future directions: light agent inside the VM for richer orchestration, YAML-driven profiles, and tighter sync tooling—while keeping Lima as the trusted VM layer.
