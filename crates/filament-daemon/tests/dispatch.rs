use std::time::Duration;

use filament_core::client::DaemonClient;
use filament_core::models::EntityStatus;
use filament_core::schema::init_pool;
use filament_daemon::config::ServeConfig;
use filament_daemon::dispatch::DispatchConfig;
use tokio_util::sync::CancellationToken;

/// Mock agent configuration for per-test isolation.
struct MockConfig {
    status: &'static str,
    summary: &'static str,
    exit_code: i32,
    delay_ms: u32,
    messages: &'static str,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            status: "completed",
            summary: "mock agent done",
            exit_code: 0,
            delay_ms: 0,
            messages: "[]",
        }
    }
}

/// Write a per-test mock agent script with hardcoded behavior.
/// Returns the path to the script. No env vars needed — avoids cross-test contamination.
fn write_mock_script(dir: &std::path::Path, config: &MockConfig) -> std::path::PathBuf {
    let script_path = dir.join("mock-agent.sh");
    let content = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
# Per-test mock agent with hardcoded behavior
{delay}
cat <<'ENDJSON'
{{"status":"{status}","summary":"{summary}","artifacts":[],"messages":{messages},"blockers":[],"questions":[]}}
ENDJSON
exit {exit_code}
"#,
        delay = if config.delay_ms > 0 {
            format!("sleep $(echo 'scale=3; {}/1000' | bc)", config.delay_ms)
        } else {
            String::new()
        },
        status = config.status,
        summary = config.summary,
        messages = config.messages,
        exit_code = config.exit_code,
    );
    std::fs::write(&script_path, content).expect("write mock script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod mock script");
    }
    script_path
}

/// Helper: start a test daemon with a per-test mock agent script.
/// Uses `serve_with_dispatch` to pass the mock command directly — no env vars needed.
async fn start_test_daemon(
    mock: &MockConfig,
) -> (DaemonClient, CancellationToken, tempfile::TempDir) {
    start_test_daemon_with_timeout(mock, 0).await
}

/// Like `start_test_daemon` but with a configurable agent timeout.
async fn start_test_daemon_with_timeout(
    mock: &MockConfig,
    agent_timeout_secs: u64,
) -> (DaemonClient, CancellationToken, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join(".fl");
    std::fs::create_dir_all(&runtime_dir).expect("create runtime dir");

    // Write per-test mock script — no MOCK_AGENT_* env vars needed
    let mock_script = write_mock_script(tmp.path(), mock);

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
        cleanup_interval_secs: 3600,
        idle_timeout_secs: 0,
        reconciliation_interval_secs: 3600, // long interval for tests
    };

    // Pass dispatch config directly — no env var contamination
    let dispatch_config = DispatchConfig {
        agent_command: mock_script.to_str().unwrap().to_string(),
        project_root: tmp.path().to_path_buf(),
        context_depth: 2,
        auto_dispatch: false,
        max_auto_dispatch: 3,
        agent_timeout_secs,
    };

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        filament_daemon::serve_with_dispatch(config, cancel_clone, Some(dispatch_config))
            .await
            .expect("daemon serve");
    });

    // Wait for socket
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

/// Create a task via the daemon client, return (entity_id, slug).
async fn create_test_task(client: &mut DaemonClient, name: &str) -> (String, String) {
    let (id, slug) = client
        .create_entity(serde_json::json!({
            "name": name,
            "entity_type": "task",
            "summary": format!("Test task: {name}"),
            "priority": 2
        }))
        .await
        .expect("create task");
    (id.to_string(), slug.to_string())
}

/// Wait for an agent run to reach a terminal state, with timeout.
async fn wait_for_run_completion(
    client: &mut DaemonClient,
    run_id: &str,
    timeout_secs: u64,
) -> String {
    let start = std::time::Instant::now();
    loop {
        let run = client.get_agent_run(run_id).await.expect("get agent run");
        if run.status.as_str() != "running" {
            return run.status.as_str().to_string();
        }
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            panic!("timed out waiting for run {run_id} to complete");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

// ---------------------------------------------------------------------------
// Tests — each test gets its own daemon + mock script, no shared state
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn dispatch_agent_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon(&MockConfig::default()).await;

    let (_task_id, slug) = create_test_task(&mut client, "dispatch-test").await;

    let run_id = client
        .dispatch_agent(&slug, "coder")
        .await
        .expect("dispatch agent");

    let status = wait_for_run_completion(&mut client, run_id.as_str(), 10).await;
    assert_eq!(status, "completed");

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn dispatch_closed_task_fails() {
    let (mut client, cancel, _tmp) = start_test_daemon(&MockConfig::default()).await;

    let (task_id, slug) = create_test_task(&mut client, "closed-task").await;

    client
        .update_entity_status(&task_id, EntityStatus::Closed)
        .await
        .expect("close task");

    let err = client
        .dispatch_agent(&slug, "coder")
        .await
        .expect_err("dispatch to closed task should fail");

    assert!(
        err.to_string().contains("AGENT_DISPATCH_FAILED"),
        "expected AGENT_DISPATCH_FAILED, got: {err}"
    );

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn get_agent_run_and_list_by_task() {
    let (mut client, cancel, _tmp) = start_test_daemon(&MockConfig::default()).await;

    let (task_id, slug) = create_test_task(&mut client, "history-test").await;

    let run_id = client
        .dispatch_agent(&slug, "coder")
        .await
        .expect("dispatch agent");

    let _ = wait_for_run_completion(&mut client, run_id.as_str(), 10).await;

    // Get single run
    let run = client
        .get_agent_run(run_id.as_str())
        .await
        .expect("get agent run");
    assert_eq!(run.task_id.as_str(), task_id);

    // List runs by task
    let runs = client
        .list_agent_runs_by_task(&task_id)
        .await
        .expect("list runs by task");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, run_id);

    // list_all_agent_runs should also include this run
    let all_runs = client
        .list_all_agent_runs(100)
        .await
        .expect("list all agent runs");
    assert!(
        all_runs.iter().any(|r| r.id == run_id),
        "list_all_agent_runs should include the dispatched run"
    );

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// Previously removed tests — now re-added with per-test mock isolation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn dispatch_agent_death() {
    // Mock agent exits with non-zero code (death scenario)
    let mock = MockConfig {
        exit_code: 1,
        ..MockConfig::default()
    };
    let (mut client, cancel, _tmp) = start_test_daemon(&mock).await;

    let (task_id, slug) = create_test_task(&mut client, "death-test").await;

    let run_id = client
        .dispatch_agent(&slug, "coder")
        .await
        .expect("dispatch agent");

    let status = wait_for_run_completion(&mut client, run_id.as_str(), 10).await;
    assert_eq!(status, "failed");

    // Task should be reverted to open after death cleanup
    let task = client.get_entity(&task_id).await.expect("get task");
    assert_eq!(
        task.status(),
        &EntityStatus::Open,
        "task should revert to open after agent death"
    );

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn dispatch_agent_already_running() {
    // Slow mock agent to keep it "running" during second dispatch attempt
    let mock = MockConfig {
        delay_ms: 3000,
        ..MockConfig::default()
    };
    let (mut client, cancel, _tmp) = start_test_daemon(&mock).await;

    let (_task_id, slug) = create_test_task(&mut client, "already-running").await;

    // First dispatch succeeds
    let _run_id = client
        .dispatch_agent(&slug, "coder")
        .await
        .expect("first dispatch");

    // Small delay to let the agent run record be created
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Second dispatch should fail with AGENT_ALREADY_RUNNING
    let err = client
        .dispatch_agent(&slug, "coder")
        .await
        .expect_err("second dispatch should fail");

    assert!(
        err.to_string().contains("AGENT_ALREADY_RUNNING"),
        "expected AGENT_ALREADY_RUNNING, got: {err}"
    );

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn dispatch_result_routes_messages() {
    // Mock agent produces a message in its result
    let mock = MockConfig {
        messages: r#"[{"to_agent":"reviewer","body":"review needed","msg_type":"question"}]"#,
        ..MockConfig::default()
    };
    let (mut client, cancel, _tmp) = start_test_daemon(&mock).await;

    let (_task_id, slug) = create_test_task(&mut client, "msg-routing-test").await;

    let run_id = client
        .dispatch_agent(&slug, "coder")
        .await
        .expect("dispatch agent");

    let status = wait_for_run_completion(&mut client, run_id.as_str(), 10).await;
    assert_eq!(status, "completed");

    // Check that the message was routed to the reviewer's inbox
    let inbox = client.get_inbox("reviewer").await.expect("get inbox");
    assert!(
        !inbox.is_empty(),
        "reviewer should have received the routed message"
    );
    assert!(inbox[0].body.as_str().contains("review needed"));

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn sequential_multi_dispatch() {
    let mock = MockConfig {
        delay_ms: 500,
        ..MockConfig::default()
    };
    let (mut client, cancel, _tmp) = start_test_daemon(&mock).await;

    // Create multiple tasks
    let (id1, slug1) = create_test_task(&mut client, "multi-task-1").await;
    let (id2, slug2) = create_test_task(&mut client, "multi-task-2").await;

    // Dispatch individually, waiting for each to complete before next dispatch
    let run_id1 = client
        .dispatch_agent(&slug1, "coder")
        .await
        .expect("dispatch 1");
    let status1 = wait_for_run_completion(&mut client, run_id1.as_str(), 10).await;
    assert_eq!(status1, "completed", "first agent should complete");

    let run_id2 = client
        .dispatch_agent(&slug2, "coder")
        .await
        .expect("dispatch 2");
    let status2 = wait_for_run_completion(&mut client, run_id2.as_str(), 10).await;
    assert_eq!(status2, "completed", "second agent should complete");

    // Verify both runs exist
    let runs1 = client
        .list_agent_runs_by_task(&id1)
        .await
        .expect("list runs 1");
    let runs2 = client
        .list_agent_runs_by_task(&id2)
        .await
        .expect("list runs 2");
    assert_eq!(runs1.len(), 1);
    assert_eq!(runs2.len(), 1);

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn dispatch_agent_timeout_kills_long_running() {
    // Mock agent sleeps for 60s — will be killed by 2s timeout
    let mock = MockConfig {
        delay_ms: 60_000,
        ..MockConfig::default()
    };
    let (mut client, cancel, _tmp) = start_test_daemon_with_timeout(&mock, 2).await;

    let (task_id, slug) = create_test_task(&mut client, "timeout-test").await;

    let run_id = client
        .dispatch_agent(&slug, "coder")
        .await
        .expect("dispatch agent");

    // Should be marked failed after ~2s timeout (give extra margin)
    let status = wait_for_run_completion(&mut client, run_id.as_str(), 15).await;
    assert_eq!(status, "failed", "agent should be killed by timeout");

    // Task should be reverted to open
    let task = client.get_entity(&task_id).await.expect("get task");
    assert_eq!(
        task.status(),
        &EntityStatus::Open,
        "task should revert to open after timeout"
    );

    cancel.cancel();
}

#[tokio::test(flavor = "multi_thread")]
async fn reconcile_dead_agent_process() {
    use filament_core::store;

    // Start daemon with fast reconciliation interval (1s)
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join(".fl");
    std::fs::create_dir_all(&runtime_dir).expect("create runtime dir");

    let db_path = runtime_dir.join("fl.db");
    let socket_path = runtime_dir.join("fl.sock");
    let pid_path = runtime_dir.join("fl.pid");

    let pool = init_pool(db_path.to_str().unwrap())
        .await
        .expect("init pool");

    // Create a task via validated DTO
    let task_id = {
        use filament_core::dto::{CreateEntityRequest, ValidCreateEntityRequest};
        let store_tmp = filament_core::store::FilamentStore::new(pool.clone());
        let req = ValidCreateEntityRequest::try_from(
            CreateEntityRequest::from_parts(
                filament_core::models::EntityType::Task,
                "reconcile-test-task".to_string(),
                Some("test task for reconciliation".to_string()),
                None,
                None,
                None,
            )
            .expect("create request"),
        )
        .expect("valid request");
        let (id, _slug) = store_tmp
            .with_transaction(|conn| {
                Box::pin(async move { store::create_entity(conn, &req).await })
            })
            .await
            .expect("create task");
        id.to_string()
    };

    // Spawn a dummy process that exits immediately to get a dead PID
    let child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    #[allow(clippy::cast_possible_wrap)]
    let dead_pid = child.id() as i32;
    // Wait for process to definitely exit
    let _ = child.wait_with_output();

    // Insert a fake "running" agent_run with the dead PID
    {
        let store_tmp = filament_core::store::FilamentStore::new(pool.clone());
        store_tmp
            .with_transaction(|conn| {
                let tid = task_id.clone();
                Box::pin(async move {
                    store::create_agent_run(conn, &tid, "coder", Some(dead_pid)).await
                })
            })
            .await
            .expect("create agent run");

        // Mark task as in_progress
        store_tmp
            .with_transaction(|conn| {
                let tid = task_id.clone();
                Box::pin(async move {
                    store::update_entity_status(conn, &tid, EntityStatus::InProgress).await
                })
            })
            .await
            .expect("update task status");
    }
    drop(pool);

    // Start daemon with 1s reconciliation interval
    let config = ServeConfig {
        socket_path: socket_path.clone(),
        db_path,
        pid_path,
        cleanup_interval_secs: 3600,
        idle_timeout_secs: 0,
        reconciliation_interval_secs: 1,
    };

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        filament_daemon::serve(config, cancel_clone)
            .await
            .expect("daemon serve");
    });

    // Wait for socket
    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut client = DaemonClient::connect(&socket_path)
        .await
        .expect("connect to daemon");

    // Wait for reconciliation to run (interval is 1s, give it 3s)
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Verify: the agent run should now be marked failed
    let running = client
        .list_running_agents()
        .await
        .expect("list running agents");
    assert!(
        running.is_empty(),
        "dead agent should have been reconciled, but found {} running",
        running.len()
    );

    // Task should be reverted to open
    let task = client.get_entity(&task_id).await.expect("get task");
    assert_eq!(
        task.status(),
        &EntityStatus::Open,
        "task should be reverted to open after reconciliation"
    );

    cancel.cancel();
}
