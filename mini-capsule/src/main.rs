use anyhow::Result;
use clap::{Parser as ClapParser, Subcommand};

#[derive(ClapParser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// run a program with tracing
    ///
    /// Examples:
    ///             capsule run pthon3 server.py
    ///             capsule run node app.js
    ///             capsule run ./binary
    ///             capsule run claude
    Run {
        program: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

fn main() {
    // Parse and execute commands
    match Cli::parse().cmd {
        Cmd::Run { program, args } => println!("run"),
    }
}
