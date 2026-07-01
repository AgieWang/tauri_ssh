use crate::error::CommandError;
use crate::models::{
    AiCommitGitWorkspaceInput, AiCommitGitWorkspaceResult, GitWorkspace, GitWorkspaceBranch,
    GitWorkspaceDetail, GitWorkspaceScanJobStatus, GitWorkspaceScanStartResult,
    ListGitWorkspacesInput, ScanGitWorkspaceRootInput, ScanGitWorkspaceRootResult,
    SwitchGitWorkspaceBranchInput, UpsertGitWorkspaceInput,
};
use crate::services::git_workspace::GitWorkspaceService;
use crate::state::AppState;

#[tauri::command]
pub fn list_git_workspaces(
    state: tauri::State<'_, AppState>,
    input: Option<ListGitWorkspacesInput>,
) -> Result<Vec<GitWorkspace>, CommandError> {
    GitWorkspaceService::list(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn upsert_git_workspace(
    state: tauri::State<'_, AppState>,
    input: UpsertGitWorkspaceInput,
) -> Result<GitWorkspace, CommandError> {
    GitWorkspaceService::upsert(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_git_workspace(
    state: tauri::State<'_, AppState>,
    workspace_key: String,
) -> Result<(), CommandError> {
    GitWorkspaceService::delete(&state.db, &workspace_key).map_err(|e| e.into())
}

#[tauri::command]
pub async fn refresh_git_workspace(
    state: tauri::State<'_, AppState>,
    workspace_key: String,
) -> Result<GitWorkspace, CommandError> {
    GitWorkspaceService::refresh(&state.db, &workspace_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn get_git_workspace_detail(
    state: tauri::State<'_, AppState>,
    workspace_key: String,
) -> Result<GitWorkspaceDetail, CommandError> {
    GitWorkspaceService::detail(&state.db, &workspace_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn scan_git_workspace_root(
    state: tauri::State<'_, AppState>,
    input: ScanGitWorkspaceRootInput,
) -> Result<ScanGitWorkspaceRootResult, CommandError> {
    GitWorkspaceService::scan_root(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn start_git_workspace_root_scan(
    app: tauri::AppHandle,
    input: ScanGitWorkspaceRootInput,
) -> Result<GitWorkspaceScanStartResult, CommandError> {
    GitWorkspaceService::start_scan_root(app, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn get_git_workspace_scan_status(
    job_id: String,
) -> Result<GitWorkspaceScanJobStatus, CommandError> {
    GitWorkspaceService::get_scan_status(&job_id).map_err(|e| e.into())
}

#[tauri::command]
pub async fn ai_commit_git_workspace(
    state: tauri::State<'_, AppState>,
    input: AiCommitGitWorkspaceInput,
) -> Result<AiCommitGitWorkspaceResult, CommandError> {
    GitWorkspaceService::ai_commit(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn pull_git_workspace(
    state: tauri::State<'_, AppState>,
    workspace_key: String,
) -> Result<GitWorkspace, CommandError> {
    GitWorkspaceService::pull(&state.db, &workspace_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_git_workspace_branches(
    state: tauri::State<'_, AppState>,
    workspace_key: String,
) -> Result<Vec<GitWorkspaceBranch>, CommandError> {
    GitWorkspaceService::branches(&state.db, &workspace_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn switch_git_workspace_branch(
    state: tauri::State<'_, AppState>,
    input: SwitchGitWorkspaceBranchInput,
) -> Result<GitWorkspace, CommandError> {
    GitWorkspaceService::switch_branch(&state.db, input)
        .await
        .map_err(|e| e.into())
}
