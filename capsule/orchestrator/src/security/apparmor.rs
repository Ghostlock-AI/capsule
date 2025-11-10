//! AppArmor profile generator
//!
//! Generates valid AppArmor profile syntax from security configuration.
//! Supports mount restrictions, network restrictions, and process restrictions.

use crate::config::{MountMode, SecurityProfile};
use anyhow::Result;

/// Generate an AppArmor profile from security configuration
///
/// # Arguments
/// * `vm_name` - Name of the VM (used in profile name)
/// * `security` - Security profile configuration
///
/// # Returns
/// A string containing the complete AppArmor profile in valid syntax,
/// or an empty string if AppArmor is disabled.
///
/// # Example
/// ```rust,ignore
/// let profile = generate_apparmor_profile("my-vm", &security_config)?;
/// std::fs::write("/etc/apparmor.d/capsule-my-vm", profile)?;
/// ```
pub fn generate_apparmor_profile(
    vm_name: &str,
    security: &SecurityProfile,
) -> Result<String> {
    // Check if AppArmor is enabled
    let apparmor = match &security.apparmor {
        Some(config) if config.enabled => config,
        _ => return Ok(String::new()),
    };

    // Determine enforcement mode
    let mode = if apparmor.enforce {
        "enforce"
    } else {
        "complain"
    };

    // Start building the profile
    let mut profile = format!(
        r#"#include <tunables/global>

profile capsule-{vm_name} flags=({mode}) {{
  #include <abstractions/base>

  # Capabilities
  capability setuid,
  capability setgid,
  capability chown,
  capability fowner,
  capability dac_override,
  capability dac_read_search,

  # Filesystem access
"#
    );

    // Add mount restrictions
    add_mount_restrictions(&mut profile, security);

    // Add standard system access
    profile.push_str(
        r#"
  # System libraries and binaries
  /lib/** rm,
  /lib64/** rm,
  /usr/lib/** rm,
  /usr/lib64/** rm,
  /usr/bin/** rix,
  /usr/sbin/** rix,
  /bin/** rix,
  /sbin/** rix,

  # Temporary files
  /tmp/** rw,
  /var/tmp/** rw,

  # Process information
  /proc/** r,
  /sys/** r,
  /dev/pts/** rw,
  /dev/tty rw,
  /dev/null rw,
  /dev/zero r,
  /dev/urandom r,
"#,
    );

    // Add network restrictions
    add_network_restrictions(&mut profile, security);

    // Add process restrictions (comments for documentation)
    add_process_restrictions(&mut profile, security);

    // Add custom rules
    if !apparmor.custom_rules.is_empty() {
        profile.push_str("\n  # Custom rules\n");
        for rule in &apparmor.custom_rules {
            profile.push_str("  ");
            profile.push_str(rule.trim());
            if !rule.trim().ends_with(',') {
                profile.push(',');
            }
            profile.push('\n');
        }
    }

    // Close the profile
    profile.push_str("}\n");

    Ok(profile)
}

/// Add mount restrictions to the AppArmor profile
fn add_mount_restrictions(profile: &mut String, security: &SecurityProfile) {
    if security.mounts.workspace_only {
        // Strict workspace-only mode
        profile.push_str("  # Mount policy: workspace only\n");
        profile.push_str("  /home/agent/workspace/** rw,\n");
        profile.push_str("  deny /home/agent/[^w]** rw,\n");
        profile.push_str("  deny /home/** rw,\n");
        profile.push_str("  deny /root/** rw,\n");
    } else {
        // Allow home directory based on mode
        match security.mounts.allow_home {
            MountMode::Writable => {
                profile.push_str("  # Mount policy: home directory (writable)\n");
                profile.push_str("  /home/agent/** rw,\n");
                profile.push_str("  owner /home/agent/** rw,\n");
            }
            MountMode::ReadOnly => {
                profile.push_str("  # Mount policy: home directory (read-only)\n");
                profile.push_str("  /home/agent/** r,\n");
                profile.push_str("  owner /home/agent/** r,\n");
            }
            MountMode::None => {
                profile.push_str("  # Mount policy: no home directory access\n");
                profile.push_str("  deny /home/agent/** rw,\n");
                profile.push_str("  deny /home/** rw,\n");
            }
        }
    }

    // Add additional allowed paths
    if !security.mounts.allowed_paths.is_empty() {
        profile.push_str("\n  # Additional allowed paths\n");
        for path in &security.mounts.allowed_paths {
            profile.push_str(&format!("  {}/** rw,\n", path.trim_end_matches('/')));
        }
    }
}

/// Add network restrictions to the AppArmor profile
fn add_network_restrictions(profile: &mut String, security: &SecurityProfile) {
    if !security.network.enabled {
        // Network completely disabled
        profile.push_str("\n  # Network policy: disabled\n");
        profile.push_str("  deny network,\n");
    } else if security.network.localhost_only {
        // Localhost only mode
        profile.push_str("\n  # Network policy: localhost only\n");
        profile.push_str("  network inet stream,\n");
        profile.push_str("  network inet6 stream,\n");
        profile.push_str("  network inet dgram,\n");
        profile.push_str("  network inet6 dgram,\n");
        profile.push_str("  network unix stream,\n");
        profile.push_str("  network unix dgram,\n");
        profile.push_str("  # Note: Firewall rules needed to enforce localhost-only at network layer\n");
    } else {
        // Network unrestricted
        profile.push_str("\n  # Network policy: enabled (unrestricted)\n");
        profile.push_str("  network,\n");
    }

    // Add allowed/blocked destinations as comments (enforced by firewall, not AppArmor)
    if !security.network.allowed_destinations.is_empty() {
        profile.push_str("\n  # Allowed destinations (enforced by firewall):\n");
        for dest in &security.network.allowed_destinations {
            profile.push_str(&format!("  # - {}\n", dest));
        }
    }

    if !security.network.blocked_destinations.is_empty() {
        profile.push_str("\n  # Blocked destinations (enforced by firewall):\n");
        for dest in &security.network.blocked_destinations {
            profile.push_str(&format!("  # - {}\n", dest));
        }
    }
}

/// Add process restriction comments to the AppArmor profile
///
/// Note: Some process restrictions (like fork limits) are better enforced
/// via ulimit, cgroups, or systemd rather than AppArmor.
fn add_process_restrictions(profile: &mut String, security: &SecurityProfile) {
    if security.processes.no_background_persistence
        || security.processes.restrict_fork
        || security.processes.max_children.is_some()
    {
        profile.push_str("\n  # Process restrictions (enforced by systemd/cgroups):\n");

        if security.processes.no_background_persistence {
            profile.push_str("  # - No background process persistence\n");
        }

        if security.processes.restrict_fork {
            profile.push_str("  # - Restricted process forking\n");
        }

        if let Some(max) = security.processes.max_children {
            profile.push_str(&format!("  # - Max children: {}\n", max));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_security_profile() -> SecurityProfile {
        use crate::config::{AppArmorConfig, MountPolicy, NetworkPolicy, ProcessPolicy};

        SecurityProfile {
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
        }
    }

    #[test]
    fn test_generate_basic_profile() {
        let security = create_test_security_profile();
        let profile = generate_apparmor_profile("test-vm", &security).unwrap();

        assert!(profile.contains("profile capsule-test-vm flags=(enforce)"));
        assert!(profile.contains("#include <abstractions/base>"));
        assert!(profile.contains("/home/agent/** rw"));
        assert!(profile.contains("network,"));
    }

    #[test]
    fn test_workspace_only_mode() {
        let mut security = create_test_security_profile();
        security.mounts.workspace_only = true;

        let profile = generate_apparmor_profile("test-vm", &security).unwrap();

        assert!(profile.contains("/home/agent/workspace/** rw"));
        assert!(profile.contains("deny /home/agent/[^w]** rw"));
    }

    #[test]
    fn test_network_disabled() {
        let mut security = create_test_security_profile();
        security.network.enabled = false;

        let profile = generate_apparmor_profile("test-vm", &security).unwrap();

        assert!(profile.contains("deny network,"));
    }

    #[test]
    fn test_localhost_only() {
        let mut security = create_test_security_profile();
        security.network.localhost_only = true;

        let profile = generate_apparmor_profile("test-vm", &security).unwrap();

        assert!(profile.contains("network inet stream"));
        assert!(profile.contains("localhost only"));
    }

    #[test]
    fn test_custom_rules() {
        let mut security = create_test_security_profile();
        if let Some(ref mut apparmor) = security.apparmor {
            apparmor.custom_rules = vec![
                "/custom/path/** rw".to_string(),
                "capability sys_admin".to_string(),
            ];
        }

        let profile = generate_apparmor_profile("test-vm", &security).unwrap();

        assert!(profile.contains("/custom/path/** rw,"));
        assert!(profile.contains("capability sys_admin,"));
    }

    #[test]
    fn test_disabled_apparmor() {
        let mut security = create_test_security_profile();
        if let Some(ref mut apparmor) = security.apparmor {
            apparmor.enabled = false;
        }

        let profile = generate_apparmor_profile("test-vm", &security).unwrap();

        assert_eq!(profile, "");
    }
}
