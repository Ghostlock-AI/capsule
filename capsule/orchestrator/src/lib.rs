// Library interface for Capsule orchestrator
// Exports modules for use in examples and tests

pub mod config;
pub mod security;

// Re-export commonly used types
pub use config::{CapsuleConfig, SecurityProfile, VmSettings};
pub use security::{generate_apparmor_profile, export_security_profiles, SecurityProfileExport};
