use ratatui::backend::TestBackend;
use ratatui::Terminal;

use filament_core::connection::FilamentConnection;
use filament_core::models::CreateEntityRequest;
use filament_core::schema::init_test_pool;
use filament_core::store::FilamentStore;

use filament_tui::{App, Tab, TaskFilter};

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
async fn task_view_empty() {
    let conn = test_conn().await;
    let mut app = App::new(conn);
    app.refresh_all().await;

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(output.contains("Tasks"), "should show tab bar with Tasks");
    assert!(output.contains("Agents"), "should show tab bar with Agents");
    assert!(
        output.contains("Reservations"),
        "should show tab bar with Reservations"
    );
    assert!(
        output.contains("Slug"),
        "should show task table header Slug"
    );
    assert!(
        output.contains("Name"),
        "should show task table header Name"
    );
    assert!(
        output.contains("Status"),
        "should show task table header Status"
    );
}

#[tokio::test]
async fn task_view_with_data() {
    let mut conn = test_conn().await;

    // Insert a task via the public CreateEntityRequest API
    conn.create_entity(CreateEntityRequest {
        name: "Build widget".to_string(),
        entity_type: "task".to_string(),
        summary: Some("Build the widget".to_string()),
        key_facts: None,
        content_path: None,
        priority: None,
    })
    .await
    .unwrap();

    let mut app = App::new(conn);
    app.refresh_all().await;

    assert_eq!(app.tasks.len(), 1, "should have one task");
    assert_eq!(app.tasks[0].entity.name().as_str(), "Build widget");

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("Build widget"),
        "should render task name in table"
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

#[test]
fn tab_switching() {
    assert_eq!(Tab::Tasks.next(), Tab::Agents);
    assert_eq!(Tab::Agents.next(), Tab::Reservations);
    assert_eq!(Tab::Reservations.next(), Tab::Tasks);

    assert_eq!(Tab::Tasks.prev(), Tab::Reservations);
    assert_eq!(Tab::Agents.prev(), Tab::Tasks);
    assert_eq!(Tab::Reservations.prev(), Tab::Agents);
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
fn task_filter_cycle() {
    let mut filter = TaskFilter::default();
    assert_eq!(filter.label(), "open");

    filter.cycle();
    assert_eq!(filter.label(), "in_progress");

    filter.cycle();
    assert_eq!(filter.label(), "blocked");

    filter.cycle();
    assert_eq!(filter.label(), "closed");

    filter.cycle();
    assert_eq!(filter.label(), "all");

    filter.cycle();
    assert_eq!(filter.label(), "open");
}
