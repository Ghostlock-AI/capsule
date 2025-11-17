# Tracee Isolation Testing Environment

This directory contains a Docker-based isolated environment for debugging tracee event capture, specifically for file operations.

## Problem Being Debugged

Currently seeing incomplete file operation logs:
```
06:57:45 [agent][bash][node][File] Renaming file from unknown to unknown
```

The "unknown to unknown" indicates we're not capturing file path information correctly. We're missing:
- File reads
- File writes
- File system crawling/directory traversal

## Goal

Capture complete file system activity to reconstruct exactly where the user/agent navigated on the file system.

## Configuration

This environment matches the VM orchestrator setup but with expanded event tracking:
- All file operations: `openat`, `open`, `close`, `read`, `write`, `pread64`, `pwrite64`, `readv`, `writev`
- Directory operations: `getdents64`, `getdents` (for directory crawling)
- File metadata: `stat`, `lstat`, `fstat`, `statx`, `newfstatat`
- Navigation: `chdir`, `fchdir`
- LSM hooks: `security_file_open`, `security_inode_rename`, `security_inode_unlink`

## Usage

### 1. Build the container
```bash
cd tracing-iso
docker build -t tracee-debug .
```

### 2. Run the container (privileged for eBPF)
```bash
docker run -it --rm \
  --privileged \
  --pid=host \
  --name tracee-test \
  tracee-debug
```

### 3. In another terminal, exec into container as agent user
```bash
docker exec -it -u agent tracee-test bash
```

### 4. Generate file operations (as agent user)
```bash
# Test 1: File reads
cat /etc/hosts
cat /etc/passwd

# Test 2: File writes
echo "test" > /home/agent/workspace/test.txt
echo "more" >> /home/agent/workspace/test.txt

# Test 3: Directory crawling
ls -la /etc/
ls -la /home/agent/
find /home/agent/workspace -type f

# Test 4: Directory navigation
cd /tmp
pwd
cd /home/agent/workspace
pwd

# Test 5: File metadata
stat /etc/hosts
file /etc/passwd

# Test 6: File renaming
mv /home/agent/workspace/test.txt /home/agent/workspace/renamed.txt
```

### 5. View raw tracee events
```bash
docker exec -it tracee-test tail -f /var/log/tracee/events.jsonl | jq .
```

Or filter to specific event types:
```bash
# File operations only
docker exec -it tracee-test tail -f /var/log/tracee/events.jsonl | jq 'select(.eventName | contains("open") or contains("read") or contains("write"))'

# Directory operations only
docker exec -it tracee-test tail -f /var/log/tracee/events.jsonl | jq 'select(.eventName | contains("getdents") or contains("chdir"))'

# File metadata operations
docker exec -it tracee-test tail -f /var/log/tracee/events.jsonl | jq 'select(.eventName | contains("stat"))'
```

### 6. Check tracee daemon status
```bash
docker exec -it tracee-test tail -f /var/log/tracee/tracee.log
```

## Analysis

After running tests, analyze:

1. **What events are captured?**
   - Are we seeing `openat` events with pathname arguments?
   - Are we seeing `getdents64` for directory listing?
   - Are we seeing `read`/`write` with file descriptors?

2. **What information is in the events?**
   - Check `args` array for pathname, flags, fd
   - Verify `parse-arguments-fds: true` is resolving file descriptors to paths
   - Check if LSM hooks provide better path information than syscalls

3. **What's missing?**
   - Compare expected operations vs. captured events
   - Identify which syscalls/events provide the most useful information

## Expected Outcome

Should be able to determine:
1. Which tracee events give us the best file operation visibility
2. Whether we need additional events or different event sources (LSM vs syscall)
3. How to update the ghostd log filter to correctly parse file paths from the captured events
4. Why we're seeing "unknown to unknown" in the current setup

## Next Steps

Based on findings:
1. Update tracee configuration in `cloud-init.yaml` with necessary events
2. Update `log-filter/src/main.rs` transformation functions to correctly extract file paths
3. Test in full VM environment
