use std::collections::HashMap;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use sqlx::{Pool, Sqlite};

use crate::error::{FilamentError, Result};
use crate::models::{
    Entity, EntityId, EntityStatus, EntityType, NonEmptyString, Priority, Relation, RelationType,
    Slug, Weight,
};
use crate::store;

// ---------------------------------------------------------------------------
// Graph node/edge data
// ---------------------------------------------------------------------------

/// Lightweight node data stored in the graph.
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub entity_id: EntityId,
    pub slug: Slug,
    pub name: NonEmptyString,
    pub entity_type: EntityType,
    pub status: EntityStatus,
    pub priority: Priority,
    pub summary: String,
}

/// Lightweight edge data stored in the graph.
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub relation_type: RelationType,
    pub weight: Weight,
}

// ---------------------------------------------------------------------------
// KnowledgeGraph
// ---------------------------------------------------------------------------

/// In-memory graph backed by petgraph, hydrated from `SQLite`.
pub struct KnowledgeGraph {
    graph: DiGraph<GraphNode, GraphEdge>,
    /// Map from entity ID to petgraph `NodeIndex`.
    index: HashMap<EntityId, NodeIndex>,
}

impl KnowledgeGraph {
    /// Create an empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
        }
    }

    /// Hydrate the graph from `SQLite`.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::Database` on SQL failure, or
    /// `FilamentError::EntityNotFound` if a relation references a missing node.
    pub async fn hydrate(&mut self, pool: &Pool<Sqlite>) -> Result<()> {
        self.graph.clear();
        self.index.clear();

        let entities = store::list_entities(pool, None, None).await?;

        for entity in &entities {
            self.add_node_from_entity(entity);
        }

        let relations = sqlx::query_as::<_, Relation>("SELECT * FROM relations")
            .fetch_all(pool)
            .await?;

        for relation in &relations {
            self.add_edge_from_relation(relation)?;
        }

        Ok(())
    }

    /// Add a node from an entity (idempotent — updates if exists).
    pub fn add_node_from_entity(&mut self, entity: &Entity) -> NodeIndex {
        let c = entity.common();
        let node = GraphNode {
            entity_id: c.id.clone(),
            slug: c.slug.clone(),
            name: c.name.clone(),
            entity_type: entity.entity_type(),
            status: c.status,
            priority: c.priority,
            summary: c.summary.clone(),
        };

        if let Some(&idx) = self.index.get(c.id.as_str()) {
            self.graph[idx] = node;
            idx
        } else {
            let id = c.id.clone();
            let idx = self.graph.add_node(node);
            self.index.insert(id, idx);
            idx
        }
    }

    /// Add an edge from a relation.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::EntityNotFound` if either endpoint is missing, or
    /// `FilamentError::Validation` if a duplicate edge (same source, target, type) exists.
    pub fn add_edge_from_relation(&mut self, relation: &Relation) -> Result<()> {
        let source = self
            .index
            .get(relation.source_id.as_str())
            .copied()
            .ok_or_else(|| FilamentError::EntityNotFound {
                id: relation.source_id.to_string(),
            })?;
        let target = self
            .index
            .get(relation.target_id.as_str())
            .copied()
            .ok_or_else(|| FilamentError::EntityNotFound {
                id: relation.target_id.to_string(),
            })?;

        // Reject duplicate edges (same source, target, relation type)
        let has_duplicate = self
            .graph
            .edges_directed(source, Direction::Outgoing)
            .any(|e| e.target() == target && e.weight().relation_type == relation.relation_type);
        if has_duplicate {
            return Err(FilamentError::Validation(format!(
                "duplicate edge: {} -{}-> {}",
                relation.source_id, relation.relation_type, relation.target_id
            )));
        }

        let edge = GraphEdge {
            relation_type: relation.relation_type.clone(),
            weight: relation.weight,
        };
        self.graph.add_edge(source, target, edge);
        Ok(())
    }

    /// Remove an edge by source, target, and relation type.
    /// Returns `true` if an edge was removed, `false` if not found.
    pub fn remove_edge(
        &mut self,
        source_id: &str,
        target_id: &str,
        relation_type: &RelationType,
    ) -> bool {
        let (Some(&src), Some(&tgt)) = (self.index.get(source_id), self.index.get(target_id))
        else {
            return false;
        };

        let edge_id = self
            .graph
            .edges_directed(src, Direction::Outgoing)
            .find(|e| e.target() == tgt && &e.weight().relation_type == relation_type)
            .map(|e| e.id());

        if let Some(id) = edge_id {
            self.graph.remove_edge(id);
            true
        } else {
            false
        }
    }

    /// Remove a node and all its edges.
    pub fn remove_node(&mut self, entity_id: &str) {
        if let Some(idx) = self.index.remove(entity_id) {
            self.graph.remove_node(idx);
            // petgraph may swap indices on removal — rebuild index
            self.rebuild_index();
        }
    }

    /// BFS traversal from a node, returning entities within `max_depth` hops.
    /// Traverses both incoming and outgoing edges (undirected neighborhood).
    #[must_use]
    pub fn traverse_bfs(&self, entity_id: &str, max_depth: usize) -> Vec<&GraphNode> {
        let Some(&start) = self.index.get(entity_id) else {
            return Vec::new();
        };

        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        visited.insert(start);
        queue.push_back((start, 0usize));

        while let Some((nx, depth)) = queue.pop_front() {
            if nx != start {
                result.push(&self.graph[nx]);
            }
            if depth >= max_depth {
                continue;
            }
            // Traverse both directions for context discovery
            for neighbor in self
                .graph
                .neighbors_directed(nx, Direction::Outgoing)
                .chain(self.graph.neighbors_directed(nx, Direction::Incoming))
            {
                if visited.insert(neighbor) {
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }

        result
    }

    /// Context query: return summaries within N hops.
    #[must_use]
    pub fn context_summaries(&self, entity_id: &str, hops: usize) -> Vec<String> {
        self.traverse_bfs(entity_id, hops)
            .iter()
            .map(|n| format!("[{}] {}: {}", n.entity_type, n.name, n.summary))
            .collect()
    }

    /// Get ready tasks: open tasks with no unclosed blockers, sorted by priority.
    #[must_use]
    pub fn ready_tasks(&self) -> Vec<&GraphNode> {
        let mut tasks: Vec<&GraphNode> = self
            .graph
            .node_indices()
            .filter_map(|idx| {
                let node = &self.graph[idx];
                if node.entity_type != EntityType::Task {
                    return None;
                }
                if node.status != EntityStatus::Open && node.status != EntityStatus::InProgress {
                    return None;
                }
                // Check if blocked: incoming "blocks" or outgoing "depends_on" from non-closed node
                let blocked_by_blocks =
                    self.graph
                        .edges_directed(idx, Direction::Incoming)
                        .any(|edge| {
                            edge.weight().relation_type == RelationType::Blocks
                                && self.graph[edge.source()].status != EntityStatus::Closed
                        });
                let blocked_by_depends =
                    self.graph
                        .edges_directed(idx, Direction::Outgoing)
                        .any(|edge| {
                            edge.weight().relation_type == RelationType::DependsOn
                                && self.graph[edge.target()].status != EntityStatus::Closed
                        });
                if blocked_by_blocks || blocked_by_depends {
                    return None;
                }
                Some(node)
            })
            .collect();

        tasks.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.name.cmp(&b.name)));
        tasks
    }

    /// Critical path: longest chain of upstream prerequisites for a task.
    ///
    /// "Upstream" means things that must complete before this entity can proceed:
    /// - Outgoing `DependsOn` edges (A `depends_on` B → B is upstream)
    /// - Incoming `Blocks` edges (B `blocks` A → B is upstream)
    ///
    /// Returns the chain of entity IDs (starting with the given entity).
    /// Safe against cycles.
    #[must_use]
    pub fn critical_path(&self, entity_id: &str) -> Vec<EntityId> {
        let Some(&start) = self.index.get(entity_id) else {
            return Vec::new();
        };

        let mut longest: Vec<EntityId> = Vec::new();
        let mut current: Vec<EntityId> = vec![EntityId::from(entity_id)];
        let mut visited = std::collections::HashSet::new();
        visited.insert(start);
        self.dfs_longest_path(start, &mut current, &mut longest, &mut visited);
        longest
    }

    fn dfs_longest_path(
        &self,
        node: NodeIndex,
        current: &mut Vec<EntityId>,
        longest: &mut Vec<EntityId>,
        visited: &mut std::collections::HashSet<NodeIndex>,
    ) {
        let mut found_dep = false;

        // Follow outgoing DependsOn: A depends_on B → B is upstream
        for edge in self.graph.edges_directed(node, Direction::Outgoing) {
            if edge.weight().relation_type == RelationType::DependsOn {
                let target = edge.target();
                if self.graph[target].status != EntityStatus::Closed && visited.insert(target) {
                    found_dep = true;
                    current.push(self.graph[target].entity_id.clone());
                    self.dfs_longest_path(target, current, longest, visited);
                    current.pop();
                    visited.remove(&target);
                }
            }
        }

        // Follow incoming Blocks: B blocks A → B is upstream of A
        for edge in self.graph.edges_directed(node, Direction::Incoming) {
            if edge.weight().relation_type == RelationType::Blocks {
                let source = edge.source();
                if self.graph[source].status != EntityStatus::Closed && visited.insert(source) {
                    found_dep = true;
                    current.push(self.graph[source].entity_id.clone());
                    self.dfs_longest_path(source, current, longest, visited);
                    current.pop();
                    visited.remove(&source);
                }
            }
        }

        if !found_dep && current.len() > longest.len() {
            longest.clone_from(current);
        }
    }

    /// Impact score: number of transitive dependents (nodes reachable via incoming edges).
    #[must_use]
    pub fn impact_score(&self, entity_id: &str) -> usize {
        let Some(&start) = self.index.get(entity_id) else {
            return 0;
        };

        // Count nodes reachable by following incoming "blocks" edges in reverse
        // i.e., who depends on this entity transitively
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for edge in self.graph.edges_directed(node, Direction::Incoming) {
                if (edge.weight().relation_type == RelationType::Blocks
                    || edge.weight().relation_type == RelationType::DependsOn)
                    && visited.insert(edge.source())
                {
                    stack.push(edge.source());
                }
            }
        }
        visited.len()
    }

    /// Batch impact scores: compute impact for multiple entities at once.
    #[must_use]
    pub fn batch_impact_scores(&self, entity_ids: &[String]) -> HashMap<String, usize> {
        entity_ids
            .iter()
            .map(|id| (id.clone(), self.impact_score(id)))
            .collect()
    }

    /// Compute `PageRank` scores for all nodes using power iteration.
    ///
    /// Returns a map of entity ID → score (scores sum to ~1.0).
    /// `damping` is the damping factor (typically 0.85).
    /// `iterations` is the number of power-iteration rounds (typically 20–100).
    ///
    /// # Panics
    ///
    /// Panics if the internal graph state is inconsistent (edge target missing from node set).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn pagerank(&self, damping: f64, iterations: usize) -> HashMap<EntityId, f64> {
        let n = self.graph.node_count();
        if n == 0 {
            return HashMap::new();
        }

        let n_f = n as f64;
        let mut scores: HashMap<NodeIndex, f64> = self
            .graph
            .node_indices()
            .map(|idx| (idx, 1.0 / n_f))
            .collect();

        for _ in 0..iterations {
            let mut next_scores: HashMap<NodeIndex, f64> = self
                .graph
                .node_indices()
                .map(|idx| (idx, (1.0 - damping) / n_f))
                .collect();

            for idx in self.graph.node_indices() {
                let out_degree = self.graph.edges_directed(idx, Direction::Outgoing).count();
                if out_degree == 0 {
                    // Dangling node: distribute evenly
                    let share = scores[&idx] * damping / n_f;
                    for val in next_scores.values_mut() {
                        *val += share;
                    }
                } else {
                    let share = scores[&idx] * damping / out_degree as f64;
                    for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                        *next_scores.get_mut(&edge.target()).expect("node exists") += share;
                    }
                }
            }

            scores = next_scores;
        }

        scores
            .into_iter()
            .map(|(idx, score)| (self.graph[idx].entity_id.clone(), score))
            .collect()
    }

    /// Compute degree centrality for all nodes.
    ///
    /// Returns a map of entity ID → `(in_degree, out_degree, total_degree)`.
    #[must_use]
    pub fn degree_centrality(&self) -> HashMap<EntityId, (usize, usize, usize)> {
        self.graph
            .node_indices()
            .map(|idx| {
                let in_deg = self.graph.edges_directed(idx, Direction::Incoming).count();
                let out_deg = self.graph.edges_directed(idx, Direction::Outgoing).count();
                (
                    self.graph[idx].entity_id.clone(),
                    (in_deg, out_deg, in_deg + out_deg),
                )
            })
            .collect()
    }

    /// Check for cycles in the graph.
    #[must_use]
    pub fn has_cycle(&self) -> bool {
        petgraph::algo::is_cyclic_directed(&self.graph)
    }

    /// Detect cycle and return the path if one exists.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::CycleDetected` with the cycle path if a cycle is found.
    pub fn check_no_cycle(&self) -> Result<()> {
        if let Err(cycle) = petgraph::algo::toposort(&self.graph, None) {
            let node = &self.graph[cycle.node_id()];
            return Err(FilamentError::CycleDetected {
                path: format!("cycle involving: {}", node.name),
            });
        }
        Ok(())
    }

    /// Number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Get a node by entity ID.
    #[must_use]
    pub fn get_node(&self, entity_id: &str) -> Option<&GraphNode> {
        self.index.get(entity_id).map(|&idx| &self.graph[idx])
    }

    /// Rebuild the index after a node removal (petgraph swaps indices).
    fn rebuild_index(&mut self) {
        self.index.clear();
        for idx in self.graph.node_indices() {
            self.index.insert(self.graph[idx].entity_id.clone(), idx);
        }
    }

    /// Walk upstream (`DependsOn` targets + `Blocks` sources) and collect summaries
    /// of Closed tasks. These represent completed predecessor results.
    #[must_use]
    pub fn upstream_artifacts(&self, entity_id: &str) -> Vec<String> {
        let Some(&start) = self.index.get(entity_id) else {
            return Vec::new();
        };

        let mut results = Vec::new();
        let mut visited = std::collections::HashSet::new();
        visited.insert(start);
        let mut stack = vec![start];

        while let Some(node) = stack.pop() {
            // Outgoing DependsOn: this node depends_on target → target is upstream
            for edge in self.graph.edges_directed(node, Direction::Outgoing) {
                if edge.weight().relation_type == RelationType::DependsOn
                    && visited.insert(edge.target())
                {
                    let target = &self.graph[edge.target()];
                    if target.status == EntityStatus::Closed {
                        results.push(format!("[completed] {}: {}", target.name, target.summary));
                    }
                    stack.push(edge.target());
                }
            }
            // Incoming Blocks: source blocks this node → source is upstream
            for edge in self.graph.edges_directed(node, Direction::Incoming) {
                if edge.weight().relation_type == RelationType::Blocks
                    && visited.insert(edge.source())
                {
                    let source = &self.graph[edge.source()];
                    if source.status == EntityStatus::Closed {
                        results.push(format!("[completed] {}: {}", source.name, source.summary));
                    }
                    stack.push(edge.source());
                }
            }
        }

        results
    }

    /// Build a rich context bundle for an entity, combining neighborhood context,
    /// critical path, impact score, and upstream artifact summaries.
    #[must_use]
    pub fn build_context_bundle(&self, entity_id: &str, depth: usize) -> ContextBundle {
        ContextBundle {
            summaries: self.context_summaries(entity_id, depth),
            critical_path: self.critical_path_names(entity_id),
            impact_score: self.impact_score(entity_id),
            upstream_artifacts: self.upstream_artifacts(entity_id),
        }
    }

    /// Critical path but return entity names instead of IDs (for prompts).
    fn critical_path_names(&self, entity_id: &str) -> Vec<String> {
        self.critical_path(entity_id)
            .into_iter()
            .filter_map(|id| self.get_node(id.as_str()).map(|n| n.name.to_string()))
            .collect()
    }

    /// Find tasks that become newly unblocked when `completed_entity_id` is completed.
    ///
    /// Returns entity IDs of Open tasks whose only remaining blocker was the given entity.
    #[must_use]
    pub fn newly_unblocked_by(&self, completed_entity_id: &str) -> Vec<EntityId> {
        let Some(&completed_idx) = self.index.get(completed_entity_id) else {
            return Vec::new();
        };

        let mut unblocked = Vec::new();

        // Find tasks that depend on the completed entity:
        // 1. Outgoing Blocks from completed → target is potentially unblocked
        for edge in self
            .graph
            .edges_directed(completed_idx, Direction::Outgoing)
        {
            if edge.weight().relation_type == RelationType::Blocks {
                let target = edge.target();
                if self.is_newly_unblocked(target, completed_idx) {
                    unblocked.push(self.graph[target].entity_id.clone());
                }
            }
        }

        // 2. Incoming DependsOn to completed → source depends on us
        for edge in self
            .graph
            .edges_directed(completed_idx, Direction::Incoming)
        {
            if edge.weight().relation_type == RelationType::DependsOn {
                let source = edge.source();
                if self.is_newly_unblocked(source, completed_idx) {
                    unblocked.push(self.graph[source].entity_id.clone());
                }
            }
        }

        unblocked
    }

    /// Check if a node would be unblocked if `just_completed` were closed.
    /// The node must be Open and have no other non-closed blockers.
    fn is_newly_unblocked(&self, node: NodeIndex, just_completed: NodeIndex) -> bool {
        let n = &self.graph[node];
        if n.entity_type != EntityType::Task || n.status != EntityStatus::Open {
            return false;
        }

        // Check all blockers except `just_completed` — must all be closed
        for edge in self.graph.edges_directed(node, Direction::Incoming) {
            if edge.weight().relation_type == RelationType::Blocks {
                let src = edge.source();
                if src != just_completed && self.graph[src].status != EntityStatus::Closed {
                    return false;
                }
            }
        }
        for edge in self.graph.edges_directed(node, Direction::Outgoing) {
            if edge.weight().relation_type == RelationType::DependsOn {
                let tgt = edge.target();
                if tgt != just_completed && self.graph[tgt].status != EntityStatus::Closed {
                    return false;
                }
            }
        }

        true
    }
}

// ---------------------------------------------------------------------------
// Context bundle
// ---------------------------------------------------------------------------

/// Rich context for a dispatched agent — combines multiple graph queries.
#[derive(Debug, Clone)]
pub struct ContextBundle {
    pub summaries: Vec<String>,
    pub critical_path: Vec<String>,
    pub impact_score: usize,
    pub upstream_artifacts: Vec<String>,
}

impl ContextBundle {
    /// Format as prompt lines for injection into agent system prompt.
    #[must_use]
    pub fn to_prompt_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();

        if !self.summaries.is_empty() {
            lines.push("--- CONTEXT ---".to_string());
            for s in &self.summaries {
                lines.push(s.clone());
            }
        }

        if !self.critical_path.is_empty() {
            lines.push("--- CRITICAL PATH ---".to_string());
            lines.push(self.critical_path.join(" → "));
        }

        if !self.upstream_artifacts.is_empty() {
            lines.push("--- UPSTREAM RESULTS ---".to_string());
            for a in &self.upstream_artifacts {
                lines.push(a.clone());
            }
        }

        if self.impact_score > 0 {
            lines.push(format!(
                "Impact: {} downstream dependents",
                self.impact_score
            ));
        }

        lines
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}
