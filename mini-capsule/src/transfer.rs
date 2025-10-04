use anyhow::{Context, Result};
use serde_json::json;
use std::path::Path;
use tokio::fs;

use crate::config::SupaConfig;
use crate::models::{RawSyscall, SessionMetadata};

pub struct SupabaseTransfer {
    config: SupaConfig,
    client: reqwest::Client,
}

impl SupabaseTransfer {
    pub fn new(config: SupaConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Transfer a session directory to Supabase
    pub async fn transfer_session(&self, session_dir: &Path) -> Result<uuid::Uuid> {
        println!("Transferring session: {:?}", session_dir);

        // 1. Read session metadata
        let metadata_path = session_dir.join("session_metadata.json");
        let metadata_json = fs::read_to_string(&metadata_path).await
            .context("Failed to read session_metadata.json")?;
        let metadata: SessionMetadata = serde_json::from_str(&metadata_json)
            .context("Failed to parse session_metadata.json")?;

        // 2. Read structured syscalls
        let syscalls_path = session_dir.join("structured_syscalls.jsonl");
        let syscalls_content = fs::read_to_string(&syscalls_path).await
            .context("Failed to read structured_syscalls.jsonl")?;

        let (syscalls, failed_count, errors) = parse_jsonl_syscalls(&syscalls_content);

        if failed_count > 0 {
            for error in &errors {
                eprintln!("{}", error);
            }
            anyhow::bail!(
                "Failed to parse {} lines in structured_syscalls.jsonl",
                failed_count
            );
        }

        // 3. Count lines in files for statistics
        let raw_trace_path = session_dir.join("raw_trace.txt");
        let failed_parse_path = session_dir.join("failed_parse_raw.txt");

        let total_syscalls = count_lines(&raw_trace_path).await.unwrap_or(0);
        let failed_parses = count_lines(&failed_parse_path).await.unwrap_or(0);
        let parsed_syscalls = syscalls.len();

        // 4. Insert session record
        let session_id = self.insert_session(
            &metadata,
            total_syscalls,
            parsed_syscalls,
            failed_parses,
            session_dir,
        ).await?;

        println!("Created session record: {}", session_id);
        println!("Parsed {} syscalls from JSONL", syscalls.len());

        // 5. Skip file uploads for now (storage not configured in local setup)
        // TODO: Re-enable when storage is configured
        // self.upload_file(&session_id, &raw_trace_path, "raw_trace.txt").await?;
        // self.upload_file(&session_id, &failed_parse_path, "failed_parse_raw.txt").await?;

        // 6. Batch insert syscalls
        let expected_count = syscalls.len();
        println!("Starting batch insert of {} syscalls...", expected_count);
        let inserted_count = self.insert_syscalls(&session_id, &syscalls).await?;

        // 7. Validate all syscalls were inserted
        if inserted_count != expected_count {
            eprintln!(
                "ERROR: Expected to insert {} syscalls but only inserted {}",
                expected_count, inserted_count
            );
            // Rollback: delete the session (cascades to syscalls)
            self.delete_session(&session_id).await?;
            anyhow::bail!(
                "Transfer failed: only {}/{} syscalls inserted, session rolled back",
                inserted_count,
                expected_count
            );
        }

        println!("Inserted {} syscalls", inserted_count);

        // 8. Update session with storage paths
        self.update_session_paths(
            &session_id,
            &format!("{}/raw_trace.txt", session_id),
            &format!("{}/failed_parse_raw.txt", session_id),
        ).await?;

        println!("Transfer complete!");

        Ok(session_id)
    }

    /// Insert session record into database
    async fn insert_session(
        &self,
        metadata: &SessionMetadata,
        total_syscalls: usize,
        parsed_syscalls: usize,
        failed_parses: usize,
        session_dir: &Path,
    ) -> Result<uuid::Uuid> {
        let url = format!("{}/rest/v1/sessions", self.config.supabase.url);

        let payload = json!({
            "id": metadata.uuid,
            "timestamp": metadata.timestamp,
            "end_timestamp": metadata.end_timestamp,
            "os": metadata.os,
            "chipset": metadata.chipset,
            "working_dir": metadata.working_dir,
            "program": metadata.program,
            "args": metadata.args,
            "total_syscalls": total_syscalls,
            "parsed_syscalls": parsed_syscalls,
            "failed_parses": failed_parses,
            "local_session_dir": session_dir.to_string_lossy().to_string(),
        });

        let response = self.client.post(&url)
            .header("apikey", &self.config.supabase.service_key)
            .header("Authorization", format!("Bearer {}", self.config.supabase.service_key))
            .header("Content-Type", "application/json")
            .header("Prefer", "return=representation")
            .json(&payload)
            .send()
            .await
            .context("Failed to insert session")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to insert session: {}", error_text);
        }

        Ok(metadata.uuid)
    }

    /// Upload file to Supabase Storage
    async fn upload_file(
        &self,
        session_id: &uuid::Uuid,
        file_path: &Path,
        file_name: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/storage/v1/object/trace-files/{}/{}",
            self.config.supabase.url,
            session_id,
            file_name
        );

        let file_bytes = fs::read(file_path).await
            .context("Failed to read file")?;

        let response = self.client.post(&url)
            .header("apikey", &self.config.supabase.service_key)
            .header("Authorization", format!("Bearer {}", self.config.supabase.service_key))
            .header("Content-Type", "text/plain")
            .body(file_bytes)
            .send()
            .await
            .context("Failed to upload file")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to upload file {}: {}", file_name, error_text);
        }

        Ok(())
    }

    /// Batch insert syscalls and return count of successfully inserted records
    async fn insert_syscalls(
        &self,
        session_id: &uuid::Uuid,
        syscalls: &[RawSyscall],
    ) -> Result<usize> {
        let url = format!("{}/rest/v1/syscalls", self.config.supabase.url);
        let batch_size = self.config.transfer.batch_size;
        let mut total_inserted = 0;

        println!("Inserting syscalls in batches of {}", batch_size);

        for (batch_num, chunk) in syscalls.chunks(batch_size).enumerate() {
            let payload: Vec<_> = chunk.iter().map(|sc| {
                json!({
                    "session_id": session_id,
                    "timestamp": sc.timestamp,
                    "pid": sc.pid,
                    "syscall_number": sc.syscall_number,
                    "syscall_name": sc.syscall_name,
                    "raw_args": sc.raw_args,
                    "raw_return": sc.raw_return,
                    "category": format!("{:?}", sc.category),
                })
            }).collect();

            println!("Batch {}: Sending {} syscalls...", batch_num + 1, payload.len());

            let response = self.client.post(&url)
                .header("apikey", &self.config.supabase.service_key)
                .header("Authorization", format!("Bearer {}", self.config.supabase.service_key))
                .header("Content-Type", "application/json")
                .header("Prefer", "return=representation")
                .json(&payload)
                .send()
                .await
                .context("Failed to insert syscalls batch")?;

            let status = response.status();
            println!("Batch {}: Response status: {}", batch_num + 1, status);

            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_default();
                eprintln!("Error response: {}", error_text);
                anyhow::bail!("Failed to insert syscalls batch {}: HTTP {}", batch_num + 1, status);
            }

            // Parse response to count inserted records
            let response_text = response.text().await?;
            println!("Batch {}: Response body length: {} bytes", batch_num + 1, response_text.len());

            let inserted: Vec<serde_json::Value> = serde_json::from_str(&response_text)
                .context(format!("Failed to parse insert response: {}", response_text))?;

            println!("Batch {}: Inserted {} records", batch_num + 1, inserted.len());
            total_inserted += inserted.len();
        }

        Ok(total_inserted)
    }

    /// Delete session (used for rollback)
    async fn delete_session(&self, session_id: &uuid::Uuid) -> Result<()> {
        let url = format!(
            "{}/rest/v1/sessions?id=eq.{}",
            self.config.supabase.url,
            session_id
        );

        let response = self.client.delete(&url)
            .header("apikey", &self.config.supabase.service_key)
            .header("Authorization", format!("Bearer {}", self.config.supabase.service_key))
            .send()
            .await
            .context("Failed to delete session")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            eprintln!("Warning: Failed to rollback session: {}", error_text);
        }

        Ok(())
    }

    /// Update session with storage paths
    async fn update_session_paths(
        &self,
        session_id: &uuid::Uuid,
        raw_trace_path: &str,
        failed_parse_path: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/rest/v1/sessions?id=eq.{}",
            self.config.supabase.url,
            session_id
        );

        let payload = json!({
            "raw_trace_path": raw_trace_path,
            "failed_parse_path": failed_parse_path,
        });

        let response = self.client.patch(&url)
            .header("apikey", &self.config.supabase.service_key)
            .header("Authorization", format!("Bearer {}", self.config.supabase.service_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .context("Failed to update session")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to update session paths: {}", error_text);
        }

        Ok(())
    }
}

/// Count lines in a file
async fn count_lines(path: &Path) -> Result<usize> {
    let content = fs::read_to_string(path).await?;
    Ok(content.lines().count())
}

/// Check if a session has already been transferred
pub async fn is_session_transferred(
    config: &SupaConfig,
    session_id: &uuid::Uuid,
) -> Result<bool> {
    let url = format!(
        "{}/rest/v1/sessions?id=eq.{}",
        config.supabase.url,
        session_id
    );

    let client = reqwest::Client::new();
    let response = client.get(&url)
        .header("apikey", &config.supabase.service_key)
        .header("Authorization", format!("Bearer {}", config.supabase.service_key))
        .send()
        .await?;

    let sessions: Vec<serde_json::Value> = response.json().await?;
    Ok(!sessions.is_empty())
}

/// Serialize syscalls to JSONL format
/// Returns JSONL string with one syscall per line
pub fn write_jsonl_syscalls(syscalls: &[RawSyscall]) -> Result<String> {
    let mut lines = Vec::new();
    for syscall in syscalls {
        let json = serde_json::to_string(syscall)
            .context("Failed to serialize syscall to JSON")?;
        lines.push(json);
    }
    Ok(lines.join("\n"))
}

/// Parse JSONL content into RawSyscall structs
/// Returns (parsed_syscalls, failed_count, error_messages)
pub fn parse_jsonl_syscalls(jsonl_content: &str) -> (Vec<RawSyscall>, usize, Vec<String>) {
    let total_lines = jsonl_content.lines().count();
    let mut syscalls: Vec<RawSyscall> = Vec::new();
    let mut failed_count = 0;
    let mut errors = Vec::new();

    for (i, line) in jsonl_content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<RawSyscall>(line) {
            Ok(syscall) => syscalls.push(syscall),
            Err(e) => {
                errors.push(format!("Line {}: {} | Content: {}", i + 1, e, line));
                failed_count += 1;
            }
        }
    }

    (syscalls, failed_count, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SyscallCategory;

    fn create_test_syscall(
        timestamp: &str,
        pid: Option<u32>,
        syscall_number: u32,
        syscall_name: &str,
        category: SyscallCategory,
    ) -> RawSyscall {
        RawSyscall {
            timestamp: timestamp.to_string(),
            pid,
            syscall_number,
            syscall_name: syscall_name.to_string(),
            raw_args: vec![],
            raw_return: "0".to_string(),
            category,
        }
    }

    #[test]
    fn test_write_jsonl_syscalls() {
        let syscalls = vec![
            create_test_syscall("00:00:01", Some(123), 1, "write", SyscallCategory::File),
            create_test_syscall("00:00:02", Some(124), 2, "read", SyscallCategory::File),
            create_test_syscall("00:00:03", None, 3, "socket", SyscallCategory::Network),
        ];

        let jsonl = write_jsonl_syscalls(&syscalls).unwrap();
        let lines: Vec<&str> = jsonl.lines().collect();

        assert_eq!(lines.len(), 3, "Should have 3 lines of JSONL");
        assert!(lines[0].contains("\"syscall_name\":\"write\""));
        assert!(lines[1].contains("\"syscall_name\":\"read\""));
        assert!(lines[2].contains("\"syscall_name\":\"socket\""));
        assert!(lines[2].contains("\"pid\":null"));
    }

    #[test]
    fn test_write_then_parse_roundtrip() {
        let original_syscalls = vec![
            create_test_syscall("00:00:01", Some(123), 1, "write", SyscallCategory::File),
            create_test_syscall("00:00:02", Some(124), 2, "read", SyscallCategory::File),
            create_test_syscall("00:00:03", None, 3, "socket", SyscallCategory::Network),
            create_test_syscall("00:00:04", Some(125), 4, "execve", SyscallCategory::Process),
        ];

        // Write syscalls to JSONL
        let jsonl = write_jsonl_syscalls(&original_syscalls).unwrap();

        // Parse them back
        let (parsed_syscalls, failed_count, errors) = parse_jsonl_syscalls(&jsonl);

        // Verify roundtrip worked perfectly
        assert_eq!(failed_count, 0, "Should have no parse failures");
        assert_eq!(errors.len(), 0, "Should have no errors");
        assert_eq!(
            parsed_syscalls.len(),
            original_syscalls.len(),
            "Should parse same number of syscalls"
        );

        for (original, parsed) in original_syscalls.iter().zip(parsed_syscalls.iter()) {
            assert_eq!(parsed.timestamp, original.timestamp);
            assert_eq!(parsed.pid, original.pid);
            assert_eq!(parsed.syscall_number, original.syscall_number);
            assert_eq!(parsed.syscall_name, original.syscall_name);
            assert_eq!(parsed.category, original.category);
        }
    }

    #[test]
    fn test_write_read_all_categories() {
        let syscalls = vec![
            create_test_syscall("00:00:01", Some(1), 1, "test1", SyscallCategory::Process),
            create_test_syscall("00:00:02", Some(2), 2, "test2", SyscallCategory::File),
            create_test_syscall("00:00:03", Some(3), 3, "test3", SyscallCategory::Network),
            create_test_syscall("00:00:04", Some(4), 4, "test4", SyscallCategory::Unknown),
        ];

        let jsonl = write_jsonl_syscalls(&syscalls).unwrap();
        let (parsed, failed, _) = parse_jsonl_syscalls(&jsonl);

        assert_eq!(failed, 0);
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].category, SyscallCategory::Process);
        assert_eq!(parsed[1].category, SyscallCategory::File);
        assert_eq!(parsed[2].category, SyscallCategory::Network);
        assert_eq!(parsed[3].category, SyscallCategory::Unknown);
    }

    #[test]
    fn test_write_read_with_complex_args() {
        let syscall = RawSyscall {
            timestamp: "00:05:14.247211".to_string(),
            pid: None,
            syscall_number: 221,
            syscall_name: "execve".to_string(),
            raw_args: vec![
                "\"/usr/bin/claude\"".to_string(),
                "[\"claude\"]".to_string(),
                "[\"ENV=value\"]".to_string(),
            ],
            raw_return: "0".to_string(),
            category: SyscallCategory::Process,
        };

        let jsonl = write_jsonl_syscalls(&[syscall.clone()]).unwrap();
        let (parsed, failed, _) = parse_jsonl_syscalls(&jsonl);

        assert_eq!(failed, 0);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].raw_args, syscall.raw_args);
        assert_eq!(parsed[0].raw_args.len(), 3);
    }

    #[test]
    fn test_write_empty_syscalls_list() {
        let syscalls: Vec<RawSyscall> = vec![];
        let jsonl = write_jsonl_syscalls(&syscalls).unwrap();
        assert_eq!(jsonl, "", "Empty list should produce empty string");

        let (parsed, failed, _) = parse_jsonl_syscalls(&jsonl);
        assert_eq!(parsed.len(), 0);
        assert_eq!(failed, 0);
    }

    #[test]
    fn test_write_read_count_consistency() {
        // Create a large batch of syscalls
        let mut syscalls = Vec::new();
        for i in 0..100 {
            syscalls.push(create_test_syscall(
                &format!("00:00:{:02}", i),
                Some(i),
                i,
                &format!("syscall_{}", i),
                SyscallCategory::File,
            ));
        }

        // Write to JSONL
        let jsonl = write_jsonl_syscalls(&syscalls).unwrap();

        // Parse back
        let (parsed, failed, _) = parse_jsonl_syscalls(&jsonl);

        // Verify counts
        assert_eq!(failed, 0, "Should have zero failures");
        assert_eq!(
            parsed.len(),
            100,
            "Should parse exactly 100 syscalls"
        );
        assert_eq!(
            parsed.len(),
            syscalls.len(),
            "Parsed count must equal written count"
        );
    }

    #[test]
    fn test_parse_valid_jsonl() {
        let jsonl = r#"{"timestamp":"00:05:14.247211","pid":null,"syscall_number":221,"syscall_name":"execve","raw_args":["\"/usr/bin/claude\"","[\"claude\"]"],"raw_return":"0","category":"Process"}
{"timestamp":"00:05:14.247818","pid":null,"syscall_number":48,"syscall_name":"faccessat","raw_args":["AT_FDCWD","\"/etc/ld.so.preload\"","R_OK"],"raw_return":"-1 ENOENT (No such file or directory)","category":"File"}
{"timestamp":"00:05:14.247976","pid":123,"syscall_number":56,"syscall_name":"openat","raw_args":["AT_FDCWD","\"/etc/ld.so.cache\"","O_RDONLY|O_CLOEXEC"],"raw_return":"3</etc/ld.so.cache>","category":"File"}"#;

        let (syscalls, failed_count, errors) = parse_jsonl_syscalls(jsonl);

        assert_eq!(syscalls.len(), 3, "Should parse 3 syscalls");
        assert_eq!(failed_count, 0, "Should have no failures");
        assert_eq!(errors.len(), 0, "Should have no errors");

        assert_eq!(syscalls[0].syscall_name, "execve");
        assert_eq!(syscalls[0].category, SyscallCategory::Process);
        assert_eq!(syscalls[1].syscall_name, "faccessat");
        assert_eq!(syscalls[1].category, SyscallCategory::File);
        assert_eq!(syscalls[2].pid, Some(123));
    }

    #[test]
    fn test_parse_jsonl_with_empty_lines() {
        let jsonl = r#"{"timestamp":"00:00:01","pid":123,"syscall_number":1,"syscall_name":"write","raw_args":[],"raw_return":"0","category":"File"}

{"timestamp":"00:00:02","pid":124,"syscall_number":2,"syscall_name":"read","raw_args":[],"raw_return":"0","category":"File"}

{"timestamp":"00:00:03","pid":125,"syscall_number":3,"syscall_name":"socket","raw_args":[],"raw_return":"0","category":"Network"}"#;

        let (syscalls, failed_count, _) = parse_jsonl_syscalls(jsonl);

        assert_eq!(syscalls.len(), 3);
        assert_eq!(failed_count, 0);
    }

    #[test]
    fn test_parse_invalid_jsonl() {
        let jsonl = r#"{"timestamp":"00:00:01","pid":123,"syscall_number":1,"syscall_name":"write","raw_args":[],"raw_return":"0","category":"File"}
this is not valid json
{"timestamp":"00:00:03","pid":125,"syscall_number":3,"syscall_name":"socket","raw_args":[],"raw_return":"0","category":"Network"}"#;

        let (syscalls, failed_count, errors) = parse_jsonl_syscalls(jsonl);

        assert_eq!(syscalls.len(), 2, "Should parse 2 valid lines");
        assert_eq!(failed_count, 1, "Should have 1 failure");
        assert_eq!(errors.len(), 1, "Should have 1 error");
        assert!(errors[0].contains("Line 2"));
    }

    #[test]
    fn test_parse_missing_category_field() {
        let jsonl = r#"{"timestamp":"00:00:01","pid":123,"syscall_number":1,"syscall_name":"write","raw_args":[],"raw_return":"0"}"#;

        let (syscalls, failed_count, errors) = parse_jsonl_syscalls(jsonl);

        assert_eq!(syscalls.len(), 0, "Should fail to parse without category");
        assert_eq!(failed_count, 1);
        assert!(errors[0].contains("missing field"));
    }

    #[test]
    fn test_parse_all_categories() {
        let jsonl = r#"{"timestamp":"00:00:01","pid":1,"syscall_number":1,"syscall_name":"test1","raw_args":[],"raw_return":"0","category":"Process"}
{"timestamp":"00:00:02","pid":2,"syscall_number":2,"syscall_name":"test2","raw_args":[],"raw_return":"0","category":"File"}
{"timestamp":"00:00:03","pid":3,"syscall_number":3,"syscall_name":"test3","raw_args":[],"raw_return":"0","category":"Network"}
{"timestamp":"00:00:04","pid":4,"syscall_number":4,"syscall_name":"test4","raw_args":[],"raw_return":"0","category":"Unknown"}"#;

        let (syscalls, failed_count, _) = parse_jsonl_syscalls(jsonl);

        assert_eq!(syscalls.len(), 4);
        assert_eq!(failed_count, 0);
        assert_eq!(syscalls[0].category, SyscallCategory::Process);
        assert_eq!(syscalls[1].category, SyscallCategory::File);
        assert_eq!(syscalls[2].category, SyscallCategory::Network);
        assert_eq!(syscalls[3].category, SyscallCategory::Unknown);
    }

    #[test]
    fn test_parse_expected_vs_actual_count() {
        let jsonl = r#"{"timestamp":"00:00:01","pid":1,"syscall_number":1,"syscall_name":"test1","raw_args":[],"raw_return":"0","category":"Process"}
{"timestamp":"00:00:02","pid":2,"syscall_number":2,"syscall_name":"test2","raw_args":[],"raw_return":"0","category":"File"}
invalid line here
{"timestamp":"00:00:04","pid":4,"syscall_number":4,"syscall_name":"test4","raw_args":[],"raw_return":"0","category":"Network"}"#;

        let total_lines = jsonl.lines().count();
        let (syscalls, failed_count, _) = parse_jsonl_syscalls(jsonl);

        // We expect 4 lines total, but only 3 should parse successfully
        assert_eq!(total_lines, 4);
        assert_eq!(syscalls.len(), 3);
        assert_eq!(failed_count, 1);

        // This is the validation we want: parsed + failed should equal non-empty lines
        let non_empty_lines = jsonl.lines().filter(|l| !l.trim().is_empty()).count();
        assert_eq!(syscalls.len() + failed_count, non_empty_lines);
    }
}
