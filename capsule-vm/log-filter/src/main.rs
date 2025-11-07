use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "capsule-log-filter")]
#[command(about = "Transform verbose Tracee events into human-readable logs")]
struct Cli {
    /// Input file (raw Tracee events)
    #[arg(long, default_value = "/var/log/tracee/events.jsonl")]
    input: PathBuf,

    /// Output file (human-readable events)
    #[arg(long, default_value = "/var/log/tracee/events-readable.jsonl")]
    output: PathBuf,

    /// Poll interval in milliseconds
    #[arg(long, default_value = "100")]
    poll_interval: u64,
}

// Raw Tracee event structure (subset we care about)
#[derive(Deserialize, Debug)]
struct RawEvent {
    timestamp: i64,
    #[serde(rename = "processId")]
    process_id: i64,
    #[serde(rename = "userId")]
    user_id: i64,
    #[serde(rename = "processName")]
    process_name: String,
    #[serde(rename = "eventName")]
    event_name: String,
    #[serde(rename = "returnValue")]
    return_value: i64,
    args: Vec<EventArg>,
}

#[derive(Deserialize, Debug)]
struct EventArg {
    name: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    arg_type: String,
    value: serde_json::Value,
}

// Human-readable event structure
#[derive(Serialize, Debug)]
struct ReadableEvent {
    timestamp: String,
    event: String,
    description: String,
    user: String,
    pid: i64,
    details: serde_json::Value,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("📡 Capsule Log Filter started");
    println!("   Input:  {}", cli.input.display());
    println!("   Output: {}", cli.output.display());

    // Wait for input file to exist
    while !cli.input.exists() {
        eprintln!("⏳ Waiting for input file to be created...");
        thread::sleep(Duration::from_secs(1));
    }

    // Open input file for reading
    let mut input_file = File::open(&cli.input)
        .with_context(|| format!("Failed to open input file: {}", cli.input.display()))?;
    let mut last_position = 0u64;

    // Open output file for appending
    let mut output_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cli.output)
        .with_context(|| format!("Failed to open output file: {}", cli.output.display()))?;

    println!("✅ Ready to process events");

    loop {
        // Seek to last read position
        input_file
            .seek(SeekFrom::Start(last_position))
            .context("Failed to seek in input file")?;

        let reader = BufReader::new(&input_file);
        let mut processed_any = false;

        for line in reader.lines() {
            let line = line.context("Failed to read line from input file")?;

            if line.trim().is_empty() {
                last_position += 1; // newline
                continue;
            }

            // Parse raw event
            match serde_json::from_str::<RawEvent>(&line) {
                Ok(raw_event) => {
                    // Transform to readable event
                    match transform_event(raw_event) {
                        Ok(readable) => {
                            // Write to output file
                            let json = serde_json::to_string(&readable)
                                .context("Failed to serialize readable event")?;
                            writeln!(output_file, "{}", json)
                                .context("Failed to write to output file")?;
                            processed_any = true;
                        }
                        Err(e) => {
                            eprintln!("⚠️  Failed to transform event: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("⚠️  Failed to parse event JSON: {}", e);
                    eprintln!("    Line: {}", &line[..line.len().min(100)]);
                }
            }

            // Update position
            last_position += line.len() as u64 + 1; // +1 for newline
        }

        // Flush output if we processed anything
        if processed_any {
            output_file.flush().context("Failed to flush output file")?;
        }

        // Sleep before next poll
        thread::sleep(Duration::from_millis(cli.poll_interval));
    }
}

fn transform_event(raw: RawEvent) -> Result<ReadableEvent> {
    // Convert timestamp to ISO 8601
    let timestamp = convert_timestamp(raw.timestamp);

    // Determine user
    let user = if raw.user_id == 1000 {
        "agent"
    } else if raw.user_id == 0 {
        "root"
    } else {
        "other"
    };

    // Event-specific transformation
    let (description, details) = match raw.event_name.as_str() {
        "sched_process_exec" | "execve" => transform_exec(&raw)?,
        "sched_process_exit" | "exit_group" | "exit" => transform_exit(&raw)?,
        "net_tcp_connect" => transform_tcp_connect(&raw)?,
        "security_socket_connect" | "connect" => transform_connect(&raw)?,
        "security_file_open" | "openat" => transform_file_open(&raw)?,
        "close" => transform_close(&raw)?,
        _ => (
            format!("Unknown event: {}", raw.event_name),
            serde_json::json!({}),
        ),
    };

    Ok(ReadableEvent {
        timestamp,
        event: raw.event_name,
        description,
        user: user.to_string(),
        pid: raw.process_id,
        details,
    })
}

fn transform_exec(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    let command = &raw.process_name;
    let argv = find_arg(raw, "argv");
    let cmdpath = find_arg(raw, "cmdpath");
    let env = find_arg(raw, "env");

    let description = format!("Executed: {}", command);

    let mut details = serde_json::json!({
        "command": command,
    });

    if let Some(args) = argv {
        details["args"] = args;
    }
    if let Some(path) = cmdpath {
        details["path"] = path;
    }
    if let Some(env_val) = env {
        // Only include env if it's non-empty
        if let Some(arr) = env_val.as_array() {
            if !arr.is_empty() {
                details["env"] = env_val;
            }
        }
    }

    Ok((description, details))
}

fn transform_exit(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    let command = &raw.process_name;
    let exit_code = raw.return_value;

    let description = format!("Process exited: {} (code: {})", command, exit_code);

    let details = serde_json::json!({
        "command": command,
        "exit_code": exit_code,
    });

    Ok((description, details))
}

fn transform_tcp_connect(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    // net_tcp_connect has clean structure: dst, dst_port, dst_dns
    let dst = find_arg(raw, "dst");
    let dst_port = find_arg(raw, "dst_port");

    let addr_str = dst.as_ref().and_then(|v| v.as_str()).unwrap_or("unknown");
    let port = dst_port.as_ref().and_then(|v| v.as_i64()).unwrap_or(0);

    let description = format!("Connected to: {}:{}", addr_str, port);

    let details = serde_json::json!({
        "remote_addr": addr_str,
        "remote_port": port,
        "protocol": "tcp"
    });

    Ok((description, details))
}

fn transform_connect(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    // Raw connect syscall - try to parse sockaddr from args
    // Tracee provides sockaddr in different formats depending on address family

    // Look for common argument names
    let remote_addr = find_arg(raw, "remote_addr")
        .or_else(|| find_arg(raw, "addr"))
        .or_else(|| find_arg(raw, "sockaddr"));

    let description = if let Some(addr) = &remote_addr {
        // Try to extract a readable address
        if let Some(addr_obj) = addr.as_object() {
            // Check if it's a sockaddr structure
            if let Some(family) = addr_obj.get("sa_family").and_then(|v| v.as_str()) {
                match family {
                    "AF_INET" | "AF_INET6" => {
                        // IP socket
                        let ip = addr_obj.get("sin_addr")
                            .or_else(|| addr_obj.get("sin6_addr"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let port = addr_obj.get("sin_port")
                            .or_else(|| addr_obj.get("sin6_port"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        format!("Connected to: {}:{}", ip, port)
                    }
                    "AF_UNIX" => {
                        // Unix domain socket
                        let path = addr_obj.get("sun_path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        format!("Connected to Unix socket: {}", path)
                    }
                    _ => format!("Connected to {} socket", family),
                }
            } else {
                format!("Connected to network socket")
            }
        } else if let Some(addr_str) = addr.as_str() {
            format!("Connected to: {}", addr_str)
        } else {
            format!("Connected to network socket")
        }
    } else {
        format!("Connected to network socket")
    };

    let mut details = serde_json::json!({});

    if let Some(addr) = remote_addr {
        details["remote_addr"] = addr;
    }

    Ok((description, details))
}

fn transform_file_open(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    // Handle both openat (has pathname) and security_file_open
    let pathname = find_arg(raw, "pathname").or_else(|| find_arg(raw, "path"));
    let flags = find_arg(raw, "flags");

    let path = pathname
        .as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Decode flags to mode
    let mode = if let Some(flags_val) = &flags {
        decode_open_flags(flags_val)
    } else {
        "unknown".to_string()
    };

    let description = format!("Opened file: {} ({})", path, mode);

    let details = serde_json::json!({
        "path": path,
        "mode": mode,
    });

    Ok((description, details))
}

fn transform_close(raw: &RawEvent) -> Result<(String, serde_json::Value)> {
    // close() syscall - closes file descriptor (could be file or socket)
    let fd = find_arg(raw, "fd");

    let fd_num = fd.as_ref().and_then(|v| v.as_i64()).unwrap_or(-1);

    // We don't know if it's a file or socket from just close()
    // This is a limitation - we'd need to track fd state
    let description = format!("Closed fd: {}", fd_num);

    let details = serde_json::json!({
        "fd": fd_num,
    });

    Ok((description, details))
}

fn find_arg(raw: &RawEvent, name: &str) -> Option<serde_json::Value> {
    raw.args
        .iter()
        .find(|arg| arg.name == name)
        .map(|arg| arg.value.clone())
}

fn decode_open_flags(flags: &serde_json::Value) -> String {
    // Parse flags integer
    let flags_int = if let Some(num) = flags.as_i64() {
        num
    } else if let Some(s) = flags.as_str() {
        // Try parsing as string (might be hex or decimal)
        if let Some(hex_str) = s.strip_prefix("0x") {
            i64::from_str_radix(hex_str, 16).unwrap_or(0)
        } else {
            s.parse::<i64>().unwrap_or(0)
        }
    } else {
        return "unknown".to_string();
    };

    // O_RDONLY = 0, O_WRONLY = 1, O_RDWR = 2
    let access_mode = flags_int & 0o3;

    let base_mode = match access_mode {
        0 => "read",
        1 => "write",
        2 => "read+write",
        _ => "unknown",
    };

    // Check for additional flags
    let mut mode_parts = vec![base_mode];

    // O_CREAT = 0100 (octal) = 64 (decimal)
    if flags_int & 0o100 != 0 {
        mode_parts.push("create");
    }

    // O_APPEND = 02000 (octal) = 1024 (decimal)
    if flags_int & 0o2000 != 0 {
        mode_parts.push("append");
    }

    // O_TRUNC = 01000 (octal) = 512 (decimal)
    if flags_int & 0o1000 != 0 {
        mode_parts.push("truncate");
    }

    if mode_parts.len() > 1 {
        format!("{}+{}", mode_parts[0], mode_parts[1..].join("+"))
    } else {
        mode_parts[0].to_string()
    }
}

fn convert_timestamp(ns: i64) -> String {
    use chrono::DateTime;

    // Tracee timestamps are in nanoseconds since epoch
    let secs = ns / 1_000_000_000;
    let nanos = (ns % 1_000_000_000) as u32;

    if let Some(dt) = DateTime::from_timestamp(secs, nanos) {
        dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    } else {
        "invalid-timestamp".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_timestamp() {
        // Example timestamp from your sample
        let ns = 1762494108829364348;
        let ts = convert_timestamp(ns);
        assert!(ts.starts_with("2025-"));
        assert!(ts.contains("T"));
        assert!(ts.contains("Z"));
    }

    #[test]
    fn test_decode_open_flags_readonly() {
        let flags = serde_json::json!(0);
        assert_eq!(decode_open_flags(&flags), "read");
    }

    #[test]
    fn test_decode_open_flags_writeonly() {
        let flags = serde_json::json!(1);
        assert_eq!(decode_open_flags(&flags), "write");
    }

    #[test]
    fn test_decode_open_flags_readwrite() {
        let flags = serde_json::json!(2);
        assert_eq!(decode_open_flags(&flags), "read+write");
    }

    #[test]
    fn test_decode_open_flags_with_create() {
        // O_WRONLY | O_CREAT = 1 | 64 = 65
        let flags = serde_json::json!(65);
        assert_eq!(decode_open_flags(&flags), "write+create");
    }

    #[test]
    fn test_decode_open_flags_with_append() {
        // O_WRONLY | O_APPEND = 1 | 1024 = 1025
        let flags = serde_json::json!(1025);
        assert_eq!(decode_open_flags(&flags), "write+append");
    }

    #[test]
    fn test_transform_exec() {
        let raw = RawEvent {
            timestamp: 1762494108829364348,
            process_id: 2769,
            user_id: 1000,
            process_name: "clear".to_string(),
            event_name: "sched_process_exec".to_string(),
            return_value: 0,
            args: vec![
                EventArg {
                    name: "cmdpath".to_string(),
                    arg_type: "const char*".to_string(),
                    value: serde_json::json!("/usr/bin/clear"),
                },
                EventArg {
                    name: "argv".to_string(),
                    arg_type: "const char**".to_string(),
                    value: serde_json::json!(["clear"]),
                },
            ],
        };

        let (desc, details) = transform_exec(&raw).unwrap();
        assert_eq!(desc, "Executed: clear");
        assert_eq!(details["command"], "clear");
        assert_eq!(details["path"], "/usr/bin/clear");
        assert_eq!(details["args"], serde_json::json!(["clear"]));
    }

    #[test]
    fn test_transform_exit() {
        let raw = RawEvent {
            timestamp: 1762494108829364348,
            process_id: 2769,
            user_id: 1000,
            process_name: "clear".to_string(),
            event_name: "sched_process_exit".to_string(),
            return_value: 0,
            args: vec![],
        };

        let (desc, details) = transform_exit(&raw).unwrap();
        assert_eq!(desc, "Process exited: clear (code: 0)");
        assert_eq!(details["command"], "clear");
        assert_eq!(details["exit_code"], 0);
    }

    #[test]
    fn test_transform_file_open() {
        let raw = RawEvent {
            timestamp: 1762494108829364348,
            process_id: 2769,
            user_id: 1000,
            process_name: "cat".to_string(),
            event_name: "security_file_open".to_string(),
            return_value: 0,
            args: vec![
                EventArg {
                    name: "pathname".to_string(),
                    arg_type: "const char*".to_string(),
                    value: serde_json::json!("/etc/hosts"),
                },
                EventArg {
                    name: "flags".to_string(),
                    arg_type: "int".to_string(),
                    value: serde_json::json!(0),
                },
            ],
        };

        let (desc, details) = transform_file_open(&raw).unwrap();
        assert_eq!(desc, "Opened file: /etc/hosts (read)");
        assert_eq!(details["path"], "/etc/hosts");
        assert_eq!(details["mode"], "read");
    }
}
