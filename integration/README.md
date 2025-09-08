# Agent + Capsule Integration

This directory contains container builds with
certain types of agents and capsule-runtime combos.

The point is to verify capsules ability to observe,
profile, and sandbox the execution of agents with
various architectures and toolsets.

### multipass VM setup

```bash
# start the multipass VM
multipass launch 24.04 --name capsule_sandbox --cpus 1 --mem 1G --disk 5G
# mount workspace
multipass mount . capsule_sandbox:/work
# start the VM
multipass start capsule_sandbox
# open a shell on the VM
multipass shell capsule_sandbox
# run a one off command
multipass exec agentbox -- ls -la /work
# install python and rust
sudo apt update
sudo apt install -y python3 python3-pip curl build-essential
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
# tear down when completed
multipass stop capsule_sandbox
# delete the VM
multipass delete capsule_sandbox
# perge deleted VM's to reclaim disk
multipass purge
```
