use crate::errors::VmError;
use crate::retry::{RetryConfig, retry_operation};
use crate::validation::{check_stderr_for_errors, health_check_vm, validate_vm_config};
use crate::vm_backend::{VmBackend, VmConfig, VmInfo};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod install;
mod template;

struct SerialLogStream {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl SerialLogStream {
    fn new(instance: String, path: PathBuf) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let join = thread::Builder::new()
            .name(format!("lima-serial-{}", instance))
            .spawn(move || Self::stream_loop(instance, path, stop_clone))
            .context("failed to start Lima serial log streamer")?;

        Ok(Self {
            stop,
            handle: Some(join),
        })
    }

    fn stream_loop(instance: String, path: PathBuf, stop: Arc<AtomicBool>) {
        let mut position: u64 = 0;
        let mut warned_missing = false;

        while !stop.load(Ordering::Relaxed) {
            match File::open(&path) {
                Ok(mut file) => {
                    warned_missing = false;

                    if let Ok(metadata) = file.metadata() {
                        if metadata.len() < position {
                            position = 0;
                        }
                    }

                    if file.seek(SeekFrom::Start(position)).is_err() {
                        position = 0;
                        continue;
                    }

                    let mut reader = BufReader::new(file);

                    loop {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }

                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => {
                                thread::sleep(Duration::from_millis(200));
                            }
                            Ok(bytes_read) => {
                                position += bytes_read as u64;
                                let trimmed = line.trim_end();
                                if !trimmed.is_empty() {
                                    println!("[lima:{}] {}", instance, trimmed);
                                    let _ = std::io::stdout().flush();
                                }
                            }
                            Err(err) => {
                                eprintln!(
                                    "[lima:{}] Failed to read serial log ({}): {}",
                                    instance,
                                    path.display(),
                                    err
                                );
                                thread::sleep(Duration::from_millis(500));
                                break;
                            }
                        }
                    }
                }
                Err(err) => {
                    if !warned_missing {
                        eprintln!(
                            "[lima:{}] Waiting for serial log ({}): {}",
                            instance,
                            path.display(),
                            err
                        );
                        warned_missing = true;
                    }
                    thread::sleep(Duration::from_millis(300));
                }
            }
        }
    }

    fn finish(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SerialLogStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Concrete implementation of [`VmBackend`] that drives Lima via the `limactl` CLI.
pub struct LimaBackend {
    binary: String,
    serial_streams: Mutex<HashMap<String, SerialLogStream>>,
}

impl LimaBackend {
    pub fn new() -> Result<Self> {
        let binary = match which::which("limactl") {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => {
                eprintln!("📦 Lima not found. Installing lima...");
                Self::install_lima()?;
                which::which("limactl")
                    .context("limactl installation completed but binary not found in PATH")?
                    .to_string_lossy()
                    .to_string()
            }
        };

        Ok(Self {
            binary,
            serial_streams: Mutex::new(HashMap::new()),
        })
    }

    fn run_command(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new(&self.binary)
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute limactl {}", args.join(" ")))
    }

    fn run_command_checked(&self, args: &[&str]) -> Result<String> {
        let output = self.run_command(args)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(VmError::CommandFailed {
                command: format!("limactl {}", args.join(" ")),
                exit_code: output.status.code(),
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            }
            .into());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        check_stderr_for_errors(&stderr, &format!("limactl {}", args.join(" ")))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn run_command_passthrough(&self, args: &[&str]) -> Result<()> {
        let mut child = Command::new(&self.binary)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to execute limactl {}", args.join(" ")))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let stdout_thread = stdout.map(|mut out| {
            thread::spawn(move || -> io::Result<()> {
                let mut writer = io::stdout();
                io::copy(&mut out, &mut writer)?;
                writer.flush()
            })
        });

        let stderr_thread = stderr.map(|mut err_pipe| {
            thread::spawn(move || -> io::Result<()> {
                let mut writer = io::stderr();
                io::copy(&mut err_pipe, &mut writer)?;
                writer.flush()
            })
        });

        let status = child
            .wait()
            .with_context(|| format!("Failed to execute limactl {}", args.join(" ")))?;

        if let Some(handle) = stdout_thread {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => eprintln!(
                    "⚠️  Failed to stream limactl stdout ({}): {}",
                    args.join(" "),
                    err
                ),
                Err(_) => eprintln!(
                    "⚠️  limactl stdout stream panicked for command: {}",
                    args.join(" ")
                ),
            }
        }

        if let Some(handle) = stderr_thread {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => eprintln!(
                    "⚠️  Failed to stream limactl stderr ({}): {}",
                    args.join(" "),
                    err
                ),
                Err(_) => eprintln!(
                    "⚠️  limactl stderr stream panicked for command: {}",
                    args.join(" ")
                ),
            }
        }

        if !status.success() {
            let exit_code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            bail!(
                "limactl {} exited with status {}",
                args.join(" "),
                exit_code
            );
        }

        Ok(())
    }

    fn parse_vm_list(&self, raw: &str) -> Result<Vec<VmInfo>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("time=") {
            return Ok(Vec::new());
        }

        let mut vms = Vec::new();
        for line in trimmed.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let data: Value =
                serde_json::from_str(line).context("Failed to parse limactl list output")?;
            let name = data["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("VM missing name field"))?
                .to_string();

            let status = data["status"].as_str().unwrap_or("Unknown").to_string();
            let state = match status.as_str() {
                "Running" => "Running",
                "Stopped" => "Stopped",
                _ => &status,
            }
            .to_string();

            let ipv4 = data["addresses"]
                .as_array()
                .map_or_else(Vec::new, |addresses| {
                    addresses
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                });

            vms.push(VmInfo {
                name,
                state,
                ipv4,
                release: Some("Ubuntu 24.04".to_string()),
            });
        }

        Ok(vms)
    }

    fn serial_log_path(instance: &str) -> Result<PathBuf> {
        let base = if let Ok(lima_home) = env::var("LIMA_HOME") {
            PathBuf::from(lima_home)
        } else if let Ok(home) = env::var("HOME") {
            PathBuf::from(home).join(".lima")
        } else {
            anyhow::bail!(
                "Unable to determine Lima home directory; set the LIMA_HOME environment variable."
            );
        };

        Ok(base.join(instance).join("serial.log"))
    }
}

impl VmBackend for LimaBackend {
    fn name(&self) -> &str {
        "lima"
    }

    fn is_available(&self) -> bool {
        which::which("limactl").is_ok()
    }

    fn ensure_available(&self) -> Result<()> {
        if !self.is_available() {
            return Err(VmError::BackendNotAvailable {
                backend: "lima".to_string(),
            }
            .into());
        }

        let output = self.run_command(&["--version"])?;
        if !output.status.success() {
            bail!("lima is installed but not working properly");
        }

        Ok(())
    }

    fn create(&self, config: &VmConfig) -> Result<()> {
        validate_vm_config(config.cpus, &config.memory, &config.disk)?;

        if self.exists(&config.name)? {
            return Err(VmError::VmAlreadyExists {
                name: config.name.clone(),
            }
            .into());
        }

        let template_path = self.render_template_with_cloudinit(config)?;

        if config.stream_serial_logs {
            let instance = config.name.clone();
            match Self::serial_log_path(&instance)
                .and_then(|path| SerialLogStream::new(instance.clone(), path))
            {
                Ok(stream) => match self.serial_streams.lock() {
                    Ok(mut map) => {
                        map.insert(instance, stream);
                    }
                    Err(err) => {
                        eprintln!(
                            "⚠️  Unable to record Lima stream handle for '{}': {}",
                            config.name, err
                        );
                    }
                },
                Err(err) => {
                    eprintln!(
                        "⚠️  Unable to stream Lima logs for '{}': {}",
                        config.name, err
                    );
                }
            }
        }

        retry_operation(
            || {
                let cpus_str = config.cpus.to_string();
                let args = [
                    "start",
                    "--name",
                    &config.name,
                    "--tty=false",
                    "--set",
                    &format!(".cpus={}", cpus_str),
                    "--set",
                    &format!(".memory=\"{}\"", config.memory),
                    "--set",
                    &format!(".disk=\"{}\"", config.disk),
                    &template_path,
                ];

                if config.stream_serial_logs {
                    self.run_command_passthrough(&args)?;
                } else {
                    self.run_command_checked(&args)?;
                }

                Ok(())
            },
            RetryConfig::new(3),
            &format!("launch VM {}", config.name),
        )?;

        self.verify_state(&config.name, "Running")?;
        println!("✅ VM provisioned (cloud-init installed tracee binary)");
        Ok(())
    }

    fn start(&self, name: &str) -> Result<()> {
        retry_operation(
            || {
                self.run_command_checked(&["start", name])?;
                Ok(())
            },
            RetryConfig::new(3),
            &format!("start VM {}", name),
        )?;

        self.verify_state(name, "Running")?;
        Ok(())
    }

    fn stop(&self, name: &str) -> Result<()> {
        self.run_command_checked(&["stop", name])?;
        self.verify_state(name, "Stopped")?;
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        let _ = self.run_command(&["stop", name]);
        self.run_command_checked(&["delete", name])?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<VmInfo>> {
        let output = self.run_command_checked(&["list", "--json"])?;
        self.parse_vm_list(&output)
    }

    fn info(&self, name: &str) -> Result<VmInfo> {
        self.list()?
            .into_iter()
            .find(|vm| vm.name == name)
            .ok_or_else(|| {
                VmError::VmNotFound {
                    name: name.to_string(),
                }
                .into()
            })
    }

    fn exec(&self, name: &str, command: &[&str]) -> Result<String> {
        let mut args = vec!["shell", name];
        args.extend_from_slice(command);
        self.run_command_checked(&args)
    }

    fn shell(&self, name: &str, user: &str) -> Result<()> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("shell").arg(name);

        // default to agent account unless explicitly requesting root
        let target_user = if user.is_empty() { "agent" } else { user };
        cmd.args(["sudo", "-iu", target_user, "/bin/bash"]);

        let status = cmd.status().context("Failed to open shell")?;

        if !status.success() {
            bail!("Shell exited with status: {}", status);
        }

        Ok(())
    }

    fn copy_to_vm(&self, name: &str, host_path: &str, guest_path: &str) -> Result<()> {
        // Use limactl copy command
        self.run_command_checked(&["copy", host_path, &format!("{}:{}", name, guest_path)])?;
        Ok(())
    }

    fn stream_tracee_logs(&self, name: &str, raw: bool, no_color: bool) -> Result<()> {
        let log_file = if raw {
            "/var/log/tracee/events.jsonl"
        } else {
            "/var/log/tracee/events-readable.jsonl"
        };

        // If raw mode, just stream the file directly
        if raw {
            let mut cmd = Command::new(&self.binary);
            cmd.arg("shell").arg(name);
            cmd.args(["sudo", "tail", "-F", "--retry", log_file]);

            let status = cmd
                .status()
                .context("Failed to stream tracee logs from VM")?;

            if status.success() {
                return Ok(());
            }

            if let Some(code) = status.code() {
                if code == 130 {
                    return Ok(());
                }
                bail!("Tracee log stream exited with status code {code}");
            }

            bail!("Tracee log stream terminated by signal");
        }

        // Pretty-print mode: tail the readable logs and format them with colors
        let mut cmd = Command::new(&self.binary);
        cmd.arg("shell").arg(name);
        cmd.args(["sudo", "tail", "-F", "--retry", log_file]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());

        let mut child = cmd
            .spawn()
            .context("Failed to stream tracee logs from VM")?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to capture log stream stdout")?;

        let reader = BufReader::new(stdout);

        // ANSI color codes
        let (dim, green, red, blue, reset) = if no_color {
            ("", "", "", "", "")
        } else {
            ("\x1b[2m", "\x1b[32m", "\x1b[31m", "\x1b[34m", "\x1b[0m")
        };

        for line in reader.lines() {
            let line = line.context("Failed to read log line")?;

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Parse JSONL line
            match serde_json::from_str::<Value>(&line) {
                Ok(event) => {
                    let timestamp = event["timestamp"]
                        .as_str()
                        .and_then(|ts| ts.split('T').nth(1))
                        .and_then(|time| time.split('.').next())
                        .unwrap_or("??:??:??");

                    let user = event["user"].as_str().unwrap_or("unknown");
                    let description = event["description"].as_str().unwrap_or("(no description)");

                    // Choose user color
                    let user_color = if user == "agent" { green } else { red };

                    // Print formatted output
                    println!(
                        "{}{}{} {}[{}]{} {}{}{}",
                        dim, timestamp, reset, user_color, user, reset, blue, description, reset
                    );
                }
                Err(_) => {
                    // If parsing fails, print the raw line
                    println!("{}", line);
                }
            }
        }

        let status = child.wait().context("Failed to wait for log stream")?;

        if status.success() {
            return Ok(());
        }

        if let Some(code) = status.code() {
            if code == 130 {
                return Ok(());
            }
            bail!("Tracee log stream exited with status code {code}");
        }

        bail!("Tracee log stream terminated by signal");
    }

    fn wait_for_ready(&self, name: &str) -> Result<()> {
        retry_operation(
            || {
                self.exec(name, &["true"])?;
                Ok(())
            },
            RetryConfig::with_delays(10, Duration::from_secs(2), Duration::from_secs(10)),
            "wait for VM SSH",
        )?;

        health_check_vm(name)?;
        Ok(())
    }

    fn verify_state(&self, name: &str, expected_state: &str) -> Result<()> {
        let info = self.info(name)?;
        if info.state != expected_state {
            return Err(VmError::unexpected_state(name, expected_state, info.state).into());
        }
        Ok(())
    }

    fn finalize_serial_logs(&self, name: &str) -> Result<()> {
        match self.serial_streams.lock() {
            Ok(mut map) => {
                if let Some(stream) = map.remove(name) {
                    stream.finish();
                }
            }
            Err(err) => {
                eprintln!("⚠️  Failed to finalize Lima logs for '{}': {}", name, err);
            }
        }
        Ok(())
    }
}
