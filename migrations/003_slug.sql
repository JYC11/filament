-- Add slug column for stable, human-typeable entity identifiers.
-- Breaking change: existing entities have NULL slugs and will fail to load.
-- Users must delete .filament/ and re-initialize.

ALTER TABLE entities ADD COLUMN slug TEXT;
CREATE UNIQUE INDEX idx_entities_slug ON entities(slug);
