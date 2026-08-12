use crate::models::knowledge_domain::terminology::KnowledgeProjectTermExpansion;
use crate::models::KnowledgeSearchHit;
use serde::{Deserialize, Serialize};

/// 新搜索页面的范围优先于召回通道；空范围不能被自动扩展为其他项目或版本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCatalogSearchInput {
    pub project_id: i64,
    #[serde(default)]
    pub project_version_id: Option<i64>,
    pub query: String,
    #[serde(default)]
    pub repository_binding_ids: Vec<i64>,
    #[serde(default)]
    pub document_types: Vec<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// 搜索页是一次可恢复的浏览会话：首次请求返回结果快照和下一页游标；索引范围在
/// 翻页前变化时，服务明确通知页面刷新，避免把新旧排序结果拼在同一列表中。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCatalogSearchPage {
    pub items: Vec<KnowledgeSearchHit>,
    pub next_cursor: Option<String>,
    pub result_snapshot: String,
    pub snapshot_changed: bool,
    /// 仅回显本次已确认且实际触发的项目术语；空数组兼容旧客户端与未配置术语的项目。
    #[serde(default)]
    pub applied_terms: Vec<KnowledgeProjectTermExpansion>,
}
