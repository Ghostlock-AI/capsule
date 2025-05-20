/// this is the command line interface definitions
/// it uses the clap library to define
/// the command and subcommand API's
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "capsule")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand)]
pub enum DaemonAction {
    // start daemon in background
    Start,
    // stop running daemon
    Stop,
    // view status info about daemon state
    Status,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run as background daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Stop the running daemon (gracefully via socket, fallback to SIGTERM)
    Shutdown,
    /// Verify daemon is running
    Status,

    Run {
        /// args for that program
        #[arg(
            value_name = "CMD…",
            num_args = 1..,
            trailing_var_arg = true
        )]
        cmd: Vec<String>,
    },
}
