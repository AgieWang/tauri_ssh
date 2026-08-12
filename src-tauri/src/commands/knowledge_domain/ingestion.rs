pub(crate) const DOMAIN: &str = "ingestion";

use tauri::Manager;

use crate::error::{AppError, CommandError};
use crate::models::{
    KnowledgeDocumentUploadBatchResult, KnowledgeDocumentUploadResult,
    PrepareKnowledgeUploadDirectoryInput, PrepareKnowledgeUploadFileInput,
    PreparedKnowledgeUploadDirectory, PreparedKnowledgeUploadFile, UploadKnowledgeAssetBatchInput,
    UploadKnowledgeAssetInput,
};
use crate::services::knowledge_domain::documents::KnowledgeDocumentService;
use crate::services::knowledge_domain::jobs::KnowledgeUploadImportJobService;
use crate::state::AppState;

/// 把桌面选择器返回的文件路径转换为短期一次性句柄，避免上传提交接口读取任意路径。
#[tauri::command]
pub fn prepare_knowledge_upload_file(
    state: tauri::State<'_, AppState>,
    input: PrepareKnowledgeUploadFileInput,
) -> Result<PreparedKnowledgeUploadFile, CommandError> {
    KnowledgeDocumentService::prepare_upload_file(
        &state.knowledge_upload_grants,
        &input.selected_path,
    )
    .map_err(Into::into)
}

/// 递归扫描用户明确选择的文件夹，并在阻塞线程中完成签名、大小和符号链接校验。
#[tauri::command]
pub async fn prepare_knowledge_upload_directory(
    app: tauri::AppHandle,
    input: PrepareKnowledgeUploadDirectoryInput,
) -> Result<PreparedKnowledgeUploadDirectory, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        KnowledgeDocumentService::prepare_upload_directory(
            &state.knowledge_upload_grants,
            &input.selected_path,
        )
    })
    .await
    .map_err(|error| AppError::Custom(format!("文件夹准备任务启动失败: {error}")))?
    .map_err(Into::into)
}

/// 复制文件和写入导入记录可能耗时，放到阻塞工作线程；只传递一次性句柄，绝对路径不离开后端。
#[tauri::command]
pub async fn create_knowledge_document_upload(
    app: tauri::AppHandle,
    input: UploadKnowledgeAssetInput,
) -> Result<KnowledgeDocumentUploadResult, CommandError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Custom(error.to_string()))?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = KnowledgeDocumentService::create_upload_import(
            &state.db,
            &app_data_dir,
            &state.knowledge_upload_grants,
            input,
        )?;
        KnowledgeUploadImportJobService::spawn_upload_import_job(app, result.import_job_id);
        Ok::<KnowledgeDocumentUploadResult, AppError>(result)
    })
    .await
    .map_err(|error| AppError::Custom(format!("上传任务启动失败: {error}")))?
    .map_err(Into::into)
}

/// 批量导入返回逐文件的已排队/失败结果；文件复制仍在后端线程完成，不阻塞桌面 IPC。
#[tauri::command]
pub async fn create_knowledge_document_upload_batch(
    app: tauri::AppHandle,
    input: UploadKnowledgeAssetBatchInput,
) -> Result<KnowledgeDocumentUploadBatchResult, CommandError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Custom(error.to_string()))?;
    let task_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        KnowledgeDocumentService::create_upload_import_batch(
            &state.db,
            &app_data_dir,
            &state.knowledge_upload_grants,
            input,
        )
    })
    .await
    .map_err(|error| AppError::Custom(format!("批量上传任务启动失败: {error}")))?
    .map_err(CommandError::from)?;
    for item in &result.items {
        if let Some(upload) = &item.result {
            KnowledgeUploadImportJobService::spawn_upload_import_job(
                task_app.clone(),
                upload.import_job_id,
            );
        }
    }
    Ok(result)
}
