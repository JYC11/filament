mod common;

use common::{
    blocks_req, depends_on_req, sample_entity_req, sample_message_req, task_req, test_db,
};
use filament_core::dto::*;
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
async fn trigger_creates_event_on_entity_insert() {
    let store = test_db().await;
    let req = sample_entity_req();

    let (id, _) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let events = get_entity_events(store.pool(), id.as_str()).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::EntityCreated);
    let diff: serde_json::Value = serde_json::from_str(events[0].diff.as_ref().unwrap()).unwrap();
    assert_eq!(diff["name"], "Test task");
    assert_eq!(diff["entity_type"], "task");
}

#[tokio::test]
async fn trigger_creates_event_on_status_change() {
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

    let events = get_entity_events(store.pool(), id.as_str()).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, EventType::EntityCreated);
    assert_eq!(events[1].event_type, EventType::StatusChange);
    let diff: serde_json::Value = serde_json::from_str(events[1].diff.as_ref().unwrap()).unwrap();
    assert_eq!(diff["status"]["old"], "open");
    assert_eq!(diff["status"]["new"], "in_progress");
}

#[tokio::test]
async fn trigger_creates_event_on_entity_delete() {
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
            Box::pin(async move { delete_entity(conn, id.as_str()).await })
        })
        .await
        .unwrap();

    // Events persist after entity deletion (no FK cascade on events)
    let events = get_entity_events(store.pool(), id.as_str()).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, EventType::EntityCreated);
    assert_eq!(events[1].event_type, EventType::EntityDeleted);
    // Delete diff has same flat format as create diff
    let diff: serde_json::Value = serde_json::from_str(events[1].diff.as_ref().unwrap()).unwrap();
    assert_eq!(diff["name"], "Test task");
    assert_eq!(diff["entity_type"], "task");
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

// ---------------------------------------------------------------------------
// Export / Import
// ---------------------------------------------------------------------------

#[tokio::test]
async fn export_empty_db_returns_valid_export_data() {
    let store = test_db().await;
    let data = export_all(store.pool(), true).await.unwrap();
    assert_eq!(data.version, 1);
    assert!(data.entities.is_empty());
    assert!(data.relations.is_empty());
    assert!(data.messages.is_empty());
    assert!(data.events.is_empty());
}

#[tokio::test]
async fn export_includes_entity_and_relation() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (id1, _) = create_entity(conn, &task_req("Task A", 1)).await?;
                let (id2, _) = create_entity(conn, &task_req("Task B", 2)).await?;
                create_relation(conn, &blocks_req(id1.as_str(), id2.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let data = export_all(store.pool(), false).await.unwrap();
    assert_eq!(data.entities.len(), 2);
    assert_eq!(data.relations.len(), 1);
    assert!(data.events.is_empty()); // no_events = true
    assert_eq!(data.version, 1);
}

#[tokio::test]
async fn import_into_empty_db_creates_entities_and_relations() {
    let store = test_db().await;

    // Create data in one store
    let source_store = test_db().await;
    source_store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (id1, _) = create_entity(conn, &task_req("Imported A", 1)).await?;
                let (id2, _) = create_entity(conn, &task_req("Imported B", 2)).await?;
                create_relation(conn, &blocks_req(id1.as_str(), id2.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let export_data = export_all(source_store.pool(), true).await.unwrap();

    // Import into the empty store
    let result = store
        .with_transaction(|conn| {
            let data = export_data.clone();
            Box::pin(async move { import_data(conn, &data, true).await })
        })
        .await
        .unwrap();

    assert_eq!(result.entities_imported, 2);
    assert_eq!(result.relations_imported, 1);
    assert!(result.events_imported > 0);

    // Verify entities exist
    let entities = list_entities(store.pool(), None, None).await.unwrap();
    assert_eq!(entities.len(), 2);
}

#[tokio::test]
async fn import_twice_is_idempotent() {
    let store = test_db().await;

    let source_store = test_db().await;
    source_store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (id1, _) = create_entity(conn, &task_req("Alpha", 1)).await?;
                let (id2, _) = create_entity(conn, &task_req("Beta", 2)).await?;
                create_relation(conn, &blocks_req(id1.as_str(), id2.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let export_data = export_all(source_store.pool(), false).await.unwrap();

    // First import
    let result1 = store
        .with_transaction(|conn| {
            let data = export_data.clone();
            Box::pin(async move { import_data(conn, &data, false).await })
        })
        .await
        .unwrap();
    assert_eq!(result1.entities_imported, 2);
    assert_eq!(result1.relations_imported, 1);

    // Second import — entities upserted via ON CONFLICT DO UPDATE (no FK cascade).
    // Relations already exist (INSERT OR IGNORE), so 0 new.
    let result2 = store
        .with_transaction(|conn| {
            let data = export_data.clone();
            Box::pin(async move { import_data(conn, &data, false).await })
        })
        .await
        .unwrap();
    assert_eq!(result2.entities_imported, 2); // upsert counts all
    assert_eq!(result2.relations_imported, 0); // already exist, no cascade

    // No duplicates
    let entities = list_entities(store.pool(), None, None).await.unwrap();
    assert_eq!(entities.len(), 2);
    let relations = list_all_relations(store.pool()).await.unwrap();
    assert_eq!(relations.len(), 1);
}

// ---------------------------------------------------------------------------
// Escalation queries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_pending_escalations_returns_blocker_messages() {
    let store = test_db().await;

    // Send a blocker message
    store
        .with_transaction(|conn| {
            let req = ValidSendMessageRequest {
                from_agent: NonEmptyString::new("agent-x").unwrap(),
                to_agent: NonEmptyString::new("user").unwrap(),
                body: NonEmptyString::new("I'm stuck").unwrap(),
                msg_type: MessageType::Blocker,
                in_reply_to: None,
                task_id: None,
            };
            Box::pin(async move { send_message(conn, &req).await })
        })
        .await
        .unwrap();

    let escalations = list_pending_escalations(store.pool()).await.unwrap();
    assert_eq!(escalations.len(), 1);
    assert_eq!(escalations[0].kind, EscalationKind::Blocker);
    assert_eq!(escalations[0].agent_name, "agent-x");
}

#[tokio::test]
async fn list_pending_escalations_empty_when_no_blockers() {
    let store = test_db().await;

    // Send a normal text message (not a blocker)
    store
        .with_transaction(|conn| {
            let req = sample_message_req();
            Box::pin(async move { send_message(conn, &req).await })
        })
        .await
        .unwrap();

    let escalations = list_pending_escalations(store.pool()).await.unwrap();
    assert!(escalations.is_empty());
}

#[tokio::test]
async fn list_pending_escalations_excludes_read_messages() {
    let store = test_db().await;

    // Send a blocker message
    let msg_id = store
        .with_transaction(|conn| {
            let req = ValidSendMessageRequest {
                from_agent: NonEmptyString::new("agent-x").unwrap(),
                to_agent: NonEmptyString::new("user").unwrap(),
                body: NonEmptyString::new("blocked").unwrap(),
                msg_type: MessageType::Blocker,
                in_reply_to: None,
                task_id: None,
            };
            Box::pin(async move { send_message(conn, &req).await })
        })
        .await
        .unwrap();

    // Mark it read
    store
        .with_transaction(|conn| {
            let id = msg_id.to_string();
            Box::pin(async move { mark_message_read(conn, &id).await })
        })
        .await
        .unwrap();

    let escalations = list_pending_escalations(store.pool()).await.unwrap();
    assert!(escalations.is_empty());
}

#[tokio::test]
async fn list_pending_escalations_blocked_agent_run_has_blocker_kind() {
    let store = test_db().await;

    // Create a task and an agent run, then finish the run as blocked
    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = task_req("Blocked Task", 2);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let run_id = store
        .with_transaction(|conn| {
            let tid = task_id.to_string();
            Box::pin(async move { create_agent_run(conn, &tid, "coder", None).await })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            let rid = run_id.to_string();
            Box::pin(async move { finish_agent_run(conn, &rid, AgentStatus::Blocked, None).await })
        })
        .await
        .unwrap();

    let escalations = list_pending_escalations(store.pool()).await.unwrap();
    assert_eq!(escalations.len(), 1);
    assert_eq!(escalations[0].kind, EscalationKind::Blocker);
}

#[tokio::test]
async fn list_pending_escalations_needs_input_agent_run_has_needs_input_kind() {
    let store = test_db().await;

    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = task_req("Input Task", 2);
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let run_id = store
        .with_transaction(|conn| {
            let tid = task_id.to_string();
            Box::pin(async move { create_agent_run(conn, &tid, "coder", None).await })
        })
        .await
        .unwrap();

    store
        .with_transaction(|conn| {
            let rid = run_id.to_string();
            Box::pin(
                async move { finish_agent_run(conn, &rid, AgentStatus::NeedsInput, None).await },
            )
        })
        .await
        .unwrap();

    let escalations = list_pending_escalations(store.pool()).await.unwrap();
    assert_eq!(escalations.len(), 1);
    assert_eq!(escalations[0].kind, EscalationKind::NeedsInput);
}

// ---------------------------------------------------------------------------
// Import error handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn import_slug_conflict_returns_validation_error() {
    let store = test_db().await;

    // Create an entity with a known slug
    store
        .with_transaction(|conn| {
            Box::pin(async move { create_entity(conn, &task_req("Existing", 1)).await })
        })
        .await
        .unwrap();

    // Export to get the entity's slug
    let export_data = export_all(store.pool(), false).await.unwrap();
    let existing_slug = export_data.entities[0].slug().to_string();

    // Build import data with a different UUID but the same slug
    let mut conflict_data = export_data.clone();
    let entity = conflict_data.entities[0].clone();
    let mut common = entity.into_common();
    common.id = EntityId::new(); // different ID, same slug
    let conflict_entity = Entity::Task(common);
    conflict_data.entities = vec![conflict_entity];

    let result = store
        .with_transaction(|conn| {
            let data = conflict_data.clone();
            Box::pin(async move { import_data(conn, &data, false).await })
        })
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    match &err {
        FilamentError::Validation(msg) => {
            assert!(
                msg.contains("slug") && msg.contains(&existing_slug),
                "expected slug conflict message, got: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }
}

#[tokio::test]
async fn import_relation_with_missing_entity_returns_validation_error() {
    let store = test_db().await;

    // Build import data with a relation pointing to non-existent entities
    let bad_relation = Relation {
        id: RelationId::new(),
        source_id: EntityId::new(),
        target_id: EntityId::new(),
        relation_type: RelationType::Owns,
        weight: Weight::DEFAULT,
        summary: String::new(),
        metadata: serde_json::Value::Object(serde_json::Map::new()),
        created_at: chrono::Utc::now(),
    };

    let import = ExportData {
        version: 1,
        exported_at: chrono::Utc::now(),
        entities: vec![],
        relations: vec![bad_relation],
        messages: vec![],
        events: vec![],
    };

    let result = store
        .with_transaction(|conn| {
            let data = import.clone();
            Box::pin(async move { import_data(conn, &data, false).await })
        })
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    match &err {
        FilamentError::Validation(msg) => {
            assert!(
                msg.contains("non-existent entity"),
                "expected FK error message, got: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// reconcile_stale_agent_runs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reconcile_stale_runs_marks_running_as_failed() {
    let store = test_db().await;

    // Create a task
    let req = sample_entity_req();
    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    // Create a running agent run
    let tid = task_id.clone();
    let run_id = store
        .with_transaction(|conn| {
            Box::pin(
                async move { create_agent_run(conn, tid.as_str(), "coder", Some(99999)).await },
            )
        })
        .await
        .unwrap();

    // Set task to in_progress (simulating what dispatch does)
    store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(async move {
                update_entity_status(conn, tid.as_str(), EntityStatus::InProgress).await
            })
        })
        .await
        .unwrap();

    // Verify preconditions
    let run = get_agent_run(store.pool(), run_id.as_str()).await.unwrap();
    assert_eq!(run.status, AgentStatus::Running);

    // Reconcile
    let count = store
        .with_transaction(|conn| Box::pin(async move { reconcile_stale_agent_runs(conn).await }))
        .await
        .unwrap();
    assert_eq!(count, 1);

    // Agent run should be failed
    let run = get_agent_run(store.pool(), run_id.as_str()).await.unwrap();
    assert_eq!(run.status, AgentStatus::Failed);
    assert!(run.finished_at.is_some());
    assert!(run.result_json.unwrap().contains("daemon restarted"));

    // Task should be reverted to open
    let entity = get_entity(store.pool(), task_id.as_str()).await.unwrap();
    assert_eq!(*entity.status(), EntityStatus::Open);
}

#[tokio::test]
async fn reconcile_stale_runs_noop_when_none_running() {
    let store = test_db().await;

    let count = store
        .with_transaction(|conn| Box::pin(async move { reconcile_stale_agent_runs(conn).await }))
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn reconcile_stale_runs_does_not_revert_closed_tasks() {
    let store = test_db().await;

    // Create a task
    let req = sample_entity_req();
    let (task_id, _) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    // Create a running agent run
    let tid = task_id.clone();
    store
        .with_transaction(|conn| {
            Box::pin(async move { create_agent_run(conn, tid.as_str(), "coder", None).await })
        })
        .await
        .unwrap();

    // Close the task (e.g., user closed it manually while agent was running)
    store
        .with_transaction(|conn| {
            let tid = task_id.clone();
            Box::pin(
                async move { update_entity_status(conn, tid.as_str(), EntityStatus::Closed).await },
            )
        })
        .await
        .unwrap();

    // Reconcile should mark run as failed but NOT revert the closed task
    store
        .with_transaction(|conn| Box::pin(async move { reconcile_stale_agent_runs(conn).await }))
        .await
        .unwrap();

    let entity = get_entity(store.pool(), task_id.as_str()).await.unwrap();
    assert_eq!(*entity.status(), EntityStatus::Closed);
}

// ---------------------------------------------------------------------------
// Lesson entity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lesson_create_and_retrieve() {
    let store = test_db().await;
    let req = common::lesson_req(
        "SQLite CHECK gotcha",
        "INSERT fails with CHECK constraint violation",
        "Recreate table with updated CHECK constraint in migration",
        "SQLite cannot ALTER CHECK constraints — must recreate table",
    );

    let (id, slug) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let entity = get_entity(store.pool(), id.as_str()).await.unwrap();
    assert_eq!(entity.entity_type(), EntityType::Lesson);
    assert_eq!(entity.name().as_str(), "SQLite CHECK gotcha");

    // Verify lesson fields round-trip through key_facts
    let fields = LessonFields::from_entity(&entity).unwrap();
    assert_eq!(fields.problem, "INSERT fails with CHECK constraint violation");
    assert_eq!(
        fields.solution,
        "Recreate table with updated CHECK constraint in migration"
    );
    assert!(fields.pattern.is_none());
    assert_eq!(
        fields.learned,
        "SQLite cannot ALTER CHECK constraints — must recreate table"
    );

    // Verify slug lookup works
    let by_slug = get_entity_by_slug(store.pool(), slug.as_str()).await.unwrap();
    assert_eq!(by_slug.entity_type(), EntityType::Lesson);
}

#[tokio::test]
async fn lesson_with_pattern() {
    let store = test_db().await;
    let fields = LessonFields {
        problem: "N+1 query on task list".to_string(),
        solution: "Use batch WHERE IN query".to_string(),
        pattern: Some("n-plus-one-fix".to_string()),
        learned: "Always check query count with tracing".to_string(),
    };
    let req = ValidCreateEntityRequest {
        name: NonEmptyString::new("N+1 query fix").unwrap(),
        entity_type: EntityType::Lesson,
        summary: fields.learned.clone(),
        key_facts: fields.to_key_facts(),
        content_path: None,
        priority: Priority::DEFAULT,
    };

    let (id, _) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let entity = get_entity(store.pool(), id.as_str()).await.unwrap();
    let round_tripped = LessonFields::from_entity(&entity).unwrap();
    assert_eq!(round_tripped.pattern.as_deref(), Some("n-plus-one-fix"));
}

#[tokio::test]
async fn lesson_listed_by_type_filter() {
    let store = test_db().await;

    // Create a task and a lesson
    let task_req = common::task_req("Some task", 1);
    let lesson_req = common::lesson_req(
        "A gotcha",
        "problem",
        "solution",
        "learned",
    );

    store
        .with_transaction(|conn| {
            let t = task_req.clone();
            let l = lesson_req.clone();
            Box::pin(async move {
                create_entity(conn, &t).await?;
                create_entity(conn, &l).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    // List only lessons
    let lessons = list_entities(store.pool(), Some("lesson"), None)
        .await
        .unwrap();
    assert_eq!(lessons.len(), 1);
    assert_eq!(lessons[0].entity_type(), EntityType::Lesson);

    // List all should include both
    let all = list_entities(store.pool(), None, None).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn resolve_lesson_type_check() {
    let store = test_db().await;
    let req = common::lesson_req("Test lesson", "p", "s", "l");

    let (_, slug) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    // resolve_lesson succeeds
    let common = resolve_lesson(store.pool(), slug.as_str()).await.unwrap();
    assert_eq!(common.name.as_str(), "Test lesson");

    // resolve_task on a lesson fails with TypeMismatch
    let err = resolve_task(store.pool(), slug.as_str()).await.unwrap_err();
    assert!(matches!(err, FilamentError::TypeMismatch { .. }));
}
