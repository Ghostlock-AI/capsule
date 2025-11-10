// Input handling for TUI

use super::app::{Section, SecurityTab, TuiApp};
use crate::config::MountMode;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key_event(app: &mut TuiApp, key: KeyEvent) {
    // Handle text input mode for VM name
    if app.input_mode && matches!(app.current_section, Section::VmSettings) && app.selected_index == 0
    {
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

    match key.code {
        KeyCode::Tab => cycle_section(app),
        KeyCode::Up => navigate_up(app),
        KeyCode::Down => navigate_down(app),
        KeyCode::Char(' ') => toggle_current_item(app),
        KeyCode::Left => cycle_security_tab_left(app),
        KeyCode::Right => cycle_security_tab_right(app),
        KeyCode::Char('i') if matches!(app.current_section, Section::VmSettings) && app.selected_index == 0 => {
            app.input_mode = true;
        }
        KeyCode::Char('+') => increment_value(app),
        KeyCode::Char('-') => decrement_value(app),
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

fn get_max_index(app: &TuiApp) -> usize {
    match app.current_section {
        Section::VmSettings => 3,
        Section::Security => match app.current_security_tab {
            SecurityTab::Profile => 0,
            SecurityTab::Mounts => 1,
            SecurityTab::Processes => 1,
            SecurityTab::Network => 1,
            SecurityTab::AppArmor => 1,
        },
        Section::Tracing => 7,
        Section::Tools => 13,
        Section::Secrets => 0,
    }
}

fn cycle_section(app: &mut TuiApp) {
    app.current_section = match app.current_section {
        Section::VmSettings => Section::Security,
        Section::Security => Section::Tracing,
        Section::Tracing => Section::Tools,
        Section::Tools => Section::Secrets,
        Section::Secrets => Section::VmSettings,
    };
    app.selected_index = 0;
}

fn cycle_security_tab_left(app: &mut TuiApp) {
    if matches!(app.current_section, Section::Security) {
        app.current_security_tab = match app.current_security_tab {
            SecurityTab::Profile => SecurityTab::AppArmor,
            SecurityTab::Mounts => SecurityTab::Profile,
            SecurityTab::Processes => SecurityTab::Mounts,
            SecurityTab::Network => SecurityTab::Processes,
            SecurityTab::AppArmor => SecurityTab::Network,
        };
        app.selected_index = 0;
    }
}

fn cycle_security_tab_right(app: &mut TuiApp) {
    if matches!(app.current_section, Section::Security) {
        app.current_security_tab = match app.current_security_tab {
            SecurityTab::Profile => SecurityTab::Mounts,
            SecurityTab::Mounts => SecurityTab::Processes,
            SecurityTab::Processes => SecurityTab::Network,
            SecurityTab::Network => SecurityTab::AppArmor,
            SecurityTab::AppArmor => SecurityTab::Profile,
        };
        app.selected_index = 0;
    }
}

fn toggle_current_item(app: &mut TuiApp) {
    match app.current_section {
        Section::VmSettings => {
            // No toggles in VM settings (use +/- for numeric values)
        }
        Section::Security => toggle_security_item(app),
        Section::Tracing => toggle_tracing_item(app),
        Section::Tools => toggle_tools_item(app),
        Section::Secrets => {
            // No toggles in secrets section
        }
    }
}

fn toggle_security_item(app: &mut TuiApp) {
    match app.current_security_tab {
        SecurityTab::Profile => {
            // Cycle through profiles
            if app.selected_index == 0 {
                app.security_profile = match app.security_profile.as_str() {
                    "minimal" => "developer".to_string(),
                    "developer" => "strict".to_string(),
                    "strict" => "custom".to_string(),
                    _ => "minimal".to_string(),
                };
            }
        }
        SecurityTab::Mounts => {
            if app.selected_index == 0 {
                app.workspace_only = !app.workspace_only;
            } else if app.selected_index == 1 {
                app.allow_home = match app.allow_home {
                    MountMode::None => MountMode::ReadOnly,
                    MountMode::ReadOnly => MountMode::Writable,
                    MountMode::Writable => MountMode::None,
                };
            }
        }
        SecurityTab::Processes => {
            if app.selected_index == 0 {
                app.no_background_processes = !app.no_background_processes;
            } else if app.selected_index == 1 {
                app.restrict_fork = !app.restrict_fork;
            }
        }
        SecurityTab::Network => {
            if app.selected_index == 0 {
                app.network_enabled = !app.network_enabled;
            } else if app.selected_index == 1 {
                app.localhost_only = !app.localhost_only;
            }
        }
        SecurityTab::AppArmor => {
            if app.selected_index == 0 {
                app.apparmor_enabled = !app.apparmor_enabled;
            } else if app.selected_index == 1 {
                app.apparmor_enforce = !app.apparmor_enforce;
            }
        }
    }
}

fn toggle_tracing_item(app: &mut TuiApp) {
    match app.selected_index {
        0 => app.tracing_enabled = !app.tracing_enabled,
        3 => app.trace_process = !app.trace_process,
        4 => app.trace_file = !app.trace_file,
        5 => app.trace_network = !app.trace_network,
        6 => app.trace_credentials = !app.trace_credentials,
        7 => app.trace_signal = !app.trace_signal,
        _ => {}
    }
}

fn toggle_tools_item(app: &mut TuiApp) {
    match app.selected_index {
        1 => app.tools_python = !app.tools_python,
        2 => app.tools_node = !app.tools_node,
        3 => app.tools_rust = !app.tools_rust,
        4 => app.tools_go = !app.tools_go,
        5 => app.tools_java = !app.tools_java,
        8 => app.tools_claude = !app.tools_claude,
        9 => app.tools_codex = !app.tools_codex,
        10 => app.tools_ollama = !app.tools_ollama,
        13 => app.tools_ffmpeg = !app.tools_ffmpeg,
        _ => {}
    }
}

fn increment_value(app: &mut TuiApp) {
    if matches!(app.current_section, Section::VmSettings) {
        match app.selected_index {
            1 => {
                if app.vm_cpus < 16 {
                    app.vm_cpus += 1;
                }
            }
            2 => {
                // Increment memory: 1G -> 2G -> 4G -> 8G -> 16G
                app.vm_memory = match app.vm_memory.as_str() {
                    "1G" => "2G".to_string(),
                    "2G" => "4G".to_string(),
                    "4G" => "8G".to_string(),
                    "8G" => "16G".to_string(),
                    _ => app.vm_memory.clone(),
                };
            }
            3 => {
                // Increment disk: 8G -> 16G -> 32G -> 64G -> 128G
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
}

fn decrement_value(app: &mut TuiApp) {
    if matches!(app.current_section, Section::VmSettings) {
        match app.selected_index {
            1 => {
                if app.vm_cpus > 1 {
                    app.vm_cpus -= 1;
                }
            }
            2 => {
                // Decrement memory: 16G -> 8G -> 4G -> 2G -> 1G
                app.vm_memory = match app.vm_memory.as_str() {
                    "16G" => "8G".to_string(),
                    "8G" => "4G".to_string(),
                    "4G" => "2G".to_string(),
                    "2G" => "1G".to_string(),
                    _ => app.vm_memory.clone(),
                };
            }
            3 => {
                // Decrement disk: 128G -> 64G -> 32G -> 16G -> 8G
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
}
