use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::FilamentError;

// ---------------------------------------------------------------------------
// sqlx newtype macro
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Typed ID macro
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// ID generation helpers
// ---------------------------------------------------------------------------

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
                // Rejection sampling to avoid modulo bias (256 % 36 = 4).
                // Accept values in 0..252 (largest multiple of 36 ≤ 256).
                let idx = loop {
                    let r = fastrand_u8();
                    if r < 252 {
                        break r % 36;
                    }
                };
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
