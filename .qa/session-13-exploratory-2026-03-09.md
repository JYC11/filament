# QA Session 13: Exploratory Testing

**Date**: 2026-03-09
**Environment**: macOS, `fl` v0.1.0 (release build), temp dir `/tmp/fl-qa-docs`
**Method**: Unscripted exploratory pass — Unicode, edge cases, error handling, concurrency, security.

## Results Summary

36 exploratory tests across 8 categories. **0 code bugs, 3 UX nits found.**

| Category | Tests | Result |
|----------|-------|--------|
| Unicode & i18n | 5 | PASS (CJK search N/A — FTS5 tokenizer) |
| Input validation | 6 | PASS (all invalid inputs rejected) |
| Edge cases | 8 | PASS |
| Concurrency | 1 | PASS (20 parallel writes, 0 data loss) |
| Security | 2 | PASS (SQLi safe, but FTS5 leaks DB error) |
| JSON output | 4 | PASS (well-formed, structured errors) |
| Search robustness | 4 | 3 PASS, 1 nit (empty query leaks DB error) |
| Conflict resolution | 2 | PASS (auto-merge + overlapping conflict both correct) |

## Findings

### Nit 1: FTS5 search leaks raw database errors on special characters

**Severity**: P4 (cosmetic, not a security issue)
**Repro**: `fl search "'"` or `fl search ""` or `fl search " "`
**Actual**: `error: Database: error returned from database: (code: 1) fts5: syntax error near ""`
**Expected**: `error: Validation: invalid search query` (user-friendly message)
**Root cause**: Search passes user input directly to FTS5 without catching syntax errors.

### Nit 2: CJK text not searchable via FTS5

**Severity**: P4 (limitation, not a bug)
**Repro**: Create entity with CJK name `認証モジュール`, then `fl search "認証"` → no results
**Root cause**: FTS5 default `unicode61` tokenizer doesn't segment CJK characters (needs ICU tokenizer or trigram).
**Note**: Entity creation, storage, and display of CJK all work perfectly. Only FTS5 search is affected.

### Nit 3: Message send accepts nonexistent recipient

**Severity**: P4 (debatable design choice)
**Repro**: `fl message send --from <valid-slug> --to zzzzzzzz --body "test"` → succeeds
**Note**: Could be intentional for fire-and-forget messaging. No crash or data corruption.

## Detailed Test Log

### Unicode & i18n (EXP-01, 03, 32)
```
fl add "認証モジュール" --type module --summary "日本語テスト"     → PASS (created)
fl add "Ünïcödé Tëst" --type module --summary "àéîõü"            → PASS (created)
fl add "emoji-test 🔥" --type module --summary "Fire emoji"        → PASS (created)
fl search "認証"                                                   → No results (FTS5 limitation)
fl search "emoji"                                                  → PASS (found)
fl lesson add "CJK lesson" --problem "問題がある" ...              → PASS (created, displayed correctly)
```

### Input Validation (EXP-02, 10, 11)
```
fl add "" --type task                    → PASS (rejected: name cannot be empty)
fl add "   " --type task                 → PASS (rejected: name cannot be empty)
fl add "ok" --type task --summary ""     → PASS (allowed, empty summary is valid)
fl update <slug> --status running        → PASS (rejected: invalid EntityStatus)
fl update <slug> --status OPEN           → PASS (case-insensitive, set to open)
fl add --priority 5                      → PASS (rejected: priority must be 0-4)
fl add --priority -1                     → PASS (rejected: unexpected argument)
fl add --priority abc                    → PASS (rejected: invalid digit)
```

### Edge Cases (EXP-04, 06-09, 22, 25)
```
10K char summary                         → PASS (stored, displayed truncated in list)
Double close same task                   → PASS (idempotent, no error)
Close non-task entity                    → PASS (rejected: type mismatch)
Self-relation                            → PASS (rejected: source and target must differ)
Duplicate relation                       → PASS (rejected: relation already exists)
Multiple relation types same pair        → PASS (allowed, all 3 shown in inspect)
Lesson with missing required fields      → PASS (rejected by clap)
Array JSON in --facts                    → PASS (accepted — array is valid JSON value)
Nested JSON in --facts                   → PASS (accepted)
```

### Concurrency (EXP-14)
```
20 parallel `fl add` commands            → PASS (all 20 created, no duplicates, no errors)
```

### Security (EXP-19)
```
SQL injection in entity name             → PASS (entity created safely, parameterized queries)
SQL injection string in FTS5 search      → Nit (leaks DB error, not a security issue)
```

### JSON Output (EXP-12, 33-35)
```
fl list --json                           → PASS (valid JSON array)
fl inspect --json                        → PASS (valid JSON object)
fl task ready --json                     → PASS (valid JSON array)
fl inspect zzzzzzzz --json               → PASS (structured error with code/message/hint/retryable)
fl list --type lesson --json             → PASS (filtered correctly, only lessons returned)
```

### Search (EXP-21, 28)
```
fl search '"exact phrase"'               → PASS (no error, no matches)
fl search 'hello OR world'              → PASS (no error)
fl search 'hello NOT world'             → PASS (no error)
fl search ""                             → Nit (leaks FTS5 DB error)
fl search " "                            → Nit (leaks FTS5 DB error)
fl search "'"                            → Nit (leaks FTS5 DB error)
```

### Conflict Resolution (EXP-18)
```
Update with stale --version (non-overlapping fields)  → PASS (auto-merged per ADR-022)
Update with stale --version (overlapping fields)       → PASS (VersionConflict raised)
fl resolve on non-conflicted entity                    → PASS (shows current state, no error)
```
