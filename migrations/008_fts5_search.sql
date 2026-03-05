-- Add FTS5 full-text search index over entities.
-- Uses external-content mode: the FTS table mirrors data from `entities`
-- and is kept in sync via triggers on INSERT/UPDATE/DELETE.

-- 1. Create FTS5 virtual table (external content, BM25 ranking)
CREATE VIRTUAL TABLE IF NOT EXISTS entities_fts USING fts5(
    name,
    summary,
    key_facts,
    content='entities',
    content_rowid='rowid'
);

-- 2. Populate FTS index from existing data
INSERT INTO entities_fts(rowid, name, summary, key_facts)
    SELECT rowid, name, summary, key_facts FROM entities;

-- 3. Triggers to keep FTS in sync with entities table
--    These fire AFTER the main entity triggers from migration 006.

CREATE TRIGGER entities_fts_insert AFTER INSERT ON entities BEGIN
    INSERT INTO entities_fts(rowid, name, summary, key_facts)
        VALUES (NEW.rowid, NEW.name, NEW.summary, NEW.key_facts);
END;

CREATE TRIGGER entities_fts_delete AFTER DELETE ON entities BEGIN
    INSERT INTO entities_fts(entities_fts, rowid, name, summary, key_facts)
        VALUES ('delete', OLD.rowid, OLD.name, OLD.summary, OLD.key_facts);
END;

CREATE TRIGGER entities_fts_update AFTER UPDATE ON entities BEGIN
    INSERT INTO entities_fts(entities_fts, rowid, name, summary, key_facts)
        VALUES ('delete', OLD.rowid, OLD.name, OLD.summary, OLD.key_facts);
    INSERT INTO entities_fts(rowid, name, summary, key_facts)
        VALUES (NEW.rowid, NEW.name, NEW.summary, NEW.key_facts);
END;
