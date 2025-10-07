//! Core types for parsed syscall data
//!
//! This module defines the type hierarchy for representing parsed syscall arguments
//! in a structured, type-safe manner. All types support JSON serialization for
//! storage in the database's `parsed_data` JSONB column.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a numeric value that could be a constant or literal
///
/// Many syscall parameters can be either symbolic constants (like `AT_FDCWD`)
/// or numeric literals (like `123`). This enum preserves that distinction.
///
/// # Examples
///
/// ```
/// use parse_syscall::types::NumericValue;
///
/// let constant = NumericValue::Constant {
///     name: "AT_FDCWD".to_string(),
///     value: Some(-100),
/// };
///
/// let literal = NumericValue::Literal(123);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum NumericValue {
    /// A symbolic constant with optional numeric value
    #[serde(rename = "constant")]
    Constant {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<i64>,
    },
    /// A numeric literal
    #[serde(rename = "literal")]
    Literal(i64),
}

impl From<NumericValue> for u32 {
    fn from(val: NumericValue) -> Self {
        match val {
            NumericValue::Constant { value: Some(v), .. } => v as u32,
            NumericValue::Constant { value: None, .. } => 0,
            NumericValue::Literal(v) => v as u32,
        }
    }
}

impl From<NumericValue> for usize {
    fn from(val: NumericValue) -> Self {
        match val {
            NumericValue::Constant { value: Some(v), .. } => v as usize,
            NumericValue::Constant { value: None, .. } => 0,
            NumericValue::Literal(v) => v as usize,
        }
    }
}

/// Represents a file descriptor with optional annotation
///
/// When using `strace -yy`, file descriptors are annotated with additional
/// information like file paths or socket details.
///
/// # Examples
///
/// ```
/// use parse_syscall::types::FileDescriptor;
///
/// // Simple FD
/// let fd = FileDescriptor { fd: 3, annotation: None };
///
/// // FD with file path
/// let fd_with_path = FileDescriptor {
///     fd: 3,
///     annotation: Some("/etc/hosts".to_string())
/// };
///
/// // FD with socket info
/// let fd_with_socket = FileDescriptor {
///     fd: 4,
///     annotation: Some("TCP:[127.0.0.1:9999]".to_string())
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDescriptor {
    pub fd: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<String>,
}

/// Represents a socket address (any family)
///
/// Socket addresses can be IPv4, IPv6, Unix domain sockets, or unknown types.
/// This enum preserves the address family and extracts relevant fields.
///
/// # Examples
///
/// ```
/// use parse_syscall::types::SockAddr;
///
/// let inet = SockAddr::Inet {
///     ip: "127.0.0.1".to_string(),
///     port: 9999,
/// };
///
/// let unix = SockAddr::Unix {
///     path: "/tmp/socket".to_string(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "address_family")]
pub enum SockAddr {
    /// IPv4 address (AF_INET)
    #[serde(rename = "AF_INET")]
    Inet { ip: String, port: u16 },

    /// IPv6 address (AF_INET6)
    #[serde(rename = "AF_INET6")]
    Inet6 { ip: String, port: u16 },

    /// Unix domain socket (AF_UNIX)
    #[serde(rename = "AF_UNIX")]
    Unix { path: String },

    /// Unknown or unsupported address family
    #[serde(rename = "unknown")]
    Unknown { family_name: String, raw: String },
}

/// Top-level enum for all parsed syscalls
///
/// Each variant represents a specific syscall with its parsed arguments.
/// If parsing fails, the `Unparsed` variant preserves the raw data.
///
/// # JSON Serialization
///
/// All variants serialize with a `syscall_type` field for easy filtering:
///
/// ```json
/// {
///   "syscall_type": "openat",
///   "dirfd": { "type": "constant", "name": "AT_FDCWD", "value": -100 },
///   "pathname": "/etc/hosts",
///   "flags": ["O_RDONLY", "O_CLOEXEC"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "syscall_type")]
pub enum ParsedSyscall {
    // ============ FILE SYSCALLS ============
    /// openat(2) - open file relative to directory fd
    #[serde(rename = "openat")]
    OpenAt(OpenAtSyscall),

    /// read(2) - read from file descriptor
    #[serde(rename = "read")]
    Read(ReadSyscall),

    /// write(2) - write to file descriptor
    #[serde(rename = "write")]
    Write(WriteSyscall),

    /// newfstatat(2) - get file status
    #[serde(rename = "newfstatat")]
    NewFStatAt(NewFStatAtSyscall),

    /// readlinkat(2) - read symbolic link
    #[serde(rename = "readlinkat")]
    ReadLinkAt(ReadLinkAtSyscall),

    /// getcwd(2) - get current working directory
    #[serde(rename = "getcwd")]
    GetCwd(GetCwdSyscall),

    // ============ PROCESS SYSCALLS ============
    /// execve(2) - execute program
    #[serde(rename = "execve")]
    Execve(ExecveSyscall),

    /// clone(2) - create child process
    #[serde(rename = "clone")]
    Clone(CloneSyscall),

    /// wait4(2) - wait for process to change state
    #[serde(rename = "wait4")]
    Wait4(Wait4Syscall),

    // ============ NETWORK SYSCALLS ============
    /// socket(2) - create socket
    #[serde(rename = "socket")]
    Socket(SocketSyscall),

    /// bind(2) - bind socket to address
    #[serde(rename = "bind")]
    Bind(BindSyscall),

    /// connect(2) - connect socket to address
    #[serde(rename = "connect")]
    Connect(ConnectSyscall),

    /// listen(2) - listen for connections
    #[serde(rename = "listen")]
    Listen(ListenSyscall),

    /// accept4(2) - accept connection
    #[serde(rename = "accept4")]
    Accept4(Accept4Syscall),

    /// sendto(2) - send message on socket
    #[serde(rename = "sendto")]
    SendTo(SendToSyscall),

    /// recvfrom(2) - receive message from socket
    #[serde(rename = "recvfrom")]
    RecvFrom(RecvFromSyscall),

    /// Fallback for unparsed syscalls
    ///
    /// Used when parsing fails or the syscall type isn't yet implemented.
    /// Preserves raw data so nothing is lost.
    #[serde(rename = "unparsed")]
    Unparsed {
        syscall_name: String,
        raw_args: Vec<String>,
    },
}

// ============================================================================
// FILE SYSCALL STRUCTURES
// ============================================================================

/// Parsed representation of openat(2)
///
/// # Strace Format
/// ```text
/// openat(AT_FDCWD, "/etc/hosts", O_RDONLY|O_CLOEXEC) = 3</etc/hosts>
/// ```
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

/// Parsed representation of read(2)
///
/// # Strace Format
/// ```text
/// read(3</etc/hosts>, "127.0.0.1 localhost\n", 4096) = 22
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadSyscall {
    pub fd: FileDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_content: Option<String>,
    pub buffer_length: usize,
    pub bytes_read: i64,
}

/// Parsed representation of write(2)
///
/// # Strace Format
/// ```text
/// write(1<pipe:[12345]>, "Hello World\n", 12) = 12
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteSyscall {
    pub fd: FileDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_content: Option<String>,
    pub buffer_length: usize,
    pub bytes_written: i64,
}

/// Parsed representation of newfstatat(2)
///
/// # Strace Format
/// ```text
/// newfstatat(AT_FDCWD, "/usr/bin/python3", {st_mode=S_IFREG|0755, st_size=5268696, ...}, 0) = 0
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewFStatAtSyscall {
    pub dirfd: NumericValue,
    pub pathname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stat_struct: Option<HashMap<String, String>>,
    pub flags: u32,
    pub return_code: i32,
}

/// Parsed representation of readlinkat(2)
///
/// # Strace Format
/// ```text
/// readlinkat(AT_FDCWD, "/usr/bin/python3", "python3.9", 4096) = 9
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadLinkAtSyscall {
    pub dirfd: NumericValue,
    pub pathname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    pub buffer_size: usize,
    pub bytes_read: i64,
}

/// Parsed representation of getcwd(2)
///
/// # Strace Format
/// ```text
/// getcwd("/working/mini-capsule", 4096) = 22
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCwdSyscall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub buffer_size: usize,
    pub bytes_returned: i64,
}

// ============================================================================
// PROCESS SYSCALL STRUCTURES
// ============================================================================

/// Parsed representation of execve(2)
///
/// # Strace Format
/// ```text
/// execve("/usr/bin/python3", ["python3", "script.py"], ["PATH=/usr/bin", ...]) = 0
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecveSyscall {
    pub pathname: String,
    pub argv: Vec<String>,
    pub argc: usize,
    pub envp: Vec<String>,
    pub envc: usize,
    pub return_code: i32,
}

/// Parsed representation of clone(2)
///
/// # Strace Format
/// ```text
/// clone(child_stack=NULL, flags=CLONE_CHILD_CLEARTID|SIGCHLD, ...) = 1234
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloneSyscall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_stack: Option<String>,
    pub flags: Vec<String>,
    pub child_pid: i32,
}

/// Parsed representation of wait4(2)
///
/// # Strace Format
/// ```text
/// wait4(4120, [{WIFEXITED(s) && WEXITSTATUS(s) == 0}], 0, NULL) = 4120
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wait4Syscall {
    pub pid: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub options: u32,
    pub returned_pid: i32,
}

// ============================================================================
// NETWORK SYSCALL STRUCTURES
// ============================================================================

/// Parsed representation of socket(2)
///
/// # Strace Format
/// ```text
/// socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC, IPPROTO_IP) = 3<TCP:[18898905]>
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SocketSyscall {
    pub domain: String,
    pub socket_type: Vec<String>,
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_fd: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_annotation: Option<String>,
}

/// Parsed representation of bind(2)
///
/// # Strace Format
/// ```text
/// bind(3<TCP:[18898905]>, {sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindSyscall {
    pub sockfd: FileDescriptor,
    pub addr: SockAddr,
    pub addrlen: usize,
    pub return_code: i32,
}

/// Parsed representation of connect(2)
///
/// # Strace Format
/// ```text
/// connect(4<TCP:[18907120]>, {sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectSyscall {
    pub sockfd: FileDescriptor,
    pub addr: SockAddr,
    pub addrlen: usize,
    pub return_code: i32,
}

/// Parsed representation of listen(2)
///
/// # Strace Format
/// ```text
/// listen(3<TCP:[127.0.0.1:9999]>, 1) = 0
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListenSyscall {
    pub sockfd: FileDescriptor,
    pub backlog: u32,
    pub return_code: i32,
}

/// Parsed representation of accept4(2)
///
/// # Strace Format
/// ```text
/// accept4(3<TCP:[127.0.0.1:9999]>, {sa_family=AF_INET, sin_port=htons(32820), sin_addr=inet_addr("127.0.0.1")}, [16], SOCK_CLOEXEC) = 5<TCP:[127.0.0.1:9999->127.0.0.1:32820]>
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Accept4Syscall {
    pub sockfd: FileDescriptor,
    pub peer_addr: SockAddr,
    pub flags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_fd: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_annotation: Option<String>,
}

/// Parsed representation of sendto(2)
///
/// # Strace Format
/// ```text
/// sendto(4<TCP:[127.0.0.1:32820->127.0.0.1:9999]>, "Hello from client!", 18, 0, NULL, 0) = 18
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendToSyscall {
    pub sockfd: FileDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_content: Option<String>,
    pub buffer_length: usize,
    pub flags: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_addr: Option<SockAddr>,
    pub bytes_sent: i64,
}

/// Parsed representation of recvfrom(2)
///
/// # Strace Format
/// ```text
/// recvfrom(5<TCP:[127.0.0.1:9999->127.0.0.1:32820]>, "Hello from client!", 1024, 0, NULL, NULL) = 18
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecvFromSyscall {
    pub sockfd: FileDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_content: Option<String>,
    pub buffer_length: usize,
    pub flags: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_addr: Option<SockAddr>,
    pub bytes_received: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_value_serialization() {
        let constant = NumericValue::Constant {
            name: "AT_FDCWD".to_string(),
            value: Some(-100),
        };
        let json = serde_json::to_string(&constant).unwrap();
        assert!(json.contains("\"type\":\"constant\""));
        assert!(json.contains("\"name\":\"AT_FDCWD\""));
        assert!(json.contains("\"value\":-100"));

        let literal = NumericValue::Literal(123);
        let json = serde_json::to_string(&literal).unwrap();
        assert!(json.contains("\"type\":\"literal\""));
    }

    #[test]
    fn test_file_descriptor_serialization() {
        let fd = FileDescriptor {
            fd: 3,
            annotation: Some("/etc/hosts".to_string()),
        };
        let json = serde_json::to_string(&fd).unwrap();
        assert!(json.contains("\"fd\":3"));
        assert!(json.contains("\"/etc/hosts\""));
    }

    #[test]
    fn test_sockaddr_serialization() {
        let inet = SockAddr::Inet {
            ip: "127.0.0.1".to_string(),
            port: 9999,
        };
        let json = serde_json::to_string(&inet).unwrap();
        assert!(json.contains("\"address_family\":\"AF_INET\""));
        assert!(json.contains("\"ip\":\"127.0.0.1\""));
        assert!(json.contains("\"port\":9999"));
    }

    #[test]
    fn test_parsed_syscall_tagging() {
        let openat = ParsedSyscall::OpenAt(OpenAtSyscall {
            dirfd: NumericValue::Literal(-100),
            pathname: "/etc/hosts".to_string(),
            flags: vec!["O_RDONLY".to_string()],
            mode: None,
            return_fd: Some(3),
            resolved_path: Some("/etc/hosts".to_string()),
        });

        let json = serde_json::to_string(&openat).unwrap();
        assert!(json.contains("\"syscall_type\":\"openat\""));
        assert!(json.contains("\"/etc/hosts\""));
    }
}
