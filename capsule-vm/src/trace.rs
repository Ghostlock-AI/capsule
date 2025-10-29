use crate::retry::{RetryConfig, retry_operation};
use crate::vm_backend::VmBackend;
use anyhow::{Result, bail};
use std::fmt;
use std::time::Duration;

const SERVICE_NAME: &str = "capsule-tracee.service";

#[derive(Debug, Clone)]
pub enum TraceeState {
    Active,
    Activating,
    Inactive,
    Failed(String),
    Unknown(String),
}

impl TraceeState {
    pub fn short_label(&self) -> &'static str {
        match self {
            TraceeState::Active => "running",
            TraceeState::Activating => "starting",
            TraceeState::Inactive => "stopped",
            TraceeState::Failed(_) => "failed",
            TraceeState::Unknown(_) => "unknown",
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            TraceeState::Failed(msg) | TraceeState::Unknown(msg) => Some(msg.as_str()),
            _ => None,
        }
    }
}

impl fmt::Display for TraceeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TraceeState::Active => write!(f, "Tracee service is running"),
            TraceeState::Activating => write!(f, "Tracee service is starting"),
            TraceeState::Inactive => write!(f, "Tracee service is inactive"),
            TraceeState::Failed(msg) => write!(f, "Tracee service failed: {}", msg),
            TraceeState::Unknown(msg) => write!(f, "Tracee service unknown status: {}", msg),
        }
    }
}

pub fn service_state(backend: &dyn VmBackend, vm_name: &str) -> Result<TraceeState> {
    let raw_state = match backend.exec(vm_name, &["sudo", "systemctl", "is-active", SERVICE_NAME]) {
        Ok(output) => output,
        Err(err) => return Ok(TraceeState::Unknown(err.to_string())),
    };

    let state = match raw_state.trim() {
        "active" => TraceeState::Active,
        "activating" | "reloading" => TraceeState::Activating,
        "inactive" | "deactivating" => TraceeState::Inactive,
        "failed" => TraceeState::Failed(fetch_status(backend, vm_name)?),
        other => TraceeState::Unknown(other.to_string()),
    };

    Ok(state)
}

pub fn wait_for_tracee(backend: &dyn VmBackend, vm_name: &str) -> Result<()> {
    retry_operation(
        || match service_state(backend, vm_name)? {
            TraceeState::Active => Ok(()),
            TraceeState::Activating => bail!("Tracee still starting"),
            TraceeState::Inactive => bail!("Tracee inactive"),
            TraceeState::Failed(reason) => bail!("Tracee failed: {}", reason),
            TraceeState::Unknown(detail) => bail!("Tracee state unknown: {}", detail),
        },
        RetryConfig::with_delays(10, Duration::from_secs(2), Duration::from_secs(6)),
        &format!("wait for Tracee on {}", vm_name),
    )
}

fn fetch_status(backend: &dyn VmBackend, vm_name: &str) -> Result<String> {
    match backend.exec(
        vm_name,
        &[
            "sudo",
            "systemctl",
            "status",
            SERVICE_NAME,
            "--no-pager",
            "--lines",
            "20",
        ],
    ) {
        Ok(output) => Ok(output),
        Err(err) => Ok(format!("unable to fetch status: {}", err)),
    }
}

#[cfg(test)]
mod tests {
    use super::TraceeState;

    #[test]
    fn short_labels_are_human_friendly() {
        assert_eq!(TraceeState::Active.short_label(), "running");
        assert_eq!(TraceeState::Activating.short_label(), "starting");
        assert_eq!(TraceeState::Inactive.short_label(), "stopped");
        assert_eq!(TraceeState::Failed("x".into()).short_label(), "failed");
        assert_eq!(TraceeState::Unknown("y".into()).short_label(), "unknown");
    }
}
