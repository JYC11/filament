use chrono::Utc;
use sqlx::{Pool, Sqlite, SqliteConnection};

use crate::error::{FilamentError, Result};
use crate::models::{
    AgentRun, AgentRunId, AgentStatus, Entity, EntityId, EntityStatus, Event, EventId, EventType,
    Message, MessageId, Relation, RelationId, Reservation, ReservationId, TtlSeconds,
    ValidCreateEntityRequest, ValidCreateRelationRequest, ValidSendMessageRequest,
};

// ---------------------------------------------------------------------------
// Executor abstraction (from workout-util pattern)
// ---------------------------------------------------------------------------

/// Type alias for a `SQLite` transaction.
pub type SqliteTx<'a> = sqlx::Transaction<'a, Sqlite>;

// ---------------------------------------------------------------------------
// FilamentStore
// ---------------------------------------------------------------------------

/// Main storage handle wrapping a `SQLite` connection pool.
#[derive(Clone)]
pub struct FilamentStore {
    pool: Pool<Sqlite>,
}

impl FilamentStore {
    /// Wrap an existing pool.
    #[must_use]
    pub const fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Access the underlying pool (for repo functions that need an executor).
    #[must_use]
    pub const fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    /// Run a closure inside a transaction. Auto-commits on `Ok`, auto-rolls back on `Err`.
    ///
    /// Use `Box::pin(async move { ... })` in the closure body to satisfy the lifetime.
    ///
    /// # Errors
    ///
    /// Returns the error from the closure or from transaction commit.
    pub async fn with_transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: for<'c> FnOnce(
            &'c mut SqliteConnection,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<T>> + Send + 'c>,
        >,
    {
        let mut tx = self.pool.begin().await?;
        let result = f(&mut tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Entity repo functions
// ---------------------------------------------------------------------------

/// Create an entity. Returns the new ID.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn create_entity(
    conn: &mut SqliteConnection,
    req: &ValidCreateEntityRequest,
) -> Result<EntityId> {
    let id = EntityId::new();
    let now = Utc::now();
    let key_facts = serde_json::to_string(&req.key_facts).unwrap_or_default();

    sqlx::query(
        "INSERT INTO entities (id, name, entity_type, summary, key_facts, content_path, status, priority, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 'open', ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(req.name.as_str())
    .bind(req.entity_type.as_str())
    .bind(&req.summary)
    .bind(&key_facts)
    .bind(&req.content_path)
    .bind(req.priority)
    .bind(now)
    .bind(now)
    .execute(conn)
    .await?;

    Ok(id)
}

/// Get an entity by ID.
///
/// # Errors
///
/// Returns `FilamentError::EntityNotFound` if no entity with that ID exists.
pub async fn get_entity(pool: &Pool<Sqlite>, id: &str) -> Result<Entity> {
    sqlx::query_as::<_, Entity>("SELECT * FROM entities WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| FilamentError::EntityNotFound { id: id.to_string() })
}

/// List entities, optionally filtered by type and/or status.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_entities(
    pool: &Pool<Sqlite>,
    entity_type: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<Entity>> {
    let mut query = String::from("SELECT * FROM entities WHERE 1=1");
    if entity_type.is_some() {
        query.push_str(" AND entity_type = ?");
    }
    if status.is_some() {
        query.push_str(" AND status = ?");
    }
    query.push_str(" ORDER BY priority ASC, created_at ASC");

    let mut q = sqlx::query_as::<_, Entity>(&query);
    if let Some(et) = entity_type {
        q = q.bind(et);
    }
    if let Some(s) = status {
        q = q.bind(s);
    }

    Ok(q.fetch_all(pool).await?)
}

/// Update entity status.
///
/// # Errors
///
/// Returns `FilamentError::EntityNotFound` if the entity doesn't exist.
pub async fn update_entity_status(
    conn: &mut SqliteConnection,
    id: &str,
    status: EntityStatus,
) -> Result<()> {
    let now = Utc::now();

    let rows = sqlx::query("UPDATE entities SET status = ?, updated_at = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(now)
        .bind(id)
        .execute(conn)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(FilamentError::EntityNotFound { id: id.to_string() });
    }
    Ok(())
}

/// Delete an entity.
///
/// # Errors
///
/// Returns `FilamentError::EntityNotFound` if the entity doesn't exist.
pub async fn delete_entity(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    let rows = sqlx::query("DELETE FROM entities WHERE id = ?")
        .bind(id)
        .execute(conn)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(FilamentError::EntityNotFound { id: id.to_string() });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Relation repo functions
// ---------------------------------------------------------------------------

/// Create a relation. Returns the new ID.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure (including FK violations).
pub async fn create_relation(
    conn: &mut SqliteConnection,
    req: &ValidCreateRelationRequest,
) -> Result<RelationId> {
    let id = RelationId::new();
    let now = Utc::now();
    let metadata = serde_json::to_string(&req.metadata).unwrap_or_default();

    sqlx::query(
        "INSERT INTO relations (id, source_id, target_id, relation_type, weight, summary, metadata, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(req.source_id.as_str())
    .bind(req.target_id.as_str())
    .bind(req.relation_type.as_str())
    .bind(req.weight)
    .bind(&req.summary)
    .bind(&metadata)
    .bind(now)
    .execute(conn)
    .await?;

    Ok(id)
}

/// List relations for an entity (as source or target).
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_relations(pool: &Pool<Sqlite>, entity_id: &str) -> Result<Vec<Relation>> {
    Ok(sqlx::query_as::<_, Relation>(
        "SELECT * FROM relations WHERE source_id = ? OR target_id = ? ORDER BY created_at ASC",
    )
    .bind(entity_id)
    .bind(entity_id)
    .fetch_all(pool)
    .await?)
}

/// Delete a relation.
///
/// # Errors
///
/// Returns `FilamentError::RelationNotFound` if the relation doesn't exist.
pub async fn delete_relation(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    let rows = sqlx::query("DELETE FROM relations WHERE id = ?")
        .bind(id)
        .execute(conn)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(FilamentError::RelationNotFound {
            source_id: id.to_string(),
            target_id: String::new(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Message repo functions
// ---------------------------------------------------------------------------

/// Send a message.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn send_message(
    conn: &mut SqliteConnection,
    req: &ValidSendMessageRequest,
) -> Result<MessageId> {
    let id = MessageId::new();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO messages (id, from_agent, to_agent, msg_type, body, status, in_reply_to, task_id, created_at)
         VALUES (?, ?, ?, ?, ?, 'unread', ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(req.from_agent.as_str())
    .bind(req.to_agent.as_str())
    .bind(req.msg_type.as_str())
    .bind(req.body.as_str())
    .bind(&req.in_reply_to)
    .bind(&req.task_id)
    .bind(now)
    .execute(conn)
    .await?;

    Ok(id)
}

/// Get inbox (unread messages for an agent).
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn get_inbox(pool: &Pool<Sqlite>, agent: &str) -> Result<Vec<Message>> {
    Ok(sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE to_agent = ? AND status = 'unread' ORDER BY created_at ASC",
    )
    .bind(agent)
    .fetch_all(pool)
    .await?)
}

/// Mark a message as read.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn mark_message_read(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    let now = Utc::now();
    sqlx::query("UPDATE messages SET status = 'read', read_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Reservation repo functions
// ---------------------------------------------------------------------------

/// Acquire a file reservation.
///
/// # Errors
///
/// Returns `FilamentError::FileReserved` if a conflicting exclusive reservation exists.
pub async fn acquire_reservation(
    conn: &mut SqliteConnection,
    agent_name: &str,
    file_glob: &str,
    exclusive: bool,
    ttl: TtlSeconds,
) -> Result<ReservationId> {
    let now = Utc::now();
    let expires_at = now + ttl.as_duration();

    // Check for conflicting exclusive reservations (simple glob equality check)
    let conflict = sqlx::query_as::<_, Reservation>(
        "SELECT * FROM file_reservations WHERE file_glob = ? AND exclusive = 1 AND expires_at > ? AND agent_name != ?",
    )
    .bind(file_glob)
    .bind(now)
    .bind(agent_name)
    .fetch_optional(&mut *conn)
    .await?;

    if let Some(r) = conflict {
        return Err(FilamentError::FileReserved {
            agent: r.agent_name,
            glob: r.file_glob,
        });
    }

    let id = ReservationId::new();
    sqlx::query(
        "INSERT INTO file_reservations (id, agent_name, file_glob, exclusive, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(agent_name)
    .bind(file_glob)
    .bind(exclusive)
    .bind(now)
    .bind(expires_at)
    .execute(conn)
    .await?;

    Ok(id)
}

/// Release a reservation.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn release_reservation(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM file_reservations WHERE id = ?")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Clean up expired reservations.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn expire_stale_reservations(conn: &mut SqliteConnection) -> Result<u64> {
    let now = Utc::now();
    let rows = sqlx::query("DELETE FROM file_reservations WHERE expires_at <= ?")
        .bind(now)
        .execute(conn)
        .await?
        .rows_affected();
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Agent run repo functions
// ---------------------------------------------------------------------------

/// Record a new agent run.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn create_agent_run(
    conn: &mut SqliteConnection,
    task_id: &str,
    agent_role: &str,
    pid: Option<i32>,
) -> Result<AgentRunId> {
    let id = AgentRunId::new();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO agent_runs (id, task_id, agent_role, pid, status, started_at)
         VALUES (?, ?, ?, ?, 'running', ?)",
    )
    .bind(id.as_str())
    .bind(task_id)
    .bind(agent_role)
    .bind(pid)
    .bind(now)
    .execute(conn)
    .await?;

    Ok(id)
}

/// Update an agent run's status and optional result.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn finish_agent_run(
    conn: &mut SqliteConnection,
    id: &str,
    status: AgentStatus,
    result_json: Option<&str>,
) -> Result<()> {
    let now = Utc::now();

    sqlx::query("UPDATE agent_runs SET status = ?, result_json = ?, finished_at = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(result_json)
        .bind(now)
        .bind(id)
        .execute(conn)
        .await?;

    Ok(())
}

/// Get running agent runs.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_running_agents(pool: &Pool<Sqlite>) -> Result<Vec<AgentRun>> {
    Ok(sqlx::query_as::<_, AgentRun>(
        "SELECT * FROM agent_runs WHERE status = 'running' ORDER BY started_at ASC",
    )
    .fetch_all(pool)
    .await?)
}

// ---------------------------------------------------------------------------
// Event log
// ---------------------------------------------------------------------------

/// Record an event.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn record_event(
    conn: &mut SqliteConnection,
    entity_id: Option<&str>,
    event_type: EventType,
    actor: &str,
    old_value: Option<&str>,
    new_value: Option<&str>,
) -> Result<EventId> {
    let id = EventId::new();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO events (id, entity_id, event_type, actor, old_value, new_value, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(entity_id)
    .bind(event_type.as_str())
    .bind(actor)
    .bind(old_value)
    .bind(new_value)
    .bind(now)
    .execute(conn)
    .await?;

    Ok(id)
}

/// Get events for an entity.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn get_entity_events(pool: &Pool<Sqlite>, entity_id: &str) -> Result<Vec<Event>> {
    Ok(sqlx::query_as::<_, Event>(
        "SELECT * FROM events WHERE entity_id = ? ORDER BY created_at ASC",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await?)
}

// ---------------------------------------------------------------------------
// Blocked entities cache
// ---------------------------------------------------------------------------

/// Rebuild the blocked entities cache for all tasks.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn rebuild_blocked_cache(conn: &mut SqliteConnection) -> Result<()> {
    let now = Utc::now();

    // Clear existing cache
    sqlx::query("DELETE FROM blocked_entities_cache")
        .execute(&mut *conn)
        .await?;

    // Find all entities that have a "blocks" relation pointing at them where the blocker is not closed
    // i.e., entity X is blocked if there exists a relation (Y blocks X) and Y.status != 'closed'
    sqlx::query(
        "INSERT INTO blocked_entities_cache (entity_id, blocker_ids_json, updated_at)
         SELECT r.target_id,
                json_group_array(r.source_id),
                ?
         FROM relations r
         JOIN entities e ON e.id = r.source_id
         WHERE r.relation_type = 'blocks'
           AND e.status != 'closed'
         GROUP BY r.target_id",
    )
    .bind(now)
    .execute(conn)
    .await?;

    Ok(())
}

/// Get ready tasks: open tasks not in the blocked cache, ordered by priority.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn ready_tasks(pool: &Pool<Sqlite>) -> Result<Vec<Entity>> {
    Ok(sqlx::query_as::<_, Entity>(
        "SELECT e.* FROM entities e
         WHERE e.entity_type = 'task'
           AND e.status IN ('open', 'in_progress')
           AND e.id NOT IN (SELECT entity_id FROM blocked_entities_cache)
         ORDER BY e.priority ASC, e.created_at ASC",
    )
    .fetch_all(pool)
    .await?)
}
