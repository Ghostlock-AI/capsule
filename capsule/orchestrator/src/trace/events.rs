//! Event category to Tracee event name mapping
//!
//! This module defines the mapping between high-level event categories
//! (process, file, network, credentials, signal) and specific Tracee event names.

use std::collections::HashMap;

/// Event categories supported by Capsule tracing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    /// Process lifecycle events (exec, exit)
    Process,
    /// File operations (open, close, rename, unlink)
    File,
    /// Network operations (connect, bind, DNS)
    Network,
    /// Credential changes (setuid, setgid, commit_creds)
    Credentials,
    /// Signal operations (kill, signal_deliver)
    Signal,
}

impl EventCategory {
    /// Get all event categories
    pub fn all() -> Vec<EventCategory> {
        vec![
            EventCategory::Process,
            EventCategory::File,
            EventCategory::Network,
            EventCategory::Credentials,
            EventCategory::Signal,
        ]
    }

    /// Get human-readable name for this category
    pub fn name(&self) -> &'static str {
        match self {
            EventCategory::Process => "process",
            EventCategory::File => "file",
            EventCategory::Network => "network",
            EventCategory::Credentials => "credentials",
            EventCategory::Signal => "signal",
        }
    }
}

/// Get the list of Tracee event names for a given category
pub fn get_events_for_category(category: EventCategory) -> Vec<&'static str> {
    match category {
        EventCategory::Process => vec![
            "sched_process_exec", // Process execution (scheduler event)
            "execve",              // Process execution (syscall - backup)
            "exit_group",          // Process termination (clean exit)
            "exit",                // Process exit (fallback)
        ],
        EventCategory::File => vec![
            "openat",                  // File open/create with flags
            "close",                   // File/socket close
            "security_inode_rename",   // File rename (LSM hook)
            "security_inode_unlink",   // File deletion (LSM hook)
        ],
        EventCategory::Network => vec![
            "net_tcp_connect",          // TCP connections with dst IP:port
            "connect",                  // Raw connect syscall (backup)
            "security_socket_bind",     // Socket bind (LSM hook)
            "bind",                     // Raw bind syscall (backup)
            "net_packet_dns_request",   // DNS query
            "net_packet_dns_response",  // DNS response
        ],
        EventCategory::Credentials => vec![
            "security_bprm_check", // Binary execution permission check
            "commit_creds",        // Credential commit
            "setuid",              // Set user ID
            "setgid",              // Set group ID
        ],
        EventCategory::Signal => vec![
            "signal_deliver", // Signal delivery
            "kill",           // Kill syscall
        ],
    }
}

/// Get a mapping of all categories to their events
pub fn get_all_event_mappings() -> HashMap<EventCategory, Vec<&'static str>> {
    EventCategory::all()
        .into_iter()
        .map(|cat| (cat, get_events_for_category(cat)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_events() {
        let events = get_events_for_category(EventCategory::Process);
        assert_eq!(events.len(), 4);
        assert!(events.contains(&"sched_process_exec"));
        assert!(events.contains(&"execve"));
        assert!(events.contains(&"exit_group"));
        assert!(events.contains(&"exit"));
    }

    #[test]
    fn test_file_events() {
        let events = get_events_for_category(EventCategory::File);
        assert_eq!(events.len(), 4);
        assert!(events.contains(&"openat"));
        assert!(events.contains(&"close"));
        assert!(events.contains(&"security_inode_rename"));
        assert!(events.contains(&"security_inode_unlink"));
    }

    #[test]
    fn test_network_events() {
        let events = get_events_for_category(EventCategory::Network);
        assert_eq!(events.len(), 6);
        assert!(events.contains(&"net_tcp_connect"));
        assert!(events.contains(&"connect"));
        assert!(events.contains(&"security_socket_bind"));
        assert!(events.contains(&"bind"));
        assert!(events.contains(&"net_packet_dns_request"));
        assert!(events.contains(&"net_packet_dns_response"));
    }

    #[test]
    fn test_credentials_events() {
        let events = get_events_for_category(EventCategory::Credentials);
        assert_eq!(events.len(), 4);
        assert!(events.contains(&"security_bprm_check"));
        assert!(events.contains(&"commit_creds"));
        assert!(events.contains(&"setuid"));
        assert!(events.contains(&"setgid"));
    }

    #[test]
    fn test_signal_events() {
        let events = get_events_for_category(EventCategory::Signal);
        assert_eq!(events.len(), 2);
        assert!(events.contains(&"signal_deliver"));
        assert!(events.contains(&"kill"));
    }

    #[test]
    fn test_all_categories() {
        let categories = EventCategory::all();
        assert_eq!(categories.len(), 5);
    }

    #[test]
    fn test_all_event_mappings() {
        let mappings = get_all_event_mappings();
        assert_eq!(mappings.len(), 5);
        assert!(mappings.contains_key(&EventCategory::Process));
        assert!(mappings.contains_key(&EventCategory::File));
        assert!(mappings.contains_key(&EventCategory::Network));
        assert!(mappings.contains_key(&EventCategory::Credentials));
        assert!(mappings.contains_key(&EventCategory::Signal));
    }
}
