use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use directories::UserDirs;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// Embedded default cloud-init template
const DEFAULT_TEMPLATE: &str = include_str!("../cloud-config.yaml");

#[derive(Parser)]
#[command(
    name = "ds",
    version,
    about = "Dimension Shifter: tiny VM orchestrator for secure agent sandboxes"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a sandbox VM and copy project PATH into the VM workspace
    Create {
        /// Name of the sandbox (VM)
        name: String,
        /// Host path to copy (read-only mounted, then synced) into the VM workspace
        path: String,
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
    /// Uninstall ds: remove configs and installed binaries (best effort)
    Uninstall,
}

fn main() -> Result<()> {
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
        } => cmd_create(
            &name,
            &path,
            cpus,
            &memory,
            &disk,
            &tools,
            template.as_deref(),
        )?,
        Cmd::Ps => cmd_ps()?,
        Cmd::Start { name } => cmd_start(&name)?,
        Cmd::Stop { name } => cmd_stop(&name)?,
        Cmd::Delete { name } => cmd_delete(&name)?,
        Cmd::Shell { name } => cmd_shell(&name)?,
        Cmd::Clean => cmd_clean()?,
        Cmd::Uninstall => cmd_uninstall()?,
    }
    Ok(())
}

/* ========================= Commands ========================= */

fn cmd_create(
    name: &str,
    path: &str,
    cpus: u8,
    memory: &str,
    disk: &str,
    tools: &str,
    template_override: Option<&Path>,
) -> Result<()> {
    // 1) Optional cloud-init: only when a template is provided; default is a plain Multipass VM
    let ci_path = if let Some(tpl) = template_override {
        let template_path = PathBuf::from(tpl);
        let ci = render_cloud_init_from_file(&template_path, name, tools)?;
        Some(write_temp("ds-cloud-init.yaml", &ci)?)
    } else {
        None
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

    // 3) mount host path read-only → /src
    let abs = canonicalize(path)?;
    run_with_progress(
        {
            let mut c = Command::new("multipass");
            c.args(["mount", "-o", "ro"]);
            c.arg(abs.as_os_str());
            c.arg(format!("{name}:/src"));
            c
        },
        "Mounting project (read-only) at /src",
    )?;

    // 4) copy into workspace, set ownership (supports either agent or ubuntu user)
    run_with_progress(
        {
            let mut c = Command::new("multipass");
            c.arg("exec").arg(name).args([
                "--", "bash", "-lc",
                "set -e\nif id agent >/dev/null 2>&1; then\n  sudo mkdir -p /home/agent/work\n  sudo rsync -a /src/ /home/agent/work/\n  sudo chown -R agent:agent /home/agent/work\nelse\n  sudo mkdir -p /home/ubuntu/work\n  sudo rsync -a /src/ /home/ubuntu/work/\n  sudo chown -R ubuntu:ubuntu /home/ubuntu/work\nfi",
            ]);
            c
        },
        "Syncing project into workspace",
    )?;

    // 5) record minimal metadata
    save_metadata(name, &abs)?;

    println!("✅ created sandbox `{name}`");
    println!("   • Mounted RO: {}", abs.display());
    println!("   • Workspace: ~/work\n");

    // 6) drop user into shell (transient UX)
    shell_into(name)
}

fn cmd_ps() -> Result<()> {
    run(&mut Command::new("multipass").arg("list"))
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
    // remove per-user ds directory
    let p = ds_dir()?;
    if p.exists() {
        fs::remove_dir_all(&p).with_context(|| format!("removing {}", p.display()))?;
        println!("Removed {}", p.display());
    } else {
        println!("No per-user cache at {}", p.display());
    }

    // remove temp cloud-init file if present
    let mut tmp = env::temp_dir();
    tmp.push("ds-cloud-init.yaml");
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
    tmp.push("ds-cloud-init.yaml");
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
        PathBuf::from("/usr/local/bin/ds"),
        PathBuf::from("/usr/local/bin/dm"),
        home.join(".local/bin/ds"),
        home.join(".local/bin/dm"),
        home.join(".cargo/bin/ds"),
        home.join(".cargo/bin/dm"),
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

    // 4) Also remove local ./dimensionshifter if present (sometimes created manually)
    let local = PathBuf::from("./dimensionshifter");
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
    // show quick status (ignore errors)
    let _ = Command::new("multipass").args(["info", name]).status();

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

/// Run a command with a spinner; also stream stdout/stderr so native progress bars are visible.
fn run_with_progress(mut cmd: Command, label: &str) -> Result<()> {
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
    Ok(home.join(".dimensionshifter"))
}

fn ensure_workspace() -> Result<()> {
    let p = ds_dir()?;
    if !p.exists() {
        fs::create_dir_all(&p)?;
    }
    Ok(())
}

fn default_template_path() -> PathBuf {
    ds_dir().expect("ds dir").join("cloud-init.tmpl.yaml")
}

fn ensure_default_template_exists(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let parent = path.parent().ok_or_else(|| anyhow!("bad template path"))?;
    if !parent.exists() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, DEFAULT_TEMPLATE.trim_start())?;
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

/* ========================= Cloud-init (file-based) ========================= */

/// Reads a cloud-init template and fills placeholders:
///   {{VM_NAME}}, {{EXTRA_PACKAGES}}, {{RUNCMD_EXTRA}}
fn render_cloud_init_from_file(template_path: &Path, vm_name: &str, tools: &str) -> Result<String> {
    let mut pkgs = vec![
        "nftables",
        "bubblewrap",
        "ca-certificates",
        "bpftool",
        "libseccomp-dev",
        "git",
    ];
    let t: Vec<&str> = tools
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if t.contains(&"python") {
        pkgs.push("python3");
        pkgs.push("python3-venv");
    }
    if t.contains(&"build") {
        pkgs.push("build-essential");
        pkgs.push("clang");
        pkgs.push("llvm");
    }

    let mut runcmd_extra = String::new();
    if t.contains(&"rust") {
        runcmd_extra.push_str(
            "  - bash -lc \"curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y\"\n\
             - bash -lc \"echo 'source $HOME/.cargo/env' >> /home/agent/.bashrc && chown agent:agent /home/agent/.bashrc\"\n"
        );
    }

    let tmpl = fs::read_to_string(template_path)
        .with_context(|| format!("reading template {}", template_path.display()))?;
    let rendered = tmpl
        .replace("{{VM_NAME}}", vm_name)
        .replace(
            "{{EXTRA_PACKAGES}}",
            &pkgs
                .iter()
                .map(|s| format!("\"{s}\""))
                .collect::<Vec<_>>()
                .join(", "),
        )
        .replace("{{RUNCMD_EXTRA}}", &runcmd_extra);
    Ok(rendered)
}

/* ========================= FS util ========================= */

fn write_temp(name: &str, content: &str) -> Result<PathBuf> {
    let mut p = env::temp_dir();
    p.push(name);
    fs::write(&p, content)?;
    Ok(p)
}

/* ========================= Embedded default template ========================= */
// Note: default_template_path is defined above to point to the per-user location
