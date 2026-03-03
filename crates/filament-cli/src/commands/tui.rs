use filament_core::error::Result;

use super::helpers;

/// Launch the TUI interface.
pub async fn run() -> Result<()> {
    let conn = helpers::connect().await?;
    filament_tui::run_tui(conn).await
}
