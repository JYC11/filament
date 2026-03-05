use std::collections::HashSet;

use serde_json::{json, Value};

/// JSON diff value — always a `serde_json::Value`.
pub type EventDiff = Value;

/// Builder for constructing structured JSON diffs from entity updates.
///
/// Two modes:
/// - **Update** (`DiffBuilder::new()`): records `{ field: { "old": x, "new": y } }` pairs
/// - **Create** (`DiffBuilder::create()`): records `{ field: value }` flat entries
pub struct DiffBuilder {
    inner: Value,
    mode: DiffMode,
}

#[derive(Debug, PartialEq, Eq)]
enum DiffMode {
    Update,
    Create,
}

impl DiffBuilder {
    /// Start building an update diff (old/new pairs).
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: json!({}),
            mode: DiffMode::Update,
        }
    }

    /// Start building a create diff (flat key-value).
    #[must_use]
    pub fn create() -> Self {
        Self {
            inner: json!({}),
            mode: DiffMode::Create,
        }
    }

    /// Record a changed field with old and new values (update mode).
    /// Only records the field if old != new.
    #[must_use]
    pub fn field(mut self, name: &str, old: &str, new: &str) -> Self {
        debug_assert_eq!(self.mode, DiffMode::Update, "use .value() for create diffs");
        if old != new {
            self.inner[name] = json!({ "old": old, "new": new });
        }
        self
    }

    /// Record a value for a created entity (create mode).
    #[must_use]
    pub fn value(mut self, name: &str, val: &str) -> Self {
        debug_assert_eq!(self.mode, DiffMode::Create, "use .field() for update diffs");
        if !val.is_empty() {
            self.inner[name] = json!(val);
        }
        self
    }

    /// Consume the builder and return the JSON diff.
    /// Returns `None` if no fields were recorded.
    #[must_use]
    pub fn build(self) -> Option<EventDiff> {
        if self.inner.as_object().is_none_or(serde_json::Map::is_empty) {
            None
        } else {
            Some(self.inner)
        }
    }
}

impl Default for DiffBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the set of field names from a JSON diff.
///
/// Works for both update diffs (`{ "field": { "old": ..., "new": ... } }`)
/// and create diffs (`{ "field": "value" }`).
#[must_use]
pub fn fields_in_diff(diff: &Value) -> HashSet<String> {
    diff.as_object()
        .map_or_else(HashSet::new, |map| map.keys().cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_diff_records_changed_fields() {
        let diff = DiffBuilder::new()
            .field("summary", "old summary", "new summary")
            .field("status", "open", "in_progress")
            .build()
            .unwrap();

        assert_eq!(diff["summary"]["old"], "old summary");
        assert_eq!(diff["summary"]["new"], "new summary");
        assert_eq!(diff["status"]["old"], "open");
        assert_eq!(diff["status"]["new"], "in_progress");
    }

    #[test]
    fn update_diff_skips_unchanged_fields() {
        let diff = DiffBuilder::new()
            .field("summary", "same", "same")
            .field("status", "open", "closed")
            .build()
            .unwrap();

        assert!(diff.get("summary").is_none());
        assert!(diff.get("status").is_some());
    }

    #[test]
    fn update_diff_returns_none_when_empty() {
        let diff = DiffBuilder::new()
            .field("summary", "same", "same")
            .build();

        assert!(diff.is_none());
    }

    #[test]
    fn create_diff_records_values() {
        let diff = DiffBuilder::create()
            .value("name", "my-task")
            .value("summary", "a summary")
            .build()
            .unwrap();

        assert_eq!(diff["name"], "my-task");
        assert_eq!(diff["summary"], "a summary");
    }

    #[test]
    fn create_diff_skips_empty_values() {
        let diff = DiffBuilder::create()
            .value("name", "my-task")
            .value("summary", "")
            .build()
            .unwrap();

        assert!(diff.get("summary").is_none());
    }

    #[test]
    fn fields_in_diff_extracts_keys() {
        let diff = json!({
            "summary": { "old": "x", "new": "y" },
            "status": { "old": "a", "new": "b" },
        });

        let fields = fields_in_diff(&diff);
        assert_eq!(fields.len(), 2);
        assert!(fields.contains("summary"));
        assert!(fields.contains("status"));
    }

    #[test]
    fn fields_in_diff_handles_null() {
        let fields = fields_in_diff(&Value::Null);
        assert!(fields.is_empty());
    }
}
