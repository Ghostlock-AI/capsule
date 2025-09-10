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

### Installation

```bash
# from your repo root
cargo build --release
# install as 'ds'
sudo install -m 0755 target/release/ds /usr/local/bin/ds
# verify
ds --help
# delete
sudo rm -rf /usr/local/bin/ds
# remove your config at ~/.dimensionshifter if desired
sudo rm -rf ~/.dimensionshifter
```

You can also create a tap and formula

```bash
brew tap yourorg/ds https://github.com/yourorg/homebrew-ds
brew install yourorg/ds/ds
```

Linux Alternative

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
# create a sandbox with Python + Rust and auto-shell into it
ds create myagent ./ --cpus 2 --mem 2G --tools "python,rust"

# list active sandboxes
ds ps

# stop / start / delete
ds stop myagent
ds start myagent
ds delete myagent

# clear cached templates and temp files
ds clean

# uninstall ds (best effort; may need sudo for /usr/local/bin)
ds uninstall

### Simple start

- Create a basic VM from the current directory with defaults and shell in:
  - `ds create sandbox .`
  - Defaults: `--cpus 2 --memory 1G --disk 8G`; copies `.` into `~/work` inside the VM (uses `ubuntu` user by default).
  - To use provisioning (packages/users), pass your own `--template ./cloud-config.yaml`.
```

---

### Troubleshooting

- Missing user 'agent': If you see errors like `chown: invalid user 'agent'` during the first sync, ensure your cloud-init template creates the `agent` user. This repo’s `cloud-config.yaml` does. Force it with `--template ./cloud-config.yaml`, or delete the cached template at `~/.dimensionshifter/cloud-init.tmpl.yaml` to let `ds` reseed it from the embedded default.
- Stale template: `ds` seeds a per-user template in `~/.dimensionshifter/cloud-init.tmpl.yaml` and reuses it. If you edited it in the past, it may override the repo version. Use `--template` to point at a known-good template for a run, or remove the stale file.
