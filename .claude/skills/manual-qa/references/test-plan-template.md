# Test Plan Template

Adapt this template to the project's command groups.

## Structure

```markdown
## Test Plan

### 1. Initialization
- [TC-01] Init creates project directory
- [TC-02] Double-init is idempotent

### 2. Entity CRUD
- [TC-03] Add with all fields
- [TC-04] Add with minimal fields
- [TC-05] List (unfiltered)
- [TC-06] List filtered by type
- [TC-07] List filtered by status
- [TC-08] Inspect/show displays all fields
- [TC-09] Update single field
- [TC-10] Update multiple fields atomically
- [TC-11] Remove entity
- [TC-12] Inspect removed entity returns error

### 3. Error Handling
- [TC-13] Invalid type/category → validation error
- [TC-14] Invalid status → validation error
- [TC-15] Update with no changes → validation error
- [TC-16] Not-found → structured error
- [TC-17] JSON error output (`--json` + error case)

### 4. Relations / Links
- [TC-18] Create relation
- [TC-19] Show/inspect displays relations
- [TC-20] Remove relation
- [TC-21] Relate with nonexistent entity → error

### 5. Workflows
- [TC-22] Multi-step workflow (create → relate → query → close)
- [TC-23] Blocking/dependency semantics
- [TC-24] Unblocking cascade

### 6. JSON Output
- [TC-25] `--json` on create returns structured ID
- [TC-26] `--json` on list returns array
- [TC-27] `--json` on query returns structured data
```

## Naming Convention

- `TC-XX` — sequential across all groups
- Groups numbered 1-N, matching command areas
- Test names are imperative: "Add entity with all fields" not "Entity added"

## Coverage Checklist

For each command, test:
- [ ] Happy path (success)
- [ ] Error path (validation failure)
- [ ] JSON output mode
- [ ] Edge case (empty result, single item, boundary values)
