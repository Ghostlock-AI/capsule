//! Tracee configuration generation
//!
//! This module generates valid /etc/tracee/config.yaml files based on
//! event categories, scope filters, and tracing profiles.

use super::events::{EventCategory, get_events_for_category};
use anyhow::Result;

// Use the actual config types from the config module
use crate::config::{TracingConfig, EventCategories, TraceScope};

/// Complete Tracee configuration with all sections
#[derive(Debug, Clone)]
pub struct TraceeConfig {
    pub tracing: TracingConfig,
}

impl Default for TraceeConfig {
    fn default() -> Self {
        Self {
            tracing: TracingConfig::default(),
        }
    }
}

impl TraceeConfig {
    /// Create a minimal tracing profile (only process events)
    pub fn minimal() -> Self {
        Self {
            tracing: TracingConfig {
                enabled: true,
                events: EventCategories {
                    process: true,
                    file: false,
                    network: false,
                    credentials: false,
                    signal: false,
                },
                scope: TraceScope::default(),
            },
        }
    }

    /// Create a full tracing profile (all event categories)
    pub fn full() -> Self {
        Self {
            tracing: TracingConfig {
                enabled: true,
                events: EventCategories {
                    process: true,
                    file: true,
                    network: true,
                    credentials: true,
                    signal: true,
                },
                scope: TraceScope::default(),
            },
        }
    }

    /// Create a developer tracing profile (process, file, network)
    pub fn developer() -> Self {
        Self {
            tracing: TracingConfig {
                enabled: true,
                events: EventCategories {
                    process: true,
                    file: true,
                    network: true,
                    credentials: false,
                    signal: false,
                },
                scope: TraceScope::default(),
            },
        }
    }
}

/// Generate a Tracee config YAML from tracing configuration
pub fn generate_tracee_config(config: &TracingConfig) -> Result<String> {
    if !config.enabled {
        return Ok(String::new());
    }

    // Collect all enabled events
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

    // Build events YAML list
    let events_yaml = events
        .iter()
        .map(|e| format!("    - {}", e))
        .collect::<Vec<_>>()
        .join("\n");

    // Build scope filters
    let mut scope_filters = Vec::new();

    // User filter
    scope_filters.push(format!("  - uid=$({})", config.scope.user));

    // New processes filter
    if config.scope.new_processes {
        scope_filters.push("  - pid=new".to_string());
    }

    // Follow child processes filter
    if config.scope.follow {
        scope_filters.push("  - follow".to_string());
    }

    let scope_yaml = scope_filters.join("\n");

    // Generate the complete YAML configuration
    let yaml = format!(
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
    );

    Ok(yaml)
}

/// Generate example Tracee configs for different profiles
pub fn generate_profile_configs() -> Result<Vec<(String, String)>> {
    let profiles = vec![
        ("minimal", TraceeConfig::minimal()),
        ("full", TraceeConfig::full()),
        ("developer", TraceeConfig::developer()),
    ];

    let mut configs = Vec::new();

    for (name, profile) in profiles {
        let yaml = generate_tracee_config(&profile.tracing)?;
        configs.push((name.to_string(), yaml));
    }

    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_config() {
        let config = TraceeConfig::minimal();
        assert!(config.tracing.enabled);
        assert!(config.tracing.events.process);
        assert!(!config.tracing.events.file);
        assert!(!config.tracing.events.network);
        assert!(!config.tracing.events.credentials);
        assert!(!config.tracing.events.signal);
    }

    #[test]
    fn test_full_config() {
        let config = TraceeConfig::full();
        assert!(config.tracing.enabled);
        assert!(config.tracing.events.process);
        assert!(config.tracing.events.file);
        assert!(config.tracing.events.network);
        assert!(config.tracing.events.credentials);
        assert!(config.tracing.events.signal);
    }

    #[test]
    fn test_developer_config() {
        let config = TraceeConfig::developer();
        assert!(config.tracing.enabled);
        assert!(config.tracing.events.process);
        assert!(config.tracing.events.file);
        assert!(config.tracing.events.network);
        assert!(!config.tracing.events.credentials);
        assert!(!config.tracing.events.signal);
    }

    #[test]
    fn test_generate_minimal_tracee_yaml() {
        let config = TraceeConfig::minimal();
        let yaml = generate_tracee_config(&config.tracing).unwrap();

        // Should contain dnscache section
        assert!(yaml.contains("dnscache:"));
        assert!(yaml.contains("enable: true"));

        // Should contain output section
        assert!(yaml.contains("output:"));
        assert!(yaml.contains("/var/log/tracee/events.jsonl"));

        // Should contain process events
        assert!(yaml.contains("sched_process_exec"));
        assert!(yaml.contains("execve"));
        assert!(yaml.contains("exit_group"));
        assert!(yaml.contains("exit"));

        // Should NOT contain file events
        assert!(!yaml.contains("openat"));
        assert!(!yaml.contains("close"));

        // Should contain scope
        assert!(yaml.contains("scope:"));
        assert!(yaml.contains("uid=$(agent)"));
        assert!(yaml.contains("pid=new"));
        assert!(yaml.contains("follow"));
    }

    #[test]
    fn test_generate_full_tracee_yaml() {
        let config = TraceeConfig::full();
        let yaml = generate_tracee_config(&config.tracing).unwrap();

        // Should contain all event categories
        assert!(yaml.contains("sched_process_exec")); // process
        assert!(yaml.contains("openat")); // file
        assert!(yaml.contains("net_tcp_connect")); // network
        assert!(yaml.contains("security_bprm_check")); // credentials
        assert!(yaml.contains("signal_deliver")); // signal
    }

    #[test]
    fn test_disabled_tracing_returns_empty() {
        let config = TracingConfig {
            enabled: false,
            events: EventCategories::default(),
            scope: TraceScope::default(),
        };

        let yaml = generate_tracee_config(&config).unwrap();
        assert_eq!(yaml, "");
    }

    #[test]
    fn test_custom_scope() {
        let config = TracingConfig {
            enabled: true,
            events: EventCategories {
                process: true,
                file: false,
                network: false,
                credentials: false,
                signal: false,
            },
            scope: TraceScope {
                user: "custom_user".to_string(),
                new_processes: false,
                follow: false,
            },
        };

        let yaml = generate_tracee_config(&config).unwrap();

        assert!(yaml.contains("uid=$(custom_user)"));
        assert!(!yaml.contains("pid=new"));
        assert!(!yaml.contains("follow"));
    }

    #[test]
    fn test_generate_profile_configs() {
        let profiles = generate_profile_configs().unwrap();

        assert_eq!(profiles.len(), 3);

        let names: Vec<&str> = profiles.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"minimal"));
        assert!(names.contains(&"full"));
        assert!(names.contains(&"developer"));

        // All should be non-empty YAML
        for (_, yaml) in &profiles {
            assert!(!yaml.is_empty());
            assert!(yaml.contains("dnscache:"));
            assert!(yaml.contains("events:"));
            assert!(yaml.contains("scope:"));
        }
    }
}
