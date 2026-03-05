-- Add version column to entities for optimistic conflict resolution (ADR-022).
-- Add diff column to events for structured change tracking.
-- Both are additive-only: existing columns preserved, defaults ensure backward compat.

ALTER TABLE entities ADD COLUMN version INTEGER NOT NULL DEFAULT 0;

ALTER TABLE events ADD COLUMN diff TEXT;
