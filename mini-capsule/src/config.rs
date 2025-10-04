use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupabaseConfig {
    pub url: String,
    pub service_key: String,
    pub anon_key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferConfig {
    pub auto_transfer: bool,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupaConfig {
    pub supabase: SupabaseConfig,
    pub transfer: TransferConfig,
}

impl SupaConfig {
    /// Load config from file or environment variables
    pub fn load() -> Result<Self> {
        // Try config file first
        if let Ok(config) = Self::load_from_file() {
            return Ok(config);
        }

        // Fall back to environment variables
        Self::load_from_env()
    }

    fn load_from_file() -> Result<Self> {
        let home = std::env::var("HOME").context("HOME not set")?;
        let config_path = PathBuf::from(home).join(".capsule/config.toml");

        if !config_path.exists() {
            anyhow::bail!("Config file not found at {:?}", config_path);
        }

        let contents = std::fs::read_to_string(&config_path)
            .context("Failed to read config file")?;

        let config: SupaConfig = toml::from_str(&contents)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    fn load_from_env() -> Result<Self> {
        // Try to load .env file (won't error if not found)
        let _ = dotenvy::dotenv();

        let supabase_url = std::env::var("SUPABASE_URL")
            .context("SUPABASE_URL environment variable not set")?;
        let service_key = std::env::var("SUPABASE_SERVICE_KEY")
            .context("SUPABASE_SERVICE_KEY environment variable not set")?;
        let anon_key = std::env::var("SUPABASE_ANON_KEY")
            .unwrap_or_else(|_| service_key.clone());

        Ok(SupaConfig {
            supabase: SupabaseConfig {
                url: supabase_url,
                service_key,
                anon_key,
                enabled: true,
            },
            transfer: TransferConfig {
                auto_transfer: false,
                batch_size: 100,
            },
        })
    }

    pub fn is_configured(&self) -> bool {
        self.supabase.enabled
            && !self.supabase.url.is_empty()
            && !self.supabase.service_key.is_empty()
    }
}
