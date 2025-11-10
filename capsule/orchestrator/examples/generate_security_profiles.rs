//! Example: Generate security profiles
//!
//! This example demonstrates generating AppArmor profiles with different security levels.
//! Run with: cargo run --example generate_security_profiles

use capsule::config::*;
use capsule::security::{export_security_profiles, generate_apparmor_profile};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("Generating example security profiles...\n");

    // Example 1: Developer profile (balanced)
    println!("=== Developer Profile ===");
    let dev_config = create_developer_config();
    generate_and_save(&dev_config, "developer")?;

    // Example 2: Strict profile (maximum security)
    println!("\n=== Strict Profile ===");
    let strict_config = create_strict_config();
    generate_and_save(&strict_config, "strict")?;

    // Example 3: Minimal profile (minimal access)
    println!("\n=== Minimal Profile ===");
    let minimal_config = create_minimal_config();
    generate_and_save(&minimal_config, "minimal")?;

    println!("\n✅ All example profiles generated successfully!");
    println!("   Check .capsule/examples/profiles/ for output files");

    Ok(())
}

fn generate_and_save(config: &CapsuleConfig, profile_type: &str) -> anyhow::Result<()> {
    // Generate the AppArmor profile
    let profile = generate_apparmor_profile(&config.vm.name, &config.security)?;

    println!("Generated AppArmor profile for '{}':", config.vm.name);
    println!("  - Profile: {}", config.security.profile);
    println!("  - Lines: {}", profile.lines().count());

    // Export to files
    let output_dir = PathBuf::from(".capsule/examples/profiles").join(profile_type);
    export_security_profiles(config, &output_dir)?;

    Ok(())
}

fn create_developer_config() -> CapsuleConfig {
    CapsuleConfig {
        vm: VmSettings {
            name: "developer-vm".to_string(),
            cpus: 2,
            memory: "2G".to_string(),
            disk: "10G".to_string(),
        },
        security: SecurityProfile {
            profile: "developer".to_string(),
            mounts: MountPolicy {
                workspace_only: false,
                allow_home: MountMode::Writable,
                allowed_paths: vec!["/opt/data".to_string()],
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

fn create_strict_config() -> CapsuleConfig {
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
                max_children: Some(10),
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
                custom_rules: vec![
                    "deny /proc/sys/** w".to_string(),
                    "deny capability sys_admin".to_string(),
                ],
            }),
            seccomp: None,
        },
        tracing: TracingConfig::default(),
        tools: ToolsConfig::default(),
        secrets: SecretsConfig::default(),
    }
}

fn create_minimal_config() -> CapsuleConfig {
    CapsuleConfig {
        vm: VmSettings {
            name: "minimal-vm".to_string(),
            cpus: 1,
            memory: "512M".to_string(),
            disk: "4G".to_string(),
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
                restrict_fork: false,
                max_children: Some(5),
            },
            network: NetworkPolicy {
                enabled: false,
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
