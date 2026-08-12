use serde::{Deserialize, Serialize};

/// 通用分页结果。数据库层使用 offset/limit，前端同时获得稳定总数。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePage<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProject {
    pub id: i64,
    pub project_key: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    /// 项目关联的已登记 Git 工作区。保留单值字段以兼容旧客户端与既有数据。
    pub git_workspace_keys: Vec<String>,
    pub git_workspace_key: String,
    pub default_branch: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRelease {
    pub id: i64,
    pub project_id: i64,
    pub version: String,
    pub tag_name: String,
    pub branch: String,
    pub commit_sha: String,
    pub description: String,
    pub released_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGitRef {
    pub ref_type: String,
    pub name: String,
    pub commit_sha: String,
    pub subject: String,
    pub committed_at: String,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSource {
    pub id: i64,
    pub source_key: String,
    pub project_id: Option<i64>,
    pub source_type: String,
    pub display_name: String,
    pub root_path: String,
    pub git_workspace_key: String,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub version_strategy: String,
    pub sync_mode: String,
    pub allow_remote_embedding: bool,
    pub enabled: bool,
    pub last_commit_sha: String,
    pub last_sync_status: String,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceScopeEntry {
    pub relative_path: String,
    pub entry_type: String,
    pub decision: String,
    pub reason: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceScopePreview {
    pub source_type: String,
    pub canonical_root: String,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub allow_remote_embedding: bool,
    pub included_files: i64,
    pub skipped_entries: i64,
    pub included_bytes: i64,
    pub truncated: bool,
    pub warnings: Vec<String>,
    pub entries: Vec<KnowledgeSourceScopeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncKnowledgeGitSourceInput {
    pub source_id: i64,
    pub release_id: Option<i64>,
    pub git_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncKnowledgeLocalSourceInput {
    pub source_id: i64,
    pub release_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartKnowledgeSourceSyncInput {
    pub source_id: i64,
    pub release_id: Option<i64>,
    pub git_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceSyncResult {
    pub source_id: i64,
    pub commit_sha: String,
    pub scanned_files: i64,
    pub created_versions: i64,
    pub unchanged_files: i64,
    pub deleted_paths: i64,
    pub skipped_files: i64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocument {
    pub id: i64,
    pub document_key: String,
    pub project_id: Option<i64>,
    pub source_id: Option<i64>,
    pub doc_type: String,
    pub title: String,
    pub logical_path: String,
    /// 仅上传任务返回来源文件夹；普通文档即使逻辑路径相似也保持为空。
    pub source_folder_name: Option<String>,
    pub status: String,
    pub sensitivity: String,
    pub tags: Vec<String>,
    pub latest_version_id: Option<i64>,
    pub allow_ai: bool,
    pub allow_mcp: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentVersion {
    pub id: i64,
    pub document_id: i64,
    pub release_id: Option<i64>,
    pub version_label: String,
    pub git_branch: String,
    pub commit_sha: String,
    pub source_path: String,
    pub mime_type: String,
    pub content: String,
    pub content_hash: String,
    pub parsed_meta: serde_json::Value,
    pub token_estimate: i64,
    pub valid: bool,
    pub created_at: String,
}

/// 文档详情的处理摘要只包含可直接展示的状态和下一步操作；任务检查点、原始错误和
/// 资产绝对路径始终留在受控后端边界，不能随详情接口输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentProcessingTaskSummary {
    pub id: i64,
    pub job_key: String,
    pub job_type: String,
    pub status: String,
    pub progress_current: i64,
    pub progress_total: i64,
    pub message: String,
    pub cancel_requested: bool,
}

/// 解析器质量与警告用于说明“部分成功”或“解析失败”，不返回解析器内部堆栈和原始文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentParseSummary {
    pub parser_id: String,
    pub parser_version: String,
    pub quality_level: String,
    pub warnings: Vec<String>,
}

/// 受控图片预览只返回可显示的副本与经二进制读取的元数据；受管目录、存储键和原始
/// 路径永不离开 Rust 侧。图片过大时调用方应显示元数据与可操作提示，而不是传输整份文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentImagePreview {
    pub document_id: i64,
    pub mime_type: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub data_url: String,
}

/// 文档处理状态与正文读取权限分离：处理中可查看原文件元数据和任务进度，但没有正文；
/// 部分成功可查看已解析结果并明确缺口；失败只显示安全摘要和可重试操作。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentProcessingSummary {
    pub status: String,
    pub message: String,
    pub failure_reason: Option<String>,
    pub content_available: bool,
    pub available_actions: Vec<String>,
    pub task: Option<KnowledgeDocumentProcessingTaskSummary>,
    pub parser: Option<KnowledgeDocumentParseSummary>,
}

/// 删除前的影响范围只用于展示和确认；永久删除默认关闭，不由本接口执行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentDeletionImpactPreview {
    pub document_id: i64,
    pub title: String,
    pub version_count: i64,
    pub chunk_count: i64,
    pub vector_count: i64,
    pub relation_count: i64,
    pub asset_count: i64,
    pub fts_entry_count: i64,
    pub permanent_deletion_enabled: bool,
    pub permanent_deletion_block_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreKnowledgeDocumentResult {
    pub document: KnowledgeDocument,
    pub rebuilt_fts_entries: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentDetail {
    pub document: KnowledgeDocument,
    pub versions: Vec<KnowledgeDocumentVersion>,
    pub processing: KnowledgeDocumentProcessingSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareKnowledgeDocumentVersionsInput {
    pub from_version_id: i64,
    pub to_version_id: i64,
}

/// 比较版本的解析产物签名只携带可追溯的哈希与解析器元数据，不返回原始资产路径或内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentComparisonArtifact {
    pub parser_id: String,
    pub parser_version: String,
    pub quality_level: String,
    pub normalized_hash: String,
    pub asset_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentComparison {
    pub from_version: KnowledgeDocumentVersion,
    pub to_version: KnowledgeDocumentVersion,
    /// 正文哈希变化即使只发生在尾随换行等不可见内容，也必须明确告知调用方。
    pub content_changed: bool,
    /// 原始资产哈希集合变化说明上传文件已变更，即使解析正文恰好相同也不能视为无变化。
    pub asset_changed: bool,
    /// 解析器标识或版本变化可能导致同一资产产生不同的结构化结果，单独展示避免误判。
    pub parser_changed: bool,
    pub unchanged: bool,
    pub common_prefix_lines: i64,
    pub common_suffix_lines: i64,
    pub removed_lines: Vec<String>,
    pub added_lines: Vec<String>,
    pub from_asset_hashes: Vec<String>,
    pub to_asset_hashes: Vec<String>,
    pub from_parse_artifacts: Vec<KnowledgeDocumentComparisonArtifact>,
    pub to_parse_artifacts: Vec<KnowledgeDocumentComparisonArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCitationDetail {
    pub citation: KnowledgeCitation,
    pub document: KnowledgeDocument,
    pub version: KnowledgeDocumentVersion,
    pub chunk: KnowledgeChunk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunk {
    pub id: i64,
    pub document_version_id: i64,
    pub chunk_index: i64,
    pub heading_path: String,
    pub content: String,
    pub content_hash: String,
    pub location: serde_json::Value,
    pub token_estimate: i64,
    pub embedding_status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingProfile {
    pub id: i64,
    pub profile_key: String,
    pub name: String,
    pub mode: String,
    pub provider_key: String,
    pub model: String,
    pub model_revision: String,
    pub dimension: i64,
    pub normalized: bool,
    pub config: serde_json::Value,
    pub fingerprint: String,
    pub status: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 独立 Profile 构建完成前的完整性校验结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingIndexValidation {
    pub profile_id: i64,
    pub profile_key: String,
    pub expected_chunks: i64,
    pub indexed_chunks: i64,
    pub stale_chunks: i64,
    pub dimension_mismatch_chunks: i64,
    /// 向量 BLOB、数值或持久化范数不合法的片段数。
    pub invalid_vector_chunks: i64,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingLifecycleResult {
    pub profile: KnowledgeEmbeddingProfile,
    pub validation: KnowledgeEmbeddingIndexValidation,
}

/// 真实短文本探测成功后才会持久化实际维度，Profile 仍保持 draft 直到独立索引构建完成。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingProfileTestResult {
    pub profile: KnowledgeEmbeddingProfile,
    pub dimension: i64,
    pub probe_text: String,
}

/// 一次可恢复的向量构建批次。调用方通过同一 jobKey 继续下一个批次；已匹配
/// content_hash/Profile 的片段会被跳过而不会重新请求模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildKnowledgeEmbeddingBatchInput {
    pub profile_id: i64,
    pub job_key: Option<String>,
    pub batch_size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingBatchResult {
    pub profile_id: i64,
    pub job_key: String,
    pub total_chunks: i64,
    pub processed_chunks: i64,
    pub embedded_chunks: i64,
    pub skipped_chunks: i64,
    pub blocked_chunks: i64,
    pub completed: bool,
    pub checkpoint: serde_json::Value,
}

/// 本地 Embedding 运行时及受控模型缓存状态；不包含模型正文或远程下载地址。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLocalEmbeddingRuntimeStatus {
    pub runtime: String,
    pub fastembed_feature_enabled: bool,
    /// 构建启用运行时且存在已校验模型缓存时为 true；首次实际使用仍会执行短文本探测。
    pub runtime_available: bool,
    pub automatic_download_enabled: bool,
    pub cache_dir: String,
    pub cache_exists: bool,
    pub cached_models: Vec<KnowledgeLocalEmbeddingCacheEntry>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLocalEmbeddingCacheEntry {
    pub model_key: String,
    pub size_bytes: i64,
    /// 导入完成时记录的稳定内容摘要，可用于后续离线运行前复核。
    pub sha256: String,
    pub imported_at: String,
}

/// 显式离线导入本地 Embedding 模型目录。调用方提供的目录只作为一次性读取源，
/// 导入后模型始终由应用数据目录托管。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportKnowledgeLocalEmbeddingModelInput {
    pub model_key: String,
    pub source_path: String,
    pub expected_sha256: String,
}

/// 本地模型导入结果不包含源文件路径、模型正文或远程地址。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLocalEmbeddingModelImportResult {
    pub model_key: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub imported_at: String,
}

/// 删除由知识库托管的离线模型缓存；活动模型是否可用由后续真实运行时探测决定。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveKnowledgeLocalEmbeddingModelInput {
    pub model_key: String,
}

/// 内部镜像下载只接受模型标识；镜像根地址由受控应用设置保存，避免前端传入任意 URL。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadKnowledgeLocalEmbeddingModelInput {
    pub model_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLocalEmbeddingDownloadProgress {
    pub stage: String,
    pub model_key: String,
    pub files_completed: i64,
    pub files_total: i64,
    pub bytes_downloaded: i64,
    pub total_bytes: i64,
}

/// 本地向量请求采用显式前缀，调用方不能通过空前缀混淆 query/document 语义。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateKnowledgeLocalEmbeddingsInput {
    pub model_key: String,
    pub texts: Vec<String>,
    pub prefix: String,
    pub batch_size: Option<i64>,
}

/// 在切换 Embedding Profile 前展示的重建工作量。
///
/// 所有数值均为聚合结果；接口不向前端返回任何待向量化的知识正文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingRebuildEstimate {
    pub target_profile_id: i64,
    pub target_profile_key: String,
    pub target_mode: String,
    pub target_dimension: i64,
    pub affected_documents: i64,
    pub affected_chunks: i64,
    pub reusable_chunks: i64,
    pub chunks_to_embed: i64,
    pub local_work_chunks: i64,
    pub remote_eligible_chunks: i64,
    pub remote_characters: i64,
    pub remote_blocked_chunks: i64,
    pub estimated_index_bytes: i64,
    pub additional_disk_bytes: i64,
    pub requires_remote_confirmation: bool,
    pub remote_sources: Vec<KnowledgeRemoteRebuildSourceEstimate>,
    pub current_index: Option<KnowledgeEmbeddingIndexAvailability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRemoteRebuildSourceEstimate {
    pub source_id: Option<i64>,
    pub source_key: String,
    pub display_name: String,
    pub eligible_chunks: i64,
    pub eligible_characters: i64,
    pub blocked_chunks: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingIndexAvailability {
    pub profile_id: i64,
    pub profile_key: String,
    pub total_chunks: i64,
    pub indexed_chunks: i64,
    pub missing_chunks: i64,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateKnowledgeEmbeddingRebuildInput {
    pub profile_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunkEmbedding {
    pub chunk_id: i64,
    pub profile_id: i64,
    pub dimension: i64,
    pub vector_norm: f64,
    pub content_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFtsCapability {
    pub fts5_available: bool,
    pub trigram_available: bool,
    pub active_tokenizer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRelation {
    pub id: i64,
    pub project_id: Option<i64>,
    pub release_id: Option<i64>,
    pub document_version_id: Option<i64>,
    pub snapshot_id: Option<i64>,
    pub sensitivity: String,
    /// `needs_rebuild` 表示来自旧版无归属关系表，默认隔离且不得参与关系召回。
    pub scope_status: String,
    pub from_type: String,
    pub from_key: String,
    pub relation_type: String,
    pub to_type: String,
    pub to_key: String,
    pub evidence: serde_json::Value,
    pub confidence: f64,
    pub confirmed: bool,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// 人工确认、来源导入和 AI 建议统一使用同一关系写入契约。`confirmed = false`
/// 的记录只能作为候选展示，不能被检索排序当作已证实事实。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKnowledgeRelationInput {
    pub id: Option<i64>,
    pub project_id: Option<i64>,
    pub release_id: Option<i64>,
    pub document_version_id: Option<i64>,
    pub snapshot_id: Option<i64>,
    pub sensitivity: String,
    pub from_type: String,
    pub from_key: String,
    pub relation_type: String,
    pub to_type: String,
    pub to_key: String,
    pub evidence: serde_json::Value,
    pub confidence: f64,
    pub confirmed: bool,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListKnowledgeRelationsInput {
    pub entity_type: Option<String>,
    pub entity_key: Option<String>,
    pub project_ids: Vec<i64>,
    pub release_ids: Vec<i64>,
    pub sensitivities: Vec<String>,
    pub confirmed_only: Option<bool>,
    pub limit: Option<i64>,
}

/// 从已解析 Markdown front matter 中导入显式关系；不接受正文语义猜测。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportKnowledgeDocumentRelationsInput {
    pub document_version_id: i64,
}

/// Commit message 仅按可配置的显式 Story/Task/Bug/Test 标识建立候选关系，不对自然语言
/// 描述做推断。调用者需提供已解析的 Commit SHA 与原始消息。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportKnowledgeCommitRelationsInput {
    pub commit_sha: String,
    pub commit_message: String,
    /// 用户为当前团队约定显式选择的标识前缀，例如 `story`、`task`；未设置时采用兼容默认值。
    pub entity_prefixes: Option<Vec<String>>,
    pub confirmed: Option<bool>,
    /// 可选的代码快照范围。提供后 Commit 必须与快照一致，且生成的关系会继承项目、
    /// 发布版本和快照边界，避免不同项目中相同的禅道编号发生串用。
    pub snapshot_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeJob {
    pub id: i64,
    pub job_key: String,
    pub job_type: String,
    pub source_id: Option<i64>,
    pub profile_id: Option<i64>,
    pub status: String,
    pub progress_current: i64,
    pub progress_total: i64,
    pub message: String,
    pub error: Option<String>,
    pub checkpoint: serde_json::Value,
    pub heartbeat_at: Option<String>,
    pub cancel_requested: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateKnowledgeJobInput {
    pub job_key: String,
    pub job_type: String,
    pub source_id: Option<i64>,
    pub profile_id: Option<i64>,
    pub message: String,
    pub checkpoint: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGenerationRun {
    pub id: i64,
    pub run_key: String,
    pub project_id: i64,
    pub release_id: Option<i64>,
    pub source_id: Option<i64>,
    pub sync_job_id: Option<i64>,
    pub template_version: String,
    pub document_types: Vec<String>,
    pub input_hash: String,
    pub status: String,
    pub generated_count: i64,
    pub skipped_count: i64,
    pub ai_summary_enabled: bool,
    pub ai_provider_key: String,
    pub ai_model: String,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoConnection {
    pub id: i64,
    pub connection_key: String,
    pub name: String,
    pub base_url: String,
    pub api_version: String,
    pub auth_mode: String,
    pub endpoint_profile: String,
    /// 仅后端用于向安全凭据存储解析引用；绝不序列化到前端、Dev API 或 MCP 响应。
    #[serde(skip_serializing)]
    pub credential_key: String,
    pub credential_configured: bool,
    pub tls_verify: bool,
    /// HTTP 只在用户逐连接明确确认风险后允许使用；默认保持 HTTPS。
    pub allow_insecure_http: bool,
    pub request_timeout_seconds: i64,
    pub page_size: i64,
    pub rate_limit_per_second: f64,
    pub capabilities: serde_json::Value,
    pub enabled: bool,
    pub last_test_status: String,
    pub last_tested_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::UpsertZentaoConnectionInput;

    #[test]
    fn zentao_connection_input_requires_credential_key() {
        let input = serde_json::from_value::<UpsertZentaoConnectionInput>(serde_json::json!({
            "connectionKey": "zentao-session",
            "name": "只读禅道连接",
            "baseUrl": "https://zentao.example.test/zentao/",
            "apiVersion": "auto",
            "authMode": "bearer",
            "endpointProfile": "",
            "tlsVerify": true,
            "allowInsecureHttp": false,
            "requestTimeoutSeconds": 30,
            "pageSize": 100,
            "rateLimitPerSecond": 5.0,
            "enabled": true
        }));

        assert!(input.is_err());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoProjectMapping {
    pub id: i64,
    pub connection_id: i64,
    pub knowledge_project_id: i64,
    pub remote_product_id: String,
    pub remote_project_id: String,
    pub remote_execution_ids: Vec<String>,
    pub release_mapping: serde_json::Value,
    pub sync_scope: serde_json::Value,
    pub sync_since: Option<String>,
    pub include_comments: bool,
    pub include_worklogs: bool,
    pub include_attachment_metadata: bool,
    pub allow_remote_embedding: bool,
    pub allow_remote_ai: bool,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoSyncCursor {
    pub id: i64,
    pub mapping_id: i64,
    pub entity_type: String,
    pub last_updated_at: String,
    pub last_external_id: String,
    pub checkpoint: serde_json::Value,
    pub last_success_at: Option<String>,
    pub last_full_sync_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoEntity {
    pub id: i64,
    pub connection_id: i64,
    pub mapping_id: i64,
    pub knowledge_project_id: i64,
    pub release_id: Option<i64>,
    pub entity_type: String,
    pub external_id: String,
    pub external_key: String,
    pub title: String,
    pub body_markdown: String,
    pub original_status: String,
    pub normalized_status: String,
    pub assignee_external_id: String,
    pub parent_external_key: String,
    pub remote_url: String,
    pub content_hash: String,
    pub raw_json_hash: String,
    pub raw_snapshot: Option<serde_json::Value>,
    pub source_created_at: Option<String>,
    pub source_updated_at: Option<String>,
    pub first_synced_at: String,
    pub last_synced_at: String,
    pub missing_count: i64,
    pub status: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoEntityRelation {
    pub id: i64,
    pub from_external_key: String,
    pub relation_type: String,
    pub to_external_key: String,
    pub evidence: serde_json::Value,
    pub source: String,
    pub confidence: f64,
    pub confirmed: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// 已同步禅道实体间的显式关系写入。关系必须携带来源字段证据，禁止由语义猜测生成。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertZentaoEntityRelationInput {
    pub from_external_key: String,
    pub relation_type: String,
    pub to_external_key: String,
    pub evidence: serde_json::Value,
    pub source: String,
    pub confidence: f64,
    pub confirmed: bool,
}

/// 适配器归一化后的禅道事实。正文与 raw_snapshot 仅能来自已授权的只读接口；调用方
/// 不可自行传入远程 URL 或凭据，也不得将附件正文混入此对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertZentaoEntityInput {
    pub connection_id: i64,
    pub mapping_id: i64,
    pub knowledge_project_id: i64,
    pub release_id: Option<i64>,
    pub entity_type: String,
    pub external_id: String,
    pub external_key: String,
    pub title: String,
    pub body_markdown: String,
    pub original_status: String,
    pub normalized_status: String,
    pub assignee_external_id: String,
    pub parent_external_key: String,
    pub remote_url: String,
    pub content_hash: String,
    pub raw_json_hash: String,
    pub raw_snapshot: Option<serde_json::Value>,
    pub source_created_at: Option<String>,
    pub source_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoSyncCursorUpdateInput {
    pub mapping_id: i64,
    pub entity_type: String,
    pub last_updated_at: String,
    pub last_external_id: String,
    pub checkpoint: serde_json::Value,
    pub completed_full_sync: bool,
}

/// 仅同步已探测能力与用户显式映射范围内的只读禅道实体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncZentaoMappingInput {
    pub mapping_id: i64,
    pub entity_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoSyncResult {
    pub mapping_id: i64,
    pub entity_type: String,
    pub fetched_count: i64,
    pub changed_count: i64,
    pub unchanged_count: i64,
    pub missing_confirmed_count: i64,
    pub cursor: ZentaoSyncCursor,
}

/// 只从已同步、规范化的禅道实体生成确定性事实文档。AI 摘要另走显式流程，不能混入本输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateZentaoKnowledgeDocumentsInput {
    pub mapping_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateZentaoKnowledgeDocumentsResult {
    pub mapping_id: i64,
    pub source_id: i64,
    pub generated_document_version_ids: Vec<i64>,
    pub entity_count: i64,
}

/// AI 摘要只允许建立在已经生成的禅道事实文档之上；Provider 与模型必须显式指定，
/// 便于审计和后续复核，绝不写回事实区域。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateZentaoAiSummaryInput {
    pub mapping_id: i64,
    pub provider_key: String,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateZentaoAiSummaryResult {
    pub mapping_id: i64,
    pub document_version_id: i64,
    pub citation_count: i64,
    pub provider_key: String,
    pub model: String,
}

/// 将既有 AI 经验以只读快照方式投影到统一知识管道，不改变原经验记录。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportKnowledgeExperiencesInput {
    pub project_id: Option<i64>,
    pub release_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportKnowledgeExperiencesResult {
    pub source_id: i64,
    pub scanned_count: i64,
    pub imported_count: i64,
    pub unchanged_count: i64,
    /// 命中秘密规则的经验只保存元数据、内容哈希与跳过原因。
    pub restricted_count: i64,
    pub generated_document_version_ids: Vec<i64>,
}

/// 从已完成静态分析的不可变代码快照生成确定性工程报告；不读取工作树、远端服务或模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateKnowledgeCodeDocumentsInput {
    pub snapshot_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateKnowledgeCodeDocumentsResult {
    pub snapshot_id: i64,
    pub source_id: i64,
    pub generated_document_version_ids: Vec<i64>,
    pub file_count: i64,
    pub symbol_count: i64,
    pub relation_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchKnowledgeCodeSymbolsInput {
    pub snapshot_id: i64,
    pub keyword: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeCallGraphInput {
    pub snapshot_id: i64,
    pub symbol_key: String,
    pub max_depth: Option<i64>,
    pub include_unconfirmed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeCallGraph {
    pub snapshot_id: i64,
    pub root_symbol_key: String,
    pub nodes: Vec<KnowledgeCodeSymbol>,
    pub edges: Vec<KnowledgeCodeRelation>,
    pub max_depth: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareKnowledgeCodeSnapshotsInput {
    pub from_snapshot_id: i64,
    pub to_snapshot_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeFileChange {
    pub change_type: String,
    pub from_path: String,
    pub to_path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeSnapshotComparison {
    pub from_snapshot: KnowledgeCodeSnapshot,
    pub to_snapshot: KnowledgeCodeSnapshot,
    pub file_changes: Vec<KnowledgeCodeFileChange>,
    pub added_symbol_keys: Vec<String>,
    pub removed_symbol_keys: Vec<String>,
    pub retained_symbol_keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeKnowledgeCodeImpactInput {
    pub snapshot_id: i64,
    pub symbol_keys: Vec<String>,
    pub max_depth: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeSnapshot {
    pub id: i64,
    pub snapshot_key: String,
    pub source_id: i64,
    pub project_id: Option<i64>,
    pub release_id: Option<i64>,
    pub snapshot_type: String,
    pub ref_name: String,
    pub commit_sha: String,
    pub base_commit_sha: String,
    pub branch_name: String,
    pub worktree_dirty: bool,
    /// 仅 dirty 工作树快照使用：保存状态集合和已授权文件的内容哈希，绝不保存源码正文。
    pub dirty_state: serde_json::Value,
    pub captured_at: String,
    pub file_count: i64,
    pub symbol_count: i64,
    pub relation_count: i64,
    pub analyzer_version: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateKnowledgeCodeSnapshotInput {
    pub snapshot_key: String,
    pub source_id: i64,
    pub project_id: Option<i64>,
    pub release_id: Option<i64>,
    pub snapshot_type: String,
    pub ref_name: String,
    pub commit_sha: String,
    pub base_commit_sha: String,
    pub branch_name: String,
    pub worktree_dirty: bool,
    pub dirty_state: serde_json::Value,
    pub captured_at: String,
    pub file_count: i64,
    pub analyzer_version: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureKnowledgeGitSnapshotInput {
    pub source_id: i64,
    pub git_ref: String,
    pub release_id: Option<i64>,
}

/// 采集当前 Git 工作树的隔离快照。该快照始终是本地观察结果，不能当作发布版本事实。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureKnowledgeDirtyWorktreeSnapshotInput {
    pub source_id: i64,
    pub release_id: Option<i64>,
}

/// 采集已授权非 Git 本地目录的当前内容哈希；该快照没有历史 Git 语义。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureKnowledgeLocalDirectorySnapshotInput {
    pub source_id: i64,
    pub release_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeFile {
    pub id: i64,
    pub snapshot_id: i64,
    pub document_version_id: Option<i64>,
    pub relative_path: String,
    pub language: String,
    pub file_size: i64,
    pub content_hash: String,
    pub analysis_level: String,
    pub is_generated: bool,
    pub is_test: bool,
    pub sensitivity: String,
    pub status: String,
    pub skip_reason: String,
    pub created_at: String,
}

/// 已授权快照中的只读源码视图。正文始终来自该快照关联的不可变文档版本，调用方不能
/// 传入任意本地路径，也不能绕过快照、敏感级别和文档有效性校验读取工作区文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeFileContent {
    pub file: KnowledgeCodeFile,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeSymbol {
    pub id: i64,
    pub snapshot_id: i64,
    pub file_id: i64,
    pub symbol_key: String,
    pub symbol_kind: String,
    pub name: String,
    pub qualified_name: String,
    pub signature: String,
    pub visibility: String,
    pub parent_symbol_key: String,
    pub start_line: i64,
    pub start_column: i64,
    pub end_line: i64,
    pub end_column: i64,
    pub doc_comment: String,
    pub content_hash: String,
    pub analysis_level: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeRelation {
    pub id: i64,
    pub snapshot_id: i64,
    pub from_symbol_key: String,
    pub relation_type: String,
    pub to_symbol_key: String,
    pub to_external_type: String,
    pub to_external_key: String,
    pub evidence_file_id: Option<i64>,
    pub evidence_start_line: Option<i64>,
    pub evidence_end_line: Option<i64>,
    pub evidence_text: String,
    pub resolver: String,
    pub confidence: f64,
    pub confirmed: bool,
    pub created_at: String,
}

/// 代码文件写入仅发生在后端的快照分析流程中。正文仍由普通文档版本表保存，避免
/// 在代码索引表重复持久化内容。
#[derive(Debug, Clone)]
pub struct KnowledgeCodeFileWriteInput {
    pub snapshot_id: i64,
    pub document_version_id: Option<i64>,
    pub relative_path: String,
    pub language: String,
    pub file_size: i64,
    pub content_hash: String,
    pub analysis_level: String,
    pub is_generated: bool,
    pub is_test: bool,
    pub sensitivity: String,
    pub status: String,
    pub skip_reason: String,
}

#[derive(Debug, Clone)]
pub struct KnowledgeCodeSymbolWriteInput {
    pub symbol_key: String,
    pub symbol_kind: String,
    pub name: String,
    pub qualified_name: String,
    pub signature: String,
    pub visibility: String,
    pub parent_symbol_key: String,
    pub start_line: i64,
    pub start_column: i64,
    pub end_line: i64,
    pub end_column: i64,
    pub doc_comment: String,
    pub content_hash: String,
    pub analysis_level: String,
}

/// 源码关系写入只接受分析器生成的稳定符号键和脱敏证据位置，避免将整段源码或任意
/// 前端输入直接落入关系表。
#[derive(Debug, Clone)]
pub struct KnowledgeCodeRelationWriteInput {
    pub from_symbol_key: String,
    pub relation_type: String,
    pub to_symbol_key: String,
    pub to_external_type: String,
    pub to_external_key: String,
    pub evidence_file_id: Option<i64>,
    pub evidence_start_line: Option<i64>,
    pub evidence_end_line: Option<i64>,
    pub evidence_text: String,
    pub resolver: String,
    pub confidence: f64,
    pub confirmed: bool,
}

/// 一次分析的聚合统计，不携带源码正文或敏感命中内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeAnalysisResult {
    pub snapshot: KnowledgeCodeSnapshot,
    pub analyzed_files: i64,
    pub skipped_files: i64,
    pub symbols: i64,
    pub documents: i64,
    pub warnings: Vec<String>,
}

/// 代码源在通用知识源之上附加分析边界；默认不包含未跟踪文件，远程 AI 分析默认可用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeSourceSettings {
    pub source_id: i64,
    pub include_untracked: bool,
    pub max_file_size_bytes: i64,
    pub allowed_languages: Vec<String>,
    pub allow_remote_processing: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCodeSource {
    pub source: KnowledgeSource,
    pub settings: KnowledgeCodeSourceSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeListInput {
    pub project_id: Option<i64>,
    pub release_id: Option<i64>,
    pub source_id: Option<i64>,
    pub keyword: Option<String>,
    pub status: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKnowledgeProjectInput {
    pub id: Option<i64>,
    pub project_key: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub git_workspace_keys: Vec<String>,
    pub git_workspace_key: String,
    pub default_branch: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKnowledgeReleaseInput {
    pub id: Option<i64>,
    pub project_id: i64,
    pub version: String,
    pub tag_name: String,
    pub branch: String,
    pub commit_sha: String,
    pub description: String,
    pub released_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKnowledgeSourceInput {
    pub id: Option<i64>,
    pub source_key: String,
    pub project_id: Option<i64>,
    pub source_type: String,
    pub display_name: String,
    pub root_path: String,
    pub git_workspace_key: String,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub version_strategy: String,
    pub sync_mode: String,
    pub allow_remote_embedding: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKnowledgeCodeSourceInput {
    pub source: UpsertKnowledgeSourceInput,
    pub include_untracked: bool,
    pub max_file_size_bytes: i64,
    pub allowed_languages: Vec<String>,
    pub allow_remote_processing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKnowledgeDocumentInput {
    pub id: Option<i64>,
    pub document_key: String,
    pub project_id: Option<i64>,
    pub source_id: Option<i64>,
    pub doc_type: String,
    pub title: String,
    pub logical_path: String,
    pub sensitivity: String,
    pub tags: Vec<String>,
    pub allow_ai: bool,
    pub allow_mcp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateKnowledgeDocumentVersionInput {
    pub document_id: i64,
    pub release_id: Option<i64>,
    pub version_label: String,
    pub git_branch: String,
    pub commit_sha: String,
    pub source_path: String,
    pub mime_type: String,
    pub content: String,
    pub content_hash: String,
    pub parsed_meta: serde_json::Value,
    pub token_estimate: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunkWriteInput {
    pub chunk_index: i64,
    pub heading_path: String,
    pub content: String,
    pub content_hash: String,
    pub location: serde_json::Value,
    pub token_estimate: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeParseInput {
    pub source_path: String,
    pub mime_type: String,
    pub content: String,
    /// Office/PDF 等二进制容器只在 Rust 内部解析路径传递；预览接口保持兼容的纯文本输入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeParsedBlock {
    pub block_type: String,
    pub heading_path: Vec<String>,
    pub content: String,
    pub start_line: i64,
    pub end_line: i64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeParsedDocument {
    pub parser_id: String,
    pub normalization_version: String,
    pub normalized_content: String,
    pub front_matter: serde_json::Value,
    pub blocks: Vec<KnowledgeParsedBlock>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunkOptions {
    pub target_chars: Option<i64>,
    pub max_chars: Option<i64>,
    pub overlap_chars: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeParseAndChunkInput {
    pub document: KnowledgeParseInput,
    pub options: Option<KnowledgeChunkOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeParseAndChunkResult {
    pub parsed: KnowledgeParsedDocument,
    pub chunk_strategy_id: String,
    pub chunks: Vec<KnowledgeChunkWriteInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKnowledgeEmbeddingProfileInput {
    pub id: Option<i64>,
    pub profile_key: String,
    pub name: String,
    pub mode: String,
    pub provider_key: String,
    pub model: String,
    pub model_revision: String,
    pub dimension: i64,
    pub normalized: bool,
    pub config: serde_json::Value,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingFingerprintInput {
    pub mode: String,
    pub provider_protocol: String,
    pub endpoint_identity: String,
    pub provider_key: String,
    pub model: String,
    pub model_revision: String,
    pub dimension: i64,
    pub normalized: bool,
    pub query_prefix: String,
    pub document_prefix: String,
    pub chunk_strategy_id: String,
    pub normalization_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertZentaoConnectionInput {
    pub id: Option<i64>,
    pub connection_key: String,
    pub name: String,
    pub base_url: String,
    pub api_version: String,
    pub auth_mode: String,
    pub endpoint_profile: String,
    pub credential_key: String,
    pub tls_verify: bool,
    pub allow_insecure_http: bool,
    pub request_timeout_seconds: i64,
    pub page_size: i64,
    pub rate_limit_per_second: f64,
    pub enabled: bool,
}

/// 禅道产品、项目和执行与本地知识项目/发布版本的显式映射。没有映射的远程数据必须
/// 保持 unversioned，不能自动归入最新版本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertZentaoProjectMappingInput {
    pub id: Option<i64>,
    pub connection_id: i64,
    pub knowledge_project_id: i64,
    pub remote_product_id: String,
    pub remote_project_id: String,
    pub remote_execution_ids: Vec<String>,
    pub release_mapping: serde_json::Value,
    pub sync_scope: serde_json::Value,
    pub sync_since: Option<String>,
    pub include_comments: bool,
    pub include_worklogs: bool,
    pub include_attachment_metadata: bool,
    pub allow_remote_embedding: bool,
    pub allow_remote_ai: bool,
    pub enabled: bool,
}

/// 只读探测的脱敏能力矩阵。它刻意不返回请求 Header、Cookie、Token 或服务器正文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoCapabilityProbeResult {
    pub connection_id: i64,
    pub api_version: String,
    pub auth_mode: String,
    pub endpoint_profile: String,
    pub capabilities: serde_json::Value,
    pub status: String,
    pub message: String,
}

/// 远程范围发现的统一最小字段，用于用户显式创建映射；不会保存完整远端对象或凭据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZentaoRemoteScopeItem {
    pub entity_type: String,
    pub external_id: String,
    pub name: String,
    pub parent_external_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchInput {
    pub query: String,
    pub project_ids: Vec<i64>,
    pub release_ids: Vec<i64>,
    pub source_ids: Vec<i64>,
    pub document_types: Vec<String>,
    pub sensitivities: Vec<String>,
    pub snapshot_id: Option<i64>,
    pub limit: Option<i64>,
    pub include_context: Option<bool>,
}

/// RAG 前的确定性查询解析结果。仅提取用户原文中可验证的标识，不调用模型改写查询。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeQueryAnalysis {
    pub query: String,
    pub project_ids: Vec<i64>,
    pub ambiguous_project_ids: Vec<i64>,
    pub releases: Vec<String>,
    pub requirement_ids: Vec<String>,
    pub commit_shas: Vec<String>,
    pub code_symbols: Vec<String>,
    pub paths: Vec<String>,
    pub api_routes: Vec<String>,
    pub tables: Vec<String>,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeVectorSearchInput {
    pub query_vector: Vec<f32>,
    pub filters: KnowledgeSearchInput,
}

/// 混合检索允许调用方在已有安全查询向量时加入向量通道；缺失向量时仍可使用 FTS
/// 和已确认关系，不会为了补齐结果而隐式调用本地或远程模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHybridSearchInput {
    pub filters: KnowledgeSearchInput,
    pub query_vector: Option<Vec<f32>>,
    pub relation_depth: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHybridSearchResult {
    pub hits: Vec<KnowledgeSearchHit>,
    pub diagnostics: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCitation {
    pub citation_key: String,
    pub source_type: String,
    pub document_id: Option<i64>,
    pub document_version_id: Option<i64>,
    pub chunk_id: Option<i64>,
    pub project_id: Option<i64>,
    pub release_id: Option<i64>,
    pub title: String,
    pub logical_path: String,
    pub heading_path: String,
    pub commit_sha: String,
    pub external_key: String,
    pub snapshot_id: Option<i64>,
    pub symbol_key: String,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchHit {
    pub score: f64,
    pub channels: Vec<String>,
    pub citation: KnowledgeCitation,
    pub content: String,
    pub diagnostics: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeConversationMessage {
    /// 只允许 `user` 或 `assistant`，用于在当前问答页面中保留连续追问的语义。
    pub role: String,
    /// 历史消息只作为指代消解上下文，不能替代当前轮检索证据。
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeAskInput {
    pub search: KnowledgeSearchInput,
    /// 仅用于模型提示的原始用户问题；为空时与检索词一致。项目问答可用更精确的
    /// 代码检索词召回证据，同时不丢失用户在原问题中的业务限定。
    #[serde(default)]
    pub original_question: Option<String>,
    /// 由受信任的业务入口选择的回答模式。普通调用保持为空；项目问答识别到
    /// “版本需求实现情况”时使用专用的需求基线与代码候选双阶段检索。
    #[serde(default)]
    pub answer_mode: Option<String>,
    pub provider_key: String,
    pub model: String,
    pub evidence_only: Option<bool>,
    /// 当前页面会话的历史消息；默认空数组以兼容旧版单轮调用方。
    #[serde(default)]
    pub conversation: Vec<KnowledgeConversationMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeAskResult {
    pub answer: String,
    /// `verified` 表示每个事实段落均有本次检索证据的有效引用；其他状态的模型输出
    /// 可以查看，但前端必须标识为未核验或非模型回答。
    pub citation_validation: String,
    pub citations: Vec<KnowledgeCitation>,
    pub conflicts: Vec<String>,
    pub evidence_gaps: Vec<String>,
    pub retrieval_diagnostics: serde_json::Value,
}

/// 实际发送给聊天 Provider 前的只读证据预览。上下文只包含已经通过项目、版本、
/// 来源和敏感级别硬过滤的片段，调用方可在发送前人工复核。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRagContextPreview {
    pub prompt: String,
    pub context: String,
    pub citations: Vec<KnowledgeCitation>,
    pub conflicts: Vec<String>,
    pub evidence_gaps: Vec<String>,
    pub retrieval_diagnostics: serde_json::Value,
}

/// 固定检索评测的一次持久化运行。指标用于在 Profile、分块或排序变化后做可比较的
/// 激活/回滚决策，绝不保存被检索的正文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRetrievalEvaluationRun {
    pub id: i64,
    pub fixture_version: String,
    pub profile_id: Option<i64>,
    pub top_k: i64,
    pub case_count: i64,
    pub recall_at_k: f64,
    pub mrr: f64,
    pub citation_accuracy: f64,
    pub version_leakage_rate: f64,
    pub refusal_accuracy: f64,
    pub p50_latency_ms: i64,
    pub p95_latency_ms: i64,
    pub details: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRetrievalEvaluationCaseResult {
    pub fixture_id: String,
    pub hit_count: i64,
    pub recall_at_k: f64,
    pub reciprocal_rank: f64,
    pub citation_accuracy: f64,
    pub version_leakage: bool,
    pub refusal_expected: bool,
    pub refusal_correct: bool,
    pub latency_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunKnowledgeRetrievalEvaluationInput {
    pub top_k: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeJobProgress {
    pub job_key: String,
    pub status: String,
    pub stage: String,
    pub current: i64,
    pub total: i64,
    pub message: String,
    pub can_cancel: bool,
    pub error: Option<KnowledgeErrorDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeErrorDetail {
    pub code: String,
    pub message: String,
    pub stage: String,
    pub source_key: String,
    pub retryable: bool,
    pub sanitized_details: serde_json::Value,
}
