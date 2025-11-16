// Security profile generation module
//
// This module provides functionality for generating security profiles (AppArmor, Seccomp)
// from configuration and exporting them as portable cloud-init snippets.

pub mod apparmor;
pub mod export;

// Re-export main functions
pub use apparmor::generate_apparmor_profile;
pub use export::{export_security_profiles, SecurityProfileExport};
