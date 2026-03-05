-- Replace explicit record_event() calls with SQLite triggers.
-- Destructive migration: drops and recreates the events table.
-- Adds a control table to disable triggers during import.

-- 1. Drop old events table and recreate with same schema
DROP TABLE IF EXISTS events;

CREATE TABLE events (
    id         TEXT PRIMARY KEY NOT NULL,
    entity_id  TEXT,
    event_type TEXT NOT NULL,
    actor      TEXT NOT NULL DEFAULT 'system',
    diff       TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_events_entity ON events(entity_id);
CREATE INDEX idx_events_type ON events(event_type);

-- 2. Trigger control: set disabled=1 to suppress triggers during import
CREATE TABLE IF NOT EXISTS _trigger_control (
    disabled INTEGER NOT NULL DEFAULT 0
);
INSERT INTO _trigger_control VALUES (0);

-- ---------------------------------------------------------------------------
-- Entity triggers
-- ---------------------------------------------------------------------------

-- INSERT: flat diff with created values (same format as delete)
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

-- UPDATE: old/new diff for each changed field
-- status-only change → 'status_change', otherwise → 'entity_updated'
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

-- DELETE: flat diff with deleted values (mirrors create format)
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

-- ---------------------------------------------------------------------------
-- Relation triggers
-- ---------------------------------------------------------------------------

CREATE TRIGGER trg_relation_insert AFTER INSERT ON relations
WHEN (SELECT disabled FROM _trigger_control) = 0
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NEW.source_id,
        'relation_created',
        'system',
        json_object('source_id', NEW.source_id, 'target_id', NEW.target_id, 'relation_type', NEW.relation_type),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

CREATE TRIGGER trg_relation_delete AFTER DELETE ON relations
WHEN (SELECT disabled FROM _trigger_control) = 0
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        OLD.source_id,
        'relation_deleted',
        'system',
        json_object('source_id', OLD.source_id, 'target_id', OLD.target_id, 'relation_type', OLD.relation_type),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

-- ---------------------------------------------------------------------------
-- Message triggers
-- ---------------------------------------------------------------------------

CREATE TRIGGER trg_message_insert AFTER INSERT ON messages
WHEN (SELECT disabled FROM _trigger_control) = 0
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NEW.task_id,
        'message_sent',
        NEW.from_agent,
        json_object('to_agent', NEW.to_agent),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

CREATE TRIGGER trg_message_read AFTER UPDATE ON messages
WHEN (SELECT disabled FROM _trigger_control) = 0
    AND OLD.status = 'unread' AND NEW.status = 'read'
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NEW.task_id,
        'message_read',
        'system',
        json_object('message_id', NEW.id),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

-- ---------------------------------------------------------------------------
-- Reservation triggers
-- ---------------------------------------------------------------------------

CREATE TRIGGER trg_reservation_insert AFTER INSERT ON file_reservations
WHEN (SELECT disabled FROM _trigger_control) = 0
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NULL,
        'reservation_acquired',
        NEW.agent_name,
        json_object('file_glob', NEW.file_glob),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

CREATE TRIGGER trg_reservation_delete AFTER DELETE ON file_reservations
WHEN (SELECT disabled FROM _trigger_control) = 0
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NULL,
        'reservation_released',
        OLD.agent_name,
        json_object('reservation_id', OLD.id),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

-- ---------------------------------------------------------------------------
-- Agent run triggers
-- ---------------------------------------------------------------------------

CREATE TRIGGER trg_agent_run_insert AFTER INSERT ON agent_runs
WHEN (SELECT disabled FROM _trigger_control) = 0
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NEW.task_id,
        'agent_started',
        NEW.agent_role,
        json_object('agent_run_id', NEW.id),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;

CREATE TRIGGER trg_agent_run_update AFTER UPDATE ON agent_runs
WHEN (SELECT disabled FROM _trigger_control) = 0
    AND OLD.status != NEW.status
BEGIN
    INSERT INTO events (id, entity_id, event_type, actor, diff, created_at)
    VALUES (
        lower(hex(randomblob(16))),
        NEW.task_id,
        'agent_finished',
        NEW.agent_role,
        json_object('status', NEW.status),
        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    );
END;
