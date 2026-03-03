-- Filament schema v1
-- All TEXT columns for enums (entity_type, status, etc.) store snake_case values.
-- Timestamps are ISO 8601 strings (chrono default for SQLite).

CREATE TABLE IF NOT EXISTS entities (
    id           TEXT PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL,
    entity_type  TEXT NOT NULL,
    summary      TEXT NOT NULL DEFAULT '',
    key_facts    TEXT NOT NULL DEFAULT '{}',
    content_path TEXT,
    content_hash TEXT,
    status       TEXT NOT NULL DEFAULT 'open',
    priority     INTEGER NOT NULL DEFAULT 2,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    CHECK (entity_type IN ('task', 'module', 'service', 'agent', 'plan', 'doc')),
    CHECK (status IN ('open', 'in_progress', 'closed', 'blocked')),
    CHECK (priority BETWEEN 0 AND 4)
);

CREATE TABLE IF NOT EXISTS relations (
    id            TEXT PRIMARY KEY NOT NULL,
    source_id     TEXT NOT NULL,
    target_id     TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    weight        REAL NOT NULL DEFAULT 1.0,
    summary       TEXT NOT NULL DEFAULT '',
    metadata      TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL,
    FOREIGN KEY (source_id) REFERENCES entities(id) ON DELETE CASCADE,
    FOREIGN KEY (target_id) REFERENCES entities(id) ON DELETE CASCADE,
    CHECK (relation_type IN ('blocks', 'depends_on', 'produces', 'owns', 'relates_to', 'assigned_to')),
    CHECK (source_id != target_id)
);

-- Messages are NOT graph entities. Separate table with inbox semantics.
-- Single table, query-based inbox/outbox views.
CREATE TABLE IF NOT EXISTS messages (
    id          TEXT PRIMARY KEY NOT NULL,
    from_agent  TEXT NOT NULL,
    to_agent    TEXT NOT NULL,
    msg_type    TEXT NOT NULL DEFAULT 'text',
    body        TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'unread',
    in_reply_to TEXT REFERENCES messages(id),
    task_id     TEXT,
    created_at  TEXT NOT NULL,
    read_at     TEXT,
    CHECK (from_agent != ''),
    CHECK (to_agent != ''),
    CHECK (msg_type IN ('text', 'question', 'blocker', 'artifact')),
    CHECK (status IN ('unread', 'read', 'archived'))
);

CREATE TABLE IF NOT EXISTS agent_runs (
    id                 TEXT PRIMARY KEY NOT NULL,
    task_id            TEXT NOT NULL,
    agent_role         TEXT NOT NULL,
    pid                INTEGER,
    status             TEXT NOT NULL,
    result_json        TEXT,
    context_budget_pct REAL,
    started_at         TEXT NOT NULL,
    finished_at        TEXT,
    CHECK (status IN ('running', 'completed', 'blocked', 'failed', 'needs_input'))
);

CREATE TABLE IF NOT EXISTS file_reservations (
    id          TEXT PRIMARY KEY NOT NULL,
    agent_name  TEXT NOT NULL,
    file_glob   TEXT NOT NULL,
    exclusive   INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    CHECK (expires_at > created_at)
);

CREATE TABLE IF NOT EXISTS blocked_entities_cache (
    entity_id       TEXT PRIMARY KEY NOT NULL,
    blocker_ids_json TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    id         TEXT PRIMARY KEY NOT NULL,
    entity_id  TEXT,
    event_type TEXT NOT NULL,
    actor      TEXT NOT NULL DEFAULT '',
    old_value  TEXT,
    new_value  TEXT,
    created_at TEXT NOT NULL
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id);
CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id);
CREATE INDEX IF NOT EXISTS idx_entities_type_status ON entities(entity_type, status);
CREATE INDEX IF NOT EXISTS idx_messages_inbox ON messages(to_agent, status);
CREATE INDEX IF NOT EXISTS idx_messages_from ON messages(from_agent);
CREATE INDEX IF NOT EXISTS idx_messages_task ON messages(task_id);
CREATE INDEX IF NOT EXISTS idx_reservations_agent ON file_reservations(agent_name);
CREATE INDEX IF NOT EXISTS idx_reservations_expires ON file_reservations(expires_at);
CREATE INDEX IF NOT EXISTS idx_events_entity ON events(entity_id);

-- Partial index: ready tasks ranked by priority
CREATE INDEX IF NOT EXISTS idx_entities_ready
    ON entities(status, priority, created_at)
    WHERE entity_type = 'task' AND status IN ('open', 'in_progress');
