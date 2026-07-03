use crate::error::CommandError;
use crate::models::{
    CodeReviewBatchParseResult, CodeReviewTask, CreateCodeReviewBatchTasksInput,
    CreateCodeReviewTaskInput, ListCodeReviewTasksInput, ParseCodeReviewBatchInput,
    RunCodeReviewAiInput,
};
use crate::services::code_review::CodeReviewService;
use crate::state::AppState;

#[tauri::command]
pub fn list_code_review_tasks(
    state: tauri::State<'_, AppState>,
    input: Option<ListCodeReviewTasksInput>,
) -> Result<Vec<CodeReviewTask>, CommandError> {
    CodeReviewService::list(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn get_code_review_task(
    state: tauri::State<'_, AppState>,
    task_key: String,
) -> Result<CodeReviewTask, CommandError> {
    CodeReviewService::get(&state.db, &task_key).map_err(|e| e.into())
}

#[tauri::command]
pub fn create_code_review_task(
    state: tauri::State<'_, AppState>,
    input: CreateCodeReviewTaskInput,
) -> Result<CodeReviewTask, CommandError> {
    CodeReviewService::create(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn create_code_review_batch_tasks(
    state: tauri::State<'_, AppState>,
    input: CreateCodeReviewBatchTasksInput,
) -> Result<Vec<CodeReviewTask>, CommandError> {
    CodeReviewService::create_batch_tasks(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn prepare_code_review_diff(
    state: tauri::State<'_, AppState>,
    task_key: String,
) -> Result<CodeReviewTask, CommandError> {
    CodeReviewService::prepare_diff(&state.db, &task_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn run_code_review_ai(
    state: tauri::State<'_, AppState>,
    input: RunCodeReviewAiInput,
) -> Result<CodeReviewTask, CommandError> {
    CodeReviewService::run_ai(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn merge_code_review_task(
    state: tauri::State<'_, AppState>,
    task_key: String,
) -> Result<CodeReviewTask, CommandError> {
    CodeReviewService::merge(&state.db, &task_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn push_code_review_task(
    state: tauri::State<'_, AppState>,
    task_key: String,
) -> Result<CodeReviewTask, CommandError> {
    CodeReviewService::push(&state.db, &task_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn abort_code_review_merge(
    state: tauri::State<'_, AppState>,
    task_key: String,
) -> Result<CodeReviewTask, CommandError> {
    CodeReviewService::abort_merge(&state.db, &task_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn cancel_code_review_task(
    state: tauri::State<'_, AppState>,
    task_key: String,
) -> Result<CodeReviewTask, CommandError> {
    CodeReviewService::cancel(&state.db, &task_key).map_err(|e| e.into())
}

#[tauri::command]
pub async fn parse_code_review_batch(
    state: tauri::State<'_, AppState>,
    input: ParseCodeReviewBatchInput,
) -> Result<CodeReviewBatchParseResult, CommandError> {
    CodeReviewService::parse_batch(&state.db, input)
        .await
        .map_err(|e| e.into())
}
