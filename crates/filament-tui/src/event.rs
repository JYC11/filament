use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use filament_core::models::{EntityStatus, EntityType, Priority};

use crate::app::{App, FilterBar, Tab};

const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Handle one round of events: poll for input, auto-refresh on tick.
pub async fn handle_events(app: &mut App) {
    // Auto-refresh on tick
    if app.should_auto_refresh() {
        app.refresh_all().await;
    }

    // Poll for crossterm events on a blocking thread so we don't block the
    // tokio runtime (crossterm::event::poll is synchronous).
    let key_event = tokio::task::spawn_blocking(|| {
        if event::poll(POLL_TIMEOUT).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                return Some(key);
            }
        }
        None
    })
    .await
    .unwrap_or(None);

    if let Some(key) = key_event {
        handle_key(app, key).await;
    }
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    // Global keys (always active)
    match key.code {
        KeyCode::Char('q') => {
            app.should_quit = true;
            return;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return;
        }
        KeyCode::Char('r') => {
            app.refresh_all().await;
            return;
        }
        _ => {}
    }

    // If detail pane is open, capture keys for it
    if app.has_detail() {
        handle_detail_key(app, key);
        return;
    }

    // If a filter bar is open, capture keys for it
    if app.filter.active_bar.is_some() {
        handle_filter_bar_key(app, key).await;
        return;
    }

    // Global navigation keys (only when no filter bar is open)
    match key.code {
        KeyCode::Tab => {
            app.active_tab = app.active_tab.next();
            return;
        }
        KeyCode::BackTab => {
            app.active_tab = app.active_tab.prev();
            return;
        }
        KeyCode::Char('1') => {
            app.active_tab = Tab::Entities;
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
        KeyCode::Char('4') => {
            app.active_tab = Tab::Messages;
            return;
        }
        _ => {}
    }

    // Tab-specific keys
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.select_next(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
        // Entities tab filter keys
        KeyCode::Char('t') if app.active_tab == Tab::Entities && !app.filter.ready_only => {
            app.filter.active_bar = Some(FilterBar::Type);
        }
        KeyCode::Char('f') if app.active_tab == Tab::Entities && !app.filter.ready_only => {
            app.filter.active_bar = Some(FilterBar::Status);
        }
        KeyCode::Char('P') if app.active_tab == Tab::Entities => {
            app.filter.active_bar = Some(FilterBar::Priority);
        }
        KeyCode::Char('F') if app.active_tab == Tab::Entities => {
            app.filter.toggle_ready_only();
            app.reset_page();
            app.refresh_entities().await;
        }
        KeyCode::Char('n') if app.active_tab == Tab::Entities => {
            app.next_page();
        }
        KeyCode::Char('p') if app.active_tab == Tab::Entities => {
            app.prev_page();
        }
        KeyCode::Enter if app.active_tab == Tab::Entities => {
            app.open_detail().await;
        }
        _ => {}
    }
}

fn handle_detail_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.close_detail(),
        KeyCode::Char('j') | KeyCode::Down => app.scroll_detail_down(),
        KeyCode::Char('k') | KeyCode::Up => app.scroll_detail_up(),
        _ => {}
    }
}

async fn handle_filter_bar_key(app: &mut App, key: KeyEvent) {
    let bar = app.filter.active_bar.expect("bar is Some");

    match key.code {
        KeyCode::Esc => {
            app.filter.active_bar = None;
        }
        // Close bar with same key that opened it
        KeyCode::Char('t') if bar == FilterBar::Type => {
            app.filter.active_bar = None;
        }
        KeyCode::Char('f') if bar == FilterBar::Status => {
            app.filter.active_bar = None;
        }
        KeyCode::Char('P') if bar == FilterBar::Priority => {
            app.filter.active_bar = None;
        }
        KeyCode::Char('0') => {
            match bar {
                FilterBar::Type => app.filter.clear_types(),
                FilterBar::Status => app.filter.clear_statuses(),
                FilterBar::Priority => app.filter.clear_priorities(),
            }
            app.reset_page();
            app.refresh_entities().await;
        }
        KeyCode::Char(c @ '1'..='7') => {
            let idx = (c as u8 - b'1') as usize;
            match bar {
                FilterBar::Type => {
                    let types = [
                        EntityType::Task,
                        EntityType::Module,
                        EntityType::Service,
                        EntityType::Agent,
                        EntityType::Plan,
                        EntityType::Doc,
                        EntityType::Lesson,
                    ];
                    if let Some(&t) = types.get(idx) {
                        app.filter.toggle_type(t);
                        app.reset_page();
                        app.refresh_entities().await;
                    }
                }
                FilterBar::Status => {
                    let statuses = [
                        EntityStatus::Open,
                        EntityStatus::InProgress,
                        EntityStatus::Blocked,
                        EntityStatus::Closed,
                    ];
                    if let Some(&s) = statuses.get(idx) {
                        app.filter.toggle_status(s);
                        app.reset_page();
                        app.refresh_entities().await;
                    }
                }
                FilterBar::Priority => {
                    if let Ok(val) = u8::try_from(idx) {
                        if let Ok(p) = Priority::new(val) {
                            app.filter.toggle_priority(p);
                            app.reset_page();
                            app.refresh_entities().await;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
