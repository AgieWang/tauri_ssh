use crate::error::CommandError;
use crate::models::{
    CreateDeploymentDryRunInput, CreateDeploymentRollbackDryRunInput, DeploymentAiAdviceInput,
    DeploymentAiAdviceResult, DeploymentDetectionResult, DeploymentEnvironmentProfile,
    DeploymentGroup, DeploymentImageStoreApp, DeploymentPlan, DeploymentRun, DeploymentRunDetail,
    DeploymentTarget, DeploymentTemplate, DetectDeploymentProjectInput,
    ExecuteDeploymentRollbackInput, ExecuteDeploymentRunInput, InstallImageStoreAppInput,
    ListDeploymentRunsInput, UpsertDeploymentGroupInput, UpsertDeploymentTargetInput,
};
use crate::services::deployment::DeploymentService;
use crate::state::AppState;

#[tauri::command]
pub fn list_deployment_templates() -> Result<Vec<DeploymentTemplate>, CommandError> {
    Ok(DeploymentService::list_templates())
}

#[tauri::command]
pub fn list_deployment_environment_profiles(
) -> Result<Vec<DeploymentEnvironmentProfile>, CommandError> {
    Ok(DeploymentService::list_environment_profiles())
}

#[tauri::command]
pub fn list_deployment_image_store_apps() -> Result<Vec<DeploymentImageStoreApp>, CommandError> {
    Ok(DeploymentService::list_image_store_apps())
}

#[tauri::command]
pub fn install_deployment_image_store_app(
    state: tauri::State<'_, AppState>,
    input: InstallImageStoreAppInput,
) -> Result<DeploymentTarget, CommandError> {
    DeploymentService::install_image_store_app(&state.db, input).map_err(|error| error.into())
}

#[tauri::command]
pub fn detect_deployment_project(
    state: tauri::State<'_, AppState>,
    input: DetectDeploymentProjectInput,
) -> Result<DeploymentDetectionResult, CommandError> {
    DeploymentService::detect_project(&state.db, input).map_err(|error| error.into())
}

#[tauri::command]
pub fn list_deployment_targets(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DeploymentTarget>, CommandError> {
    DeploymentService::list_targets(&state.db).map_err(|error| error.into())
}

#[tauri::command]
pub fn upsert_deployment_target(
    state: tauri::State<'_, AppState>,
    input: UpsertDeploymentTargetInput,
) -> Result<DeploymentTarget, CommandError> {
    DeploymentService::upsert_target(&state.db, input).map_err(|error| error.into())
}

#[tauri::command]
pub fn delete_deployment_target(
    state: tauri::State<'_, AppState>,
    target_key: String,
) -> Result<(), CommandError> {
    DeploymentService::delete_target(&state.db, &target_key).map_err(|error| error.into())
}

#[tauri::command]
pub fn list_deployment_groups(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DeploymentGroup>, CommandError> {
    DeploymentService::list_groups(&state.db).map_err(|error| error.into())
}

#[tauri::command]
pub fn upsert_deployment_group(
    state: tauri::State<'_, AppState>,
    input: UpsertDeploymentGroupInput,
) -> Result<DeploymentGroup, CommandError> {
    DeploymentService::upsert_group(&state.db, input).map_err(|error| error.into())
}

#[tauri::command]
pub fn delete_deployment_group(
    state: tauri::State<'_, AppState>,
    group_key: String,
) -> Result<(), CommandError> {
    DeploymentService::delete_group(&state.db, &group_key).map_err(|error| error.into())
}

#[tauri::command]
pub async fn create_deployment_dry_run(
    state: tauri::State<'_, AppState>,
    input: CreateDeploymentDryRunInput,
) -> Result<DeploymentPlan, CommandError> {
    DeploymentService::create_dry_run(&state.db, input)
        .await
        .map_err(|error| error.into())
}

#[tauri::command]
pub async fn execute_deployment_run(
    state: tauri::State<'_, AppState>,
    input: ExecuteDeploymentRunInput,
) -> Result<DeploymentRunDetail, CommandError> {
    DeploymentService::execute_run(&state.db, input)
        .await
        .map_err(|error| error.into())
}

#[tauri::command]
pub fn list_deployment_runs(
    state: tauri::State<'_, AppState>,
    input: ListDeploymentRunsInput,
) -> Result<Vec<DeploymentRun>, CommandError> {
    DeploymentService::list_runs(&state.db, input).map_err(|error| error.into())
}

#[tauri::command]
pub fn get_deployment_run_detail(
    state: tauri::State<'_, AppState>,
    run_id: String,
) -> Result<DeploymentRunDetail, CommandError> {
    DeploymentService::get_run_detail(&state.db, &run_id).map_err(|error| error.into())
}

#[tauri::command]
pub async fn create_deployment_rollback_dry_run(
    state: tauri::State<'_, AppState>,
    input: CreateDeploymentRollbackDryRunInput,
) -> Result<DeploymentPlan, CommandError> {
    DeploymentService::create_rollback_dry_run(&state.db, input)
        .await
        .map_err(|error| error.into())
}

#[tauri::command]
pub async fn execute_deployment_rollback(
    state: tauri::State<'_, AppState>,
    input: ExecuteDeploymentRollbackInput,
) -> Result<DeploymentRunDetail, CommandError> {
    DeploymentService::execute_rollback(&state.db, input)
        .await
        .map_err(|error| error.into())
}

#[tauri::command]
pub async fn ask_deployment_ai_advice(
    state: tauri::State<'_, AppState>,
    input: DeploymentAiAdviceInput,
) -> Result<DeploymentAiAdviceResult, CommandError> {
    DeploymentService::ai_advice(&state.db, input)
        .await
        .map_err(|error| error.into())
}
