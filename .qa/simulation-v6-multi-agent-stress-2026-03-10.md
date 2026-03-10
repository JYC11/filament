# Simulation v6 Log: Multi-Agent Stress Test (Real Concurrent Processes)

**Date:** 2026-03-10
**Working dir:** `/tmp/filament-sim-v6/`
**Mode:** Daemon (concurrent access via Unix socket)
**Method:** tmux + multiple `claude -p` processes running simultaneously

## Scenario

5 agents migrate a Rails monolith to microservices across 12 tasks. Unlike sims 1-5 (single session, simulated agents), this uses **real concurrent Claude instances** hitting the same filament daemon via Unix socket. Each agent is a separate `claude -p` process with its own context, coordinating ONLY through filament messaging, reservations, and task state.

## Setup

- 5 agents: architect, backend-alpha, backend-beta, devops-eng, qa-lead
- 6 modules/services: monolith-app, auth-service, payment-service, api-gateway, k8s-infra, ci-pipeline
- 12 tasks with complex dependency graph (fan-out, fan-in, diamond, linear chain)
- 1 seeded lesson (database-per-service) for agents to discover
- 7 ownership/dependency relations between agents, modules, and services

## Execution Waves

### Wave 1: Architect (solo)
Single `claude -p` process. Design decomposition.

| Step | Action | Result |
|------|--------|--------|
| 1.1 | Searched knowledge base | Found seeded lesson (database-per-service) |
| 1.2 | Assigned + started design-decomposition | Status → in_progress |
| 1.3 | Designed service boundaries | Auth: 7 endpoints, Payment: 6 endpoints, Kong gateway JWT |
| 1.4 | Escalated question (strangler fig vs big bang) | Message type=question to user |
| 1.5 | Captured 2 lessons | auth-first-extraction, gateway-auth-contract |
| 1.6 | Reserved docs/architecture/** (exclusive) | TTL=1800 |
| 1.7 | Closed task, released reservation | **4 tasks unblocked** (fan-out) |

**Verdict: PASS** — Search-before-solve, escalation, lesson capture, reservation lifecycle, dependency fan-out.

### Wave 2: Alpha + Beta + DevOps (3 CONCURRENT processes)
Three `claude -p` instances launched simultaneously via tmux, all hitting the daemon.

| Step | Action | Result |
|------|--------|--------|
| 2.1 | Alpha reserved src/auth/** (exclusive) | Lock acquired |
| 2.2 | Alpha reserved src/shared/** (non-exclusive) | Lock acquired |
| 2.3 | Beta reserved src/payments/** (exclusive) | Lock acquired |
| 2.4 | Beta tried exclusive src/shared/** | Contention detected — alpha already held it |
| 2.5 | Beta messaged alpha about src/shared/** contention | Inter-agent messaging for conflict resolution |
| 2.6 | DevOps reserved infra/**, deploy/**, .github/** | 3 exclusive locks, no conflicts |
| 2.7 | Alpha searched lessons, found architect's learnings | Cross-wave knowledge transfer |
| 2.8 | Alpha sent beta auth token contract | JWT format, X-User-ID/X-User-Roles headers |
| 2.9 | Beta sent devops infra requirements | Redis, PostgreSQL encryption, Stripe webhook ingress |
| 2.10 | Alpha sent devops auth infra requirements | Separate DB, Kong config, RSA keypair |
| 2.11 | DevOps checked inbox, acknowledged both agents | Read messages from alpha and beta |
| 2.12 | All 3 escalated questions to user | OAuth scope, PCI compliance, infra sizing |
| 2.13 | Alpha captured 2 lessons | RS256-JWT, access/refresh token split |
| 2.14 | Beta captured 2 lessons | Idempotency-first, Stripe webhook bypass |
| 2.15 | DevOps captured 1 lesson | namespace-isolation-with-irsa |
| 2.16 | DevOps ran pagerank + degree | Correct analytics |
| 2.17 | Beta closed 2 tasks, DevOps closed 2 tasks | Alpha left auth-extraction in_progress (awaiting user) |
| 2.18 | DevOps released all reservations | Clean release |

**Verdict: PASS** — Real concurrent daemon access, file reservation contention (alpha blocked beta on src/shared/**), inter-agent messaging (3 agents exchanging requirements), cross-agent knowledge discovery, 4 escalations, 5 lessons captured concurrently.

### Wave 3: Architect (gateway + staging deploy)
Single process. Gateway configuration informed by lessons from Wave 2 agents.

| Step | Action | Result |
|------|--------|--------|
| 3.1 | Read lessons from alpha, beta, devops | Found RS256, webhook bypass, namespace isolation |
| 3.2 | Configured gateway referencing 3 lessons | JWT validation, webhook bypass, routing |
| 3.3 | Captured gateway lesson | Kong dual-auth pattern |
| 3.4 | Closed gateway + staging-deploy tasks | **2 tasks unblocked** (integration + load tests) |

**Verdict: PASS** — Cross-agent knowledge synthesis at gateway layer, lesson-informed architecture decisions.

### Wave 4: QA Lead (integration + load tests)
Single process. Knowledge synthesis across all 9 previous lessons.

| Step | Action | Result |
|------|--------|--------|
| 4.1 | Read ALL 9 lessons from 4 agents | Complete knowledge corpus review |
| 4.2 | Designed 41 integration test scenarios (6 suites) | Informed by lessons |
| 4.3 | Designed 7 load test scenarios with SLOs | Risk-based from lesson analysis |
| 4.4 | Identified 5 key risks from lessons | Auth SPOF, idempotency races, webhook flood, Redis failure, event bus gap |
| 4.5 | Captured meta-lesson | knowledge-driven-testing pattern |
| 4.6 | Sent test plan to architect | Summary with risks + coverage |
| 4.7 | Closed both test tasks | Production cutover unblocked |

**Verdict: PASS** — Full knowledge synthesis, risk identification from cross-agent lessons, meta-lesson capture.

### Wave 5: Production Cutover (final)
Single process. Close-out and analytics.

| Step | Action | Result |
|------|--------|--------|
| 5.1 | Reviewed all lessons + closed tasks | Full state verification |
| 5.2 | Go/no-go escalation to user | Summary with risk mitigations |
| 5.3 | Closed production-cutover | **All 12/12 tasks closed** |
| 5.4 | PageRank analysis | production-cutover (0.137) > staging-deploy (0.119) — correct convergence points |
| 5.5 | Degree centrality | staging-deploy (6) highest, architect (5) highest out-degree — correct coordination hub |
| 5.6 | Export final state | final-export.json written |
| 5.7 | Verified 0 active reservations | All released |

**Verdict: PASS** — Complete graph closure, analytics correctness, clean export.

## Features Exercised

| Feature | Tested | Result |
|---------|--------|--------|
| **Daemon concurrent access** | ✓ | 3 processes simultaneously via Unix socket |
| **Real `claude -p` multi-agent** | ✓ | 7 total agent launches across 5 waves |
| **Reservation contention** | ✓ | Beta blocked on src/shared/** by alpha |
| **Exclusive reservations** | ✓ | auth/**, payments/**, infra/**, deploy/**, .github/** |
| **Non-exclusive reservations** | ✓ | src/shared/** (alpha non-exclusive) |
| **Reservation release** | ✓ | All released, 0 active at end |
| **Inter-agent messaging** | ✓ | alpha→beta (token contract), beta→devops (infra reqs), alpha→devops (infra reqs), beta→alpha (contention), qa→architect (test plan) |
| **Escalations to user** | ✓ | 5 escalations: DB strategy, OAuth scope, PCI compliance, infra sizing, go/no-go |
| **User responses** | ✓ | 4 responses, agents proceeded on assumptions when no response yet |
| **Lesson capture** | ✓ | 10 lessons (1 seeded + 9 agent-created) |
| **Cross-agent knowledge transfer** | ✓ | Alpha read architect's lessons, architect read alpha+beta's lessons, QA read all 9 |
| **Search-before-solve** | ✓ | Every agent searched before starting work |
| **Dependency fan-out** | ✓ | design→4 tasks |
| **Dependency fan-in** | ✓ | 3 tasks→staging-deploy |
| **Diamond dependencies** | ✓ | auth-contracts + payment-contracts → gateway |
| **Linear chain** | ✓ | staging→tests→cutover |
| **Dynamic task state** | ✓ | Alpha left task in_progress (awaiting user), completed by coordinator |
| **PageRank analytics** | ✓ | Correct convergence point identification |
| **Degree centrality** | ✓ | Correct hub identification |
| **Export** | ✓ | Full state snapshot |
| **Ownership relations** | ✓ | Agent→module ownership |
| **Task assignment** | ✓ | Each agent self-assigned via `fl task assign` |

## Agent Communication Map

```
                  ┌──────────────┐
                  │     user     │
                  └──────┬───────┘
         ▲ questions│escalations ▼ responses
    ┌────┴─────────────────────────────────┐
    │                                       │
┌───┴────┐   token contract   ┌────────┐   │
│ alpha  │ ──────────────────→│  beta  │   │
│(auth)  │←── contention msg ─│(pay)   │   │
└───┬────┘                    └───┬────┘   │
    │ infra reqs                  │ infra   │
    └────────────┐    ┌───────────┘ reqs   │
                 ▼    ▼                     │
              ┌────────┐                    │
              │ devops │                    │
              └────────┘                    │
                                            │
┌──────────┐  test plan  ┌──────────┐       │
│ qa-lead  │ ──────────→ │architect │ ──────┘
└──────────┘             └──────────┘
```

## Key Differences from Sims 1-5

| Aspect | Sims 1-5 | Sim 6 |
|--------|----------|-------|
| Agent instances | Single session plays all roles | Real `claude -p` processes |
| Concurrency | Sequential (simulated) | Actual concurrent daemon access |
| Context isolation | Shared (single context) | Complete isolation per agent |
| Communication | Orchestrator knows all state | Agents genuinely only know what's in their inbox |
| Contention | Scripted | Emergent (beta's src/shared reservation failed naturally) |
| Knowledge transfer | Orchestrator ensures it | Agents independently searched and discovered lessons |
| Decision making | Scripted outcomes | Each agent made independent design decisions |

## Final Metrics

- **Total tasks:** 12 (all closed)
- **Total lessons:** 10 (1 seeded + 9 captured by agents)
- **Total messages:** 12+ inter-agent/user messages
- **Total escalations:** 5 (all addressed)
- **Total reservations:** 8 created, 0 remaining
- **Active file contention:** 1 instance (src/shared/**)
- **Claude instances used:** 7 (across 5 waves)
- **Concurrent processes:** 3 max (Wave 2)
- **Daemon uptime:** Full simulation duration

## Verdict: PASS

First real multi-agent simulation with concurrent Claude instances coordinating through filament's daemon. All features exercised successfully. The daemon handled concurrent access without errors. Agents independently discovered each other's knowledge, communicated requirements, and resolved file contention — all through filament's coordination primitives.
