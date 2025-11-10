// TUI module for interactive Capsule VM configuration

mod app;
mod handlers;
mod render;

pub use app::TuiApp;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

/// Launch the TUI configuration wizard
/// Returns Some(CapsuleConfig) if user completes configuration, None if cancelled
pub fn run_tui() -> Result<Option<crate::config::CapsuleConfig>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = TuiApp::default();
    let result = run_app(&mut terminal, &mut app);

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

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut TuiApp,
) -> Result<Option<crate::config::CapsuleConfig>> {
    loop {
        terminal.draw(|f| render::render_ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(None),
                KeyCode::Enter => {
                    if app.is_ready_to_submit() {
                        return Ok(Some(app.build_config()?));
                    }
                }
                _ => handlers::handle_key_event(app, key),
            }
        }
    }
}
