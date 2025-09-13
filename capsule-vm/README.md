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
multipass transfer -r . myagent:/home/ubuntu/work

# enter and work
multipass shell myagent

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
  - Then: `multipass transfer -r . sandbox:/home/ubuntu/work && multipass shell sandbox`
  - Defaults: `--cpus 2 --memory 1G --disk 8G`.
```

---

### Troubleshooting

- Boot race: If `multipass shell <name>` fails right after create, wait a few seconds and try again (cloud-init finishing up).
- Cloud-init: Edit `./cloud-init.yaml` to customize packages and setup. Use `--template <path>` to point at another file.
- Clean state: `capsule-vm clean` removes `~/.capsule-vm` metadata. For images/VMs, use `multipass delete <name> && multipass purge`.

---

### Uninstall and Full Wipe

```bash
# best-effort uninstall (removes configs and common install locations)
capsule-vm uninstall

# or manual removal
sudo rm -f /usr/local/bin/capsule-vm
rm -rf ~/.capsule-vm

# remove all Multipass VMs and cached images (affects ALL VMs)
multipass delete --all && multipass purge
```

### Update Without Cached State

- Rebuild + reinstall: `./scripts/update.sh`
- Ensure fresh VM image: `multipass delete <name> && multipass purge`
- Ensure fresh metadata: `capsule-vm clean`
- Ensure fresh provisioning: edit `./cloud-init.yaml` or pass `--template <path>` explicitly on create

---

### Behavior Model: Capsule wrapper over Multipass

- Capsule orchestrates Multipass under the hood and aims for a cohesive UX.
- Visual entry: on `capsule-vm shell <name>`, Capsule installs a login banner (MOTD) so you immediately see you’re inside the capsule.
- Convention over config: sane defaults (Ubuntu 24.04, non-root user, workspace at `~/work`).
- Extensible tooling: `--tools` installs via a generated script, idempotent with markers in `/var/lib/capsule-vm/tools/`.
- Future directions: light agent inside the VM for richer orchestration, YAML-driven profiles, and tighter sync tooling—while keeping Multipass as the trusted VM layer.
