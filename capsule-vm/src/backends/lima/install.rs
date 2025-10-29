use super::LimaBackend;
use anyhow::{Context, Result, bail};
use std::fs;
use std::process::Command;

impl LimaBackend {
    pub(super) fn install_lima() -> Result<()> {
        println!("🔧 Detecting platform and installing Lima...");
        match std::env::consts::OS {
            "macos" => Self::install_lima_macos(),
            "linux" => Self::install_lima_linux(),
            other => bail!(
                "Unsupported OS: {}. Please install Lima manually from https://lima-vm.io/",
                other
            ),
        }
    }

    fn install_lima_macos() -> Result<()> {
        println!("🍺 Installing Lima via Homebrew...");
        if which::which("brew").is_err() {
            bail!(
                "Homebrew not found. Please install it first:\n\
                /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"\n\
                Or install Lima manually: https://lima-vm.io/"
            );
        }

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

    fn install_lima_linux() -> Result<()> {
        println!("🐧 Installing Lima on Linux...");
        match Self::detect_linux_distro()?.as_str() {
            "ubuntu" | "debian" => Self::install_lima_debian(),
            "fedora" | "rhel" | "centos" => Self::install_lima_fedora(),
            "arch" => Self::install_lima_arch(),
            other => {
                eprintln!("⚠️  Unknown Linux distribution: {}", other);
                eprintln!("Attempting generic installation...");
                Self::install_lima_generic_linux()
            }
        }
    }

    fn detect_linux_distro() -> Result<String> {
        if let Ok(content) = fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if let Some(id) = line.strip_prefix("ID=") {
                    return Ok(id.trim_matches('"').to_lowercase());
                }
            }
        }
        Ok("unknown".to_string())
    }

    fn install_lima_debian() -> Result<()> {
        println!("📦 Installing Lima via apt...");

        let status = Command::new("sudo")
            .args(["apt-get", "update", "-y"])
            .status()
            .context("Failed to run apt-get update")?;
        if !status.success() {
            bail!("apt-get update failed");
        }

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

        let pkg_manager = if which::which("dnf").is_ok() {
            "dnf"
        } else {
            "yum"
        };

        let status = Command::new("sudo")
            .args([pkg_manager, "install", "-y", "lima"])
            .status()
            .with_context(|| format!("Failed to run {} install lima", pkg_manager))?;

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
            .context("Failed to run pacman install lima")?;

        if !status.success() {
            eprintln!("⚠️  Pacman install failed, trying generic install...");
            return Self::install_lima_generic_linux();
        }

        println!("✅ Lima installed successfully!");
        Ok(())
    }

    fn install_lima_generic_linux() -> Result<()> {
        println!("⬇️  Falling back to GitHub release installer for Lima...");

        let lima_arch = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            other => bail!("Unsupported CPU architecture: {}", other),
        };

        let install_script = format!(
            r#"#!/bin/bash
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

        let script_path = "/tmp/install-lima.sh";
        fs::write(script_path, install_script).context("Failed to write install script")?;

        Command::new("chmod")
            .args(["+x", script_path])
            .status()
            .context("Failed to make install script executable")?;

        let status = Command::new("bash")
            .arg(script_path)
            .status()
            .context("Failed to run Lima installation script")?;

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
}
