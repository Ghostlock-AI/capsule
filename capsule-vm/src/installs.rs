use crate::vm_backend::VmBackend;
use anyhow::{Context, Result, bail};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};
use std::env;
use std::fs;

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
        "dev-min" => vec!["git", "python", "pip"]
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
        "web" => vec!["node", "npm", "bun"]
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
        // Identity or known names
        other => vec![other.to_string()],
    }
}

#[derive(Clone)]
struct ToolDef {
    // one or more shell lines to execute (as root unless wrapped with run_as_user)
    lines: &'static [&'static str],
    // tool names that must be installed first
    deps: &'static [&'static str],
    // whether apt-get update is required (for apt-based installs)
    needs_apt: bool,
}

fn registry() -> HashMap<&'static str, ToolDef> {
    let mut m = HashMap::new();
    m.insert(
        "git",
        ToolDef {
            lines: &["apt-get install -y git"],
            deps: &[],
            needs_apt: true,
        },
    );
    m.insert(
        "python",
        ToolDef {
            lines: &["apt-get install -y python3 python3-pip python3-venv"],
            deps: &[],
            needs_apt: true,
        },
    );
    m.insert(
        "pip",
        ToolDef {
            lines: &[
                "apt-get install -y python3-pip",
                "if ! command -v pip >/dev/null 2>&1 && command -v pip3 >/dev/null 2>&1; then ln -sf /usr/bin/pip3 /usr/local/bin/pip || true; fi",
            ],
            deps: &["python"],
            needs_apt: true,
        },
    );
    m.insert(
        "build",
        ToolDef {
            lines: &["apt-get install -y build-essential pkg-config libssl-dev cmake"],
            deps: &[],
            needs_apt: true,
        },
    );
    m.insert(
        "node",
        ToolDef {
            lines: &["apt-get install -y nodejs npm"],
            deps: &[],
            needs_apt: true,
        },
    );
    m.insert(
        "npm",
        ToolDef {
            lines: &["apt-get install -y npm"],
            deps: &["node"],
            needs_apt: true,
        },
    );
    m.insert(
        "rust",
        ToolDef {
            lines: &[r#"run_as_user 'curl --proto '\''=https'\'' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'"#],
            deps: &[],
            needs_apt: false,
        },
    );
    m.insert(
        "cargo",
        ToolDef {
            lines: &[],
            deps: &["rust"],
            needs_apt: false,
        },
    );
    m.insert(
        "bun",
        ToolDef {
            lines: &[r#"run_as_user 'curl -fsSL https://bun.sh/install | bash'"#],
            deps: &[],
            needs_apt: false,
        },
    );
    m.insert(
        "codex",
        ToolDef {
            lines: &[r#"run_as_user 'pip install anthropic-codex'"#],
            deps: &["pip"],
            needs_apt: false,
        },
    );
    m
}

/// Resolve dependencies and return tools in install order using Kahn's algorithm
fn resolve_install_order(requested: &[String]) -> Result<Vec<String>> {
    let reg = registry();
    let mut order = Vec::new();
    let mut to_install = BTreeSet::new();

    // Build set of all tools we need to install (including dependencies)
    let mut queue = VecDeque::new();
    for tool in requested {
        queue.push_back(tool.clone());
    }

    while let Some(tool) = queue.pop_front() {
        if to_install.contains(&tool) {
            continue;
        }
        to_install.insert(tool.clone());

        if let Some(def) = reg.get(tool.as_str()) {
            for dep in def.deps {
                queue.push_back(dep.to_string());
            }
        }
    }

    // Kahn's topological sort
    let mut in_degree: BTreeMap<String, usize> = BTreeMap::new();
    let mut adj_list: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for tool in &to_install {
        in_degree.entry(tool.clone()).or_insert(0);
        if let Some(def) = reg.get(tool.as_str()) {
            for dep in def.deps {
                adj_list
                    .entry(dep.to_string())
                    .or_default()
                    .push(tool.clone());
                *in_degree.entry(tool.clone()).or_insert(0) += 1;
            }
        }
    }

    let mut ready = BinaryHeap::new();
    for (tool, &deg) in &in_degree {
        if deg == 0 {
            ready.push(Reverse(tool.clone()));
        }
    }

    while let Some(Reverse(tool)) = ready.pop() {
        order.push(tool.clone());
        if let Some(dependents) = adj_list.get(&tool) {
            for dep in dependents {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        ready.push(Reverse(dep.clone()));
                    }
                }
            }
        }
    }

    if order.len() != to_install.len() {
        bail!("Circular dependency detected in tools");
    }

    Ok(order)
}

fn build_install_script(tools: &[String]) -> Result<String> {
    let reg = registry();
    let ordered = resolve_install_order(tools)?;

    let mut script = String::new();
    script.push_str("#!/usr/bin/env bash\n");
    script.push_str("set -e\n\n");

    // Helper function to run commands as ubuntu user
    script.push_str("run_as_user() {\n");
    script.push_str("  sudo -u ubuntu -i bash -c \"$1\"\n");
    script.push_str("}\n\n");

    // Marker directory
    script.push_str("MARKER_DIR='/var/lib/capsule-vm/tools'\n");
    script.push_str("mkdir -p \"$MARKER_DIR\"\n\n");

    // Collect all apt packages for batch installation
    let mut apt_packages = Vec::new();
    for tool in &ordered {
        if let Some(def) = reg.get(tool.as_str())
            && def.needs_apt
        {
            // Extract apt packages from install lines
            for line in def.lines {
                if line.contains("apt-get install") {
                    // Simple parser: extract package names after "apt-get install -y"
                    if let Some(pkgs_part) = line.split("apt-get install -y").nth(1) {
                        for pkg in pkgs_part.split_whitespace() {
                            if !apt_packages.contains(&pkg.to_string()) {
                                apt_packages.push(pkg.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Batch install all apt packages if any
    if !apt_packages.is_empty() {
        script.push_str("echo 'Installing system packages (batched)...'\n");
        script.push_str("apt-get update -y\n");
        script.push_str("DEBIAN_FRONTEND=noninteractive apt-get install -y");
        for pkg in &apt_packages {
            script.push(' ');
            script.push_str(pkg);
        }
        script.push('\n');
        script.push('\n');
    }

    let mut unknowns = Vec::new();

    for tool in &ordered {
        if let Some(def) = reg.get(tool.as_str()) {
            let marker = format!("$MARKER_DIR/{}.installed", tool);
            script.push_str(&format!("if [ -f \"{}\" ]; then\n", marker));
            script.push_str(&format!("  echo 'Skipping {} (already installed)'\n", tool));
            script.push_str("else\n");
            script.push_str(&format!("  echo 'Installing {}...'\n", tool));

            for line in def.lines {
                // Skip apt-get install lines since we batched them above
                if line.contains("apt-get install") {
                    continue;
                }
                script.push_str("  ");
                script.push_str(line);
                script.push('\n');
            }

            script.push_str(&format!("  touch \"{}\"\n", marker));
            script.push_str(&format!("  echo 'Installed {}'\n", tool));
            script.push_str("fi\n\n");
        } else {
            unknowns.push(tool.clone());
        }
    }

    if !unknowns.is_empty() {
        script.push('\n');
        for u in unknowns {
            script.push_str(&format!(
                "echo 'No installer mapping for `{}`; skipping'\n",
                u
            ));
        }
    }

    Ok(script)
}

/// Transfer and execute the built script inside the VM using the backend
pub(crate) fn install_tools(backend: &dyn VmBackend, vm_name: &str, tools_csv: &str) -> Result<()> {
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
    backend.transfer(vm_name, &host_path, "/home/ubuntu/capsule-vm-setup.sh")?;

    // Execute in VM
    backend.exec(
        vm_name,
        &[
            "bash",
            "-lc",
            "sudo bash -lc 'chmod +x /home/ubuntu/capsule-vm-setup.sh && /home/ubuntu/capsule-vm-setup.sh'",
        ],
    )?;

    // Clean up temp file
    let _ = fs::remove_file(&host_path);

    Ok(())
}

pub(crate) fn supported_tools() -> Vec<String> {
    let mut v: Vec<String> = registry().keys().cloned().map(|s| s.to_string()).collect();
    v.sort();
    v
}

pub(crate) fn print_supported_tools() -> Result<()> {
    println!("Supported tools:");
    let reg = registry();
    for tool in supported_tools() {
        let def = reg.get(tool.as_str()).unwrap();
        let deps_str = if def.deps.is_empty() {
            String::new()
        } else {
            format!(" (deps: {})", def.deps.join(", "))
        };
        println!("  • {}{}", tool, deps_str);
    }

    println!("\nBundles:");
    println!("  • dev-min: git, python, pip");
    println!("  • web: node, npm, bun");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tools_expands_bundles_and_deduplicates() {
        let tools = parse_tools("python,dev-min,pip,Python");
        assert_eq!(tools, vec!["python", "git", "pip"]);
    }

    #[test]
    fn resolve_install_order_respects_dependencies() -> Result<()> {
        let requested = parse_tools("pip");
        let order = resolve_install_order(&requested)?;
        assert_eq!(order, vec!["python".to_string(), "pip".to_string()]);
        Ok(())
    }

    #[test]
    fn build_install_script_batches_packages_and_marks_completion() -> Result<()> {
        let script = build_install_script(&parse_tools("python,pip"))?;
        assert_eq!(script.matches("apt-get update -y").count(), 1);
        assert!(script.contains(
            "DEBIAN_FRONTEND=noninteractive apt-get install -y python3 python3-pip python3-venv"
        ));
        assert!(script.contains("touch \"$MARKER_DIR/python.installed\""));
        assert!(script.contains("touch \"$MARKER_DIR/pip.installed\""));
        Ok(())
    }

    #[test]
    fn build_install_script_handles_unknown_tool() -> Result<()> {
        let script = build_install_script(&vec!["unknown".to_string()])?;
        assert!(script.contains("No installer mapping for `unknown`; skipping"));
        Ok(())
    }
}
