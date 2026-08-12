use crate::error::CommandError;
use crate::models::knowledge_domain::graph::{
    KnowledgeGraphBuildInput, KnowledgeGraphBuildResult, KnowledgeGraphProjection,
    KnowledgeGraphQueryInput,
};
use crate::services::knowledge_domain::graph::KnowledgeGraphService;
use crate::state::AppState;

/// 为一个项目版本构建本地知识图谱；构建失败不会替换上一次已启用的投影。
#[tauri::command]
pub fn build_knowledge_project_graph(
    state: tauri::State<'_, AppState>,
    input: KnowledgeGraphBuildInput,
) -> Result<KnowledgeGraphBuildResult, CommandError> {
    KnowledgeGraphService::build(&state.db, input).map_err(Into::into)
}

/// 查询当前启用图谱的有界子图。前端只能提供项目、版本与显示范围，不能传 SQL 或本地路径。
#[tauri::command]
pub fn query_knowledge_project_graph(
    state: tauri::State<'_, AppState>,
    input: KnowledgeGraphQueryInput,
) -> Result<KnowledgeGraphProjection, CommandError> {
    KnowledgeGraphService::query(&state.db, input).map_err(Into::into)
}
