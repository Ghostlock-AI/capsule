use crate::errors::VmError;
use crate::retry::{retry_operation, RetryConfig};
use crate::validation::{check_stderr_for_errors, health_check_vm, validate_vm_config};
use crate::vm_backend::{VmBackend, VmConfig, VmInfo};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub struct LimaBackend {
    binary: String,
}

impl LimaBackend {
    pub fn new() -> Result<Self> {
        // Try to find limactl
        let binary = match which::which("limactl") {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => {
                // Not found, attempt to install
                eprintln!("📦 Lima not found. Installing lima...");
                Self::install_lima()?;

                // Try again after installation
                which::which("limactl")
                    .context("limactl installation completed but binary not found in PATH")?
                    .to_string_lossy()
                    .to_string()
            }
        };

        Ok(Self { binary })
    }

    /// Install Lima on the system
    fn install_lima() -> Result<()> {
        println!("🔧 Detecting platform and installing Lima...");

        // Detect OS
        let os = std::env::consts::OS;

        match os {
            "macos" => Self::install_lima_macos(),
            "linux" => Self::install_lima_linux(),
            _ => bail!("Unsupported OS: {}. Please install Lima manually from https://lima-vm.io/", os),
        }
    }

    /// Install Lima on macOS using Homebrew
    fn install_lima_macos() -> Result<()> {
        println!("🍺 Installing Lima via Homebrew...");

        // Check if brew is installed
        if which::which("brew").is_err() {
            bail!(
                "Homebrew not found. Please install it first:\n\
                /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"\n\
                Or install Lima manually: https://lima-vm.io/"
            );
        }

        // Install lima
        let status = Command::new("brew")
            .args(["install", "lima"])
            .status()
            .context("Failed to run brew install lima")?;

        if !status.success() {
            bail!("Failed to install Lima via Homebrew. Please install manually: https://lima-vm.io/");
        }

        println!("✅ Lima installed successfully!");
        Ok(())
    }

    /// Install Lima on Linux
    fn install_lima_linux() -> Result<()> {
        println!("🐧 Installing Lima on Linux...");

        // Detect Linux distribution
        let distro = Self::detect_linux_distro()?;

        match distro.as_str() {
            "ubuntu" | "debian" => Self::install_lima_debian(),
            "fedora" | "rhel" | "centos" => Self::install_lima_fedora(),
            "arch" => Self::install_lima_arch(),
            _ => {
                eprintln!("⚠️  Unknown Linux distribution: {}", distro);
                eprintln!("Attempting generic installation...");
                Self::install_lima_generic_linux()
            }
        }
    }

    fn detect_linux_distro() -> Result<String> {
        // Try reading /etc/os-release
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("ID=") {
                    let id = line.strip_prefix("ID=").unwrap().trim_matches('"');
                    return Ok(id.to_lowercase());
                }
            }
        }

        Ok("unknown".to_string())
    }

    fn install_lima_debian() -> Result<()> {
        println!("📦 Installing Lima via apt...");

        // Update package list
        let status = Command::new("sudo")
            .args(["apt-get", "update", "-y"])
            .status()
            .context("Failed to run apt-get update")?;

        if !status.success() {
            bail!("apt-get update failed");
        }

        // Install lima
        let status = Command::new("sudo")
            .args(["apt-get", "install", "-y", "lima"])
            .status()
            .context("Failed to run apt-get install lima")?;

        if !status.success() {
            eprintln!("⚠️  Package manager installation failed, trying generic install...");
            return Self::install_lima_generic_linux();
        }

        println!("✅ Lima installed successfully!");
        Ok(())
    }

    fn install_lima_fedora() -> Result<()> {
        println!("📦 Installing Lima via dnf/yum...");

        // Try dnf first (Fedora), fall back to yum (RHEL/CentOS)
        let pkg_manager = if which::which("dnf").is_ok() {
            "dnf"
        } else {
            "yum"
        };

        let status = Command::new("sudo")
            .args([pkg_manager, "install", "-y", "lima"])
            .status()
            .context(format!("Failed to run {} install lima", pkg_manager))?;

        if !status.success() {
            eprintln!("⚠️  Package manager installation failed, trying generic install...");
            return Self::install_lima_generic_linux();
        }

        println!("✅ Lima installed successfully!");
        Ok(())
    }

    fn install_lima_arch() -> Result<()> {
        println!("📦 Installing Lima via pacman...");

        let status = Command::new("sudo")
            .args(["pacman", "-S", "--noconfirm", "lima"])
            .status()
            .context("Failed to run pacman -S lima")?;

        if !status.success() {
            eprintln!("⚠️  Package manager installation failed, trying generic install...");
            return Self::install_lima_generic_linux();
        }

        println!("✅ Lima installed successfully!");
        Ok(())
    }

    fn install_lima_generic_linux() -> Result<()> {
        println!("📦 Installing Lima from GitHub releases...");

        // Detect architecture
        let arch = std::env::consts::ARCH;
        let lima_arch = match arch {
            "x86_64" => "x86_64",
            "aarch64" | "arm64" => "aarch64",
            _ => bail!("Unsupported architecture: {}. Please install Lima manually: https://lima-vm.io/", arch),
        };

        // Download and install latest release
        let install_script = format!(
            r#"
#!/bin/bash
set -e

LIMA_VERSION=$(curl -fsSL https://api.github.com/repos/lima-vm/lima/releases/latest | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')
LIMA_URL="https://github.com/lima-vm/lima/releases/download/v${{LIMA_VERSION}}/lima-${{LIMA_VERSION}}-Linux-{}.tar.gz"

echo "Downloading Lima v${{LIMA_VERSION}}..."
curl -fsSL "$LIMA_URL" -o /tmp/lima.tar.gz

echo "Extracting..."
sudo tar -C /usr/local -xzf /tmp/lima.tar.gz

echo "Cleaning up..."
rm /tmp/lima.tar.gz

echo "Lima installed to /usr/local/bin"
"#,
            lima_arch
        );

        // Write script to temp file
        let script_path = "/tmp/install-lima.sh";
        fs::write(script_path, install_script)
            .context("Failed to write install script")?;

        // Make executable
        Command::new("chmod")
            .args(["+x", script_path])
            .status()
            .context("Failed to make install script executable")?;

        // Execute installation script
        let status = Command::new("bash")
            .arg(script_path)
            .status()
            .context("Failed to run Lima installation script")?;

        // Clean up
        let _ = fs::remove_file(script_path);

        if !status.success() {
            bail!(
                "Lima installation failed. Please install manually:\n\
                https://lima-vm.io/docs/installation/"
            );
        }

        println!("✅ Lima installed successfully!");
        Ok(())
    }

    /// Execute a limactl command and return output
    fn run_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new(&self.binary)
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute limactl {}", args.join(" ")))?;

        Ok(output)
    }

    /// Execute command and check for success
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

        // Check stderr for hidden errors
        let stderr = String::from_utf8_lossy(&output.stderr);
        check_stderr_for_errors(&stderr, &format!("limactl {}", args.join(" ")))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Generate a Lima YAML configuration file
    fn generate_lima_config(&self, config: &VmConfig) -> Result<String> {
        let cloud_init_section = if let Some(ref ci_path) = config.cloud_init {
            // Read cloud-init content
            let ci_content = fs::read_to_string(ci_path)
                .context("Failed to read cloud-init file")?;

            // Lima expects cloud-init to be embedded in the provision section
            format!(
                r#"
provision:
  - mode: system
    script: |
      #!/bin/bash
      set -e
      # Apply cloud-init manually
      {}
"#,
                ci_content.lines()
                    .filter(|l| !l.starts_with("#cloud-config"))
                    .collect::<Vec<_>>()
                    .join("\n      ")
            )
        } else {
            String::new()
        };

        Ok(format!(
            r#"# Lima VM configuration for Capsule VM
cpus: {}
memory: "{}"
disk: "{}"

images:
  - location: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-amd64.img"
    arch: "x86_64"
  - location: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-arm64.img"
    arch: "aarch64"

mounts:
  - location: "~"
    writable: false
  - location: "/tmp/lima"
    writable: true

ssh:
  localPort: 0
  loadDotSSHPubKeys: true

containerd:
  system: false
  user: false

# Use ubuntu user for compatibility with Multipass
user:
  name: ubuntu
  uid: 1000
  home: /home/ubuntu
  shell: /bin/bash
{}
"#,
            config.cpus,
            config.memory,
            config.disk,
            cloud_init_section
        ))
    }

    /// Parse lima list JSON output
    fn parse_vm_list(&self, json_output: &str) -> Result<Vec<VmInfo>> {
        // Lima outputs warnings to stdout when there are no instances
        // These start with "time=" so we need to filter them out
        let json_output = json_output.trim();

        // If output is empty or contains warning messages, return empty list
        if json_output.is_empty() || json_output.starts_with("time=") {
            return Ok(Vec::new());
        }

        let data: serde_json::Value = serde_json::from_str(json_output)
            .context("Failed to parse lima list output")?;

        // Lima returns a single object for one VM, or an array for multiple
        let list = if data.is_array() {
            data.as_array().unwrap().clone()
        } else {
            vec![data]
        };

        let mut vms = Vec::new();
        for vm in list {
            let name = vm["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("VM missing name field"))?
                .to_string();

            let status = vm["status"]
                .as_str()
                .unwrap_or("Unknown")
                .to_string();

            // Lima uses different state names, normalize them
            let state = match status.as_str() {
                "Running" => "Running",
                "Stopped" => "Stopped",
                _ => &status,
            }
            .to_string();

            // Extract IP addresses if available
            let ipv4 = if let Some(addresses) = vm["addresses"].as_array() {
                addresses
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            } else {
                vec![]
            };

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

        // Verify lima is working
        let output = self.run_command(&["--version"])?;
        if !output.status.success() {
            bail!("lima is installed but not working properly");
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

        // Generate Lima configuration
        let lima_config = self.generate_lima_config(config)?;

        // Write config to temp file
        let config_path = format!("/tmp/capsule-vm-{}.yaml", config.name);
        fs::write(&config_path, lima_config)
            .context("Failed to write Lima config file")?;

        // Launch VM with retry
        retry_operation(
            || {
                let output = self.run_command(&[
                    "start",
                    "--name",
                    &config.name,
                    "--tty=false",
                    &config_path,
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

        // Clean up temp config file
        let _ = fs::remove_file(&config_path);

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
        // Stop first (best effort)
        let _ = self.run_command(&["stop", name]);

        // Delete the VM
        self.run_command_checked(&["delete", name])?;

        Ok(())
    }

    fn list(&self) -> Result<Vec<VmInfo>> {
        let output = self.run_command_checked(&["list", "--json"])?;
        self.parse_vm_list(&output)
    }

    fn info(&self, name: &str) -> Result<VmInfo> {
        let vms = self.list()?;
        vms.into_iter()
            .find(|vm| vm.name == name)
            .ok_or_else(|| VmError::VmNotFound { name: name.to_string() }.into())
    }

    fn exec(&self, name: &str, command: &[&str]) -> Result<String> {
        let mut args = vec!["shell", name];
        args.extend_from_slice(command);

        self.run_command_checked(&args)
    }

    fn exec_passthrough(&self, name: &str, command: &[&str]) -> Result<()> {
        let mut args = vec!["shell", name];
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
        // Lima uses scp-like syntax for copy
        let source_str = source
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid source path"))?;

        self.run_command_checked(&["copy", source_str, &format!("{}:{}", name, dest)])?;

        Ok(())
    }

    fn mount(&self, name: &str, source: &Path, dest: &str) -> Result<()> {
        // Lima mounts are configured in the VM YAML file
        // The host's home directory is already mounted, so we use a bind mount
        // to make the source path available at the destination

        let source_str = source
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid source path"))?;

        // Create the destination directory with correct ownership
        self.exec(name, &["sudo", "mkdir", "-p", dest])?;
        self.exec(name, &["sudo", "chown", "ubuntu:ubuntu", dest])?;

        // Use bind mount to mount the source (which is already accessible via Lima's mount)
        // to the destination path
        self.exec(
            name,
            &[
                "sudo",
                "mount",
                "--bind",
                source_str,
                dest,
            ],
        )?;

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
        // Best effort - Lima handles mounts differently
        let _ = self.run_command(&["unmount", name]);
        Ok(())
    }

    fn wait_for_ready(&self, name: &str) -> Result<()> {
        println!("⏳ Waiting for VM to be ready...");

        // Lima VMs are generally ready faster than multipass
        // Just wait for SSH to be available
        retry_operation(
            || {
                self.exec(name, &["true"])?;
                Ok(())
            },
            RetryConfig::with_delays(
                10,
                Duration::from_secs(2),
                Duration::from_secs(10),
            ),
            "wait for VM SSH",
        )?;

        // Run comprehensive health checks
        health_check_vm(name, "lima")?;

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
