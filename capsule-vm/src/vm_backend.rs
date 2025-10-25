use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration for creating a VM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    pub name: String,
    pub cpus: u8,
    pub memory: String,
    pub disk: String,
    pub cloud_init: Option<String>, // Path to cloud-init file
}

impl VmConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cpus: 2,
            memory: "1G".to_string(),
            disk: "8G".to_string(),
            cloud_init: None,
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
}

/// Information about a VM instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub name: String,
    pub state: String,
    pub ipv4: Vec<String>,
    pub release: Option<String>,
}

/// VM backend trait - abstraction over Multipass/Lima
pub trait VmBackend: Send + Sync {
    /// Returns the name of this backend (e.g., "multipass", "lima")
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

    /// Execute a command in a VM without capturing output (passthrough)
    fn exec_passthrough(&self, name: &str, command: &[&str]) -> Result<()>;

    /// Open an interactive shell in a VM
    fn shell(&self, name: &str) -> Result<()>;

    /// Transfer a file to a VM
    fn transfer(&self, name: &str, source: &Path, dest: &str) -> Result<()>;

    /// Mount a directory from host to VM
    fn mount(&self, name: &str, source: &Path, dest: &str) -> Result<()>;

    /// Unmount a directory
    fn umount(&self, name: &str) -> Result<()>;

    /// Wait for VM to be ready (with health checks)
    fn wait_for_ready(&self, name: &str) -> Result<()>;

    /// Verify VM state after an operation
    fn verify_state(&self, name: &str, expected_state: &str) -> Result<()>;
}

/// Factory for creating VM backends
pub fn create_backend(backend_type: &str) -> Result<Box<dyn VmBackend>> {
    match backend_type.to_lowercase().as_str() {
        "multipass" => {
            let backend = crate::backends::multipass::MultipassBackend::new()?;
            Ok(Box::new(backend))
        }
        "lima" => {
            let backend = crate::backends::lima::LimaBackend::new()?;
            Ok(Box::new(backend))
        }
        _ => anyhow::bail!("Unknown backend type: {}", backend_type),
    }
}

/// Get the default backend based on platform and availability
pub fn get_default_backend() -> Result<Box<dyn VmBackend>> {
    // Try Lima first (faster on M1 Mac, new default)
    if let Ok(backend) = crate::backends::lima::LimaBackend::new() {
        if backend.is_available() {
            return Ok(Box::new(backend));
        }
    }

    // Try multipass as fallback
    if let Ok(backend) = crate::backends::multipass::MultipassBackend::new() {
        if backend.is_available() {
            return Ok(Box::new(backend));
        }
    }

    anyhow::bail!(
        "No VM backend available. Please install either Lima or Multipass.\n\
         Lima: https://lima-vm.io/ (recommended for M1/M2 Macs)\n\
         Multipass: https://multipass.run/"
    )
}
