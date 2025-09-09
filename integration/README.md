# Agent + Capsule Integration

This directory contains container builds with
certain types of agents and capsule-runtime combos.

The point is to verify capsules ability to observe,
profile, and sandbox the execution of agents with
various architectures and toolsets.

### multipass VM setup

```bash
# start the multipass VM
multipass launch 24.04 --name capsule-sandbox --cpus 1 --memory 1G --disk 5G
# mount workspace
multipass mount . capsule-sandbox:/work
# start the VM
multipass start capsule-sandbox
# open a shell on the VM
multipass shell capsule-sandbox
# run a one off command
multipass exec capsule-sandbox -- ls -la /work
# install python and rust
sudo apt update
sudo apt install -y python3 python3-pip curl build-essential
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
# tear down when completed
multipass stop capsule-sandbox
# delete the VM
multipass delete capsule-sandbox
# perge deleted VM's to reclaim disk
multipass purge
```
