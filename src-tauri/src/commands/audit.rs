use crate::error::CommandError;
use crate::models::{AuditLog, AuditLogExportResult, CreateAuditLogInput, ListAuditLogsInput};
use crate::services::audit::AuditService;
use crate::state::AppState;

#[tauri::command]
pub fn list_audit_logs(
    state: tauri::State<'_, AppState>,
    input: ListAuditLogsInput,
) -> Result<Vec<AuditLog>, CommandError> {
    AuditService::list(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn create_audit_log(
    state: tauri::State<'_, AppState>,
    input: CreateAuditLogInput,
) -> Result<AuditLog, CommandError> {
    AuditService::create(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn export_audit_logs(
    state: tauri::State<'_, AppState>,
    input: ListAuditLogsInput,
) -> Result<AuditLogExportResult, CommandError> {
    AuditService::export(&state.db, input).map_err(|e| e.into())
}
