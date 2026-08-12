use crate::error::CommandError;
use crate::models::knowledge_domain::terminology::{
    KnowledgeProjectTerm, UpsertKnowledgeProjectTermInput,
};
use crate::services::knowledge_domain::terminology::KnowledgeProjectTerminologyService;
use crate::state::AppState;

/// 读取当前项目已确认的术语。未确认候选不会通过该 Command 暴露给检索链路。
#[tauri::command]
pub fn list_knowledge_project_terms(
    state: tauri::State<'_, AppState>,
    project_id: i64,
) -> Result<Vec<KnowledgeProjectTerm>, CommandError> {
    KnowledgeProjectTerminologyService::list(&state.db, project_id).map_err(Into::into)
}

/// 保存本地人工确认的项目术语映射；Service 会校验项目范围、别名和确认说明。
#[tauri::command]
pub fn upsert_knowledge_project_term(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeProjectTermInput,
) -> Result<KnowledgeProjectTerm, CommandError> {
    KnowledgeProjectTerminologyService::upsert(&state.db, input).map_err(Into::into)
}

/// 删除前同时校验项目和术语归属，避免前端拿到其他项目的 ID 后跨项目修改。
#[tauri::command]
pub fn delete_knowledge_project_term(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    term_id: i64,
) -> Result<(), CommandError> {
    KnowledgeProjectTerminologyService::delete(&state.db, project_id, term_id).map_err(Into::into)
}
