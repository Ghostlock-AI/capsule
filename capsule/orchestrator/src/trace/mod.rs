//! Tracee configuration generation module
//!
//! This module provides functionality to generate dynamic Tracee configurations
//! based on event categories, scope filters, and tracing profiles.

pub mod config_gen;
pub mod events;

pub use config_gen::{generate_tracee_config, TraceeConfig};
pub use events::{EventCategory, get_events_for_category};
