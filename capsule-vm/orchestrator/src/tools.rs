use clap::ValueEnum;
use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, ValueEnum)]
pub enum ToolKind {
    /// Installs Node.js (snap) and the OpenAI Codex CLI (npm)
    #[value(alias = "codex-cli")]
    Codex,
    /// Installs Node.js (snap) and the Anthropic Claude Code CLI (npm)
    #[value(name = "claude", alias = "claudecode", alias = "claude-code")]
    ClaudeCode,
    /// Installs Node.js (snap) only
    #[value(alias = "nodejs")]
    Node,
    /// Installs Node.js (snap) as a prerequisite for global npm toolchains
    #[value(name = "npm", alias = "npm-cli")]
    Npm,
    /// Installs Python 3 alternate snapshot
    #[value(name = "python3", alias = "python")]
    Python3,
    /// Installs Rust toolchain via rustup snap
    #[value(name = "rust", alias = "rustup")]
    Rust,
}

pub struct ToolDefinition {
    pub kind: ToolKind,
    pub name: &'static str,
    pub description: &'static str,
    pub setup_steps: &'static [&'static str],
}

impl ToolKind {
    pub fn definition(self) -> &'static ToolDefinition {
        TOOL_DEFINITIONS
            .iter()
            .find(|def| def.kind == self)
            .expect("tool definition missing")
    }
}

impl fmt::Display for ToolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.definition().name)
    }
}

pub fn all_tools() -> &'static [ToolDefinition] {
    &TOOL_DEFINITIONS
}

const STEP_SNAP_NODE: &str = "snap install --classic node";
const STEP_NPM_CODEX: &str = "npm install -g @openai/codex";
const STEP_NPM_CLAUDE: &str = "npm install -g @anthropic-ai/claude-code";
const STEP_SNAP_PYTHON3_ALT: &str = "snap install python3-alt";
const STEP_SNAP_RUSTUP: &str = "snap install --classic rustup";
const STEP_RUSTUP_DEFAULT: &str = "sudo -H -u agent /snap/bin/rustup default stable";

const TOOL_DEFINITIONS: [ToolDefinition; 6] = [
    ToolDefinition {
        kind: ToolKind::Codex,
        name: "codex",
        description: "Installs Node.js via snap and the OpenAI Codex CLI globally (npm)",
        setup_steps: &[STEP_SNAP_NODE, STEP_NPM_CODEX],
    },
    ToolDefinition {
        kind: ToolKind::ClaudeCode,
        name: "claude",
        description: "Installs Node.js via snap and the Anthropic Claude Code CLI globally (npm)",
        setup_steps: &[STEP_SNAP_NODE, STEP_NPM_CLAUDE],
    },
    ToolDefinition {
        kind: ToolKind::Node,
        name: "node",
        description: "Installs Node.js and npm from the official snap",
        setup_steps: &[STEP_SNAP_NODE],
    },
    ToolDefinition {
        kind: ToolKind::Npm,
        name: "npm",
        description: "Ensures Node.js (and npm) are available via snap",
        setup_steps: &[STEP_SNAP_NODE],
    },
    ToolDefinition {
        kind: ToolKind::Python3,
        name: "python3",
        description: "Installs the python3-alt snap (system-wide Python 3)",
        setup_steps: &[STEP_SNAP_PYTHON3_ALT],
    },
    ToolDefinition {
        kind: ToolKind::Rust,
        name: "rust",
        description: "Installs rustup via snap for Rust toolchains",
        setup_steps: &[STEP_SNAP_RUSTUP, STEP_RUSTUP_DEFAULT],
    },
];
