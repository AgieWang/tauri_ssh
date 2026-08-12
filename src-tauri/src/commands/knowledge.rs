use crate::error::CommandError;
use crate::models::{
    AnalyzeKnowledgeCodeImpactInput, BuildKnowledgeEmbeddingBatchInput,
    CaptureKnowledgeDirtyWorktreeSnapshotInput, CaptureKnowledgeGitSnapshotInput,
    CaptureKnowledgeLocalDirectorySnapshotInput, CompareKnowledgeCodeSnapshotsInput,
    CompareKnowledgeDocumentVersionsInput, DownloadKnowledgeLocalEmbeddingModelInput,
    EstimateKnowledgeEmbeddingRebuildInput, GenerateKnowledgeCodeDocumentsInput,
    GenerateKnowledgeCodeDocumentsResult, GenerateKnowledgeLocalEmbeddingsInput,
    GenerateZentaoAiSummaryInput, GenerateZentaoAiSummaryResult,
    GenerateZentaoKnowledgeDocumentsInput, GenerateZentaoKnowledgeDocumentsResult,
    ImportKnowledgeCommitRelationsInput, ImportKnowledgeDocumentRelationsInput,
    ImportKnowledgeExperiencesInput, ImportKnowledgeExperiencesResult,
    ImportKnowledgeLocalEmbeddingModelInput, KnowledgeAskInput, KnowledgeAskResult, KnowledgeChunk,
    KnowledgeCitationDetail, KnowledgeCodeAnalysisResult, KnowledgeCodeCallGraph,
    KnowledgeCodeCallGraphInput, KnowledgeCodeFile, KnowledgeCodeFileContent,
    KnowledgeCodeSnapshot, KnowledgeCodeSnapshotComparison, KnowledgeCodeSource, KnowledgeDocument,
    KnowledgeDocumentComparison, KnowledgeDocumentDetail, KnowledgeDocumentVersion,
    KnowledgeEmbeddingBatchResult, KnowledgeEmbeddingFingerprintInput,
    KnowledgeEmbeddingIndexValidation, KnowledgeEmbeddingLifecycleResult,
    KnowledgeEmbeddingProfile, KnowledgeEmbeddingProfileTestResult,
    KnowledgeEmbeddingRebuildEstimate, KnowledgeFtsCapability, KnowledgeGitRef,
    KnowledgeHybridSearchInput, KnowledgeHybridSearchResult, KnowledgeJob, KnowledgeListInput,
    KnowledgeLocalEmbeddingDownloadProgress, KnowledgeLocalEmbeddingModelImportResult,
    KnowledgeLocalEmbeddingRuntimeStatus, KnowledgePage, KnowledgeParseAndChunkInput,
    KnowledgeParseAndChunkResult, KnowledgeProject, KnowledgeQueryAnalysis,
    KnowledgeRagContextPreview, KnowledgeRelation, KnowledgeRelease,
    KnowledgeRetrievalEvaluationRun, KnowledgeSearchHit, KnowledgeSearchInput, KnowledgeSource,
    KnowledgeSourceScopePreview, KnowledgeSourceSyncResult, KnowledgeVectorSearchInput,
    ListKnowledgeRelationsInput, RemoveKnowledgeLocalEmbeddingModelInput,
    RunKnowledgeRetrievalEvaluationInput, SearchKnowledgeCodeSymbolsInput,
    StartKnowledgeSourceSyncInput, SyncKnowledgeGitSourceInput, SyncKnowledgeLocalSourceInput,
    SyncZentaoMappingInput, UpsertKnowledgeCodeSourceInput, UpsertKnowledgeDocumentInput,
    UpsertKnowledgeEmbeddingProfileInput, UpsertKnowledgeProjectInput,
    UpsertKnowledgeRelationInput, UpsertKnowledgeReleaseInput, UpsertKnowledgeSourceInput,
    UpsertZentaoConnectionInput, UpsertZentaoProjectMappingInput, ZentaoCapabilityProbeResult,
    ZentaoConnection, ZentaoProjectMapping, ZentaoRemoteScopeItem, ZentaoSyncResult,
};
use crate::services::knowledge::KnowledgeService;
use crate::services::knowledge_embedding::KnowledgeEmbeddingService;
use crate::services::knowledge_local_embedding::KnowledgeLocalEmbeddingService;
use crate::services::knowledge_retrieval::KnowledgeRetrievalService;
use crate::services::knowledge_rollout::KnowledgeRolloutService;
use crate::state::AppState;
use tauri::{Emitter, Manager};

#[tauri::command]
pub fn list_knowledge_projects(
    state: tauri::State<'_, AppState>,
    input: Option<KnowledgeListInput>,
) -> Result<KnowledgePage<KnowledgeProject>, CommandError> {
    KnowledgeService::list_projects(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn list_zentao_connections(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ZentaoConnection>, CommandError> {
    KnowledgeService::list_zentao_connections(&state.db).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_zentao_connection(
    state: tauri::State<'_, AppState>,
    input: UpsertZentaoConnectionInput,
) -> Result<ZentaoConnection, CommandError> {
    KnowledgeService::upsert_zentao_connection(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn delete_zentao_connection(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    KnowledgeService::delete_zentao_connection(&state.db, id).map_err(Into::into)
}

#[tauri::command]
pub async fn probe_zentao_connection(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<ZentaoCapabilityProbeResult, CommandError> {
    KnowledgeService::probe_zentao_connection(&state.db, id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn discover_zentao_remote_scopes(
    state: tauri::State<'_, AppState>,
    connection_id: i64,
) -> Result<Vec<ZentaoRemoteScopeItem>, CommandError> {
    KnowledgeService::discover_zentao_remote_scopes(&state.db, connection_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn upsert_zentao_project_mapping(
    state: tauri::State<'_, AppState>,
    input: UpsertZentaoProjectMappingInput,
) -> Result<ZentaoProjectMapping, CommandError> {
    KnowledgeService::upsert_zentao_project_mapping(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn list_zentao_project_mappings(
    state: tauri::State<'_, AppState>,
    connection_id: Option<i64>,
) -> Result<Vec<ZentaoProjectMapping>, CommandError> {
    KnowledgeService::list_zentao_project_mappings(&state.db, connection_id).map_err(Into::into)
}

#[tauri::command]
pub async fn sync_zentao_mapping(
    state: tauri::State<'_, AppState>,
    input: SyncZentaoMappingInput,
) -> Result<Vec<ZentaoSyncResult>, CommandError> {
    KnowledgeService::sync_zentao_mapping(&state.db, input)
        .await
        .map_err(Into::into)
}

/// 从本地已同步且规范化的禅道实体生成可追溯的事实文档；不会发起远端请求或调用模型。
#[tauri::command]
pub fn generate_zentao_fact_documents(
    state: tauri::State<'_, AppState>,
    input: GenerateZentaoKnowledgeDocumentsInput,
) -> Result<GenerateZentaoKnowledgeDocumentsResult, CommandError> {
    KnowledgeService::generate_zentao_fact_documents(&state.db, input).map_err(Into::into)
}

/// 仅以已生成的禅道事实文档为输入生成 AI 摘要；服务会校验引用并把摘要与事实文档分离。
#[tauri::command]
pub async fn generate_zentao_ai_summary(
    state: tauri::State<'_, AppState>,
    input: GenerateZentaoAiSummaryInput,
) -> Result<GenerateZentaoAiSummaryResult, CommandError> {
    KnowledgeService::generate_zentao_ai_summary(&state.db, input)
        .await
        .map_err(Into::into)
}

/// 只读投影既有经验库，保持原经验 Commands 和 MCP 行为不变。
#[tauri::command]
pub fn import_knowledge_ai_experiences(
    state: tauri::State<'_, AppState>,
    input: ImportKnowledgeExperiencesInput,
) -> Result<ImportKnowledgeExperiencesResult, CommandError> {
    KnowledgeService::import_ai_experiences(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn analyze_knowledge_query(
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<KnowledgeQueryAnalysis, CommandError> {
    KnowledgeRetrievalService::analyze_query(&state.db, &query).map_err(Into::into)
}

#[tauri::command]
pub fn search_knowledge_fts(
    state: tauri::State<'_, AppState>,
    input: KnowledgeSearchInput,
) -> Result<Vec<KnowledgeSearchHit>, CommandError> {
    KnowledgeRetrievalService::search_fts(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn search_knowledge_hybrid(
    state: tauri::State<'_, AppState>,
    input: KnowledgeHybridSearchInput,
) -> Result<KnowledgeHybridSearchResult, CommandError> {
    KnowledgeRetrievalService::search_hybrid(&state.db, input).map_err(Into::into)
}

/// 预览经硬过滤后的证据上下文，不调用任何远程模型。
#[tauri::command]
pub fn preview_knowledge_rag_context(
    state: tauri::State<'_, AppState>,
    search: KnowledgeSearchInput,
) -> Result<KnowledgeRagContextPreview, CommandError> {
    KnowledgeRetrievalService::preview_rag_context(&state.db, search).map_err(Into::into)
}

/// 使用现有 AI Provider 对已审核的知识证据进行问答。无证据时服务会拒答，避免生成
/// 未经来源支撑的内部结论。
#[tauri::command]
pub async fn ask_knowledge(
    state: tauri::State<'_, AppState>,
    input: KnowledgeAskInput,
) -> Result<KnowledgeAskResult, CommandError> {
    KnowledgeRetrievalService::ask(&state.db, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn run_fixed_knowledge_retrieval_evaluation(
    state: tauri::State<'_, AppState>,
    input: Option<RunKnowledgeRetrievalEvaluationInput>,
) -> Result<KnowledgeRetrievalEvaluationRun, CommandError> {
    KnowledgeRetrievalService::run_fixed_evaluation(
        &state.db,
        input.unwrap_or(RunKnowledgeRetrievalEvaluationInput { top_k: None }),
    )
    .map_err(Into::into)
}

/// 运行单个可恢复的本地向量构建批次；模型或运行时失败时不触发远程回退。
#[tauri::command]
pub async fn build_knowledge_local_embedding_batch(
    app: tauri::AppHandle,
    input: BuildKnowledgeEmbeddingBatchInput,
) -> Result<KnowledgeEmbeddingBatchResult, CommandError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::Custom(error.to_string()))?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        KnowledgeEmbeddingService::build_local_embedding_batch(&state.db, &app_data_dir, input)
    })
    .await
    .map_err(|error| crate::error::AppError::Custom(format!("本地向量构建任务失败: {error}")))?
    .map_err(Into::into)
}

/// 远程构建只在用户显式选择 remote Profile 后才调用；每个片段的来源和敏感策略仍由
/// Service 在请求 Provider 前校验，桌面 Command 不保有正文或凭据。
#[tauri::command]
pub async fn build_knowledge_remote_embedding_batch(
    state: tauri::State<'_, AppState>,
    input: BuildKnowledgeEmbeddingBatchInput,
) -> Result<KnowledgeEmbeddingBatchResult, CommandError> {
    KnowledgeEmbeddingService::build_remote_embedding_batch(&state.db, input)
        .await
        .map_err(Into::into)
}

/// 兼容旧客户端的能力查询：远程向量化始终可用，是否发送具体正文仍由来源级授权
/// 和内容安全策略决定。
#[tauri::command]
pub fn get_knowledge_remote_embedding_enabled(
    state: tauri::State<'_, AppState>,
) -> Result<bool, CommandError> {
    KnowledgeEmbeddingService::remote_embedding_enabled(&state.db).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_knowledge_relation(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeRelationInput,
) -> Result<KnowledgeRelation, CommandError> {
    KnowledgeService::upsert_relation(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_relations(
    state: tauri::State<'_, AppState>,
    input: Option<ListKnowledgeRelationsInput>,
) -> Result<Vec<KnowledgeRelation>, CommandError> {
    KnowledgeService::list_relations(
        &state.db,
        input.unwrap_or(ListKnowledgeRelationsInput {
            entity_type: None,
            entity_key: None,
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            sensitivities: Vec::new(),
            confirmed_only: Some(false),
            limit: Some(100),
        }),
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn confirm_knowledge_relation(
    state: tauri::State<'_, AppState>,
    id: i64,
    confirmed: bool,
) -> Result<KnowledgeRelation, CommandError> {
    KnowledgeService::confirm_relation(&state.db, id, confirmed).map_err(Into::into)
}

#[tauri::command]
pub fn import_knowledge_document_relations(
    state: tauri::State<'_, AppState>,
    input: ImportKnowledgeDocumentRelationsInput,
) -> Result<Vec<KnowledgeRelation>, CommandError> {
    KnowledgeService::import_document_front_matter_relations(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn import_knowledge_commit_relations(
    state: tauri::State<'_, AppState>,
    input: ImportKnowledgeCommitRelationsInput,
) -> Result<Vec<KnowledgeRelation>, CommandError> {
    KnowledgeService::import_commit_message_relations(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_knowledge_project(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeProjectInput,
) -> Result<KnowledgeProject, CommandError> {
    KnowledgeService::upsert_project(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn delete_knowledge_project(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    KnowledgeService::delete_project(&state.db, id).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_releases(
    state: tauri::State<'_, AppState>,
    project_id: i64,
) -> Result<Vec<KnowledgeRelease>, CommandError> {
    KnowledgeService::list_releases(&state.db, project_id).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_knowledge_release(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeReleaseInput,
) -> Result<KnowledgeRelease, CommandError> {
    KnowledgeService::upsert_release(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn delete_knowledge_release(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    KnowledgeService::delete_release(&state.db, id).map_err(Into::into)
}

#[cfg(test)]
mod compatibility_contract_tests {
    /// 旧 IPC 入口须继续由兼容门面处理，避免领域拆分后出现同名 Command 仍在、但调用了
    /// 已废弃实现的情况。业务结果与 JSON 外观由 Service/API 层测试进一步覆盖。
    #[test]
    fn legacy_catalog_commands_still_delegate_to_knowledge_service_facade() {
        let source = include_str!("knowledge.rs");
        for (command, delegation) in [
            (
                "pub fn list_knowledge_projects(",
                "KnowledgeService::list_projects(&state.db, input)",
            ),
            (
                "pub fn upsert_knowledge_project(",
                "KnowledgeService::upsert_project(&state.db, input)",
            ),
            (
                "pub fn delete_knowledge_project(",
                "KnowledgeService::delete_project(&state.db, id)",
            ),
            (
                "pub fn list_knowledge_releases(",
                "KnowledgeService::list_releases(&state.db, project_id)",
            ),
            (
                "pub fn upsert_knowledge_release(",
                "KnowledgeService::upsert_release(&state.db, input)",
            ),
            (
                "pub fn delete_knowledge_release(",
                "KnowledgeService::delete_release(&state.db, id)",
            ),
        ] {
            assert!(source.contains(command), "旧 Command 缺失: {command}");
            assert!(
                source.contains(delegation),
                "旧 Command 未委托兼容门面: {delegation}"
            );
        }
    }
}

#[tauri::command]
pub async fn discover_knowledge_git_refs(
    state: tauri::State<'_, AppState>,
    workspace_key: String,
) -> Result<Vec<KnowledgeGitRef>, CommandError> {
    KnowledgeService::discover_git_refs(&state.db, &workspace_key)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn capture_knowledge_git_snapshot(
    state: tauri::State<'_, AppState>,
    input: CaptureKnowledgeGitSnapshotInput,
) -> Result<KnowledgeCodeSnapshot, CommandError> {
    KnowledgeService::capture_git_snapshot(&state.db, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn capture_knowledge_dirty_worktree_snapshot(
    state: tauri::State<'_, AppState>,
    input: CaptureKnowledgeDirtyWorktreeSnapshotInput,
) -> Result<KnowledgeCodeSnapshot, CommandError> {
    KnowledgeService::capture_dirty_worktree_snapshot(&state.db, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn capture_knowledge_local_directory_snapshot(
    state: tauri::State<'_, AppState>,
    input: CaptureKnowledgeLocalDirectorySnapshotInput,
) -> Result<KnowledgeCodeSnapshot, CommandError> {
    KnowledgeService::capture_local_directory_snapshot(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub async fn analyze_knowledge_code_snapshot(
    state: tauri::State<'_, AppState>,
    snapshot_id: i64,
) -> Result<KnowledgeCodeAnalysisResult, CommandError> {
    KnowledgeService::analyze_code_snapshot(&state.db, snapshot_id)
        .await
        .map_err(Into::into)
}

/// 基于已完成分析的快照重生成固定工程报告，不读取本地文件或调用任何远端服务。
#[tauri::command]
pub fn generate_knowledge_code_documents(
    state: tauri::State<'_, AppState>,
    input: GenerateKnowledgeCodeDocumentsInput,
) -> Result<GenerateKnowledgeCodeDocumentsResult, CommandError> {
    KnowledgeService::generate_code_snapshot_documents(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn search_knowledge_code_symbols(
    state: tauri::State<'_, AppState>,
    input: SearchKnowledgeCodeSymbolsInput,
) -> Result<Vec<crate::models::KnowledgeCodeSymbol>, CommandError> {
    KnowledgeService::search_code_symbols(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_code_files(
    state: tauri::State<'_, AppState>,
    snapshot_id: i64,
) -> Result<Vec<KnowledgeCodeFile>, CommandError> {
    KnowledgeService::list_code_files(&state.db, snapshot_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_knowledge_code_file_content(
    state: tauri::State<'_, AppState>,
    snapshot_id: i64,
    file_id: i64,
) -> Result<KnowledgeCodeFileContent, CommandError> {
    KnowledgeService::get_code_file_content(&state.db, snapshot_id, file_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_knowledge_code_call_graph(
    state: tauri::State<'_, AppState>,
    input: KnowledgeCodeCallGraphInput,
) -> Result<KnowledgeCodeCallGraph, CommandError> {
    KnowledgeService::code_call_graph(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn compare_knowledge_code_snapshots(
    state: tauri::State<'_, AppState>,
    input: CompareKnowledgeCodeSnapshotsInput,
) -> Result<KnowledgeCodeSnapshotComparison, CommandError> {
    KnowledgeService::compare_code_snapshots(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn analyze_knowledge_code_impact(
    state: tauri::State<'_, AppState>,
    input: AnalyzeKnowledgeCodeImpactInput,
) -> Result<KnowledgeCodeCallGraph, CommandError> {
    KnowledgeService::analyze_code_impact(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_code_snapshots(
    state: tauri::State<'_, AppState>,
    source_id: Option<i64>,
) -> Result<Vec<KnowledgeCodeSnapshot>, CommandError> {
    state
        .db
        .list_knowledge_code_snapshots(source_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_sources(
    state: tauri::State<'_, AppState>,
    project_id: Option<i64>,
) -> Result<Vec<KnowledgeSource>, CommandError> {
    KnowledgeService::list_sources(&state.db, project_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_code_sources(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<KnowledgeCodeSource>, CommandError> {
    KnowledgeService::list_code_sources(&state.db).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_knowledge_code_source(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeCodeSourceInput,
) -> Result<KnowledgeCodeSource, CommandError> {
    KnowledgeService::upsert_code_source(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_knowledge_source(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeSourceInput,
) -> Result<KnowledgeSource, CommandError> {
    KnowledgeService::upsert_source(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_knowledge_sources_atomically(
    state: tauri::State<'_, AppState>,
    inputs: Vec<UpsertKnowledgeSourceInput>,
) -> Result<Vec<KnowledgeSource>, CommandError> {
    KnowledgeService::upsert_sources_atomically(&state.db, inputs).map_err(Into::into)
}

#[tauri::command]
pub fn delete_knowledge_source(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    KnowledgeService::delete_source(&state.db, id).map_err(Into::into)
}

#[tauri::command]
pub fn preview_knowledge_source_scope(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeSourceInput,
) -> Result<KnowledgeSourceScopePreview, CommandError> {
    KnowledgeService::preview_source_scope(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn preview_knowledge_code_source_scope(
    state: tauri::State<'_, AppState>,
    source_id: i64,
) -> Result<KnowledgeSourceScopePreview, CommandError> {
    KnowledgeService::preview_code_source_scope(&state.db, source_id).map_err(Into::into)
}

#[tauri::command]
pub async fn sync_knowledge_git_source(
    state: tauri::State<'_, AppState>,
    input: SyncKnowledgeGitSourceInput,
) -> Result<KnowledgeSourceSyncResult, CommandError> {
    KnowledgeService::sync_git_source(&state.db, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn sync_knowledge_local_source(
    state: tauri::State<'_, AppState>,
    input: SyncKnowledgeLocalSourceInput,
) -> Result<KnowledgeSourceSyncResult, CommandError> {
    KnowledgeService::sync_local_source(&state.db, input).map_err(Into::into)
}

/// 将既有 ai_experiences 只读投影到统一知识文档索引；不修改经验库记录或其 MCP 接口。
#[tauri::command]
pub fn sync_knowledge_experience_source(
    state: tauri::State<'_, AppState>,
    source_id: i64,
    release_id: Option<i64>,
) -> Result<KnowledgeSourceSyncResult, CommandError> {
    KnowledgeService::sync_experience_source(&state.db, source_id, release_id).map_err(Into::into)
}

#[tauri::command]
pub fn start_knowledge_source_sync(
    app: tauri::AppHandle,
    input: StartKnowledgeSourceSyncInput,
) -> Result<KnowledgeJob, CommandError> {
    KnowledgeService::start_source_sync_job(app, input).map_err(Into::into)
}

#[tauri::command]
pub fn get_knowledge_job(
    state: tauri::State<'_, AppState>,
    job_key: String,
) -> Result<KnowledgeJob, CommandError> {
    KnowledgeService::get_job(&state.db, &job_key).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_jobs(
    state: tauri::State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<KnowledgeJob>, CommandError> {
    KnowledgeService::list_jobs(&state.db, limit).map_err(Into::into)
}

#[tauri::command]
pub fn cancel_knowledge_job(
    app: tauri::AppHandle,
    job_key: String,
) -> Result<KnowledgeJob, CommandError> {
    KnowledgeService::cancel_job(&app, &job_key).map_err(Into::into)
}

#[tauri::command]
pub fn retry_knowledge_job(
    app: tauri::AppHandle,
    job_key: String,
) -> Result<KnowledgeJob, CommandError> {
    KnowledgeService::retry_job(app, &job_key).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_documents(
    state: tauri::State<'_, AppState>,
    input: Option<KnowledgeListInput>,
) -> Result<KnowledgePage<KnowledgeDocument>, CommandError> {
    KnowledgeService::list_documents(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn get_knowledge_document_detail(
    state: tauri::State<'_, AppState>,
    document_id: i64,
) -> Result<KnowledgeDocumentDetail, CommandError> {
    KnowledgeService::get_document_detail(&state.db, document_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_document_versions(
    state: tauri::State<'_, AppState>,
    document_id: i64,
) -> Result<Vec<KnowledgeDocumentVersion>, CommandError> {
    KnowledgeService::list_document_versions(&state.db, document_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_document_chunks(
    state: tauri::State<'_, AppState>,
    document_version_id: i64,
) -> Result<Vec<KnowledgeChunk>, CommandError> {
    KnowledgeService::list_document_chunks(&state.db, document_version_id).map_err(Into::into)
}

#[tauri::command]
pub fn compare_knowledge_document_versions(
    state: tauri::State<'_, AppState>,
    input: CompareKnowledgeDocumentVersionsInput,
) -> Result<KnowledgeDocumentComparison, CommandError> {
    KnowledgeService::compare_document_versions(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn get_knowledge_citation_detail(
    state: tauri::State<'_, AppState>,
    chunk_id: i64,
) -> Result<KnowledgeCitationDetail, CommandError> {
    KnowledgeService::get_citation_detail(&state.db, chunk_id).map_err(Into::into)
}

#[tauri::command]
pub fn preview_knowledge_parse_and_chunk(
    input: KnowledgeParseAndChunkInput,
) -> Result<KnowledgeParseAndChunkResult, CommandError> {
    KnowledgeService::preview_parse_and_chunk(input).map_err(Into::into)
}

#[tauri::command]
pub fn parse_and_index_knowledge_document_version(
    state: tauri::State<'_, AppState>,
    document_version_id: i64,
    options: Option<crate::models::KnowledgeChunkOptions>,
) -> Result<KnowledgeParseAndChunkResult, CommandError> {
    KnowledgeService::parse_and_index_document_version(&state.db, document_version_id, options)
        .map_err(Into::into)
}

#[tauri::command]
pub fn calculate_knowledge_embedding_fingerprint(
    input: KnowledgeEmbeddingFingerprintInput,
) -> Result<String, CommandError> {
    KnowledgeEmbeddingService::calculate_fingerprint(&input).map_err(Into::into)
}

#[tauri::command]
pub fn list_knowledge_embedding_profiles(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<KnowledgeEmbeddingProfile>, CommandError> {
    KnowledgeEmbeddingService::list_profiles(&state.db).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_knowledge_embedding_profile(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeEmbeddingProfileInput,
) -> Result<KnowledgeEmbeddingProfile, CommandError> {
    KnowledgeEmbeddingService::upsert_profile(&state.db, input).map_err(Into::into)
}

/// 仅返回本地运行时和缓存元数据，模型下载必须由后续显式确认流程发起。
#[tauri::command]
pub fn get_knowledge_local_embedding_runtime_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<KnowledgeLocalEmbeddingRuntimeStatus, CommandError> {
    KnowledgeRolloutService::require(&state.db, "local_embedding")?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::Custom(error.to_string()))?;
    KnowledgeLocalEmbeddingService::runtime_status(&app_data_dir).map_err(Into::into)
}

/// 模型导入必须由用户显式发起；服务只会复制到应用数据目录并校验给定 SHA-256。
#[tauri::command]
pub fn import_knowledge_local_embedding_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: ImportKnowledgeLocalEmbeddingModelInput,
) -> Result<KnowledgeLocalEmbeddingModelImportResult, CommandError> {
    KnowledgeRolloutService::require(&state.db, "local_embedding")?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::Custom(error.to_string()))?;
    KnowledgeLocalEmbeddingService::import_model(&app_data_dir, input).map_err(Into::into)
}

/// 下载地址只从受控设置 `knowledge.local_embedding.internal_mirror_url` 读取，避免前端
/// 传入任意网络目标。进度事件不含 URL、正文或模型文件内容。
#[tauri::command]
pub async fn download_knowledge_local_embedding_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: DownloadKnowledgeLocalEmbeddingModelInput,
) -> Result<KnowledgeLocalEmbeddingModelImportResult, CommandError> {
    KnowledgeRolloutService::require(&state.db, "local_embedding")?;
    let mirror_url = state
        .db
        .get_config("knowledge.local_embedding.internal_mirror_url")?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            crate::error::AppError::InvalidInput("尚未配置内部模型镜像地址".to_string())
        })?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::Custom(error.to_string()))?;
    let event_app = app.clone();
    KnowledgeLocalEmbeddingService::download_model_from_mirror(
        &app_data_dir,
        &mirror_url,
        input,
        move |progress: KnowledgeLocalEmbeddingDownloadProgress| {
            let _ = event_app.emit("knowledge-local-embedding-download-progress", progress);
        },
    )
    .await
    .map_err(Into::into)
}

/// 此低层 Command 只提供受控本地推理；重建任务可通过 Service 传入取消检查点。
#[tauri::command]
pub async fn generate_knowledge_local_embeddings(
    app: tauri::AppHandle,
    input: GenerateKnowledgeLocalEmbeddingsInput,
) -> Result<Vec<Vec<f32>>, CommandError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::Custom(error.to_string()))?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        KnowledgeRolloutService::require(&state.db, "local_embedding")?;
        KnowledgeLocalEmbeddingService::generate_embeddings(&app_data_dir, input, || false)
    })
    .await
    .map_err(|error| crate::error::AppError::Custom(format!("本地向量推理任务失败: {error}")))?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn test_knowledge_local_embedding_profile(
    app: tauri::AppHandle,
    profile_id: i64,
) -> Result<KnowledgeEmbeddingProfileTestResult, CommandError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::Custom(error.to_string()))?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        KnowledgeEmbeddingService::test_local_profile(&state.db, &app_data_dir, profile_id)
    })
    .await
    .map_err(|error| crate::error::AppError::Custom(format!("本地向量探测任务失败: {error}")))?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn test_knowledge_remote_embedding_profile(
    state: tauri::State<'_, AppState>,
    profile_id: i64,
) -> Result<KnowledgeEmbeddingProfileTestResult, CommandError> {
    KnowledgeEmbeddingService::test_remote_profile(&state.db, profile_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn remove_knowledge_local_embedding_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: RemoveKnowledgeLocalEmbeddingModelInput,
) -> Result<(), CommandError> {
    KnowledgeRolloutService::require(&state.db, "local_embedding")?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::Custom(error.to_string()))?;
    KnowledgeLocalEmbeddingService::remove_model(&app_data_dir, input).map_err(Into::into)
}

#[tauri::command]
pub fn estimate_knowledge_embedding_rebuild(
    state: tauri::State<'_, AppState>,
    input: EstimateKnowledgeEmbeddingRebuildInput,
) -> Result<KnowledgeEmbeddingRebuildEstimate, CommandError> {
    KnowledgeEmbeddingService::estimate_rebuild(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn begin_knowledge_embedding_profile_rebuild(
    state: tauri::State<'_, AppState>,
    profile_id: i64,
) -> Result<KnowledgeEmbeddingLifecycleResult, CommandError> {
    KnowledgeEmbeddingService::begin_profile_rebuild(&state.db, profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn validate_knowledge_embedding_profile_rebuild(
    state: tauri::State<'_, AppState>,
    profile_id: i64,
) -> Result<KnowledgeEmbeddingIndexValidation, CommandError> {
    KnowledgeEmbeddingService::validate_profile_rebuild(&state.db, profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn complete_knowledge_embedding_profile_rebuild(
    state: tauri::State<'_, AppState>,
    profile_id: i64,
) -> Result<KnowledgeEmbeddingLifecycleResult, CommandError> {
    KnowledgeEmbeddingService::complete_profile_rebuild(&state.db, profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn activate_knowledge_embedding_profile_rebuild(
    state: tauri::State<'_, AppState>,
    profile_id: i64,
) -> Result<KnowledgeEmbeddingLifecycleResult, CommandError> {
    KnowledgeEmbeddingService::activate_profile_rebuild(&state.db, profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn rollback_knowledge_embedding_profile_rebuild(
    state: tauri::State<'_, AppState>,
    previous_profile_id: i64,
) -> Result<KnowledgeEmbeddingLifecycleResult, CommandError> {
    KnowledgeEmbeddingService::rollback_profile_rebuild(&state.db, previous_profile_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn retire_knowledge_embedding_profile_rebuild(
    state: tauri::State<'_, AppState>,
    profile_id: i64,
) -> Result<KnowledgeEmbeddingLifecycleResult, CommandError> {
    KnowledgeEmbeddingService::retire_profile_rebuild(&state.db, profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn search_active_knowledge_vectors(
    state: tauri::State<'_, AppState>,
    input: KnowledgeVectorSearchInput,
) -> Result<Vec<KnowledgeSearchHit>, CommandError> {
    KnowledgeEmbeddingService::search_active_vectors(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn upsert_knowledge_document(
    state: tauri::State<'_, AppState>,
    input: UpsertKnowledgeDocumentInput,
) -> Result<KnowledgeDocument, CommandError> {
    KnowledgeService::upsert_document(&state.db, input).map_err(Into::into)
}

#[tauri::command]
pub fn delete_knowledge_document(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), CommandError> {
    KnowledgeService::delete_document(&state.db, id).map_err(Into::into)
}

#[tauri::command]
pub fn ensure_knowledge_fts(
    state: tauri::State<'_, AppState>,
) -> Result<KnowledgeFtsCapability, CommandError> {
    KnowledgeService::ensure_fts(&state.db).map_err(Into::into)
}

#[tauri::command]
pub fn rebuild_knowledge_fts(state: tauri::State<'_, AppState>) -> Result<i64, CommandError> {
    KnowledgeService::rebuild_fts(&state.db).map_err(Into::into)
}
