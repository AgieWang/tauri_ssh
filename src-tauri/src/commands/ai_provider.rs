use crate::error::CommandError;
use crate::models::{
    AiProvider, AiProviderAskInput, AiProviderAskResult, AiProviderModelListInput,
    AiProviderModelListResult, AiProviderRoute, AiProviderTestResult, UpsertAiProviderInput,
    UpsertAiProviderRouteInput,
};
use crate::services::ai_provider::AiProviderService;
use crate::state::AppState;

#[tauri::command]
pub fn list_ai_providers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AiProvider>, CommandError> {
    AiProviderService::list(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_ai_provider(
    state: tauri::State<'_, AppState>,
    input: UpsertAiProviderInput,
) -> Result<AiProvider, CommandError> {
    AiProviderService::upsert(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_ai_provider(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<(), CommandError> {
    AiProviderService::delete(&state.db, &key).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_ai_provider_routes(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AiProviderRoute>, CommandError> {
    AiProviderService::list_routes(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_ai_provider_route(
    state: tauri::State<'_, AppState>,
    input: UpsertAiProviderRouteInput,
) -> Result<AiProviderRoute, CommandError> {
    AiProviderService::upsert_route(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn test_ai_provider(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<AiProviderTestResult, CommandError> {
    AiProviderService::test(&state.db, &key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_ai_provider_models(
    state: tauri::State<'_, AppState>,
    input: AiProviderModelListInput,
) -> Result<AiProviderModelListResult, CommandError> {
    AiProviderService::list_models(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn ask_ai_provider(
    state: tauri::State<'_, AppState>,
    input: AiProviderAskInput,
) -> Result<AiProviderAskResult, CommandError> {
    AiProviderService::ask(&state.db, input)
        .await
        .map_err(|e| e.into())
}
