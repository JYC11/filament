use filament_core::error::Result;

use super::helpers;

/// Start the MCP stdio server.
/// Reads `FILAMENT_AGENT_ROLE` env var to enforce per-role tool filtering.
pub async fn run() -> Result<()> {
    let root = helpers::find_project_root()?;
    let conn = filament_core::connection::FilamentConnection::auto_detect(&root).await?;

    // Read role from env var (set by dispatch via MCP config)
    let allowed_tools = std::env::var("FILAMENT_AGENT_ROLE")
        .ok()
        .and_then(|role_str| role_str.parse::<filament_daemon::roles::AgentRole>().ok())
        .map(filament_daemon::roles::allowed_tools);

    filament_daemon::mcp::run_mcp_stdio(conn, allowed_tools).await
}
