use serde_json::json;

use crate::AgentState;
use core::ProcessEvent;

/// Types of human-readable events we can emit
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanEventKind {
    Exec,
    Clone,
    Fork,
    VFork,
    Wait,
    ExitBegin,
    Exit,
    FileOpen,
    FileAccess,
    FileStat,
    SymlinkRead,
}

impl HumanEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HumanEventKind::Exec => "exec",
            HumanEventKind::Clone => "clone",
            HumanEventKind::Fork => "fork",
            HumanEventKind::VFork => "vfork",
            HumanEventKind::Wait => "wait",
            HumanEventKind::ExitBegin => "exit_begin",
            HumanEventKind::Exit => "exit",
            HumanEventKind::FileOpen => "file_open",
            HumanEventKind::FileAccess => "file_access",
            HumanEventKind::FileStat => "file_stat",
            HumanEventKind::SymlinkRead => "symlink_read",
        }
    }
}

/// Filter controls which kinds of human events are emitted
#[derive(Clone, Debug)]
pub struct HumanEventFilter {
    pub exec: bool,
    pub clone_: bool,
    pub fork: bool,
    pub vfork: bool,
    pub wait: bool,
    pub exit_begin: bool,
    pub exit: bool,
    pub file_open: bool,
    pub file_access: bool,
    pub file_stat: bool,
    pub symlink_read: bool,
}

impl HumanEventFilter {
    /// Default: enable exec, clone, fork, vfork; disable wait and exit events
    pub fn default() -> Self {
        Self {
            exec: true,
            clone_: true,
            fork: true,
            vfork: true,
            wait: false,
            exit_begin: false,
            exit: false,
            file_open: true,
            file_access: true,
            file_stat: false,
            symlink_read: true,
        }
    }

    pub fn is_enabled(&self, kind: HumanEventKind) -> bool {
        match kind {
            HumanEventKind::Exec => self.exec,
            HumanEventKind::Clone => self.clone_,
            HumanEventKind::Fork => self.fork,
            HumanEventKind::VFork => self.vfork,
            HumanEventKind::Wait => self.wait,
            HumanEventKind::ExitBegin => self.exit_begin,
            HumanEventKind::Exit => self.exit,
            HumanEventKind::FileOpen => self.file_open,
            HumanEventKind::FileAccess => self.file_access,
            HumanEventKind::FileStat => self.file_stat,
            HumanEventKind::SymlinkRead => self.symlink_read,
        }
    }

    /// Parse filter from env vars; falls back to default if unset or invalid
    /// CAPSULE_EVENTS enables explicit kinds (comma-separated), e.g. "exec,clone,wait"
    /// CAPSULE_EVENTS_DISABLE disables explicit kinds, e.g. "wait,exit,exit_begin"
    pub fn from_env_or_default() -> Self {
        let mut f = Self::default();

        if let Ok(enable_str) = std::env::var("CAPSULE_EVENTS") {
            // Reset all to false; enable listed kinds
            f.exec = false;
            f.clone_ = false;
            f.fork = false;
            f.vfork = false;
            f.wait = false;
            f.exit_begin = false;
            f.exit = false;
            for item in enable_str.split(',').map(|s| s.trim().to_lowercase()) {
                match item.as_str() {
                    "exec" => f.exec = true,
                    "clone" => f.clone_ = true,
                    "fork" => f.fork = true,
                    "vfork" => f.vfork = true,
                    "wait" => f.wait = true,
                    "exit_begin" => f.exit_begin = true,
                    "exit" => f.exit = true,
                    "file_open" => f.file_open = true,
                    "file_access" => f.file_access = true,
                    "file_stat" => f.file_stat = true,
                    "symlink_read" => f.symlink_read = true,
                    "*" | "all" => {
                        f.exec = true;
                        f.clone_ = true;
                        f.fork = true;
                        f.vfork = true;
                        f.wait = true;
                        f.exit_begin = true;
                        f.exit = true;
                        f.file_open = true;
                        f.file_access = true;
                        f.file_stat = true;
                        f.symlink_read = true;
                    }
                    _ => {}
                }
            }
        }

        if let Ok(disable_str) = std::env::var("CAPSULE_EVENTS_DISABLE") {
            for item in disable_str.split(',').map(|s| s.trim().to_lowercase()) {
                match item.as_str() {
                    "exec" => f.exec = false,
                    "clone" => f.clone_ = false,
                    "fork" => f.fork = false,
                    "vfork" => f.vfork = false,
                    "wait" => f.wait = false,
                    "exit_begin" => f.exit_begin = false,
                    "exit" => f.exit = false,
                    "file_open" => f.file_open = false,
                    "file_access" => f.file_access = false,
                    "file_stat" => f.file_stat = false,
                    "symlink_read" => f.symlink_read = false,
                    _ => {}
                }
            }
        }

        f
    }
}

/// Build a human-readable message and JSON `extra` for a ProcessEvent.
/// Returns (kind, message, extra). Timestamp and process_name are added by caller.
pub fn compose_process_event(
    state: &AgentState,
    event: &ProcessEvent,
) -> Option<(HumanEventKind, String, serde_json::Value)> {
    use core::ProcessEventType as T;

    match &event.event_type {
        T::Exec => {
            let cmd = if event.command_line.is_empty() {
                "<unknown>".to_string()
            } else {
                event.command_line.join(" ")
            };
            Some((HumanEventKind::Exec, format!("PID {} executed: {}", event.pid, cmd), json!({"argv": event.command_line})))
        }
        T::Clone { child_pid } => Some((
            HumanEventKind::Clone,
            format!("PID {} cloned child {}", event.pid, child_pid),
            json!({"child_pid": child_pid}),
        )),
        T::Fork { child_pid } => Some((
            HumanEventKind::Fork,
            format!("PID {} forked child {}", event.pid, child_pid),
            json!({"child_pid": child_pid}),
        )),
        T::VFork { child_pid } => Some((
            HumanEventKind::VFork,
            format!("PID {} vforked child {}", event.pid, child_pid),
            json!({"child_pid": child_pid}),
        )),
        T::Exit => {
            let msg = match event.exit_code {
                Some(code) => format!("PID {} began exiting (code {})", event.pid, code),
                None => format!("PID {} began exiting", event.pid),
            };
            Some((HumanEventKind::ExitBegin, msg, json!({"exit_code": event.exit_code})))
        }
        T::FullyExited => {
            let msg = match event.exit_code {
                Some(code) => format!("PID {} exited (code {})", event.pid, code),
                None => format!("PID {} exited", event.pid),
            };
            Some((HumanEventKind::Exit, msg, json!({"exit_code": event.exit_code})))
        }
        T::Wait { child_pid, child_exit_code } => {
            let msg = match child_exit_code {
                Some(code) => format!("PID {} waited for child {} (exit {})", event.pid, child_pid, code),
                None => format!("PID {} waited for child {}", event.pid, child_pid),
            };
            Some((HumanEventKind::Wait, msg, json!({"child_pid": child_pid, "child_exit_code": child_exit_code})))
        }
    }
}
