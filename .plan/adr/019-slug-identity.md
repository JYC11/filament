# ADR-019: Slug-based entity identity

**Date:** 2026-03-03
**Status:** Accepted

## Context

Entities were identified by UUID (`EntityId`) internally and looked up by name via `resolve_entity()` / `get_entity_by_name()`. Names are non-unique — two entities with the same name cause ambiguity. UUIDs are stable but not human-typeable (`a1b2c3d4-e5f6-...`). Agents and CLI users needed a short, unique, stable identifier they could type and pass between commands.

## Decision

Add a randomly-generated 8-character base36 slug (`[a-z0-9]`) as the primary human-facing identifier:

| Aspect | Before | After |
|--------|--------|-------|
| Human-facing ID | name (non-unique) | slug (unique, 8-char) |
| Internal ID | UUID | UUID (unchanged) |
| Resolution order | name → UUID fallback | slug → UUID fallback |
| Name field | lookup key | display label only |
| DB constraint | none on name | UNIQUE on slug |

### Slug type

`Slug` is a validated newtype: 8-char `[a-z0-9]`, with `TryFrom<String>`, `Display`, `FromStr`, sqlx encode/decode, and serde support. Generated via `Slug::new()` using the existing `fastrand_u8()` pattern.

### Resolution

`resolve_entity(slug_or_id)` tries slug first (via `get_entity_by_slug`), then UUID fallback. No name-based resolution — names are display-only.

### CLI output

Commands print slugs on creation: `Created entity: ab12cd34 (uuid)`. List output: `[ab12cd34] my-entity (module, open)`. All commands accept slugs as arguments.

## Consequences

- **Breaking change**: existing `.filament/` databases must be deleted and re-initialized (`.filament/` is gitignored, per-user)
- **No ambiguity**: every entity has a unique, stable, human-typeable identifier
- **Agent-friendly**: 8 chars is easy to pass in tool parameters and messages
- **Collision probability**: 36^8 ≈ 2.8 trillion — negligible for project-scale use

## Migration

`migrations/003_slug.sql` adds `slug TEXT` column with `UNIQUE` index. No backfill — users delete `.filament/` on breaking changes.
