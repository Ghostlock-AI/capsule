//! Session directory management and metadata
//!
//! Creates session directories and manages session metadata for capsule runs.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub start_time: DateTime<Utc>,
    pub command_line: Vec<String>,
    pub session_dir: PathBuf,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    Completed,
    Failed(String),
}

pub struct SessionManager;

impl SessionManager {
    /// Create a new session directory and return metadata
    pub async fn create_session(command_line: Vec<String>) -> Result<SessionMetadata> {
        let start_time = Utc::now();
        let session_id = Self::generate_session_id(&start_time);
        let session_dir = Self::get_session_dir(&session_id)?;

        // Create the session directory
        fs::create_dir_all(&session_dir).await?;

        let metadata = SessionMetadata {
            session_id,
            start_time,
            command_line,
            session_dir,
            status: SessionStatus::Running,
        };

        // Write metadata file
        Self::write_metadata(&metadata).await?;

        Ok(metadata)
    }

    /// Update session status and write metadata
    pub async fn update_session_status(
        metadata: &mut SessionMetadata,
        status: SessionStatus,
    ) -> Result<()> {
        metadata.status = status;
        Self::write_metadata(metadata).await
    }

    /// Generate session ID from timestamp
    fn generate_session_id(start_time: &DateTime<Utc>) -> String {
        // Format: 2024-01-15T14:30:00Z-abc123
        let timestamp = start_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let random_suffix = Self::generate_random_suffix();
        format!("{}-{}", timestamp, random_suffix)
    }

    /// Generate random suffix for session ID
    fn generate_random_suffix() -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut hasher);
        format!("{:x}", hasher.finish()).chars().take(6).collect()
    }

    /// Get the base capsule directory
    fn get_capsule_dir() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;

        Ok(Path::new(&home).join(".capsule"))
    }

    /// Get session directory path
    fn get_session_dir(session_id: &str) -> Result<PathBuf> {
        Ok(Self::get_capsule_dir()?.join("runs").join(session_id))
    }

    /// Ensure base directories exist
    pub async fn ensure_base_directories() -> Result<()> {
        let capsule_dir = Self::get_capsule_dir()?;
        let runs_dir = capsule_dir.join("runs");

        fs::create_dir_all(&runs_dir).await?;

        Ok(())
    }

    /// Write metadata to session directory
    async fn write_metadata(metadata: &SessionMetadata) -> Result<()> {
        let metadata_path = metadata.session_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(metadata)?;

        fs::write(&metadata_path, metadata_json).await?;

        Ok(())
    }

    /// Get session directory as string for use in other modules
    pub fn session_dir_string(metadata: &SessionMetadata) -> String {
        metadata.session_dir.to_string_lossy().to_string()
    }

    /// List all sessions (for future use)
    pub async fn list_sessions() -> Result<Vec<String>> {
        let runs_dir = Self::get_capsule_dir()?.join("runs");

        if !runs_dir.exists() {
            return Ok(vec![]);
        }

        let mut sessions = vec![];
        let mut entries = fs::read_dir(&runs_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    sessions.push(name.to_string());
                }
            }
        }

        sessions.sort();
        Ok(sessions)
    }
}
