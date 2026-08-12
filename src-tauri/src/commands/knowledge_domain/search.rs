use crate::error::CommandError;
use crate::models::knowledge_domain::search::{
    KnowledgeCatalogSearchInput, KnowledgeCatalogSearchPage,
};
use crate::services::knowledge_domain::search::KnowledgeCatalogSearchService;
use crate::state::AppState;

/// 新知识工作台的项目内搜索入口。返回结果快照与不透明游标，页面无需自行计算偏移量。
#[tauri::command]
pub fn search_knowledge_catalog(
    state: tauri::State<'_, AppState>,
    input: KnowledgeCatalogSearchInput,
) -> Result<KnowledgeCatalogSearchPage, CommandError> {
    KnowledgeCatalogSearchService::search(&state.db, input).map_err(Into::into)
}
