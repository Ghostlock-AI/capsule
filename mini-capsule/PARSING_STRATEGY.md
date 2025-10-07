# Syscall Parsing Strategy Document

**Created**: 2025-10-06
**Status**: Research Complete, Ready for Implementation
**For**: Phase 1 - Core Infrastructure

---

## Executive Summary

This document outlines the technical strategy for parsing strace output in Rust. Based on research into parsing libraries and strace format analysis, we recommend a **hybrid approach**: use `nom` for complex structures (arrays, structs) and simple Rust string methods for simpler patterns (flags, constants).

---

## Library Selection: nom vs pest vs regex vs manual

### Research Findings

| Library | Pros | Cons | Best For |
|---------|------|------|----------|
| **nom** | • Fastest performance<br>• Zero-copy parsing<br>• Excellent error handling<br>• Battle-tested (used in production) | • Steeper learning curve<br>• More verbose code | Binary formats, nested structures, production systems |
| **pest** | • Grammar-based (PEG)<br>• LSP support<br>• Good for prototyping | • Slower than nom<br>• Separate grammar file | DSLs, configuration formats |
| **regex** | • Built-in to Rust<br>• Simple to use | • Cannot handle nesting<br>• Not for context-dependent parsing | Simple patterns only |
| **Manual** | • Full control<br>• No dependencies | • Error-prone<br>• Reinventing wheel | Very simple parsing |

### Decision: Hybrid Approach

**Use nom for**:
- Nested structures: `{sa_family=AF_INET, sin_addr=inet_addr("127.0.0.1")}`
- Arrays: `["python3", "script.py", "arg1"]`
- Escaped strings: `"/path/with\"quotes"`
- Complex patterns requiring context

**Use manual string parsing for**:
- Flag splitting: `"O_RDONLY|O_CLOEXEC"` → simple `split('|')`
- Constant recognition: `"AT_FDCWD"` → hashmap lookup
- Simple numeric parsing: `"123"` → `parse::<i64>()`

**Rationale**:
- Leverage nom's performance for complex cases
- Keep simple cases simple
- Avoid over-engineering

---

## Strace Format Analysis

### Pattern Catalog

Based on analysis of `scripts/script_outputs/*/raw_trace.txt`:

| Pattern | Format | Frequency | Complexity |
|---------|--------|-----------|------------|
| **Quoted strings** | `"/etc/hosts"` | Very High | Low |
| **Bitwise flags** | `O_RDONLY\|O_CLOEXEC` | Very High | Low |
| **Structs** | `{field=value, ...}` | High | **High** |
| **Arrays** | `["item", "item"]` | Medium | **High** |
| **Constants** | `AT_FDCWD`, `AF_INET` | High | Low |
| **Hex numbers** | `0xffffdf523a40` | Medium | Low |
| **Decimals** | `123`, `-1` | Very High | Low |
| **Octals** | `0755` | Low | Low |
| **FD annotations** | `3</etc/hosts>` | High | Medium |
| **Socket annotations** | `3<TCP:[127.0.0.1:9999]>` | Medium | Medium |
| **Function calls** | `inet_addr("127.0.0.1")` | Medium | Medium |
| **Nested structs** | `{a={b=c}}` | Low | **Very High** |

### Critical Insight: Nesting Depth

**Most structs are 1-level deep**, occasionally 2-levels:
```
{sa_family=AF_INET, sin_addr=inet_addr("127.0.0.1")}  ← 1 level, but inet_addr() is a function call
{st_dev=makedev(0, 0x4c), st_mode=S_IFREG|0755, ...}  ← 1 level, makedev() is a function call
```

**Arrays can be 100+ elements** (execve argv/envp):
```
["python3", "script.py", "arg1", ..., "arg100"]
```

**Function calls within values**:
- `htons(9999)` - host to network short
- `inet_addr("127.0.0.1")` - IP address
- `makedev(0, 0x4c)` - device number

→ **Strategy**: Parse function calls as opaque strings, extract the meaningful part later

---

## Parsing Architecture

### Three-Layer Architecture

```
┌──────────────────────────────────────────────────────┐
│ Layer 1: Raw String Splitting (Manual)              │
│ Input: "AT_FDCWD, \"/etc/hosts\", O_RDONLY|O_CLOEXEC"│
│ Output: ["AT_FDCWD", "\"/etc/hosts\"", "O_RDONLY|..."]│
└────────────────────┬─────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────┐
│ Layer 2: Type Detection (Pattern Matching)          │
│ Classify each arg: String? Struct? Array? Flag?     │
└────────────────────┬─────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────┐
│ Layer 3: Specialized Parsing (nom + manual)         │
│ ├─ Strings: nom escaped_string                      │
│ ├─ Structs: nom delimited + separated_pair          │
│ ├─ Arrays: nom delimited + separated_list           │
│ ├─ Flags: manual split + trim                       │
│ └─ Constants: hashmap lookup                        │
└──────────────────────────────────────────────────────┘
```

### Why This Architecture?

1. **Layer 1 is already done** - `src/trace.rs` splits args by comma
2. **Layer 2 is simple** - pattern recognition via first char: `"` = string, `{` = struct, `[` = array
3. **Layer 3 is surgical** - apply right tool for right job

---

## nom Parser Implementations

### Parser 1: Escaped String

**Input**: `"\"/etc/hosts\""`
**Output**: `"/etc/hosts"`

```rust
use nom::{
    bytes::complete::{escaped, is_not, tag},
    character::complete::char,
    sequence::delimited,
    IResult,
};

pub fn parse_escaped_string(input: &str) -> IResult<&str, String> {
    let (input, content) = delimited(
        char('"'),                           // Opening quote
        escaped(is_not("\\\""), '\\', char('"')),  // Content with escapes
        char('"'),                           // Closing quote
    )(input)?;

    Ok((input, content.to_string()))
}

#[test]
fn test_parse_escaped_string() {
    assert_eq!(
        parse_escaped_string("\"/etc/hosts\""),
        Ok(("", "/etc/hosts".to_string()))
    );
    assert_eq!(
        parse_escaped_string("\"/path/with\\\"quotes\""),
        Ok(("", "/path/with\"quotes".to_string()))
    );
}
```

### Parser 2: Bitwise Flags (Manual)

**Input**: `"O_RDONLY|O_CLOEXEC"`
**Output**: `vec!["O_RDONLY", "O_CLOEXEC"]`

```rust
pub fn parse_flags(input: &str) -> Vec<String> {
    if input.trim() == "0" || input.trim().is_empty() {
        return vec![];
    }

    input
        .split('|')
        .map(|s| s.trim().to_string())
        .collect()
}

#[test]
fn test_parse_flags() {
    assert_eq!(
        parse_flags("O_RDONLY|O_CLOEXEC"),
        vec!["O_RDONLY", "O_CLOEXEC"]
    );
    assert_eq!(parse_flags("0"), Vec::<String>::new());
    assert_eq!(parse_flags("O_RDONLY"), vec!["O_RDONLY"]);
}
```

### Parser 3: Struct (nom)

**Input**: `"{sa_family=AF_INET, sin_port=htons(9999)}"`
**Output**: `HashMap { "sa_family": "AF_INET", "sin_port": "htons(9999)" }`

```rust
use nom::{
    bytes::complete::{tag, take_until},
    character::complete::{char, multispace0},
    multi::separated_list0,
    sequence::{delimited, separated_pair},
    IResult,
};
use std::collections::HashMap;

// Parse a single key=value pair
fn struct_pair(input: &str) -> IResult<&str, (&str, &str)> {
    separated_pair(
        take_until("="),                    // Key
        char('='),
        take_until_comma_or_brace,          // Value (may contain nested parens)
    )(input)
}

// Parse comma-separated list of pairs
fn struct_content(input: &str) -> IResult<&str, Vec<(&str, &str)>> {
    separated_list0(
        delimited(multispace0, char(','), multispace0),
        struct_pair,
    )(input)
}

// Main struct parser
pub fn parse_struct(input: &str) -> IResult<&str, HashMap<String, String>> {
    let (input, pairs) = delimited(
        char('{'),
        struct_content,
        char('}'),
    )(input)?;

    let map = pairs
        .into_iter()
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();

    Ok((input, map))
}

// Helper to consume until comma or closing brace (handling nested parens)
fn take_until_comma_or_brace(input: &str) -> IResult<&str, &str> {
    let mut depth = 0;
    let mut idx = 0;

    for (i, ch) in input.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => return Ok((&input[i..], &input[..i])),
            '}' if depth == 0 => return Ok((&input[i..], &input[..i])),
            _ => {}
        }
        idx = i + ch.len_utf8();
    }

    Ok(("", &input[..idx]))
}

#[test]
fn test_parse_struct() {
    let input = "{sa_family=AF_INET, sin_port=htons(9999)}";
    let (_, result) = parse_struct(input).unwrap();

    assert_eq!(result.get("sa_family"), Some(&"AF_INET".to_string()));
    assert_eq!(result.get("sin_port"), Some(&"htons(9999)".to_string()));
}
```

### Parser 4: Array (nom)

**Input**: `["python3", "script.py", "arg1"]`
**Output**: `vec!["python3", "script.py", "arg1"]`

```rust
use nom::{
    bytes::complete::tag,
    character::complete::{char, multispace0},
    multi::separated_list0,
    sequence::delimited,
    IResult,
};

pub fn parse_array(input: &str) -> IResult<&str, Vec<String>> {
    let (input, items) = delimited(
        char('['),
        separated_list0(
            delimited(multispace0, char(','), multispace0),
            parse_escaped_string,  // Reuse string parser
        ),
        char(']'),
    )(input)?;

    Ok((input, items))
}

#[test]
fn test_parse_array() {
    let input = r#"["python3", "script.py", "arg1"]"#;
    let (_, result) = parse_array(input).unwrap();

    assert_eq!(result, vec!["python3", "script.py", "arg1"]);
}
```

### Parser 5: File Descriptor (Manual + Regex)

**Input**: `"3</etc/hosts>"`
**Output**: `FileDescriptor { fd: 3, annotation: Some("/etc/hosts") }`

```rust
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref FD_REGEX: Regex = Regex::new(r"^(\d+)(?:<(.+?)>)?$").unwrap();
}

#[derive(Debug, PartialEq)]
pub struct FileDescriptor {
    pub fd: u32,
    pub annotation: Option<String>,
}

pub fn parse_fd(input: &str) -> Result<FileDescriptor, String> {
    let caps = FD_REGEX
        .captures(input.trim())
        .ok_or_else(|| format!("Invalid FD format: {}", input))?;

    let fd = caps[1].parse::<u32>()
        .map_err(|e| format!("Invalid FD number: {}", e))?;

    let annotation = caps.get(2).map(|m| m.as_str().to_string());

    Ok(FileDescriptor { fd, annotation })
}

#[test]
fn test_parse_fd() {
    assert_eq!(
        parse_fd("3</etc/hosts>"),
        Ok(FileDescriptor {
            fd: 3,
            annotation: Some("/etc/hosts".to_string())
        })
    );

    assert_eq!(
        parse_fd("3<TCP:[127.0.0.1:9999]>"),
        Ok(FileDescriptor {
            fd: 3,
            annotation: Some("TCP:[127.0.0.1:9999]".to_string())
        })
    );

    assert_eq!(
        parse_fd("3"),
        Ok(FileDescriptor {
            fd: 3,
            annotation: None
        })
    );
}
```

### Parser 6: Numeric Value (Manual)

**Input**: `"AT_FDCWD"`, `"123"`, `"0x7b"`, `"0755"`
**Output**: `NumericValue` enum

```rust
use lazy_static::lazy_static;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum NumericValue {
    Constant { name: String, value: Option<i64> },
    Literal(i64),
}

lazy_static! {
    static ref CONSTANTS: HashMap<&'static str, i64> = {
        let mut m = HashMap::new();
        // File constants
        m.insert("AT_FDCWD", -100);
        m.insert("O_RDONLY", 0);
        m.insert("O_WRONLY", 1);
        m.insert("O_RDWR", 2);
        // Network constants
        m.insert("AF_INET", 2);
        m.insert("AF_INET6", 10);
        m.insert("AF_UNIX", 1);
        m.insert("SOCK_STREAM", 1);
        m.insert("SOCK_DGRAM", 2);
        m.insert("IPPROTO_IP", 0);
        m.insert("IPPROTO_TCP", 6);
        m.insert("IPPROTO_UDP", 17);
        // Add more as needed
        m
    };
}

pub fn parse_numeric(input: &str) -> Result<NumericValue, String> {
    let trimmed = input.trim();

    // Check if it's a known constant
    if let Some(&value) = CONSTANTS.get(trimmed) {
        return Ok(NumericValue::Constant {
            name: trimmed.to_string(),
            value: Some(value),
        });
    }

    // Check if it looks like a constant (all caps with underscores)
    if trimmed.chars().all(|c| c.is_uppercase() || c == '_' || c.is_numeric()) {
        return Ok(NumericValue::Constant {
            name: trimmed.to_string(),
            value: None,  // Unknown constant
        });
    }

    // Try parsing as hex
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        let num = i64::from_str_radix(&trimmed[2..], 16)
            .map_err(|e| format!("Invalid hex: {}", e))?;
        return Ok(NumericValue::Literal(num));
    }

    // Try parsing as octal
    if trimmed.starts_with('0') && trimmed.len() > 1 && trimmed[1..].chars().all(|c| c.is_digit(8)) {
        let num = i64::from_str_radix(&trimmed[1..], 8)
            .map_err(|e| format!("Invalid octal: {}", e))?;
        return Ok(NumericValue::Literal(num));
    }

    // Parse as decimal (including negative)
    let num = trimmed.parse::<i64>()
        .map_err(|e| format!("Invalid number: {}", e))?;
    Ok(NumericValue::Literal(num))
}

#[test]
fn test_parse_numeric() {
    assert_eq!(
        parse_numeric("AT_FDCWD"),
        Ok(NumericValue::Constant {
            name: "AT_FDCWD".to_string(),
            value: Some(-100)
        })
    );

    assert_eq!(parse_numeric("123"), Ok(NumericValue::Literal(123)));
    assert_eq!(parse_numeric("-1"), Ok(NumericValue::Literal(-1)));
    assert_eq!(parse_numeric("0x7b"), Ok(NumericValue::Literal(123)));
    assert_eq!(parse_numeric("0755"), Ok(NumericValue::Literal(493)));
}
```

---

## Type Hierarchy Design

### Core Types

```rust
// src/parse_syscall/types.rs

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Represents a numeric value that could be a constant or literal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NumericValue {
    #[serde(rename = "constant")]
    Constant {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<i64>,
    },
    #[serde(rename = "literal")]
    Literal(i64),
}

/// Represents a file descriptor with optional annotation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDescriptor {
    pub fd: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<String>,
}

/// Represents a socket address (any family)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family")]
pub enum SockAddr {
    #[serde(rename = "AF_INET")]
    Inet { ip: String, port: u16 },

    #[serde(rename = "AF_INET6")]
    Inet6 { ip: String, port: u16 },

    #[serde(rename = "AF_UNIX")]
    Unix { path: String },

    #[serde(rename = "unknown")]
    Unknown { family: String, raw: String },
}

/// Top-level enum for all parsed syscalls
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "syscall_type")]
pub enum ParsedSyscall {
    #[serde(rename = "openat")]
    OpenAt(OpenAtSyscall),

    #[serde(rename = "read")]
    Read(ReadSyscall),

    #[serde(rename = "write")]
    Write(WriteSyscall),

    #[serde(rename = "execve")]
    Execve(ExecveSyscall),

    // Network syscalls (Phase 4)
    #[serde(rename = "socket")]
    Socket(SocketSyscall),

    #[serde(rename = "bind")]
    Bind(BindSyscall),

    // ... more syscalls ...

    /// Fallback for unparsed syscalls
    #[serde(rename = "unparsed")]
    Unparsed {
        syscall_name: String,
        raw_args: Vec<String>,
    },
}

// Specific syscall structs (examples)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenAtSyscall {
    pub dirfd: NumericValue,
    pub pathname: String,
    pub flags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_fd: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadSyscall {
    pub fd: FileDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_content: Option<String>,
    pub buffer_length: usize,
    pub bytes_read: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecveSyscall {
    pub pathname: String,
    pub argv: Vec<String>,
    pub argc: usize,
    pub envp: Vec<String>,
    pub envc: usize,
    pub return_code: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SocketSyscall {
    pub domain: String,
    pub socket_type: Vec<String>,  // Can have flags like SOCK_STREAM|SOCK_CLOEXEC
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_fd: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_annotation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindSyscall {
    pub sockfd: FileDescriptor,
    pub addr: SockAddr,
    pub return_code: i32,
}
```

### JSON Serialization Examples

```json
// OpenAt example
{
  "syscall_type": "openat",
  "dirfd": {
    "type": "constant",
    "name": "AT_FDCWD",
    "value": -100
  },
  "pathname": "/etc/hosts",
  "flags": ["O_RDONLY", "O_CLOEXEC"],
  "return_fd": 3,
  "resolved_path": "/etc/hosts"
}

// Execve example
{
  "syscall_type": "execve",
  "pathname": "/usr/bin/python3",
  "argv": ["python3", "script.py"],
  "argc": 2,
  "envp": ["PATH=/usr/bin", "HOME=/root"],
  "envc": 2,
  "return_code": 0
}

// Bind example
{
  "syscall_type": "bind",
  "sockfd": {
    "fd": 3,
    "annotation": "TCP:[18898905]"
  },
  "addr": {
    "family": "AF_INET",
    "ip": "127.0.0.1",
    "port": 9999
  },
  "return_code": 0
}
```

---

## Error Handling Strategy

### Three-Tier Error Handling

1. **Parse Errors** → Return `Err` from parser functions
2. **Conversion Errors** → Log warning, return `ParsedSyscall::Unparsed`
3. **Missing Data** → Use `Option<T>` fields

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("nom parsing failed: {0}")]
    NomError(String),

    #[error("Invalid numeric value: {0}")]
    InvalidNumber(String),
}

// Graceful fallback
pub fn parse_syscall_safe(raw: &RawSyscall) -> ParsedSyscall {
    match parse_syscall(raw) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("Failed to parse {}: {}", raw.syscall_name, e);
            ParsedSyscall::Unparsed {
                syscall_name: raw.syscall_name.clone(),
                raw_args: raw.raw_args.clone(),
            }
        }
    }
}
```

---

## Performance Considerations

### Benchmarking Plan

1. **Create benchmark suite** using `criterion`
2. **Measure per-parser performance**:
   - Target: <100μs per syscall
   - Acceptable: <1ms per syscall
   - Concern: >10ms per syscall

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_parse_struct(c: &mut Criterion) {
    let input = "{sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr(\"127.0.0.1\")}";

    c.bench_function("parse_struct", |b| {
        b.iter(|| parse_struct(black_box(input)))
    });
}

criterion_group!(benches, benchmark_parse_struct);
criterion_main!(benches);
```

### Optimization Strategies

If performance is an issue:
1. **Use `Cow<str>`** instead of `String` where possible (zero-copy)
2. **Cache regex compilations** (already using `lazy_static`)
3. **Parallelize parsing** with `rayon` (for batch processing)
4. **Profile with `flamegraph`** to identify bottlenecks

---

## Testing Strategy

### Test Pyramid

```
           ┌─────────────────┐
           │  Integration    │  ← End-to-end with real traces
           │  Tests (10%)    │
           ├─────────────────┤
           │  Parser Tests   │  ← Each syscall parser
           │  (30%)          │
           ├─────────────────┤
           │  Utility Tests  │  ← Each utility function
           │  (60%)          │  ← Property tests
           └─────────────────┘
```

### Test Data Sources

1. **Real traces**: Use `scripts/script_outputs/*/raw_trace.txt`
2. **Edge cases**: Manually craft tricky inputs
3. **Property tests**: Use `proptest` for fuzzing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_parse_flags_never_panics(s in ".*") {
        let _ = parse_flags(&s);  // Should never panic
    }

    #[test]
    fn test_parse_numeric_roundtrip(n in -1000i64..1000i64) {
        let s = n.to_string();
        let parsed = parse_numeric(&s).unwrap();
        assert_eq!(parsed, NumericValue::Literal(n));
    }
}
```

---

## Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
nom = "7.1"
regex = "1.10"
lazy_static = "1.4"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"

[dev-dependencies]
criterion = "0.5"
proptest = "1.4"
```

---

## Implementation Checklist

See `IMPLEMENTATION_CHECKLIST.md` for detailed task breakdown.

**Phase 1 Priorities**:
1. ✅ Research complete
2. ⏳ Implement utility parsers
3. ⏳ Write comprehensive tests
4. ⏳ Benchmark performance

---

## Next Steps

1. **Create module structure**: `src/parse_syscall/`
2. **Implement utilities**: Start with `parse_flags()` and `parse_numeric()`
3. **Test extensively**: Achieve >90% coverage
4. **Benchmark**: Ensure performance targets met
5. **Move to Phase 2**: Implement first syscall parser (openat)

---

## References

- nom documentation: https://docs.rs/nom
- nom guide: https://developerlife.com/2023/02/20/guide-to-nom-parsing/
- Strace man page: https://man7.org/linux/man-pages/man1/strace.1.html
- Our trace examples: `scripts/script_outputs/*/raw_trace.txt`
- Full plan: `SYSCALL_PARSING_EXPANSION_PLAN.md`
