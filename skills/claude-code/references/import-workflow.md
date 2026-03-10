# Import Workflow Reference

How to import an existing project into filament's knowledge graph.

## Step 1: Initialize

```bash
cd /path/to/project
fl init
```

## Step 2: Import Code Structure (modules/services)

Create entities for major code units:

```bash
fl add <crate-name> --type module --summary "..." [--content path/to/README.md]
```

Create ownership relations:

```bash
fl relate project-root owns <crate-name>
```

## Step 3: Import Documentation

```bash
fl add <doc-name> --type doc --summary "..." --content path/to/doc.md \
  --facts '{"category":"adr","status":"accepted"}'
```

## Step 4: Import Tasks

From a plan document, create tasks with dependencies:

```bash
fl task add <task-name> --summary "..." --priority N
fl task add <dependent> --summary "..." --depends-on <task-name>
```

Or use `--blocks`:

```bash
fl task add <blocker> --summary "..." --blocks <blocked-task>
```

## Step 5: Mark Completed Work

```bash
fl task close <completed-task>
```

## Step 6: Verify

```bash
fl list                              # all entities
fl task ready                        # what's unblocked
fl context --around <central-entity> --depth 2
```

## Naming Conventions

- Use kebab-case for entity names: `phase-3-daemon`, `filament-core`
- Prefix ADRs: `adr-003-unified-graph`
- Prefix phases: `phase-1-core`, `phase-2-cli`
- Tasks use action verbs: `implement-daemon`, `fix-reservation-bug`
