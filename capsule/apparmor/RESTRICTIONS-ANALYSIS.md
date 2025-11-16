# AppArmor Restrictions Analysis for AI Coding Agent

## Current Restrictions

### File System Access

**✅ Currently Allowed:**
- Read/execute system binaries: `/bin/**`, `/usr/**`, `/lib/**`, `/sbin/**`
- Read bash config: `/etc/bash.bashrc`, `~/.bashrc`, etc.
- Read/write workspace: `/workspace/**`
- Read/write temp: `/tmp/**`
- Read/write logs: `/var/log/capsule/**`
- Read/write bash history: `~/.bash_history`

**❌ Currently Denied:**
- SSH keys: `~/.ssh/**`
- System configs: `/etc/shadow`, `/etc/sudoers`
- Root directory: `/root/**`
- System logs: `/var/log/**` (except capsule)

### Capabilities

**✅ Currently Allowed:**
- `chown` - Change file ownership
- `dac_override` - Bypass file read/write/execute permission checks
- `dac_read_search` - Bypass file/directory read permission checks
- `fowner` - Bypass permission checks on operations that require file ownership
- `fsetid` - Don't clear setuid/setgid bits when file is modified
- `setgid` - Manipulate process GIDs
- `setuid` - Manipulate process UIDs

**❌ Currently Denied:**
- `sys_admin` - System administration operations
- `net_admin` - Network administration
- `sys_ptrace` - Trace arbitrary processes
- `sys_module` - Load/unload kernel modules
- `sys_rawio` - Raw I/O operations
- `sys_boot` - Reboot the system
- `sys_time` - Set system time
- `mac_admin` - Override MAC (AppArmor/SELinux)
- `mac_override` - Override MAC access

### Network Access

**✅ Currently Allowed:**
- `inet` - IPv4 networking
- `inet6` - IPv6 networking
- `unix` - Unix domain sockets

### Signals

**✅ Currently Allowed:**
- All signal operations

---

## Issues with Current Configuration for AI Agent

### 🔴 CRITICAL ISSUES

1. **Too Much Read Access**
   - Agent can read **entire filesystem** via `/usr/**`, `/lib/**`, `/bin/**`
   - Can potentially find sensitive data in `/usr/local`, `/usr/share`, etc.
   - **Risk**: Data exfiltration, reading other users' code in `/usr/local/src`

2. **Can Execute Arbitrary Binaries**
   - `rix` permission on `/usr/**` allows running ANY installed program
   - Can run `curl`, `wget`, `ssh`, `git`, etc.
   - **Risk**: Can exfiltrate data, connect to arbitrary servers

3. **Dangerous Capabilities**
   - `dac_override` - Can bypass ALL file permission checks
   - `setuid`/`setgid` - Can change to other users (if setuid binaries exist)
   - **Risk**: Privilege escalation, impersonation

4. **No Process Restrictions**
   - Can fork unlimited child processes
   - Can create background daemons/servers
   - No restrictions on process creation
   - **Risk**: Resource exhaustion, persistent backdoors

5. **Full `/tmp` Access**
   - Can read other users' temp files
   - Can write to shared temp space
   - **Risk**: Information leakage, temp file attacks

---

## Recommended Restrictions for AI Coding Agent

### Filesystem: Principle of Least Privilege

#### ✅ Should Allow:

**Workspace (Full Access)**
```yaml
read_write:
  - /workspace/**
```

**Python & Standard Tools (Execute Only)**
```yaml
read_execute:
  - /usr/bin/python3*
  - /usr/bin/pip3*
  - /usr/bin/git
  - /usr/bin/curl
  - /usr/bin/wget
  - /usr/bin/vim
  - /usr/bin/nano
  - /usr/bin/ls
  - /usr/bin/cat
  - /usr/bin/grep
  - /usr/bin/find
  - /usr/bin/sed
  - /usr/bin/awk
  - /bin/bash
  - /bin/sh
```

**Python Libraries (Read for imports)**
```yaml
read_only:
  - /usr/lib/python3*/**
  - /usr/local/lib/python3*/**
  - owner @{HOME}/.local/lib/python3*/**
```

**Package Installation (Limited)**
```yaml
read_write:
  - owner @{HOME}/.local/**  # pip install --user
  - /workspace/.venv/**       # Virtual environments
```

**Isolated Temp**
```yaml
read_write:
  - /tmp/agent-{pid}/**       # Per-process temp only
  - owner @{HOME}/.cache/**   # User cache
```

#### ❌ Should Deny:

**Sensitive System Files**
```yaml
deny:
  - /etc/**                    # All system config
  - /root/**                   # Root's home
  - /home/*/                   # Other users' homes
  - /var/**                    # System state (except logs)
  - /usr/local/src/**          # Other users' source code
  - /opt/**                    # Optional software
  - /srv/**                    # Service data
  - owner @{HOME}/.ssh/**      # SSH keys
  - owner @{HOME}/.gnupg/**    # GPG keys
  - owner @{HOME}/.aws/**      # AWS credentials
  - owner @{HOME}/.config/**   # User config (may have tokens)
```

**Dangerous Binaries**
```yaml
deny:
  - /usr/bin/sudo rwklx
  - /usr/bin/su rwklx
  - /usr/bin/passwd rwklx
  - /usr/bin/chsh rwklx
  - /usr/bin/docker rwklx
  - /usr/bin/systemctl rwklx
```

### Capabilities: Minimal Set

#### ✅ Allow (Only What's Needed):

```yaml
allow:
  # File operations (but limit dac_override if possible)
  - fowner
  - fsetid

  # For package installation
  - chown
```

#### ❌ Deny (Everything Else):

```yaml
deny:
  - dac_override        # NO bypassing permissions
  - dac_read_search     # NO bypassing read permissions
  - setuid              # NO changing UIDs
  - setgid              # NO changing GIDs
  - sys_admin           # NO system admin
  - net_admin           # NO network admin
  - sys_ptrace          # NO debugging other processes
  - sys_module          # NO kernel modules
  - sys_rawio           # NO raw I/O
  - sys_chroot          # NO chroot
  - sys_nice            # NO priority manipulation
  - sys_resource        # NO resource limit changes
```

### Process Control

**Prevent Daemon/Background Processes:**

AppArmor doesn't directly prevent forking, but we can:

1. **Limit child process execution** with `ix` (inherit profile):
   ```apparmor
   /usr/bin/python3* ix,  # Child inherits same restrictions
   ```

2. **Deny daemon-like binaries**:
   ```apparmor
   deny /usr/sbin/sshd rwklx,
   deny /usr/bin/screen rwklx,
   deny /usr/bin/tmux rwklx,
   deny /usr/bin/nohup rwklx,
   deny /usr/bin/disown rwklx,
   ```

3. **Use seccomp for fork/exec limits** (complementary):
   - Limit max processes with `RLIMIT_NPROC`
   - Deny `clone()` with CLONE_NEWPID (namespace creation)

### Network: Restrict Outbound Connections

**Allow (for internet search, package installation):**
```yaml
network:
  allow:
    - inet  tcp   # HTTP/HTTPS
    - inet6 tcp
```

**Deny (prevent server creation):**
```yaml
network:
  deny:
    - inet  tcp   bind   # NO TCP servers
    - inet  udp   bind   # NO UDP servers
```

*Note: AppArmor network rules are limited. Consider using network namespaces or firewall rules for better control.*

---

## Additional Restrictions to Consider

### 1. **Mount Operations**
```yaml
deny:
  - mount
  - umount
```
Prevents mounting new filesystems or unmounting existing ones.

### 2. **IPC (Inter-Process Communication)**
```yaml
deny:
  - dbus          # Prevent D-Bus access
  - signal send   # Prevent signaling other processes
```
Limits communication with other processes.

### 3. **Ptrace (Process Tracing)**
```yaml
deny:
  - ptrace        # Prevent debugging other processes
```
Already covered by denying `sys_ptrace` capability.

### 4. **File Locking**
```yaml
# Allow for normal file operations
file:
  - /workspace/** rwlk

# But deny locking system files
deny:
  - /etc/** rwklx
```

### 5. **Execution Transitions**

Prevent transitioning to unconfined mode:
```apparmor
# Force all children to inherit profile
/usr/bin/python3* ix,
/usr/bin/* ix,

# Never allow unconfined execution
deny /** ux,
```

### 6. **File Permissions (umask)**

AppArmor doesn't control umask, but you can set it in the shell wrapper:
```bash
#!/bin/bash
umask 027  # Files: 640, Dirs: 750
exec aa-exec -p /bin/bash -- /bin/bash "$@"
```

---

## Complementary Technologies

AppArmor alone isn't enough. Consider combining with:

### 1. **Seccomp-BPF (System Call Filtering)**

More fine-grained than AppArmor for:
- Limiting process creation (`clone`, `fork`)
- Preventing privilege escalation (`setuid`, `setgid`, `capset`)
- Blocking dangerous syscalls (`ptrace`, `reboot`, `kexec_load`)

### 2. **Namespaces (Isolation)**

- **PID namespace**: Process isolation
- **Network namespace**: Network isolation
- **Mount namespace**: Filesystem isolation
- **User namespace**: UID/GID isolation

### 3. **Cgroups (Resource Limits)**

- CPU limits
- Memory limits
- Process count limits
- I/O bandwidth limits

### 4. **Firewall Rules (iptables/nftables)**

- Block outbound connections to specific IPs/ports
- Allow only HTTP/HTTPS to package repositories
- Block all inbound connections

---

## Recommended Profile for AI Coding Agent

See `profile-config-strict.yaml` for a hardened configuration suitable for AI agents.

### Key Principles:

1. **Workspace-centric**: All work happens in `/workspace/`
2. **Read-only system**: Can't modify system files
3. **Explicit tool allowlist**: Only approved tools can execute
4. **No privilege escalation**: Minimal capabilities
5. **Network limited**: Outbound only, no servers
6. **No daemon creation**: All processes inherit restrictions
7. **No filesystem traversal**: Can't browse outside workspace

### Trade-offs:

- ✅ **Security**: Strong isolation, minimal attack surface
- ❌ **Flexibility**: Can't install system packages, limited tool access
- ⚖️ **Usability**: May need to add tools to allowlist over time

---

## Implementation Steps

1. **Start with strict profile** (`profile-config-strict.yaml`)
2. **Test with real AI agent workloads**
3. **Monitor AppArmor denials**: `sudo aa-logprof` or `dmesg | grep apparmor`
4. **Add necessary permissions** one-by-one to allowlist
5. **Document each permission** and why it's needed
6. **Never use `dac_override`** - it defeats the purpose
7. **Combine with seccomp** for syscall filtering
8. **Use network namespaces** for network isolation

---

## Questions to Answer

1. **Should agent be able to install packages?**
   - If yes: Allow `pip install --user` (writes to `~/.local`)
   - If no: Deny all writes outside `/workspace`

2. **Should agent be able to run servers?**
   - If yes: Allow `bind()` syscall (but limit ports)
   - If no: Deny with AppArmor + seccomp + firewall

3. **Should agent be able to access internet?**
   - If yes: Allow outbound TCP (but monitor)
   - If no: Use network namespace with no connectivity

4. **Should agent be able to fork processes?**
   - Probably yes (for `subprocess.run()` in Python)
   - But limit with cgroups (`pids.max`)

5. **Should agent be able to read system libraries?**
   - Probably yes (for Python imports)
   - But use `r` (read-only), never `w` or `x`
