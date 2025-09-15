use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, CommandFactory};
use std::io::Write;
use directories::UserDirs;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// No embedded cloud-init; use on-disk YAML (./cloud-init.yaml or --template)

mod installs;

const ASCII_LOGO: &str = include_str!("ascii_logo.txt");

fn red_banner() -> String {
    format!("[31m{}[0m", ASCII_LOGO)
}

#[derive(Parser)]
#[command(
    name = "capsule-vm",
    version,
    about = "Capsule VM: tiny VM orchestrator for secure agent sandboxes"
)]
struct Cli {
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
        /// Tools to install inside VM: comma-separated (python,rust,git,build)
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
            if t.starts_with("Commands:") { in_commands = true; }
            else if t.starts_with("Options:") { in_commands = false; }
            if in_commands {
                if t.starts_with("  ") && t.len() > 2 {
                    let rest = &t[2..];
                    if !rest.is_empty() && !rest.chars().next().unwrap().is_whitespace() {
                        let mut it = rest.splitn(2, char::is_whitespace);
                        let name = it.next().unwrap_or("");
                        let rem = it.next().unwrap_or("");
                        out.push_str("  [31m");
                        out.push_str(name);
                        out.push_str("[0m");
                        out.push_str(rem);
                        out.push('\n');
                        continue;
                    }
                }
            }
            out.push_str(t);
            out.push('\n');
        }
        print!("{}", out);
        std::io::stdout().flush().ok();
        return Ok(());}


    let cli = Cli::parse();
    ensure_workspace()?;
    ensure_multipass()?; // hard dependency

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
            let path_ref = path.as_deref();
            cmd_create(
                &name,
                path_ref,
                cpus,
                &memory,
                &disk,
                &tools,
                template.as_deref(),
            )?
        }
        Cmd::Ps => cmd_ps()?,
        Cmd::Start { name } => cmd_start(&name)?,
        Cmd::Stop { name } => cmd_stop(&name)?,
        Cmd::Delete { name } => cmd_delete(&name)?,
        Cmd::Shell { name } => cmd_shell(&name)?,
        Cmd::Clean => cmd_clean()?,
        Cmd::Uninstall => cmd_uninstall()?,
        Cmd::Tools { cmd } => match cmd {
            ToolsCmd::Install { name, tools } => cmd_tools_install(&name, &tools)?,
        },
    }
    Ok(())
}

/* ========================= Commands ========================= */

fn cmd_create(
    name: &str,
    path: Option<&str>,
    cpus: u8,
    memory: &str,
    disk: &str,
    tools: &str,
    template_override: Option<&Path>,
) -> Result<()> {
    // 1) Cloud-init: use provided template if any, otherwise ./cloud-init.yaml if present
    let ci_path: Option<PathBuf> = if let Some(tpl) = template_override {
        Some(PathBuf::from(tpl))
    } else {
        let p = PathBuf::from("./cloud-init.yaml");
        if p.exists() {
            Some(p)
        } else {
            None
        }
    };

    // 2) launch VM (progress)
    run_with_progress(
        {
            let mut c = Command::new("multipass");
            c.args([
                "launch",
                "24.04",
                "--name",
                name,
                "--cpus",
                &cpus.to_string(),
                "--memory",
                memory,
                "--disk",
                disk,
            ]);
            if let Some(p) = ci_path.as_ref() {
                c.args(["--cloud-init", p.to_str().unwrap()]);
            }
            c
        },
        &format!("Creating VM `{name}`"),
    )?;

    // 3) Record minimal metadata
    if let Some(p) = path {
        let abs = canonicalize(p)?;
        save_metadata(name, &abs)?;
    } else {
        save_metadata(name, Path::new("(none)"))?;
    }

    // 4) Wait until VM is ready (cloud-init complete), then install requested tools
    wait_for_vm_ready(name)?;
    installs::install_tools(name, tools)?;
    // 5) Setup workspace: live-mount host path if provided, else create empty workspace dir
    if let Some(p) = path {
        setup_workspace(name, p)?;
    } else {
        create_workspace_dir(name)?;
    }

    // 6) Print next steps
    println!("✅ Created VM `{name}` (Ubuntu 24.04)");
    println!("Next steps:");
    println!("  • Enter the VM:  capsule-vm shell {name}");
    println!("  • Workspace:     live at ~/workspace");
    println!("  • List VMs:      multipass list");
    println!("  • Delete VM:     multipass delete {name} && multipass purge");
    Ok(())
}

fn cmd_ps() -> Result<()> {
    run_passthrough(&mut Command::new("multipass").arg("list"))
}

fn cmd_start(name: &str) -> Result<()> {
    run_with_progress(
        {
            let mut c = Command::new("multipass");
            c.args(["start", name]);
            c
        },
        &format!("Starting `{name}`"),
    )?;
    // auto-shell for transient feel
    shell_into(name)
}

fn cmd_stop(name: &str) -> Result<()> {
    run_with_progress(
        {
            let mut c = Command::new("multipass");
            c.args(["stop", name]);
            c
        },
        &format!("Stopping `{name}`"),
    )
}

fn cmd_delete(name: &str) -> Result<()> {
    // best-effort umount/stop
    let _ = Command::new("multipass").args(["umount", name]).status();
    let _ = Command::new("multipass").args(["stop", name]).status();

    run_with_progress(
        {
            let mut c = Command::new("multipass");
            c.args(["delete", name]);
            c
        },
        &format!("Deleting `{name}`"),
    )?;
    run_with_progress(
        {
            let mut c = Command::new("multipass");
            c.arg("purge");
            c
        },
        "Purging deleted images",
    )?;
    remove_metadata(name)?;
    println!("🗑️  deleted `{name}` and purged images.");
    Ok(())
}

fn cmd_shell(name: &str) -> Result<()> {
    shell_into(name)
}

// (no quick helper; simplified default create flow)

fn cmd_clean() -> Result<()> {
    // remove per-user capsule-vm directory
    let p = ds_dir()?;
    if p.exists() {
        fs::remove_dir_all(&p).with_context(|| format!("removing {}", p.display()))?;
        println!("Removed {}", p.display());
    } else {
        println!("No per-user cache at {}", p.display());
    }

    // remove temp cloud-init file if present
    let mut tmp = env::temp_dir();
    tmp.push("capsule-vm-cloud-init.yaml");
    if tmp.exists() {
        fs::remove_file(&tmp).with_context(|| format!("removing {}", tmp.display()))?;
        println!("Removed {}", tmp.display());
    }

    println!("✅ Cleaned cached templates and temp files.");
    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    use std::ffi::OsString;

    // 1) Remove per-user config/cache
    let p = ds_dir()?;
    if p.exists() {
        fs::remove_dir_all(&p).with_context(|| format!("removing {}", p.display()))?;
        println!("Removed {}", p.display());
    } else {
        println!("No per-user cache at {}", p.display());
    }

    // 2) Remove temp cloud-init file if present
    let mut tmp = env::temp_dir();
    tmp.push("capsule-vm-cloud-init.yaml");
    if tmp.exists() {
        fs::remove_file(&tmp).with_context(|| format!("removing {}", tmp.display()))?;
        println!("Removed {}", tmp.display());
    }

    // 3) Remove common install locations (best effort)
    let home = UserDirs::new()
        .ok_or_else(|| anyhow!("cannot locate home directory"))?
        .home_dir()
        .to_path_buf();

    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from("/usr/local/bin/capsule-vm"),
        home.join(".local/bin/capsule-vm"),
        home.join(".cargo/bin/capsule-vm"),
    ];

    if let Ok(curr) = std::env::current_exe() {
        candidates.push(curr);
    }

    let mut removed_any = false;
    for c in candidates {
        if c.exists() {
            match fs::remove_file(&c) {
                Ok(_) => {
                    println!("Removed {}", c.display());
                    removed_any = true;
                }
                Err(e) => {
                    println!("Could not remove {}: {}", c.display(), e);
                }
            }
        }
    }

    // 4) Also remove local ./capsule-vm if present (sometimes created manually)
    let local = PathBuf::from("./capsule-vm");
    if local.exists() {
        fs::remove_dir_all(&local).with_context(|| format!("removing {}", local.display()))?;
        println!("Removed {}", local.display());
        removed_any = true;
    }

    println!("✅ Uninstall complete (best effort). Some system paths may require sudo.");
    if !removed_any {
        println!("Nothing to remove in common locations.");
    }
    Ok(())
}

/* ========================= Helpers ========================= */

fn shell_into(name: &str) -> Result<()> {
    // Prepare branded login (suppress default MOTD, add banner + info)
    let _ = ensure_capsule_info(name);
    let _ = ensure_login_profile(name);

    let mut child = Command::new("multipass")
        .args(["shell", name])
        .spawn()
        .context("failed to spawn shell")?;
    let status = child.wait()?;
    if !status.success() {
        bail!("shell exited with failure");
    }
    Ok(())
}

fn setup_workspace(name: &str, host_path: &str) -> Result<()> {
    let abs = canonicalize(host_path)?;
    // Ensure target dir exists and owned by ubuntu
    run_with_progress({
        let mut c = Command::new("multipass");
        c.args([
            "exec",
            name,
            "--",
            "bash",
            "-lc",
            "sudo mkdir -p /home/ubuntu/workspace && sudo chown ubuntu:ubuntu /home/ubuntu/workspace",
        ]);
        c
    }, &format!("Preparing workspace dir on `{}`", name))?;

    // Live mount host path into the VM
    run_with_progress({
        let mut c = Command::new("multipass");
        c.args([
            "mount",
            abs.to_str().unwrap(),
            &format!("{name}:/home/ubuntu/workspace"),
        ]);
        c
    }, &format!("Mounting workspace from host: {}", abs.display()))?;

    ensure_login_profile(name)?;
    Ok(())
}

fn create_workspace_dir(name: &str) -> Result<()> {
    run_with_progress({
        let mut c = Command::new("multipass");
        c.args([
            "exec",
            name,
            "--",
            "bash",
            "-lc",
            "sudo mkdir -p /home/ubuntu/workspace && sudo chown ubuntu:ubuntu /home/ubuntu/workspace",
        ]);
        c
    }, "Preparing empty workspace dir")?;
    ensure_login_profile(name)?;
    Ok(())
}

fn ensure_login_profile(name: &str) -> Result<()> {
    let banner = red_banner();
    let content = format!(r#"# Capsule login helpers
export PATH="/tools/bin:$PATH"
# Print banner and capsule info on interactive login
if [ -t 1 ] && [ -n "${{PS1:-}}" ]; then
  cat <<'BANNER'
{}
BANNER
  if command -v capsule-info >/dev/null 2>&1; then
    capsule-info
  fi
  # Auto-enter workspace if logging into home
  if [ -d "$HOME/workspace" ] && [ "$PWD" = "$HOME" ]; then
    cd "$HOME/workspace"
  fi
  echo "Manage tools: capsule tools list | install <csv> | remove <names>"
fi
"#, banner);
    let mut host_path = env::temp_dir();
    host_path.push("capsule-profile.sh");
    fs::write(&host_path, content).with_context(|| format!("writing {}", host_path.display()))?;
    run_with_progress({
        let mut c = Command::new("multipass");
        c.args([
            "transfer",
            host_path.to_str().unwrap(),
            &format!("{name}:/tmp/capsule-profile.sh"),
        ]);
        c
    }, &format!("Uploading login profile to `{name}`"))?;
    run_with_progress({
        let mut c = Command::new("multipass");
        c.args([
            "exec",
            name,
            "--",
            "bash",
            "-lc",
            "sudo install -m 0644 /tmp/capsule-profile.sh /etc/profile.d/10-capsule.sh && sudo -u ubuntu touch /home/ubuntu/.hushlogin",
        ]);
        c
    }, &format!("Activating login profile on `{name}`"))?;
    Ok(())
}


fn ensure_capsule_info(name: &str) -> anyhow::Result<()> {
    // Small helper to print a concise VM + tools summary
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
tools_dir="/var/lib/capsule-vm/tools"
workspace="$HOME/workspace"

name=$(hostname)
os=$(grep -E '^PRETTY_NAME=' /etc/os-release | cut -d= -f2- | tr -d '"')
kernel=$(uname -r)
uptime=$(uptime -p || true)
cores=$(nproc)
load=$(awk '{print $1, $2, $3}' /proc/loadavg)
mem_used=$(free -h | awk '/^Mem:/ {print $3}')
mem_total=$(free -h | awk '/^Mem:/ {print $2}')
swap_used=$(free -h | awk '/^Swap:/ {print $3}')
swap_total=$(free -h | awk '/^Swap:/ {print $2}')
root_used=$(df -h / | awk 'NR==2 {print $3}')
root_total=$(df -h / | awk 'NR==2 {print $2}')
ws_used=$(df -h "$workspace" 2>/dev/null | awk 'NR==2 {print $3}')
ws_total=$(df -h "$workspace" 2>/dev/null | awk 'NR==2 {print $2}')
ipv4=$(hostname -I 2>/dev/null | awk '{print $1}')
mount_src=""
src_file="/var/lib/capsule-vm/workspace_source.txt"
if [ -f "$src_file" ]; then
  mount_src=$(cat "$src_file")
else
  ml=$(findmnt -n -o SOURCE "$workspace" 2>/dev/null || true)
  mount_src="$ml"
fi

echo "Capsule VM session info for $name"

echo -e "[31mName:[0m $name"
echo -e "[31mOS:[0m $os"
echo -e "[31mKernel:[0m $kernel"
echo -e "[31mUptime:[0m ${uptime:-n/a}"
echo -e "[31mCPU:[0m $cores cores, load $load"
echo -e "[31mMemory:[0m $mem_used / $mem_total (swap $swap_used / $swap_total)"
if [ -n "${ws_used:-}" ] && [ -n "${ws_total:-}" ]; then
  echo -e "[31mDisk(/):[0m $root_used / $root_total, workspace $ws_used / $ws_total"
else
  echo -e "[31mDisk(/):[0m $root_used / $root_total"
fi
echo -e "[31mIPv4:[0m ${ipv4:-n/a}"
if [ -n "$mount_src" ]; then
  echo -e "[31mMounts:[0m $mount_src"
fi

echo "Tools:"
if ls -1 "$tools_dir"/*.installed >/dev/null 2>&1; then
  for f in "$tools_dir"/*.installed; do b=$(basename "$f"); echo "  - ${b%.installed}"; done | sort
else
  echo "  (none)"
fi
"#;   let mut host_path = std::env::temp_dir();
    host_path.push("capsule-info.sh");
    std::fs::write(&host_path, script)?;
    crate::run_with_progress({
        let mut c = std::process::Command::new("multipass");
        c.args([
            "transfer",
            host_path.to_str().unwrap(),
            &format!("{name}:/tmp/capsule-info.sh"),
        ]);
        c
    }, &format!("Uploading capsule-info to `{name}`"))?;
    crate::run_with_progress({
        let mut c = std::process::Command::new("multipass");
        c.args([
            "exec",
            name,
            "--",
            "bash",
            "-lc",
            "sudo install -m 0755 /tmp/capsule-info.sh /usr/local/bin/capsule-info",
        ]);
        c
    }, &format!("Installing capsule-info on `{name}`"))?;
    Ok(())
}

fn set_shell_banner(name: &str) -> Result<()> {
    // Write logo to a temp file on host, transfer to VM, then place as /etc/motd
    let mut host_path = env::temp_dir();
    host_path.push("capsule-vm-banner.txt");
    fs::write(&host_path, ASCII_LOGO)
        .with_context(|| format!("writing {}", host_path.display()))?;

    let mut transfer = Command::new("multipass");
    transfer.args([
        "transfer",
        host_path.to_str().unwrap(),
        &format!("{}:/tmp/capsule-vm-banner.txt", name),
    ]);
    run_with_progress(transfer, &format!("Uploading banner to `{}`", name))?;

    let mut exec = Command::new("multipass");
    exec.args([
        "exec",
        name,
        "--",
        "bash",
        "-lc",
        "sudo cp /tmp/capsule-vm-banner.txt /etc/motd || true",
    ]);
    run_with_progress(exec, &format!("Refreshing login banner on `{}`", name))?;
    Ok(())
}

fn cmd_tools_install(name: &str, tools: &str) -> Result<()> {
    // Best-effort start in case VM is stopped
    let _ = run_with_progress(
        {
            let mut c = Command::new("multipass");
            c.args(["start", name]);
            c
        },
        &format!("Ensuring `{name}` is running"),
    );

    wait_for_vm_ready(name)?;
    installs::install_tools(name, tools)?;
    println!("✅ Installed tools on `{name}`: {tools}");
    Ok(())
}

fn run(cmd: &mut Command) -> Result<()> {
    let out = cmd
        .output()
        .with_context(|| format!("failed to run: {:?}", cmd))?;
    if !out.status.success() {
        bail!(
            "command failed\nSTDOUT:\n{}\nSTDERR:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Run a command and print its stdout/stderr directly (inherit console).
fn run_passthrough(cmd: &mut Command) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to run: {:?}", cmd))?;
    if !status.success() {
        bail!("command failed with status {status}");
    }
    Ok(())
}

/// Run a command with a spinner; also stream stdout/stderr so native progress bars are visible.
pub(crate) fn run_with_progress(mut cmd: Command, label: &str) -> Result<()> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawn failed: {label}"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner} {msg}")?.tick_chars("/|\\- "));
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message(label.to_string());

    let t_out = std::thread::spawn({
        let pb = pb.clone();
        move || {
            for line in BufReader::new(stdout).lines().flatten() {
                pb.println(line);
            }
        }
    });
    let t_err = std::thread::spawn({
        let pb = pb.clone();
        move || {
            for line in BufReader::new(stderr).lines().flatten() {
                pb.println(line);
            }
        }
    });

    let status = child.wait()?;
    t_out.join().ok();
    t_err.join().ok();

    if status.success() {
        pb.finish_with_message(format!("{label}: done"));
        Ok(())
    } else {
        pb.abandon_with_message(format!("{label}: failed"));
        bail!("{label} failed with status {status}");
    }
}

fn canonicalize(p: &str) -> Result<PathBuf> {
    let path = PathBuf::from(p);
    let abs = if path.is_absolute() {
        path
    } else {
        env::current_dir()?.join(path)
    };
    Ok(abs.canonicalize()?)
}

/* ========================= Workspace / Metadata ========================= */

fn ds_dir() -> Result<PathBuf> {
    let home = UserDirs::new()
        .ok_or_else(|| anyhow!("cannot locate home directory"))?
        .home_dir()
        .to_path_buf();
    Ok(home.join(".capsule-vm"))
}

fn ensure_workspace() -> Result<()> {
    let p = ds_dir()?;
    if !p.exists() {
        fs::create_dir_all(&p)?;
    }
    Ok(())
}

fn save_metadata(name: &str, src: &Path) -> Result<()> {
    let meta_path = ds_dir()?.join(format!("{}.meta", name));
    let content = format!("name={}\nsource={}\n", name, src.display());
    fs::write(meta_path, content)?;
    Ok(())
}

fn remove_metadata(name: &str) -> Result<()> {
    let meta_path = ds_dir()?.join(format!("{}.meta", name));
    let _ = fs::remove_file(meta_path);
    Ok(())
}

/* ========================= Prereq: Multipass ========================= */

#[derive(Debug)]
enum HostOs {
    Mac,
    Linux,
    Windows,
    Unknown,
}

fn detect_os() -> HostOs {
    match std::env::consts::OS {
        "macos" => HostOs::Mac,
        "linux" => HostOs::Linux,
        "windows" => HostOs::Windows,
        _ => HostOs::Unknown,
    }
}

fn ensure_multipass() -> Result<()> {
    if which::which("multipass").is_ok() {
        return Ok(());
    }

    eprintln!("❌ multipass not found on PATH.");
    match detect_os() {
        HostOs::Mac => eprintln!("   Install with: brew install --cask multipass"),
        HostOs::Linux => {
            eprintln!("   Install with: sudo snap install multipass   (or see your distro)")
        }
        HostOs::Windows => eprintln!("   Install with: winget install -e --id Canonical.Multipass"),
        HostOs::Unknown => eprintln!("   Install from https://multipass.run"),
    }
    Err(anyhow!("missing dependency: multipass"))
}

/* ========================= Cloud-init ========================= */
// Dynamic rendering removed. Use an on-disk YAML file via --template or ./cloud-init.yaml.

/* ========================= VM Readiness ========================= */

pub(crate) fn wait_for_vm_ready(name: &str) -> Result<()> {
    // Prefer cloud-init readiness inside the VM
    let mut c = Command::new("multipass");
    c.args([
        "exec",
        name,
        "--",
        "bash",
        "-lc",
        "cloud-init status --wait || true",
    ]);
    run_with_progress(c, &format!("Waiting for `{name}` to finish cloud-init"))?;

    // Also quickly confirm system is running
    let mut c2 = Command::new("multipass");
    c2.args([
        "exec",
        name,
        "--",
        "bash",
        "-lc",
        "systemctl is-system-running --wait || true",
    ]);
    run_with_progress(c2, &format!("Verifying `{name}` system readiness"))
}
