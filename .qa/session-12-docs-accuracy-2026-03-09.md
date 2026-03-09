# QA Session 12: Docs Accuracy Audit

**Date**: 2026-03-09
**Environment**: macOS, `fl` v0.1.0 (release build), temp dir `/tmp/fl-qa-docs`
**Method**: Execute every documented `fl` command literally from README.md, compare --help to docs, verify error codes, check for stale references.

## Results Summary

| ID | Test | Result | Notes |
|----|------|--------|-------|
| DOC-01 | README.md examples | FAIL → FIXED | `critical-path` → `blocker-depth`, bare `seed` requires `--file` |
| DOC-02 | CLAUDE.md command table | PASS | All commands match |
| DOC-03 | Filament skill file | PASS | All commands match |
| DOC-04 | `fl --help` accuracy | FAIL → FIXED | Same as DOC-01 issues, now corrected in README |
| DOC-05 | Error code table | PASS | Exit codes 0,2,3,4,6 confirmed; 5,7 hard to trigger |
| DOC-06 | Stale `filament` references | PASS → FIXED | User-facing docs clean; `.plan/gotchas.md` + test-guide skill refs fixed |
| DOC-07 | Skill scenario schema | N/A | No scenario JSON files exist in current structure |
| DOC-08 | Bug report skill | N/A | Prompt-based skill, no file artifact to validate |
| DOC-09 | Manual QA skill | N/A | Prompt-based skill, no file artifact to validate |
| DOC-10 | Entity type table | PASS | All 7 types (task, module, service, agent, plan, doc, lesson) created |

**Score: 7/7 PASS (after fixes), 3 N/A**

## Doc Issues Found and Fixed

### Fix 1: README `fl task critical-path` → `fl task blocker-depth`

**Location**: README.md
**Issue**: Documented `critical-path` subcommand; actual command is `blocker-depth`
**Fix**: Updated README to use `fl task blocker-depth`

### Fix 2: README bare `fl seed` → `fl seed --file`

**Location**: README.md
**Issue**: Documented bare `fl seed` as auto-parsing CLAUDE.md; actually requires `--file`
**Fix**: Updated README examples to always include `--file`

### Fix 3: README `fl update` examples incomplete

**Location**: README.md
**Issue**: Only showed `--status` and `--summary`; missed `--priority`, `--facts`, `--content`
**Fix**: Added missing flag examples

### Fix 4: README config table — daemon-only settings unlabeled

**Location**: README.md
**Issue**: `idle_timeout_secs`, `reconciliation_interval_secs`, `agent_timeout_secs` not marked as daemon-only
**Fix**: Added "(daemon only)" annotation

### Fix 5: Stale `filament` command references

**Locations**: `.plan/gotchas.md` (3 refs), `.claude/skills/test-guide/SKILL.md` (1 ref)
**Fix**: Updated to `fl`

## Additional Findings (no fix needed)

### Finding 1: `fl update` flags underdocumented in README

README update section only shows `--status` and `--summary` examples. Actual `fl update --help` also supports `--priority`, `--facts`, `--content`, `--version`. These were added in session 83 but README wasn't updated.

### Finding 2: `config show` / `config init` missing 3 daemon settings

README config table documents `idle_timeout_secs`, `reconciliation_interval_secs`, `agent_timeout_secs` as config settings, but:
- `fl config show` doesn't display them
- `fl config init` template doesn't include them

These settings exist in source (`filament-core/src/config.rs`) and are accessible via env vars, but the CLI config commands don't expose them.

### Finding 3: `.plan/gotchas.md` has stale `filament` command references

3 occurrences of old `filament` command name:
- `filament relate` arg order
- `filament update` requires at least one flag
- `filament serve --foreground`

### Finding 4: `.claude/skills/test-guide/SKILL.md` has stale reference

Line 44: `filament init` should be `fl init`

## Detailed Test Log

### DOC-01: README.md examples (entity CRUD)
```
fl add "Authentication Module" --type module --summary "Handles JWT auth" --priority 1  → PASS
fl add "API Gateway" --type service --summary "..." --facts '{"port": 8080}'            → PASS
fl add "Architecture Decision" --type doc --summary "..." --content test-adr.md         → PASS
fl inspect <slug>                                                                        → PASS
fl update <slug> --status in_progress                                                    → PASS
fl read <slug>                                                                           → PASS
fl list / --type / --status / --status all / --type+status                               → PASS
fl remove <slug>                                                                         → PASS
fl relate / fl context / fl unrelate                                                     → PASS
fl task add (basic, --depends-on, --blocks)                                              → PASS
fl task list / --status all / --unblocked                                                → PASS
fl task show / fl task ready / fl task ready --limit 5                                   → PASS
fl task critical-path                                                                    → FAIL (command doesn't exist)
fl task close / fl task assign                                                           → PASS
fl lesson add / list / list --pattern / show / delete                                    → PASS
fl search / search --type lesson                                                         → PASS
fl message send / inbox / read                                                           → PASS
fl reserve (shared + exclusive) / reservations / release                                 → PASS
fl export / export --output / export --no-events                                         → PASS
fl import --input                                                                        → PASS
fl escalations                                                                           → PASS
fl config show / config init / config path                                               → PASS
fl seed --file / --dry-run                                                               → PASS
fl seed (bare)                                                                           → FAIL (requires --file or --files)
fl hook install / check / check --agent / uninstall                                      → PASS
fl audit                                                                                 → PASS
fl pagerank / fl degree                                                                  → PASS
```

### DOC-05: Error code verification
```
Exit 0 (success):   fl list → exit 0                                    ✓
Exit 2 (CLI arg):   fl --badarg → exit 2                                ✓
Exit 3 (not found): fl inspect zzzzzzzz → exit 3                       ✓
Exit 4 (validation): fl update <slug> (no flags) → exit 4              ✓
Exit 5 (database):  Cannot trigger from CLI (would need corrupt DB)     N/A
Exit 6 (conflict):  fl reserve exclusive on taken file → exit 6         ✓
Exit 7 (I/O error): Cannot trigger from CLI (would need disk error)     N/A
```

### DOC-06: Stale `filament` CLI references
- README.md: CLEAN (0 stale references)
- CLAUDE.md: CLEAN (0 stale references)
- Skill SKILL.md: CLEAN (0 stale references)
- Source code (crates/): CLEAN (0 stale references)
- .plan/gotchas.md: 3 stale references (low priority)
- .claude/skills/test-guide/SKILL.md: 1 stale reference
- .plan/*.md and .qa/*.md: many stale references (historical records)

### DOC-10: Entity type creation
```
task:    fl add "test-task" --type task --summary "..."                  → PASS
module:  fl add "test-module" --type module --summary "..."             → PASS
service: fl add "test-service" --type service --summary "..."           → PASS
agent:   fl add "test-agent" --type agent --summary "..."               → PASS
plan:    fl add "test-plan" --type plan --summary "..." --content ...   → PASS (requires --content)
doc:     fl add "test-doc" --type doc --summary "..." --content ...     → PASS (requires --content)
lesson:  fl lesson add "test-lesson" --problem/--solution/--learned     → PASS
```
