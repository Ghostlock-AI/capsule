use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use which::which;

fn main() -> Result<()> {
    let mut editor = DefaultEditor::new().context("failed to initialise capsule shell")?;

    if let Some(path) = history_path()? {
        if editor.load_history(&path).is_err() {
            // No history yet; ignore.
        }
    }

    loop {
        match editor.readline("agent$ ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Prevent users from bypassing the shell wrapper.
                if matches!(trimmed, "exit" | "logout") {
                    println!("Capsule shell keeps this session open. Use Ctrl+D to clear the line or disconnect.");
                    continue;
                }

                if let Some(path) = history_path()? {
                    match editor.add_history_entry(line.as_str()) {
                        Ok(true) => {
                            if let Err(err) = editor.save_history(&path) {
                                eprintln!("warning: failed to persist history: {err}");
                            }
                        }
                        Ok(false) => {}
                        Err(err) => eprintln!("warning: failed to record history entry: {err}"),
                    }
                }

                if let Err(err) = run_session(trimmed) {
                    eprintln!("shell: {err}");
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: start a fresh line.
                println!();
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: keep the shell alive but give the user a clean prompt.
                println!();
                continue;
            }
            Err(err) => return Err(anyhow!(err)).context("capsule shell input failed"),
        }
    }
}

fn run_session(command: &str) -> Result<()> {
    let session_id = make_session_id();
    let cwd = env::current_dir().context("failed to read current working directory")?;

    let launcher_path = locate_launcher()?;

    let status = Command::new(launcher_path)
        .arg("--session")
        .arg(&session_id)
        .arg("--cwd")
        .arg(cwd.display().to_string())
        .arg("--cmd")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to invoke launcher for session {session_id}"))?;

    if let Some(code) = status.code() {
        if code != 0 {
            eprintln!("launcher exited with status {code}");
        }
    } else {
        eprintln!("launcher terminated by signal");
    }

    Ok(())
}

fn locate_launcher() -> Result<PathBuf> {
    // First try PATH resolution, then fall back to sibling of the current executable.
    if let Ok(path) = which("launcher") {
        return Ok(path);
    }

    let current = env::current_exe().context("failed to resolve shell executable path")?;
    if let Some(dir) = current.parent() {
        let fallback = dir.join("launcher");
        if fallback.exists() {
            return Ok(fallback);
        }
    }

    Err(anyhow!("launcher is not installed in PATH"))
}

fn make_session_id() -> String {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let random: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect();
    format!("{timestamp}-{random}")
}

fn history_path() -> Result<Option<PathBuf>> {
    let home = match env::var("HOME") {
        Ok(path) => PathBuf::from(path),
        Err(_) => return Ok(None),
    };
    let history = home.join(".capsule_history");
    if !history.exists() {
        if let Some(parent) = history.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to ensure history directory {}",
                    parent.display()
                )
            })?;
        }
        std::fs::File::create(&history).with_context(|| {
            format!("failed to create history file {}", history.display())
        })?;
    }
    Ok(Some(history))
}
