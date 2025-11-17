# Example: What Tracee Captured

## Test Commands Run
```bash
cd /home/agent/workspace
echo "Hello from agent" > demo.txt
cat demo.txt
ls -la
mv demo.txt final.txt
cat final.txt
```

## Raw Tracee Events Captured

### Event Timeline (User-Friendly View)
```
06:29:35 [bash] sched_process_exec: bash -c cd /home/agent/workspace && echo "Hello from agent" > demo.txt && cat demo.txt && ls -la && mv demo.txt final.txt && cat final.txt
06:29:35 [cat] sched_process_exec: cat demo.txt
06:29:35 [ls] sched_process_exec: ls -la
06:29:35 [mv] sched_process_exec: mv demo.txt final.txt
06:29:35 [mv] security_inode_rename: /home/agent/workspace/demo.txt -> /home/agent/workspace/final.txt
06:29:35 [cat] sched_process_exec: cat final.txt
```

## Full Event Detail Example

Here's the complete `security_inode_rename` event that tracee captured for `mv demo.txt final.txt`:

```json
{
  "timestamp": 1763360975462329841,
  "processId": 24366,
  "userId": 1001,
  "processName": "mv",
  "executable": {
    "path": "/usr/bin/mv"
  },
  "eventName": "security_inode_rename",
  "syscall": "renameat2",
  "returnValue": 0,
  "args": [
    {
      "name": "old_path",        ← KEY FINDING: It's "old_path", NOT "old_name"
      "type": "const char*",
      "value": "/home/agent/workspace/demo.txt"
    },
    {
      "name": "new_path",        ← KEY FINDING: It's "new_path", NOT "new_name"
      "type": "const char*",
      "value": "/home/agent/workspace/final.txt"
    }
  ]
}
```

## The Bug in ghostd (log-filter/src/main.rs)

### Current Code (BROKEN - line 507-508)
```rust
fn transform_rename(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    let old_path = find_arg(raw, "old_name").or_else(|| find_arg(raw, "oldpath"));
    let new_path = find_arg(raw, "new_name").or_else(|| find_arg(raw, "newpath"));
    //                              ^^^^^^^^                    ^^^^^^^^
    //                              WRONG!                      WRONG!

    let old = old_path.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");
    let new = new_path.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");

    let description = format!("Renaming file from {} to {}", old, new);
    ...
}
```

**Result**: Since `old_name` and `new_name` don't exist in the args, both return `None`, so we get:
```
Renaming file from unknown to unknown
```

### Fixed Code
```rust
fn transform_rename(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    let old_path = find_arg(raw, "old_path").or_else(|| find_arg(raw, "oldpath"));
    let new_path = find_arg(raw, "new_path").or_else(|| find_arg(raw, "newpath"));
    //                              ^^^^^^^^                    ^^^^^^^^
    //                              CORRECT!                    CORRECT!

    let old = old_path.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");
    let new = new_path.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");

    let description = format!("Renaming file from {} to {}", old, new);
    ...
}
```

**Result**: Now it correctly finds the paths and outputs:
```
Renaming file from /home/agent/workspace/demo.txt to /home/agent/workspace/final.txt
```

## What We're NOT Capturing (But Should Be)

Notice what's **missing** from the events:

1. **File creation**: `echo "Hello from agent" > demo.txt`
   - No `openat` event (syscalls don't work in Docker)
   - Would need `openat` or `security_file_open` to capture this

2. **File reads**: `cat demo.txt` and `cat final.txt`
   - We see the `cat` process executed
   - But no `openat` event showing which file was opened
   - No `read` event showing data was read

3. **Directory listing**: `ls -la`
   - We see the `ls` process executed
   - But no `getdents64` events showing directory traversal
   - No `stat` events showing file metadata checks

## What a Complete Log Should Look Like

With all recommended events configured, we should capture:

```
06:29:35 [bash] Changed directory to /home/agent/workspace
06:29:35 [bash] Opening file demo.txt for write+create
06:29:35 [bash] Writing to file demo.txt
06:29:35 [bash] Closing file descriptor 3
06:29:35 [cat] Opening file demo.txt for read
06:29:35 [cat] Reading from file demo.txt
06:29:35 [cat] Closing file descriptor 3
06:29:35 [ls] Listing directory /home/agent/workspace
06:29:35 [ls] Checking file demo.txt
06:29:35 [mv] Renaming file from demo.txt to final.txt
06:29:35 [cat] Opening file final.txt for read
06:29:35 [cat] Reading from file final.txt
06:29:35 [cat] Closing file descriptor 3
```

## Events Currently Configured vs. What's Needed

### Currently in cloud-init.yaml
```yaml
events:
  - sched_process_exec    ✅ Working
  - execve                ✅ Working (backup)
  - exit_group            ✅ Working
  - exit                  ✅ Working
  - openat                ❌ Not captured (Docker syscall limitation)
  - close                 ❌ Not captured (Docker syscall limitation)
  - security_inode_rename ✅ Working (LSM hook)
```

### What Should Be Added
```yaml
events:
  # ... existing events ...

  # File system traversal (MISSING)
  - getdents64            # Directory listing
  - getdents              # Directory listing (legacy)

  # File metadata (MISSING)
  - stat                  # File existence, ls -l
  - lstat                 # File metadata (no symlink follow)
  - fstat                 # File metadata by fd
  - statx                 # Extended file metadata
  - newfstatat            # File status at path

  # Directory navigation (MISSING)
  - chdir                 # Track cd commands
  - fchdir                # Track directory changes by fd

  # Optional: File I/O (VERY VERBOSE)
  - read                  # File read operations
  - write                 # File write operations
```

## Docker Limitation Note

In this Docker test, syscall events (`openat`, `read`, `write`, `close`) don't work due to:
```
{"level":"warn","msg":"Event canceled because of missing kernel symbol dependency",
 "missing symbols":["sys_call_table"],"event":"syscall_table_check"}
```

This is a **Docker-specific limitation**. In a real VM (Lima-based), all syscall events should work correctly.

LSM hooks (`security_inode_rename`, `security_file_open`) work in both Docker and VMs.

## Summary

✅ **What works**: Process execution and LSM hooks (rename, socket connect)
❌ **What's broken**: The log filter looks for wrong argument names (`old_name` instead of `old_path`)
⚠️ **What's missing**: Events for file opens, reads, writes, directory traversal, and file metadata checks
🐛 **Docker caveat**: Syscall events don't work in Docker, but will work in real VM

The fix is simple: change 2 lines in the log filter to use the correct argument names.
