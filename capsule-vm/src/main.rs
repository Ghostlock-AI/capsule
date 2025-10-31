use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use std::io::Write;
use std::path::{Path, PathBuf};

// New modules
mod backends;
mod errors;
mod retry;
mod validation;
mod vm_backend;

use vm_backend::{VmBackend, VmConfig, create_backend, get_default_backend};

const ASCII_LOGO: &str = include_str!("ascii_logo.txt");

fn red_banner() -> String {
    format!("\x1b[90m{}\x1b[0m", ASCII_LOGO)
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
    /// Create a sandbox VM
    Create {
        /// Name of the sandbox (VM)
        name: String,
        /// vCPUs (e.g., 1, 2)
        #[arg(long, default_value_t = 2)]
        cpus: u8,
        /// Memory (e.g., 1G, 2048M)
        #[arg(long, default_value = "1G")]
        memory: String,
        /// Disk size (e.g., 8G)
        #[arg(long, default_value = "8G")]
        disk: String,
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
                    out.push_str("  \x1b[90m");
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
            cpus,
            memory,
            disk,
            template,
        } => cmd_create(
            backend.as_ref(),
            &name,
            cpus,
            &memory,
            &disk,
            template.as_deref(),
        )?,
        Cmd::Ps => cmd_ps(backend.as_ref())?,
        Cmd::Start { name } => cmd_start(backend.as_ref(), &name)?,
        Cmd::Stop { name } => cmd_stop(backend.as_ref(), &name)?,
        Cmd::Delete { name } => cmd_delete(backend.as_ref(), &name)?,
        Cmd::Shell { name } => cmd_shell(backend.as_ref(), &name)?,
    }
    Ok(())
}

/* ========================= Commands ========================= */

fn cmd_create(
    backend: &dyn VmBackend,
    name: &str,
    cpus: u8,
    memory: &str,
    disk: &str,
    template_override: Option<&Path>,
) -> Result<()> {
    println!("Creating VM '{}'...", name);

    let ci_path: Option<String> = if let Some(tpl) = template_override {
        Some(tpl.to_string_lossy().to_string())
    } else {
        let default = PathBuf::from("./cloud-init.yaml");
        if default.exists() {
            Some(default.to_string_lossy().to_string())
        } else {
            None
        }
    };

    let mut config = VmConfig::new(name)
        .with_cpus(cpus)
        .with_memory(memory)
        .with_disk(disk);

    if let Some(ci) = ci_path {
        config = config.with_cloud_init(ci);
    }

    backend.create(&config)?;
    backend.wait_for_ready(name)?;

    println!(
        "VM '{}' is ready. Run 'capsule-vm shell {}' and 'tracee --version' inside.",
        name, name
    );
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
    println!("{}", "-".repeat(84));

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
    Ok(())
}

fn cmd_stop(backend: &dyn VmBackend, name: &str) -> Result<()> {
    println!("⏸  Stopping VM '{}'...", name);
    backend.stop(name)?;
    println!("✅ VM stopped!");
    Ok(())
}

fn cmd_delete(backend: &dyn VmBackend, name: &str) -> Result<()> {
    println!("🗑  Deleting VM '{}'...", name);
    backend.delete(name)?;
    println!("✅ VM deleted!");
    Ok(())
}

fn cmd_shell(backend: &dyn VmBackend, name: &str) -> Result<()> {
    backend.shell(name)?;
    Ok(())
}
