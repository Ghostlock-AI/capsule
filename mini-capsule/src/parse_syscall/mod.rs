//! Syscall parsing module
//!
//! This module provides structured parsing of strace output, converting raw string
//! arguments into type-safe, semantic representations. It sits on top of the existing
//! `RawSyscall` parser and provides a higher-level abstraction.
//!
//! # Architecture
//!
//! ```text
//! RawSyscall (trace.rs) → ParsedSyscall (parse_syscall) → JSONB (database)
//! ```
//!
//! # Usage
//!
//! ```rust
//! use mini_capsule::models::RawSyscall;
//! use mini_capsule::parse_syscall::{parse_syscall, ParsedSyscall};
//!
//! let raw = RawSyscall { /* ... */ };
//! let parsed = parse_syscall(&raw);
//!
//! match parsed {
//!     ParsedSyscall::OpenAt(openat) => {
//!         println!("Opened file: {}", openat.pathname);
//!     }
//!     ParsedSyscall::Unparsed { .. } => {
//!         // Parsing not implemented or failed, use raw data
//!     }
//!     _ => {}
//! }
//! ```
//!
//! # Graceful Degradation
//!
//! If parsing fails, the syscall is wrapped in `ParsedSyscall::Unparsed`, ensuring
//! that raw data is never lost. This allows incremental rollout of parsers.

pub mod types;
pub mod utils;

// Re-export commonly used types
pub use types::{
    Accept4Syscall, BindSyscall, ConnectSyscall, ExecveSyscall, FileDescriptor, ListenSyscall,
    NumericValue, OpenAtSyscall, ParsedSyscall, ReadSyscall, RecvFromSyscall, SendToSyscall,
    SockAddr, SocketSyscall, WriteSyscall,
};
pub use utils::{
    parse_array, parse_fd, parse_flags, parse_numeric, parse_return_code, parse_return_fd,
    parse_sockaddr, parse_string, parse_struct, ParseError,
};

use crate::models::{RawSyscall, SyscallCategory};

/// Parse a RawSyscall into a structured ParsedSyscall
///
/// This is the main entry point for syscall parsing. It dispatches to specific
/// parsers based on the syscall name and category.
///
/// # Examples
///
/// ```rust
/// use mini_capsule::models::RawSyscall;
/// use mini_capsule::parse_syscall::parse_syscall;
///
/// let raw = RawSyscall {
///     timestamp: "13:01:09.180154".to_string(),
///     pid: None,
///     syscall_number: 56,
///     syscall_name: "openat".to_string(),
///     raw_args: vec![
///         "AT_FDCWD".to_string(),
///         "\"/etc/hosts\"".to_string(),
///         "O_RDONLY|O_CLOEXEC".to_string(),
///     ],
///     raw_return: "3</etc/hosts>".to_string(),
///     category: SyscallCategory::File,
/// };
///
/// let parsed = parse_syscall(&raw);
/// ```
pub fn parse_syscall(raw: &RawSyscall) -> ParsedSyscall {
    // Dispatch to specific parser based on syscall name and category
    let result: Result<ParsedSyscall, ParseError> = match (raw.syscall_name.as_str(), &raw.category) {
        // File syscalls
        ("openat", SyscallCategory::File) => parse_openat(raw),
        ("read", SyscallCategory::File) => parse_read(raw),
        ("write", SyscallCategory::File) => parse_write(raw),

        // Process syscalls
        ("execve", SyscallCategory::Process) => parse_execve(raw),

        // Network syscalls
        ("socket", SyscallCategory::Network) => parse_socket(raw),
        ("bind", SyscallCategory::Network) => parse_bind(raw),
        ("connect", SyscallCategory::Network) => parse_connect(raw),
        ("listen", SyscallCategory::Network) => parse_listen(raw),
        ("accept4", SyscallCategory::Network) => parse_accept4(raw),
        ("sendto", SyscallCategory::Network) => parse_sendto(raw),
        ("recvfrom", SyscallCategory::Network) => parse_recvfrom(raw),

        // Fallback for unimplemented or unknown syscalls
        _ => Ok(ParsedSyscall::Unparsed {
            syscall_name: raw.syscall_name.clone(),
            raw_args: raw.raw_args.clone(),
        }),
    };

    // If parsing fails, wrap in Unparsed variant (silent fallback)
    result.unwrap_or_else(|_e| {
        ParsedSyscall::Unparsed {
            syscall_name: raw.syscall_name.clone(),
            raw_args: raw.raw_args.clone(),
        }
    })
}

// ============================================================================
// FILE SYSCALL PARSERS
// ============================================================================

fn parse_openat(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    // Validate argument count
    if raw.raw_args.len() < 3 {
        return Err(ParseError::MissingField("openat requires at least 3 args".to_string()));
    }

    // Parse arguments
    let dirfd = parse_numeric(&raw.raw_args[0])?;
    let pathname = parse_string(&raw.raw_args[1])?;
    let flags = parse_flags(&raw.raw_args[2]);

    // Check for optional mode parameter (present if O_CREAT in flags)
    let mode = if flags.contains(&"O_CREAT".to_string()) && raw.raw_args.len() > 3 {
        Some(parse_numeric(&raw.raw_args[3])?.into())
    } else {
        None
    };

    // Parse return value
    let (return_fd, resolved_path) = if raw.raw_return.starts_with('-') {
        (None, None)
    } else {
        let (fd, path) = parse_return_fd(&raw.raw_return)?;
        (Some(fd), path)
    };

    Ok(ParsedSyscall::OpenAt(OpenAtSyscall {
        dirfd,
        pathname,
        flags,
        mode,
        return_fd,
        resolved_path,
    }))
}

fn parse_read(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 3 {
        return Err(ParseError::MissingField("read requires 3 args".to_string()));
    }

    let fd = parse_fd(&raw.raw_args[0])?;
    let buffer_content = if raw.raw_args[1] != "NULL" {
        parse_string(&raw.raw_args[1]).ok()
    } else {
        None
    };
    let buffer_length = parse_numeric(&raw.raw_args[2])?.into();
    let bytes_read = parse_return_code(&raw.raw_return)? as i64;

    Ok(ParsedSyscall::Read(ReadSyscall {
        fd,
        buffer_content,
        buffer_length,
        bytes_read,
    }))
}

fn parse_write(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 3 {
        return Err(ParseError::MissingField("write requires 3 args".to_string()));
    }

    let fd = parse_fd(&raw.raw_args[0])?;
    let buffer_content = if raw.raw_args[1] != "NULL" {
        parse_string(&raw.raw_args[1]).ok()
    } else {
        None
    };
    let buffer_length = parse_numeric(&raw.raw_args[2])?.into();
    let bytes_written = parse_return_code(&raw.raw_return)? as i64;

    Ok(ParsedSyscall::Write(WriteSyscall {
        fd,
        buffer_content,
        buffer_length,
        bytes_written,
    }))
}

// ============================================================================
// PROCESS SYSCALL PARSERS
// ============================================================================

fn parse_execve(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 3 {
        return Err(ParseError::MissingField("execve requires 3 args".to_string()));
    }

    let pathname = parse_string(&raw.raw_args[0])?;
    let argv = parse_array(&raw.raw_args[1])?;
    let argc = argv.len();
    let envp = parse_array(&raw.raw_args[2])?;
    let envc = envp.len();
    let return_code = parse_return_code(&raw.raw_return)?;

    Ok(ParsedSyscall::Execve(ExecveSyscall {
        pathname,
        argv,
        argc,
        envp,
        envc,
        return_code: return_code as i32,
    }))
}

// ============================================================================
// NETWORK SYSCALL PARSERS
// ============================================================================

fn parse_socket(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 3 {
        return Err(ParseError::MissingField("socket requires 3 args".to_string()));
    }

    let domain = raw.raw_args[0].trim().to_string();
    let socket_type = parse_flags(&raw.raw_args[1]);
    let protocol = raw.raw_args[2].trim().to_string();

    let (return_fd, socket_annotation) = if raw.raw_return.starts_with('-') {
        (None, None)
    } else {
        let (fd, annotation) = parse_return_fd(&raw.raw_return)?;
        (Some(fd), annotation)
    };

    Ok(ParsedSyscall::Socket(SocketSyscall {
        domain,
        socket_type,
        protocol,
        return_fd,
        socket_annotation,
    }))
}

fn parse_bind(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 3 {
        return Err(ParseError::MissingField("bind requires 3 args".to_string()));
    }

    let sockfd = parse_fd(&raw.raw_args[0])?;
    let addr = parse_sockaddr(&raw.raw_args[1])?;
    let addrlen = parse_numeric(&raw.raw_args[2])?.into();
    let return_code = parse_return_code(&raw.raw_return)?;

    Ok(ParsedSyscall::Bind(BindSyscall {
        sockfd,
        addr,
        addrlen,
        return_code: return_code as i32,
    }))
}

fn parse_connect(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 3 {
        return Err(ParseError::MissingField("connect requires 3 args".to_string()));
    }

    let sockfd = parse_fd(&raw.raw_args[0])?;
    let addr = parse_sockaddr(&raw.raw_args[1])?;
    let addrlen = parse_numeric(&raw.raw_args[2])?.into();
    let return_code = parse_return_code(&raw.raw_return)?;

    Ok(ParsedSyscall::Connect(ConnectSyscall {
        sockfd,
        addr,
        addrlen,
        return_code: return_code as i32,
    }))
}

fn parse_listen(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 2 {
        return Err(ParseError::MissingField("listen requires 2 args".to_string()));
    }

    let sockfd = parse_fd(&raw.raw_args[0])?;
    let backlog = parse_numeric(&raw.raw_args[1])?.into();
    let return_code = parse_return_code(&raw.raw_return)?;

    Ok(ParsedSyscall::Listen(ListenSyscall {
        sockfd,
        backlog,
        return_code: return_code as i32,
    }))
}

fn parse_accept4(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 4 {
        return Err(ParseError::MissingField("accept4 requires 4 args".to_string()));
    }

    let sockfd = parse_fd(&raw.raw_args[0])?;
    let peer_addr = parse_sockaddr(&raw.raw_args[1])?;
    let flags = parse_flags(&raw.raw_args[3]);

    let (return_fd, connection_annotation) = if raw.raw_return.starts_with('-') {
        (None, None)
    } else {
        let (fd, annotation) = parse_return_fd(&raw.raw_return)?;
        (Some(fd), annotation)
    };

    Ok(ParsedSyscall::Accept4(Accept4Syscall {
        sockfd,
        peer_addr,
        flags,
        return_fd,
        connection_annotation,
    }))
}

fn parse_sendto(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 6 {
        return Err(ParseError::MissingField("sendto requires 6 args".to_string()));
    }

    let sockfd = parse_fd(&raw.raw_args[0])?;
    let buffer_content = if raw.raw_args[1] != "NULL" {
        parse_string(&raw.raw_args[1]).ok()
    } else {
        None
    };
    let buffer_length = parse_numeric(&raw.raw_args[2])?.into();
    let flags = parse_numeric(&raw.raw_args[3])?.into();
    let dest_addr = if raw.raw_args[4].trim() != "NULL" {
        Some(parse_sockaddr(&raw.raw_args[4])?)
    } else {
        None
    };
    let bytes_sent = parse_return_code(&raw.raw_return)? as i64;

    Ok(ParsedSyscall::SendTo(SendToSyscall {
        sockfd,
        buffer_content,
        buffer_length,
        flags,
        dest_addr,
        bytes_sent,
    }))
}

fn parse_recvfrom(raw: &RawSyscall) -> Result<ParsedSyscall, ParseError> {
    if raw.raw_args.len() < 6 {
        return Err(ParseError::MissingField("recvfrom requires 6 args".to_string()));
    }

    let sockfd = parse_fd(&raw.raw_args[0])?;
    let buffer_content = if raw.raw_args[1] != "NULL" {
        parse_string(&raw.raw_args[1]).ok()
    } else {
        None
    };
    let buffer_length = parse_numeric(&raw.raw_args[2])?.into();
    let flags = parse_numeric(&raw.raw_args[3])?.into();
    let src_addr = if raw.raw_args[4].trim() != "NULL" {
        Some(parse_sockaddr(&raw.raw_args[4])?)
    } else {
        None
    };
    let bytes_received = parse_return_code(&raw.raw_return)? as i64;

    Ok(ParsedSyscall::RecvFrom(RecvFromSyscall {
        sockfd,
        buffer_content,
        buffer_length,
        flags,
        src_addr,
        bytes_received,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // FILE SYSCALL TESTS
    // ========================================================================

    #[test]
    fn test_parse_openat_success() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 56,
            syscall_name: "openat".to_string(),
            raw_args: vec![
                "AT_FDCWD".to_string(),
                "\"/etc/hosts\"".to_string(),
                "O_RDONLY|O_CLOEXEC".to_string(),
            ],
            raw_return: "3</etc/hosts>".to_string(),
            category: SyscallCategory::File,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::OpenAt(openat) => {
                assert_eq!(openat.pathname, "/etc/hosts");
                assert_eq!(openat.flags, vec!["O_RDONLY", "O_CLOEXEC"]);
                assert_eq!(openat.return_fd, Some(3));
                assert_eq!(openat.resolved_path, Some("/etc/hosts".to_string()));
                assert!(openat.mode.is_none());
            }
            _ => panic!("Expected OpenAt variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_openat_with_mode() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 56,
            syscall_name: "openat".to_string(),
            raw_args: vec![
                "AT_FDCWD".to_string(),
                "\"/tmp/newfile\"".to_string(),
                "O_WRONLY|O_CREAT".to_string(),
                "0644".to_string(),
            ],
            raw_return: "4</tmp/newfile>".to_string(),
            category: SyscallCategory::File,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::OpenAt(openat) => {
                assert_eq!(openat.pathname, "/tmp/newfile");
                assert_eq!(openat.flags, vec!["O_WRONLY", "O_CREAT"]);
                assert_eq!(openat.return_fd, Some(4));
                assert!(openat.mode.is_some());
            }
            _ => panic!("Expected OpenAt variant"),
        }
    }

    #[test]
    fn test_parse_read() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 0,
            syscall_name: "read".to_string(),
            raw_args: vec![
                "3</etc/hosts>".to_string(),
                r#""127.0.0.1 localhost\n""#.to_string(),
                "4096".to_string(),
            ],
            raw_return: "22".to_string(),
            category: SyscallCategory::File,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Read(read) => {
                assert_eq!(read.fd.fd, 3);
                assert_eq!(read.fd.annotation, Some("/etc/hosts".to_string()));
                assert_eq!(read.buffer_content, Some("127.0.0.1 localhost\\n".to_string()));
                assert_eq!(read.buffer_length, 4096);
                assert_eq!(read.bytes_read, 22);
            }
            _ => panic!("Expected Read variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_write() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 1,
            syscall_name: "write".to_string(),
            raw_args: vec![
                "1<pipe:[12345]>".to_string(),
                r#""Hello World\n""#.to_string(),
                "12".to_string(),
            ],
            raw_return: "12".to_string(),
            category: SyscallCategory::File,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Write(write) => {
                assert_eq!(write.fd.fd, 1);
                assert_eq!(write.fd.annotation, Some("pipe:[12345]".to_string()));
                assert_eq!(write.buffer_content, Some("Hello World\\n".to_string()));
                assert_eq!(write.buffer_length, 12);
                assert_eq!(write.bytes_written, 12);
            }
            _ => panic!("Expected Write variant, got {:?}", parsed),
        }
    }

    // ========================================================================
    // PROCESS SYSCALL TESTS
    // ========================================================================

    #[test]
    fn test_parse_execve() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 59,
            syscall_name: "execve".to_string(),
            raw_args: vec![
                "\"/usr/bin/python3\"".to_string(),
                "[\"python3\", \"script.py\"]".to_string(),
                "[\"PATH=/usr/bin\", \"HOME=/root\"]".to_string(),
            ],
            raw_return: "0".to_string(),
            category: SyscallCategory::Process,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Execve(execve) => {
                assert_eq!(execve.pathname, "/usr/bin/python3");
                assert_eq!(execve.argv, vec!["python3", "script.py"]);
                assert_eq!(execve.argc, 2);
                assert_eq!(execve.envp, vec!["PATH=/usr/bin", "HOME=/root"]);
                assert_eq!(execve.envc, 2);
                assert_eq!(execve.return_code, 0);
            }
            _ => panic!("Expected Execve variant, got {:?}", parsed),
        }
    }

    // ========================================================================
    // NETWORK SYSCALL TESTS
    // ========================================================================

    #[test]
    fn test_parse_socket() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 41,
            syscall_name: "socket".to_string(),
            raw_args: vec![
                "AF_INET".to_string(),
                "SOCK_STREAM|SOCK_CLOEXEC".to_string(),
                "IPPROTO_IP".to_string(),
            ],
            raw_return: "3<TCP:[18898905]>".to_string(),
            category: SyscallCategory::Network,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Socket(socket) => {
                assert_eq!(socket.domain, "AF_INET");
                assert_eq!(socket.socket_type, vec!["SOCK_STREAM", "SOCK_CLOEXEC"]);
                assert_eq!(socket.protocol, "IPPROTO_IP");
                assert_eq!(socket.return_fd, Some(3));
                assert_eq!(socket.socket_annotation, Some("TCP:[18898905]".to_string()));
            }
            _ => panic!("Expected Socket variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_bind() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 49,
            syscall_name: "bind".to_string(),
            raw_args: vec![
                "3<TCP:[18898905]>".to_string(),
                "{sa_family=AF_INET, sin_port=htons(9999), sin_addr=inet_addr(\"127.0.0.1\")}".to_string(),
                "16".to_string(),
            ],
            raw_return: "0".to_string(),
            category: SyscallCategory::Network,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Bind(bind) => {
                assert_eq!(bind.sockfd.fd, 3);
                assert_eq!(bind.sockfd.annotation, Some("TCP:[18898905]".to_string()));
                match bind.addr {
                    SockAddr::Inet { ip, port } => {
                        assert_eq!(ip, "127.0.0.1");
                        assert_eq!(port, 9999);
                    }
                    _ => panic!("Expected Inet sockaddr"),
                }
                assert_eq!(bind.addrlen, 16);
                assert_eq!(bind.return_code, 0);
            }
            _ => panic!("Expected Bind variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_connect() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 42,
            syscall_name: "connect".to_string(),
            raw_args: vec![
                "4<TCP:[18907120]>".to_string(),
                "{sa_family=AF_INET, sin_port=htons(8080), sin_addr=inet_addr(\"192.168.1.1\")}".to_string(),
                "16".to_string(),
            ],
            raw_return: "0".to_string(),
            category: SyscallCategory::Network,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Connect(connect) => {
                assert_eq!(connect.sockfd.fd, 4);
                match connect.addr {
                    SockAddr::Inet { ip, port } => {
                        assert_eq!(ip, "192.168.1.1");
                        assert_eq!(port, 8080);
                    }
                    _ => panic!("Expected Inet sockaddr"),
                }
                assert_eq!(connect.return_code, 0);
            }
            _ => panic!("Expected Connect variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_listen() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 50,
            syscall_name: "listen".to_string(),
            raw_args: vec![
                "3<TCP:[127.0.0.1:9999]>".to_string(),
                "128".to_string(),
            ],
            raw_return: "0".to_string(),
            category: SyscallCategory::Network,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Listen(listen) => {
                assert_eq!(listen.sockfd.fd, 3);
                assert_eq!(listen.backlog, 128);
                assert_eq!(listen.return_code, 0);
            }
            _ => panic!("Expected Listen variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_accept4() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 288,
            syscall_name: "accept4".to_string(),
            raw_args: vec![
                "3<TCP:[127.0.0.1:9999]>".to_string(),
                "{sa_family=AF_INET, sin_port=htons(32820), sin_addr=inet_addr(\"127.0.0.1\")}".to_string(),
                "[16]".to_string(),
                "SOCK_CLOEXEC".to_string(),
            ],
            raw_return: "5<TCP:[127.0.0.1:9999->127.0.0.1:32820]>".to_string(),
            category: SyscallCategory::Network,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Accept4(accept) => {
                assert_eq!(accept.sockfd.fd, 3);
                match accept.peer_addr {
                    SockAddr::Inet { ip, port } => {
                        assert_eq!(ip, "127.0.0.1");
                        assert_eq!(port, 32820);
                    }
                    _ => panic!("Expected Inet sockaddr"),
                }
                assert_eq!(accept.flags, vec!["SOCK_CLOEXEC"]);
                assert_eq!(accept.return_fd, Some(5));
                assert_eq!(accept.connection_annotation, Some("TCP:[127.0.0.1:9999->127.0.0.1:32820]".to_string()));
            }
            _ => panic!("Expected Accept4 variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_sendto() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 44,
            syscall_name: "sendto".to_string(),
            raw_args: vec![
                "4<TCP:[127.0.0.1:32820->127.0.0.1:9999]>".to_string(),
                r#""GET / HTTP/1.1\r\n""#.to_string(),
                "16".to_string(),
                "0".to_string(),
                "NULL".to_string(),
                "0".to_string(),
            ],
            raw_return: "16".to_string(),
            category: SyscallCategory::Network,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::SendTo(sendto) => {
                assert_eq!(sendto.sockfd.fd, 4);
                assert_eq!(sendto.buffer_content, Some("GET / HTTP/1.1\\r\\n".to_string()));
                assert_eq!(sendto.buffer_length, 16);
                assert_eq!(sendto.flags, 0);
                assert!(sendto.dest_addr.is_none());
                assert_eq!(sendto.bytes_sent, 16);
            }
            _ => panic!("Expected SendTo variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_recvfrom() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 45,
            syscall_name: "recvfrom".to_string(),
            raw_args: vec![
                "5<TCP:[127.0.0.1:9999->127.0.0.1:32820]>".to_string(),
                r#""GET / HTTP/1.1\r\n""#.to_string(),
                "1024".to_string(),
                "0".to_string(),
                "NULL".to_string(),
                "NULL".to_string(),
            ],
            raw_return: "16".to_string(),
            category: SyscallCategory::Network,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::RecvFrom(recvfrom) => {
                assert_eq!(recvfrom.sockfd.fd, 5);
                assert_eq!(recvfrom.buffer_content, Some("GET / HTTP/1.1\\r\\n".to_string()));
                assert_eq!(recvfrom.buffer_length, 1024);
                assert_eq!(recvfrom.flags, 0);
                assert!(recvfrom.src_addr.is_none());
                assert_eq!(recvfrom.bytes_received, 16);
            }
            _ => panic!("Expected RecvFrom variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_parse_unknown_syscall_unparsed() {
        let raw = RawSyscall {
            timestamp: "13:01:09.180154".to_string(),
            pid: None,
            syscall_number: 999,
            syscall_name: "unknown_syscall".to_string(),
            raw_args: vec!["arg1".to_string(), "arg2".to_string()],
            raw_return: "0".to_string(),
            category: SyscallCategory::Unknown,
        };

        let parsed = parse_syscall(&raw);

        match parsed {
            ParsedSyscall::Unparsed { syscall_name, raw_args } => {
                assert_eq!(syscall_name, "unknown_syscall");
                assert_eq!(raw_args.len(), 2);
            }
            _ => panic!("Expected Unparsed variant"),
        }
    }
}
