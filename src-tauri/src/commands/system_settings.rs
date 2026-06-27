use crate::error::CommandError;
use crate::models::{SystemSettings, SystemSettingsExportResult, UpdateSystemSettingsInput};
use crate::services::system_settings::SystemSettingsService;
use crate::state::AppState;

#[tauri::command]
pub fn get_system_settings(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<SystemSettings, CommandError> {
    SystemSettingsService::get_with_autostart(&state.db, &app).map_err(|e| e.into())
}

#[tauri::command]
pub fn update_system_settings(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    input: UpdateSystemSettingsInput,
) -> Result<SystemSettings, CommandError> {
    SystemSettingsService::update_with_autostart(&state.db, &app, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn reset_system_settings(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<SystemSettings, CommandError> {
    SystemSettingsService::reset_with_autostart(&state.db, &app).map_err(|e| e.into())
}

#[tauri::command]
pub fn export_system_settings(
    state: tauri::State<'_, AppState>,
) -> Result<SystemSettingsExportResult, CommandError> {
    SystemSettingsService::export(&state.db).map_err(|e| e.into())
}
