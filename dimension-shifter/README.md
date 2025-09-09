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
sudo rm /usr/local/bin/ds
# (plus remove your config at ~/.dimensionshifter if desired)
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
```
