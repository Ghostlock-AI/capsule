//! Security profile export functionality
//!
//! Exports generated security profiles to files with portable cloud-init snippets.
//! This makes Capsule profiles usable in any VM system, not just Lima.

use crate::config::CapsuleConfig;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Exported security profile bundle
#[derive(Debug, Clone)]
pub struct SecurityProfileExport {
    /// Generated AppArmor profile content
    pub apparmor_profile: String,

    /// Optional Seccomp-BPF profile (future enhancement)
    pub seccomp_profile: Option<String>,

    /// Cloud-init snippet for applying profiles
    pub cloud_init_snippet: String,
}

/// Export security profiles to a directory
///
/// Generates AppArmor profiles and cloud-init snippets that can be used
/// to apply security profiles in any VM system.
///
/// # Arguments
/// * `config` - Complete Capsule configuration
/// * `output_dir` - Directory to write profile files to
///
/// # Returns
/// A `SecurityProfileExport` containing all generated content
///
/// # Example
/// ```rust,ignore
/// let export = export_security_profiles(&config, "./profiles")?;
/// println!("AppArmor profile written to: ./profiles/apparmor-my-vm.profile");
/// println!("Cloud-init snippet: ./profiles/security-cloud-init.yaml");
/// ```
pub fn export_security_profiles(
    config: &CapsuleConfig,
    output_dir: impl AsRef<Path>,
) -> Result<SecurityProfileExport> {
    let output_dir = output_dir.as_ref();
    fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create directory: {}", output_dir.display()))?;

    // Generate AppArmor profile
    let apparmor =
        super::apparmor::generate_apparmor_profile(&config.vm.name, &config.security)?;

    let mut apparmor_path_str = String::new();

    if !apparmor.is_empty() {
        let apparmor_path = output_dir.join(format!("apparmor-{}.profile", config.vm.name));
        fs::write(&apparmor_path, &apparmor)
            .with_context(|| format!("Failed to write {}", apparmor_path.display()))?;
        apparmor_path_str = apparmor_path.to_string_lossy().to_string();
        println!("✅ AppArmor profile: {}", apparmor_path.display());
    }

    // Generate cloud-init snippet for applying profiles
    let cloud_init_snippet = if !apparmor.is_empty() {
        generate_cloud_init_snippet(config, &apparmor)
    } else {
        "# No security profiles enabled\n".to_string()
    };

    let cloud_init_path = output_dir.join("security-cloud-init.yaml");
    fs::write(&cloud_init_path, &cloud_init_snippet)
        .with_context(|| format!("Failed to write {}", cloud_init_path.display()))?;
    println!("✅ Cloud-init snippet: {}", cloud_init_path.display());

    // Generate README with usage instructions
    let readme = generate_readme(config, &apparmor_path_str);
    let readme_path = output_dir.join("README.md");
    fs::write(&readme_path, &readme)
        .with_context(|| format!("Failed to write {}", readme_path.display()))?;
    println!("✅ Usage instructions: {}", readme_path.display());

    Ok(SecurityProfileExport {
        apparmor_profile: apparmor,
        seccomp_profile: None, // Future enhancement
        cloud_init_snippet,
    })
}

/// Generate a cloud-init snippet for applying security profiles
fn generate_cloud_init_snippet(config: &CapsuleConfig, apparmor_profile: &str) -> String {
    // Indent each line of the AppArmor profile for proper YAML embedding
    let indented_profile = apparmor_profile
        .lines()
        .map(|line| format!("      {}", line))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"# Capsule Security Profile - Cloud-Init Snippet
# Generated for VM: {}
#
# This snippet can be merged into any cloud-init configuration to apply
# the security profile during VM provisioning.

write_files:
  - path: /etc/apparmor.d/capsule-{}
    permissions: '0644'
    owner: root:root
    content: |
{}

runcmd:
  # Load and enforce AppArmor profile
  - apparmor_parser -r /etc/apparmor.d/capsule-{}
  - aa-enforce capsule-{}
  - echo "✅ Capsule security profile applied: capsule-{}"
"#,
        config.vm.name,
        config.vm.name,
        indented_profile,
        config.vm.name,
        config.vm.name,
        config.vm.name,
    )
}

/// Generate a README with usage instructions
fn generate_readme(config: &CapsuleConfig, apparmor_path: &str) -> String {
    let profile_name = format!("capsule-{}", config.vm.name);

    format!(
        r#"# Capsule Security Profile - {}

This directory contains the exported security profile for the Capsule VM `{}`.

## Files

- `apparmor-{}.profile` - AppArmor profile defining security restrictions
- `security-cloud-init.yaml` - Cloud-init snippet for automatic deployment
- `README.md` - This file

## Profile Configuration

- **Profile Type**: {}
- **Mount Policy**: {}
- **Network Policy**: {}
- **Enforcement Mode**: {}

## Usage

### Option 1: Manual Installation (Any Linux System)

```bash
# Copy the AppArmor profile
sudo cp {} /etc/apparmor.d/{}

# Load and enforce the profile
sudo apparmor_parser -r /etc/apparmor.d/{}
sudo aa-enforce {}
```

### Option 2: Cloud-Init Integration

Merge the contents of `security-cloud-init.yaml` into your VM's cloud-init configuration.

**For Lima:**
```yaml
# In your Lima YAML template:
provision:
  - mode: system
    script: |
      #!/bin/bash
      # ... existing provisioning ...

      # Add contents from security-cloud-init.yaml here
```

**For Cloud Providers (AWS, GCP, Azure):**
```bash
# Merge with your existing cloud-init config
cat security-cloud-init.yaml >> your-cloud-init.yaml
```

**For Terraform:**
```hcl
resource "aws_instance" "vm" {{
  user_data = file("security-cloud-init.yaml")
  # ... other config ...
}}
```

### Option 3: Apply to Running System

```bash
# Copy the profile
scp apparmor-{}.profile user@vm:/tmp/profile

# On the VM:
sudo mv /tmp/profile /etc/apparmor.d/{}
sudo apparmor_parser -r /etc/apparmor.d/{}
sudo aa-enforce {}
```

## Verification

Check that the profile is loaded and enforced:

```bash
# List loaded profiles
sudo aa-status | grep capsule

# Should show:
#   {} (enforce)

# Test the restrictions
su - agent
# Try operations that should be blocked by the profile
```

## Customization

To modify the profile:

1. Edit the AppArmor profile file
2. Reload: `sudo apparmor_parser -r /etc/apparmor.d/{}`
3. Test your changes in complain mode first:
   ```bash
   sudo aa-complain {}
   # Test...
   sudo aa-enforce {}
   ```

## Troubleshooting

**Profile won't load:**
```bash
# Check syntax
sudo apparmor_parser -p /etc/apparmor.d/{}

# View parser errors
sudo journalctl -u apparmor
```

**Denials in logs:**
```bash
# View AppArmor denials
sudo dmesg | grep -i apparmor
# or
sudo journalctl | grep -i apparmor | grep -i denied
```

**Temporarily disable:**
```bash
sudo aa-disable {}
```

## Profile Details

This profile restricts:
- Filesystem access based on mount policy
- Network capabilities
- Process execution
- System capabilities

For more information, see: https://gitlab.com/apparmor/apparmor/-/wikis/home
"#,
        config.vm.name,
        config.vm.name,
        config.vm.name,
        config.security.profile,
        format_mount_policy(config),
        format_network_policy(config),
        format_enforcement_mode(config),
        apparmor_path,
        profile_name,
        profile_name,
        profile_name,
        config.vm.name,
        profile_name,
        profile_name,
        profile_name,
        profile_name,
        profile_name,
        profile_name,
        profile_name,
        profile_name,
        profile_name,
    )
}

fn format_mount_policy(config: &CapsuleConfig) -> String {
    if config.security.mounts.workspace_only {
        "Workspace only (strict isolation)".to_string()
    } else {
        match config.security.mounts.allow_home {
            crate::config::MountMode::Writable => {
                "Home directory (writable)".to_string()
            }
            crate::config::MountMode::ReadOnly => {
                "Home directory (read-only)".to_string()
            }
            crate::config::MountMode::None => "No home access".to_string(),
        }
    }
}

fn format_network_policy(config: &CapsuleConfig) -> String {
    if !config.security.network.enabled {
        "Disabled (no network)".to_string()
    } else if config.security.network.localhost_only {
        "Localhost only".to_string()
    } else {
        "Enabled (unrestricted)".to_string()
    }
}

fn format_enforcement_mode(config: &CapsuleConfig) -> String {
    match &config.security.apparmor {
        Some(cfg) if cfg.enabled && cfg.enforce => "Enforce".to_string(),
        Some(cfg) if cfg.enabled => "Complain (audit only)".to_string(),
        _ => "Disabled".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    fn create_test_config() -> CapsuleConfig {
        use crate::config::{TracingConfig, ToolsConfig, SecretsConfig};

        CapsuleConfig {
            vm: VmSettings {
                name: "test-vm".to_string(),
                cpus: 2,
                memory: "2G".to_string(),
                disk: "8G".to_string(),
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
            tracing: TracingConfig::default(),
            tools: ToolsConfig::default(),
            secrets: SecretsConfig::default(),
        }
    }

    #[test]
    fn test_generate_cloud_init_snippet() {
        let config = create_test_config();
        let apparmor = "profile test {...}";
        let snippet = generate_cloud_init_snippet(&config, apparmor);

        assert!(snippet.contains("write_files:"));
        assert!(snippet.contains("/etc/apparmor.d/capsule-test-vm"));
        assert!(snippet.contains("apparmor_parser -r"));
        assert!(snippet.contains("aa-enforce"));
    }

    #[test]
    fn test_format_policies() {
        let config = create_test_config();

        assert_eq!(format_mount_policy(&config), "Home directory (writable)");
        assert_eq!(format_network_policy(&config), "Enabled (unrestricted)");
        assert_eq!(format_enforcement_mode(&config), "Enforce");
    }
}
