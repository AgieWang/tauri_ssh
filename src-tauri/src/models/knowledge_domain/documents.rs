use serde::{Deserialize, Serialize};

/// 可变草稿始终与正式文档版本分开，避免未提交内容进入检索、图谱或问答。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentDraftInput {
    /// 草稿标识与正式文档标识分离；未提供时创建，提供时以 revision 进行更新。
    #[serde(default)]
    pub draft_id: Option<i64>,
    #[serde(default)]
    pub document_id: Option<i64>,
    pub project_id: i64,
    pub title: String,
    pub content: String,
    /// 手工文档默认是 Markdown；富文本仅作为显式类型保存，不会自动进入正式索引。
    #[serde(default = "default_draft_document_type")]
    pub doc_type: String,
    #[serde(default)]
    pub base_version_id: Option<i64>,
    #[serde(default)]
    pub revision: Option<i64>,
    /// 编辑者仅用于草稿冲突提示和审计摘要，缺省时由服务端使用本地用户标识。
    #[serde(default)]
    pub editor_label: Option<String>,
}

fn default_draft_document_type() -> String {
    "markdown".to_string()
}

/// 草稿在提交为正式版本之前不参与标题、全文、向量、图谱和问答索引。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentDraft {
    pub id: i64,
    pub document_id: Option<i64>,
    pub project_id: i64,
    pub title: String,
    pub content: String,
    pub doc_type: String,
    pub base_version_id: Option<i64>,
    pub revision: i64,
    pub editor_label: String,
}

/// 乐观并发失败也返回服务端当前草稿，界面可保留本地输入供用户比较或重试。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentDraftSaveResult {
    pub draft: KnowledgeDocumentDraft,
    pub conflict: bool,
}

/// 以不可变历史版本创建或更新恢复草稿。提供既有草稿时必须同时给出其修订号，避免用
/// 历史正文静默覆盖编辑中的新内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreKnowledgeDocumentVersionToDraftInput {
    pub source_version_id: i64,
    #[serde(default)]
    pub draft_id: Option<i64>,
    #[serde(default)]
    pub revision: Option<i64>,
    #[serde(default)]
    pub editor_label: Option<String>,
}

/// 恢复永远创建或更新草稿，正式版本仍须由用户显式提交。发生并发冲突时 `draft`
/// 是服务端当前正文，可直接与 `source_version` 的历史正文比较。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreKnowledgeDocumentVersionToDraftResult {
    pub source_version: crate::models::KnowledgeDocumentVersion,
    pub draft: KnowledgeDocumentDraft,
    pub conflict: bool,
}

/// 确认提交草稿的输入。项目版本为可选范围事实，不提供时不会被系统猜测为“最新版本”。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitKnowledgeDocumentDraftInput {
    pub draft_id: i64,
    pub revision: i64,
    pub version_label: String,
    #[serde(default)]
    pub project_version_id: Option<i64>,
    /// 仅当文档明确适用于项目的全部版本时使用固定范围值 `project_all_versions`。
    /// 省略项目版本和跨版本范围都会被拒绝，避免未绑定文档泄漏到版本检索中。
    #[serde(default)]
    pub cross_version_scope: Option<String>,
    #[serde(default)]
    pub commit_message: Option<String>,
    #[serde(default)]
    pub author_label: Option<String>,
}

/// 已提交版本只返回可审计的标识与索引排队状态；正文仍通过详情接口按权限读取。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentCommitResult {
    pub document_id: i64,
    pub document_version_id: i64,
    pub parent_version_id: Option<i64>,
    pub content_hash: String,
    pub index_job_id: i64,
    pub index_job_status: String,
}

/// 一次提交只会创建新版本；项目版本未指定时由 Service 明确拒绝或标记跨版本范围。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentVersionBindingInput {
    pub document_version_id: i64,
    #[serde(default)]
    pub project_version_id: Option<i64>,
    #[serde(default)]
    pub repository_binding_id: Option<i64>,
    #[serde(default)]
    pub cross_version_scope: Option<String>,
}
