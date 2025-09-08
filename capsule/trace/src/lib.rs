//! Process tracing via Linux strace
//!
//! This crate handles subprocess execution and raw strace output streaming.
//! It sends raw strace lines that the parse/ crate converts to StraceEvent structs.

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub struct LinuxTracer;

/// Traces Program Execution in Linux Environments
impl LinuxTracer {
    /// run strace with cancellation support and broadcast channel
    /// strace manual https://man7.org/linux/man-pages/man1/strace.1.html
    ///
    /// Ex: capsule run claude
    ///
    /// Executes claude binary with strace enabled.
    ///
    /// * Arguments
    ///
    /// `cmdline` - command line input
    ///             Ex: capsule run claude
    /// `tx_raw` - a tokio Sender, used to broadcast
    ///            raw strace lines to all connected Receivers
    /// `cancellation_token` - Ctrl + C
    ///                        A way to take keyboard
    ///                        input to terminate the program
    ///
    /// * Returns
    ///
    /// anyhow Result
    ///
    pub async fn run_with_cancellation(
        cmdline: Vec<String>,
        tx_raw: broadcast::Sender<String>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        if cmdline.is_empty() {
            anyhow::bail!("trace: command line empty");
        }
        // Build strace command - process-focused for now
        let mut child = Command::new("strace");
        child
            .arg("--follow-forks") // follow forks
            .arg("-tt") // timestamps with microseconds
            .arg("-v") // expand arrays/structs (full argv/env)
            .arg("-yy") // decode FDs and sockets to human-readable
            .arg("-s").arg("65535") // print full strings (avoid "\"..." truncation)
            .arg("-e")
            // Trace process, file, and network syscalls to see IO and connections
            .arg("trace=process,file,network")
            .arg("--")
            .args(&cmdline)
            .stdin(Stdio::inherit())
            // stdout will still be the terminal that ran the command
            // this gives the experience of transience
            .stdout(Stdio::inherit()) // Program output goes to user's terminal
            .stderr(Stdio::piped()) // Syscall traces captured here
            .kill_on_drop(true); // Ensure child is killed when dropped
        let mut child = child.spawn().with_context(|| "failed to spawn strace")?;

        // async-read strace output from stderr
        let stderr = child.stderr.take().unwrap();
        let mut rdr = BufReader::new(stderr).lines();

        tokio::select! {
            // Read strace lines and send raw strings
            result = async {
                while let Some(line) = rdr.next_line().await? {
                    // Send raw strace line for parsing downstream
                    if tx_raw.send(line).is_err() {
                        break; // No more receivers
                    }
                }
                Ok::<(), anyhow::Error>(())
            } => {
                if let Err(_) = result {
                    // Error reading strace output
                }
            },

            // Handle cancellation
            _ = cancellation_token.cancelled() => {
                let pid = child.id().unwrap_or(0);
                if pid > 0 {
                    // Kill the entire process group
                    let _ = kill_process_group(pid).await;
                }

                // Force kill the strace process itself
                let _ = child.kill();
            }
        }

        // Ensure child is terminated
        let _exit_status = child.wait().await?;

        Ok(())
    }

    // kills processes groups by process id
    async fn kill_process_group(pid: u32) -> Result<()> {
        use tokio::process::Command;

        // Terminating process group

        // First, try to kill child processes nicely
        let _ = Command::new("pkill")
            .arg("-TERM")
            .arg("-P")
            .arg(pid.to_string())
            .output()
            .await;

        // Give processes a moment to terminate gracefully
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Then force kill any remaining processes
        let _ = Command::new("pkill")
            .arg("-KILL")
            .arg("-P")
            .arg(pid.to_string())
            .output()
            .await;

        // Also kill the main process
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(pid.to_string())
            .output()
            .await;

        // Sent termination signals to process group
        Ok(())
    }
}

// terminates a process given a process
//
// `id` - unsigned integer for running process
async fn kill_process_group(pid: u32) -> Result<()> {
    LinuxTracer::kill_process_group(pid).await
}
