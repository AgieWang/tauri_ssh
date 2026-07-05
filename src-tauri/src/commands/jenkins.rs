use crate::error::CommandError;
use crate::models::{
    ApprovalRequest, CleanupJenkinsArtifactInput, CreateJenkinsArtifactDeploymentCandidateInput,
    CreateJenkinsBuildDeploymentDryRunInput, DeleteJenkinsParameterTemplateInput,
    DeploymentCandidate, DeploymentPlan, DownloadJenkinsArtifactInput,
    ExecuteJenkinsBuildApprovedInput, ExecuteJenkinsBuildStopApprovedInput,
    ForgetJenkinsRecentParameterValueInput, GenerateJenkinsFailureAnalysisInput,
    GetJenkinsBuildInput, GetJenkinsJobDetailInput, InspectJenkinsFileParameterInput,
    JenkinsArtifact, JenkinsBuild, JenkinsBuildAnalysis, JenkinsBuildLogInput,
    JenkinsBuildLogResult, JenkinsBuildStopResult, JenkinsBuildTriggerResult, JenkinsConnection,
    JenkinsConnectionTestResult, JenkinsFileParameterMetadata, JenkinsJob, JenkinsJobDetail,
    JenkinsParameterDefinitionsResult, JenkinsParameterTemplate, JenkinsQueueItem,
    JenkinsRecentParameterValue, ListJenkinsArtifactsInput, ListJenkinsBuildsInput,
    ListJenkinsConnectionsInput, ListJenkinsJobsInput, ListJenkinsParameterTemplatesInput,
    ListJenkinsParametersInput, ListJenkinsRecentParameterValuesInput, PollJenkinsQueueItemInput,
    RecordJenkinsLogCopyAuditInput, SetJenkinsJobFavoriteInput, StopJenkinsBuildInput,
    TriggerJenkinsBuildInput, UpsertJenkinsConnectionInput, UpsertJenkinsParameterTemplateInput,
    VerifyJenkinsParameterDefinitionHashInput,
};
use crate::services::jenkins::JenkinsService;
use crate::state::AppState;

#[tauri::command]
pub fn list_jenkins_connections(
    state: tauri::State<'_, AppState>,
    input: ListJenkinsConnectionsInput,
) -> Result<Vec<JenkinsConnection>, CommandError> {
    JenkinsService::list_connections(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_jenkins_connection(
    state: tauri::State<'_, AppState>,
    input: UpsertJenkinsConnectionInput,
) -> Result<JenkinsConnection, CommandError> {
    JenkinsService::upsert_connection(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_jenkins_connection(
    state: tauri::State<'_, AppState>,
    connection_key: String,
) -> Result<(), CommandError> {
    JenkinsService::delete_connection(&state.db, &connection_key).map_err(|e| e.into())
}

#[tauri::command]
pub fn restore_jenkins_connection(
    state: tauri::State<'_, AppState>,
    connection_key: String,
) -> Result<JenkinsConnection, CommandError> {
    JenkinsService::restore_connection(&state.db, &connection_key).map_err(|e| e.into())
}

#[tauri::command]
pub fn duplicate_jenkins_connection(
    state: tauri::State<'_, AppState>,
    connection_key: String,
) -> Result<JenkinsConnection, CommandError> {
    JenkinsService::duplicate_connection(&state.db, &connection_key).map_err(|e| e.into())
}

#[tauri::command]
pub async fn test_jenkins_connection(
    state: tauri::State<'_, AppState>,
    connection_key: String,
) -> Result<JenkinsConnectionTestResult, CommandError> {
    JenkinsService::test_connection(&state.db, &connection_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_jenkins_jobs(
    state: tauri::State<'_, AppState>,
    input: ListJenkinsJobsInput,
) -> Result<Vec<JenkinsJob>, CommandError> {
    JenkinsService::list_jobs(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn get_jenkins_job_detail(
    state: tauri::State<'_, AppState>,
    input: GetJenkinsJobDetailInput,
) -> Result<JenkinsJobDetail, CommandError> {
    JenkinsService::get_job_detail(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn set_jenkins_job_favorite(
    state: tauri::State<'_, AppState>,
    input: SetJenkinsJobFavoriteInput,
) -> Result<bool, CommandError> {
    JenkinsService::set_job_favorite(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_jenkins_builds(
    state: tauri::State<'_, AppState>,
    input: ListJenkinsBuildsInput,
) -> Result<Vec<JenkinsBuild>, CommandError> {
    JenkinsService::list_builds(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn sync_unfinished_jenkins_runs(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    connection_key: String,
) -> Result<Vec<JenkinsBuild>, CommandError> {
    JenkinsService::sync_unfinished_runs_for_connection(&app, &state.db, &connection_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_jenkins_parameters(
    state: tauri::State<'_, AppState>,
    input: ListJenkinsParametersInput,
) -> Result<JenkinsParameterDefinitionsResult, CommandError> {
    JenkinsService::list_parameters(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn list_jenkins_recent_parameter_values(
    state: tauri::State<'_, AppState>,
    input: ListJenkinsRecentParameterValuesInput,
) -> Result<Vec<JenkinsRecentParameterValue>, CommandError> {
    JenkinsService::list_recent_parameter_values(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn forget_jenkins_recent_parameter_value(
    state: tauri::State<'_, AppState>,
    input: ForgetJenkinsRecentParameterValueInput,
) -> Result<bool, CommandError> {
    JenkinsService::forget_recent_parameter_value(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_jenkins_parameter_templates(
    state: tauri::State<'_, AppState>,
    input: ListJenkinsParameterTemplatesInput,
) -> Result<Vec<JenkinsParameterTemplate>, CommandError> {
    JenkinsService::list_parameter_templates(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_jenkins_parameter_template(
    state: tauri::State<'_, AppState>,
    input: UpsertJenkinsParameterTemplateInput,
) -> Result<JenkinsParameterTemplate, CommandError> {
    JenkinsService::upsert_parameter_template(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_jenkins_parameter_template(
    state: tauri::State<'_, AppState>,
    input: DeleteJenkinsParameterTemplateInput,
) -> Result<bool, CommandError> {
    JenkinsService::delete_parameter_template(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn verify_jenkins_parameter_definition_hash(
    state: tauri::State<'_, AppState>,
    input: VerifyJenkinsParameterDefinitionHashInput,
) -> Result<JenkinsParameterDefinitionsResult, CommandError> {
    JenkinsService::verify_parameter_definition_hash(
        &state.db,
        &input.connection_key,
        &input.job_full_name,
        &input.parameter_definition_hash,
    )
    .await
    .map_err(|e| e.into())
}

#[tauri::command]
pub fn inspect_jenkins_file_parameter(
    input: InspectJenkinsFileParameterInput,
) -> Result<JenkinsFileParameterMetadata, CommandError> {
    JenkinsService::inspect_file_parameter(input).map_err(|e| e.into())
}

#[tauri::command]
pub fn create_jenkins_build_trigger_approval(
    state: tauri::State<'_, AppState>,
    input: TriggerJenkinsBuildInput,
) -> Result<ApprovalRequest, CommandError> {
    JenkinsService::create_build_trigger_approval(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn execute_jenkins_build_trigger_approved(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: ExecuteJenkinsBuildApprovedInput,
) -> Result<JenkinsBuildTriggerResult, CommandError> {
    JenkinsService::execute_build_trigger_approved_with_event(&app, &state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn trigger_jenkins_build_without_approval(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: TriggerJenkinsBuildInput,
) -> Result<JenkinsBuildTriggerResult, CommandError> {
    JenkinsService::trigger_build_without_approval_with_event(&app, &state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn create_jenkins_build_stop_approval(
    state: tauri::State<'_, AppState>,
    input: StopJenkinsBuildInput,
) -> Result<ApprovalRequest, CommandError> {
    JenkinsService::create_build_stop_approval(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn execute_jenkins_build_stop_approved(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: ExecuteJenkinsBuildStopApprovedInput,
) -> Result<JenkinsBuildStopResult, CommandError> {
    JenkinsService::execute_build_stop_approved_with_event(&app, &state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn stop_jenkins_build_without_approval(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: StopJenkinsBuildInput,
) -> Result<JenkinsBuildStopResult, CommandError> {
    JenkinsService::stop_build_without_approval_with_event(&app, &state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn get_jenkins_build_detail(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: GetJenkinsBuildInput,
) -> Result<JenkinsBuild, CommandError> {
    JenkinsService::get_build_detail_with_event(&app, &state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn read_jenkins_build_log(
    state: tauri::State<'_, AppState>,
    input: JenkinsBuildLogInput,
) -> Result<JenkinsBuildLogResult, CommandError> {
    JenkinsService::read_build_log(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn record_jenkins_log_copy_audit(
    state: tauri::State<'_, AppState>,
    input: RecordJenkinsLogCopyAuditInput,
) -> Result<(), CommandError> {
    JenkinsService::record_log_copy_audit(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn generate_jenkins_failure_analysis(
    state: tauri::State<'_, AppState>,
    input: GenerateJenkinsFailureAnalysisInput,
) -> Result<JenkinsBuildAnalysis, CommandError> {
    JenkinsService::generate_failure_analysis(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn get_latest_jenkins_build_analysis(
    state: tauri::State<'_, AppState>,
    input: GetJenkinsBuildInput,
) -> Result<Option<JenkinsBuildAnalysis>, CommandError> {
    JenkinsService::get_latest_build_analysis(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_jenkins_queue(
    state: tauri::State<'_, AppState>,
    connection_key: String,
) -> Result<Vec<JenkinsQueueItem>, CommandError> {
    JenkinsService::list_queue(&state.db, connection_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn poll_jenkins_queue_item(
    state: tauri::State<'_, AppState>,
    input: PollJenkinsQueueItemInput,
) -> Result<JenkinsQueueItem, CommandError> {
    JenkinsService::poll_queue_item(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_jenkins_artifacts(
    state: tauri::State<'_, AppState>,
    input: ListJenkinsArtifactsInput,
) -> Result<Vec<JenkinsArtifact>, CommandError> {
    JenkinsService::list_artifacts(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn download_jenkins_artifact(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: DownloadJenkinsArtifactInput,
) -> Result<JenkinsArtifact, CommandError> {
    JenkinsService::download_artifact(&app, &state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn cleanup_jenkins_artifact_local_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: CleanupJenkinsArtifactInput,
) -> Result<JenkinsArtifact, CommandError> {
    JenkinsService::cleanup_artifact_local_file(&app, &state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn create_jenkins_artifact_deployment_candidate(
    state: tauri::State<'_, AppState>,
    input: CreateJenkinsArtifactDeploymentCandidateInput,
) -> Result<DeploymentCandidate, CommandError> {
    JenkinsService::create_artifact_deployment_candidate(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn create_jenkins_build_deployment_dry_run(
    state: tauri::State<'_, AppState>,
    input: CreateJenkinsBuildDeploymentDryRunInput,
) -> Result<DeploymentPlan, CommandError> {
    JenkinsService::create_build_deployment_dry_run(&state.db, input)
        .await
        .map_err(|e| e.into())
}
