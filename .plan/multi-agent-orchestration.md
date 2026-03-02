# Multi-Agent Orchestration Plan

Based on analysis of ~50+ sessions (Feb 23 – Mar 1) from the Koupang project.

---

## Current Workflow Analysis

### Work Mode Breakdown

| Mode | % of Sessions | Example Prompts |
|------|---------------|-----------------|
| Planning | ~25% | `/plan`, design discussions, plan revisions, architectural debates |
| Implementation | ~40% | "start the next bead", "do the next 2 tasks in parallel" |
| Review/QA | ~15% | "come back with observations", test optimization, concurrency tests |
| Chore/Docs | ~15% | CLAUDE.md updates, script creation, deduplication, memory cleanup |
| Task mgmt | ~5% | Creating beads, priority management, dependency wiring |

### Pain Points Identified

1. **Context exhaustion** — frequent "end session, start fresh" (~2-3 restarts per work block)
2. **Sequential bottleneck** — "do the next bead" one at a time even when multiple are unblocked
3. **Manual coordination overhead** — user is the human router between planning, implementation, and review
4. **Doc maintenance tax** — "update relevant CLAUDE.md files" appears in almost every session
5. **Context re-acquisition cost** — every new session starts with `/project-context` + re-reading plans/beads

### Current Daily Loop

```
1. /project-context         ← manual context load
2. /br                      ← check tasks manually
3. "do the next bead"       ← sequential, one at a time
4. review code              ← manual
5. "update CLAUDE.md"       ← manual doc maintenance
6. commit                   ← manual
7. "end session, start fresh" ← context exhaustion
```

---

## Approach 1: Enhanced Claude Code (No New Tooling)

Maximize existing setup by making it more parallel.

### A. Dispatcher Skill (`/dispatch`)

Reads unblocked beads and launches parallel subagents in worktrees:

```
# Instead of:
"do the next bead"
"cool, do the next bead"
"let's do the next 2 in parallel"

# One command:
"/dispatch"
# → reads br, identifies unblocked beads, launches N parallel subagents in worktrees
```

### B. Auto-Doc Hook

Post-commit hook spawning a background agent to update CLAUDE.md files automatically.

### C. Post-Implementation Pipeline Skill (`/post-impl`)

Runs tests → code review → doc update as an automated pipeline after implementation.

**Pros**: Zero new infrastructure, builds on existing setup.
**Cons**: Still single orchestrator (user), limited true parallelism.

---

## Approach 2: Shell Script Orchestrator

A shell script is the natural fit — the orchestrator just glues together three CLIs (`claude`, `br`, `git`).

### Architecture

```
┌──────────────────────────────────────┐
│       dispatch.sh (orchestrator)      │
│  Reads beads → assigns to agents     │
│  Manages worktrees and merges        │
│  Aggregates results                   │
├──────────┬──────────┬────────────────┤
│ Planner  │ Coder x2 │ Reviewer       │
│ Agent    │ Agents   │ Agent          │
│          │(worktree)│                │
└──────────┴──────────┴────────────────┘
        ▲                    │
        │    beads_rust      │
        └────(shared state)──┘
```

### Core Idea

```
br query (unblocked tasks) → git worktree (isolation) → claude -p (execution) → merge back
```

### Agent Roles

| Agent | Focus | Tools | Trigger |
|-------|-------|-------|---------|
| Planner | Architecture, plan mode only, never writes code | Read, Grep, Glob, Write (plans only) | New feature request or plan revision |
| Coder (x2-3) | Implementation from plans, one bead at a time | Full toolset, runs in worktree | Unblocked bead available |
| Reviewer | Code review, test analysis, no code writing | Read, Grep, Bash (tests only) | Bead marked as done |
| DocKeeper | CLAUDE.md + MEMORY.md maintenance only | Read, Write, Edit | Post-commit or on-demand |

### Key Claude Code CLI Flags

| Flag | Purpose |
|---|---|
| `claude -p "prompt"` | Non-interactive mode — send prompt, get result, exit |
| `claude -p "prompt" --cwd /path` | Run in a specific directory (e.g., a worktree) |
| `claude --model sonnet` | Use cheaper/faster model for routine tasks |
| `claude --output-format json` | Structured output for parsing results |
| `claude --allowedTools "Read,Write,Edit,Bash"` | Restrict tools per agent role |
| `claude --systemPrompt "..."` | Custom system prompt per agent role |

### Full Script

```bash
#!/usr/bin/env bash
# dispatch.sh — parallel multi-agent orchestrator
# Usage: ./dispatch.sh [--max-parallel N] [--role coder|reviewer|dockeeper] [--dry-run]

set -euo pipefail

PROJECT_DIR="${PROJECT_DIR:-$(pwd)}"
MAX_PARALLEL="${MAX_PARALLEL:-2}"
WORKTREE_BASE="$PROJECT_DIR/.claude/worktrees"

# ─── Agent Role Definitions ───

CODER_SYSTEM="You are an implementation agent. You receive a single task (bead).
Implement it fully: write code, run tests, fix failures. Do not modify CLAUDE.md files.
Do not work on anything outside the task scope. Commit when done."

REVIEWER_SYSTEM="You are a review agent. You receive a git diff.
Check for: scope creep, missing tests, pattern violations, security issues.
Output a short pass/fail verdict with reasoning. Do not modify any files."

DOCKEEPER_SYSTEM="You are a documentation agent.
Update all relevant CLAUDE.md files to reflect recent code changes.
Keep docs compact — tables and trees over prose. Do not modify source code."

# ─── Step 1: Get unblocked beads ───

get_unblocked_beads() {
    cd "$PROJECT_DIR"
    br query --status open --format json 2>/dev/null \
        | jq -r '.[] | select(.blocked_by | length == 0) | .id'
}

# ─── Step 2: Create worktree per bead ───

create_worktree() {
    local bead_id="$1"
    local worktree_path="$WORKTREE_BASE/$bead_id"
    local branch_name="agent/$bead_id"

    mkdir -p "$WORKTREE_BASE"
    git -C "$PROJECT_DIR" worktree add -b "$branch_name" "$worktree_path" HEAD 2>/dev/null
    echo "$worktree_path"
}

# ─── Step 3: Run agent in worktree ───

run_agent() {
    local bead_id="$1"
    local worktree_path="$2"
    local role="${3:-coder}"
    local system_prompt

    case "$role" in
        coder)    system_prompt="$CODER_SYSTEM" ;;
        reviewer) system_prompt="$REVIEWER_SYSTEM" ;;
        dockeeper) system_prompt="$DOCKEEPER_SYSTEM" ;;
    esac

    # Get bead description for the prompt
    local bead_desc
    bead_desc=$(cd "$PROJECT_DIR" && br show "$bead_id" 2>/dev/null)

    # Run Claude non-interactively in the worktree
    claude -p "Implement this task:\n\n$bead_desc" \
        --cwd "$worktree_path" \
        --systemPrompt "$system_prompt" \
        --output-format json \
        2>"$WORKTREE_BASE/$bead_id.stderr" \
        >"$WORKTREE_BASE/$bead_id.result.json"
}

# ─── Step 4: Merge and cleanup ───

merge_worktree() {
    local bead_id="$1"
    local worktree_path="$WORKTREE_BASE/$bead_id"
    local branch_name="agent/$bead_id"

    cd "$PROJECT_DIR"

    # Check if agent made any commits
    local new_commits
    new_commits=$(git log HEAD.."$branch_name" --oneline 2>/dev/null | wc -l | tr -d ' ')

    if [ "$new_commits" -gt 0 ]; then
        echo "[merge] $bead_id: $new_commits new commit(s)"
        git merge --no-ff "$branch_name" -m "Merge agent/$bead_id"
    else
        echo "[skip] $bead_id: no commits"
    fi

    # Cleanup
    git worktree remove "$worktree_path" 2>/dev/null || true
    git branch -d "$branch_name" 2>/dev/null || true
}

# ─── Main Loop ───

main() {
    local beads
    beads=$(get_unblocked_beads)

    if [ -z "$beads" ]; then
        echo "No unblocked beads found."
        exit 0
    fi

    echo "=== Unblocked beads ==="
    echo "$beads"
    echo ""

    # Launch agents in parallel (up to MAX_PARALLEL)
    local pids=()
    local bead_ids=()
    local count=0

    for bead_id in $beads; do
        if [ "$count" -ge "$MAX_PARALLEL" ]; then
            # Wait for any one to finish before launching more
            wait -n "${pids[@]}" 2>/dev/null || true
        fi

        echo "[launch] $bead_id"
        local worktree_path
        worktree_path=$(create_worktree "$bead_id")

        run_agent "$bead_id" "$worktree_path" "coder" &
        pids+=($!)
        bead_ids+=("$bead_id")
        ((count++))
    done

    # Wait for all agents to finish
    echo "Waiting for ${#pids[@]} agent(s)..."
    wait "${pids[@]}" 2>/dev/null || true

    # Merge results
    echo ""
    echo "=== Merging results ==="
    for bead_id in "${bead_ids[@]}"; do
        merge_worktree "$bead_id"
    done

    # Optional: run review agent on the merged diff
    echo ""
    echo "=== Running review ==="
    local diff
    diff=$(git diff HEAD~${#bead_ids[@]}..HEAD)
    if [ -n "$diff" ]; then
        claude -p "Review this diff:\n\n$diff" \
            --systemPrompt "$REVIEWER_SYSTEM" \
            --output-format text
    fi

    echo ""
    echo "Done. ${#bead_ids[@]} bead(s) processed."
}

main "$@"
```

### Simpler Variants

#### Sequential dispatcher (no worktrees)

For projects where parallel worktrees are overkill — automates the "do the next bead" loop:

```bash
#!/usr/bin/env bash
# dispatch-simple.sh — sequential bead executor
cd "${PROJECT_DIR:-$(pwd)}"

for bead_id in $(br query --status open --format json | jq -r '.[] | select(.blocked_by | length == 0) | .id'); do
    bead_desc=$(br show "$bead_id")
    echo "=== Working on $bead_id ==="

    claude -p "Implement this task, run tests, commit when done:\n\n$bead_desc"

    echo "=== Review $bead_id? (y/n/q) ==="
    read -r answer
    case "$answer" in
        q) break ;;
        n) continue ;;
        y) claude -p "Review the last commit for scope creep and pattern violations" ;;
    esac
done
```

#### Doc-update-only

Run after any implementation session to auto-update docs:

```bash
#!/usr/bin/env bash
# auto-doc.sh — update CLAUDE.md files based on recent changes
cd "${PROJECT_DIR:-$(pwd)}"

diff=$(git diff HEAD~1..HEAD --stat)
claude -p "These files changed recently:\n\n$diff\n\nUpdate all relevant CLAUDE.md files to reflect these changes. Keep docs compact." \
    --systemPrompt "$DOCKEEPER_SYSTEM"
```

### Why Shell Over Python/Rust

| Concern | Shell | Python/Rust |
|---|---|---|
| Dependencies | None (claude, br, git, jq) | SDK install, venv/cargo |
| Portability | Works on any project | Needs per-project setup |
| Complexity | ~80 lines for full orchestrator | ~200+ lines for same thing |
| Debugging | `bash -x dispatch.sh` | Proper debugger needed |
| JSON parsing | `jq` (already installed) | Native but overkill |
| Parallelism | `&` + `wait` | Async runtime / threads |
| Limitation | Complex error recovery | Better error handling |

### When to Graduate to Python/Rust

- Need sophisticated retry logic per agent
- Need agent-to-agent communication mid-task
- Need persistent state across orchestrator runs
- Need to parse/route structured agent output

For the current workflow, shell covers 95% of needs.

---

## Making It General-Purpose

The script is project-agnostic if:
1. Task source is pluggable: `br query` can be swapped for any task CLI (GitHub Issues, Linear, Jira CLI)
2. Agent prompts come from a config file, not hardcoded
3. Project dir is passed as env/arg, not assumed

```bash
# Config file: .claude/agents.conf
TASK_CMD="br query --status open --format json"
TASK_PARSE="jq -r '.[] | select(.blocked_by | length == 0) | .id'"
TASK_SHOW="br show"
CODER_PROMPT_FILE=".claude/prompts/coder.md"
REVIEWER_PROMPT_FILE=".claude/prompts/reviewer.md"
```

---

## Recommended Rollout

### Phase 1: New Skills (Immediate)

| Skill | What it automates |
|-------|-------------------|
| `/dispatch` | Reads unblocked beads, launches parallel subagents in worktrees |
| `/auto-doc` | Updates all relevant CLAUDE.md files based on recent changes |
| `/post-impl` | Runs tests + code review + doc update as a pipeline after implementation |

### Phase 2: Shell Orchestrator (For Order/Payment Saga)

The 35-bead dependency DAG across 4 plans is the perfect test case:

- Plan 1 (shared infra) beads run in sequence
- Plan 2 (cart) runs fully parallel to Plan 1 once dependencies resolve
- Plan 3 (order+payment) depends on Plan 1 completion
- Plan 4 (docs) runs as background agent after each milestone

### Target Daily Loop

```
1. /dispatch                ← auto-loads context, identifies parallelizable beads,
                              launches N subagents in worktrees
2. review diffs             ← agents come back with completed work
3. /post-impl               ← automated: tests → review → doc update → commit
4. repeat                   ← no session restarts; subagents have fresh context
```

### Key Win

Subagents don't share the main context window. Each starts fresh with just CLAUDE.md + bead description. Context exhaustion stops being a problem because heavy lifting happens in isolated subagent contexts.

---

## Phase 3: Inter-Agent Communication

The dispatch script (Phase 2) treats agents as fire-and-forget: launch, wait, merge. But real workflows need agents to talk to each other.

### When Do Agents Need to Communicate?

Scenarios from actual Koupang sessions:

| Scenario | From | To | What's communicated |
|---|---|---|---|
| Coder hits a design question | Coder | Planner | "Should inventory use optimistic or pessimistic locking?" |
| Reviewer finds issue | Reviewer | Coder | "Missing error handling on payment timeout path" |
| Coder discovers shared code needed | Coder A | Coder B | "I created `shared::outbox::types` — you'll need it too" |
| Implementation reveals plan gap | Coder | Planner | "Plan says 'use Redis' but no Redis config exists yet" |
| DocKeeper needs change context | Coder | DocKeeper | "Added 3 new endpoints, changed auth model" |
| Coder finishes dependency | Coder | Orchestrator | "bead-3cu done — bead-3vv is now unblocked" |

### Communication Patterns

#### Pattern 1: Mailbox Files (Simplest)

Each agent gets a mailbox directory. Agents write messages as files, orchestrator routes them.

```
.claude/mailbox/
├── coder-bd-3cu/
│   ├── outbox.md          ← messages FROM this agent
│   └── inbox.md           ← messages TO this agent
├── reviewer/
│   ├── outbox.md
│   └── inbox.md
└── orchestrator/
    └── events.log         ← append-only event stream
```

Agent system prompts include:
```
When you encounter a blocker or question for another agent, write it to
your outbox file at .claude/mailbox/<your-id>/outbox.md in this format:

## Message
- **to**: planner | coder | reviewer | orchestrator
- **type**: question | blocker | artifact | status
- **body**: <your message>
```

The orchestrator polls mailboxes between agent runs and routes messages into the right inbox before the next agent invocation.

```bash
route_messages() {
    for outbox in "$MAILBOX_DIR"/*/outbox.md; do
        local agent_dir=$(dirname "$outbox")
        local agent_id=$(basename "$agent_dir")

        # Parse messages and route to recipients
        while IFS= read -r line; do
            if [[ "$line" =~ \*\*to\*\*:\ (.+) ]]; then
                local target="${BASH_REMATCH[1]}"
                # Append to target's inbox
                cat "$outbox" >> "$MAILBOX_DIR/$target/inbox.md"
            fi
        done < "$outbox"

        # Clear processed outbox
        > "$outbox"
    done
}
```

**Pros**: Dead simple, human-readable, debuggable with `cat`.
**Cons**: Only works between agent runs (not mid-execution), polling-based.

#### Pattern 2: Structured Event Log (Medium)

Single append-only log file. All agents write events, orchestrator reacts.

```bash
# .claude/agent-events.jsonl  (one JSON object per line)
{"ts":"2026-03-02T14:30:00Z","agent":"coder-bd-3cu","type":"completed","bead":"bd-3cu","artifacts":["shared/src/outbox/types.rs"]}
{"ts":"2026-03-02T14:30:05Z","agent":"coder-bd-3cu","type":"unblocked","beads":["bd-3vv","bd-2sx"]}
{"ts":"2026-03-02T14:31:00Z","agent":"coder-bd-3vv","type":"question","to":"planner","body":"Should relay use at-least-once or exactly-once?"}
{"ts":"2026-03-02T14:35:00Z","agent":"reviewer","type":"issue","bead":"bd-3cu","severity":"warn","body":"No test for concurrent claim_batch"}
```

Orchestrator logic becomes event-driven:

```bash
process_events() {
    tail -n +$LAST_PROCESSED "$EVENT_LOG" | while read -r event; do
        local type=$(echo "$event" | jq -r '.type')
        local agent=$(echo "$event" | jq -r '.agent')

        case "$type" in
            completed)
                local bead=$(echo "$event" | jq -r '.bead')
                br close "$bead"
                # Check if new beads are unblocked
                schedule_newly_unblocked
                # Trigger review
                launch_reviewer "$bead"
                ;;
            question)
                local to=$(echo "$event" | jq -r '.to')
                local body=$(echo "$event" | jq -r '.body')
                # Route to appropriate agent or escalate to user
                if [ "$to" = "planner" ]; then
                    launch_planner_with_question "$body"
                else
                    echo "[ESCALATE] $agent asks: $body"
                fi
                ;;
            issue)
                local severity=$(echo "$event" | jq -r '.severity')
                local bead=$(echo "$event" | jq -r '.bead')
                if [ "$severity" = "error" ]; then
                    # Re-launch coder with the review feedback
                    relaunch_coder_with_feedback "$bead" "$event"
                fi
                ;;
            blocker)
                echo "[BLOCKED] $agent: $(echo "$event" | jq -r '.body')"
                # Pause agent's bead, notify user
                ;;
        esac
    done
}
```

**Pros**: Full audit trail, event-driven reactions, `jq`-parseable.
**Cons**: More complex orchestrator, still between-runs only.

#### Pattern 3: Agent Handoff Protocol (Most Structured)

Each `claude -p` invocation returns structured JSON output. The orchestrator parses it to determine next actions.

Define a standard output schema that all agents must follow:

```json
{
  "status": "completed|blocked|needs_review|needs_input",
  "bead_id": "bd-3cu",
  "summary": "Implemented outbox types module with 5 event types",
  "artifacts": {
    "files_created": ["shared/src/outbox/types.rs"],
    "files_modified": ["shared/src/lib.rs"],
    "tests_added": 8,
    "tests_passing": true
  },
  "messages": [
    {
      "to": "reviewer",
      "body": "Concurrency handling in claim_batch needs careful review"
    },
    {
      "to": "coder-bd-3vv",
      "body": "I exported OutboxEvent and OutboxStatus from shared::outbox — use those types"
    }
  ],
  "blockers": [],
  "questions": [
    {
      "to": "planner",
      "body": "Should we add partition support now or defer?",
      "options": ["now", "defer to P3 bead"]
    }
  ]
}
```

Agent system prompts enforce this:
```
After completing your work, output a JSON summary as the LAST thing you write.
The JSON must follow this schema: [schema above]
This allows the orchestrator to route your results to other agents.
```

Orchestrator parses and routes:

```bash
process_agent_output() {
    local bead_id="$1"
    local result_file="$WORKTREE_BASE/$bead_id.result.json"

    local status=$(jq -r '.status' "$result_file")
    local messages=$(jq -c '.messages[]?' "$result_file")
    local questions=$(jq -c '.questions[]?' "$result_file")

    case "$status" in
        completed)
            merge_worktree "$bead_id"
            # Route messages to other agents
            echo "$messages" | while read -r msg; do
                local to=$(echo "$msg" | jq -r '.to')
                local body=$(echo "$msg" | jq -r '.body')
                append_to_inbox "$to" "$bead_id" "$body"
            done
            # Launch reviewer
            launch_reviewer "$bead_id"
            ;;
        blocked)
            local blockers=$(jq -r '.blockers[]' "$result_file")
            echo "[BLOCKED] $bead_id: $blockers"
            # Escalate to user
            ;;
        needs_input)
            # Route questions — to planner agent or escalate to user
            echo "$questions" | while read -r q; do
                local to=$(echo "$q" | jq -r '.to')
                if [ "$to" = "user" ]; then
                    echo "[QUESTION] $(echo "$q" | jq -r '.body')"
                    echo "Options: $(echo "$q" | jq -r '.options | join(", ")')"
                else
                    launch_agent_with_question "$to" "$q"
                fi
            done
            ;;
    esac
}
```

**Pros**: Fully structured, orchestrator can make smart routing decisions, composable.
**Cons**: Relies on LLM producing valid JSON (needs validation/fallback), most complex.

### Communication Escalation Model

Not everything should be agent-to-agent. Some things need the user:

```
Level 0: Agent handles internally     (compile error → fix it)
Level 1: Agent-to-agent via mailbox   (artifact sharing, status updates)
Level 2: Orchestrator routes           (unblock next bead, trigger review)
Level 3: Escalate to user             (design questions, ambiguous requirements)
```

The orchestrator decides escalation level:

```bash
# Questions about implementation details → route to planner agent
# Questions about requirements or preferences → escalate to user
# Blockers that another agent can resolve → route to that agent
# Blockers that need human decision → escalate to user

escalate() {
    local question="$1"
    local from="$2"

    # Simple heuristic: if the question mentions "should we" or "prefer" → user
    if echo "$question" | grep -qiE "should (we|I)|prefer|trade-?off|which approach"; then
        echo "[USER] $from asks: $question"
        # Could use terminal-notifier, osascript, or just print
    else
        # Route to planner agent
        launch_planner_with_question "$question"
    fi
}
```

### Shared Context Between Agents

Agents working on related beads need shared context without sharing a context window. Options:

#### A. CLAUDE.md as shared memory (already have this)

Agents read CLAUDE.md for project state. DocKeeper agent updates it after merges. This is the "eventual consistency" model — agents get slightly stale context but it's usually good enough.

#### B. Bead-specific context files

When assigning a bead, the orchestrator generates a context bundle:

```bash
prepare_context() {
    local bead_id="$1"
    local worktree="$2"
    local context_file="$worktree/.claude/bead-context.md"

    cat > "$context_file" << EOF
# Context for $bead_id

## Bead Description
$(br show "$bead_id")

## Dependencies (already completed)
$(br query --blocks "$bead_id" --status closed --format brief)

## Artifacts from dependencies
$(for dep in $(br query --blocks "$bead_id" --status closed --format json | jq -r '.[].id'); do
    echo "### $dep"
    cat "$MAILBOX_DIR/coder-$dep/outbox.md" 2>/dev/null || echo "(no output)"
done)

## Related files changed recently
$(git log --oneline -10 --name-only | head -30)
EOF
}
```

This way, when Coder B starts on a bead that depends on Coder A's work, it automatically gets Coder A's output summary as context.

#### C. Artifact registry

Simple file tracking what each agent produced:

```bash
# .claude/artifacts.jsonl
{"bead":"bd-3cu","agent":"coder","files":["shared/src/outbox/types.rs"],"exports":["OutboxEvent","OutboxStatus"]}
{"bead":"bd-w54","agent":"coder","files":["shared/migrations/"],"exports":["outbox_events table"]}
```

Orchestrator includes relevant artifacts in downstream agent prompts:

```bash
get_upstream_artifacts() {
    local bead_id="$1"
    local deps=$(br query --blocks "$bead_id" --format json | jq -r '.[].id')
    for dep in $deps; do
        jq -r "select(.bead == \"$dep\")" "$ARTIFACT_REGISTRY"
    done
}
```

### Putting It Together: Communication-Aware Dispatch

Upgraded dispatch loop incorporating communication:

```
┌─────────────┐
│  Orchestrator │
│              │
│  1. Read beads (br query)
│  2. Build context bundles (dependency artifacts + CLAUDE.md)
│  3. Launch agents in worktrees
│  4. Poll event log / parse output JSON
│  5. Route messages between agents
│  6. Escalate to user when needed
│  7. Merge completed work
│  8. Update beads + trigger downstream
│  9. Repeat until no unblocked beads
│              │
│  Communication layer:
│  - Mailbox files (simple async messages)
│  - Event log (audit trail + reactive triggers)
│  - Structured output (handoff protocol)
│  - Artifact registry (dependency context)
└─────────────┘
```

### Recommended Build Order for Communication

| Step | What | Complexity | Unlocks |
|---|---|---|---|
| 1 | Structured JSON output from agents | Low — just system prompt change | Orchestrator can parse results |
| 2 | Artifact registry | Low — append-only JSONL file | Downstream agents get dependency context |
| 3 | Event log | Medium — orchestrator needs event loop | Reactive scheduling, audit trail |
| 4 | Mailbox routing | Medium — message parsing + routing | Agent-to-agent async communication |
| 5 | Escalation heuristics | Medium — pattern matching on questions | Smart user-vs-agent routing |
| 6 | Bead context bundles | Low — template + jq | Agents start with full dependency context |

Steps 1, 2, and 6 are quick wins that don't change the orchestrator loop much. Steps 3-5 require the orchestrator to become event-driven rather than batch-oriented.

---

## Alternative Topology: Multiple Live Claude Instances

The dispatch script uses `claude -p` (non-interactive, fire-and-forget). But you could instead run multiple **interactive** Claude sessions simultaneously — each in its own terminal, each with its own context window.

### Subagents vs Live Instances

| | `claude -p` (subagents) | Multiple `claude` sessions |
|---|---|---|
| **Lifecycle** | Spawned per bead, exits when done | Long-running, handles multiple beads |
| **Interactivity** | None — fire and forget | Full — user can intervene mid-task |
| **Context window** | Fresh per invocation (no buildup) | Persists across beads (accumulates context) |
| **User oversight** | Post-hoc (review output files) | Real-time (watch each terminal) |
| **Questions** | Must be routed via orchestrator | Agent asks user directly in its terminal |
| **Orchestration** | Shell script manages everything | User + filesystem coordination |
| **Parallelism** | Automatic via `&` + `wait` | Manual (user decides what to assign where) |
| **Cost per bead** | Context load on every invocation | Amortized — one context load, many beads |

### When Live Instances Win

- **Design-heavy work** where agents need back-and-forth with the user (the order/payment saga planning)
- **Related beads** that share context — an instance that just finished bead A already has the context for bead B
- **Debugging sessions** where you want to watch the agent work and course-correct in real time
- **Long-running roles** like a dedicated reviewer that reviews everything as it comes in

### When Subagents Win

- **Independent beads** with no shared context — clean slate is actually better
- **Batch processing** — 10 unblocked beads, just run them all
- **Unattended execution** — walk away and come back to results
- **Routine work** — doc updates, test runs, linting — no user oversight needed

### Hybrid: Live Orchestrator + Subagent Workers

The most practical setup: **you run one interactive Claude session as the orchestrator**, and it spawns `claude -p` subagents for the grunt work.

```
┌─────────────────────────────────────────────────┐
│  Terminal 1: Interactive Claude (you + orchestrator)  │
│                                                       │
│  You: "/dispatch"                                     │
│  Claude: reads beads, launches subagents              │
│  Claude: "3 agents working: bd-3cu, bd-w54, bd-2sx"   │
│  Claude: "bd-3cu completed. Review? [y/n]"            │
│  You: "y"                                             │
│  Claude: launches reviewer subagent on bd-3cu         │
│  Claude: "Reviewer passed. bd-3vv now unblocked."     │
│  You: "launch it"                                     │
│  Claude: launches coder subagent for bd-3vv           │
│                                                       │
│  Subagents (background):                              │
│  ├── claude -p "implement bd-3cu" --cwd worktree-1    │
│  ├── claude -p "implement bd-w54" --cwd worktree-2    │
│  └── claude -p "review bd-3cu" --cwd worktree-1       │
└─────────────────────────────────────────────────┘
```

This gives you:
- **One place to watch** — your main terminal
- **Real-time control** — you decide what launches next
- **Subagent isolation** — workers don't pollute your context window
- **Interactive planning** — you can `/plan` in the main session while workers execute

### Multi-Terminal Setup (Full Interactive)

For maximum oversight, run dedicated instances in separate terminals:

```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ Terminal 1        │  │ Terminal 2        │  │ Terminal 3        │
│ PLANNER           │  │ CODER-A           │  │ CODER-B           │
│ --cwd /koupang    │  │ --cwd worktree-a  │  │ --cwd worktree-b  │
│                   │  │                   │  │                   │
│ Focus: arch       │  │ Focus: bd-3cu     │  │ Focus: bd-w54     │
│ decisions, plan   │  │ outbox types      │  │ test migrations   │
│ reviews, answers  │  │                   │  │                   │
│ design questions  │  │                   │  │                   │
└──────────────────┘  └──────────────────┘  └──────────────────┘
         │                     │                      │
         └─────────────────────┴──────────────────────┘
                          shared filesystem
                    (.claude/mailbox/, artifacts.jsonl)
```

How it works in practice:

```
# Terminal 1 (Planner) — you start it with a role-specific system prompt
claude --systemPrompt "$(cat .claude/prompts/planner.md)" --cwd /koupang

# Terminal 2 (Coder A) — in a worktree
git worktree add .claude/worktrees/bd-3cu -b agent/bd-3cu
claude --systemPrompt "$(cat .claude/prompts/coder.md)" --cwd .claude/worktrees/bd-3cu

# Terminal 3 (Coder B) — in another worktree
git worktree add .claude/worktrees/bd-w54 -b agent/bd-w54
claude --systemPrompt "$(cat .claude/prompts/coder.md)" --cwd .claude/worktrees/bd-w54
```

### Communication Between Live Instances

All the communication patterns from Phase 3 still apply. The key difference: live instances can **poll continuously** rather than only communicate between runs.

#### File Watcher Pattern

Each agent's system prompt includes instructions to check its inbox before starting new work:

```markdown
# In each agent's system prompt:

Before starting any new task, check your inbox:
  cat .claude/mailbox/<your-role>/inbox.md

After completing a task, write to your outbox:
  Write to .claude/mailbox/<your-role>/outbox.md

After writing to your outbox, append to the event log:
  Append to .claude/agent-events.jsonl
```

The user (or a background `fswatch` script) monitors the event log and tells agents about messages:

```bash
# watcher.sh — runs in a background terminal
# Monitors the event log and sends desktop notifications

fswatch -o .claude/agent-events.jsonl | while read; do
    last_event=$(tail -1 .claude/agent-events.jsonl)
    type=$(echo "$last_event" | jq -r '.type')
    agent=$(echo "$last_event" | jq -r '.agent')
    body=$(echo "$last_event" | jq -r '.body // .summary // empty')

    case "$type" in
        completed)
            terminal-notifier -title "Agent: $agent" -message "Completed: $body"
            ;;
        question)
            to=$(echo "$last_event" | jq -r '.to')
            terminal-notifier -title "Question for $to" -message "$body" -sound default
            ;;
        blocker)
            terminal-notifier -title "BLOCKED: $agent" -message "$body" -sound Basso
            ;;
    esac
done
```

#### Session Resume for Continuity

If a coder instance finishes a bead and the next unblocked bead is related, you can keep the same session going (context carries over). If unrelated, start a fresh session:

```
# Same instance, next bead (context carries over):
You: "bd-3cu is done. Next unblocked bead is bd-3vv (outbox relay) which depends on it. Start."

# vs. fresh instance (clean context):
# Kill terminal 2, start new one for unrelated bead
claude --systemPrompt "$(cat .claude/prompts/coder.md)" --cwd .claude/worktrees/bd-xyz
```

### tmux Layout for Multi-Instance

Automate the multi-terminal setup:

```bash
#!/usr/bin/env bash
# multi-agent-tmux.sh — sets up a tmux session with agent panes

SESSION="agents"
PROJECT_DIR="${PROJECT_DIR:-$(pwd)}"

tmux new-session -d -s "$SESSION" -n "orchestrator"

# Pane 0: Orchestrator / Planner (interactive)
tmux send-keys -t "$SESSION:0" "cd $PROJECT_DIR && claude" C-m

# Pane 1: Coder A (worktree)
tmux split-window -h -t "$SESSION:0"
tmux send-keys -t "$SESSION:0.1" "cd $PROJECT_DIR" C-m

# Pane 2: Coder B (worktree)
tmux split-window -v -t "$SESSION:0.1"
tmux send-keys -t "$SESSION:0.2" "cd $PROJECT_DIR" C-m

# Pane 3: Event watcher
tmux new-window -t "$SESSION" -n "events"
tmux send-keys -t "$SESSION:1" "cd $PROJECT_DIR && tail -f .claude/agent-events.jsonl | jq ." C-m

tmux select-window -t "$SESSION:0"
tmux select-pane -t 0
tmux attach -t "$SESSION"
```

### Decision Matrix: Which Topology When

| Scenario | Best Topology |
|---|---|
| 10 independent beads, routine implementation | `claude -p` subagents (batch) |
| 3 related beads building on each other | 1 live instance, sequential |
| Designing a new service (lots of back-and-forth) | 1 live planner instance |
| Order saga: infra + cart + payment in parallel | 2-3 live instances in worktrees |
| Doc updates after a merge | `claude -p` subagent (fire-and-forget) |
| Code review of complex diff | 1 live reviewer instance |
| Full pipeline: plan → implement → review → doc | Hybrid: live orchestrator + subagent workers |

### Key Insight

The communication infrastructure (mailbox, event log, artifact registry) is **topology-agnostic**. It works the same whether agents are:
- Subagents spawned by `claude -p`
- Live interactive sessions in separate terminals
- A mix of both

The filesystem IS the message bus. Every pattern uses files that any Claude instance can read/write regardless of how it was launched.

---

## Knowledge Graph as Shared Agent Memory

> Reference: `~/.claude/plans/knowledge-graph-cli-plan.md` — full design for the `kg` tool.

The knowledge graph (`kg`) was already designed for concurrent multi-agent access: a daemon (`kg-server`) owns the graph behind a `tokio::sync::RwLock`, agents connect via Unix socket, multiple readers can traverse simultaneously, writes are serialized. This makes it a natural fit as the shared memory layer for multi-agent orchestration.

### How KG Replaces/Augments the File-Based Communication Layer

The Phase 3 communication patterns (mailbox, event log, artifact registry) are all flat files. The KG can subsume most of them:

| File-Based Pattern | KG Equivalent | Advantage |
|---|---|---|
| Mailbox files | Agent writes entity: `kg add "msg-from-coder-A" --type message --summary "exported OutboxEvent"` | Queryable, no polling directory |
| Artifact registry (JSONL) | `kg add "bd-3cu" --type bead --facts '{"files":["outbox/types.rs"],"exports":["OutboxEvent"]}'` | Agents query with `kg context --around "bd-3vv" --depth 1` to find upstream artifacts |
| Event log | `kg relate "coder-A" "completed" "bd-3cu" --summary "30 tests, committed 607d759"` | Events become traversable graph edges, not a flat log to grep |
| CLAUDE.md as shared memory | `kg context --around "catalog" --depth 2` | Structured, queryable, no stale-doc problem — agents update entities as they work |
| Bead context bundles | `kg context --around "bd-3vv" --depth 2` auto-includes dependency beads, their artifacts, related modules | No manual context file generation |

### Concurrent Access Patterns

From the KG design, the concurrency model handles multi-agent scenarios natively:

```
Agent A (Coder): kg add "OutboxRelay" --type module --summary "Polls outbox, publishes to Kafka"
                 ↓ (write lock, serialized)

Agent B (Coder): kg context --around "OutboxEvent" --depth 2
                 ↓ (read lock, concurrent with other reads)
                 → sees OutboxRelay immediately if A's write completed
                 → sees stale graph if A's write is in-flight (consistent snapshot)

Agent C (Reviewer): kg inspect "OutboxRelay" "OutboxEvent"
                    ↓ (read lock, concurrent with B)
                    → Tier 2 key_facts for both entities
```

Key properties:
- **No clashes**: daemon serializes writes via `RwLock`. Two agents adding entities at the same time don't corrupt state.
- **Idempotent upserts**: `kg add` uses `INSERT ... ON CONFLICT DO UPDATE`. Agents re-adding the same entity (e.g., after a session restart) just update it.
- **Read-heavy workload**: most agent operations are reads (context, inspect). Reads are fully concurrent. Writes are rare (task completion, artifact registration).

### KG-Integrated Agent Workflow

Agent system prompts would include KG instructions:

```markdown
# In every agent's system prompt:

## Context Acquisition
At the start of your task, query the knowledge graph:
  kg context --around "<your-bead-id>" --depth 2

This gives you: the bead description, upstream dependencies, related modules,
artifacts from previous agents, and module summaries.

For implementation details:
  kg inspect "<entity-name>"      # key_facts (imports, patterns, types)
  kg read "<entity-name>"         # full content (design docs, ADRs)

## Artifact Registration
After completing work, register what you built:
  kg add "<module-name>" --type module --summary "..." --facts '{"files":[...],"exports":[...]}'
  kg relate "<your-bead-id>" "produced" "<module-name>"
  kg relate "<module-name>" "depends_on" "<upstream-module>"

## Status Updates
  kg update "<your-bead-id>" --facts '{"status":"completed","commit":"abc123","tests":30}'
```

### Example: Two Coders Working on Related Beads

```
Timeline:

1. Coder A starts bd-3cu (outbox types):
   $ kg context --around "bd-3cu" --depth 2
   → sees bead description, dependencies, related shared module

2. Coder A finishes:
   $ kg add "outbox-types" --type module \
       --summary "Event types and status enum for outbox pattern" \
       --facts '{"files":["shared/src/outbox/types.rs"],"exports":["OutboxEvent","OutboxStatus","EventPayload"]}'
   $ kg relate "bd-3cu" "produced" "outbox-types"
   $ kg update "bd-3cu" --facts '{"status":"completed","commit":"607d759"}'

3. Coder B starts bd-3vv (outbox relay), which depends on bd-3cu:
   $ kg context --around "bd-3vv" --depth 2
   → automatically sees:
     - bd-3vv description
     - bd-3cu (dependency) → status: completed, commit: 607d759
     - outbox-types (produced by bd-3cu) → files, exports
     - shared module → existing patterns

   Coder B now knows exactly what types are available and where they live,
   without reading Coder A's mailbox or parsing an artifact JSONL file.
```

### KG as Orchestration Tool

Beyond shared memory, the KG itself could **drive orchestration decisions**. The graph knows the project topology — which modules exist, what depends on what, which beads produced what artifacts. An orchestrator could query it to make scheduling decisions:

```bash
# Orchestrator queries KG to find what's ready to work on

# "Which beads are completed and what did they produce?"
kg list --type bead | jq 'select(.key_facts.status == "completed")'

# "What modules does bd-3vv need that already exist?"
kg context --around "bd-3vv" --depth 1
# → if all dependency modules exist → safe to schedule

# "Which modules have no tests yet?" (trigger reviewer)
kg list --type module | jq 'select(.key_facts.tests == null)'

# "What's the critical path through the bead DAG?"
kg context --around "mvp-milestone" --depth 10
# → traverse the dependency chain, find the longest unfinished path
```

This turns the KG into a **queryable project model** that the orchestrator consults rather than just reading `br query`. The bead DAG is already in beads_rust, but the KG adds:
- Module topology (what code exists, what exports what)
- Artifact tracking (which bead produced which module)
- Cross-cutting concerns (which modules touch auth, which touch Kafka)
- Agent activity history (who worked on what, when)

### KG as the Orchestrator Itself

Taken further: a Claude instance with access to both `kg` and `br` could BE the orchestrator. It doesn't need a shell script — it can:

1. `kg context --around "project-root" --depth 3` to understand current state
2. `br query --status open` to find unblocked beads
3. `kg context --around "<bead-id>" --depth 2` to build context for each bead
4. Launch subagents with the KG context as their prompt
5. After subagent completion, update the KG with results
6. Query the KG to find what's newly unblocked
7. Repeat

```
┌──────────────────────────────────────────────┐
│  Claude Orchestrator Instance                 │
│  Tools: kg, br, claude -p, git               │
│                                               │
│  Loop:                                        │
│   1. kg context → understand project state    │
│   2. br query → find work                     │
│   3. kg context per bead → build prompts      │
│   4. claude -p → launch workers               │
│   5. kg update → register results             │
│   6. goto 1                                   │
│                                               │
│  The KG IS the shared memory + message bus    │
│  The orchestrator IS a Claude instance        │
│  No shell script needed                       │
└──────────────────────────────────────────────┘
```

In this model, the shell script orchestrator from Approach 2 becomes unnecessary — the KG + an interactive Claude session replaces it entirely. The Claude instance uses `kg` the same way it uses `br`: as a CLI tool to query and update state.

### Change Notifications (Future)

The KG plan's "Future Considerations" section mentions **change notifications** over the Unix socket — agents subscribe to entity changes and get notified. This would enable:

- Coder B gets notified the moment Coder A registers a new module
- Reviewer gets notified when any bead moves to "completed"
- DocKeeper gets notified when module entities are added or updated
- Orchestrator gets notified when bead status changes, triggering scheduling

This is the reactive event-driven model from Phase 3's event log, but built into the KG daemon instead of relying on `fswatch` on flat files.

### KG Build Dependencies

The KG needs to exist before it can be used for orchestration. Build order:

```
1. kg-core + kg-server + kg-cli (the tool itself)
   ↓
2. Seed the KG with existing project topology
   (modules, services, beads, dependencies — one-time import)
   ↓
3. Add kg commands to agent system prompts
   (context acquisition + artifact registration)
   ↓
4. Replace file-based communication with KG queries
   (mailbox → entities, artifact registry → relations)
   ↓
5. [Future] Change notifications for reactive orchestration
```

Step 2 could be automated: a script reads CLAUDE.md files, `br query`, and the git log to seed the initial graph. After that, agents maintain it as they work.

---

## Implementation Priority

1. `/dispatch` skill — highest immediate value, zero infrastructure
2. `/auto-doc` skill — removes the most frequent chore
3. `/post-impl` skill — automates the review-test-commit pipeline
4. Shell orchestrator script — build when starting the order/payment saga
5. Agent communication layer — structured output + artifact registry first, then event log + mailbox
6. Multi-instance tooling — tmux launcher script, `fswatch` event watcher, role-specific system prompts in `.claude/prompts/`
7. Knowledge graph (`kg`) — build the tool, seed project topology, integrate into agent prompts, then gradually replace file-based communication with KG queries
8. KG-as-orchestrator — once KG is stable, experiment with a Claude instance using `kg` + `br` as the orchestrator instead of a shell script
