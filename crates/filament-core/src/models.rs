use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::FilamentError;

// ---------------------------------------------------------------------------
// Typed ID macro
// ---------------------------------------------------------------------------

/// Generate `as_str()`, `Display`, and `FromStr` for enums with `snake_case` string mapping.
macro_rules! impl_enum_str {
    ($name:ident { $($variant:ident => $str:literal),+ $(,)? }) => {
        impl $name {
            #[must_use]
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $str,)+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = FilamentError;
            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                match s {
                    $(s if s.eq_ignore_ascii_case($str) => Ok(Self::$variant),)+
                    _ => Err(FilamentError::Validation(format!(
                        "invalid {}: '{}'", stringify!($name), s
                    ))),
                }
            }
        }
    };
}

/// Generate sqlx `Decode` (with validation), `Encode`, and `Type` (with `compatible`)
/// for newtypes whose inner type matches the SQL type.
macro_rules! impl_sqlx_newtype {
    ($name:ident, $inner:ty) => {
        impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for $name {
            fn decode(
                value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
            ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
                let v = <$inner as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)?;
                Self::new(v).map_err(|e| e.to_string().into())
            }
        }

        impl sqlx::Encode<'_, sqlx::Sqlite> for $name {
            fn encode_by_ref(
                &self,
                args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
            ) -> std::result::Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <$inner as sqlx::Encode<'_, sqlx::Sqlite>>::encode_by_ref(&self.0, args)
            }
        }

        impl sqlx::Type<sqlx::Sqlite> for $name {
            fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
                <$inner as sqlx::Type<sqlx::Sqlite>>::type_info()
            }

            fn compatible(ty: &<sqlx::Sqlite as sqlx::Database>::TypeInfo) -> bool {
                <$inner as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
            }
        }
    };
}

/// Generate a newtype wrapper around `String` for type-safe IDs.
macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        pub struct $name(pub String);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(uuid_v7())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = std::convert::Infallible;
            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                Ok(Self(s.to_string()))
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        // sqlx decode/encode as TEXT
        impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for $name {
            fn decode(
                value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
            ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
                let s = <String as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)?;
                Ok(Self(s))
            }
        }

        impl sqlx::Encode<'_, sqlx::Sqlite> for $name {
            fn encode_by_ref(
                &self,
                args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
            ) -> std::result::Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <String as sqlx::Encode<'_, sqlx::Sqlite>>::encode_by_ref(&self.0, args)
            }
        }

        impl sqlx::Type<sqlx::Sqlite> for $name {
            fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
                <String as sqlx::Type<sqlx::Sqlite>>::type_info()
            }

            fn compatible(ty: &<sqlx::Sqlite as sqlx::Database>::TypeInfo) -> bool {
                <String as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }
    };
}

/// Generate a UUID v7 (time-ordered) as a string.
#[allow(clippy::cast_possible_truncation)]
fn uuid_v7() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch");
    let millis = now.as_millis() as u64; // safe: won't overflow for ~500k years

    let mut bytes = [0u8; 16];

    // Timestamp (48 bits) — truncation to u8 is intentional (extracting individual bytes)
    bytes[0] = (millis >> 40) as u8;
    bytes[1] = (millis >> 32) as u8;
    bytes[2] = (millis >> 24) as u8;
    bytes[3] = (millis >> 16) as u8;
    bytes[4] = (millis >> 8) as u8;
    bytes[5] = millis as u8;

    // Random bits for the rest
    let rand_bytes: [u8; 10] = std::array::from_fn(|_| fastrand_u8());
    bytes[6..16].copy_from_slice(&rand_bytes);

    // Version 7
    bytes[6] = (bytes[6] & 0x0F) | 0x70;
    // Variant 10xx
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// Simple random byte using thread-local state (no external dep).
#[allow(clippy::cast_possible_truncation)]
fn fastrand_u8() -> u8 {
    use std::cell::Cell;
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    thread_local! {
        static RNG: Cell<u64> = Cell::new(RandomState::new().build_hasher().finish());
    }
    RNG.with(|cell| {
        // xorshift64
        let mut s = cell.get();
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        cell.set(s);
        s as u8
    })
}

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------

typed_id!(EntityId);
typed_id!(RelationId);
typed_id!(MessageId);
typed_id!(ReservationId);
typed_id!(AgentRunId);
typed_id!(EventId);

// ---------------------------------------------------------------------------
// Value types
// ---------------------------------------------------------------------------

/// Task priority level (0 = highest, 4 = lowest). Invalid values rejected at construction.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "u8", into = "u8")]
pub struct Priority(u8);

impl Priority {
    pub const DEFAULT: Self = Self(2);

    /// # Errors
    ///
    /// Returns `FilamentError::Validation` if value > 4.
    pub fn new(value: u8) -> std::result::Result<Self, FilamentError> {
        if value > 4 {
            return Err(FilamentError::Validation(format!(
                "priority must be 0-4, got {value}"
            )));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<u8> for Priority {
    type Error = String;
    fn try_from(value: u8) -> std::result::Result<Self, String> {
        Self::new(value).map_err(|e| e.to_string())
    }
}

impl From<Priority> for u8 {
    fn from(p: Priority) -> Self {
        p.0
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for Priority {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
        let n = <i32 as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)?;
        let n = u8::try_from(n).map_err(|_| format!("priority out of u8 range: {n}"))?;
        Self::new(n).map_err(|e| e.to_string().into())
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for Priority {
    fn encode_by_ref(
        &self,
        args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
    ) -> std::result::Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let val = i32::from(self.0);
        <i32 as sqlx::Encode<'_, sqlx::Sqlite>>::encode_by_ref(&val, args)
    }
}

impl sqlx::Type<sqlx::Sqlite> for Priority {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <i32 as sqlx::Type<sqlx::Sqlite>>::type_info()
    }

    fn compatible(ty: &<sqlx::Sqlite as sqlx::Database>::TypeInfo) -> bool {
        <i32 as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

/// Relation weight. Must be non-negative and finite.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
pub struct Weight(f64);

impl Weight {
    pub const DEFAULT: Self = Self(1.0);

    /// # Errors
    ///
    /// Returns `FilamentError::Validation` if value is NaN, infinite, or negative.
    pub fn new(value: f64) -> std::result::Result<Self, FilamentError> {
        if !value.is_finite() || value < 0.0 {
            return Err(FilamentError::Validation(format!(
                "weight must be non-negative and finite, got {value}"
            )));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

impl std::fmt::Display for Weight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<f64> for Weight {
    type Error = String;
    fn try_from(value: f64) -> std::result::Result<Self, String> {
        Self::new(value).map_err(|e| e.to_string())
    }
}

impl From<Weight> for f64 {
    fn from(w: Weight) -> Self {
        w.0
    }
}

impl_sqlx_newtype!(Weight, f64);

/// Context budget as a fraction (0.0–1.0). Must be finite and within range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
pub struct BudgetPct(f64);

impl BudgetPct {
    /// # Errors
    ///
    /// Returns `FilamentError::Validation` if value is outside 0.0–1.0 or not finite.
    pub fn new(value: f64) -> std::result::Result<Self, FilamentError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(FilamentError::Validation(format!(
                "budget percentage must be 0.0-1.0, got {value}"
            )));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

impl std::fmt::Display for BudgetPct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.0}%", self.0 * 100.0)
    }
}

impl TryFrom<f64> for BudgetPct {
    type Error = String;
    fn try_from(value: f64) -> std::result::Result<Self, String> {
        Self::new(value).map_err(|e| e.to_string())
    }
}

impl From<BudgetPct> for f64 {
    fn from(b: BudgetPct) -> Self {
        b.0
    }
}

impl_sqlx_newtype!(BudgetPct, f64);

/// A string guaranteed to be non-empty (trimmed).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct NonEmptyString(String);

impl NonEmptyString {
    /// # Errors
    ///
    /// Returns `FilamentError::Validation` if the string is empty after trimming.
    pub fn new(s: impl Into<String>) -> std::result::Result<Self, FilamentError> {
        let s = s.into();
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
            return Err(FilamentError::Validation(
                "string cannot be empty".to_string(),
            ));
        }
        Ok(Self(trimmed))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NonEmptyString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<str> for NonEmptyString {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for NonEmptyString {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl TryFrom<String> for NonEmptyString {
    type Error = String;
    fn try_from(s: String) -> std::result::Result<Self, String> {
        Self::new(s).map_err(|e| e.to_string())
    }
}

impl From<NonEmptyString> for String {
    fn from(s: NonEmptyString) -> Self {
        s.0
    }
}

impl AsRef<str> for NonEmptyString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for NonEmptyString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl_sqlx_newtype!(NonEmptyString, String);

/// 8-character base36 slug for stable, human-typeable entity identifiers.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    /// Generate a new random slug (8 chars, `[a-z0-9]`).
    ///
    /// # Panics
    ///
    /// Panics if UTF-8 construction fails (impossible — all chars are ASCII).
    #[allow(clippy::cast_possible_truncation)]
    #[must_use]
    pub fn new() -> Self {
        let chars: Vec<u8> = (0..8)
            .map(|_| {
                let idx = fastrand_u8() % 36;
                if idx < 10 {
                    b'0' + idx
                } else {
                    b'a' + (idx - 10)
                }
            })
            .collect();
        Self(String::from_utf8(chars).expect("ASCII only"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Slug {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Slug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Slug {
    type Error = String;
    fn try_from(s: String) -> std::result::Result<Self, String> {
        let s = s.trim().to_string();
        if s.len() != 8
            || !s
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!("invalid slug: '{s}' (expected 8-char [a-z0-9])"));
        }
        Ok(Self(s))
    }
}

impl From<Slug> for String {
    fn from(s: Slug) -> Self {
        s.0
    }
}

impl std::str::FromStr for Slug {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for Slug {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
        let s = <String as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)?;
        Self::try_from(s).map_err(std::convert::Into::into)
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for Slug {
    fn encode_by_ref(
        &self,
        args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
    ) -> std::result::Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <String as sqlx::Encode<'_, sqlx::Sqlite>>::encode_by_ref(&self.0, args)
    }
}

impl sqlx::Type<sqlx::Sqlite> for Slug {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }

    fn compatible(ty: &<sqlx::Sqlite as sqlx::Database>::TypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

impl std::borrow::Borrow<str> for Slug {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Reservation TTL in seconds. Must be > 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtlSeconds(u32);

impl TtlSeconds {
    /// # Errors
    ///
    /// Returns `FilamentError::Validation` if value is 0.
    pub fn new(value: u32) -> std::result::Result<Self, FilamentError> {
        if value == 0 {
            return Err(FilamentError::Validation(
                "TTL must be greater than 0".to_string(),
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn as_duration(&self) -> chrono::Duration {
        chrono::Duration::seconds(i64::from(self.0))
    }
}

impl std::fmt::Display for TtlSeconds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}s", self.0)
    }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Task,
    Module,
    Service,
    Agent,
    Plan,
    Doc,
}

impl_enum_str!(EntityType {
    Task => "task",
    Module => "module",
    Service => "service",
    Agent => "agent",
    Plan => "plan",
    Doc => "doc",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    Blocks,
    DependsOn,
    Produces,
    Owns,
    RelatesTo,
    AssignedTo,
}

impl_enum_str!(RelationType {
    Blocks => "blocks",
    DependsOn => "depends_on",
    Produces => "produces",
    Owns => "owns",
    RelatesTo => "relates_to",
    AssignedTo => "assigned_to",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EntityStatus {
    Open,
    InProgress,
    Closed,
    Blocked,
}

impl_enum_str!(EntityStatus {
    Open => "open",
    InProgress => "in_progress",
    Closed => "closed",
    Blocked => "blocked",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Text,
    Question,
    Blocker,
    Artifact,
}

impl_enum_str!(MessageType {
    Text => "text",
    Question => "question",
    Blocker => "blocker",
    Artifact => "artifact",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Unread,
    Read,
}

impl_enum_str!(MessageStatus {
    Unread => "unread",
    Read => "read",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Completed,
    Blocked,
    Failed,
    NeedsInput,
}

impl_enum_str!(AgentStatus {
    Running => "running",
    Completed => "completed",
    Blocked => "blocked",
    Failed => "failed",
    NeedsInput => "needs_input",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    EntityCreated,
    EntityUpdated,
    EntityDeleted,
    StatusChange,
    RelationCreated,
    RelationDeleted,
    MessageSent,
    MessageRead,
    ReservationAcquired,
    ReservationReleased,
    AgentStarted,
    AgentFinished,
}

impl_enum_str!(EventType {
    EntityCreated => "entity_created",
    EntityUpdated => "entity_updated",
    EntityDeleted => "entity_deleted",
    StatusChange => "status_change",
    RelationCreated => "relation_created",
    RelationDeleted => "relation_deleted",
    MessageSent => "message_sent",
    MessageRead => "message_read",
    ReservationAcquired => "reservation_acquired",
    ReservationReleased => "reservation_released",
    AgentStarted => "agent_started",
    AgentFinished => "agent_finished",
});

// ---------------------------------------------------------------------------
// DB row structs
// ---------------------------------------------------------------------------

/// Shared fields for all entity types.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EntityCommon {
    pub id: EntityId,
    pub slug: Slug,
    pub name: NonEmptyString,
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content_path: Option<String>,
    pub content_hash: Option<String>,
    pub status: EntityStatus,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Domain entity — an algebraic data type with one variant per entity type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "entity_type", rename_all = "snake_case")]
pub enum Entity {
    Task(EntityCommon),
    Module(EntityCommon),
    Service(EntityCommon),
    Agent(EntityCommon),
    Plan(EntityCommon),
    Doc(EntityCommon),
}

impl Entity {
    /// Access the common fields shared by all entity types.
    #[must_use]
    pub const fn common(&self) -> &EntityCommon {
        match self {
            Self::Task(c)
            | Self::Module(c)
            | Self::Service(c)
            | Self::Agent(c)
            | Self::Plan(c)
            | Self::Doc(c) => c,
        }
    }

    /// Consume and return the common fields.
    #[must_use]
    pub fn into_common(self) -> EntityCommon {
        match self {
            Self::Task(c)
            | Self::Module(c)
            | Self::Service(c)
            | Self::Agent(c)
            | Self::Plan(c)
            | Self::Doc(c) => c,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &EntityId {
        &self.common().id
    }

    #[must_use]
    pub const fn slug(&self) -> &Slug {
        &self.common().slug
    }

    #[must_use]
    pub const fn name(&self) -> &NonEmptyString {
        &self.common().name
    }

    #[must_use]
    pub const fn entity_type(&self) -> EntityType {
        match self {
            Self::Task(_) => EntityType::Task,
            Self::Module(_) => EntityType::Module,
            Self::Service(_) => EntityType::Service,
            Self::Agent(_) => EntityType::Agent,
            Self::Plan(_) => EntityType::Plan,
            Self::Doc(_) => EntityType::Doc,
        }
    }

    #[must_use]
    pub const fn status(&self) -> &EntityStatus {
        &self.common().status
    }

    #[must_use]
    pub const fn priority(&self) -> Priority {
        self.common().priority
    }

    #[must_use]
    pub fn summary(&self) -> &str {
        &self.common().summary
    }

    /// Consume the entity, returning the inner `EntityCommon` if it is a Task.
    ///
    /// # Errors
    ///
    /// Returns `TypeMismatch` if the entity is not a task.
    pub fn into_task(self) -> Result<EntityCommon, FilamentError> {
        match self {
            Self::Task(c) => Ok(c),
            other => Err(FilamentError::TypeMismatch {
                expected: EntityType::Task,
                actual: other.entity_type(),
                slug: other.slug().clone(),
            }),
        }
    }

    /// Consume the entity, returning the inner `EntityCommon` if it is an Agent.
    ///
    /// # Errors
    ///
    /// Returns `TypeMismatch` if the entity is not an agent.
    pub fn into_agent(self) -> Result<EntityCommon, FilamentError> {
        match self {
            Self::Agent(c) => Ok(c),
            other => Err(FilamentError::TypeMismatch {
                expected: EntityType::Agent,
                actual: other.entity_type(),
                slug: other.slug().clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Relation {
    pub id: RelationId,
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub relation_type: RelationType,
    pub weight: Weight,
    pub summary: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub id: MessageId,
    pub from_agent: NonEmptyString,
    pub to_agent: NonEmptyString,
    pub msg_type: MessageType,
    pub body: NonEmptyString,
    pub status: MessageStatus,
    pub in_reply_to: Option<MessageId>,
    pub task_id: Option<EntityId>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Reservation {
    pub id: ReservationId,
    pub agent_name: String,
    pub file_glob: String,
    pub exclusive: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct AgentRun {
    pub id: AgentRunId,
    pub task_id: EntityId,
    pub agent_role: NonEmptyString,
    pub pid: Option<i32>,
    pub status: AgentStatus,
    pub result_json: Option<String>,
    pub context_budget_pct: Option<BudgetPct>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub id: EventId,
    pub entity_id: Option<EntityId>,
    pub event_type: EventType,
    pub actor: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Agent protocol (parsed from subprocess JSON output)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentResult {
    pub status: AgentStatus,
    pub task_id: Option<EntityId>,
    pub summary: String,
    pub artifacts: Vec<String>,
    pub messages: Vec<AgentMessage>,
    pub blockers: Vec<String>,
    pub questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentMessage {
    pub to_agent: NonEmptyString,
    pub body: NonEmptyString,
    pub msg_type: MessageType,
}

// ---------------------------------------------------------------------------
// Validated DTOs (boundary validation via TryFrom)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEntityRequest {
    pub name: String,
    pub entity_type: String,
    pub summary: Option<String>,
    pub key_facts: Option<serde_json::Value>,
    pub content_path: Option<String>,
    pub priority: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct ValidCreateEntityRequest {
    pub name: NonEmptyString,
    pub entity_type: EntityType,
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content_path: Option<String>,
    pub priority: Priority,
}

impl TryFrom<CreateEntityRequest> for ValidCreateEntityRequest {
    type Error = FilamentError;

    fn try_from(req: CreateEntityRequest) -> std::result::Result<Self, Self::Error> {
        let name = NonEmptyString::new(&req.name)
            .map_err(|_| FilamentError::Validation("name cannot be empty".to_string()))?;

        let entity_type = match req.entity_type.to_lowercase().as_str() {
            "task" => EntityType::Task,
            "module" => EntityType::Module,
            "service" => EntityType::Service,
            "agent" => EntityType::Agent,
            "plan" => EntityType::Plan,
            "doc" => EntityType::Doc,
            other => {
                return Err(FilamentError::Validation(format!(
                    "invalid entity type: '{other}' (expected: task, module, service, agent, plan, doc)"
                )));
            }
        };

        let priority = Priority::new(req.priority.unwrap_or(2))?;

        Ok(Self {
            name,
            entity_type,
            summary: req.summary.unwrap_or_default(),
            key_facts: req.key_facts.unwrap_or_else(|| serde_json::json!({})),
            content_path: req.content_path,
            priority,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelationRequest {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub weight: Option<f64>,
    pub summary: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ValidCreateRelationRequest {
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub relation_type: RelationType,
    pub weight: Weight,
    pub summary: String,
    pub metadata: serde_json::Value,
}

impl TryFrom<CreateRelationRequest> for ValidCreateRelationRequest {
    type Error = FilamentError;

    fn try_from(req: CreateRelationRequest) -> std::result::Result<Self, Self::Error> {
        let source_id = req.source_id.trim().to_string();
        let target_id = req.target_id.trim().to_string();

        if source_id.is_empty() {
            return Err(FilamentError::Validation(
                "source_id cannot be empty".to_string(),
            ));
        }
        if target_id.is_empty() {
            return Err(FilamentError::Validation(
                "target_id cannot be empty".to_string(),
            ));
        }
        if source_id == target_id {
            return Err(FilamentError::Validation(
                "source_id and target_id must differ".to_string(),
            ));
        }

        let relation_type = match req.relation_type.to_lowercase().as_str() {
            "blocks" => RelationType::Blocks,
            "depends_on" => RelationType::DependsOn,
            "produces" => RelationType::Produces,
            "owns" => RelationType::Owns,
            "relates_to" => RelationType::RelatesTo,
            "assigned_to" => RelationType::AssignedTo,
            other => {
                return Err(FilamentError::Validation(format!(
                    "invalid relation type: '{other}' (expected: blocks, depends_on, produces, owns, relates_to, assigned_to)"
                )));
            }
        };

        let weight = Weight::new(req.weight.unwrap_or(1.0))?;

        Ok(Self {
            source_id: EntityId::from(source_id),
            target_id: EntityId::from(target_id),
            relation_type,
            weight,
            summary: req.summary.unwrap_or_default(),
            metadata: req.metadata.unwrap_or_else(|| serde_json::json!({})),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub from_agent: String,
    pub to_agent: String,
    pub body: String,
    pub msg_type: Option<String>,
    pub in_reply_to: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidSendMessageRequest {
    pub from_agent: NonEmptyString,
    pub to_agent: NonEmptyString,
    pub body: NonEmptyString,
    pub msg_type: MessageType,
    pub in_reply_to: Option<MessageId>,
    pub task_id: Option<EntityId>,
}

impl TryFrom<SendMessageRequest> for ValidSendMessageRequest {
    type Error = FilamentError;

    fn try_from(req: SendMessageRequest) -> std::result::Result<Self, Self::Error> {
        let from_agent = NonEmptyString::new(&req.from_agent)
            .map_err(|_| FilamentError::Validation("from_agent cannot be empty".to_string()))?;
        let to_agent = NonEmptyString::new(&req.to_agent)
            .map_err(|_| FilamentError::Validation("to_agent cannot be empty".to_string()))?;
        let body = NonEmptyString::new(&req.body)
            .map_err(|_| FilamentError::Validation("message body cannot be empty".to_string()))?;

        let msg_type = match req.msg_type.as_deref().unwrap_or("text") {
            "text" => MessageType::Text,
            "question" => MessageType::Question,
            "blocker" => MessageType::Blocker,
            "artifact" => MessageType::Artifact,
            other => {
                return Err(FilamentError::Validation(format!(
                    "invalid message type: '{other}' (expected: text, question, blocker, artifact)"
                )));
            }
        };

        Ok(Self {
            from_agent,
            to_agent,
            body,
            msg_type,
            in_reply_to: req.in_reply_to.map(MessageId::from),
            task_id: req.task_id.map(EntityId::from),
        })
    }
}
