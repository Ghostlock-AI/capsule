use crate::errors::VmError;
use crate::retry::{RetryConfig, retry_operation};
use crate::validation::{check_stderr_for_errors, health_check_vm, validate_vm_config};
use crate::vm_backend::{VmBackend, VmConfig, VmInfo};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

mod install;
mod template;

/// Concrete implementation of [`VmBackend`] that drives Lima via the `limactl` CLI.
pub struct LimaBackend {
    binary: String,
}

impl LimaBackend {
    pub fn new() -> Result<Self> {
        let binary = match which::which("limactl") {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => {
                eprintln!("📦 Lima not found. Installing lima...");
                Self::install_lima()?;
                which::which("limactl")
                    .context("limactl installation completed but binary not found in PATH")?
                    .to_string_lossy()
                    .to_string()
            }
        };

        Ok(Self { binary })
    }

    fn run_command(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new(&self.binary)
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute limactl {}", args.join(" ")))
    }

    fn run_command_checked(&self, args: &[&str]) -> Result<String> {
        let output = self.run_command(args)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(VmError::CommandFailed {
                command: format!("limactl {}", args.join(" ")),
                exit_code: output.status.code(),
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            }
            .into());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        check_stderr_for_errors(&stderr, &format!("limactl {}", args.join(" ")))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn parse_vm_list(&self, raw: &str) -> Result<Vec<VmInfo>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("time=") {
            return Ok(Vec::new());
        }

        let mut vms = Vec::new();
        for line in trimmed.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let data: Value =
                serde_json::from_str(line).context("Failed to parse limactl list output")?;
            let name = data["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("VM missing name field"))?
                .to_string();

            let status = data["status"].as_str().unwrap_or("Unknown").to_string();
            let state = match status.as_str() {
                "Running" => "Running",
                "Stopped" => "Stopped",
                _ => &status,
            }
            .to_string();

            let ipv4 = data["addresses"]
                .as_array()
                .map_or_else(Vec::new, |addresses| {
                    addresses
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                });

            vms.push(VmInfo {
                name,
                state,
                ipv4,
                release: Some("Ubuntu 24.04".to_string()),
            });
        }

        Ok(vms)
    }
}

impl VmBackend for LimaBackend {
    fn name(&self) -> &str {
        "lima"
    }

    fn is_available(&self) -> bool {
        which::which("limactl").is_ok()
    }

    fn ensure_available(&self) -> Result<()> {
        if !self.is_available() {
            return Err(VmError::BackendNotAvailable {
                backend: "lima".to_string(),
            }
            .into());
        }

        let output = self.run_command(&["--version"])?;
        if !output.status.success() {
            bail!("lima is installed but not working properly");
        }

        Ok(())
    }

    fn create(&self, config: &VmConfig) -> Result<()> {
        validate_vm_config(config.cpus, &config.memory, &config.disk)?;

        if self.exists(&config.name)? {
            return Err(VmError::VmAlreadyExists {
                name: config.name.clone(),
            }
            .into());
        }

        let template_path = self.render_template_with_cloudinit(config)?;

        retry_operation(
            || {
                let cpus_str = config.cpus.to_string();
                let output = self.run_command(&[
                    "start",
                    "--name",
                    &config.name,
                    "--tty=false",
                    "--set",
                    &format!(".cpus={}", cpus_str),
                    "--set",
                    &format!(".memory=\"{}\"", config.memory),
                    "--set",
                    &format!(".disk=\"{}\"", config.disk),
                    &template_path,
                ])?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    bail!("VM launch failed: {}", stderr);
                }
                Ok(())
            },
            RetryConfig::new(3),
            &format!("launch VM {}", config.name),
        )?;

        self.verify_state(&config.name, "Running")?;
        println!("✅ VM provisioned (cloud-init installed tracee binary)");
        Ok(())
    }

    fn start(&self, name: &str) -> Result<()> {
        retry_operation(
            || {
                self.run_command_checked(&["start", name])?;
                Ok(())
            },
            RetryConfig::new(3),
            &format!("start VM {}", name),
        )?;

        self.verify_state(name, "Running")?;
        Ok(())
    }

    fn stop(&self, name: &str) -> Result<()> {
        self.run_command_checked(&["stop", name])?;
        self.verify_state(name, "Stopped")?;
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        let _ = self.run_command(&["stop", name]);
        self.run_command_checked(&["delete", name])?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<VmInfo>> {
        let output = self.run_command_checked(&["list", "--json"])?;
        self.parse_vm_list(&output)
    }

    fn info(&self, name: &str) -> Result<VmInfo> {
        self.list()?
            .into_iter()
            .find(|vm| vm.name == name)
            .ok_or_else(|| {
                VmError::VmNotFound {
                    name: name.to_string(),
                }
                .into()
            })
    }

    fn exec(&self, name: &str, command: &[&str]) -> Result<String> {
        let mut args = vec!["shell", name];
        args.extend_from_slice(command);
        self.run_command_checked(&args)
    }

    fn shell(&self, name: &str, user: Option<&str>) -> Result<()> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("shell").arg(name);

        if let Some(user) = user {
            // leverage sudo to escalate for development sessions
            cmd.args(["sudo", "-iu", user]);
        }

        let status = cmd.status().context("Failed to open shell")?;

        if !status.success() {
            bail!("Shell exited with status: {}", status);
        }

        Ok(())
    }

    fn wait_for_ready(&self, name: &str) -> Result<()> {
        retry_operation(
            || {
                self.exec(name, &["true"])?;
                Ok(())
            },
            RetryConfig::with_delays(10, Duration::from_secs(2), Duration::from_secs(10)),
            "wait for VM SSH",
        )?;

        health_check_vm(name)?;
        Ok(())
    }

    fn verify_state(&self, name: &str, expected_state: &str) -> Result<()> {
        let info = self.info(name)?;
        if info.state != expected_state {
            return Err(VmError::unexpected_state(name, expected_state, info.state).into());
        }
        Ok(())
    }
}
