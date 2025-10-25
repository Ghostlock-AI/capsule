use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VmError {
    #[error("VM operation failed: {operation}")]
    OperationFailed {
        operation: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("VM '{name}' in unexpected state: {actual} (expected {expected})")]
    UnexpectedState {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("VM operation timed out after {duration:?}: {operation}")]
    Timeout {
        operation: String,
        duration: Duration,
    },

    #[error("Silent failure detected in {operation}: {details}")]
    SilentFailure {
        operation: String,
        details: String,
        exit_code: i32,
        stderr: String,
    },

    #[error("VM '{name}' does not exist")]
    VmNotFound { name: String },

    #[error("VM '{name}' already exists")]
    VmAlreadyExists { name: String },

    #[error("Invalid configuration: {details}")]
    InvalidConfig { details: String },

    #[error("Health check failed: {check_name}")]
    HealthCheckFailed {
        check_name: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Mount validation failed: {details}")]
    MountFailed { details: String },

    #[error("Network not ready for VM '{name}'")]
    NetworkNotReady { name: String },

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
    pub fn operation_failed(operation: impl Into<String>, source: anyhow::Error) -> Self {
        Self::OperationFailed {
            operation: operation.into(),
            source,
        }
    }

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

    pub fn timeout(operation: impl Into<String>, duration: Duration) -> Self {
        Self::Timeout {
            operation: operation.into(),
            duration,
        }
    }

    pub fn silent_failure(
        operation: impl Into<String>,
        details: impl Into<String>,
        exit_code: i32,
        stderr: impl Into<String>,
    ) -> Self {
        Self::SilentFailure {
            operation: operation.into(),
            details: details.into(),
            exit_code,
            stderr: stderr.into(),
        }
    }
}
