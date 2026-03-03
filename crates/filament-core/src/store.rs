use chrono::Utc;
use sqlx::{Pool, Sqlite, SqliteConnection};

use crate::error::{FilamentError, Result};
use crate::models::{
    AgentRun, AgentRunId, AgentStatus, Entity, EntityCommon, EntityId, EntityStatus, EntityType,
    Event, EventId, EventType, Message, MessageId, NonEmptyString, Priority, Relation, RelationId,
    Reservation, ReservationId, Slug, TtlSeconds, ValidCreateEntityRequest,
    ValidCreateRelationRequest, ValidSendMessageRequest,
};

// ---------------------------------------------------------------------------
// Internal DB row struct (not part of public API)
// ---------------------------------------------------------------------------

/// Flat row struct for sqlx queries — converted to `Entity` ADT via `From`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct EntityRow {
    pub id: EntityId,
    pub slug: Slug,
    pub name: NonEmptyString,
    pub entity_type: EntityType,
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content_path: Option<String>,
    pub content_hash: Option<String>,
    pub status: EntityStatus,
    pub priority: Priority,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

impl From<EntityRow> for Entity {
    fn from(row: EntityRow) -> Self {
        let common = EntityCommon {
            id: row.id,
            slug: row.slug,
            name: row.name,
            summary: row.summary,
            key_facts: row.key_facts,
            content_path: row.content_path,
            content_hash: row.content_hash,
            status: row.status,
            priority: row.priority,
            created_at: row.created_at,
            updated_at: row.updated_at,
        };
        match row.entity_type {
            EntityType::Task => Self::Task(common),
            EntityType::Module => Self::Module(common),
            EntityType::Service => Self::Service(common),
            EntityType::Agent => Self::Agent(common),
            EntityType::Plan => Self::Plan(common),
            EntityType::Doc => Self::Doc(common),
        }
    }
}

// ---------------------------------------------------------------------------
// Executor abstraction (from workout-util pattern)
// ---------------------------------------------------------------------------

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

/// Create an entity. Returns the new `(ID, Slug)`.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
///
/// # Panics
///
/// Panics if `serde_json::Value` serialization fails (infallible in practice).
pub async fn create_entity(
    conn: &mut SqliteConnection,
    req: &ValidCreateEntityRequest,
) -> Result<(EntityId, Slug)> {
    let id = EntityId::new();
    let slug = Slug::new();
    let now = Utc::now();
    let key_facts =
        serde_json::to_string(&req.key_facts).expect("Value serialization is infallible");

    sqlx::query(
        "INSERT INTO entities (id, slug, name, entity_type, summary, key_facts, content_path, status, priority, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'open', ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(slug.as_str())
    .bind(req.name.as_str())
    .bind(req.entity_type.as_str())
    .bind(&req.summary)
    .bind(&key_facts)
    .bind(&req.content_path)
    .bind(req.priority)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;

    record_event(
        conn,
        Some(id.as_str()),
        EventType::EntityCreated,
        "system",
        None,
        Some(req.name.as_str()),
    )
    .await?;

    Ok((id, slug))
}

/// Get an entity by ID.
///
/// # Errors
///
/// Returns `FilamentError::EntityNotFound` if no entity with that ID exists.
pub async fn get_entity(pool: &Pool<Sqlite>, id: &str) -> Result<Entity> {
    sqlx::query_as::<_, EntityRow>("SELECT * FROM entities WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(Entity::from)
        .ok_or_else(|| FilamentError::EntityNotFound { id: id.to_string() })
}

/// Get an entity by slug.
///
/// # Errors
///
/// Returns `FilamentError::EntityNotFound` if no entity with that slug exists.
pub async fn get_entity_by_slug(pool: &Pool<Sqlite>, slug: &str) -> Result<Entity> {
    sqlx::query_as::<_, EntityRow>("SELECT * FROM entities WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await?
        .map(Entity::from)
        .ok_or_else(|| FilamentError::EntityNotFound {
            id: format!("slug:{slug}"),
        })
}

/// Resolve an entity by slug (first) or UUID fallback.
///
/// # Errors
///
/// Returns `FilamentError::EntityNotFound` if neither slug nor ID matches.
pub async fn resolve_entity(pool: &Pool<Sqlite>, slug_or_id: &str) -> Result<Entity> {
    // Try slug first (most common usage)
    match get_entity_by_slug(pool, slug_or_id).await {
        Ok(entity) => return Ok(entity),
        Err(FilamentError::EntityNotFound { .. }) => {}
        Err(e) => return Err(e),
    }
    // Fall back to UUID lookup
    get_entity(pool, slug_or_id).await
}

/// Resolve an entity and verify it is a task.
///
/// # Errors
///
/// Returns `TypeMismatch` if the entity is not a task.
pub async fn resolve_task(pool: &Pool<Sqlite>, slug_or_id: &str) -> Result<EntityCommon> {
    let entity = resolve_entity(pool, slug_or_id).await?;
    match entity {
        Entity::Task(c) => Ok(c),
        other => Err(FilamentError::TypeMismatch {
            expected: EntityType::Task,
            actual: other.entity_type(),
            slug: other.slug().clone(),
        }),
    }
}

/// Resolve an entity and verify it is an agent.
///
/// # Errors
///
/// Returns `TypeMismatch` if the entity is not an agent.
pub async fn resolve_agent(pool: &Pool<Sqlite>, slug_or_id: &str) -> Result<EntityCommon> {
    let entity = resolve_entity(pool, slug_or_id).await?;
    match entity {
        Entity::Agent(c) => Ok(c),
        other => Err(FilamentError::TypeMismatch {
            expected: EntityType::Agent,
            actual: other.entity_type(),
            slug: other.slug().clone(),
        }),
    }
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

    let mut q = sqlx::query_as::<_, EntityRow>(&query);
    if let Some(et) = entity_type {
        q = q.bind(et);
    }
    if let Some(s) = status {
        q = q.bind(s);
    }

    Ok(q.fetch_all(pool)
        .await?
        .into_iter()
        .map(Entity::from)
        .collect())
}

/// Update entity summary.
///
/// # Errors
///
/// Returns `FilamentError::EntityNotFound` if the entity doesn't exist.
pub async fn update_entity_summary(
    conn: &mut SqliteConnection,
    id: &str,
    summary: &str,
) -> Result<()> {
    let now = Utc::now();

    let rows = sqlx::query("UPDATE entities SET summary = ?, updated_at = ? WHERE id = ?")
        .bind(summary)
        .bind(now)
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(FilamentError::EntityNotFound { id: id.to_string() });
    }

    record_event(
        conn,
        Some(id),
        EventType::EntityUpdated,
        "system",
        None,
        Some(summary),
    )
    .await?;

    Ok(())
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
        .execute(&mut *conn)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(FilamentError::EntityNotFound { id: id.to_string() });
    }

    record_event(
        conn,
        Some(id),
        EventType::StatusChange,
        "system",
        None,
        Some(status.as_str()),
    )
    .await?;

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
        .execute(&mut *conn)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(FilamentError::EntityNotFound { id: id.to_string() });
    }

    record_event(
        conn,
        Some(id),
        EventType::EntityDeleted,
        "system",
        None,
        None,
    )
    .await?;

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
///
/// # Panics
///
/// Panics if `serde_json::Value` serialization fails (infallible in practice).
pub async fn create_relation(
    conn: &mut SqliteConnection,
    req: &ValidCreateRelationRequest,
) -> Result<RelationId> {
    let id = RelationId::new();
    let now = Utc::now();
    let metadata = serde_json::to_string(&req.metadata).expect("Value serialization is infallible");

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
    .execute(&mut *conn)
    .await?;

    let detail = format!(
        "{} -{}- {}",
        req.source_id, req.relation_type, req.target_id
    );
    record_event(
        conn,
        Some(req.source_id.as_str()),
        EventType::RelationCreated,
        "system",
        None,
        Some(&detail),
    )
    .await?;

    Ok(id)
}

/// Get a relation by ID.
///
/// # Errors
///
/// Returns `FilamentError::RelationNotFound` if no relation with that ID exists.
pub async fn get_relation(pool: &Pool<Sqlite>, id: &str) -> Result<Relation> {
    sqlx::query_as::<_, Relation>("SELECT * FROM relations WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| FilamentError::RelationNotFound { id: id.to_string() })
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

/// Delete a relation by its endpoints and type.
///
/// # Errors
///
/// Returns `FilamentError::RelationNotFound` if no matching relation exists.
pub async fn delete_relation_by_endpoints(
    conn: &mut SqliteConnection,
    source_id: &str,
    target_id: &str,
    relation_type: &str,
) -> Result<()> {
    let rows = sqlx::query(
        "DELETE FROM relations WHERE source_id = ? AND target_id = ? AND relation_type = ?",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(relation_type)
    .execute(&mut *conn)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(FilamentError::RelationNotFound {
            id: format!("{source_id} -{relation_type}-> {target_id}"),
        });
    }

    let detail = format!("{source_id} -{relation_type}-> {target_id}");
    record_event(
        conn,
        Some(source_id),
        EventType::RelationDeleted,
        "system",
        Some(&detail),
        None,
    )
    .await?;

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
    .execute(&mut *conn)
    .await?;

    record_event(
        conn,
        req.task_id.as_ref().map(EntityId::as_str),
        EventType::MessageSent,
        req.from_agent.as_str(),
        None,
        Some(req.to_agent.as_str()),
    )
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
    let rows = sqlx::query(
        "UPDATE messages SET status = 'read', read_at = ? WHERE id = ? AND status = 'unread'",
    )
    .bind(now)
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(FilamentError::MessageNotFound { id: id.to_string() });
    }

    record_event(conn, None, EventType::MessageRead, "system", None, Some(id)).await?;

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

    // Check for conflicting reservations (simple glob equality check).
    // Exclusive requests conflict with ANY existing reservation by other agents.
    // Non-exclusive requests only conflict with exclusive reservations by other agents.
    let conflict_sql = if exclusive {
        "SELECT * FROM file_reservations WHERE file_glob = ? AND expires_at > ? AND agent_name != ?"
    } else {
        "SELECT * FROM file_reservations WHERE file_glob = ? AND exclusive = 1 AND expires_at > ? AND agent_name != ?"
    };
    let conflict = sqlx::query_as::<_, Reservation>(conflict_sql)
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
    .execute(&mut *conn)
    .await?;

    record_event(
        conn,
        None,
        EventType::ReservationAcquired,
        agent_name,
        None,
        Some(file_glob),
    )
    .await?;

    Ok(id)
}

/// Release a reservation.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn release_reservation(conn: &mut SqliteConnection, id: &str) -> Result<()> {
    let result = sqlx::query("DELETE FROM file_reservations WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;

    if result.rows_affected() == 0 {
        return Err(FilamentError::ReservationNotFound { id: id.to_string() });
    }

    record_event(
        conn,
        None,
        EventType::ReservationReleased,
        "system",
        Some(id),
        None,
    )
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

/// List active reservations, optionally filtered by agent.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_reservations(
    pool: &Pool<Sqlite>,
    agent: Option<&str>,
) -> Result<Vec<Reservation>> {
    let now = Utc::now();
    if let Some(agent_name) = agent {
        Ok(sqlx::query_as::<_, Reservation>(
            "SELECT * FROM file_reservations WHERE agent_name = ? AND expires_at > ? ORDER BY created_at ASC",
        )
        .bind(agent_name)
        .bind(now)
        .fetch_all(pool)
        .await?)
    } else {
        Ok(sqlx::query_as::<_, Reservation>(
            "SELECT * FROM file_reservations WHERE expires_at > ? ORDER BY created_at ASC",
        )
        .bind(now)
        .fetch_all(pool)
        .await?)
    }
}

/// Find a reservation by glob pattern and agent name.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn find_reservation(
    pool: &Pool<Sqlite>,
    file_glob: &str,
    agent_name: &str,
) -> Result<Option<Reservation>> {
    let now = Utc::now();
    Ok(sqlx::query_as::<_, Reservation>(
        "SELECT * FROM file_reservations WHERE file_glob = ? AND agent_name = ? AND expires_at > ?",
    )
    .bind(file_glob)
    .bind(agent_name)
    .bind(now)
    .fetch_optional(pool)
    .await?)
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
    .execute(&mut *conn)
    .await?;

    record_event(
        conn,
        Some(task_id),
        EventType::AgentStarted,
        agent_role,
        None,
        Some(id.as_str()),
    )
    .await?;

    Ok(id)
}

/// Update an agent run's status and optional result.
///
/// # Errors
///
/// Returns `FilamentError::AgentRunNotFound` if the agent run doesn't exist.
pub async fn finish_agent_run(
    conn: &mut SqliteConnection,
    id: &str,
    status: AgentStatus,
    result_json: Option<&str>,
) -> Result<()> {
    let now = Utc::now();

    // Look up the run to get task_id and agent_role for the event
    let run: AgentRun = sqlx::query_as::<_, AgentRun>("SELECT * FROM agent_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| FilamentError::AgentRunNotFound { id: id.to_string() })?;

    sqlx::query("UPDATE agent_runs SET status = ?, result_json = ?, finished_at = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(result_json)
        .bind(now)
        .bind(id)
        .execute(&mut *conn)
        .await?;

    record_event(
        conn,
        Some(run.task_id.as_str()),
        EventType::AgentFinished,
        run.agent_role.as_str(),
        None,
        Some(status.as_str()),
    )
    .await?;

    Ok(())
}

/// Get a single agent run by ID.
///
/// # Errors
///
/// Returns `FilamentError::AgentRunNotFound` if no run with that ID exists.
pub async fn get_agent_run(pool: &Pool<Sqlite>, id: &str) -> Result<AgentRun> {
    sqlx::query_as::<_, AgentRun>("SELECT * FROM agent_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| FilamentError::AgentRunNotFound { id: id.to_string() })
}

/// List agent runs for a specific task, ordered by start time (most recent first).
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_agent_runs_by_task(pool: &Pool<Sqlite>, task_id: &str) -> Result<Vec<AgentRun>> {
    Ok(sqlx::query_as::<_, AgentRun>(
        "SELECT * FROM agent_runs WHERE task_id = ? ORDER BY started_at DESC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?)
}

/// Check if a task has a running agent.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn has_running_agent(pool: &Pool<Sqlite>, task_id: &str) -> Result<bool> {
    let row: Option<(i32,)> =
        sqlx::query_as("SELECT 1 FROM agent_runs WHERE task_id = ? AND status = 'running' LIMIT 1")
            .bind(task_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

/// Release all reservations held by a specific agent. Used for death cleanup.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn release_reservations_by_agent(
    conn: &mut SqliteConnection,
    agent_name: &str,
) -> Result<u64> {
    let rows = sqlx::query("DELETE FROM file_reservations WHERE agent_name = ?")
        .bind(agent_name)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(rows)
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

    // An entity is blocked if:
    //   1. Someone blocks it: relation (Y blocks X) and Y.status != 'closed'
    //   2. It depends on something: relation (X depends_on Y) and Y.status != 'closed'
    sqlx::query(
        "INSERT INTO blocked_entities_cache (entity_id, blocker_ids_json, updated_at)
         SELECT blocked_id, json_group_array(blocker_id), ?
         FROM (
             -- Y blocks X: X is blocked by Y
             SELECT r.target_id AS blocked_id, r.source_id AS blocker_id
             FROM relations r
             JOIN entities e ON e.id = r.source_id
             WHERE r.relation_type = 'blocks' AND e.status != 'closed'
             UNION ALL
             -- X depends_on Y: X is blocked by Y
             SELECT r.source_id AS blocked_id, r.target_id AS blocker_id
             FROM relations r
             JOIN entities e ON e.id = r.target_id
             WHERE r.relation_type = 'depends_on' AND e.status != 'closed'
         )
         GROUP BY blocked_id",
    )
    .bind(now)
    .execute(conn)
    .await?;

    Ok(())
}

/// Get ready tasks: open tasks not in the blocked cache, ordered by priority.
///
/// Automatically rebuilds the blocked cache before querying.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn ready_tasks(conn: &mut SqliteConnection) -> Result<Vec<Entity>> {
    rebuild_blocked_cache(conn).await?;

    let rows = sqlx::query_as::<_, EntityRow>(
        "SELECT e.* FROM entities e
         WHERE e.entity_type = 'task'
           AND e.status IN ('open', 'in_progress')
           AND e.id NOT IN (SELECT entity_id FROM blocked_entities_cache)
         ORDER BY e.priority ASC, e.created_at ASC",
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows.into_iter().map(Entity::from).collect())
}
