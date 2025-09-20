//! Inter-process communication for live session monitoring
//!
//! Manages session lock files and Unix domain sockets for sharing
//! live AgentState between `capsule run` and `capsule monitor`

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use state::AgentState;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Active session information stored in lock file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLock {
    /// Session ID
    pub session_id: String,
    /// Process ID of the running capsule session
    pub pid: u32,
    /// Command being traced
    pub command: Vec<String>,
    /// When the session started
    pub started_at: String,
    /// Path to Unix domain socket for state communication
    pub socket_path: PathBuf,
}

/// Manages the active session lock file
pub struct SessionLockManager;

impl SessionLockManager {
    /// Path to the global active session lock file
    pub fn lock_file_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".capsule")
            .join("ACTIVE_SESSION")
    }

    /// Create a session lock file for the current process
    pub async fn create_lock(session_id: String, command: Vec<String>) -> Result<SessionLock> {
        let lock_path = Self::lock_file_path();

        // Ensure parent directory exists
        if let Some(parent) = lock_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Check if there's already an active session
        if let Ok(existing_lock) = Self::read_lock().await {
            if Self::is_process_running(existing_lock.pid) {
                return Err(anyhow!(
                    "Active session already running: {} (PID {})",
                    existing_lock.session_id,
                    existing_lock.pid
                ));
            } else {
                // Stale lock file, remove it
                let _ = tokio::fs::remove_file(&lock_path).await;
            }
        }

        // Create socket path
        let socket_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".capsule")
            .join("sessions")
            .join(&session_id)
            .join("monitor.sock");

        // Ensure socket directory exists
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Create session lock
        let session_lock = SessionLock {
            session_id,
            pid: process::id(),
            command,
            started_at: chrono::Utc::now().to_rfc3339(),
            socket_path,
        };

        // Write lock file
        let lock_json = serde_json::to_string_pretty(&session_lock)?;
        tokio::fs::write(&lock_path, lock_json).await?;

        Ok(session_lock)
    }

    /// Read the current session lock file
    pub async fn read_lock() -> Result<SessionLock> {
        let lock_path = Self::lock_file_path();
        let lock_content = tokio::fs::read_to_string(&lock_path).await?;
        let session_lock: SessionLock = serde_json::from_str(&lock_content)?;
        Ok(session_lock)
    }

    /// Remove the session lock file
    pub async fn remove_lock() -> Result<()> {
        let lock_path = Self::lock_file_path();
        tokio::fs::remove_file(&lock_path).await?;
        Ok(())
    }

    /// Check if a process is still running
    fn is_process_running(pid: u32) -> bool {
        // On Unix systems, check if process exists
        #[cfg(unix)]
        {
            unsafe {
                let pid = pid as i32;
                libc::kill(pid, 0) == 0
            }
        }

        #[cfg(not(unix))]
        {
            // Fallback for non-Unix systems
            // This is a simplified check and may not be 100% accurate
            true
        }
    }

    /// Get active session if it exists and is valid
    pub async fn get_active_session() -> Result<SessionLock> {
        let session_lock = Self::read_lock().await?;

        if Self::is_process_running(session_lock.pid) {
            Ok(session_lock)
        } else {
            // Clean up stale lock
            let _ = Self::remove_lock().await;
            Err(anyhow!("No active session found"))
        }
    }
}

/// State server that broadcasts AgentState updates via Unix domain socket
pub struct StateServer {
    listener: UnixListener,
    state: Arc<RwLock<AgentState>>,
}

impl StateServer {
    /// Create a new state server
    pub async fn new(socket_path: &Path, state: Arc<RwLock<AgentState>>) -> Result<Self> {
        // Remove existing socket if it exists
        let _ = tokio::fs::remove_file(socket_path).await;

        let listener = UnixListener::bind(socket_path)?;

        Ok(Self { listener, state })
    }

    /// Run the state server
    pub async fn run(self, cancellation_token: CancellationToken) -> Result<()> {
        tracing::info!("State server listening on {:?}", self.listener.local_addr());

        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let state = self.state.clone();
                            let token = cancellation_token.clone();

                            // Spawn handler for this client
                            tokio::spawn(async move {
                                if let Err(e) = handle_monitor_client(stream, state, token).await {
                                    tracing::warn!("Monitor client error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = cancellation_token.cancelled() => {
                    tracing::info!("State server shutting down");
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Handle a monitor client connection
async fn handle_monitor_client(
    mut stream: UnixStream,
    state: Arc<RwLock<AgentState>>,
    cancellation_token: CancellationToken,
) -> Result<()> {
    tracing::info!("Monitor client connected");

    // Send state updates every 500ms
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Read current state and send to client
                let state_snapshot = {
                    let state_guard = state.read().await;
                    state_guard.clone()
                };

                // Serialize state
                let state_json = match serde_json::to_string(&state_snapshot) {
                    Ok(json) => json,
                    Err(e) => {
                        tracing::warn!("Failed to serialize state: {}", e);
                        continue;
                    }
                };

                // Send state with length prefix
                let state_bytes = state_json.as_bytes();
                let length = state_bytes.len() as u32;

                if let Err(e) = stream.write_all(&length.to_le_bytes()).await {
                    tracing::debug!("Client disconnected: {}", e);
                    break;
                }

                if let Err(e) = stream.write_all(state_bytes).await {
                    tracing::debug!("Client disconnected: {}", e);
                    break;
                }
            }
            _ = cancellation_token.cancelled() => {
                tracing::info!("Monitor client handler shutting down");
                break;
            }
        }
    }

    tracing::info!("Monitor client disconnected");
    Ok(())
}

/// State client for connecting to a running session
pub struct StateClient {
    stream: UnixStream,
}

impl StateClient {
    /// Connect to a running session's state server
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(Self { stream })
    }

    /// Receive the next state update
    pub async fn receive_state(&mut self) -> Result<AgentState> {
        // Read length prefix
        let mut length_bytes = [0u8; 4];
        self.stream.read_exact(&mut length_bytes).await?;
        let length = u32::from_le_bytes(length_bytes) as usize;

        // Read state data
        let mut state_bytes = vec![0u8; length];
        self.stream.read_exact(&mut state_bytes).await?;

        // Deserialize state
        let state_json = String::from_utf8(state_bytes)?;
        let state: AgentState = serde_json::from_str(&state_json)?;

        Ok(state)
    }
}

// Add libc dependency for process checking
#[cfg(unix)]
extern crate libc;
