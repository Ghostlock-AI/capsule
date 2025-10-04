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

use crate::models::{RawSyscall, SyscallCategory};

pub struct LinuxTracer;

// =============================================
// SYSCALL PARSING
// =============================================

/// Parse a raw strace line into a RawSyscall structure
pub fn parse_raw_syscall(line: &str) -> Result<RawSyscall> {
    // Handle empty lines or special markers
    if line.trim().is_empty() || line.contains("<unfinished") || line.contains("resumed>") {
        anyhow::bail!("Skipping unfinished/resumed syscall line");
    }

    // Extract optional PID: "[pid  1234] "
    let (pid, line_after_pid) = extract_pid(line);

    // Extract timestamp: "HH:MM:SS.microseconds "
    let (timestamp, line_after_ts) = extract_timestamp(line_after_pid)?;

    // Extract syscall number: "[ 123] "
    let (syscall_number, line_after_num) = extract_syscall_number(line_after_ts)?;

    // Extract syscall name: "syscall_name("
    let (syscall_name, line_after_name) = extract_syscall_name(line_after_num)?;

    // Find the matching closing paren for arguments
    let (raw_args_str, line_after_args) = extract_args_section(line_after_name)?;

    // Extract return value: "= ..."
    let raw_return = extract_return_value(line_after_args)?;

    // Split arguments respecting nesting
    let raw_args = split_arguments(raw_args_str);

    // Categorize syscall
    let category = categorize_syscall(&syscall_name);

    Ok(RawSyscall {
        timestamp,
        pid,
        syscall_number,
        syscall_name,
        raw_args,
        raw_return,
        category,
    })
}

/// Extract PID if present: "[pid  1234] " -> Some(1234)
fn extract_pid(line: &str) -> (Option<u32>, &str) {
    if let Some(stripped) = line.strip_prefix("[pid") {
        if let Some(close_idx) = stripped.find(']') {
            let pid_str = stripped[..close_idx].trim();
            if let Ok(pid) = pid_str.parse::<u32>() {
                return (Some(pid), stripped[close_idx + 1..].trim_start());
            }
        }
    }
    (None, line)
}

/// Extract timestamp: "HH:MM:SS.microseconds " -> ("HH:MM:SS.microseconds", rest)
fn extract_timestamp(line: &str) -> Result<(String, &str)> {
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    if parts.len() < 2 {
        anyhow::bail!("No timestamp found");
    }
    Ok((parts[0].to_string(), parts[1]))
}

/// Extract syscall number: "[ 123] " -> (123, rest)
fn extract_syscall_number(line: &str) -> Result<(u32, &str)> {
    if !line.starts_with('[') {
        anyhow::bail!("No syscall number bracket found");
    }

    if let Some(close_idx) = line.find(']') {
        let num_str = line[1..close_idx].trim();
        let syscall_number = num_str.parse::<u32>()
            .context("Failed to parse syscall number")?;
        Ok((syscall_number, line[close_idx + 1..].trim_start()))
    } else {
        anyhow::bail!("No closing bracket for syscall number");
    }
}

/// Extract syscall name: "syscall_name(" -> ("syscall_name", rest)
fn extract_syscall_name(line: &str) -> Result<(String, &str)> {
    if let Some(paren_idx) = line.find('(') {
        let name = line[..paren_idx].trim().to_string();
        Ok((name, &line[paren_idx + 1..]))
    } else {
        anyhow::bail!("No opening parenthesis for syscall arguments");
    }
}

/// Extract arguments section between ( and ) respecting nesting
fn extract_args_section(line: &str) -> Result<(&str, &str)> {
    let mut depth = 1; // We already consumed the opening '('
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();

    while i < chars.len() && depth > 0 {
        match chars[i] {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }

    if depth != 0 {
        anyhow::bail!("Unmatched parentheses in arguments");
    }

    // i points just after the closing ')'
    let args_str = &line[..i - 1]; // Exclude the closing ')'
    let rest = &line[i..].trim_start();

    Ok((args_str, rest))
}

/// Extract return value: "= value" or "= -1 ERRNO (message)"
fn extract_return_value(line: &str) -> Result<String> {
    if let Some(eq_idx) = line.find('=') {
        Ok(line[eq_idx + 1..].trim().to_string())
    } else {
        anyhow::bail!("No return value found (no '=' sign)");
    }
}

/// Split arguments by comma, respecting nesting of (), {}, []
fn split_arguments(args_str: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut depth = 0;
    let mut in_quotes = false;
    let mut escape_next = false;

    for ch in args_str.chars() {
        if escape_next {
            current_arg.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' => {
                escape_next = true;
                current_arg.push(ch);
            }
            '"' => {
                in_quotes = !in_quotes;
                current_arg.push(ch);
            }
            '(' | '{' | '[' if !in_quotes => {
                depth += 1;
                current_arg.push(ch);
            }
            ')' | '}' | ']' if !in_quotes => {
                depth -= 1;
                current_arg.push(ch);
            }
            ',' if depth == 0 && !in_quotes => {
                // This is a top-level argument separator
                args.push(current_arg.trim().to_string());
                current_arg.clear();
            }
            _ => {
                current_arg.push(ch);
            }
        }
    }

    // Don't forget the last argument
    if !current_arg.trim().is_empty() {
        args.push(current_arg.trim().to_string());
    }

    args
}

/// Categorize syscall by name
fn categorize_syscall(name: &str) -> SyscallCategory {
    match name {
        // Process syscalls
        "execve" | "clone" | "fork" | "vfork" | "wait4" | "exit" | "exit_group"
        | "getpid" | "getppid" => SyscallCategory::Process,

        // File syscalls
        "openat" | "read" | "write" | "close" | "newfstatat" | "readlinkat"
        | "getcwd" | "chdir" | "unlinkat" | "renameat" | "faccessat" => SyscallCategory::File,

        // Network syscalls
        "socket" | "bind" | "connect" | "listen" | "accept" | "accept4"
        | "sendto" | "recvfrom" | "shutdown" | "setsockopt" | "getsockopt" => SyscallCategory::Network,

        _ => SyscallCategory::Unknown,
    }
}

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
    pub async fn trace(
        cmdline: Vec<String>,
        tx_raw: broadcast::Sender<String>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        if cmdline.is_empty() {
            anyhow::bail!("trace: command line empty");
        }
        // Build strace command - process-focused for now
        let mut child = Command::new("strace");
        // CRITICAL: The order of these matters for the parsing
        child
            .arg("--follow-forks") // follow forks
            .arg("-n")
            .arg("-tt") // timestamps with microseconds
            .arg("-v") // expand arrays/structs (full argv/env)
            .arg("-yy") // decode FDs and sockets to human-readable
            .arg("-s")
            .arg("65535") // print full strings (avoid "\"..." truncation)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pid() {
        // With PID
        let (pid, rest) = extract_pid("[pid   536] 19:31:36.986502 rest of line");
        assert_eq!(pid, Some(536));
        assert_eq!(rest, "19:31:36.986502 rest of line");

        // With PID, no extra spaces
        let (pid, rest) = extract_pid("[pid 123] timestamp");
        assert_eq!(pid, Some(123));
        assert_eq!(rest, "timestamp");

        // Without PID
        let (pid, rest) = extract_pid("19:31:36.986502 no pid here");
        assert_eq!(pid, None);
        assert_eq!(rest, "19:31:36.986502 no pid here");
    }

    #[test]
    fn test_parse_raw_syscall_with_pid() {
        let line = "[pid   536] 19:31:36.986502 [  35] unlinkat(AT_FDCWD, \"/root/.claude.json.lock\", AT_REMOVEDIR) = 0";
        let syscall = parse_raw_syscall(line).unwrap();

        assert_eq!(syscall.pid, Some(536));
        assert_eq!(syscall.timestamp, "19:31:36.986502");
        assert_eq!(syscall.syscall_number, 35);
        assert_eq!(syscall.syscall_name, "unlinkat");
        assert_eq!(syscall.raw_args.len(), 3);
        assert_eq!(syscall.raw_args[0], "AT_FDCWD");
        assert_eq!(syscall.raw_args[1], "\"/root/.claude.json.lock\"");
        assert_eq!(syscall.raw_args[2], "AT_REMOVEDIR");
        assert_eq!(syscall.raw_return, "0");
        assert_eq!(syscall.category, SyscallCategory::File);
    }

    #[test]
    fn test_parse_raw_syscall_without_pid() {
        let line = "19:31:36.986502 [  35] unlinkat(AT_FDCWD, \"/root/.claude.json.lock\", AT_REMOVEDIR) = 0";
        let syscall = parse_raw_syscall(line).unwrap();

        assert_eq!(syscall.pid, None);
        assert_eq!(syscall.timestamp, "19:31:36.986502");
        assert_eq!(syscall.syscall_number, 35);
        assert_eq!(syscall.syscall_name, "unlinkat");
    }

    #[test]
    fn test_parse_execve_syscall() {
        let line = "[pid   123] 00:05:14.247211 [ 221] execve(\"/usr/bin/claude\", [\"claude\"], [\"ENV=value\"]) = 0";
        let syscall = parse_raw_syscall(line).unwrap();

        assert_eq!(syscall.pid, Some(123));
        assert_eq!(syscall.syscall_name, "execve");
        assert_eq!(syscall.category, SyscallCategory::Process);
        assert_eq!(syscall.raw_args.len(), 3);
    }

    #[test]
    fn test_split_arguments() {
        // Simple args
        let args = split_arguments("arg1, arg2, arg3");
        assert_eq!(args, vec!["arg1", "arg2", "arg3"]);

        // Args with nested brackets
        let args = split_arguments("AT_FDCWD, \"/path/file\", [\"item1\", \"item2\"]");
        assert_eq!(args.len(), 3);
        assert_eq!(args[2], "[\"item1\", \"item2\"]");

        // Empty args
        let args = split_arguments("");
        assert_eq!(args.len(), 0);
    }

    #[test]
    fn test_categorize_syscall() {
        assert_eq!(categorize_syscall("execve"), SyscallCategory::Process);
        assert_eq!(categorize_syscall("openat"), SyscallCategory::File);
        assert_eq!(categorize_syscall("socket"), SyscallCategory::Network);
        assert_eq!(categorize_syscall("unknown_syscall"), SyscallCategory::Unknown);
    }
}
