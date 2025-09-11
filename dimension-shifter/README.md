# Dimension Shifter

A lightweight VM orchestrator for running AI agents securely on your local machine via CLI.

---

D.S. for developers who want to spin up a safe, isolated “dimension” for coding or general-purpose agents (Codex, Claude Code, custom tools) without exposing their host OS. It is not intended for production servers.

Think of D.S. like instant ramen for sandboxed agent interactions.
One step to a sandboxed Linux VM with tracing, basic tooling, and a user controlled workspace.

Unlike containers, D.S. always runs agents in a full VM with hardened defaults and built in tracing of every action the agent takes, so all sessions are fully auditable, replayable, and secure to a limited degree (though far more than running the agentwithout the VM).

---

### Features

- basic secure sandbox: agent is not root user, basic seccomp denylist, noexec workspace, network restrictions.
- **full tracing**: process, file I/O, nework actions recorded.
- **transient by default**: create VM, shell in, run your agent, then delete to reclaim space.
- **convenient tools**: install things like python, rust, other dev tools with a flag.
- **workspace control**: only the directories you mount are visible; everything else is off limits.

---

### Install (system-wide as `ds`)

```bash
# build
cargo build --release
# install system-wide
sudo install -m 0755 target/release/ds /usr/local/bin/ds
# verify
ds --help && ds --version
```

Linux (user install)

```bash
mkdir -p ~/.local/bin
cp target/release/ds ~/.local/bin/
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
exec $SHELL
```

Windows Alternative

```bash
# build in a dev shell
cargo build --release
# put it on PATH (one-liner for current user)
$dest="$env:USERPROFILE\.cargo\bin\ds.exe"; New-Item -Force -ItemType Directory "$env:USERPROFILE\.cargo\bin" | Out-Null; Copy-Item target\release\ds.exe $dest
[Environment]::SetEnvironmentVariable('Path', $env:Path + ';' + "$env:USERPROFILE\.cargo\bin", 'User')
# verify
ds --help
```

---

### Example Usage

```bash
# create a VM with defaults (uses ./cloud-init.yaml)
ds create myagent . --cpus 2 --memory 1G

# copy your current dir into the VM when it's ready
multipass transfer -r . myagent:/home/ubuntu/work

# enter and work
multipass shell myagent

# list active sandboxes
ds ps

# stop / start / delete
ds stop myagent
ds start myagent
ds delete myagent

# clear cached templates and temp files
ds clean

### Simple start

- Create a basic VM from the current directory with defaults:
  - `ds create sandbox .`
  - Then: `multipass transfer -r . sandbox:/home/ubuntu/work && multipass shell sandbox`
  - Defaults: `--cpus 2 --memory 1G --disk 8G`.
```

---

### Troubleshooting

- Boot race: If `multipass shell <name>` fails right after create, wait a few seconds and try again (cloud-init finishing up).
- Cloud-init: Edit `./cloud-init.yaml` to customize packages and setup. Use `--template <path>` to point at another file.
- Clean state: `ds clean` removes `~/.dimensionshifter` metadata. For images/VMs, use `multipass delete <name> && multipass purge`.

---

### Uninstall and Full Wipe

```bash
# best-effort uninstall (removes configs and common install locations)
ds uninstall

# or manual removal
sudo rm -f /usr/local/bin/ds
rm -rf ~/.dimensionshifter

# remove all Multipass VMs and cached images (affects ALL VMs)
multipass delete --all && multipass purge
```

### Update Without Cached State

- Rebuild + reinstall: `cargo build --release && sudo install -m 0755 target/release/ds /usr/local/bin/ds`
- Ensure fresh VM image: `multipass delete <name> && multipass purge`
- Ensure fresh ds metadata: `ds clean`
- Ensure fresh provisioning: edit `./cloud-init.yaml` or pass `--template <path>` explicitly on create
