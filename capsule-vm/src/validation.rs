use anyhow::{bail, Context, Result};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

/// VM state enum for validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmState {
    Running,
    Stopped,
    Deleted,
    Starting,
    Stopping,
}

impl VmState {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "running" => Some(VmState::Running),
            "stopped" => Some(VmState::Stopped),
            "deleted" => Some(VmState::Deleted),
            "starting" => Some(VmState::Starting),
            "stopping" => Some(VmState::Stopping),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            VmState::Running => "Running",
            VmState::Stopped => "Stopped",
            VmState::Deleted => "Deleted",
            VmState::Starting => "Starting",
            VmState::Stopping => "Stopping",
        }
    }
}

/// Health check definition
#[derive(Clone)]
pub struct HealthCheck {
    pub name: &'static str,
    pub check_fn: fn(&str) -> Result<()>,
    pub timeout: Duration,
    pub required: bool,
}

impl HealthCheck {
    /// Execute health check with retry until timeout
    pub fn run(&self, vm_name: &str) -> Result<()> {
        let start = Instant::now();
        loop {
            match (self.check_fn)(vm_name) {
                Ok(_) => return Ok(()),
                Err(e) if start.elapsed() >= self.timeout => {
                    if self.required {
                        return Err(e).context(format!(
                            "Required health check '{}' failed after {:?}",
                            self.name, self.timeout
                        ));
                    } else {
                        eprintln!("⚠️  Optional health check '{}' failed: {}", self.name, e);
                        return Ok(());
                    }
                }
                Err(_) => thread::sleep(Duration::from_secs(2)),
            }
        }
    }
}

/// Validates that stderr doesn't contain error indicators even if exit code is 0
pub fn check_stderr_for_errors(stderr: &str, operation: &str) -> Result<()> {
    let error_indicators = ["error:", "failed:", "fatal:", "exception:", "panic"];

    for indicator in &error_indicators {
        if stderr.to_lowercase().contains(indicator) {
            eprintln!(
                "⚠️  Warning: {} completed but stderr contains '{}':",
                operation, indicator
            );
            eprintln!("{}", stderr);
            // For now just warn, could optionally fail
        }
    }

    Ok(())
}

/// Comprehensive health check suite for a VM
pub fn health_check_vm(name: &str, backend: &str) -> Result<()> {
    let checks = get_health_checks(backend);

    for check in checks {
        check.run(name)?;
    }

    Ok(())
}

fn get_health_checks(backend: &str) -> Vec<HealthCheck> {
    match backend {
        "multipass" => vec![
            HealthCheck {
                name: "VM Running",
                check_fn: check_vm_running_multipass,
                timeout: Duration::from_secs(30),
                required: true,
            },
            HealthCheck {
                name: "Network Ready",
                check_fn: check_network_ready_multipass,
                timeout: Duration::from_secs(20),
                required: true,
            },
            HealthCheck {
                name: "Cloud-init Complete",
                check_fn: check_cloud_init_complete,
                timeout: Duration::from_secs(180),
                required: true,
            },
            HealthCheck {
                name: "System Running",
                check_fn: check_system_running,
                timeout: Duration::from_secs(60),
                required: true,
            },
        ],
        "lima" => vec![
            HealthCheck {
                name: "VM Running",
                check_fn: check_vm_running_lima,
                timeout: Duration::from_secs(30),
                required: true,
            },
            HealthCheck {
                name: "Network Ready",
                check_fn: check_network_ready_lima,
                timeout: Duration::from_secs(20),
                required: true,
            },
            HealthCheck {
                name: "SSH Ready",
                check_fn: check_ssh_ready_lima,
                timeout: Duration::from_secs(30),
                required: true,
            },
        ],
        _ => vec![],
    }
}

// Multipass-specific checks
fn check_vm_running_multipass(name: &str) -> Result<()> {
    let output = Command::new("multipass")
        .args(["list", "--format", "json"])
        .output()?;

    if !output.status.success() {
        bail!("failed to list VMs");
    }

    let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let list = data["list"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid list format"))?;

    for vm in list {
        if vm["name"].as_str() == Some(name) {
            if vm["state"].as_str() == Some("Running") {
                return Ok(());
            }
        }
    }

    bail!("VM not running")
}

fn check_network_ready_multipass(name: &str) -> Result<()> {
    let output = Command::new("multipass")
        .args(["exec", name, "--", "ip", "addr", "show"])
        .output()?;

    if !output.status.success() {
        bail!("network check command failed");
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    // Check for an IP address (not just 127.0.0.1)
    if !output_str.contains("inet ")
        || !output_str
            .split("inet ")
            .skip(1)
            .any(|s| !s.starts_with("127."))
    {
        bail!("no network interface with IP");
    }

    Ok(())
}

// Lima-specific checks
fn check_vm_running_lima(name: &str) -> Result<()> {
    let output = Command::new("limactl")
        .args(["list", "--json"])
        .output()?;

    if !output.status.success() {
        bail!("failed to list VMs");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_output = stdout.trim();

    // Lima outputs warnings when there are no instances
    if json_output.is_empty() || json_output.starts_with("time=") {
        bail!("VM not found");
    }

    // Lima returns JSON objects one per line
    for line in json_output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let vm: serde_json::Value = serde_json::from_str(line)?;
        if vm["name"].as_str() == Some(name) {
            if vm["status"].as_str() == Some("Running") {
                return Ok(());
            }
        }
    }

    bail!("VM not running")
}

fn check_network_ready_lima(name: &str) -> Result<()> {
    let output = Command::new("limactl")
        .args(["shell", name, "ip", "addr", "show"])
        .output()?;

    if !output.status.success() {
        bail!("network check command failed");
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    if !output_str.contains("inet ")
        || !output_str
            .split("inet ")
            .skip(1)
            .any(|s| !s.starts_with("127."))
    {
        bail!("no network interface with IP");
    }

    Ok(())
}

fn check_ssh_ready_lima(name: &str) -> Result<()> {
    let output = Command::new("limactl")
        .args(["shell", name, "true"])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        bail!("SSH not accepting connections")
    }
}

// Common checks
fn check_cloud_init_complete(name: &str) -> Result<()> {
    let output = Command::new("multipass")
        .args(["exec", name, "--", "cloud-init", "status"])
        .output()?;

    if !output.status.success() {
        bail!("cloud-init status command failed");
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    if output_str.contains("status: done") {
        Ok(())
    } else if output_str.contains("status: error") {
        bail!("cloud-init reported error state")
    } else {
        bail!("cloud-init not yet complete")
    }
}

fn check_system_running(name: &str) -> Result<()> {
    let output = Command::new("multipass")
        .args(["exec", name, "--", "systemctl", "is-system-running"])
        .output()?;

    let status = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // "running" or "degraded" are acceptable
    if status == "running" || status == "degraded" {
        Ok(())
    } else {
        bail!("system not running: {}", status)
    }
}

/// Validates VM configuration before attempting to create
pub fn validate_vm_config(cpus: u8, memory: &str, disk: &str) -> Result<()> {
    // CPU validation
    if cpus == 0 || cpus > 64 {
        bail!("Invalid CPU count: {} (must be 1-64)", cpus);
    }

    // Memory validation
    let mem_value = parse_memory_size(memory).context("invalid memory format")?;
    if mem_value < 512 * 1024 * 1024 {
        bail!("Memory too small: {} (minimum 512M)", memory);
    }

    // Disk validation
    let disk_value = parse_disk_size(disk).context("invalid disk format")?;
    if disk_value < 2 * 1024 * 1024 * 1024 {
        bail!("Disk too small: {} (minimum 2G)", disk);
    }

    Ok(())
}

fn parse_memory_size(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();
    let (num_str, multiplier) = if let Some(n) = s.strip_suffix('G') {
        (n, 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix('M') {
        (n, 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("GIB") {
        (n, 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("MIB") {
        (n, 1024 * 1024)
    } else {
        (s.as_str(), 1)
    };

    let num: u64 = num_str.parse().context("invalid numeric value")?;
    Ok(num * multiplier)
}

fn parse_disk_size(s: &str) -> Result<u64> {
    parse_memory_size(s) // Same format
}

/// Verifies that a mount exists and is accessible
pub fn verify_mount_exists(vm_name: &str, mount_point: &str, backend: &str) -> Result<bool> {
    let output = match backend {
        "multipass" => Command::new("multipass")
            .args(["exec", vm_name, "--", "mountpoint", "-q", mount_point])
            .output()?,
        "lima" => Command::new("limactl")
            .args(["shell", vm_name, "mountpoint", "-q", mount_point])
            .output()?,
        _ => bail!("unknown backend: {}", backend),
    };

    Ok(output.status.success())
}

/// Waits for VM to be ready (polling-based)
pub fn wait_for_vm_ready(vm_name: &str, backend: &str, timeout: Duration) -> Result<()> {
    let start = Instant::now();

    loop {
        if start.elapsed() >= timeout {
            bail!("Timeout waiting for VM to be ready after {:?}", timeout);
        }

        match health_check_vm(vm_name, backend) {
            Ok(_) => return Ok(()),
            Err(_) => thread::sleep(Duration::from_secs(5)),
        }
    }
}
