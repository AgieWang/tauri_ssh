use crate::error::CommandError;
use crate::models::{JumpServerOpenResult, JumpServerSession, UpsertJumpServerSessionInput};
use crate::services::jumpserver::JumpServerService;
use crate::state::AppState;

#[tauri::command]
pub fn list_jumpserver_sessions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<JumpServerSession>, CommandError> {
    JumpServerService::list(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_jumpserver_session(
    state: tauri::State<'_, AppState>,
    input: UpsertJumpServerSessionInput,
) -> Result<JumpServerSession, CommandError> {
    JumpServerService::upsert(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn open_jumpserver_session(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<JumpServerOpenResult, CommandError> {
    JumpServerService::open(&state.db, &key).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_jumpserver_session(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<(), CommandError> {
    JumpServerService::delete(&state.db, &key).map_err(|e| e.into())
}
