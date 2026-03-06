mod app;
mod event;
pub mod ui;
pub mod views;

pub use app::{App, EntityRow, FilterBar, FilterState, Tab};

use std::io;

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use filament_core::connection::FilamentConnection;
use filament_core::error::{FilamentError, Result};

/// Launch the TUI application.
///
/// # Errors
///
/// Returns an error if the terminal fails to initialize or encounters a runtime error.
pub async fn run_tui(conn: FilamentConnection) -> Result<()> {
    // Install panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));

    setup_terminal().map_err(FilamentError::Io)?;

    let result = run_app(conn).await;

    restore_terminal().map_err(FilamentError::Io)?;

    result
}

fn setup_terminal() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    Ok(())
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

async fn run_app(conn: FilamentConnection) -> Result<()> {
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).map_err(FilamentError::Io)?;

    let mut app = App::new(conn);
    // Load config from current directory (best effort)
    let cwd = std::env::current_dir().ok();
    app.load_config(cwd.as_deref());
    app.refresh_all().await;

    loop {
        terminal
            .draw(|frame| ui::draw(frame, &mut app))
            .map_err(FilamentError::Io)?;

        event::handle_events(&mut app).await;

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
