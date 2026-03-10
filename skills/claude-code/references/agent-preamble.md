# Filament CLI Reference (Agent Preamble)

Include this in every `claude -p` agent prompt to prevent CLI syntax errors.

## Essential Commands

```bash
# Tasks
fl task ready                              # show unblocked tasks by priority
fl task list [--status open|closed|in_progress|blocked]
fl task show <SLUG>                        # details + relations
fl task add <TITLE> --summary "..."        # create task (optional: --priority 0-4)
fl task assign <SLUG> --to <AGENT_SLUG>    # assign task to agent
fl task close <SLUG>                       # mark done
fl update <SLUG> --status in_progress      # change status (open|closed|in_progress|blocked)
fl update <SLUG> --summary "new summary"   # update summary

# Search (FTS5)
fl search "query terms"                    # full-text across names, summaries, key_facts
fl search "query" --type lesson            # filter by entity type
fl search "query" --limit 5               # limit results

# Lessons
fl lesson list                             # all lessons
fl lesson show <SLUG>                      # structured: problem/solution/pattern/learned
fl lesson add "title" \
  --problem "what was failing" \
  --solution "how to fix it" \
  --learned "key insight" \
  --pattern "pattern-name"                 # capture knowledge

# Messaging
fl message send --from <AGENT_SLUG> --to <AGENT_SLUG> --body "text" --type text
fl message send --from <AGENT_SLUG> --to user --body "question?" --type question
fl message send --from <AGENT_SLUG> --to user --body "blocked on X" --type blocker
fl message inbox <AGENT_SLUG>              # read your messages
fl escalations                             # pending blockers/questions to user

# Relations
fl relate <SRC_SLUG> blocks <TGT_SLUG>    # A blocks B = B can't start until A closes
fl relate <SRC_SLUG> owns <TGT_SLUG>
fl relate <SRC_SLUG> depends_on <TGT_SLUG>
fl relate <SRC_SLUG> relates_to <TGT_SLUG>

# File Reservations
fl reserve "glob/pattern/**" --agent <AGENT_SLUG> --exclusive --ttl 3600
fl reserve "glob/pattern/**" --agent <AGENT_SLUG> --ttl 3600   # non-exclusive (shared)
fl release "glob/pattern/**" --agent <AGENT_SLUG>
fl reservations                            # list active reservations

# Entities
fl add <NAME> --type agent --summary "..."
fl add <NAME> --type module --summary "..."
fl add <NAME> --type service --summary "..."
fl inspect <SLUG>                          # show entity + relations
fl list [--type TYPE] [--status STATUS]

# Analytics
fl pagerank                                # PageRank scores
fl degree                                  # degree centrality
fl export --output path.json               # snapshot full graph
```

## Common Mistakes to Avoid

- `fl task assign` uses `--to`, not positional: `fl task assign SLUG --to AGENT_SLUG`
- `fl message send` requires all flags: `--from`, `--to`, `--body`, `--type`
- Message types are: `text`, `question`, `blocker`, `artifact`
- `fl reserve` requires `--agent` flag, glob must be quoted: `fl reserve "src/**" --agent SLUG`
- `fl update` changes entity fields; `fl task close` is the shortcut for closing tasks
- Slugs are 8-char alphanumeric (e.g., `jv63j5kq`), not names
- `blocks` direction: `A blocks B` means B is blocked until A closes
- Priority: 0 = highest (critical), 4 = lowest (nice-to-have), default = 2

## Agent Protocol

1. `fl task ready` — find unblocked work
2. `fl search "topic" --type lesson` — search before solving
3. `fl task assign <SLUG> --to <YOUR_SLUG>` + `fl update <SLUG> --status in_progress`
4. Do the work
5. `fl message send --from <YOUR_SLUG> --to user --body "..." --type question` — escalate if stuck
6. `fl message inbox <YOUR_SLUG>` — check for responses
7. `fl lesson add "title" --problem "..." --solution "..." --learned "..."` — capture knowledge
8. `fl task close <SLUG>`
