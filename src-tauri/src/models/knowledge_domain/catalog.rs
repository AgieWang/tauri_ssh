use serde::{Deserialize, Serialize};

/// 项目版本的推荐选择方式。未知值必须在 IPC 边界拒绝，避免 Service 把拼写错误当作默认策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeVersionStrategy {
    Manual,
    TagOrBranch,
    Branch,
    Tag,
}

impl KnowledgeVersionStrategy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::TagOrBranch => "tag_or_branch",
            Self::Branch => "branch",
            Self::Tag => "tag",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(Self::Manual),
            "tag_or_branch" => Some(Self::TagOrBranch),
            "branch" => Some(Self::Branch),
            "tag" => Some(Self::Tag),
            _ => None,
        }
    }
}

impl Default for KnowledgeVersionStrategy {
    fn default() -> Self {
        Self::Manual
    }
}

/// 版本清单中允许解析的 Git 引用类型。分支、标签和提交以外的引用不能穿透 IPC。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeGitRefType {
    Branch,
    Tag,
    Commit,
}

impl KnowledgeGitRefType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Tag => "tag",
            Self::Commit => "commit",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value {
            "branch" => Some(Self::Branch),
            "tag" => Some(Self::Tag),
            "commit" => Some(Self::Commit),
            _ => None,
        }
    }
}

/// 将已登记 Git 工作区关联到项目的明确请求。路径从不由前端传入，避免绕过工作区授权。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryBindingInput {
    pub workspace_key: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default)]
    pub version_strategy: KnowledgeVersionStrategy,
}

/// 多仓库关联的原子输入；空集合必须由 Service 拒绝而非默认为解除关联。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRepositoryBindingInput {
    pub project_id: i64,
    #[serde(default)]
    pub repositories: Vec<RepositoryBindingInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRepositoryBinding {
    pub id: i64,
    pub project_id: i64,
    pub workspace_key: String,
    pub alias: String,
    pub repository_role: String,
    pub default_branch: String,
    pub version_strategy: KnowledgeVersionStrategy,
    pub enabled: bool,
    pub deleted_at: Option<String>,
}

/// 目录页面使用的只读仓库探测结果。不返回工作区绝对路径、未提交文件名或 Git 原始错误，
/// 避免把本地敏感目录和无关状态暴露给不需要它们的页面。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRepositoryAvailability {
    pub repository_binding_id: i64,
    pub workspace_key: String,
    pub available: bool,
    pub branch: String,
    pub head_commit: String,
    pub dirty: bool,
    pub changed_file_count: u32,
    pub message: String,
}

/// 项目版本由每个仓库的请求引用组成；Service 会将其解析为不可变 Commit 清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectVersionManifestInput {
    pub project_id: i64,
    pub version: String,
    #[serde(default)]
    pub repositories: Vec<ProjectVersionRepositoryRefInput>,
}

/// 已冻结的单仓库版本清单项。`requested_ref_*` 保存用户选择，`resolved_commit_sha` 是
/// 后续同步、分析、检索与问答实际使用的不可变证据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeReleaseRepositoryManifest {
    pub id: i64,
    pub release_id: i64,
    pub repository_binding_id: i64,
    pub requested_ref_type: KnowledgeGitRefType,
    pub requested_ref_name: String,
    pub resolved_commit_sha: String,
    pub capture_kind: String,
    pub inclusion_status: String,
    pub exclusion_reason: String,
    pub worktree_dirty: bool,
    pub captured_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectVersionManifestResult {
    pub release_id: i64,
    pub project_id: i64,
    pub version: String,
    pub status: String,
    pub repositories: Vec<KnowledgeReleaseRepositoryManifest>,
}

/// 一个版本阶段的可解释完成度。数量始终来自本地持久化数据，不以页面状态或推测替代。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectVersionStageCompleteness {
    pub stage: String,
    pub label: String,
    pub status: String,
    pub completed_count: i64,
    pub total_count: i64,
    pub summary: String,
}

/// 版本管理页一次呈现清单和各处理阶段，普通用户无需理解内部任务或数据库标识。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectVersionCompleteness {
    pub release_id: i64,
    pub project_id: i64,
    pub version: String,
    pub status: String,
    pub stages: Vec<KnowledgeProjectVersionStageCompleteness>,
}

/// 只补齐当前版本已经冻结的正文派生数据；不读取 Git 工作区，也不会改写版本清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectVersionBackfillInput {
    pub release_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVersionRepositoryRefInput {
    pub repository_binding_id: i64,
    pub ref_type: KnowledgeGitRefType,
    pub ref_name: String,
    #[serde(default)]
    pub excluded: bool,
}
