use anyhow::{bail, Context, Result};
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

// New modules
mod backends;
mod errors;
mod retry;
mod tools;
mod validation;
mod vm_backend;

use tools::{all_tools, ToolKind};
use vm_backend::{create_backend, get_default_backend, VmBackend, VmConfig};

const ASCII_LOGO: &str = include_str!("ascii_logo.txt");

fn red_banner() -> String {
    ASCII_LOGO.to_string()
}

#[derive(Parser)]
#[command(
    name = "capsule",
    version,
    about = "The secure, self-auditing, agent sandbox."
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
        /// Stream Lima serial logs to stdout while provisioning (disable with --no-stream-logs)
        #[arg(long = "no-stream-logs", action = ArgAction::SetFalse, default_value_t = true)]
        stream_logs: bool,
        /// Install predefined tool bundles inside the VM after provisioning (e.g. --tools codex)
        #[arg(long = "tools", value_enum, value_delimiter = ',', num_args = 1..)]
        tools: Vec<ToolKind>,
        /// Path to .env file containing secrets (stored in VM as environment variables)
        #[arg(long)]
        secrets: Option<PathBuf>,
    },
    /// List sandboxes
    Ps,
    /// Start sandbox
    Start { name: String },
    /// Stop sandbox
    Stop { name: String },
    /// Delete sandbox (and purge deleted images)
    Delete { name: String },
    /// List available tool bundles for installation
    Tools,
    /// Stream tracee event logs from the sandbox
    Logs {
        name: String,
        /// Show raw Tracee events instead of human-readable format
        #[arg(long)]
        raw: bool,
        /// Disable colored output
        #[arg(long)]
        no_color: bool,
    },
    /// Open a shell into the sandbox
    Shell {
        name: String,
        /// Connect as root inside the VM (defaults to agent user)
        #[arg(long)]
        root: bool,
    },
    /// Manage secrets (environment variables) in a VM
    Secrets {
        /// Name of the VM
        name: String,
        /// Path to .env file to set/update secrets
        #[arg(long)]
        file: Option<PathBuf>,
        /// Show current secrets (keys only, not values)
        #[arg(long)]
        list: bool,
    },
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
                    out.push_str("  \x1b[1m");
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
            stream_logs,
            tools,
            secrets,
        } => cmd_create(
            backend.as_ref(),
            &name,
            cpus,
            &memory,
            &disk,
            template.as_deref(),
            stream_logs,
            &tools,
            secrets.as_deref(),
        )?,
        Cmd::Ps => cmd_ps(backend.as_ref())?,
        Cmd::Start { name } => cmd_start(backend.as_ref(), &name)?,
        Cmd::Stop { name } => cmd_stop(backend.as_ref(), &name)?,
        Cmd::Delete { name } => cmd_delete(backend.as_ref(), &name)?,
        Cmd::Tools => cmd_tools()?,
        Cmd::Logs {
            name,
            raw,
            no_color,
        } => cmd_logs(backend.as_ref(), &name, raw, no_color)?,
        Cmd::Shell { name, root } => {
            let user = if root { "root" } else { "agent" };
            cmd_shell(backend.as_ref(), &name, user)?
        }
        Cmd::Secrets { name, file, list } => {
            cmd_secrets(backend.as_ref(), &name, file.as_deref(), list)?
        }
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
    stream_logs: bool,
    tools: &[ToolKind],
    secrets_file: Option<&Path>,
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

    if stream_logs {
        println!("📡 Streaming Lima serial logs while provisioning...");
        config = config.with_stream_serial_logs(true);
    }

    // Handle secrets file
    if let Some(env_file) = secrets_file {
        println!("🔐 Loading secrets from {}...", env_file.display());
        let secrets = parse_env_file(env_file)?;
        println!("   Loaded {} secret(s)", secrets.len());
        config = config.with_secrets(secrets);
    }

    let mut unique_tools = Vec::new();
    let mut seen_tools = HashSet::new();
    for &tool in tools {
        if seen_tools.insert(tool) {
            unique_tools.push(tool.definition());
        }
    }

    if !unique_tools.is_empty() {
        println!("🧰 Preparing tool bundles via cloud-init:");
        for def in &unique_tools {
            println!("   - {}: {}", def.name, def.description);
        }

        let mut seen_commands: HashSet<&'static str> = HashSet::new();
        let mut post_install_commands: Vec<String> = Vec::new();
        for def in &unique_tools {
            for &step in def.setup_steps {
                if seen_commands.insert(step) {
                    post_install_commands.push(step.to_string());
                }
            }
        }

        if !post_install_commands.is_empty() {
            config = config.with_post_install_commands(post_install_commands);
        }
    }

    let provisioning_result = (|| -> Result<()> {
        backend.create(&config)?;
        backend.wait_for_ready(name)?;
        Ok(())
    })();

    if stream_logs {
        if let Err(err) = backend.finalize_serial_logs(name) {
            eprintln!(
                "⚠️  Failed to finalize Lima log stream for '{}': {}",
                name, err
            );
        }
    }

    provisioning_result?;

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

fn cmd_shell(backend: &dyn VmBackend, name: &str, user: &str) -> Result<()> {
    println!(
        "🔗 Opening shell to '{}' as {} (Ctrl-D to exit)...",
        name, user
    );
    backend.shell(name, user)?;
    Ok(())
}

fn cmd_logs(backend: &dyn VmBackend, name: &str, raw: bool, no_color: bool) -> Result<()> {
    if raw {
        println!(
            "📄 Streaming raw tracee events for '{}' (Ctrl-C to stop)...",
            name
        );
        backend.stream_tracee_logs(name, true, no_color)?;
    } else {
        println!(
            "📄 Streaming human-readable logs for '{}' (Ctrl-C to stop)...",
            name
        );
        backend.stream_tracee_logs(name, false, no_color)?;
    }
    Ok(())
}

fn cmd_tools() -> Result<()> {
    println!("{:<12} {}", "Tool", "Description");
    println!("{}", "-".repeat(72));
    for def in all_tools() {
        println!("\x1b[1m{:<12}\x1b[0m {}", def.name, def.description);
    }
    println!();
    println!("Install during create: capsule-vm create <name> --tools <tool>[,<tool>...]");
    Ok(())
}

fn cmd_secrets(backend: &dyn VmBackend, name: &str, file: Option<&Path>, list: bool) -> Result<()> {
    if list {
        // List current secret keys (not values)
        println!("🔑 Listing secrets for VM '{}'...", name);
        let output = backend.exec(name, &["sudo", "cat", "/etc/capsule/secrets.env"])?;

        println!("Current secret keys:");
        for line in output.lines() {
            if let Some(key) = line.split('=').next() {
                if !key.starts_with('#') && !key.trim().is_empty() {
                    println!("  - {}", key.trim());
                }
            }
        }
        return Ok(());
    }

    if let Some(env_file) = file {
        println!("🔐 Updating secrets for VM '{}'...", name);

        // Read and parse .env file
        let secrets = parse_env_file(env_file)?;

        // Generate secrets file content
        let mut content = String::from("# Capsule VM Secrets - DO NOT EDIT MANUALLY\n");
        for (key, value) in secrets {
            content.push_str(&format!("{}={}\n", key, value));
        }

        // Write to temporary file on host
        let temp_path = format!("/tmp/capsule-secrets-{}.env", name);
        std::fs::write(&temp_path, &content)?;

        // Copy to VM using limactl copy
        backend.copy_to_vm(name, &temp_path, "/tmp/secrets.env")?;

        // Move to final location with correct permissions
        backend.exec(
            name,
            &["sudo", "mv", "/tmp/secrets.env", "/etc/capsule/secrets.env"],
        )?;
        backend.exec(name, &["sudo", "chmod", "0644", "/etc/capsule/secrets.env"])?;
        backend.exec(
            name,
            &["sudo", "chown", "root:root", "/etc/capsule/secrets.env"],
        )?;

        // Cleanup
        let _ = std::fs::remove_file(&temp_path);

        println!("✅ Secrets updated! New shells will have updated environment.");
        println!("   (Existing shells need to be restarted)");
        return Ok(());
    }

    println!("Usage: capsule-vm secrets <name> --file .env");
    println!("       capsule-vm secrets <name> --list");
    Ok(())
}

fn parse_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read .env file: {}", path.display()))?;

    let mut secrets = HashMap::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Validate key (alphanumeric + underscore)
            if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                bail!("Invalid variable name '{}' at line {}", key, line_num + 1);
            }

            secrets.insert(key.to_string(), value.to_string());
        } else {
            bail!("Invalid .env format at line {}: missing '='", line_num + 1);
        }
    }

    Ok(secrets)
}
