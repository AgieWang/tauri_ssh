use crate::error::CommandError;
use crate::models::{
    SshConfigImportResult, SshServer, SshServerConnectionTestInput, SshServerTestResult,
    UpsertSshServerInput,
};
use crate::services::ssh_server::SshServerService;
use crate::state::AppState;

#[tauri::command]
pub fn list_ssh_servers(state: tauri::State<'_, AppState>) -> Result<Vec<SshServer>, CommandError> {
    SshServerService::list(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_ssh_server(
    state: tauri::State<'_, AppState>,
    input: UpsertSshServerInput,
) -> Result<SshServer, CommandError> {
    SshServerService::upsert(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_ssh_server(
    state: tauri::State<'_, AppState>,
    alias: String,
) -> Result<(), CommandError> {
    SshServerService::delete(&state.db, &alias).map_err(|e| e.into())
}

#[tauri::command]
pub fn import_ssh_config(
    state: tauri::State<'_, AppState>,
    path: Option<String>,
) -> Result<SshConfigImportResult, CommandError> {
    SshServerService::import_ssh_config(&state.db, path).map_err(|e| e.into())
}

#[tauri::command]
pub async fn test_ssh_server(
    state: tauri::State<'_, AppState>,
    alias: String,
) -> Result<SshServerTestResult, CommandError> {
    SshServerService::test(&state.db, &alias)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn test_ssh_server_connection(
    input: SshServerConnectionTestInput,
) -> Result<SshServerTestResult, CommandError> {
    SshServerService::test_connection(input)
        .await
        .map_err(|e| e.into())
}
