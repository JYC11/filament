use std::path::Path;

use serde::Deserialize;

/// Project-level configuration loaded from `.filament/config.toml`.
///
/// All fields are optional — missing values fall back to defaults.
/// Environment variables (`FILAMENT_*`) override config file values.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FilamentConfig {
    /// Default priority for new entities (1–5, default 2).
    pub default_priority: Option<u8>,
    /// Default output format: "json" or "text" (default "text").
    pub output_format: Option<OutputFormat>,
    /// Command to run agents (default "claude").
    pub agent_command: Option<String>,
    /// Auto-dispatch unblocked tasks on completion (default false).
    pub auto_dispatch: Option<bool>,
    /// Graph context depth for agent prompts (default 2).
    pub context_depth: Option<usize>,
    /// Max auto-dispatched tasks per completion event (default 3).
    pub max_auto_dispatch: Option<usize>,
    /// Seconds between stale reservation cleanup sweeps (default 60).
    pub cleanup_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Text,
    Json,
}

impl FilamentConfig {
    /// Load config from `.filament/config.toml` under the given project root.
    /// Returns default config if file doesn't exist or can't be parsed.
    #[must_use]
    pub fn load(project_root: &Path) -> Self {
        let config_path = project_root.join(".filament").join("config.toml");
        std::fs::read_to_string(&config_path)
            .map_or_else(|_| Self::default(), |contents| {
                toml::from_str(&contents).unwrap_or_default()
            })
    }

    /// Whether JSON output is the default format.
    #[must_use]
    pub const fn json_output(&self) -> bool {
        matches!(self.output_format, Some(OutputFormat::Json))
    }

    /// Resolve agent command: env var > config > "claude".
    #[must_use]
    pub fn resolve_agent_command(&self) -> String {
        std::env::var("FILAMENT_AGENT_COMMAND")
            .ok()
            .or_else(|| self.agent_command.clone())
            .unwrap_or_else(|| "claude".to_string())
    }

    /// Resolve auto-dispatch: env var > config > false.
    #[must_use]
    pub fn resolve_auto_dispatch(&self) -> bool {
        if let Ok(v) = std::env::var("FILAMENT_AUTO_DISPATCH") {
            return v == "1" || v == "true";
        }
        self.auto_dispatch.unwrap_or(false)
    }

    /// Resolve context depth: env var > config > 2.
    #[must_use]
    pub fn resolve_context_depth(&self) -> usize {
        std::env::var("FILAMENT_CONTEXT_DEPTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(self.context_depth)
            .unwrap_or(2)
    }

    /// Resolve max auto-dispatch: env var > config > 3.
    #[must_use]
    pub fn resolve_max_auto_dispatch(&self) -> usize {
        std::env::var("FILAMENT_MAX_AUTO_DISPATCH")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(self.max_auto_dispatch)
            .unwrap_or(3)
    }

    /// Resolve cleanup interval: config > 60.
    #[must_use]
    pub fn resolve_cleanup_interval_secs(&self) -> u64 {
        self.cleanup_interval_secs.unwrap_or(60)
    }

    /// Resolve default priority: config > 2.
    #[must_use]
    pub fn resolve_default_priority(&self) -> u8 {
        self.default_priority.unwrap_or(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_defaults() {
        let cfg = FilamentConfig::default();
        assert_eq!(cfg.resolve_agent_command(), "claude");
        assert!(!cfg.resolve_auto_dispatch());
        assert_eq!(cfg.resolve_context_depth(), 2);
        assert_eq!(cfg.resolve_max_auto_dispatch(), 3);
        assert_eq!(cfg.resolve_cleanup_interval_secs(), 60);
        assert_eq!(cfg.resolve_default_priority(), 2);
        assert!(!cfg.json_output());
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
default_priority = 3
output_format = "json"
agent_command = "my-agent"
auto_dispatch = true
context_depth = 4
max_auto_dispatch = 5
cleanup_interval_secs = 120
"#;
        let cfg: FilamentConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.resolve_default_priority(), 3);
        assert!(cfg.json_output());
        assert_eq!(cfg.resolve_agent_command(), "my-agent");
        assert!(cfg.resolve_auto_dispatch());
        assert_eq!(cfg.resolve_context_depth(), 4);
        assert_eq!(cfg.resolve_max_auto_dispatch(), 5);
        assert_eq!(cfg.resolve_cleanup_interval_secs(), 120);
    }

    #[test]
    fn parse_partial_config() {
        let toml_str = r#"
default_priority = 4
"#;
        let cfg: FilamentConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.resolve_default_priority(), 4);
        assert_eq!(cfg.resolve_agent_command(), "claude");
        assert!(!cfg.resolve_auto_dispatch());
    }

    #[test]
    fn load_missing_file_returns_default() {
        let cfg = FilamentConfig::load(std::path::Path::new("/nonexistent/path"));
        assert_eq!(cfg.resolve_default_priority(), 2);
    }
}
