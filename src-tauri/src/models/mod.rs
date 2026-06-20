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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    pub theme: String,
    pub auto_update: bool,
    pub audit_retention_days: i64,
    pub log_level: String,
    pub backup_dir: String,
    pub platform: String,
    pub close_behavior: String,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSystemSettingsInput {
    pub theme: String,
    pub auto_update: bool,
    pub audit_retention_days: i64,
    pub log_level: String,
    pub backup_dir: String,
    pub platform: String,
    pub close_behavior: String,
    pub language: String,
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
