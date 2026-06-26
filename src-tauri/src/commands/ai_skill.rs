use crate::error::CommandError;
use crate::models::{
    AiExperience, AiExperienceMatch, AiExperienceRecallInput, AiRunbook, AiSkill,
    AiSkillPromptPreviewInput, AiSkillPromptPreviewResult, AiSkillTriggerInput,
    AiSkillTriggerResult, ListAiSkillsInput, ListAiSkillsResult, RunAiRunbookInput,
    SyncBuiltinAiSkillsResult, UpsertAiExperienceInput, UpsertAiRunbookInput, UpsertAiSkillInput,
};
use crate::services::ai_skill::AiSkillService;
use crate::state::AppState;

#[tauri::command]
pub fn sync_builtin_ai_skills(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SyncBuiltinAiSkillsResult, CommandError> {
    AiSkillService::sync_builtin(&app, &state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_ai_skills(
    state: tauri::State<'_, AppState>,
    input: ListAiSkillsInput,
) -> Result<ListAiSkillsResult, CommandError> {
    AiSkillService::list(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_ai_skill(
    state: tauri::State<'_, AppState>,
    input: UpsertAiSkillInput,
) -> Result<AiSkill, CommandError> {
    AiSkillService::upsert(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn set_ai_skill_enabled(
    state: tauri::State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<AiSkill, CommandError> {
    AiSkillService::set_enabled(&state.db, id, enabled).map_err(|e| e.into())
}

#[tauri::command]
pub fn copy_ai_skill(state: tauri::State<'_, AppState>, id: i64) -> Result<AiSkill, CommandError> {
    AiSkillService::copy_skill(&state.db, id).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_ai_skill(state: tauri::State<'_, AppState>, id: i64) -> Result<(), CommandError> {
    AiSkillService::delete(&state.db, id).map_err(|e| e.into())
}

#[tauri::command]
pub fn restore_builtin_ai_skill(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<AiSkill, CommandError> {
    AiSkillService::restore_builtin(&state.db, id).map_err(|e| e.into())
}

#[tauri::command]
pub fn test_ai_skill_trigger(
    state: tauri::State<'_, AppState>,
    input: AiSkillTriggerInput,
) -> Result<AiSkillTriggerResult, CommandError> {
    AiSkillService::test_trigger(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn preview_ai_skill_prompt(
    state: tauri::State<'_, AppState>,
    input: AiSkillPromptPreviewInput,
) -> Result<AiSkillPromptPreviewResult, CommandError> {
    AiSkillService::prompt_preview(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_ai_experiences(
    state: tauri::State<'_, AppState>,
    keyword: Option<String>,
) -> Result<Vec<AiExperience>, CommandError> {
    AiSkillService::list_experiences(&state.db, keyword).map_err(|e| e.into())
}

#[tauri::command]
pub fn recall_ai_experiences(
    state: tauri::State<'_, AppState>,
    input: AiExperienceRecallInput,
) -> Result<Vec<AiExperienceMatch>, CommandError> {
    AiSkillService::recall_experiences(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_ai_experience(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UpsertAiExperienceInput,
) -> Result<AiExperience, CommandError> {
    AiSkillService::upsert_experience(&app, &state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_ai_experience(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    AiSkillService::delete_experience(&state.db, id).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_ai_runbooks(
    state: tauri::State<'_, AppState>,
    keyword: Option<String>,
) -> Result<Vec<AiRunbook>, CommandError> {
    AiSkillService::list_runbooks(&state.db, keyword).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_ai_runbook(
    state: tauri::State<'_, AppState>,
    input: UpsertAiRunbookInput,
) -> Result<AiRunbook, CommandError> {
    AiSkillService::upsert_runbook(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn run_ai_runbook(
    state: tauri::State<'_, AppState>,
    input: RunAiRunbookInput,
) -> Result<crate::models::AiRunbookRunResult, CommandError> {
    AiSkillService::run_runbook(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_ai_runbook(state: tauri::State<'_, AppState>, id: i64) -> Result<(), CommandError> {
    AiSkillService::delete_runbook(&state.db, id).map_err(|e| e.into())
}
