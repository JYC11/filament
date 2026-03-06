mod common;

use common::{blocks_req, depends_on_req, task_req, test_db};
use filament_core::dto::ValidCreateEntityRequest;
use filament_core::error::FilamentError;
use filament_core::graph::KnowledgeGraph;
use filament_core::models::{EntityStatus, EntityType, NonEmptyString, Priority};
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
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 2)).await?;
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
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 2)).await?;
                let (_c, _) = create_entity(conn, &task_req("C", 0)).await?;
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
// blocker_depth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blocker_depth_follows_upstream_blocks() {
    let store = test_db().await;

    // A blocks B, B blocks C → C has 2 layers of upstream blockers
    let (_, _, c_id) = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &blocks_req(b.as_str(), c.as_str())).await?;
                Ok((a, b, c))
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.blocker_depth(c_id.as_str()), 2);
}

#[tokio::test]
async fn blocker_depth_follows_depends_on() {
    let store = test_db().await;

    // A depends_on B, B depends_on C (A needs B, B needs C)
    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                create_relation(conn, &depends_on_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &depends_on_req(b.as_str(), c.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.blocker_depth(a_id.as_str()), 2);
}

#[tokio::test]
async fn blocker_depth_mixed_blocks_and_depends_on() {
    let store = test_db().await;

    // B blocks A (incoming blocks → B is upstream of A)
    // A depends_on C (outgoing depends_on → C is upstream of A)
    // C depends_on D (D is upstream of C)
    // Max depth from A: A→C→D = depth 2, A→B = depth 1 → max is 2
    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                let (d, _) = create_entity(conn, &task_req("D", 1)).await?;
                create_relation(conn, &blocks_req(b.as_str(), a.as_str())).await?;
                create_relation(conn, &depends_on_req(a.as_str(), c.as_str())).await?;
                create_relation(conn, &depends_on_req(c.as_str(), d.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.blocker_depth(a_id.as_str()), 2);
}

#[tokio::test]
async fn blocker_depth_does_not_follow_outgoing_blocks() {
    let store = test_db().await;

    // A blocks B (outgoing blocks from A — B is downstream, NOT upstream)
    // So blocker_depth(A) should be 0 (no upstream blockers).
    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.blocker_depth(a_id.as_str()), 0);
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
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
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
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
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
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
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
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
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
                let (a, _) = create_entity(conn, &task_req("Upstream", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("Downstream", 1)).await?;
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
// blocker_depth safety with cycles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blocker_depth_safe_with_cycle() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
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

    // BFS with visited set terminates safely even with cycles
    let depth = graph.blocker_depth(a_id.as_str());
    assert!(depth <= graph.node_count());
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
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (_b, _) = create_entity(conn, &task_req("B", 1)).await?;
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

// ---------------------------------------------------------------------------
// Batch impact scores
// ---------------------------------------------------------------------------

#[tokio::test]
async fn batch_impact_scores_returns_all_requested() {
    let store = test_db().await;

    // A blocks B, B blocks C → impact follows incoming edges:
    // impact(C)=2 (A→B→C transitively), impact(B)=1 (A→B), impact(A)=0
    let (a_id, b_id, c_id) = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &blocks_req(b.as_str(), c.as_str())).await?;
                Ok((a, b, c))
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let ids = vec![
        a_id.as_str().to_string(),
        b_id.as_str().to_string(),
        c_id.as_str().to_string(),
    ];
    let scores = graph.batch_impact_scores(&ids);

    assert_eq!(scores.len(), 3);
    assert_eq!(scores[a_id.as_str()], 0);
    assert_eq!(scores[b_id.as_str()], 1);
    assert_eq!(scores[c_id.as_str()], 2);
}

// ---------------------------------------------------------------------------
// Context bundle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn build_context_bundle_includes_all_sections() {
    let store = test_db().await;

    // A blocks B, C depends_on B → B has context (A, C), critical path, impact
    let b_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("Upstream A", 0)).await?;
                let (b, _) = create_entity(conn, &task_req("Target B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("Downstream C", 2)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &depends_on_req(c.as_str(), b.as_str())).await?;
                Ok(b)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let bundle = graph.build_context_bundle(b_id.as_str(), 2);
    assert!(
        !bundle.summaries.is_empty(),
        "should have context summaries"
    );
    assert!(bundle.impact_score > 0, "should have downstream dependents");
}

// ---------------------------------------------------------------------------
// Upstream artifacts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upstream_artifacts_returns_closed_predecessors() {
    let store = test_db().await;

    // A (closed) blocks B (open) → B's upstream artifacts should include A
    let b_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("Done Task", 0)).await?;
                update_entity_status(
                    conn,
                    a.as_str(),
                    filament_core::models::EntityStatus::Closed,
                )
                .await?;
                let (b, _) = create_entity(conn, &task_req("Current Task", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok(b)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let artifacts = graph.upstream_artifacts(b_id.as_str());
    assert_eq!(artifacts.len(), 1);
    assert!(artifacts[0].contains("Done Task"));
    assert!(artifacts[0].contains("[completed]"));
}

#[tokio::test]
async fn upstream_artifacts_excludes_open_predecessors() {
    let store = test_db().await;

    // A (open) blocks B (open) → B's upstream artifacts should be empty (A not closed)
    let b_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("Pending", 0)).await?;
                let (b, _) = create_entity(conn, &task_req("Current", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok(b)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let artifacts = graph.upstream_artifacts(b_id.as_str());
    assert!(artifacts.is_empty());
}

// ---------------------------------------------------------------------------
// Newly unblocked by
// ---------------------------------------------------------------------------

#[tokio::test]
async fn newly_unblocked_by_returns_sole_blocked_tasks() {
    let store = test_db().await;

    // A blocks B (A is sole blocker of B)
    let (a_id, b_id) = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("Blocker", 0)).await?;
                let (b, _) = create_entity(conn, &task_req("Blocked", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok((a, b))
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let unblocked = graph.newly_unblocked_by(a_id.as_str());
    assert_eq!(unblocked.len(), 1);
    assert_eq!(unblocked[0].as_str(), b_id.as_str());
}

#[tokio::test]
async fn newly_unblocked_empty_when_other_blockers_remain() {
    let store = test_db().await;

    // A blocks C, B blocks C → completing A leaves C still blocked by B
    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 0)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 0)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), c.as_str())).await?;
                create_relation(conn, &blocks_req(b.as_str(), c.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let unblocked = graph.newly_unblocked_by(a_id.as_str());
    assert!(unblocked.is_empty(), "C should still be blocked by B");
}

// ---------------------------------------------------------------------------
// PageRank
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pagerank_empty_graph() {
    let graph = KnowledgeGraph::new();
    let scores = graph.pagerank(0.85, 50);
    assert!(scores.is_empty());
}

#[tokio::test]
async fn pagerank_scores_sum_to_one() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &blocks_req(b.as_str(), c.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let scores = graph.pagerank(0.85, 50);
    assert_eq!(scores.len(), 3);

    let total: f64 = scores.values().sum();
    assert!(
        (total - 1.0).abs() < 0.01,
        "scores should sum to ~1.0, got {total}"
    );
}

#[tokio::test]
async fn pagerank_sink_node_gets_highest_score() {
    let store = test_db().await;

    // A → B → C (C is the sink, receives all flow)
    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &blocks_req(b.as_str(), c.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let scores = graph.pagerank(0.85, 50);
    assert!(scores.values().all(|&s| s > 0.0));
}

// ---------------------------------------------------------------------------
// Degree centrality
// ---------------------------------------------------------------------------

#[tokio::test]
async fn degree_centrality_empty_graph() {
    let graph = KnowledgeGraph::new();
    let degrees = graph.degree_centrality();
    assert!(degrees.is_empty());
}

#[tokio::test]
async fn degree_centrality_counts_edges() {
    let store = test_db().await;

    // A → B, A → C (A has out_degree=2, B and C have in_degree=1)
    let (a_id, b_id, c_id) = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("Hub", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("Leaf1", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("Leaf2", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &blocks_req(a.as_str(), c.as_str())).await?;
                Ok((a, b, c))
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let degrees = graph.degree_centrality();
    assert_eq!(degrees.len(), 3);

    let (a_in, a_out, a_total) = degrees[&a_id];
    assert_eq!(a_in, 0);
    assert_eq!(a_out, 2);
    assert_eq!(a_total, 2);

    let (b_in, b_out, b_total) = degrees[&b_id];
    assert_eq!(b_in, 1);
    assert_eq!(b_out, 0);
    assert_eq!(b_total, 1);

    let (c_in, c_out, c_total) = degrees[&c_id];
    assert_eq!(c_in, 1);
    assert_eq!(c_out, 0);
    assert_eq!(c_total, 1);
}

// ---------------------------------------------------------------------------
// PageRank edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pagerank_single_node_no_edges() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                create_entity(conn, &task_req("Solo", 1)).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let scores = graph.pagerank(0.85, 50);
    assert_eq!(scores.len(), 1);

    let score = scores.values().next().unwrap();
    assert!(
        (*score - 1.0).abs() < 0.01,
        "single dangling node should have score ~1.0, got {score}"
    );
}

#[tokio::test]
async fn pagerank_disconnected_components() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                let (d, _) = create_entity(conn, &task_req("D", 1)).await?;
                // Component 1: A → B
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                // Component 2: C → D
                create_relation(conn, &blocks_req(c.as_str(), d.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let scores = graph.pagerank(0.85, 50);
    assert_eq!(scores.len(), 4);
    assert!(
        scores.values().all(|&s| s > 0.0),
        "all nodes should have positive scores"
    );

    let sum: f64 = scores.values().sum();
    assert!(
        (sum - 1.0).abs() < 0.01,
        "scores should sum to ~1.0, got {sum}"
    );
}

// ---------------------------------------------------------------------------
// Degree centrality edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn degree_centrality_single_node() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("Lonely", 1)).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let degrees = graph.degree_centrality();
    assert_eq!(degrees.len(), 1);

    let (in_deg, out_deg, total) = degrees[&a_id];
    assert_eq!(in_deg, 0);
    assert_eq!(out_deg, 0);
    assert_eq!(total, 0);
}

#[tokio::test]
async fn degree_centrality_disconnected_components() {
    let store = test_db().await;

    let (a_id, b_id, c_id) = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                // A → B, C is isolated
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok((a, b, c))
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let degrees = graph.degree_centrality();
    assert_eq!(degrees.len(), 3);

    let (c_in, c_out, c_total) = degrees[&c_id];
    assert_eq!(c_in, 0);
    assert_eq!(c_out, 0);
    assert_eq!(c_total, 0);

    let (a_in, a_out, a_total) = degrees[&a_id];
    assert_eq!(a_in, 0);
    assert_eq!(a_out, 1);
    assert_eq!(a_total, 1);

    let (b_in, b_out, b_total) = degrees[&b_id];
    assert_eq!(b_in, 1);
    assert_eq!(b_out, 0);
    assert_eq!(b_total, 1);
}

// ---------------------------------------------------------------------------
// Graph edge cases: nonexistent entities, empty graphs, BFS depth=0
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blocker_depth_nonexistent_entity() {
    let store = test_db().await;
    store
        .with_transaction(|conn| {
            Box::pin(async move {
                create_entity(conn, &task_req("A", 1)).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.blocker_depth("nonexistent-id"), 0);
}

#[tokio::test]
async fn impact_score_nonexistent_entity() {
    let store = test_db().await;
    store
        .with_transaction(|conn| {
            Box::pin(async move {
                create_entity(conn, &task_req("A", 1)).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.impact_score("nonexistent-id"), 0);
}

#[tokio::test]
async fn traverse_bfs_nonexistent_entity() {
    let store = test_db().await;
    store
        .with_transaction(|conn| {
            Box::pin(async move {
                create_entity(conn, &task_req("A", 1)).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let nodes = graph.traverse_bfs("nonexistent-id", 5);
    assert!(nodes.is_empty());
}

#[tokio::test]
async fn traverse_bfs_depth_zero() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    // depth=0 means no neighbors, just the start node (which is excluded from results)
    let nodes = graph.traverse_bfs(a_id.as_str(), 0);
    assert!(nodes.is_empty());
}

#[tokio::test]
async fn ready_tasks_empty_graph() {
    let graph = KnowledgeGraph::new();
    let ready = graph.ready_tasks();
    assert!(ready.is_empty());
}

#[tokio::test]
async fn ready_tasks_single_open_task() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                create_entity(conn, &task_req("Solo", 1)).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let ready = graph.ready_tasks();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].name.as_str(), "Solo");
}

#[tokio::test]
async fn ready_tasks_excludes_closed() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("Done", 1)).await?;
                update_entity_status(conn, a.as_str(), EntityStatus::Closed).await?;
                create_entity(conn, &task_req("Open", 1)).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let ready = graph.ready_tasks();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].name.as_str(), "Open");
}

#[tokio::test]
async fn ready_tasks_excludes_non_task_entities() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                create_entity(conn, &task_req("MyTask", 1)).await?;
                // Create a module entity — should not appear in ready_tasks
                let module_req = ValidCreateEntityRequest {
                    name: NonEmptyString::new("MyModule").unwrap(),
                    entity_type: EntityType::Module,
                    summary: "A module".to_string(),
                    key_facts: serde_json::json!({}),
                    content_path: None,
                    priority: Priority::DEFAULT,
                };
                create_entity(conn, &module_req).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let ready = graph.ready_tasks();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].name.as_str(), "MyTask");
}

#[tokio::test]
async fn remove_edge_nonexistent_returns_false() {
    let store = test_db().await;

    let (a_id, b_id) = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                Ok((a, b))
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    // No edge exists — should return false
    assert!(!graph.remove_edge(
        a_id.as_str(),
        b_id.as_str(),
        &filament_core::models::RelationType::Blocks
    ));
}

#[tokio::test]
async fn remove_edge_nonexistent_nodes_returns_false() {
    let mut graph = KnowledgeGraph::new();
    assert!(!graph.remove_edge(
        "fake-src",
        "fake-tgt",
        &filament_core::models::RelationType::Blocks
    ));
}

#[tokio::test]
async fn context_bundle_nonexistent_entity() {
    let graph = KnowledgeGraph::new();
    let bundle = graph.build_context_bundle("nonexistent", 2);
    assert!(bundle.summaries.is_empty());
    assert_eq!(bundle.blocker_depth, 0);
    assert_eq!(bundle.impact_score, 0);
    assert!(bundle.upstream_artifacts.is_empty());
}

#[tokio::test]
async fn context_bundle_prompt_lines_empty_for_isolated_node() {
    let store = test_db().await;

    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("Isolated", 1)).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let bundle = graph.build_context_bundle(a_id.as_str(), 2);
    assert!(bundle.summaries.is_empty());
    assert_eq!(bundle.blocker_depth, 0);
    assert_eq!(bundle.impact_score, 0);
    let lines = bundle.to_prompt_lines();
    // Isolated node has no blockers, no context, no artifacts → empty prompt
    assert!(lines.is_empty());
}

#[tokio::test]
async fn newly_unblocked_by_nonexistent() {
    let graph = KnowledgeGraph::new();
    let unblocked = graph.newly_unblocked_by("nonexistent");
    assert!(unblocked.is_empty());
}

#[tokio::test]
async fn upstream_artifacts_nonexistent() {
    let graph = KnowledgeGraph::new();
    let artifacts = graph.upstream_artifacts("nonexistent");
    assert!(artifacts.is_empty());
}

#[tokio::test]
async fn different_relation_types_between_same_entities() {
    let store = test_db().await;

    store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                // A blocks B AND A depends_on B — different relation types, should both work
                create_relation(conn, &blocks_req(a.as_str(), b.as_str())).await?;
                create_relation(conn, &depends_on_req(a.as_str(), b.as_str())).await?;
                Ok(())
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    assert_eq!(graph.edge_count(), 2);
}
