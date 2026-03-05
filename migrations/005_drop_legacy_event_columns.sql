-- Drop legacy old_value/new_value columns from events table.
-- All events now use the structured `diff` JSON column instead.
-- Requires SQLite 3.35.0+ (ALTER TABLE DROP COLUMN).

ALTER TABLE events DROP COLUMN old_value;
ALTER TABLE events DROP COLUMN new_value;
