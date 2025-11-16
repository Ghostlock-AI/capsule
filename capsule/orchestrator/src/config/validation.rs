use super::schema::*;
use anyhow::{bail, Result};

/// Validate the complete configuration
pub fn validate_config(config: &CapsuleConfig) -> Result<()> {
    // Validate VM settings
    validate_vm_settings(&config.vm)?;

    // Validate security profile
    validate_security_profile(&config.security)?;

    // Validate tracing config
    validate_tracing(&config.tracing)?;

    // Validate tools config
    validate_tools(&config.tools)?;

    Ok(())
}

fn validate_vm_settings(vm: &VmSettings) -> Result<()> {
    if vm.name.is_empty() {
        bail!("VM name cannot be empty");
    }

    // Validate name contains only alphanumeric, dash, and underscore
    if !vm.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        bail!("VM name must contain only alphanumeric characters, dashes, and underscores");
    }

    if vm.cpus == 0 {
        bail!("CPUs must be at least 1");
    }

    if vm.cpus > 64 {
        bail!("CPUs cannot exceed 64");
    }

    // Validate memory format (e.g., "2G", "1024M")
    validate_memory_format(&vm.memory)?;

    // Validate disk format
    validate_disk_format(&vm.disk)?;

    Ok(())
}

fn validate_memory_format(mem: &str) -> Result<()> {
    if mem.is_empty() {
        bail!("Memory specification cannot be empty");
    }

    let valid = mem.ends_with('G') || mem.ends_with('M');
    if !valid {
        bail!("Memory must end with 'G' (gigabytes) or 'M' (megabytes), e.g., '2G' or '1024M'");
    }

    // Parse the numeric part
    let numeric_part = &mem[..mem.len() - 1];
    if numeric_part.parse::<u64>().is_err() {
        bail!("Memory specification must start with a valid number, e.g., '2G' or '1024M'");
    }

    // Validate reasonable ranges
    let value = numeric_part.parse::<u64>().unwrap();
    if mem.ends_with('G') && value == 0 {
        bail!("Memory must be at least 1G");
    }
    if mem.ends_with('M') && value < 512 {
        bail!("Memory must be at least 512M");
    }
    if mem.ends_with('G') && value > 256 {
        bail!("Memory cannot exceed 256G");
    }

    Ok(())
}

fn validate_disk_format(disk: &str) -> Result<()> {
    if disk.is_empty() {
        bail!("Disk specification cannot be empty");
    }

    let valid = disk.ends_with('G') || disk.ends_with('M');
    if !valid {
        bail!("Disk must end with 'G' (gigabytes) or 'M' (megabytes), e.g., '10G' or '8192M'");
    }

    // Parse the numeric part
    let numeric_part = &disk[..disk.len() - 1];
    if numeric_part.parse::<u64>().is_err() {
        bail!("Disk specification must start with a valid number, e.g., '10G' or '8192M'");
    }

    // Validate reasonable ranges
    let value = numeric_part.parse::<u64>().unwrap();
    if disk.ends_with('G') && value == 0 {
        bail!("Disk must be at least 1G");
    }
    if disk.ends_with('M') && value < 1024 {
        bail!("Disk must be at least 1024M (1G)");
    }
    if disk.ends_with('G') && value > 1024 {
        bail!("Disk cannot exceed 1024G (1TB)");
    }

    Ok(())
}

fn validate_security_profile(profile: &SecurityProfile) -> Result<()> {
    // Validate profile name
    let valid_profiles = ["minimal", "developer", "strict", "custom"];
    if !valid_profiles.contains(&profile.profile.as_str()) {
        bail!(
            "Invalid security profile '{}'. Must be one of: {}",
            profile.profile,
            valid_profiles.join(", ")
        );
    }

    // Validate mount policy
    validate_mount_policy(&profile.mounts)?;

    // Validate process policy
    validate_process_policy(&profile.processes)?;

    // Validate network policy
    validate_network_policy(&profile.network)?;

    // Validate AppArmor config if present
    if let Some(ref apparmor) = profile.apparmor {
        validate_apparmor_config(apparmor)?;
    }

    // Validate Seccomp config if present
    if let Some(ref seccomp) = profile.seccomp {
        validate_seccomp_config(seccomp)?;
    }

    Ok(())
}

fn validate_mount_policy(policy: &MountPolicy) -> Result<()> {
    // Validate allowed paths are absolute
    for path in &policy.allowed_paths {
        if !path.starts_with('/') {
            bail!("Mount path '{}' must be an absolute path starting with '/'", path);
        }
    }

    Ok(())
}

fn validate_process_policy(policy: &ProcessPolicy) -> Result<()> {
    // Validate max_children is reasonable if set
    if let Some(max) = policy.max_children {
        if max == 0 {
            bail!("max_children must be at least 1 if specified");
        }
        if max > 10000 {
            bail!("max_children cannot exceed 10000");
        }
    }

    Ok(())
}

fn validate_network_policy(policy: &NetworkPolicy) -> Result<()> {
    // Validate CIDR formats for allowed/blocked destinations
    for dest in &policy.allowed_destinations {
        validate_cidr_or_ip(dest)?;
    }

    for dest in &policy.blocked_destinations {
        validate_cidr_or_ip(dest)?;
    }

    // Warn if network disabled but localhost_only is true
    if !policy.enabled && policy.localhost_only {
        // This is not an error, but localhost_only has no effect when network is disabled
        // We'll just allow it for now
    }

    Ok(())
}

fn validate_cidr_or_ip(value: &str) -> Result<()> {
    // Basic validation - check if it looks like an IP or CIDR
    // Full IP parsing could use a dedicated library
    if value.is_empty() {
        bail!("IP/CIDR cannot be empty");
    }

    // Check for CIDR notation
    if value.contains('/') {
        let parts: Vec<&str> = value.split('/').collect();
        if parts.len() != 2 {
            bail!("Invalid CIDR format '{}': must be IP/prefix", value);
        }

        // Validate prefix length
        if let Ok(prefix) = parts[1].parse::<u8>() {
            if prefix > 128 {
                bail!("Invalid CIDR prefix length in '{}': must be 0-128", value);
            }
        } else {
            bail!("Invalid CIDR prefix in '{}': must be a number", value);
        }
    }

    // Basic check for IP format (IPv4 or IPv6)
    let ip_part = if value.contains('/') {
        value.split('/').next().unwrap()
    } else {
        value
    };

    // Very basic validation - just check it has reasonable characters
    let valid_chars = ip_part.chars().all(|c| {
        c.is_ascii_hexdigit() || c == '.' || c == ':' || c == '-'
    });

    if !valid_chars {
        bail!("Invalid IP/CIDR format '{}'", value);
    }

    Ok(())
}

fn validate_apparmor_config(_config: &AppArmorConfig) -> Result<()> {
    // AppArmor custom rules could be validated for syntax
    // For now, we'll accept any rules and let AppArmor itself validate
    Ok(())
}

fn validate_seccomp_config(config: &SeccompConfig) -> Result<()> {
    // If seccomp is enabled and default is deny, must have at least some allowed syscalls
    if config.enabled {
        match config.default_action {
            SeccompAction::Deny => {
                if config.allowed_syscalls.is_empty() {
                    bail!("Seccomp with default_action=deny requires at least one allowed syscall");
                }
            }
            SeccompAction::Allow => {
                // It's fine to have no blocked syscalls
            }
        }
    }

    Ok(())
}

fn validate_tracing(tracing: &TracingConfig) -> Result<()> {
    // Ensure at least one event category is enabled if tracing is enabled
    if tracing.enabled {
        let any_enabled = tracing.events.process
            || tracing.events.file
            || tracing.events.network
            || tracing.events.credentials
            || tracing.events.signal;

        if !any_enabled {
            bail!("Tracing is enabled but no event categories are selected. Enable at least one category.");
        }
    }

    // Validate trace scope user is not empty
    if tracing.scope.user.is_empty() {
        bail!("Trace scope user cannot be empty");
    }

    Ok(())
}

fn validate_tools(_tools: &ToolsConfig) -> Result<()> {
    // Tools config is fairly simple - not much to validate
    // Utilities are just strings, we'll let the provisioning handle invalid tools
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_memory_format_valid() {
        assert!(validate_memory_format("2G").is_ok());
        assert!(validate_memory_format("1024M").is_ok());
        assert!(validate_memory_format("512M").is_ok());
        assert!(validate_memory_format("256G").is_ok());
    }

    #[test]
    fn test_validate_memory_format_invalid() {
        assert!(validate_memory_format("").is_err());
        assert!(validate_memory_format("2").is_err());
        assert!(validate_memory_format("2K").is_err());
        assert!(validate_memory_format("G2").is_err());
        assert!(validate_memory_format("0G").is_err());
        assert!(validate_memory_format("256M").is_err()); // Too small
        assert!(validate_memory_format("512G").is_err()); // Too large
    }

    #[test]
    fn test_validate_disk_format_valid() {
        assert!(validate_disk_format("10G").is_ok());
        assert!(validate_disk_format("8192M").is_ok());
        assert!(validate_disk_format("1024M").is_ok());
        assert!(validate_disk_format("100G").is_ok());
    }

    #[test]
    fn test_validate_disk_format_invalid() {
        assert!(validate_disk_format("").is_err());
        assert!(validate_disk_format("10").is_err());
        assert!(validate_disk_format("10T").is_err());
        assert!(validate_disk_format("0G").is_err());
        assert!(validate_disk_format("512M").is_err()); // Too small
        assert!(validate_disk_format("2000G").is_err()); // Too large
    }

    #[test]
    fn test_validate_cidr_valid() {
        assert!(validate_cidr_or_ip("192.168.1.0/24").is_ok());
        assert!(validate_cidr_or_ip("10.0.0.0/8").is_ok());
        assert!(validate_cidr_or_ip("192.168.1.1").is_ok());
        assert!(validate_cidr_or_ip("2001:db8::/32").is_ok());
    }

    #[test]
    fn test_validate_cidr_invalid() {
        assert!(validate_cidr_or_ip("").is_err());
        assert!(validate_cidr_or_ip("192.168.1.0/").is_err());
        assert!(validate_cidr_or_ip("192.168.1.0/256").is_err());
        assert!(validate_cidr_or_ip("192.168.1.0/24/32").is_err());
    }

    #[test]
    fn test_validate_security_profile_valid() {
        let profile = SecurityProfile {
            profile: "developer".to_string(),
            mounts: MountPolicy::default(),
            processes: ProcessPolicy::default(),
            network: NetworkPolicy::default(),
            apparmor: None,
            seccomp: None,
        };
        assert!(validate_security_profile(&profile).is_ok());
    }

    #[test]
    fn test_validate_security_profile_invalid() {
        let profile = SecurityProfile {
            profile: "invalid-profile".to_string(),
            mounts: MountPolicy::default(),
            processes: ProcessPolicy::default(),
            network: NetworkPolicy::default(),
            apparmor: None,
            seccomp: None,
        };
        assert!(validate_security_profile(&profile).is_err());
    }

    #[test]
    fn test_validate_tracing_no_events() {
        let tracing = TracingConfig {
            enabled: true,
            events: EventCategories {
                process: false,
                file: false,
                network: false,
                credentials: false,
                signal: false,
            },
            scope: TraceScope::default(),
        };
        assert!(validate_tracing(&tracing).is_err());
    }

    #[test]
    fn test_validate_tracing_valid() {
        let tracing = TracingConfig::default();
        assert!(validate_tracing(&tracing).is_ok());
    }

    #[test]
    fn test_validate_mount_policy_absolute_paths() {
        let policy = MountPolicy {
            workspace_only: false,
            allow_home: MountMode::Writable,
            allowed_paths: vec!["/tmp".to_string(), "/opt/data".to_string()],
        };
        assert!(validate_mount_policy(&policy).is_ok());

        let invalid_policy = MountPolicy {
            workspace_only: false,
            allow_home: MountMode::Writable,
            allowed_paths: vec!["tmp".to_string()],
        };
        assert!(validate_mount_policy(&invalid_policy).is_err());
    }

    #[test]
    fn test_validate_process_policy() {
        let policy = ProcessPolicy {
            no_background_persistence: true,
            restrict_fork: false,
            max_children: Some(100),
        };
        assert!(validate_process_policy(&policy).is_ok());

        let invalid_policy = ProcessPolicy {
            no_background_persistence: true,
            restrict_fork: false,
            max_children: Some(0),
        };
        assert!(validate_process_policy(&invalid_policy).is_err());
    }

    #[test]
    fn test_validate_seccomp_deny_with_allowed() {
        let config = SeccompConfig {
            enabled: true,
            default_action: SeccompAction::Deny,
            blocked_syscalls: vec![],
            allowed_syscalls: vec!["read".to_string(), "write".to_string()],
        };
        assert!(validate_seccomp_config(&config).is_ok());

        let invalid_config = SeccompConfig {
            enabled: true,
            default_action: SeccompAction::Deny,
            blocked_syscalls: vec![],
            allowed_syscalls: vec![],
        };
        assert!(validate_seccomp_config(&invalid_config).is_err());
    }
}
