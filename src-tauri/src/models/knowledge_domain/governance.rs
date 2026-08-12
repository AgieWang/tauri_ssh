use serde::{Deserialize, Serialize};

/// 分域功能开关只表达发布资格；授权检查仍由 PolicyService 在实际读写边界执行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFeatureFlag {
    pub feature: String,
    #[serde(default)]
    pub project_id: Option<i64>,
    pub enabled: bool,
}
