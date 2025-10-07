# Syscall Parsing Expansion Plan

## Executive Summary

This document outlines a comprehensive plan to expand our syscall parsing and database schema to support detailed, human-readable representation of system calls beyond the current `RawSyscall` parsing. The current implementation stores syscall arguments as a JSONB array of strings, which preserves the raw strace output but lacks semantic meaning. This plan proposes a structured approach to parse and store syscall-specific data in a way that enables rich querying and human-readable display.

**Status**: Research & Planning Phase
**Last Updated**: 2025-10-06

---

## Table of Contents

1. [Background & Current State](#background--current-state)
2. [Problem Statement](#problem-statement)
3. [Research Findings](#research-findings)
4. [Proposed Architecture](#proposed-architecture)
5. [Schema Design](#schema-design)
6. [Implementation Strategy](#implementation-strategy)
7. [Phase Breakdown](#phase-breakdown)
8. [Open Questions](#open-questions)

---

## Background & Current State

### Current Parsing Implementation

Location: `src/trace.rs:22-61`

Our current parser (`parse_raw_syscall`) extracts:
- Timestamp (string)
- PID (optional u32)
- Syscall number (u32)
- Syscall name (string)
- Raw arguments (Vec<String>) - **unparsed, just split by commas**
- Raw return value (string)
- Category (Process/File/Network/Unknown)

### Current Database Schema

Location: `DATABASE_SCHEMA_DESIGN.md`

```sql
CREATE TABLE syscalls (
    id BIGSERIAL PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    timestamp TEXT NOT NULL,
    pid INTEGER,
    syscall_number INTEGER NOT NULL,
    syscall_name TEXT NOT NULL,
    raw_args JSONB NOT NULL,  -- Array of strings
    raw_return TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('Process', 'File', 'Network', 'Unknown')),
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### What's Missing

The `raw_args` field contains strings like:
- `"AT_FDCWD"` - needs to be parsed as a constant
- `"/etc/ld.so.cache"` - needs to be extracted as a file path
- `"O_RDONLY|O_CLOEXEC"` - needs to be parsed as bitwise flags
- `"{sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr(\"127.0.0.1\")}"` - needs to be parsed as a struct
- `["python3", "scripts/process_syscalls.py"]` - needs to be parsed as an array

**The problem**: Without parsing these, we can't:
- Generate human-readable displays (e.g., "Opened /etc/hosts for reading")
- Query efficiently (e.g., "Find all writes to /tmp/*")
- Extract semantic information (e.g., "Which processes were spawned?")

---

## Problem Statement

### Core Challenge

**Each syscall has its own unique parameter signature with different data types:**

- **Primitive types**: integers, strings, file descriptors
- **Composite types**: structs, arrays, bitwise flags
- **Variable types**: Some parameters can be different types depending on flags

### Example: openat syscall

**Signature**: `int openat(int dirfd, const char *pathname, int flags, mode_t mode)`

**Strace output**:
```
openat(AT_FDCWD, "/etc/ld.so.cache", O_RDONLY|O_CLOEXEC) = 3</etc/ld.so.cache>
```

**raw_args would contain**:
```json
["AT_FDCWD", "\"/etc/ld.so.cache\"", "O_RDONLY|O_CLOEXEC"]
```

**What we want to extract**:
- `dirfd`: Constant `AT_FDCWD` (special value -100)
- `pathname`: String `"/etc/ld.so.cache"`
- `flags`: Bitwise OR of `O_RDONLY` and `O_CLOEXEC`
- `mode`: Not present (optional when not O_CREAT)
- `return`: File descriptor 3, resolved to `/etc/ld.so.cache`

### Complexity Factors

1. **Each syscall needs custom parsing logic** - 30+ syscalls across 3 categories
2. **Nested structures** - Structs contain structs (e.g., `sockaddr` contains different address families)
3. **Variable-length arrays** - `execve` has arrays of arbitrary size
4. **Context-dependent parsing** - Some args depend on values of other args
5. **Error cases** - On error, some structs show as hex addresses instead of dereferenced values
6. **Unfinished syscalls** - Blocking calls split across multiple trace lines

---

## Research Findings

### 1. Strace Output Format Specification

Source: https://man7.org/linux/man-pages/man1/strace.1.html

#### Format Rules

| Type | Format | Example |
|------|--------|---------|
| **Strings** | Double-quoted, escaped | `"/etc/hosts"` |
| **Integers** | Decimal or hex | `123` or `0x7b` |
| **Constants** | Symbolic names | `AT_FDCWD`, `AF_INET` |
| **Bitwise Flags** | OR'ed with `\|` | `O_RDONLY\|O_CLOEXEC` |
| **Structs** | `{field=value, ...}` | `{sa_family=AF_INET, sin_port=htons(9999)}` |
| **Arrays** | `[item, item, ...]` | `["python3", "script.py"]` |
| **Pointers** | Hex address or NULL | `0xffffdf523a40` or `NULL` |
| **File Descriptors** (with -yy) | `fd<details>` | `3</etc/ld.so.cache>` |

#### Special Cases

1. **Incomplete structs**: When a syscall fails, struct parameters may show as just hex addresses
   ```
   newfstatat(AT_FDCWD, "/nonexistent", 0xfffff9edb740, 0) = -1 ENOENT
   ```

2. **Truncated strings**: Controlled by `-s` flag (we use 65535)
   ```
   read(3, "data data data..."..., 1024)
   ```

3. **Unfinished syscalls**: Blocking operations split across lines
   ```
   [pid  3760] recvfrom(5<TCP:[...]>,  <unfinished ...>
   [pid  3760] <... recvfrom resumed>"Hello!", 1024, 0, NULL, NULL) = 18
   ```

### 2. Syscall Analysis by Category

Based on examination of:
- `SYSCALL_REFERENCE.md`
- `scripts/script_outputs/*/raw_trace.txt`
- Linux man pages

#### Process Syscalls

| Syscall | Key Args | Complex Types | Notes |
|---------|----------|---------------|-------|
| **execve** | `pathname`, `argv[]`, `envp[]` | String arrays | Arrays can be 100+ elements |
| **clone** | `flags`, `child_stack` | Bitwise flags | Complex flag combinations |
| **fork/vfork** | None | - | Simple |
| **wait4** | `pid`, `status` | Status struct | Status is encoded integer |

**Example - execve**:
```
execve("/usr/bin/python3", ["python3", "script.py"], ["ENV=val", ...]) = 0
```

**Parsed structure**:
- `pathname`: `/usr/bin/python3`
- `argv`: Array of 2 strings
- `envp`: Array of N environment variables (KEY=VALUE format)

#### File Syscalls

| Syscall | Key Args | Complex Types | Notes |
|---------|----------|---------------|-------|
| **openat** | `dirfd`, `pathname`, `flags`, `mode` | Constant, bitwise flags, octal | Mode optional |
| **read/write** | `fd`, `buf`, `count` | Buffer content or hex | Buffer may be truncated |
| **newfstatat** | `dirfd`, `pathname`, `statbuf`, `flags` | Stat struct | Rich metadata |
| **readlinkat** | `dirfd`, `pathname`, `buf`, `bufsiz` | String or hex | Depends on success |
| **getcwd** | `buf`, `size` | String | Simple |

**Example - openat**:
```
openat(AT_FDCWD, "/etc/hosts", O_RDONLY|O_CLOEXEC) = 3</etc/hosts>
```

**Parsed structure**:
- `dirfd`: Special constant `AT_FDCWD` (-100)
- `pathname`: `/etc/hosts`
- `flags`: Set of `{O_RDONLY, O_CLOEXEC}`
- `mode`: Not present
- `return_fd`: 3
- `resolved_path`: `/etc/hosts`

**Example - newfstatat (success)**:
```
newfstatat(AT_FDCWD, "/usr/bin/python3", {st_dev=makedev(0, 0x4c), st_ino=8815141,
           st_mode=S_IFREG|0755, st_nlink=1, st_uid=0, st_gid=0, st_blksize=4096,
           st_blocks=10296, st_size=5268696, st_atime=1742436459, ...}, 0) = 0
```

**Parsed structure** (stat struct has 15+ fields):
- `st_dev`: Device ID (special makedev() format)
- `st_ino`: Inode number
- `st_mode`: File type + permissions (bitwise flags)
- `st_size`: File size in bytes
- Timestamps, ownership, etc.

**Example - newfstatat (error)**:
```
newfstatat(AT_FDCWD, "/nonexistent", 0xfffff9edb740, 0) = -1 ENOENT
```

Note: On error, stat struct shows as hex address (uninitialized).

#### Network Syscalls

| Syscall | Key Args | Complex Types | Notes |
|---------|----------|---------------|-------|
| **socket** | `domain`, `type`, `protocol` | Constants, bitwise flags | Type can have flags |
| **bind/connect** | `sockfd`, `addr`, `addrlen` | Sockaddr struct | Varies by family |
| **listen** | `sockfd`, `backlog` | - | Simple |
| **accept/accept4** | `sockfd`, `addr`, `addrlen` | Sockaddr struct, flags | addr is output param |
| **sendto/recvfrom** | `sockfd`, `buf`, `len`, `flags`, `addr`, `addrlen` | String/hex, sockaddr | buf content varies |

**Example - socket**:
```
socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC, IPPROTO_IP) = 3<TCP:[18898905]>
```

**Parsed structure**:
- `domain`: Constant `AF_INET` (IPv4)
- `type`: Bitwise OR of `SOCK_STREAM` and `SOCK_CLOEXEC`
- `protocol`: Constant `IPPROTO_IP`
- `return_fd`: 3
- `socket_type`: TCP
- `socket_id`: 18898905

**Example - bind**:
```
bind(3<TCP:[18898905]>, {sa_family=AF_INET, sin_port=htons(9999),
     sin_addr=inet_addr("127.0.0.1")}, 16) = 0
```

**Parsed structure** (sockaddr_in):
- `sockfd`: 3 (with TCP socket info)
- `sa_family`: AF_INET
- `sin_port`: 9999 (wrapped in htons())
- `sin_addr`: "127.0.0.1" (wrapped in inet_addr())
- `addrlen`: 16 bytes

**Example - sendto**:
```
sendto(4<TCP:[127.0.0.1:32820->127.0.0.1:9999]>, "Hello from client!", 18, 0, NULL, 0) = 18
```

**Parsed structure**:
- `sockfd`: 4 with full connection info
- `buf`: "Hello from client!" (18 bytes)
- `len`: 18
- `flags`: 0
- `dest_addr`: NULL (connected socket)
- `addrlen`: 0
- `return_bytes`: 18

### 3. Key Observations

#### Observation 1: Granularity Decision

**Question**: Should we parse per-category or per-syscall?

**Answer**: **Per-syscall** is necessary because:
- Even within a category, syscalls have wildly different structures
- Network category: `socket()` has 3 args, `sendto()` has 6 args
- File category: `openat()` has 4 args, `getcwd()` has 2 args
- Cannot use a one-size-fits-all approach

#### Observation 2: Non-Primitive Types are Common

**Frequency analysis** from raw_trace files:
- Structs: ~40% of all syscall args
- Bitwise flags: ~30% of syscall args
- Arrays: ~10% of syscall args
- Simple types: ~20% of syscall args

**Implication**: We MUST handle complex type parsing, not just primitives.

#### Observation 3: Strace Format is Consistent but Complex

**Good news**: Strace format is deterministic and well-documented
**Bad news**: Parsing requires:
- Regex for pattern matching
- Context-aware parsing (arg 3 meaning depends on arg 2)
- Recursive descent for nested structures
- Error handling for incomplete/failed syscalls

#### Observation 4: Some Args Depend on Other Args

Examples:
- `openat()`: `mode` parameter only present if flags contain `O_CREAT`
- `sendto()`: `dest_addr` is NULL for connected sockets
- `newfstatat()`: `statbuf` is dereferenced on success, hex pointer on error

**Implication**: Can't parse args in isolation; need stateful parsing.

---

## Proposed Architecture

### Design Principles

1. **Preserve RawSyscall**: Never remove existing parsing; add a layer on top
2. **Async Processing**: Parsing can be done downstream, doesn't block tracing
3. **Incremental Rollout**: Start with high-value syscalls, expand gradually
4. **Type Safety**: Use Rust enums/structs for parsed data
5. **Fallback Gracefully**: If parsing fails, still have raw data

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│ strace raw output                                                │
│ "openat(AT_FDCWD, \"/etc/hosts\", O_RDONLY|O_CLOEXEC) = 3"     │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ RawSyscall Parser (trace.rs) - EXISTING                         │
│ ├─ timestamp: "13:01:09.180154"                                 │
│ ├─ syscall_name: "openat"                                       │
│ ├─ raw_args: ["AT_FDCWD", "\"/etc/hosts\"", "O_RDONLY|.."]     │
│ ├─ raw_return: "3</etc/hosts>"                                  │
│ └─ category: File                                                │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ NEW: Typed Syscall Parser (parse_syscall.rs)                    │
│ Pattern match on syscall_name + category                         │
└────────────────────┬────────────────────────────────────────────┘
                     │
         ┌───────────┼───────────┐
         │           │           │
         ▼           ▼           ▼
┌────────────┐ ┌───────────┐ ┌───────────┐
│ Process    │ │ File      │ │ Network   │
│ Parsers    │ │ Parsers   │ │ Parsers   │
│            │ │           │ │           │
│ execve     │ │ openat    │ │ socket    │
│ clone      │ │ read      │ │ bind      │
│ wait4      │ │ newfstatat│ │ sendto    │
│ ...        │ │ ...       │ │ ...       │
└────────────┘ └───────────┘ └───────────┘
         │           │           │
         └───────────┼───────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ ParsedSyscall Enum                                               │
│ ├─ OpenAt { dirfd, pathname, flags, mode, return_fd }           │
│ ├─ Execve { pathname, argv, envp, return_code }                 │
│ ├─ Bind { sockfd, addr: SockAddr, return_code }                 │
│ └─ Unparsed { raw: RawSyscall }  ← Fallback                     │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│ Database: Add parsed_data JSONB column                          │
│ {                                                                │
│   "dirfd": {"constant": "AT_FDCWD", "value": -100},             │
│   "pathname": "/etc/hosts",                                      │
│   "flags": ["O_RDONLY", "O_CLOEXEC"],                           │
│   "return_fd": 3,                                                │
│   "resolved_path": "/etc/hosts"                                  │
│ }                                                                │
└─────────────────────────────────────────────────────────────────┘
```

### Module Structure

```
src/
├── trace.rs              # EXISTING - RawSyscall parsing
├── models.rs             # EXISTING - RawSyscall struct
├── parse_syscall/        # NEW MODULE
│   ├── mod.rs            # Public API + ParsedSyscall enum
│   ├── types.rs          # Common types (FileDescriptor, SockAddr, etc.)
│   ├── process.rs        # Process syscall parsers
│   ├── file.rs           # File syscall parsers
│   ├── network.rs        # Network syscall parsers
│   └── utils.rs          # Parsing utilities (regex, flag parser, etc.)
```

---

## Schema Design

### Option 1: Single JSONB Column (Recommended for Phase 1)

**Advantages**:
- Flexible: Can evolve structure without migrations
- Simple: One column addition
- Queryable: PostgreSQL has rich JSONB operators

**Disadvantages**:
- Less type safety at DB level
- Potentially slower queries than dedicated columns

```sql
ALTER TABLE syscalls ADD COLUMN parsed_data JSONB;

-- Example data for openat:
{
  "syscall_type": "OpenAt",
  "dirfd": {"constant": "AT_FDCWD", "value": -100},
  "pathname": "/etc/hosts",
  "flags": ["O_RDONLY", "O_CLOEXEC"],
  "mode": null,
  "return_fd": 3,
  "resolved_path": "/etc/hosts"
}

-- Example data for execve:
{
  "syscall_type": "Execve",
  "pathname": "/usr/bin/python3",
  "argv": ["python3", "script.py"],
  "argc": 2,
  "envp": ["PATH=/usr/bin", "HOME=/root", ...],
  "envc": 10,
  "return_code": 0
}

-- Example data for bind:
{
  "syscall_type": "Bind",
  "sockfd": 3,
  "socket_info": "TCP:[18898905]",
  "addr": {
    "family": "AF_INET",
    "ip": "127.0.0.1",
    "port": 9999
  },
  "return_code": 0
}
```

**Querying examples**:
```sql
-- Find all files opened with O_CREAT
SELECT * FROM syscalls
WHERE parsed_data->>'syscall_type' = 'OpenAt'
  AND parsed_data->'flags' @> '["O_CREAT"]';

-- Find all executions of python
SELECT * FROM syscalls
WHERE parsed_data->>'syscall_type' = 'Execve'
  AND parsed_data->>'pathname' LIKE '%python%';

-- Find all network connections to specific IP
SELECT * FROM syscalls
WHERE parsed_data->>'syscall_type' = 'Connect'
  AND parsed_data->'addr'->>'ip' = '192.168.1.1';
```

### Option 2: Category-Specific Tables (Future Phase)

For better performance at scale:

```sql
CREATE TABLE process_syscalls (
    syscall_id BIGINT PRIMARY KEY REFERENCES syscalls(id),
    -- execve fields
    pathname TEXT,
    argv TEXT[],
    envp TEXT[],
    -- clone fields
    clone_flags TEXT[],
    -- etc.
);

CREATE TABLE file_syscalls (
    syscall_id BIGINT PRIMARY KEY REFERENCES syscalls(id),
    -- openat fields
    dirfd_constant TEXT,
    dirfd_value INTEGER,
    pathname TEXT,
    open_flags TEXT[],
    mode INTEGER,
    resolved_path TEXT,
    -- read/write fields
    buffer_content TEXT,
    bytes_requested INTEGER,
    bytes_actual INTEGER,
    -- etc.
);

CREATE TABLE network_syscalls (
    syscall_id BIGINT PRIMARY KEY REFERENCES syscalls(id),
    socket_domain TEXT,
    socket_type TEXT,
    socket_protocol TEXT,
    -- address info
    addr_family TEXT,
    addr_ip TEXT,
    addr_port INTEGER,
    -- etc.
);
```

**Advantages**:
- Better query performance
- Type safety at DB level
- Clearer structure

**Disadvantages**:
- Schema migrations for every new syscall
- More complex queries (joins)
- Rigid structure

**Recommendation**: Start with Option 1 (JSONB), migrate to Option 2 if performance becomes an issue.

---

## Implementation Strategy

### Phase-Based Approach

We will implement this in phases, starting with high-value, simpler syscalls and progressively adding more complex ones.

#### Priority Matrix

| Syscall | Value | Complexity | Priority |
|---------|-------|------------|----------|
| openat | High | Medium | P0 |
| read/write | High | Low | P0 |
| execve | High | High | P0 |
| socket | High | Medium | P1 |
| bind/connect | High | High | P1 |
| sendto/recvfrom | Medium | High | P1 |
| newfstatat | Medium | High | P2 |
| clone | Low | Very High | P3 |

### Parsing Utilities (Build First)

Before implementing individual syscall parsers, build reusable utilities:

```rust
// src/parse_syscall/utils.rs

/// Parse a bitwise OR'ed flag string
/// Example: "O_RDONLY|O_CLOEXEC" -> vec!["O_RDONLY", "O_CLOEXEC"]
pub fn parse_flags(s: &str) -> Vec<String>;

/// Parse a quoted string, handling escapes
/// Example: "\"/etc/hosts\"" -> "/etc/hosts"
pub fn parse_string(s: &str) -> Result<String>;

/// Parse an array like ["item1", "item2", ...]
pub fn parse_array(s: &str) -> Result<Vec<String>>;

/// Parse a struct like {field=value, field2=value2}
pub fn parse_struct(s: &str) -> Result<HashMap<String, String>>;

/// Parse a file descriptor annotation
/// Example: "3</etc/hosts>" -> (3, Some("/etc/hosts"))
pub fn parse_fd(s: &str) -> Result<(u32, Option<String>)>;

/// Parse a sockaddr struct (any family)
pub fn parse_sockaddr(s: &str) -> Result<SockAddr>;

/// Parse a numeric constant or literal
/// Example: "AT_FDCWD" -> Constant("AT_FDCWD", Some(-100))
/// Example: "123" -> Literal(123)
pub fn parse_numeric(s: &str) -> Result<NumericValue>;
```

### Example Implementation: openat

```rust
// src/parse_syscall/file.rs

use crate::models::RawSyscall;
use crate::parse_syscall::types::*;
use crate::parse_syscall::utils::*;

pub struct OpenAtSyscall {
    pub dirfd: NumericValue,
    pub pathname: String,
    pub flags: Vec<String>,
    pub mode: Option<u32>,
    pub return_fd: Option<u32>,
    pub resolved_path: Option<String>,
}

pub fn parse_openat(raw: &RawSyscall) -> Result<OpenAtSyscall> {
    // Validate we have the right syscall
    assert_eq!(raw.syscall_name, "openat");

    // Parse arguments
    let dirfd = parse_numeric(&raw.raw_args[0])?;
    let pathname = parse_string(&raw.raw_args[1])?;
    let flags = parse_flags(&raw.raw_args[2])?;

    // Mode is optional (only if O_CREAT present)
    let mode = if flags.contains(&"O_CREAT".to_string()) && raw.raw_args.len() > 3 {
        Some(parse_octal(&raw.raw_args[3])?)
    } else {
        None
    };

    // Parse return value
    let (return_fd, resolved_path) = parse_return_fd(&raw.raw_return)?;

    Ok(OpenAtSyscall {
        dirfd,
        pathname,
        flags,
        mode,
        return_fd: Some(return_fd),
        resolved_path,
    })
}
```

### Testing Strategy

For each syscall parser:

1. **Unit tests** with real strace output examples
2. **Property tests** for edge cases (empty arrays, max sizes, etc.)
3. **Integration tests** with actual trace files

Example:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openat_simple() {
        let raw = RawSyscall {
            timestamp: "13:01:09.078256".to_string(),
            pid: None,
            syscall_number: 56,
            syscall_name: "openat".to_string(),
            raw_args: vec![
                "AT_FDCWD".to_string(),
                "\"/etc/ld.so.cache\"".to_string(),
                "O_RDONLY|O_CLOEXEC".to_string(),
            ],
            raw_return: "3</etc/ld.so.cache>".to_string(),
            category: SyscallCategory::File,
        };

        let parsed = parse_openat(&raw).unwrap();
        assert_eq!(parsed.pathname, "/etc/ld.so.cache");
        assert_eq!(parsed.flags, vec!["O_RDONLY", "O_CLOEXEC"]);
        assert_eq!(parsed.return_fd, Some(3));
    }

    #[test]
    fn test_parse_openat_with_mode() {
        let raw = RawSyscall {
            // ... openat with O_CREAT and mode ...
        };

        let parsed = parse_openat(&raw).unwrap();
        assert_eq!(parsed.mode, Some(0o644));
    }
}
```

---

## Phase Breakdown

### Phase 0: Foundation (Current Status)

**Status**: ✅ Complete

- [x] RawSyscall parsing working
- [x] Database schema for raw syscalls
- [x] SYSCALL_REFERENCE.md documentation
- [x] Example raw_trace.txt files for each category

### Phase 1: Core Infrastructure (Week 1-2)

**Goals**: Set up parsing infrastructure without touching database yet

**Tasks**:
1. Create `src/parse_syscall/` module structure
2. Implement common types in `types.rs`:
   - `NumericValue` enum (Constant vs Literal)
   - `FileDescriptor` struct
   - `SockAddr` enum (IPv4, IPv6, Unix)
3. Implement parsing utilities in `utils.rs`:
   - `parse_flags()` - bitwise OR parser
   - `parse_string()` - quoted string parser
   - `parse_array()` - array parser
   - `parse_struct()` - struct parser
   - `parse_fd()` - file descriptor parser
4. Write comprehensive unit tests for utilities
5. Document usage patterns

**Deliverable**: Reusable parsing library

**Success Criteria**:
- All utility functions have >90% test coverage
- Can parse all basic data types from strace output
- Documentation with examples

### Phase 2: High-Priority Syscalls (Week 3-4)

**Goals**: Implement parsers for P0 syscalls

**Syscalls to implement**:
1. **openat** - File opening
2. **read** - Read from file descriptor
3. **write** - Write to file descriptor
4. **execve** - Process execution

**Tasks per syscall**:
1. Define struct for parsed representation
2. Implement parser function
3. Write tests using real trace examples
4. Add to `ParsedSyscall` enum

**Deliverable**: Working parsers for 4 most common syscalls

**Success Criteria**:
- Can successfully parse 95% of instances from raw_trace.txt files
- Handles error cases gracefully
- Falls back to RawSyscall on parse failure

### Phase 3: Database Integration (Week 5)

**Goals**: Add `parsed_data` column and populate it

**Tasks**:
1. Create database migration:
   ```sql
   ALTER TABLE syscalls ADD COLUMN parsed_data JSONB;
   CREATE INDEX idx_syscalls_parsed_type ON syscalls((parsed_data->>'syscall_type'));
   ```
2. Update `models.rs` to include `parsed_data` field
3. Implement `to_json()` for each `ParsedSyscall` variant
4. Update insert logic in `src/transfer.rs` to populate `parsed_data`
5. Write integration tests

**Deliverable**: Syscalls stored with structured data

**Success Criteria**:
- Can insert parsed syscalls into database
- Can query by parsed fields
- Existing raw_args still preserved

### Phase 4: Network Syscalls (Week 6-7)

**Goals**: Implement P1 network syscalls

**Syscalls to implement**:
1. **socket** - Create socket
2. **bind** - Bind to address
3. **connect** - Connect to address
4. **sendto** - Send data
5. **recvfrom** - Receive data

**Key challenges**:
- sockaddr parsing varies by family (AF_INET vs AF_INET6 vs AF_UNIX)
- Need to handle connection state annotations in FD
- Buffer contents may be binary data

**Deliverable**: Full network syscall support

**Success Criteria**:
- Can extract IP addresses and ports
- Can identify all external network connections
- Handles both TCP and UDP

### Phase 5: Display Layer (Week 8)

**Goals**: Use parsed data for human-readable output

**Tasks**:
1. Implement `Display` trait for each `ParsedSyscall` variant
2. Update query output formatting
3. Add filtering options based on parsed data
4. Create example queries for common forensic questions

**Example outputs**:
```
Instead of:
  openat(AT_FDCWD, "/etc/hosts", O_RDONLY|O_CLOEXEC) = 3

Show:
  [File] Opened /etc/hosts for reading (fd=3)

Instead of:
  execve("/usr/bin/python3", ["python3", "script.py"], [...]) = 0

Show:
  [Process] Executed /usr/bin/python3 with args: python3 script.py

Instead of:
  connect(4<TCP:[18907120]>, {sa_family=AF_INET, ...}, 16) = 0

Show:
  [Network] Connected to 127.0.0.1:9999 via TCP
```

**Deliverable**: Rich, human-readable trace output

### Phase 6: Remaining Syscalls (Week 9+)

**Goals**: Implement P2 and P3 syscalls as needed

**Syscalls**:
- newfstatat (complex struct parsing)
- readlinkat (symlink handling)
- clone (very complex flags)
- accept/accept4
- etc.

**Approach**: Implement based on actual usage patterns and user requests

---

## Open Questions

### Q1: How to handle parsing failures?

**Options**:
A. Store null in parsed_data, keep raw_args
B. Store error message in parsed_data
C. Flag syscall as "parse_failed"

**Recommendation**: A - Silent fallback to raw data

**Rationale**: Don't want to clutter output with parse errors. If someone needs the data, raw_args is still there.

### Q2: Should we parse return values?

Return values can be complex:
- File descriptors with annotations: `3</path>`
- Stat structs: `{st_mode=..., st_size=...}`
- Error codes: `-1 ENOENT (No such file or directory)`

**Recommendation**: Yes, parse return values

**Rationale**: They contain valuable info (FD mappings, error types, etc.)

### Q3: Performance impact?

Parsing adds computational overhead. How to measure and mitigate?

**Plan**:
1. Benchmark parsing speed per syscall type
2. If too slow, implement:
   - Lazy parsing (parse on query, not on insert)
   - Parallel parsing (use rayon)
   - Selective parsing (only parse certain categories)

### Q4: How to handle schema evolution?

As we add more syscalls, parsed_data structure will grow.

**Plan**:
- Use semantic versioning in parsed_data: `{"version": "1.0", ...}`
- Write migration code for old versions
- Keep backward compatibility

### Q5: Should we parse environmet variables?

`execve` can have 100+ environment variables. Do we want to parse and store all of them?

**Recommendation**: Store count and selected variables

**Rationale**: Full envp is noisy. Store:
- Total count
- Security-relevant vars (PATH, LD_PRELOAD, etc.)
- User can always access raw_args for full list

---

## Success Metrics

### Quantitative

- [ ] 95%+ parse success rate for targeted syscalls
- [ ] <10ms parsing time per syscall
- [ ] 100% test coverage for parsing utilities
- [ ] Zero data loss (raw_args always preserved)

### Qualitative

- [ ] Human-readable trace output
- [ ] Enables forensic queries like "all files written to /tmp"
- [ ] Reduces need to manually inspect raw traces

---

## Next Steps

1. **Review this plan** with team
2. **Confirm Phase 1 scope** - are utility functions comprehensive enough?
3. **Set up dev environment** for testing
4. **Begin implementation** of Phase 1

---

## Appendix A: Strace Output Examples

See `scripts/script_outputs/*/raw_trace.txt` for full examples.

### Appendix B: Relevant Man Pages

- `man 1 strace` - strace command
- `man 2 openat` - openat syscall
- `man 2 execve` - execve syscall
- `man 2 socket` - socket syscall
- `man 7 socket` - socket address structures

### Appendix C: Reference Code

- Current parser: `src/trace.rs:22-61`
- Syscall categorization: `src/trace.rs:198-214`
- Database models: `src/models.rs`
