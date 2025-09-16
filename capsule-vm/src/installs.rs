use crate::run_with_progress;
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Parse a comma-separated `--tools` string into a normalized, deduplicated list,
/// expanding any bundle aliases. Returns tool names in dependency-safe order later
/// via `build_install_script` topological sort.
pub(crate) fn parse_tools(csv: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in csv.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        for name in expand_alias(raw) {
            let k = name.to_lowercase();
            if seen.insert(k.clone()) {
                out.push(k);
            }
        }
    }
    out
}

fn expand_alias(name: &str) -> Vec<String> {
    match name.to_lowercase().as_str() {
        // Bundles
        "dev-min" => vec!["git", "python", "pip"].into_iter().map(|s| s.to_string()).collect(),
        "web" => vec!["node", "npm", "bun"].into_iter().map(|s| s.to_string()).collect(),
        // Identity or known names
        other => vec![other.to_string()],
    }
}

#[derive(Clone)]
struct ToolDef {
    name: &'static str,
    // one or more shell lines to execute (as root unless wrapped with run_as_user)
    lines: &'static [&'static str],
    // tool names that must be installed first
    deps: &'static [&'static str],
    // whether apt-get update is required (for apt-based installs)
    needs_apt: bool,
}

fn registry() -> HashMap<&'static str, ToolDef> {
    // Common lines helpers
    // We'll add a run_as_user() function in the script; here we just call it.
    let mut m = HashMap::new();
    m.insert(
        "git",
        ToolDef {
            name: "git",
            lines: &["apt-get install -y git"],
            deps: &[],
            needs_apt: true,
        },
    );
    m.insert(
        "python",
        ToolDef {
            name: "python",
            lines: &["apt-get install -y python3 python3-pip python3-venv"],
            deps: &[],
            needs_apt: true,
        },
    );
    m.insert(
        "pip",
        ToolDef {
            name: "pip",
            lines: &[
                "apt-get install -y python3-pip",
                // ensure `pip` resolves to pip3 for the ubuntu user envs
                "if ! command -v pip >/dev/null 2>&1 && command -v pip3 >/dev/null 2>&1; then ln -sf /usr/bin/pip3 /usr/local/bin/pip || true; fi",
            ],
            deps: &["python"],
            needs_apt: true,
        },
    );
    m.insert(
        "build",
        ToolDef {
            name: "build",
            lines: &["apt-get install -y build-essential pkg-config libssl-dev cmake"],
            deps: &[],
            needs_apt: true,
        },
    );
    m.insert(
        "node",
        ToolDef {
            name: "node",
            lines: &["apt-get install -y nodejs npm"],
            deps: &[],
            needs_apt: true,
        },
    );
    m.insert(
        "npm",
        ToolDef {
            name: "npm",
            lines: &["apt-get install -y npm"],
            deps: &["node"],
            needs_apt: true,
        },
    );
    m.insert(
        "bun",
        ToolDef {
            name: "bun",
            lines: &[
                // install bun for ubuntu user if not present
                r#"run_as_user 'if [ ! -x "$HOME/.bun/bin/bun" ]; then curl -fsSL https://bun.sh/install | bash; fi'"#,
            ],
            deps: &[],
            needs_apt: false,
        },
    );
    m.insert("rust", ToolDef { name: "rust", lines: &[
        r#"run_as_user 'if ! command -v rustup >/dev/null 2>&1; then curl -fsSL https://sh.rustup.rs | sh -s -- -y; fi'"#,
        r#"run_as_user 'source "$HOME/.cargo/env" >/dev/null 2>&1 || true; rustup toolchain install stable -y || true'"#,
    ], deps: &[], needs_apt: false });
    m.insert("rustup", ToolDef { name: "rustup", lines: &[
        r#"run_as_user 'if ! command -v rustup >/dev/null 2>&1; then curl -fsSL https://sh.rustup.rs | sh -s -- -y; fi'"#
    ], deps: &[], needs_apt: false });
    m.insert("cargo", ToolDef { name: "cargo", lines: &[
        r#"run_as_user 'if ! command -v rustup >/dev/null 2>&1; then curl -fsSL https://sh.rustup.rs | sh -s -- -y; fi'"#,
        r#"run_as_user 'source "$HOME/.cargo/env" >/dev/null 2>&1 || true; rustup toolchain install stable -y || true'"#,
    ], deps: &[], needs_apt: false });
    // Placeholders for future CLIs; wire deps and keep script safe
    m.insert(
        "codex",
        ToolDef {
            name: "codex",
            lines: &[
                // Assuming pip-installable CLI; placeholder
                r#"run_as_user 'if command -v pip3 >/dev/null 2>&1; then python3 -m pip install --user codex-cli || true; else echo "pip not available; skipping codex"; fi'"#,
            ],
            deps: &["pip"],
            needs_apt: false,
        },
    );
    m.insert("claude-code", ToolDef { name: "claude-code", lines: &[
        r#"echo 'claude-code installer not implemented; see docs'"#,
    ], deps: &[], needs_apt: false });
    m.insert("vim", ToolDef { name: "vim", lines: &["apt-get install -y vim"], deps: &[], needs_apt: true });
    m.insert("strace", ToolDef { name: "strace", lines: &["apt-get install -y strace"], deps: &[], needs_apt: true });
    m.insert("ptrace", ToolDef { name: "ptrace", lines: &[
        r#"echo 'ptrace is a kernel facility; no install needed. Use strace/ltrace.'"#
    ], deps: &[], needs_apt: false });
    m
}

/// Resolve dependencies and build a single bash script with idempotent installs.
pub(crate) fn build_install_script(requested: &[String]) -> Result<String> {
    let reg = registry();
    // Collect closure of deps
    let mut want: HashSet<&'static str> = HashSet::new();
    let mut q = VecDeque::new();
    for r in requested {
        q.push_back(r.to_lowercase());
    }
    while let Some(name) = q.pop_front() {
        if let Some(def) = reg.get(name.as_str()) {
            if want.insert(def.name) {
                for d in def.deps {
                    q.push_back(d.to_string());
                }
            }
        } else {
            // Unknown tool: ignore here; we'll echo at run time if requested directly
            // We do not add it to `want` because we have no installer or deps.
        }
    }

    // Topological order: simple Kahn over small set
    let mut indeg: HashMap<&'static str, usize> = HashMap::new();
    let mut adj: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    for t in &want {
        if let Some(def) = reg.get(*t) {
            indeg.entry(def.name).or_insert(0);
            for &d in def.deps {
                if want.contains(&d) {
                    *indeg.entry(def.name).or_insert(0) += 1;
                    adj.entry(d).or_default().push(def.name);
                }
            }
        } else {
            indeg.entry(t).or_insert(0);
        }
    }
    let mut order: Vec<&'static str> = Vec::new();
    let mut z: Vec<&'static str> = indeg
        .iter()
        .filter(|&(_, v)| *v == 0)
        .map(|(k, _)| *k)
        .collect();
    while let Some(n) = z.pop() {
        order.push(n);
        if let Some(children) = adj.get(n) {
            for &m2 in children {
                if let Some(v) = indeg.get_mut(m2) {
                    *v -= 1;
                    if *v == 0 {
                        z.push(m2);
                    }
                }
            }
        }
    }

    // Determine if apt-get update is needed
    let mut needs_apt_update = false;
    for t in &order {
        if let Some(def) = reg.get(*t) {
            if def.needs_apt {
                needs_apt_update = true;
                break;
            }
        }
    }

    // Build script
    let mut script = String::new();
    script.push_str("#!/usr/bin/env bash\n");
    script.push_str("set -euo pipefail\n");
    script.push_str("export DEBIAN_FRONTEND=noninteractive\n");
    script.push_str("mkdir -p /var/lib/capsule-vm/tools\n");
    script.push_str(
        r#"run_as_user() { su -l -s /bin/bash ubuntu -c "$*"; }"#,
    );
    script.push('\n');
    if needs_apt_update {
        script.push_str("apt-get update -y\n");
    }
    for t in order {
        let marker = format!("/var/lib/capsule-vm/tools/{}.installed", t);
        script.push_str(&format!(
            "\nif [ -f '{marker}' ]; then\n  echo 'Skipping {t} (already installed)';\nelse\n  echo 'Installing {t}...';\n"
        ));
        if let Some(def) = reg.get(t) {
            for &line in def.lines {
                script.push_str(line);
                script.push('\n');
            }
        } else {
            script.push_str(&format!("echo 'No installer for {t}; skipping'\n"));
        }
        script.push_str(&format!("  touch '{marker}'\n  echo 'Installed {t}'\nfi\n"));
    }
    // For any unknown requested tools, emit a single notice line during run
    let unknowns: Vec<String> = requested
        .iter()
        .filter(|t| !reg.contains_key(t.as_str()))
        .cloned()
        .collect();
    if !unknowns.is_empty() {
        script.push_str("\n");
        for u in unknowns {
            script.push_str(&format!("echo 'No installer mapping for `{}`; skipping'\n", u));
        }
    }
    Ok(script)
}

/// Transfer and execute the built script inside the VM using multipass
pub(crate) fn install_tools(vm_name: &str, tools_csv: &str) -> Result<()> {
    let tools = parse_tools(tools_csv);
    if tools.is_empty() {
        return Ok(());
    }

    let script = build_install_script(&tools)?;
    // Write to a temp file on host
    let mut host_path = env::temp_dir();
    host_path.push(format!("capsule-vm-setup-{}.sh", vm_name));
    fs::write(&host_path, script).with_context(|| format!("writing {}", host_path.display()))?;

    // Transfer to VM
    let mut transfer = Command::new("multipass");
    transfer.args([
        "transfer",
        host_path.to_str().unwrap(),
        &format!("{}:/home/ubuntu/capsule-vm-setup.sh", vm_name),
    ]);
    run_with_progress(transfer, &format!("Uploading installer to `{}`", vm_name))?;

    // Execute in VM
    let mut exec = Command::new("multipass");
    exec.args([
        "exec",
        vm_name,
        "--",
        "bash",
        "-lc",
        "sudo bash -lc 'chmod +x /home/ubuntu/capsule-vm-setup.sh && /home/ubuntu/capsule-vm-setup.sh'",
    ]);
    run_with_progress(
        exec,
        &format!("Installing tools on `{}`: {}", vm_name, tools.join(",")),
    )?;

    Ok(())
}

pub(crate) fn supported_tools() -> Vec<String> {
    let mut v: Vec<String> = registry().keys().cloned().map(|s| s.to_string()).collect();
    v.sort();
    v
}
