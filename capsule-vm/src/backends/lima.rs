use crate::errors::VmError;
use crate::retry::{RetryConfig, retry_operation};
use crate::validation::{check_stderr_for_errors, health_check_vm, validate_vm_config};
use crate::vm_backend::{VmBackend, VmConfig, VmInfo};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
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
            _ => bail!(
                "Unsupported OS: {}. Please install Lima manually from https://lima-vm.io/",
                os
            ),
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
            bail!(
                "Failed to install Lima via Homebrew. Please install manually: https://lima-vm.io/"
            );
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
            _ => bail!(
                "Unsupported architecture: {}. Please install Lima manually: https://lima-vm.io/",
                arch
            ),
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
        fs::write(script_path, install_script).context("Failed to write install script")?;

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

    /// Get the path to the Lima template file (legacy method)
    fn get_template_path(&self) -> Result<String> {
        // Look for template in current directory first
        let local_template = PathBuf::from("./lima-template.yaml");
        if local_template.exists() {
            return Ok(local_template.to_string_lossy().to_string());
        }

        // Look in the binary's directory
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let exe_template = exe_dir.join("lima-template.yaml");
                if exe_template.exists() {
                    return Ok(exe_template.to_string_lossy().to_string());
                }
            }
        }

        // Fallback: write embedded template to temp file
        let template_content = include_str!("../../lima-template.yaml");
        let temp_path = "/tmp/capsule-vm-lima-template.yaml";
        fs::write(temp_path, template_content).context("Failed to write embedded Lima template")?;
        Ok(temp_path.to_string())
    }

    /// Render Lima template with cloud-init content injected
    fn render_template_with_cloudinit(&self, config: &VmConfig) -> Result<String> {
        // Check if new template system exists
        let new_template_path = PathBuf::from("templates/lima-base.yaml");
        if !new_template_path.exists() {
            // Fall back to old method if new templates don't exist
            return self.get_template_path();
        }

        // Read base template
        let template_content = fs::read_to_string(&new_template_path)
            .context("Failed to read templates/lima-base.yaml")?;

        // Read cloud-init script
        let cloud_init_path = config
            .cloud_init
            .as_ref()
            .map(|p| p.clone())
            .unwrap_or_else(|| "./cloud-init.yaml".to_string());

        let cloud_init_content =
            fs::read_to_string(&cloud_init_path).context("Failed to read cloud-init file")?;

        // Convert cloud-init YAML to shell script for Lima provision
        let cloud_init_script = self.convert_cloudinit_to_script(&cloud_init_content)?;

        // Inject cloud-init into template
        let rendered = template_content.replace("{{CLOUD_INIT_CONTENT}}", &cloud_init_script);

        // Write rendered template to runtime directory
        let runtime_dir = PathBuf::from(std::env::var("HOME")?).join(".capsule-vm/runtime");
        fs::create_dir_all(&runtime_dir)?;

        let output_path = runtime_dir.join(format!("{}.yaml", config.name));
        fs::write(&output_path, rendered)?;

        Ok(output_path.to_string_lossy().to_string())
    }

    /// Run Tracee provisioning using the provision-tracee.sh script logic
    fn run_tracee_provision(&self, vm_name: &str) -> Result<()> {
        use std::process::Command;

        // Run the provision-tracee.sh script if it exists
        let script_path = PathBuf::from("./scripts/provision-tracee.sh");
        if script_path.exists() {
            let output = Command::new("bash")
                .arg(&script_path)
                .arg(vm_name)
                .output()
                .context("Failed to run provision-tracee.sh")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("Tracee provision warning: {}", stderr);
            }
        }

        Ok(())
    }

    /// Run the provision script from the template
    fn run_provision_script(&self, vm_name: &str) -> Result<()> {
        // Read the cloud-init to extract provision commands
        let cloud_init_content =
            fs::read_to_string("./cloud-init.yaml").context("Failed to read cloud-init.yaml")?;

        // Extract and create write_files first (AppArmor profile, capsule-run)
        self.create_writefiles_from_cloudinit(vm_name, &cloud_init_content)?;

        // Then run provision script
        let provision_script = self.convert_cloudinit_to_script(&cloud_init_content)?;

        // Write script to temp file and execute in VM
        let script_content = format!("#!/bin/bash\nset -e\n{}", provision_script);
        let temp_file = "/tmp/capsule-provision.sh";

        self.exec(
            vm_name,
            &[
                "sudo",
                "bash",
                "-c",
                &format!(
                    "cat > {} << 'PROVISION_EOF'\n{}\nPROVISION_EOF",
                    temp_file, script_content
                ),
            ],
        )?;
        self.exec(vm_name, &["sudo", "chmod", "+x", temp_file])?;
        self.exec(vm_name, &["sudo", "bash", temp_file])?;
        self.exec(vm_name, &["sudo", "rm", temp_file])?;

        Ok(())
    }

    /// Create write_files from cloud-init (AppArmor profile, capsule-run, etc)
    fn create_writefiles_from_cloudinit(&self, vm_name: &str, cloud_init: &str) -> Result<()> {
        // Simple parser: extract write_files section
        let mut in_write_files = false;
        let mut current_file: Option<(String, String, String)> = None; // (path, permissions, content)
        let mut collecting_content = false;
        let mut content_lines = Vec::new();

        for line in cloud_init.lines() {
            if line.trim() == "write_files:" {
                in_write_files = true;
                continue;
            }

            if in_write_files {
                // End of write_files section
                if !line.starts_with("  ") && !line.trim().is_empty() {
                    break;
                }

                // New file entry
                if line.starts_with("  - path:") {
                    // Save previous file if any
                    if let Some((path, perms, content)) = current_file.take() {
                        self.write_file_to_vm(vm_name, &path, &perms, &content)?;
                    }

                    let path = line.split("path:").nth(1).unwrap().trim().to_string();
                    current_file = Some((path, String::new(), String::new()));
                    collecting_content = false;
                    content_lines.clear();
                } else if line.trim().starts_with("permissions:") {
                    if let Some(ref mut file) = current_file {
                        let perms = line
                            .split("permissions:")
                            .nth(1)
                            .unwrap()
                            .trim()
                            .trim_matches('"')
                            .to_string();
                        file.1 = perms;
                    }
                } else if line.trim() == "content: |" {
                    collecting_content = true;
                } else if collecting_content && line.starts_with("      ") {
                    content_lines.push(line[6..].to_string());
                }
            }
        }

        // Save last file
        if let Some((path, perms, _)) = current_file {
            let content = content_lines.join("\n");
            self.write_file_to_vm(vm_name, &path, &perms, &content)?;
        }

        Ok(())
    }

    /// Write a single file to the VM
    fn write_file_to_vm(
        &self,
        vm_name: &str,
        path: &str,
        permissions: &str,
        content: &str,
    ) -> Result<()> {
        let escaped_content = content.replace('\'', "'\"'\"'");
        self.exec(
            vm_name,
            &[
                "sudo",
                "bash",
                "-c",
                &format!(
                    "cat > {} << 'WRITEFILE_EOF'\n{}\nWRITEFILE_EOF",
                    path, escaped_content
                ),
            ],
        )?;
        if !permissions.is_empty() {
            self.exec(vm_name, &["sudo", "chmod", permissions, path])?;
        }
        Ok(())
    }

    /// Convert cloud-init YAML to shell script for Lima provision
    fn convert_cloudinit_to_script(&self, cloud_init_yaml: &str) -> Result<String> {
        // Parse cloud-init YAML and extract runcmd section
        // For simplicity, we'll extract the runcmd section as shell commands
        let mut script = String::new();
        let mut in_runcmd = false;

        for line in cloud_init_yaml.lines() {
            if line.trim() == "runcmd:" {
                in_runcmd = true;
                continue;
            }

            if in_runcmd {
                // Check if we've left the runcmd section
                if !line.starts_with("  - ") && !line.trim().is_empty() && !line.starts_with("    ")
                {
                    break;
                }

                // Extract command (remove "  - " prefix) and add proper indentation for YAML
                if line.starts_with("  - ") {
                    let cmd = line.trim_start_matches("  - ");
                    script.push_str("      "); // 6 spaces for proper YAML indentation under script: |
                    script.push_str(cmd);
                    script.push('\n');
                }
            }
        }

        Ok(script)
    }

    /// Generate a Lima YAML configuration file (legacy, kept for reference)
    #[allow(dead_code)]
    fn generate_lima_config(&self, config: &VmConfig) -> Result<String> {
        let cloud_init_section = if let Some(ref ci_path) = config.cloud_init {
            // Read cloud-init content
            let ci_content =
                fs::read_to_string(ci_path).context("Failed to read cloud-init file")?;

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
                ci_content
                    .lines()
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
    writable: true
  - location: "/tmp/lima"
    writable: true

ssh:
  localPort: 0
  loadDotSSHPubKeys: true

containerd:
  system: false
  user: false

# Use ubuntu user for compatibility with the default cloud images
user:
  name: ubuntu
  uid: 1000
  home: /home/ubuntu
  shell: /bin/bash
{}
"#,
            config.cpus, config.memory, config.disk, cloud_init_section
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

        // Lima returns JSON objects one per line for multiple VMs
        let mut list = Vec::new();
        for line in json_output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let data: serde_json::Value =
                serde_json::from_str(line).context("Failed to parse lima list output")?;
            list.push(data);
        }

        let mut vms = Vec::new();
        for vm in list {
            let name = vm["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("VM missing name field"))?
                .to_string();

            let status = vm["status"].as_str().unwrap_or("Unknown").to_string();

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

        // Use new template system with cloud-init integration
        let template_path = self.render_template_with_cloudinit(config)?;

        // Launch VM with retry using template + overrides for cpu/memory/disk
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

        // Verify VM was created successfully
        self.verify_state(&config.name, "Running")?;

        // Tracee is installed via cloud-init (no additional provisioning needed)
        println!("✅ VM provisioned (cloud-init includes Tracee, AppArmor, and security profiles)");

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
        vms.into_iter().find(|vm| vm.name == name).ok_or_else(|| {
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
        self.exec(name, &["sudo", "mount", "--bind", source_str, dest])?;

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
        // Lima VMs are generally quick to become reachable over SSH
        retry_operation(
            || {
                self.exec(name, &["true"])?;
                Ok(())
            },
            RetryConfig::with_delays(10, Duration::from_secs(2), Duration::from_secs(10)),
            "wait for VM SSH",
        )?;

        // Run comprehensive health checks
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
