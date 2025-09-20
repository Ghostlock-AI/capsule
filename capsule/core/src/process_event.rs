//! Process lifecycle events
//!
//! Domain-specific events for process creation, execution, and termination.
//! These are converted from SyscallEvents when we detect process-related syscalls
//! like clone, fork, execve, exit_group, etc.
//!
//! ProcessEvent represents the semantic meaning of process syscalls,
//! while SyscallEvent represents the raw syscall data.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessEvent {
    /// Timestamp in microseconds since epoch
    pub timestamp: u64,
    /// Process ID of the event
    pub pid: u32,
    /// parent process ID (0 if unknown / root)
    pub ppid: u32,
    /// Type of process event
    pub event_type: ProcessEventType,
    /// full comand line including arguments
    /// Example: ["python3", "/usr/bin/script.py", "--verbose"]
    pub command_line: Vec<String>,
    /// working directory at tie of event (if available)
    pub working_dir: Option<String>,
    /// exit code (only for Exit events)
    pub exit_code: Option<i32>,
}

/// Process event types we track
///
/// Enhanced to track complete process lifecycle and parent-child relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessEventType {
    /// Process created via clone/fork (child PID known)
    Clone { child_pid: u32 },
    /// Process created via fork (child PID known)
    Fork { child_pid: u32 },
    /// Process created via vfork (child PID known)
    VFork { child_pid: u32 },
    /// Process executed new program (execve)
    Exec,
    /// Process started exit sequence (exit/exit_group syscall called)
    Exit,
    /// Process fully terminated (strace "+++ exited +++" or equivalent)
    FullyExited,
    /// Parent waited for child process (wait4, waitpid)
    Wait {
        child_pid: u32,
        child_exit_code: Option<i32>,
    },
}

impl ProcessEvent {
    /// Create a new clone event (parent creates child)
    pub fn clone(timestamp: u64, parent_pid: u32, child_pid: u32) -> Self {
        Self {
            timestamp,
            pid: parent_pid,
            ppid: 0, // Will be resolved by state tracker
            event_type: ProcessEventType::Clone { child_pid },
            command_line: Vec::new(), // No command line for clone
            working_dir: None,
            exit_code: None,
        }
    }

    /// Create a new fork event (parent creates child)
    pub fn fork(timestamp: u64, parent_pid: u32, child_pid: u32) -> Self {
        Self {
            timestamp,
            pid: parent_pid,
            ppid: 0, // Will be resolved by state tracker
            event_type: ProcessEventType::Fork { child_pid },
            command_line: Vec::new(), // No command line for fork
            working_dir: None,
            exit_code: None,
        }
    }

    /// Create a new vfork event (parent creates child)
    pub fn vfork(timestamp: u64, parent_pid: u32, child_pid: u32) -> Self {
        Self {
            timestamp,
            pid: parent_pid,
            ppid: 0, // Will be resolved by state tracker
            event_type: ProcessEventType::VFork { child_pid },
            command_line: Vec::new(), // No command line for vfork
            working_dir: None,
            exit_code: None,
        }
    }

    /// Create a new exec event (process executes new program)
    pub fn exec(
        timestamp: u64,
        pid: u32,
        ppid: u32,
        command_line: Vec<String>,
        working_dir: Option<String>,
    ) -> Self {
        Self {
            timestamp,
            pid,
            ppid,
            event_type: ProcessEventType::Exec,
            command_line,
            working_dir,
            exit_code: None,
        }
    }

    /// Create a new exit event (exit syscall called - process entering exit sequence)
    pub fn exit(timestamp: u64, pid: u32, exit_code: Option<i32>) -> Self {
        Self {
            timestamp,
            pid,
            ppid: 0, // not relevant for exit events
            event_type: ProcessEventType::Exit,
            command_line: Vec::new(), // not relevant for exit events
            working_dir: None,
            exit_code,
        }
    }

    /// Create a new fully exited event (process completely terminated)
    pub fn fully_exited(timestamp: u64, pid: u32, exit_code: Option<i32>) -> Self {
        Self {
            timestamp,
            pid,
            ppid: 0, // not relevant for exit events
            event_type: ProcessEventType::FullyExited,
            command_line: Vec::new(), // not relevant for exit events
            working_dir: None,
            exit_code,
        }
    }

    /// Create a new wait event (parent waits for child)
    pub fn wait(
        timestamp: u64,
        parent_pid: u32,
        child_pid: u32,
        child_exit_code: Option<i32>,
    ) -> Self {
        Self {
            timestamp,
            pid: parent_pid,
            ppid: 0, // not relevant for wait events
            event_type: ProcessEventType::Wait {
                child_pid,
                child_exit_code,
            },
            command_line: Vec::new(), // not relevant for wait events
            working_dir: None,
            exit_code: None,
        }
    }

    /// get the executable nae from command line
    pub fn executable(&self) -> Option<&str> {
        self.command_line.first().map(|s| s.as_str())
    }
}
