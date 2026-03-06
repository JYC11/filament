---
name: roleplay-sim
description: >
  Roleplay a multi-agent orchestration simulation using the filament CLI.
  Accepts either a custom scenario definition (JSON/YAML format) or falls back
  to the built-in "web app rewrite" scenario. Exercises entities, relations,
  tasks, messaging, escalations, reservations, export/import.
  Triggers on: "start rp", "pause rp", "end rp", "roleplay", "simulation",
  "simulate agents", "run simulation".
---

# Roleplay Simulation — Multi-Agent Orchestration

## Your Role

You are the **simulation narrator and executor**. You play ALL roles:
- **Narrator**: explain what's happening and why before each cycle
- **Executor**: run the filament CLI commands
- **Observer**: verify results after each cycle and comment on what happened

Speak in a natural, engaging tone. Before each cycle, briefly set the scene (1-2 sentences).
After each cycle, summarize what changed in the system. Use the filament CLI output to
drive the narrative — don't fabricate results.

## Commands

| Command | Action |
|---------|--------|
| `start rp` | Build release binary, init temp project, seed data, begin cycle 1. If state file exists, **resume** from saved cycle. |
| `start rp <scenario-file>` | Same as above but uses the custom scenario definition instead of the built-in one. |
| `pause rp` | Stop after current cycle, save state to file, ask user for feedback |
| `end rp` | Clean up temp project + state file, summarize what was demonstrated |

## Custom Scenario Format

Instead of the hardcoded "web app rewrite" scenario, users can provide a JSON scenario
file. When `start rp` is invoked with a file path argument, load and validate it.

### Scenario Definition Schema

```json
{
  "name": "My Custom Scenario",
  "description": "One-line description of the simulation",

  "modules": [
    {
      "name": "module-name",
      "summary": "What this module represents"
    }
  ],

  "agents": [
    {
      "name": "agent-name",
      "summary": "Role description"
    }
  ],

  "tasks": [
    {
      "name": "task-name",
      "summary": "What needs to be done",
      "priority": 1,
      "blocks": ["other-task-name"]
    }
  ],

  "docs": [
    {
      "name": "doc-name",
      "summary": "Reference material",
      "relates_to": ["module-name"]
    }
  ],

  "plans": [
    {
      "name": "plan-name",
      "summary": "Planning document",
      "owns": ["task-name-1", "task-name-2"]
    }
  ],

  "cycles": [
    {
      "title": "Cycle title",
      "scene": "Narrative setup for this cycle (1-2 sentences)",
      "actions": [
        {"type": "assign", "task": "task-name", "agent": "agent-name"},
        {"type": "start", "task": "task-name"},
        {"type": "message", "from": "agent-name", "to": "other-agent", "body": "Message text", "msg_type": "text"},
        {"type": "close", "task": "task-name"},
        {"type": "block", "task": "task-name", "reason": "Why it's blocked"},
        {"type": "escalate", "from": "agent-name", "body": "Escalation message", "msg_type": "blocker"},
        {"type": "resolve", "task": "task-name"},
        {"type": "reserve", "glob": "src/api/**", "agent": "agent-name", "exclusive": true},
        {"type": "release", "glob": "src/api/**", "agent": "agent-name"},
        {"type": "query", "command": "task ready"},
        {"type": "query", "command": "context --around <task-name> --depth 2"},
        {"type": "query", "command": "escalations"},
        {"type": "mkdir", "path": "src/api"},
        {"type": "write_file", "path": "src/api/routes.txt", "content": "GET /users\nPOST /users\nGET /users/:id", "agent": "agent-name"},
        {"type": "read_file", "path": "src/api/routes.txt", "agent": "other-agent"}
      ]
    }
  ]
}
```

### Action Types Reference

| Action Type | Required Fields | Description |
|-------------|----------------|-------------|
| `assign` | task, agent | Assign task to agent |
| `start` | task | Set task status to in_progress |
| `close` | task | Close the task |
| `block` | task, reason | Set task to blocked |
| `resolve` | task | Set blocked task back to in_progress |
| `message` | from, to, body, msg_type | Send a message (text/artifact/blocker/question) |
| `escalate` | from, body, msg_type | Send message to "user" (creates escalation) |
| `reserve` | glob, agent | Reserve files (optional: exclusive, ttl) |
| `release` | glob, agent | Release file reservation |
| `query` | command | Run an `fl` query command and narrate the result |
| `write_file` | path, content, agent | Agent writes a .txt file (simulates real work output) |
| `read_file` | path, agent | Agent reads a .txt file written by another agent |
| `mkdir` | path | Create a directory for agent workspace |

### File Actions — Simulating Real Work

Agents should produce **actual files** during the simulation to make it feel realistic. All file
operations happen inside the `/tmp/fl-sim/` project directory.

- `mkdir`: Create directories for agent workspaces (e.g., `src/api/`, `docs/architecture/`)
- `write_file`: Agent writes a `.txt` file with content representing their work output (designs, code
  sketches, review notes, test plans). The `agent` field is narrated as the author.
- `read_file`: Agent reads a file written by another agent (simulates handoff, review, collaboration).

**Rules:**
- Only `.txt` files — safe, simple, no execution risk
- Paths are relative to `/tmp/fl-sim/` (the simulation project root)
- File writes pair naturally with `reserve`/`release` — reserve the glob before writing, release after
- Narrate file operations as part of the story: "Alice writes her API design to `docs/api-spec.txt`"

**Execution:**
```bash
# mkdir
mkdir -p /tmp/fl-sim/src/api

# write_file
cat > /tmp/fl-sim/src/api/routes.txt << 'EOF'
GET /users - List all users
POST /users - Create user
GET /users/:id - Get user by ID
EOF

# read_file
cat /tmp/fl-sim/src/api/routes.txt
```

### Name Resolution

Entity names in the scenario file are resolved to slugs at runtime. The simulation:
1. Creates all entities during setup
2. Captures slug mappings (name -> slug)
3. Replaces `<name>` references in cycle actions with actual slugs

### Validation

On load, validate:
- All task names referenced in `blocks` exist in `tasks`
- All agent names referenced in cycles exist in `agents`
- All task names referenced in cycles exist in `tasks`
- All module names referenced in `relates_to` exist in `modules`
- All task names referenced in `owns` exist in `tasks`
- Each cycle has at least one action
- No duplicate entity names across all types

If validation fails, report errors and do not start the simulation.

### Example: Minimal Scenario

```json
{
  "name": "API Migration",
  "description": "Migrate REST API from v1 to v2",
  "modules": [
    {"name": "api-v1", "summary": "Legacy REST API"},
    {"name": "api-v2", "summary": "New REST API with OpenAPI spec"}
  ],
  "agents": [
    {"name": "alice", "summary": "Backend engineer"},
    {"name": "bob", "summary": "API reviewer"}
  ],
  "tasks": [
    {"name": "audit-v1", "summary": "Audit existing endpoints", "priority": 0},
    {"name": "design-v2", "summary": "Design v2 schema", "priority": 1, "blocks": []},
    {"name": "implement-v2", "summary": "Build new endpoints", "priority": 1, "blocks": ["design-v2"]},
    {"name": "migrate-clients", "summary": "Update API clients", "priority": 2, "blocks": ["implement-v2"]}
  ],
  "docs": [],
  "plans": [
    {"name": "migration-plan", "summary": "API v1->v2 migration", "owns": ["audit-v1", "design-v2", "implement-v2", "migrate-clients"]}
  ],
  "cycles": [
    {
      "title": "Audit existing API",
      "scene": "Alice starts by cataloguing all v1 endpoints.",
      "actions": [
        {"type": "assign", "task": "audit-v1", "agent": "alice"},
        {"type": "start", "task": "audit-v1"},
        {"type": "message", "from": "alice", "to": "bob", "body": "Found 47 endpoints. 12 are unused. Documenting in api-spec.", "msg_type": "artifact"},
        {"type": "close", "task": "audit-v1"},
        {"type": "query", "command": "task ready"}
      ]
    },
    {
      "title": "Design blocked by missing requirements",
      "scene": "Bob starts the v2 design but realizes auth requirements are unclear.",
      "actions": [
        {"type": "assign", "task": "design-v2", "agent": "bob"},
        {"type": "start", "task": "design-v2"},
        {"type": "escalate", "from": "bob", "body": "Need clarification: should v2 support API keys AND OAuth2, or just OAuth2?", "msg_type": "question"},
        {"type": "block", "task": "design-v2", "reason": "Waiting for auth requirements clarification"},
        {"type": "query", "command": "escalations"}
      ]
    }
  ]
}
```

### Loading a Custom Scenario

When `start rp <path>` is invoked:

1. Read and parse the JSON file
2. Validate the schema (report all errors, not just the first)
3. Announce: "Loading custom scenario: <name> — <description>"
4. Run the standard setup (build, init temp project)
5. Create entities from the scenario definition (modules, agents, tasks, docs, plans)
6. Set up relations (blocks, owns, relates_to, depends_on)
7. Capture all slug mappings
8. Execute cycles in order, narrating each one

### Built-in Scenarios

Scenario files are in `scenarios/` relative to this skill:

| File | Description | Entities | Cycles |
|------|-------------|----------|--------|
| `web-app-rewrite.json` | Linear dependency chain, 4 agents, 8 tasks | 18 | 11 |
| `microservices-migration.json` | Diamond dependency graph, 6 agents, 15 tasks | 27 | 9 |
| `advanced-features.json` | Diamond deps, 3 agents, 5 tasks — parallel dispatch, escalations | 10 | 5 |
| `knowledge-capture.json` | Lessons, FTS5 search, file actions, graph analytics, onboarding | 11 | 7 |
| `infra-governance.json` | Config, hooks, audit, pagerank, degree, file actions | 8 | 4 |
| `mega-stress-test.json` | Everything: 8 modules, 8 agents, 20 tasks, 15 cycles, all features | 44 | 15 |

### Default Scenario

If `start rp` is invoked without a file path, use `scenarios/web-app-rewrite.json`.

## State File: `/tmp/fl-sim/rp-state.json`

The state file enables **session survival**. Context windows fill up — the user may need to
restart the session mid-simulation. The state file preserves everything needed to resume.

### State file format
```json
{
  "last_completed_cycle": 3,
  "next_cycle": 4,
  "slugs": {
    "api-gateway": "a1b2c3d4",
    "auth-service": "e5f6g7h8",
    "data-layer": "i9j0k1l2",
    "frontend": "m3n4o5p6",
    "alice": "q7r8s9t0",
    "bob": "u1v2w3x4",
    "carol": "y5z6a7b8",
    "dave": "c9d0e1f2",
    "design-architecture": "g3h4i5j6",
    "setup-database": "k7l8m9n0",
    "implement-auth": "o1p2q3r4",
    "implement-api": "s5t6u7v8",
    "implement-frontend": "w9x0y1z2",
    "integration-tests": "a3b4c5d6",
    "code-review": "e7f8g9h0",
    "deploy-staging": "i1j2k3l4",
    "rewrite-plan": "m5n6o7p8",
    "api-spec": "q9r0s1t2",
    "auth-design": "u3v4w5x6"
  },
  "notes": "Cycle 3 ended with implement-auth blocked. Two escalations pending."
}
```

### Save state (on `pause rp`)
After completing the current cycle, write the state file:
```bash
cat > /tmp/fl-sim/rp-state.json << 'STATEEOF'
{ ... current state ... }
STATEEOF
```

### Resume (on `start rp` when state file exists)
1. Check for `/tmp/fl-sim/rp-state.json`
2. If it exists, read it and announce: "Resuming simulation from cycle N"
3. Load the slug mappings from the state file — **do NOT re-seed**
4. Run `fl list --type task --status all` to show current state
5. Continue from `next_cycle`

### What to capture in state
- All entity slug mappings (name → slug)
- Last completed cycle number
- Free-text notes about what happened (for narrator context)

## Prerequisite

The `fl` binary must be on PATH. Build with:
```bash
make build CRATE=all RELEASE=1
```

## Setup Phase (on `start rp`)

### 1. Build and init

```bash
make build CRATE=all RELEASE=1
cd /tmp && rm -rf fl-sim && mkdir fl-sim && cd fl-sim
fl init
```

### 2. Load and seed scenario

Load the scenario JSON (default or user-provided), then for each section:

1. **Modules**: `fl add <name> --type module --summary "<summary>"` for each
2. **Agents**: `fl add <name> --type agent --summary "<summary>"` for each
3. **Tasks**: `fl task add <name> --summary "<summary>" --priority <N>` for each
4. **Docs**: `fl add <name> --type doc --summary "<summary>"` for each
5. **Plans**: `fl add <name> --type plan --summary "<summary>"` for each
6. **Blocking relations**: For each task with `blocks`, run `fl relate <task> blocks <blocked-task>`
7. **Plan ownership**: For each plan with `owns`, run `fl relate <plan> owns <task>`
8. **Doc relations**: For each doc with `relates_to`, run `fl relate <doc> relates_to <module>`
9. **Extra relations**: For each entry in `extra_relations`, run `fl relate <source> <type> <target>`

**Capture all slugs** from creation output — you need them for cycle actions.

### 3. Verify seed

```bash
fl list --type task --status all
fl list --type agent
fl list --type module
fl task ready
```

Narrate: show the dependency structure and which tasks are initially unblocked.

### 4. Execute cycles

For each cycle in the scenario:
1. Announce the cycle title and narrate the scene
2. Execute each action, resolving entity names to slugs
3. For `reserve` actions that are expected to fail (conflict demonstrations), narrate exit code 6 as correct behavior
4. After all actions, summarize what changed

---

## Cleanup Phase (on `end rp`)

```bash
rm -rf /tmp/fl-sim
```

### Summary template

Print a summary of what was demonstrated:

```
## Simulation Summary

**Entities created:** 18 (4 modules, 4 agents, 8 tasks, 1 plan, 2 docs)
**Relations created:** ~20 (blocks, depends_on, owns, relates_to, assigned_to)
**Messages sent:** ~12 (text, artifact, blocker, question)
**Escalations raised:** 3 (2 blockers, 1 question) — all resolved
**Reservation conflicts:** 1 — correctly prevented
**Export/import:** verified round-trip integrity

### Patterns demonstrated:
1. Dependency chain — tasks unblock sequentially as predecessors close
2. Escalation workflow — agents raise blockers/questions, humans respond
3. File reservations — advisory locking prevents conflicts
4. Inter-agent messaging — direct async communication
5. Graph queries — context, critical-path, ready-task computation
6. Data portability — export/import preserves full state
```

---

## Pause Behavior (on `pause rp`)

1. Stop after the current cycle completes
2. **Write the state file** to `/tmp/fl-sim/rp-state.json` with:
   - `last_completed_cycle`: the cycle number just finished
   - `next_cycle`: the next cycle to run
   - `slugs`: all entity name → slug mappings
   - `notes`: brief narrator context (what happened, any pending escalations/blockers)
3. Print:
   - What cycle just finished
   - What the next cycle would be
   - Current system state (open tasks, pending escalations, active reservations)
4. Tell the user: "State saved. You can restart the session and say `start rp` to resume from cycle N."
5. Ask: "Want to continue, skip ahead, or adjust anything?"

## Resume Behavior (on `start rp` when `/tmp/fl-sim/rp-state.json` exists)

1. Read the state file
2. Announce: "Resuming from cycle N. Here's where we left off: [notes]"
3. Load slug mappings — **do not re-create entities**
4. Run `fl list --type task --status all` and `fl escalations` to show current state
5. Continue from `next_cycle`
6. The cwd must be `/tmp/fl-sim/`

## Important Notes

- **All slugs are dynamic** — capture them from `fl add` output and use them throughout
- **Don't fabricate CLI output** — run the actual commands and narrate based on real results
- **The simulation runs in `/tmp/fl-sim/`** — completely isolated, won't affect the main project
- **No daemon needed** — all cycles use direct CLI commands
- **Exit code 6** on reservation conflict is expected, not an error — narrate it as the system working correctly
- **State file enables session restart** — always save state on `pause rp` so a new session can resume
- **On resume, trust the state file** — don't re-run setup or re-seed entities, just continue from the saved cycle
