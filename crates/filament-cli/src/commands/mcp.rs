use filament_core::error::Result;

use super::helpers;

/// Start the MCP stdio server.
pub async fn run() -> Result<()> {
    let root = helpers::find_project_root()?;
    let conn = filament_core::connection::FilamentConnection::auto_detect(&root).await?;
    filament_daemon::mcp::run_mcp_stdio(conn).await
}
