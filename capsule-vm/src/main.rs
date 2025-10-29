use anyhow::{Context, Result, anyhow};
use clap::{CommandFactory, Parser, Subcommand};
use directories::UserDirs;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::fs::canonicalize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

// New modules
mod backends;
mod errors;
mod installs;
mod retry;
mod validation;
mod vm_backend;

use vm_backend::{VmBackend, VmConfig, create_backend, get_default_backend};

const ASCII_LOGO: &str = include_str!("ascii_logo.txt");

fn red_banner() -> String {
    format!("\x1b[1;31m{}\x1b[0m", ASCII_LOGO)
}

#[derive(Parser)]
#[command(
    name = "capsule-vm",
    version,
    about = "Capsule VM: tiny VM orchestrator for secure, traceable, ephemeral agents"
)]
struct Cli {
    /// Backend to use (currently only 'lima'). Defaults to lima if available.
    #[arg(long, global = true)]
    backend: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a sandbox VM and optionally live-mount PATH into the VM workspace
    Create {
        /// Name of the sandbox (VM)
        name: String,
        /// Optional host path to live-mount into /home/ubuntu/workspace (omit for no sharing)
        path: Option<String>,
        /// vCPUs (e.g., 1, 2)
        #[arg(long, default_value_t = 2)]
        cpus: u8,
        /// Memory (e.g., 1G, 2048M)
        #[arg(long, default_value = "1G")]
        memory: String,
        /// Disk size (e.g., 8G)
        #[arg(long, default_value = "8G")]
        disk: String,
        /// Tools to install inside VM: comma-separated (python,rust,git,build). Use 'none' to skip tool installation.
        #[arg(long, default_value = "python,rust,git,build")]
        tools: String,
        /// Optional explicit cloud-init template path (overrides default)
        #[arg(long)]
        template: Option<PathBuf>,
    },
    /// List sandboxes
    Ps,
    /// Start sandbox (and open a shell)
    Start { name: String },
    /// Stop sandbox
    Stop { name: String },
    /// Delete sandbox (and purge deleted images)
    Delete { name: String },
    /// Open a shell into the sandbox
    Shell { name: String },
    /// Remove cached templates and metadata
    Clean,
    /// Uninstall capsule-vm: remove configs and installed binaries (best effort)
    Uninstall,
    /// Manage tools inside an existing sandbox
    Tools {
        #[command(subcommand)]
        cmd: ToolsCmd,
    },
}

#[derive(Subcommand)]
enum ToolsCmd {
    /// Install tools into an existing VM
    Install {
        /// Name of the sandbox (VM)
        name: String,
        /// Tools to install inside VM: comma-separated (python,rust,git,build)
        #[arg(long)]
        tools: String,
    },
    /// List supported tool names (host side)
    List,
}

struct CreateOptions<'a> {
    name: &'a str,
    path: Option<&'a str>,
    cpus: u8,
    memory: &'a str,
    disk: &'a str,
    tools: &'a str,
    template_override: Option<&'a Path>,
}

fn main() -> Result<()> {
    // Show ASCII banner before default/top-level help
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() == 1 || (argv.len() == 2 && (argv[1] == "--help" || argv[1] == "-h")) {
        println!("{}", red_banner());
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        cmd.write_help(&mut buf)?;
        let s = String::from_utf8_lossy(&buf);
        let mut out = String::new();
        let mut in_commands = false;
        for line in s.lines() {
            let t = line.trim_end();
            if t.starts_with("Commands:") {
                in_commands = true;
            } else if t.starts_with("Options:") {
                in_commands = false;
            }
            if in_commands && t.starts_with("  ") && t.len() > 2 {
                let rest = &t[2..];
                if !rest.is_empty() && !rest.chars().next().unwrap().is_whitespace() {
                    let mut it = rest.splitn(2, char::is_whitespace);
                    let name = it.next().unwrap_or("");
                    let rem = it.next().unwrap_or("");
                    out.push_str("  \x1b[1;31m");
                    out.push_str(name);
                    out.push_str("\x1b[0m");
                    out.push_str(rem);
                    out.push('\n');
                    continue;
                }
            }
            out.push_str(t);
            out.push('\n');
        }
        print!("{}", out);
        std::io::stdout().flush().ok();
        return Ok(());
    }

    let cli = Cli::parse();
    ensure_workspace()?;

    // Get backend (from CLI arg or auto-detect)
    let backend: Box<dyn VmBackend> = if let Some(ref backend_name) = cli.backend {
        create_backend(backend_name)?
    } else {
        get_default_backend()?
    };

    println!("🔧 Using backend: {}", backend.name());
    backend.ensure_available()?;

    match cli.cmd {
        Cmd::Create {
            name,
            path,
            cpus,
            memory,
            disk,
            tools,
            template,
        } => {
            let options = CreateOptions {
                name: &name,
                path: path.as_deref(),
                cpus,
                memory: &memory,
                disk: &disk,
                tools: &tools,
                template_override: template.as_deref(),
            };
            cmd_create(backend.as_ref(), options)?
        }
        Cmd::Ps => cmd_ps(backend.as_ref())?,
        Cmd::Start { name } => cmd_start(backend.as_ref(), &name)?,
        Cmd::Stop { name } => cmd_stop(backend.as_ref(), &name)?,
        Cmd::Delete { name } => cmd_delete(backend.as_ref(), &name)?,
        Cmd::Shell { name } => cmd_shell(backend.as_ref(), &name)?,
        Cmd::Clean => cmd_clean()?,
        Cmd::Uninstall => cmd_uninstall()?,
        Cmd::Tools { cmd } => match cmd {
            ToolsCmd::Install { name, tools } => {
                cmd_tools_install(backend.as_ref(), &name, &tools)?
            }
            ToolsCmd::List => cmd_tools_list()?,
        },
    }
    Ok(())
}

/* ========================= Commands ========================= */

fn cmd_create(backend: &dyn VmBackend, opts: CreateOptions<'_>) -> Result<()> {
    let start_time = Instant::now();
    let spinner_style = ProgressStyle::default_spinner()
        .template("{spinner:.cyan} {msg} [{elapsed_precise}]")
        .unwrap();

    // 1) Resolve cloud-init template
    let ci_path: Option<String> = if let Some(tpl) = opts.template_override {
        Some(tpl.to_string_lossy().to_string())
    } else {
        let p = PathBuf::from("./cloud-init.yaml");
        if p.exists() {
            Some(p.to_string_lossy().to_string())
        } else {
            None
        }
    };

    // 2) Create VM config
    let mut config = VmConfig::new(opts.name)
        .with_cpus(opts.cpus)
        .with_memory(opts.memory)
        .with_disk(opts.disk);

    if let Some(ci) = ci_path {
        config = config.with_cloud_init(ci);
    }

    // 3) Create VM with backend
    let step_start = Instant::now();
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style.clone());
    spinner.set_message(format!("Creating VM '{}'", opts.name));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    backend.create(&config)?;
    spinner.finish_and_clear();
    println!(
        "✅ Created VM '{}' ({:.1}s)",
        opts.name,
        step_start.elapsed().as_secs_f64()
    );

    // 4) Record metadata
    if let Some(p) = opts.path {
        let abs = canonicalize(p)?;
        save_metadata(opts.name, &abs)?;
    } else {
        save_metadata(opts.name, Path::new("(none)"))?;
    }

    // 5) Wait for VM to be ready
    let step_start = Instant::now();
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style.clone());
    spinner.set_message("Waiting for VM to be ready");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    backend.wait_for_ready(opts.name)?;
    spinner.finish_and_clear();
    println!(
        "✅ VM is ready ({:.1}s)",
        step_start.elapsed().as_secs_f64()
    );

    // 6) Install tools
    if !opts.tools.is_empty() && opts.tools != "none" {
        let step_start = Instant::now();
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(spinner_style.clone());
        spinner.set_message(format!("Installing tools: {}", opts.tools));
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        installs::install_tools(backend, opts.name, opts.tools)?;
        spinner.finish_and_clear();
        println!(
            "✅ Installed tools: {} ({:.1}s)",
            opts.tools,
            step_start.elapsed().as_secs_f64()
        );
    }

    // 7) Setup workspace
    if let Some(p) = opts.path {
        let step_start = Instant::now();
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(spinner_style.clone());
        spinner.set_message("Mounting workspace");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        setup_workspace(backend, opts.name, p)?;
        spinner.finish_and_clear();
        println!(
            "✅ Workspace mounted at ~/workspace ({:.1}s)",
            step_start.elapsed().as_secs_f64()
        );
    } else {
        create_workspace_dir(backend, opts.name)?;
    }

    // 8) Print summary
    let elapsed = start_time.elapsed();
    println!(
        "\n🎉 VM '{}' ready in {:.1}s",
        opts.name,
        elapsed.as_secs_f64()
    );
    println!("   • Enter the VM:  capsule-vm shell {}", opts.name);
    println!("   • Workspace:     live at ~/workspace");
    Ok(())
}

fn cmd_ps(backend: &dyn VmBackend) -> Result<()> {
    let mut vms = match backend.list() {
        Ok(vms) => vms,
        Err(e) => {
            eprintln!("⚠️  Failed to list VMs from {}: {}", backend.name(), e);
            Vec::new()
        }
    };

    for vm in &mut vms {
        vm.release = Some(format!(
            "{} ({})",
            vm.release.as_deref().unwrap_or(""),
            backend.name()
        ));
    }

    println!(
        "{:<20} {:<15} {:<20} {:<25}",
        "Name", "State", "IPv4", "Backend"
    );
    println!("{}", "-".repeat(80));

    for vm in vms {
        let ip = if vm.ipv4.is_empty() {
            "-".to_string()
        } else {
            vm.ipv4.join(", ")
        };
        let backend_info = vm.release.unwrap_or_else(|| "Unknown".to_string());
        println!(
            "{:<20} {:<15} {:<20} {:<25}",
            vm.name, vm.state, ip, backend_info
        );
    }

    Ok(())
}

fn cmd_start(backend: &dyn VmBackend, name: &str) -> Result<()> {
    println!("▶️  Starting VM '{}'...", name);
    backend.start(name)?;
    println!("✅ VM started!");

    // Auto-shell for transient feel
    shell_into(backend, name)
}

fn cmd_stop(backend: &dyn VmBackend, name: &str) -> Result<()> {
    println!("⏸  Stopping VM '{}'...", name);
    backend.stop(name)?;
    println!("✅ VM stopped!");
    Ok(())
}

fn cmd_delete(backend: &dyn VmBackend, name: &str) -> Result<()> {
    let start_time = Instant::now();
    let spinner_style = ProgressStyle::default_spinner()
        .template("{spinner:.cyan} {msg} [{elapsed_precise}]")
        .unwrap();

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style);
    spinner.set_message(format!("Deleting VM '{}'", name));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    backend.delete(name)?;
    remove_metadata(name)?;

    spinner.finish_and_clear();
    let elapsed = start_time.elapsed();
    println!("✅ VM '{}' deleted in {:.1}s", name, elapsed.as_secs_f64());
    Ok(())
}

fn cmd_shell(backend: &dyn VmBackend, name: &str) -> Result<()> {
    shell_into(backend, name)
}

fn cmd_tools_install(backend: &dyn VmBackend, name: &str, tools: &str) -> Result<()> {
    installs::install_tools(backend, name, tools)
}

fn cmd_tools_list() -> Result<()> {
    installs::print_supported_tools()
}

fn cmd_clean() -> Result<()> {
    if let Some(user_dirs) = UserDirs::new() {
        let home = user_dirs.home_dir();
        let ws = home.join(".capsule-vm");
        if ws.exists() {
            fs::remove_dir_all(&ws).context("Failed to remove ~/.capsule-vm")?;
            println!("Removed ~/.capsule-vm");
        } else {
            println!("~/.capsule-vm not found, nothing to clean");
        }
    }
    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    println!("Uninstalling capsule-vm (best effort)...");

    // Remove metadata
    cmd_clean()?;

    // Try to remove binary from common locations
    let paths = vec![
        "/usr/local/bin/capsule-vm",
        "/usr/bin/capsule-vm",
        "~/.local/bin/capsule-vm",
    ];

    for p in paths {
        let expanded = shellexpand::tilde(p);
        let path = Path::new(expanded.as_ref());
        if path.exists() {
            if let Err(e) = fs::remove_file(path) {
                eprintln!("Failed to remove {}: {}", p, e);
            } else {
                println!("Removed {}", p);
            }
        }
    }

    println!("Uninstall complete.");
    println!("Note: VMs created with capsule-vm are not deleted automatically.");
    println!("Use Lima (limactl) to manage them.");
    Ok(())
}

/* ========================= Helper Functions ========================= */

fn shell_into(backend: &dyn VmBackend, name: &str) -> Result<()> {
    // Ensure login profile is installed (capsule-info, motd, etc.)
    let _ = ensure_login_profile(backend, name);

    backend.shell(name)?;
    Ok(())
}

fn setup_workspace(backend: &dyn VmBackend, name: &str, host_path: &str) -> Result<()> {
    let abs = canonicalize(host_path)?;

    // Create workspace directory in VM
    backend.exec(name, &["sudo", "mkdir", "-p", "/home/ubuntu/workspace"])?;

    backend.exec(
        name,
        &[
            "sudo",
            "chown",
            "-R",
            "ubuntu:ubuntu",
            "/home/ubuntu/workspace",
        ],
    )?;

    // Mount
    backend.mount(name, &abs, "/home/ubuntu/workspace")?;

    // Ensure login profile
    ensure_login_profile(backend, name)?;

    Ok(())
}

fn create_workspace_dir(backend: &dyn VmBackend, name: &str) -> Result<()> {
    backend.exec(name, &["sudo", "mkdir", "-p", "/home/ubuntu/workspace"])?;

    backend.exec(
        name,
        &[
            "sudo",
            "chown",
            "-R",
            "ubuntu:ubuntu",
            "/home/ubuntu/workspace",
        ],
    )?;

    // Ensure login profile
    ensure_login_profile(backend, name)?;

    Ok(())
}

fn ensure_login_profile(backend: &dyn VmBackend, name: &str) -> Result<()> {
    // Install capsule-info script
    ensure_capsule_info(backend, name)?;

    // Create profile.d script that shows banner and capsule-info
    let profile_script = r#"#!/bin/bash
# Capsule VM login profile

# Show MOTD on login (if exists)
if [ -f /etc/motd ]; then
    cat /etc/motd
fi

# Show capsule info
if [ -f /usr/local/bin/capsule-info ]; then
    /usr/local/bin/capsule-info
fi
"#;

    // Write profile script to temp file
    let temp_path = std::env::temp_dir().join(format!("capsule-profile-{}.sh", name));
    fs::write(&temp_path, profile_script)?;

    // Transfer and install
    backend.transfer(name, &temp_path, "/tmp/10-capsule.sh")?;

    backend.exec(
        name,
        &[
            "sudo",
            "mv",
            "/tmp/10-capsule.sh",
            "/etc/profile.d/10-capsule.sh",
        ],
    )?;

    backend.exec(
        name,
        &["sudo", "chmod", "+x", "/etc/profile.d/10-capsule.sh"],
    )?;

    // Clean up temp file
    let _ = fs::remove_file(&temp_path);

    Ok(())
}

fn ensure_capsule_info(backend: &dyn VmBackend, name: &str) -> Result<()> {
    let script = r#"#!/bin/bash
# capsule-info: Display Capsule VM information

echo "═══════════════════════════════════════════════════════════"
echo "  CAPSULE VM: $HOSTNAME"
echo "═══════════════════════════════════════════════════════════"
echo ""

# Workspace info
if mountpoint -q /home/ubuntu/workspace 2>/dev/null; then
    echo "📂 Workspace: /home/ubuntu/workspace (mounted from host)"
else
    echo "📂 Workspace: /home/ubuntu/workspace (local)"
fi

# IP address
IP=$(hostname -I | awk '{print $1}')
echo "🌐 IP Address: ${IP:-N/A}"

# Resources
CPUS=$(nproc)
MEM=$(free -h | awk '/^Mem:/ {print $2}')
echo "💻 Resources: ${CPUS} CPUs, ${MEM} RAM"

echo ""
echo "═══════════════════════════════════════════════════════════"
"#;

    // Write script to temp file
    let temp_path = std::env::temp_dir().join(format!("capsule-info-{}.sh", name));
    fs::write(&temp_path, script)?;

    // Transfer and install
    backend.transfer(name, &temp_path, "/tmp/capsule-info")?;

    backend.exec(
        name,
        &[
            "sudo",
            "mv",
            "/tmp/capsule-info",
            "/usr/local/bin/capsule-info",
        ],
    )?;

    backend.exec(
        name,
        &["sudo", "chmod", "+x", "/usr/local/bin/capsule-info"],
    )?;

    // Clean up temp file
    let _ = fs::remove_file(&temp_path);

    Ok(())
}

/* ========================= Workspace & Metadata ========================= */

fn ensure_workspace() -> Result<()> {
    if let Some(user_dirs) = UserDirs::new() {
        let home = user_dirs.home_dir();
        let ws = home.join(".capsule-vm");
        if !ws.exists() {
            fs::create_dir_all(&ws).context("Failed to create ~/.capsule-vm")?;
        }
        Ok(())
    } else {
        Err(anyhow!("Cannot determine user home directory"))
    }
}

fn save_metadata(name: &str, source: &Path) -> Result<()> {
    if let Some(user_dirs) = UserDirs::new() {
        let home = user_dirs.home_dir();
        let meta_file = home.join(".capsule-vm").join(format!("{}.meta", name));
        let content = format!("name={}\nsource={}\n", name, source.display());
        fs::write(&meta_file, content).context("Failed to write metadata")?;
    }
    Ok(())
}

fn remove_metadata(name: &str) -> Result<()> {
    if let Some(user_dirs) = UserDirs::new() {
        let home = user_dirs.home_dir();
        let meta_file = home.join(".capsule-vm").join(format!("{}.meta", name));
        if meta_file.exists() {
            fs::remove_file(&meta_file)?;
        }
    }
    Ok(())
}
