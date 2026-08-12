use tauri::Manager;

use crate::error::{AppError, CommandError};
use serde::Deserialize;

use crate::models::knowledge_domain::qa::{
    KnowledgeQaSession, KnowledgeQaSessionDetail, KnowledgeScopedQuestionInput,
    PersistKnowledgeQaRoundInput,
};
use crate::models::KnowledgeAskResult;
use crate::services::knowledge_domain::qa::KnowledgeScopedQuestionService;
use crate::services::knowledge_domain::qa_export::KnowledgeQaExportService;
use crate::state::AppState;

pub(crate) const DOMAIN: &str = "qa";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveKnowledgeQaMarkdownInput {
    pub path: String,
    pub content: String,
}

/// 项目问答必须在后端从路由范围构造检索条件；页面无法传入全局项目或任意本地路径。
#[tauri::command]
pub async fn ask_knowledge_scoped_question(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: KnowledgeScopedQuestionInput,
) -> Result<KnowledgeAskResult, CommandError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::Custom(error.to_string()))?;
    KnowledgeScopedQuestionService::ask(&state.db, &app_data_dir, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_qa_sessions(
    state: tauri::State<'_, AppState>,
    project_id: i64,
) -> Result<Vec<KnowledgeQaSession>, CommandError> {
    KnowledgeScopedQuestionService::list_sessions(&state.db, project_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_knowledge_qa_session(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    session_id: i64,
) -> Result<KnowledgeQaSessionDetail, CommandError> {
    KnowledgeScopedQuestionService::get_session(&state.db, project_id, session_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn persist_knowledge_qa_round(
    state: tauri::State<'_, AppState>,
    input: PersistKnowledgeQaRoundInput,
) -> Result<KnowledgeQaSessionDetail, CommandError> {
    KnowledgeScopedQuestionService::persist_round(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn delete_knowledge_qa_session(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    session_id: i64,
) -> Result<(), CommandError> {
    KnowledgeScopedQuestionService::delete_session(&state.db, project_id, session_id)
        .map_err(Into::into)
}

/// 保存用户手动触发的 Markdown 问答记录；文件内容由 Service 统一脱敏并原子写入。
#[tauri::command]
pub fn save_knowledge_qa_markdown(
    input: SaveKnowledgeQaMarkdownInput,
) -> Result<String, CommandError> {
    KnowledgeQaExportService::save_markdown(&input.path, &input.content).map_err(Into::into)
}
