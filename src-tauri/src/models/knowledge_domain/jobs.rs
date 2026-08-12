use serde::{Deserialize, Serialize};

/// 统一编排器允许的任务类型。未知任务必须在进入持久化层前被 serde 拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeDomainJobType {
    Sync,
    Upload,
    Parse,
    Analysis,
    Embedding,
    Graph,
    Backfill,
}

/// 所有长任务均必须携带可复现的幂等键，不使用 UI 点击次数作为唯一性依据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDomainJobRequest {
    pub job_type: KnowledgeDomainJobType,
    pub idempotency_key: String,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub project_version_id: Option<i64>,
    #[serde(default)]
    pub payload_ref: Option<String>,
}
