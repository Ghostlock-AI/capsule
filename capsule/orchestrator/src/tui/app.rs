// TUI application state and configuration builder

use crate::config::*;
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    BasicSettings,
    Security,
    Tracing,
    Tools,
    Secrets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityTab {
    Profile,
    Mounts,
    Processes,
    Network,
    AppArmor,
}

pub struct TuiApp {
    pub current_section: Section,
    pub current_security_tab: SecurityTab,

    // Basic Settings (formerly VM Settings)
    pub vm_name: String,
    pub vm_cpus: u8,
    pub vm_memory: String,
    pub vm_disk: String,

    // Security
    pub security_profile: String,
    pub workspace_only: bool,
    pub allow_home: MountMode,
    pub no_background_processes: bool,
    pub restrict_fork: bool,
    pub network_enabled: bool,
    pub localhost_only: bool,
    pub apparmor_enabled: bool,
    pub apparmor_enforce: bool,

    // Tracing
    pub tracing_enabled: bool,
    pub trace_process: bool,
    pub trace_file: bool,
    pub trace_network: bool,
    pub trace_credentials: bool,
    pub trace_signal: bool,

    // Tools
    pub tools_python: bool,
    pub tools_node: bool,
    pub tools_rust: bool,
    pub tools_go: bool,
    pub tools_java: bool,
    pub tools_claude: bool,
    pub tools_codex: bool,
    pub tools_ollama: bool,
    pub tools_ffmpeg: bool,

    // Secrets - now inline .env editor
    pub secrets_text: String,
    pub secrets_inline: HashMap<String, String>,

    // UI State
    pub selected_index: usize,
    pub input_mode: bool, // For text input (VM name, secrets editor)
    pub secrets_cursor_line: usize,
}

impl Default for TuiApp {
    fn default() -> Self {
        Self {
            current_section: Section::BasicSettings,
            current_security_tab: SecurityTab::Profile,

            vm_name: String::new(),
            vm_cpus: 2,
            vm_memory: "2G".to_string(),
            vm_disk: "8G".to_string(),

            security_profile: "developer".to_string(),
            workspace_only: false,
            allow_home: MountMode::Writable,
            no_background_processes: true,
            restrict_fork: false,
            network_enabled: true,
            localhost_only: false,
            apparmor_enabled: true,
            apparmor_enforce: true,

            tracing_enabled: true,
            trace_process: true,
            trace_file: true,
            trace_network: true,
            trace_credentials: false,
            trace_signal: false,

            tools_python: false,
            tools_node: false,
            tools_rust: false,
            tools_go: false,
            tools_java: false,
            tools_claude: false,
            tools_codex: false,
            tools_ollama: false,
            tools_ffmpeg: false,

            secrets_text: String::new(),
            secrets_inline: HashMap::new(),

            selected_index: 0,
            input_mode: false,
            secrets_cursor_line: 0,
        }
    }
}

impl TuiApp {
    /// Check if configuration is ready to submit (has VM name)
    pub fn is_ready_to_submit(&self) -> bool {
        !self.vm_name.is_empty()
    }

    /// Parse secrets_text into secrets_inline HashMap (KEY=VALUE format)
    pub fn parse_secrets(&mut self) {
        self.secrets_inline.clear();
        for line in self.secrets_text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                self.secrets_inline.insert(
                    key.trim().to_string(),
                    value.trim().to_string(),
                );
            }
        }
    }

    /// Build CapsuleConfig from TUI state
    pub fn build_config(&self) -> Result<CapsuleConfig> {
        let mut runtimes = Vec::new();
        if self.tools_python {
            runtimes.push(RuntimeTool::Python3);
        }
        if self.tools_node {
            runtimes.push(RuntimeTool::Node);
        }
        if self.tools_rust {
            runtimes.push(RuntimeTool::Rust);
        }
        if self.tools_go {
            runtimes.push(RuntimeTool::Go);
        }
        if self.tools_java {
            runtimes.push(RuntimeTool::Java);
        }

        let mut ai_tools = Vec::new();
        if self.tools_claude {
            ai_tools.push(AiTool::Claude);
        }
        if self.tools_codex {
            ai_tools.push(AiTool::Codex);
        }
        if self.tools_ollama {
            ai_tools.push(AiTool::Ollama);
        }

        let mut utilities = Vec::new();
        if self.tools_ffmpeg {
            utilities.push("ffmpeg".to_string());
        }

        Ok(CapsuleConfig {
            vm: VmSettings {
                name: self.vm_name.clone(),
                cpus: self.vm_cpus,
                memory: self.vm_memory.clone(),
                disk: self.vm_disk.clone(),
            },
            security: SecurityProfile {
                profile: self.security_profile.clone(),
                mounts: MountPolicy {
                    workspace_only: self.workspace_only,
                    allow_home: self.allow_home.clone(),
                    allowed_paths: Vec::new(),
                },
                processes: ProcessPolicy {
                    no_background_persistence: self.no_background_processes,
                    restrict_fork: self.restrict_fork,
                    max_children: None,
                },
                network: NetworkPolicy {
                    enabled: self.network_enabled,
                    localhost_only: self.localhost_only,
                    allowed_destinations: Vec::new(),
                    blocked_destinations: Vec::new(),
                },
                apparmor: Some(AppArmorConfig {
                    enabled: self.apparmor_enabled,
                    enforce: self.apparmor_enforce,
                    custom_rules: Vec::new(),
                }),
                seccomp: None,
            },
            tracing: TracingConfig {
                enabled: self.tracing_enabled,
                events: EventCategories {
                    process: self.trace_process,
                    file: self.trace_file,
                    network: self.trace_network,
                    credentials: self.trace_credentials,
                    signal: self.trace_signal,
                },
                scope: TraceScope {
                    user: "agent".to_string(),
                    new_processes: true,
                    follow: true,
                },
            },
            tools: ToolsConfig {
                runtimes,
                ai_tools,
                utilities,
            },
            secrets: SecretsConfig {
                env_file: None,
                inline: self.secrets_inline.clone(),
            },
        })
    }
}
