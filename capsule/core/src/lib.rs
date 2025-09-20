// Universal syscall representation
pub mod syscall_event;
// Domain-specific event types
pub mod domain_event;
pub mod process_event;

// Re-export commonly used types
pub use domain_event::DomainEvent;
pub use process_event::{ProcessEvent, ProcessEventType};
pub use syscall_event::{
    CredentialSyscall, FileIoSyscall, MemorySyscall, NetworkSyscall, ProcessSyscall, SignalSyscall,
    SyscallCategory, SyscallEvent,
};
