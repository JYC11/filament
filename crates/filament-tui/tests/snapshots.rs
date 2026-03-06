use ratatui::backend::TestBackend;
use ratatui::Terminal;

use filament_core::connection::FilamentConnection;
use filament_core::dto::CreateEntityRequest;
use filament_core::models::EntityType;
use filament_core::schema::init_test_pool;
use filament_core::store::FilamentStore;

use filament_tui::{App, FilterState, Tab};

async fn test_conn() -> FilamentConnection {
    let pool = init_test_pool().await.unwrap();
    let store = FilamentStore::new(pool);
    FilamentConnection::Direct(store)
}

fn buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut result = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            result.push_str(cell.symbol());
        }
        result.push('\n');
    }
    result
}

#[tokio::test]
async fn entity_view_empty() {
    let conn = test_conn().await;
    let mut app = App::new(conn);
    app.refresh_all().await;

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("Entities"),
        "should show tab bar with Entities"
    );
    assert!(output.contains("Agents"), "should show tab bar with Agents");
    assert!(
        output.contains("Reservations"),
        "should show tab bar with Reservations"
    );
    assert!(
        output.contains("Messages"),
        "should show tab bar with Messages"
    );
    assert!(
        output.contains("Slug"),
        "should show entity table header Slug"
    );
    assert!(
        output.contains("Name"),
        "should show entity table header Name"
    );
    assert!(
        output.contains("Status"),
        "should show entity table header Status"
    );
}

#[tokio::test]
async fn entity_view_with_data() {
    let mut conn = test_conn().await;

    conn.create_entity(CreateEntityRequest {
        name: "Build widget".to_string(),
        entity_type: EntityType::Task,
        summary: Some("Build the widget".to_string()),
        key_facts: None,
        content_path: None,
        priority: None,
    })
    .await
    .unwrap();

    let mut app = App::new(conn);
    app.refresh_all().await;

    assert_eq!(app.entities.len(), 1, "should have one entity");
    assert_eq!(app.entities[0].entity.name().as_str(), "Build widget");

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("Build widget"),
        "should render entity name in table"
    );
}

#[tokio::test]
async fn agent_view_empty() {
    let conn = test_conn().await;
    let mut app = App::new(conn);
    app.active_tab = Tab::Agents;
    app.refresh_all().await;

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("Role"),
        "should show agent table header Role"
    );
    assert!(output.contains("PID"), "should show agent table header PID");
    assert!(
        output.contains("Duration"),
        "should show agent table header Duration"
    );
}

#[tokio::test]
async fn reservation_view_empty() {
    let conn = test_conn().await;
    let mut app = App::new(conn);
    app.active_tab = Tab::Reservations;
    app.refresh_all().await;

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("Agent"),
        "should show reservation table header Agent"
    );
    assert!(
        output.contains("Glob"),
        "should show reservation table header Glob"
    );
    assert!(
        output.contains("Time Left"),
        "should show reservation table header Time Left"
    );
}

#[tokio::test]
async fn message_view_empty() {
    let conn = test_conn().await;
    let mut app = App::new(conn);
    app.active_tab = Tab::Messages;
    app.refresh_all().await;

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("Kind"),
        "should show message table header Kind"
    );
    assert!(
        output.contains("Agent"),
        "should show message table header Agent"
    );
    assert!(
        output.contains("Body"),
        "should show message table header Body"
    );
}

#[test]
fn tab_switching() {
    assert_eq!(Tab::Entities.next(), Tab::Agents);
    assert_eq!(Tab::Agents.next(), Tab::Reservations);
    assert_eq!(Tab::Reservations.next(), Tab::Messages);
    assert_eq!(Tab::Messages.next(), Tab::Entities);

    assert_eq!(Tab::Entities.prev(), Tab::Messages);
    assert_eq!(Tab::Agents.prev(), Tab::Entities);
    assert_eq!(Tab::Reservations.prev(), Tab::Agents);
    assert_eq!(Tab::Messages.prev(), Tab::Reservations);
}

#[tokio::test]
async fn status_bar_renders() {
    let conn = test_conn().await;
    let mut app = App::new(conn);
    app.refresh_all().await;

    let backend = TestBackend::new(100, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("direct"),
        "should show connection mode 'direct'"
    );
    assert!(
        output.contains("refreshed"),
        "should show refresh timestamp"
    );
}

#[test]
fn filter_state_defaults() {
    let filter = FilterState::default();
    assert!(filter.types.contains(&EntityType::Task));
    assert!(filter
        .statuses
        .contains(&filament_core::models::EntityStatus::Open));
    assert!(filter.priorities.is_empty());
    assert!(!filter.ready_only);
    assert!(filter.active_bar.is_none());
    assert_eq!(filter.label(), "task | open");
}

#[test]
fn filter_state_toggle_type() {
    let mut filter = FilterState::default();
    // Start with {Task}, toggle Module on
    filter.toggle_type(EntityType::Module);
    assert!(filter.types.contains(&EntityType::Task));
    assert!(filter.types.contains(&EntityType::Module));
    // Toggle Task off
    filter.toggle_type(EntityType::Task);
    assert!(!filter.types.contains(&EntityType::Task));
    assert!(filter.is_single_type(EntityType::Module));
}

#[test]
fn filter_state_clear() {
    let mut filter = FilterState::default();
    filter.clear_types();
    assert!(filter.types.is_empty());
    filter.clear_statuses();
    assert!(filter.statuses.is_empty());
    assert_eq!(filter.label(), "all");
}

#[test]
fn filter_state_ready_only() {
    let mut filter = FilterState::default();
    filter.toggle_ready_only();
    assert!(filter.ready_only);
    assert_eq!(filter.label(), "ready");
    filter.toggle_ready_only();
    assert!(!filter.ready_only);
}

#[test]
fn filter_state_priority_toggle() {
    use filament_core::models::Priority;
    let mut filter = FilterState::default();
    let p0 = Priority::new(0).unwrap();
    let p1 = Priority::new(1).unwrap();
    filter.toggle_priority(p0);
    filter.toggle_priority(p1);
    assert!(filter.priorities.contains(&p0));
    assert!(filter.priorities.contains(&p1));
    // Toggle p0 off
    filter.toggle_priority(p0);
    assert!(!filter.priorities.contains(&p0));
}

#[tokio::test]
async fn entity_view_multi_type_columns() {
    let mut conn = test_conn().await;

    conn.create_entity(CreateEntityRequest {
        name: "My Task".to_string(),
        entity_type: EntityType::Task,
        summary: Some("A task".to_string()),
        key_facts: None,
        content_path: None,
        priority: None,
    })
    .await
    .unwrap();

    conn.create_entity(CreateEntityRequest {
        name: "My Module".to_string(),
        entity_type: EntityType::Module,
        summary: Some("A module".to_string()),
        key_facts: None,
        content_path: None,
        priority: None,
    })
    .await
    .unwrap();

    let mut app = App::new(conn);
    // Show all types
    app.filter.types.clear();
    app.filter.statuses.clear();
    app.refresh_all().await;

    assert_eq!(app.entities.len(), 2, "should have two entities");

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    // Generic columns should show Type column
    assert!(
        output.contains("Type"),
        "multi-type view should show Type column header: {output}"
    );
    assert!(output.contains("My Task"), "should show task: {output}");
    assert!(output.contains("My Module"), "should show module: {output}");
}

// ---------------------------------------------------------------------------
// Paging
// ---------------------------------------------------------------------------

#[tokio::test]
async fn paging_basics() {
    let mut conn = test_conn().await;

    // Create 3 tasks, set page_size to 2 so we get 2 pages
    for i in 1..=3 {
        conn.create_entity(CreateEntityRequest {
            name: format!("Task {i}"),
            entity_type: EntityType::Task,
            summary: Some(format!("task {i}")),
            key_facts: None,
            content_path: None,
            priority: None,
        })
        .await
        .unwrap();
    }

    let mut app = App::new(conn);
    app.page_size = 2;
    app.refresh_all().await;

    assert_eq!(app.entities.len(), 3);
    assert_eq!(app.total_pages(), 2);
    assert_eq!(app.visible_entities().len(), 2);
    assert!(!app.has_prev_page());
    assert!(app.has_next_page());

    app.next_page();
    assert_eq!(app.page, 1);
    assert_eq!(app.visible_entities().len(), 1);
    assert!(app.has_prev_page());
    assert!(!app.has_next_page());

    app.prev_page();
    assert_eq!(app.page, 0);
    assert_eq!(app.visible_entities().len(), 2);
}

#[tokio::test]
async fn paging_reset_on_filter_change() {
    let mut conn = test_conn().await;
    for i in 1..=3 {
        conn.create_entity(CreateEntityRequest {
            name: format!("Task {i}"),
            entity_type: EntityType::Task,
            summary: Some(format!("task {i}")),
            key_facts: None,
            content_path: None,
            priority: None,
        })
        .await
        .unwrap();
    }

    let mut app = App::new(conn);
    app.page_size = 2;
    app.refresh_all().await;
    app.next_page();
    assert_eq!(app.page, 1);

    // Changing filter resets page
    app.reset_page();
    assert_eq!(app.page, 0);
}

#[tokio::test]
async fn paging_no_indicator_single_page() {
    let mut conn = test_conn().await;
    conn.create_entity(CreateEntityRequest {
        name: "Only task".to_string(),
        entity_type: EntityType::Task,
        summary: Some("one task".to_string()),
        key_facts: None,
        content_path: None,
        priority: None,
    })
    .await
    .unwrap();

    let mut app = App::new(conn);
    app.refresh_all().await;

    assert_eq!(app.total_pages(), 1);

    let backend = TestBackend::new(100, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    // Single page should NOT show page indicator
    assert!(
        !output.contains("1/1"),
        "single page should not show page indicator: {output}"
    );
}

#[tokio::test]
async fn page_indicator_in_title() {
    let mut conn = test_conn().await;
    for i in 1..=3 {
        conn.create_entity(CreateEntityRequest {
            name: format!("Task {i}"),
            entity_type: EntityType::Task,
            summary: Some(format!("task {i}")),
            key_facts: None,
            content_path: None,
            priority: None,
        })
        .await
        .unwrap();
    }

    let mut app = App::new(conn);
    app.page_size = 2;
    app.refresh_all().await;

    let backend = TestBackend::new(100, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("1/2"),
        "should show page 1/2 in title: {output}"
    );
}

// ---------------------------------------------------------------------------
// format_seconds edge cases
// ---------------------------------------------------------------------------

#[test]
fn format_seconds_zero() {
    assert_eq!(filament_tui::views::format_seconds(0), "0s");
}

#[test]
fn format_seconds_one() {
    assert_eq!(filament_tui::views::format_seconds(1), "1s");
}

#[test]
fn format_seconds_59() {
    assert_eq!(filament_tui::views::format_seconds(59), "59s");
}

#[test]
fn format_seconds_60() {
    assert_eq!(filament_tui::views::format_seconds(60), "1m00s");
}

#[test]
fn format_seconds_119() {
    assert_eq!(filament_tui::views::format_seconds(119), "1m59s");
}

#[test]
fn format_seconds_3599() {
    assert_eq!(filament_tui::views::format_seconds(3599), "59m59s");
}

#[test]
fn format_seconds_3600() {
    assert_eq!(filament_tui::views::format_seconds(3600), "1h00m");
}

#[test]
fn format_seconds_7322() {
    assert_eq!(filament_tui::views::format_seconds(7322), "2h02m");
}

#[test]
fn format_seconds_negative() {
    let result = filament_tui::views::format_seconds(-5);
    assert_eq!(result, "-5s");
}
