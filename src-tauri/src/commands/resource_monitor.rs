use crate::error::CommandError;
use crate::models::{
    CollectResourceBatchInput, CollectResourceBatchResult, ListResourceAlertEventsInput,
    ListResourceAlertRulesInput, ResourceAlertEvent, ResourceAlertRule, ResourceMetricSnapshot,
    ResourceMonitorOverview, ResourceMonitorTarget, ResourceSnapshotListInput,
    UpsertResourceAlertRuleInput, UpsertResourceMonitorTargetInput,
};
use crate::services::resource_monitor::ResourceMonitorService;
use crate::state::AppState;

#[tauri::command]
pub fn list_resource_monitor_targets(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ResourceMonitorTarget>, CommandError> {
    ResourceMonitorService::list_targets(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_resource_monitor_target(
    state: tauri::State<'_, AppState>,
    input: UpsertResourceMonitorTargetInput,
) -> Result<ResourceMonitorTarget, CommandError> {
    ResourceMonitorService::upsert_target(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_resource_monitor_target(
    state: tauri::State<'_, AppState>,
    target_type: String,
    target_key: String,
) -> Result<(), CommandError> {
    ResourceMonitorService::delete_target(&state.db, &target_type, &target_key)
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn get_resource_monitor_overview(
    state: tauri::State<'_, AppState>,
) -> Result<ResourceMonitorOverview, CommandError> {
    ResourceMonitorService::overview(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_resource_metric_snapshots(
    state: tauri::State<'_, AppState>,
    input: ResourceSnapshotListInput,
) -> Result<Vec<ResourceMetricSnapshot>, CommandError> {
    ResourceMonitorService::list_snapshots(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub async fn collect_server_resource_snapshot(
    state: tauri::State<'_, AppState>,
    alias: String,
) -> Result<ResourceMetricSnapshot, CommandError> {
    ResourceMonitorService::collect_server(&state.db, &alias)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn collect_database_resource_snapshot(
    state: tauri::State<'_, AppState>,
    connection_key: String,
) -> Result<ResourceMetricSnapshot, CommandError> {
    ResourceMonitorService::collect_database(&state.db, &connection_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn collect_redis_resource_snapshot(
    state: tauri::State<'_, AppState>,
    connection_key: String,
) -> Result<ResourceMetricSnapshot, CommandError> {
    ResourceMonitorService::collect_redis(&state.db, &connection_key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn collect_resource_snapshots_batch(
    state: tauri::State<'_, AppState>,
    input: CollectResourceBatchInput,
) -> Result<CollectResourceBatchResult, CommandError> {
    ResourceMonitorService::collect_batch(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub fn list_resource_alert_rules(
    state: tauri::State<'_, AppState>,
    input: ListResourceAlertRulesInput,
) -> Result<Vec<ResourceAlertRule>, CommandError> {
    ResourceMonitorService::list_alert_rules(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_resource_alert_rule(
    state: tauri::State<'_, AppState>,
    input: UpsertResourceAlertRuleInput,
) -> Result<ResourceAlertRule, CommandError> {
    ResourceMonitorService::upsert_alert_rule(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_resource_alert_rule(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    ResourceMonitorService::delete_alert_rule(&state.db, id).map_err(|e| e.into())
}

#[tauri::command]
pub fn list_resource_alert_events(
    state: tauri::State<'_, AppState>,
    input: ListResourceAlertEventsInput,
) -> Result<Vec<ResourceAlertEvent>, CommandError> {
    ResourceMonitorService::list_alert_events(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn resolve_resource_alert_event(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    ResourceMonitorService::resolve_alert_event(&state.db, id).map_err(|e| e.into())
}
