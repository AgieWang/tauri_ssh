use serde::{Deserialize, Serialize};

/// 图谱构建始终锁定到一个已存在的项目版本；知识来源只来自本地已入库文档和关系，
/// 不会在构建过程中把正文发送给远程服务。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphBuildInput {
    pub project_id: i64,
    pub project_version_id: i64,
    #[serde(default)]
    pub include_unconfirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphBuildResult {
    pub build_id: i64,
    pub build_key: String,
    pub project_id: i64,
    pub project_version_id: i64,
    pub node_count: u32,
    pub edge_count: u32,
    /// 相同来源哈希已是当前启用投影时直接复用，避免构建过程中短暂替换可见图谱。
    pub reused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphQueryInput {
    pub project_id: i64,
    pub project_version_id: i64,
    #[serde(default)]
    pub root_entity_key: Option<String>,
    #[serde(default)]
    pub root_entity_type: Option<String>,
    #[serde(default = "default_depth")]
    pub depth: u8,
    #[serde(default = "default_node_limit")]
    pub node_limit: u32,
    #[serde(default)]
    pub include_unconfirmed: bool,
}

fn default_depth() -> u8 {
    1
}

fn default_node_limit() -> u32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphNode {
    pub id: i64,
    pub entity_type: String,
    pub entity_key: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphEdge {
    pub id: i64,
    pub from_node_id: i64,
    pub relation_type: String,
    pub to_node_id: i64,
    pub evidence: serde_json::Value,
    pub confidence: f64,
    pub confirmed: bool,
    pub source_relation_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphProjection {
    pub build_id: i64,
    pub build_key: String,
    pub project_id: i64,
    pub project_version_id: i64,
    pub nodes: Vec<KnowledgeGraphNode>,
    pub edges: Vec<KnowledgeGraphEdge>,
    pub truncated: bool,
}
