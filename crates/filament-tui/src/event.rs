use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Tab};

const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Handle one round of events: poll for input, auto-refresh on tick.
pub async fn handle_events(app: &mut App) {
    // Auto-refresh on tick
    if app.should_auto_refresh() {
        app.refresh_all().await;
    }

    // Poll for crossterm events
    if event::poll(POLL_TIMEOUT).unwrap_or(false) {
        if let Ok(Event::Key(key)) = event::read() {
            handle_key(app, key).await;
        }
    }
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    // Global keys
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
            return;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return;
        }
        KeyCode::Tab => {
            app.active_tab = app.active_tab.next();
            return;
        }
        KeyCode::BackTab => {
            app.active_tab = app.active_tab.prev();
            return;
        }
        KeyCode::Char('1') => {
            app.active_tab = Tab::Tasks;
            return;
        }
        KeyCode::Char('2') => {
            app.active_tab = Tab::Agents;
            return;
        }
        KeyCode::Char('3') => {
            app.active_tab = Tab::Reservations;
            return;
        }
        KeyCode::Char('r') => {
            app.refresh_all().await;
            return;
        }
        _ => {}
    }

    // Tab-specific keys
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.select_next(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
        KeyCode::Char('c') if app.active_tab == Tab::Tasks => {
            if let Err(e) = app.close_selected_task().await {
                app.status_message = Some(format!("Error closing task: {e}"));
            }
        }
        KeyCode::Char('f') if app.active_tab == Tab::Tasks => {
            app.task_filter.cycle();
            app.refresh_tasks().await;
        }
        _ => {}
    }
}
