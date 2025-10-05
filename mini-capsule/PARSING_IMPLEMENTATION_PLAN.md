# Syscall Trace Parsing Implementation Plan

## Overview
This document outlines the strategy for parsing strace output into structured Rust data types for the mini-capsule project. The parsing follows a two-phase approach: first parsing into a generic `RawSyscall` structure, then converting to specific typed syscall structures.

---

## Current State

### What We Have
- **Trace Collection**: `LinuxTracer` in `src/trace.rs` streams raw strace output to files
- **Data Models**: `SessionMetadata` in `src/models.rs` tracks session info
- **Trace Files**: Example traces in `scripts/script_outputs/` organized by category:
  - `process_syscalls/` - execve, clone, fork, wait
  - `file_syscalls/` - openat, read, write, newfstatat, getcwd
  - `network_syscalls/` - socket, bind, connect, listen, accept, send, recv

### Strace Format Analysis
All syscalls follow this high-level pattern:
```
TIMESTAMP [SYSCALL_NUM] syscall_name(arg1, arg2, ..., argN) = RETURN_VALUE
```

#### Examples:
```
20:59:31.450720 [  56] openat(AT_FDCWD, "/working/mini-capsule/scripts", O_RDONLY|O_NONBLOCK|O_CLOEXEC|O_DIRECTORY) = 3</working/mini-capsule/scripts>

13:01:09.180154 [ 198] socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC, IPPROTO_IP) = 3<TCP:[18898905]>

20:59:31.408307 [  79] newfstatat(AT_FDCWD, "/usr/local/cargo/bin/python3", 0xfffff9edb740, 0) = -1 ENOENT (No such file or directory)

21:00:22.340048 [ 221] execve("/usr/bin/python3", ["python3", "scripts/process_syscalls.py"], ["HOSTNAME=...", ...]) = 0
```

### Key Challenges
1. **Complex Argument Structures**: Args can be:
   - Simple values: `0`, `1`, `AT_FDCWD`
   - Hex addresses: `0xfffff9edb740`
   - Strings: `"/path/to/file"`
   - Arrays: `["arg1", "arg2", "arg3"]`
   - Structs: `{sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr("127.0.0.1")}`
   - Flags (bitwise OR): `O_RDONLY|O_NONBLOCK|O_CLOEXEC`

2. **Nested Structures**: Arguments can contain nested arrays and structs
3. **Variable Return Types**: Returns can be simple integers, errno values, or descriptors with metadata
4. **Comma Ambiguity**: Commas appear in both top-level arg separation AND within nested structures

---

## Proposed Architecture

### Phase 1: Generic Parsing → `RawSyscall`

Create a high-level structure that captures the syscall without interpreting arguments:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSyscall {
    pub timestamp: String,           // "20:59:31.450720"
    pub syscall_number: u32,         // 56
    pub syscall_name: String,        // "openat"
    pub raw_args: Vec<String>,       // ["AT_FDCWD", "\"/working/...\"", "O_RDONLY|..."]
    pub raw_return: String,          // "3</working/mini-capsule/scripts>"
    pub category: SyscallCategory,   // Derived from syscall_name
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyscallCategory {
    Process,
    File,
    Network,
    Unknown,
}
```

### Phase 2: Specific Parsing → Typed Syscalls

Convert `RawSyscall` to specific typed structures:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParsedSyscall {
    OpenAt(OpenAtSyscall),
    NewFStatAt(NewFStatAtSyscall),
    Socket(SocketSyscall),
    Bind(BindSyscall),
    Connect(ConnectSyscall),
    Execve(ExecveSyscall),
    // ... more syscalls
    Unknown(RawSyscall),
}

// Example: File I/O
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAtSyscall {
    pub timestamp: String,
    pub syscall_number: u32,
    pub dirfd: String,              // "AT_FDCWD" or numeric
    pub pathname: String,            // "/path/to/file"
    pub flags: Vec<String>,          // ["O_RDONLY", "O_CLOEXEC"]
    pub mode: Option<String>,        // Some("0644") or None
    pub result: OpenAtResult,
    pub category: SyscallCategory,   // File
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpenAtResult {
    Success { fd: i32, resolved_path: Option<String> },  // 3, Some("/working/...")
    Error { errno: String, message: String },             // "ENOENT", "No such file..."
}

// Example: Network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketSyscall {
    pub timestamp: String,
    pub syscall_number: u32,
    pub domain: String,              // "AF_INET", "AF_UNIX"
    pub socket_type: Vec<String>,    // ["SOCK_STREAM", "SOCK_CLOEXEC"]
    pub protocol: String,            // "IPPROTO_IP", "IPPROTO_TCP"
    pub result: SocketResult,
    pub category: SyscallCategory,   // Network
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SocketResult {
    Success { fd: i32, socket_info: Option<String> },  // 3, Some("TCP:[18898905]")
    Error { errno: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindSyscall {
    pub timestamp: String,
    pub syscall_number: u32,
    pub sockfd: String,              // "3<TCP:[18898905]>"
    pub addr: SocketAddr,            // Parsed address structure
    pub addrlen: u32,                // 16
    pub result: BindResult,
    pub category: SyscallCategory,   // Network
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketAddr {
    pub family: String,              // "AF_INET"
    pub port: Option<u16>,           // Some(9999)
    pub addr: Option<String>,        // Some("127.0.0.1")
    pub raw: String,                 // Full raw representation
}

// Example: Process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecveSyscall {
    pub timestamp: String,
    pub syscall_number: u32,
    pub pathname: String,            // "/usr/bin/python3"
    pub argv: Vec<String>,           // ["python3", "script.py"]
    pub envp: Vec<String>,           // ["PATH=...", "HOME=..."]
    pub result: ExecveResult,
    pub category: SyscallCategory,   // Process
}
```

---

## Implementation Strategy

### Step 1: Core Parsing Functions (Week 1-2)

**File**: `src/trace_parser.rs` (new)

```rust
mod trace_parser {
    use anyhow::Result;

    /// Parse a single strace line into RawSyscall
    pub fn parse_raw_syscall(line: &str) -> Result<RawSyscall> {
        // 1. Extract timestamp
        // 2. Extract syscall_number (between [ ])
        // 3. Extract syscall_name (before first '(')
        // 4. Extract arguments (between '(' and ')' - handle nesting!)
        // 5. Extract return value (after '=')
        // 6. Determine category from syscall_name
    }

    /// Split arguments respecting nesting
    /// This is CRITICAL - naive split on ',' will break!
    fn split_arguments(args_str: &str) -> Vec<String> {
        // Track depth of (), {}, []
        // Only split on ',' when depth == 0
    }

    /// Determine syscall category from name
    fn categorize_syscall(name: &str) -> SyscallCategory {
        match name {
            "execve" | "clone" | "fork" | "wait4" | "exit" => SyscallCategory::Process,
            "openat" | "read" | "write" | "newfstatat" | "getcwd" | ... => SyscallCategory::File,
            "socket" | "bind" | "connect" | "listen" | "accept" | ... => SyscallCategory::Network,
            _ => SyscallCategory::Unknown,
        }
    }
}
```

### Step 2: Specific Syscall Parsers (Week 3-4)

Create individual parser modules for each syscall type:

**File**: `src/syscalls/mod.rs` (new)
```rust
pub mod openat;
pub mod newfstatat;
pub mod socket;
pub mod bind;
pub mod connect;
pub mod execve;
// ... more modules
```

**File**: `src/syscalls/openat.rs` (example)
```rust
use crate::trace_parser::RawSyscall;
use anyhow::{Result, Context};

pub fn parse_openat(raw: &RawSyscall) -> Result<OpenAtSyscall> {
    // Expect 3 or 4 args: dirfd, pathname, flags, [mode]
    if raw.raw_args.len() < 3 || raw.raw_args.len() > 4 {
        anyhow::bail!("openat expects 3-4 args, got {}", raw.raw_args.len());
    }

    let dirfd = raw.raw_args[0].clone();
    let pathname = parse_string_arg(&raw.raw_args[1])?;
    let flags = parse_flags(&raw.raw_args[2]);
    let mode = if raw.raw_args.len() == 4 {
        Some(raw.raw_args[3].clone())
    } else {
        None
    };

    let result = parse_openat_result(&raw.raw_return)?;

    Ok(OpenAtSyscall {
        timestamp: raw.timestamp.clone(),
        syscall_number: raw.syscall_number,
        dirfd,
        pathname,
        flags,
        mode,
        result,
        category: SyscallCategory::File,
    })
}

fn parse_openat_result(return_str: &str) -> Result<OpenAtResult> {
    // Parse "3</working/mini-capsule/scripts>"
    // OR "-1 ENOENT (No such file or directory)"
}
```

### Step 3: Dispatch and Integration (Week 5)

**File**: `src/trace_parser.rs` (extend)
```rust
pub fn parse_syscall(raw: RawSyscall) -> Result<ParsedSyscall> {
    match raw.syscall_name.as_str() {
        "openat" => Ok(ParsedSyscall::OpenAt(syscalls::openat::parse_openat(&raw)?)),
        "newfstatat" => Ok(ParsedSyscall::NewFStatAt(syscalls::newfstatat::parse_newfstatat(&raw)?)),
        "socket" => Ok(ParsedSyscall::Socket(syscalls::socket::parse_socket(&raw)?)),
        "bind" => Ok(ParsedSyscall::Bind(syscalls::bind::parse_bind(&raw)?)),
        "connect" => Ok(ParsedSyscall::Connect(syscalls::connect::parse_connect(&raw)?)),
        "execve" => Ok(ParsedSyscall::Execve(syscalls::execve::parse_execve(&raw)?)),
        // ... more syscalls
        _ => Ok(ParsedSyscall::Unknown(raw)),
    }
}
```

### Step 4: Testing Strategy (Ongoing)

**File**: `src/trace_parser.rs` (tests)
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openat_success() {
        let line = r#"20:59:31.450720 [  56] openat(AT_FDCWD, "/working/mini-capsule/scripts", O_RDONLY|O_NONBLOCK|O_CLOEXEC|O_DIRECTORY) = 3</working/mini-capsule/scripts>"#;

        let raw = parse_raw_syscall(line).unwrap();
        assert_eq!(raw.syscall_name, "openat");
        assert_eq!(raw.syscall_number, 56);
        assert_eq!(raw.category, SyscallCategory::File);

        let parsed = parse_syscall(raw).unwrap();
        match parsed {
            ParsedSyscall::OpenAt(openat) => {
                assert_eq!(openat.pathname, "/working/mini-capsule/scripts");
                assert_eq!(openat.flags, vec!["O_RDONLY", "O_NONBLOCK", "O_CLOEXEC", "O_DIRECTORY"]);
            }
            _ => panic!("Expected OpenAt variant"),
        }
    }

    #[test]
    fn test_parse_socket() {
        let line = r#"13:01:09.180154 [ 198] socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC, IPPROTO_IP) = 3<TCP:[18898905]>"#;
        let raw = parse_raw_syscall(line).unwrap();
        let parsed = parse_syscall(raw).unwrap();

        match parsed {
            ParsedSyscall::Socket(socket) => {
                assert_eq!(socket.domain, "AF_INET");
                assert_eq!(socket.socket_type, vec!["SOCK_STREAM", "SOCK_CLOEXEC"]);
            }
            _ => panic!("Expected Socket variant"),
        }
    }
}
```

**File**: `tests/integration_test.rs` (new)
```rust
// Test parsing entire trace files
#[test]
fn test_parse_file_syscalls_trace() {
    let trace = std::fs::read_to_string("scripts/script_outputs/file_syscalls/raw_trace.txt").unwrap();
    let mut parsed_count = 0;
    let mut error_count = 0;

    for line in trace.lines() {
        match parse_raw_syscall(line) {
            Ok(raw) => {
                match parse_syscall(raw) {
                    Ok(_) => parsed_count += 1,
                    Err(_) => error_count += 1,
                }
            }
            Err(_) => error_count += 1,
        }
    }

    println!("Parsed: {}, Errors: {}", parsed_count, error_count);
    assert!(parsed_count > 100); // Should parse most syscalls
}
```

---

## Incremental Development Approach

### Why Start Small?
Given the complexity, we should **NOT** try to support all syscalls at once. Instead:

1. **Week 1**: Build core `RawSyscall` parser
   - Parse timestamp, syscall_number, name
   - Implement smart argument splitting (handles nesting)
   - Parse return values
   - Add category detection
   - **Goal**: Parse ANY syscall into `RawSyscall` successfully

2. **Week 2**: Implement 3-5 common syscalls
   - `openat` (file, common, moderate complexity)
   - `socket` (network, simple args)
   - `bind` (network, struct args)
   - `newfstatat` (file, large struct return)
   - `execve` (process, array args)
   - **Goal**: Prove the two-phase approach works

3. **Week 3-4**: Expand syscall coverage
   - Add 10-15 more syscalls based on frequency in traces
   - Identify common patterns and create helper functions
   - **Goal**: Cover 80% of syscalls in our example traces

4. **Week 5+**: Polish and Extend
   - Handle edge cases (unfinished syscalls, resumptions)
   - Add error recovery
   - Support rare syscalls
   - **Goal**: Production-ready parser

### Syscall Priority List (by frequency in traces)

**High Priority** (Week 2):
- `openat` - file opening
- `newfstatat` - file stat
- `socket` - network socket creation
- `bind` - network binding
- `connect` - network connection
- `execve` - process execution

**Medium Priority** (Week 3):
- `read`, `write` - I/O
- `listen`, `accept`, `sendto`, `recvfrom` - network operations
- `readlinkat` - symlink reading
- `getcwd` - current directory
- `clone`, `fork` - process creation
- `wait4` - process waiting

**Lower Priority** (Week 4+):
- Less common syscalls
- Edge cases
- Unknown syscalls → keep as `ParsedSyscall::Unknown(RawSyscall)`

---

## File Structure

```
mini-capsule/
├── src/
│   ├── main.rs                 # CLI entry point (existing)
│   ├── models.rs               # SessionMetadata (existing)
│   ├── trace.rs                # LinuxTracer (existing)
│   ├── trace_parser.rs         # NEW: Core parsing logic
│   │   ├── RawSyscall struct
│   │   ├── parse_raw_syscall()
│   │   ├── split_arguments()
│   │   └── parse_syscall() dispatcher
│   └── syscalls/               # NEW: Specific syscall parsers
│       ├── mod.rs
│       ├── openat.rs
│       ├── newfstatat.rs
│       ├── socket.rs
│       ├── bind.rs
│       ├── connect.rs
│       ├── execve.rs
│       └── ...
├── tests/
│   └── integration_test.rs     # NEW: End-to-end parsing tests
└── PARSING_IMPLEMENTATION_PLAN.md  # This document
```

---

## Key Design Decisions

### 1. Two-Phase Parsing (RawSyscall → ParsedSyscall)
**Why**: Separates concerns. Phase 1 handles generic format complexity, Phase 2 handles syscall-specific semantics.

### 2. Keep Raw Strings Initially
**Why**: Argument interpretation is complex. Better to keep raw strings in `RawSyscall` and parse them in Phase 2 with full context.

### 3. Use Enums for Results
**Why**: Returns can be success OR error. Enum captures this better than Option/Result.

### 4. Category on Every Syscall
**Why**: Enables easy filtering by category (process/file/network) without matching on enum variants.

### 5. Incremental Syscall Support
**Why**: There are 300+ Linux syscalls. We should focus on the 20-30 most common ones first, with a clear path to add more.

---

## Testing Plan

### Unit Tests
- Each parsing function in `trace_parser.rs`
- Each syscall-specific parser
- Argument splitting with various nesting levels
- Edge cases (errors, incomplete lines)

### Integration Tests
- Parse entire example trace files
- Verify statistics (e.g., "95% of lines parsed successfully")
- Compare parsed data against known ground truth

### Property-Based Tests (Optional)
- Generate random valid strace lines
- Ensure parser never panics
- Ensure round-trip: parse → serialize → parse gives same result

---

## Potential Gotchas

### 1. Unfinished Syscalls
Some traces have:
```
[pid  3760] 13:01:09.699073 [ 207] recvfrom(5<TCP:[...]>,  <unfinished ...>
[pid  3760] 13:01:09.699190 [ 207] <... recvfrom resumed>"Hello", 1024, ...) = 18
```
**Solution**: Detect and skip `<unfinished ...>` lines initially. Add support later if needed.

### 2. PID Prefixes
Some lines have `[pid XXXX]` prefix.
**Solution**: Make PID parsing optional in timestamp extraction.

### 3. Multi-line Arguments
Very long arrays might wrap.
**Solution**: Start with single-line assumption. Add multi-line support if we encounter it.

### 4. Hex Pointers
Arguments like `0xfffff9edb740` are memory addresses.
**Solution**: Keep as string. Don't try to interpret.

### 5. Macros in Output
`makedev(0, 0x4c)`, `htons(9999)`, `inet_addr("127.0.0.1")`
**Solution**: Keep as string initially. Can parse later if needed.

---

## Success Metrics

### Phase 1 Success
- [ ] Parse 100% of example traces into `RawSyscall` without panics
- [ ] Correctly split arguments for all nesting levels
- [ ] Correctly categorize 95%+ of syscalls

### Phase 2 Success (Per Syscall)
- [ ] 100% unit test coverage for implemented syscalls
- [ ] Correctly parse 95%+ of instances in example traces
- [ ] Clear error messages for malformed input

### Overall Success
- [ ] Parse complete session traces into structured data
- [ ] Enable queries like "show all network syscalls" or "find failed file opens"
- [ ] Support adding new syscalls with <100 lines of code
- [ ] Documentation and examples for extending the parser

---

## Next Steps

1. **Review this plan** - Does the approach make sense? Any major issues?
2. **Create `RawSyscall` struct** - Start with the data model
3. **Implement `parse_raw_syscall()`** - Core parsing logic
4. **Test on example traces** - Ensure we can parse everything to `RawSyscall`
5. **Pick first syscall** - Start with `openat` as proof of concept
6. **Iterate** - Add syscalls one by one, refactoring as we find patterns

---

## Questions for Discussion

1. **Argument Storage**: Should `raw_args` be `Vec<String>` or something richer (like `Vec<ArgValue>` enum)?
2. **Error Handling**: Should parsing errors be fatal or should we continue with `Unknown` variants?
3. **Performance**: Is String cloning acceptable, or should we use references/`Cow<str>`?
4. **Serialization**: Do we need to serialize to JSON, or is this just for internal use?
5. **Partial Parsing**: Should we support parsing just metadata (timestamp, name) without full arg parsing?

---

## Appendix: Syscall Reference

### Common Syscalls by Category

**Process**:
- execve, clone, fork, vfork, wait4, exit, exit_group, getpid, getppid

**File**:
- openat, read, write, close, newfstatat, readlinkat, getcwd, chdir, unlinkat, renameat

**Network**:
- socket, bind, connect, listen, accept, accept4, sendto, recvfrom, shutdown, setsockopt, getsockopt

**Memory**:
- mmap, munmap, mprotect, brk

**Signals**:
- rt_sigaction, rt_sigprocmask, kill

This is not exhaustive - just the most common ones in our traces.
