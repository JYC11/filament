# Simulation v2 Log: Daemon-Mode Microservices Migration

**Date:** 2026-03-04
**Binary:** filament 0.1.0 (release build)
**Mode:** Daemon (Unix socket, PID 22475)
**Project:** `/tmp/filament-sim-v2/`

## Summary

| Metric | Count |
|--------|-------|
| Entities | 27 (6 modules, 6 agents, 15 tasks) |
| Relations | 47 (21 blocks, 10 depends_on, 5 owns, 11 other) |
| Messages | 32 |
| Escalations | 5 (3 blockers, 2 questions) |
| Reservations created | 8 |
| Reservation conflicts | 1 (proto/shared.proto) |
| Events | 160 |
| Export size | 90 KB JSON |
| Critical path length | 6 steps |

## Scenario: Microservices Migration

Monolith decomposed into 5 microservices. Diamond dependency graph with parallel branches converging.

### Dependency Graph
```
arch-design ──┬── user-svc-impl ───┬── notification-svc-impl
              │                    ├── api-gateway-impl ──┬── integration-tests ──┬── canary-deploy ── full-deploy ── post-mortem-doc
db-migration ─┤── order-svc-impl ──┤                     │                       │
              │                    │                      │                       │
              └── payment-svc-impl ┴── security-audit ────┘── load-tests ─────────┘
                                   │
infra-setup ─── monitoring-setup ──┘ (monitoring also blocks integration-tests)
```

### Agents
| Slug | Name | Role | Assigned Tasks |
|------|------|------|----------------|
| es77t0j9 | alice | coder/backend | db-migration-plan, user-svc-impl, order-svc-impl, api-gateway-impl |
| qimfwx5p | bob | coder/payments | payment-svc-impl |
| jjvoq66l | carol | coder/frontend | api-gateway-impl (vetoed, reassigned) |
| s5oz2gdp | dave | coder/infra | infra-setup, monitoring-setup, notification-svc-impl, load-tests, canary-deploy, full-deploy |
| pl8y8zay | eve | reviewer/QA | security-audit, integration-tests |
| qerwbqp2 | frank | planner/lead | arch-design, post-mortem-doc |

## Cycle-by-Cycle Log

### Cycle 1: Init + Start Daemon
```
filament init
filament serve
```
- Daemon started (PID 22475)
- Socket: `.filament/filament.sock`
- PID file: `.filament/filament.pid`

### Cycle 2: Seed Modules (6)
```
filament add monolith --type module --summary "Legacy monolith..."
filament add user-service --type module ...
filament add order-service --type module ...
filament add payment-service --type module ...
filament add notification-service --type module ...
filament add api-gateway --type module ...
```
All routed through daemon (no direct DB fallback messages).

### Cycle 3: Seed Agents (6)
```
filament add alice --type agent --summary "Backend dev" --facts '{"role":"coder",...}'
filament add bob --type agent ...
filament add carol --type agent ...
filament add dave --type agent ...
filament add eve --type agent ...
filament add frank --type agent ...
```

### Cycle 4: Seed Tasks (15) + Wire Dependencies (21 relations)
- 15 tasks across 5 phases created via `filament task add`
- 21 `blocks` relations wired: `A blocks B` = B can't start until A closes
- 10 agent/module `depends_on` and `owns` relations
- **Key learning:** `blocks` direction matters: `arch-design blocks user-svc-impl` (not vice versa)

### Cycle 5: Verify Graph
```
filament task ready
→ arch-design [P0], db-migration-plan [P0], infra-setup [P1]

filament task critical-path rftjbnbd
→ full-deploy ← canary ← load-tests ← api-gateway ← payment-svc ← db-migration-plan (6 steps)
```

### Cycle 6: Phase 1 Assignments
- Frank assigned arch-design, Alice assigned db-migration-plan, Dave assigned infra-setup
- All marked in_progress
- Frank messages team with instructions

### Cycle 7: File Reservations
```
filament reserve "docs/architecture/**" --agent frank --exclusive --ttl 3600
filament reserve "migrations/**" --agent alice --exclusive --ttl 3600
filament reserve "infra/**" --agent dave --exclusive --ttl 3600
```
3 exclusive reservations active.

### Cycle 8: Phase 1 Completes
- Frank closes arch-design → sends artifact messages to alice, bob, carol
- Alice closes db-migration-plan → sends artifact to frank
- **Unblocked:** user-svc-impl, order-svc-impl, payment-svc-impl (Phase 2)

### Cycle 9: Phase 2 Begins + Reservation Conflict
- Alice takes user-svc + order-svc, Bob takes payment-svc
- Bob reserves `proto/shared.proto` exclusively
- **CONFLICT:** Alice tries to reserve same file → `FILE_RESERVED` error (exit code 6)

### Cycle 10: Escalation — Alice Blocked
```
filament message send --from alice --to user --body "BLOCKED: proto/shared.proto reserved by Bob..." --type blocker
filament escalations
→ 2 escalations (question + blocker from alice)
```

### Cycle 11: Frank Resolves Conflict
- Frank mediates: Bob releases proto, Alice claims it
- Reservation handoff successful

### Cycle 12: Dave Completes Infra — Bob Hits PCI Blocker
- Dave closes infra-setup → **monitoring-setup unblocked**
- Bob sends blocker: PCI compliance review not approved
- payment-svc-impl marked `blocked`
- 3 escalations total

### Cycle 13: Phase 2 Progress
- Dave picks up monitoring-setup + notification-svc-impl
- Alice closes user-svc-impl + order-svc-impl
- **notification-svc-impl unblocked** (was waiting on user+order)
- api-gateway still blocked by payment-svc (PCI)

### Cycle 14: Eve Raises Quality Concern
- Eve reviews Alice's services: rate limiting missing on auth endpoints
- Escalation sent to user as `question` type
- 4 escalations total

### Cycle 15: PCI Unblocked + Context Bundle
- User resolves Bob's PCI blocker → payment-svc resumes
- User responds to Eve: rate limiting at gateway layer
- **Context bundle demo:** `context --around api-gateway-impl --depth 3` → 20 connected entities

### Cycle 16: Phase 3 Completes + Frank Vetoes Carol
- Bob closes payment-svc, Dave closes monitoring + notification
- Carol starts api-gateway → **Frank vetoes** (Express.js, should be axum)
- **Task reassigned** from Carol to Alice
- Eve starts security-audit

### Cycle 17: Gateway + Security Audit Done
- Carol releases gateway reservation → Alice claims it
- Alice completes gateway (axum + tower middleware)
- Eve completes security audit (2 minor findings)
- **Unblocked:** integration-tests, load-tests

### Cycle 18: Bug Found → Task Reopened → Fixed
- Eve finds duplicate email bug in notification-service during integration testing
- **Blocker escalation** (#5)
- notification-svc-impl **reopened** (status → in_progress)
- Dave fixes idempotency bug → closes notification-svc again
- Dave completes load-tests, Eve completes integration-tests

### Cycle 19: Phase 5 — Deploy Chain
- canary-deploy → full-deploy → post-mortem-doc
- 3 tasks closed in rapid succession (auto-dispatch pattern)
- Each task closure unblocked exactly one next task

### Cycle 20: Final Verification + Export/Import
```
filament task list --status closed → 15/15 tasks
filament task ready → "No ready tasks"
filament export → 90KB JSON (27 entities, 47 relations, 32 messages, 160 events)
filament import (new project) → round-trip verified
```

### Cycle 21: Clean Shutdown
```
filament stop → SIGTERM to PID 22475
Socket file removed (clean shutdown)
```

## Features Exercised

| Feature | Status | Notes |
|---------|--------|-------|
| Daemon mode (serve/stop) | PASS | All commands routed through Unix socket |
| Entity CRUD | PASS | 27 entities created, updated, inspected |
| Task lifecycle | PASS | open → in_progress → closed (+ blocked + reopened) |
| Diamond dependencies | PASS | Non-linear graph with parallel branches converging |
| task ready | PASS | Correctly computed unblocked tasks at each phase |
| critical-path | PASS | 6-step chain identified correctly |
| File reservations | PASS | Exclusive locks, TTL, conflict detection |
| Reservation conflict | PASS | FILE_RESERVED error (exit 6) on duplicate exclusive |
| Inter-agent messaging | PASS | 32 messages across 6 agents + user |
| Message types | PASS | text, artifact, question, blocker all used |
| Escalations | PASS | 5 escalations (3 blocker, 2 question) surfaced |
| Context bundles | PASS | Depth-3 BFS returned 20 connected entities |
| Task reassignment | PASS | api-gateway reassigned from carol to alice |
| Task reopening | PASS | notification-svc reopened after bug found |
| Export/Import | PASS | 90KB round-trip, all data preserved |
| Clean shutdown | PASS | Socket removed on SIGTERM |

## Not Exercised (Deferred)

| Feature | Reason |
|---------|--------|
| True parallel agents (separate processes) | Would need multiple terminal sessions |
| TTL expiry + auto-cleanup | Would need to wait for TTL to expire |
| Agent death simulation | Would need to kill a background process |
| Auto-dispatch (FILAMENT_AUTO_DISPATCH=1) | Simulated manually (close → ready → assign chain) |
| MCP server interaction | Requires external MCP client |
| TUI live observation | Requires separate terminal + screenshots |

## Complications Simulated

1. **PCI compliance blocker** — payment-svc blocked by external review, then unblocked
2. **Proto file reservation conflict** — Bob held exclusive lock, Alice couldn't reserve
3. **Quality concern escalation** — Eve raised rate limiting question, user resolved
4. **Tech lead veto** — Frank vetoed Carol's Express.js gateway, reassigned to Alice (axum)
5. **Bug found during integration testing** — notification duplicate emails, task reopened
6. **3-task deploy chain** — canary → full → post-mortem, sequential unblocking
