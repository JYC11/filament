mod common;

use common::{
    blocks_req, depends_on_req, sample_entity_req, sample_message_req, task_req, test_db,
};
use filament_core::error::FilamentError;
use filament_core::models::*;
use filament_core::store::*;

// ---------------------------------------------------------------------------
// Transaction semantics
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transaction_commits_on_ok() {
    let store = test_db().await;
    let req = sample_entity_req();

    let (id, _) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let entity = get_entity(store.pool(), id.as_str()).await.unwrap();
    assert_eq!(entity.name(), "Test task");
}

#[tokio::test]
async fn transaction_rolls_back_on_err() {
    let store = test_db().await;
    let req = sample_entity_req();

    let result: Result<(), FilamentError> = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move {
                create_entity(conn, &req).await?;
                Err(FilamentError::Validation("forced error".to_string()))
            })
        })
        .await;

    assert!(result.is_err());

    let entities = list_entities(store.pool(), None, None).await.unwrap();
    assert!(entities.is_empty());
}

// ---------------------------------------------------------------------------
// Schema CHECK constraints (DB-level enforcement)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn check_constraint_rejects_invalid_entity_type() {
    let store = test_db().await;

    let result = store
        .with_transaction(|conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO entities (id, name, entity_type, summary, status, priority, created_at, updated_at)
                     VALUES ('e1', 'test', 'INVALID_TYPE', '', 'open', 2, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
                )
                .execute(conn)
                .await?;
                Ok(())
            })
        })
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn check_constraint_rejects_invalid_status() {
    let store = test_db().await;

    let result = store
        .with_transaction(|conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO entities (id, name, entity_type, summary, status, priority, created_at, updated_at)
                     VALUES ('e1', 'test', 'task', '', 'INVALID_STATUS', 2, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
                )
                .execute(conn)
                .await?;
                Ok(())
            })
        })
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn check_constraint_rejects_priority_out_of_range() {
    let store = test_db().await;

    let result = store
        .with_transaction(|conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO entities (id, name, entity_type, summary, status, priority, created_at, updated_at)
                     VALUES ('e1', 'test', 'task', '', 'open', 99, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
                )
                .execute(conn)
                .await?;
                Ok(())
            })
        })
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn check_constraint_rejects_self_referencing_relation() {
    let store = test_db().await;

    // First create an entity to reference
    store
        .with_transaction(|conn| {
            let req = sample_entity_req();
            Box::pin(async move {
                create_entity(conn, &req).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let entities = list_entities(store.pool(), None, None).await.unwrap();
    let id = entities[0].id().as_str();

    let result = store
        .with_transaction(|conn| {
            let id = id.to_string();
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO relations (id, source_id, target_id, relation_type, created_at)
                     VALUES ('r1', ?, ?, 'blocks', '2024-01-01T00:00:00Z')",
                )
                .bind(&id)
                .bind(&id)
                .execute(conn)
                .await?;
                Ok(())
            })
        })
        .await;

    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// FK cascade
// ---------------------------------------------------------------------------

#[tokio::test]
async fn entity_not_found() {
    let store = test_db().await;
    let err = get_entity(store.pool(), "nonexistent").await.unwrap_err();
    assert!(matches!(err, FilamentError::EntityNotFound { .. }));
}

#[tokio::test]
async fn delete_entity_cascades_relations() {
    let store = test_db().await;
    let req1 = sample_entity_req();
    let mut req2 = sample_entity_req();
    req2.name = NonEmptyString::new("Blocker task").unwrap();

    let (id1, id2) = store
        .with_transaction(|conn| {
            let req1 = req1.clone();
            let req2 = req2.clone();
            Box::pin(async move {
                let (id1, _) = create_entity(conn, &req1).await?;
                let (id2, _) = create_entity(conn, &req2).await?;
                let rel_req = blocks_req(id1.as_str(), id2.as_str());
                create_relation(conn, &rel_req).await?;
                Ok((id1, id2))
            })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            let id1 = id1.clone();
            Box::pin(async move { delete_entity(conn, id1.as_str()).await })
        })
        .await
        .unwrap();

    let relations = list_relations(store.pool(), id2.as_str()).await.unwrap();
    assert!(relations.is_empty());
}

// ---------------------------------------------------------------------------
// Status update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_entity_status_works() {
    let store = test_db().await;
    let req = sample_entity_req();

    let (id, _) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            let id = id.clone();
            Box::pin(async move {
                update_entity_status(conn, id.as_str(), EntityStatus::InProgress).await
            })
        })
        .await
        .unwrap();

    let entity = get_entity(store.pool(), id.as_str()).await.unwrap();
    assert_eq!(*entity.status(), EntityStatus::InProgress);
}

// ---------------------------------------------------------------------------
// Reservation SQL
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reservation_acquire_and_conflict() {
    let store = test_db().await;
    let ttl = TtlSeconds::new(3600).unwrap();

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-1", "src/*.rs", ReservationMode::Exclusive, ttl)
                    .await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let result = store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-2", "src/*.rs", ReservationMode::Exclusive, ttl)
                    .await?;
                Ok(())
            })
        })
        .await;

    assert!(matches!(result, Err(FilamentError::FileReserved { .. })));
}

#[tokio::test]
async fn reservation_release_allows_reacquire() {
    let store = test_db().await;
    let ttl = TtlSeconds::new(3600).unwrap();

    let id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-1", "src/*.rs", ReservationMode::Exclusive, ttl)
                    .await
            })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            let id = id.clone();
            Box::pin(async move { release_reservation(conn, id.as_str()).await })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-2", "src/*.rs", ReservationMode::Exclusive, ttl)
                    .await?;
                Ok(())
            })
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn exclusive_reservation_conflicts_with_nonexclusive() {
    let store = test_db().await;
    let ttl = TtlSeconds::new(3600).unwrap();

    // Agent-1 acquires a non-exclusive reservation
    store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-1", "src/*.rs", ReservationMode::Shared, ttl)
                    .await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    // Agent-2 tries to acquire an exclusive reservation on the same glob — should fail
    let result = store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-2", "src/*.rs", ReservationMode::Exclusive, ttl)
                    .await?;
                Ok(())
            })
        })
        .await;

    assert!(matches!(result, Err(FilamentError::FileReserved { .. })));
}

#[tokio::test]
async fn mark_already_read_message_returns_not_found() {
    let store = test_db().await;
    let msg = sample_message_req();

    let id = store
        .with_transaction(|conn| {
            let msg = msg.clone();
            Box::pin(async move { send_message(conn, &msg).await })
        })
        .await
        .unwrap();

    // Mark as read the first time — should succeed
    store
        .with_transaction(|conn| {
            let id = id.clone();
            Box::pin(async move { mark_message_read(conn, id.as_str()).await })
        })
        .await
        .unwrap();

    // Mark as read again — should return MessageAlreadyRead (not MessageNotFound)
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            Box::pin(async move { mark_message_read(conn, id.as_str()).await })
        })
        .await;

    assert!(matches!(
        result,
        Err(FilamentError::MessageAlreadyRead { .. })
    ));
}

// ---------------------------------------------------------------------------
// Message queries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn message_send_and_inbox() {
    let store = test_db().await;
    let msg = sample_message_req();

    store
        .with_transaction(|conn| {
            let msg = msg.clone();
            Box::pin(async move {
                send_message(conn, &msg).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let inbox = get_inbox(store.pool(), "agent-b").await.unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].body, "hello");
    assert_eq!(inbox[0].from_agent, "agent-a");
}

#[tokio::test]
async fn message_mark_read_removes_from_inbox() {
    let store = test_db().await;
    let msg = sample_message_req();

    let id = store
        .with_transaction(|conn| {
            let msg = msg.clone();
            Box::pin(async move { send_message(conn, &msg).await })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            let id = id.clone();
            Box::pin(async move { mark_message_read(conn, id.as_str()).await })
        })
        .await
        .unwrap();

    let inbox = get_inbox(store.pool(), "agent-b").await.unwrap();
    assert!(inbox.is_empty());
}

// ---------------------------------------------------------------------------
// Blocked cache + ready tasks (complex query)
// ---------------------------------------------------------------------------

#[tokio::test]
#[allow(clippy::similar_names)]
async fn ready_tasks_excludes_blocked() {
    let store = test_db().await;

    let mut req_blocker = sample_entity_req();
    req_blocker.name = NonEmptyString::new("Blocker").unwrap();
    req_blocker.priority = Priority::new(1).unwrap();

    let mut req_blocked = sample_entity_req();
    req_blocked.name = NonEmptyString::new("Blocked").unwrap();
    req_blocked.priority = Priority::new(0).unwrap();

    let mut req_free = sample_entity_req();
    req_free.name = NonEmptyString::new("Free").unwrap();
    req_free.priority = Priority::new(0).unwrap();

    let ready = store
        .with_transaction(|conn| {
            let req_blocker = req_blocker.clone();
            let req_blocked = req_blocked.clone();
            let req_free = req_free.clone();
            Box::pin(async move {
                let (blocker_id, _) = create_entity(conn, &req_blocker).await?;
                let (blocked_id, _) = create_entity(conn, &req_blocked).await?;
                let (_free_id, _) = create_entity(conn, &req_free).await?;

                let rel = blocks_req(blocker_id.as_str(), blocked_id.as_str());
                create_relation(conn, &rel).await?;

                ready_tasks(conn).await
            })
        })
        .await
        .unwrap();
    assert_eq!(ready.len(), 2); // Blocker + Free
    assert!(ready.iter().all(|e| e.name() != "Blocked"));
}

#[tokio::test]
#[allow(clippy::similar_names)]
async fn ready_tasks_excludes_depends_on() {
    let store = test_db().await;

    let mut req_dependency = sample_entity_req();
    req_dependency.name = NonEmptyString::new("Dependency").unwrap();
    req_dependency.priority = Priority::new(1).unwrap();

    let mut req_dependent = sample_entity_req();
    req_dependent.name = NonEmptyString::new("Dependent").unwrap();
    req_dependent.priority = Priority::new(0).unwrap();

    let mut req_free = sample_entity_req();
    req_free.name = NonEmptyString::new("FreeTask").unwrap();
    req_free.priority = Priority::new(0).unwrap();

    let ready = store
        .with_transaction(|conn| {
            let req_dependency = req_dependency.clone();
            let req_dependent = req_dependent.clone();
            let req_free = req_free.clone();
            Box::pin(async move {
                let (dep_id, _) = create_entity(conn, &req_dependency).await?;
                let (dependent_id, _) = create_entity(conn, &req_dependent).await?;
                let (_free_id, _) = create_entity(conn, &req_free).await?;

                // Dependent depends_on Dependency (Dependent is blocked until Dependency closes)
                let rel = depends_on_req(dependent_id.as_str(), dep_id.as_str());
                create_relation(conn, &rel).await?;

                ready_tasks(conn).await
            })
        })
        .await
        .unwrap();
    assert_eq!(ready.len(), 2); // Dependency + FreeTask
    assert!(ready.iter().all(|e| e.name() != "Dependent"));
}

// ---------------------------------------------------------------------------
// Event log
// ---------------------------------------------------------------------------

#[tokio::test]
async fn event_recording() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                record_event(
                    conn,
                    Some("e1"),
                    EventType::StatusChange,
                    "cli",
                    Some("open"),
                    Some("closed"),
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let events = get_entity_events(store.pool(), "e1").await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::StatusChange);
}

// ---------------------------------------------------------------------------
// Expire stale reservations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn expire_stale_reservations_cleans_expired() {
    let store = test_db().await;

    // Insert a reservation that already expired (expires_at in the past)
    store
        .with_transaction(|conn| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO file_reservations (id, agent_name, file_glob, exclusive, created_at, expires_at)
                     VALUES ('r1', 'old-agent', 'src/*.rs', 1, '2020-01-01T00:00:00Z', '2020-01-02T00:00:00Z')",
                )
                .execute(&mut *conn)
                .await?;
                let expired = expire_stale_reservations(conn).await?;
                assert_eq!(expired, 1);
                Ok(())
            })
        })
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Agent run lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn agent_run_create_and_finish() {
    let store = test_db().await;

    // Create an entity for the task_id reference
    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = sample_entity_req();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let run_id = store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(async move { create_agent_run(conn, tid.as_str(), "coder", Some(1234)).await })
        })
        .await
        .unwrap();

    // Should appear in running list
    let running = list_running_agents(store.pool()).await.unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].agent_role, "coder");
    assert_eq!(running[0].pid, Some(1234));

    // Finish the run
    store
        .with_transaction(|conn| {
            let rid = run_id.clone();
            Box::pin(async move {
                finish_agent_run(
                    conn,
                    rid.as_str(),
                    AgentStatus::Completed,
                    Some(r#"{"ok":true}"#),
                )
                .await
            })
        })
        .await
        .unwrap();

    // Should no longer appear in running list
    let running = list_running_agents(store.pool()).await.unwrap();
    assert!(running.is_empty());
}

// ---------------------------------------------------------------------------
// Finish nonexistent agent run
// ---------------------------------------------------------------------------

#[tokio::test]
async fn finish_nonexistent_agent_run_returns_error() {
    let store = test_db().await;

    let err = store
        .with_transaction(|conn| {
            Box::pin(async move {
                finish_agent_run(conn, "nonexistent", AgentStatus::Completed, None).await
            })
        })
        .await
        .unwrap_err();

    assert!(matches!(err, FilamentError::AgentRunNotFound { .. }));
}

// ---------------------------------------------------------------------------
// List relations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_relations_returns_both_directions() {
    let store = test_db().await;

    let (id1, id2, id3) = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (id1, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (id2, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (id3, _) = create_entity(conn, &task_req("C", 1)).await?;
                // A blocks B, C blocks A
                create_relation(conn, &blocks_req(id1.as_str(), id2.as_str())).await?;
                create_relation(conn, &blocks_req(id3.as_str(), id1.as_str())).await?;
                Ok((id1, id2, id3))
            })
        })
        .await
        .unwrap();

    // A has 2 relations (as source of one, target of other)
    let rels = list_relations(store.pool(), id1.as_str()).await.unwrap();
    assert_eq!(rels.len(), 2);

    // B has 1 relation (as target)
    let rels = list_relations(store.pool(), id2.as_str()).await.unwrap();
    assert_eq!(rels.len(), 1);

    // C has 1 relation (as source)
    let rels = list_relations(store.pool(), id3.as_str()).await.unwrap();
    assert_eq!(rels.len(), 1);
}

// ---------------------------------------------------------------------------
// Delete relation not found
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Mark nonexistent message read
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mark_nonexistent_message_read_returns_error() {
    let store = test_db().await;

    let err = store
        .with_transaction(|conn| {
            Box::pin(async move { mark_message_read(conn, "nonexistent").await })
        })
        .await
        .unwrap_err();

    assert!(matches!(err, FilamentError::MessageNotFound { .. }));
}

// ---------------------------------------------------------------------------
// Release nonexistent reservation (idempotent — no error)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn release_nonexistent_reservation_returns_error() {
    let store = test_db().await;

    let err = store
        .with_transaction(|conn| {
            Box::pin(async move { release_reservation(conn, "nonexistent").await })
        })
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            filament_core::error::FilamentError::ReservationNotFound { .. }
        ),
        "expected ReservationNotFound, got: {err:?}"
    );
}

// ---------------------------------------------------------------------------
// get_agent_run
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_agent_run_returns_run() {
    let store = test_db().await;

    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = sample_entity_req();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let run_id = store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(async move { create_agent_run(conn, tid.as_str(), "coder", Some(42)).await })
        })
        .await
        .unwrap();

    let run = get_agent_run(store.pool(), run_id.as_str()).await.unwrap();
    assert_eq!(run.id, run_id);
    assert_eq!(run.agent_role, "coder");
    assert_eq!(run.pid, Some(42));
    assert_eq!(run.status, AgentStatus::Running);
}

#[tokio::test]
async fn get_agent_run_not_found() {
    let store = test_db().await;
    let err = get_agent_run(store.pool(), "nonexistent")
        .await
        .unwrap_err();
    assert!(matches!(err, FilamentError::AgentRunNotFound { .. }));
}

// ---------------------------------------------------------------------------
// has_running_agent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn has_running_agent_true_when_running() {
    let store = test_db().await;

    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = sample_entity_req();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(async move { create_agent_run(conn, tid.as_str(), "coder", None).await })
        })
        .await
        .unwrap();

    assert!(has_running_agent(store.pool(), task_id.as_str())
        .await
        .unwrap());
}

#[tokio::test]
async fn has_running_agent_false_when_none() {
    let store = test_db().await;
    assert!(!has_running_agent(store.pool(), "nonexistent-task")
        .await
        .unwrap());
}

#[tokio::test]
async fn has_running_agent_false_after_finish() {
    let store = test_db().await;

    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = sample_entity_req();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let run_id = store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(async move { create_agent_run(conn, tid.as_str(), "coder", None).await })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            let rid = run_id.clone();
            Box::pin(async move {
                finish_agent_run(conn, rid.as_str(), AgentStatus::Completed, None).await
            })
        })
        .await
        .unwrap();

    assert!(!has_running_agent(store.pool(), task_id.as_str())
        .await
        .unwrap());
}

// ---------------------------------------------------------------------------
// has_running_agent_conn (transaction-safe version)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn has_running_agent_conn_in_transaction() {
    let store = test_db().await;

    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = sample_entity_req();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    // Initially no running agent — check inside a transaction
    let result = store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(async move { has_running_agent_conn(conn, tid.as_str()).await })
        })
        .await
        .unwrap();
    assert!(!result);

    // Create a running agent
    store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(async move { create_agent_run(conn, tid.as_str(), "coder", None).await })
        })
        .await
        .unwrap();

    // Now should be true
    let result = store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(async move { has_running_agent_conn(conn, tid.as_str()).await })
        })
        .await
        .unwrap();
    assert!(result);
}

// ---------------------------------------------------------------------------
// list_agent_runs_by_task
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_agent_runs_by_task_returns_all_runs() {
    let store = test_db().await;

    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = sample_entity_req();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    // Create two runs
    for role in &["coder", "reviewer"] {
        store
            .with_transaction(|conn| {
                let tid = task_id.clone();
                let role = role.to_string();
                Box::pin(async move { create_agent_run(conn, tid.as_str(), &role, None).await })
            })
            .await
            .unwrap();
    }

    let runs = list_agent_runs_by_task(store.pool(), task_id.as_str())
        .await
        .unwrap();
    assert_eq!(runs.len(), 2);
}

// ---------------------------------------------------------------------------
// release_reservations_by_agent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn release_reservations_by_agent_releases_all() {
    let store = test_db().await;

    // Create two reservations for the same agent
    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let ttl = filament_core::models::TtlSeconds::new(3600).unwrap();
                acquire_reservation(
                    conn,
                    "my-agent",
                    "src/*.rs",
                    ReservationMode::Exclusive,
                    ttl,
                )
                .await?;
                acquire_reservation(conn, "my-agent", "tests/*.rs", ReservationMode::Shared, ttl)
                    .await?;
                // Also one for a different agent (should not be released)
                acquire_reservation(
                    conn,
                    "other-agent",
                    "docs/*.md",
                    ReservationMode::Exclusive,
                    ttl,
                )
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let released = store
        .with_transaction(|conn| {
            Box::pin(async move { release_reservations_by_agent(conn, "my-agent").await })
        })
        .await
        .unwrap();

    assert_eq!(released, 2);

    // other-agent's reservation should still exist
    let remaining = list_reservations(store.pool(), None).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].agent_name, "other-agent");
}

// ---------------------------------------------------------------------------
// Batch queries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blocked_by_counts_returns_correct_counts() {
    let store = test_db().await;

    // Create three tasks: A depends_on B, A depends_on C
    let (id_a, _) = store
        .with_transaction(|conn| {
            let req = task_req("Task A", 3);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let (id_b, _) = store
        .with_transaction(|conn| {
            let req = task_req("Task B", 2);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let (id_c, _) = store
        .with_transaction(|conn| {
            let req = task_req("Task C", 1);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    // A depends_on B
    store
        .with_transaction(|conn| {
            let req = depends_on_req(id_a.as_str(), id_b.as_str());
            Box::pin(async move { create_relation(conn, &req).await })
        })
        .await
        .unwrap();

    // A depends_on C
    store
        .with_transaction(|conn| {
            let req = depends_on_req(id_a.as_str(), id_c.as_str());
            Box::pin(async move { create_relation(conn, &req).await })
        })
        .await
        .unwrap();

    let counts = blocked_by_counts(store.pool()).await.unwrap();

    // A is source of 2 DependsOn relations → blocked_by_count = 2
    assert_eq!(counts.get(id_a.as_str()).copied().unwrap_or(0), 2);
    // B is target only → blocked_by_count = 0
    assert_eq!(counts.get(id_b.as_str()).copied().unwrap_or(0), 0);
    // C is target only → blocked_by_count = 0
    assert_eq!(counts.get(id_c.as_str()).copied().unwrap_or(0), 0);
}

#[tokio::test]
async fn blocked_by_counts_includes_blocks_relations() {
    let store = test_db().await;

    // Create three tasks: B blocks A (via blocks relation), A depends_on C
    let (id_a, _) = store
        .with_transaction(|conn| {
            let req = task_req("Task A", 3);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let (id_b, _) = store
        .with_transaction(|conn| {
            let req = task_req("Task B", 2);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let (id_c, _) = store
        .with_transaction(|conn| {
            let req = task_req("Task C", 1);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    // B blocks A (blocks relation: source=B, target=A → A is blocked)
    store
        .with_transaction(|conn| {
            let req = blocks_req(id_b.as_str(), id_a.as_str());
            Box::pin(async move { create_relation(conn, &req).await })
        })
        .await
        .unwrap();

    // A depends_on C (depends_on relation: source=A, target=C → A is blocked)
    store
        .with_transaction(|conn| {
            let req = depends_on_req(id_a.as_str(), id_c.as_str());
            Box::pin(async move { create_relation(conn, &req).await })
        })
        .await
        .unwrap();

    let counts = blocked_by_counts(store.pool()).await.unwrap();

    // A is blocked by 2 things: B (via blocks) + C (via depends_on)
    assert_eq!(counts.get(id_a.as_str()).copied().unwrap_or(0), 2);
    // B is not blocked by anything
    assert_eq!(counts.get(id_b.as_str()).copied().unwrap_or(0), 0);
    // C is not blocked by anything
    assert_eq!(counts.get(id_c.as_str()).copied().unwrap_or(0), 0);
}

// ---------------------------------------------------------------------------
// Batch get entities
// ---------------------------------------------------------------------------

#[tokio::test]
async fn batch_get_entities_returns_requested() {
    let store = test_db().await;

    let (id_a, _) = store
        .with_transaction(|conn| {
            let req = task_req("Alpha", 1);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let (id_b, _) = store
        .with_transaction(|conn| {
            let req = task_req("Beta", 2);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let (id_c, _) = store
        .with_transaction(|conn| {
            let req = task_req("Gamma", 3);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    // Fetch two of three
    let ids = [id_a.as_str(), id_c.as_str()];
    let map = batch_get_entities(store.pool(), &ids).await.unwrap();

    assert_eq!(map.len(), 2);
    assert_eq!(map[id_a.as_str()].name().as_str(), "Alpha");
    assert_eq!(map[id_c.as_str()].name().as_str(), "Gamma");
    assert!(!map.contains_key(id_b.as_str()));
}

#[tokio::test]
async fn batch_get_entities_empty_ids_returns_empty() {
    let store = test_db().await;
    let ids: [&str; 0] = [];
    let map = batch_get_entities(store.pool(), &ids).await.unwrap();
    assert!(map.is_empty());
}

#[tokio::test]
async fn batch_get_entities_missing_ids_are_omitted() {
    let store = test_db().await;

    let (id_a, _) = store
        .with_transaction(|conn| {
            let req = task_req("Exists", 1);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let ids = [id_a.as_str(), "nonexistent-id"];
    let map = batch_get_entities(store.pool(), &ids).await.unwrap();

    assert_eq!(map.len(), 1);
    assert_eq!(map[id_a.as_str()].name().as_str(), "Exists");
}

// ---------------------------------------------------------------------------
// Regression: duplicate relation returns validation error (not raw DB error)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_duplicate_relation_returns_validation_error() {
    let store = test_db().await;
    let req1 = task_req("Source", 1);
    let req2 = task_req("Target", 1);

    let (id1, id2) = store
        .with_transaction(|conn| {
            let req1 = req1.clone();
            let req2 = req2.clone();
            Box::pin(async move {
                let (id1, _) = create_entity(conn, &req1).await?;
                let (id2, _) = create_entity(conn, &req2).await?;
                Ok((id1, id2))
            })
        })
        .await
        .unwrap();

    // First relation succeeds
    store
        .with_transaction(|conn| {
            let rel = depends_on_req(id1.as_str(), id2.as_str());
            Box::pin(async move { create_relation(conn, &rel).await.map(|_| ()) })
        })
        .await
        .unwrap();

    // Duplicate relation returns Validation error, not Database error
    let result = store
        .with_transaction(|conn| {
            let rel = depends_on_req(id1.as_str(), id2.as_str());
            Box::pin(async move { create_relation(conn, &rel).await.map(|_| ()) })
        })
        .await;

    let err = result.unwrap_err();
    assert!(
        matches!(err, FilamentError::Validation(ref msg) if msg.contains("relation already exists")),
        "expected Validation('relation already exists'), got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Regression: empty glob pattern rejected in acquire_reservation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn acquire_reservation_rejects_empty_glob() {
    let store = test_db().await;

    let result = store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(
                    conn,
                    "test-agent",
                    "",
                    ReservationMode::from(false),
                    TtlSeconds::new(3600).unwrap(),
                )
                .await
            })
        })
        .await;

    let err = result.unwrap_err();
    assert!(
        matches!(err, FilamentError::Validation(ref msg) if msg.contains("cannot be empty")),
        "expected Validation('cannot be empty'), got: {err}"
    );
}

#[tokio::test]
async fn acquire_reservation_rejects_whitespace_only_glob() {
    let store = test_db().await;

    let result = store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(
                    conn,
                    "test-agent",
                    "   ",
                    ReservationMode::from(false),
                    TtlSeconds::new(3600).unwrap(),
                )
                .await
            })
        })
        .await;

    let err = result.unwrap_err();
    assert!(
        matches!(err, FilamentError::Validation(ref msg) if msg.contains("cannot be empty")),
        "expected Validation('cannot be empty'), got: {err}"
    );
}
