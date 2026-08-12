use serde::{Deserialize, Serialize};

use crate::models::{KnowledgeAskResult, KnowledgeConversationMessage};

/// 问答入口显式绑定当前项目和版本，避免跨项目/版本证据泄漏。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeScopedQuestionInput {
    pub project_id: i64,
    pub project_version_id: i64,
    pub question: String,
    #[serde(default)]
    pub repository_binding_ids: Vec<i64>,
    /// 仅在请求远程 AI 回答时必填；证据预览不需要 Provider。
    #[serde(default)]
    pub provider_key: String,
    /// 必须与已配置 Provider 的默认聊天模型一致，避免调用方绕开模型配置。
    #[serde(default)]
    pub model: String,
    /// 只返回本地检索到的可追溯证据，不发送问题或正文给聊天 Provider。
    #[serde(default)]
    pub evidence_only: bool,
    /// 当前页面会话的历史用户问题与助手回答，用于支持连续追问。
    #[serde(default)]
    pub conversation: Vec<KnowledgeConversationMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQaSession {
    pub id: i64,
    pub project_id: i64,
    pub project_version_id: i64,
    pub release_commit_sha: String,
    pub provider_key: String,
    pub model: String,
    pub title: String,
    pub message_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQaMessage {
    pub id: i64,
    pub session_id: i64,
    pub role: String,
    pub content: String,
    pub evidence_only: bool,
    pub answer: Option<KnowledgeAskResult>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQaSessionDetail {
    pub session: KnowledgeQaSession,
    pub messages: Vec<KnowledgeQaMessage>,
}

/// 每轮回答完成后一次性写入用户消息和助手消息；`session_id` 为空时在同一事务内
/// 创建会话，避免 AI 成功但只留下空会话。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistKnowledgeQaRoundInput {
    #[serde(default)]
    pub session_id: Option<i64>,
    pub project_id: i64,
    pub project_version_id: i64,
    #[serde(default)]
    pub provider_key: String,
    #[serde(default)]
    pub model: String,
    pub question: String,
    pub answer: KnowledgeAskResult,
    #[serde(default)]
    pub evidence_only: bool,
}
