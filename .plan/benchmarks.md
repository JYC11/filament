# Orchestration Tools Benchmark

Comparison of beads_rust and the Flywheel ecosystem against Filament's planned design.

---

## beads_rust (br)

**What it is**: Local-first issue tracker. Single Rust binary. SQLite + JSONL dual persistence.

### Project Layout Lessons

| Aspect | beads_rust | Filament |
|--------|-----------|----------|
| Structure | Single crate, `br` binary | 4-crate workspace |
| Source size | 54K lines Rust, 80K lines tests (1.5x ratio) | TBD |
| SQLite | `fsqlite` (pure-Rust fork) | `sqlx` (standard) |
| Async | None — fully synchronous | tokio throughout |
| Toolchain | Nightly (edition 2024, rust 1.88) | Should target stable |
| Lint config | `unsafe_code = "forbid"`, clippy pedantic+nursery denied | Adopt this |
| Build profile | `opt-level = "z"`, LTO, strip, panic=abort | Adopt for release |

### Technical Patterns Worth Adopting

1. **Agent-friendly structured errors**: Every error variant has a machine-readable code (`ISSUE_NOT_FOUND`, `CYCLE_DETECTED`), retryable flag, hint string, and categorized exit code. `StructuredError` wraps errors for JSON output to AI agents.

2. **Intent detection / fuzzy matching**: `--status wip` → `in_progress`, `--type story` → `feature`. Levenshtein distance for ID typo suggestions. Synonym maps via `LazyLock<HashMap>`.

3. **Blocked cache as materialized view**: `blocked_issues_cache` table stores precomputed blocked status. Rebuilt on dependency/status changes. Avoids expensive recursive queries.

4. **Dirty tracking for incremental sync**: Mutations mark issues in `dirty_issues` table. Auto-flush only exports changed records.

5. **`MutationContext` transaction pattern**: All mutations go through `SqliteStorage::mutate()` — manages transactions, event recording, dirty tracking, cache invalidation. Closure-based API prevents transaction leaks.

6. **Regression tests named after bugs**: `repro_*.rs` files. Self-documenting test history.

7. **JSON Schema derives (`schemars`)**: Every model type derives JSON Schema for agent tool integration.

### Technical Patterns to Avoid

1. **Pure-Rust SQLite (`fsqlite`)**: Immature, 17 sub-crate patches. Use `sqlx` with standard SQLite.
2. **Massive single files**: `sqlite.rs` = 6,114 lines. Split into modules.
3. **36-column issues table**: Compat baggage from Go predecessor. Start lean.
4. **Nightly-only**: `edition = "2024"` requires nightly. Target stable.
5. **No graph library**: Hand-rolled SQL cycle detection. Use petgraph.

### Schema Design Notes

beads_rust uses 10 tables, 20+ indexes. Key insight: the `issues` table CHECK constraint enforces status/closed_at consistency at the DB level:
```sql
CHECK (
    (status = 'closed' AND closed_at IS NOT NULL) OR
    (status = 'tombstone') OR
    (status NOT IN ('closed', 'tombstone') AND closed_at IS NULL)
)
```

Filament should use similar DB-level invariant enforcement on entities/relations.

---

## Flywheel Ecosystem

**What it is**: ~15 interlocking tools for running 20-50 Claude Code agents simultaneously. Not a single binary — a toolbox of independent programs.

### Key Components

| Tool | Language | Role |
|------|----------|------|
| `claude_code_agent_farm` | Python | Spawns agents in tmux panes, monitors heartbeats |
| `mcp_agent_mail` / `_rust` | Python/Rust | Agent messaging, file reservations, identity (MCP server) |
| `beads_rust` + `beads_viewer` | Rust | Task management + graph analytics (PageRank, critical path) |
| `destructive_command_guard` | Rust | Pre-execution safety hook |
| `meta_skill` | Rust | Skill management with hybrid search |
| `flywheel_gateway` | TypeScript | Fleet dashboard, key rotation, WebSocket monitoring |
| `coding_agent_session_search` | Rust | Cross-agent session indexing |

### Architecture: No Single Orchestrator

The ecosystem composes through:
1. **Filesystem** — shared JSON coordination files
2. **MCP protocol** — HTTP-based tool calls (34 tools in agent mail)
3. **SQLite** — shared state per tool
4. **Git** — shared repo + audit trail
5. **tmux** — process management / monitoring

No Unix sockets anywhere. Everything is HTTP or filesystem.

### Critical Design Insights for Filament

#### 1. Agent Death Is Normal, Not An Error
The entire ecosystem assumes agents die constantly (context overflow, crashes, memory wipe). Design implications:
- **No ringleader agents** — no single point of failure
- **Reservations expire on TTL** — crashed agents don't hold resources hostage
- **Identities are semi-persistent** — exist for coordination, can vanish without breaking system

#### 2. Worktrees Are Explicitly Rejected
> "Worktrees demolish development velocity and create debt you need to pay later when the agents diverge."

Instead: advisory file reservations with TTL + pre-commit guard that blocks commits touching reserved files. Conflicts surface early through communication.

#### 3. No Broadcast Messaging
Agents default to broadcasting everything if you let them, burning context on irrelevant messages. The mail model forces agents to address specific recipients. Messages stored in Git archive, not in agent context windows — keeps communication off the token budget.

#### 4. Destructive Command Guard Is Essential
`dcg`: SIMD-accelerated pattern matching, heredoc/inline-script scanning, 49+ security packs, agent-specific trust levels. Fail-open design (never blocks workflow due to timeouts). Filament needs this or should integrate it.

#### 5. MCP Is The Standard Agent Interface
Every tool in the ecosystem exposes MCP tools. This is how agents discover and call into infrastructure. Filament's daemon should expose MCP tools, not just a custom JSON-RPC protocol.

#### 6. Graph-Based Task Intelligence Validates Our Approach
`beads_viewer` computes PageRank, critical path, betweenness centrality, and HITS on task dependency graphs to determine "what should each agent work on next." This is exactly what Filament's unified graph enables natively.

#### 7. Context Window Management Is First-Class
Agent farm monitors context percentage per agent, auto-clears when below threshold (20%). Filament's dispatcher needs context budget awareness.

#### 8. Advisory File Reservations With TTL
Not hard locks — advisory leases that expire. Pre-commit guard provides enforcement at the commit boundary. This is the right abstraction for multi-agent file coordination.

#### 9. Dual Persistence (SQLite + Git)
Both MCP Agent Mail and Meta Skill use this pattern: SQLite for fast queries, Git for audit trail. Neither is privileged. If one corrupts, the other can rebuild. Consider adding Git audit trail to Filament.

#### 10. Settings Corruption At Scale
Running many concurrent Claude Code sessions can corrupt `~/.claude/settings.json`. The farm implements automatic backup/restore with file locking and atomic operations.

#### 11. Prompt-Only Coordination Works
The cooperating agents mode uses NO code enforcement — the LLM reads/writes JSON files in `/coordination/`. This suggests that for capable models, coordination behavior can come from instruction alone without complex infrastructure.

---

## Implications for Filament

### Design Changes to Consider

| Area | Current Plan | Suggested Change | Rationale |
|------|-------------|-----------------|-----------|
| Agent interface | Custom JSON-RPC only | Add MCP server | Ecosystem standard for agent tooling |
| Worktrees | Optional (`--worktree`) | Drop in favor of file reservations | Flywheel's experience shows they cause more harm than good |
| File coordination | Not planned | Add advisory file reservations with TTL | Essential for multi-agent file safety |
| Safety | Not planned | Integrate or build destructive command guard | Essential infrastructure for agent dispatching |
| Error design | `anyhow` | Structured errors with codes, hints, retryable flags | Agent-friendly error handling (from beads_rust) |
| Context management | Not planned | Track agent context budget, auto-clear | Agents die from context overflow regularly |
| Persistence | SQLite only | Consider SQLite + Git audit trail | Resilience and auditability |
| Messaging | Broadcast possible | Default to targeted messaging only | Prevent context pollution |
| Agent resilience | Not specified | Design for agent death: TTL leases, no ringleaders | Core design principle from Flywheel |
| Lint config | Not specified | `unsafe_code = "forbid"`, clippy pedantic | From beads_rust |
| Schema | Not specified | DB-level CHECK constraints on status invariants | From beads_rust |

### What Filament Does Better

1. **Unified graph model** — tasks, agents, knowledge, messages all as typed nodes with typed edges. Neither beads_rust (flat issues) nor Flywheel (separate tools per concern) achieve this.
2. **Single binary with daemon mode** — vs Flywheel's 15-tool toolbox approach. Simpler deployment.
3. **Structured agent output protocol** — `AgentResult` JSON vs Flywheel's "observe via heartbeat/tmux" approach. More reliable result collection.
4. **In-memory graph traversal** — petgraph vs hand-rolled SQL queries. Better for complex graph operations (context queries, dependency analysis).
5. **Async-native** — tokio throughout vs beads_rust's synchronous-only design. Required for concurrent agent management.
