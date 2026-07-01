use serde::{Deserialize, Serialize};

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub key: String,
    pub value: String,
}

/// 系统信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub app_version: String,
    pub data_dir: String,
}

/// SSH 服务器配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshServer {
    pub alias: String,
    pub group_name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub source: String,
    pub auth_type: String,
    pub auth_ref: String,
    pub identity_file: String,
    pub password_masked: Option<String>,
    pub has_password: bool,
    pub proxy_jump: String,
    pub ai_policy: String,
    pub status: String,
    pub enabled: bool,
    pub last_connected_at: Option<String>,
    pub updated_at: String,
}

/// SSH 服务器创建/更新输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertSshServerInput {
    pub alias: String,
    pub group_name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub source: String,
    pub auth_type: String,
    pub auth_ref: String,
    pub identity_file: String,
    pub password: Option<String>,
    pub clear_password: Option<bool>,
    pub proxy_jump: String,
    pub ai_policy: String,
    pub status: Option<String>,
    pub enabled: bool,
}

/// SSH 服务器临时连接测试输入，不落库。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshServerConnectionTestInput {
    pub alias: Option<String>,
    pub host: String,
    pub port: i64,
}

/// SSH 服务器连接测试结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshServerTestResult {
    pub ok: bool,
    pub alias: String,
    pub endpoint: String,
    pub latency_ms: i64,
    pub message: String,
}

/// SSH Config 导入结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigImportResult {
    pub imported: i64,
    pub skipped: i64,
    pub servers: Vec<SshServer>,
}

/// AI Provider 配置。密钥只以掩码状态返回给前端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProvider {
    pub key: String,
    pub name: String,
    pub region: String,
    pub protocol: String,
    pub default_model: String,
    pub status: String,
    pub endpoint: String,
    pub auth_type: String,
    pub api_key_masked: Option<String>,
    pub has_api_key: bool,
    pub latency_ms: Option<i64>,
    pub cost_level: String,
    pub capabilities: Vec<String>,
    pub models: Vec<String>,
    pub scenario_fit: Vec<String>,
    pub fallback: String,
    pub enabled: bool,
    pub updated_at: String,
}

/// AI Provider 创建/更新输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertAiProviderInput {
    pub key: String,
    pub name: String,
    pub region: String,
    pub protocol: String,
    pub default_model: String,
    pub status: String,
    pub endpoint: String,
    pub auth_type: String,
    pub api_key: Option<String>,
    pub clear_api_key: Option<bool>,
    pub cost_level: String,
    pub capabilities: Vec<String>,
    pub models: Vec<String>,
    pub scenario_fit: Vec<String>,
    pub fallback: String,
    pub enabled: bool,
}

/// AI Provider 模型列表读取输入。API Key 只用于本次后端请求，不会返回给前端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderModelListInput {
    pub key: String,
    pub protocol: String,
    pub endpoint: String,
    pub auth_type: String,
    pub api_key: Option<String>,
}

/// AI Provider 模型列表读取结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderModelListResult {
    pub provider_key: String,
    pub models: Vec<String>,
    pub source: String,
}

/// AI Provider 场景路由。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderRoute {
    pub scenario: String,
    pub primary_provider_key: String,
    pub fallback_provider_key: String,
    pub requirement: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertAiProviderRouteInput {
    pub scenario: String,
    pub primary_provider_key: String,
    pub fallback_provider_key: String,
    pub requirement: String,
}

/// Provider 连接测试结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderTestResult {
    pub ok: bool,
    pub provider_key: String,
    pub provider_name: String,
    pub model: String,
    pub endpoint: String,
    pub latency_ms: i64,
    pub status_code: Option<u16>,
    pub message: String,
}

/// AI Provider 问答输入。用于终端中文自然语言直问等场景。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderAskInput {
    pub prompt: String,
    pub provider_key: Option<String>,
    pub system_prompt: Option<String>,
    pub skill_scope: Option<String>,
    pub use_skill_trigger: Option<bool>,
}

/// AI Provider 问答结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderAskResult {
    pub provider_key: String,
    pub provider_name: String,
    pub model: String,
    pub answer: String,
    pub latency_ms: i64,
}

/// AI Skill 记录。内置 Skill 来自打包资源，用户 Skill 来自 SQLite。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkill {
    pub id: i64,
    pub skill_key: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub scopes: Vec<String>,
    pub trigger_words: Vec<String>,
    pub tags: Vec<String>,
    pub priority: i64,
    pub enabled: bool,
    pub builtin: bool,
    pub source: String,
    pub source_path: String,
    pub content_hash: String,
    pub missing: bool,
    pub builtin_version: i64,
    pub user_overridden: bool,
    pub allow_mcp: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertAiSkillInput {
    pub id: Option<i64>,
    pub skill_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub scopes: Vec<String>,
    pub trigger_words: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub priority: Option<i64>,
    pub enabled: Option<bool>,
    pub allow_mcp: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAiSkillsInput {
    pub keyword: Option<String>,
    pub source: Option<String>,
    pub show_builtin: Option<bool>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillStats {
    pub total: i64,
    pub user: i64,
    pub builtin: i64,
    pub enabled: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAiSkillsResult {
    pub items: Vec<AiSkill>,
    pub stats: AiSkillStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillTriggerInput {
    pub prompt: String,
    pub scope: Option<String>,
    pub include_global: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillMatch {
    pub skill: AiSkill,
    pub matched_words: Vec<String>,
    pub score: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExperienceMatch {
    pub experience: AiExperience,
    pub matched_words: Vec<String>,
    pub score: i64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillTriggerResult {
    pub prompt: String,
    pub scope: String,
    pub matches: Vec<AiSkillMatch>,
    pub experiences: Vec<AiExperienceMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillPromptPreviewInput {
    pub prompt: Option<String>,
    pub scope: String,
    pub include_global: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSkillPromptPreviewResult {
    pub scope: String,
    pub skills: Vec<AiSkill>,
    pub experiences: Vec<AiExperienceMatch>,
    pub prompt_fragment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExperienceRecallInput {
    pub prompt: String,
    pub scope: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBuiltinAiSkillsResult {
    pub scanned: i64,
    pub inserted: i64,
    pub updated: i64,
    pub missing: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExperience {
    pub id: i64,
    pub experience_key: String,
    pub title: String,
    pub symptom: String,
    pub cause: String,
    pub solution: String,
    pub scenario: String,
    pub source: String,
    pub tags: Vec<String>,
    pub references_json: String,
    pub markdown_path: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertAiExperienceInput {
    pub id: Option<i64>,
    pub experience_key: Option<String>,
    pub title: String,
    pub symptom: Option<String>,
    pub cause: Option<String>,
    pub solution: Option<String>,
    pub scenario: Option<String>,
    pub source: Option<String>,
    pub tags: Option<Vec<String>>,
    pub references_json: Option<String>,
    pub markdown_path: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRunbookStep {
    pub id: String,
    pub title: String,
    pub step_type: String,
    pub content: String,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRunbook {
    pub id: i64,
    pub runbook_key: String,
    pub name: String,
    pub description: String,
    pub scenario: String,
    pub tags: Vec<String>,
    pub steps: Vec<AiRunbookStep>,
    pub enabled: bool,
    pub allow_mcp: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertAiRunbookInput {
    pub id: Option<i64>,
    pub runbook_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub scenario: Option<String>,
    pub tags: Option<Vec<String>>,
    pub steps: Option<Vec<AiRunbookStep>>,
    pub enabled: Option<bool>,
    pub allow_mcp: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunAiRunbookInput {
    pub id: Option<i64>,
    pub runbook_key: Option<String>,
    pub server_alias: Option<String>,
    pub database_connection_key: Option<String>,
    pub database_name: Option<String>,
    pub requester: Option<String>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRunbookStepResult {
    pub step_id: String,
    pub title: String,
    pub step_type: String,
    pub risk_level: String,
    pub status: String,
    pub message: String,
    pub output: serde_json::Value,
    pub approval_id: Option<i64>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRunbookRunResult {
    pub runbook: AiRunbook,
    pub status: String,
    pub message: String,
    pub steps: Vec<AiRunbookStepResult>,
}

/// 本地凭据保险库条目。敏感值永不返回前端，只返回掩码状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialVaultItem {
    pub key: String,
    pub credential_type: String,
    pub scope: String,
    pub status: String,
    pub description: String,
    pub secret_masked: Option<String>,
    pub has_secret: bool,
    pub enabled: bool,
    pub rotated_at: Option<String>,
    pub updated_at: String,
}

/// 新增/更新凭据输入。secret 只用于本次写入，不会返回前端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertCredentialInput {
    pub key: String,
    pub credential_type: String,
    pub scope: String,
    pub status: Option<String>,
    pub description: String,
    pub secret: Option<String>,
    pub clear_secret: Option<bool>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeCredentialInput {
    pub key: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotateCredentialInput {
    pub key: String,
    pub secret: String,
}

/// AI/MCP 安全凭证元数据。敏感值只在 Rust 后端短暂解密，永不返回前端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredential {
    pub id: i64,
    pub credential_key: String,
    pub display_name: String,
    pub provider: String,
    pub credential_type: String,
    pub account_name: String,
    pub base_url: String,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub folder: String,
    pub description: String,
    pub status: String,
    pub enabled: bool,
    pub allow_mcp: bool,
    pub approval_policy: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub usage_count: i64,
    pub has_secret: bool,
    pub secret_masked: Option<String>,
    pub rotated_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSecureCredentialsInput {
    pub keyword: Option<String>,
    pub provider: Option<String>,
    pub status: Option<String>,
    pub allow_mcp: Option<bool>,
}

/// 新增/更新安全凭证输入。secret 只用于本次写入，不会回显。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertSecureCredentialInput {
    pub id: Option<i64>,
    pub credential_key: String,
    pub display_name: String,
    pub provider: String,
    pub credential_type: String,
    pub account_name: Option<String>,
    pub base_url: Option<String>,
    pub scopes: Vec<String>,
    pub tags: Vec<String>,
    pub folder: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub enabled: Option<bool>,
    pub allow_mcp: Option<bool>,
    pub approval_policy: Option<String>,
    pub expires_at: Option<String>,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotateSecureCredentialInput {
    pub credential_key: String,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSecureCredentialEnabledInput {
    pub credential_key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialPolicySettings {
    pub default_session_ttl_minutes: i64,
    pub max_response_items: i64,
    pub allow_readonly_auto: bool,
    pub require_approval_for_all: bool,
    pub allow_http_custom_headers: bool,
    pub http_allowed_domains: Vec<String>,
    pub rate_limit_per_minute: i64,
    pub max_concurrent_sessions: i64,
    pub allow_default_branch_commits: bool,
    pub allow_high_risk_repo_ops: bool,
    pub allow_delete_branch: bool,
    pub allow_delete_tag: bool,
    pub allow_delete_release: bool,
    pub allow_update_ref: bool,
    pub allow_update_repo_settings: bool,
    pub updated_at: Option<String>,
}

impl Default for SecureCredentialPolicySettings {
    fn default() -> Self {
        Self {
            default_session_ttl_minutes: 30,
            max_response_items: 100,
            allow_readonly_auto: true,
            require_approval_for_all: false,
            allow_http_custom_headers: false,
            http_allowed_domains: Vec::new(),
            rate_limit_per_minute: 60,
            max_concurrent_sessions: 5,
            allow_default_branch_commits: false,
            allow_high_risk_repo_ops: false,
            allow_delete_branch: false,
            allow_delete_tag: false,
            allow_delete_release: false,
            allow_update_ref: false,
            allow_update_repo_settings: false,
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSecureCredentialPolicySettingsInput {
    pub default_session_ttl_minutes: i64,
    pub max_response_items: i64,
    pub allow_readonly_auto: bool,
    pub require_approval_for_all: bool,
    pub allow_http_custom_headers: bool,
    pub http_allowed_domains: Vec<String>,
    pub rate_limit_per_minute: i64,
    pub max_concurrent_sessions: i64,
    pub allow_default_branch_commits: bool,
    pub allow_high_risk_repo_ops: bool,
    pub allow_delete_branch: bool,
    pub allow_delete_tag: bool,
    pub allow_delete_release: bool,
    pub allow_update_ref: bool,
    pub allow_update_repo_settings: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialOverview {
    pub total: i64,
    pub active: i64,
    pub disabled: i64,
    pub mcp_enabled: i64,
    pub expiring_soon: i64,
    pub weekly_calls: i64,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialAuditLog {
    pub id: i64,
    pub actor: String,
    pub source: String,
    pub provider: String,
    pub credential_key: String,
    pub action: String,
    pub risk: String,
    pub result: String,
    pub duration_ms: i64,
    pub request_id: String,
    pub approval_id: Option<i64>,
    pub detail_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSecureCredentialAuditLogsInput {
    pub keyword: Option<String>,
    pub source: Option<String>,
    pub provider: Option<String>,
    pub credential_key: Option<String>,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub risk: Option<String>,
    pub result: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSecureCredentialAuditLogInput {
    pub actor: String,
    pub source: String,
    pub provider: String,
    pub credential_key: String,
    pub action: String,
    pub risk: String,
    pub result: String,
    pub duration_ms: i64,
    pub request_id: Option<String>,
    pub approval_id: Option<i64>,
    pub detail_json: Option<String>,
}

/// 安全凭证短期会话。AI/MCP 只能拿 session_id，不能反查凭证明文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialSession {
    pub id: i64,
    pub session_id: String,
    pub credential_key: String,
    pub provider: String,
    pub caller: String,
    pub scopes: Vec<String>,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub call_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSecureCredentialSessionsInput {
    pub credential_key: Option<String>,
    pub status: Option<String>,
    pub caller: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSecureCredentialSessionInput {
    pub credential_key: String,
    pub caller: Option<String>,
    pub scopes: Vec<String>,
    pub ttl_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialSessionStatus {
    pub session: SecureCredentialSession,
    pub valid: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialProviderTestInput {
    pub credential_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialProviderTestResult {
    pub ok: bool,
    pub credential_key: String,
    pub provider: String,
    pub account: String,
    pub status_code: Option<u16>,
    pub latency_ms: i64,
    pub message: String,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialRepositoryListInput {
    pub session_id: String,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialRepository {
    pub id: String,
    pub name: String,
    pub full_name: String,
    pub web_url: String,
    pub visibility: String,
    pub default_branch: String,
    pub permissions: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialGitReadInput {
    pub session_id: String,
    pub resource: String,
    pub repo: Option<String>,
    pub path: Option<String>,
    pub reference: Option<String>,
    pub state: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialProviderReadResult {
    pub provider: String,
    pub resource: String,
    pub status_code: u16,
    pub url: String,
    pub body: serde_json::Value,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialHttpRequestInput {
    pub session_id: String,
    pub path: String,
    pub query_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialHttpWriteInput {
    pub session_id: String,
    pub method: String,
    pub path: String,
    pub query_json: Option<serde_json::Value>,
    pub body_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialHttpRequestResult {
    pub status_code: u16,
    pub url: String,
    pub body: serde_json::Value,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialGitWriteInput {
    pub session_id: String,
    pub operation: String,
    pub repo: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureCredentialGitWriteResult {
    pub provider: String,
    pub operation: String,
    pub repo: String,
    pub status_code: u16,
    pub body: serde_json::Value,
}

/// 安全凭证模块中的本地 Git 工作区。只保存仓库路径和凭证引用，不保存任何 Git 密钥。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspace {
    pub id: i64,
    pub workspace_key: String,
    pub name: String,
    pub repo_path: String,
    pub credential_key: String,
    pub branch: String,
    pub remote_url: String,
    pub status: String,
    pub changed_files: i64,
    pub ahead: i64,
    pub behind: i64,
    pub description: String,
    pub last_scanned_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListGitWorkspacesInput {
    pub keyword: Option<String>,
    pub credential_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertGitWorkspaceInput {
    pub id: Option<i64>,
    pub workspace_key: String,
    pub name: String,
    pub repo_path: String,
    pub credential_key: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanGitWorkspaceRootInput {
    pub root_path: String,
    pub credential_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanGitWorkspaceRootResult {
    pub workspaces: Vec<GitWorkspace>,
    pub discovered: i64,
    pub scanned_entries: i64,
    pub skipped_entries: i64,
    pub limited: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceScanStartResult {
    pub job_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceScanJobStatus {
    pub job_id: String,
    pub status: String,
    pub message: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub result: Option<ScanGitWorkspaceRootResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceDetail {
    pub workspace: GitWorkspace,
    pub status_text: String,
    pub recent_log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCommitGitWorkspaceInput {
    pub workspace_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCommitGitWorkspaceResult {
    pub workspace: GitWorkspace,
    pub commit_message: String,
    pub commit_hash: String,
    pub provider_name: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceBranch {
    pub name: String,
    pub display_name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub last_commit_hash: String,
    pub last_commit_message: String,
    pub last_commit_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchGitWorkspaceBranchInput {
    pub workspace_key: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeGitWorkspaceBranchInput {
    pub workspace_key: String,
    pub source_branch: String,
    pub target_branch: String,
}

/// 数据库连接配置。敏感密码只返回掩码状态，不返回明文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConnection {
    pub key: String,
    pub name: String,
    pub group_name: String,
    pub db_type: String,
    pub connection_mode: String,
    pub host: String,
    pub port: i64,
    pub database_name: String,
    pub username: String,
    pub auth_type: String,
    pub credential_ref: String,
    pub password_masked: Option<String>,
    pub has_password: bool,
    pub ssh_server_alias: String,
    pub security_mode: String,
    pub ai_policy: String,
    pub page_size: i64,
    pub status: String,
    pub enabled: bool,
    pub last_connected_at: Option<String>,
    pub notes: String,
    pub updated_at: String,
}

/// 数据库连接创建/更新输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertDatabaseConnectionInput {
    pub key: String,
    pub name: String,
    pub group_name: String,
    pub db_type: String,
    pub connection_mode: String,
    pub host: String,
    pub port: i64,
    pub database_name: String,
    pub username: String,
    pub auth_type: String,
    pub credential_ref: String,
    pub password: Option<String>,
    pub clear_password: Option<bool>,
    pub ssh_server_alias: String,
    pub security_mode: String,
    pub ai_policy: String,
    pub page_size: i64,
    pub status: Option<String>,
    pub enabled: bool,
    pub notes: String,
}

/// 数据库连接测试结果。当前阶段测试目标 TCP 端点可达性。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConnectionTestResult {
    pub ok: bool,
    pub connection_key: String,
    pub endpoint: String,
    pub latency_ms: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseQueryInput {
    pub connection_key: String,
    pub database_name: Option<String>,
    pub sql: String,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Value>,
    pub row_count: i64,
    pub rows_affected: i64,
    pub page: i64,
    pub page_size: i64,
    pub duration_ms: i64,
    pub truncated: bool,
    pub statement_type: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseNameListInput {
    pub connection_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseNameListResult {
    pub connection_key: String,
    pub databases: Vec<String>,
    pub current: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSchemaInput {
    pub connection_key: String,
    pub database_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseTableSchema {
    pub name: String,
    pub schema_name: Option<String>,
    pub object_type: String,
    pub columns: Vec<String>,
    pub column_details: Vec<DatabaseColumnSchema>,
    pub indexes: Vec<DatabaseIndexSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseColumnSchema {
    pub name: String,
    pub data_type: String,
    pub column_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub extra: String,
    pub ordinal_position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseIndexSchema {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSchemaResult {
    pub connection_key: String,
    pub database_name: Option<String>,
    pub tables: Vec<DatabaseTableSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseExportInput {
    pub connection_key: String,
    pub database_name: Option<String>,
    pub mode: String,
    pub table_name: Option<String>,
    pub sql: Option<String>,
    pub include_data: Option<bool>,
    pub max_rows: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseExportResult {
    pub file_name: String,
    pub file_path: String,
    pub row_count: i64,
    pub table_count: i64,
    pub mode: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisScanInput {
    pub connection_key: String,
    pub database_name: Option<String>,
    pub pattern: Option<String>,
    pub cursor: Option<u64>,
    pub count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDescribeKeysInput {
    pub connection_key: String,
    pub database_name: Option<String>,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyEntry {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisScanResult {
    pub cursor: u64,
    pub keys: Vec<RedisKeyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDatabaseListInput {
    pub connection_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDatabaseInfo {
    pub name: String,
    pub index: u8,
    pub key_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisDatabaseListResult {
    pub connection_key: String,
    pub current: String,
    pub databases: Vec<RedisDatabaseInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyTreeInput {
    pub connection_key: String,
    pub database_name: Option<String>,
    pub pattern: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisKeyTreeResult {
    pub connection_key: String,
    pub database_name: Option<String>,
    pub pattern: String,
    pub keys: Vec<String>,
    pub total_scanned: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisValuePreviewInput {
    pub connection_key: String,
    pub database_name: Option<String>,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisValuePreview {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub preview: serde_json::Value,
}

/// AI/Agent 操作审批请求。用于把需要人工确认的远程命令、文件写入等动作落到本地队列。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: i64,
    pub source: String,
    pub requester: String,
    pub server_alias: String,
    pub action: String,
    pub risk: String,
    pub status: String,
    pub command: String,
    pub resource: String,
    pub reason: String,
    pub summary: String,
    pub payload_json: String,
    pub decision_note: String,
    pub decided_by: String,
    pub decided_at: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建审批请求输入。payload_json 保留原始上下文，后续执行类工具可复用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApprovalRequestInput {
    pub source: String,
    pub requester: String,
    pub server_alias: String,
    pub action: String,
    pub risk: String,
    pub command: String,
    pub resource: String,
    pub reason: String,
    pub summary: String,
    pub payload_json: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideApprovalRequestInput {
    pub id: i64,
    pub decision: String,
    pub note: String,
    pub decided_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListApprovalRequestsInput {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// 堡垒机 Web SSH 会话入口。只保存合规入口和引用信息，不保存或提取真实登录凭据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JumpServerSession {
    pub key: String,
    pub name: String,
    pub endpoint: String,
    pub web_url: String,
    pub session_ref: String,
    pub group_name: String,
    pub account_hint: String,
    pub asset_hint: String,
    pub protocol: String,
    pub ai_mode: String,
    pub status: String,
    pub notes: String,
    pub enabled: bool,
    pub last_opened_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertJumpServerSessionInput {
    pub key: String,
    pub name: String,
    pub endpoint: String,
    pub web_url: String,
    pub session_ref: String,
    pub group_name: String,
    pub account_hint: String,
    pub asset_hint: String,
    pub protocol: String,
    pub ai_mode: String,
    pub status: Option<String>,
    pub notes: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JumpServerOpenResult {
    pub key: String,
    pub web_url: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLog {
    pub id: i64,
    pub occurred_at: String,
    pub actor: String,
    pub source: String,
    pub server_alias: String,
    pub action: String,
    pub risk: String,
    pub result: String,
    pub summary: String,
    pub detail_json: String,
    pub request_id: String,
    pub approval_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAuditLogsInput {
    pub actor: Option<String>,
    pub source: Option<String>,
    pub server_alias: Option<String>,
    pub action: Option<String>,
    pub risk: Option<String>,
    pub result: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAuditLogInput {
    pub actor: String,
    pub source: String,
    pub server_alias: String,
    pub action: String,
    pub risk: String,
    pub result: String,
    pub summary: String,
    pub detail_json: Option<String>,
    pub request_id: Option<String>,
    pub approval_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogExportResult {
    pub file_name: String,
    pub content: String,
    pub count: usize,
}

/// 资源监控目标，复用服务器别名或数据库连接 Key。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMonitorTarget {
    pub id: Option<i64>,
    pub target_type: String,
    pub target_key: String,
    pub display_name: String,
    pub group_name: String,
    pub enabled: bool,
    pub collect_interval_sec: i64,
    pub last_status: String,
    pub last_collected_at: Option<String>,
    pub last_error: Option<String>,
    pub latest_snapshot: Option<ResourceMetricSnapshot>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertResourceMonitorTargetInput {
    pub target_type: String,
    pub target_key: String,
    pub display_name: Option<String>,
    pub enabled: Option<bool>,
    pub collect_interval_sec: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetricSnapshot {
    pub id: i64,
    pub target_type: String,
    pub target_key: String,
    pub status: String,
    pub collected_at: String,
    pub duration_ms: i64,
    pub summary: serde_json::Value,
    pub metrics: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSnapshotListInput {
    pub target_type: Option<String>,
    pub target_key: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectResourceBatchInput {
    pub target_type: Option<String>,
    pub only_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectResourceBatchResult {
    pub total: i64,
    pub success: i64,
    pub failed: i64,
    pub snapshots: Vec<ResourceMetricSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMonitorOverview {
    pub total_targets: i64,
    pub enabled_targets: i64,
    pub healthy_targets: i64,
    pub warning_targets: i64,
    pub failed_targets: i64,
    pub open_alerts: i64,
    pub latest_collected_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAlertRule {
    pub id: i64,
    pub target_type: String,
    pub target_key: String,
    pub metric_key: String,
    pub operator: String,
    pub threshold_value: f64,
    pub severity: String,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertResourceAlertRuleInput {
    pub id: Option<i64>,
    pub target_type: String,
    pub target_key: Option<String>,
    pub metric_key: String,
    pub operator: String,
    pub threshold_value: f64,
    pub severity: String,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourceAlertRulesInput {
    pub target_type: Option<String>,
    pub target_key: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAlertEvent {
    pub id: i64,
    pub rule_id: i64,
    pub target_type: String,
    pub target_key: String,
    pub severity: String,
    pub status: String,
    pub metric_key: String,
    pub metric_value: f64,
    pub threshold_value: f64,
    pub message: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub resolved_at: Option<String>,
    pub snapshot_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourceAlertEventsInput {
    pub status: Option<String>,
    pub target_type: Option<String>,
    pub target_key: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    pub theme: String,
    pub auto_update: bool,
    pub launch_on_startup: bool,
    pub audit_retention_days: i64,
    pub log_level: String,
    pub backup_dir: String,
    pub database_download_dir: String,
    pub platform: String,
    pub close_behavior: String,
    pub language: String,
    pub ai_unrestricted_until: Option<String>,
    pub dangerous_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSystemSettingsInput {
    pub theme: String,
    pub auto_update: bool,
    pub launch_on_startup: bool,
    pub audit_retention_days: i64,
    pub log_level: String,
    pub backup_dir: String,
    pub database_download_dir: String,
    pub platform: String,
    pub close_behavior: String,
    pub language: String,
    pub ai_unrestricted_until: Option<String>,
    pub dangerous_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiUnrestrictedState {
    pub active: bool,
    pub until: Option<String>,
    pub remaining_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableAiUnrestrictedInput {
    pub minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettingsExportResult {
    pub file_name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandInput {
    pub server_alias: String,
    pub command: String,
    pub timeout_secs: Option<u64>,
    pub initiated_by_ai: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandResult {
    pub server_alias: String,
    pub command: String,
    pub exit_status: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: i64,
    pub blocked: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionStartInput {
    pub server_alias: String,
    pub cols: Option<u32>,
    pub rows: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionStartResult {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionWriteInput {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionResizeInput {
    pub session_id: String,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionCloseInput {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionEvent {
    pub session_id: String,
    pub kind: String,
    pub data: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpListInput {
    pub server_alias: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpFileEntry {
    pub name: String,
    pub path: String,
    pub parent: String,
    pub file_type: String,
    pub size: u64,
    pub modified_at: Option<i64>,
    pub permissions: String,
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpListResult {
    pub server_alias: String,
    pub path: String,
    pub parent: String,
    pub entries: Vec<SftpFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpReadTextInput {
    pub server_alias: String,
    pub path: String,
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpReadTextResult {
    pub server_alias: String,
    pub path: String,
    pub content: String,
    pub size: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpWriteTextInput {
    pub server_alias: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpTransferPathInput {
    pub server_alias: String,
    pub remote_path: String,
    pub local_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpCreateDirectoryInput {
    pub server_alias: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpCreateFileInput {
    pub server_alias: String,
    pub path: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpRenameInput {
    pub server_alias: String,
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpDeleteInput {
    pub server_alias: String,
    pub path: String,
    pub file_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpOperationResult {
    pub ok: bool,
    pub server_alias: String,
    pub path: String,
    pub message: String,
    pub bytes: Option<u64>,
}

/// MCP Server 端点状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    pub server_name: String,
    pub streamable_http_url: String,
    pub stdio_command: String,
    pub stdio_args: Vec<String>,
    pub local_only: bool,
    pub http_reachable: bool,
    pub platform: String,
    pub notes: Vec<String>,
}

/// 可自动写入配置的 Agent 客户端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpClientConfig {
    pub key: String,
    pub name: String,
    pub vendor: String,
    pub description: String,
    pub config_path: String,
    pub scope: String,
    pub transport: String,
    pub status: String,
    pub installed: bool,
    pub configured: bool,
    pub last_configured_at: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolPermission {
    pub tool: String,
    pub policy: String,
    pub audit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpManualSnippet {
    pub title: String,
    pub transport: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOverview {
    pub status: McpServerStatus,
    pub clients: Vec<McpClientConfig>,
    pub tools: Vec<McpToolPermission>,
    pub snippets: Vec<McpManualSnippet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureMcpClientInput {
    pub client_key: String,
    pub transport: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureMcpClientResult {
    pub client: McpClientConfig,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub message: String,
    pub snippet: String,
}

/// 自动部署目标。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentTarget {
    pub id: i64,
    pub target_key: String,
    pub name: String,
    pub server_alias: String,
    pub recipe: String,
    pub source_type: String,
    pub project_path: String,
    pub git_url: String,
    pub git_ref: String,
    pub git_credential_key: String,
    pub docker_build_mode: String,
    pub workdir: String,
    pub deploy_root: String,
    pub domain: String,
    pub https_enabled: bool,
    pub port: Option<i64>,
    pub health_check_url: String,
    pub config_json: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 自动部署目标创建/更新输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertDeploymentTargetInput {
    pub id: Option<i64>,
    pub target_key: String,
    pub name: String,
    pub server_alias: String,
    pub recipe: String,
    pub source_type: String,
    pub project_path: Option<String>,
    pub git_url: Option<String>,
    pub git_ref: Option<String>,
    pub git_credential_key: Option<String>,
    pub docker_build_mode: Option<String>,
    pub workdir: Option<String>,
    pub deploy_root: Option<String>,
    pub domain: Option<String>,
    pub https_enabled: Option<bool>,
    pub port: Option<i64>,
    pub health_check_url: Option<String>,
    pub config_json: Option<String>,
    pub enabled: Option<bool>,
}

/// 自动部署组。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentGroup {
    pub id: i64,
    pub group_key: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub targets: Vec<DeploymentGroupTarget>,
    pub created_at: String,
    pub updated_at: String,
}

/// 部署组内目标排序项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentGroupTarget {
    pub target_key: String,
    pub target_name: String,
    pub sort_order: i64,
    pub enabled: bool,
}

/// 自动部署组创建/更新输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertDeploymentGroupInput {
    pub id: Option<i64>,
    pub group_key: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub targets: Vec<DeploymentGroupTargetInput>,
}

/// 部署组目标输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentGroupTargetInput {
    pub target_key: String,
    pub sort_order: Option<i64>,
    pub enabled: Option<bool>,
}

/// 内置部署配方。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentTemplate {
    pub key: String,
    pub name: String,
    pub description: String,
    pub scenario: String,
    pub risk: String,
    pub supported_sources: Vec<String>,
    pub required_profiles: Vec<String>,
}

/// 部署环境方案。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentEnvironmentProfile {
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub checks: Vec<String>,
}

/// 自动部署镜像商店应用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentImageStoreApp {
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub image: String,
    pub tag: String,
    pub default_port: Option<i64>,
    pub container_port: Option<i64>,
    pub volume_path: String,
    pub env: Vec<DeploymentImageStoreEnv>,
    pub notes: Vec<String>,
}

/// 镜像商店应用环境变量模板。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentImageStoreEnv {
    pub key: String,
    pub label: String,
    pub default_value: String,
    pub required: bool,
    pub secret: bool,
}

/// 一键安装镜像商店应用输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallImageStoreAppInput {
    pub app_key: String,
    pub target_key: String,
    pub name: String,
    pub server_alias: String,
    pub port: Option<i64>,
    pub deploy_root: Option<String>,
    pub image_tag: Option<String>,
    pub env_json: Option<String>,
    pub enabled: Option<bool>,
}

/// 自动部署项目检测输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectDeploymentProjectInput {
    pub source_type: String,
    pub project_path: Option<String>,
    pub git_url: Option<String>,
    pub git_ref: Option<String>,
    pub git_credential_key: Option<String>,
}

/// 自动部署项目检测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentDetectionResult {
    pub source_type: String,
    pub project_root: String,
    pub git_url: String,
    pub git_ref: String,
    pub commit: String,
    pub candidates: Vec<DeploymentCandidate>,
    pub warnings: Vec<String>,
}

/// 检测出的候选部署目标。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentCandidate {
    pub key: String,
    pub name: String,
    pub recipe: String,
    pub confidence: i64,
    pub source_type: String,
    pub workdir: String,
    pub build_command: String,
    pub start_command: String,
    pub artifact_dir: String,
    pub dockerfile: String,
    pub compose_file: String,
    pub exposed_ports: Vec<i64>,
    pub env_files: Vec<String>,
    pub detected_frameworks: Vec<String>,
    pub warnings: Vec<String>,
    pub config_json: String,
}

/// 自动部署 dry-run 输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDeploymentDryRunInput {
    pub target_key: Option<String>,
    pub group_key: Option<String>,
}

/// 自动部署 dry-run 计划。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentPlan {
    pub plan_id: String,
    pub target_key: String,
    pub group_key: String,
    pub title: String,
    pub recipe: String,
    pub server_alias: String,
    pub status: String,
    pub risk: String,
    pub approval_required: bool,
    pub environment: DeploymentEnvironmentProbe,
    pub stages: Vec<DeploymentPlanStage>,
    pub warnings: Vec<String>,
    pub created_at: String,
}

/// 自动部署 dry-run 环境探测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentEnvironmentProbe {
    pub server_alias: String,
    pub os: String,
    pub arch: String,
    pub user: String,
    pub disk_available_kb: Option<i64>,
    pub docker_version: String,
    pub compose_version: String,
    pub nginx_version: String,
    pub openresty_version: String,
    pub git_version: String,
    pub port_available: Option<bool>,
    pub domain_resolved: Option<bool>,
    pub checks: Vec<DeploymentProbeCheck>,
    pub raw_output: String,
}

/// 自动部署环境探测单项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentProbeCheck {
    pub key: String,
    pub label: String,
    pub status: String,
    pub message: String,
}

/// 自动部署 dry-run 阶段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentPlanStage {
    pub key: String,
    pub title: String,
    pub risk: String,
    pub approval_required: bool,
    pub command_preview: String,
    pub summary: String,
    pub status: String,
}

/// 自动部署执行输入。默认按 target/group 重新生成 dry-run 后执行；continueRunId 用于继续已审批步骤。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteDeploymentRunInput {
    pub target_key: Option<String>,
    pub group_key: Option<String>,
    pub plan_id: Option<String>,
    pub continue_run_id: Option<String>,
    pub created_by: Option<String>,
}

/// 自动部署回滚 dry-run 输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDeploymentRollbackDryRunInput {
    pub target_key: String,
    pub run_id: Option<String>,
}

/// 自动部署回滚执行输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteDeploymentRollbackInput {
    pub target_key: String,
    pub run_id: Option<String>,
    pub created_by: Option<String>,
}

/// 自动部署运行记录过滤。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDeploymentRunsInput {
    pub target_key: Option<String>,
    pub group_key: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// 自动部署运行记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentRun {
    pub id: i64,
    pub run_id: String,
    pub target_key: String,
    pub group_key: String,
    pub status: String,
    pub version_label: String,
    pub summary: String,
    pub plan_json: String,
    pub created_by: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
}

/// 自动部署运行步骤。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentRunStep {
    pub id: i64,
    pub run_id: String,
    pub step_key: String,
    pub title: String,
    pub status: String,
    pub command_preview: String,
    pub stdout_preview: String,
    pub stderr_preview: String,
    pub exit_code: Option<i64>,
    pub approval_id: Option<i64>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
}

/// 自动部署运行详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentRunDetail {
    pub run: DeploymentRun,
    pub steps: Vec<DeploymentRunStep>,
}

/// 部署 AI 建议输入。可直接传入当前 dry-run 计划，或指定 target/group 由后端重新生成计划。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAiAdviceInput {
    pub target_key: Option<String>,
    pub group_key: Option<String>,
    pub plan: Option<DeploymentPlan>,
    pub prompt: Option<String>,
    pub provider_key: Option<String>,
}

/// 部署 AI 建议结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAiAdviceResult {
    pub provider_key: String,
    pub provider_name: String,
    pub model: String,
    pub answer: String,
    pub latency_ms: i64,
    pub generated_at: String,
}
