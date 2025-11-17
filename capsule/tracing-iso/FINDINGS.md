# Tracee File Operation Tracking - Findings

## Problem Summary

Current logs show:
```
06:57:45 [agent][bash][node][File] Renaming file from unknown to unknown
```

The "unknown to unknown" indicates the log filter (`ghostd`) is not correctly extracting file paths from tracee events.

## Root Cause Analysis

### Issue 1: Missing Syscall Events (Docker Environment Only)

In the Docker test environment, tracee cannot hook syscalls due to missing kernel symbol:
```
{"level":"warn","msg":"Event canceled because of missing kernel symbol dependency","missing symbols":["sys_call_table"],"event":"syscall_table_check"}
```

This means syscall-level events (`openat`, `read`, `write`, `close`, `getdents64`, etc.) are NOT captured in Docker.

**Impact**: Only LSM (Linux Security Module) hook events are captured:
- `security_inode_rename` ✅ (works)
- `security_file_open` ❓ (configured but not seen in test)
- `openat`, `read`, `write`, `close` ❌ (not captured in Docker)

**Note**: This is a Docker limitation. In a real VM (Lima-based), syscalls should work correctly.

### Issue 2: Incorrect Argument Name in Log Filter

The `ghostd` log filter in `log-filter/src/main.rs` has a bug in the `transform_rename()` function at line 507:

```rust
fn transform_rename(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    let old_path = find_arg(raw, "old_name").or_else(|| find_arg(raw, "oldpath"));
    let new_path = find_arg(raw, "new_name").or_else(|| find_arg(raw, "newpath"));

    let old = old_path.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");
    let new = new_path.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");
```

**Actual argument names from tracee** (confirmed via test):
```json
{
  "eventName": "security_inode_rename",
  "args": [
    {
      "name": "old_path",  // ← NOT "old_name"
      "type": "const char*",
      "value": "/home/agent/workspace/test.txt"
    },
    {
      "name": "new_path",  // ← NOT "new_name"
      "type": "const char*",
      "value": "/home/agent/workspace/renamed.txt"
    }
  ]
}
```

The filter looks for `old_name` and `new_name`, but tracee provides `old_path` and `new_path`.

## Testing Results

### Captured Events in Docker Test

1. **Process execution** (`sched_process_exec`) - ✅ Working
   - Includes full `argv` with file paths when applicable
   - Example: `["mv", "/home/agent/workspace/test.txt", "/home/agent/workspace/renamed.txt"]`

2. **File rename** (`security_inode_rename`) - ✅ Working (LSM hook)
   - Provides both old and new paths
   - Example: `old_path: "/home/agent/workspace/test.txt"`

3. **Network connections** (`security_socket_connect`) - ✅ Working (LSM hook)

4. **File operations** (`openat`, `read`, `write`, `close`) - ❌ Not captured (syscall limitation in Docker)

### What About File Reads/Writes?

In Docker, we cannot test syscall-based file operations due to kernel limitations. However, examining the VM configuration and existing code:

#### Current Configuration (`cloud-init.yaml`)
```yaml
events:
  - openat                # File open/create with flags
  - close                 # File/socket close
```

#### What's Missing for Full File System Tracking

To reconstruct "exactly where the user/agent went on the file system", we need:

1. **Directory traversal**: `getdents64`, `getdents` (currently NOT configured)
2. **File metadata access**: `stat`, `lstat`, `fstat`, `statx`, `newfstatat` (currently NOT configured)
3. **Directory navigation**: `chdir`, `fchdir` (currently NOT configured)
4. **File reads/writes**: `read`, `write`, `pread64`, `pwrite64` (currently NOT configured)

#### Why These Are Important

- **`getdents64`**: Captures directory listing (`ls`, file tree traversal)
- **`stat` family**: Captures file existence checks, `ls -l`, file discovery
- **`chdir`**: Tracks current working directory changes (`cd`)
- **`read`/`write`**: Tracks actual file content access (may be too verbose)

## Recommended Fixes

### Fix 1: Update `log-filter/src/main.rs` - `transform_rename()`

Change line 507-508 from:
```rust
let old_path = find_arg(raw, "old_name").or_else(|| find_arg(raw, "oldpath"));
let new_path = find_arg(raw, "new_name").or_else(|| find_arg(raw, "newpath"));
```

To:
```rust
let old_path = find_arg(raw, "old_path").or_else(|| find_arg(raw, "oldpath"));
let new_path = find_arg(raw, "new_path").or_else(|| find_arg(raw, "newpath"));
```

### Fix 2: Add File System Traversal Events to `cloud-init.yaml`

Add these events to the tracee configuration:
```yaml
events:
  # Existing events
  - sched_process_exec
  - execve
  - exit_group
  - exit
  - openat
  - close

  # ADD THESE for complete file system tracking:
  - getdents64            # Directory listing (ls, find, tree)
  - getdents              # Directory listing (legacy)
  - stat                  # File metadata (ls -l, file checks)
  - lstat                 # File metadata (don't follow symlinks)
  - fstat                 # File metadata by fd
  - statx                 # Extended file status
  - newfstatat            # File status at path
  - chdir                 # Track directory navigation
  - fchdir                # Track directory navigation by fd

  # Existing network/security events...
```

### Fix 3: Add Transformation Handlers to `log-filter/src/main.rs`

Add handlers for new events in the `transform_event()` match statement (around line 204):

```rust
match event_name.as_str() {
    "sched_process_exec" | "execve" => transform_exec(&raw)?,
    "sched_process_exit" | "exit_group" | "exit" => transform_exit(&raw)?,
    // ... existing handlers ...

    // ADD THESE:
    "getdents64" | "getdents" => transform_getdents(&raw)?,
    "stat" | "lstat" | "fstat" | "statx" | "newfstatat" => transform_stat(&raw)?,
    "chdir" | "fchdir" => transform_chdir(&raw)?,

    _ => (
        format!("Unhandled event: {}", raw.event_name),
        serde_json::json!({}),
    ),
}
```

And implement the handlers:

```rust
fn transform_getdents(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    let pathname = find_arg(raw, "pathname").or_else(|| find_arg(raw, "path"));
    let path = pathname.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");

    let description = format!("Listing directory {}", path);
    let details = serde_json::json!({
        "path": path,
    });

    Ok((description, details))
}

fn transform_stat(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    let pathname = find_arg(raw, "pathname").or_else(|| find_arg(raw, "path"));
    let path = pathname.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");

    let description = format!("Checking file {}", path);
    let details = serde_json::json!({
        "path": path,
    });

    Ok((description, details))
}

fn transform_chdir(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    let pathname = find_arg(raw, "pathname").or_else(|| find_arg(raw, "path"));
    let path = pathname.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");

    let description = format!("Changed directory to {}", path);
    let details = serde_json::json!({
        "path": path,
    });

    Ok((description, details))
}
```

## Testing in Real VM

Since Docker has syscall limitations, test in the actual Lima VM:

1. Deploy fixes to VM
2. Run test commands:
   ```bash
   # As agent user
   cd /home/agent/workspace
   ls -la /etc/
   find /tmp -name "*.txt"
   cat /etc/hosts
   echo "test" > test.txt
   mv test.txt renamed.txt
   ```

3. Check logs:
   ```bash
   capsule-vm logs <vm-name>
   ```

Expected output with fixes:
```
HH:MM:SS [agent][bash] Changed directory to /home/agent/workspace
HH:MM:SS [agent][ls] Listing directory /etc
HH:MM:SS [agent][find] Listing directory /tmp
HH:MM:SS [agent][cat] Opening file /etc/hosts for read
HH:MM:SS [agent][bash] Opening file test.txt for write+create
HH:MM:SS [agent][mv] Renaming file from test.txt to renamed.txt
```

## Performance Considerations

Adding `getdents64`, `stat`, and related events will increase log volume significantly, as these are called frequently:
- `ls` triggers multiple `getdents64` + `stat` per file
- File tree traversal can generate thousands of events
- `stat` is called for existence checks, autocomplete, etc.

**Mitigation**:
- The `ghostd` filter already reduces event size by 90% (1905 bytes → 199 bytes)
- Consider log rotation and retention policies
- Monitor `/var/log/tracee/` disk usage

## Conclusion

### Immediate Actions Required

1. **Fix the rename bug** in `log-filter/src/main.rs:507-508`
   - Change `old_name` → `old_path`
   - Change `new_name` → `new_path`

2. **Add file system traversal events** to tracee config
   - `getdents64`, `getdents` for directory listing
   - `stat`, `lstat`, `fstat`, `statx`, `newfstatat` for file metadata
   - `chdir`, `fchdir` for directory navigation

3. **Add transformation handlers** in log filter
   - Implement `transform_getdents()`
   - Implement `transform_stat()`
   - Implement `transform_chdir()`

### Long-term Considerations

- **Log volume**: Monitor disk usage with expanded event set
- **Event filtering**: Consider filtering noisy paths (e.g., `/dev/`, `/proc/`, `/sys/`)
- **Performance impact**: eBPF overhead is minimal, but log I/O may be significant
- **Alternative approach**: Consider using `security_file_open` LSM hook instead of `openat` syscall for file access (may have less overhead)

## Docker vs VM Behavior

**Docker limitation**: Cannot test syscall events due to missing `sys_call_table` kernel symbol
**VM (Lima)**: Full syscall support expected, should capture all configured events

The LSM hooks (`security_inode_rename`, `security_file_open`, etc.) work in both environments and may provide better compatibility across different kernel configurations.
