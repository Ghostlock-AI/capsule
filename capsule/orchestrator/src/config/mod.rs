use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

mod schema;
mod validation;
mod defaults;

pub use schema::*;
pub use validation::*;
pub use defaults::*;

/// Load configuration from YAML file
pub fn load_config(path: impl AsRef<Path>) -> Result<CapsuleConfig> {
    let content = fs::read_to_string(path.as_ref())
        .with_context(|| format!("Failed to read {}", path.as_ref().display()))?;

    let config: CapsuleConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML in {}", path.as_ref().display()))?;

    validate_config(&config)
        .with_context(|| format!("Configuration validation failed for {}", path.as_ref().display()))?;

    Ok(config)
}

/// Save configuration to YAML file
pub fn save_config(config: &CapsuleConfig, path: impl AsRef<Path>) -> Result<()> {
    // Validate before saving
    validate_config(config)
        .context("Cannot save invalid configuration")?;

    let yaml = serde_yaml::to_string(config)
        .context("Failed to serialize configuration to YAML")?;

    fs::write(path.as_ref(), yaml)
        .with_context(|| format!("Failed to write to {}", path.as_ref().display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_save_and_load_config() {
        let config = developer_profile();

        // Create a temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_path_buf();

        // Save config
        save_config(&config, &temp_path).unwrap();

        // Load config back
        let loaded = load_config(&temp_path).unwrap();

        // Verify key fields
        assert_eq!(loaded.vm.name, config.vm.name);
        assert_eq!(loaded.vm.cpus, config.vm.cpus);
        assert_eq!(loaded.security.profile, config.security.profile);
        assert_eq!(loaded.tracing.enabled, config.tracing.enabled);
    }

    #[test]
    fn test_load_invalid_yaml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "invalid: yaml: content:").unwrap();

        let result = load_config(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_save_invalid_config() {
        let mut config = developer_profile();
        config.vm.name = "".to_string(); // Invalid: empty name

        let temp_file = NamedTempFile::new().unwrap();
        let result = save_config(&config, temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_yaml_roundtrip_preserves_data() {
        let config = strict_profile();

        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: CapsuleConfig = serde_yaml::from_str(&yaml).unwrap();

        // Verify complex nested structures
        assert_eq!(parsed.security.mounts.workspace_only, config.security.mounts.workspace_only);
        assert_eq!(parsed.tracing.events.process, config.tracing.events.process);
        assert_eq!(parsed.tools.runtimes.len(), config.tools.runtimes.len());
    }
}
