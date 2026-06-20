use tauri::State;

use crate::error::CommandError;
use crate::models::{
    SftpCreateDirectoryInput, SftpCreateFileInput, SftpDeleteInput, SftpListInput, SftpListResult,
    SftpOperationResult, SftpReadTextInput, SftpReadTextResult, SftpRenameInput,
    SftpTransferPathInput, SftpWriteTextInput,
};
use crate::services::sftp::SftpService;
use crate::state::AppState;

#[tauri::command]
pub fn sftp_list(
    state: State<'_, AppState>,
    input: SftpListInput,
) -> Result<SftpListResult, CommandError> {
    SftpService::list(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn sftp_read_text(
    state: State<'_, AppState>,
    input: SftpReadTextInput,
) -> Result<SftpReadTextResult, CommandError> {
    SftpService::read_text(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn sftp_write_text(
    state: State<'_, AppState>,
    input: SftpWriteTextInput,
) -> Result<SftpOperationResult, CommandError> {
    SftpService::write_text(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn sftp_upload(
    state: State<'_, AppState>,
    input: SftpTransferPathInput,
) -> Result<SftpOperationResult, CommandError> {
    SftpService::upload(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn sftp_download(
    state: State<'_, AppState>,
    input: SftpTransferPathInput,
) -> Result<SftpOperationResult, CommandError> {
    SftpService::download(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn sftp_create_directory(
    state: State<'_, AppState>,
    input: SftpCreateDirectoryInput,
) -> Result<SftpOperationResult, CommandError> {
    SftpService::create_directory(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn sftp_create_file(
    state: State<'_, AppState>,
    input: SftpCreateFileInput,
) -> Result<SftpOperationResult, CommandError> {
    SftpService::create_file(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn sftp_rename(
    state: State<'_, AppState>,
    input: SftpRenameInput,
) -> Result<SftpOperationResult, CommandError> {
    SftpService::rename(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn sftp_delete(
    state: State<'_, AppState>,
    input: SftpDeleteInput,
) -> Result<SftpOperationResult, CommandError> {
    SftpService::delete(&state.db, input).map_err(Into::into)
}
