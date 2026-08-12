use serde::{Deserialize, Serialize};

/// 已确认的项目内业务术语。别名通常是代码符号或团队约定英文名，不能作为跨项目通用词典。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectTerm {
    pub id: i64,
    pub project_id: i64,
    pub term: String,
    pub aliases: Vec<String>,
    pub confirmation_note: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 写入时要求确认说明，避免 AI 猜测或未经确认的中英文关系直接改变搜索结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKnowledgeProjectTermInput {
    #[serde(default)]
    pub id: Option<i64>,
    pub project_id: i64,
    pub term: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub confirmation_note: String,
    #[serde(default)]
    pub created_by: Option<String>,
}

/// 本次搜索实际采用的受控映射，供页面解释为什么会出现代码命中。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectTermExpansion {
    pub term: String,
    pub aliases: Vec<String>,
}
