use anyhow::Result;

/// Configuration for creating a VM
#[derive(Debug, Clone)]
pub struct VmConfig {
    pub name: String,
    pub cpus: u8,
    pub memory: String,
    pub disk: String,
    pub cloud_init: Option<String>, // Path to cloud-init file
    pub stream_serial_logs: bool,
}

impl VmConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cpus: 2,
            memory: "1G".to_string(),
            disk: "8G".to_string(),
            cloud_init: None,
            stream_serial_logs: false,
        }
    }

    pub fn with_cpus(mut self, cpus: u8) -> Self {
        self.cpus = cpus;
        self
    }

    pub fn with_memory(mut self, memory: impl Into<String>) -> Self {
        self.memory = memory.into();
        self
    }

    pub fn with_disk(mut self, disk: impl Into<String>) -> Self {
        self.disk = disk.into();
        self
    }

    pub fn with_cloud_init(mut self, path: impl Into<String>) -> Self {
        self.cloud_init = Some(path.into());
        self
    }

    pub fn with_stream_serial_logs(mut self, enable: bool) -> Self {
        self.stream_serial_logs = enable;
        self
    }
}

/// Information about a VM instance
#[derive(Debug, Clone)]
pub struct VmInfo {
    pub name: String,
    pub state: String,
    pub ipv4: Vec<String>,
    pub release: Option<String>,
}

/// VM backend trait - abstraction over the VM implementation (currently Lima-only)
pub trait VmBackend: Send + Sync {
    /// Returns the name of this backend (e.g., "lima")
    fn name(&self) -> &str;

    /// Check if the backend is available on the system
    fn is_available(&self) -> bool;

    /// Ensure backend is installed and ready
    fn ensure_available(&self) -> Result<()>;

    /// Create and launch a new VM
    fn create(&self, config: &VmConfig) -> Result<()>;

    /// Start an existing VM
    fn start(&self, name: &str) -> Result<()>;

    /// Stop a running VM
    fn stop(&self, name: &str) -> Result<()>;

    /// Delete a VM
    fn delete(&self, name: &str) -> Result<()>;

    /// List all VMs
    fn list(&self) -> Result<Vec<VmInfo>>;

    /// Get information about a specific VM
    fn info(&self, name: &str) -> Result<VmInfo>;

    /// Check if a VM exists
    fn exists(&self, name: &str) -> Result<bool> {
        match self.info(name) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Execute a command in a VM
    fn exec(&self, name: &str, command: &[&str]) -> Result<String>;

    /// Open an interactive shell inside the VM as the specified user
    fn shell(&self, name: &str, user: &str) -> Result<()>;

    /// Wait for VM to be ready (with health checks)
    fn wait_for_ready(&self, name: &str) -> Result<()>;

    /// Verify VM state after an operation
    fn verify_state(&self, name: &str, expected_state: &str) -> Result<()>;

    /// Cleanup any transient resources (e.g., log streams) created during provisioning
    fn finalize_serial_logs(&self, _name: &str) -> Result<()> {
        Ok(())
    }
}

/// Factory for creating VM backends
pub fn create_backend(backend_type: &str) -> Result<Box<dyn VmBackend>> {
    match backend_type.to_lowercase().as_str() {
        "lima" => {
            let backend = crate::backends::lima::LimaBackend::new()?;
            Ok(Box::new(backend))
        }
        _ => anyhow::bail!(
            "Unknown backend type: {}. Lima is the only supported backend.",
            backend_type
        ),
    }
}

/// Get the default backend based on platform and availability
pub fn get_default_backend() -> Result<Box<dyn VmBackend>> {
    if let Ok(backend) = crate::backends::lima::LimaBackend::new()
        && backend.is_available()
    {
        return Ok(Box::new(backend));
    }

    anyhow::bail!("No VM backend available. Please install Lima from https://lima-vm.io/.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_config_defaults_are_sensible() {
        let config = VmConfig::new("demo");
        assert_eq!(config.name, "demo");
        assert_eq!(config.cpus, 2);
        assert_eq!(config.memory, "1G");
        assert_eq!(config.disk, "8G");
        assert!(config.cloud_init.is_none());
        assert!(!config.stream_serial_logs);
    }

    #[test]
    fn vm_config_builder_overrides_fields() {
        let config = VmConfig::new("test")
            .with_cpus(4)
            .with_memory("2G")
            .with_disk("16G")
            .with_cloud_init("/tmp/cloud.yaml")
            .with_stream_serial_logs(true);

        assert_eq!(config.cpus, 4);
        assert_eq!(config.memory, "2G");
        assert_eq!(config.disk, "16G");
        assert_eq!(config.cloud_init.as_deref(), Some("/tmp/cloud.yaml"));
        assert!(config.stream_serial_logs);
    }
}
