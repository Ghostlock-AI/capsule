// Input handling for TUI

use super::app::TuiApp;
use crate::config::MountMode;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key_event(app: &mut TuiApp, key: KeyEvent) {
    // Handle text input mode for VM name (index 1)
    if app.input_mode && app.selected_index == 1 {
        match key.code {
            KeyCode::Char(c) => {
                app.vm_name.push(c);
            }
            KeyCode::Backspace => {
                app.vm_name.pop();
            }
            KeyCode::Esc | KeyCode::Enter => {
                app.input_mode = false;
            }
            _ => {}
        }
        return;
    }

    // Handle text input mode for secrets editor
    if app.input_mode && app.selected_index > 30 {
        match key.code {
            KeyCode::Char(c) => {
                app.secrets_text.push(c);
            }
            KeyCode::Backspace => {
                app.secrets_text.pop();
            }
            KeyCode::Enter => {
                app.secrets_text.push('\n');
            }
            KeyCode::Esc => {
                app.input_mode = false;
                app.parse_secrets(); // Parse when exiting edit mode
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Up => navigate_up(app),
        KeyCode::Down => navigate_down(app),
        KeyCode::Char(' ') => toggle_current_item(app),
        KeyCode::Left => decrement_value(app),
        KeyCode::Right => increment_value(app),
        KeyCode::Char('i') if app.selected_index == 1 => {
            app.input_mode = true;
        }
        KeyCode::Char('e') if app.selected_index > 30 => {
            app.input_mode = true;
        }
        KeyCode::Char('+') | KeyCode::Char('=') => increment_value(app),
        KeyCode::Char('-') | KeyCode::Char('_') => decrement_value(app),
        _ => {}
    }
}

fn navigate_up(app: &mut TuiApp) {
    if app.selected_index > 0 {
        app.selected_index -= 1;
    }
}

fn navigate_down(app: &mut TuiApp) {
    let max_index = get_max_index(app);
    if app.selected_index < max_index {
        app.selected_index += 1;
    }
}

fn get_max_index(_app: &TuiApp) -> usize {
    // Unified form has ~35+ items total
    40
}

fn toggle_current_item(app: &mut TuiApp) {
    match app.selected_index {
        // Basic Settings - no toggles (indices 1-4)

        // Security section (indices 6-13)
        6 => {
            // Security profile - cycle through profiles
            app.security_profile = match app.security_profile.as_str() {
                "minimal" => "developer".to_string(),
                "developer" => "strict".to_string(),
                "strict" => "custom".to_string(),
                _ => "minimal".to_string(),
            };
        }
        7 => app.workspace_only = !app.workspace_only,
        8 => {
            // Home directory - cycle through modes
            app.allow_home = match app.allow_home {
                MountMode::None => MountMode::ReadOnly,
                MountMode::ReadOnly => MountMode::Writable,
                MountMode::Writable => MountMode::None,
            };
        }
        9 => app.no_background_processes = !app.no_background_processes,
        10 => app.restrict_fork = !app.restrict_fork,
        11 => app.network_enabled = !app.network_enabled,
        12 => app.localhost_only = !app.localhost_only,
        13 => app.apparmor_enabled = !app.apparmor_enabled,
        14 => app.apparmor_enforce = !app.apparmor_enforce,

        // Tracing section (indices 16-21)
        16 => app.tracing_enabled = !app.tracing_enabled,
        17 => app.trace_process = !app.trace_process,
        18 => app.trace_file = !app.trace_file,
        19 => app.trace_network = !app.trace_network,
        20 => app.trace_credentials = !app.trace_credentials,
        21 => app.trace_signal = !app.trace_signal,

        // Tools section (indices 23-31)
        23 => app.tools_python = !app.tools_python,
        24 => app.tools_node = !app.tools_node,
        25 => app.tools_rust = !app.tools_rust,
        26 => app.tools_go = !app.tools_go,
        27 => app.tools_java = !app.tools_java,
        28 => app.tools_claude = !app.tools_claude,
        29 => app.tools_codex = !app.tools_codex,
        30 => app.tools_ollama = !app.tools_ollama,
        31 => app.tools_ffmpeg = !app.tools_ffmpeg,

        _ => {}
    }
}

fn increment_value(app: &mut TuiApp) {
    match app.selected_index {
        2 => {
            // CPUs
            if app.vm_cpus < 64 {
                app.vm_cpus += 1;
            }
        }
        3 => {
            // Memory: 1G -> 2G -> 4G -> 8G -> 16G
            app.vm_memory = match app.vm_memory.as_str() {
                "1G" => "2G".to_string(),
                "2G" => "4G".to_string(),
                "4G" => "8G".to_string(),
                "8G" => "16G".to_string(),
                _ => app.vm_memory.clone(),
            };
        }
        4 => {
            // Disk: 8G -> 16G -> 32G -> 64G -> 128G
            app.vm_disk = match app.vm_disk.as_str() {
                "8G" => "16G".to_string(),
                "16G" => "32G".to_string(),
                "32G" => "64G".to_string(),
                "64G" => "128G".to_string(),
                _ => app.vm_disk.clone(),
            };
        }
        _ => {}
    }
}

fn decrement_value(app: &mut TuiApp) {
    match app.selected_index {
        2 => {
            // CPUs
            if app.vm_cpus > 1 {
                app.vm_cpus -= 1;
            }
        }
        3 => {
            // Memory: 16G -> 8G -> 4G -> 2G -> 1G
            app.vm_memory = match app.vm_memory.as_str() {
                "16G" => "8G".to_string(),
                "8G" => "4G".to_string(),
                "4G" => "2G".to_string(),
                "2G" => "1G".to_string(),
                _ => app.vm_memory.clone(),
            };
        }
        4 => {
            // Disk: 128G -> 64G -> 32G -> 16G -> 8G
            app.vm_disk = match app.vm_disk.as_str() {
                "128G" => "64G".to_string(),
                "64G" => "32G".to_string(),
                "32G" => "16G".to_string(),
                "16G" => "8G".to_string(),
                _ => app.vm_disk.clone(),
            };
        }
        _ => {}
    }
}
