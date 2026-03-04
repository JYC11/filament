---
name: roleplay-sim
description: >
  Roleplay a multi-agent orchestration simulation using the filament CLI.
  Exercises the full system end-to-end: entities, relations, tasks, messaging,
  escalations, reservations, export/import. Creates a temp project, seeds it
  with a realistic "web app rewrite" scenario, then walks through 11 simulation
  cycles demonstrating agent coordination patterns.
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
| `pause rp` | Stop after current cycle, save state to file, ask user for feedback |
| `end rp` | Clean up temp project + state file, summarize what was demonstrated |

## State File: `/tmp/filament-sim/rp-state.json`

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
cat > /tmp/filament-sim/rp-state.json << 'STATEEOF'
{ ... current state ... }
STATEEOF
```

### Resume (on `start rp` when state file exists)
1. Check for `/tmp/filament-sim/rp-state.json`
2. If it exists, read it and announce: "Resuming simulation from cycle N"
3. Load the slug mappings from the state file — **do NOT re-seed**
4. Run `filament list --type task --status all` to show current state
5. Continue from `next_cycle`

### What to capture in state
- All entity slug mappings (name → slug)
- Last completed cycle number
- Free-text notes about what happened (for narrator context)

## Prerequisite

The filament binary must be on PATH. Build with:
```bash
make build CRATE=all RELEASE=1
```

## Setup Phase (on `start rp`)

### 1. Build and init

```bash
make build CRATE=all RELEASE=1
cd /tmp && rm -rf filament-sim && mkdir filament-sim && cd filament-sim
filament init
```

### 2. Seed entities

Create these entities and **capture their slugs** from the output (the 8-char code shown
on creation). You'll need slugs for all subsequent commands.

**Modules (4):**
```bash
filament add api-gateway --type module --summary "HTTP routing layer — Express.js with rate limiting and request validation"
filament add auth-service --type module --summary "JWT authentication + session management with Redis backing"
filament add data-layer --type module --summary "PostgreSQL models, migrations, and query layer (Prisma)"
filament add frontend --type module --summary "React 18 SPA with TypeScript, React Query, and Tailwind CSS"
```

**Agents (4):**
```bash
filament add alice --type agent --summary "Senior backend engineer — 5yr exp, owns auth and API"
filament add bob --type agent --summary "Frontend specialist — React expert, accessibility focus"
filament add carol --type agent --summary "Staff engineer — code reviewer, architecture guardian"
filament add dave --type agent --summary "Tech lead — planner, architect, tie-breaker on decisions"
```

**Tasks (8) with dependency chain:**
```bash
filament task add design-architecture --summary "Design system architecture: service boundaries, API contracts, data model" --priority 0
filament task add setup-database --summary "PostgreSQL schema, migrations (0001_users, 0002_sessions, 0003_oauth_tokens), seed data" --priority 1
filament task add implement-auth --summary "OAuth2 + JWT: login/logout, token refresh, session management, Google provider" --priority 1
filament task add implement-api --summary "REST API endpoints: /users, /sessions, /auth/*, rate limiting, validation middleware" --priority 1
filament task add implement-frontend --summary "React SPA: login page, dashboard, user profile, API integration via React Query" --priority 2
filament task add integration-tests --summary "End-to-end test suite: auth flow, CRUD operations, error scenarios, load testing" --priority 1
filament task add code-review --summary "Full codebase review: security audit, performance review, accessibility check" --priority 2
filament task add deploy-staging --summary "Deploy to staging: Docker compose, CI/CD pipeline, smoke tests, monitoring setup" --priority 0
```

**Set up blocking chain** (use captured slugs):
```bash
filament relate <design-architecture> blocks <setup-database>
filament relate <setup-database> blocks <implement-auth>
filament relate <implement-auth> blocks <implement-api>
filament relate <implement-api> blocks <implement-frontend>
filament relate <implement-frontend> blocks <integration-tests>
filament relate <integration-tests> blocks <code-review>
filament relate <code-review> blocks <deploy-staging>
```

**Plan:**
```bash
filament add rewrite-plan --type plan --summary "Web App Rewrite v2 — master plan covering 8 tasks across 4 modules"
```

**Docs:**
```bash
filament add api-spec --type doc --summary "REST API specification: endpoints, request/response schemas, auth headers, error codes"
filament add auth-design --type doc --summary "Auth architecture: OAuth2 flow diagrams, JWT structure, session lifecycle, security considerations"
```

**Additional relations** (module dependencies, ownership, documentation links):
```bash
filament relate <auth-service> depends_on <data-layer>
filament relate <api-gateway> depends_on <auth-service>
filament relate <frontend> depends_on <api-gateway>
filament relate <rewrite-plan> owns <design-architecture>
filament relate <rewrite-plan> owns <setup-database>
filament relate <rewrite-plan> owns <implement-auth>
filament relate <rewrite-plan> owns <implement-api>
filament relate <rewrite-plan> owns <implement-frontend>
filament relate <rewrite-plan> owns <integration-tests>
filament relate <rewrite-plan> owns <code-review>
filament relate <rewrite-plan> owns <deploy-staging>
filament relate <api-spec> relates_to <api-gateway>
filament relate <auth-design> relates_to <auth-service>
```

### 3. Verify seed

```bash
filament list --type task --status all
filament list --type agent
filament list --type module
filament task ready
filament task critical-path <deploy-staging>
```

Narrate: show the full dependency chain, explain that only `design-architecture` is unblocked.

---

## Simulation Cycles

### Cycle 1: Planner completes architecture design

**Scene:** Dave the architect kicks off the project by designing the system.

```bash
filament task ready
filament task assign <design-architecture> --to <dave>
filament update <design-architecture> --status in_progress
```

Pause for effect. Then simulate completion:

```bash
filament message send --from <dave> --to <alice> --body "Architecture finalized. DB schema in api-spec doc. Start with PostgreSQL setup — tables: users, sessions, oauth_tokens. Use UUID PKs." --type text
filament message send --from <dave> --to <bob> --body "Frontend can use React Query for API calls. See api-spec for endpoints. Auth will use httpOnly cookies, not localStorage." --type text
filament task close <design-architecture>
```

Verify:
```bash
filament task ready
```

Narrate: `setup-database` is now unblocked. The chain advances.

### Cycle 2: Database setup with file reservations

**Scene:** Alice claims the database work and locks the relevant files.

```bash
filament task assign <setup-database> --to <alice>
filament update <setup-database> --status in_progress
filament reserve "src/db/**" --agent <alice> --exclusive --ttl 3600
filament reservations
```

Simulate completion:
```bash
filament message send --from <alice> --to <carol> --body "DB migrations ready for review: 0001_users.sql, 0002_sessions.sql, 0003_oauth_tokens.sql. Indexes on email and session_token." --type artifact
filament release "src/db/**" --agent <alice>
filament task close <setup-database>
filament task ready
```

Narrate: Alice finished fast. `implement-auth` is now unblocked. Carol got the artifact notification.

### Cycle 3: Blocker escalation

**Scene:** Alice starts auth but hits a wall — she needs OAuth credentials nobody provided.

```bash
filament task assign <implement-auth> --to <alice>
filament update <implement-auth> --status in_progress
filament message send --from <alice> --to user --body "BLOCKED: Need OAuth2 client_id and client_secret for Google provider. Cannot proceed without credentials. Who has access to the Google Cloud Console?" --type blocker
filament update <implement-auth> --status blocked
filament escalations
```

Narrate: The escalation system catches this. In a real setup, the TUI would show an alert. The human operator needs to respond.

### Cycle 4: Question escalation

**Scene:** While blocked, Alice raises a design question too.

```bash
filament message send --from <alice> --to user --body "Should we support both OAuth2 and SAML, or just OAuth2? SAML adds ~2 days of work but enterprise customers need it. What's the priority?" --type question
filament escalations
```

Narrate: Now there are TWO escalations — a blocker AND a question. Both need human attention.

### Cycle 5: User resolves escalations, work continues

**Scene:** The user (you) responds to Alice, unblocking her.

```bash
filament message send --from user --to <alice> --body "OAuth2 only for now. We'll add SAML in v2. Credentials: use env vars OAUTH_CLIENT_ID and OAUTH_CLIENT_SECRET — they're in the team vault." --type text
filament update <implement-auth> --status in_progress
```

Simulate auth completion:
```bash
filament message send --from <alice> --to <carol> --body "Auth module complete. JWT tokens with 1h expiry, refresh token rotation every 7d. Google OAuth2 flow tested. Ready for review." --type artifact
filament task close <implement-auth>
filament task ready
filament escalations
```

Narrate: Escalations cleared. `implement-api` is now unblocked. The chain keeps moving.

### Cycle 6: File reservation conflict

**Scene:** Alice and Bob both try to claim the API directory.

```bash
filament reserve "src/api/**" --agent <alice> --exclusive --ttl 3600
filament reserve "src/api/**" --agent <bob> --exclusive --ttl 3600
```

The second reserve should fail with exit code 6 (resource conflict). Continue:

```bash
filament reservations
filament release "src/api/**" --agent <alice>
filament reserve "src/api/**" --agent <bob> --exclusive --ttl 3600
filament reservations
filament release "src/api/**" --agent <bob>
```

Narrate: The advisory locking system prevented a conflict. Alice released, then Bob got the lock.

### Cycle 7: Parallel work + inter-agent messaging

**Scene:** Alice builds the API while Carol reviews the auth work. Agents communicate directly.

```bash
filament task assign <implement-api> --to <alice>
filament update <implement-api> --status in_progress
filament message send --from <carol> --to <alice> --body "Auth review complete: LGTM overall. One nit: add rate limiting to /auth/token endpoint (10 req/min per IP). Non-blocking, can be done during API work." --type text
filament message send --from <carol> --to <dave> --body "Auth module review complete. Code quality is solid. JWT implementation follows OWASP guidelines. Recommending we proceed to API phase." --type artifact
filament message inbox <alice>
filament task close <implement-api>
filament task ready
```

Narrate: Carol's review feedback goes directly to Alice. Dave gets a status update. The messaging system enables async coordination without a central bottleneck.

### Cycle 8: Frontend work + failure scenario

**Scene:** Bob starts the frontend but hits a React 19 breaking change.

```bash
filament task assign <implement-frontend> --to <bob>
filament update <implement-frontend> --status in_progress
filament message send --from <bob> --to user --body "BLOCKED: Build fails on React 19. Breaking change in useEffect cleanup causes 12 component failures. Options: (1) downgrade to React 18 (~30min), (2) refactor all 12 components (~4hrs). Need a decision." --type blocker
filament update <implement-frontend> --status blocked
filament escalations
```

Resolve and complete:
```bash
filament message send --from user --to <bob> --body "Downgrade to React 18 for now. We'll upgrade post-launch when the ecosystem stabilizes." --type text
filament update <implement-frontend> --status in_progress
filament message send --from <bob> --to <carol> --body "Frontend complete. React 18, TypeScript strict mode, 94% Lighthouse score. React Query handles all API state. Ready for review." --type artifact
filament task close <implement-frontend>
```

Narrate: Another escalation cycle. The human made a pragmatic call — ship with React 18, upgrade later.

### Cycle 9: Context queries and graph exploration

**Scene:** Step back and explore the knowledge graph we've built.

```bash
filament context --around <implement-auth> --depth 2
filament task critical-path <deploy-staging>
filament list --type task --status all
filament list --type agent
filament task ready
```

Narrate: The graph shows the rich web of relationships — blocking chains, ownership, agent assignments, module dependencies. This is the value of a knowledge graph over flat task lists.

### Cycle 10: Final sprint — close remaining tasks

**Scene:** The team sprints to finish. Integration tests, code review, then deploy.

```bash
filament task assign <integration-tests> --to <alice>
filament update <integration-tests> --status in_progress
filament task close <integration-tests>
filament task ready

filament task assign <code-review> --to <carol>
filament update <code-review> --status in_progress
filament message send --from <carol> --to <dave> --body "Full review complete. 3 minor issues filed, no blockers. Security audit passed. Ready for staging deploy." --type artifact
filament task close <code-review>
filament task ready

filament task assign <deploy-staging> --to <dave>
filament update <deploy-staging> --status in_progress
filament task close <deploy-staging>

filament task list --status all
filament task ready
```

Narrate: All 8 tasks closed. `task ready` returns nothing — the project is complete.

### Cycle 11: Export/import round-trip

**Scene:** Export everything, import into a fresh project, verify integrity.

```bash
filament export --output /tmp/filament-sim/snapshot.json
cat /tmp/filament-sim/snapshot.json | head -50
```

Create second project and import:
```bash
mkdir -p /tmp/filament-sim2 && cd /tmp/filament-sim2
filament init
filament import --input /tmp/filament-sim/snapshot.json
filament list --type task --status all
filament list --type agent
```

Verify counts match the original. Then clean up:
```bash
rm -rf /tmp/filament-sim2
```

Narrate: Full data portability. Every entity, relation, message, and event survived the round-trip.

---

## Cleanup Phase (on `end rp`)

```bash
rm -rf /tmp/filament-sim
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
2. **Write the state file** to `/tmp/filament-sim/rp-state.json` with:
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

## Resume Behavior (on `start rp` when `/tmp/filament-sim/rp-state.json` exists)

1. Read the state file
2. Announce: "Resuming from cycle N. Here's where we left off: [notes]"
3. Load slug mappings — **do not re-create entities**
4. Run `filament list --type task --status all` and `filament escalations` to show current state
5. Continue from `next_cycle`
6. The cwd must be `/tmp/filament-sim/`

## Important Notes

- **All slugs are dynamic** — capture them from `filament add` output and use them throughout
- **Don't fabricate CLI output** — run the actual commands and narrate based on real results
- **The simulation runs in `/tmp/filament-sim/`** — completely isolated, won't affect the main project
- **No daemon needed** — all cycles use direct CLI commands
- **Exit code 6** on reservation conflict is expected, not an error — narrate it as the system working correctly
- **State file enables session restart** — always save state on `pause rp` so a new session can resume
- **On resume, trust the state file** — don't re-run setup or re-seed entities, just continue from the saved cycle
