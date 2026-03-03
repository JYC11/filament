-- Unique constraint: prevent duplicate edges (same source, target, relation type)
CREATE UNIQUE INDEX IF NOT EXISTS idx_relations_unique
    ON relations(source_id, target_id, relation_type);

-- Index for list_running_agents query (WHERE status = 'running')
CREATE INDEX IF NOT EXISTS idx_agent_runs_status ON agent_runs(status);
