//! Universal syscall event representation
//!
//! This module defines the base SyscallEvent that can represent any syscall
//! from any tracer (strace, eBPF, dtrace, etc.). It's platform-agnostic and
//! contains only the raw syscall information without domain-specific parsing.
//!
//! SyscallEvent is the universal currency of the system - everything downstream
//! works with these, then converts to domain-specific events as needed.

use serde::{Deserialize, Serialize};

/// Universal syscall event that can represent any system call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallEvent {
    /// Process ID that made the syscall
    pub pid: u32,
    /// Timestamp in microseconds since epoch
    pub timestamp: u64,
    /// Name of the syscall (e.g., "execve", "clone", "open", "socket")
    pub syscall_name: String,
    /// Raw argument strings as parsed from tracer output
    pub args: Vec<String>,
    /// Raw result string from syscall (if available)
    pub result: Option<String>,
    /// Complete raw line from tracer for debugging/logging
    pub raw_line: String,
}

/// Categories of syscalls for initial filtering and routing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyscallCategory {
    /// Process lifecycle syscalls
    Process(ProcessSyscall),
    /// File I/O syscalls
    FileIo(FileIoSyscall),
    /// Network syscalls
    Network(NetworkSyscall),
    /// Credential/permission syscalls
    Credential(CredentialSyscall),
    /// Memory management syscalls
    Memory(MemorySyscall),
    /// Signal handling syscalls
    Signal(SignalSyscall),
    /// Unknown or uncategorized syscalls
    Unknown,
}

/// Process lifecycle syscalls
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessSyscall {
    Execve,
    Clone,
    Fork,
    VFork,
    Exit, // Added for handling exit() syscall
    ExitGroup,
    ProcessExited, // Synthetic event for strace "+++ exited +++" annotations
    Wait4,
    WaitPid,
}

/// File I/O syscalls
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileIoSyscall {
    Open,
    OpenAt,
    FAccessAt,
    FAccessAt2,
    Read,
    Write,
    Close,
    Unlink,
    UnlinkAt,
    Rename,
    RenameAt,
    Mkdir,
    Rmdir,
    Stat,
    FStat,
    NewFStatAt,
    LStat,
    Access,
    ReadLinkAt,
    Chmod,
    FChmod,
    Chown,
    FChown,
    LChown,
}

/// Network syscalls
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkSyscall {
    Socket,
    Connect,
    Bind,
    Listen,
    Accept,
    Accept4,
    Send,
    Recv,
    SendTo,
    RecvFrom,
    SendMsg,
    RecvMsg,
    GetSockName,
    GetPeerName,
    SetSockOpt,
    GetSockOpt,
}

/// Credential/permission syscalls
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialSyscall {
    SetUid,
    SetGid,
    SetEUid,
    SetEGid,
    SetReUid,
    SetReGid,
    SetResUid,
    SetResGid,
    GetUid,
    GetGid,
    GetEUid,
    GetEGid,
}

/// Memory management syscalls
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemorySyscall {
    Mmap,
    Munmap,
    Mprotect,
    Brk,
    Sbrk,
    Mlock,
    Munlock,
    MlockAll,
    MunlockAll,
}

/// Signal handling syscalls
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalSyscall {
    Kill,
    Signal,
    SigAction,
    SigProcMask,
    SigSuspend,
    SigPending,
}

impl SyscallEvent {
    /// Create a new syscall event
    pub fn new(
        pid: u32,
        timestamp: u64,
        syscall_name: String,
        args: Vec<String>,
        result: Option<String>,
        raw_line: String,
    ) -> Self {
        Self {
            pid,
            timestamp,
            syscall_name,
            args,
            result,
            raw_line,
        }
    }

    /// Categorize the syscall based on its name
    pub fn categorize(&self) -> SyscallCategory {
        categorize_syscall(&self.syscall_name)
    }
}

/// Categorize a syscall by its name
pub fn categorize_syscall(syscall_name: &str) -> SyscallCategory {
    match syscall_name {
        // Process lifecycle
        "execve" => SyscallCategory::Process(ProcessSyscall::Execve),
        "clone" => SyscallCategory::Process(ProcessSyscall::Clone),
        "fork" => SyscallCategory::Process(ProcessSyscall::Fork),
        "vfork" => SyscallCategory::Process(ProcessSyscall::VFork),
        "exit" => SyscallCategory::Process(ProcessSyscall::Exit),
        "exit_group" => SyscallCategory::Process(ProcessSyscall::ExitGroup),
        "process_exited" => SyscallCategory::Process(ProcessSyscall::ProcessExited), // Synthetic strace event
        "wait4" => SyscallCategory::Process(ProcessSyscall::Wait4),
        "waitpid" => SyscallCategory::Process(ProcessSyscall::WaitPid),

        // File I/O
        "open" => SyscallCategory::FileIo(FileIoSyscall::Open),
        "openat" => SyscallCategory::FileIo(FileIoSyscall::OpenAt),
        "faccessat" => SyscallCategory::FileIo(FileIoSyscall::FAccessAt),
        "faccessat2" => SyscallCategory::FileIo(FileIoSyscall::FAccessAt2),
        "read" => SyscallCategory::FileIo(FileIoSyscall::Read),
        "write" => SyscallCategory::FileIo(FileIoSyscall::Write),
        "close" => SyscallCategory::FileIo(FileIoSyscall::Close),
        "unlink" => SyscallCategory::FileIo(FileIoSyscall::Unlink),
        "unlinkat" => SyscallCategory::FileIo(FileIoSyscall::UnlinkAt),
        "rename" => SyscallCategory::FileIo(FileIoSyscall::Rename),
        "renameat" => SyscallCategory::FileIo(FileIoSyscall::RenameAt),
        "mkdir" => SyscallCategory::FileIo(FileIoSyscall::Mkdir),
        "rmdir" => SyscallCategory::FileIo(FileIoSyscall::Rmdir),
        "stat" => SyscallCategory::FileIo(FileIoSyscall::Stat),
        "fstat" => SyscallCategory::FileIo(FileIoSyscall::FStat),
        "newfstatat" => SyscallCategory::FileIo(FileIoSyscall::NewFStatAt),
        "lstat" => SyscallCategory::FileIo(FileIoSyscall::LStat),
        "access" => SyscallCategory::FileIo(FileIoSyscall::Access),
        "readlinkat" => SyscallCategory::FileIo(FileIoSyscall::ReadLinkAt),
        "chmod" => SyscallCategory::FileIo(FileIoSyscall::Chmod),
        "fchmod" => SyscallCategory::FileIo(FileIoSyscall::FChmod),
        "chown" => SyscallCategory::FileIo(FileIoSyscall::Chown),
        "fchown" => SyscallCategory::FileIo(FileIoSyscall::FChown),
        "lchown" => SyscallCategory::FileIo(FileIoSyscall::LChown),

        // Network
        "socket" => SyscallCategory::Network(NetworkSyscall::Socket),
        "connect" => SyscallCategory::Network(NetworkSyscall::Connect),
        "bind" => SyscallCategory::Network(NetworkSyscall::Bind),
        "listen" => SyscallCategory::Network(NetworkSyscall::Listen),
        "accept" => SyscallCategory::Network(NetworkSyscall::Accept),
        "accept4" => SyscallCategory::Network(NetworkSyscall::Accept4),
        "send" => SyscallCategory::Network(NetworkSyscall::Send),
        "recv" => SyscallCategory::Network(NetworkSyscall::Recv),
        "sendto" => SyscallCategory::Network(NetworkSyscall::SendTo),
        "recvfrom" => SyscallCategory::Network(NetworkSyscall::RecvFrom),
        "sendmsg" => SyscallCategory::Network(NetworkSyscall::SendMsg),
        "recvmsg" => SyscallCategory::Network(NetworkSyscall::RecvMsg),
        "getsockname" => SyscallCategory::Network(NetworkSyscall::GetSockName),
        "getpeername" => SyscallCategory::Network(NetworkSyscall::GetPeerName),
        "setsockopt" => SyscallCategory::Network(NetworkSyscall::SetSockOpt),
        "getsockopt" => SyscallCategory::Network(NetworkSyscall::GetSockOpt),

        // Credentials
        "setuid" => SyscallCategory::Credential(CredentialSyscall::SetUid),
        "setgid" => SyscallCategory::Credential(CredentialSyscall::SetGid),
        "seteuid" => SyscallCategory::Credential(CredentialSyscall::SetEUid),
        "setegid" => SyscallCategory::Credential(CredentialSyscall::SetEGid),
        "setreuid" => SyscallCategory::Credential(CredentialSyscall::SetReUid),
        "setregid" => SyscallCategory::Credential(CredentialSyscall::SetReGid),
        "setresuid" => SyscallCategory::Credential(CredentialSyscall::SetResUid),
        "setresgid" => SyscallCategory::Credential(CredentialSyscall::SetResGid),
        "getuid" => SyscallCategory::Credential(CredentialSyscall::GetUid),
        "getgid" => SyscallCategory::Credential(CredentialSyscall::GetGid),
        "geteuid" => SyscallCategory::Credential(CredentialSyscall::GetEUid),
        "getegid" => SyscallCategory::Credential(CredentialSyscall::GetEGid),

        // Memory
        "mmap" => SyscallCategory::Memory(MemorySyscall::Mmap),
        "munmap" => SyscallCategory::Memory(MemorySyscall::Munmap),
        "mprotect" => SyscallCategory::Memory(MemorySyscall::Mprotect),
        "brk" => SyscallCategory::Memory(MemorySyscall::Brk),
        "sbrk" => SyscallCategory::Memory(MemorySyscall::Sbrk),
        "mlock" => SyscallCategory::Memory(MemorySyscall::Mlock),
        "munlock" => SyscallCategory::Memory(MemorySyscall::Munlock),
        "mlockall" => SyscallCategory::Memory(MemorySyscall::MlockAll),
        "munlockall" => SyscallCategory::Memory(MemorySyscall::MunlockAll),

        // Signals
        "kill" => SyscallCategory::Signal(SignalSyscall::Kill),
        "signal" => SyscallCategory::Signal(SignalSyscall::Signal),
        "sigaction" => SyscallCategory::Signal(SignalSyscall::SigAction),
        "sigprocmask" => SyscallCategory::Signal(SignalSyscall::SigProcMask),
        "sigsuspend" => SyscallCategory::Signal(SignalSyscall::SigSuspend),
        "sigpending" => SyscallCategory::Signal(SignalSyscall::SigPending),

        // Default to unknown
        _ => SyscallCategory::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_process_syscalls() {
        assert_eq!(
            categorize_syscall("execve"),
            SyscallCategory::Process(ProcessSyscall::Execve)
        );
        assert_eq!(
            categorize_syscall("clone"),
            SyscallCategory::Process(ProcessSyscall::Clone)
        );
        assert_eq!(
            categorize_syscall("fork"),
            SyscallCategory::Process(ProcessSyscall::Fork)
        );
        assert_eq!(
            categorize_syscall("exit_group"),
            SyscallCategory::Process(ProcessSyscall::ExitGroup)
        );
    }

    #[test]
    fn test_categorize_file_syscalls() {
        assert_eq!(
            categorize_syscall("open"),
            SyscallCategory::FileIo(FileIoSyscall::Open)
        );
        assert_eq!(
            categorize_syscall("read"),
            SyscallCategory::FileIo(FileIoSyscall::Read)
        );
        assert_eq!(
            categorize_syscall("write"),
            SyscallCategory::FileIo(FileIoSyscall::Write)
        );
        assert_eq!(
            categorize_syscall("close"),
            SyscallCategory::FileIo(FileIoSyscall::Close)
        );
    }

    #[test]
    fn test_categorize_network_syscalls() {
        assert_eq!(
            categorize_syscall("socket"),
            SyscallCategory::Network(NetworkSyscall::Socket)
        );
        assert_eq!(
            categorize_syscall("connect"),
            SyscallCategory::Network(NetworkSyscall::Connect)
        );
        assert_eq!(
            categorize_syscall("bind"),
            SyscallCategory::Network(NetworkSyscall::Bind)
        );
    }

    #[test]
    fn test_categorize_unknown_syscall() {
        assert_eq!(
            categorize_syscall("unknown_syscall"),
            SyscallCategory::Unknown
        );
        assert_eq!(categorize_syscall(""), SyscallCategory::Unknown);
    }

    #[test]
    fn test_syscall_event_categorize() {
        let event = SyscallEvent::new(
            1234,
            12345678,
            "execve".to_string(),
            vec![],
            None,
            "test".to_string(),
        );
        assert_eq!(
            event.categorize(),
            SyscallCategory::Process(ProcessSyscall::Execve)
        );
    }
}
