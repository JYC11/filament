use std::path::Path;

use serde::Deserialize;

/// Project-level configuration loaded from `.fl/config.toml`.
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
    /// Idle timeout in seconds before daemon auto-shuts down (default 1800 = 30 min, 0 = never).
    pub idle_timeout_secs: Option<u64>,
    /// Seconds between dead-agent reconciliation sweeps (default 30).
    pub reconciliation_interval_secs: Option<u64>,
    /// Max seconds an agent subprocess may run before being killed (default 3600 = 1h, 0 = no limit).
    pub agent_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Text,
    Json,
}

impl FilamentConfig {
    /// Load config from `.fl/config.toml` under the given project root.
    /// Returns default config if file doesn't exist or can't be parsed.
    #[must_use]
    pub fn load(project_root: &Path) -> Self {
        let config_path = project_root.join(".fl").join("config.toml");
        std::fs::read_to_string(&config_path).map_or_else(
            |_| Self::default(),
            |contents| toml::from_str(&contents).unwrap_or_default(),
        )
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

    /// Resolve cleanup interval: env var > config > 60.
    #[must_use]
    pub fn resolve_cleanup_interval_secs(&self) -> u64 {
        std::env::var("FILAMENT_CLEANUP_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(self.cleanup_interval_secs)
            .unwrap_or(60)
    }

    /// Resolve default priority: config > 2. Clamps to valid range 0-4.
    #[must_use]
    pub fn resolve_default_priority(&self) -> u8 {
        self.default_priority.unwrap_or(2).min(4)
    }

    /// Resolve idle timeout: env var > config > 1800 (30 min). 0 means never.
    #[must_use]
    pub fn resolve_idle_timeout_secs(&self) -> u64 {
        std::env::var("FILAMENT_IDLE_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(self.idle_timeout_secs)
            .unwrap_or(1800)
    }

    /// Resolve reconciliation interval: env var > config > 30.
    #[must_use]
    pub fn resolve_reconciliation_interval_secs(&self) -> u64 {
        std::env::var("FILAMENT_RECONCILIATION_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(self.reconciliation_interval_secs)
            .unwrap_or(30)
    }

    /// Resolve agent timeout: env var > config > 3600 (1 hour). 0 means no limit.
    #[must_use]
    pub fn resolve_agent_timeout_secs(&self) -> u64 {
        std::env::var("FILAMENT_AGENT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(self.agent_timeout_secs)
            .unwrap_or(3600)
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    #[test]
    #[serial]
    fn default_config_has_sensible_defaults() {
        let cfg = FilamentConfig::default();
        assert_eq!(cfg.resolve_agent_command(), "claude");
        assert!(!cfg.resolve_auto_dispatch());
        assert_eq!(cfg.resolve_context_depth(), 2);
        assert_eq!(cfg.resolve_max_auto_dispatch(), 3);
        assert_eq!(cfg.resolve_cleanup_interval_secs(), 60);
        assert_eq!(cfg.resolve_idle_timeout_secs(), 1800);
        assert_eq!(cfg.resolve_reconciliation_interval_secs(), 30);
        assert_eq!(cfg.resolve_agent_timeout_secs(), 3600);
        assert_eq!(cfg.resolve_default_priority(), 2);
        assert!(!cfg.json_output());
    }

    #[test]
    #[serial]
    fn parse_full_config() {
        let toml_str = r#"
default_priority = 3
output_format = "json"
agent_command = "my-agent"
auto_dispatch = true
context_depth = 4
max_auto_dispatch = 5
cleanup_interval_secs = 120
idle_timeout_secs = 600
reconciliation_interval_secs = 15
agent_timeout_secs = 7200
"#;
        let cfg: FilamentConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.resolve_default_priority(), 3);
        assert!(cfg.json_output());
        assert_eq!(cfg.resolve_agent_command(), "my-agent");
        assert!(cfg.resolve_auto_dispatch());
        assert_eq!(cfg.resolve_context_depth(), 4);
        assert_eq!(cfg.resolve_max_auto_dispatch(), 5);
        assert_eq!(cfg.resolve_cleanup_interval_secs(), 120);
        assert_eq!(cfg.resolve_idle_timeout_secs(), 600);
        assert_eq!(cfg.resolve_reconciliation_interval_secs(), 15);
        assert_eq!(cfg.resolve_agent_timeout_secs(), 7200);
    }

    #[test]
    #[serial]
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

    #[test]
    fn out_of_range_priority_clamped_to_max() {
        let toml_str = "default_priority = 255\n";
        let cfg: FilamentConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.resolve_default_priority(), 4);
    }

    #[test]
    #[serial]
    fn env_var_overrides_config_value() {
        std::env::set_var("FILAMENT_CLEANUP_INTERVAL", "30");
        let cfg = FilamentConfig {
            cleanup_interval_secs: Some(120),
            ..Default::default()
        };
        let result = cfg.resolve_cleanup_interval_secs();
        std::env::remove_var("FILAMENT_CLEANUP_INTERVAL");
        assert_eq!(result, 30);
    }

    #[test]
    #[serial]
    fn env_var_overrides_context_depth() {
        std::env::set_var("FILAMENT_CONTEXT_DEPTH", "5");
        let cfg = FilamentConfig {
            context_depth: Some(10),
            ..Default::default()
        };
        let result = cfg.resolve_context_depth();
        std::env::remove_var("FILAMENT_CONTEXT_DEPTH");
        assert_eq!(result, 5);
    }

    #[test]
    #[serial]
    fn env_var_auto_dispatch_true_string() {
        // Test "true" string
        std::env::set_var("FILAMENT_AUTO_DISPATCH", "true");
        let cfg = FilamentConfig::default();
        let result_true = cfg.resolve_auto_dispatch();
        std::env::remove_var("FILAMENT_AUTO_DISPATCH");
        assert!(result_true);

        // Test "1" string
        std::env::set_var("FILAMENT_AUTO_DISPATCH", "1");
        let result_one = cfg.resolve_auto_dispatch();
        std::env::remove_var("FILAMENT_AUTO_DISPATCH");
        assert!(result_one);
    }

    #[test]
    #[serial]
    fn env_var_overrides_reconciliation_interval() {
        std::env::set_var("FILAMENT_RECONCILIATION_INTERVAL", "10");
        let cfg = FilamentConfig {
            reconciliation_interval_secs: Some(60),
            ..Default::default()
        };
        let result = cfg.resolve_reconciliation_interval_secs();
        std::env::remove_var("FILAMENT_RECONCILIATION_INTERVAL");
        assert_eq!(result, 10);
    }

    #[test]
    #[serial]
    fn env_var_overrides_agent_timeout() {
        std::env::set_var("FILAMENT_AGENT_TIMEOUT", "120");
        let cfg = FilamentConfig {
            agent_timeout_secs: Some(7200),
            ..Default::default()
        };
        let result = cfg.resolve_agent_timeout_secs();
        std::env::remove_var("FILAMENT_AGENT_TIMEOUT");
        assert_eq!(result, 120);
    }

    #[test]
    #[serial]
    fn agent_timeout_zero_means_no_limit() {
        let cfg = FilamentConfig {
            agent_timeout_secs: Some(0),
            ..Default::default()
        };
        assert_eq!(cfg.resolve_agent_timeout_secs(), 0);
    }

    #[test]
    #[serial]
    fn env_var_invalid_value_falls_back() {
        std::env::set_var("FILAMENT_MAX_AUTO_DISPATCH", "not_a_number");
        let cfg = FilamentConfig {
            max_auto_dispatch: Some(7),
            ..Default::default()
        };
        let result = cfg.resolve_max_auto_dispatch();
        std::env::remove_var("FILAMENT_MAX_AUTO_DISPATCH");
        // Invalid env var can't parse, so falls back to config value
        assert_eq!(result, 7);
    }
}
