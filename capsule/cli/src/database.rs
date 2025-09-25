//! Database connection and operations

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::{Client, NoTls};

use crate::transfer::{RunMetadata, SyscallEventData};

/// Database configuration and connection management
pub struct Database {
    client: Client,
}

impl Database {
    /// Connect to the database
    pub async fn connect(database_url: &str) -> Result<Self> {
        println!("🔌 Connecting to database: {}", database_url);

        let (client, connection) = tokio_postgres::connect(database_url, NoTls)
            .await
            .context("Failed to connect to database")?;

        // Handle the connection in a background task
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Database connection error: {}", e);
            }
        });

        // Test the connection
        client
            .simple_query("SELECT 1")
            .await
            .context("Failed to test database connection")?;

        println!("✅ Database connection established");

        Ok(Self { client })
    }

    /// Check if a run already exists in the database
    pub async fn run_exists(&self, run_id: &str) -> Result<bool> {
        let row = self
            .client
            .query_one("SELECT EXISTS(SELECT 1 FROM runs WHERE id = $1)", &[&run_id])
            .await
            .context("Failed to check if run exists")?;

        Ok(row.get(0))
    }

    /// Insert or update run metadata
    pub async fn upsert_run_metadata(&self, metadata: &RunMetadata) -> Result<()> {
        let query = r#"
            INSERT INTO runs (
                id, command_line, working_directory, start_time,
                log_directory, agent_type, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                command_line = EXCLUDED.command_line,
                working_directory = EXCLUDED.working_directory,
                start_time = EXCLUDED.start_time,
                log_directory = EXCLUDED.log_directory,
                agent_type = EXCLUDED.agent_type
        "#;

        // Convert command_line vec to string
        let command_string = metadata.command_line.join(" ");

        // Infer agent type from command
        let agent_type = infer_agent_type(&metadata.command_line);

        self.client
            .execute(
                query,
                &[
                    &metadata.session_id,
                    &command_string,
                    &metadata.session_dir.to_string_lossy().to_string(),
                    &metadata.start_time,
                    &metadata.session_dir.to_string_lossy().to_string(),
                    &agent_type,
                    &Utc::now(),
                ],
            )
            .await
            .context("Failed to insert run metadata")?;

        Ok(())
    }

    /// Insert syscall events in batch
    pub async fn insert_syscall_events(&self, run_id: &str, events: &[SyscallEventData]) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }

        let query = r#"
            INSERT INTO syscall_events (
                run_id, timestamp_us, pid, syscall, args, return_value, raw_line,
                tid, ppid, exe_path, cwd, argv, uid, gid, euid, egid, capabilities,
                fd, abs_path, fd_map, resource_type, operation,
                permission_bits, byte_count, latency_us, network_info, risk_tags, high_level_kind
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                $18, $19, $20, $21, $22,
                $23, $24, $25, $26, $27, $28
            )
        "#;

        let mut inserted = 0;

        for event in events {
            // Convert args to BIGINT array
            let args_array: Vec<Option<i64>> = event.args.iter()
                .map(|arg| arg.parse::<i64>().ok())
                .collect();

            // Pad to exactly 6 elements
            let mut padded_args = args_array;
            padded_args.resize(6, None);

            // Convert network info to JSON if present
            let network_json: Option<Value> = None; // TODO: implement network parsing

            // Convert fd_map to JSON if present
            let fd_map_json: Option<Value> = None; // TODO: implement fd_map parsing

            // Convert risk tags
            let risk_tags: Vec<&str> = Vec::new(); // TODO: implement risk tag parsing

            match self.client.execute(
                query,
                &[
                    &run_id,                                                    // $1
                    &(event.timestamp as i64),                                 // $2
                    &(event.pid as i32),                                       // $3
                    &event.syscall_name,                                       // $4
                    &padded_args,                                             // $5
                    &event.result.as_ref().and_then(|r| r.parse::<i64>().ok()), // $6
                    &event.raw_line,                                          // $7
                    &None::<i32>,                                             // $8 tid
                    &None::<i32>,                                             // $9 ppid
                    &None::<String>,                                          // $10 exe_path
                    &None::<String>,                                          // $11 cwd
                    &None::<Vec<String>>,                                     // $12 argv
                    &None::<i32>,                                             // $13 uid
                    &None::<i32>,                                             // $14 gid
                    &None::<i32>,                                             // $15 euid
                    &None::<i32>,                                             // $16 egid
                    &None::<i64>,                                             // $17 capabilities
                    &None::<i32>,                                             // $18 fd
                    &None::<String>,                                          // $19 abs_path
                    &fd_map_json,                                             // $20
                    &None::<String>,                                          // $21 resource_type
                    &None::<String>,                                          // $22 operation
                    &None::<i32>,                                             // $23 permission_bits
                    &None::<i64>,                                             // $24 byte_count
                    &None::<i64>,                                             // $25 latency_us
                    &network_json,                                            // $26
                    &risk_tags,                                               // $27
                    &None::<String>,                                          // $28 high_level_kind
                ]
            ).await {
                Ok(_) => inserted += 1,
                Err(e) => {
                    eprintln!("Warning: Failed to insert syscall event: {}", e);
                    // Continue with other events
                }
            }
        }

        Ok(inserted)
    }

    /// Update run statistics after inserting events
    pub async fn update_run_stats(&self, _run_id: &str) -> Result<()> {
        // Since we changed to TEXT, we need to update the function call
        // For now, just skip the stats update as the function expects UUID
        println!("  📈 Skipping statistics update (function needs UUID)");
        Ok(())
    }

    /// Get run statistics for display
    pub async fn get_run_stats(&self, run_id: &str) -> Result<RunStats> {
        let row = self
            .client
            .query_one(
                r#"
                SELECT
                    total_syscalls,
                    total_risk_events,
                    total_network_events,
                    total_file_operations
                FROM runs WHERE id = $1
                "#,
                &[&run_id],
            )
            .await
            .context("Failed to get run statistics")?;

        Ok(RunStats {
            total_syscalls: row.get(0),
            total_risk_events: row.get(1),
            total_network_events: row.get(2),
            total_file_operations: row.get(3),
        })
    }

    /// List all runs in the database
    pub async fn list_runs(&self) -> Result<Vec<RunSummary>> {
        let rows = self
            .client
            .query(
                r#"
                SELECT
                    id,
                    command_line,
                    start_time,
                    duration_ms,
                    agent_type,
                    total_syscalls
                FROM recent_runs
                ORDER BY start_time DESC
                LIMIT 10
                "#,
                &[],
            )
            .await
            .context("Failed to list runs")?;

        let mut runs = Vec::new();
        for row in rows {
            runs.push(RunSummary {
                id: row.get(0),
                command_line: row.get(1),
                start_time: row.get(2),
                duration_ms: row.get(3),
                agent_type: row.get(4),
                total_syscalls: row.get(5),
            });
        }

        Ok(runs)
    }
}

/// Run statistics from database
#[derive(Debug)]
pub struct RunStats {
    pub total_syscalls: i32,
    pub total_risk_events: i32,
    pub total_network_events: i32,
    pub total_file_operations: i32,
}

/// Run summary for listing
#[derive(Debug)]
pub struct RunSummary {
    pub id: String,
    pub command_line: String,
    pub start_time: DateTime<Utc>,
    pub duration_ms: Option<i64>,
    pub agent_type: Option<String>,
    pub total_syscalls: i32,
}

/// Infer agent type from command line arguments
fn infer_agent_type(command_line: &[String]) -> String {
    if command_line.is_empty() {
        return "unknown".to_string();
    }

    let program = &command_line[0].to_lowercase();

    if program.contains("claude") {
        "claude".to_string()
    } else if program.contains("cursor") {
        "cursor".to_string()
    } else if program.contains("python") || program.contains("py") {
        "python".to_string()
    } else if program.contains("node") || program.contains("npm") {
        "nodejs".to_string()
    } else if program.contains("rust") || program.contains("cargo") {
        "rust".to_string()
    } else {
        "system".to_string()
    }
}