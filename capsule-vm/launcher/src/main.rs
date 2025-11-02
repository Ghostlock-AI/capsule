use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, setgid, setuid, ForkResult, Gid, Pid, Uid};
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::ffi::CString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use which::which;

const SESSION_ROOT: &str = "/var/log/tracee/sessions";
const TRACE_CONFIG: &str = "/etc/tracee/session.yaml";

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    session: String,

    #[arg(long)]
    cmd: String,

    #[arg(long)]
    cwd: Option<PathBuf>,
}

#[derive(Serialize)]
struct Metadata {
    session_id: String,
    command: String,
    cwd: String,
    environment: BTreeMap<String, String>,
    start_time: DateTime<Utc>,
    end_time: Option<DateTime<Utc>>,
    exit_status: Option<ExitSummary>,
    tracee_exit_status: Option<ExitSummary>,
    tracee_log: String,
    events_log: String,
}

#[derive(Serialize, Clone)]
struct ExitSummary {
    code: Option<i32>,
    signal: Option<String>,
    signal_number: Option<i32>,
}

struct AgentContext {
    uid: Uid,
    gid: Gid,
    #[cfg_attr(any(target_vendor = "apple", target_os = "redox", target_os = "haiku"), allow(dead_code))]
    name: String,
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("launcher: {err:#}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32> {
    if !Uid::effective().is_root() {
        bail!("launcher must run with effective UID 0");
    }

    let args = Args::parse();
    let agent = resolve_agent()?;

    let session_root = Path::new(SESSION_ROOT);
    fs::create_dir_all(session_root)
        .with_context(|| format!("failed to create session root {}", session_root.display()))?;
    let session_dir = session_root.join(&args.session);
    fs::create_dir_all(&session_dir)
        .with_context(|| format!("failed to create session directory {}", session_dir.display()))?;

    let events_log = session_dir.join("events.jsonl");
    let tracee_log = session_dir.join("tracee.log");
    let metadata_path = session_dir.join("metadata.json");

    let cwd = match &args.cwd {
        Some(path) => path.clone(),
        None => env::current_dir().context("failed to determine working directory")?,
    };

    let mut environment: BTreeMap<String, String> = env::vars().collect();
    environment.insert("TRACE_SESSION_ID".into(), args.session.clone());

    let start_time = Utc::now();
    let mut metadata = Metadata {
        session_id: args.session.clone(),
        command: args.cmd.clone(),
        cwd: cwd.display().to_string(),
        environment: environment.clone(),
        start_time,
        end_time: None,
        exit_status: None,
        tracee_exit_status: None,
        tracee_log: tracee_log.display().to_string(),
        events_log: events_log.display().to_string(),
    };

    write_metadata(&metadata_path, &metadata)?;

    // Fork the launcher so the child can exec the agent command.
    let child_pid = match unsafe { fork()? } {
        ForkResult::Child => {
            if let Err(err) = launch_agent_command(&args.cmd, &cwd, &environment, &agent) {
                eprintln!("launcher: {err:#}");
                std::process::exit(1);
            }
            unreachable!("execve should replace the child process");
        }
        ForkResult::Parent { child } => child,
    };

    let mut tracee_child = match start_tracee(child_pid, &session_dir, &events_log, &tracee_log) {
        Ok(child) => child,
        Err(err) => {
            let _ = kill(child_pid, Signal::SIGTERM);
            let _ = waitpid(child_pid, None);
            return Err(err);
        }
    };

    let exit_summary = wait_for_child(child_pid)?;
    metadata.exit_status = Some(exit_summary.clone());

    // Signal Tracee to stop, then wait for it.
    let tracee_summary = stop_tracee(&mut tracee_child)?;
    metadata.tracee_exit_status = Some(tracee_summary);
    metadata.end_time = Some(Utc::now());
    write_metadata(&metadata_path, &metadata)?;

    Ok(calculate_exit_code(&exit_summary))
}

fn launch_agent_command(
    command: &str,
    cwd: &Path,
    environment: &BTreeMap<String, String>,
    agent: &AgentContext,
) -> Result<()> {
    #[cfg(not(any(target_vendor = "apple", target_os = "redox", target_os = "haiku")))]
    {
        let agent_name_cstr = CString::new(agent.name.clone())
            .map_err(|_| anyhow::anyhow!("agent username contains embedded NUL byte"))?;
        nix::unistd::initgroups(&agent_name_cstr, agent.gid)
            .with_context(|| "failed to initialise supplementary groups")?;
    }
    setgid(agent.gid).with_context(|| "failed to drop to agent gid")?;
    setuid(agent.uid).with_context(|| "failed to drop to agent uid")?;

    nix::unistd::chdir(cwd).with_context(|| format!("failed to change directory to {}", cwd.display()))?;

    let shell_path = CString::new("/bin/bash")?;
    let command_cstr = CString::new(command)
        .map_err(|_| anyhow::anyhow!("command contains embedded NUL byte"))?;
    let argv = vec![CString::new("bash")?, CString::new("-lc")?, command_cstr];

    let mut env_pairs: Vec<CString> = Vec::with_capacity(environment.len());
    for (key, value) in environment {
        let entry = CString::new(format!("{key}={value}"))
            .map_err(|_| anyhow::anyhow!("environment variable {key} contains embedded NUL byte"))?;
        env_pairs.push(entry);
    }

    nix::unistd::execve(&shell_path, &argv, &env_pairs)
        .map_err(|err| anyhow::anyhow!("failed to exec command via bash: {err}"))?;

    unreachable!("execve should not return");
}

fn start_tracee(
    child_pid: Pid,
    session_dir: &Path,
    events_log: &Path,
    tracee_log: &Path,
) -> Result<std::process::Child> {
    let tracee_path = which("tracee").context("tracee binary not found in PATH")?;

    let events_arg = format!("json:{}", events_log.display());
    let scope_pid = format!("pid={}", child_pid);

    let mut cmd = Command::new(tracee_path);
    cmd.arg("--config")
        .arg(TRACE_CONFIG)
        .arg("--scope")
        .arg(scope_pid)
        .arg("--scope")
        .arg("follow")
        .arg("--output")
        .arg(events_arg)
        .arg("--log")
        .arg(format!("file:{}", tracee_log.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .current_dir(session_dir);

    cmd.spawn().context("failed to spawn tracee for session")
}

fn wait_for_child(child_pid: Pid) -> Result<ExitSummary> {
    loop {
        #[allow(unreachable_patterns)]
        match waitpid(child_pid, None).context("failed to wait for agent process")? {
            WaitStatus::Exited(_, code) => {
                return Ok(ExitSummary {
                    code: Some(code),
                    signal: None,
                    signal_number: None,
                });
            }
            WaitStatus::Signaled(_, sig, _) => {
                return Ok(ExitSummary {
                    code: None,
                    signal: Some(sig.as_str().to_string()),
                    signal_number: Some(sig as i32),
                });
            }
            WaitStatus::Stopped(_, _) | WaitStatus::Continued(_) | WaitStatus::StillAlive => continue,
            _ => continue,
        }
    }
}

fn stop_tracee(child: &mut std::process::Child) -> Result<ExitSummary> {
    let pid = Pid::from_raw(child.id() as i32);
    let _ = kill(pid, Signal::SIGTERM);
    let status = child.wait().context("failed to wait for tracee process")?;
    if let Some(code) = status.code() {
        Ok(ExitSummary {
            code: Some(code),
            signal: None,
            signal_number: None,
        })
    } else {
        Ok(ExitSummary {
            code: None,
            signal: Some("terminated".into()),
            signal_number: None,
        })
    }
}

fn calculate_exit_code(exit: &ExitSummary) -> i32 {
    if let Some(code) = exit.code {
        code
    } else if let Some(signal) = exit.signal_number {
        128 + signal
    } else {
        1
    }
}

fn write_metadata(path: &Path, metadata: &Metadata) -> Result<()> {
    let mut file = File::create(path)
        .with_context(|| format!("failed to open metadata file {}", path.display()))?;
    let json = serde_json::to_vec_pretty(metadata).context("failed to serialise session metadata")?;
    file.write_all(&json).context("failed to write session metadata")?;
    file.write_all(b"\n").ok();
    Ok(())
}

fn resolve_agent() -> Result<AgentContext> {
    let user = nix::unistd::User::from_name("agent")
        .map_err(|err| anyhow::anyhow!("failed to resolve agent user: {err}"))?
        .ok_or_else(|| anyhow::anyhow!("agent user does not exist"))?;
    Ok(AgentContext {
        uid: user.uid,
        gid: user.gid,
        name: user.name,
    })
}
