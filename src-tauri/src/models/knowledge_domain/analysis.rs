use serde::{Deserialize, Serialize};

/// AI 分析草稿只能引用已冻结的快照和证据；确认入库由独立提交操作完成。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateKnowledgeAnalysisDraftInput {
    pub project_id: i64,
    pub project_version_id: i64,
    pub snapshot_ids: Vec<i64>,
    #[serde(default)]
    pub provider_key: Option<String>,
    #[serde(default)]
    pub template_key: Option<String>,
}

/// AI 生成的项目分析草稿。它与正式知识文档版本分离，用户编辑并确认前不会进入搜索、
/// 图谱或问答索引。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeAnalysisDraft {
    pub id: i64,
    pub analysis_run_id: i64,
    pub project_id: i64,
    pub project_version_id: i64,
    pub snapshot_ids: Vec<i64>,
    pub provider_key: String,
    pub model: String,
    pub template_key: String,
    pub content: String,
    /// 每一个引用都指向本次固定代码快照中的一个文件，而不是由模型自由编造路径。
    pub claim_refs: Vec<String>,
    pub status: String,
    pub confirmed_document_version_id: Option<i64>,
}

/// 用户确认入库时可以编辑标题与正文；后端会创建新的不可变知识文档版本，而非覆盖 AI
/// 草稿或任意既有文档。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmKnowledgeAnalysisDraftInput {
    pub draft_id: i64,
    pub title: String,
    pub content: String,
    pub version_label: String,
    #[serde(default)]
    pub author_label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmKnowledgeAnalysisDraftResult {
    pub draft: KnowledgeAnalysisDraft,
    pub document: crate::models::knowledge_domain::documents::KnowledgeDocumentCommitResult,
}
