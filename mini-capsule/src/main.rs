mod config;
mod models;
mod query;
mod trace;
mod transfer;

use anyhow::{Context, Result};
use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use trace::LinuxTracer;

// =============================================
// CLI COMMANDS
// =============================================
#[derive(ClapParser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Trace a program with syscall monitoring
    ///
    /// Examples:
    ///             minic trace python3 server.py
    ///             minic trace node app.js
    ///             minic trace ./binary
    ///             minic trace claude
    Trace {
        program: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Transfer sessions to Supabase
    ///
    /// Examples:
    ///             capsule transfer --all
    ///             capsule transfer --session 20251002_223525
    ///             capsule transfer --all --force
    Transfer {
        /// Transfer all untransferred sessions
        #[arg(long)]
        all: bool,

        /// Transfer a specific session by directory name (e.g., "20251002_223525")
        #[arg(long)]
        session: Option<String>,

        /// Force re-transfer even if already transferred
        #[arg(long)]
        force: bool,
    },

    /// Query the database using natural language
    ///
    /// Examples:
    ///             minic query "show me the last session"
    ///             minic query "what network syscalls happened in the last session"
    ///             minic query "which programs made the most file operations"
    Query {
        /// Natural language query
        question: String,
    },

    /// Configure AI settings (Anthropic API key)
    ///
    /// Examples:
    ///             minic ai-setup
    AiSetup,
}

// =============================================
// HELPERS
// =============================================
async fn setup_capsule_directory() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let capsule_dir = PathBuf::from(home).join(".capsule");

    if !capsule_dir.exists() {
        fs::create_dir_all(&capsule_dir)
            .await
            .context("Failed to create ~/.capsule directory")?;
    }

    Ok(capsule_dir)
}

// =============================================
// ROUTES: The paths of CLI commands
// =============================================
async fn run(program: String, args: Vec<String>) -> Result<()> {
    // Setup capsule directory
    let capsule_dir = setup_capsule_directory().await?;

    // Get working directory
    let working_dir = std::env::current_dir()
        .context("Failed to get current directory")?
        .to_string_lossy()
        .to_string();

    // Create session metadata
    let mut session = models::SessionMetadata::new(program.clone(), args.clone(), working_dir);

    // Create session directory with timestamp
    let session_dir_name = session.timestamp.format("%Y%m%d_%H%M%S").to_string();
    let session_dir = capsule_dir.join(session_dir_name);
    fs::create_dir_all(&session_dir)
        .await
        .context("Failed to create session directory")?;

    // Create raw trace file
    let trace_file_path = session_dir.join("raw_trace.txt");
    let mut trace_file = fs::File::create(&trace_file_path)
        .await
        .context("Failed to create raw_trace.txt")?;

    // Create structured syscalls file (JSONL format)
    let structured_file_path = session_dir.join("structured_syscalls.jsonl");
    let mut structured_file = fs::File::create(&structured_file_path)
        .await
        .context("Failed to create structured_syscalls.jsonl")?;

    // Create failed parse file
    let failed_file_path = session_dir.join("failed_parse_raw.txt");
    let mut failed_file = fs::File::create(&failed_file_path)
        .await
        .context("Failed to create failed_parse_raw.txt")?;

    let (tx, mut rx1) = broadcast::channel::<String>(1024);
    let mut rx2 = tx.subscribe();
    let mut rx3 = tx.subscribe();
    let cancellation_token = CancellationToken::new();

    let cancel_clone = cancellation_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_clone.cancel();
    });

    let mut cmdline = vec![program];
    cmdline.extend(args);

    let trace_handle =
        tokio::spawn(async move { LinuxTracer::trace(cmdline, tx, cancellation_token).await });

    // Write raw trace output to file
    let raw_writer_handle = tokio::spawn(async move {
        while let Ok(line) = rx1.recv().await {
            if let Err(_) = trace_file.write_all(line.as_bytes()).await {
                break;
            }
            if let Err(_) = trace_file.write_all(b"\n").await {
                break;
            }
        }
    });

    // Parse and write structured syscalls to JSONL file
    let structured_writer_handle = tokio::spawn(async move {
        while let Ok(line) = rx2.recv().await {
            // Try to parse the line into a RawSyscall
            if let Ok(raw_syscall) = trace::parse_raw_syscall(&line) {
                // Serialize to JSON and write as single line
                if let Ok(json) = serde_json::to_string(&raw_syscall) {
                    if let Err(_) = structured_file.write_all(json.as_bytes()).await {
                        break;
                    }
                    if let Err(_) = structured_file.write_all(b"\n").await {
                        break;
                    }
                }
            }
            // Silently skip unparseable lines (empty, unfinished syscalls, etc.)
        }
    });

    // Write failed parse lines to separate file
    let failed_writer_handle = tokio::spawn(async move {
        while let Ok(line) = rx3.recv().await {
            // Try to parse, if it fails, write to failed file
            if let Err(_) = trace::parse_raw_syscall(&line) {
                if let Err(_) = failed_file.write_all(line.as_bytes()).await {
                    break;
                }
                if let Err(_) = failed_file.write_all(b"\n").await {
                    break;
                }
            }
        }
    });

    trace_handle.await??;
    raw_writer_handle.abort();
    structured_writer_handle.abort();
    failed_writer_handle.abort();

    // Mark session as complete
    session.complete();

    // Write session metadata
    let metadata_path = session_dir.join("session_metadata.json");
    let metadata_json =
        serde_json::to_string_pretty(&session).context("Failed to serialize session metadata")?;
    fs::write(&metadata_path, metadata_json)
        .await
        .context("Failed to write session metadata")?;

    Ok(())
}

async fn transfer_command(
    all: bool,
    session: Option<String>,
    force: bool,
) -> Result<()> {
    use config::SupaConfig;
    use transfer::{is_session_transferred, SupabaseTransfer};

    // Load config
    let config = SupaConfig::load()
        .context("Failed to load config. Make sure ~/.capsule/config.toml exists or SUPABASE_URL/SUPABASE_SERVICE_KEY are set")?;

    if !config.is_configured() {
        anyhow::bail!("Supabase is not configured. Check your config file or environment variables.");
    }

    let transfer_client = SupabaseTransfer::new(config.clone());

    // Get capsule directory
    let capsule_dir = setup_capsule_directory().await?;

    // Get sessions to transfer
    let sessions_to_transfer = if all {
        // Find all session directories
        get_all_sessions(&capsule_dir).await?
    } else if let Some(session_name) = session {
        // Transfer specific session
        vec![capsule_dir.join(session_name)]
    } else {
        anyhow::bail!("Must specify --all or --session <name>");
    };

    if sessions_to_transfer.is_empty() {
        println!("No sessions found to transfer.");
        return Ok(());
    }

    println!("Found {} session(s) to transfer", sessions_to_transfer.len());

    // Transfer each session
    for session_dir in sessions_to_transfer {
        // Read session metadata to get UUID
        let metadata_path = session_dir.join("session_metadata.json");
        if !metadata_path.exists() {
            println!("Skipping {:?} - no metadata file", session_dir);
            continue;
        }

        let metadata_json = fs::read_to_string(&metadata_path).await?;
        let metadata: models::SessionMetadata = serde_json::from_str(&metadata_json)?;

        // Check if already transferred
        if !force && is_session_transferred(&config, &metadata.uuid).await? {
            println!(
                "Session {} already transferred (use --force to re-transfer)",
                metadata.uuid
            );
            continue;
        }

        // Transfer
        match transfer_client.transfer_session(&session_dir).await {
            Ok(session_id) => {
                println!("✓ Transferred session: {}", session_id);
            }
            Err(e) => {
                eprintln!("✗ Failed to transfer {:?}: {}", session_dir, e);
            }
        }
    }

    println!("\nTransfer complete!");
    Ok(())
}

async fn ai_setup_command() -> Result<()> {
    use config::SupaConfig;
    use std::io::{self, Write};

    println!("🤖 AI Setup - Configure Anthropic API Key\n");

    // Load existing config or create new one
    let mut config = match SupaConfig::load() {
        Ok(cfg) => {
            println!("✓ Loaded existing config from ~/.capsule/config.toml");
            cfg
        }
        Err(_) => {
            println!("⚠ No existing config found. Please run a command that creates the config first.");
            anyhow::bail!("Config file not found. Run 'minic trace' or 'minic transfer' first to create ~/.capsule/config.toml");
        }
    };

    // Show current status
    if let Some(ai) = &config.ai {
        if let Some(key) = &ai.anthropic_api_key {
            if !key.is_empty() {
                println!("Current API key: {}...{}", &key[..7.min(key.len())],
                    if key.len() > 7 { "****" } else { "" });
            }
        }
    }

    // Prompt for new API key
    print!("\nEnter your Anthropic API key (starts with 'sk-ant-'): ");
    io::stdout().flush()?;

    let mut api_key = String::new();
    io::stdin().read_line(&mut api_key)?;
    let api_key = api_key.trim().to_string();

    // Validate format
    if !api_key.starts_with("sk-ant-") {
        anyhow::bail!("Invalid API key format. Anthropic API keys start with 'sk-ant-'");
    }

    if api_key.len() < 20 {
        anyhow::bail!("API key seems too short. Please check and try again.");
    }

    // Save to config
    config.set_anthropic_api_key(api_key)?;

    println!("\n✓ API key saved to ~/.capsule/config.toml");
    println!("  You can now use: minic query \"your question here\"");

    Ok(())
}

async fn query_command(question: String) -> Result<()> {
    use config::SupaConfig;
    use query::QueryEngine;

    // Load config
    let config = SupaConfig::load().context(
        "Failed to load config. Make sure ~/.capsule/config.toml exists or SUPABASE_URL/SUPABASE_SERVICE_KEY are set",
    )?;

    if !config.is_configured() {
        anyhow::bail!("Supabase is not configured. Check your config file or environment variables.");
    }

    // Get Anthropic API key from config or environment
    let api_key = config.get_anthropic_api_key().context(
        "Anthropic API key not configured. Run 'minic ai-setup' to configure it."
    )?;

    // Connect to database
    // Convert Supabase Kong URL (http://supabase-kong:8000) to Postgres URL
    let database_url = if config.supabase.url.contains("supabase-kong") {
        "postgresql://postgres:postgres@supabase-db:5432/postgres".to_string()
    } else {
        // For production/custom URLs, parse and replace port
        let db_host = config.supabase.url
            .replace("http://", "")
            .replace("https://", "")
            .replace(":8000", ":5432");
        format!("postgresql://postgres:postgres@{}/postgres", db_host)
    };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    // Create query engine
    let engine = QueryEngine::new(pool, api_key);

    // Execute query
    engine.run(&question).await?;

    Ok(())
}

async fn get_all_sessions(capsule_dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut sessions = Vec::new();
    let mut entries = fs::read_dir(capsule_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            // Check if it has session_metadata.json
            if path.join("session_metadata.json").exists() {
                sessions.push(path);
            }
        }
    }

    Ok(sessions)
}

async fn check_supabase_config() -> Result<()> {
    use config::SupaConfig;

    match SupaConfig::load() {
        Ok(config) => {
            if config.is_configured() {
                println!("✓ Supabase configured: {}", config.supabase.url);
            } else {
                eprintln!("⚠ Supabase config found but not enabled");
            }
        }
        Err(_) => {
            eprintln!("⚠ No Supabase config found");

            // Auto-create config with local dev keys
            let home = std::env::var("HOME").context("HOME not set")?;
            let config_path = PathBuf::from(&home).join(".capsule/config.toml");

            let default_config = r#"# Capsule Configuration File
# These are throwaway local development keys for docker-compose
# For production, update these values or set environment variables

[supabase]
url = "http://supabase-kong:8000"
service_key = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZS1kZW1vIiwicm9sZSI6InNlcnZpY2Vfcm9sZSIsImV4cCI6MTk4MzgxMjk5Nn0.EGIM96RAZx35lJzdJsyH-qQwv8Hdp7fsn3W0YpN81IU"
anon_key = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZS1kZW1vIiwicm9sZSI6ImFub24iLCJleHAiOjE5ODM4MTI5OTZ9.CRXP1A7WOeoJeXxjNni43kdQwgnWNReilDMblYTn_I0"
enabled = true

[transfer]
auto_transfer = false
batch_size = 100
"#;

            // Ensure .capsule directory exists
            let capsule_dir = PathBuf::from(&home).join(".capsule");
            if !capsule_dir.exists() {
                fs::create_dir_all(&capsule_dir).await
                    .context("Failed to create ~/.capsule directory")?;
            }

            // Write default config
            fs::write(&config_path, default_config).await
                .context("Failed to write default config")?;

            println!("✓ Created default config at {:?}", config_path);
            println!("  Using local docker-compose Supabase (throwaway keys)");
        }
    }
    println!();
    Ok(())
}

// =============================================
// MAIN: Where CLI commands connect to routes
// =============================================
#[tokio::main]
async fn main() {
    // Check Supabase config on startup (auto-creates if missing)
    if let Err(e) = check_supabase_config().await {
        eprintln!("Warning: Failed to check/create config: {}", e);
    }

    match Cli::parse().cmd {
        Cmd::Trace { program, args } => {
            if let Err(e) = run(program, args).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Cmd::Transfer { all, session, force } => {
            if let Err(e) = transfer_command(all, session, force).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Cmd::Query { question } => {
            if let Err(e) = query_command(question).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Cmd::AiSetup => {
            if let Err(e) = ai_setup_command().await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
