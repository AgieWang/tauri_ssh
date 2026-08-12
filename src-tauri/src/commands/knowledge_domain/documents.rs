use crate::error::CommandError;
use crate::models::{
    CommitKnowledgeDocumentDraftInput, KnowledgeDocument, KnowledgeDocumentCommitResult,
    KnowledgeDocumentDeletionImpactPreview, KnowledgeDocumentDraftInput,
    KnowledgeDocumentDraftSaveResult, KnowledgeDocumentImagePreview, KnowledgeListInput,
    KnowledgePage, RestoreKnowledgeDocumentResult, RestoreKnowledgeDocumentVersionToDraftInput,
    RestoreKnowledgeDocumentVersionToDraftResult,
};
use crate::services::knowledge_domain::documents::KnowledgeDocumentService;
use crate::services::knowledge_domain::jobs::KnowledgeDocumentJobService;
use crate::state::AppState;
use tauri::Manager;

/// 保存人工文档草稿。正式提交、索引和版本绑定由后续独立 Command 负责，不能在这里
/// 把未提交内容写入正式文档链路。
#[tauri::command]
pub fn save_knowledge_document_draft(
    state: tauri::State<'_, AppState>,
    input: KnowledgeDocumentDraftInput,
) -> Result<KnowledgeDocumentDraftSaveResult, CommandError> {
    KnowledgeDocumentService::save_manual_draft(&state.db, input).map_err(Into::into)
}

/// 确认提交草稿后创建不可变正式版本，并在事务完成后调度后台索引；不会修改旧版本。
#[tauri::command]
pub fn commit_knowledge_document_draft(
    app: tauri::AppHandle,
    input: CommitKnowledgeDocumentDraftInput,
) -> Result<KnowledgeDocumentCommitResult, CommandError> {
    let state = app.state::<AppState>();
    let result = KnowledgeDocumentService::commit_manual_draft(&state.db, input)?;
    KnowledgeDocumentJobService::spawn_document_index_job(
        app,
        result.document_version_id,
        result.index_job_id,
    );
    Ok(result)
}

/// 将历史正文放入新草稿或经修订校验的既有草稿；用户随后通过提交草稿创建新的正式版本，
/// 因此此 Command 不会覆盖任何已提交历史。
#[tauri::command]
pub fn restore_knowledge_document_version_to_draft(
    state: tauri::State<'_, AppState>,
    input: RestoreKnowledgeDocumentVersionToDraftInput,
) -> Result<RestoreKnowledgeDocumentVersionToDraftResult, CommandError> {
    KnowledgeDocumentService::restore_version_to_draft(&state.db, input).map_err(Into::into)
}

/// 查询项目回收站中的软删除文档；该列表不返回正文或资产数据，恢复仍需显式操作。
#[tauri::command]
pub fn list_deleted_knowledge_documents(
    state: tauri::State<'_, AppState>,
    input: Option<KnowledgeListInput>,
) -> Result<KnowledgePage<KnowledgeDocument>, CommandError> {
    KnowledgeDocumentService::list_deleted_documents(&state.db, input).map_err(Into::into)
}

/// 返回删除确认所需的本地影响计数；该接口不会执行永久删除。
#[tauri::command]
pub fn preview_knowledge_document_deletion(
    state: tauri::State<'_, AppState>,
    document_id: i64,
) -> Result<KnowledgeDocumentDeletionImpactPreview, CommandError> {
    KnowledgeDocumentService::preview_deletion(&state.db, document_id).map_err(Into::into)
}

/// 恢复软删除文档并幂等回建当前版本全文索引，不改写历史版本与受控资产。
#[tauri::command]
pub fn restore_knowledge_document(
    state: tauri::State<'_, AppState>,
    document_id: i64,
) -> Result<RestoreKnowledgeDocumentResult, CommandError> {
    KnowledgeDocumentService::restore(&state.db, document_id).map_err(Into::into)
}

/// 仅为非受限文档的当前图片资产返回受控预览副本；绝不把受管存储路径暴露给前端。
#[tauri::command]
pub fn get_knowledge_document_image_preview(
    app: tauri::AppHandle,
    document_id: i64,
) -> Result<KnowledgeDocumentImagePreview, CommandError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::Custom(error.to_string()))?;
    let state = app.state::<AppState>();
    KnowledgeDocumentService::get_document_image_preview(&state.db, &app_data_dir, document_id)
        .map_err(Into::into)
}
