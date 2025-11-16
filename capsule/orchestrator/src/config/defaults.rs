use super::schema::*;

/// Create a minimal security profile with strict isolation
pub fn minimal_profile() -> CapsuleConfig {
    CapsuleConfig {
        vm: VmSettings {
            name: "minimal-vm".to_string(),
            cpus: 1,
            memory: "1G".to_string(),
            disk: "5G".to_string(),
        },
        security: SecurityProfile {
            profile: "minimal".to_string(),
            mounts: MountPolicy {
                workspace_only: true,
                allow_home: MountMode::None,
                allowed_paths: vec![],
            },
            processes: ProcessPolicy {
                no_background_persistence: true,
                restrict_fork: true,
                max_children: Some(50),
            },
            network: NetworkPolicy {
                enabled: true,
                localhost_only: true,
                allowed_destinations: vec![],
                blocked_destinations: vec![],
            },
            apparmor: Some(AppArmorConfig {
                enabled: true,
                enforce: true,
                custom_rules: vec![],
            }),
            seccomp: None,
        },
        tracing: TracingConfig {
            enabled: true,
            events: EventCategories {
                process: true,
                file: true,
                network: true,
                credentials: true,
                signal: true,
            },
            scope: TraceScope {
                user: "agent".to_string(),
                new_processes: true,
                follow: true,
            },
        },
        tools: ToolsConfig {
            runtimes: vec![],
            ai_tools: vec![],
            utilities: vec![],
        },
        secrets: SecretsConfig {
            env_file: None,
            inline: std::collections::HashMap::new(),
        },
    }
}

/// Create a developer-friendly profile with balanced security
pub fn developer_profile() -> CapsuleConfig {
    CapsuleConfig {
        vm: VmSettings {
            name: "dev-vm".to_string(),
            cpus: 2,
            memory: "2G".to_string(),
            disk: "10G".to_string(),
        },
        security: SecurityProfile {
            profile: "developer".to_string(),
            mounts: MountPolicy {
                workspace_only: false,
                allow_home: MountMode::Writable,
                allowed_paths: vec![],
            },
            processes: ProcessPolicy {
                no_background_persistence: true,
                restrict_fork: false,
                max_children: None,
            },
            network: NetworkPolicy {
                enabled: true,
                localhost_only: false,
                allowed_destinations: vec![],
                blocked_destinations: vec![],
            },
            apparmor: Some(AppArmorConfig {
                enabled: true,
                enforce: true,
                custom_rules: vec![],
            }),
            seccomp: None,
        },
        tracing: TracingConfig {
            enabled: true,
            events: EventCategories {
                process: true,
                file: true,
                network: true,
                credentials: false,
                signal: false,
            },
            scope: TraceScope {
                user: "agent".to_string(),
                new_processes: true,
                follow: true,
            },
        },
        tools: ToolsConfig {
            runtimes: vec![RuntimeTool::Python3, RuntimeTool::Node],
            ai_tools: vec![AiTool::Claude],
            utilities: vec!["ffmpeg".to_string()],
        },
        secrets: SecretsConfig {
            env_file: Some(".env".to_string()),
            inline: std::collections::HashMap::new(),
        },
    }
}

/// Create a strict security profile with maximum restrictions
pub fn strict_profile() -> CapsuleConfig {
    CapsuleConfig {
        vm: VmSettings {
            name: "strict-vm".to_string(),
            cpus: 1,
            memory: "1G".to_string(),
            disk: "5G".to_string(),
        },
        security: SecurityProfile {
            profile: "strict".to_string(),
            mounts: MountPolicy {
                workspace_only: true,
                allow_home: MountMode::None,
                allowed_paths: vec![],
            },
            processes: ProcessPolicy {
                no_background_persistence: true,
                restrict_fork: true,
                max_children: Some(25),
            },
            network: NetworkPolicy {
                enabled: true,
                localhost_only: true,
                allowed_destinations: vec![],
                blocked_destinations: vec!["0.0.0.0/0".to_string()], // Block all external by default
            },
            apparmor: Some(AppArmorConfig {
                enabled: true,
                enforce: true,
                custom_rules: vec![],
            }),
            seccomp: Some(SeccompConfig {
                enabled: true,
                default_action: SeccompAction::Allow,
                blocked_syscalls: vec![
                    "ptrace".to_string(),
                    "process_vm_readv".to_string(),
                    "process_vm_writev".to_string(),
                    "personality".to_string(),
                ],
                allowed_syscalls: vec![],
            }),
        },
        tracing: TracingConfig {
            enabled: true,
            events: EventCategories {
                process: true,
                file: true,
                network: true,
                credentials: true,
                signal: true,
            },
            scope: TraceScope {
                user: "agent".to_string(),
                new_processes: true,
                follow: true,
            },
        },
        tools: ToolsConfig {
            runtimes: vec![],
            ai_tools: vec![],
            utilities: vec![],
        },
        secrets: SecretsConfig {
            env_file: None,
            inline: std::collections::HashMap::new(),
        },
    }
}

/// Create a custom profile template for user customization
pub fn custom_profile() -> CapsuleConfig {
    CapsuleConfig {
        vm: VmSettings {
            name: "custom-vm".to_string(),
            cpus: 2,
            memory: "2G".to_string(),
            disk: "8G".to_string(),
        },
        security: SecurityProfile {
            profile: "custom".to_string(),
            mounts: MountPolicy::default(),
            processes: ProcessPolicy::default(),
            network: NetworkPolicy::default(),
            apparmor: Some(AppArmorConfig::default()),
            seccomp: None,
        },
        tracing: TracingConfig::default(),
        tools: ToolsConfig::default(),
        secrets: SecretsConfig::default(),
    }
}

/// Get a preset profile by name
pub fn get_preset_profile(name: &str) -> Option<CapsuleConfig> {
    match name {
        "minimal" => Some(minimal_profile()),
        "developer" => Some(developer_profile()),
        "strict" => Some(strict_profile()),
        "custom" => Some(custom_profile()),
        _ => None,
    }
}

/// List all available preset profile names
pub fn list_preset_profiles() -> Vec<&'static str> {
    vec!["minimal", "developer", "strict", "custom"]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::validation::validate_config;

    #[test]
    fn test_minimal_profile_validates() {
        let config = minimal_profile();
        assert_eq!(config.security.profile, "minimal");
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_developer_profile_validates() {
        let config = developer_profile();
        assert_eq!(config.security.profile, "developer");
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_strict_profile_validates() {
        let config = strict_profile();
        assert_eq!(config.security.profile, "strict");
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_custom_profile_validates() {
        let config = custom_profile();
        assert_eq!(config.security.profile, "custom");
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_get_preset_profile() {
        assert!(get_preset_profile("minimal").is_some());
        assert!(get_preset_profile("developer").is_some());
        assert!(get_preset_profile("strict").is_some());
        assert!(get_preset_profile("custom").is_some());
        assert!(get_preset_profile("invalid").is_none());
    }

    #[test]
    fn test_list_preset_profiles() {
        let profiles = list_preset_profiles();
        assert_eq!(profiles.len(), 4);
        assert!(profiles.contains(&"minimal"));
        assert!(profiles.contains(&"developer"));
        assert!(profiles.contains(&"strict"));
        assert!(profiles.contains(&"custom"));
    }

    #[test]
    fn test_minimal_has_strict_settings() {
        let config = minimal_profile();
        assert!(config.security.mounts.workspace_only);
        assert!(matches!(config.security.mounts.allow_home, MountMode::None));
        assert!(config.security.processes.restrict_fork);
        assert!(config.security.network.localhost_only);
    }

    #[test]
    fn test_developer_has_permissive_settings() {
        let config = developer_profile();
        assert!(!config.security.mounts.workspace_only);
        assert!(matches!(config.security.mounts.allow_home, MountMode::Writable));
        assert!(!config.security.processes.restrict_fork);
        assert!(!config.security.network.localhost_only);
        assert!(!config.tools.runtimes.is_empty());
    }

    #[test]
    fn test_strict_has_maximum_restrictions() {
        let config = strict_profile();
        assert!(config.security.mounts.workspace_only);
        assert!(config.security.processes.restrict_fork);
        assert!(config.security.network.localhost_only);
        assert!(config.security.seccomp.is_some());
        assert!(config.tracing.events.credentials);
        assert!(config.tracing.events.signal);
    }
}
