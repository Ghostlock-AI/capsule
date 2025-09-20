//! Example: How TUI would access shared AgentState
//!
//! Demonstrates reading live processes from shared state

use state::{AgentState, ProcessTracker};
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Create shared state (same as in pipeline)
    let (tracker, shared_state) = ProcessTracker::new(Some("claude".to_string()));

    // TUI component reads from shared state
    tokio::spawn(async move {
        loop {
            // Read current state (async)
            {
                let state = shared_state.read().await;

                println!("=== Agent State ===");
                println!("Active processes: {}", state.active_count());
                println!("Capsule target: {:?}", state.capsule_target);

                // Display live processes (what TUI would show)
                for process in state.live_processes() {
                    println!(
                        "  PID {} ({}): {} -> {}",
                        process.pid,
                        process.ppid,
                        process.name,
                        process.command_line.join(" ")
                    );
                }

                if let Some(root) = state.root_process() {
                    println!("Root process: {} (PID {})", root.name, root.pid);
                }
            } // RwLock released here

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    // Simulate tracker receiving events...
    tokio::time::sleep(Duration::from_secs(10)).await;
}
