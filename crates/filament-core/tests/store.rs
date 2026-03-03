mod common;

use common::{blocks_req, sample_entity_req, sample_message_req, test_db};
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

    let id = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();

    let entity = get_entity(store.pool(), id.as_str()).await.unwrap();
    assert_eq!(entity.name, "Test task");
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
    let id = entities[0].id.as_str();

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
    req2.name = "Blocker task".to_string();

    let (id1, id2) = store
        .with_transaction(|conn| {
            let req1 = req1.clone();
            let req2 = req2.clone();
            Box::pin(async move {
                let id1 = create_entity(conn, &req1).await?;
                let id2 = create_entity(conn, &req2).await?;
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

    let id = store
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
    assert_eq!(entity.status, EntityStatus::InProgress);
}

// ---------------------------------------------------------------------------
// Reservation SQL
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reservation_acquire_and_conflict() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-1", "src/*.rs", true, 3600).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let result = store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-2", "src/*.rs", true, 3600).await?;
                Ok(())
            })
        })
        .await;

    assert!(matches!(result, Err(FilamentError::FileReserved { .. })));
}

#[tokio::test]
async fn reservation_release_allows_reacquire() {
    let store = test_db().await;

    let id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                acquire_reservation(conn, "agent-1", "src/*.rs", true, 3600).await
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
                acquire_reservation(conn, "agent-2", "src/*.rs", true, 3600).await?;
                Ok(())
            })
        })
        .await
        .unwrap();
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
    req_blocker.name = "Blocker".to_string();
    req_blocker.priority = 1;

    let mut req_blocked = sample_entity_req();
    req_blocked.name = "Blocked".to_string();
    req_blocked.priority = 0;

    let mut req_free = sample_entity_req();
    req_free.name = "Free".to_string();
    req_free.priority = 0;

    store
        .with_transaction(|conn| {
            let req_blocker = req_blocker.clone();
            let req_blocked = req_blocked.clone();
            let req_free = req_free.clone();
            Box::pin(async move {
                let blocker_id = create_entity(conn, &req_blocker).await?;
                let blocked_id = create_entity(conn, &req_blocked).await?;
                let _free_id = create_entity(conn, &req_free).await?;

                let rel = blocks_req(blocker_id.as_str(), blocked_id.as_str());
                create_relation(conn, &rel).await?;

                rebuild_blocked_cache(conn).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let ready = ready_tasks(store.pool()).await.unwrap();
    assert_eq!(ready.len(), 2); // Blocker + Free
    assert!(ready.iter().all(|e| e.name != "Blocked"));
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
                    "status_change",
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
    assert_eq!(events[0].event_type, "status_change");
}
