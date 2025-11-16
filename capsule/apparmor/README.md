# AppArmor Isolation Container

This directory contains a Docker-based AppArmor security profile for isolating agent workloads.

## Overview

The container provides:
- **Ubuntu 20.04** base image
- **AppArmor** security profiles configured via YAML
- **Python 3** for testing and scripting
- Automatic profile enforcement on container start
- Configurable file access, capabilities, and network rules

## Files

- `Dockerfile` - Container image definition
- `profile-config.yaml` - AppArmor profile configuration (easy to customize)
- `generate-profile.py` - Python script to generate AppArmor profile from YAML
- `entrypoint.sh` - Startup script that loads and enforces the profile
- `test-apparmor.py` - Test script to verify all restrictions work correctly
- `README.md` - This file

## Quick Start

### Build the image

```bash
cd isolation
docker build -t apparmor-isolation .
```

### Run with AppArmor enforcement

To run with the custom AppArmor profile enforced:

```bash
docker run -it --rm \
  --security-opt apparmor=capsule-agent-workload \
  apparmor-isolation
```

**Note:** The custom profile needs to be loaded on the host system for full enforcement. See "Loading Profile on Host" below.

### Run as root (for testing/debugging)

```bash
docker run -it --rm apparmor-isolation
```

### Run the test suite

Inside the container:

```bash
python3 /workspace/test-apparmor.py
```

## Profile Configuration

The security profile is defined in `profile-config.yaml`. You can customize:

### Allowed Read-Only Paths
```yaml
file_rules:
  read_only:
    - /usr/**
    - /lib/**
    - /bin/**
```

### Allowed Read-Write Paths
```yaml
file_rules:
  read_write:
    - /workspace/**
    - /tmp/**
```

### Denied Paths
```yaml
file_rules:
  deny:
    - owner @{HOME}/.ssh/**
    - /etc/**
    - /root/**
```

### Capabilities
```yaml
capabilities:
  allow:
    - chown
    - dac_override
  deny:
    - sys_admin
    - net_admin
    - sys_ptrace
```

## Default Security Posture

### Allowed Operations
- ✅ Read system binaries (`/usr/**`, `/lib/**`, `/bin/**`)
- ✅ Read/write to `/workspace/**` and `/tmp/**`
- ✅ Read/write to `/var/log/capsule/**`
- ✅ Execute Python, bash, and other interpreters
- ✅ Basic capabilities (chown, dac_override, setuid/setgid)
- ✅ Network access (inet, inet6, unix)

### Denied Operations
- ❌ Access to `/etc/**` (configuration files)
- ❌ Access to `/root/**` (root home directory)
- ❌ Access to `~/.ssh/**` (SSH keys)
- ❌ Access to `/var/log/**` (except `/var/log/capsule/**`)
- ❌ Sensitive capabilities (sys_admin, net_admin, sys_ptrace)

## Loading Profile on Host

For full AppArmor enforcement, load the profile on your Docker host:

```bash
# Copy the generated profile from the container
docker run --rm apparmor-isolation cat /etc/apparmor.d/capsule-agent-workload > /tmp/capsule-agent-workload

# Load it on the host
sudo cp /tmp/capsule-agent-workload /etc/apparmor.d/
sudo apparmor_parser -r -W /etc/apparmor.d/capsule-agent-workload

# Verify it's loaded
sudo aa-status | grep capsule
```

Then run the container with the profile:

```bash
docker run -it --rm \
  --security-opt apparmor=capsule-agent-workload \
  apparmor-isolation
```

## Testing

The test script (`test-apparmor.py`) verifies:

1. ✅ Read access to system binaries
2. ✅ Read/write access to workspace
3. ✅ Read/write access to /tmp
4. ❌ Denied access to /etc
5. ❌ Denied access to /root
6. ❌ Denied access to .ssh
7. ✅ Python execution works
8. ❌ Denied access to /var/log (except /var/log/capsule)
9. ✅ Default working directory is /workspace

### Run tests as non-root user

To properly test AppArmor restrictions, create a non-root user:

```bash
docker run -it --rm \
  --security-opt apparmor=capsule-agent-workload \
  apparmor-isolation bash -c "useradd -m testuser && su - testuser -c 'cd /workspace && python3 test-apparmor.py'"
```

### Root Access

Root users can bypass AppArmor restrictions for debugging and log access:

```bash
# As root, view AppArmor logs
docker run -it --rm apparmor-isolation bash -c "
  # AppArmor logs (if available)
  cat /var/log/apparmor/* 2>/dev/null || echo 'No AppArmor logs found'

  # Or check kernel messages
  dmesg | grep -i apparmor || echo 'No AppArmor kernel messages'
"
```

## Customization

1. Edit `profile-config.yaml` to change security rules
2. Rebuild the Docker image
3. Test with `test-apparmor.py`

## Troubleshooting

### Profile not enforced

If the profile isn't being enforced, check:

1. Is AppArmor enabled on the host? `sudo aa-status`
2. Is the profile loaded? `sudo aa-status | grep capsule`
3. Are you running with `--security-opt apparmor=capsule-agent-workload`?

### Permission denied errors

This is expected! The profile is working. If you need to allow additional paths:

1. Edit `profile-config.yaml`
2. Add paths to `read_only` or `read_write` sections
3. Rebuild the image

### Testing shows failures

Some tests may fail if:
- Running as root (AppArmor doesn't restrict root as much)
- Profile not loaded on host
- Files don't exist in the container

## Architecture Notes

- AppArmor profiles are path-based (unlike SELinux which is label-based)
- The profile is generated at build time from YAML
- The entrypoint script attempts to load it at runtime
- Full enforcement requires the profile on the host system
- Root users can still access restricted paths for debugging
