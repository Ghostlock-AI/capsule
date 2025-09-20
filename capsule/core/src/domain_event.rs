//! Domain-specific event enumeration
//!
//! Wraps all possible domain-specific events (Process, FileIo, Network, etc.)
//! This allows the system to handle different types of events uniformly
//! while maintaining type safety and extensibility.
//!
//! DomainEvent is used internally by state management and other consumers
//! that need to work with semantically parsed events rather than raw syscalls.

use crate::process_event::ProcessEvent;
use serde::{Deserialize, Serialize};

/// Domain-specific events converted from SyscallEvents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    /// Process lifecycle events (clone, fork, execve, exit, wait)
    Process(ProcessEvent),
    // Future domain events - commented out for now
    // /// File I/O events (open, read, write, close, etc.)
    // FileIo(FileIoEvent),
    // /// Network events (socket, connect, bind, etc.)
    // Network(NetworkEvent),
    // /// Credential events (setuid, setgid, etc.)
    // Credential(CredentialEvent),
    // /// Memory events (mmap, munmap, etc.)
    // Memory(MemoryEvent),
    // /// Signal events (kill, signal, etc.)
    // Signal(SignalEvent),
}

impl DomainEvent {
    /// Get the PID associated with this domain event
    pub fn pid(&self) -> u32 {
        match self {
            DomainEvent::Process(event) => event.pid,
            // Future: handle other domain event types
        }
    }

    /// Get the timestamp associated with this domain event
    pub fn timestamp(&self) -> u64 {
        match self {
            DomainEvent::Process(event) => event.timestamp,
            // Future: handle other domain event types
        }
    }
}

// Future domain event types - commented out for now
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct FileIoEvent {
//     pub pid: u32,
//     pub timestamp: u64,
//     pub operation: FileOperation,
//     pub path: Option<String>,
//     pub fd: Option<i32>,
//     pub bytes: Option<usize>,
//     pub flags: Option<i32>,
//     pub mode: Option<u32>,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub enum FileOperation {
//     Open { flags: i32, mode: Option<u32> },
//     Read { bytes: usize },
//     Write { bytes: usize },
//     Close,
//     Unlink,
//     Rename { old_path: String, new_path: String },
//     Mkdir { mode: u32 },
//     Rmdir,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct NetworkEvent {
//     pub pid: u32,
//     pub timestamp: u64,
//     pub operation: NetworkOperation,
//     pub socket_fd: Option<i32>,
//     pub address: Option<String>,
//     pub port: Option<u16>,
//     pub bytes: Option<usize>,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub enum NetworkOperation {
//     Socket { domain: i32, socket_type: i32, protocol: i32 },
//     Connect { address: String, port: u16 },
//     Bind { address: String, port: u16 },
//     Listen { backlog: i32 },
//     Accept,
//     Send { bytes: usize },
//     Recv { bytes: usize },
// }
