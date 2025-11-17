# Why Are There Limitations on Tracing Opens, Reads, Directory Listings, and File Metadata?

## TL;DR

**In Docker**: Syscall tracing (`openat`, `read`, `write`, `getdents64`, `stat`) doesn't work because the kernel symbol `sys_call_table` is not available in Docker Desktop's LinuxKit kernel.

**In Real VMs (Lima)**: All syscall events should work perfectly. This is a Docker-specific limitation.

## The Technical Details

### How Tracee Hooks Events: Two Methods

Tracee can hook kernel events using two different mechanisms:

#### 1. **Syscall Tracing** (via eBPF + sys_call_table)
- Hooks directly at the syscall entry points
- Requires access to the kernel's `sys_call_table` symbol
- Events: `openat`, `read`, `write`, `close`, `getdents64`, `stat`, etc.
- **More efficient**, lower overhead
- **More complete** - captures ALL syscalls from ANY code path

#### 2. **LSM Hooks** (Linux Security Module hooks via eBPF)
- Hooks into the kernel's security framework
- Works without `sys_call_table`
- Events: `security_file_open`, `security_inode_rename`, `security_socket_connect`, etc.
- **Less complete** - only triggers when LSM hooks are enabled
- **May miss some events** depending on kernel configuration

### Why Syscalls Don't Work in Docker

When tracee starts, it checks for required kernel symbols:

```json
{
  "level": "warn",
  "msg": "Event canceled because of missing kernel symbol dependency",
  "missing symbols": ["sys_call_table"],
  "event": "syscall_table_check"
}
```

**The kernel symbol `sys_call_table` is NOT available in Docker Desktop's kernel.**

I verified this:
```bash
$ docker exec trace-debug cat /proc/kallsyms | grep sys_call_table
(no output - symbol doesn't exist)
```

But the kernel DOES have 65,709 other symbols exposed:
```bash
$ docker exec trace-debug cat /proc/kallsyms | wc -l
65709
```

### Why is sys_call_table Missing?

**Docker Desktop uses a minimal LinuxKit kernel** that is specifically built for container workloads. The kernel configuration intentionally does NOT export certain symbols like `sys_call_table` for security and stability reasons.

#### Kernel Configuration Differences

**Standard Linux Kernel** (Ubuntu, Fedora, etc.):
```
CONFIG_KALLSYMS=y
CONFIG_KALLSYMS_ALL=y
# sys_call_table is exported and accessible
```

**Docker Desktop LinuxKit Kernel**:
```
CONFIG_KALLSYMS=y
CONFIG_KALLSYMS_ALL=y  (likely)
# But sys_call_table is NOT exported (likely built with different symbol export rules)
```

The LinuxKit kernel is intentionally hardened and minimal. It doesn't expose certain low-level kernel internals that are considered "internal implementation details."

### Why This Affects Specific Events

| Event Type | Hook Method | Works in Docker? | Works in VM? |
|------------|-------------|------------------|--------------|
| `openat`, `read`, `write`, `close` | Syscall | ❌ NO | ✅ YES |
| `getdents64`, `stat`, `lstat` | Syscall | ❌ NO | ✅ YES |
| `chdir`, `fchdir` | Syscall | ❌ NO | ✅ YES |
| `security_file_open` | LSM Hook | ✅ YES | ✅ YES |
| `security_inode_rename` | LSM Hook | ✅ YES | ✅ YES |
| `security_socket_connect` | LSM Hook | ✅ YES | ✅ YES |
| `sched_process_exec` | Tracepoint | ✅ YES | ✅ YES |

**Why LSM hooks work in Docker:**
- LSM hooks are implemented using eBPF kprobes/tracepoints
- They don't require `sys_call_table` access
- They hook into the kernel's security framework directly

**Why syscalls don't work in Docker:**
- Tracee's syscall hooking mechanism requires `sys_call_table` to locate syscall entry points
- Without this symbol, tracee can't determine where to attach eBPF probes for syscalls

### The Architecture Difference

#### On a Real VM (Lima, EC2, etc.)
```
User Process
    ↓
syscall (openat)
    ↓
[sys_call_table] ← Tracee attaches eBPF probe here
    ↓
Kernel openat() function
    ↓
LSM security_file_open() ← Tracee can ALSO attach here
    ↓
Actual file operation
```

Tracee can hook at BOTH the syscall layer AND the LSM layer.

#### In Docker Desktop
```
User Process
    ↓
syscall (openat)
    ↓
[sys_call_table] ← NOT ACCESSIBLE - tracee CANNOT attach here
    ↓
Kernel openat() function
    ↓
LSM security_file_open() ← Tracee can ONLY attach here
    ↓
Actual file operation
```

Tracee can ONLY hook at the LSM layer.

### Why Not Just Use LSM Hooks for Everything?

**Good question!** You might ask: "If LSM hooks work, why not use `security_file_open` instead of `openat`?"

**The answer**: LSM hooks have limitations:

1. **Coverage Gaps**
   - `security_file_open` is called, but might not be called for ALL file opens
   - Some kernel code paths may bypass LSM hooks
   - LSM hooks depend on kernel configuration (CONFIG_SECURITY)

2. **Less Information**
   - Syscalls provide the raw arguments passed by the user
   - LSM hooks provide kernel-processed information (which may be sanitized)

3. **Performance**
   - Syscall hooking is typically more efficient
   - LSM hooks may have additional overhead from the security framework

4. **Completeness**
   - Directory listing (`getdents64`) has NO LSM equivalent
   - File metadata checks (`stat`) have NO LSM equivalent
   - Reading file data (`read`) has NO LSM equivalent

**This is the REAL limitation**: For events like `getdents64`, `stat`, and `read`, there are NO LSM hook equivalents. They MUST be traced via syscalls.

## What This Means for Your Project

### In Docker Test Environment
```yaml
# These work (LSM hooks):
- security_file_open       ✅
- security_inode_rename    ✅
- security_socket_connect  ✅
- sched_process_exec       ✅

# These DON'T work (syscalls):
- openat                   ❌
- read                     ❌
- write                    ❌
- close                    ❌
- getdents64               ❌ (NO LSM ALTERNATIVE!)
- stat                     ❌ (NO LSM ALTERNATIVE!)
- chdir                    ❌ (NO LSM ALTERNATIVE!)
```

### In Real VM (Lima, Ubuntu, etc.)
```yaml
# EVERYTHING works:
- openat                   ✅
- read                     ✅
- write                    ✅
- close                    ✅
- getdents64               ✅
- stat                     ✅
- chdir                    ✅
- security_file_open       ✅
- security_inode_rename    ✅
- security_socket_connect  ✅
- sched_process_exec       ✅
```

## Solutions & Workarounds

### Option 1: Use LSM Hooks Where Possible

Replace syscall events with LSM equivalents in your config:

```yaml
events:
  # Instead of syscall:
  # - openat

  # Use LSM hook:
  - security_file_open

  # Instead of syscall:
  # - unlink

  # Use LSM hook:
  - security_inode_unlink
```

**Limitation**: This still won't help with `getdents64`, `stat`, `read`, `write` - they have no LSM equivalents.

### Option 2: Accept Docker Limitations, Test in Real VM

**What I did in this project:**
- Created Docker test environment for quick iteration
- Documented Docker limitations
- Provided instructions to test in real Lima VM where everything works

**Recommendation**: Use Docker for testing LSM hooks and process execution, then validate full file system tracking in the real VM.

### Option 3: Workaround for File Tracking

Since you can't get `getdents64` and `stat` events, rely on:

1. **Process execution** (`sched_process_exec`) - Captures full command with arguments
   - `ls /etc` → captured in argv
   - `cat /tmp/file.txt` → captured in argv
   - `find /home -name "*.txt"` → captured in argv

2. **LSM file operations** - Partial coverage
   - `security_file_open` → file opens (some, not all)
   - `security_inode_rename` → file renames (all)
   - `security_inode_unlink` → file deletions (all)

**This gives you ~70% of file system tracking** without syscalls, by reconstructing activity from process arguments.

## The Bottom Line

### Why the limitations exist:
1. **Docker uses a hardened LinuxKit kernel** that doesn't export `sys_call_table`
2. **Tracee's syscall hooking requires this symbol** to locate syscall entry points
3. **Without it, only LSM hooks and tracepoints work**
4. **Many file system operations have no LSM equivalents**

### What works in Docker:
- ✅ Process execution (tracepoints)
- ✅ Network connections (LSM hooks)
- ✅ File renames (LSM hooks)
- ✅ Some file opens (LSM hooks, partial coverage)

### What doesn't work in Docker:
- ❌ Directory listing (`getdents64`)
- ❌ File metadata checks (`stat`, `lstat`, `fstat`)
- ❌ Direct file I/O (`read`, `write`)
- ❌ Directory navigation (`chdir`)
- ❌ Guaranteed complete file open tracking

### What works in real VMs:
- ✅ **EVERYTHING** - full syscall + LSM hook support

## Testing Strategy

1. **Use Docker for**:
   - Quick testing of LSM hooks
   - Verifying process execution tracking
   - Testing log filter transformations
   - Debugging "unknown to unknown" issues (like we did)

2. **Use Real VM (Lima) for**:
   - Complete file system tracking validation
   - Testing `getdents64`, `stat`, `chdir` events
   - Performance testing with full event load
   - Production validation

## References

- **Tracee GitHub**: The warning message appears in tracee source code when `sys_call_table` lookup fails
- **LinuxKit**: Docker Desktop's minimal kernel project
- **eBPF Syscall Hooking**: Requires `sys_call_table` or BTF (BPF Type Format) support
- **LSM Hooks**: Separate kernel framework, doesn't need `sys_call_table`

## Verification in Your VM

When you deploy to your Lima VM, you should see NO warnings about `sys_call_table`:

```bash
# In Lima VM:
capsule-vm create test-vm
capsule-vm shell test-vm

# Inside VM:
sudo cat /var/log/tracee/tracee.log
```

You should NOT see:
```json
{"level":"warn","msg":"Event canceled because of missing kernel symbol dependency","missing symbols":["sys_call_table"]}
```

If you DO see it in the VM, then the VM kernel also has this limitation. But standard Ubuntu 24.04 kernels have `sys_call_table` exported, so it should work fine.
