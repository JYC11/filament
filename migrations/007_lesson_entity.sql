-- Add 'lesson' to entity_type CHECK constraint.
-- SQLite requires table recreation to modify CHECK constraints.

PRAGMA foreign_keys=OFF;

-- 1. Create new table with updated CHECK
CREATE TABLE entities_new (
    id           TEXT PRIMARY KEY NOT NULL,
    slug         TEXT UNIQUE NOT NULL,
    name         TEXT NOT NULL,
    entity_type  TEXT NOT NULL,
    summary      TEXT NOT NULL DEFAULT '',
    key_facts    TEXT NOT NULL DEFAULT '{}',
    content_path TEXT,
    content_hash TEXT,
    status       TEXT NOT NULL DEFAULT 'open',
    priority     INTEGER NOT NULL DEFAULT 2,
    version      INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    CHECK (entity_type IN ('task', 'module', 'service', 'agent', 'plan', 'doc', 'lesson')),
    CHECK (status IN ('open', 'in_progress', 'closed', 'blocked')),
    CHECK (priority BETWEEN 0 AND 4)
);

-- 2. Copy existing data
INSERT INTO entities_new SELECT * FROM entities;

-- 3. Drop old entity triggers (they reference the old table)
DROP TRIGGER IF EXISTS trg_entity_insert;
DROP TRIGGER IF EXISTS trg_entity_update;
DROP TRIGGER IF EXISTS trg_entity_delete;

-- 4. Drop old table and rename
DROP TABLE entities;
ALTER TABLE entities_new RENAME TO entities;

-- 5. Recreate indexes
CREATE INDEX idx_entities_type_status ON entities(entity_type, status);
CREATE UNIQUE INDEX idx_entities_slug ON entities(slug);
CREATE INDEX idx_entities_ready
    ON entities(status, priority, created_at)
    WHERE entity_type IN ('task', 'lesson') AND status IN ('open', 'in_progress');

-- 6. Recreate entity triggers (from migration 006)

CREATE TRIGGER trg_entity_insert AFTER INSERT ON entities
WHEN (SELECT disabled FROM _trigger_control) = 0
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NEW.id,
        'entity_created',
        'system',
        json_object(
            'name', NEW.name,
            'entity_type', NEW.entity_type,
            'summary', NEW.summary,
            'priority', CAST(NEW.priority AS TEXT)
        ),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

CREATE TRIGGER trg_entity_update AFTER UPDATE ON entities
WHEN (SELECT disabled FROM _trigger_control) = 0
    AND (OLD.name != NEW.name OR OLD.summary != NEW.summary OR OLD.status != NEW.status
         OR OLD.priority != NEW.priority OR OLD.key_facts != NEW.key_facts
         OR coalesce(OLD.content_path, '') != coalesce(NEW.content_path, ''))
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NEW.id,
        CASE
            WHEN OLD.status != NEW.status
                AND OLD.name = NEW.name AND OLD.summary = NEW.summary
                AND OLD.priority = NEW.priority AND OLD.key_facts = NEW.key_facts
                AND coalesce(OLD.content_path, '') = coalesce(NEW.content_path, '')
            THEN 'status_change'
            ELSE 'entity_updated'
        END,
        'system',
        (SELECT json_group_object(key, json(value)) FROM (
            SELECT 'name' AS key, json_object('old', OLD.name, 'new', NEW.name) AS value
                WHERE OLD.name != NEW.name
            UNION ALL
            SELECT 'summary', json_object('old', OLD.summary, 'new', NEW.summary)
                WHERE OLD.summary != NEW.summary
            UNION ALL
            SELECT 'status', json_object('old', OLD.status, 'new', NEW.status)
                WHERE OLD.status != NEW.status
            UNION ALL
            SELECT 'priority', json_object('old', CAST(OLD.priority AS TEXT), 'new', CAST(NEW.priority AS TEXT))
                WHERE OLD.priority != NEW.priority
            UNION ALL
            SELECT 'key_facts', json_object('old', OLD.key_facts, 'new', NEW.key_facts)
                WHERE OLD.key_facts != NEW.key_facts
            UNION ALL
            SELECT 'content_path', json_object('old', coalesce(OLD.content_path, ''), 'new', coalesce(NEW.content_path, ''))
                WHERE coalesce(OLD.content_path, '') != coalesce(NEW.content_path, '')
        )),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

CREATE TRIGGER trg_entity_delete AFTER DELETE ON entities
WHEN (SELECT disabled FROM _trigger_control) = 0
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        OLD.id,
        'entity_deleted',
        'system',
        json_object(
            'name', OLD.name,
            'entity_type', OLD.entity_type,
            'summary', OLD.summary,
            'priority', CAST(OLD.priority AS TEXT)
        ),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

PRAGMA foreign_keys=ON;
