# QA-04: Lessons & Search

**Date**: 2026-03-09
**Session**: 84
**Task**: `j519l4mn`
**Result**: 7/7 PASS, 0 bugs found

## Test Results

| # | Test | Expected | Actual | Result |
|---|------|----------|--------|--------|
| 1 | lesson add with all fields | Creates lesson with problem/solution/learned/pattern | All 4 fields stored and displayed, exit 0 | PASS |
| 2 | lesson list by pattern | Filters to matching pattern only | 2 lessons with pattern "sqlx-custom-types" returned | PASS |
| 3 | lesson show structured output | 4 structured fields displayed | Problem/Solution/Pattern/Learned all present | PASS |
| 4 | search basic/phrase/OR/NOT | All FTS5 operators work | Basic: 2 results, phrase: 1 result, OR: 2 results, NOT: 1 result (correct exclusion) | PASS |
| 5 | search by type filter | --type lesson excludes non-lesson entities | 3 total results unfiltered, 2 lesson results filtered | PASS |
| 6 | empty results | Graceful "No results found" | "No results found." with exit 0 | PASS |
| 7 | BM25 ranking | Most relevant result ranked first | "sqlx newtype gotcha" ranked first with score 1.11 for query "sqlx newtype" | PASS |

## Observations

- BM25 scores displayed inline (e.g., `0.58`, `1.96`) — useful for understanding ranking
- Type filter works correctly, only returns entities of specified type
- NOT operator properly excludes matching documents
- Phrase search requires double-quoting within the shell (single quotes wrapping double)
- Empty search returns graceful message, not an error

## Environment

- macOS Darwin 25.3.0, Rust 1.94.0, fl at ~/.local/bin/fl
- Temp dir: /tmp/fl-qa-lessons
