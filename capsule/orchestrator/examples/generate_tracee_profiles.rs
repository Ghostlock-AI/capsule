//! Generate example Tracee configuration profiles
//!
//! This example generates Tracee YAML configurations for different tracing profiles:
//! - minimal: Only process events
//! - developer: Process, file, and network events
//! - full: All event categories
//!
//! Run with: cargo run --example generate_tracee_profiles

use std::fs;
use std::path::Path;

// We need to temporarily include the trace module code inline
// since we can't easily import from src/ in an example

mod trace {
    pub mod events {
        use std::collections::HashMap;

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum EventCategory {
            Process,
            File,
            Network,
            Credentials,
            Signal,
        }

        impl EventCategory {
            pub fn all() -> Vec<EventCategory> {
                vec![
                    EventCategory::Process,
                    EventCategory::File,
                    EventCategory::Network,
                    EventCategory::Credentials,
                    EventCategory::Signal,
                ]
            }
        }

        pub fn get_events_for_category(category: EventCategory) -> Vec<&'static str> {
            match category {
                EventCategory::Process => vec![
                    "sched_process_exec",
                    "execve",
                    "exit_group",
                    "exit",
                ],
                EventCategory::File => vec![
                    "openat",
                    "close",
                    "security_inode_rename",
                    "security_inode_unlink",
                ],
                EventCategory::Network => vec![
                    "net_tcp_connect",
                    "connect",
                    "security_socket_bind",
                    "bind",
                    "net_packet_dns_request",
                    "net_packet_dns_response",
                ],
                EventCategory::Credentials => vec![
                    "security_bprm_check",
                    "commit_creds",
                    "setuid",
                    "setgid",
                ],
                EventCategory::Signal => vec![
                    "signal_deliver",
                    "kill",
                ],
            }
        }
    }

    pub mod config_gen {
        use super::events::{EventCategory, get_events_for_category};

        #[derive(Debug, Clone)]
        pub struct TracingConfig {
            pub enabled: bool,
            pub events: EventCategories,
            pub scope: TraceScope,
        }

        #[derive(Debug, Clone)]
        pub struct EventCategories {
            pub process: bool,
            pub file: bool,
            pub network: bool,
            pub credentials: bool,
            pub signal: bool,
        }

        #[derive(Debug, Clone)]
        pub struct TraceScope {
            pub user: String,
            pub new_processes: bool,
            pub follow: bool,
        }

        impl Default for TraceScope {
            fn default() -> Self {
                Self {
                    user: "agent".to_string(),
                    new_processes: true,
                    follow: true,
                }
            }
        }

        pub fn generate_tracee_config(config: &TracingConfig) -> String {
            if !config.enabled {
                return String::new();
            }

            let mut events = Vec::new();

            if config.events.process {
                events.extend(get_events_for_category(EventCategory::Process));
            }
            if config.events.file {
                events.extend(get_events_for_category(EventCategory::File));
            }
            if config.events.network {
                events.extend(get_events_for_category(EventCategory::Network));
            }
            if config.events.credentials {
                events.extend(get_events_for_category(EventCategory::Credentials));
            }
            if config.events.signal {
                events.extend(get_events_for_category(EventCategory::Signal));
            }

            let events_yaml = events
                .iter()
                .map(|e| format!("    - {}", e))
                .collect::<Vec<_>>()
                .join("\n");

            let mut scope_filters = Vec::new();
            scope_filters.push(format!("  - uid=$({})", config.scope.user));
            if config.scope.new_processes {
                scope_filters.push("  - pid=new".to_string());
            }
            if config.scope.follow {
                scope_filters.push("  - follow".to_string());
            }
            let scope_yaml = scope_filters.join("\n");

            format!(
                r#"dnscache:
  enable: true
containers:
  enrich: false
proctree:
  source: both
  cache:
    process: 8192
    thread: 8192
output:
  options:
    parse-arguments: true
    parse-arguments-fds: true
    exec-hash: digest-inode
  json:
    files:
      - /var/log/tracee/events.jsonl
events:
{}
scope:
{}
"#,
                events_yaml, scope_yaml
            )
        }
    }
}

use trace::config_gen::{EventCategories, TraceScope, TracingConfig, generate_tracee_config};

fn main() {
    let output_dir = Path::new("examples/tracee-profiles");
    fs::create_dir_all(output_dir).expect("Failed to create output directory");

    // Minimal profile: only process events
    let minimal = TracingConfig {
        enabled: true,
        events: EventCategories {
            process: true,
            file: false,
            network: false,
            credentials: false,
            signal: false,
        },
        scope: TraceScope::default(),
    };

    let minimal_yaml = generate_tracee_config(&minimal);
    fs::write(output_dir.join("minimal.yaml"), minimal_yaml)
        .expect("Failed to write minimal.yaml");
    println!("Generated: examples/tracee-profiles/minimal.yaml");

    // Developer profile: process, file, network events
    let developer = TracingConfig {
        enabled: true,
        events: EventCategories {
            process: true,
            file: true,
            network: true,
            credentials: false,
            signal: false,
        },
        scope: TraceScope::default(),
    };

    let developer_yaml = generate_tracee_config(&developer);
    fs::write(output_dir.join("developer.yaml"), developer_yaml)
        .expect("Failed to write developer.yaml");
    println!("Generated: examples/tracee-profiles/developer.yaml");

    // Full profile: all event categories
    let full = TracingConfig {
        enabled: true,
        events: EventCategories {
            process: true,
            file: true,
            network: true,
            credentials: true,
            signal: true,
        },
        scope: TraceScope::default(),
    };

    let full_yaml = generate_tracee_config(&full);
    fs::write(output_dir.join("full.yaml"), full_yaml)
        .expect("Failed to write full.yaml");
    println!("Generated: examples/tracee-profiles/full.yaml");

    println!("\nAll Tracee profile configs generated successfully!");
}
