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
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, area);
}

fn render_content(f: &mut Frame, area: Rect, app: &TuiApp) {
    let sections = ["VM Settings", "Security", "Tracing", "Tools", "Secrets"];
    let selected_section = match app.current_section {
        Section::VmSettings => 0,
        Section::Security => 1,
        Section::Tracing => 2,
        Section::Tools => 3,
        Section::Secrets => 4,
    };

    let tabs = Tabs::new(sections.iter().map(|s| Line::from(*s)).collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL).title("Sections"))
        .select(selected_section)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    f.render_widget(tabs, chunks[0]);

    match app.current_section {
        Section::VmSettings => render_vm_settings(f, chunks[1], app),
        Section::Security => render_security(f, chunks[1], app),
        Section::Tracing => render_tracing(f, chunks[1], app),
        Section::Tools => render_tools(f, chunks[1], app),
        Section::Secrets => render_secrets(f, chunks[1], app),
    }
}

fn render_vm_settings(f: &mut Frame, area: Rect, app: &TuiApp) {
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
        format!("CPUs: {}", app.vm_cpus),
        format!("Memory: {}", app.vm_memory),
        format!("Disk: {}", app.vm_disk),
    ];

    let list_items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Yellow)
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
            .title("VM Configuration"),
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
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Green)
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
                    .fg(Color::Yellow)
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
                    .fg(Color::Yellow)
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
                    .fg(Color::Yellow)
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
                    .fg(Color::Yellow)
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
                    .fg(Color::Yellow)
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
                    .fg(Color::Yellow)
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
    let env_file = app.secrets_env_file.as_deref().unwrap_or("<none>");
    let items = vec![
        format!("Environment file: {}", env_file),
        "".to_string(),
        "Press 'e' to specify .env file path".to_string(),
    ];

    let list_items: Vec<ListItem> = items.into_iter().map(ListItem::new).collect();
    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Secrets & Environment"),
    );

    f.render_widget(list, area);
}

fn render_footer(f: &mut Frame, area: Rect, app: &TuiApp) {
    let help_text = if app.is_ready_to_submit() {
        "Tab: Sections | ←→: Security tabs | ↑↓: Navigate | Space: Toggle | Enter: Create | q: Quit"
    } else {
        "Tab: Sections | ←→: Security tabs | ↑↓: Navigate | Space: Toggle | q: Quit | (Enter VM name first)"
    };

    let footer = Paragraph::new(help_text)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(footer, area);
}

fn checkbox(label: &str, checked: bool) -> String {
    let symbol = if checked { "[✓]" } else { "[ ]" };
    format!("{} {}", symbol, label)
}
