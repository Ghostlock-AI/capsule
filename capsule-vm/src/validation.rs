use anyhow::{Context, Result, bail};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

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

/// Comprehensive health check suite for a Lima VM
pub fn health_check_vm(name: &str) -> Result<()> {
    let checks = get_health_checks();

    for check in checks {
        check.run(name)?;
    }

    Ok(())
}

fn get_health_checks() -> Vec<HealthCheck> {
    vec![
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
    ]
}

// Lima-specific checks
fn check_vm_running_lima(name: &str) -> Result<()> {
    let output = Command::new("limactl").args(["list", "--json"]).output()?;

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
        if vm["name"].as_str() == Some(name) && vm["status"].as_str() == Some("Running") {
            return Ok(());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_vm_config_accepts_reasonable_values() {
        assert!(validate_vm_config(2, "1G", "8G").is_ok());
    }

    #[test]
    fn validate_vm_config_rejects_zero_cpus() {
        assert!(validate_vm_config(0, "1G", "8G").is_err());
    }

    #[test]
    fn validate_vm_config_rejects_too_small_memory() {
        assert!(validate_vm_config(2, "256M", "8G").is_err());
    }

    #[test]
    fn validate_vm_config_rejects_too_small_disk() {
        assert!(validate_vm_config(2, "1G", "1G").is_err());
    }

    #[test]
    fn parse_memory_size_supports_binary_suffixes() {
        assert_eq!(parse_memory_size("1GiB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("512MiB").unwrap(), 512 * 1024 * 1024);
    }

    #[test]
    fn parse_memory_size_rejects_invalid_input() {
        assert!(parse_memory_size("not-a-size").is_err());
    }

    #[test]
    fn check_stderr_for_errors_returns_ok_even_on_warning() {
        assert!(check_stderr_for_errors("warning: error: something happened", "op").is_ok());
    }
}
