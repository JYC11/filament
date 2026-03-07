use chrono::Utc;
use sqlx::{Pool, Sqlite, SqliteConnection};

use std::collections::HashSet;

use crate::diff::fields_in_diff;
use crate::dto::{
    EntityChangeset, Escalation, EscalationKind, ExportData, ImportResult,
    ValidCreateEntityRequest, ValidCreateRelationRequest, ValidSendMessageRequest,
};
use crate::error::{FieldConflict, FilamentError, Result};
use crate::models::{
    AgentRun, AgentRunId, AgentStatus, ContentRef, Entity, EntityCommon, EntityId, EntityStatus,
    EntityType, Event, Message, MessageId, NonEmptyString, Priority, Relation, RelationId,
    Reservation, ReservationId, ReservationMode, Slug, TtlSeconds,
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
    pub version: i64,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

/// Row struct for FTS5 search results — entity fields + rank.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct SearchRow {
    #[sqlx(flatten)]
    pub entity: EntityRow,
    pub rank: f64,
}

impl From<EntityRow> for Entity {
    fn from(row: EntityRow) -> Self {
        let content = row.content_path.map(|path| ContentRef {
            path,
            hash: row.content_hash,
        });
        let common = EntityCommon {
            id: row.id,
            slug: row.slug,
            name: row.name,
            summary: row.summary,
            key_facts: row.key_facts,
            content,
            status: row.status,
            priority: row.priority,
            version: row.version,
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
            EntityType::Lesson => Self::Lesson(common),
        }
    }
}

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
    resolve_entity(pool, slug_or_id).await?.into_task()
}

/// Resolve an entity and verify it is an agent.
///
/// # Errors
///
/// Returns `TypeMismatch` if the entity is not an agent.
pub async fn resolve_agent(pool: &Pool<Sqlite>, slug_or_id: &str) -> Result<EntityCommon> {
    resolve_entity(pool, slug_or_id).await?.into_agent()
}

/// Resolve an entity and verify it is a lesson.
///
/// # Errors
///
/// Returns `TypeMismatch` if the entity is not a lesson.
pub async fn resolve_lesson(pool: &Pool<Sqlite>, slug_or_id: &str) -> Result<EntityCommon> {
    resolve_entity(pool, slug_or_id).await?.into_lesson()
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

/// List lessons, optionally filtered by pattern name (SQL `LIKE` on `key_facts.pattern`).
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_lessons(
    pool: &Pool<Sqlite>,
    status: Option<&str>,
    pattern: Option<&str>,
) -> Result<Vec<Entity>> {
    let mut query = String::from("SELECT * FROM entities WHERE entity_type = 'lesson'");
    if status.is_some() {
        query.push_str(" AND status = ?");
    }
    if pattern.is_some() {
        query.push_str(" AND json_extract(key_facts, '$.pattern') LIKE ?");
    }
    query.push_str(" ORDER BY priority ASC, created_at ASC");

    let mut q = sqlx::query_as::<_, EntityRow>(&query);
    if let Some(s) = status {
        q = q.bind(s);
    }
    if let Some(p) = pattern {
        q = q.bind(format!("%{p}%"));
    }

    Ok(q.fetch_all(pool)
        .await?
        .into_iter()
        .map(Entity::from)
        .collect())
}

/// Search entities using FTS5 full-text search with BM25 ranking.
///
/// Searches across `name`, `summary`, and `key_facts` fields.
/// Optional `entity_type` filter restricts results to a specific type.
/// Returns results ordered by relevance (best match first), limited to `limit`.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn search_entities(
    pool: &Pool<Sqlite>,
    query: &str,
    entity_type: Option<&str>,
    limit: u32,
) -> Result<Vec<(Entity, f64)>> {
    // FTS5 external-content join: match on FTS, join back to entities for full row
    let sql = if entity_type.is_some() {
        "SELECT e.id, e.slug, e.name, e.entity_type, e.summary, e.key_facts, \
                e.content_path, e.content_hash, e.status, e.priority, e.version, \
                e.created_at, e.updated_at, f.rank \
         FROM entities_fts f \
         JOIN entities e ON e.rowid = f.rowid \
         WHERE entities_fts MATCH ? AND e.entity_type = ? \
         ORDER BY f.rank \
         LIMIT ?"
    } else {
        "SELECT e.id, e.slug, e.name, e.entity_type, e.summary, e.key_facts, \
                e.content_path, e.content_hash, e.status, e.priority, e.version, \
                e.created_at, e.updated_at, f.rank \
         FROM entities_fts f \
         JOIN entities e ON e.rowid = f.rowid \
         WHERE entities_fts MATCH ? \
         ORDER BY f.rank \
         LIMIT ?"
    };

    let mut q = sqlx::query_as::<_, SearchRow>(sql).bind(query);
    if let Some(et) = entity_type {
        q = q.bind(et);
    }
    q = q.bind(limit);

    Ok(q.fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            // FTS5 BM25 rank is negative (lower = better). Negate for display
            // so higher values mean more relevant.
            let relevance = -row.rank;
            (Entity::from(row.entity), relevance)
        })
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

    let rows = sqlx::query(
        "UPDATE entities SET summary = ?, version = version + 1, updated_at = ? WHERE id = ?",
    )
    .bind(summary)
    .bind(now)
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(FilamentError::EntityNotFound { id: id.to_string() });
    }

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

    let rows = sqlx::query(
        "UPDATE entities SET status = ?, version = version + 1, updated_at = ? WHERE id = ?",
    )
    .bind(status.as_str())
    .bind(now)
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(FilamentError::EntityNotFound { id: id.to_string() });
    }

    Ok(())
}

/// Unified entity update with optimistic conflict resolution.
///
/// If `changeset.expected_version` matches the current version, applies changes and bumps the version.
/// If there is a version mismatch, attempts auto-merge on non-overlapping fields.
/// If overlapping fields conflict, returns `VersionConflict`.
///
/// # Errors
///
/// Returns `EntityNotFound` if the entity doesn't exist.
/// Returns `VersionConflict` if concurrent changes overlap.
/// Returns `Validation` if the changeset is empty.
pub async fn update_entity(
    conn: &mut SqliteConnection,
    id: &str,
    changeset: &EntityChangeset,
) -> Result<Entity> {
    if changeset.is_empty() {
        return Err(FilamentError::Validation(
            "changeset has no fields to update".to_string(),
        ));
    }

    // Read current state
    let row: EntityRow = sqlx::query_as::<_, EntityRow>("SELECT * FROM entities WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| FilamentError::EntityNotFound { id: id.to_string() })?;

    let current_version = row.version;

    // Version check + merge
    if changeset.expected_version != current_version {
        try_auto_merge(
            conn,
            id,
            changeset.expected_version,
            current_version,
            changeset,
        )
        .await?;
    }

    // Apply changes via dynamic SQL
    let now = Utc::now();
    let new_version = current_version + 1;
    let new_name = changeset.name.as_ref().unwrap_or(&row.name);
    let new_summary = changeset.summary.as_deref().unwrap_or(&row.summary);
    let new_status = changeset.status.as_ref().unwrap_or(&row.status);
    let new_priority = changeset.priority.unwrap_or(row.priority);
    let old_key_facts_str = serde_json::to_string(&row.key_facts).unwrap_or_default();
    let new_key_facts = changeset.key_facts.as_deref().unwrap_or(&old_key_facts_str);
    let new_content_path = changeset
        .content_path
        .as_deref()
        .or(row.content_path.as_deref());

    let result = sqlx::query(
        "UPDATE entities SET name = ?, summary = ?, status = ?, priority = ?, \
         key_facts = ?, content_path = ?, version = ?, updated_at = ? \
         WHERE id = ? AND version = ?",
    )
    .bind(new_name.as_str())
    .bind(new_summary)
    .bind(new_status.as_str())
    .bind(new_priority)
    .bind(new_key_facts)
    .bind(new_content_path)
    .bind(new_version)
    .bind(now)
    .bind(id)
    .bind(current_version)
    .execute(&mut *conn)
    .await?;

    if result.rows_affected() == 0 {
        return Err(FilamentError::VersionConflict {
            entity_id: id.to_string(),
            current_version: current_version + 1,
            conflicts: vec![],
        });
    }

    // Re-fetch and return updated entity
    let updated = sqlx::query_as::<_, EntityRow>("SELECT * FROM entities WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(Entity::from(updated))
}

/// Attempt auto-merge when version mismatch detected.
///
/// Looks at events since the caller's expected version to find which fields
/// were changed remotely. If the changeset's fields don't overlap with
/// remotely changed fields, the merge succeeds silently. If they overlap,
/// returns `VersionConflict` with all conflicting fields.
async fn try_auto_merge(
    conn: &mut SqliteConnection,
    entity_id: &str,
    expected_version: i64,
    current_version: i64,
    changeset: &EntityChangeset,
) -> Result<()> {
    // Get recent events since the expected version.
    // We only need the last N events where N = current_version - expected_version.
    let version_diff = current_version.saturating_sub(expected_version);
    let events_since = usize::try_from(version_diff).unwrap_or(usize::MAX);
    let limit = i64::try_from(events_since).unwrap_or(i64::MAX);
    let events: Vec<Event> = sqlx::query_as::<_, Event>(
        "SELECT * FROM events WHERE entity_id = ? AND diff IS NOT NULL \
         ORDER BY created_at DESC LIMIT ?",
    )
    .bind(entity_id)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;

    let recent_events: Vec<&Event> = events.iter().collect();

    let mut remotely_changed: HashSet<String> = HashSet::new();

    if recent_events.is_empty() {
        // No diff events found — conservatively assume all fields changed
        // (events recorded before migration have diff = NULL)
        return Err(FilamentError::VersionConflict {
            entity_id: entity_id.to_string(),
            current_version,
            conflicts: build_conflict_list(conn, entity_id, changeset).await?,
        });
    }

    for event in &recent_events {
        if let Some(ref diff_str) = event.diff {
            if let Ok(diff_val) = serde_json::from_str(diff_str) {
                remotely_changed.extend(fields_in_diff(&diff_val));
            }
        } else {
            // NULL-diff event in the range — treat conservatively
            return Err(FilamentError::VersionConflict {
                entity_id: entity_id.to_string(),
                current_version,
                conflicts: build_conflict_list(conn, entity_id, changeset).await?,
            });
        }
    }

    // Check for overlap
    let local_fields: HashSet<String> = changeset
        .changed_field_names()
        .into_iter()
        .map(String::from)
        .collect();

    let overlapping: HashSet<&String> = local_fields.intersection(&remotely_changed).collect();

    if overlapping.is_empty() {
        // No overlap — auto-merge succeeds, proceed with update
        Ok(())
    } else {
        Err(FilamentError::VersionConflict {
            entity_id: entity_id.to_string(),
            current_version,
            conflicts: build_conflict_list(conn, entity_id, changeset).await?,
        })
    }
}

/// Build the list of `FieldConflict` entries for all fields in the changeset
/// that conflict with current DB values.
async fn build_conflict_list(
    conn: &mut SqliteConnection,
    entity_id: &str,
    changeset: &EntityChangeset,
) -> Result<Vec<FieldConflict>> {
    let row: EntityRow = sqlx::query_as::<_, EntityRow>("SELECT * FROM entities WHERE id = ?")
        .bind(entity_id)
        .fetch_one(&mut *conn)
        .await?;

    let mut conflicts = Vec::new();
    if let Some(ref yours) = changeset.name {
        conflicts.push(FieldConflict {
            field: "name".to_string(),
            your_value: yours.to_string(),
            their_value: row.name.to_string(),
        });
    }
    if let Some(ref yours) = changeset.summary {
        conflicts.push(FieldConflict {
            field: "summary".to_string(),
            your_value: yours.clone(),
            their_value: row.summary.clone(),
        });
    }
    if let Some(ref yours) = changeset.status {
        conflicts.push(FieldConflict {
            field: "status".to_string(),
            your_value: yours.to_string(),
            their_value: row.status.to_string(),
        });
    }
    if let Some(yours) = changeset.priority {
        conflicts.push(FieldConflict {
            field: "priority".to_string(),
            your_value: yours.to_string(),
            their_value: row.priority.to_string(),
        });
    }
    if let Some(ref yours) = changeset.key_facts {
        conflicts.push(FieldConflict {
            field: "key_facts".to_string(),
            your_value: yours.clone(),
            their_value: serde_json::to_string(&row.key_facts).unwrap_or_default(),
        });
    }
    if let Some(ref yours) = changeset.content_path {
        conflicts.push(FieldConflict {
            field: "content_path".to_string(),
            your_value: yours.clone(),
            their_value: row.content_path.unwrap_or_default(),
        });
    }
    Ok(conflicts)
}

/// Delete an entity.
///
/// If `expected_version` is provided, the delete only succeeds if the entity's version matches.
/// This prevents deleting a stale entity that was concurrently modified.
///
/// # Errors
///
/// Returns `FilamentError::EntityNotFound` if the entity doesn't exist.
/// Returns `FilamentError::VersionConflict` if the version doesn't match.
pub async fn delete_entity(
    conn: &mut SqliteConnection,
    id: &str,
    expected_version: Option<i64>,
) -> Result<()> {
    let rows = if let Some(version) = expected_version {
        let r = sqlx::query("DELETE FROM entities WHERE id = ? AND version = ?")
            .bind(id)
            .bind(version)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        if r == 0 {
            // Distinguish "not found" from "version mismatch"
            let exists = sqlx::query_scalar::<_, i64>("SELECT version FROM entities WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
            if let Some(current) = exists {
                return Err(FilamentError::VersionConflict {
                    entity_id: id.to_string(),
                    current_version: current,
                    conflicts: vec![],
                });
            }
        }
        r
    } else {
        sqlx::query("DELETE FROM entities WHERE id = ?")
            .bind(id)
            .execute(&mut *conn)
            .await?
            .rows_affected()
    };

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

    let insert_result = sqlx::query(
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
    .await;

    if let Err(sqlx::Error::Database(ref db_err)) = insert_result {
        if db_err.code().as_deref() == Some("2067") {
            return Err(FilamentError::Validation(format!(
                "relation already exists: {} -{}- {}",
                req.source_id, req.relation_type, req.target_id
            )));
        }
    }
    insert_result?;

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
        // Distinguish "message doesn't exist" from "already read"
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM messages WHERE id = ?)")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
        return if exists {
            Err(FilamentError::MessageAlreadyRead { id: id.to_string() })
        } else {
            Err(FilamentError::MessageNotFound { id: id.to_string() })
        };
    }

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
    mode: ReservationMode,
    ttl: TtlSeconds,
) -> Result<ReservationId> {
    let trimmed = file_glob.trim();
    if trimmed.is_empty() {
        return Err(FilamentError::Validation(
            "file glob pattern cannot be empty".to_string(),
        ));
    }
    let now = Utc::now();
    let expires_at = now + ttl.as_duration();

    // Check for conflicting reservations (simple glob equality check).
    // Exclusive requests conflict with ANY existing reservation by other agents.
    // Shared requests only conflict with exclusive reservations by other agents.
    let conflict_sql = if mode.is_exclusive() {
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
            agent: r.agent_name.to_string(),
            glob: r.file_glob.to_string(),
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
    .bind(mode)
    .bind(now)
    .bind(expires_at)
    .execute(&mut *conn)
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

    // Verify the run exists before updating
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_runs WHERE id = ?)")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;

    if !exists {
        return Err(FilamentError::AgentRunNotFound { id: id.to_string() });
    }

    sqlx::query("UPDATE agent_runs SET status = ?, result_json = ?, finished_at = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(result_json)
        .bind(now)
        .bind(id)
        .execute(&mut *conn)
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

/// Check if a task has a running agent (pool version for read-only queries).
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

/// Check if a task has a running agent (connection version for use inside transactions).
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn has_running_agent_conn(conn: &mut SqliteConnection, task_id: &str) -> Result<bool> {
    let row: Option<(i32,)> =
        sqlx::query_as("SELECT 1 FROM agent_runs WHERE task_id = ? AND status = 'running' LIMIT 1")
            .bind(task_id)
            .fetch_optional(&mut *conn)
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

/// Count how many things block each entity (i.e., "blocked by" count).
///
/// An entity X is "blocked by" another entity if:
/// - X has an outgoing `DependsOn` relation (X `depends_on` Y → Y blocks X)
/// - X has an incoming `Blocks` relation (Y `blocks` X)
///
/// Returns a map of `entity_id → count` for all entities with at least one blocker.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn blocked_by_counts(
    pool: &Pool<Sqlite>,
) -> Result<std::collections::HashMap<String, usize>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT entity_id, COUNT(*) FROM ( \
           SELECT source_id AS entity_id FROM relations WHERE relation_type = 'depends_on' \
           UNION ALL \
           SELECT target_id AS entity_id FROM relations WHERE relation_type = 'blocks' \
         ) GROUP BY entity_id",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, count)| (id, usize::try_from(count).unwrap_or(0)))
        .collect())
}

/// Batch-fetch entities by their IDs in a single query.
///
/// Returns a map of `id → Entity` for all found entities. Missing IDs are silently
/// omitted from the result.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn batch_get_entities(
    pool: &Pool<Sqlite>,
    ids: &[&str],
) -> Result<std::collections::HashMap<String, Entity>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // Build `WHERE id IN (?, ?, ...)` with one placeholder per ID
    let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
    let query = format!(
        "SELECT * FROM entities WHERE id IN ({})",
        placeholders.join(", ")
    );

    let mut q = sqlx::query_as::<_, EntityRow>(&query);
    for id in ids {
        q = q.bind(*id);
    }

    let rows = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let id = r.id.to_string();
            (id, Entity::from(r))
        })
        .collect())
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

/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_all_agent_runs(pool: &Pool<Sqlite>, limit: u32) -> Result<Vec<AgentRun>> {
    Ok(
        sqlx::query_as::<_, AgentRun>("SELECT * FROM agent_runs ORDER BY started_at DESC LIMIT ?")
            .bind(limit)
            .fetch_all(pool)
            .await?,
    )
}

/// Mark all `running` agent runs as `failed` and revert their tasks to `open`.
///
/// Called on daemon startup to reconcile stale state left by an unclean shutdown.
/// Returns the number of agent runs that were reconciled.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn reconcile_stale_agent_runs(conn: &mut SqliteConnection) -> Result<u64> {
    let now = Utc::now();

    // Collect task IDs before updating, so we can revert their status
    let stale_task_ids: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT task_id FROM agent_runs WHERE status = 'running'")
            .fetch_all(&mut *conn)
            .await?;

    let rows = sqlx::query(
        "UPDATE agent_runs SET status = 'failed', \
         result_json = '{\"error\":\"daemon restarted — stale run reconciled\"}', \
         finished_at = ? \
         WHERE status = 'running'",
    )
    .bind(now)
    .execute(&mut *conn)
    .await?
    .rows_affected();

    // Revert affected tasks from in_progress back to open
    for (task_id,) in &stale_task_ids {
        // Only revert if the task is still in_progress (may have been closed by other means)
        sqlx::query(
            "UPDATE entities SET status = 'open', version = version + 1, updated_at = ? \
             WHERE id = ? AND status = 'in_progress'",
        )
        .bind(now)
        .bind(task_id.as_str())
        .execute(&mut *conn)
        .await?;
    }

    Ok(rows)
}

// ---------------------------------------------------------------------------
// Event log
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Export / Import
// ---------------------------------------------------------------------------

/// List all relations in the database.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_all_relations(pool: &Pool<Sqlite>) -> Result<Vec<Relation>> {
    Ok(
        sqlx::query_as::<_, Relation>("SELECT * FROM relations ORDER BY created_at ASC")
            .fetch_all(pool)
            .await?,
    )
}

/// List all messages in the database.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_all_messages(pool: &Pool<Sqlite>) -> Result<Vec<Message>> {
    Ok(
        sqlx::query_as::<_, Message>("SELECT * FROM messages ORDER BY created_at ASC")
            .fetch_all(pool)
            .await?,
    )
}

/// List all events in the database.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_all_events(pool: &Pool<Sqlite>) -> Result<Vec<Event>> {
    Ok(
        sqlx::query_as::<_, Event>("SELECT * FROM events ORDER BY created_at ASC")
            .fetch_all(pool)
            .await?,
    )
}

/// Export all data from the database.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn export_all(pool: &Pool<Sqlite>, include_events: bool) -> Result<ExportData> {
    let entities = list_entities(pool, None, None).await?;
    let relations = list_all_relations(pool).await?;
    let messages = list_all_messages(pool).await?;
    let events = if include_events {
        list_all_events(pool).await?
    } else {
        Vec::new()
    };

    Ok(ExportData {
        version: 1,
        exported_at: Utc::now(),
        entities,
        relations,
        messages,
        events,
    })
}

/// Import data into the database (upsert entities, skip-duplicate relations/messages/events).
///
/// Runs inside a single transaction for atomicity.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn import_data(
    conn: &mut SqliteConnection,
    data: &ExportData,
    include_events: bool,
) -> Result<ImportResult> {
    // Disable triggers to prevent duplicate events during import
    sqlx::query("UPDATE _trigger_control SET disabled = 1")
        .execute(&mut *conn)
        .await?;

    let result = async {
        let entities_imported = import_entities(&mut *conn, &data.entities).await?;
        let relations_imported = import_relations(&mut *conn, &data.relations).await?;
        let messages_imported = import_messages(&mut *conn, &data.messages).await?;
        let events_imported = if include_events {
            import_events(&mut *conn, &data.events).await?
        } else {
            0
        };

        Ok(ImportResult {
            entities_imported,
            relations_imported,
            messages_imported,
            events_imported,
        })
    }
    .await;

    // Re-enable triggers regardless of import outcome
    sqlx::query("UPDATE _trigger_control SET disabled = 0")
        .execute(&mut *conn)
        .await?;

    result
}

async fn import_entities(conn: &mut SqliteConnection, entities: &[Entity]) -> Result<usize> {
    let mut count = 0;
    for entity in entities {
        let c = entity.common();
        let key_facts =
            serde_json::to_string(&c.key_facts).expect("Value serialization is infallible");
        let (content_path, content_hash) = c.content.as_ref().map_or((None, None), |cr| {
            (Some(cr.path.as_str()), cr.hash.as_deref())
        });

        // Use ON CONFLICT DO UPDATE to upsert entities without triggering
        // CASCADE deletes that INSERT OR REPLACE would cause on relations.
        sqlx::query(
            "INSERT INTO entities (id, slug, name, entity_type, summary, key_facts, content_path, content_hash, status, priority, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
               slug = excluded.slug,
               name = excluded.name,
               entity_type = excluded.entity_type,
               summary = excluded.summary,
               key_facts = excluded.key_facts,
               content_path = excluded.content_path,
               content_hash = excluded.content_hash,
               status = excluded.status,
               priority = excluded.priority,
               updated_at = excluded.updated_at",
        )
        .bind(c.id.as_str())
        .bind(c.slug.as_str())
        .bind(c.name.as_str())
        .bind(entity.entity_type().as_str())
        .bind(&c.summary)
        .bind(&key_facts)
        .bind(content_path)
        .bind(content_hash)
        .bind(c.status.as_str())
        .bind(c.priority)
        .bind(c.created_at)
        .bind(c.updated_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db_err) if db_err.message().contains("UNIQUE constraint failed: entities.slug") => {
                FilamentError::Validation(format!(
                    "import conflict: slug '{}' is already used by a different entity",
                    c.slug,
                ))
            }
            _ => FilamentError::from(e),
        })?;
        count += 1;
    }
    Ok(count)
}

async fn import_relations(conn: &mut SqliteConnection, relations: &[Relation]) -> Result<usize> {
    let mut count = 0;
    for rel in relations {
        let metadata =
            serde_json::to_string(&rel.metadata).expect("Value serialization is infallible");
        let result = sqlx::query(
            "INSERT OR IGNORE INTO relations (id, source_id, target_id, relation_type, weight, summary, metadata, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(rel.id.as_str())
        .bind(rel.source_id.as_str())
        .bind(rel.target_id.as_str())
        .bind(rel.relation_type.as_str())
        .bind(rel.weight)
        .bind(&rel.summary)
        .bind(&metadata)
        .bind(rel.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db_err) if db_err.message().contains("FOREIGN KEY constraint failed") => {
                FilamentError::Validation(format!(
                    "import error: relation '{}' references non-existent entity (source: {}, target: {})",
                    rel.id, rel.source_id, rel.target_id,
                ))
            }
            _ => FilamentError::from(e),
        })?;
        if result.rows_affected() > 0 {
            count += 1;
        }
    }
    Ok(count)
}

async fn import_messages(conn: &mut SqliteConnection, messages: &[Message]) -> Result<usize> {
    let mut count = 0;
    for msg in messages {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO messages (id, from_agent, to_agent, msg_type, body, status, in_reply_to, task_id, created_at, read_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(msg.id.as_str())
        .bind(msg.from_agent.as_str())
        .bind(msg.to_agent.as_str())
        .bind(msg.msg_type.as_str())
        .bind(msg.body.as_str())
        .bind(msg.status.as_str())
        .bind(&msg.in_reply_to)
        .bind(&msg.task_id)
        .bind(msg.created_at)
        .bind(msg.read_at)
        .execute(&mut *conn)
        .await?;
        if result.rows_affected() > 0 {
            count += 1;
        }
    }
    Ok(count)
}

async fn import_events(conn: &mut SqliteConnection, events: &[Event]) -> Result<usize> {
    let mut count = 0;
    for evt in events {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO events (id, entity_id, event_type, actor, diff, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(evt.id.as_str())
        .bind(evt.entity_id.as_ref().map(EntityId::as_str))
        .bind(evt.event_type.as_str())
        .bind(&evt.actor)
        .bind(&evt.diff)
        .bind(evt.created_at)
        .execute(&mut *conn)
        .await?;
        if result.rows_affected() > 0 {
            count += 1;
        }
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// Escalation queries
// ---------------------------------------------------------------------------

/// List pending escalations: unread blocker/question messages + blocked/`needs_input` agent runs.
///
/// # Errors
///
/// Returns `FilamentError::Database` on SQL failure.
pub async fn list_pending_escalations(pool: &Pool<Sqlite>) -> Result<Vec<Escalation>> {
    // Unread blocker/question messages
    let mut escalations = escalations_from_messages(pool).await?;

    // Blocked/needs_input agent runs
    let run_escalations = escalations_from_agent_runs(pool).await?;
    escalations.extend(run_escalations);

    // Sort by created_at
    escalations.sort_by_key(|e| e.created_at);

    Ok(escalations)
}

async fn escalations_from_messages(pool: &Pool<Sqlite>) -> Result<Vec<Escalation>> {
    #[derive(sqlx::FromRow)]
    struct MsgRow {
        from_agent: String,
        task_id: Option<String>,
        msg_type: String,
        body: String,
        created_at: chrono::DateTime<Utc>,
    }

    let rows = sqlx::query_as::<_, MsgRow>(
        "SELECT from_agent, task_id, msg_type, body, created_at \
         FROM messages \
         WHERE status = 'unread' AND msg_type IN ('blocker', 'question') \
         ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let kind = if r.msg_type == "question" {
                EscalationKind::Question
            } else {
                EscalationKind::Blocker
            };
            Escalation {
                kind,
                agent_name: r.from_agent,
                task_id: r.task_id,
                body: r.body,
                created_at: r.created_at,
            }
        })
        .collect())
}

async fn escalations_from_agent_runs(pool: &Pool<Sqlite>) -> Result<Vec<Escalation>> {
    #[derive(sqlx::FromRow)]
    struct RunRow {
        agent_role: String,
        task_id: String,
        status: String,
        started_at: chrono::DateTime<Utc>,
    }

    let rows = sqlx::query_as::<_, RunRow>(
        "SELECT agent_role, task_id, status, started_at \
         FROM agent_runs \
         WHERE status IN ('blocked', 'needs_input') \
         ORDER BY started_at ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let kind = if r.status == "blocked" {
                EscalationKind::Blocker
            } else {
                EscalationKind::NeedsInput
            };
            Escalation {
                kind,
                agent_name: r.agent_role,
                task_id: Some(r.task_id),
                body: format!("Agent run status: {}", r.status),
                created_at: r.started_at,
            }
        })
        .collect())
}
