// UI rendering for TUI using ratatui

use super::app::{Section, SecurityTab, TuiApp};
use crate::config::MountMode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame,
};

pub fn render_ui(f: &mut Frame, app: &TuiApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    render_header(f, chunks[0]);
    render_content(f, chunks[1], app);
    render_footer(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: Rect) {
    let title = Paragraph::new("Capsule VM Configuration")
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, area);
}

fn render_content(f: &mut Frame, area: Rect, app: &TuiApp) {
    // Build all form items in one list
    let mut items = Vec::new();
    let mut item_index = 0;

    // Basic Settings section
    items.push("=== BASIC SETTINGS ===".to_string());
    items.push(format!(
        "Name: {}{}",
        if app.vm_name.is_empty() {
            "<press 'i' to enter name>"
        } else {
            &app.vm_name
        },
        if app.selected_index == item_index + 1 && app.input_mode {
            "_"
        } else {
            ""
        }
    ));
    item_index += 1;

    items.push(format!("CPUs: {} (min: 1, max: 64) [←→ to adjust]", app.vm_cpus));
    item_index += 1;

    items.push(format!("Memory: {} (1G/2G/4G/8G/16G) [←→ to adjust]", app.vm_memory));
    item_index += 1;

    items.push(format!("Disk: {} (8G/16G/32G/64G/128G) [←→ to adjust]", app.vm_disk));
    item_index += 1;

    items.push("".to_string());

    // Security section
    items.push("=== SECURITY ===".to_string());
    items.push(format!("Profile: {} (space to cycle)", app.security_profile));
    item_index += 1;

    items.push(checkbox("Workspace only (no host mounts)", app.workspace_only));
    item_index += 1;

    let home_mode = match app.allow_home {
        MountMode::None => "none",
        MountMode::ReadOnly => "readonly",
        MountMode::Writable => "writable",
    };
    items.push(format!("Home directory: {} (space to cycle)", home_mode));
    item_index += 1;

    items.push(checkbox("No background process persistence", app.no_background_processes));
    item_index += 1;

    items.push(checkbox("Restrict process forking", app.restrict_fork));
    item_index += 1;

    items.push(checkbox("Network enabled", app.network_enabled));
    item_index += 1;

    items.push(checkbox("Localhost only (block external)", app.localhost_only));
    item_index += 1;

    items.push(checkbox("AppArmor enabled", app.apparmor_enabled));
    item_index += 1;

    items.push(checkbox("AppArmor enforce mode", app.apparmor_enforce));
    item_index += 1;

    items.push("".to_string());

    // Tracing section
    items.push("=== KERNEL TRACING ===".to_string());
    items.push(checkbox("Tracing enabled", app.tracing_enabled));
    item_index += 1;

    items.push(checkbox("  Process events (exec, exit)", app.trace_process));
    item_index += 1;

    items.push(checkbox("  File events (open, close)", app.trace_file));
    item_index += 1;

    items.push(checkbox("  Network events (connect, bind)", app.trace_network));
    item_index += 1;

    items.push(checkbox("  Credential events (setuid)", app.trace_credentials));
    item_index += 1;

    items.push(checkbox("  Signal events (kill)", app.trace_signal));
    item_index += 1;

    items.push("".to_string());

    // Tools section
    items.push("=== TOOLS & RUNTIMES ===".to_string());
    items.push(checkbox("Python 3", app.tools_python));
    item_index += 1;

    items.push(checkbox("Node.js", app.tools_node));
    item_index += 1;

    items.push(checkbox("Rust", app.tools_rust));
    item_index += 1;

    items.push(checkbox("Go", app.tools_go));
    item_index += 1;

    items.push(checkbox("Java", app.tools_java));
    item_index += 1;

    items.push(checkbox("Claude Code CLI", app.tools_claude));
    item_index += 1;

    items.push(checkbox("OpenAI Codex CLI", app.tools_codex));
    item_index += 1;

    items.push(checkbox("Ollama (local LLMs)", app.tools_ollama));
    item_index += 1;

    items.push(checkbox("FFmpeg", app.tools_ffmpeg));
    item_index += 1;

    items.push("".to_string());

    // Secrets section header
    items.push("=== SECRETS & ENVIRONMENT ===".to_string());
    items.push("Press 'e' to edit environment variables".to_string());
    items.push("Example: API_KEY=sk_test_1234567890abcdef".to_string());

    if !app.secrets_text.is_empty() {
        items.push("".to_string());
        items.push("Current .env content:".to_string());
        for line in app.secrets_text.lines() {
            items.push(format!("  {}", line));
        }
    }

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("VM Configuration Form - Use ↑↓ to navigate, Space to toggle, ←→ to adjust"),
    );

    f.render_widget(list, area);
}

fn render_basic_settings(f: &mut Frame, area: Rect, app: &TuiApp) {
    let items = vec![
        format!(
            "Name: {}{}",
            if app.vm_name.is_empty() {
                "<enter name>"
            } else {
                &app.vm_name
            },
            if app.selected_index == 0 && app.input_mode {
                "_"
            } else {
                ""
            }
        ),
        format!("CPUs: {} (min: 1, max: 64) [←→ to adjust]", app.vm_cpus),
        format!("Memory: {} (1G/2G/4G/8G/16G) [←→ to adjust]", app.vm_memory),
        format!("Disk: {} (8G/16G/32G/64G/128G) [←→ to adjust]", app.vm_disk),
    ];

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Basic Settings - Press 'i' on Name to edit, use +/- on values"),
    );

    f.render_widget(list, area);
}

fn render_security(f: &mut Frame, area: Rect, app: &TuiApp) {
    let security_tabs = ["Profile", "Mounts", "Processes", "Network", "AppArmor"];
    let selected_tab = match app.current_security_tab {
        SecurityTab::Profile => 0,
        SecurityTab::Mounts => 1,
        SecurityTab::Processes => 2,
        SecurityTab::Network => 3,
        SecurityTab::AppArmor => 4,
    };

    let tabs = Tabs::new(security_tabs.iter().map(|s| Line::from(*s)).collect::<Vec<_>>())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Security Options"),
        )
        .select(selected_tab)
        .style(Style::default())
        .highlight_style(
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    f.render_widget(tabs, chunks[0]);

    match app.current_security_tab {
        SecurityTab::Profile => render_security_profile(f, chunks[1], app),
        SecurityTab::Mounts => render_mounts(f, chunks[1], app),
        SecurityTab::Processes => render_processes(f, chunks[1], app),
        SecurityTab::Network => render_network(f, chunks[1], app),
        SecurityTab::AppArmor => render_apparmor(f, chunks[1], app),
    }
}

fn render_security_profile(f: &mut Frame, area: Rect, app: &TuiApp) {
    let items = vec![
        format!("Profile: {}", app.security_profile),
        "".to_string(),
        "Available profiles:".to_string(),
        "  minimal    - Strict isolation, minimal access".to_string(),
        "  developer  - Balanced for development (default)".to_string(),
        "  strict     - Maximum security restrictions".to_string(),
    ];

    let list_items: Vec<ListItem> = items.into_iter().map(ListItem::new).collect();
    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Security Profile"),
    );

    f.render_widget(list, area);
}

fn render_mounts(f: &mut Frame, area: Rect, app: &TuiApp) {
    let home_mode = match app.allow_home {
        MountMode::None => "none",
        MountMode::ReadOnly => "readonly",
        MountMode::Writable => "writable",
    };

    let items = vec![
        checkbox("Workspace only (no host mounts)", app.workspace_only),
        format!("Home directory: {}", home_mode),
    ];

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(list_items).block(Block::default().borders(Borders::ALL).title("Mount Policy"));

    f.render_widget(list, area);
}

fn render_processes(f: &mut Frame, area: Rect, app: &TuiApp) {
    let items = vec![
        checkbox(
            "No background process persistence",
            app.no_background_processes,
        ),
        checkbox("Restrict process forking", app.restrict_fork),
    ];

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list =
        List::new(list_items).block(Block::default().borders(Borders::ALL).title("Process Policy"));

    f.render_widget(list, area);
}

fn render_network(f: &mut Frame, area: Rect, app: &TuiApp) {
    let items = vec![
        checkbox("Network enabled", app.network_enabled),
        checkbox("Localhost only (block external)", app.localhost_only),
    ];

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list =
        List::new(list_items).block(Block::default().borders(Borders::ALL).title("Network Policy"));

    f.render_widget(list, area);
}

fn render_apparmor(f: &mut Frame, area: Rect, app: &TuiApp) {
    let items = vec![
        checkbox("AppArmor enabled", app.apparmor_enabled),
        checkbox("Enforce mode (vs complain)", app.apparmor_enforce),
    ];

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("AppArmor Configuration"),
    );

    f.render_widget(list, area);
}

fn render_tracing(f: &mut Frame, area: Rect, app: &TuiApp) {
    let items = vec![
        checkbox("Tracing enabled", app.tracing_enabled),
        "".to_string(),
        "Event Categories:".to_string(),
        checkbox("  Process (exec, exit)", app.trace_process),
        checkbox("  File (open, close, rename)", app.trace_file),
        checkbox("  Network (connect, bind)", app.trace_network),
        checkbox("  Credentials (setuid, setgid)", app.trace_credentials),
        checkbox("  Signal (kill, signal)", app.trace_signal),
    ];

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list =
        List::new(list_items).block(Block::default().borders(Borders::ALL).title("Kernel Tracing"));

    f.render_widget(list, area);
}

fn render_tools(f: &mut Frame, area: Rect, app: &TuiApp) {
    let items = vec![
        "Language Runtimes:".to_string(),
        checkbox("  Python 3", app.tools_python),
        checkbox("  Node.js", app.tools_node),
        checkbox("  Rust", app.tools_rust),
        checkbox("  Go", app.tools_go),
        checkbox("  Java", app.tools_java),
        "".to_string(),
        "AI/ML Tools:".to_string(),
        checkbox("  Claude Code CLI", app.tools_claude),
        checkbox("  OpenAI Codex CLI", app.tools_codex),
        checkbox("  Ollama (local LLMs)", app.tools_ollama),
        "".to_string(),
        "Utilities:".to_string(),
        checkbox("  FFmpeg", app.tools_ffmpeg),
    ];

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title("Tools & Software"));

    f.render_widget(list, area);
}

fn render_secrets(f: &mut Frame, area: Rect, app: &TuiApp) {
    let mut text = if app.secrets_text.is_empty() {
        "# Enter environment variables in .env format:\n# KEY=VALUE\n# One per line\n#\n# Example:\n# API_KEY=sk_test_1234567890abcdef\n# DATABASE_URL=postgresql://localhost/mydb\n# DEBUG=true\n\n".to_string()
    } else {
        app.secrets_text.clone()
    };

    if app.input_mode && matches!(app.current_section, Section::Secrets) {
        text.push('_');
    }

    let title = if app.input_mode && matches!(app.current_section, Section::Secrets) {
        "Secrets & Environment - [EDITING] Press Esc when done"
    } else {
        "Secrets & Environment - Press 'e' to edit .env content"
    };

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(title))
        .style(Style::default());

    f.render_widget(paragraph, area);
}

fn render_footer(f: &mut Frame, area: Rect, app: &TuiApp) {
    let help_text = if app.is_ready_to_submit() {
        "Tab: Sections | ←→: Adjust/Tabs | ↑↓: Navigate | Space: Toggle | i/e: Edit | Enter: Create | q: Quit"
    } else {
        "Tab: Sections | ←→: Adjust/Tabs | ↑↓: Navigate | Space: Toggle | i/e: Edit | q: Quit | (Enter VM name first)"
    };

    let footer = Paragraph::new(help_text)
        .style(Style::default())
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(footer, area);
}

fn checkbox(label: &str, checked: bool) -> String {
    let symbol = if checked { "[✓]" } else { "[ ]" };
    format!("{} {}", symbol, label)
}
