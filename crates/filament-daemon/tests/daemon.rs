use std::path::PathBuf;
use std::time::Duration;

use filament_core::client::DaemonClient;
use filament_core::schema::init_pool;
use filament_daemon::config::ServeConfig;
use tokio_util::sync::CancellationToken;

/// Helper: start a test daemon with a fresh DB in a temp dir.
/// Returns a `DaemonClient`, the cancel token, and the temp dir handle (for lifetime).
async fn start_test_daemon() -> (DaemonClient, CancellationToken, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join(".filament");
    std::fs::create_dir_all(&runtime_dir).expect("create runtime dir");

    let db_path = runtime_dir.join("filament.db");
    let socket_path = runtime_dir.join("filament.sock");
    let pid_path = runtime_dir.join("filament.pid");

    // Init the database with migrations
    let pool = init_pool(db_path.to_str().unwrap())
        .await
        .expect("init pool");
    drop(pool);

    let config = ServeConfig {
        socket_path: socket_path.clone(),
        db_path,
        pid_path,
        cleanup_interval_secs: 3600, // long interval for tests
    };

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        filament_daemon::serve(config, cancel_clone)
            .await
            .expect("daemon serve");
    });

    // Wait for socket to appear
    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let client = DaemonClient::connect(&socket_path)
        .await
        .expect("connect to daemon");

    (client, cancel, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn daemon_starts_and_accepts_connection() {
    let (_client, cancel, _tmp) = start_test_daemon().await;
    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn entity_crud_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create
    let id = client
        .create_entity(serde_json::json!({
            "name": "test-task",
            "entity_type": "task",
            "summary": "A test task",
            "priority": 1
        }))
        .await
        .expect("create entity");

    // Get
    let entity = client.get_entity(id.as_str()).await.expect("get entity");
    assert_eq!(entity.name, "test-task");
    assert_eq!(entity.summary, "A test task");

    // Update status
    client
        .update_entity_status(id.as_str(), "in_progress")
        .await
        .expect("update status");

    let entity = client.get_entity(id.as_str()).await.expect("get updated");
    assert_eq!(entity.status.as_str(), "in_progress");

    // List
    let entities = client
        .list_entities(Some("task"), None)
        .await
        .expect("list entities");
    assert_eq!(entities.len(), 1);

    // Delete
    client
        .delete_entity(id.as_str())
        .await
        .expect("delete entity");

    let result = client.get_entity(id.as_str()).await;
    assert!(result.is_err());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn relation_crud_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    let id_a = client
        .create_entity(serde_json::json!({
            "name": "module-a",
            "entity_type": "module",
        }))
        .await
        .expect("create a");

    let id_b = client
        .create_entity(serde_json::json!({
            "name": "module-b",
            "entity_type": "module",
        }))
        .await
        .expect("create b");

    // Create relation
    let _rel_id = client
        .create_relation(serde_json::json!({
            "source_id": id_a.as_str(),
            "target_id": id_b.as_str(),
            "relation_type": "depends_on",
        }))
        .await
        .expect("create relation");

    // List relations
    let rels = client
        .list_relations(id_a.as_str())
        .await
        .expect("list relations");
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].relation_type.as_str(), "depends_on");

    // Delete relation
    client
        .delete_relation(id_a.as_str(), id_b.as_str(), "depends_on")
        .await
        .expect("delete relation");

    let rels = client
        .list_relations(id_a.as_str())
        .await
        .expect("list after delete");
    assert!(rels.is_empty());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn message_operations_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Send a message
    let msg_id = client
        .send_message(serde_json::json!({
            "from_agent": "agent-a",
            "to_agent": "agent-b",
            "body": "Hello from the test",
            "msg_type": "text",
        }))
        .await
        .expect("send message");

    // Check inbox
    let inbox = client.get_inbox("agent-b").await.expect("get inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].body, "Hello from the test");

    // Mark as read
    client.mark_message_read(&msg_id).await.expect("mark read");

    // Inbox should be empty now
    let inbox = client.get_inbox("agent-b").await.expect("inbox after read");
    assert!(inbox.is_empty());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn reservation_operations_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Acquire
    let res_id = client
        .acquire_reservation("agent-a", "src/*.rs", true, 300)
        .await
        .expect("acquire reservation");

    // Release
    client
        .release_reservation(&res_id)
        .await
        .expect("release reservation");

    // Acquire again (should succeed since released)
    let _res_id2 = client
        .acquire_reservation("agent-b", "src/*.rs", true, 300)
        .await
        .expect("acquire after release");

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn graph_operations_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create tasks with dependency
    let task_a = client
        .create_entity(serde_json::json!({
            "name": "task-a",
            "entity_type": "task",
            "summary": "First task",
            "priority": 1,
        }))
        .await
        .expect("create task-a");

    let task_b = client
        .create_entity(serde_json::json!({
            "name": "task-b",
            "entity_type": "task",
            "summary": "Second task",
            "priority": 2,
        }))
        .await
        .expect("create task-b");

    // task-b depends_on task-a
    client
        .create_relation(serde_json::json!({
            "source_id": task_b.as_str(),
            "target_id": task_a.as_str(),
            "relation_type": "depends_on",
        }))
        .await
        .expect("create dependency");

    // Ready tasks: only task-a should be ready (task-b is blocked)
    let ready = client.ready_tasks().await.expect("ready tasks");
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].name, "task-a");

    // Critical path from task-b
    let path = client
        .critical_path(task_b.as_str())
        .await
        .expect("critical path");
    assert!(path.len() >= 1);

    // Impact score of task-a
    let score = client
        .impact_score(task_a.as_str())
        .await
        .expect("impact score");
    assert!(score >= 1);

    // Context query
    let context = client
        .context_query(task_a.as_str(), Some(2))
        .await
        .expect("context query");
    assert!(!context.is_empty());

    // Check cycle
    let has_cycle = client.check_cycle().await.expect("check cycle");
    assert!(!has_cycle);

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_clients() {
    let (client, cancel, tmp) = start_test_daemon().await;
    drop(client); // Don't need the initial client

    let socket_path: PathBuf = tmp.path().join(".filament/filament.sock");

    let mut handles = Vec::new();
    for i in 0..5 {
        let path = socket_path.clone();
        handles.push(tokio::spawn(async move {
            let mut c = DaemonClient::connect(&path).await.expect("connect");
            let name = format!("concurrent-entity-{i}");
            let id = c
                .create_entity(serde_json::json!({
                    "name": name,
                    "entity_type": "task",
                    "summary": format!("Created by client {i}"),
                }))
                .await
                .expect("create");
            let entity = c.get_entity(id.as_str()).await.expect("get");
            assert_eq!(entity.name.as_str(), name.as_str());
        }));
    }

    for h in handles {
        h.await.expect("join");
    }

    // Verify all 5 exist
    let mut verify_client = DaemonClient::connect(&socket_path).await.expect("connect");
    let all = verify_client
        .list_entities(Some("task"), None)
        .await
        .expect("list");
    assert_eq!(all.len(), 5);

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn stale_reservation_cleanup() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join(".filament");
    std::fs::create_dir_all(&runtime_dir).expect("create runtime dir");

    let db_path = runtime_dir.join("filament.db");
    let socket_path = runtime_dir.join("filament.sock");
    let pid_path = runtime_dir.join("filament.pid");

    let pool = init_pool(db_path.to_str().unwrap())
        .await
        .expect("init pool");
    drop(pool);

    let config = ServeConfig {
        socket_path: socket_path.clone(),
        db_path,
        pid_path,
        cleanup_interval_secs: 1, // very fast cleanup
    };

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        filament_daemon::serve(config, cancel_clone)
            .await
            .expect("daemon serve");
    });

    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut client = DaemonClient::connect(&socket_path).await.expect("connect");

    // Acquire a reservation with very short TTL (1 second)
    let _res_id = client
        .acquire_reservation("agent-cleanup", "test/*.rs", true, 1)
        .await
        .expect("acquire");

    // Wait for TTL to expire + cleanup interval to fire
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Now a different agent should be able to acquire the same glob
    let _res_id2 = client
        .acquire_reservation("agent-other", "test/*.rs", true, 300)
        .await
        .expect("acquire after cleanup");

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_request_returns_error() {
    let (client, cancel, tmp) = start_test_daemon().await;
    drop(client);

    let socket_path = tmp.path().join(".filament/filament.sock");

    // Send raw malformed JSON over the socket
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let stream = tokio::net::UnixStream::connect(&socket_path)
        .await
        .expect("connect");
    let (reader, mut writer) = stream.into_split();

    writer
        .write_all(b"this is not json\n")
        .await
        .expect("write");
    writer.flush().await.expect("flush");

    let mut lines = BufReader::new(reader).lines();
    let response_line = lines.next_line().await.expect("read").expect("line");

    let response: filament_core::protocol::Response =
        serde_json::from_str(&response_line).expect("parse response");
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, "PROTOCOL_ERROR");

    cancel.cancel();
}
