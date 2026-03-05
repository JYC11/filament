mod common;

use common::{sample_entity_req, test_db};
use filament_core::diff::{fields_in_diff, DiffBuilder};
use filament_core::dto::EntityChangeset;
use filament_core::error::FilamentError;
use filament_core::models::EntityStatus;
use filament_core::store::*;

// ---------------------------------------------------------------------------
// Helper: create entity and return (id, version=0)
// ---------------------------------------------------------------------------

async fn create_test_entity(store: &FilamentStore) -> String {
    let req = sample_entity_req();
    let (id, _) = store
        .with_transaction(|conn| {
            let req = req.clone();
            Box::pin(async move { create_entity(conn, &req).await })
        })
        .await
        .unwrap();
    id.to_string()
}

// ---------------------------------------------------------------------------
// Version bump
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_bumps_version_from_0_to_1() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    let entity = get_entity(store.pool(), &id).await.unwrap();
    assert_eq!(entity.common().version, 0);

    let changeset = EntityChangeset {
        summary: Some("updated summary".to_string()),
        expected_version: 0,
        ..default_changeset()
    };

    let updated = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = changeset.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    assert_eq!(updated.common().version, 1);
    assert_eq!(updated.common().summary, "updated summary");
}

#[tokio::test]
async fn update_with_matching_version_succeeds() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    // First update: 0 → 1
    let cs1 = EntityChangeset {
        summary: Some("v1".to_string()),
        expected_version: 0,
        ..default_changeset()
    };
    let e1 = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs1.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();
    assert_eq!(e1.common().version, 1);

    // Second update: 1 → 2
    let cs2 = EntityChangeset {
        summary: Some("v2".to_string()),
        expected_version: 1,
        ..default_changeset()
    };
    let e2 = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs2.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();
    assert_eq!(e2.common().version, 2);
    assert_eq!(e2.common().summary, "v2");
}

// ---------------------------------------------------------------------------
// Auto-merge: non-overlapping fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn auto_merge_non_overlapping_fields() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    // Agent A updates summary (version 0 → 1)
    let cs_a = EntityChangeset {
        summary: Some("agent-a summary".to_string()),
        expected_version: 0,
        ..default_changeset()
    };
    store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_a.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    // Agent B updates status, but still expects version 0 (stale read)
    let cs_b = EntityChangeset {
        status: Some(EntityStatus::InProgress),
        expected_version: 0, // stale!
        ..default_changeset()
    };
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_b.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await;

    // Should auto-merge because fields don't overlap
    let entity = result.unwrap();
    assert_eq!(entity.common().version, 2);
    assert_eq!(entity.common().summary, "agent-a summary"); // preserved from agent A
    assert_eq!(*entity.status(), EntityStatus::InProgress); // applied from agent B
}

// ---------------------------------------------------------------------------
// Conflict: overlapping fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn conflict_on_overlapping_fields() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    // Agent A updates summary (version 0 → 1)
    let cs_a = EntityChangeset {
        summary: Some("agent-a summary".to_string()),
        expected_version: 0,
        ..default_changeset()
    };
    store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_a.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    // Agent B also updates summary, expecting version 0 (stale)
    let cs_b = EntityChangeset {
        summary: Some("agent-b summary".to_string()),
        expected_version: 0, // stale!
        ..default_changeset()
    };
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_b.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await;

    match result {
        Err(FilamentError::VersionConflict {
            current_version,
            conflicts,
            ..
        }) => {
            assert_eq!(current_version, 1);
            assert!(!conflicts.is_empty());
            let summary_conflict = conflicts.iter().find(|c| c.field == "summary").unwrap();
            assert_eq!(summary_conflict.your_value, "agent-b summary");
            assert_eq!(summary_conflict.their_value, "agent-a summary");
        }
        Ok(_) => panic!("expected VersionConflict, got Ok"),
        Err(e) => panic!("expected VersionConflict, got {e:?}"),
    }
}

#[tokio::test]
async fn conflict_includes_all_overlapping_fields() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    // Agent A updates summary AND status (version 0 → 1)
    let cs_a = EntityChangeset {
        summary: Some("a-summary".to_string()),
        status: Some(EntityStatus::InProgress),
        expected_version: 0,
        ..default_changeset()
    };
    store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_a.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    // Agent B also updates both fields with stale version
    let cs_b = EntityChangeset {
        summary: Some("b-summary".to_string()),
        status: Some(EntityStatus::Closed),
        expected_version: 0,
        ..default_changeset()
    };
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_b.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await;

    match result {
        Err(FilamentError::VersionConflict { conflicts, .. }) => {
            assert_eq!(conflicts.len(), 2);
            let fields: Vec<&str> = conflicts.iter().map(|c| c.field.as_str()).collect();
            assert!(fields.contains(&"summary"));
            assert!(fields.contains(&"status"));
        }
        other => panic!("expected VersionConflict, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Empty changeset rejected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_changeset_returns_validation_error() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    let cs = default_changeset();
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await;

    assert!(matches!(result, Err(FilamentError::Validation(_))));
}

// ---------------------------------------------------------------------------
// Create entity records a diff in the event
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_entity_records_diff_in_event() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    let events = get_entity_events(store.pool(), &id).await.unwrap();
    assert!(!events.is_empty());

    let create_event = &events[0];
    assert!(create_event.diff.is_some());

    let diff: serde_json::Value = serde_json::from_str(create_event.diff.as_ref().unwrap()).unwrap();
    assert_eq!(diff["name"], "Test task");
    assert_eq!(diff["entity_type"], "task");
}

// ---------------------------------------------------------------------------
// Update records a diff in the event
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_entity_records_diff_in_event() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    let cs = EntityChangeset {
        summary: Some("new summary".to_string()),
        status: Some(EntityStatus::InProgress),
        expected_version: 0,
        ..default_changeset()
    };
    store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    let events = get_entity_events(store.pool(), &id).await.unwrap();
    // First event is create, second is update
    let update_event = &events[1];
    assert!(update_event.diff.is_some());

    let diff: serde_json::Value =
        serde_json::from_str(update_event.diff.as_ref().unwrap()).unwrap();
    assert_eq!(diff["summary"]["old"], "A test task");
    assert_eq!(diff["summary"]["new"], "new summary");
    assert_eq!(diff["status"]["old"], "open");
    assert_eq!(diff["status"]["new"], "in_progress");
}

// ---------------------------------------------------------------------------
// DiffBuilder unit tests (complement the tests in diff.rs)
// ---------------------------------------------------------------------------

#[test]
fn diff_builder_create_mode() {
    let diff = DiffBuilder::create()
        .value("name", "foo")
        .value("priority", "2")
        .build()
        .unwrap();

    assert_eq!(diff["name"], "foo");
    assert_eq!(diff["priority"], "2");
}

#[test]
fn fields_in_diff_works_on_update_diff() {
    let diff = DiffBuilder::new()
        .field("summary", "old", "new")
        .field("status", "open", "closed")
        .field("priority", "2", "2") // same value — should NOT appear
        .build()
        .unwrap();

    let fields = fields_in_diff(&diff);
    assert_eq!(fields.len(), 2);
    assert!(fields.contains("summary"));
    assert!(fields.contains("status"));
    assert!(!fields.contains("priority"));
}

// ---------------------------------------------------------------------------
// Version conflict error properties
// ---------------------------------------------------------------------------

#[test]
fn version_conflict_error_code_and_retryable() {
    let err = FilamentError::VersionConflict {
        entity_id: "abc".to_string(),
        current_version: 5,
        conflicts: vec![],
    };
    assert_eq!(err.error_code(), "VERSION_CONFLICT");
    assert!(err.is_retryable());
    assert!(err.hint().unwrap().contains("resolve"));
}

// ---------------------------------------------------------------------------
// Version displayed in entity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn entity_has_version_zero_on_create() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    let entity = get_entity(store.pool(), &id).await.unwrap();
    assert_eq!(entity.common().version, 0);
}

// ---------------------------------------------------------------------------
// Edge case: multiple sequential updates then stale merge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn auto_merge_across_multiple_version_gaps() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    // Three sequential updates: summary v0→1, status v1→2, priority v2→3
    for (i, cs) in [
        EntityChangeset {
            summary: Some(format!("summary-v{}", 1)),
            expected_version: 0,
            ..default_changeset()
        },
        EntityChangeset {
            status: Some(EntityStatus::InProgress),
            expected_version: 1,
            ..default_changeset()
        },
        EntityChangeset {
            priority: Some(filament_core::models::Priority::new(0).unwrap()),
            expected_version: 2,
            ..default_changeset()
        },
    ]
    .into_iter()
    .enumerate()
    {
        let _ = i;
        let result = store
            .with_transaction(|conn| {
                let id = id.clone();
                let cs = cs.clone();
                Box::pin(async move { update_entity(conn, &id, &cs).await })
            })
            .await
            .unwrap();
        assert!(result.common().version > 0);
    }

    // Now a stale agent at version 0 updates content_path (never touched)
    let stale_cs = EntityChangeset {
        content_path: Some("docs/readme.md".to_string()),
        expected_version: 0, // 3 versions behind!
        ..default_changeset()
    };
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = stale_cs.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await;

    // Should auto-merge because content_path was never touched
    let entity = result.unwrap();
    assert_eq!(entity.common().version, 4);
    assert_eq!(
        entity.common().content.as_ref().unwrap().path,
        "docs/readme.md"
    );
    // All previous changes preserved
    assert_eq!(entity.common().summary, "summary-v1");
    assert_eq!(*entity.status(), EntityStatus::InProgress);
}

// ---------------------------------------------------------------------------
// Edge case: two agents race on same field, first wins, second conflicts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_agents_race_same_field_second_conflicts() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    // Both agents read version 0
    // Agent A wins the race
    let cs_a = EntityChangeset {
        status: Some(EntityStatus::InProgress),
        expected_version: 0,
        ..default_changeset()
    };
    store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_a.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    // Agent B loses — same field, same stale version
    let cs_b = EntityChangeset {
        status: Some(EntityStatus::Closed),
        expected_version: 0,
        ..default_changeset()
    };
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_b.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await;

    match result {
        Err(FilamentError::VersionConflict {
            conflicts,
            current_version,
            ..
        }) => {
            assert_eq!(current_version, 1);
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].field, "status");
            assert_eq!(conflicts[0].your_value, "closed");
            assert_eq!(conflicts[0].their_value, "in_progress");
        }
        other => panic!("expected VersionConflict, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Edge case: retry after conflict succeeds with fresh version
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retry_after_conflict_with_fresh_version_succeeds() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    // Agent A updates summary
    let cs_a = EntityChangeset {
        summary: Some("from-a".to_string()),
        expected_version: 0,
        ..default_changeset()
    };
    store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_a.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    // Agent B tries with stale version — conflict
    let cs_b_stale = EntityChangeset {
        summary: Some("from-b".to_string()),
        expected_version: 0,
        ..default_changeset()
    };
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_b_stale.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await;
    assert!(matches!(result, Err(FilamentError::VersionConflict { .. })));

    // Agent B re-reads entity, gets version 1, retries
    let entity = get_entity(store.pool(), &id).await.unwrap();
    assert_eq!(entity.common().version, 1);

    let cs_b_fresh = EntityChangeset {
        summary: Some("from-b".to_string()),
        expected_version: 1, // fresh
        ..default_changeset()
    };
    let updated = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_b_fresh.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    assert_eq!(updated.common().version, 2);
    assert_eq!(updated.common().summary, "from-b");
}

// ---------------------------------------------------------------------------
// Edge case: partial overlap — some fields conflict, some don't
// ---------------------------------------------------------------------------

#[tokio::test]
async fn partial_overlap_still_conflicts() {
    let store = test_db().await;
    let id = create_test_entity(&store).await;

    // Agent A updates summary + status
    let cs_a = EntityChangeset {
        summary: Some("a-summary".to_string()),
        status: Some(EntityStatus::InProgress),
        expected_version: 0,
        ..default_changeset()
    };
    store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_a.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await
        .unwrap();

    // Agent B updates summary (overlaps) + content_path (doesn't overlap)
    let cs_b = EntityChangeset {
        summary: Some("b-summary".to_string()),
        content_path: Some("new/path.md".to_string()),
        expected_version: 0,
        ..default_changeset()
    };
    let result = store
        .with_transaction(|conn| {
            let id = id.clone();
            let cs = cs_b.clone();
            Box::pin(async move { update_entity(conn, &id, &cs).await })
        })
        .await;

    // Even though content_path doesn't overlap, the summary conflict blocks everything
    match result {
        Err(FilamentError::VersionConflict { conflicts, .. }) => {
            // Conflicts should list ALL fields in the changeset, not just overlapping ones
            assert!(conflicts.iter().any(|c| c.field == "summary"));
            assert!(conflicts.iter().any(|c| c.field == "content_path"));
        }
        other => panic!("expected VersionConflict, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Edge case: entity not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_nonexistent_entity_returns_not_found() {
    let store = test_db().await;

    let cs = EntityChangeset {
        summary: Some("x".to_string()),
        expected_version: 0,
        ..default_changeset()
    };
    let result = store
        .with_transaction(|conn| {
            let cs = cs.clone();
            Box::pin(async move { update_entity(conn, "nonexistent-id", &cs).await })
        })
        .await;

    assert!(matches!(result, Err(FilamentError::EntityNotFound { .. })));
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn default_changeset() -> EntityChangeset {
    EntityChangeset {
        name: None,
        summary: None,
        status: None,
        priority: None,
        key_facts: None,
        content_path: None,
        expected_version: 0,
    }
}
