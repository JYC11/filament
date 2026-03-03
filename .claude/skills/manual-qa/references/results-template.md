# Results File Template

Use this structure for `.qa/manual-qa-results.md`.

```markdown
# <Project> CLI — Manual QA Results

**Date**: YYYY-MM-DD
**Binary**: `target/release/<binary>`
**QA dir**: `/tmp/<project>-qa`

---

## Test Plan

### 1. <Group Name>
- [TC-01] Description
- [TC-02] Description

### 2. <Group Name>
- [TC-03] Description
...

---

## Results

### TC-01: Description
` ` `
$ <command>
<output>
exit: <code>
` ` `
**Result**: PASS | FAIL — <reasoning if FAIL>

### TC-02: Description
...

---

## Bugs Found

### BUG-01: Short title (TC-XX)

**Severity**: Critical | High | Medium | Low
**Description**: What happened vs what should happen.
**Expected**: What the correct behavior is.
**Actual**: What actually happened.
**Fix**: Description of the fix applied.
**Status**: FIXED | OPEN | WON'T FIX

---

## Summary

| Group | Tests | Pass | Fail | Notes |
|-------|-------|------|------|-------|
| Group 1 | N | N | 0 | |
| Group 2 | N | N | 0 | |
| **Total** | **N** | **N** | **0** | |

<Final summary line with pass rate and any bugs found/fixed.>
```

## Exit Code Reference

Document the project's exit codes here for quick reference during QA:

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | CLI usage error (clap) |
| 3 | Not found |
| 4 | Validation error |
| 5 | Storage error |
| 6 | Conflict (e.g., file reserved) |
