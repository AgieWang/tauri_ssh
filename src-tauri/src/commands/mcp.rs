use crate::error::CommandError;
use crate::models::{ConfigureMcpClientInput, ConfigureMcpClientResult, McpOverview};
use crate::services::mcp::McpService;
use crate::services::system_settings::SystemSettingsService;
use crate::state::AppState;

#[tauri::command]
pub fn get_mcp_overview(state: tauri::State<'_, AppState>) -> Result<McpOverview, CommandError> {
    let enabled = SystemSettingsService::is_mcp_enabled(&state.db)?;
    McpService::overview_with_enabled(enabled).map_err(Into::into)
}

#[tauri::command]
pub fn configure_mcp_client(
    input: ConfigureMcpClientInput,
) -> Result<ConfigureMcpClientResult, CommandError> {
    McpService::configure(input).map_err(Into::into)
}
