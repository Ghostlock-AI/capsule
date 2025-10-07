# Syscall Parsing Implementation Checklist

**Project**: Expand syscall parsing for human-readable forensics
**Plan Document**: `SYSCALL_PARSING_EXPANSION_PLAN.md`
**Started**: 2025-10-06

---

## Quick Navigation

- **Current Code**: `src/trace.rs` (RawSyscall parser)
- **Models**: `src/models.rs`
- **Database**: `DATABASE_SCHEMA_DESIGN.md`
- **Reference**: `SYSCALL_REFERENCE.md`
- **Examples**: `scripts/script_outputs/*/raw_trace.txt`
- **Full Plan**: `SYSCALL_PARSING_EXPANSION_PLAN.md`

---

## Phase 0: Foundation ✅ COMPLETE

- [x] RawSyscall parsing working
- [x] Database schema for raw syscalls
- [x] SYSCALL_REFERENCE.md documentation
- [x] Example raw_trace.txt files for each category
- [x] Comprehensive implementation plan

---

## Phase 1: Core Infrastructure (Week 1-2) 🔄 IN PROGRESS

### 1.1 Research & Design

- [ ] Research regex patterns for strace format parsing
- [ ] Research Rust parsing libraries (nom, pest, regex, manual)
- [ ] Design type hierarchy for parsed values
- [ ] Create parsing strategy document

### 1.2 Module Structure

- [ ] Create `src/parse_syscall/` directory
- [ ] Create `src/parse_syscall/mod.rs` with public API
- [ ] Create `src/parse_syscall/types.rs` stub
- [ ] Create `src/parse_syscall/utils.rs` stub
- [ ] Create `src/parse_syscall/process.rs` stub
- [ ] Create `src/parse_syscall/file.rs` stub
- [ ] Create `src/parse_syscall/network.rs` stub

### 1.3 Common Types (`types.rs`)

- [ ] Define `NumericValue` enum (Constant vs Literal)
  ```rust
  pub enum NumericValue {
      Constant { name: String, value: Option<i64> },
      Literal(i64),
  }
  ```
- [ ] Define `FileDescriptor` struct
  ```rust
  pub struct FileDescriptor {
      pub fd: u32,
      pub annotation: Option<String>, // e.g., "</etc/hosts>"
  }
  ```
- [ ] Define `SockAddr` enum (IPv4, IPv6, Unix)
  ```rust
  pub enum SockAddr {
      Inet { ip: String, port: u16 },
      Inet6 { ip: String, port: u16 },
      Unix { path: String },
  }
  ```
- [ ] Define `ParsedSyscall` enum skeleton
- [ ] Write unit tests for type constructors

### 1.4 Parsing Utilities (`utils.rs`)

#### 1.4.1 Flag Parser
- [ ] Implement `parse_flags(s: &str) -> Result<Vec<String>>`
- [ ] Handle OR operator: `"O_RDONLY|O_CLOEXEC"` → `["O_RDONLY", "O_CLOEXEC"]`
- [ ] Handle single flags: `"O_RDONLY"` → `["O_RDONLY"]`
- [ ] Handle empty/numeric: `"0"` → `[]`
- [ ] Test with real examples from traces

#### 1.4.2 String Parser
- [ ] Implement `parse_string(s: &str) -> Result<String>`
- [ ] Handle quoted strings: `"\"/etc/hosts\""` → `"/etc/hosts"`
- [ ] Handle escape sequences: `"\\n"` → newline
- [ ] Handle non-UTF8 bytes (hex escapes)
- [ ] Test with complex paths and special chars

#### 1.4.3 Array Parser
- [ ] Implement `parse_array(s: &str) -> Result<Vec<String>>`
- [ ] Handle string arrays: `["arg1", "arg2"]`
- [ ] Handle nested quotes and escapes
- [ ] Handle empty arrays: `[]`
- [ ] Handle malformed arrays gracefully
- [ ] Test with execve argv/envp examples

#### 1.4.4 Struct Parser
- [ ] Implement `parse_struct(s: &str) -> Result<HashMap<String, String>>`
- [ ] Handle basic structs: `{field=value, field2=value2}`
- [ ] Handle nested structs: `{a={b=c}}`
- [ ] Handle function calls in values: `makedev(0, 0x4c)`
- [ ] Handle partial structs (with `...`)
- [ ] Test with sockaddr and stat examples

#### 1.4.5 File Descriptor Parser
- [ ] Implement `parse_fd(s: &str) -> Result<FileDescriptor>`
- [ ] Handle annotated FDs: `3</etc/hosts>` → `(3, Some("/etc/hosts"))`
- [ ] Handle socket annotations: `3<TCP:[127.0.0.1:9999]>`
- [ ] Handle bare FDs: `3` → `(3, None)`
- [ ] Test with all annotation types

#### 1.4.6 SockAddr Parser
- [ ] Implement `parse_sockaddr(s: &str) -> Result<SockAddr>`
- [ ] Parse AF_INET: `{sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr("127.0.0.1")}`
- [ ] Extract IP from `inet_addr("...")`
- [ ] Extract port from `htons(...)`
- [ ] Handle AF_INET6 (future)
- [ ] Handle AF_UNIX (future)
- [ ] Test with bind/connect examples

#### 1.4.7 Numeric Parser
- [ ] Implement `parse_numeric(s: &str) -> Result<NumericValue>`
- [ ] Recognize constants: `"AT_FDCWD"` → `Constant { name: "AT_FDCWD", value: Some(-100) }`
- [ ] Parse decimals: `"123"` → `Literal(123)`
- [ ] Parse hex: `"0x7b"` → `Literal(123)`
- [ ] Parse octal: `"0755"` → `Literal(493)`
- [ ] Maintain constant table for common values
- [ ] Test with all numeric patterns

### 1.5 Testing

- [ ] Create `tests/` directory in `parse_syscall/`
- [ ] Write unit tests for each utility (>90% coverage goal)
- [ ] Create test data from real traces
- [ ] Test error handling and edge cases
- [ ] Benchmark parsing performance

### 1.6 Documentation

- [ ] Document each utility function with examples
- [ ] Create usage guide in `parse_syscall/README.md`
- [ ] Document known limitations and edge cases

**Deliverable**: Reusable parsing library with comprehensive tests

---

## Phase 2: High-Priority Syscalls (Week 3-4) ⏳ NEXT

### 2.1 openat Parser

- [ ] Define `OpenAtSyscall` struct in `file.rs`
  ```rust
  pub struct OpenAtSyscall {
      pub dirfd: NumericValue,
      pub pathname: String,
      pub flags: Vec<String>,
      pub mode: Option<u32>,
      pub return_fd: Option<u32>,
      pub resolved_path: Option<String>,
  }
  ```
- [ ] Implement `parse_openat(raw: &RawSyscall) -> Result<OpenAtSyscall>`
- [ ] Handle optional mode parameter (O_CREAT check)
- [ ] Parse return FD with annotation
- [ ] Write tests with 10+ real examples
- [ ] Test error cases (ENOENT, etc.)

### 2.2 read Parser

- [ ] Define `ReadSyscall` struct in `file.rs`
- [ ] Implement `parse_read(raw: &RawSyscall) -> Result<ReadSyscall>`
- [ ] Handle buffer content (may be hex or string)
- [ ] Handle truncated buffers (`"..."`)
- [ ] Parse byte counts
- [ ] Write tests with various buffer types

### 2.3 write Parser

- [ ] Define `WriteSyscall` struct in `file.rs`
- [ ] Implement `parse_write(raw: &RawSyscall) -> Result<WriteSyscall>`
- [ ] Handle buffer content
- [ ] Parse byte counts
- [ ] Write tests

### 2.4 execve Parser

- [ ] Define `ExecveSyscall` struct in `process.rs`
  ```rust
  pub struct ExecveSyscall {
      pub pathname: String,
      pub argv: Vec<String>,
      pub envp: Vec<String>,
      pub return_code: i32,
  }
  ```
- [ ] Implement `parse_execve(raw: &RawSyscall) -> Result<ExecveSyscall>`
- [ ] Parse argv array (can be 100+ elements)
- [ ] Parse envp array (can be 100+ elements)
- [ ] Handle PATH search failures (multiple execve calls)
- [ ] Write tests with real examples

### 2.5 Integration

- [ ] Add all 4 parsers to `ParsedSyscall` enum
- [ ] Implement dispatcher in `mod.rs`:
  ```rust
  pub fn parse_syscall(raw: &RawSyscall) -> ParsedSyscall {
      match (raw.syscall_name.as_str(), &raw.category) {
          ("openat", SyscallCategory::File) => parse_openat(raw),
          ("read", SyscallCategory::File) => parse_read(raw),
          ("write", SyscallCategory::File) => parse_write(raw),
          ("execve", SyscallCategory::Process) => parse_execve(raw),
          _ => ParsedSyscall::Unparsed(raw.clone()),
      }
  }
  ```
- [ ] Write integration tests with full trace files

**Deliverable**: Working parsers for 4 most common syscalls

---

## Phase 3: Database Integration (Week 5) ⏳ PENDING

### 3.1 Database Migration

- [ ] Write SQL migration file:
  ```sql
  ALTER TABLE syscalls ADD COLUMN parsed_data JSONB;
  CREATE INDEX idx_syscalls_parsed_type
    ON syscalls((parsed_data->>'syscall_type'));
  ```
- [ ] Test migration on dev database
- [ ] Document rollback procedure

### 3.2 Serialization

- [ ] Implement `to_json()` for `OpenAtSyscall`
- [ ] Implement `to_json()` for `ReadSyscall`
- [ ] Implement `to_json()` for `WriteSyscall`
- [ ] Implement `to_json()` for `ExecveSyscall`
- [ ] Add `syscall_type` field to all JSON output
- [ ] Test JSON schema matches design

### 3.3 Model Updates

- [ ] Add `parsed_data` field to Syscall model in `src/models.rs`
- [ ] Implement serialization/deserialization
- [ ] Update `new()` constructor

### 3.4 Insert Logic

- [ ] Update `src/transfer.rs` to call parser
- [ ] Update insert statement to include parsed_data
- [ ] Handle parse failures gracefully (null in parsed_data)
- [ ] Test with real trace insertions

### 3.5 Testing

- [ ] End-to-end test: trace → parse → insert → query
- [ ] Verify JSONB queries work
- [ ] Performance test with 10,000+ syscalls

**Deliverable**: Syscalls stored with structured data

---

## Phase 4: Network Syscalls (Week 6-7) ⏳ PENDING

### 4.1 socket Parser

- [ ] Define `SocketSyscall` struct in `network.rs`
- [ ] Implement `parse_socket()`
- [ ] Parse domain constants (AF_INET, AF_INET6)
- [ ] Parse type with flags (SOCK_STREAM|SOCK_CLOEXEC)
- [ ] Parse protocol constants
- [ ] Parse return FD with socket annotation
- [ ] Write tests

### 4.2 bind Parser

- [ ] Define `BindSyscall` struct in `network.rs`
- [ ] Implement `parse_bind()`
- [ ] Parse sockaddr using `parse_sockaddr()`
- [ ] Extract IP and port
- [ ] Write tests

### 4.3 connect Parser

- [ ] Define `ConnectSyscall` struct in `network.rs`
- [ ] Implement `parse_connect()`
- [ ] Parse sockaddr
- [ ] Write tests

### 4.4 sendto Parser

- [ ] Define `SendToSyscall` struct in `network.rs`
- [ ] Implement `parse_sendto()`
- [ ] Parse buffer content
- [ ] Handle NULL dest_addr for connected sockets
- [ ] Write tests

### 4.5 recvfrom Parser

- [ ] Define `RecvFromSyscall` struct in `network.rs`
- [ ] Implement `parse_recvfrom()`
- [ ] Handle `<unfinished ...>` pattern
- [ ] Parse received data
- [ ] Write tests

### 4.6 Integration

- [ ] Add network syscalls to `ParsedSyscall` enum
- [ ] Update dispatcher
- [ ] Implement to_json() for all
- [ ] Update database schema if needed
- [ ] Write integration tests

**Deliverable**: Full network syscall support

---

## Phase 5: Display Layer (Week 8) ⏳ PENDING

### 5.1 Display Trait Implementation

- [ ] Implement `Display` for `OpenAtSyscall`
  - Format: `"[File] Opened /etc/hosts for reading (fd=3)"`
- [ ] Implement `Display` for `ReadSyscall`
  - Format: `"[File] Read 1024 bytes from fd=3"`
- [ ] Implement `Display` for `WriteSyscall`
  - Format: `"[File] Wrote 18 bytes to fd=4"`
- [ ] Implement `Display` for `ExecveSyscall`
  - Format: `"[Process] Executed /usr/bin/python3 with args: python3 script.py"`
- [ ] Implement `Display` for network syscalls
  - Format: `"[Network] Connected to 127.0.0.1:9999 via TCP"`

### 5.2 Query Integration

- [ ] Add `--format` flag to query command (raw vs human)
- [ ] Implement human-readable output mode
- [ ] Add filtering by parsed fields
- [ ] Create example forensic queries:
  - "All files written to /tmp"
  - "All network connections to external IPs"
  - "All python processes spawned"

### 5.3 Documentation

- [ ] Update query examples in README
- [ ] Document new display format
- [ ] Add forensic query cookbook

**Deliverable**: Rich, human-readable trace output

---

## Phase 6: Remaining Syscalls (Week 9+) ⏳ PENDING

### Priority 2 Syscalls

- [ ] newfstatat (complex stat struct)
- [ ] readlinkat (symlink handling)
- [ ] accept/accept4 (sockaddr output param)
- [ ] listen (simple)

### Priority 3 Syscalls

- [ ] clone (very complex flags)
- [ ] wait4 (status struct)
- [ ] Additional file syscalls as needed
- [ ] Additional network syscalls as needed

---

## Ongoing Tasks

### Testing
- [ ] Maintain >90% test coverage
- [ ] Add integration tests for each new syscall
- [ ] Performance benchmarks
- [ ] Test with diverse trace files

### Documentation
- [ ] Keep SYSCALL_REFERENCE.md updated
- [ ] Document new parsers
- [ ] Update schema documentation
- [ ] Add examples to guides

### Refactoring
- [ ] Identify common patterns
- [ ] Extract reusable code
- [ ] Optimize performance bottlenecks

---

## Success Metrics

### Phase 1 Success
- [ ] All utilities have >90% test coverage
- [ ] Can parse all basic data types
- [ ] Benchmarks show <1ms per utility call

### Phase 2 Success
- [ ] 95%+ parse success rate for 4 syscalls
- [ ] Handles error cases gracefully
- [ ] Falls back to RawSyscall on parse failure

### Phase 3 Success
- [ ] Can insert parsed syscalls into database
- [ ] Can query by parsed fields
- [ ] Existing raw_args still preserved
- [ ] No data loss

### Phase 4 Success
- [ ] Can extract IP addresses and ports
- [ ] Can identify all external network connections
- [ ] Handles both TCP and UDP

### Overall Success
- [ ] 95%+ parse success rate for all targeted syscalls
- [ ] <10ms parsing time per syscall
- [ ] 100% test coverage for parsing utilities
- [ ] Zero data loss (raw_args always preserved)
- [ ] Human-readable trace output
- [ ] Enables forensic queries

---

## Notes & Learnings

### 2025-10-06
- Created comprehensive implementation plan
- Identified need for robust struct/array parsers
- Key insight: each syscall needs custom parsing logic
- Priority: build utilities first, then syscalls

### Add notes here as implementation progresses...

---

## Quick Reference

### Common Constants
- `AT_FDCWD = -100` (current working directory)
- `AF_INET = 2` (IPv4)
- `SOCK_STREAM = 1` (TCP)
- `O_RDONLY = 0` (read-only)

### Strace Format Patterns
- Flags: `FLAG1|FLAG2|FLAG3`
- Structs: `{field=value, field=value}`
- Arrays: `["item", "item", "item"]`
- Strings: `"quoted string"`
- FDs: `3</path/to/file>`

### File Locations
- Parsing code: `src/parse_syscall/`
- Tests: `src/parse_syscall/tests/`
- Examples: `scripts/script_outputs/*/raw_trace.txt`
- Reference: `SYSCALL_REFERENCE.md`
- Plan: `SYSCALL_PARSING_EXPANSION_PLAN.md`
