use thiserror::Error;

#[derive(Error, Debug)]
pub enum VmError {
    #[error("VM '{name}' in unexpected state: {actual} (expected {expected})")]
    UnexpectedState {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("VM '{name}' does not exist")]
    VmNotFound { name: String },

    #[error("VM '{name}' already exists")]
    VmAlreadyExists { name: String },

    #[error("Backend not available: {backend}")]
    BackendNotAvailable { backend: String },

    #[error("Command execution failed: {command}")]
    CommandFailed {
        command: String,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

impl VmError {
    pub fn unexpected_state(
        name: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::UnexpectedState {
            name: name.into(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }
}
