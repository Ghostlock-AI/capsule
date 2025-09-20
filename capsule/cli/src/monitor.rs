//! Live process monitoring TUI using ratatui
//!
//! Shows real-time process list with keyboard navigation

use crate::ipc::StateClient;
use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{
        Block, Borders, Cell, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
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
    Networking,
    Actions,
}

/// TUI application state
struct MonitorApp {
    /// Shared agent state from tracker
    agent_state: Arc<RwLock<AgentState>>,
    /// Process table selection state
    process_state: TableState,
    /// Networking table selection state
    network_state: TableState,
    /// Syscall scroll position (line offset)
    syscall_scroll: u16,
    /// Syscall selection state (index in full list)
    syscall_selected: Option<usize>,
    /// Number of rows currently in networking table
    network_row_count: usize,
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
    /// Networking pane rect for focus/mouse interaction
    network_rect: Rect,
    actions_rect: Rect,
    /// Root area cached from last draw
    root_area: Rect,
    /// X position of the divider (right edge of process pane)
    divider_x: u16,
    /// Whether user is dragging the divider
    dragging_divider: bool,
    /// Process pane width percentage (1..=99)
    process_pct: u16,
    /// Process pane vertical percentage (top portion dedicated to processes)
    process_height_pct: u16,
}

impl MonitorApp {
    /// Create new monitor app with shared state
    fn new(agent_state: Arc<RwLock<AgentState>>) -> Self {
        Self {
            agent_state,
            process_state: TableState::default(),
            network_state: TableState::default(),
            syscall_scroll: 0,
            syscall_selected: None,
            network_row_count: 0,
            should_quit: false,
            refresh_rate: Duration::from_millis(500), // 2 FPS refresh
            last_refresh: Instant::now(),
            auto_scroll: true, // Start with auto-scroll enabled
            focus: FocusPane::Processes,
            process_rect: Rect::default(),
            network_rect: Rect::default(),
            actions_rect: Rect::default(),
            root_area: Rect::default(),
            divider_x: 0,
            dragging_divider: false,
            process_pct: 30, // default narrower process pane
            process_height_pct: 65,
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
                FocusPane::Networking => {
                    let new = self.network_state.selected().unwrap_or(0).saturating_sub(1);
                    self.network_state.select(Some(new));
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
                FocusPane::Networking => {
                    let current = self.network_state.selected().unwrap_or(0);
                    let mut new = current.saturating_add(1);
                    if self.network_row_count == 0 {
                        new = 0;
                    } else if new >= self.network_row_count {
                        new = self.network_row_count.saturating_sub(1);
                    }
                    self.network_state.select(Some(new));
                }
                FocusPane::Actions => {
                    self.auto_scroll = false;
                    if let Some(sel) = self.syscall_selected {
                        self.syscall_selected = Some(sel.saturating_add(1));
                    }
                }
            },
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPane::Processes => FocusPane::Networking,
                    FocusPane::Networking => FocusPane::Actions,
                    FocusPane::Actions => FocusPane::Processes,
                };
            }
            KeyCode::BackTab => {
                self.focus = match self.focus {
                    FocusPane::Processes => FocusPane::Actions,
                    FocusPane::Networking => FocusPane::Processes,
                    FocusPane::Actions => FocusPane::Networking,
                };
            }
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
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let x = mouse.column as u16;
                let y = mouse.row as u16;
                let in_process = x >= self.process_rect.x
                    && x < self.process_rect.x.saturating_add(self.process_rect.width)
                    && y >= self.process_rect.y
                    && y < self.process_rect.y.saturating_add(self.process_rect.height);
                let in_network = x >= self.network_rect.x
                    && x < self.network_rect.x.saturating_add(self.network_rect.width)
                    && y >= self.network_rect.y
                    && y < self.network_rect.y.saturating_add(self.network_rect.height);
                let in_actions = x >= self.actions_rect.x
                    && x < self.actions_rect.x.saturating_add(self.actions_rect.width)
                    && y >= self.actions_rect.y
                    && y < self.actions_rect.y.saturating_add(self.actions_rect.height);
                if in_process {
                    self.focus = FocusPane::Processes;
                } else if in_network {
                    self.focus = FocusPane::Networking;
                } else if in_actions {
                    self.focus = FocusPane::Actions;
                }

                // Start divider drag if clicking near divider
                let divider = self.divider_x;
                if x == divider || x + 1 == divider || x == divider.saturating_sub(1) {
                    self.dragging_divider = true;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.dragging_divider {
                    let x = mouse.column as u16;
                    // compute new pct relative to root_area
                    let total_w = self.root_area.width.max(1);
                    let left_w = x.saturating_sub(self.root_area.x).min(total_w - 1);
                    let mut pct = (left_w as u32 * 100 / total_w as u32) as u16;
                    // clamp
                    if pct < 10 {
                        pct = 10;
                    }
                    if pct > 80 {
                        pct = 80;
                    }
                    self.process_pct = pct;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging_divider = false;
            }
            MouseEventKind::ScrollDown => {
                if self.focus == FocusPane::Actions {
                    self.auto_scroll = false;
                    self.syscall_scroll = self.syscall_scroll.saturating_add(1);
                }
            }
            MouseEventKind::ScrollUp => {
                if self.focus == FocusPane::Actions {
                    self.auto_scroll = false;
                    self.syscall_scroll = self.syscall_scroll.saturating_sub(1);
                }
            }
            _ => {}
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
        self.root_area = area;

        // Split into two columns with adjustable percentage
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(self.process_pct),
                Constraint::Percentage(100 - self.process_pct),
            ])
            .split(area);
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(self.process_height_pct),
                Constraint::Percentage(100 - self.process_height_pct),
            ])
            .split(chunks[0]);
        // Cache rects for click focus
        self.process_rect = left_chunks[0];
        self.network_rect = left_chunks[1];
        self.actions_rect = chunks[1];
        self.divider_x = self.process_rect.x.saturating_add(self.process_rect.width);

        // Read current state (non-blocking)
        let (proc_rows, process_count, events, peer_rows, selected_pid, using_global_peers) =
            match self.agent_state.try_read() {
                Ok(state) => {
                    let sorted = state.processes_by_state();
                    let count = sorted.len();
                    let proc_rows: Vec<(String, state::ProcessState, u32, u32)> = sorted
                        .iter()
                        .map(|p| (p.name.clone(), p.state.clone(), p.pid, p.ppid))
                        .collect();
                    let events: Vec<_> = state
                        .recent_human_events_meta()
                        .into_iter()
                        .cloned()
                        .collect();

                    let total_rows = proc_rows.len();
                    if total_rows == 0 {
                        self.process_state.select(None);
                    } else {
                        let mut sel = self.process_state.selected().unwrap_or(0);
                        if sel >= total_rows {
                            sel = total_rows - 1;
                        }
                        self.process_state.select(Some(sel));
                    }
                    let selected_pid = self
                        .process_state
                        .selected()
                        .and_then(|idx| proc_rows.get(idx).map(|(_, _, pid, _)| *pid));

                    let mut using_global_peers = false;
                    let mut peer_rows = if let Some(pid) = selected_pid {
                        state.network_peers_for(pid)
                    } else {
                        Vec::new()
                    };
                    if peer_rows.is_empty() {
                        using_global_peers = true;
                        peer_rows = state
                            .network_peers_global()
                            .into_iter()
                            .map(|(_, summary)| summary)
                            .collect();
                    }

                    (
                        proc_rows,
                        count,
                        events,
                        peer_rows,
                        selected_pid,
                        using_global_peers,
                    )
                }
                Err(_) => {
                    // State locked, show placeholder
                    (vec![], 0, vec![], Vec::new(), None, true)
                }
            };

        // Left column: Process table
        let process_title = format!(" Processes ({}) ", process_count);
        let process_block = Block::default()
            .title(process_title)
            .borders(Borders::ALL)
            .border_style(if self.focus == FocusPane::Processes {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });
        // Build header and rows with alternating background colors
        let header = Row::new(vec!["NAME", "S", "PID", "PPID"])
            .style(Style::default().add_modifier(Modifier::BOLD));
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
            let mut row_style = alt_row_style(i);
            if matches!(proc_state, state::ProcessState::Exited) {
                row_style = row_style.add_modifier(Modifier::DIM);
            }
            rows.push(
                Row::new(vec![
                    Cell::from(name),
                    Cell::from(Span::styled(
                        state_code.to_string(),
                        Style::default().fg(state_color),
                    )),
                    Cell::from(pid),
                    Cell::from(ppid),
                ])
                .style(row_style),
            );
        }

        // Clamp process selection to available rows
        if rows.is_empty() {
            rows.push(
                Row::new(vec![
                    Cell::from("No processes"),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                ])
                .style(alt_row_style(0)),
            );
        }

        let table = Table::new(
            rows,
            [
                Constraint::Length(17),
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Length(6),
            ],
        )
        .header(header)
        .block(process_block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(table, self.process_rect, &mut self.process_state);

        // Networking pane: bottom-left list of peers
        self.network_row_count = peer_rows.len();
        if self.network_row_count == 0 {
            self.network_state.select(None);
        } else {
            let mut sel = self.network_state.selected().unwrap_or(0);
            if sel >= self.network_row_count {
                sel = self.network_row_count - 1;
            }
            self.network_state.select(Some(sel));
        }

        let network_title = if using_global_peers || selected_pid.is_none() {
            " Networking (top peers) ".to_string()
        } else if let Some(sel_idx) = self.process_state.selected() {
            if let Some((name, _, pid_value, _)) = proc_rows.get(sel_idx) {
                format!(" Networking ({}:{}) ", name, pid_value)
            } else {
                " Networking ".to_string()
            }
        } else {
            " Networking ".to_string()
        };

        let network_header = Row::new(vec!["REMOTE", "PROTO", "SENT", "RECV", "LAST"])
            .style(Style::default().add_modifier(Modifier::BOLD));
        let mut network_rows_tui: Vec<Row> = Vec::new();
        for (i, peer) in peer_rows.iter().enumerate() {
            let mut host = peer.display.clone();
            if host.is_empty() {
                host = peer.remote_ip.clone();
            }
            if host != peer.remote_ip && !peer.remote_ip.is_empty() {
                host = format!("{} ({})", host, peer.remote_ip);
            }
            if let Some(port) = peer.remote_port {
                host = format!("{}:{}", host, port);
            }
            if peer.is_external {
                host.push_str(" [ext]");
            }
            let proto = peer.protocol.clone().unwrap_or_else(|| "-".to_string());
            let sent = format_bytes(peer.bytes_sent);
            let recv = format_bytes(peer.bytes_recv);
            let last = format_since(peer.last_ts);
            let mut style = alt_row_style(i);
            if peer.is_external {
                style = style.fg(Color::Yellow);
            }
            network_rows_tui.push(
                Row::new(vec![
                    Cell::from(host),
                    Cell::from(proto),
                    Cell::from(sent),
                    Cell::from(recv),
                    Cell::from(last),
                ])
                .style(style),
            );
        }

        if network_rows_tui.is_empty() {
            network_rows_tui.push(
                Row::new(vec![
                    Cell::from("No network activity"),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                ])
                .style(alt_row_style(0)),
            );
        }

        let network_table = Table::new(
            network_rows_tui,
            [
                Constraint::Percentage(46),
                Constraint::Length(7),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(12),
            ],
        )
        .header(network_header)
        .block(
            Block::default()
                .title(network_title)
                .borders(Borders::ALL)
                .border_style(if self.focus == FocusPane::Networking {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                }),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(network_table, self.network_rect, &mut self.network_state);

        // Right column: Actions stream (as selectable list)
        const ITEM_HEIGHT: usize = 5;
        let (mut visible_items, scroll_pos, total_items, available_height) = if events.is_empty() {
            (
                vec![ListItem::new("Waiting for events...")],
                0usize,
                0usize,
                self.actions_rect.height.saturating_sub(2) as usize,
            )
        } else {
            let available_height = self.actions_rect.height.saturating_sub(2) as usize; // Subtract border
            let total = events.len();
            let per_page = std::cmp::max(1, available_height / ITEM_HEIGHT);
            let scroll_pos = if self.auto_scroll {
                if total > per_page {
                    total.saturating_sub(per_page)
                } else {
                    0
                }
            } else {
                let max_scroll = total.saturating_sub(per_page);
                std::cmp::min(self.syscall_scroll as usize, max_scroll)
            };
            let end_idx = std::cmp::min(scroll_pos + per_page, total);
            let slice: Vec<ListItem> = events[scroll_pos..end_idx]
                .iter()
                .enumerate()
                .map(|(i, ev)| {
                    let style = alt_row_style(i);
                    let cat_color = category_color(ev.category);
                    let header = Line::from(vec![
                        Span::styled(
                            format!("{}", ev.category),
                            Style::default().fg(cat_color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("  "),
                        Span::styled(ev.ts_str.clone(), Style::default().fg(Color::Gray)),
                    ]);
                    let line2 = Line::raw(format!("{} ({})", ev.process_name, ev.pid));
                    let args_line = Line::raw(format!(
                        "{} {}",
                        ev.action,
                        ev.args.clone().unwrap_or_default()
                    ));
                    ListItem::new(vec![Line::raw(""), header, line2, args_line, Line::raw("")])
                        .style(style)
                })
                .collect();
            (slice, scroll_pos, total, available_height)
        };

        // Clamp syscall_selected and ensure it's visible
        if total_items == 0 {
            self.syscall_selected = None;
        } else if let Some(sel) = self.syscall_selected {
            if sel >= total_items {
                self.syscall_selected = Some(total_items - 1);
            }
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
            if sel >= scroll_pos && sel < scroll_pos + available_height {
                Some(sel - scroll_pos)
            } else {
                None
            }
        });
        let mut syscall_state = ListState::default();
        syscall_state.select(selected_relative);

        let scroll_indicator = if total_items > 0 {
            if self.auto_scroll {
                " Actions [AUTO] "
            } else {
                " Actions "
            }
        } else {
            " Actions "
        };

        let actions_block = Block::default()
            .title(scroll_indicator)
            .borders(Borders::ALL)
            .border_style(if self.focus == FocusPane::Actions {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });

        // Ensure alternating bg also for empty placeholder
        if visible_items.len() == 1 && total_items == 0 {
            visible_items[0] = visible_items[0].clone().style(alt_row_style(0));
        }
        let actions_list = List::new(visible_items)
            .block(actions_block)
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(40, 40, 40))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(actions_list, self.actions_rect, &mut syscall_state);

        // Render scrollbar for Actions (live)
        if total_items > 0 {
            let mut sb_state = ScrollbarState::new(total_items.saturating_mul(ITEM_HEIGHT));
            sb_state = sb_state.position(scroll_pos.saturating_mul(ITEM_HEIGHT));
            let sb = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None);
            frame.render_stateful_widget(
                sb,
                self.actions_rect.inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 1,
                }),
                &mut sb_state,
            );
        }

        // Render scrollbar for Actions
        if total_items > 0 {
            let mut sb_state = ScrollbarState::new(total_items.saturating_mul(ITEM_HEIGHT));
            sb_state = sb_state.position(scroll_pos.saturating_mul(ITEM_HEIGHT));
            let sb = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None);
            frame.render_stateful_widget(
                sb,
                self.actions_rect.inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 1,
                }),
                &mut sb_state,
            );
        }

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

    #[allow(dead_code)]
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
        // Use ANSI palette so terminals map appropriately for theme
        "Process" => Color::Green,
        "Network" => Color::Yellow, // closest to orange in ANSI
        "File IO" => Color::Magenta,
        _ => Color::Gray,
    }
}

fn derive_category_from_plain(s: &str) -> &'static str {
    let sl = s.to_lowercase();
    if sl.contains("connect") || sl.contains("sent ") || sl.contains("recv") {
        "Network"
    } else if sl.contains("open")
        || sl.contains("access")
        || sl.contains("stat ")
        || sl.contains("readlink")
    {
        "File IO"
    } else if sl.contains("exec")
        || sl.contains("fork")
        || sl.contains("vfork")
        || sl.contains("exit")
        || sl.contains("wait")
        || sl.contains("clone")
    {
        "Process"
    } else {
        "Process"
    }
}

fn derive_action_and_args_from_message(msg: &str) -> (String, String) {
    let mut it = msg.split_whitespace();
    if let Some(first) = it.next() {
        let action = first.to_string();
        let args = it.collect::<Vec<_>>().join(" ");
        (action, args)
    } else {
        (String::new(), String::new())
    }
}

fn alt_row_style(index: usize) -> Style {
    // Use terminal theme-relative color: Indexed(8) is often a gray that adapts
    if index % 2 == 0 {
        Style::default()
    } else {
        Style::default().bg(Color::Indexed(8)) // bright black / gray
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{} B", bytes)
    } else if b < MB {
        format!("{:.1} KB", b / KB)
    } else if b < GB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{:.1} GB", b / GB)
    }
}

fn format_since(ts: u64) -> String {
    if ts == 0 {
        return "-".to_string();
    }
    let now = chrono::Utc::now().timestamp_micros() as u64;
    let delta = now.saturating_sub(ts);
    let seconds = delta / 1_000_000;
    if seconds < 1 {
        "just now".to_string()
    } else if seconds < 60 {
        format!("{}s ago", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s ago", seconds / 60, seconds % 60)
    } else if seconds < 86400 {
        format!("{}h {}m ago", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}d {}h ago", seconds / 86400, (seconds % 86400) / 3600)
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
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
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
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Live monitor app state
struct LiveMonitorApp {
    /// Current agent state
    current_state: Option<AgentState>,
    /// Process table selection state
    process_state: TableState,
    /// Networking table selection state
    network_state: TableState,
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
    network_rect: Rect,
    actions_rect: Rect,
    /// Root area cached from last draw
    root_area: Rect,
    /// X position of the divider (right edge of process pane)
    divider_x: u16,
    /// Whether user is dragging the divider
    dragging_divider: bool,
    /// Process pane width percentage
    process_pct: u16,
    /// Process pane vertical split percentage
    process_height_pct: u16,
    /// Number of networking rows
    network_row_count: usize,
}

impl LiveMonitorApp {
    fn new() -> Self {
        Self {
            current_state: None,
            process_state: TableState::default(),
            network_state: TableState::default(),
            syscall_scroll: 0,
            syscall_selected: None,
            should_quit: false,
            auto_scroll: true,
            focus: FocusPane::Processes,
            process_rect: Rect::default(),
            network_rect: Rect::default(),
            actions_rect: Rect::default(),
            root_area: Rect::default(),
            divider_x: 0,
            dragging_divider: false,
            process_pct: 30,
            process_height_pct: 65,
            network_row_count: 0,
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
                FocusPane::Networking => {
                    let new = self.network_state.selected().unwrap_or(0).saturating_sub(1);
                    self.network_state.select(Some(new));
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
                FocusPane::Networking => {
                    let current = self.network_state.selected().unwrap_or(0);
                    let mut new = current.saturating_add(1);
                    if self.network_row_count == 0 {
                        new = 0;
                    } else if new >= self.network_row_count {
                        new = self.network_row_count.saturating_sub(1);
                    }
                    self.network_state.select(Some(new));
                }
                FocusPane::Actions => {
                    self.auto_scroll = false;
                    if let Some(sel) = self.syscall_selected {
                        self.syscall_selected = Some(sel.saturating_add(1));
                    }
                }
            },
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPane::Processes => FocusPane::Networking,
                    FocusPane::Networking => FocusPane::Actions,
                    FocusPane::Actions => FocusPane::Processes,
                };
            }
            KeyCode::BackTab => {
                self.focus = match self.focus {
                    FocusPane::Processes => FocusPane::Actions,
                    FocusPane::Networking => FocusPane::Processes,
                    FocusPane::Actions => FocusPane::Networking,
                };
            }
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
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let x = mouse.column as u16;
                let y = mouse.row as u16;
                let in_process = x >= self.process_rect.x
                    && x < self.process_rect.x.saturating_add(self.process_rect.width)
                    && y >= self.process_rect.y
                    && y < self.process_rect.y.saturating_add(self.process_rect.height);
                let in_network = x >= self.network_rect.x
                    && x < self.network_rect.x.saturating_add(self.network_rect.width)
                    && y >= self.network_rect.y
                    && y < self.network_rect.y.saturating_add(self.network_rect.height);
                let in_actions = x >= self.actions_rect.x
                    && x < self.actions_rect.x.saturating_add(self.actions_rect.width)
                    && y >= self.actions_rect.y
                    && y < self.actions_rect.y.saturating_add(self.actions_rect.height);
                if in_process {
                    self.focus = FocusPane::Processes;
                } else if in_network {
                    self.focus = FocusPane::Networking;
                } else if in_actions {
                    self.focus = FocusPane::Actions;
                }
                let divider = self.divider_x;
                if x == divider || x + 1 == divider || x == divider.saturating_sub(1) {
                    self.dragging_divider = true;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.dragging_divider {
                    let x = mouse.column as u16;
                    let total_w = self.root_area.width.max(1);
                    let left_w = x.saturating_sub(self.root_area.x).min(total_w - 1);
                    let mut pct = (left_w as u32 * 100 / total_w as u32) as u16;
                    if pct < 10 {
                        pct = 10;
                    }
                    if pct > 80 {
                        pct = 80;
                    }
                    self.process_pct = pct;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging_divider = false;
            }
            MouseEventKind::ScrollDown => {
                if self.focus == FocusPane::Actions {
                    self.auto_scroll = false;
                    self.syscall_scroll = self.syscall_scroll.saturating_add(1);
                }
            }
            MouseEventKind::ScrollUp => {
                if self.focus == FocusPane::Actions {
                    self.auto_scroll = false;
                    self.syscall_scroll = self.syscall_scroll.saturating_sub(1);
                }
            }
            _ => {}
        }
    }

    /// Update with new state
    fn update_state(&mut self, state: AgentState) {
        self.current_state = Some(state);
    }

    /// Draw the TUI
    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.root_area = area;

        // Split into two columns with adjustable percentage
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(self.process_pct),
                Constraint::Percentage(100 - self.process_pct),
            ])
            .split(area);
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(self.process_height_pct),
                Constraint::Percentage(100 - self.process_height_pct),
            ])
            .split(chunks[0]);
        self.divider_x = left_chunks[0].x.saturating_add(left_chunks[0].width);
        // Cache rects for click focus
        self.process_rect = left_chunks[0];
        self.network_rect = left_chunks[1];
        self.actions_rect = chunks[1];

        let (
            proc_rows,
            process_count,
            events_meta,
            events_plain,
            peer_rows,
            selected_pid,
            using_global_peers,
        ) = match &self.current_state {
            Some(state) => {
                let sorted = state.processes_by_state();
                let count = sorted.len();
                let proc_rows: Vec<(String, state::ProcessState, u32, u32)> = sorted
                    .iter()
                    .map(|p| (p.name.clone(), p.state.clone(), p.pid, p.ppid))
                    .collect();
                let events_meta: Vec<_> = state
                    .recent_human_events_meta()
                    .into_iter()
                    .cloned()
                    .collect();
                let events_plain: Vec<String> = state
                    .recent_human_events()
                    .into_iter()
                    .cloned()
                    .map(|s| s.to_string())
                    .collect();

                let total_rows = proc_rows.len();
                if total_rows == 0 {
                    self.process_state.select(None);
                } else {
                    let mut sel = self.process_state.selected().unwrap_or(0);
                    if sel >= total_rows {
                        sel = total_rows - 1;
                    }
                    self.process_state.select(Some(sel));
                }
                let selected_pid = self
                    .process_state
                    .selected()
                    .and_then(|idx| proc_rows.get(idx).map(|(_, _, pid, _)| *pid));

                let mut using_global_peers = false;
                let mut peer_rows = if let Some(pid) = selected_pid {
                    state.network_peers_for(pid)
                } else {
                    Vec::new()
                };
                if peer_rows.is_empty() {
                    using_global_peers = true;
                    peer_rows = state
                        .network_peers_global()
                        .into_iter()
                        .map(|(_, summary)| summary)
                        .collect();
                }

                (
                    proc_rows,
                    count,
                    events_meta,
                    events_plain,
                    peer_rows,
                    selected_pid,
                    using_global_peers,
                )
            }
            None => (vec![], 0, vec![], vec![], Vec::new(), None, true),
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
            .border_style(if self.focus == FocusPane::Processes {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });
        // Build header and rows
        let header = Row::new(vec!["NAME", "S", "PID", "PPID"])
            .style(Style::default().add_modifier(Modifier::BOLD));
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
            let mut row_style = alt_row_style(i);
            if matches!(proc_state, state::ProcessState::Exited) {
                row_style = row_style.add_modifier(Modifier::DIM);
            }
            rows.push(
                Row::new(vec![
                    Cell::from(name),
                    Cell::from(Span::styled(
                        state_code.to_string(),
                        Style::default().fg(state_color),
                    )),
                    Cell::from(pid),
                    Cell::from(ppid),
                ])
                .style(row_style),
            );
        }
        // Clamp selection
        if rows.is_empty() {
            rows.push(
                Row::new(vec![
                    Cell::from("Connecting to session..."),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                ])
                .style(alt_row_style(0)),
            );
        }
        let table = Table::new(
            rows,
            [
                Constraint::Length(17),
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Length(6),
            ],
        )
        .header(header)
        .block(process_block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(table, self.process_rect, &mut self.process_state);

        // Networking pane
        self.network_row_count = peer_rows.len();
        if self.network_row_count == 0 {
            self.network_state.select(None);
        } else {
            let mut sel = self.network_state.selected().unwrap_or(0);
            if sel >= self.network_row_count {
                sel = self.network_row_count - 1;
            }
            self.network_state.select(Some(sel));
        }

        let network_title = if using_global_peers || selected_pid.is_none() {
            " Networking (top peers) ".to_string()
        } else if let Some(sel_idx) = self.process_state.selected() {
            if let Some((name, _, pid_value, _)) = proc_rows.get(sel_idx) {
                format!(" Networking ({}:{}) ", name, pid_value)
            } else {
                " Networking ".to_string()
            }
        } else {
            " Networking ".to_string()
        };

        let network_header = Row::new(vec!["REMOTE", "PROTO", "SENT", "RECV", "LAST"])
            .style(Style::default().add_modifier(Modifier::BOLD));
        let mut network_rows_tui: Vec<Row> = Vec::new();
        for (i, peer) in peer_rows.iter().enumerate() {
            let mut host = peer.display.clone();
            if host.is_empty() {
                host = peer.remote_ip.clone();
            }
            if host != peer.remote_ip && !peer.remote_ip.is_empty() {
                host = format!("{} ({})", host, peer.remote_ip);
            }
            if let Some(port) = peer.remote_port {
                host = format!("{}:{}", host, port);
            }
            if peer.is_external {
                host.push_str(" [ext]");
            }
            let proto = peer.protocol.clone().unwrap_or_else(|| "-".to_string());
            let sent = format_bytes(peer.bytes_sent);
            let recv = format_bytes(peer.bytes_recv);
            let last = format_since(peer.last_ts);
            let mut style = alt_row_style(i);
            if peer.is_external {
                style = style.fg(Color::Yellow);
            }
            network_rows_tui.push(
                Row::new(vec![
                    Cell::from(host),
                    Cell::from(proto),
                    Cell::from(sent),
                    Cell::from(recv),
                    Cell::from(last),
                ])
                .style(style),
            );
        }
        if network_rows_tui.is_empty() {
            network_rows_tui.push(
                Row::new(vec![
                    Cell::from("No network activity"),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                ])
                .style(alt_row_style(0)),
            );
        }

        let network_table = Table::new(
            network_rows_tui,
            [
                Constraint::Percentage(46),
                Constraint::Length(7),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(12),
            ],
        )
        .header(network_header)
        .block(
            Block::default()
                .title(network_title)
                .borders(Borders::ALL)
                .border_style(if self.focus == FocusPane::Networking {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                }),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(network_table, self.network_rect, &mut self.network_state);

        // Right column: Actions stream (as selectable list)
        const ITEM_HEIGHT: usize = 5; // top pad + header + name+action + args + bottom pad
        let (mut visible_items, scroll_pos, total_items, available_height) = if events_meta
            .is_empty()
            && events_plain.is_empty()
        {
            let placeholder = if self.current_state.is_some() {
                "Waiting for events..."
            } else {
                "Connecting to session..."
            };
            (
                vec![ListItem::new(placeholder)],
                0usize,
                0usize,
                self.actions_rect.height.saturating_sub(2) as usize,
            )
        } else {
            let available_height = self.actions_rect.height.saturating_sub(2) as usize; // Subtract border
            let total_lines = if !events_meta.is_empty() {
                events_meta.len()
            } else {
                events_plain.len()
            };
            let items_per_page = std::cmp::max(1, available_height / ITEM_HEIGHT);
            let scroll_pos = if self.auto_scroll {
                if total_lines > items_per_page {
                    total_lines.saturating_sub(items_per_page)
                } else {
                    0
                }
            } else {
                let max_scroll = total_lines.saturating_sub(items_per_page);
                std::cmp::min(self.syscall_scroll as usize, max_scroll)
            };
            let end_idx = std::cmp::min(scroll_pos + items_per_page, total_lines);
            let slice: Vec<ListItem> = if !events_meta.is_empty() {
                events_meta[scroll_pos..end_idx]
                    .iter()
                    .enumerate()
                    .map(|(i, ev)| {
                        let style = alt_row_style(i);
                        let cat_color = category_color(ev.category);
                        let header = Line::from(vec![
                            Span::styled(
                                format!("{}", ev.category),
                                Style::default().fg(cat_color).add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("  "),
                            Span::styled(ev.ts_str.clone(), Style::default().fg(Color::Gray)),
                        ]);
                        let line2 = Line::raw(format!("{} ({})", ev.process_name, ev.pid));
                        let args_line = Line::styled(
                            format!("{} {}", ev.action, ev.args.clone().unwrap_or_default()),
                            Style::default().fg(Color::Gray),
                        );
                        ListItem::new(vec![Line::raw(""), header, line2, args_line, Line::raw("")])
                            .style(style)
                    })
                    .collect()
            } else {
                events_plain[scroll_pos..end_idx]
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let style = alt_row_style(i);
                        // Parse: "<ts> rest..."
                        let (ts, rest) = if let Some(space_idx) = s.find(' ') {
                            (&s[..space_idx], &s[space_idx + 1..])
                        } else {
                            ("", s.as_str())
                        };
                        let category = derive_category_from_plain(rest);
                        let cat_color = category_color(category);
                        let header = Line::from(vec![
                            Span::styled(
                                category.to_string(),
                                Style::default().fg(cat_color).add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("  "),
                            Span::styled(ts.to_string(), Style::default().fg(Color::Gray)),
                        ]);
                        // rest: "process_name pid message"
                        let mut parts = rest.splitn(3, ' ');
                        let pname = parts.next().unwrap_or("");
                        let pid_str = parts.next().unwrap_or("");
                        let msg = parts.next().unwrap_or("");
                        let (action, args_str) = derive_action_and_args_from_message(msg);
                        let line2 = Line::raw(format!("{} ({})", pname, pid_str));
                        let args_line = Line::styled(
                            format!("{} {}", action, args_str),
                            Style::default().fg(Color::Gray),
                        );
                        ListItem::new(vec![Line::raw(""), header, line2, args_line, Line::raw("")])
                            .style(style)
                    })
                    .collect()
            };
            (slice, scroll_pos, total_lines, available_height)
        };

        // Clamp syscall_selected and ensure it's visible
        if total_items == 0 {
            self.syscall_selected = None;
        } else if let Some(sel) = self.syscall_selected {
            if sel >= total_items {
                self.syscall_selected = Some(total_items - 1);
            }
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
            if sel >= scroll_pos && sel < scroll_pos + available_height {
                Some(sel - scroll_pos)
            } else {
                None
            }
        });
        let mut syscall_state = ListState::default();
        syscall_state.select(selected_relative);

        let scroll_indicator = if total_items > 0 {
            if self.auto_scroll {
                " Actions [AUTO] "
            } else {
                " Actions "
            }
        } else {
            " Actions "
        };

        let actions_block = Block::default()
            .title(scroll_indicator)
            .borders(Borders::ALL)
            .border_style(if self.focus == FocusPane::Actions {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            });

        if visible_items.len() == 1 && total_items == 0 {
            visible_items[0] = visible_items[0].clone().style(alt_row_style(0));
        }
        let actions_list = List::new(visible_items)
            .block(actions_block)
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(40, 40, 40))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(actions_list, self.actions_rect, &mut syscall_state);

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
                        if app.handle_key(key) {
                            break;
                        }
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
