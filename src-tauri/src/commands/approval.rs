use crate::error::CommandError;
use crate::models::{
    ApprovalRequest, CreateApprovalRequestInput, DecideApprovalRequestInput,
    ListApprovalRequestsInput,
};
use crate::services::approval::ApprovalService;
use crate::state::AppState;

#[tauri::command]
pub fn list_approval_requests(
    state: tauri::State<'_, AppState>,
    input: ListApprovalRequestsInput,
) -> Result<Vec<ApprovalRequest>, CommandError> {
    ApprovalService::list(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn create_approval_request(
    state: tauri::State<'_, AppState>,
    input: CreateApprovalRequestInput,
) -> Result<ApprovalRequest, CommandError> {
    ApprovalService::create(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn decide_approval_request(
    state: tauri::State<'_, AppState>,
    input: DecideApprovalRequestInput,
) -> Result<ApprovalRequest, CommandError> {
    ApprovalService::decide(&state.db, input).map_err(|e| e.into())
}
