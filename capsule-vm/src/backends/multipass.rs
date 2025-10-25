use crate::errors::VmError;
use crate::retry::{retry_operation, RetryConfig};
use crate::validation::{check_stderr_for_errors, health_check_vm, validate_vm_config};
use crate::vm_backend::{VmBackend, VmConfig, VmInfo};
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub struct MultipassBackend {
    binary: String,
}

impl MultipassBackend {
    pub fn new() -> Result<Self> {
        // Try to find multipass
        let binary = match which::which("multipass") {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => {
                // Not found, attempt to install
                eprintln!("📦 Multipass not found. Installing multipass...");
                Self::install_multipass()?;

                // Try again after installation
                which::which("multipass")
                    .context("multipass installation completed but binary not found in PATH")?
                    .to_string_lossy()
                    .to_string()
            }
        };

        Ok(Self { binary })
    }

    /// Install Multipass on the system
    fn install_multipass() -> Result<()> {
        println!("🔧 Detecting platform and installing Multipass...");

        // Detect OS
        let os = std::env::consts::OS;

        match os {
            "macos" => Self::install_multipass_macos(),
            "linux" => Self::install_multipass_linux(),
            "windows" => Self::install_multipass_windows(),
            _ => bail!("Unsupported OS: {}. Please install Multipass manually from https://multipass.run/", os),
        }
    }

    /// Install Multipass on macOS using Homebrew
    fn install_multipass_macos() -> Result<()> {
        println!("🍺 Installing Multipass via Homebrew...");

        // Check if brew is installed
        if which::which("brew").is_err() {
            bail!(
                "Homebrew not found. Please install it first:\n\
                /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"\n\
                Or install Multipass manually: https://multipass.run/"
            );
        }

        // Install multipass
        let status = Command::new("brew")
            .args(["install", "--cask", "multipass"])
            .status()
            .context("Failed to run brew install multipass")?;

        if !status.success() {
            bail!("Failed to install Multipass via Homebrew. Please install manually: https://multipass.run/");
        }

        println!("✅ Multipass installed successfully!");
        Ok(())
    }

    /// Install Multipass on Linux
    fn install_multipass_linux() -> Result<()> {
        println!("🐧 Installing Multipass on Linux via snap...");

        // Multipass on Linux is primarily distributed via snap
        if which::which("snap").is_err() {
            bail!(
                "Snap not found. Multipass on Linux requires snap.\n\
                Please install snap first or install Multipass manually: https://multipass.run/"
            );
        }

        // Install multipass via snap
        let status = Command::new("sudo")
            .args(["snap", "install", "multipass"])
            .status()
            .context("Failed to run snap install multipass")?;

        if !status.success() {
            bail!("Failed to install Multipass via snap. Please install manually: https://multipass.run/");
        }

        println!("✅ Multipass installed successfully!");
        Ok(())
    }

    /// Install Multipass on Windows
    fn install_multipass_windows() -> Result<()> {
        println!("🪟 Installing Multipass on Windows...");

        // Check for winget (Windows Package Manager)
        if which::which("winget").is_ok() {
            println!("📦 Installing via winget...");

            let status = Command::new("winget")
                .args(["install", "Canonical.Multipass"])
                .status()
                .context("Failed to run winget install Multipass")?;

            if status.success() {
                println!("✅ Multipass installed successfully!");
                return Ok(());
            }
        }

        // Check for chocolatey
        if which::which("choco").is_ok() {
            println!("🍫 Installing via chocolatey...");

            let status = Command::new("choco")
                .args(["install", "multipass", "-y"])
                .status()
                .context("Failed to run choco install multipass")?;

            if status.success() {
                println!("✅ Multipass installed successfully!");
                return Ok(());
            }
        }

        // Manual installation required
        bail!(
            "Could not auto-install Multipass on Windows.\n\
            Please install manually from: https://multipass.run/\n\
            Or install a package manager (winget or chocolatey) first."
        );
    }

    /// Execute a multipass command and return output
    fn run_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new(&self.binary)
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute multipass {}", args.join(" ")))?;

        Ok(output)
    }

    /// Execute command and check for success
    fn run_command_checked(&self, args: &[&str]) -> Result<String> {
        let output = self.run_command(args)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(VmError::CommandFailed {
                command: format!("multipass {}", args.join(" ")),
                exit_code: output.status.code(),
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            }
            .into());
        }

        // Check stderr for hidden errors
        let stderr = String::from_utf8_lossy(&output.stderr);
        check_stderr_for_errors(&stderr, &format!("multipass {}", args.join(" ")))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Parse multipass list JSON output
    fn parse_vm_list(&self, json_output: &str) -> Result<Vec<VmInfo>> {
        let data: serde_json::Value = serde_json::from_str(json_output)
            .context("Failed to parse multipass list output")?;

        let list = data["list"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid list format from multipass"))?;

        let mut vms = Vec::new();
        for vm in list {
            let name = vm["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("VM missing name field"))?
                .to_string();
            let state = vm["state"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("VM missing state field"))?
                .to_string();

            let ipv4 = if let Some(ipv4_array) = vm["ipv4"].as_array() {
                ipv4_array
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            } else {
                vec![]
            };

            let release = vm["release"].as_str().map(String::from);

            vms.push(VmInfo {
                name,
                state,
                ipv4,
                release,
            });
        }

        Ok(vms)
    }
}

impl VmBackend for MultipassBackend {
    fn name(&self) -> &str {
        "multipass"
    }

    fn is_available(&self) -> bool {
        which::which("multipass").is_ok()
    }

    fn ensure_available(&self) -> Result<()> {
        if !self.is_available() {
            return Err(VmError::BackendNotAvailable {
                backend: "multipass".to_string(),
            }
            .into());
        }

        // Verify multipass is working
        let output = self.run_command(&["version"])?;
        if !output.status.success() {
            bail!("multipass is installed but not working properly");
        }

        Ok(())
    }

    fn create(&self, config: &VmConfig) -> Result<()> {
        // Validate configuration first
        validate_vm_config(config.cpus, &config.memory, &config.disk)?;

        // Check if VM already exists
        if self.exists(&config.name)? {
            return Err(VmError::VmAlreadyExists {
                name: config.name.clone(),
            }
            .into());
        }

        // Build launch command
        let cpus_str = config.cpus.to_string();
        let mut args = vec![
            "launch",
            "24.04",
            "--name",
            &config.name,
            "--cpus",
            &cpus_str,
            "--memory",
            &config.memory,
            "--disk",
            &config.disk,
        ];

        let cloud_init_path;
        if let Some(ci) = &config.cloud_init {
            cloud_init_path = ci.clone();
            args.push("--cloud-init");
            args.push(&cloud_init_path);
        }

        // Execute launch with retry
        retry_operation(
            || {
                let output = self.run_command(&args)?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    bail!("VM launch failed: {}", stderr);
                }
                Ok(())
            },
            RetryConfig::new(3),
            &format!("launch VM {}", config.name),
        )?;

        // Verify VM was created successfully
        self.verify_state(&config.name, "Running")?;

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
        // Best-effort umount and stop
        let _ = self.run_command(&["umount", name]);
        let _ = self.run_command(&["stop", name]);

        // Delete the VM
        self.run_command_checked(&["delete", name])?;

        // Purge deleted VMs
        self.run_command_checked(&["purge"])?;

        Ok(())
    }

    fn list(&self) -> Result<Vec<VmInfo>> {
        let output = self.run_command_checked(&["list", "--format", "json"])?;
        self.parse_vm_list(&output)
    }

    fn info(&self, name: &str) -> Result<VmInfo> {
        let vms = self.list()?;
        vms.into_iter()
            .find(|vm| vm.name == name)
            .ok_or_else(|| VmError::VmNotFound { name: name.to_string() }.into())
    }

    fn exec(&self, name: &str, command: &[&str]) -> Result<String> {
        let mut args = vec!["exec", name, "--"];
        args.extend_from_slice(command);

        self.run_command_checked(&args)
    }

    fn exec_passthrough(&self, name: &str, command: &[&str]) -> Result<()> {
        let mut args = vec!["exec", name, "--"];
        args.extend_from_slice(command);

        let status = Command::new(&self.binary)
            .args(&args)
            .status()
            .context("Failed to execute command")?;

        if !status.success() {
            bail!("Command failed with status: {}", status);
        }

        Ok(())
    }

    fn shell(&self, name: &str) -> Result<()> {
        let status = Command::new(&self.binary)
            .args(["shell", name])
            .status()
            .context("Failed to open shell")?;

        if !status.success() {
            bail!("Shell exited with status: {}", status);
        }

        Ok(())
    }

    fn transfer(&self, name: &str, source: &Path, dest: &str) -> Result<()> {
        let source_str = source
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid source path"))?;
        let dest_full = format!("{}:{}", name, dest);

        self.run_command_checked(&["transfer", source_str, &dest_full])?;

        Ok(())
    }

    fn mount(&self, name: &str, source: &Path, dest: &str) -> Result<()> {
        let source_str = source
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid source path"))?;
        let dest_full = format!("{}:{}", name, dest);

        self.run_command_checked(&["mount", source_str, &dest_full])?;

        // Verify mount succeeded
        let check = self.exec(name, &["mountpoint", "-q", dest]);
        if check.is_err() {
            return Err(VmError::MountFailed {
                details: format!("Mount operation completed but {} is not mounted", dest),
            }
            .into());
        }

        Ok(())
    }

    fn umount(&self, name: &str) -> Result<()> {
        // Best effort - don't fail if already unmounted
        let _ = self.run_command(&["umount", name]);
        Ok(())
    }

    fn wait_for_ready(&self, name: &str) -> Result<()> {
        println!("⏳ Waiting for VM to be ready...");

        // Wait for cloud-init to complete
        let _ = retry_operation(
            || {
                self.exec(
                    name,
                    &["bash", "-lc", "cloud-init status --wait || true"],
                )?;
                Ok(())
            },
            RetryConfig::with_delays(
                5,
                Duration::from_secs(5),
                Duration::from_secs(30),
            ),
            "wait for cloud-init",
        );

        // Wait for systemd to be running
        let _ = retry_operation(
            || {
                self.exec(
                    name,
                    &["bash", "-lc", "systemctl is-system-running --wait || true"],
                )?;
                Ok(())
            },
            RetryConfig::with_delays(
                5,
                Duration::from_secs(3),
                Duration::from_secs(20),
            ),
            "wait for systemd",
        );

        // Run comprehensive health checks
        health_check_vm(name, "multipass")?;

        println!("✅ VM is ready!");
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
