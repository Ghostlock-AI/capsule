mod models;
mod trace;

use anyhow::{Context, Result};
use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use trace::LinuxTracer;

// =============================================
// CLI COMMANDS
// =============================================
#[derive(ClapParser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// run a program with tracing
    ///
    /// Examples:
    ///             capsule run pthon3 server.py
    ///             capsule run node app.js
    ///             capsule run ./binary
    ///             capsule run claude
    Run {
        program: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

// =============================================
// HELPERS
// =============================================
async fn setup_capsule_directory() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let capsule_dir = PathBuf::from(home).join(".capsule");

    if !capsule_dir.exists() {
        fs::create_dir_all(&capsule_dir)
            .await
            .context("Failed to create ~/.capsule directory")?;
    }

    Ok(capsule_dir)
}

// =============================================
// ROUTES: The paths of CLI commands
// =============================================
async fn run(program: String, args: Vec<String>) -> Result<()> {
    // Setup capsule directory
    let capsule_dir = setup_capsule_directory().await?;

    // Get working directory
    let working_dir = std::env::current_dir()
        .context("Failed to get current directory")?
        .to_string_lossy()
        .to_string();

    // Create session metadata
    let mut session = models::SessionMetadata::new(program.clone(), args.clone(), working_dir);

    // Create session directory with timestamp
    let session_dir_name = session.timestamp.format("%Y%m%d_%H%M%S").to_string();
    let session_dir = capsule_dir.join(session_dir_name);
    fs::create_dir_all(&session_dir)
        .await
        .context("Failed to create session directory")?;

    // Create raw trace file
    let trace_file_path = session_dir.join("raw_trace.txt");
    let mut trace_file = fs::File::create(&trace_file_path)
        .await
        .context("Failed to create raw_trace.txt")?;

    // Create structured syscalls file (JSONL format)
    let structured_file_path = session_dir.join("structured_syscalls.jsonl");
    let mut structured_file = fs::File::create(&structured_file_path)
        .await
        .context("Failed to create structured_syscalls.jsonl")?;

    // Create failed parse file
    let failed_file_path = session_dir.join("failed_parse_raw.txt");
    let mut failed_file = fs::File::create(&failed_file_path)
        .await
        .context("Failed to create failed_parse_raw.txt")?;

    let (tx, mut rx1) = broadcast::channel::<String>(1024);
    let mut rx2 = tx.subscribe();
    let mut rx3 = tx.subscribe();
    let cancellation_token = CancellationToken::new();

    let cancel_clone = cancellation_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_clone.cancel();
    });

    let mut cmdline = vec![program];
    cmdline.extend(args);

    let trace_handle =
        tokio::spawn(async move { LinuxTracer::trace(cmdline, tx, cancellation_token).await });

    // Write raw trace output to file
    let raw_writer_handle = tokio::spawn(async move {
        while let Ok(line) = rx1.recv().await {
            if let Err(_) = trace_file.write_all(line.as_bytes()).await {
                break;
            }
            if let Err(_) = trace_file.write_all(b"\n").await {
                break;
            }
        }
    });

    // Parse and write structured syscalls to JSONL file
    let structured_writer_handle = tokio::spawn(async move {
        while let Ok(line) = rx2.recv().await {
            // Try to parse the line into a RawSyscall
            if let Ok(raw_syscall) = trace::parse_raw_syscall(&line) {
                // Serialize to JSON and write as single line
                if let Ok(json) = serde_json::to_string(&raw_syscall) {
                    if let Err(_) = structured_file.write_all(json.as_bytes()).await {
                        break;
                    }
                    if let Err(_) = structured_file.write_all(b"\n").await {
                        break;
                    }
                }
            }
            // Silently skip unparseable lines (empty, unfinished syscalls, etc.)
        }
    });

    // Write failed parse lines to separate file
    let failed_writer_handle = tokio::spawn(async move {
        while let Ok(line) = rx3.recv().await {
            // Try to parse, if it fails, write to failed file
            if let Err(_) = trace::parse_raw_syscall(&line) {
                if let Err(_) = failed_file.write_all(line.as_bytes()).await {
                    break;
                }
                if let Err(_) = failed_file.write_all(b"\n").await {
                    break;
                }
            }
        }
    });

    trace_handle.await??;
    raw_writer_handle.abort();
    structured_writer_handle.abort();
    failed_writer_handle.abort();

    // Mark session as complete
    session.complete();

    // Write session metadata
    let metadata_path = session_dir.join("session_metadata.json");
    let metadata_json =
        serde_json::to_string_pretty(&session).context("Failed to serialize session metadata")?;
    fs::write(&metadata_path, metadata_json)
        .await
        .context("Failed to write session metadata")?;

    Ok(())
}

// =============================================
// MAIN: Where CLI commands connect to routes
// =============================================
#[tokio::main]
async fn main() {
    match Cli::parse().cmd {
        Cmd::Run { program, args } => {
            if let Err(e) = run(program, args).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
