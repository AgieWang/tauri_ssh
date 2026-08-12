use crate::error::CommandError;
use crate::models::{
    KnowledgeProjectVersionCompleteness, KnowledgeProjectVersionManifestInput,
    KnowledgeProjectVersionManifestResult, KnowledgeRepositoryAvailability,
    KnowledgeRepositoryBinding, KnowledgeRepositoryBindingInput,
};
use crate::services::knowledge_domain::catalog::KnowledgeCatalogService;
use crate::state::AppState;

/// 读取项目当前有效的仓库关联。历史已解除关联不在默认列表中，仍由审计与版本清单保留。
#[tauri::command]
pub fn list_knowledge_project_repository_bindings(
    state: tauri::State<'_, AppState>,
    project_id: i64,
) -> Result<Vec<KnowledgeRepositoryBinding>, CommandError> {
    KnowledgeCatalogService::list_repository_bindings(&state.db, project_id).map_err(Into::into)
}

/// 原子替换当前项目的活动仓库关联；Service 会先校验每个工作区均已登记，DAO 保留历史。
#[tauri::command]
pub fn replace_knowledge_project_repository_bindings(
    state: tauri::State<'_, AppState>,
    input: KnowledgeRepositoryBindingInput,
) -> Result<Vec<KnowledgeRepositoryBinding>, CommandError> {
    KnowledgeCatalogService::replace_repository_bindings(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn unlink_knowledge_project_repository_binding(
    state: tauri::State<'_, AppState>,
    repository_binding_id: i64,
) -> Result<(), CommandError> {
    KnowledgeCatalogService::unlink_repository_binding(&state.db, repository_binding_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn inspect_knowledge_project_repository_binding(
    state: tauri::State<'_, AppState>,
    repository_binding_id: i64,
) -> Result<KnowledgeRepositoryAvailability, CommandError> {
    KnowledgeCatalogService::inspect_repository_binding(&state.db, repository_binding_id)
        .await
        .map_err(Into::into)
}

/// 创建多仓库项目版本的不可变 Commit 清单；Git 解析与完整性校验由目录 Service 执行。
#[tauri::command]
pub async fn create_knowledge_project_version_manifest(
    state: tauri::State<'_, AppState>,
    input: KnowledgeProjectVersionManifestInput,
) -> Result<KnowledgeProjectVersionManifestResult, CommandError> {
    KnowledgeCatalogService::create_project_version_manifest(&state.db, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn get_knowledge_project_version_manifest(
    state: tauri::State<'_, AppState>,
    release_id: i64,
) -> Result<KnowledgeProjectVersionManifestResult, CommandError> {
    KnowledgeCatalogService::get_project_version_manifest(&state.db, release_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_knowledge_project_version_completeness(
    state: tauri::State<'_, AppState>,
    release_id: i64,
) -> Result<KnowledgeProjectVersionCompleteness, CommandError> {
    KnowledgeCatalogService::get_project_version_completeness(&state.db, release_id)
        .map_err(Into::into)
}
