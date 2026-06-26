use crate::error::CommandError;
use crate::models::{
    DatabaseConnection, DatabaseConnectionTestResult, DatabaseExportInput, DatabaseExportResult,
    DatabaseNameListInput, DatabaseNameListResult, DatabaseQueryInput, DatabaseQueryResult,
    DatabaseSchemaInput, DatabaseSchemaResult, RedisDatabaseListInput, RedisDatabaseListResult,
    RedisDescribeKeysInput, RedisKeyTreeInput, RedisKeyTreeResult, RedisScanInput, RedisScanResult,
    RedisValuePreview, RedisValuePreviewInput, UpsertDatabaseConnectionInput,
};
use crate::services::database_ops::DatabaseOpsService;
use crate::state::AppState;

#[tauri::command]
pub fn list_database_connections(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DatabaseConnection>, CommandError> {
    DatabaseOpsService::list_connections(&state.db).map_err(|e| e.into())
}

#[tauri::command]
pub fn upsert_database_connection(
    state: tauri::State<'_, AppState>,
    input: UpsertDatabaseConnectionInput,
) -> Result<DatabaseConnection, CommandError> {
    DatabaseOpsService::upsert_connection(&state.db, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_database_connection(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<(), CommandError> {
    DatabaseOpsService::delete_connection(&state.db, &key).map_err(|e| e.into())
}

#[tauri::command]
pub async fn test_database_connection(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<DatabaseConnectionTestResult, CommandError> {
    DatabaseOpsService::test_connection(&state.db, &key)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn execute_database_readonly_query(
    state: tauri::State<'_, AppState>,
    input: DatabaseQueryInput,
) -> Result<DatabaseQueryResult, CommandError> {
    DatabaseOpsService::execute_readonly_query(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_database_names(
    state: tauri::State<'_, AppState>,
    input: DatabaseNameListInput,
) -> Result<DatabaseNameListResult, CommandError> {
    DatabaseOpsService::list_database_names(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_database_schema(
    state: tauri::State<'_, AppState>,
    input: DatabaseSchemaInput,
) -> Result<DatabaseSchemaResult, CommandError> {
    DatabaseOpsService::list_database_schema(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn execute_database_sql(
    state: tauri::State<'_, AppState>,
    input: DatabaseQueryInput,
) -> Result<DatabaseQueryResult, CommandError> {
    DatabaseOpsService::execute_sql(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn execute_database_sql_batch(
    state: tauri::State<'_, AppState>,
    input: DatabaseQueryInput,
) -> Result<Vec<DatabaseQueryResult>, CommandError> {
    DatabaseOpsService::execute_sql_batch(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn export_database(
    state: tauri::State<'_, AppState>,
    input: DatabaseExportInput,
) -> Result<DatabaseExportResult, CommandError> {
    DatabaseOpsService::export_database(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn scan_redis_keys(
    state: tauri::State<'_, AppState>,
    input: RedisScanInput,
) -> Result<RedisScanResult, CommandError> {
    DatabaseOpsService::scan_redis_keys(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn describe_redis_keys(
    state: tauri::State<'_, AppState>,
    input: RedisDescribeKeysInput,
) -> Result<RedisScanResult, CommandError> {
    DatabaseOpsService::describe_redis_keys(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_redis_databases(
    state: tauri::State<'_, AppState>,
    input: RedisDatabaseListInput,
) -> Result<RedisDatabaseListResult, CommandError> {
    DatabaseOpsService::list_redis_databases(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn list_redis_key_tree(
    state: tauri::State<'_, AppState>,
    input: RedisKeyTreeInput,
) -> Result<RedisKeyTreeResult, CommandError> {
    DatabaseOpsService::list_redis_key_tree(&state.db, input)
        .await
        .map_err(|e| e.into())
}

#[tauri::command]
pub async fn get_redis_value_preview(
    state: tauri::State<'_, AppState>,
    input: RedisValuePreviewInput,
) -> Result<RedisValuePreview, CommandError> {
    DatabaseOpsService::get_redis_value_preview(&state.db, input)
        .await
        .map_err(|e| e.into())
}
