use crate::error::CommandError;
use crate::models::{SystemSettings, SystemSettingsExportResult, UpdateSystemSettingsInput};
use crate::services::system_settings::SystemSettingsService;
use crate::state::AppState;

#[tauri::command]
pub fn get_system_settings(
    state: tauri::State<'_, AppState>,
) -> Result<SystemSettings, CommandError> {
    SystemSettingsService::get(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn update_system_settings(
    state: tauri::State<'_, AppState>,
    input: UpdateSystemSettingsInput,
) -> Result<SystemSettings, CommandError> {
    SystemSettingsService::update(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn reset_system_settings(
    state: tauri::State<'_, AppState>,
) -> Result<SystemSettings, CommandError> {
    SystemSettingsService::reset(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn export_system_settings(
    state: tauri::State<'_, AppState>,
) -> Result<SystemSettingsExportResult, CommandError> {
    SystemSettingsService::export(&state.db).map_err(|e| e.into())
}
