//! 源码分析领域复用已验证的只读快照与静态分析。AI 仅能接收同一项目版本下、经过
//! 来源授权且已完成分析的固定快照正文；它生成的内容先保存在草稿表，用户编辑确认后
//! 才会创建正式知识文档版本。

use sha2::{Digest, Sha256};

use crate::database::knowledge_domain::analysis::{
    analysis_draft_commit_message, KnowledgeAnalysisDraftRecord, KnowledgeAnalysisRunRecord,
    NewKnowledgeAnalysisDraft, NewKnowledgeAnalysisRun,
};
use crate::database::Database;
use crate::error::AppError;
use crate::models::knowledge_domain::analysis::{
    ConfirmKnowledgeAnalysisDraftInput, ConfirmKnowledgeAnalysisDraftResult,
    CreateKnowledgeAnalysisDraftInput, KnowledgeAnalysisDraft,
};
use crate::models::{
    CaptureKnowledgeGitSnapshotInput, GenerateKnowledgeCodeDocumentsInput,
    GenerateKnowledgeCodeDocumentsResult, KnowledgeCitation, KnowledgeCodeAnalysisResult,
    KnowledgeCodeSnapshot, KnowledgeCodeSource,
};
use crate::services::ai_provider::AiProviderService;
use crate::services::knowledge::{audit_knowledge, KnowledgeService};
use crate::services::knowledge_domain::catalog::KnowledgeCatalogService;
use crate::services::knowledge_domain::documents::KnowledgeDocumentService;
use crate::services::knowledge_policy::KnowledgePolicyService;

const DEFAULT_DRAFT_TEMPLATE: &str = "project-implementation-analysis-v1";
const MAX_ANALYSIS_CONTEXT_CHARS: usize = 48_000;
const RETRY_ANALYSIS_CONTEXT_CHARS: usize = 12_000;
const MAX_ANALYSIS_FILE_CHARS: usize = 6_000;
const MAX_ANALYSIS_FILES: usize = 24;

pub(crate) const DOMAIN: &str = "analysis";

/// 新工作台的源码分析服务。领域入口额外校验项目范围，避免页面只凭快照或源 ID
/// 越过项目边界读取其它项目的代码元数据。
pub struct KnowledgeAnalysisService;

impl KnowledgeAnalysisService {
    pub fn recover_interrupted_state(db: &Database) -> Result<i64, AppError> {
        let (runs, drafts) = db.recover_interrupted_knowledge_analysis_state()?;
        let recovered = runs.saturating_add(drafts);
        if recovered > 0 {
            audit_knowledge(
                db,
                "knowledge_analysis_recover_interrupted",
                "L2",
                "成功",
                "恢复中断的 AI 分析运行或确认草稿",
                serde_json::json!({"runs": runs, "drafts": drafts}),
            );
        }
        Ok(recovered)
    }

    pub fn list_project_code_sources(
        db: &Database,
        project_id: i64,
    ) -> Result<Vec<KnowledgeCodeSource>, AppError> {
        validate_project_id(project_id)?;
        Ok(KnowledgeService::list_code_sources(db)?
            .into_iter()
            .filter(|source| source.source.project_id == Some(project_id))
            .collect())
    }

    pub fn list_project_code_snapshots(
        db: &Database,
        project_id: i64,
        source_id: Option<i64>,
    ) -> Result<Vec<KnowledgeCodeSnapshot>, AppError> {
        validate_project_id(project_id)?;
        let sources = Self::list_project_code_sources(db, project_id)?;
        let source_ids = sources
            .into_iter()
            .map(|source| source.source.id)
            .collect::<std::collections::HashSet<_>>();

        if let Some(source_id) = source_id {
            validate_positive_id(source_id, "源码知识源 ID")?;
            if !source_ids.contains(&source_id) {
                return Err(AppError::NotFound(format!(
                    "项目 {project_id} 下不存在源码知识源: {source_id}"
                )));
            }
        }

        Ok(db
            .list_knowledge_code_snapshots(source_id)?
            .into_iter()
            .filter(|snapshot| source_ids.contains(&snapshot.source_id))
            .collect())
    }

    pub async fn capture_git_snapshot(
        db: &Database,
        project_id: i64,
        input: CaptureKnowledgeGitSnapshotInput,
    ) -> Result<KnowledgeCodeSnapshot, AppError> {
        Self::ensure_project_code_source(db, project_id, input.source_id)?;
        // 旧服务通过 Git 对象数据库读取固定 Commit，且会校验 release/source 的项目范围；
        // 不允许本领域入口退化成 checkout 或工作树修改。
        KnowledgeService::capture_git_snapshot(db, input).await
    }

    pub async fn analyze_snapshot(
        db: &Database,
        project_id: i64,
        snapshot_id: i64,
    ) -> Result<KnowledgeCodeAnalysisResult, AppError> {
        Self::ensure_project_snapshot(db, project_id, snapshot_id)?;
        KnowledgeService::analyze_code_snapshot(db, snapshot_id).await
    }

    pub fn generate_documents(
        db: &Database,
        project_id: i64,
        input: GenerateKnowledgeCodeDocumentsInput,
    ) -> Result<GenerateKnowledgeCodeDocumentsResult, AppError> {
        Self::ensure_project_snapshot(db, project_id, input.snapshot_id)?;
        // 这是静态证据的固定模板报告，不会调用外部 Provider，也不会生成待确认的 AI 草稿。
        KnowledgeService::generate_code_snapshot_documents(db, input)
    }

    /// 对固定项目版本的已分析 Git 快照生成 AI 草稿。此处显式收集每段源码对应的不可变
    /// 文档版本引用，并在远程调用前复用统一安全策略；浏览器无法传递文件路径或工作树正文。
    pub async fn create_ai_draft(
        db: &Database,
        input: CreateKnowledgeAnalysisDraftInput,
    ) -> Result<KnowledgeAnalysisDraft, AppError> {
        validate_project_id(input.project_id)?;
        validate_positive_id(input.project_version_id, "项目版本 ID")?;
        let release = db
            .get_knowledge_release_by_id(input.project_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("项目版本不存在: {}", input.project_version_id))
            })?;
        if release.project_id != input.project_id {
            return Err(AppError::InvalidInput(
                "项目版本不属于当前项目，不能生成跨项目分析草稿".to_string(),
            ));
        }
        let mut snapshot_ids = input.snapshot_ids;
        snapshot_ids.sort_unstable();
        snapshot_ids.dedup();
        if snapshot_ids.is_empty() {
            return Err(AppError::InvalidInput(
                "请至少选择一个已完成静态分析的代码快照".to_string(),
            ));
        }
        if snapshot_ids.len() > 12 {
            return Err(AppError::InvalidInput(
                "一次最多分析 12 个代码快照，请按项目版本分批处理".to_string(),
            ));
        }

        let template_key = input
            .template_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_DRAFT_TEMPLATE)
            .to_string();
        if !template_key.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        }) {
            return Err(AppError::InvalidInput(
                "分析模板标识只能包含小写字母、数字和连字符".to_string(),
            ));
        }

        let mut citations = Vec::new();
        let mut evidence_sections = Vec::new();
        let mut snapshots = Vec::new();
        let mut remaining_context_chars = MAX_ANALYSIS_CONTEXT_CHARS;
        for snapshot_id in &snapshot_ids {
            let snapshot = Self::ensure_project_snapshot(db, input.project_id, *snapshot_id)?;
            if snapshot.release_id != Some(input.project_version_id)
                || snapshot.snapshot_type != "git_commit"
                || snapshot.worktree_dirty
                || snapshot.status != "analyzed"
            {
                return Err(AppError::InvalidInput(
                    "AI 分析只接受当前项目版本下已完成分析的只读 Git Commit 快照".to_string(),
                ));
            }
            let (snapshot_citations, section) =
                snapshot_code_evidence(db, &snapshot, &mut remaining_context_chars)?;
            citations.extend(snapshot_citations);
            evidence_sections.push(section);
            snapshots.push(snapshot);
        }
        if citations.is_empty() {
            return Err(AppError::InvalidInput(
                "所选快照没有可安全发送给 AI 的活动代码文件".to_string(),
            ));
        }
        let context = evidence_sections.join("\n\n");
        KnowledgePolicyService::authorize_remote_ai_context(db, &citations, &context)?;
        // 只将脱敏后的副本交给 Provider；原始代码证据只用于本地引用、哈希和后续确认。
        let sanitized_context = KnowledgePolicyService::sanitize_remote_ai_context(&context)?;

        let manifest =
            KnowledgeCatalogService::get_project_version_manifest(db, input.project_version_id)?;
        let manifest_hash = sha256(&serde_json::to_vec(&manifest)?);
        let evidence_hash = sha256(context.as_bytes());
        let snapshot_fingerprint = snapshots
            .iter()
            .map(|snapshot| format!("{}:{}", snapshot.id, snapshot.commit_sha))
            .collect::<Vec<_>>()
            .join(",");
        let run_key = format!(
            "analysis:{}:{}:{}:{}",
            input.project_id,
            input.project_version_id,
            template_key,
            sha256(snapshot_fingerprint.as_bytes())
        );
        let run = db.create_knowledge_analysis_run(&NewKnowledgeAnalysisRun {
            run_key,
            project_id: input.project_id,
            release_id: input.project_version_id,
            manifest_hash,
            analyzer_version: "knowledge-ai-analysis-v1".to_string(),
            include_rules_json: "[]".to_string(),
            exclude_rules_json: "[]".to_string(),
            snapshot_ids_json: serde_json::to_string(&snapshot_ids)?,
            evidence_hash,
        })?;
        if let Some(existing) =
            db.get_knowledge_analysis_draft_by_run_and_template(run.id, &template_key)?
        {
            match existing.status.as_str() {
                "confirmed" => {
                    return Err(AppError::InvalidInput(
                        "该快照组合的 AI 分析已确认入库；请重新捕获或选择新的代码快照".to_string(),
                    ));
                }
                "draft" | "reviewing" => return Ok(draft_into_model(existing, run)),
                "confirming" => {
                    return Err(AppError::InvalidInput(
                        "AI 分析草稿正在确认入库，请勿重复提交".to_string(),
                    ));
                }
                _ => {}
            }
        }
        if !db.claim_knowledge_analysis_run(run.id)? {
            return Err(AppError::InvalidInput(
                "相同快照组合的 AI 分析正在生成，请稍后刷新查看草稿".to_string(),
            ));
        }
        audit_knowledge(
            db,
            "knowledge_analysis_ai_draft_generate",
            "L2",
            "开始",
            "已授权向 AI Provider 发送固定代码分析上下文",
            serde_json::json!({
                "analysisRunId": run.id,
                "projectId": input.project_id,
                "releaseId": input.project_version_id,
                "snapshotIds": snapshot_ids,
                "citationCount": citations.len(),
            }),
        );

        let provider_key = input.provider_key.filter(|value| !value.trim().is_empty());
        let provider_prompt =
            analysis_provider_prompt(input.project_id, &release.version, &sanitized_context);
        let provider_result = AiProviderService::ask(
            db,
            analysis_provider_input(provider_prompt, provider_key.clone()),
        )
        .await;
        let provider_result = match provider_result {
            Err(error) if should_retry_with_compact_context(&error) => {
                let compact_context =
                    truncate_chars(&sanitized_context, RETRY_ANALYSIS_CONTEXT_CHARS);
                // 首次请求等待响应期间，来源或文档权限可能已被收回；重试属于一次新的
                // 外发操作，必须按当前状态重新授权，不能复用首次请求的检查结果。
                KnowledgePolicyService::authorize_remote_ai_context(
                    db,
                    &citations,
                    &compact_context,
                )?;
                audit_knowledge(
                    db,
                    "knowledge_analysis_ai_draft_generate",
                    "L2",
                    "重试",
                    "Provider 响应中断，已使用精简证据自动重试一次",
                    serde_json::json!({
                        "analysisRunId": run.id,
                        "fullContextChars": sanitized_context.chars().count(),
                        "retryContextChars": compact_context.chars().count(),
                    }),
                );
                AiProviderService::ask(
                    db,
                    analysis_provider_input(
                        analysis_provider_prompt(
                            input.project_id,
                            &release.version,
                            &compact_context,
                        ),
                        provider_key,
                    ),
                )
                .await
            }
            result => result,
        };
        let provider_result = match provider_result {
            Ok(result) => result,
            Err(error) => {
                let _ = db.update_knowledge_analysis_run_status(run.id, "failed");
                audit_knowledge(
                    db,
                    "knowledge_analysis_ai_draft_generate",
                    "L2",
                    "失败",
                    "AI Provider 未生成项目分析草稿",
                    serde_json::json!({"analysisRunId": run.id, "snapshotIds": snapshot_ids}),
                );
                return Err(error);
            }
        };
        let claim_refs = match extract_verified_claim_refs(&provider_result.answer, &citations) {
            Ok(claim_refs) => claim_refs,
            Err(error) => {
                let _ = db.update_knowledge_analysis_run_status(run.id, "failed");
                audit_knowledge(
                    db,
                    "knowledge_analysis_ai_draft_generate",
                    "L2",
                    "拒绝",
                    "AI 输出缺少可验证的代码引用",
                    serde_json::json!({"analysisRunId": run.id, "snapshotIds": snapshot_ids}),
                );
                return Err(error);
            }
        };
        let persisted = match db.upsert_knowledge_analysis_draft(&NewKnowledgeAnalysisDraft {
            analysis_run_id: run.id,
            provider_key: provider_result.provider_key,
            model: provider_result.model,
            template_key,
            content: provider_result.answer,
            claim_refs_json: serde_json::to_string(&claim_refs)?,
        }) {
            Ok(draft) => draft,
            Err(error) => {
                let _ = db.update_knowledge_analysis_run_status(run.id, "failed");
                return Err(error);
            }
        };
        db.update_knowledge_analysis_run_status(run.id, "completed")?;
        audit_knowledge(
            db,
            "knowledge_analysis_ai_draft_generate",
            "L2",
            "成功",
            "生成待人工确认的项目分析草稿",
            serde_json::json!({
                "analysisRunId": run.id,
                "draftId": persisted.id,
                "snapshotIds": snapshot_ids,
                "claimRefCount": claim_refs.len(),
            }),
        );
        Ok(draft_into_model(persisted, run))
    }

    /// AI 输出必须由用户复核和编辑后才会走普通文档的草稿/提交链路。确认过程不会修改
    /// 已存在文档，并会把生成草稿与最终不可变文档版本关联起来供后续审计。
    pub fn confirm_ai_draft(
        db: &Database,
        input: ConfirmKnowledgeAnalysisDraftInput,
    ) -> Result<ConfirmKnowledgeAnalysisDraftResult, AppError> {
        validate_positive_id(input.draft_id, "分析草稿 ID")?;
        let draft = db.claim_knowledge_analysis_draft_confirmation(input.draft_id)?;
        let original_claim_refs = match parse_claim_refs(&draft.claim_refs_json) {
            Ok(claim_refs) => claim_refs,
            Err(error) => {
                let _ = db.release_knowledge_analysis_draft_confirmation(draft.id);
                return Err(error);
            }
        };
        if let Err(error) = validate_confirmed_content(&input.content, &original_claim_refs) {
            let _ = db.release_knowledge_analysis_draft_confirmation(draft.id);
            return Err(error);
        }
        let run = db
            .get_knowledge_analysis_run_by_id(draft.analysis_run_id)?
            .ok_or_else(|| AppError::NotFound("分析草稿对应的运行记录不存在".to_string()));
        let run = match run {
            Ok(run) => run,
            Err(error) => {
                let _ = db.release_knowledge_analysis_draft_confirmation(draft.id);
                return Err(error);
            }
        };
        if let Err(error) = reauthorize_confirmed_claim_refs(db, &run, &original_claim_refs) {
            let _ = db.release_knowledge_analysis_draft_confirmation(draft.id);
            return Err(error);
        }
        let saved = KnowledgeDocumentService::save_manual_draft(
            db,
            crate::models::knowledge_domain::documents::KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: None,
                project_id: run.project_id,
                title: input.title,
                content: input.content,
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: None,
                editor_label: input.author_label.clone(),
            },
        );
        let saved = match saved {
            Ok(saved) => saved,
            Err(error) => {
                let _ = db.release_knowledge_analysis_draft_confirmation(draft.id);
                return Err(error);
            }
        };
        let document = KnowledgeDocumentService::commit_analysis_draft(
            db,
            crate::models::knowledge_domain::documents::CommitKnowledgeDocumentDraftInput {
                draft_id: saved.draft.id,
                revision: saved.draft.revision,
                version_label: input.version_label,
                project_version_id: Some(run.release_id),
                cross_version_scope: None,
                commit_message: Some(analysis_draft_commit_message(draft.id)),
                author_label: input.author_label,
            },
            draft.id,
        );
        let document = match document {
            Ok(document) => document,
            Err(error) => {
                let _ = db.release_knowledge_analysis_draft_confirmation(draft.id);
                return Err(error);
            }
        };
        db.confirm_knowledge_analysis_draft(draft.id, document.document_version_id)?;
        let confirmed = db
            .get_knowledge_analysis_draft_by_id(draft.id)?
            .ok_or_else(|| AppError::Custom("确认分析草稿后未找到记录".to_string()))?;
        audit_knowledge(
            db,
            "knowledge_analysis_ai_draft_confirm",
            "L2",
            "成功",
            "确认 AI 项目分析草稿并创建不可变知识文档版本",
            serde_json::json!({
                "analysisRunId": run.id,
                "draftId": draft.id,
                "documentVersionId": document.document_version_id,
                "snapshotIds": snapshot_ids_from_run(&run),
                "claimRefCount": original_claim_refs.len(),
            }),
        );
        Ok(ConfirmKnowledgeAnalysisDraftResult {
            draft: draft_into_model(confirmed, run.clone()),
            document,
        })
    }

    fn ensure_project_code_source(
        db: &Database,
        project_id: i64,
        source_id: i64,
    ) -> Result<(), AppError> {
        validate_project_id(project_id)?;
        validate_positive_id(source_id, "源码知识源 ID")?;
        let exists = Self::list_project_code_sources(db, project_id)?
            .into_iter()
            .any(|source| source.source.id == source_id);
        if exists {
            Ok(())
        } else {
            Err(AppError::NotFound(format!(
                "项目 {project_id} 下不存在源码知识源: {source_id}"
            )))
        }
    }

    fn ensure_project_snapshot(
        db: &Database,
        project_id: i64,
        snapshot_id: i64,
    ) -> Result<KnowledgeCodeSnapshot, AppError> {
        validate_project_id(project_id)?;
        validate_positive_id(snapshot_id, "源码快照 ID")?;
        let snapshot = db
            .get_knowledge_code_snapshot_by_id(snapshot_id)?
            .ok_or_else(|| AppError::NotFound(format!("源码快照不存在: {snapshot_id}")))?;
        Self::ensure_project_code_source(db, project_id, snapshot.source_id)?;
        Ok(snapshot)
    }
}

fn snapshot_code_evidence(
    db: &Database,
    snapshot: &KnowledgeCodeSnapshot,
    remaining: &mut usize,
) -> Result<(Vec<KnowledgeCitation>, String), AppError> {
    let mut citations = Vec::new();
    let mut evidence = Vec::new();
    for file in db
        .list_knowledge_code_files(snapshot.id)?
        .into_iter()
        .filter(|file| file.status == "active" && file.sensitivity == "internal")
        .take(MAX_ANALYSIS_FILES)
    {
        if *remaining == 0 {
            break;
        }
        let Some(version_id) = file.document_version_id else {
            continue;
        };
        let version = db
            .get_knowledge_document_version_by_id(version_id)?
            .ok_or_else(|| AppError::NotFound("代码文件关联的文档版本不存在".to_string()))?;
        let document = db
            .get_knowledge_document_by_id(version.document_id)?
            .ok_or_else(|| AppError::NotFound("代码文件关联的知识文档不存在".to_string()))?;
        let citation_key = format!("code:{}:file:{}", snapshot.id, file.id);
        let content = truncate_chars(&version.content, MAX_ANALYSIS_FILE_CHARS.min(*remaining));
        if content.trim().is_empty() {
            continue;
        }
        *remaining = remaining.saturating_sub(content.chars().count());
        citations.push(KnowledgeCitation {
            citation_key: citation_key.clone(),
            source_type: "code_file".to_string(),
            document_id: Some(document.id),
            document_version_id: Some(version.id),
            chunk_id: None,
            project_id: snapshot.project_id,
            release_id: snapshot.release_id,
            title: document.title,
            logical_path: file.relative_path.clone(),
            heading_path: file.relative_path.clone(),
            commit_sha: snapshot.commit_sha.clone(),
            external_key: String::new(),
            snapshot_id: Some(snapshot.id),
            symbol_key: String::new(),
            start_line: Some(1),
            end_line: Some(
                i64::try_from(content.lines().count())
                    .unwrap_or(i64::MAX)
                    .max(1),
            ),
            excerpt: truncate_chars(&content, 500),
        });
        evidence.push(format!(
            "### [{}] {}\nCommit: {}\n```{}\n{}\n```",
            citation_key, file.relative_path, snapshot.commit_sha, file.language, content
        ));
    }
    Ok((citations, evidence.join("\n\n")))
}

fn extract_verified_claim_refs(
    answer: &str,
    citations: &[KnowledgeCitation],
) -> Result<Vec<String>, AppError> {
    let allowed = citations
        .iter()
        .map(|citation| citation.citation_key.clone())
        .collect::<std::collections::HashSet<_>>();
    extract_verified_claim_refs_from_allowed(answer, &allowed)
}

fn extract_verified_claim_refs_from_allowed(
    answer: &str,
    allowed: &std::collections::HashSet<String>,
) -> Result<Vec<String>, AppError> {
    let mut refs = Vec::new();
    let mut rest = answer;
    while let Some(start) = rest.find("[code:") {
        let tail = &rest[start + 1..];
        let Some(end) = tail.find(']') else {
            return Err(AppError::InvalidInput(
                "AI 分析草稿包含未闭合的代码引用，已拒绝保存".to_string(),
            ));
        };
        let reference = &tail[..end];
        if !allowed.contains(reference) {
            return Err(AppError::InvalidInput(
                "AI 分析草稿包含无法验证的代码引用，已拒绝保存".to_string(),
            ));
        }
        if !refs.iter().any(|item| item == reference) {
            refs.push(reference.to_string());
        }
        rest = &tail[end + 1..];
    }
    if refs.is_empty() {
        return Err(AppError::InvalidInput(
            "AI 分析草稿缺少可验证的代码引用，已拒绝保存".to_string(),
        ));
    }
    Ok(refs)
}

fn parse_claim_refs(claim_refs_json: &str) -> Result<Vec<String>, AppError> {
    let claim_refs = serde_json::from_str::<Vec<String>>(claim_refs_json).map_err(|_| {
        AppError::Custom("分析草稿的代码引用审计记录已损坏，不能确认入库".to_string())
    })?;
    if claim_refs.is_empty()
        || claim_refs
            .iter()
            .any(|reference| !is_code_citation(reference))
    {
        return Err(AppError::Custom(
            "分析草稿缺少有效的原始代码引用，不能确认入库".to_string(),
        ));
    }
    Ok(claim_refs)
}

/// 人工编辑可以压缩、改写或补充结论，但至少保留一条最初已验证的代码引用；所有 code
/// 引用都只能来自原草稿，防止确认阶段把自由编造的引用写入正式知识库。
fn validate_confirmed_content(
    content: &str,
    original_claim_refs: &[String],
) -> Result<(), AppError> {
    let allowed = original_claim_refs.iter().cloned().collect();
    extract_verified_claim_refs_from_allowed(content, &allowed).map(|_| ())
}

/// 草稿在生成后可能经历来源禁用或文档降密。确认正式入库前重新从固定快照中解析原始
/// 引用并执行当前安全策略，不能将已经失效的证据永久固化。
fn reauthorize_confirmed_claim_refs(
    db: &Database,
    run: &KnowledgeAnalysisRunRecord,
    original_claim_refs: &[String],
) -> Result<(), AppError> {
    let snapshot_ids = snapshot_ids_from_run(run);
    if snapshot_ids.is_empty() {
        return Err(AppError::Custom(
            "分析运行缺少快照审计记录，不能确认入库".to_string(),
        ));
    }
    let mut remaining_context_chars = MAX_ANALYSIS_CONTEXT_CHARS;
    let mut citations = Vec::new();
    for snapshot_id in snapshot_ids {
        let snapshot =
            KnowledgeAnalysisService::ensure_project_snapshot(db, run.project_id, snapshot_id)?;
        if snapshot.release_id != Some(run.release_id) || snapshot.status != "analyzed" {
            return Err(AppError::InvalidInput(
                "分析草稿引用的代码快照已不再是当前项目版本的已分析证据，不能确认入库".to_string(),
            ));
        }
        let (snapshot_citations, _) =
            snapshot_code_evidence(db, &snapshot, &mut remaining_context_chars)?;
        citations.extend(snapshot_citations);
    }
    let allowed = citations
        .iter()
        .map(|citation| citation.citation_key.clone())
        .collect::<std::collections::HashSet<_>>();
    if original_claim_refs
        .iter()
        .any(|reference| !allowed.contains(reference))
    {
        return Err(AppError::InvalidInput(
            "分析草稿引用的代码证据已失效或受限，不能确认入库".to_string(),
        ));
    }
    let claimed_citations = citations
        .into_iter()
        .filter(|citation| original_claim_refs.contains(&citation.citation_key))
        .collect::<Vec<_>>();
    let context = claimed_citations
        .iter()
        .map(|citation| citation.excerpt.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    KnowledgePolicyService::authorize_remote_ai_context(db, &claimed_citations, &context)
}

fn is_code_citation(reference: &str) -> bool {
    let mut parts = reference.split(':');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next(), parts.next()),
        (Some("code"), Some(snapshot_id), Some("file"), Some(file_id), None)
            if snapshot_id.parse::<i64>().is_ok_and(|id| id > 0)
                && file_id.parse::<i64>().is_ok_and(|id| id > 0)
    )
}

fn draft_into_model(
    draft: KnowledgeAnalysisDraftRecord,
    run: KnowledgeAnalysisRunRecord,
) -> KnowledgeAnalysisDraft {
    KnowledgeAnalysisDraft {
        id: draft.id,
        analysis_run_id: draft.analysis_run_id,
        project_id: run.project_id,
        project_version_id: run.release_id,
        snapshot_ids: snapshot_ids_from_run(&run),
        provider_key: draft.provider_key,
        model: draft.model,
        template_key: draft.template_key,
        content: draft.content,
        claim_refs: serde_json::from_str(&draft.claim_refs_json).unwrap_or_default(),
        status: draft.status,
        confirmed_document_version_id: draft.confirmed_version_id,
    }
}

fn snapshot_ids_from_run(run: &KnowledgeAnalysisRunRecord) -> Vec<i64> {
    serde_json::from_str::<Vec<i64>>(&run.snapshot_ids_json)
        .unwrap_or_default()
        .into_iter()
        .filter(|snapshot_id| *snapshot_id > 0)
        .collect()
}

fn sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        format!(
            "{}\n…（为控制上下文长度已截断）",
            value.chars().take(max_chars).collect::<String>()
        )
    }
}

fn analysis_provider_prompt(project_id: i64, release_version: &str, context: &str) -> String {
    format!(
        "请基于以下固定 Git Commit 的源码证据，生成项目实现分析 Markdown。\n\
输出包括：系统概览、模块职责、关键接口/数据、调用关系、测试与配置线索、证据不足项。\n\
只能陈述证据中可验证的事实，不能补全未出现的业务规则；每个事实段末必须带至少一个\n\
形如 [code:快照ID:file:文件ID] 的引用。候选关系须明确说明尚未人工确认。\n\
\n项目：{project_id}；版本：{release_version}\n\n证据：\n{context}"
    )
}

fn analysis_provider_input(
    prompt: String,
    provider_key: Option<String>,
) -> crate::models::AiProviderAskInput {
    crate::models::AiProviderAskInput {
        prompt,
        provider_key,
        system_prompt: Some(
            "你是受控的代码知识分析助手。只根据用户提供的固定版本代码证据输出中文 Markdown；\n\
不得输出任何凭据、连接串或推测性结论。每个事实段必须使用给定 code 引用。"
                .to_string(),
        ),
        skill_scope: Some("knowledge".to_string()),
        use_skill_trigger: Some(false),
    }
}

fn should_retry_with_compact_context(error: &AppError) -> bool {
    matches!(error, AppError::ProviderTransient(_))
        || matches!(error, AppError::Custom(message)
            if message.contains("读取 Provider 响应失败")
                || message.contains("Provider 返回空响应")
                || message.contains("Provider 连接超时"))
}

fn validate_project_id(project_id: i64) -> Result<(), AppError> {
    validate_positive_id(project_id, "项目 ID")
}

fn validate_positive_id(id: i64, label: &str) -> Result<(), AppError> {
    if id > 0 {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!("{label} 必须为正整数")))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_verified_claim_refs, reauthorize_confirmed_claim_refs,
        should_retry_with_compact_context, truncate_chars, validate_confirmed_content,
        KnowledgeAnalysisService,
    };
    use crate::database::knowledge_domain::analysis::KnowledgeAnalysisRunRecord;
    use crate::database::Database;
    use crate::error::AppError;
    use crate::models::knowledge_domain::analysis::CreateKnowledgeAnalysisDraftInput;
    use crate::models::{
        CaptureKnowledgeGitSnapshotInput, CreateKnowledgeCodeSnapshotInput, KnowledgeCitation,
        UpsertKnowledgeCodeSourceInput, UpsertKnowledgeProjectInput, UpsertKnowledgeReleaseInput,
        UpsertKnowledgeSourceInput,
    };

    fn project_input(project_key: &str, name: &str) -> UpsertKnowledgeProjectInput {
        UpsertKnowledgeProjectInput {
            id: None,
            project_key: project_key.to_string(),
            name: name.to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        }
    }

    fn code_source_input(project_id: i64, source_key: &str) -> UpsertKnowledgeCodeSourceInput {
        UpsertKnowledgeCodeSourceInput {
            source: UpsertKnowledgeSourceInput {
                id: None,
                source_key: source_key.to_string(),
                project_id: Some(project_id),
                source_type: "git_workspace".to_string(),
                display_name: source_key.to_string(),
                root_path: String::new(),
                git_workspace_key: "unused-in-scope-test".to_string(),
                include_globs: vec!["**/*".to_string()],
                exclude_globs: Vec::new(),
                version_strategy: "git_ref".to_string(),
                sync_mode: "manual".to_string(),
                allow_remote_embedding: false,
                enabled: true,
            },
            include_untracked: false,
            max_file_size_bytes: 1024,
            allowed_languages: vec!["rust".to_string()],
            allow_remote_processing: false,
        }
    }

    #[test]
    fn retries_only_transient_provider_response_failures_with_compact_context() {
        assert!(should_retry_with_compact_context(
            &AppError::ProviderTransient("Provider 响应传输中断，请重试".to_string(),)
        ));
        assert!(should_retry_with_compact_context(&AppError::Custom(
            "读取 Provider 响应失败，响应可能被中断，请稍后重试".to_string(),
        )));
        assert!(should_retry_with_compact_context(&AppError::Custom(
            "Provider 返回空响应，请稍后重试或检查服务网关配置".to_string(),
        )));
        assert!(!should_retry_with_compact_context(&AppError::InvalidInput(
            "Provider 已禁用".to_string(),
        )));
        assert_eq!(
            truncate_chars("甲乙丙丁", 2),
            "甲乙\n…（为控制上下文长度已截断）"
        );
    }

    #[tokio::test]
    async fn rejects_foreign_project_source_and_snapshot_before_reading_git_or_analyzing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project_a = database.upsert_knowledge_project(&project_input("project-a", "项目 A"))?;
        let project_b = database.upsert_knowledge_project(&project_input("project-b", "项目 B"))?;
        let own_source =
            database.upsert_knowledge_code_source(&code_source_input(project_a.id, "source-a"))?;
        let foreign_source =
            database.upsert_knowledge_code_source(&code_source_input(project_b.id, "source-b"))?;
        let foreign_snapshot =
            database.upsert_knowledge_code_snapshot(&CreateKnowledgeCodeSnapshotInput {
                snapshot_key: "scope-test:source-b".to_string(),
                source_id: foreign_source.source.id,
                project_id: Some(project_b.id),
                release_id: None,
                snapshot_type: "git_commit".to_string(),
                ref_name: "HEAD".to_string(),
                commit_sha: "a".repeat(40),
                base_commit_sha: String::new(),
                branch_name: "main".to_string(),
                worktree_dirty: false,
                dirty_state: serde_json::json!({}),
                captured_at: "2026-08-04T00:00:00Z".to_string(),
                file_count: 0,
                analyzer_version: "scope-test".to_string(),
                status: "captured".to_string(),
            })?;

        let project_a_sources =
            KnowledgeAnalysisService::list_project_code_sources(&database, project_a.id)?;
        assert_eq!(project_a_sources.len(), 1);
        assert_eq!(project_a_sources[0].source.id, own_source.source.id);

        let capture_error = KnowledgeAnalysisService::capture_git_snapshot(
            &database,
            project_a.id,
            CaptureKnowledgeGitSnapshotInput {
                source_id: foreign_source.source.id,
                git_ref: "HEAD".to_string(),
                release_id: None,
            },
        )
        .await
        .expect_err("跨项目来源必须在读取 Git 前被拒绝");
        assert!(capture_error.to_string().contains("项目"));

        let analysis_error = KnowledgeAnalysisService::analyze_snapshot(
            &database,
            project_a.id,
            foreign_snapshot.id,
        )
        .await
        .expect_err("跨项目快照必须在静态分析前被拒绝");
        assert!(analysis_error.to_string().contains("项目"));
        Ok(())
    }

    #[tokio::test]
    async fn ai_draft_rejects_snapshot_not_bound_to_the_selected_project_version(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project =
            database.upsert_knowledge_project(&project_input("analysis-a", "分析项目"))?;
        let release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "main".to_string(),
            commit_sha: "a".repeat(40),
            description: String::new(),
            released_at: None,
        })?;
        let source =
            database.upsert_knowledge_code_source(&code_source_input(project.id, "source-a"))?;
        let unbound_snapshot =
            database.upsert_knowledge_code_snapshot(&CreateKnowledgeCodeSnapshotInput {
                snapshot_key: "analysis-a:unbound".to_string(),
                source_id: source.source.id,
                project_id: Some(project.id),
                release_id: None,
                snapshot_type: "git_commit".to_string(),
                ref_name: "HEAD".to_string(),
                commit_sha: "a".repeat(40),
                base_commit_sha: String::new(),
                branch_name: "main".to_string(),
                worktree_dirty: false,
                dirty_state: serde_json::json!({}),
                captured_at: "2026-08-04T00:00:00Z".to_string(),
                file_count: 0,
                analyzer_version: "scope-test".to_string(),
                status: "analyzed".to_string(),
            })?;

        let error = KnowledgeAnalysisService::create_ai_draft(
            &database,
            CreateKnowledgeAnalysisDraftInput {
                project_id: project.id,
                project_version_id: release.id,
                snapshot_ids: vec![unbound_snapshot.id],
                provider_key: None,
                template_key: None,
            },
        )
        .await
        .expect_err("未绑定版本的快照不能发送给 AI");
        assert!(error.to_string().contains("只读 Git Commit 快照"));
        Ok(())
    }

    #[test]
    fn ai_draft_requires_only_known_code_citations() {
        let citation = KnowledgeCitation {
            citation_key: "code:7:file:8".to_string(),
            source_type: "code_file".to_string(),
            document_id: Some(1),
            document_version_id: Some(2),
            chunk_id: None,
            project_id: Some(3),
            release_id: Some(4),
            title: "订单服务".to_string(),
            logical_path: "src/order.rs".to_string(),
            heading_path: "src/order.rs".to_string(),
            commit_sha: "a".repeat(40),
            external_key: String::new(),
            snapshot_id: Some(7),
            symbol_key: String::new(),
            start_line: Some(1),
            end_line: Some(2),
            excerpt: "代码".to_string(),
        };
        assert_eq!(
            extract_verified_claim_refs("结论 [code:7:file:8]", &[citation.clone()])
                .expect("已知引用应允许"),
            vec!["code:7:file:8"]
        );
        assert!(extract_verified_claim_refs("结论 [code:7:file:9]", &[citation]).is_err());
    }

    #[test]
    fn confirmed_ai_draft_keeps_only_original_verified_code_citations() {
        let original_refs = vec!["code:7:file:8".to_string()];
        assert!(validate_confirmed_content("人工复核结论 [code:7:file:8]", &original_refs).is_ok());
        assert!(validate_confirmed_content("人工复核结论", &original_refs).is_err());
        assert!(validate_confirmed_content("伪造引用 [code:7:file:9]", &original_refs).is_err());
    }

    #[test]
    fn confirmation_rejects_a_run_without_persisted_snapshot_evidence() {
        let database = Database::init(":memory:").expect("内存数据库可用");
        let run = KnowledgeAnalysisRunRecord {
            id: 1,
            run_key: "analysis:missing-snapshots".to_string(),
            project_id: 1,
            release_id: 1,
            manifest_hash: "manifest".to_string(),
            analyzer_version: "v1".to_string(),
            snapshot_ids_json: "[]".to_string(),
            evidence_hash: "evidence".to_string(),
            status: "completed".to_string(),
            finished_at: None,
        };
        assert!(
            reauthorize_confirmed_claim_refs(&database, &run, &["code:1:file:2".to_string()],)
                .is_err()
        );
    }
}
