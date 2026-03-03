use std::time::Duration;

use filament_core::client::DaemonClient;
use filament_core::models::EntityStatus;
use filament_core::schema::init_pool;
use filament_daemon::config::ServeConfig;
use tokio_util::sync::CancellationToken;

/// Helper: start a test daemon with dispatch support using the mock agent script.
/// Each test should set its own env vars BEFORE calling this (or use defaults).
async fn start_test_daemon() -> (DaemonClient, CancellationToken, tempfile::TempDir) {
    // Find the mock-agent.sh script relative to workspace root
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let mock_script = workspace_root.join("util-scripts/mock-agent.sh");

    // Ensure mock script is executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&mock_script)
            .expect("mock-agent.sh exists")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&mock_script, perms).expect("chmod mock-agent.sh");
    }

    // Set the mock agent command (read by DispatchConfig::from_project_root at daemon startup)
    std::env::set_var("FILAMENT_AGENT_COMMAND", mock_script.to_str().unwrap());

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
        cleanup_interval_secs: 3600,
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

// Use serial test execution to avoid env var conflicts.
// Tests modify MOCK_AGENT_* env vars which are process-global.

#[tokio::test(flavor = "multi_thread")]
async fn dispatch_agent_via_socket() {
    let (mut client, cancel, _tmp) = start_test_daemon().await;

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
    let (mut client, cancel, _tmp) = start_test_daemon().await;

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
    let (mut client, cancel, _tmp) = start_test_daemon().await;

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

    cancel.cancel();
}
