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
pub struct AiConfig {
    pub anthropic_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupaConfig {
    pub supabase: SupabaseConfig,
    pub transfer: TransferConfig,
    #[serde(default)]
    pub ai: Option<AiConfig>,
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
            ai: None,
        })
    }

    pub fn is_configured(&self) -> bool {
        self.supabase.enabled
            && !self.supabase.url.is_empty()
            && !self.supabase.service_key.is_empty()
    }

    /// Get Anthropic API key from config or environment
    pub fn get_anthropic_api_key(&self) -> Result<String> {
        // Try config first
        if let Some(ai) = &self.ai {
            if let Some(key) = &ai.anthropic_api_key {
                if !key.is_empty() {
                    return Ok(key.clone());
                }
            }
        }

        // Fall back to environment variable
        std::env::var("ANTHROPIC_API_KEY")
            .context("Anthropic API key not found in config or ANTHROPIC_API_KEY environment variable")
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let home = std::env::var("HOME").context("HOME not set")?;
        let config_path = PathBuf::from(home).join(".capsule/config.toml");

        let toml_string = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        std::fs::write(&config_path, toml_string)
            .context(format!("Failed to write config to {:?}", config_path))?;

        Ok(())
    }

    /// Update AI config and save
    pub fn set_anthropic_api_key(&mut self, api_key: String) -> Result<()> {
        self.ai = Some(AiConfig {
            anthropic_api_key: Some(api_key),
        });
        self.save()
    }
}
