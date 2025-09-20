//! Tokio pipeline orchestration for trace → parse → track
//!
//! Coordinates the async pipeline using JoinSet, broadcast channels,
//! and cancellation tokens for graceful shutdown.

use crate::ipc::{SessionLockManager, StateServer};
use anyhow::Result;
use core::SyscallEvent;
use io::{StreamCoordinator, StreamReceiver};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub struct Pipeline {
    cancellation_token: CancellationToken,
    task_set: JoinSet<Result<()>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
            task_set: JoinSet::new(),
        }
    }

    /// Run the complete pipeline: trace → parse → track
    pub async fn run(&mut self, cmdline: Vec<String>, session_dir: String) -> Result<()> {
        info!("Starting pipeline for command: {:?}", cmdline);

        // Initialize debug logging if enabled
        state::init_debug_logging();

        // Extract session ID from session_dir path
        let session_id = PathBuf::from(&session_dir)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid session directory"))?
            .to_string();

        // Create session lock for monitoring
        let session_lock = SessionLockManager::create_lock(session_id, cmdline.clone()).await?;
        info!("Created session lock: {}", session_lock.session_id);

        // Create shared state for tracking and monitoring
        let (tracker, shared_state) = state::ProcessTracker::new(
            cmdline.first().map(|s| s.clone()), // Use first command as target
        );

        // Start state server for monitoring
        let state_server =
            StateServer::new(&session_lock.socket_path, shared_state.clone()).await?;
        let state_cancellation = self.cancellation_token.clone();
        self.task_set
            .spawn(async move { state_server.run(state_cancellation).await });

        // Create broadcast channels for pipeline communication
        let (tx_raw, _) = broadcast::channel::<String>(8192);
        let (tx_syscalls, _) = broadcast::channel::<SyscallEvent>(4096);
        // Human-readable events channel
        let (tx_human, _) = broadcast::channel::<String>(4096);

        // Ready synchronization - wait for all tasks to be ready
        let (ready_tx, mut ready_rx) = mpsc::channel::<()>(4); // Increased for state server

        // Setup stream coordinator
        let mut coordinator = StreamCoordinator::new(PathBuf::from(&session_dir));

        // Add receivers - for now just the strace output
        coordinator.add_receiver("syscalls.jsonl");

        // Start all receivers - syscalls.jsonl still gets raw strings for now
        let receiver_handles = coordinator
            .start_all(
                tx_raw.subscribe(), // Keep raw strings for file logging
                self.cancellation_token.clone(),
            )
            .await?;

        // Add receiver handles to task set
        for handle in receiver_handles {
            self.task_set.spawn(async move {
                match handle.await {
                    Ok(result) => result,
                    Err(e) => Err(anyhow::anyhow!("Receiver task failed: {}", e)),
                }
            });
        }

        // Start human events file sink (events.txt)
        let human_receiver = StreamReceiver::new(PathBuf::from(&session_dir).join("events.jsonl"));
        let human_handle = human_receiver
            .start(tx_human.subscribe(), self.cancellation_token.clone())
            .await?;
        self.task_set.spawn(async move {
            match human_handle.await {
                Ok(result) => result,
                Err(e) => Err(anyhow::anyhow!("Human events receiver failed: {}", e)),
            }
        });

        // Spawn trace task
        self.task_set.spawn(spawn_trace_task(
            cmdline,
            tx_raw.clone(),
            ready_tx.clone(),
            self.cancellation_token.clone(),
        ));

        // Spawn parse task
        self.task_set.spawn(spawn_parse_task(
            tx_raw.subscribe(),
            tx_syscalls.clone(),
            ready_tx.clone(),
            self.cancellation_token.clone(),
        ));

        // Attach human sender to tracker and spawn track task with shared state
        let tracker = tracker.with_human_sender(tx_human.clone());
        self.task_set.spawn(spawn_track_task(
            tx_syscalls.subscribe(),
            tracker,
            ready_tx,
            self.cancellation_token.clone(),
        ));

        // Wait for all tasks to signal ready
        info!("Waiting for pipeline tasks to be ready...");
        for i in 0..3 {
            match ready_rx.recv().await {
                Some(()) => info!("Task {} ready", i + 1),
                None => return Err(anyhow::anyhow!("Ready channel closed unexpectedly")),
            }
        }
        info!("All pipeline tasks ready, starting execution");

        // Handle graceful shutdown
        tokio::select! {
            // Wait for tasks to complete naturally
            result = self.wait_for_tasks() => {
                match result {
                    Ok(()) => info!("Pipeline completed successfully"),
                    Err(e) => error!("Pipeline error: {}", e),
                }
            },
            // Handle Ctrl+C
            _ = tokio::signal::ctrl_c() => {
                warn!("Received Ctrl+C, initiating graceful shutdown...");
                self.cancellation_token.cancel();

                // Give tasks 5 seconds to clean up
                let shutdown_result = tokio::time::timeout(
                    Duration::from_secs(5),
                    self.wait_for_tasks()
                ).await;

                match shutdown_result {
                    Ok(Ok(())) => info!("Graceful shutdown completed"),
                    Ok(Err(e)) => warn!("Shutdown with errors: {}", e),
                    Err(_) => warn!("Shutdown timeout, some tasks may not have cleaned up"),
                }
            }
        }

        // Clean up session lock
        info!("Cleaning up session lock");
        if let Err(e) = SessionLockManager::remove_lock().await {
            warn!("Failed to remove session lock: {}", e);
        }

        Ok(())
    }
}

async fn spawn_trace_task(
    cmdline: Vec<String>,
    tx_raw: broadcast::Sender<String>,
    ready_tx: mpsc::Sender<()>,
    cancellation_token: CancellationToken,
) -> Result<()> {
    // Signal ready immediately (trace doesn't need setup)
    ready_tx
        .send(())
        .await
        .map_err(|_| anyhow::anyhow!("Ready channel closed"))?;

    // Start tracing
    trace::LinuxTracer::run_with_cancellation(cmdline, tx_raw, cancellation_token).await
}

async fn spawn_parse_task(
    mut rx_raw: broadcast::Receiver<String>,
    tx_syscalls: broadcast::Sender<SyscallEvent>,
    ready_tx: mpsc::Sender<()>,
    cancellation_token: CancellationToken,
) -> Result<()> {
    // Signal ready immediately (parser doesn't need setup)
    ready_tx
        .send(())
        .await
        .map_err(|_| anyhow::anyhow!("Ready channel closed"))?;

    // Parse strace lines and emit ALL syscalls as SyscallEvents
    loop {
        tokio::select! {
            line_result = rx_raw.recv() => {
                match line_result {
                    Ok(line) => {
                        // Parse the strace line
                        let parse_result = parse::StraceParser::parse_line(&line);

                        if let parse::StraceParseResult::Event(syscall_event) = parse_result {
                            // Broadcast ALL syscalls - no filtering at this layer
                            if tx_syscalls.send(syscall_event).is_err() {
                                // No more receivers
                                break;
                            }
                        }
                    },
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Parser lagged by {} events", n);
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            },
            _ = cancellation_token.cancelled() => {
                info!("Parse task received cancellation signal");
                break;
            }
        }
    }

    Ok(())
}

async fn spawn_track_task(
    rx_syscalls: broadcast::Receiver<SyscallEvent>,
    tracker: state::ProcessTracker,
    ready_tx: mpsc::Sender<()>,
    cancellation_token: CancellationToken,
) -> Result<()> {
    // Run the tracker with shared state - now receives SyscallEvent
    // Note: This will break until we update ProcessTracker in Phase 3
    tracker
        .run_syscall(rx_syscalls, ready_tx, cancellation_token)
        .await
}

// Conversion functions removed - domain parsing now happens in state layer

impl Pipeline {
    async fn wait_for_tasks(&mut self) -> Result<()> {
        while let Some(result) = self.task_set.join_next().await {
            match result {
                Ok(task_result) => {
                    if let Err(e) = task_result {
                        error!("Task error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Task join error: {}", e);
                }
            }
        }
        Ok(())
    }
}

// Tests removed - conversion logic moved to state layer
