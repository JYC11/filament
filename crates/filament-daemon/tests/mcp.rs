use filament_core::connection::FilamentConnection;
use filament_core::schema::init_pool;
use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};

/// Helper: create a direct `FilamentConnection` backed by a fresh temp DB.
async fn test_connection() -> (FilamentConnection, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let pool = init_pool(db_path.to_str().unwrap())
        .await
        .expect("init pool");
    let store = filament_core::store::FilamentStore::new(pool);
    (FilamentConnection::Direct(store), tmp)
}

/// Minimal MCP client handler (needed by rmcp to create a client service).
#[derive(Default, Clone)]
struct TestClient;
impl ClientHandler for TestClient {}

/// Spawn MCP server on one end of a duplex, return client peer on the other.
async fn start_mcp_client(
    conn: FilamentConnection,
) -> rmcp::service::RunningService<rmcp::RoleClient, TestClient> {
    let (server_io, client_io) = tokio::io::duplex(16384);

    tokio::spawn(async move {
        let _ = filament_daemon::mcp::run_mcp_transport(conn, server_io).await;
    });

    TestClient.serve(client_io).await.expect("MCP client init")
}

/// Spawn MCP server with tool filtering.
async fn start_mcp_client_filtered(
    conn: FilamentConnection,
    allowed: &'static [&'static str],
) -> rmcp::service::RunningService<rmcp::RoleClient, TestClient> {
    let (server_io, client_io) = tokio::io::duplex(16384);

    tokio::spawn(async move {
        let _ =
            filament_daemon::mcp::run_mcp_transport_filtered(conn, server_io, Some(allowed)).await;
    });

    TestClient.serve(client_io).await.expect("MCP client init")
}

/// Build a `CallToolRequestParams` from name and JSON args.
fn call(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    CallToolRequestParams {
        meta: None,
        name: name.to_string().into(),
        arguments: args.as_object().cloned(),
        task: None,
    }
}

/// Extract text from a `CallToolResult`.
fn extract_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.raw.as_text().map(|t| t.text.as_str()))
        .collect::<Vec<_>>()
        .join("")
}

/// Extract slug from an add tool response: "Created: {slug} ({id})"
fn extract_slug(text: &str) -> String {
    text.strip_prefix("Created: ")
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn tools_list_returns_all_tools() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;

    let result = client
        .peer()
        .list_tools(Default::default())
        .await
        .expect("list_tools");

    assert_eq!(
        result.tools.len(),
        filament_daemon::mcp::TOOL_COUNT,
        "expected {} tools, got {}: {:?}",
        filament_daemon::mcp::TOOL_COUNT,
        result.tools.len(),
        result.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    // Verify expected tool names
    let names: Vec<&str> = result.tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in &[
        "fl_task_ready",
        "fl_task_close",
        "fl_context",
        "fl_message_send",
        "fl_message_inbox",
        "fl_message_read",
        "fl_reserve",
        "fl_release",
        "fl_reservations",
        "fl_inspect",
        "fl_list",
        "fl_add",
        "fl_update",
        "fl_delete",
        "fl_relate",
        "fl_unrelate",
    ] {
        assert!(names.contains(expected), "missing tool: {expected}");
    }

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_add_and_list() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    // Add an entity
    let result = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({
                "name": "mcp-test-task",
                "entity_type": "task",
                "summary": "Test task via MCP",
            }),
        ))
        .await
        .expect("call filament_add");

    assert!(
        !result.is_error.unwrap_or(false),
        "fl_add should succeed: {result:?}"
    );
    let text = extract_text(&result);
    assert!(text.contains("Created:"), "expected 'Created:' in: {text}");

    // List entities
    let result = peer
        .call_tool(call(
            "fl_list",
            serde_json::json!({ "entity_type": "task" }),
        ))
        .await
        .expect("call filament_list");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    assert!(
        text.contains("mcp-test-task"),
        "list should contain the entity: {text}"
    );

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_inspect_entity() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    let add_result = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({
                "name": "inspect-me",
                "entity_type": "doc",
                "summary": "A doc to inspect",
            }),
        ))
        .await
        .expect("add");

    let slug = extract_slug(&extract_text(&add_result));

    let result = peer
        .call_tool(call(
            "fl_inspect",
            serde_json::json!({ "slug": slug }),
        ))
        .await
        .expect("inspect");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(parsed["entity"]["name"], "inspect-me");
    assert_eq!(parsed["entity"]["entity_type"], "doc");

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_task_close() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    let add_result = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({
                "name": "closeable-task",
                "entity_type": "task",
                "summary": "Will be closed",
            }),
        ))
        .await
        .expect("add");

    let slug = extract_slug(&extract_text(&add_result));

    let result = peer
        .call_tool(call(
            "fl_task_close",
            serde_json::json!({ "slug": slug }),
        ))
        .await
        .expect("close");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    assert!(text.contains("Closed:"), "expected 'Closed:' in: {text}");

    // Verify it's closed via inspect
    let result = peer
        .call_tool(call(
            "fl_inspect",
            serde_json::json!({ "slug": slug }),
        ))
        .await
        .expect("inspect");

    let text = extract_text(&result);
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(parsed["entity"]["status"], "closed");

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_update_entity() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    let add_result = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({
                "name": "updatable",
                "entity_type": "task",
                "summary": "Before update",
            }),
        ))
        .await
        .expect("add");

    let slug = extract_slug(&extract_text(&add_result));

    let result = peer
        .call_tool(call(
            "fl_update",
            serde_json::json!({
                "slug": slug,
                "summary": "After update",
                "status": "in_progress",
            }),
        ))
        .await
        .expect("update");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    assert!(text.contains("summary and status"), "got: {text}");

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_messaging() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    // Create agent entities first (message_send validates recipient exists)
    let add_a = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({"name": "agent-a", "entity_type": "agent", "summary": "Agent A"}),
        ))
        .await
        .expect("add agent-a");
    assert!(!add_a.is_error.unwrap_or(false));
    let slug_a = extract_slug(&extract_text(&add_a));

    let add_b = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({"name": "agent-b", "entity_type": "agent", "summary": "Agent B"}),
        ))
        .await
        .expect("add agent-b");
    assert!(!add_b.is_error.unwrap_or(false));
    let slug_b = extract_slug(&extract_text(&add_b));

    let result = peer
        .call_tool(call(
            "fl_message_send",
            serde_json::json!({
                "from_agent": slug_a,
                "to_agent": slug_b,
                "body": "Hello from MCP",
            }),
        ))
        .await
        .expect("send");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    assert!(text.contains("Message sent:"), "got: {text}");

    let result = peer
        .call_tool(call(
            "fl_message_inbox",
            serde_json::json!({ "agent": slug_b }),
        ))
        .await
        .expect("inbox");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    assert!(
        text.contains("Hello from MCP"),
        "inbox should contain message: {text}"
    );

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_reservations() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    let result = peer
        .call_tool(call(
            "fl_reserve",
            serde_json::json!({
                "file_glob": "src/**/*.rs",
                "agent": "test-agent",
                "ttl_secs": 60,
            }),
        ))
        .await
        .expect("reserve");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    assert!(text.contains("Reservation acquired:"), "got: {text}");

    let res_id = text.strip_prefix("Reservation acquired: ").unwrap();

    // List reservations
    let result = peer
        .call_tool(call(
            "fl_reservations",
            serde_json::json!({ "agent": "test-agent" }),
        ))
        .await
        .expect("list reservations");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    assert!(text.contains("src/**/*.rs"), "reservations: {text}");

    // Release
    let result = peer
        .call_tool(call(
            "fl_release",
            serde_json::json!({ "reservation_id": res_id }),
        ))
        .await
        .expect("release");

    assert!(!result.is_error.unwrap_or(false));
    assert_eq!(extract_text(&result), "Reservation released");

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_task_ready() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    for name in &["ready-a", "ready-b"] {
        peer.call_tool(call(
            "fl_add",
            serde_json::json!({
                "name": name,
                "entity_type": "task",
                "summary": format!("Task {name}"),
            }),
        ))
        .await
        .expect("add");
    }

    let result = peer
        .call_tool(call(
            "fl_task_ready",
            serde_json::json!({ "limit": 10 }),
        ))
        .await
        .expect("ready tasks");

    assert!(!result.is_error.unwrap_or(false));
    let text = extract_text(&result);
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&text).expect("valid JSON");
    assert!(
        tasks.len() >= 2,
        "expected >= 2 ready tasks, got {}",
        tasks.len()
    );

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_error_nonexistent_entity() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    let result = peer
        .call_tool(call(
            "fl_inspect",
            serde_json::json!({ "slug": "does-not-exist" }),
        ))
        .await
        .expect("inspect");

    assert!(
        result.is_error.unwrap_or(false),
        "inspecting nonexistent entity should be an error"
    );
    let text = extract_text(&result);
    assert!(
        text.contains("ENTITY_NOT_FOUND"),
        "error should contain code: {text}"
    );

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_update_validation_error() {
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    let add_result = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({
                "name": "needs-update",
                "entity_type": "task",
                "summary": "Test",
            }),
        ))
        .await
        .expect("add");

    let slug = extract_slug(&extract_text(&add_result));

    // Update with neither summary nor status — should error
    let result = peer
        .call_tool(call("fl_update", serde_json::json!({ "slug": slug })))
        .await
        .expect("update");

    assert!(
        result.is_error.unwrap_or(false),
        "update with no fields should be an error"
    );

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_filtering_blocks_disallowed() {
    let (conn, _tmp) = test_connection().await;

    // Only allow inspect and list — not add, delete, reserve, etc.
    let client = start_mcp_client_filtered(conn, &["fl_inspect", "fl_list"]).await;
    let peer = client.peer();

    // Allowed tool should work (list returns empty but no error)
    let result = peer
        .call_tool(call(
            "fl_list",
            serde_json::json!({ "entity_type": "task" }),
        ))
        .await
        .expect("list");
    assert!(
        !result.is_error.unwrap_or(false),
        "fl_list should be allowed"
    );

    // Disallowed tool should return an error
    let result = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({
                "name": "blocked-task",
                "entity_type": "task",
                "summary": "Should fail",
            }),
        ))
        .await
        .expect("add should return result");
    assert!(
        result.is_error.unwrap_or(false),
        "fl_add should be blocked by filter"
    );
    let text = extract_text(&result);
    assert!(
        text.contains("not allowed"),
        "error should mention 'not allowed': {text}"
    );

    // Another disallowed tool
    let result = peer
        .call_tool(call(
            "fl_reserve",
            serde_json::json!({
                "file_glob": "*.rs",
                "agent": "test",
                "ttl_secs": 60,
            }),
        ))
        .await
        .expect("reserve should return result");
    assert!(
        result.is_error.unwrap_or(false),
        "fl_reserve should be blocked by filter"
    );

    client.cancel().await.expect("cancel");
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_filtering_none_allows_all() {
    // No filter = all tools allowed (unfiltered mode, like CLI `fl mcp`)
    let (conn, _tmp) = test_connection().await;
    let client = start_mcp_client(conn).await;
    let peer = client.peer();

    // Add should work without filter
    let result = peer
        .call_tool(call(
            "fl_add",
            serde_json::json!({
                "name": "unfiltered-task",
                "entity_type": "task",
                "summary": "No filter",
            }),
        ))
        .await
        .expect("add");
    assert!(
        !result.is_error.unwrap_or(false),
        "fl_add should work without filter"
    );

    client.cancel().await.expect("cancel");
}
