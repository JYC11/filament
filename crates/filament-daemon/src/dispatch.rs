use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use filament_core::dto::{AgentResult, SendMessageRequest, ValidSendMessageRequest};
use filament_core::error::{FilamentError, Result};
use filament_core::graph::ContextBundle;
use filament_core::models::{AgentRunId, AgentStatus, EntityStatus, MessageType};
use filament_core::store;
use std::process::Command;
use tracing::{debug, error, info, warn};

use crate::roles::{self, AgentRole};
use crate::state::SharedState;

pub use crate::state::DispatchConfig;

/// Guard that kills a child process and cleans up MCP config on drop,
/// unless explicitly disarmed after a successful transaction.
struct ChildGuard {
    child: Option<std::process::Child>,
    mcp_config_path: Option<PathBuf>,
}

impl ChildGuard {
    const fn new(child: std::process::Child, mcp_config_path: PathBuf) -> Self {
        Self {
            child: Some(child),
            mcp_config_path: Some(mcp_config_path),
        }
    }

    /// Disarm the guard and return the child process for monitoring.
    /// After this call, Drop will not kill the child.
    fn disarm(mut self) -> (std::process::Child, PathBuf) {
        let child = self.child.take().expect("child already taken");
        let path = self.mcp_config_path.take().expect("path already taken");
        (child, path)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            warn!("killing orphaned child process pid={}", child.id());
            let _ = child.kill();
            let _ = child.wait(); // reap zombie
        }
        if let Some(path) = self.mcp_config_path.take() {
            cleanup_mcp_config(&path);
        }
    }
}

/// Build a temporary MCP config JSON file for the subprocess.
/// The agent's role is passed via `FILAMENT_AGENT_ROLE` env var so the MCP
/// server can enforce per-role tool filtering.
///
/// # Errors
///
/// Returns `AgentDispatchFailed` if JSON serialization fails, or `Io` if the file cannot be written.
pub fn build_mcp_config(
    run_id: &AgentRunId,
    project_root: &Path,
    role: AgentRole,
) -> Result<PathBuf> {
    let runtime_dir = project_root.join(".fl");
    let config_path = runtime_dir.join(format!("mcp-{run_id}.json"));

    let filament_bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fl"));

    let config = serde_json::json!({
        "mcpServers": {
            "fl": {
                "command": filament_bin.to_string_lossy(),
                "args": ["mcp"],
                "cwd": project_root.to_string_lossy(),
                "env": {
                    "FILAMENT_AGENT_ROLE": role.as_str(),
                },
            }
        }
    });

    let config_str =
        serde_json::to_string_pretty(&config).map_err(|e| FilamentError::AgentDispatchFailed {
            reason: e.to_string(),
        })?;
    std::fs::write(&config_path, config_str)?;

    Ok(config_path)
}

/// Build the full system prompt: role prompt + task info + context bundle.
#[must_use]
pub fn build_system_prompt(
    role: AgentRole,
    task_name: &str,
    summary: &str,
    context: &ContextBundle,
) -> String {
    use std::fmt::Write;

    let mut prompt = roles::system_prompt(role).to_string();
    prompt.push_str("\n\n--- TASK ---\n");
    let _ = writeln!(prompt, "Task: {task_name}");
    if !summary.is_empty() {
        let _ = writeln!(prompt, "Summary: {summary}");
    }
    let lines = context.to_prompt_lines();
    if !lines.is_empty() {
        prompt.push('\n');
        for line in &lines {
            prompt.push_str(line);
            prompt.push('\n');
        }
    }
    prompt
}

/// Dispatch a single agent subprocess. Returns the run ID immediately.
/// The agent is monitored asynchronously via `tokio::spawn`.
///
/// # Errors
///
/// Returns errors if the task is invalid, already has a running agent, or spawn fails.
pub async fn dispatch_agent(
    state: &Arc<SharedState>,
    config: &DispatchConfig,
    task_slug: &str,
    role: AgentRole,
) -> Result<AgentRunId> {
    // Resolve task
    let task = store::resolve_task(state.store.pool(), task_slug).await?;
    let task_id = task.id.as_str().to_string();
    let task_name = task.name.to_string();
    let task_slug_resolved = task.slug.as_str().to_string();
    let summary = task.summary.clone();

    // Pre-dispatch checks (status)
    if !matches!(task.status, EntityStatus::Open | EntityStatus::InProgress) {
        return Err(FilamentError::AgentDispatchFailed {
            reason: format!(
                "task '{task_slug}' has status '{}', expected open or in_progress",
                task.status
            ),
        });
    }

    // Pre-flight check: fast reject if agent already running (avoids spawning
    // a subprocess that would be immediately orphaned).
    if store::has_running_agent(state.store.pool(), &task_id).await? {
        return Err(FilamentError::AgentAlreadyRunning {
            task_id: task_id.clone(),
        });
    }

    // Build MCP config with role for tool filtering
    let mcp_config_path = build_mcp_config(&AgentRunId::new(), &config.project_root, role)?;

    // Build system prompt with rich context bundle
    let context = {
        let graph = state.graph_read().await;
        graph.build_context_bundle(&task_id, config.context_depth)
    };
    let system_prompt = build_system_prompt(role, &task_name, &summary, &context);

    // Spawn subprocess using std::process (not tokio::process) so that
    // wait_with_output() uses direct waitpid() instead of tokio's SIGCHLD
    // machinery, which can lose notifications when multiple children exit
    // before their monitors are polled.
    let mut cmd = Command::new(&config.agent_command);
    cmd.arg("-p")
        .arg(&system_prompt)
        .arg("--mcp-config")
        .arg(&mcp_config_path)
        .current_dir(&config.project_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| FilamentError::AgentDispatchFailed {
            reason: format!("failed to spawn '{}': {e}", config.agent_command),
        })?;

    // Guard kills the child + cleans MCP config if the transaction below fails.
    // Disarmed only after successful transaction commit.
    let guard = ChildGuard::new(child, mcp_config_path);

    #[allow(clippy::cast_possible_wrap)]
    let pid = guard.child.as_ref().map(|c| c.id() as i32);

    // Atomically check for running agent + create run in a single transaction
    let agent_name = format!("{}-{task_slug_resolved}", role.as_str());
    let run_id = state
        .store
        .with_transaction(|conn| {
            let task_id = task_id.clone();
            let role_name = role.as_str().to_string();
            Box::pin(async move {
                // Check for running agent inside the transaction to prevent TOCTOU races
                if store::has_running_agent_conn(conn, &task_id).await? {
                    return Err(FilamentError::AgentAlreadyRunning {
                        task_id: task_id.clone(),
                    });
                }
                store::create_agent_run(conn, &task_id, &role_name, pid).await
            })
        })
        .await?;

    // Transaction succeeded — disarm the guard so the child isn't killed.
    let (child, mcp_config_path) = guard.disarm();

    // Update task to in_progress
    state
        .store
        .with_transaction(|conn| {
            let task_id = task_id.clone();
            Box::pin(async move {
                store::update_entity_status(conn, &task_id, EntityStatus::InProgress).await
            })
        })
        .await?;

    // Spawn monitor task IMMEDIATELY after agent_run creation + status update.
    // Minimises the gap between cmd.spawn() and wait_with_output() to prevent
    // child-exit races (P1 bug: monitors failing to reap fast-exiting children
    // during batch dispatch).
    let monitor_state = Arc::clone(state);
    let monitor_run_id = run_id.clone();
    let monitor_task_id = task_id.clone();
    let monitor_agent_name = agent_name.clone();
    let monitor_mcp_config = mcp_config_path.clone();
    let monitor_timeout = config.agent_timeout_secs;
    tokio::spawn(async move {
        monitor_agent(
            &monitor_state,
            child,
            &monitor_run_id,
            &monitor_task_id,
            &monitor_agent_name,
            &monitor_mcp_config,
            monitor_timeout,
        )
        .await;
    });

    // Refresh graph (non-critical, safe to run after monitor is already watching)
    state
        .graph_write()
        .await
        .hydrate(state.store.pool())
        .await?;

    info!(
        run_id = %run_id,
        task = %task_slug,
        role = %role,
        pid = ?pid,
        "agent dispatched"
    );

    Ok(run_id)
}

/// Wait for agent child process to finish, with optional timeout.
/// Returns `Err(message)` if the wait failed or timed out.
async fn await_agent_output(
    state: &Arc<SharedState>,
    child: std::process::Child,
    run_id: &AgentRunId,
    timeout_secs: u64,
) -> std::result::Result<std::process::Output, String> {
    let blocking_wait = tokio::task::spawn_blocking(move || child.wait_with_output());

    if timeout_secs > 0 {
        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), blocking_wait)
            .await
        {
            Ok(Ok(Ok(output))) => Ok(output),
            Ok(Ok(Err(e))) => {
                error!(run_id = %run_id, "failed to wait on agent: {e}");
                Err(e.to_string())
            }
            Ok(Err(e)) => {
                error!(run_id = %run_id, "monitor task panicked: {e}");
                Err(e.to_string())
            }
            Err(_elapsed) => {
                warn!(run_id = %run_id, timeout_secs = timeout_secs, "agent timed out, killing");
                kill_agent_by_run_id(state, run_id).await;
                Err(format!("agent timed out after {timeout_secs}s"))
            }
        }
    } else {
        match blocking_wait.await {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(e)) => {
                error!(run_id = %run_id, "failed to wait on agent: {e}");
                Err(e.to_string())
            }
            Err(e) => {
                error!(run_id = %run_id, "monitor task panicked: {e}");
                Err(e.to_string())
            }
        }
    }
}

/// Monitor a spawned agent subprocess, parse its output, and route the result.
///
/// Uses `spawn_blocking` for `wait_with_output()` to avoid tokio's SIGCHLD
/// race condition where child exit notifications are lost when monitors
/// aren't polled before the child exits.
///
/// If `timeout_secs > 0`, the agent is killed after that duration.
async fn monitor_agent(
    state: &Arc<SharedState>,
    child: std::process::Child,
    run_id: &AgentRunId,
    task_id: &str,
    agent_name: &str,
    mcp_config_path: &Path,
    timeout_secs: u64,
) {
    let output = match await_agent_output(state, child, run_id, timeout_secs).await {
        Ok(output) => output,
        Err(msg) => {
            finish_run_failed(state, run_id, task_id, agent_name, &msg).await;
            cleanup_mcp_config(mcp_config_path);
            return;
        }
    };

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    debug!(
        run_id = %run_id,
        exit_code = exit_code,
        stdout_len = stdout.len(),
        stderr_len = stderr.len(),
        "agent exited"
    );

    if !stderr.is_empty() {
        debug!(run_id = %run_id, stderr = %stderr, "agent stderr");
    }

    if !output.status.success() {
        let reason = format!("agent exited with code {exit_code}");
        warn!(run_id = %run_id, reason = %reason, "agent failed");
        finish_run_failed(state, run_id, task_id, agent_name, &reason).await;
        cleanup_mcp_config(mcp_config_path);
        return;
    }

    // Parse output
    match parse_agent_output(&stdout) {
        Ok(result) => {
            let unblocked = route_result(state, run_id, task_id, agent_name, &result).await;
            // Auto-dispatch unblocked tasks using boxed futures to break the
            // async type cycle (dispatch_agent → monitor_agent → dispatch_agent).
            if !unblocked.is_empty() {
                if let Some(config) = state.dispatch_config() {
                    for slug in unblocked {
                        let s = Arc::clone(state);
                        let c = config.clone();
                        let rid = run_id.clone();
                        tokio::spawn(dispatch_agent_boxed(s, c, slug, rid));
                    }
                }
            }
        }
        Err(e) => {
            warn!(run_id = %run_id, "failed to parse agent output: {e}");
            finish_run_failed(
                state,
                run_id,
                task_id,
                agent_name,
                &format!("output parse failed: {e}"),
            )
            .await;
        }
    }

    cleanup_mcp_config(mcp_config_path);
}

/// Parse agent stdout for an `AgentResult` JSON.
/// Tries full stdout first, then scans lines from the end.
///
/// # Errors
///
/// Returns an error string if no valid `AgentResult` JSON is found.
pub fn parse_agent_output(stdout: &str) -> std::result::Result<AgentResult, String> {
    // Try full stdout as JSON
    if let Ok(result) = serde_json::from_str::<AgentResult>(stdout.trim()) {
        return Ok(result);
    }

    // Scan lines from the end, looking for JSON
    for line in stdout.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(result) = serde_json::from_str::<AgentResult>(trimmed) {
                return Ok(result);
            }
        }
    }

    Err("no valid AgentResult JSON found in output".to_string())
}

/// Route a successful `AgentResult`: update run, send messages, update task status.
/// Returns entity IDs of newly unblocked tasks (for auto-dispatch by caller).
async fn route_result(
    state: &Arc<SharedState>,
    run_id: &AgentRunId,
    task_id: &str,
    agent_name: &str,
    result: &AgentResult,
) -> Vec<String> {
    let result_json = serde_json::to_string(result).ok();

    // Finish the run
    let agent_status = result.status.clone();
    if let Err(e) = state
        .store
        .with_transaction(|conn| {
            let id = run_id.to_string();
            let status = agent_status.clone();
            let rj = result_json.clone();
            Box::pin(async move { store::finish_agent_run(conn, &id, status, rj.as_deref()).await })
        })
        .await
    {
        error!(run_id = %run_id, "failed to finish agent run: {e}");
    }

    // Route messages
    for msg in &result.messages {
        route_single_message(
            state,
            run_id,
            agent_name,
            task_id,
            msg.to_agent.as_str(),
            msg.body.as_str(),
            Some(msg.msg_type.clone()),
        )
        .await;
    }

    // Route blockers as messages to "user"
    for blocker in &result.blockers {
        route_single_message(
            state,
            run_id,
            agent_name,
            task_id,
            "user",
            blocker,
            Some(MessageType::Blocker),
        )
        .await;
    }

    // Route questions as messages to "user"
    for question in &result.questions {
        route_single_message(
            state,
            run_id,
            agent_name,
            task_id,
            "user",
            question,
            Some(MessageType::Question),
        )
        .await;
    }

    // Update task status based on agent result
    let new_task_status = match result.status {
        AgentStatus::Completed => Some(EntityStatus::Closed),
        AgentStatus::Blocked => Some(EntityStatus::Blocked),
        AgentStatus::Failed => Some(EntityStatus::Open),
        AgentStatus::NeedsInput | AgentStatus::Running => None,
    };

    if let Some(status) = new_task_status {
        if let Err(e) = state
            .store
            .with_transaction(|conn| {
                let tid = task_id.to_string();
                Box::pin(async move { store::update_entity_status(conn, &tid, status).await })
            })
            .await
        {
            error!(run_id = %run_id, "failed to update task status: {e}");
        }
    }

    // Release reservations when agent exits (completed, failed, or blocked — subprocess is gone)
    if matches!(
        result.status,
        AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Blocked
    ) {
        if let Err(e) = state
            .store
            .with_transaction(|conn| {
                let name = agent_name.to_string();
                Box::pin(async move { store::release_reservations_by_agent(conn, &name).await })
            })
            .await
        {
            warn!(run_id = %run_id, "failed to release agent reservations: {e}");
        }
    }

    // Refresh graph
    if let Err(e) = refresh_graph(state).await {
        warn!(run_id = %run_id, "failed to refresh graph: {e}");
    }

    // Collect newly unblocked tasks for auto-dispatch (caller handles dispatch
    // to avoid async type cycle: dispatch_agent → monitor → route_result → dispatch_agent)
    let unblocked = if result.status == AgentStatus::Completed {
        collect_unblocked_for_dispatch(state, task_id, run_id).await
    } else {
        Vec::new()
    };

    info!(
        run_id = %run_id,
        status = %result.status,
        summary = %result.summary,
        "agent result routed"
    );

    unblocked
}

/// Route a single message through the store.
async fn route_single_message(
    state: &Arc<SharedState>,
    run_id: &AgentRunId,
    from: &str,
    task_id: &str,
    to: &str,
    body: &str,
    msg_type: Option<MessageType>,
) {
    let req = SendMessageRequest {
        from_agent: from.to_string(),
        to_agent: to.to_string(),
        body: body.to_string(),
        msg_type,
        in_reply_to: None,
        task_id: Some(task_id.to_string()),
    };
    match ValidSendMessageRequest::try_from(req) {
        Ok(valid) => {
            if let Err(e) = state
                .store
                .with_transaction(|conn| {
                    let valid = valid.clone();
                    Box::pin(async move { store::send_message(conn, &valid).await })
                })
                .await
            {
                warn!(run_id = %run_id, "failed to route message: {e}");
            }
        }
        Err(e) => {
            warn!(run_id = %run_id, from = %from, to = %to, "message validation failed: {e}");
        }
    }
}

/// Collect task slugs newly unblocked by a completed task, for auto-dispatch.
/// Only returns slugs if `DispatchConfig.auto_dispatch` is enabled.
async fn collect_unblocked_for_dispatch(
    state: &Arc<SharedState>,
    completed_task_id: &str,
    run_id: &AgentRunId,
) -> Vec<String> {
    let config = match state.dispatch_config() {
        Some(c) if c.auto_dispatch => c,
        _ => return Vec::new(),
    };

    let unblocked = {
        let graph = state.graph_read().await;
        graph.newly_unblocked_by(completed_task_id)
    };

    if unblocked.is_empty() {
        return Vec::new();
    }

    let limit = config.max_auto_dispatch;
    info!(
        run_id = %run_id,
        count = unblocked.len(),
        limit = limit,
        "collecting unblocked tasks for auto-dispatch"
    );

    let mut slugs = Vec::new();
    for entity_id in unblocked.into_iter().take(limit) {
        match store::get_entity(state.store.pool(), entity_id.as_str()).await {
            Ok(entity) => slugs.push(entity.common().slug.to_string()),
            Err(e) => {
                warn!(run_id = %run_id, entity_id = %entity_id, "failed to look up entity for auto-dispatch: {e}");
            }
        }
    }
    slugs
}

/// Boxed-future wrapper for `dispatch_agent` to break async type recursion.
/// Used by auto-dispatch: `monitor_agent` → `route_result` → `dispatch_agent`.
fn dispatch_agent_boxed(
    state: Arc<SharedState>,
    config: DispatchConfig,
    slug: String,
    parent_run_id: AgentRunId,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async move {
        match dispatch_agent(&state, &config, &slug, AgentRole::Coder).await {
            Ok(new_run_id) => {
                info!(run_id = %parent_run_id, new_run_id = %new_run_id, task = %slug, "auto-dispatched agent");
            }
            Err(e) => {
                warn!(run_id = %parent_run_id, task = %slug, "auto-dispatch failed: {e}");
            }
        }
    })
}

/// Mark a run as failed, release reservations, revert task to open.
async fn finish_run_failed(
    state: &Arc<SharedState>,
    run_id: &AgentRunId,
    task_id: &str,
    agent_name: &str,
    error_msg: &str,
) {
    // Mark run as failed
    if let Err(e) = state
        .store
        .with_transaction(|conn| {
            let id = run_id.to_string();
            let result_json = serde_json::json!({"error": error_msg}).to_string();
            Box::pin(async move {
                store::finish_agent_run(conn, &id, AgentStatus::Failed, Some(&result_json)).await
            })
        })
        .await
    {
        error!(run_id = %run_id, "failed to mark run as failed: {e}");
    }

    // Revert task to open
    if let Err(e) = state
        .store
        .with_transaction(|conn| {
            let tid = task_id.to_string();
            Box::pin(
                async move { store::update_entity_status(conn, &tid, EntityStatus::Open).await },
            )
        })
        .await
    {
        error!(run_id = %run_id, "failed to revert task status: {e}");
    }

    // Release reservations
    if let Err(e) = state
        .store
        .with_transaction(|conn| {
            let name = agent_name.to_string();
            Box::pin(async move { store::release_reservations_by_agent(conn, &name).await })
        })
        .await
    {
        warn!(run_id = %run_id, "failed to release agent reservations: {e}");
    }

    // Refresh graph
    if let Err(e) = refresh_graph(state).await {
        warn!(run_id = %run_id, "failed to refresh graph: {e}");
    }
}

async fn refresh_graph(state: &Arc<SharedState>) -> Result<()> {
    let mut graph = state.graph_write().await;
    graph.hydrate(state.store.pool()).await
}

/// Kill an agent process by looking up its PID from the `agent_run` record.
async fn kill_agent_by_run_id(state: &Arc<SharedState>, run_id: &AgentRunId) {
    let runs = match store::list_running_agents(state.store.pool()).await {
        Ok(r) => r,
        Err(e) => {
            error!(run_id = %run_id, "failed to look up agent run for kill: {e}");
            return;
        }
    };
    let run_id_str = run_id.to_string();
    if let Some(run) = runs.iter().find(|r| r.id.to_string() == run_id_str) {
        if let Some(pid) = run.pid {
            let _ = Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .status();
            // Give 2s for graceful exit, then SIGKILL
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let alive = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if alive {
                let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            }
        }
    }
}

fn cleanup_mcp_config(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("failed to cleanup MCP config {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use filament_core::graph::ContextBundle;

    #[test]
    fn test_parse_agent_output_full_json() {
        let json = r#"{"status":"completed","summary":"done","artifacts":[],"messages":[],"blockers":[],"questions":[]}"#;
        let result = parse_agent_output(json).unwrap();
        assert_eq!(result.status, AgentStatus::Completed);
        assert_eq!(result.summary, "done");
    }

    #[test]
    fn test_parse_agent_output_with_noise() {
        let output = "Starting agent...\nProcessing task...\n{\"status\":\"completed\",\"summary\":\"done\",\"artifacts\":[],\"messages\":[],\"blockers\":[],\"questions\":[]}\n";
        let result = parse_agent_output(output).unwrap();
        assert_eq!(result.status, AgentStatus::Completed);
    }

    #[test]
    fn test_parse_agent_output_invalid() {
        let output = "no json here\njust text output\n";
        assert!(parse_agent_output(output).is_err());
    }

    #[test]
    fn test_build_system_prompt() {
        let bundle = ContextBundle {
            summaries: vec!["Module: auth".to_string(), "Depends: session".to_string()],
            blocker_depth: 2,
            impact_score: 3,
            upstream_artifacts: vec!["[completed] setup-db: schema ready".to_string()],
        };
        let prompt = build_system_prompt(
            AgentRole::Coder,
            "fix-bug",
            "Fix the login validation bug",
            &bundle,
        );
        assert!(prompt.contains("Coder agent"));
        assert!(prompt.contains("fix-bug"));
        assert!(prompt.contains("Fix the login validation bug"));
        assert!(prompt.contains("Module: auth"));
        assert!(prompt.contains("Blocker depth: 2"));
        assert!(prompt.contains("UPSTREAM RESULTS"));
        assert!(prompt.contains("setup-db"));
        assert!(prompt.contains("3 downstream"));
    }

    #[test]
    fn test_build_system_prompt_no_context() {
        let bundle = ContextBundle {
            summaries: vec![],
            blocker_depth: 0,
            impact_score: 0,
            upstream_artifacts: vec![],
        };
        let prompt =
            build_system_prompt(AgentRole::Reviewer, "review-pr", "Review PR #42", &bundle);
        assert!(prompt.contains("Reviewer agent"));
        assert!(prompt.contains("review-pr"));
        assert!(!prompt.contains("CONTEXT"));
        assert!(!prompt.contains("Blocker depth"));
        assert!(!prompt.contains("UPSTREAM RESULTS"));
    }

    #[test]
    fn test_child_guard_kills_on_drop() {
        // Spawn a long-running process
        let mut verification_child = Command::new("sleep")
            .arg("60")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn sleep");

        let tmp =
            std::env::temp_dir().join(format!("mcp-guard-test-{}.json", verification_child.id()));
        std::fs::write(&tmp, "{}").unwrap();
        assert!(tmp.exists());

        // Verify process is alive before guard
        assert!(
            verification_child.try_wait().unwrap().is_none(),
            "child should be running"
        );

        // Create guard and drop it — child should be killed, config cleaned up
        {
            let _guard = ChildGuard::new(verification_child, tmp.clone());
        }
        // After guard drop, we can't check the child directly since it's moved,
        // but MCP config should be cleaned up
        assert!(
            !tmp.exists(),
            "MCP config should be removed after guard drop"
        );
    }

    #[test]
    fn test_child_guard_disarm_preserves_child() {
        let child = Command::new("sleep")
            .arg("60")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn sleep");

        let tmp = std::env::temp_dir().join(format!("mcp-guard-test-disarm-{}.json", child.id()));
        std::fs::write(&tmp, "{}").unwrap();

        let guard = ChildGuard::new(child, tmp.clone());
        let (mut child, path) = guard.disarm();

        // Process should still be alive after disarm
        assert!(
            child.try_wait().unwrap().is_none(),
            "child should still be running after disarm"
        );

        // Clean up
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&path);
    }
}
