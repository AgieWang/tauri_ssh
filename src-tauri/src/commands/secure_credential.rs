use crate::error::CommandError;
use crate::models::{
    CreateSecureCredentialSessionInput, ListSecureCredentialSessionsInput,
    ListSecureCredentialAuditLogsInput, ListSecureCredentialsInput, RotateSecureCredentialInput,
    SecureCredential, SecureCredentialAuditLog, SecureCredentialGitReadInput,
    SecureCredentialGitWriteInput, SecureCredentialGitWriteResult,
    SecureCredentialHttpRequestInput, SecureCredentialHttpRequestResult,
    SecureCredentialHttpWriteInput, SecureCredentialOverview, SecureCredentialPolicySettings,
    SecureCredentialProviderReadResult, SecureCredentialProviderTestInput,
    SecureCredentialProviderTestResult,
    SecureCredentialRepository, SecureCredentialRepositoryListInput, SecureCredentialSession,
    SecureCredentialSessionStatus, SetSecureCredentialEnabledInput,
    UpdateSecureCredentialPolicySettingsInput, UpsertSecureCredentialInput,
};
use crate::services::secure_credential::SecureCredentialService;
use crate::state::AppState;

#[tauri::command]
pub fn get_secure_credential_overview(
    state: tauri::State<'_, AppState>,
) -> Result<SecureCredentialOverview, CommandError> {
    SecureCredentialService::overview(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_secure_credential_audit_logs(
    state: tauri::State<'_, AppState>,
    input: Option<ListSecureCredentialAuditLogsInput>,
) -> Result<Vec<SecureCredentialAuditLog>, CommandError> {
    SecureCredentialService::list_audit_logs(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn get_secure_credential_policy_settings(
    state: tauri::State<'_, AppState>,
) -> Result<SecureCredentialPolicySettings, CommandError> {
    SecureCredentialService::policy_settings(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn update_secure_credential_policy_settings(
    state: tauri::State<'_, AppState>,
    input: UpdateSecureCredentialPolicySettingsInput,
) -> Result<SecureCredentialPolicySettings, CommandError> {
    SecureCredentialService::update_policy_settings(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_secure_credentials(
    state: tauri::State<'_, AppState>,
    input: Option<ListSecureCredentialsInput>,
) -> Result<Vec<SecureCredential>, CommandError> {
    SecureCredentialService::list(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_secure_credential(
    state: tauri::State<'_, AppState>,
    input: UpsertSecureCredentialInput,
) -> Result<SecureCredential, CommandError> {
    SecureCredentialService::upsert(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn rotate_secure_credential(
    state: tauri::State<'_, AppState>,
    input: RotateSecureCredentialInput,
) -> Result<SecureCredential, CommandError> {
    SecureCredentialService::rotate(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn set_secure_credential_enabled(
    state: tauri::State<'_, AppState>,
    input: SetSecureCredentialEnabledInput,
) -> Result<SecureCredential, CommandError> {
    SecureCredentialService::set_enabled(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_secure_credential(
    state: tauri::State<'_, AppState>,
    credential_key: String,
) -> Result<(), CommandError> {
    SecureCredentialService::delete(&state.db, &credential_key).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_secure_credential_sessions(
    state: tauri::State<'_, AppState>,
    input: Option<ListSecureCredentialSessionsInput>,
) -> Result<Vec<SecureCredentialSession>, CommandError> {
    SecureCredentialService::list_sessions(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn create_secure_credential_session(
    state: tauri::State<'_, AppState>,
    input: CreateSecureCredentialSessionInput,
) -> Result<SecureCredentialSession, CommandError> {
    SecureCredentialService::create_session(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn get_secure_credential_session_status(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<SecureCredentialSessionStatus, CommandError> {
    SecureCredentialService::session_status(&state.db, &session_id).map_err(|e| e.into())
}

#[tauri::command]
pub fn revoke_secure_credential_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<SecureCredentialSession, CommandError> {
    SecureCredentialService::revoke_session(&state.db, &session_id).map_err(|e| e.into())
}

#[tauri::command]
pub async fn test_secure_credential_provider(
    state: tauri::State<'_, AppState>,
    input: SecureCredentialProviderTestInput,
) -> Result<SecureCredentialProviderTestResult, CommandError> {
    SecureCredentialService::test_provider(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_secure_credential_repositories(
    state: tauri::State<'_, AppState>,
    input: SecureCredentialRepositoryListInput,
) -> Result<Vec<SecureCredentialRepository>, CommandError> {
    SecureCredentialService::list_repositories(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn secure_credential_git_readonly_request(
    state: tauri::State<'_, AppState>,
    input: SecureCredentialGitReadInput,
) -> Result<SecureCredentialProviderReadResult, CommandError> {
    SecureCredentialService::git_readonly_request(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn secure_credential_http_readonly_request(
    state: tauri::State<'_, AppState>,
    input: SecureCredentialHttpRequestInput,
) -> Result<SecureCredentialHttpRequestResult, CommandError> {
    SecureCredentialService::http_readonly_request(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn secure_credential_http_write_request(
    state: tauri::State<'_, AppState>,
    input: SecureCredentialHttpWriteInput,
) -> Result<SecureCredentialHttpRequestResult, CommandError> {
    SecureCredentialService::http_write_request(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn execute_secure_credential_git_write(
    state: tauri::State<'_, AppState>,
    input: SecureCredentialGitWriteInput,
) -> Result<SecureCredentialGitWriteResult, CommandError> {
    SecureCredentialService::execute_git_write(&state.db, input)
        .await
        .map_err(|e| e.into())
}
