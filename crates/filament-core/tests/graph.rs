mod common;

use common::{blocks_req, task_req, test_db};
use filament_core::error::FilamentError;
use filament_core::graph::KnowledgeGraph;
use filament_core::store::*;

// ---------------------------------------------------------------------------
// Hydration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hydrate_matches_store_counts() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let b = create_entity(conn, &task_req("B", 2)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1);
}

// ---------------------------------------------------------------------------
// ready_tasks
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ready_tasks_excludes_blocked_nodes() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let b = create_entity(conn, &task_req("B", 2)).await?;
                let _c = create_entity(conn, &task_req("C", 0)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let ready = graph.ready_tasks();
    let names: Vec<&str> = ready.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"A"));
    assert!(names.contains(&"C"));
    assert!(!names.contains(&"B"));
    // C (priority 0) should come before A (priority 1)
    assert_eq!(ready[0].name, "C");
    assert_eq!(ready[1].name, "A");
}

// ---------------------------------------------------------------------------
// critical_path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn critical_path_follows_dependencies() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let b = create_entity(conn, &task_req("B", 1)).await?;
                let c = create_entity(conn, &task_req("C", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &blocks_req(b.as_str(), c.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let path = graph.critical_path(a_id.as_str());
    assert_eq!(path.len(), 3); // A -> B -> C
}

// ---------------------------------------------------------------------------
// impact_score
// ---------------------------------------------------------------------------

#[tokio::test]
async fn impact_score_counts_transitive_dependents() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let b = create_entity(conn, &task_req("B", 1)).await?;
                let c = create_entity(conn, &task_req("C", 1)).await?;
                // B blocks A, C blocks A (so A has 2 incoming blockers)
                create_relation(conn, &blocks_req(b.as_str(), a.as_str())).await?;
                create_relation(conn, &blocks_req(c.as_str(), a.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.impact_score(a_id.as_str()), 2);
}

// ---------------------------------------------------------------------------
// Cycle detection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_cycle_in_dag() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let b = create_entity(conn, &task_req("B", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert!(!graph.has_cycle());
    graph.check_no_cycle().unwrap();
}

#[tokio::test]
async fn cycle_detected_returns_error() {
    let store = test_db().await;

    // Create entities and then manually insert a cycle via raw SQL
    // (our app-level validation prevents this, but graph must detect it)
    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let b = create_entity(conn, &task_req("B", 1)).await?;
                // A blocks B
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                // B blocks A (creates cycle via raw SQL to bypass app validation)
                sqlx::query(
                    "INSERT INTO relations (id, source_id, target_id, relation_type, created_at)
                     VALUES ('cycle-rel', ?, ?, 'blocks', '2024-01-01T00:00:00Z')",
                )
                .bind(b.as_str())
                .bind(a.as_str())
                .execute(conn)
                .await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert!(graph.has_cycle());
    let err = graph.check_no_cycle().unwrap_err();
    assert!(matches!(err, FilamentError::CycleDetected { .. }));
}

// ---------------------------------------------------------------------------
// Context query (BFS)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_summaries_within_hops() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let b = create_entity(conn, &task_req("B", 1)).await?;
                let c = create_entity(conn, &task_req("C", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &blocks_req(b.as_str(), c.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let summaries = graph.context_summaries(a_id.as_str(), 1);
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].contains('B'));
}

// ---------------------------------------------------------------------------
// Context query follows both directions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn context_traverses_incoming_edges() {
    let store = test_db().await;

    let (a_id, b_id) = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("Upstream", 1)).await?;
                let b = create_entity(conn, &task_req("Downstream", 1)).await?;
                // Upstream blocks Downstream (edge from A to B)
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok((a, b))
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    // Query context around Downstream — should find Upstream via incoming edge
    let summaries = graph.context_summaries(b_id.as_str(), 1);
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].contains("Upstream"));

    // Also verify the outgoing direction still works
    let summaries = graph.context_summaries(a_id.as_str(), 1);
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].contains("Downstream"));
}

// ---------------------------------------------------------------------------
// critical_path safety with cycles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn critical_path_safe_with_cycle() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let b = create_entity(conn, &task_req("B", 1)).await?;
                // A blocks B
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                // B blocks A (cycle via raw SQL)
                sqlx::query(
                    "INSERT INTO relations (id, source_id, target_id, relation_type, created_at)
                     VALUES ('cycle-rel', ?, ?, 'blocks', '2024-01-01T00:00:00Z')",
                )
                .bind(b.as_str())
                .bind(a.as_str())
                .execute(conn)
                .await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    // Should return a path without stack overflow, even with a cycle
    let path = graph.critical_path(a_id.as_str());
    assert!(!path.is_empty());
    // Path length is bounded (can't loop forever)
    assert!(path.len() <= graph.node_count());
}

// ---------------------------------------------------------------------------
// Node removal
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remove_node_updates_counts() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let a = create_entity(conn, &task_req("A", 1)).await?;
                let _b = create_entity(conn, &task_req("B", 1)).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();
    assert_eq!(graph.node_count(), 2);

    graph.remove_node(a_id.as_str());
    assert_eq!(graph.node_count(), 1);
    assert!(graph.get_node(a_id.as_str()).is_none());
}
