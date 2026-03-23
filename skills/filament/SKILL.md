---
name: filament
description: >
  Use the filament CLI for project knowledge management, task tracking, lesson capture,
  and inter-agent coordination. Filament stores entities (tasks, modules, services, agents,
  plans, docs, lessons), relations between them, messages, and file reservations in a local
  SQLite database. Use when: managing project context, tracking tasks, recording gotchas
  and lessons, recording architecture decisions, creating knowledge graph entries,
  coordinating agent work, or querying project structure.
  Triggers on: "filament", "add entity", "add task", "add lesson", "track this",
  "create a task", "gotcha", "lesson", "record a lesson", "search before solving",
  "capture lesson", "search knowledge", "relate these", "what blocks",
  "critical path", "ready tasks", "project graph", "knowledge graph", "file reservation",
  "agent message", "what's next", "dispatch agent", "escalations", "export", "import",
  "daemon", "tui".
---

# Filament CLI Reference

Local-only knowledge graph + task manager + multi-agent orchestrator. Data in `.fl/` (via `fl init`).

Every entity gets a unique **8-char slug** (`[a-z0-9]`). All commands accept slugs.

## Task Workflow

```bash
fl task ready                                    # pick unblocked work
fl task assign <SLUG> --to <AGENT_SLUG>          # --to flag REQUIRED
fl update <SLUG> --status in_progress
fl search "topic" --type lesson                  # query is FIRST positional arg
fl lesson show <SLUG>                            # read found lessons
# ... do the work ...
fl lesson add "title" --problem "..." --solution "..." --learned "..." --pattern "name"
fl task close <SLUG>
```

Capture lessons for: surprising errors, non-obvious conventions, debugging > few minutes.
Skip for: typos, missing imports, language basics.

## Commands

### Entities
```bash
fl add <NAME> --type <TYPE> --summary "..."      # types: task|module|service|agent|plan|doc|lesson
fl inspect <SLUG>                                # details + relations
fl list [--type TYPE] [--status STATUS]
fl update <SLUG> [--summary "..."] [--status open|closed|in_progress|blocked]
fl remove <SLUG>                                 # cascades relations
```

### Tasks
```bash
fl task add <TITLE> --summary "..." [--priority 0-4] [--blocks SLUG] [--depends-on SLUG]
fl task list [--status STATUS]                   # NOTE: --status and --unblocked cannot combine
fl task ready [--limit N]                        # unblocked, ranked by priority
fl task show <SLUG>
fl task close <SLUG>
fl task assign <SLUG> --to <AGENT_SLUG>          # --to is REQUIRED, not positional
fl task blocker-depth <SLUG>
```

### Lessons
```bash
fl lesson add <TITLE> --problem "..." --solution "..." --learned "..." [--pattern NAME]
fl lesson list [--pattern NAME]
fl lesson show <SLUG>                            # structured: problem/solution/pattern/learned
```

Gotchas are ALWAYS lessons (not docs). Patterns enable cross-project knowledge transfer.

### Search (FTS5)
```bash
fl search "query" [--type TYPE] [--limit N]      # query is FIRST positional arg
```

### Relations
```bash
fl relate <SRC> <TYPE> <TGT>                     # types: blocks|depends_on|produces|owns|relates_to|assigned_to
fl unrelate <SRC> <TYPE> <TGT>
# blocks direction: "A blocks B" = B cannot start until A closes
```

### Messaging
```bash
fl message send --from <A> --to <B> --body "..." --type text|question|blocker|artifact
fl message inbox <SLUG>                          # use SLUG not name
fl escalations                                   # pending blockers/questions to user
# Escalate: send --to user with --type question or --type blocker
# ALL flags required: --from, --to, --body, --type
```

### File Reservations
```bash
fl reserve "glob/**" --agent <SLUG> [--exclusive] [--ttl SECS]   # QUOTE the glob
fl release "glob/**" --agent <SLUG>
fl reservations [--agent SLUG] [--clean]
```

### Graph & Analytics
```bash
fl context --around <SLUG> --depth N
fl pagerank
fl degree
```

### Infrastructure
```bash
fl serve / fl stop                               # daemon for multi-agent concurrent access
fl export [--output PATH] / fl import [--input PATH]
fl mcp                                           # MCP stdio server (16 tools)
fl tui                                           # interactive dashboard
fl seed --file PATH                              # create Doc entities from files
fl audit [--branch NAME]                         # snapshot graph to git branch
fl config show / init / path
fl watch [--events ...]
fl hook install / uninstall / check
```

Global flags: `--json` | `-v` (debug) | `-vv` (trace) | `-q` (quiet)

## Entity Types

| Type | Purpose | When to use |
|------|---------|-------------|
| `task` | Work items with priority + deps | Bugs, features, work to track |
| `module` | Code structure | Relate tasks to code |
| `service` | Runtime components | Databases, servers |
| `agent` | Actors (required for assign/message/reserve) | AI or human workers |
| `plan` | Planning docs (`--content path.md`) | Group tasks via `owns` |
| `doc` | Reference material (`--content path.md`) | ADRs, specs |
| `lesson` | Knowledge capture (use `fl lesson add`) | Gotchas, patterns |

## Exit Codes

0=success, 2=arg error, 3=not found, 4=validation, 5=db error, 6=conflict, 7=I/O.
With `--json`: `code`, `message`, `hint`, `retryable` fields.

## Multi-Agent Dispatch (tmux + claude -p)

Real concurrent Claude agents coordinating through filament. Each is an isolated `claude -p` process.

```bash
# 1. Daemon + scenario setup
fl serve
fl add agent-name --type agent --summary "role"
fl task add task-name --summary "..." --priority N
fl relate <blocker> blocks <blocked>

# 2. Agent launch script (one per agent)
#!/bin/bash
cd /path/to/project
unset CLAUDECODE                                 # REQUIRED from within Claude Code
SKILL_DIR="$HOME/.claude/skills/filament"        # adjust if installed elsewhere
PREAMBLE=$(cat "$SKILL_DIR/references/agent-preamble.md")
claude -p "${PREAMBLE}
$(cat agent-prompt.md)" --allowedTools 'Bash(*)' 2>&1 | tee log-agent.txt

# 3. Launch waves via tmux
tmux new-session -d -s sim -n monitor
tmux new-window -t sim -n agent1 && tmux send-keys -t sim:agent1 'bash launch-agent1.sh' Enter
tmux new-window -t sim -n agent2 && tmux send-keys -t sim:agent2 'bash launch-agent2.sh' Enter

# 4. Monitor between waves
fl escalations
fl message send --from user --to <SLUG> --body "answer" --type text
fl task list && fl reservations
```

Agent prompts MUST include `references/agent-preamble.md` for correct CLI syntax.
`claude -p` is one-shot: agents escalate, proceed on assumptions, and exit.

### Worktrees for Parallel Agents

When launching subagents via the Agent tool (not tmux), use `isolation: "worktree"` so each agent
gets its own copy of the repo. This avoids build artifact contention (`target/` directory locks),
merge conflicts from simultaneous edits, and eliminates the need for file reservations between
subagents. The worktree is auto-cleaned if the agent makes no changes; if it does, results are
returned with the worktree branch for you to merge.

```
Agent(prompt="...", isolation="worktree")  # each agent gets isolated repo copy
```

Use worktrees for code-writing agents. Use the shared repo (no isolation) for read-only agents
(research, exploration, code review).

## Tips

- Priority: 0 = highest, 4 = lowest (default 2)
- `blocks` direction: `A blocks B` means B waits for A
- Daemon routes CLI through Unix socket for concurrent access
- `FILAMENT_AUTO_DISPATCH=1` chains agent runs on newly-unblocked tasks
