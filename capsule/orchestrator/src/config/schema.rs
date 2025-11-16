use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsuleConfig {
    /// VM metadata
    pub vm: VmSettings,

    /// Security and sandboxing
    pub security: SecurityProfile,

    /// Kernel tracing configuration
    pub tracing: TracingConfig,

    /// Tools and software to install
    pub tools: ToolsConfig,

    /// Environment variables and secrets
    #[serde(default)]
    pub secrets: SecretsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSettings {
    pub name: String,
    #[serde(default = "default_cpus")]
    pub cpus: u8,
    #[serde(default = "default_memory")]
    pub memory: String,
    #[serde(default = "default_disk")]
    pub disk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityProfile {
    /// Sandbox profile preset: minimal, developer, strict, custom
    #[serde(default = "default_profile")]
    pub profile: String,

    /// Mount restrictions
    pub mounts: MountPolicy,

    /// Process restrictions
    pub processes: ProcessPolicy,

    /// Network restrictions
    pub network: NetworkPolicy,

    /// AppArmor profile configuration
    #[serde(default)]
    pub apparmor: Option<AppArmorConfig>,

    /// Seccomp-BPF syscall filtering
    #[serde(default)]
    pub seccomp: Option<SeccompConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPolicy {
    /// Restrict to workspace directory only
    #[serde(default)]
    pub workspace_only: bool,

    /// Allow home directory mount (read-only or writable)
    #[serde(default)]
    pub allow_home: MountMode,

    /// Additional allowed mount points
    #[serde(default)]
    pub allowed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountMode {
    None,
    ReadOnly,
    Writable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessPolicy {
    /// Prevent background processes from persisting
    #[serde(default)]
    pub no_background_persistence: bool,

    /// Restrict process spawning
    #[serde(default)]
    pub restrict_fork: bool,

    /// Limit child processes
    #[serde(default)]
    pub max_children: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Allow all network access
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Block external network (localhost only)
    #[serde(default)]
    pub localhost_only: bool,

    /// Allowed destination IPs/CIDRs
    #[serde(default)]
    pub allowed_destinations: Vec<String>,

    /// Blocked destination IPs/CIDRs
    #[serde(default)]
    pub blocked_destinations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Enable kernel tracing
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Event categories to trace
    pub events: EventCategories,

    /// Trace scope (user-based filtering)
    #[serde(default)]
    pub scope: TraceScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCategories {
    #[serde(default = "default_true")]
    pub process: bool,
    #[serde(default = "default_true")]
    pub file: bool,
    #[serde(default = "default_true")]
    pub network: bool,
    #[serde(default)]
    pub credentials: bool,
    #[serde(default)]
    pub signal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceScope {
    /// Only trace specific user (default: agent)
    #[serde(default = "default_agent_user")]
    pub user: String,

    /// Trace new processes only (pid=new)
    #[serde(default = "default_true")]
    pub new_processes: bool,

    /// Follow child processes
    #[serde(default = "default_true")]
    pub follow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// Language runtimes
    #[serde(default)]
    pub runtimes: Vec<RuntimeTool>,

    /// AI/ML tools
    #[serde(default)]
    pub ai_tools: Vec<AiTool>,

    /// System utilities
    #[serde(default)]
    pub utilities: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeTool {
    Python3,
    Node,
    Rust,
    Go,
    Java,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiTool {
    Claude,
    Codex,
    Ollama,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Path to .env file
    #[serde(default)]
    pub env_file: Option<String>,

    /// Inline key-value secrets
    #[serde(default)]
    pub inline: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppArmorConfig {
    /// Enable AppArmor profile
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Enforce mode (vs complain mode)
    #[serde(default = "default_true")]
    pub enforce: bool,

    /// Custom AppArmor rules (raw syntax)
    #[serde(default)]
    pub custom_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompConfig {
    /// Enable Seccomp-BPF filtering
    #[serde(default)]
    pub enabled: bool,

    /// Default action (allow or deny)
    #[serde(default = "default_allow")]
    pub default_action: SeccompAction,

    /// Blocked syscalls
    #[serde(default)]
    pub blocked_syscalls: Vec<String>,

    /// Allowed syscalls (when default is deny)
    #[serde(default)]
    pub allowed_syscalls: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeccompAction {
    Allow,
    Deny,
}

// Default value functions
fn default_cpus() -> u8 { 2 }
fn default_memory() -> String { "2G".to_string() }
fn default_disk() -> String { "8G".to_string() }
fn default_profile() -> String { "developer".to_string() }
fn default_true() -> bool { true }
fn default_agent_user() -> String { "agent".to_string() }
fn default_allow() -> SeccompAction { SeccompAction::Allow }

impl Default for MountMode {
    fn default() -> Self { MountMode::Writable }
}

impl Default for MountPolicy {
    fn default() -> Self {
        MountPolicy {
            workspace_only: false,
            allow_home: MountMode::Writable,
            allowed_paths: Vec::new(),
        }
    }
}

impl Default for ProcessPolicy {
    fn default() -> Self {
        ProcessPolicy {
            no_background_persistence: true,
            restrict_fork: false,
            max_children: None,
        }
    }
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        NetworkPolicy {
            enabled: true,
            localhost_only: false,
            allowed_destinations: Vec::new(),
            blocked_destinations: Vec::new(),
        }
    }
}

impl Default for EventCategories {
    fn default() -> Self {
        EventCategories {
            process: true,
            file: true,
            network: true,
            credentials: false,
            signal: false,
        }
    }
}

impl Default for TraceScope {
    fn default() -> Self {
        TraceScope {
            user: "agent".to_string(),
            new_processes: true,
            follow: true,
        }
    }
}

impl Default for TracingConfig {
    fn default() -> Self {
        TracingConfig {
            enabled: true,
            events: EventCategories::default(),
            scope: TraceScope::default(),
        }
    }
}

impl Default for ToolsConfig {
    fn default() -> Self {
        ToolsConfig {
            runtimes: Vec::new(),
            ai_tools: Vec::new(),
            utilities: Vec::new(),
        }
    }
}

impl Default for SecretsConfig {
    fn default() -> Self {
        SecretsConfig {
            env_file: None,
            inline: HashMap::new(),
        }
    }
}

impl Default for AppArmorConfig {
    fn default() -> Self {
        AppArmorConfig {
            enabled: true,
            enforce: true,
            custom_rules: Vec::new(),
        }
    }
}

impl Default for SeccompConfig {
    fn default() -> Self {
        SeccompConfig {
            enabled: false,
            default_action: SeccompAction::Allow,
            blocked_syscalls: Vec::new(),
            allowed_syscalls: Vec::new(),
        }
    }
}
