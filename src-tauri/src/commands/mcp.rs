use crate::error::CommandError;
use crate::models::{ConfigureMcpClientInput, ConfigureMcpClientResult, McpOverview};
use crate::services::mcp::McpService;

#[tauri::command]
pub fn get_mcp_overview() -> Result<McpOverview, CommandError> {
    McpService::overview().map_err(Into::into)
}

#[tauri::command]
pub fn configure_mcp_client(
    input: ConfigureMcpClientInput,
) -> Result<ConfigureMcpClientResult, CommandError> {
    McpService::configure(input).map_err(Into::into)
}
