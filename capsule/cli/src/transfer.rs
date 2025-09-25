//! Transfer logic for moving capsule runs to database

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::database::Database;
use crate::session::SessionManager;

/// Run metadata from local storage
#[derive(Debug, Serialize, Deserialize)]
pub struct RunMetadata {
    pub session_id: String, // Keep as string, convert to UUID later
    pub start_time: DateTime<Utc>,
    pub command_line: Vec<String>,
    pub session_dir: PathBuf,
    pub status: String,
}

/// Syscall event data parsed from JSONL
#[derive(Debug)]
pub struct SyscallEventData {
    pub timestamp: u64,
    pub pid: u32,
    pub syscall_name: String,
    pub syscall_number: Option<i32>,
    pub args: Vec<String>,
    pub result: Option<String>,
    pub raw_line: String,
}

/// Transfer state tracking
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TransferState {
    pub last_transfer: Option<DateTime<Utc>>,
    pub transferred_runs: HashSet<String>,
}

impl TransferState {
    /// Load transfer state from disk
    pub fn load() -> Result<Self> {
        let state_file = SessionManager::base_dir().join("transfer_state.json");

        if !state_file.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&state_file)
            .context("Failed to read transfer state file")?;

        let state: Self = serde_json::from_str(&content)
            .context("Failed to parse transfer state")?;

        Ok(state)
    }

    /// Save transfer state to disk
    pub fn save(&self) -> Result<()> {
        let state_file = SessionManager::base_dir().join("transfer_state.json");

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize transfer state")?;

        fs::write(&state_file, content)
            .context("Failed to write transfer state file")?;

        Ok(())
    }

    /// Mark a run as transferred
    pub fn mark_transferred(&mut self, run_id: &str) {
        self.transferred_runs.insert(run_id.to_string());
        self.last_transfer = Some(Utc::now());
    }

    /// Check if a run has been transferred
    pub fn is_transferred(&self, run_id: &str) -> bool {
        self.transferred_runs.contains(run_id)
    }
}

/// Transfer runs to database
pub struct Transfer {
    database: Database,
    state: TransferState,
}

impl Transfer {
    /// Create a new transfer instance
    pub async fn new(database_url: &str) -> Result<Self> {
        let database = Database::connect(database_url).await?;
        let state = TransferState::load()
            .context("Failed to load transfer state")?;

        Ok(Self { database, state })
    }

    /// Transfer all untransferred runs
    pub async fn transfer_all(&mut self, dry_run: bool) -> Result<()> {
        let runs_dir = SessionManager::base_dir().join("runs");

        if !runs_dir.exists() {
            println!("📂 No runs directory found at: {}", runs_dir.display());
            return Ok(());
        }

        let mut run_dirs = Vec::new();
        for entry in fs::read_dir(&runs_dir)
            .context("Failed to read runs directory")?
        {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                run_dirs.push(entry.path());
            }
        }

        if run_dirs.is_empty() {
            println!("📂 No run directories found");
            return Ok(());
        }

        println!("🔍 Found {} run directories", run_dirs.len());

        let mut transferred = 0;
        let mut skipped = 0;
        let mut errors = 0;
        let total_runs = run_dirs.len();

        for run_dir in &run_dirs {
            let run_id = run_dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            // Skip if already transferred
            if self.state.is_transferred(run_id) {
                skipped += 1;
                continue;
            }

            // Skip if exists in database
            match self.database.run_exists(run_id).await {
                Ok(true) => {
                    println!("⏭️  Run {} already exists in database, marking as transferred", run_id);
                    self.state.mark_transferred(run_id);
                    skipped += 1;
                    continue;
                }
                Ok(false) => {
                    // Continue with transfer
                }
                Err(e) => {
                    eprintln!("❌ Error checking if run {} exists: {}", run_id, e);
                    errors += 1;
                    continue;
                }
            }

            if dry_run {
                println!("🔍 Would transfer run: {}", run_id);
                continue;
            }

            match self.transfer_single_run(&run_dir).await {
                Ok(()) => {
                    println!("✅ Transferred run: {}", run_id);
                    self.state.mark_transferred(run_id);
                    transferred += 1;
                }
                Err(e) => {
                    eprintln!("❌ Failed to transfer run {}: {}", run_id, e);
                    errors += 1;
                }
            }
        }

        if !dry_run {
            self.state.save()
                .context("Failed to save transfer state")?;
        }

        // Summary
        if dry_run {
            println!("\n📊 Dry run summary: {} would be transferred, {} already transferred",
                    total_runs - skipped, skipped);
        } else {
            println!("\n📊 Transfer summary: {} transferred, {} skipped, {} errors",
                    transferred, skipped, errors);
        }

        Ok(())
    }

    /// Transfer a specific run by ID
    pub async fn transfer_by_id(&mut self, run_id: &str, dry_run: bool) -> Result<()> {
        let run_dir = SessionManager::base_dir().join("runs").join(run_id);

        if !run_dir.exists() {
            anyhow::bail!("Run directory not found: {}", run_dir.display());
        }

        if self.state.is_transferred(run_id) {
            println!("⏭️  Run {} already transferred", run_id);
            return Ok(());
        }

        if dry_run {
            println!("🔍 Would transfer run: {}", run_id);
            return Ok(());
        }

        self.transfer_single_run(&run_dir).await
            .context("Failed to transfer run")?;

        self.state.mark_transferred(run_id);
        self.state.save()
            .context("Failed to save transfer state")?;

        println!("✅ Transferred run: {}", run_id);
        Ok(())
    }

    /// Transfer a single run directory
    async fn transfer_single_run(&self, run_dir: &Path) -> Result<()> {
        // Load metadata
        let metadata = self.load_run_metadata(run_dir)?;

        // Insert run metadata
        self.database.upsert_run_metadata(&metadata).await
            .context("Failed to insert run metadata")?;

        // Load and insert syscall events
        let events = self.load_syscall_events(run_dir)?;
        if !events.is_empty() {
            let inserted = self.database.insert_syscall_events(&metadata.session_id, &events).await
                .context("Failed to insert syscall events")?;

            println!("  📊 Inserted {} syscall events", inserted);
        }

        // Update statistics
        self.database.update_run_stats(&metadata.session_id).await
            .context("Failed to update run statistics")?;

        Ok(())
    }

    /// Load run metadata from directory
    fn load_run_metadata(&self, run_dir: &Path) -> Result<RunMetadata> {
        let metadata_file = run_dir.join("metadata.json");

        if !metadata_file.exists() {
            anyhow::bail!("Metadata file not found: {}", metadata_file.display());
        }

        let content = fs::read_to_string(&metadata_file)
            .context("Failed to read metadata file")?;

        let metadata: RunMetadata = serde_json::from_str(&content)
            .context("Failed to parse metadata JSON")?;

        Ok(metadata)
    }

    /// Load syscall events from JSONL files
    fn load_syscall_events(&self, run_dir: &Path) -> Result<Vec<SyscallEventData>> {
        let mut events = Vec::new();

        // Try syscalls.jsonl first (most common)
        let syscalls_file = run_dir.join("syscalls.jsonl");
        if syscalls_file.exists() {
            events.extend(self.parse_jsonl_file(&syscalls_file)?);
        }

        // Try events.jsonl as backup
        if events.is_empty() {
            let events_file = run_dir.join("events.jsonl");
            if events_file.exists() {
                events.extend(self.parse_jsonl_file(&events_file)?);
            }
        }

        Ok(events)
    }

    /// Parse a JSONL file containing syscall events
    fn parse_jsonl_file(&self, file_path: &Path) -> Result<Vec<SyscallEventData>> {
        let content = fs::read_to_string(file_path)
            .context("Failed to read JSONL file")?;

        let mut events = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            // Skip header lines
            if line.contains("\"session\":") || line.contains("\"start\":") {
                continue;
            }

            match self.parse_syscall_event_line(line) {
                Ok(event) => events.push(event),
                Err(e) => {
                    // Log but continue with other lines
                    eprintln!("Warning: Failed to parse line {} in {}: {}",
                            line_num + 1, file_path.display(), e);
                }
            }
        }

        Ok(events)
    }

    /// Parse a single line of syscall event data
    fn parse_syscall_event_line(&self, line: &str) -> Result<SyscallEventData> {
        // Try to parse as SyscallEvent from core crate first
        if let Ok(core_event) = serde_json::from_str::<core::SyscallEvent>(line) {
            return Ok(SyscallEventData {
                timestamp: core_event.timestamp,
                pid: core_event.pid,
                syscall_name: core_event.syscall_name,
                syscall_number: core_event.syscall_number,
                args: core_event.args,
                result: core_event.result,
                raw_line: core_event.raw_line,
            });
        }

        // Fallback: try to parse as generic JSON and extract fields
        let value: serde_json::Value = serde_json::from_str(line)
            .context("Failed to parse line as JSON")?;

        let timestamp = value.get("timestamp")
            .and_then(|v| v.as_u64())
            .or_else(|| value.get("ts").and_then(|v| v.as_u64()))
            .context("Missing timestamp field")?;

        let pid = value.get("pid")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .context("Missing pid field")?;

        let syscall_name = value.get("syscall_name")
            .or_else(|| value.get("call"))
            .or_else(|| value.get("syscall"))
            .and_then(|v| v.as_str())
            .context("Missing syscall name field")?
            .to_string();

        let syscall_number = value.get("syscall_number")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);

        let args = value.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();

        let result = value.get("result")
            .or_else(|| value.get("retval"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let raw_line = value.get("raw_line")
            .or_else(|| value.get("raw"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| line.to_string());

        Ok(SyscallEventData {
            timestamp,
            pid,
            syscall_name,
            syscall_number,
            args,
            result,
            raw_line,
        })
    }
}