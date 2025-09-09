use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use directories::UserDirs;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use which;

struct Cli {
    cmd: Cmd,
}

#[derive(subcommand)]
enum Cmd {
    /// create a sandbox VM and copy project PATH into the VM workspace
    Create {
        /// name of the sandbox (VM)
        name: String,
        /// host path to copy (read-only mounted, then synced) into the VM workspace
        path: String,
        /// vCPUs (1, 2)
        #[arg(long, default_value = 2)]
        cpus: u8,
        /// memory (1G, 2048M)
        #[arg(long, default_value = "1G")]
        mem: String,
        /// disk size (8G)
        #[arg(long, default_value = "8G")]
        disk: String,
        /// tools to install inside VM: comma-separated (python, rust, git, build)
        tools: String,
    },
    /// list VM's
    Ps,
    /// start sandbox and open a shell
    Start { name: String },
    /// stop sandbox
    Stop { name: String },
    /// delete sandbox (and purge deleted images)
    Delete { name: String },
    /// open a shell into the sandbox
    Shell { name: String },
}

fn main() -> Result<()> {
    todo!("implement me");
    Ok(())
}

/* =============================== Prereq: Multipass =================================== */

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
    eprintln!("multipass not found on PATH.");
    match detect_os() {
        HostOs::Mac => eprintln!("   Install with: brew install --cask multipass"),
        HostOs::Linux => {
            eprintln!("   Install with: sudo snap install multipass   (or see your distro)")
        }
        HostOs::Windows => eprintln!("   Install with: winget install -e --id Canonical.Multipass"),
        HostOs::Unknown => eprintln!("   Install from https://multipass.run"),
    }
    Err(anyhow!("mussing dependency: multipass"))
}
