use crate::error::CommandError;
use crate::models::{
    AuthorizeCredentialInput, CredentialVaultItem, RotateCredentialInput, UpsertCredentialInput,
};
use crate::services::credential_vault::CredentialVaultService;
use crate::state::AppState;

#[tauri::command]
pub fn list_credentials(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<CredentialVaultItem>, CommandError> {
    CredentialVaultService::list(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_credential(
    state: tauri::State<'_, AppState>,
    input: UpsertCredentialInput,
) -> Result<CredentialVaultItem, CommandError> {
    CredentialVaultService::upsert(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn authorize_credential(
    state: tauri::State<'_, AppState>,
    input: AuthorizeCredentialInput,
) -> Result<CredentialVaultItem, CommandError> {
    CredentialVaultService::authorize(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn rotate_credential(
    state: tauri::State<'_, AppState>,
    input: RotateCredentialInput,
) -> Result<CredentialVaultItem, CommandError> {
    CredentialVaultService::rotate(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_credential(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<(), CommandError> {
    CredentialVaultService::delete(&state.db, &key).map_err(|e| e.into())
}
