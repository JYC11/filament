use ratatui::backend::TestBackend;
use ratatui::Terminal;

use filament_core::connection::FilamentConnection;
use filament_core::dto::{CreateEntityRequest, CreateRelationRequest};
use filament_core::models::{EntityType, RelationType};
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
        output.contains("Messages"),
        "should show tab bar with Messages"
    );
    assert!(output.contains("Graph"), "should show tab bar with Graph");
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
    assert_eq!(Tab::Tasks.next(), Tab::Agents);
    assert_eq!(Tab::Agents.next(), Tab::Reservations);
    assert_eq!(Tab::Reservations.next(), Tab::Messages);
    assert_eq!(Tab::Messages.next(), Tab::Graph);
    assert_eq!(Tab::Graph.next(), Tab::Tasks);

    assert_eq!(Tab::Tasks.prev(), Tab::Graph);
    assert_eq!(Tab::Agents.prev(), Tab::Tasks);
    assert_eq!(Tab::Reservations.prev(), Tab::Agents);
    assert_eq!(Tab::Messages.prev(), Tab::Reservations);
    assert_eq!(Tab::Graph.prev(), Tab::Messages);
}

#[tokio::test]
async fn graph_view_empty() {
    let conn = test_conn().await;
    let mut app = App::new(conn);
    app.active_tab = Tab::Graph;
    app.refresh_all().await;

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(output.contains("Graph"), "should show Graph block title");
    assert!(
        output.contains("No entities"),
        "should show empty graph message"
    );
}

#[tokio::test]
async fn graph_view_with_data() {
    let mut conn = test_conn().await;

    // Create two tasks with a blocks relation
    let (id_a, slug_a) = conn
        .create_entity(CreateEntityRequest {
            name: "Setup DB".to_string(),
            entity_type: EntityType::Task,
            summary: Some("Initialize database".to_string()),
            key_facts: None,
            content_path: None,
            priority: None,
        })
        .await
        .unwrap();

    let (id_b, _slug_b) = conn
        .create_entity(CreateEntityRequest {
            name: "Build API".to_string(),
            entity_type: EntityType::Task,
            summary: Some("REST API layer".to_string()),
            key_facts: None,
            content_path: None,
            priority: None,
        })
        .await
        .unwrap();

    // A blocks B (B depends on A)
    conn.create_relation(CreateRelationRequest {
        source_id: id_a.to_string(),
        target_id: id_b.to_string(),
        relation_type: RelationType::Blocks,
        weight: None,
        summary: None,
        metadata: None,
    })
    .await
    .unwrap();

    let mut app = App::new(conn);
    app.active_tab = Tab::Graph;
    app.refresh_all().await;

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| filament_tui::ui::draw(frame, &mut app))
        .unwrap();

    let output = buffer_to_string(&terminal);
    assert!(
        output.contains("Setup DB"),
        "should render parent task name: {output}"
    );
    assert!(
        output.contains("Build API"),
        "should render child task name: {output}"
    );
    assert!(
        output.contains(&slug_a.to_string()),
        "should show slug in graph: {output}"
    );
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
