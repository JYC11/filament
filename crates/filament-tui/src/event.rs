use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use filament_core::dto::{EntitySortField, MessageSortField};
use filament_core::models::{EntityStatus, EntityType, MessageStatus, MessageType, Priority};

use crate::app::{App, FilterBar, MessageFilterBar, Tab};

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
    // If reply mode is active, capture keys for text input first —
    // only Ctrl-C escapes; all other keys go to the reply buffer.
    if app.has_reply() {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.should_quit = true;
            return;
        }
        handle_reply_key(app, key).await;
        return;
    }

    // Global keys (active when not in reply mode)
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

    // If entity detail pane is open, capture keys for it
    if app.has_detail() {
        handle_detail_key(app, key);
        return;
    }

    // If message detail pane is open, capture keys for it
    if app.has_message_detail() {
        handle_message_detail_key(app, key);
        return;
    }

    // If an entity filter bar is open, capture keys for it
    if app.filter.active_bar.is_some() {
        handle_filter_bar_key(app, key).await;
        return;
    }

    // If a message filter bar is open, capture keys for it
    if app.msg_filter.active_bar.is_some() {
        handle_msg_filter_bar_key(app, key).await;
        return;
    }

    // Global navigation keys (only when no filter bar is open)
    let new_tab = match key.code {
        KeyCode::Tab => Some(app.active_tab.next()),
        KeyCode::BackTab => Some(app.active_tab.prev()),
        KeyCode::Char('1') => Some(Tab::Entities),
        KeyCode::Char('2') => Some(Tab::Agents),
        KeyCode::Char('3') => Some(Tab::Reservations),
        KeyCode::Char('4') => Some(Tab::Messages),
        KeyCode::Char('5') => Some(Tab::Config),
        KeyCode::Char('6') => Some(Tab::Analytics),
        _ => None,
    };
    if let Some(tab) = new_tab {
        app.active_tab = tab;
        return;
    }

    // Tab-specific keys
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.select_next(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
        _ => match app.active_tab {
            Tab::Entities => handle_entities_key(app, key).await,
            Tab::Messages => handle_messages_key(app, key).await,
            Tab::Agents => {
                if key.code == KeyCode::Char('h') {
                    app.agent_show_history = !app.agent_show_history;
                    app.refresh_agents().await;
                }
            }
            Tab::Analytics => {
                if key.code == KeyCode::Enter {
                    app.refresh_analytics().await;
                }
            }
            _ => {}
        },
    }
}

async fn handle_entities_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('t') if !app.filter.ready_only => {
            app.filter.active_bar = Some(FilterBar::Type);
        }
        KeyCode::Char('f') if !app.filter.ready_only => {
            app.filter.active_bar = Some(FilterBar::Status);
        }
        KeyCode::Char('P') => {
            app.filter.active_bar = Some(FilterBar::Priority);
        }
        KeyCode::Char('s') => {
            app.filter.active_bar = Some(FilterBar::Sort);
        }
        KeyCode::Char('F') => {
            app.filter.toggle_ready_only();
            app.reset_page();
            app.refresh_entities().await;
        }
        KeyCode::Char('n') => {
            app.next_page().await;
        }
        KeyCode::Char('p') => {
            app.prev_page().await;
        }
        KeyCode::Enter => {
            app.open_detail().await;
        }
        _ => {}
    }
}

async fn handle_messages_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('t') => {
            app.msg_filter.active_bar = Some(MessageFilterBar::Type);
        }
        KeyCode::Char('f') => {
            app.msg_filter.active_bar = Some(MessageFilterBar::Status);
        }
        KeyCode::Char('s') => {
            app.msg_filter.active_bar = Some(MessageFilterBar::Sort);
        }
        KeyCode::Char('a') => {
            app.cycle_msg_participant().await;
        }
        KeyCode::Char('n') => {
            app.msg_next_page().await;
        }
        KeyCode::Char('p') => {
            app.msg_prev_page().await;
        }
        KeyCode::Char('R') => {
            app.start_reply();
        }
        KeyCode::Enter => {
            app.open_message_detail().await;
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

fn handle_message_detail_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.close_message_detail(),
        KeyCode::Char('j') | KeyCode::Down => app.scroll_message_detail_down(),
        KeyCode::Char('k') | KeyCode::Up => app.scroll_message_detail_up(),
        KeyCode::Char('R') => {
            app.close_message_detail();
            app.start_reply();
        }
        _ => {}
    }
}

async fn handle_reply_key(app: &mut App, key: KeyEvent) {
    let Some(ref mut reply) = app.reply else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            app.cancel_reply();
        }
        KeyCode::Enter => {
            app.send_reply().await;
        }
        KeyCode::Backspace => reply.backspace(),
        KeyCode::Delete => reply.delete(),
        KeyCode::Left => reply.move_left(),
        KeyCode::Right => reply.move_right(),
        KeyCode::Home => reply.home(),
        KeyCode::End => reply.end(),
        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            reply.cycle_type();
        }
        KeyCode::Char(c) => reply.insert_char(c),
        _ => {}
    }
}

async fn handle_filter_bar_key(app: &mut App, key: KeyEvent) {
    let bar = app.filter.active_bar.expect("bar is Some");

    // Sort bar has its own handling
    if bar == FilterBar::Sort {
        handle_sort_bar_key(app, key).await;
        return;
    }

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
                FilterBar::Sort => unreachable!(),
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
                FilterBar::Sort => unreachable!(),
            }
        }
        _ => {}
    }
}

async fn handle_sort_bar_key(app: &mut App, key: KeyEvent) {
    const SORT_FIELDS: [EntitySortField; 5] = [
        EntitySortField::Name,
        EntitySortField::Priority,
        EntitySortField::Status,
        EntitySortField::Updated,
        EntitySortField::Created,
    ];

    match key.code {
        KeyCode::Esc | KeyCode::Char('s') => {
            app.filter.active_bar = None;
        }
        KeyCode::Char(c @ '1'..='5') => {
            let idx = (c as u8 - b'1') as usize;
            if let Some(&field) = SORT_FIELDS.get(idx) {
                app.sort.set_field(field);
                app.reset_page();
                app.refresh_entities().await;
            }
        }
        _ => {}
    }
}

async fn handle_msg_filter_bar_key(app: &mut App, key: KeyEvent) {
    let bar = app.msg_filter.active_bar.expect("bar is Some");

    match bar {
        MessageFilterBar::Type => handle_msg_type_bar(app, key).await,
        MessageFilterBar::Status => handle_msg_status_bar(app, key).await,
        MessageFilterBar::Sort => handle_msg_sort_bar_key(app, key).await,
    }
}

async fn handle_msg_type_bar(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('t') => {
            app.msg_filter.active_bar = None;
        }
        KeyCode::Char('0') => {
            app.msg_filter.clear_types();
            app.msg_reset_page();
            app.refresh_messages().await;
        }
        KeyCode::Char(c @ '1'..='4') => {
            let types = [
                MessageType::Text,
                MessageType::Question,
                MessageType::Blocker,
                MessageType::Artifact,
            ];
            let idx = (c as u8 - b'1') as usize;
            if let Some(t) = types.get(idx) {
                app.msg_filter.toggle_type(t.clone());
                app.msg_reset_page();
                app.refresh_messages().await;
            }
        }
        _ => {}
    }
}

async fn handle_msg_status_bar(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('f') => {
            app.msg_filter.active_bar = None;
        }
        KeyCode::Char('1') => {
            app.msg_filter.read_status = None; // All
            app.msg_reset_page();
            app.refresh_messages().await;
        }
        KeyCode::Char('2') => {
            app.msg_filter.read_status = Some(MessageStatus::Unread);
            app.msg_reset_page();
            app.refresh_messages().await;
        }
        KeyCode::Char('3') => {
            app.msg_filter.read_status = Some(MessageStatus::Read);
            app.msg_reset_page();
            app.refresh_messages().await;
        }
        _ => {}
    }
}

async fn handle_msg_sort_bar_key(app: &mut App, key: KeyEvent) {
    const SORT_FIELDS: [MessageSortField; 4] = [
        MessageSortField::Time,
        MessageSortField::Type,
        MessageSortField::From,
        MessageSortField::Status,
    ];

    match key.code {
        KeyCode::Esc | KeyCode::Char('s') => {
            app.msg_filter.active_bar = None;
        }
        KeyCode::Char(c @ '1'..='4') => {
            let idx = (c as u8 - b'1') as usize;
            if let Some(&field) = SORT_FIELDS.get(idx) {
                app.msg_sort.set_field(field);
                app.msg_reset_page();
                app.refresh_messages().await;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ReplyState;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use filament_core::connection::FilamentConnection;
    use filament_core::schema::init_test_pool;
    use filament_core::store::FilamentStore;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    async fn test_app() -> App {
        let pool = init_test_pool().await.unwrap();
        let store = FilamentStore::new(pool);
        let conn = FilamentConnection::Direct(store);
        App::new(conn)
    }

    fn enter_reply(app: &mut App) {
        app.reply = Some(ReplyState::new(
            "agent-a".to_string(),
            "msg-123".to_string(),
        ));
    }

    fn fake_detail() -> crate::views::detail::DetailData {
        use filament_core::models::{Entity, EntityCommon, NonEmptyString, Slug};
        crate::views::detail::DetailData {
            entity: Entity::Task(EntityCommon {
                id: "test-id".into(),
                slug: Slug::new(),
                name: NonEmptyString::new("Test").unwrap(),
                summary: String::new(),
                key_facts: serde_json::json!({}),
                content: None,
                status: EntityStatus::Open,
                priority: Priority::new(2).unwrap(),
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }),
            relations: Vec::new(),
            events: Vec::new(),
            blocker_depth: 0,
            name_map: std::collections::HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Reply mode: all printable keys go to buffer, not global handlers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn reply_mode_captures_lowercase_r() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key(KeyCode::Char('r'))).await;

        assert!(!app.should_quit);
        assert!(app.reply.is_some());
        assert_eq!(app.reply.as_ref().unwrap().buffer, "r");
    }

    #[tokio::test]
    async fn reply_mode_captures_lowercase_q() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key(KeyCode::Char('q'))).await;

        assert!(!app.should_quit, "'q' must not quit while in reply mode");
        assert_eq!(app.reply.as_ref().unwrap().buffer, "q");
    }

    #[tokio::test]
    async fn reply_mode_captures_number_keys() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key(KeyCode::Char('1'))).await;
        handle_key(&mut app, key(KeyCode::Char('2'))).await;

        assert_eq!(
            app.active_tab,
            Tab::Entities,
            "numbers must not switch tabs"
        );
        assert_eq!(app.reply.as_ref().unwrap().buffer, "12");
    }

    #[tokio::test]
    async fn reply_mode_captures_tab_key() {
        let mut app = test_app().await;
        enter_reply(&mut app);
        let initial_tab = app.active_tab;

        handle_key(&mut app, key(KeyCode::Tab)).await;

        assert_eq!(
            app.active_tab, initial_tab,
            "Tab must not switch tabs in reply mode"
        );
    }

    #[tokio::test]
    async fn reply_mode_esc_cancels() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key(KeyCode::Esc)).await;

        assert!(app.reply.is_none());
    }

    #[tokio::test]
    async fn reply_mode_ctrl_c_quits() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key_ctrl('c')).await;

        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn reply_mode_full_sentence() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        for c in "test response".chars() {
            handle_key(&mut app, key(KeyCode::Char(c))).await;
        }

        assert_eq!(app.reply.as_ref().unwrap().buffer, "test response");
    }

    #[tokio::test]
    async fn reply_mode_backspace_deletes() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key(KeyCode::Char('a'))).await;
        handle_key(&mut app, key(KeyCode::Char('b'))).await;
        handle_key(&mut app, key(KeyCode::Backspace)).await;

        assert_eq!(app.reply.as_ref().unwrap().buffer, "a");
    }

    // -----------------------------------------------------------------------
    // Normal mode: global keys work as expected
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn normal_mode_q_quits() {
        let mut app = test_app().await;

        handle_key(&mut app, key(KeyCode::Char('q'))).await;

        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn normal_mode_r_does_not_quit() {
        let mut app = test_app().await;

        handle_key(&mut app, key(KeyCode::Char('r'))).await;

        assert!(!app.should_quit);
        assert!(app.reply.is_none());
    }

    #[tokio::test]
    async fn normal_mode_number_switches_tab() {
        let mut app = test_app().await;

        handle_key(&mut app, key(KeyCode::Char('4'))).await;

        assert_eq!(app.active_tab, Tab::Messages);
    }

    #[tokio::test]
    async fn normal_mode_tab_cycles() {
        let mut app = test_app().await;
        assert_eq!(app.active_tab, Tab::Entities);

        handle_key(&mut app, key(KeyCode::Tab)).await;

        assert_eq!(app.active_tab, Tab::Agents);
    }

    #[tokio::test]
    async fn normal_mode_backtab_cycles_backward() {
        let mut app = test_app().await;
        app.active_tab = Tab::Agents;

        handle_key(&mut app, key(KeyCode::BackTab)).await;

        assert_eq!(app.active_tab, Tab::Entities);
    }

    #[tokio::test]
    async fn normal_mode_ctrl_c_quits() {
        let mut app = test_app().await;

        handle_key(&mut app, key_ctrl('c')).await;

        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn normal_mode_all_tab_numbers() {
        for (ch, expected) in [
            ('1', Tab::Entities),
            ('2', Tab::Agents),
            ('3', Tab::Reservations),
            ('4', Tab::Messages),
            ('5', Tab::Config),
            ('6', Tab::Analytics),
        ] {
            let mut app = test_app().await;
            handle_key(&mut app, key(KeyCode::Char(ch))).await;
            assert_eq!(
                app.active_tab, expected,
                "key '{ch}' should switch to {expected:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Entity tab key handlers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn entities_t_opens_type_filter_bar() {
        let mut app = test_app().await;
        app.active_tab = Tab::Entities;

        handle_key(&mut app, key(KeyCode::Char('t'))).await;

        assert_eq!(app.filter.active_bar, Some(FilterBar::Type));
    }

    #[tokio::test]
    async fn entities_t_blocked_in_ready_mode() {
        let mut app = test_app().await;
        app.active_tab = Tab::Entities;
        app.filter.ready_only = true;

        handle_key(&mut app, key(KeyCode::Char('t'))).await;

        assert!(app.filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn entities_f_opens_status_filter_bar() {
        let mut app = test_app().await;
        app.active_tab = Tab::Entities;

        handle_key(&mut app, key(KeyCode::Char('f'))).await;

        assert_eq!(app.filter.active_bar, Some(FilterBar::Status));
    }

    #[tokio::test]
    async fn entities_f_blocked_in_ready_mode() {
        let mut app = test_app().await;
        app.active_tab = Tab::Entities;
        app.filter.ready_only = true;

        handle_key(&mut app, key(KeyCode::Char('f'))).await;

        assert!(app.filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn entities_shift_p_opens_priority_bar() {
        let mut app = test_app().await;
        app.active_tab = Tab::Entities;

        handle_key(&mut app, key(KeyCode::Char('P'))).await;

        assert_eq!(app.filter.active_bar, Some(FilterBar::Priority));
    }

    #[tokio::test]
    async fn entities_s_opens_sort_bar() {
        let mut app = test_app().await;
        app.active_tab = Tab::Entities;

        handle_key(&mut app, key(KeyCode::Char('s'))).await;

        assert_eq!(app.filter.active_bar, Some(FilterBar::Sort));
    }

    #[tokio::test]
    async fn entities_shift_f_toggles_ready_mode() {
        let mut app = test_app().await;
        app.active_tab = Tab::Entities;
        assert!(!app.filter.ready_only);

        handle_key(&mut app, key(KeyCode::Char('F'))).await;

        assert!(app.filter.ready_only);
    }

    // -----------------------------------------------------------------------
    // Message tab key handlers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn messages_t_opens_type_filter_bar() {
        let mut app = test_app().await;
        app.active_tab = Tab::Messages;

        handle_key(&mut app, key(KeyCode::Char('t'))).await;

        assert_eq!(app.msg_filter.active_bar, Some(MessageFilterBar::Type));
    }

    #[tokio::test]
    async fn messages_f_opens_status_filter_bar() {
        let mut app = test_app().await;
        app.active_tab = Tab::Messages;

        handle_key(&mut app, key(KeyCode::Char('f'))).await;

        assert_eq!(app.msg_filter.active_bar, Some(MessageFilterBar::Status));
    }

    #[tokio::test]
    async fn messages_s_opens_sort_bar() {
        let mut app = test_app().await;
        app.active_tab = Tab::Messages;

        handle_key(&mut app, key(KeyCode::Char('s'))).await;

        assert_eq!(app.msg_filter.active_bar, Some(MessageFilterBar::Sort));
    }

    // -----------------------------------------------------------------------
    // Entity filter bar key handlers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn type_filter_bar_esc_closes() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Type);

        handle_key(&mut app, key(KeyCode::Esc)).await;

        assert!(app.filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn type_filter_bar_same_key_closes() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Type);

        handle_key(&mut app, key(KeyCode::Char('t'))).await;

        assert!(app.filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn status_filter_bar_same_key_closes() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Status);

        handle_key(&mut app, key(KeyCode::Char('f'))).await;

        assert!(app.filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn priority_filter_bar_same_key_closes() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Priority);

        handle_key(&mut app, key(KeyCode::Char('P'))).await;

        assert!(app.filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn type_filter_bar_0_clears() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Type);
        app.filter.toggle_type(EntityType::Module);

        handle_key(&mut app, key(KeyCode::Char('0'))).await;

        assert!(app.filter.types.is_empty());
    }

    #[tokio::test]
    async fn type_filter_bar_1_toggles_task() {
        let mut app = test_app().await;
        app.filter.types.clear();
        app.filter.active_bar = Some(FilterBar::Type);

        handle_key(&mut app, key(KeyCode::Char('1'))).await;

        assert!(app.filter.types.contains(&EntityType::Task));
    }

    #[tokio::test]
    async fn sort_bar_esc_closes() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Sort);

        handle_key(&mut app, key(KeyCode::Esc)).await;

        assert!(app.filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn sort_bar_s_closes() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Sort);

        handle_key(&mut app, key(KeyCode::Char('s'))).await;

        assert!(app.filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn sort_bar_number_sets_field() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Sort);

        handle_key(&mut app, key(KeyCode::Char('1'))).await;

        assert_eq!(app.sort.field, EntitySortField::Name);
    }

    // -----------------------------------------------------------------------
    // Message filter bar key handlers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn msg_type_bar_esc_closes() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Type);

        handle_key(&mut app, key(KeyCode::Esc)).await;

        assert!(app.msg_filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn msg_type_bar_t_closes() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Type);

        handle_key(&mut app, key(KeyCode::Char('t'))).await;

        assert!(app.msg_filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn msg_type_bar_0_clears() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Type);
        app.msg_filter.toggle_type(MessageType::Text);

        handle_key(&mut app, key(KeyCode::Char('0'))).await;

        assert!(app.msg_filter.msg_types.is_empty());
    }

    #[tokio::test]
    async fn msg_type_bar_1_toggles_text() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Type);

        handle_key(&mut app, key(KeyCode::Char('1'))).await;

        assert!(app.msg_filter.msg_types.contains(&MessageType::Text));
    }

    #[tokio::test]
    async fn msg_status_bar_esc_closes() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Status);

        handle_key(&mut app, key(KeyCode::Esc)).await;

        assert!(app.msg_filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn msg_status_bar_f_closes() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Status);

        handle_key(&mut app, key(KeyCode::Char('f'))).await;

        assert!(app.msg_filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn msg_status_bar_2_sets_unread() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Status);

        handle_key(&mut app, key(KeyCode::Char('2'))).await;

        assert_eq!(app.msg_filter.read_status, Some(MessageStatus::Unread));
    }

    #[tokio::test]
    async fn msg_status_bar_3_sets_read() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Status);

        handle_key(&mut app, key(KeyCode::Char('3'))).await;

        assert_eq!(app.msg_filter.read_status, Some(MessageStatus::Read));
    }

    #[tokio::test]
    async fn msg_status_bar_1_clears_to_all() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Status);
        app.msg_filter.read_status = Some(MessageStatus::Unread);

        handle_key(&mut app, key(KeyCode::Char('1'))).await;

        assert!(app.msg_filter.read_status.is_none());
    }

    #[tokio::test]
    async fn msg_sort_bar_esc_closes() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Sort);

        handle_key(&mut app, key(KeyCode::Esc)).await;

        assert!(app.msg_filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn msg_sort_bar_s_closes() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Sort);

        handle_key(&mut app, key(KeyCode::Char('s'))).await;

        assert!(app.msg_filter.active_bar.is_none());
    }

    #[tokio::test]
    async fn msg_sort_bar_number_sets_field() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Sort);

        handle_key(&mut app, key(KeyCode::Char('3'))).await;

        assert_eq!(app.msg_sort.field, MessageSortField::From);
    }

    // -----------------------------------------------------------------------
    // Detail pane key handlers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn detail_pane_esc_closes() {
        let mut app = test_app().await;
        // Fake open a detail pane
        app.detail = Some(fake_detail());

        handle_key(&mut app, key(KeyCode::Esc)).await;

        assert!(app.detail.is_none());
    }

    #[tokio::test]
    async fn detail_pane_j_scrolls_down() {
        let mut app = test_app().await;
        app.detail = Some(fake_detail());

        handle_key(&mut app, key(KeyCode::Char('j'))).await;

        assert_eq!(app.detail_scroll, 1);
    }

    #[tokio::test]
    async fn detail_pane_k_scrolls_up() {
        let mut app = test_app().await;
        app.detail = Some(fake_detail());
        app.detail_scroll = 5;

        handle_key(&mut app, key(KeyCode::Char('k'))).await;

        assert_eq!(app.detail_scroll, 4);
    }

    // -----------------------------------------------------------------------
    // Detail pane blocks normal navigation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn detail_pane_blocks_tab_switching() {
        let mut app = test_app().await;
        app.detail = Some(fake_detail());

        handle_key(&mut app, key(KeyCode::Char('4'))).await;

        // Should NOT switch tab — detail pane captures keys
        assert_eq!(app.active_tab, Tab::Entities);
    }

    // -----------------------------------------------------------------------
    // Agent tab key handlers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn agent_h_toggles_history() {
        let mut app = test_app().await;
        app.active_tab = Tab::Agents;
        assert!(!app.agent_show_history);

        handle_key(&mut app, key(KeyCode::Char('h'))).await;

        assert!(app.agent_show_history);
    }

    // -----------------------------------------------------------------------
    // Navigation j/k
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn j_k_navigation_does_not_crash_empty() {
        let mut app = test_app().await;
        // entities list is empty
        handle_key(&mut app, key(KeyCode::Char('j'))).await;
        handle_key(&mut app, key(KeyCode::Char('k'))).await;
        // no crash = pass
    }

    // -----------------------------------------------------------------------
    // Reply mode: Ctrl-T cycles type, Delete/Left/Right/Home/End
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn reply_mode_ctrl_t_cycles_type() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key_ctrl('t')).await;

        assert_eq!(app.reply.as_ref().unwrap().msg_type, MessageType::Question);
    }

    #[tokio::test]
    async fn reply_mode_delete_key() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key(KeyCode::Char('a'))).await;
        handle_key(&mut app, key(KeyCode::Char('b'))).await;
        handle_key(&mut app, key(KeyCode::Home)).await;
        handle_key(&mut app, key(KeyCode::Delete)).await;

        assert_eq!(app.reply.as_ref().unwrap().buffer, "b");
    }

    #[tokio::test]
    async fn reply_mode_left_right_navigation() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key(KeyCode::Char('a'))).await;
        handle_key(&mut app, key(KeyCode::Char('b'))).await;
        handle_key(&mut app, key(KeyCode::Left)).await;
        handle_key(&mut app, key(KeyCode::Char('X'))).await;

        assert_eq!(app.reply.as_ref().unwrap().buffer, "aXb");
    }

    #[tokio::test]
    async fn reply_mode_home_end() {
        let mut app = test_app().await;
        enter_reply(&mut app);

        handle_key(&mut app, key(KeyCode::Char('a'))).await;
        handle_key(&mut app, key(KeyCode::Char('b'))).await;
        handle_key(&mut app, key(KeyCode::Home)).await;
        handle_key(&mut app, key(KeyCode::Char('X'))).await;

        assert_eq!(app.reply.as_ref().unwrap().buffer, "Xab");

        handle_key(&mut app, key(KeyCode::End)).await;
        handle_key(&mut app, key(KeyCode::Char('Z'))).await;

        assert_eq!(app.reply.as_ref().unwrap().buffer, "XabZ");
    }

    // -----------------------------------------------------------------------
    // Filter bar blocks global keys
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn filter_bar_blocks_tab_switch() {
        let mut app = test_app().await;
        app.filter.active_bar = Some(FilterBar::Type);

        handle_key(&mut app, key(KeyCode::Char('4'))).await;

        // '4' is out of type range (1-7 valid) so it's a no-op,
        // but it should NOT switch tabs
        assert_eq!(app.active_tab, Tab::Entities);
    }

    #[tokio::test]
    async fn msg_filter_bar_blocks_tab_switch() {
        let mut app = test_app().await;
        app.msg_filter.active_bar = Some(MessageFilterBar::Type);

        handle_key(&mut app, key(KeyCode::Tab)).await;

        assert_eq!(app.active_tab, Tab::Entities);
    }
}
