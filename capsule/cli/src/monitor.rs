//! Live process monitoring TUI using ratatui
//!
//! Shows real-time process list with keyboard navigation

use crate::ipc::StateClient;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crossterm::event::{EnableMouseCapture, DisableMouseCapture};
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Table, Row, Cell, TableState, ListState,
        HighlightSpacing,
    },
};
use state::{AgentState, LiveProcess};
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// TUI application state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusPane {
    Processes,
    Actions,
}

/// TUI application state
struct MonitorApp {
    /// Shared agent state from tracker
    agent_state: Arc<RwLock<AgentState>>,
    /// Process table selection state
    process_state: TableState,
    /// Syscall scroll position (line offset)
    syscall_scroll: u16,
    /// Syscall selection state (index in full list)
    syscall_selected: Option<usize>,
    /// Whether to quit the app
    should_quit: bool,
    /// Auto-refresh interval
    refresh_rate: Duration,
    /// Last refresh time for auto-update
    last_refresh: Instant,
    /// Auto-scroll mode for syscalls
    auto_scroll: bool,
    /// Which pane is focused
    focus: FocusPane,
    /// Cached rects for hit testing mouse clicks
    process_rect: Rect,
    actions_rect: Rect,
}

impl MonitorApp {
    /// Create new monitor app with shared state
    fn new(agent_state: Arc<RwLock<AgentState>>) -> Self {
        Self {
            agent_state,
            process_state: TableState::default(),
            syscall_scroll: 0,
            syscall_selected: None,
            should_quit: false,
            refresh_rate: Duration::from_millis(500), // 2 FPS refresh
            last_refresh: Instant::now(),
            auto_scroll: true, // Start with auto-scroll enabled
            focus: FocusPane::Processes,
            process_rect: Rect::default(),
            actions_rect: Rect::default(),
        }
    }

    /// Handle keyboard input
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            // Navigation depends on focused pane
            KeyCode::Up => match self.focus {
                FocusPane::Processes => {
                    let new = self.process_state.selected().unwrap_or(0).saturating_sub(1);
                    self.process_state.select(Some(new));
                }
                FocusPane::Actions => {
                    self.auto_scroll = false;
                    // Move selection up; if none, select last visible
                    if let Some(sel) = self.syscall_selected {
                        self.syscall_selected = sel.checked_sub(1);
                    }
                }
            },
            KeyCode::Down => match self.focus {
                FocusPane::Processes => {
                    let new = self.process_state.selected().unwrap_or(0).saturating_add(1);
                    self.process_state.select(Some(new));
                }
                FocusPane::Actions => {
                    self.auto_scroll = false;
                    if let Some(sel) = self.syscall_selected {
                        self.syscall_selected = Some(sel.saturating_add(1));
                    }
                }
            },
            // Syscall scrolling (actions pane)
            KeyCode::PageUp => {
                self.auto_scroll = false;
                self.syscall_scroll = self.syscall_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.auto_scroll = false;
                self.syscall_scroll = self.syscall_scroll.saturating_add(10);
            }
            KeyCode::Home => {
                self.auto_scroll = false;
                self.syscall_scroll = 0;
            }
            KeyCode::End => {
                self.auto_scroll = true; // Re-enable auto-scroll when going to end
                self.syscall_scroll = 0; // Will be set to max in draw()
            }
            KeyCode::Char(' ') => {
                // Toggle auto-scroll
                self.auto_scroll = !self.auto_scroll;
            }
            KeyCode::Char('r') => self.force_refresh(),
            _ => {}
        }
        false
    }

    /// Handle mouse clicks to change focus
    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            let x = mouse.column as u16;
            let y = mouse.row as u16;
            let in_process = x >= self.process_rect.x
                && x < self.process_rect.x.saturating_add(self.process_rect.width)
                && y >= self.process_rect.y
                && y < self.process_rect.y.saturating_add(self.process_rect.height);
            let in_actions = x >= self.actions_rect.x
                && x < self.actions_rect.x.saturating_add(self.actions_rect.width)
                && y >= self.actions_rect.y
                && y < self.actions_rect.y.saturating_add(self.actions_rect.height);
            if in_process {
                self.focus = FocusPane::Processes;
            } else if in_actions {
                self.focus = FocusPane::Actions;
            }
        }
    }

    /// Force immediate refresh
    fn force_refresh(&mut self) {
        self.last_refresh = Instant::now() - self.refresh_rate;
    }

    /// Check if we need to auto-refresh
    fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= self.refresh_rate
    }

    /// Update refresh timestamp
    fn mark_refreshed(&mut self) {
        self.last_refresh = Instant::now();
    }

    /// Draw the TUI
    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Split into two columns: 40% processes, 60% actions
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);
        // Cache rects for click focus
        self.process_rect = chunks[0];
        self.actions_rect = chunks[1];

        // Read current state (non-blocking)
        let (proc_rows, process_count, events) = match self.agent_state.try_read() {
            Ok(state) => {
                let sorted = state.processes_by_state();
                let count = sorted.len();
                let proc_rows: Vec<(String, state::ProcessState, u32, u32)> = sorted
                    .iter()
                    .map(|p| (p.name.clone(), p.state.clone(), p.pid, p.ppid))
                    .collect();
                let events: Vec<_> = state.recent_human_events_meta().into_iter().cloned().collect();
                (proc_rows, count, events)
            }
            Err(_) => {
                // State locked, show placeholder
                (vec![], 0, vec![])
            }
        };

        // Left column: Process table
        let process_title = format!(" Processes ({}) ", process_count);
        let process_block = Block::default()
            .title(process_title)
            .borders(Borders::ALL)
            .border_style(if self.focus == FocusPane::Processes { Style::default().fg(Color::Cyan) } else { Style::default() });
        // Build header and rows with alternating background colors
        let header = Row::new(vec!["NAME", "S", "PID", "PPID"]).style(Style::default().add_modifier(Modifier::BOLD));
        let mut rows: Vec<Row> = Vec::new();
        for (i, (name_raw, proc_state, pid_val, ppid_val)) in proc_rows.iter().enumerate() {
            let (state_code, state_color) = match proc_state {
                state::ProcessState::Spawning => ('S', Color::Yellow),
                state::ProcessState::Active => ('A', Color::Green),
                state::ProcessState::Waiting => ('W', Color::Rgb(255, 165, 0)),
                state::ProcessState::Exiting => ('X', Color::Red),
                state::ProcessState::Exited => ('E', Color::Rgb(128, 128, 128)),
            };
            let name = if name_raw.len() > 16 {
                format!("{}...", &name_raw[..13])
            } else {
                name_raw.clone()
            };
            let pid = pid_val.to_string();
            let ppid = ppid_val.to_string();
            let bg = if i % 2 == 0 { Color::Rgb(22, 22, 22) } else { Color::Rgb(12, 12, 12) };
            rows.push(
                Row::new(vec![
                    Cell::from(name),
                    Cell::from(Span::styled(state_code.to_string(), Style::default().fg(state_color))),
                    Cell::from(pid),
                    Cell::from(ppid),
                ])
                .style(Style::default().bg(bg)),
            );
        }

        // Clamp process selection to available rows
        let total_rows = rows.len();
        if total_rows == 0 {
            self.process_state.select(None);
            rows.push(Row::new(vec![Cell::from("No processes"), Cell::from(""), Cell::from(""), Cell::from("")]).style(Style::default().bg(Color::Rgb(12,12,12))));
        } else if let Some(sel) = self.process_state.selected() {
            if sel >= total_rows { self.process_state.select(Some(total_rows - 1)); }
        } else {
            self.process_state.select(Some(0));
        }

        let table = Table::new(rows, [
                Constraint::Length(17),
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Length(6),
            ])
            .header(header)
            .block(process_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(table, chunks[0], &mut self.process_state);

        // Right column: Actions stream (as selectable list)
        let (mut visible_items, scroll_pos, total_items, available_height) = if events.is_empty() {
            (vec![ListItem::new("Waiting for events...")], 0usize, 0usize, chunks[1].height.saturating_sub(2) as usize)
        } else {
            let available_height = chunks[1].height.saturating_sub(2) as usize; // Subtract border
            let total = events.len();
            let scroll_pos = if self.auto_scroll {
                if total > available_height { total.saturating_sub(available_height) } else { 0 }
            } else {
                let max_scroll = total.saturating_sub(available_height);
                std::cmp::min(self.syscall_scroll as usize, max_scroll)
            };
            let end_idx = std::cmp::min(scroll_pos + available_height, total);
            let slice: Vec<ListItem> = events[scroll_pos..end_idx]
                .iter()
                .enumerate()
                .map(|(i, ev)| {
                    let bg = if i % 2 == 0 { Color::Rgb(22,22,22) } else { Color::Rgb(12,12,12) };
                    let cat_color = category_color(ev.category);
                    let header = Line::from(vec![
                        Span::styled(format!("{}", ev.category), Style::default().fg(cat_color).add_modifier(Modifier::BOLD)),
                        Span::raw("  "),
                        Span::styled(ev.ts_str.clone(), Style::default().fg(Color::Gray)),
                    ]);
                    let body = Line::raw(format!("{} {} {}", ev.process_name, ev.pid, ev.message));
                    ListItem::new(vec![header, body]).style(Style::default().bg(bg))
                })
                .collect();
            (slice, scroll_pos, total, available_height)
        };

        // Clamp syscall_selected and ensure it's visible
        if total_items == 0 {
            self.syscall_selected = None;
        } else if let Some(sel) = self.syscall_selected {
            if sel >= total_items { self.syscall_selected = Some(total_items - 1); }
        } else if self.focus == FocusPane::Actions {
            // Initialize selection to last line when focusing actions
            self.syscall_selected = Some(total_items.saturating_sub(1));
        }

        // If selection is outside visible window, adjust scroll to reveal it
        if let (Some(sel), false) = (self.syscall_selected, self.auto_scroll) {
            if sel < scroll_pos {
                self.syscall_scroll = sel as u16;
            } else if sel >= scroll_pos + available_height && available_height > 0 {
                let new_start = sel.saturating_sub(available_height - 1);
                self.syscall_scroll = new_start as u16;
            }
        }

        let selected_relative = self.syscall_selected.and_then(|sel| {
            if sel >= scroll_pos && sel < scroll_pos + available_height { Some(sel - scroll_pos) } else { None }
        });
        let mut syscall_state = ListState::default();
        syscall_state.select(selected_relative);

        let scroll_indicator = if total_items > 0 {
            if self.auto_scroll { " Actions [AUTO] " } else { " Actions " }
        } else { " Actions " };

        let actions_block = Block::default()
            .title(scroll_indicator)
            .borders(Borders::ALL)
            .border_style(if self.focus == FocusPane::Actions { Style::default().fg(Color::Cyan) } else { Style::default() });

        // Ensure alternating bg also for empty placeholder
        if visible_items.len() == 1 && total_items == 0 {
            visible_items[0] = visible_items[0].clone().style(Style::default().bg(Color::Rgb(12,12,12)));
        }
        let actions_list = List::new(visible_items)
            .block(actions_block)
            .highlight_style(Style::default().bg(Color::Rgb(40,40,40)).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(actions_list, chunks[1], &mut syscall_state);

        // Show help at bottom of left column
        let help_area = Rect {
            x: chunks[0].x + 1,
            y: chunks[0].y + chunks[0].height - 2,
            width: chunks[0].width - 2,
            height: 1,
        };

        let help_text = "Click to focus pane • ↑/↓ move • PgUp/PgDn scroll • SPACE auto • q quit";
        frame.render_widget(Paragraph::new(help_text), help_area);
    }

    /// Format process runtime
    fn format_runtime(&self, start_time: u64) -> String {
        let now = chrono::Utc::now().timestamp_micros() as u64;
        let runtime_micros = now.saturating_sub(start_time);
        let runtime_secs = runtime_micros / 1_000_000;

        if runtime_secs < 60 {
            format!("{}s", runtime_secs)
        } else if runtime_secs < 3600 {
            format!("{}m{}s", runtime_secs / 60, runtime_secs % 60)
        } else {
            format!("{}h{}m", runtime_secs / 3600, (runtime_secs % 3600) / 60)
        }
    }
}

fn category_color(category: &str) -> Color {
    match category {
        // Process: Forest Green
        "Process" => Color::Rgb(34, 139, 34),
        // Network: Orange
        "Network" => Color::Rgb(255, 165, 0),
        // File IO: Magenta
        "File IO" => Color::Magenta,
        _ => Color::Gray,
    }
}

fn derive_category_from_plain(s: &str) -> &'static str {
    let sl = s.to_lowercase();
    if sl.contains("connect") || sl.contains("sent ") || sl.contains("recv") {
        "Network"
    } else if sl.contains("open") || sl.contains("access") || sl.contains("stat ") || sl.contains("readlink") {
        "File IO"
    } else if sl.contains("exec") || sl.contains("fork") || sl.contains("vfork") || sl.contains("exit") || sl.contains("wait") || sl.contains("clone") {
        "Process"
    } else {
        "Process"
    }
}

/// Convert processes to list items (free function to avoid borrowing issues)
fn create_process_list_items(processes: &[&LiveProcess]) -> Vec<ListItem<'static>> {
    if processes.is_empty() {
        return vec![
            ListItem::new("  PID   PPID  NAME         COMMAND"),
            ListItem::new("  ---   ----  ----         -------"),
            ListItem::new("  No active processes"),
        ];
    }

    let mut items = vec![
        ListItem::new("  PID   PPID  NAME         COMMAND"),
        ListItem::new("  ---   ----  ----         -------"),
    ];

    for process in processes {
        let command = if process.command_line.len() > 1 {
            process.command_line.join(" ")
        } else {
            process.command_line.first().cloned().unwrap_or_default()
        };

        // Truncate long commands
        let command = if command.len() > 40 {
            format!("{}...", &command[..37])
        } else {
            command
        };

        let line = format!(
            "{:>5} {:>5} {:12} {}",
            process.pid, process.ppid, process.name, command
        );

        items.push(ListItem::new(line));
    }

    items
}

/// Convert processes to list items with state and command info
fn create_process_state_items(processes: &[&state::LiveProcess]) -> Vec<ListItem<'static>> {
    if processes.is_empty() {
        return vec![
            ListItem::new("NAME              S  PID   PPID"),
            ListItem::new("----------------  -  ----- -----"),
            ListItem::new("No processes"),
        ];
    }

    let mut items = vec![
        ListItem::new("NAME              S  PID   PPID"),
        ListItem::new("----------------  -  ----- -----"),
    ];

    for process in processes {
        // Format state to match ProcessState enum exactly with requested colors
        let (state_code, state_color) = match process.state {
            state::ProcessState::Spawning => ('S', Color::Yellow), // Spawning (yellow)
            state::ProcessState::Active => ('A', Color::Green),    // Active (green)
            state::ProcessState::Waiting => ('W', Color::Rgb(255, 165, 0)), // Waiting (orange)
            state::ProcessState::Exiting => ('X', Color::Red),     // Exiting (red)
            state::ProcessState::Exited => ('E', Color::Rgb(128, 128, 128)), // Exited (grey)
        };

        // Use process name, truncate/pad to fixed width for alignment
        let name = if process.name.len() > 16 {
            format!("{}...", &process.name[..13])
        } else {
            process.name.clone()
        };

        let line_text = format!(
            "{:17} {:1} {:>5} {:>5}",
            name, state_code, process.pid, process.ppid
        );

        // Create ListItem with colored state
        let line_item = ListItem::new(Span::styled(line_text, Style::default().fg(state_color)));
        items.push(line_item);
    }

    items
}

/// Main monitor TUI entry point for demo/shared state
pub async fn run_monitor(agent_state: Arc<RwLock<AgentState>>) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = MonitorApp::new(agent_state);

    // Main event loop
    let result = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    result
}

/// Live monitor TUI entry point - connects to running session
pub async fn run_monitor_live(socket_path: &Path) -> Result<()> {
    // Connect to state server
    let mut state_client = StateClient::connect(socket_path).await?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app with live state updates
    let mut app = LiveMonitorApp::new();

    // Main event loop
    let result = run_live_app(&mut terminal, &mut app, &mut state_client).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    result
}

/// Live monitor app state
struct LiveMonitorApp {
    /// Current agent state
    current_state: Option<AgentState>,
    /// Process table selection state
    process_state: TableState,
    /// Syscall scroll position (line offset)
    syscall_scroll: u16,
    /// Syscall selection state (index in full list)
    syscall_selected: Option<usize>,
    /// Whether to quit the app
    should_quit: bool,
    /// Auto-scroll mode for syscalls
    auto_scroll: bool,
    /// Which pane is focused
    focus: FocusPane,
    /// Cached rects for hit testing mouse clicks
    process_rect: Rect,
    actions_rect: Rect,
}

impl LiveMonitorApp {
    fn new() -> Self {
        Self {
            current_state: None,
            process_state: TableState::default(),
            syscall_scroll: 0,
            syscall_selected: None,
            should_quit: false,
            auto_scroll: true,
            focus: FocusPane::Processes,
            process_rect: Rect::default(),
            actions_rect: Rect::default(),
        }
    }

    /// Handle keyboard input
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            // Navigation depends on focused pane
            KeyCode::Up => match self.focus {
                FocusPane::Processes => {
                    let new = self.process_state.selected().unwrap_or(0).saturating_sub(1);
                    self.process_state.select(Some(new));
                }
                FocusPane::Actions => {
                    self.auto_scroll = false;
                    if let Some(sel) = self.syscall_selected {
                        self.syscall_selected = sel.checked_sub(1);
                    }
                }
            },
            KeyCode::Down => match self.focus {
                FocusPane::Processes => {
                    let new = self.process_state.selected().unwrap_or(0).saturating_add(1);
                    self.process_state.select(Some(new));
                }
                FocusPane::Actions => {
                    self.auto_scroll = false;
                    if let Some(sel) = self.syscall_selected {
                        self.syscall_selected = Some(sel.saturating_add(1));
                    }
                }
            },
            // Syscall scrolling (actions pane)
            KeyCode::PageUp => {
                self.auto_scroll = false;
                self.syscall_scroll = self.syscall_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.auto_scroll = false;
                self.syscall_scroll = self.syscall_scroll.saturating_add(10);
            }
            KeyCode::Home => {
                self.auto_scroll = false;
                self.syscall_scroll = 0;
            }
            KeyCode::End => {
                self.auto_scroll = true; // Re-enable auto-scroll when going to end
                self.syscall_scroll = 0; // Will be set to max in draw()
            }
            KeyCode::Char(' ') => {
                // Toggle auto-scroll
                self.auto_scroll = !self.auto_scroll;
            }
            _ => {}
        }
        false
    }

    /// Handle mouse clicks to change focus
    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            let x = mouse.column as u16;
            let y = mouse.row as u16;
            let in_process = x >= self.process_rect.x
                && x < self.process_rect.x.saturating_add(self.process_rect.width)
                && y >= self.process_rect.y
                && y < self.process_rect.y.saturating_add(self.process_rect.height);
            let in_actions = x >= self.actions_rect.x
                && x < self.actions_rect.x.saturating_add(self.actions_rect.width)
                && y >= self.actions_rect.y
                && y < self.actions_rect.y.saturating_add(self.actions_rect.height);
            if in_process {
                self.focus = FocusPane::Processes;
            } else if in_actions {
                self.focus = FocusPane::Actions;
            }
        }
    }

    /// Update with new state
    fn update_state(&mut self, state: AgentState) {
        self.current_state = Some(state);
    }

    /// Draw the TUI
    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Split into two columns: 40% processes, 60% actions
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);
        // Cache rects for click focus
        self.process_rect = chunks[0];
        self.actions_rect = chunks[1];

        let (proc_rows, process_count, events_meta, events_plain) = match &self.current_state {
            Some(state) => {
                let sorted = state.processes_by_state();
                let count = sorted.len();
                let proc_rows: Vec<(String, state::ProcessState, u32, u32)> = sorted
                    .iter()
                    .map(|p| (p.name.clone(), p.state.clone(), p.pid, p.ppid))
                    .collect();
                let events_meta: Vec<_> = state.recent_human_events_meta().into_iter().cloned().collect();
                let events_plain: Vec<String> = state.recent_human_events().into_iter().cloned().map(|s| s.to_string()).collect();
                (proc_rows, count, events_meta, events_plain)
            }
            None => (
                vec![],
                0,
                vec![],
                vec![],
            ),
        };

        // Left column: Process list
        let process_title = if self.current_state.is_some() {
            format!(" Processes ({}) ", process_count)
        } else {
            " Processes (connecting...) ".to_string()
        };

        let process_block = Block::default()
            .title(process_title)
            .borders(Borders::ALL)
            .border_style(if self.focus == FocusPane::Processes { Style::default().fg(Color::Cyan) } else { Style::default() });
        // Build header and rows
        let header = Row::new(vec!["NAME", "S", "PID", "PPID"]).style(Style::default().add_modifier(Modifier::BOLD));
        let mut rows: Vec<Row> = Vec::new();
        for (i, (name_raw, proc_state, pid_val, ppid_val)) in proc_rows.iter().enumerate() {
            let (state_code, state_color) = match proc_state {
                state::ProcessState::Spawning => ('S', Color::Yellow),
                state::ProcessState::Active => ('A', Color::Green),
                state::ProcessState::Waiting => ('W', Color::Rgb(255, 165, 0)),
                state::ProcessState::Exiting => ('X', Color::Red),
                state::ProcessState::Exited => ('E', Color::Rgb(128, 128, 128)),
            };
            let name = if name_raw.len() > 16 { format!("{}...", &name_raw[..13]) } else { name_raw.clone() };
            let pid = pid_val.to_string();
            let ppid = ppid_val.to_string();
            let bg = if i % 2 == 0 { Color::Rgb(22, 22, 22) } else { Color::Rgb(12, 12, 12) };
            rows.push(Row::new(vec![
                Cell::from(name),
                Cell::from(Span::styled(state_code.to_string(), Style::default().fg(state_color))),
                Cell::from(pid),
                Cell::from(ppid),
            ]).style(Style::default().bg(bg)));
        }
        // Clamp selection
        let total_rows = rows.len();
        if total_rows == 0 {
            self.process_state.select(None);
            rows.push(Row::new(vec![Cell::from("Connecting to session..."), Cell::from(""), Cell::from(""), Cell::from("")]).style(Style::default().bg(Color::Rgb(12,12,12))));
        } else if let Some(sel) = self.process_state.selected() {
            if sel >= total_rows { self.process_state.select(Some(total_rows - 1)); }
        } else {
            self.process_state.select(Some(0));
        }
        let table = Table::new(rows, [Constraint::Length(17), Constraint::Length(2), Constraint::Length(6), Constraint::Length(6)])
            .header(header)
            .block(process_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(table, chunks[0], &mut self.process_state);

        // Right column: Actions stream (as selectable list)
        let (mut visible_items, scroll_pos, total_items, available_height) = if events_meta.is_empty() && events_plain.is_empty() {
            let placeholder = if self.current_state.is_some() { "Waiting for events..." } else { "Connecting to session..." };
            (vec![ListItem::new(placeholder)], 0usize, 0usize, chunks[1].height.saturating_sub(2) as usize)
        } else {
            let available_height = chunks[1].height.saturating_sub(2) as usize; // Subtract border
            let total_lines = if !events_meta.is_empty() { events_meta.len() } else { events_plain.len() };
            let scroll_pos = if self.auto_scroll {
                if total_lines > available_height { total_lines.saturating_sub(available_height) } else { 0 }
            } else {
                let max_scroll = total_lines.saturating_sub(available_height);
                std::cmp::min(self.syscall_scroll as usize, max_scroll)
            };
            let end_idx = std::cmp::min(scroll_pos + available_height, total_lines);
            let slice: Vec<ListItem> = if !events_meta.is_empty() {
                events_meta[scroll_pos..end_idx]
                    .iter()
                    .enumerate()
                    .map(|(i, ev)| {
                        let bg = if i % 2 == 0 { Color::Rgb(22,22,22) } else { Color::Rgb(12,12,12) };
                        let cat_color = category_color(ev.category);
                        let header = Line::from(vec![
                            Span::styled(format!("{}", ev.category), Style::default().fg(cat_color).add_modifier(Modifier::BOLD)),
                            Span::raw("  "),
                            Span::styled(ev.ts_str.clone(), Style::default().fg(Color::Gray)),
                        ]);
                        let body = Line::raw(format!("{} {} {}", ev.process_name, ev.pid, ev.message));
                        ListItem::new(vec![header, body]).style(Style::default().bg(bg))
                    })
                    .collect()
            } else {
                events_plain[scroll_pos..end_idx]
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let bg = if i % 2 == 0 { Color::Rgb(22,22,22) } else { Color::Rgb(12,12,12) };
                        // Parse: "<ts> rest..."
                        let (ts, rest) = if let Some(space_idx) = s.find(' ') { (&s[..space_idx], &s[space_idx+1..]) } else { ("", s.as_str()) };
                        let category = derive_category_from_plain(rest);
                        let cat_color = category_color(category);
                        let header = Line::from(vec![
                            Span::styled(category.to_string(), Style::default().fg(cat_color).add_modifier(Modifier::BOLD)),
                            Span::raw("  "),
                            Span::styled(ts.to_string(), Style::default().fg(Color::Gray)),
                        ]);
                        let body = Line::raw(rest.to_string());
                        ListItem::new(vec![header, body]).style(Style::default().bg(bg))
                    })
                    .collect()
            };
            (slice, scroll_pos, total_lines, available_height)
        };

        // Clamp syscall_selected and ensure it's visible
        if total_items == 0 {
            self.syscall_selected = None;
        } else if let Some(sel) = self.syscall_selected {
            if sel >= total_items { self.syscall_selected = Some(total_items - 1); }
        } else if self.focus == FocusPane::Actions {
            self.syscall_selected = Some(total_items.saturating_sub(1));
        }

        // If selection is outside visible window, adjust scroll to reveal it
        if let (Some(sel), false) = (self.syscall_selected, self.auto_scroll) {
            if sel < scroll_pos {
                self.syscall_scroll = sel as u16;
            } else if sel >= scroll_pos + available_height && available_height > 0 {
                let new_start = sel.saturating_sub(available_height - 1);
                self.syscall_scroll = new_start as u16;
            }
        }

        let selected_relative = self.syscall_selected.and_then(|sel| {
            if sel >= scroll_pos && sel < scroll_pos + available_height { Some(sel - scroll_pos) } else { None }
        });
        let mut syscall_state = ListState::default();
        syscall_state.select(selected_relative);

        let scroll_indicator = if total_items > 0 { if self.auto_scroll { " Actions [AUTO] " } else { " Actions " } } else { " Actions " };

        let actions_block = Block::default()
            .title(scroll_indicator)
            .borders(Borders::ALL)
            .border_style(if self.focus == FocusPane::Actions { Style::default().fg(Color::Cyan) } else { Style::default() });

        if visible_items.len() == 1 && total_items == 0 {
            visible_items[0] = visible_items[0].clone().style(Style::default().bg(Color::Rgb(12,12,12)));
        }
        let actions_list = List::new(visible_items)
            .block(actions_block)
            .highlight_style(Style::default().bg(Color::Rgb(40,40,40)).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(actions_list, chunks[1], &mut syscall_state);

        // Show help at bottom of left column
        let help_area = Rect {
            x: chunks[0].x + 1,
            y: chunks[0].y + chunks[0].height - 2,
            width: chunks[0].width - 2,
            height: 1,
        };

        let help_text = "Click to focus pane • ↑/↓ move • PgUp/PgDn scroll • SPACE auto • q quit";
        frame.render_widget(Paragraph::new(help_text), help_area);
    }
}

/// Run the live TUI application
async fn run_live_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut LiveMonitorApp,
    state_client: &mut StateClient,
) -> Result<()> {
    loop {
        // Draw UI
        terminal.draw(|frame| app.draw(frame))?;

        // Check for new state updates or keyboard input
        tokio::select! {
            // Receive state updates
            state_result = state_client.receive_state() => {
                match state_result {
                    Ok(state) => {
                        app.update_state(state);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to receive state update: {}", e);
                        // Session might have ended
                        break;
                    }
                }
            }

            // Handle keyboard input
            input_result = tokio::task::spawn_blocking(|| -> Result<Option<Event>> {
                if event::poll(Duration::from_millis(100))? {
                    let ev = event::read()?;
                    return Ok(Some(ev));
                }
                Ok(None)
            }) => {
                match input_result {
                    Ok(Ok(Some(ev))) => {
                        match ev {
                            Event::Key(key) => {
                                if key.kind == KeyEventKind::Press {
                                    if app.handle_key(key) { break; }
                                }
                            }
                            Event::Mouse(m) => {
                                app.handle_mouse(m);
                            }
                            _ => {}
                        }
                    }
                    Ok(Ok(None)) => {
                        // No input, continue
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Input error: {}", e);
                    }
                    Err(e) => {
                        tracing::warn!("Input task error: {}", e);
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Run the TUI application
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut MonitorApp,
) -> Result<()> {
    loop {
        // Draw UI
        terminal.draw(|frame| app.draw(frame))?;

        // Handle events with timeout for auto-refresh
        let timeout = app.refresh_rate.saturating_sub(app.last_refresh.elapsed());

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        if app.handle_key(key) { break; }
                    }
                }
                Event::Mouse(m) => {
                    app.handle_mouse(m);
                }
                _ => {}
            }
        }

        // Auto-refresh if needed
        if app.should_refresh() {
            app.mark_refreshed();
            // UI will refresh on next draw cycle
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
