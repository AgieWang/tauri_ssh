use crate::error::CommandError;
use crate::models::knowledge_domain::analysis::{
    ConfirmKnowledgeAnalysisDraftInput, ConfirmKnowledgeAnalysisDraftResult,
    CreateKnowledgeAnalysisDraftInput, KnowledgeAnalysisDraft,
};
use crate::models::{
    CaptureKnowledgeGitSnapshotInput, GenerateKnowledgeCodeDocumentsInput,
    GenerateKnowledgeCodeDocumentsResult, KnowledgeCodeAnalysisResult, KnowledgeCodeSnapshot,
    KnowledgeCodeSource,
};
use crate::services::knowledge_domain::analysis::KnowledgeAnalysisService;
use crate::state::AppState;

pub(crate) const DOMAIN: &str = "analysis";

/// 仅列出当前项目登记的源码知识源，避免新工作台跨项目拼接源 ID。
#[tauri::command]
pub fn list_knowledge_analysis_code_sources(
    state: tauri::State<'_, AppState>,
    project_id: i64,
) -> Result<Vec<KnowledgeCodeSource>, CommandError> {
    KnowledgeAnalysisService::list_project_code_sources(&state.db, project_id).map_err(Into::into)
}

/// 仅列出当前项目的代码快照；指定 source 时仍会校验它属于该项目。
#[tauri::command]
pub fn list_knowledge_analysis_code_snapshots(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    source_id: Option<i64>,
) -> Result<Vec<KnowledgeCodeSnapshot>, CommandError> {
    KnowledgeAnalysisService::list_project_code_snapshots(&state.db, project_id, source_id)
        .map_err(Into::into)
}

/// 捕获不可变 Git Commit 快照。输入仍复用旧 DTO，保证 releaseId、Git ref 等已验证字段
/// 不会在迁移期间漂移；项目 ID 是新工作台额外的授权范围。
#[tauri::command]
pub async fn capture_knowledge_analysis_git_snapshot(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    input: CaptureKnowledgeGitSnapshotInput,
) -> Result<KnowledgeCodeSnapshot, CommandError> {
    KnowledgeAnalysisService::capture_git_snapshot(&state.db, project_id, input)
        .await
        .map_err(Into::into)
}

/// 运行现有的确定性静态分析；分析服务会维持快照状态和失败边界。
#[tauri::command]
pub async fn analyze_knowledge_analysis_snapshot(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    snapshot_id: i64,
) -> Result<KnowledgeCodeAnalysisResult, CommandError> {
    KnowledgeAnalysisService::analyze_snapshot(&state.db, project_id, snapshot_id)
        .await
        .map_err(Into::into)
}

/// 生成基于已分析快照的固定模板报告。该 Command 不生成、保存或确认 AI 草稿。
#[tauri::command]
pub fn generate_knowledge_analysis_documents(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    input: GenerateKnowledgeCodeDocumentsInput,
) -> Result<GenerateKnowledgeCodeDocumentsResult, CommandError> {
    KnowledgeAnalysisService::generate_documents(&state.db, project_id, input).map_err(Into::into)
}

/// 仅能以同一项目版本中已冻结、已分析的 Git Commit 为证据生成 AI 草稿。正式文档写入
/// 仍须经过单独的人工确认 Command，避免远程模型输出直接进入知识库。
#[tauri::command]
pub async fn create_knowledge_analysis_ai_draft(
    state: tauri::State<'_, AppState>,
    input: CreateKnowledgeAnalysisDraftInput,
) -> Result<KnowledgeAnalysisDraft, CommandError> {
    KnowledgeAnalysisService::create_ai_draft(&state.db, input)
        .await
        .map_err(Into::into)
}

/// 用户编辑 AI 草稿后显式确认入库。服务层会创建新的知识文档版本并记录草稿与版本的
/// 审计关联，不覆盖历史文档或原始草稿。
#[tauri::command]
pub fn confirm_knowledge_analysis_ai_draft(
    state: tauri::State<'_, AppState>,
    input: ConfirmKnowledgeAnalysisDraftInput,
) -> Result<ConfirmKnowledgeAnalysisDraftResult, CommandError> {
    KnowledgeAnalysisService::confirm_ai_draft(&state.db, input).map_err(Into::into)
}
