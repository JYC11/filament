pub mod agents;
pub mod analytics;
pub mod config;
pub mod detail;
pub mod entities;
pub mod filter_bar;
pub mod messages;
pub mod reservations;

/// Format a duration in seconds to a human-readable string (e.g., "42s", "3m05s", "1h30m").
/// Negative values are clamped to 0.
pub fn format_seconds(secs: i64) -> String {
    let secs = secs.max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m{s:02}s")
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{h}h{m:02}m")
    }
}
