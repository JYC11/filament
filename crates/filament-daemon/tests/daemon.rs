use std::path::PathBuf;
use std::time::Duration;

use filament_core::client::DaemonClient;
use filament_core::models::{EntityStatus, EntityType};
use filament_core::schema::init_pool;
use filament_daemon::config::ServeConfig;
use tokio_util::sync::CancellationToken;

/// Helper: start a test daemon with a fresh DB in a temp dir.
/// Returns a `DaemonClient`, the cancel token, and the temp dir handle (for lifetime).
async fn start_test_daemon() -> (DaemonClient, CancellationToken, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join(".fl");
    std::fs::create_dir_all(&runtime_dir).expect("create runtime dir");

    let db_path = runtime_dir.join("fl.db");
    let socket_path = runtime_dir.join("fl.sock");
    let pid_path = runtime_dir.join("fl.pid");

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
        idle_timeout_secs: 0,        // no idle timeout in tests
        reconciliation_interval_secs: 3600,
    };

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        filament_daemon::serve(config, cancel_clone)
            .await
            .expect("daemon serve");
    });

    // Wait for daemon to accept connections (not just socket file to exist)
    let mut client = None;
    for _ in 0..100 {
        if socket_path.exists() {
            if let Ok(c) = DaemonClient::connect(&socket_path).await {
                client = Some(c);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let client = client.expect("connect to daemon");

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
    let (id, _slug) = client
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
    assert_eq!(entity.name(), "test-task");
    assert_eq!(entity.summary(), "A test task");

    // Update status
    client
        .update_entity_status(id.as_str(), EntityStatus::InProgress)
        .await
        .expect("update status");

    let entity = client.get_entity(id.as_str()).await.expect("get updated");
    assert_eq!(entity.status().as_str(), "in_progress");

    // List
    let entities = client
        .list_entities(Some(EntityType::Task), None)
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
async fn update_summary_refreshes_graph() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create two related entities
    let (id_a, _) = client
        .create_entity(serde_json::json!({
            "name": "graph-node-a",
            "entity_type": "module",
            "summary": "Original summary A",
        }))
        .await
        .expect("create a");

    let (id_b, _) = client
        .create_entity(serde_json::json!({
            "name": "graph-node-b",
            "entity_type": "module",
            "summary": "Original summary B",
        }))
        .await
        .expect("create b");

    client
        .create_relation(serde_json::json!({
            "source_id": id_a.as_str(),
            "target_id": id_b.as_str(),
            "relation_type": "relates_to",
        }))
        .await
        .expect("relate a to b");

    // Context query from B should show A's original summary as a neighbor
    let ctx = client
        .context_query(id_b.as_str(), Some(1))
        .await
        .expect("context before update");
    assert!(
        ctx.iter().any(|s| s.contains("Original summary A")),
        "should contain original summary A: {ctx:?}"
    );

    // Update summary of entity A
    client
        .update_entity_summary(id_a.as_str(), "Updated summary A")
        .await
        .expect("update summary");

    // Context query from B should now show A's updated summary
    let ctx_after = client
        .context_query(id_b.as_str(), Some(1))
        .await
        .expect("context after update");
    assert!(
        ctx_after.iter().any(|s| s.contains("Updated summary A")),
        "should contain updated summary A: {ctx_after:?}"
    );
    assert!(
        !ctx_after.iter().any(|s| s.contains("Original summary A")),
        "should NOT contain stale summary A: {ctx_after:?}"
    );

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn relation_crud_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    let (id_a, _) = client
        .create_entity(serde_json::json!({
            "name": "module-a",
            "entity_type": "module",
        }))
        .await
        .expect("create a");

    let (id_b, _) = client
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

    // Create agent entities so participant validation passes
    let (agent_a, _) = client
        .create_entity(serde_json::json!({
            "name": "agent-a",
            "entity_type": "agent",
            "summary": "Agent A",
        }))
        .await
        .expect("create agent-a");
    let (agent_b, _) = client
        .create_entity(serde_json::json!({
            "name": "agent-b",
            "entity_type": "agent",
            "summary": "Agent B",
        }))
        .await
        .expect("create agent-b");

    // Send a message
    let msg_id = client
        .send_message(serde_json::json!({
            "from_agent": agent_a.as_str(),
            "to_agent": agent_b.as_str(),
            "body": "Hello from the test",
            "msg_type": "text",
        }))
        .await
        .expect("send message");

    // Check inbox
    let inbox = client.get_inbox(agent_b.as_str()).await.expect("get inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].body, "Hello from the test");

    // Mark as read
    client
        .mark_message_read(msg_id.as_str())
        .await
        .expect("mark read");

    // Inbox should be empty now
    let inbox = client
        .get_inbox(agent_b.as_str())
        .await
        .expect("inbox after read");
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
        .release_reservation(res_id.as_str())
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
    let (task_a, _) = client
        .create_entity(serde_json::json!({
            "name": "task-a",
            "entity_type": "task",
            "summary": "First task",
            "priority": 1,
        }))
        .await
        .expect("create task-a");

    let (task_b, _) = client
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
    assert_eq!(ready[0].name(), "task-a");

    // Blocker depth from task-b (blocked by task-a, so depth >= 1)
    let depth = client
        .blocker_depth(task_b.as_str())
        .await
        .expect("blocker depth");
    assert!(depth >= 1);

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

    let socket_path: PathBuf = tmp.path().join(".fl/fl.sock");

    let mut handles = Vec::new();
    for i in 0..5 {
        let path = socket_path.clone();
        handles.push(tokio::spawn(async move {
            let mut c = DaemonClient::connect(&path).await.expect("connect");
            let name = format!("concurrent-entity-{i}");
            let (id, _) = c
                .create_entity(serde_json::json!({
                    "name": name,
                    "entity_type": "task",
                    "summary": format!("Created by client {i}"),
                }))
                .await
                .expect("create");
            let entity = c.get_entity(id.as_str()).await.expect("get");
            assert_eq!(entity.name(), name.as_str());
        }));
    }

    for h in handles {
        h.await.expect("join");
    }

    // Verify all 5 exist
    let mut verify_client = DaemonClient::connect(&socket_path).await.expect("connect");
    let all = verify_client
        .list_entities(Some(EntityType::Task), None)
        .await
        .expect("list");
    assert_eq!(all.len(), 5);

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn stale_reservation_cleanup() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join(".fl");
    std::fs::create_dir_all(&runtime_dir).expect("create runtime dir");

    let db_path = runtime_dir.join("fl.db");
    let socket_path = runtime_dir.join("fl.sock");
    let pid_path = runtime_dir.join("fl.pid");

    let pool = init_pool(db_path.to_str().unwrap())
        .await
        .expect("init pool");
    drop(pool);

    let config = ServeConfig {
        socket_path: socket_path.clone(),
        db_path,
        pid_path,
        cleanup_interval_secs: 1, // very fast cleanup
        idle_timeout_secs: 0,
        reconciliation_interval_secs: 3600,
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
async fn agent_run_operations_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create a task entity to associate the run with
    let (task_id, _) = client
        .create_entity(serde_json::json!({
            "name": "run-target-task",
            "entity_type": "task",
            "summary": "Task for agent run test",
        }))
        .await
        .expect("create task");

    // Create agent run
    let run_id = client
        .create_agent_run(task_id.as_str(), "code-reviewer", Some(12345))
        .await
        .expect("create agent run");

    // List running agents
    let running = client.list_running_agents().await.expect("list running");
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].agent_role, "code-reviewer");

    // Finish agent run
    client
        .finish_agent_run(run_id.as_str(), "completed", Some(r#"{"result":"ok"}"#))
        .await
        .expect("finish agent run");

    // List running agents — should be empty now
    let running = client
        .list_running_agents()
        .await
        .expect("list after finish");
    assert!(running.is_empty());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn list_all_agent_runs_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create two tasks with agent runs
    let (task_id_1, _) = client
        .create_entity(serde_json::json!({
            "name": "all-runs-task-1",
            "entity_type": "task",
            "summary": "First task",
        }))
        .await
        .expect("create task 1");

    let (task_id_2, _) = client
        .create_entity(serde_json::json!({
            "name": "all-runs-task-2",
            "entity_type": "task",
            "summary": "Second task",
        }))
        .await
        .expect("create task 2");

    let run_id_1 = client
        .create_agent_run(task_id_1.as_str(), "coder", Some(11111))
        .await
        .expect("create run 1");

    let run_id_2 = client
        .create_agent_run(task_id_2.as_str(), "reviewer", Some(22222))
        .await
        .expect("create run 2");

    // list_all_agent_runs should return both runs
    let all_runs = client
        .list_all_agent_runs(100)
        .await
        .expect("list all agent runs");
    assert_eq!(all_runs.len(), 2);

    let ids: Vec<_> = all_runs.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&run_id_1));
    assert!(ids.contains(&run_id_2));

    // list_agent_runs_by_task should return only the run for that task
    let task1_runs = client
        .list_agent_runs_by_task(task_id_1.as_str())
        .await
        .expect("list by task 1");
    assert_eq!(task1_runs.len(), 1);
    assert_eq!(task1_runs[0].id, run_id_1);

    // Finish both runs
    client
        .finish_agent_run(run_id_1.as_str(), "completed", None)
        .await
        .expect("finish run 1");
    client
        .finish_agent_run(run_id_2.as_str(), "completed", None)
        .await
        .expect("finish run 2");

    // list_all_agent_runs still returns finished runs (they're historical)
    let all_after = client
        .list_all_agent_runs(100)
        .await
        .expect("list all after finish");
    assert_eq!(all_after.len(), 2);

    // But list_running_agents returns empty
    let running = client.list_running_agents().await.expect("list running");
    assert!(running.is_empty());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn list_all_agent_runs_respects_limit() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create 3 tasks with agent runs
    let mut run_ids = Vec::new();
    for i in 0..3 {
        let (task_id, _) = client
            .create_entity(serde_json::json!({
                "name": format!("limit-task-{i}"),
                "entity_type": "task",
                "summary": format!("Task {i}"),
            }))
            .await
            .unwrap_or_else(|_| panic!("create task {i}"));

        let run_id = client
            .create_agent_run(task_id.as_str(), "coder", Some(30000 + i))
            .await
            .unwrap_or_else(|_| panic!("create run {i}"));

        client
            .finish_agent_run(run_id.as_str(), "completed", None)
            .await
            .unwrap_or_else(|_| panic!("finish run {i}"));

        run_ids.push(run_id);
    }

    // Limit = 2 should return only 2
    let limited = client
        .list_all_agent_runs(2)
        .await
        .expect("list all with limit 2");
    assert_eq!(limited.len(), 2);

    // Limit = 100 should return all 3
    let all = client
        .list_all_agent_runs(100)
        .await
        .expect("list all with limit 100");
    assert_eq!(all.len(), 3);

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn entity_events_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create entity — should generate an entity_created event
    let (id, _) = client
        .create_entity(serde_json::json!({
            "name": "event-test",
            "entity_type": "task",
            "summary": "Test events",
        }))
        .await
        .expect("create entity");

    // Get events — should have entity_created
    let events = client
        .get_entity_events(id.as_str())
        .await
        .expect("get events");
    assert_eq!(events.len(), 1, "expected 1 event after create");
    assert_eq!(events[0].event_type.as_str(), "entity_created");

    // Update status — should generate a status_change event
    client
        .update_entity_status(id.as_str(), EntityStatus::InProgress)
        .await
        .expect("update status");

    let events = client
        .get_entity_events(id.as_str())
        .await
        .expect("get events after status update");
    assert_eq!(events.len(), 2, "expected 2 events after status change");
    assert_eq!(events[1].event_type.as_str(), "status_change");

    // Verify get_entity_events for nonexistent entity returns empty
    let events = client
        .get_entity_events("nonexistent-id")
        .await
        .expect("get events for missing entity");
    assert!(events.is_empty());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn update_entity_status_invalid_returns_error() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    let (id, _) = client
        .create_entity(serde_json::json!({
            "name": "status-test",
            "entity_type": "task",
            "summary": "Test invalid status",
        }))
        .await
        .expect("create entity");

    // Try invalid status — parse should fail before reaching the client
    let result: std::result::Result<EntityStatus, _> = "totally_bogus_status".parse();
    assert!(result.is_err(), "expected error for invalid status");

    // Entity should still have original status
    let entity = client.get_entity(id.as_str()).await.expect("get entity");
    assert_eq!(entity.status().as_str(), "open");

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_relation_invalid_type_returns_error() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    let (id_a, _) = client
        .create_entity(serde_json::json!({
            "name": "rel-err-a",
            "entity_type": "module",
        }))
        .await
        .expect("create a");

    let (id_b, _) = client
        .create_entity(serde_json::json!({
            "name": "rel-err-b",
            "entity_type": "module",
        }))
        .await
        .expect("create b");

    // Try to delete a relation with an invalid relation_type
    let result = client
        .delete_relation(id_a.as_str(), id_b.as_str(), "not_a_real_type")
        .await;
    assert!(result.is_err(), "expected error for invalid relation type");

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn get_entity_by_slug_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create entity
    let (id, slug) = client
        .create_entity(serde_json::json!({
            "name": "slug-lookup",
            "entity_type": "module",
            "summary": "Test get by slug",
        }))
        .await
        .expect("create entity");

    // Look up by slug
    let entity = client
        .get_entity_by_slug(slug.as_str())
        .await
        .expect("get by slug");
    assert_eq!(entity.id(), &id);
    assert_eq!(entity.name(), "slug-lookup");

    // Nonexistent slug should error
    let result = client.get_entity_by_slug("zz999999").await;
    assert!(result.is_err());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn update_entity_summary_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    let (id, _) = client
        .create_entity(serde_json::json!({
            "name": "summary-test",
            "entity_type": "task",
            "summary": "Original summary",
        }))
        .await
        .expect("create entity");

    // Update summary
    client
        .update_entity_summary(id.as_str(), "Updated summary text")
        .await
        .expect("update summary");

    // Verify
    let entity = client.get_entity(id.as_str()).await.expect("get entity");
    assert_eq!(entity.summary(), "Updated summary text");

    // Update summary of nonexistent entity should error
    let result = client.update_entity_summary("nonexistent-id", "nope").await;
    assert!(result.is_err());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn find_reservation_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Acquire a reservation
    let res_id = client
        .acquire_reservation("agent-find", "src/*.rs", false, 300)
        .await
        .expect("acquire");

    // Find by glob + agent
    let found = client
        .find_reservation("src/*.rs", "agent-find")
        .await
        .expect("find reservation");
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, res_id);
    assert_eq!(found.agent_name, "agent-find");
    assert_eq!(found.file_glob, "src/*.rs");

    // Find with wrong agent returns None
    let not_found = client
        .find_reservation("src/*.rs", "other-agent")
        .await
        .expect("find other");
    assert!(not_found.is_none());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn list_reservations_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Acquire several reservations
    client
        .acquire_reservation("agent-a", "foo/*.rs", false, 300)
        .await
        .expect("acquire 1");
    client
        .acquire_reservation("agent-b", "bar/*.rs", true, 300)
        .await
        .expect("acquire 2");
    client
        .acquire_reservation("agent-a", "baz/*.rs", false, 300)
        .await
        .expect("acquire 3");

    // List all
    let all = client.list_reservations(None).await.expect("list all");
    assert_eq!(all.len(), 3);

    // List filtered by agent
    let agent_a = client
        .list_reservations(Some("agent-a"))
        .await
        .expect("list agent-a");
    assert_eq!(agent_a.len(), 2);
    assert!(agent_a.iter().all(|r| r.agent_name == "agent-a"));

    let agent_b = client
        .list_reservations(Some("agent-b"))
        .await
        .expect("list agent-b");
    assert_eq!(agent_b.len(), 1);
    assert_eq!(agent_b[0].agent_name, "agent-b");
    assert!(agent_b[0].mode.is_exclusive());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_agent_task_scheduling() {
    let (client, cancel, tmp) = start_test_daemon().await;
    drop(client);

    let socket_path: PathBuf = tmp.path().join(".fl/fl.sock");

    // Setup: create task graph
    let mut setup = DaemonClient::connect(&socket_path).await.expect("connect");

    let (core_refactor, _) = setup
        .create_entity(serde_json::json!({
            "name": "core-refactor",
            "entity_type": "task",
            "summary": "Refactor core module",
            "priority": 1,
        }))
        .await
        .expect("create core-refactor");

    let (write_tests, _) = setup
        .create_entity(serde_json::json!({
            "name": "write-tests",
            "entity_type": "task",
            "summary": "Write tests for refactored code",
            "priority": 2,
        }))
        .await
        .expect("create write-tests");

    let (code_review, _) = setup
        .create_entity(serde_json::json!({
            "name": "code-review",
            "entity_type": "task",
            "summary": "Review the refactored code",
            "priority": 2,
        }))
        .await
        .expect("create code-review");

    // write-tests depends_on core-refactor
    setup
        .create_relation(serde_json::json!({
            "source_id": write_tests.as_str(),
            "target_id": core_refactor.as_str(),
            "relation_type": "depends_on",
        }))
        .await
        .expect("write-tests depends_on core-refactor");

    // code-review depends_on core-refactor
    setup
        .create_relation(serde_json::json!({
            "source_id": code_review.as_str(),
            "target_id": core_refactor.as_str(),
            "relation_type": "depends_on",
        }))
        .await
        .expect("code-review depends_on core-refactor");

    // Verify: only core-refactor is ready
    let ready = setup.ready_tasks().await.expect("ready tasks initial");
    assert_eq!(ready.len(), 1, "only core-refactor should be ready");
    assert_eq!(ready[0].name(), "core-refactor");

    // Agent A: claims core-refactor, marks in_progress, then closed
    let sp = socket_path.clone();
    let cr_id = core_refactor.clone();
    let agent_a = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp).await.expect("agent-a connect");
        c.update_entity_status(cr_id.as_str(), EntityStatus::InProgress)
            .await
            .expect("agent-a in_progress");
        c.update_entity_status(cr_id.as_str(), EntityStatus::Closed)
            .await
            .expect("agent-a closed");
    });

    agent_a.await.expect("agent-a join");

    // After Agent A finishes: Agent B and Agent C concurrently query ready_tasks
    // Verify both tasks are now ready before concurrent agents claim them
    let mut pre_check = DaemonClient::connect(&socket_path)
        .await
        .expect("pre-check connect");
    let ready = pre_check
        .ready_tasks()
        .await
        .expect("pre-check ready tasks");
    assert_eq!(
        ready.len(),
        2,
        "both write-tests and code-review should be ready"
    );
    let ready_names: Vec<&str> = ready.iter().map(|e| e.name().as_str()).collect();
    assert!(
        ready_names.contains(&"write-tests"),
        "write-tests should be ready"
    );
    assert!(
        ready_names.contains(&"code-review"),
        "code-review should be ready"
    );

    // Agent B and C concurrently claim their respective tasks
    let sp_b = socket_path.clone();
    let wt_id = write_tests.clone();
    let agent_b = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_b).await.expect("agent-b connect");
        c.update_entity_status(wt_id.as_str(), EntityStatus::InProgress)
            .await
            .expect("agent-b in_progress");
        c.update_entity_status(wt_id.as_str(), EntityStatus::Closed)
            .await
            .expect("agent-b closed");
    });

    let sp_c = socket_path.clone();
    let cr_rev_id = code_review.clone();
    let agent_c = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_c).await.expect("agent-c connect");
        c.update_entity_status(cr_rev_id.as_str(), EntityStatus::InProgress)
            .await
            .expect("agent-c in_progress");
        c.update_entity_status(cr_rev_id.as_str(), EntityStatus::Closed)
            .await
            .expect("agent-c closed");
    });

    agent_b.await.expect("agent-b join");
    agent_c.await.expect("agent-c join");

    // Final: verify all 3 tasks are closed
    let mut verify = DaemonClient::connect(&socket_path)
        .await
        .expect("verify connect");
    let all_tasks = verify
        .list_entities(Some(EntityType::Task), Some(EntityStatus::Closed))
        .await
        .expect("list closed tasks");
    assert_eq!(all_tasks.len(), 3, "all 3 tasks should be closed");

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_agent_reservation_conflicts() {
    let (client, cancel, tmp) = start_test_daemon().await;
    drop(client);

    let socket_path: PathBuf = tmp.path().join(".fl/fl.sock");

    // Agent A: acquires exclusive reservation on src/**/*.rs
    let sp_a = socket_path.clone();
    let agent_a_handle = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_a).await.expect("agent-a connect");
        let res_id = c
            .acquire_reservation("agent-a", "src/**/*.rs", true, 300)
            .await
            .expect("agent-a acquire exclusive");
        res_id
    });
    let res_id_a = agent_a_handle.await.expect("agent-a join");

    // Agent B (concurrent): tries the same glob — expects error
    let sp_b = socket_path.clone();
    let agent_b_conflict = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_b).await.expect("agent-b connect");
        let result = c
            .acquire_reservation("agent-b", "src/**/*.rs", true, 300)
            .await;
        assert!(
            result.is_err(),
            "agent-b should fail on conflicting exclusive reservation"
        );
    });

    // Agent C (concurrent): acquires non-overlapping glob — succeeds
    let sp_c = socket_path.clone();
    let agent_c_handle = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_c).await.expect("agent-c connect");
        let res_id = c
            .acquire_reservation("agent-c", "docs/**/*.md", true, 300)
            .await
            .expect("agent-c acquire non-overlapping should succeed");
        res_id
    });

    agent_b_conflict.await.expect("agent-b join");
    let _res_id_c = agent_c_handle.await.expect("agent-c join");

    // Agent A: releases reservation
    let mut release_client = DaemonClient::connect(&socket_path)
        .await
        .expect("release connect");
    release_client
        .release_reservation(res_id_a.as_str())
        .await
        .expect("agent-a release");

    // Agent B retries: now succeeds
    let sp_b2 = socket_path.clone();
    let agent_b_retry = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_b2)
            .await
            .expect("agent-b retry connect");
        let _res_id = c
            .acquire_reservation("agent-b", "src/**/*.rs", true, 300)
            .await
            .expect("agent-b retry should succeed after release");
    });
    agent_b_retry.await.expect("agent-b retry join");

    // Final: list_reservations shows 2 active (agent-b + agent-c)
    let mut verify = DaemonClient::connect(&socket_path)
        .await
        .expect("verify connect");
    let all_res = verify
        .list_reservations(None)
        .await
        .expect("list all reservations");
    assert_eq!(
        all_res.len(),
        2,
        "should have 2 active reservations (agent-b + agent-c)"
    );

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_agent_messaging_workflow() {
    let (client, cancel, tmp) = start_test_daemon().await;
    drop(client);

    let socket_path: PathBuf = tmp.path().join(".fl/fl.sock");

    // Create agents and a task to link messages to
    let mut setup = DaemonClient::connect(&socket_path)
        .await
        .expect("setup connect");
    let (writer_slug, _) = setup
        .create_entity(serde_json::json!({
            "name": "code-writer",
            "entity_type": "agent",
            "summary": "Code writer agent",
        }))
        .await
        .expect("create code-writer");
    let (reviewer_slug, _) = setup
        .create_entity(serde_json::json!({
            "name": "code-reviewer",
            "entity_type": "agent",
            "summary": "Code reviewer agent",
        }))
        .await
        .expect("create code-reviewer");
    let (task_id, _) = setup
        .create_entity(serde_json::json!({
            "name": "review-task",
            "entity_type": "task",
            "summary": "Task for messaging test",
        }))
        .await
        .expect("create task");

    // Agent A ("code-writer"): sends a question to "code-reviewer"
    let sp_a = socket_path.clone();
    let tid = task_id.clone();
    let ws = writer_slug.clone();
    let rs = reviewer_slug.clone();
    let agent_a_send = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_a).await.expect("agent-a connect");
        let msg_id = c
            .send_message(serde_json::json!({
                "from_agent": ws.as_str(),
                "to_agent": rs.as_str(),
                "body": "Is the error handling correct in module X?",
                "msg_type": "question",
                "task_id": tid.as_str(),
            }))
            .await
            .expect("agent-a send question");
        msg_id
    });
    let question_msg_id = agent_a_send.await.expect("agent-a send join");

    // Agent B ("code-reviewer"): checks inbox, finds the question, marks read, sends reply
    let sp_b = socket_path.clone();
    let tid2 = task_id.clone();
    let ws2 = writer_slug.clone();
    let rs2 = reviewer_slug.clone();
    let agent_b_reply = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_b).await.expect("agent-b connect");

        // Check inbox
        let inbox = c.get_inbox(rs2.as_str()).await.expect("agent-b inbox");
        assert_eq!(inbox.len(), 1, "code-reviewer should have 1 message");
        assert_eq!(inbox[0].body, "Is the error handling correct in module X?");
        assert_eq!(inbox[0].msg_type.as_str(), "question");

        // Mark as read
        c.mark_message_read(inbox[0].id.as_str())
            .await
            .expect("agent-b mark read");

        // Verify inbox is now empty
        let inbox_after = c
            .get_inbox(rs2.as_str())
            .await
            .expect("agent-b inbox after read");
        assert!(
            inbox_after.is_empty(),
            "inbox should be empty after mark_read"
        );

        // Send reply
        let reply_id = c
            .send_message(serde_json::json!({
                "from_agent": rs2.as_str(),
                "to_agent": ws2.as_str(),
                "body": "Yes, looks good. Approved.",
                "msg_type": "text",
                "task_id": tid2.as_str(),
            }))
            .await
            .expect("agent-b send reply");
        reply_id
    });
    let _reply_msg_id = agent_b_reply.await.expect("agent-b reply join");

    // Agent A: checks inbox, finds the reply
    let sp_a2 = socket_path.clone();
    let rs3 = reviewer_slug.clone();
    let ws3 = writer_slug.clone();
    let agent_a_read = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_a2)
            .await
            .expect("agent-a read connect");
        let inbox = c.get_inbox(ws3.as_str()).await.expect("agent-a inbox");
        assert_eq!(inbox.len(), 1, "code-writer should have 1 reply");
        assert_eq!(inbox[0].body, "Yes, looks good. Approved.");
        assert_eq!(inbox[0].from_agent, rs3.as_str());
    });
    agent_a_read.await.expect("agent-a read join");

    // Agent C ("observer"): checks inbox — should be empty
    let sp_c = socket_path.clone();
    let agent_c_check = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp_c).await.expect("agent-c connect");
        let inbox = c.get_inbox("observer").await.expect("observer inbox");
        assert!(
            inbox.is_empty(),
            "observer should have no messages (targeted messaging)"
        );
    });
    agent_c_check.await.expect("agent-c check join");

    // Drop the unused variable
    let _ = question_msg_id;

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_agent_full_workflow() {
    let (client, cancel, tmp) = start_test_daemon().await;
    drop(client);

    let socket_path: PathBuf = tmp.path().join(".fl/fl.sock");

    // Setup: create linear task chain: design → implement → test → review
    let mut setup = DaemonClient::connect(&socket_path)
        .await
        .expect("setup connect");

    let (design, _) = setup
        .create_entity(serde_json::json!({
            "name": "design",
            "entity_type": "task",
            "summary": "Design the feature",
            "priority": 1,
        }))
        .await
        .expect("create design");

    let (implement, _) = setup
        .create_entity(serde_json::json!({
            "name": "implement",
            "entity_type": "task",
            "summary": "Implement the feature",
            "priority": 2,
        }))
        .await
        .expect("create implement");

    let (test, _) = setup
        .create_entity(serde_json::json!({
            "name": "test",
            "entity_type": "task",
            "summary": "Test the implementation",
            "priority": 3,
        }))
        .await
        .expect("create test");

    let (review, _) = setup
        .create_entity(serde_json::json!({
            "name": "review",
            "entity_type": "task",
            "summary": "Review everything",
            "priority": 4,
        }))
        .await
        .expect("create review");

    // Create agent entities for messaging validation
    let (designer_slug, _) = setup
        .create_entity(serde_json::json!({
            "name": "designer",
            "entity_type": "agent",
            "summary": "Designer agent",
        }))
        .await
        .expect("create designer agent");
    let (implementer_slug, _) = setup
        .create_entity(serde_json::json!({
            "name": "implementer",
            "entity_type": "agent",
            "summary": "Implementer agent",
        }))
        .await
        .expect("create implementer agent");
    let (tester_slug, _) = setup
        .create_entity(serde_json::json!({
            "name": "tester",
            "entity_type": "agent",
            "summary": "Tester agent",
        }))
        .await
        .expect("create tester agent");

    // implement depends_on design
    setup
        .create_relation(serde_json::json!({
            "source_id": implement.as_str(),
            "target_id": design.as_str(),
            "relation_type": "depends_on",
        }))
        .await
        .expect("implement depends_on design");

    // test depends_on implement
    setup
        .create_relation(serde_json::json!({
            "source_id": test.as_str(),
            "target_id": implement.as_str(),
            "relation_type": "depends_on",
        }))
        .await
        .expect("test depends_on implement");

    // review depends_on test
    setup
        .create_relation(serde_json::json!({
            "source_id": review.as_str(),
            "target_id": test.as_str(),
            "relation_type": "depends_on",
        }))
        .await
        .expect("review depends_on test");

    // Phase 1: Only design is ready. Agent A claims it.
    let ready = setup.ready_tasks().await.expect("ready tasks phase 1");
    assert_eq!(ready.len(), 1, "only design should be ready");
    assert_eq!(ready[0].name(), "design");

    let sp = socket_path.clone();
    let design_id = design.clone();
    let implement_id = implement.clone();
    let ds = designer_slug.clone();
    let is = implementer_slug.clone();
    let phase1 = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp).await.expect("agent-a connect");

        // Create agent run
        let run_id = c
            .create_agent_run(design_id.as_str(), "designer", Some(1001))
            .await
            .expect("create agent run for design");

        // Set in_progress
        c.update_entity_status(design_id.as_str(), EntityStatus::InProgress)
            .await
            .expect("design in_progress");

        // Acquire reservation on docs
        let res_id = c
            .acquire_reservation("designer", "docs/**", true, 300)
            .await
            .expect("designer acquire docs");

        // "Do work" — then finish
        c.finish_agent_run(run_id.as_str(), "completed", Some(r#"{"design":"done"}"#))
            .await
            .expect("finish design run");

        c.update_entity_status(design_id.as_str(), EntityStatus::Closed)
            .await
            .expect("design closed");

        c.release_reservation(res_id.as_str())
            .await
            .expect("designer release");

        // Send message to implementer
        c.send_message(serde_json::json!({
            "from_agent": ds.as_str(),
            "to_agent": is.as_str(),
            "body": "Design is done, you can start implementing",
            "msg_type": "text",
            "task_id": implement_id.as_str(),
        }))
        .await
        .expect("designer send message");
    });
    phase1.await.expect("phase 1 join");

    // Phase 2: implement is now ready. Agent B reads inbox and claims it.
    let sp2 = socket_path.clone();
    let impl_id = implement.clone();
    let test_id = test.clone();
    let ds2 = designer_slug.clone();
    let is2 = implementer_slug.clone();
    let ts2 = tester_slug.clone();
    let phase2 = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp2).await.expect("agent-b connect");

        // Read inbox
        let inbox = c.get_inbox(is2.as_str()).await.expect("implementer inbox");
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].from_agent, ds2.as_str());
        c.mark_message_read(inbox[0].id.as_str())
            .await
            .expect("mark read");

        // Check ready tasks
        let ready = c.ready_tasks().await.expect("ready tasks phase 2");
        assert_eq!(ready.len(), 1, "only implement should be ready");
        assert_eq!(ready[0].name(), "implement");

        // Create run, claim task, acquire reservation
        let run_id = c
            .create_agent_run(impl_id.as_str(), "implementer", Some(1002))
            .await
            .expect("create implement run");
        c.update_entity_status(impl_id.as_str(), EntityStatus::InProgress)
            .await
            .expect("implement in_progress");
        let res_id = c
            .acquire_reservation("implementer", "src/**", true, 300)
            .await
            .expect("implementer acquire src");

        // Finish
        c.finish_agent_run(run_id.as_str(), "completed", Some(r#"{"impl":"done"}"#))
            .await
            .expect("finish implement run");
        c.update_entity_status(impl_id.as_str(), EntityStatus::Closed)
            .await
            .expect("implement closed");
        c.release_reservation(res_id.as_str())
            .await
            .expect("implementer release");

        // Send message to tester
        c.send_message(serde_json::json!({
            "from_agent": is2.as_str(),
            "to_agent": ts2.as_str(),
            "body": "Implementation done, please test",
            "msg_type": "text",
            "task_id": test_id.as_str(),
        }))
        .await
        .expect("implementer send message");
    });
    phase2.await.expect("phase 2 join");

    // Phase 3: Agent C picks up test, then review (sequential)
    let sp3 = socket_path.clone();
    let tst_id = test.clone();
    let rev_id = review.clone();
    let phase3 = tokio::spawn(async move {
        let mut c = DaemonClient::connect(&sp3).await.expect("agent-c connect");

        // Test task
        let ready = c.ready_tasks().await.expect("ready tasks phase 3a");
        assert_eq!(ready.len(), 1, "only test should be ready");
        assert_eq!(ready[0].name(), "test");

        let run_id = c
            .create_agent_run(tst_id.as_str(), "tester", Some(1003))
            .await
            .expect("create test run");
        c.update_entity_status(tst_id.as_str(), EntityStatus::InProgress)
            .await
            .expect("test in_progress");
        c.finish_agent_run(run_id.as_str(), "completed", Some(r#"{"tests":"pass"}"#))
            .await
            .expect("finish test run");
        c.update_entity_status(tst_id.as_str(), EntityStatus::Closed)
            .await
            .expect("test closed");

        // Review task
        let ready = c.ready_tasks().await.expect("ready tasks phase 3b");
        assert_eq!(ready.len(), 1, "only review should be ready");
        assert_eq!(ready[0].name(), "review");

        let run_id = c
            .create_agent_run(rev_id.as_str(), "reviewer", Some(1004))
            .await
            .expect("create review run");
        c.update_entity_status(rev_id.as_str(), EntityStatus::InProgress)
            .await
            .expect("review in_progress");
        c.finish_agent_run(
            run_id.as_str(),
            "completed",
            Some(r#"{"review":"approved"}"#),
        )
        .await
        .expect("finish review run");
        c.update_entity_status(rev_id.as_str(), EntityStatus::Closed)
            .await
            .expect("review closed");
    });
    phase3.await.expect("phase 3 join");

    // Final assertions
    let mut verify = DaemonClient::connect(&socket_path)
        .await
        .expect("verify connect");

    // All 4 tasks closed
    let closed = verify
        .list_entities(Some(EntityType::Task), Some(EntityStatus::Closed))
        .await
        .expect("list closed");
    assert_eq!(closed.len(), 4, "all 4 tasks should be closed");

    // No running agents
    let running = verify.list_running_agents().await.expect("list running");
    assert!(running.is_empty(), "no agents should be running");

    // Ready tasks should be empty (all closed)
    let ready = verify.ready_tasks().await.expect("final ready tasks");
    assert!(ready.is_empty(), "no tasks should be ready (all closed)");

    // Blocker depth from review — all tasks are closed, so depth is 0
    let depth = verify
        .blocker_depth(review.as_str())
        .await
        .expect("blocker depth");
    assert_eq!(
        depth, 0,
        "all upstream blockers are closed, depth should be 0"
    );

    // Events were recorded for design entity
    let events = verify
        .get_entity_events(design.as_str())
        .await
        .expect("design events");
    assert!(
        events.len() >= 2,
        "design should have at least entity_created + status_change events"
    );

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_request_returns_error() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (client, cancel, tmp) = start_test_daemon().await;
    drop(client);

    let socket_path = tmp.path().join(".fl/fl.sock");

    // Send raw malformed JSON over the socket
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

#[tokio::test(flavor = "multi_thread")]
async fn subscribe_receives_entity_notifications() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    // Create a second client for subscribing
    let socket_path = _tmp.path().join(".fl").join("fl.sock");
    let mut sub_client = DaemonClient::connect(&socket_path).await.unwrap();

    let mut stream = sub_client
        .subscribe(filament_core::protocol::SubscribeParams::default())
        .await
        .unwrap();

    // Give subscriber a moment to be ready
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create an entity via the regular client
    let params = serde_json::json!({
        "name": "watch-test",
        "entity_type": "task",
        "summary": "testing notifications",
        "priority": 2,
    });
    client.create_entity(params).await.unwrap();

    // Read the notification with a timeout
    let notification = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout waiting for notification")
        .expect("read notification")
        .expect("notification should not be None");

    assert_eq!(notification.event_type, "entity_created");
    assert!(notification.entity_id.is_some());

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn subscribe_with_filter_only_receives_matching() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

    let socket_path = _tmp.path().join(".fl").join("fl.sock");
    let mut sub_client = DaemonClient::connect(&socket_path).await.unwrap();

    // Subscribe only to status_change events
    let mut stream = sub_client
        .subscribe(filament_core::protocol::SubscribeParams {
            event_types: vec!["status_change".to_string()],
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create entity (should NOT trigger for this subscriber)
    let params = serde_json::json!({
        "name": "filter-test",
        "entity_type": "task",
        "summary": "testing filter",
        "priority": 2,
    });
    let (id, _slug) = client.create_entity(params).await.unwrap();

    // Update status (SHOULD trigger)
    client
        .update_entity_status(id.as_str(), EntityStatus::InProgress)
        .await
        .unwrap();

    // Read notification — should be the status_change, not entity_created
    let notification = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout")
        .expect("read")
        .expect("not None");

    assert_eq!(notification.event_type, "status_change");

    cancel.cancel();
}
