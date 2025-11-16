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
    /// Installs ffmpeg for video/audio processing
    #[value(name = "ffmpeg")]
    Ffmpeg,
    /// Installs Ollama for running LLMs locally
    #[value(name = "ollama")]
    Ollama,
    /// Installs GitHub CLI (gh) via apt
    #[value(name = "gh", alias = "github", alias = "github-cli")]
    GitHubCli,
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
const STEP_SNAP_FFMPEG: &str = "sudo snap install --edge ffmpeg";
const STEP_SNAP_OLLAMA: &str = "sudo snap install ollama";
const STEP_GH_ADD_KEY: &str = "curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg";
const STEP_GH_CHMOD_KEY: &str = "sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg";
const STEP_GH_ADD_REPO: &str = r#"echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null"#;
const STEP_GH_APT_UPDATE: &str = "sudo apt-get update";
const STEP_GH_APT_INSTALL: &str = "sudo apt-get install -y gh";
const STEP_GIT_CONFIG_NAME: &str = r#"if [ -n "$GIT_USER_NAME" ]; then sudo -u agent git config --global user.name "$GIT_USER_NAME"; echo "Git user.name set to $GIT_USER_NAME"; fi"#;
const STEP_GIT_CONFIG_EMAIL: &str = r#"if [ -n "$GIT_USER_EMAIL" ]; then sudo -u agent git config --global user.email "$GIT_USER_EMAIL"; echo "Git user.email set to $GIT_USER_EMAIL"; fi"#;
const STEP_GIT_CONFIG_CREDENTIAL: &str = "sudo -u agent git config --global credential.helper store";
const STEP_GH_AUTH: &str = r#"if [ -n "$GITHUB_TOKEN" ]; then echo "$GITHUB_TOKEN" | sudo -u agent gh auth login --with-token && echo "GitHub CLI authenticated"; else echo "GITHUB_TOKEN not set, skipping authentication"; fi"#;

const TOOL_DEFINITIONS: [ToolDefinition; 9] = [
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
    ToolDefinition {
        kind: ToolKind::Ffmpeg,
        name: "ffmpeg",
        description: "Installs ffmpeg via snap for video/audio processing",
        setup_steps: &[STEP_SNAP_FFMPEG],
    },
    ToolDefinition {
        kind: ToolKind::Ollama,
        name: "ollama",
        description: "Installs Ollama via snap for running LLMs locally",
        setup_steps: &[STEP_SNAP_OLLAMA],
    },
    ToolDefinition {
        kind: ToolKind::GitHubCli,
        name: "gh",
        description: "Installs GitHub CLI (gh) via apt for GitHub operations",
        setup_steps: &[
            STEP_GH_ADD_KEY,
            STEP_GH_CHMOD_KEY,
            STEP_GH_ADD_REPO,
            STEP_GH_APT_UPDATE,
            STEP_GH_APT_INSTALL,
            STEP_GIT_CONFIG_NAME,
            STEP_GIT_CONFIG_EMAIL,
            STEP_GIT_CONFIG_CREDENTIAL,
            STEP_GH_AUTH,
        ],
    },
];
