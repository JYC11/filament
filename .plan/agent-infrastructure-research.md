# Agent Infrastructure Research

Date: 2026-03-23

Research into agent isolation, codebase knowledge graphs, runtime guardrails, and portable agent environment packaging.

---

## 1. VibeBox — Per-Project Agent Sandbox

**Repo:** `/Users/admin/Desktop/code/vibebox` (local)
**What:** Rust CLI that spins up per-project Linux microVMs on macOS Apple Silicon via Apple's Virtualization Framework. Hard VM boundary for coding agents.
**Version:** 0.3.2 (crates.io), MIT license

### How It Works

- `cd my-project && vibebox` — boots Debian 13 guest with explicit mount allowlists from `vibebox.toml`
- Two binaries: `vibebox` (user CLI) + `vibebox-supervisor` (background daemon managing VM lifecycle)
- VirtioFS for file sharing (only declared mounts), NAT networking (outbound only), SSH for terminal access
- Multi-attach: multiple terminals share one running VM, reference-counted auto-shutdown (default 20s idle)
- Warm re-entry <5s on M3 Mac; first boot provisions build-essential, openssh, mise, claude-code, codex

### Architecture

```
vibebox (CLI) ──spawn──> vibebox-supervisor (daemon)
     │                         │
     │ Unix socket             │ Apple VZ Framework
     │ (.vibebox/vm.sock)      │ (objc2-virtualization)
     │                         │
     └──SSH──> Linux guest ◄───┘
               (Debian 13 arm64)
               VirtioFS mounts
               NAT network
```

Key source files:
- `src/vm.rs` — Apple VZ API usage, disk management, mount wiring, serial I/O (1,348 lines)
- `src/vm_manager.rs` — supervisor daemon, auto-shutdown event loop, Unix socket
- `src/instance.rs` — SSH keypair, instance.toml, SSH session execution
- `src/config.rs` — vibebox.toml schema, validation, config path boundary enforcement
- `src/provision.sh` — one-time guest provisioning (packages, SSH hardening)
- `src/ssh.sh` — per-session guest setup (user creation, mise install, IP discovery)

### File Layout

```
<project>/
├── vibebox.toml               # per-project VM config
└── .vibebox/
    ├── instance.toml          # id, ssh_user, sudo_password, vm_ipv4
    ├── instance.raw           # VM disk image
    ├── ssh_key / ssh_key.pub  # ed25519 keypair
    ├── vm.sock                # Unix socket (supervisor <-> CLI)
    ├── vm.pid / vm.lock       # process management
    └── *.log                  # CLI + supervisor logs

~/.cache/vibebox/
├── debian-13-nocloud-arm64-*.tar.xz   # base image (SHA-512 verified)
├── default.raw                         # provisioned base (shared across projects)
└── .guest-mise-cache/                  # mise cache shared across VMs
```

### Security Model

- Full Linux guest kernel via Apple Virtualization Framework (hypervisor boundary)
- Network: NAT only (outbound internet, no inbound, no host LAN access)
- Filesystem: only explicitly declared VirtioFS mounts; `.git/` masked with tmpfs
- Guest user `vibecoder` (not root for normal SSH sessions)

### Stack

Rust (edition 2024, stable 1.91.1+), objc2 + objc2-virtualization (Apple VZ), clap, ratatui, serde/toml, tracing, libc

### Limitations

- macOS Apple Silicon only (cross-platform QEMU backend in progress: `.plan/cross-platform-vm-backend.md`)
- No `vibebox stop` command (must `vibebox reset` to wipe)
- No snapshot/rollback
- SSH user always `vibecoder` (not configurable)
- disk_gb only on first init (change requires reset)
- `.git` tmpfs mask not fully secure (guest root can unmount)
- `mock-vm` feature flag for CI testing without hardware virtualization

### Comparison

| Approach | Isolation | Setup Cost | Startup | Use Case |
|----------|-----------|------------|---------|----------|
| vibebox | Full VM (hypervisor) | vibebox.toml | <5s warm | Daily driver agent sandbox |
| Docker | Shared kernel | Dockerfile | <1s | CI, batch, non-adversarial |
| railguard | Policy interception | railguard.yaml | 0ms (hook) | Same-host guardrails |
| sandbox-exec | Process syscall filter | .sb profile | 0ms | Limited, macOS only |
| gVisor | Kernel emulation | runsc config | ~1s | Cloud workloads |

---

## 2. Dgraph — Distributed Graph Database

**Repo:** https://github.com/dgraph-io/dgraph
**What:** Open-source distributed graph database with native GraphQL + DQL query support.
**Version:** 24.x (calendar versioning), Apache 2.0 (community) / BSL (enterprise)

### Architecture

- **Storage:** Badger (own Go LSM key-value store, SSD-optimized)
- **Data model:** Predicate-based (not property graph). Predicates typed globally. Posting lists for edge traversal.
- **Distribution:** Alpha nodes (data + queries) + Zero nodes (cluster coordination). Raft consensus per shard group.
- **Transactions:** ACID with MVCC, strongly consistent within shard group. Linearizable reads.
- **Query:** Dual interface — standard GraphQL (`/graphql`) + DQL for power queries (recursive, shortest-path, aggregation)

### API Surface

| Interface | Details |
|-----------|---------|
| GraphQL | `/graphql` — schema-defined, subscription support, spec-compliant |
| DQL | `/query`, `/mutate`, `/commit` — recursive, shortest-path, aggregation |
| gRPC | Internal cluster + client use (Go `dgo` client) |
| REST | HTTP endpoints for DQL mutations/queries, JSON req/res |
| Clients | Go (`dgo`), JS/TS (`dgraph-js`), Python (`pydgraph`), Java (`dgraph4j`) |
| Admin | `/admin` GraphQL for schema management, backup, restore, health |

### Deployment

- Self-hosted: Docker Compose (dev), Kubernetes Helm chart (prod), bare metal
- Minimum: 1x Zero + 1x Alpha. HA: 3x Zero + 3x+ Alpha per shard group (Raft quorum)
- Not embeddable (unlike SQLite/Badger) — must run as separate server process
- Cloud: Dgraph Cloud / Hypermode (uncertain post-acquisition)

### Comparison

| Feature | Dgraph | Neo4j | ArangoDB | SurrealDB |
|---------|--------|-------|----------|-----------|
| Query | DQL + GraphQL | Cypher | AQL | SurrealQL |
| Storage | Badger (own) | Custom (Java) | RocksDB | RocksDB/TiKV |
| Distribution | Native Raft | Enterprise only | Enterprise only | Built-in |
| ACID | Yes | Yes | Yes | Yes |
| GraphQL native | Yes | Plugin | No | No |
| Vector search | No | Yes (5.x) | No | Yes |
| Embedded mode | No | No | No | Yes |
| License | Apache 2.0 / BSL | GPL/Commercial | Apache 2.0 | BSL |
| Language | Go | Java | C++ | Rust |

### Assessment for Agent Knowledge Graphs

**Strengths:**
- Native GraphQL (LLM-friendly query generation)
- GraphQL subscriptions (push on changes, good for agent coordination)
- Fast multi-hop traversal (O(log n) via posting lists)
- Distributed ACID (multi-agent concurrent access)

**Weaknesses:**
- No native vector/embedding support (critical gap for semantic search)
- Requires separate server process (operational overhead vs. SQLite)
- Uncertain maintenance trajectory under Hypermode acquisition
- BSL on enterprise features (ACL, encryption at rest)
- Smaller ecosystem than Neo4j

**Verdict:** Overkill for single-machine/small-team agent workflows. Filament's SQLite + petgraph is better matched. Consider SurrealDB (multi-model, embedded, vectors, Rust) or Weaviate (graph + native vectors) if scaling beyond single-machine.

---

## 3. GitNexus — Codebase Knowledge Graph for Agents

**Repo:** https://github.com/abhigyanpatwari/GitNexus
**What:** Indexes any codebase into a knowledge graph (dependencies, call chains, clusters, execution flows) and exposes it via MCP tools for AI agent codebase awareness.
**Version:** 1.4.7 (npm), PolyForm Noncommercial license

### How It Works

```bash
npx gitnexus analyze    # Index repo, install skills+hooks, create context files
npx gitnexus mcp        # Start MCP server (stdio) — serves all indexed repos
npx gitnexus serve      # HTTP server for web UI connection
```

### Indexing Pipeline

1. **Structure** — file tree walk, folder/file relationships
2. **Parsing** — Tree-sitter ASTs: functions, classes, methods, interfaces
3. **Resolution** — imports, calls, heritage, constructor inference, self/this receivers (cross-file, language-aware)
4. **Clustering** — Leiden community detection via Graphology
5. **Processes** — execution flow tracing from entry points through call chains
6. **Search** — hybrid BM25 + semantic (HuggingFace transformers.js) + RRF

### MCP Tools (7)

| Tool | What |
|------|------|
| `list_repos` | Discover all indexed repositories |
| `query` | Process-grouped hybrid search (BM25 + semantic + RRF) |
| `context` | 360-degree symbol view — categorized refs, process participation |
| `impact` | Blast radius with depth grouping and confidence scores |
| `detect_changes` | Git-diff → affected processes mapping |
| `rename` | Multi-file coordinated rename (graph + text search) |
| `cypher` | Raw Cypher graph queries |

### Architecture

```
~/.gitnexus/registry.json          # Global repo registry (paths + metadata)
<project>/.gitnexus/               # Per-project index (gitignored)
    └── LadybugDB native store     # Graph + vector index

MCP server reads registry → lazy-opens LadybugDB connections per repo
(max 5 concurrent, evicted after 5min idle)
```

### Stack

TypeScript/Node.js, Tree-sitter (13 languages: TS, JS, Python, Java, Kotlin, C#, Go, Rust, PHP, Ruby, Swift, C, C++), LadybugDB (embedded graph DB, formerly KuzuDB), Graphology (clustering), HuggingFace transformers.js (embeddings), MCP SDK

### Notable Design Choices

- **Precomputed relational intelligence:** clustering, tracing, and scoring happen at index time, not runtime LLM exploration. Tools return complete context in one call.
- **Multi-repo MCP:** one global MCP server serves all indexed repos. No per-project MCP config needed.
- **Auto-generates Claude Code skills** per detected code community (`gitnexus analyze --skills`)
- **Claude Code hooks:** PreToolUse (enrich searches with graph context) + PostToolUse (auto-reindex after commits)
- **Web UI:** fully client-side (WASM Tree-sitter + LadybugDB + WebGL graph viz via Sigma.js)

### 13-Language Support Matrix

Full import resolution, exports, heritage, type annotations, constructor inference for: TypeScript, JavaScript, Python, Java, Kotlin, C#, Go, Rust, PHP, Ruby. Partial for: Swift, C, C++.

### License Warning

**PolyForm Noncommercial 1.0.0** — not open source for commercial use. Fine for personal/research, but cannot be used in a commercial product or service without a separate license.

### Relevance to Filament

GitNexus solves codebase awareness (what code exists, how it connects). Filament solves project coordination (tasks, agents, lessons, knowledge graph). Complementary, not competing. The MCP integration pattern — one `npx` command sets up MCP + skills + hooks — is worth studying for filament's own distribution.

---

## 4. Railguard — Runtime Guardrails for Claude Code

**Repo:** https://github.com/railyard-dev/railguard
**What:** Intercepts every Claude Code tool call and decides in <2ms: allow, block, or ask. The middle ground between `--dangerously-skip-permissions` and approving every action.
**Version:** 0.5.1 (crates.io), MIT license, 151 tests

### Installation

```bash
cargo install railguard
railguard install
# Then use Claude Code normally — railguard hooks in transparently
```

### What It Guards

Every tool call passes through railguard, not just Bash:

| Tool | Policy |
|------|--------|
| **Bash** | Command classification, pipe analysis, evasion detection |
| **Read** | Sensitive path detection (~/.ssh, ~/.aws, .env, ...) |
| **Write/Edit** | Path fencing + content inspection for secrets/dangerous payloads |
| **Memory** | Classifies writes: secrets→blocked, behavioral injection→asks, factual→allowed |

### Context-Aware Decisions

Same command gets different decisions based on context:
- `rm dist/bundle.js` inside project → allowed
- `rm ~/.bashrc` outside project → blocked
- `git push --force origin main` → asks

### Two-Layer Security

1. **Semantic rules** — pattern matching catches the obvious stuff instantly
2. **OS-level sandbox** — `sandbox-exec` (macOS) / `bwrap` (Linux) resolves what actually executes at the kernel level, catches base64/pipe/script evasion

### Memory Safety

- Every memory write classified (secrets, behavioral instructions, factual, overwrites, deletions)
- Content hashing for tamper detection between sessions
- Secrets (API keys, JWTs, private keys) → always blocked
- Behavioral injection ("skip safety checks") → always asks
- Deletions → always blocked

### Additional Features

- **Path fencing:** ~/.ssh, ~/.aws, ~/.gnupg, /etc fenced by default
- **Multi-agent coordination:** file locking per session, self-healing locks
- **Dashboard + replay:** real-time monitoring, session replay (ratatui)
- **Recovery:** file snapshots, per-edit or full-session rollback

### Configuration

```yaml
# railguard.yaml — changes take effect immediately
blocklist:
  - name: terraform-destroy
    pattern: "terraform\\s+destroy"
approve:
  - name: terraform-apply
    pattern: "terraform\\s+apply"
allowlist:
  - name: terraform-plan
    pattern: "terraform\\s+plan"
```

### Stack

Rust, clap (CLI), regex (pattern matching), ratatui+crossterm (dashboard), sha2 (content hashing), serde_yaml (config), libc, dialoguer. Also distributable via npm (postinstall downloads Rust binary).

### Architecture

```
src/
├── block/          # Command blocking logic
├── configure.rs    # Config management
├── context.rs      # Execution context
├── coord/          # Multi-agent coordination
├── dashboard/      # Real-time TUI dashboard
├── fence/          # Path fencing
├── hook/           # Claude Code hook integration
├── install/        # Installation logic
├── memory/         # Memory write classification
├── policy/         # Policy engine
├── replay/         # Session replay
├── sandbox/        # OS-level sandbox (sandbox-exec / bwrap)
├── shell.rs        # Shell analysis
├── snapshot/       # File snapshots + rollback
├── threat/         # Threat detection
└── trace/          # Execution tracing
```

### Relevance

Railguard and vibebox are complementary isolation layers:
- **Railguard** = policy interception at tool-call level (lightweight, same host, <2ms)
- **VibeBox** = hard VM boundary (heavy isolation, separate kernel)
- Can stack both: railguard inside vibebox for defense in depth

Railguard's hook-based approach (`.claude-plugin/plugin.json`, `hooks/hooks.json`) is a pattern to study for filament's own Claude Code integration.

---

## 5. Portable Agent Environment + Cross-Device Sync

### Problem Statement

When spinning up an isolated environment (VM via vibebox, container via Docker) for an AI agent, the agent needs:

| Component | Where it lives today | Portable? |
|-----------|---------------------|-----------|
| Agent binary (claude, codex) | npm in host | Reinstall in guest |
| Custom CLI (fl) | ~/.local/bin/fl (macOS binary) | Wrong arch for Linux guest |
| Global config (~/.claude/CLAUDE.md) | Host only | Must copy |
| Project config (./CLAUDE.md) | Git repo | Already portable |
| Skills (~/.claude/skills/) | Host only | Must copy |
| Memory (~/.claude/projects/*/memory/) | Host only | Must copy |
| Settings/hooks | Host only | Must copy |
| Knowledge graph (.fl/fl.db) | Per-project SQLite | Binary, not git-friendly |

### Approach A: Mount + Provision (vibebox-native)

Mount host config as read-only into the VM:

```toml
# vibebox.toml
[mounts]
claude-config = { host = "~/.claude", guest = "/home/vibecoder/.claude", mode = "read-only" }
project = { host = ".", guest = "/home/vibecoder/project", mode = "read-write" }
```

Problem: `fl` is macOS arm64, won't run in Linux guest. Need to download Linux binary in provision step.

Pro: Zero Docker, config stays on host, VM gets read-only view.
Con: Every new VM needs to download `fl`. Config changes require host-side edits.

### Approach B: Docker/OCI Image

Build an image with everything baked in:

```dockerfile
FROM debian:bookworm-slim
RUN curl -fsSL .../install.sh | sh          # fl (Linux)
RUN npm install -g @anthropic-ai/claude-code
COPY bundle/ /root/.claude/                  # Config + skills + memory
ENTRYPOINT ["claude", "-p"]
```

Pro: Hermetic, versioned, reproducible.
Con: Docker-in-VM overhead, registry push required, config changes require rebuild.

### Approach C: Agent Bundle Tarball (recommended)

A portable archive produced by a new `fl` subcommand:

```bash
fl bundle export --output agent-env.tar.gz
```

Produces:

```
agent-env/
├── bin/
│   └── fl-linux-arm64          # From GitHub releases (cross-compiled)
├── claude/
│   ├── CLAUDE.md               # Global config
│   ├── settings.json           # Settings (secrets stripped)
│   ├── skills/                 # All skills
│   └── memory/                 # Relevant memory files
├── project/
│   ├── CLAUDE.md               # Project config
│   └── .fl/
│       └── graph-export.json   # Filament graph (portable JSON)
└── setup.sh                    # Idempotent install script for guest
```

`setup.sh` runs inside VM/container:

```bash
#!/bin/sh
set -eu
cp bin/fl-linux-* ~/.local/bin/fl && chmod +x ~/.local/bin/fl
cp -r claude/* ~/.claude/
cd /workspace && fl init && fl import --file project/.fl/graph-export.json
```

Works with:
- **vibebox:** mount tarball dir, run setup.sh on first attach
- **Docker:** COPY + RUN in Dockerfile
- **Any SSH VM:** scp + tar xf + ./setup.sh

### Cross-Device Knowledge Graph Sync via Git

SQLite doesn't diff in git. Current `fl export` dumps monolithic JSON. Better approach:

#### Per-Entity File Export

```
.fl/
├── fl.db                    # .gitignored (local SQLite)
├── graph/                   # git-tracked export directory
│   ├── entities/
│   │   ├── abc12345.json    # One file per entity (slug-named)
│   │   ├── def67890.json
│   │   └── ...
│   ├── relations.json       # All relations (cross-cutting)
│   ├── messages.json        # Recent messages
│   └── meta.json            # Export metadata (timestamp, device, version)
└── content/                 # Content files (already on disk, already tracked)
```

One-file-per-entity means git diffs show exactly which entities changed.

#### Sync Commands

```bash
fl sync export    # SQLite -> .fl/graph/ (JSON files)
fl sync import    # .fl/graph/ -> SQLite (merge: latest-wins by updated_at)
fl sync status    # Show what's changed since last export
```

#### Conflict Resolution

- Use `updated_at` timestamps, latest wins for non-overlapping fields
- For genuine conflicts: flag and user resolves (builds on ADR-022 optimistic conflict resolution)
- Entity version numbers prevent silent overwrites

#### .gitignore Pattern

```gitignore
# .fl/.gitignore
fl.db
fl.db-wal
fl.db-shm
*.sock
*.pid
*.lock
# graph/ and content/ are tracked
```

#### Workflow

```
Device A (work done)
  fl sync export          # SQLite -> .fl/graph/*.json
  git add .fl/graph/ && git commit && git push

Device B (new session)
  git pull
  fl sync import          # .fl/graph/*.json -> SQLite (merge)
```

### What Exists vs. What's New

| Feature | Status |
|---------|--------|
| `fl export` (monolithic JSON) | Exists |
| `fl import` (JSON -> SQLite) | Exists |
| `fl audit` (snapshot to git branch) | Exists |
| Per-entity file export (`fl sync export`) | New |
| Merge-aware import (`fl sync import`) | New |
| Agent bundle export (`fl bundle export`) | New |
| vibebox.toml template for agent setup | New |

---

## Cross-Cutting Observations

### Complementary Layers

These tools form a layered agent safety + productivity stack:

```
Layer 4: GitNexus        — codebase awareness (what code exists, how it connects)
Layer 3: Filament         — project coordination (tasks, agents, lessons, knowledge graph)
Layer 2: Railguard        — runtime guardrails (policy interception, <2ms per call)
Layer 1: VibeBox          — hard isolation (full VM boundary, separate kernel)
Layer 0: Agent Bundle     — portable environment (binaries + config + graph)
```

### Distribution Patterns Worth Adopting

1. **GitNexus's one-command setup:** `npx gitnexus analyze` does indexing + skill install + hook registration + context file creation. One command, full integration.
2. **Railguard's dual distribution:** cargo install (Rust users) + npm postinstall (everyone else). The npm package downloads the pre-built Rust binary.
3. **VibeBox's mount model:** explicit allowlists in a simple TOML. No Dockerfile ceremony.

### Licensing Summary

| Project | License | Commercial Use |
|---------|---------|----------------|
| VibeBox | MIT | Yes |
| Dgraph | Apache 2.0 / BSL | Community yes, enterprise features delayed |
| GitNexus | PolyForm Noncommercial | No (without separate license) |
| Railguard | MIT | Yes |
| Filament | (own) | N/A |

### Next Steps

1. Write a plan for `fl bundle export` (agent environment packaging)
2. Write a plan for `fl sync export/import` (per-entity git-friendly graph sync)
3. Evaluate vibebox integration (mount template + provision hook for fl)
4. Consider railguard integration (complementary to filament's agent dispatch)
5. Study GitNexus MCP pattern for filament's own MCP distribution improvements
