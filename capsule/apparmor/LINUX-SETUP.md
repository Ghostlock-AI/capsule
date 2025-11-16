# AppArmor Setup for Linux VMs

This guide shows how to set up AppArmor restrictions on a Linux VM for the `agent` user.

## Quick Setup (Linux Only)

### 1. Install AppArmor

```bash
sudo apt update
sudo apt install -y apparmor apparmor-utils

# Verify it's running
sudo systemctl status apparmor
sudo aa-status
```

### 2. Create the AppArmor Profile

```bash
sudo vim /etc/apparmor.d/capsule-agent-workload
```

Paste the following profile:

```apparmor
#include <tunables/global>

/bin/bash {
  #include <abstractions/base>

  # Allowed capabilities
  capability chown,
  capability dac_override,
  capability dac_read_search,
  capability fowner,
  capability fsetid,
  capability setgid,
  capability setuid,

  # Explicitly denied capabilities
  deny capability sys_admin,
  deny capability net_admin,
  deny capability sys_ptrace,
  deny capability sys_module,

  # Allow reading bash configuration files
  /etc/bash.bashrc r,
  /etc/profile r,
  /etc/profile.d/** r,
  owner @{HOME}/.bashrc r,
  owner @{HOME}/.bash_profile r,
  owner @{HOME}/.bash_login r,
  owner @{HOME}/.profile r,
  owner @{HOME}/.bash_logout r,
  owner @{HOME}/.bash_history rw,

  # Explicitly denied file paths
  deny owner @{HOME}/.ssh/** rwklx,
  deny /etc/shadow rwklx,
  deny /etc/sudoers rwklx,
  deny /etc/sudoers.d/** rwklx,
  deny /root/** rwklx,
  deny /var/log/** rwklx,

  # Read and execute access to system binaries
  /usr/** rix,
  /lib/** rix,
  /lib64/** rix,
  /bin/** rix,
  /sbin/** rix,

  # Read-write access
  /workspace/** rw,
  /tmp/** rw,
  /var/log/capsule/** rw,

  # Network access
  network inet,
  network inet6,
  network unix,

  # Signal access
  signal,

  # Allow execution of shells and interpreters
  /bin/bash ix,
  /bin/dash ix,
  /bin/sh ix,
  /usr/bin/python3* ix,

  # Proc and sys access
  @{PROC}/ r,
  @{PROC}/@{pid}/** r,
  /sys/kernel/mm/transparent_hugepage/hpage_pmd_size r,
}
```

### 3. Load the Profile

```bash
# Load the profile into the kernel
sudo apparmor_parser -r /etc/apparmor.d/capsule-agent-workload

# Verify it loaded
sudo aa-status | grep capsule
# Should show: /bin/bash in enforce mode
```

### 4. Create Restricted Shell Wrapper

```bash
# Create the wrapper script
sudo tee /usr/local/bin/restricted-bash << 'EOF'
#!/bin/bash
# AppArmor-restricted bash wrapper
exec aa-exec -p /bin/bash -- /bin/bash "$@"
EOF

# Make it executable
sudo chmod +x /usr/local/bin/restricted-bash

# Add to valid shells list
echo /usr/local/bin/restricted-bash | sudo tee -a /etc/shells
```

### 5. Apply to Agent User

```bash
# Change agent's shell to the restricted wrapper
sudo usermod -s /usr/local/bin/restricted-bash agent

# Create workspace directory
sudo mkdir -p /workspace
sudo chmod 755 /workspace

# Verify the change
grep agent /etc/passwd
# Should show: agent:x:1000:1000:light magician:/home/agent:/usr/local/bin/restricted-bash
```

### 6. Test the Restrictions

```bash
# Login as agent
sudo -i -u agent

# Check AppArmor is active
cat /proc/self/attr/current
# Should show: /bin/bash (enforce)

# Test denied operations
cat /etc/shadow          # Permission denied ✓
ls /root                 # Permission denied ✓
ls ~/.ssh                # Permission denied ✓

# Test allowed operations
echo "test" > /workspace/file.txt   # Works ✓
cat /workspace/file.txt             # Works ✓
python3 --version                   # Works ✓
```

## What's Allowed vs Denied

### ✅ ALLOWED

- Read and execute system binaries (`/bin/**`, `/usr/**`, `/lib/**`)
- Read bash config files (`/etc/bash.bashrc`, `~/.bashrc`)
- Read/write to `/workspace/**` and `/tmp/**`
- Read/write to `/var/log/capsule/**`
- Read/write bash history (`~/.bash_history`)
- Execute Python, bash, and other interpreters
- Basic capabilities (chown, dac_override, setuid/setgid)
- Network access (inet, inet6, unix)

### ❌ DENIED

- Access `/etc/shadow`, `/etc/sudoers` (sensitive config)
- Access `/root/**` (root's home directory)
- Access `~/.ssh/**` (SSH keys)
- Access `/var/log/**` (except `/var/log/capsule/**`)
- Sensitive capabilities (sys_admin, net_admin, sys_ptrace, sys_module)

### 👤 Root User

Root user is **not restricted** and can access everything for system administration.

## Troubleshooting

### Profile not loading

```bash
# Check for syntax errors
sudo apparmor_parser --preprocess /etc/apparmor.d/capsule-agent-workload

# View errors in logs
sudo dmesg | grep -i apparmor | tail -20
```

### Permission denied on bash startup

Make sure bash config files are allowed in the profile:
- `/etc/bash.bashrc r,`
- `owner @{HOME}/.bashrc r,`

### Workspace not writable

```bash
# Fix permissions
sudo mkdir -p /workspace
sudo chmod 755 /workspace
```

### Check current profile

```bash
# See what profile is active
cat /proc/self/attr/current
```

## Customizing the Profile

1. Edit `/etc/apparmor.d/capsule-agent-workload`
2. Reload: `sudo apparmor_parser -r /etc/apparmor.d/capsule-agent-workload`
3. Test as agent user

## Removing Restrictions

```bash
# Change agent back to normal bash
sudo usermod -s /bin/bash agent

# Unload the profile
sudo apparmor_parser -R /etc/apparmor.d/capsule-agent-workload
```

## Notes

- **Linux only**: AppArmor is not available on macOS or Windows
- **Automatic activation**: The wrapper ensures the profile is always active when agent logs in
- **Root bypass**: Root user is intentionally not restricted for system administration
- **Profile enforcement**: Uses `sudo -i -u agent` for login to activate the profile properly
