# Knowledge Graph for Claude Code — Design Plan

## Overview

A Rust-based knowledge graph service (`kg-server`) exposed over a Unix socket, designed for concurrent access by multiple LLM agents. A thin CLI client (`kg`) forwards commands to the daemon. Stores entities and relationships with a **three-tier content model** to balance context richness against token cost during graph traversal.

---

## Architecture

```
┌──────────┐  ┌──────────┐  ┌──────────┐
│  Agent A  │  │  Agent B  │  │  Human   │
│  (Claude) │  │  (Claude) │  │  (CLI)   │
└─────┬────┘  └─────┬────┘  └─────┬────┘
      │             │             │
      │   JSON over Unix socket   │
      └──────┬──────┴──────┬──────┘
             ▼             ▼
      ┌──────────────────────────┐
      │        kg-server         │
      │                          │
      │  ┌────────────────────┐  │
      │  │ KnowledgeStore     │  │  ← trait, all operations go through here
      │  │  ┌──────────────┐  │  │
      │  │  │ petgraph     │  │  │  ← single shared in-memory graph
      │  │  │ StableGraph  │  │  │
      │  │  └──────────────┘  │  │
      │  │  ┌──────────────┐  │  │
      │  │  │ SQLite (WAL) │  │  │  ← source of truth
      │  │  └──────────────┘  │  │
      │  └────────────────────┘  │
      └──────────────────────────┘
```

### Project Layout

```
kg/
├── Cargo.toml
├── kg-server/          # daemon binary
│   └── src/
│       ├── main.rs
│       └── server.rs
├── kg-cli/             # thin client binary
│   └── src/
│       └── main.rs
├── kg-core/            # shared library
│   └── src/
│       ├── lib.rs
│       ├── store.rs        # KnowledgeStore trait + impl
│       ├── graph.rs        # petgraph wrapper, hydration, traversal
│       ├── models.rs       # Entity, Relation, request/response types
│       └── protocol.rs     # JSON-RPC message types
└── migrations/
    └── 001_init.sql
```

### On-Disk Layout

```
.kg/
├── knowledge.db
├── kg.sock             # Unix socket (created by daemon)
├── kg.pid              # PID file for daemon management
└── content/
    ├── auth-service.md
    └── database-pool.md
```

---

## Three-Tier Content Model

| Tier | Field | Size | Loaded During | Purpose |
|------|-------|------|---------------|---------|
| 1 | `summary` | 1-3 sentences | Every traversal | Relevance routing |
| 2 | `key_facts` | 5-20 JSON k/v pairs | After relevance filtering | LLM reasoning |
| 3 | `content_path` → file | Unbounded | Explicit deep dive | Full reference material |

**Traversal-then-select pattern:**

1. `kg context --around "auth" --depth 2` → Tier 1 summaries (~50-100 tokens/node)
2. `kg inspect "AuthService" "JWTConfig"` → Tier 2 key_facts (~200-500 tokens/node)
3. `kg read "AuthService"` → Tier 3 full content (unbounded, single node)

---

## DDL (SQLite)

```sql
CREATE TABLE IF NOT EXISTS entities (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    entity_type  TEXT NOT NULL,
    summary      TEXT NOT NULL,
    key_facts    TEXT NOT NULL DEFAULT '{}',
    content_path TEXT,
    content_hash TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS relations (
    id             INTEGER PRIMARY KEY,
    source_id      INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    target_id      INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation_type  TEXT NOT NULL,
    weight         REAL NOT NULL DEFAULT 1.0,
    summary        TEXT,
    metadata       TEXT NOT NULL DEFAULT '{}',
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(source_id, target_id, relation_type)
);

CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id);
CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);
```

---

## Crates

### kg-core (shared library)

| Crate | Purpose |
|-------|---------|
| `sqlx` (sqlite, runtime-tokio) | Async SQLite access |
| `petgraph` | In-memory graph traversal (StableGraph) |
| `tokio` | Async runtime, RwLock for shared graph |
| `serde` + `serde_json` | Serialization, JSON protocol |
| `anyhow` | Error handling |
| `slug` | Content filename generation |
| `blake3` | Content file change detection |
| `strsim` | Fuzzy name matching (optional) |

### kg-server (daemon)

| Crate | Purpose |
|-------|---------|
| `tokio` (full, net, signal) | Unix socket listener, graceful shutdown |
| `tracing` + `tracing-subscriber` | Structured logging |
| `daemonize` | Optional background daemonization |

### kg-cli (thin client)

| Crate | Purpose |
|-------|---------|
| `clap` (derive) | CLI argument parsing |
| `tokio` | Async runtime for socket client |

---

## Core Data Structures

```rust
// --- models.rs ---

// SQLite row types
#[derive(Debug, Clone, FromRow)]
struct EntityRow { id, name, entity_type, summary, key_facts, content_path, content_hash, ... }

#[derive(Debug, Clone, FromRow)]
struct RelationRow { id, source_id, target_id, relation_type, weight, summary, metadata, ... }

// Petgraph node/edge weights
struct EntityNode { db_id: i64, name: String, entity_type: String, summary: String }
struct RelationEdge { db_id: i64, relation_type: String, weight: f64, summary: Option<String> }

// --- graph.rs ---

struct KnowledgeGraph {
    graph: StableGraph<EntityNode, RelationEdge, Directed>,
    db_to_node: HashMap<i64, NodeIndex>,
    node_to_db: HashMap<NodeIndex, i64>,
}

// --- store.rs ---

// Wraps the graph + db behind a lock for concurrent access
struct KnowledgeStore {
    graph: tokio::sync::RwLock<KnowledgeGraph>,
    pool: SqlitePool,
    content_dir: PathBuf,
}
```

---

## Concurrency Model

The daemon owns a single `KnowledgeStore`. Concurrent access is managed with `tokio::sync::RwLock` over the in-memory graph:

```rust
impl KnowledgeStore {
    // Reads take a read lock — multiple concurrent readers allowed
    async fn context(&self, around: &str, depth: usize) -> Result<Vec<EntitySummary>> {
        let graph = self.graph.read().await;
        // ... traverse graph ...
    }

    // Writes take a write lock — exclusive access
    async fn add_entity(&self, req: AddEntityRequest) -> Result<Entity> {
        // 1. Write to SQLite first (source of truth)
        let row = sqlx::query_as(/* upsert */)
            .fetch_one(&self.pool).await?;

        // 2. Update in-memory graph under write lock
        let mut graph = self.graph.write().await;
        graph.upsert_node(row);

        Ok(entity)
    }
}
```

**Properties:**
- Multiple agents can read/traverse concurrently (read lock)
- Writes are serialized (write lock) — no lost updates, no dirty reads
- SQLite is always written first — if the process crashes, the graph is rebuilt from SQLite on next startup
- No cross-process locking needed since there's a single daemon process

---

## Protocol (JSON over Unix Socket)

Newline-delimited JSON (JSON-RPC style) over `.kg/kg.sock`:

### Request

```json
{
    "id": "req_001",
    "method": "add_entity",
    "params": {
        "name": "AuthService",
        "entity_type": "module",
        "summary": "Handles JWT auth for all API endpoints",
        "key_facts": { "lang": "rust", "entry": "src/auth/mod.rs" },
        "content": "./docs/auth_design.md"
    }
}
```

### Response

```json
{
    "id": "req_001",
    "result": {
        "id": 1,
        "name": "AuthService",
        "entity_type": "module",
        "summary": "Handles JWT auth for all API endpoints",
        "created": true
    }
}
```

### Error

```json
{
    "id": "req_001",
    "error": {
        "code": "NOT_FOUND",
        "message": "Entity 'FooService' not found"
    }
}
```

### Methods

| Method | Params | Returns | Lock |
|--------|--------|---------|------|
| `add_entity` | name, entity_type, summary, key_facts?, content? | Entity | Write |
| `remove_entity` | name | bool | Write |
| `update_entity` | name, summary?, key_facts?, content? | Entity | Write |
| `add_relation` | source, target, relation_type, summary?, weight? | Relation | Write |
| `remove_relation` | source, target, relation_type | bool | Write |
| `context` | around, depth, limit? | EntitySummary[] | Read |
| `inspect` | names[] | EntityDetail[] | Read |
| `read` | name | EntityFull | Read |
| `list` | entity_type? | EntitySummary[] | Read |
| `export` | — | full graph JSON | Read |
| `import` | entities[] | ImportResult | Write |

---

## CLI (Thin Client)

The CLI connects to `.kg/kg.sock`, sends a JSON request, prints the JSON response to stdout:

```bash
# Daemon management
kg-server start                # start daemon, create .kg/ if needed
kg-server stop                 # graceful shutdown
kg-server status               # check if running

# Entity CRUD (forwarded to daemon)
kg add <name> --type <type> --summary "..." [--facts '{}'] [--content ./path.md]
kg remove <name>
kg update <name> [--summary "..."] [--facts '{}'] [--content ./path.md]

# Relations
kg relate <source> <relation_type> <target> [--summary "..."] [--weight 1.0]
kg unrelate <source> <relation_type> <target>

# Querying (Tier 1 — summaries)
kg context --around <name> --depth <N> [--limit 20]
kg list [--type <type>]

# Inspection (Tier 2 — key_facts)
kg inspect <name> [<name2> ...]

# Deep dive (Tier 3 — full content)
kg read <name>

# Maintenance
kg export
kg import < entities.jsonl
```

All output is JSON to stdout. Errors to stderr. The CLI auto-starts the daemon if `.kg/kg.sock` is not found.

---

## Key Design Decisions

1. **Daemon-first architecture.** A long-running `kg-server` owns the database and in-memory graph. All access goes through the daemon via Unix socket. This ensures a single consistent view of the graph for all clients.

2. **Thin CLI client.** `kg` is a lightweight wrapper that serializes commands to JSON, sends them over the Unix socket, and prints the response. No graph logic, no database access.

3. **`KnowledgeStore` trait boundary.** All graph operations go through a single trait. This keeps the protocol layer decoupled from storage logic and makes testing straightforward.

4. **`tokio::sync::RwLock` for concurrency.** Multiple readers can traverse concurrently. Writes are serialized. No external locking infrastructure needed.

5. **SQLite is source of truth.** Writes persist to SQLite before updating the in-memory graph. On startup, the daemon hydrates petgraph from SQLite. If the process crashes, no data is lost.

6. **Three-tier content model.** Summaries enable cheap traversal; key_facts enable reasoning; content files hold full reference material. The LLM decides when to go deeper.

7. **Idempotent writes (upsert).** `add_entity` and `add_relation` use `INSERT ... ON CONFLICT DO UPDATE`. Agents may re-add entities across sessions without error.

8. **Edge summaries.** Relations carry a `summary` field so the LLM understands *why* a relationship exists, not just that it does.

9. **WAL journal mode.** SQLite is configured for WAL mode for improved read concurrency alongside the daemon's write serialization.

10. **StableGraph over Graph.** Node/edge indices remain valid after deletions, which is required for the bidirectional ID map.

11. **Auto-start daemon.** The CLI checks for `.kg/kg.sock` and starts the daemon automatically if needed, so agents don't need to manage lifecycle explicitly.

12. **Newline-delimited JSON protocol.** Simple, debuggable, no protobuf compilation step. Upgradeable to gRPC later since the `KnowledgeStore` trait isolates the protocol layer.

---

## Future Considerations

- **Change notifications.** Agents subscribe to entity/relation changes over the socket. Enables reactive workflows where Agent B is notified when Agent A modifies a shared entity.
- **Event log table.** Append-only log of all mutations for auditability and conflict resolution.
- **Conflict policies.** Configurable per-field merge strategies (last-write-wins, agent priority, manual review).
- **Semantic search.** Embeddings stored as BLOBs on entities, with cosine similarity for `kg context --semantic "authentication flow"`.
- **gRPC upgrade.** Swap JSON-over-socket for tonic/gRPC if throughput or type safety becomes a bottleneck.
