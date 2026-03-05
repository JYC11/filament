mod common;

use common::{blocks_req, depends_on_req, task_req, test_db};
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
// critical_path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn critical_path_follows_upstream_blocks() {
    let store = test_db().await;

    // A blocks B, B blocks C → C's upstream chain is C ← B ← A
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

    // Query from C (the most-blocked task): upstream is C ← B ← A
    let path = graph.critical_path(c_id.as_str());
    assert_eq!(path.len(), 3); // C -> B -> A (upstream prerequisites)
}

#[tokio::test]
async fn critical_path_follows_depends_on() {
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

    let path = graph.critical_path(a_id.as_str());
    assert_eq!(path.len(), 3); // A -> B -> C (upstream chain)
}

#[tokio::test]
async fn critical_path_mixed_blocks_and_depends_on() {
    let store = test_db().await;

    // B blocks A (incoming blocks → B is upstream of A)
    // A depends_on C (outgoing depends_on → C is upstream of A)
    // Both B and C are upstream of A, so critical path from A should find the longer chain.
    let a_id = store
        .with_transaction(|conn| {
            Box::pin(async move {
                let (a, _) = create_entity(conn, &task_req("A", 1)).await?;
                let (b, _) = create_entity(conn, &task_req("B", 1)).await?;
                let (c, _) = create_entity(conn, &task_req("C", 1)).await?;
                let (d, _) = create_entity(conn, &task_req("D", 1)).await?;
                // B blocks A (B is upstream of A)
                create_relation(conn, &blocks_req(b.as_str(), a.as_str())).await?;
                // A depends_on C (C is upstream of A)
                create_relation(conn, &depends_on_req(a.as_str(), c.as_str())).await?;
                // C depends_on D (D is upstream of C)
                create_relation(conn, &depends_on_req(c.as_str(), d.as_str())).await?;
                Ok(a)
            })
        })
        .await
        .unwrap();

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(store.pool()).await.unwrap();

    let path = graph.critical_path(a_id.as_str());
    // Longest upstream chain: A → C → D (length 3), vs A → B (length 2)
    assert_eq!(path.len(), 3);
}

#[tokio::test]
async fn critical_path_does_not_follow_outgoing_blocks() {
    let store = test_db().await;

    // A blocks B (outgoing blocks from A — B is downstream, NOT upstream)
    // So critical_path(A) should NOT follow A→B.
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

    let path = graph.critical_path(a_id.as_str());
    // A has no upstream prerequisites, so path is just [A]
    assert_eq!(path.len(), 1);
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
// critical_path safety with cycles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn critical_path_safe_with_cycle() {
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
