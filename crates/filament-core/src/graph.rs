use std::collections::HashMap;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{Bfs, EdgeRef};
use petgraph::Direction;
use sqlx::{Pool, Sqlite};

use crate::error::{FilamentError, Result};
use crate::models::{
    Entity, EntityId, EntityStatus, EntityType, NonEmptyString, Priority, Relation, RelationType,
    Weight,
};

// ---------------------------------------------------------------------------
// Graph node/edge data
// ---------------------------------------------------------------------------

/// Lightweight node data stored in the graph.
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub entity_id: EntityId,
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

        let entities = sqlx::query_as::<_, Entity>("SELECT * FROM entities")
            .fetch_all(pool)
            .await?;

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
        let node = GraphNode {
            entity_id: entity.id.clone(),
            name: entity.name.clone(),
            entity_type: entity.entity_type.clone(),
            status: entity.status.clone(),
            priority: entity.priority,
            summary: entity.summary.clone(),
        };

        if let Some(&idx) = self.index.get(entity.id.as_str()) {
            self.graph[idx] = node;
            idx
        } else {
            let id = entity.id.clone();
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

    /// Remove a node and all its edges.
    pub fn remove_node(&mut self, entity_id: &str) {
        if let Some(idx) = self.index.remove(entity_id) {
            self.graph.remove_node(idx);
            // petgraph may swap indices on removal — rebuild index
            self.rebuild_index();
        }
    }

    /// BFS traversal from a node, returning entities within `max_depth` hops.
    #[must_use]
    pub fn traverse_bfs(&self, entity_id: &str, max_depth: usize) -> Vec<&GraphNode> {
        let Some(&start) = self.index.get(entity_id) else {
            return Vec::new();
        };

        let mut result = Vec::new();
        let mut bfs = Bfs::new(&self.graph, start);
        let mut depth_map: HashMap<NodeIndex, usize> = HashMap::new();
        depth_map.insert(start, 0);

        while let Some(nx) = bfs.next(&self.graph) {
            let depth = depth_map[&nx];
            if depth > max_depth {
                continue;
            }
            if nx != start {
                result.push(&self.graph[nx]);
            }
            // Record depth for neighbors
            for neighbor in self.graph.neighbors(nx) {
                depth_map.entry(neighbor).or_insert(depth + 1);
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
                // Check if any incoming "blocks" edge comes from a non-closed node
                let is_blocked = self
                    .graph
                    .edges_directed(idx, Direction::Incoming)
                    .any(|edge| {
                        edge.weight().relation_type == RelationType::Blocks
                            && self.graph[edge.source()].status != EntityStatus::Closed
                    });
                if is_blocked {
                    return None;
                }
                Some(node)
            })
            .collect();

        tasks.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.name.cmp(&b.name)));
        tasks
    }

    /// Critical path: longest dependency chain from a task to completion.
    /// Returns the chain of entity IDs. Safe against cycles.
    #[must_use]
    pub fn critical_path(&self, entity_id: &str) -> Vec<EntityId> {
        let Some(&start) = self.index.get(entity_id) else {
            return Vec::new();
        };

        // DFS to find the longest path through "blocks"/"depends_on" edges
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
        for edge in self.graph.edges_directed(node, Direction::Outgoing) {
            let etype = &edge.weight().relation_type;
            if *etype == RelationType::Blocks || *etype == RelationType::DependsOn {
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
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}
