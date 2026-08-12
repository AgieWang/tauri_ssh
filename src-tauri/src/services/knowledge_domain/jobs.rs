use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};
use tauri::Manager;

use crate::database::knowledge::{
    CompleteKnowledgeDocumentIndexJobInput, FailKnowledgeDocumentIndexJobInput,
};
use crate::database::knowledge_domain::documents::{
    CompleteKnowledgeDocumentUploadImport, NewKnowledgeDocumentParseArtifact,
};
use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    CreateKnowledgeDocumentVersionInput, KnowledgeJob, KnowledgeParseAndChunkInput,
    KnowledgeParseInput,
};
use crate::services::ai_provider::AiProviderService;
use crate::services::knowledge_domain::upload_validation::FILE_PARSE_TIMEOUT;
use crate::services::knowledge_local_ocr::{KnowledgeLocalOcrService, LocalImageOcrOutcome};
use crate::services::knowledge_parser::KnowledgeParserService;
use crate::services::knowledge_policy::detect_sensitive_content;
use crate::services::knowledge_rollout::KnowledgeRolloutService;
use crate::state::AppState;

pub(crate) const DOMAIN: &str = "jobs";

pub(crate) struct KnowledgeDocumentJobService;

pub(crate) struct KnowledgeUploadImportJobService;

enum DocumentIndexOutcome {
    Completed(Box<KnowledgeJob>),
    Cancelled,
    Failed(AppError),
}

impl KnowledgeDocumentJobService {
    /// 文档提交事务只负责持久化“待索引”事实；事务提交后再异步调度，避免任务读到
    /// 半提交的版本。任务持久化状态允许应用重启后通过重试按钮恢复。
    pub(crate) fn spawn_document_index_job(
        app: tauri::AppHandle,
        document_version_id: i64,
        job_id: i64,
    ) {
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            if let Err(error) = Self::run_document_index_job(&state.db, document_version_id, job_id)
            {
                log::warn!("知识文档索引任务执行异常 (job {job_id}): {error}");
            }
        });
    }

    pub(crate) fn retry_document_index_job(
        app: tauri::AppHandle,
        job_id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        let state = app.state::<AppState>();
        let document_version_id = state
            .db
            .find_knowledge_document_version_id_by_index_job_id(job_id)?
            .ok_or_else(|| AppError::NotFound("索引任务对应的文档版本不存在".to_string()))?;
        let restarted = state.db.restart_knowledge_job(job_id)?;
        Self::spawn_document_index_job(app, document_version_id, job_id);
        Ok(restarted)
    }

    /// 由后台线程或测试同步执行的正式索引入口。仅消费已提交版本的冻结正文；草稿和
    /// 前端路径均不参与该流程，因此不会把未提交内容写进检索索引。
    pub(crate) fn run_document_index_job(
        db: &Database,
        document_version_id: i64,
        job_id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(document_version_id, "文档版本 ID")?;
        validate_positive_id(job_id, "索引任务 ID")?;
        let job = db
            .get_knowledge_job_by_id(job_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识任务不存在: {job_id}")))?;
        if job.job_type != "document_index" {
            return Err(AppError::InvalidInput(
                "当前任务不是文档索引任务".to_string(),
            ));
        }
        // 用户在后台线程获得 CPU 前取消队列任务时，DAO 已将它标记为 cancelled。
        // 此处正常返回，避免任务调度日志把预期取消误报为执行异常。
        if job.status == "cancelled" {
            return Ok(job);
        }
        if db.find_knowledge_document_version_id_by_index_job_id(job_id)?
            != Some(document_version_id)
        {
            return Err(AppError::InvalidInput(
                "索引任务与文档版本不匹配".to_string(),
            ));
        }

        let running = db.mark_knowledge_job_running(
            job_id,
            "parse",
            "正在解析并建立文档索引",
            &document_index_checkpoint(document_version_id, "parse", 0, 1, None),
        )?;
        if running.cancel_requested || db.is_knowledge_job_cancel_requested(job_id)? {
            return finish_cancelled(db, document_version_id, job_id);
        }

        let outcome = match parse_and_store_document_chunks(db, document_version_id, job_id) {
            Ok(Some(job)) => DocumentIndexOutcome::Completed(Box::new(job)),
            Ok(None) => DocumentIndexOutcome::Cancelled,
            Err(error) => DocumentIndexOutcome::Failed(error),
        };

        match outcome {
            DocumentIndexOutcome::Completed(job) => Ok(*job),
            DocumentIndexOutcome::Cancelled => finish_cancelled(db, document_version_id, job_id),
            DocumentIndexOutcome::Failed(error) => {
                let safe_error = truncate_error(&error.to_string());
                db.fail_knowledge_document_index_job_or_cancel(FailKnowledgeDocumentIndexJobInput {
                    job_id,
                    error: &safe_error,
                    failed_checkpoint: &document_index_checkpoint(
                        document_version_id,
                        "failed",
                        0,
                        1,
                        None,
                    ),
                    cancelled_checkpoint: &document_index_checkpoint(
                        document_version_id,
                        "cancelled",
                        0,
                        1,
                        None,
                    ),
                })
            }
        }
    }
}

impl KnowledgeUploadImportJobService {
    pub(crate) fn spawn_upload_import_job(app: tauri::AppHandle, job_id: i64) {
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let result = app
                .path()
                .app_data_dir()
                .map_err(|error| AppError::Custom(error.to_string()))
                .and_then(|dir| Self::run_upload_import_job(&state.db, &dir, job_id));
            if let Err(error) = result {
                log::warn!("知识上传导入任务执行异常 (job {job_id}): {error}");
            }
        });
    }

    pub(crate) fn retry_upload_import_job(
        app: tauri::AppHandle,
        job_id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        let state = app.state::<AppState>();
        let restarted = state.db.restart_knowledge_document_upload(job_id)?;
        Self::spawn_upload_import_job(app, job_id);
        Ok(restarted)
    }

    pub(crate) fn run_upload_import_job(
        db: &Database,
        app_data_dir: &Path,
        job_id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        let job = db
            .get_knowledge_job_by_id(job_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识任务不存在: {job_id}")))?;
        if job.job_type != "upload_import" {
            return Ok(job);
        }
        if let Err(error) = KnowledgeRolloutService::require(db, "catalog") {
            return db.fail_knowledge_document_upload_import_or_cancel(
                job_id,
                &truncate_error(&error.to_string()),
            );
        }
        if job.status == "cancelled" {
            return finish_upload_cancelled(db, job_id);
        }
        let upload = match db.get_pending_knowledge_document_upload(job_id)? {
            Some(upload) => upload,
            None => {
                return db.fail_knowledge_document_upload_import_or_cancel(
                    job_id,
                    "上传任务对应的受控资产不存在或已结束",
                );
            }
        };
        let running = match db.mark_knowledge_job_running(
            job_id,
            "parse",
            "正在解析上传文件",
            &serde_json::json!({"documentId": upload.document_id, "assetId": upload.asset_id}),
        ) {
            Ok(running) => running,
            Err(_error)
                if db
                    .is_knowledge_job_cancel_requested(job_id)
                    .unwrap_or(false) =>
            {
                return finish_upload_cancelled(db, job_id);
            }
            Err(error) => {
                return db.fail_knowledge_document_upload_import_or_cancel(
                    job_id,
                    &truncate_error(&error.to_string()),
                );
            }
        };
        if running.cancel_requested || db.is_knowledge_job_cancel_requested(job_id)? {
            return finish_upload_cancelled(db, job_id);
        }
        let result: Result<Option<KnowledgeJob>, AppError> = (|| {
            let bytes = read_verified_asset(
                app_data_dir,
                &upload.storage_key,
                &upload.content_hash,
                upload.size_bytes,
            )?;
            let (parse_mime_type, text, binary_content, ocr_metadata) =
                if upload.mime_type.starts_with("image/") {
                    if upload.allow_remote_ocr {
                        let recognized =
                            tauri::async_runtime::block_on(AiProviderService::recognize_image(
                                db,
                                &upload.ocr_provider_key,
                                &upload.mime_type,
                                &bytes,
                            ))?;
                        if let Some(rule) = detect_sensitive_content(&recognized.text) {
                            return Err(AppError::InvalidInput(format!(
                                "图片 OCR 结果命中敏感内容规则（{rule}），未建立索引"
                            )));
                        }
                        (
                            "application/x-knowledge-ocr".to_string(),
                            recognized.text,
                            None,
                            Some(serde_json::json!({
                                "mode": "remote",
                                "providerKey": recognized.provider_key,
                                "model": recognized.model,
                                "consent": "upload_explicit",
                            })),
                        )
                    } else {
                        // 本机 OCR 是默认离线尝试：从受控资产读取，不发送网络。系统
                        // Vision/开发工具缺失、超时或没有文字时降级到元数据，而不丢弃图片。
                        match KnowledgeLocalOcrService::recognize_image(
                            app_data_dir,
                            &upload.mime_type,
                            &bytes,
                        )? {
                            LocalImageOcrOutcome::Recognized { engine, text } => {
                                if let Some(rule) = detect_sensitive_content(&text) {
                                    return Err(AppError::InvalidInput(format!(
                                        "图片 OCR 结果命中敏感内容规则（{rule}），未建立索引"
                                    )));
                                }
                                (
                                    "application/x-knowledge-local-ocr".to_string(),
                                    text,
                                    None,
                                    Some(serde_json::json!({
                                        "mode": "local",
                                        "engine": engine,
                                    })),
                                )
                            }
                            LocalImageOcrOutcome::Unavailable { reason } => (
                                upload.mime_type.clone(),
                                String::new(),
                                Some(bytes),
                                Some(serde_json::json!({
                                    "mode": "local_unavailable",
                                    "reason": reason,
                                })),
                            ),
                        }
                    }
                } else if upload.mime_type.starts_with("text/") {
                    String::from_utf8(bytes.clone())
                        .map_err(|_| AppError::InvalidInput("文本文件不是 UTF-8 编码".to_string()))
                        .map(|content| (upload.mime_type.clone(), content, Some(bytes), None))?
                } else {
                    (upload.mime_type.clone(), String::new(), Some(bytes), None)
                };
            let mut parsed =
                KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
                    document: KnowledgeParseInput {
                        source_path: upload.original_name.clone(),
                        mime_type: parse_mime_type,
                        content: text,
                        binary_content,
                    },
                    options: None,
                })?;
            if let Some(metadata) = &ocr_metadata {
                if metadata["mode"] == "remote" {
                    parsed
                        .parsed
                        .warnings
                        .push("正文由远程 OCR 识别，请保留图片原件复核".to_string());
                }
                if metadata["mode"] == "local" {
                    parsed
                        .parsed
                        .warnings
                        .push("正文由本机 OCR 识别，请保留图片原件复核".to_string());
                }
                if metadata["mode"] == "local_unavailable" {
                    let reason = metadata["reason"].as_str().unwrap_or("当前设备不可用");
                    parsed.parsed.warnings.push(format!(
                        "本机 OCR 暂不可用（{reason}），已仅按标题和图片元数据建立索引"
                    ));
                }
                parsed.parsed.front_matter["ocr"] = metadata.clone();
            }
            if db.is_knowledge_job_cancel_requested(job_id)? {
                return Ok(None);
            }
            db.ensure_knowledge_fts()?;
            let token_estimate = parsed.chunks.iter().map(|chunk| chunk.token_estimate).sum();
            let parser_id = parsed.parsed.parser_id.clone();
            let normalized_hash = format!(
                "{:x}",
                Sha256::digest(parsed.parsed.normalized_content.as_bytes())
            );
            let parse_artifact = NewKnowledgeDocumentParseArtifact {
                // 正式版本 ID 仅能在原子提交事务中分配，DAO 会在写入前将此字段替换为该 ID。
                document_version_id: 0,
                asset_id: Some(upload.asset_id),
                parser_id: parser_id.clone(),
                parser_version: parser_version(&parser_id),
                quality_level: if parsed.parsed.warnings.is_empty() {
                    "complete".to_string()
                } else {
                    "partial".to_string()
                },
                warning_json: serde_json::to_string(&parsed.parsed.warnings)?,
                normalized_hash,
                structure_json: serde_json::to_string(&serde_json::json!({
                    "normalizationVersion": parsed.parsed.normalization_version.clone(),
                    "frontMatter": parsed.parsed.front_matter.clone(),
                    "blocks": parsed.parsed.blocks.clone(),
                    "chunkStrategyId": parsed.chunk_strategy_id.clone(),
                }))?,
            };
            let completed = db.complete_knowledge_document_upload_import(
                CompleteKnowledgeDocumentUploadImport {
                    import_job_id: job_id,
                    version: &CreateKnowledgeDocumentVersionInput {
                        document_id: upload.document_id,
                        release_id: upload.release_id,
                        version_label: format!("上传-{}", &upload.content_hash[..12]),
                        git_branch: String::new(),
                        commit_sha: String::new(),
                        // 逻辑路径在上传登记时已与文档绑定；版本、搜索和引用必须共享同一来源事实。
                        source_path: upload.logical_path.clone(),
                        mime_type: upload.mime_type.clone(),
                        content: parsed.parsed.normalized_content.clone(),
                        content_hash: upload.content_hash.clone(),
                        parsed_meta: serde_json::json!({
                            "parserId": parser_id.clone(),
                            "normalizationVersion": parsed.parsed.normalization_version.clone(),
                            "chunkStrategyId": parsed.chunk_strategy_id.clone(),
                            "warnings": parsed.parsed.warnings.clone(),
                            "parseTimeoutSeconds": FILE_PARSE_TIMEOUT.as_secs(),
                            "ocr": ocr_metadata.clone(),
                        }),
                        token_estimate,
                    },
                    chunks: &parsed.chunks,
                    parse_artifact: &parse_artifact,
                    message: "上传文档已解析并建立索引",
                    checkpoint: &serde_json::json!({
                        "documentId": upload.document_id,
                        "assetId": upload.asset_id,
                        "stage": "completed",
                        "parserId": parser_id,
                        "ocr": ocr_metadata,
                    }),
                },
            )?;
            Ok(Some(completed.job))
        })();
        match result {
            Ok(Some(job)) => Ok(job),
            Ok(None) => finish_upload_cancelled(db, job_id),
            Err(error) => {
                let message = truncate_error(&error.to_string());
                db.fail_knowledge_document_upload_import_or_cancel(job_id, &message)
            }
        }
    }
}

fn read_verified_asset(
    root: &Path,
    storage_key: &str,
    content_hash: &str,
    size_bytes: i64,
) -> Result<Vec<u8>, AppError> {
    if storage_key.contains("..")
        || !storage_key.starts_with("sha256/")
        || content_hash.len() != 64
        || size_bytes < 0
    {
        return Err(AppError::InvalidInput("受控上传资产元数据无效".to_string()));
    }
    let path = root.join("knowledge-assets").join(storage_key);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != size_bytes as u64
    {
        return Err(AppError::InvalidInput(
            "受控上传资产已损坏或被替换".to_string(),
        ));
    }
    let bytes = fs::read(path)?;
    if format!("{:x}", Sha256::digest(&bytes)) != content_hash {
        return Err(AppError::InvalidInput(
            "受控上传资产哈希校验失败".to_string(),
        ));
    }
    Ok(bytes)
}

fn finish_upload_cancelled(db: &Database, job_id: i64) -> Result<KnowledgeJob, AppError> {
    db.cancel_knowledge_document_upload_import(job_id)
}

fn parser_version(parser_id: &str) -> String {
    parser_id
        .rsplit_once("-v")
        .map(|(_, version)| format!("v{version}"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn parse_and_store_document_chunks(
    db: &Database,
    document_version_id: i64,
    job_id: i64,
) -> Result<Option<KnowledgeJob>, AppError> {
    let version = db
        .get_knowledge_document_version_by_id(document_version_id)?
        .ok_or_else(|| AppError::NotFound(format!("知识文档版本不存在: {document_version_id}")))?;
    if db.is_knowledge_job_cancel_requested(job_id)? {
        return Ok(None);
    }
    // `replace_knowledge_document_chunks` 只在 FTS 表已经存在时同步分块。必须在
    // 写入前创建索引，避免首次全文搜索才创建空表而漏掉刚提交的文档。
    db.ensure_knowledge_fts()?;
    let parsed = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
        document: KnowledgeParseInput {
            source_path: version.source_path,
            mime_type: version.mime_type,
            content: version.content,
            binary_content: None,
        },
        options: None,
    })?;
    if db.is_knowledge_job_cancel_requested(job_id)? {
        return Ok(None);
    }
    let token_estimate = parsed
        .chunks
        .iter()
        .map(|chunk| chunk.token_estimate)
        .sum::<i64>();
    let parser_id = parsed.parsed.parser_id.clone();
    let chunk_count = i64::try_from(parsed.chunks.len())
        .map_err(|_| AppError::InvalidInput("文档分块数量超出支持范围".to_string()))?;
    let completed_checkpoint = document_index_checkpoint(
        document_version_id,
        "completed",
        chunk_count,
        chunk_count,
        Some(&parser_id),
    );
    let parsed_meta = serde_json::json!({
        "parserId": parser_id,
        "normalizationVersion": parsed.parsed.normalization_version,
        "chunkStrategyId": parsed.chunk_strategy_id,
        "frontMatter": parsed.parsed.front_matter,
        "warnings": parsed.parsed.warnings,
        "parseTimeoutSeconds": FILE_PARSE_TIMEOUT.as_secs(),
    });
    match db.replace_knowledge_document_chunks_and_finish_job(
        CompleteKnowledgeDocumentIndexJobInput {
            document_version_id,
            parsed_meta: &parsed_meta,
            token_estimate,
            chunks: &parsed.chunks,
            job_id,
            message: "文档索引已完成",
            checkpoint: &completed_checkpoint,
        },
    ) {
        Ok(job) => Ok(Some(job)),
        Err(_) if db.is_knowledge_job_cancel_requested(job_id)? => Ok(None),
        Err(error) => Err(error),
    }
}

fn finish_cancelled(
    db: &Database,
    document_version_id: i64,
    job_id: i64,
) -> Result<KnowledgeJob, AppError> {
    db.finish_knowledge_job(
        job_id,
        "cancelled",
        "文档索引已安全取消",
        None,
        &document_index_checkpoint(document_version_id, "cancelled", 0, 1, None),
    )
}

fn document_index_checkpoint(
    document_version_id: i64,
    stage: &str,
    current: i64,
    total: i64,
    parser_id: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "documentVersionId": document_version_id,
        "stage": stage,
        "current": current,
        "total": total,
        "parserId": parser_id,
    })
}

fn validate_positive_id(value: i64, field: &str) -> Result<(), AppError> {
    if value <= 0 {
        return Err(AppError::InvalidInput(format!("{field} 必须大于 0")));
    }
    Ok(())
}

fn truncate_error(error: &str) -> String {
    const LIMIT: usize = 500;
    let error = error.trim();
    if error.chars().count() <= LIMIT {
        return error.to_string();
    }
    format!("{}…", error.chars().take(LIMIT).collect::<String>())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::KnowledgeDocumentJobService;
    use crate::database::Database;
    use crate::models::{
        CommitKnowledgeDocumentDraftInput, CreateKnowledgeJobInput, KnowledgeDocumentDraftInput,
        KnowledgeSearchInput, UpsertKnowledgeProjectInput, UpsertKnowledgeReleaseInput,
    };
    use crate::services::knowledge_domain::documents::{
        KnowledgeDocumentService, KnowledgeUploadGrantRegistry,
    };
    use lopdf::dictionary;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn database() -> Result<Database, Box<dyn std::error::Error>> {
        Ok(Database::init(":memory:")?)
    }

    #[test]
    fn document_index_job_indexes_committed_content_and_finishes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "job-index".to_string(),
                name: "索引任务项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: None,
                project_id: project.id,
                title: "退款规则".to_string(),
                content: "# 退款规则\n\n退款需要审批。".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: None,
                editor_label: None,
            },
        )?
        .draft;
        let committed = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: draft.id,
                revision: draft.revision,
                version_label: "v1".to_string(),
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                commit_message: None,
                author_label: None,
            },
        )?;

        let finished = KnowledgeDocumentJobService::run_document_index_job(
            &database,
            committed.document_version_id,
            committed.index_job_id,
        )?;

        assert_eq!(finished.status, "completed");
        assert!(!database
            .list_knowledge_chunks(committed.document_version_id)?
            .is_empty());
        let hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "退款需要审批".to_string(),
            project_ids: vec![project.id],
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["markdown".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].citation.document_version_id,
            Some(committed.document_version_id)
        );
        Ok(())
    }

    #[test]
    fn cancelled_queued_document_index_job_does_not_write_chunks(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "job-cancelled".to_string(),
                name: "取消索引任务项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let draft = KnowledgeDocumentService::save_manual_draft(
            &database,
            KnowledgeDocumentDraftInput {
                draft_id: None,
                document_id: None,
                project_id: project.id,
                title: "待取消索引".to_string(),
                content: "# 待取消索引\n\n这段内容不应进入索引。".to_string(),
                doc_type: "markdown".to_string(),
                base_version_id: None,
                revision: None,
                editor_label: None,
            },
        )?
        .draft;
        let committed = KnowledgeDocumentService::commit_manual_draft(
            &database,
            CommitKnowledgeDocumentDraftInput {
                draft_id: draft.id,
                revision: draft.revision,
                version_label: "v1".to_string(),
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                commit_message: None,
                author_label: None,
            },
        )?;

        let cancelled = database.request_knowledge_job_cancel(committed.index_job_id)?;
        let result = KnowledgeDocumentJobService::run_document_index_job(
            &database,
            committed.document_version_id,
            committed.index_job_id,
        )?;

        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(result.status, "cancelled");
        assert!(database
            .list_knowledge_chunks(committed.document_version_id)?
            .is_empty());
        Ok(())
    }

    #[test]
    fn failed_document_index_completion_preserves_a_prior_cancel_request(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let checkpoint = serde_json::json!({"stage": "parse"});
        let job = database.create_knowledge_job(&CreateKnowledgeJobInput {
            job_key: "document-index-failure-cancel".to_string(),
            job_type: "document_index".to_string(),
            source_id: None,
            profile_id: None,
            message: "等待建立文档索引".to_string(),
            checkpoint: checkpoint.clone(),
        })?;
        database.mark_knowledge_job_running(job.id, "parse", "正在解析", &checkpoint)?;
        database.request_knowledge_job_cancel(job.id)?;

        let finished = database.fail_knowledge_document_index_job_or_cancel(
            crate::database::knowledge::FailKnowledgeDocumentIndexJobInput {
                job_id: job.id,
                error: "模拟解析错误",
                failed_checkpoint: &serde_json::json!({"stage": "failed"}),
                cancelled_checkpoint: &serde_json::json!({"stage": "cancelled"}),
            },
        )?;

        assert_eq!(finished.status, "cancelled");
        assert_eq!(finished.checkpoint["stage"], "cancelled");
        assert!(finished.error.is_none());
        Ok(())
    }

    #[test]
    fn upload_import_indexes_verified_managed_markdown_asset(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("tauri-knowledge-upload-job-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        let database_path = root.join("knowledge.db");
        let database = Database::init(database_path.to_str().ok_or("测试路径必须为 UTF-8")?)?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "upload-job".to_string(),
                name: "上传任务项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let source = root.join("上传说明.md");
        fs::write(&source, "# 上传说明\n\n上传文档需要建立索引。")?;
        let grants = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &grants,
            source.to_str().ok_or("测试路径必须为 UTF-8")?,
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &grants,
            crate::models::UploadKnowledgeAssetInput {
                project_id: project.id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        let finished = super::KnowledgeUploadImportJobService::run_upload_import_job(
            &database,
            &root,
            upload.import_job_id,
        )?;
        assert_eq!(finished.status, "completed");
        assert_eq!(
            database
                .list_knowledge_document_versions(upload.document_id)?
                .len(),
            1
        );
        let version = database
            .list_knowledge_document_versions(upload.document_id)?
            .into_iter()
            .next()
            .expect("上传成功后必须保留正式版本");
        let artifacts = database.list_knowledge_document_comparison_artifacts(version.id)?;
        assert_eq!(artifacts.len(), 1, "上传解析必须保留可追溯的解析产物");
        assert_eq!(artifacts[0].parser_id, "markdown-parser-v1");
        assert_eq!(artifacts[0].quality_level, "complete");
        assert_eq!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "上传文档需要建立索引".to_string(),
                    project_ids: vec![project.id],
                    release_ids: Vec::new(),
                    source_ids: Vec::new(),
                    document_types: vec!["markdown".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .len(),
            1
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_import_sanitizes_html_prototype_and_indexes_visible_content(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-html-upload-job-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root)?;
        let database_path = root.join("knowledge.db");
        let database = Database::init(database_path.to_str().ok_or("测试路径必须为 UTF-8")?)?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "html-upload-job".to_string(),
                name: "HTML 原型上传项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let source = root.join("订单原型.html");
        fs::write(
            &source,
            "<html><head><title>订单创建原型</title><script>steal()</script></head>\
             <body><h1>创建订单</h1><p>需要校验客户状态。</p></body></html>",
        )?;
        let grants = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &grants,
            source.to_str().ok_or("测试路径必须为 UTF-8")?,
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &grants,
            crate::models::UploadKnowledgeAssetInput {
                project_id: project.id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: Some("订单原型".to_string()),
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        // 导入登记后立即移除用户原始路径。后台任务必须只从内容寻址资产读取，
        // 不能因未来实现回读用户文件而在文件被替换后解析出不同内容。
        fs::remove_file(&source)?;

        let finished = super::KnowledgeUploadImportJobService::run_upload_import_job(
            &database,
            &root,
            upload.import_job_id,
        )?;

        assert_eq!(finished.status, "completed");
        let version = database
            .list_knowledge_document_versions(upload.document_id)?
            .into_iter()
            .next()
            .expect("HTML 上传成功后必须保留正式版本");
        let document = database
            .get_knowledge_document_by_id(upload.document_id)?
            .expect("HTML 上传成功后必须保留文档记录");
        assert_eq!(document.source_folder_name.as_deref(), Some("订单原型"));
        assert_eq!(
            document.logical_path,
            "upload-folder/订单原型/订单原型.html"
        );
        assert_eq!(version.source_path, "upload-folder/订单原型/订单原型.html");
        assert!(!version.content.contains("steal"));
        let artifacts = database.list_knowledge_document_comparison_artifacts(version.id)?;
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].parser_id, "html-parser-v1");
        let hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "校验客户状态".to_string(),
            project_ids: vec![project.id],
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["html".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].citation.logical_path,
            "upload-folder/订单原型/订单原型.html"
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "steal".to_string(),
                    project_ids: vec![project.id],
                    release_ids: Vec::new(),
                    source_ids: Vec::new(),
                    document_types: vec!["html".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "HTML 脚本内容不得进入全文索引"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_import_indexes_managed_docx_asset_without_external_references(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-docx-upload-job-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root)?;
        let database_path = root.join("knowledge.db");
        let database = Database::init(database_path.to_str().ok_or("测试路径必须为 UTF-8")?)?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "docx-upload-job".to_string(),
                name: "DOCX 上传项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let source = root.join("退款审批设计.docx");
        let file = fs::File::create(&source)?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in [
            ("[Content_Types].xml", "<Types/>"),
            (
                "word/document.xml",
                r#"<w:document xmlns:w="w" xmlns:r="r" xmlns:a="a"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>退款审批设计</w:t></w:r></w:p>
<w:p><w:r><w:t>提交退款前必须校验订单状态。</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>审批人需要记录原因</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>字段</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>规则</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:drawing><a:blip r:embed="rIdSafe"/></w:drawing></w:r></w:p>
<w:p><w:r><w:drawing><a:blip r:embed="rIdExternal"/></w:drawing></w:r></w:p>
</w:body></w:document>"#,
            ),
            (
                "word/_rels/document.xml.rels",
                r#"<Relationships><Relationship Id="rIdSafe" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/approval.png"/><Relationship Id="rIdExternal" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="https://untrusted.invalid/leak.png" TargetMode="External"/></Relationships>"#,
            ),
            ("word/activeX/control.bin", "never execute"),
        ] {
            archive.start_file(name, options)?;
            archive.write_all(content.as_bytes())?;
        }
        archive.finish()?;

        let grants = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &grants,
            source.to_str().ok_or("测试路径必须为 UTF-8")?,
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &grants,
            crate::models::UploadKnowledgeAssetInput {
                project_id: project.id,
                project_version_id: None,
                cross_version_scope: Some("project_all_versions".to_string()),
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        // 导入登记后用户原始文件可以删除。任务只能消费已哈希校验的受管资产，
        // 从而防止原路径被替换后将未核验内容写入正式版本与全文索引。
        fs::remove_file(&source)?;

        let finished = super::KnowledgeUploadImportJobService::run_upload_import_job(
            &database,
            &root,
            upload.import_job_id,
        )?;

        assert_eq!(finished.status, "completed");
        let document = database
            .get_knowledge_document_by_id(upload.document_id)?
            .expect("DOCX 上传成功后必须保留文档记录");
        assert_eq!(document.status, "active");
        assert_eq!(document.doc_type, "docx");
        let version = database
            .list_knowledge_document_versions(upload.document_id)?
            .into_iter()
            .next()
            .expect("DOCX 上传成功后必须生成正式版本");
        assert_eq!(document.latest_version_id, Some(version.id));
        assert_eq!(
            version.mime_type,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert!(version.content.contains("提交退款前必须校验订单状态"));
        assert!(!version.content.contains("untrusted.invalid"));
        assert_eq!(version.parsed_meta["parserId"], "docx-parser-v1");
        assert!(version.parsed_meta["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings.iter().any(|warning| warning
                .as_str()
                .is_some_and(|value| value.contains("ActiveX")))));
        let artifacts = database.list_knowledge_document_comparison_artifacts(version.id)?;
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].parser_id, "docx-parser-v1");
        assert_eq!(artifacts[0].parser_version, "v1");
        assert_eq!(artifacts[0].quality_level, "partial");
        let chunks = database.list_knowledge_chunks(version.id)?;
        assert!(!chunks.is_empty(), "DOCX 正式版本必须生成可检索分块");
        assert!(chunks
            .iter()
            .any(|chunk| chunk.content.contains("校验订单状态")));

        let matching_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "校验订单状态".to_string(),
            project_ids: vec![project.id],
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["docx".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(matching_hits.len(), 1);
        assert_eq!(
            matching_hits[0].citation.document_version_id,
            Some(version.id)
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "untrusted".to_string(),
                    project_ids: vec![project.id],
                    release_ids: Vec::new(),
                    source_ids: Vec::new(),
                    document_types: vec!["docx".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "DOCX 外部关系不得进入全文索引"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_import_indexes_managed_xlsx_asset_without_external_references(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-xlsx-upload-job-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root)?;
        let database_path = root.join("knowledge.db");
        let database = Database::init(database_path.to_str().ok_or("测试路径必须为 UTF-8")?)?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "xlsx-upload-job".to_string(),
                name: "XLSX 上传项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let target_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "release/v1.0.0".to_string(),
            commit_sha: "1".repeat(40),
            description: "XLSX 归属版本".to_string(),
            released_at: None,
        })?;
        let other_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v2.0.0".to_string(),
            tag_name: "v2.0.0".to_string(),
            branch: "release/v2.0.0".to_string(),
            commit_sha: "2".repeat(40),
            description: "不应命中的其他版本".to_string(),
            released_at: None,
        })?;
        let source = root.join("退款规则.xlsx");
        let file = fs::File::create(&source)?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in [
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="需求统计" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>事项</t></is></c><c r="B1" t="inlineStr"><is><t>规则</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>退款审批</t></is></c><c r="B2" t="inlineStr"><is><t>订单退款必须校验状态</t></is></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdExternal" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://untrusted.invalid/external-link" TargetMode="External"/></Relationships>"#,
            ),
        ] {
            archive.start_file(name, options)?;
            archive.write_all(content.as_bytes())?;
        }
        archive.finish()?;

        let grants = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &grants,
            source.to_str().ok_or("测试路径必须为 UTF-8")?,
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &grants,
            crate::models::UploadKnowledgeAssetInput {
                project_id: project.id,
                project_version_id: Some(target_release.id),
                cross_version_scope: None,
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        // 用户原路径在导入登记后不再可信；任务必须从内容寻址受管资产读取 XLSX。
        fs::remove_file(&source)?;

        let finished = super::KnowledgeUploadImportJobService::run_upload_import_job(
            &database,
            &root,
            upload.import_job_id,
        )?;

        assert_eq!(finished.status, "completed");
        let document = database
            .get_knowledge_document_by_id(upload.document_id)?
            .expect("XLSX 上传成功后必须保留文档记录");
        assert_eq!(document.status, "active");
        assert_eq!(document.doc_type, "xlsx");
        let version = database
            .list_knowledge_document_versions(upload.document_id)?
            .into_iter()
            .next()
            .expect("XLSX 上传成功后必须生成正式版本");
        assert_eq!(document.latest_version_id, Some(version.id));
        assert_eq!(version.release_id, Some(target_release.id));
        let bindings = database.list_knowledge_document_version_bindings(version.id)?;
        assert_eq!(bindings.len(), 1, "正式版本必须只有一个明确范围绑定");
        assert_eq!(bindings[0].release_id, Some(target_release.id));
        assert!(bindings[0].cross_version_scope.is_empty());
        assert_eq!(
            version.mime_type,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert!(version
            .content
            .contains("需求统计!B2: 订单退款必须校验状态"));
        assert!(!version.content.contains("untrusted.invalid"));
        assert_eq!(version.parsed_meta["parserId"], "xlsx-parser-v1");
        let artifacts = database.list_knowledge_document_comparison_artifacts(version.id)?;
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].parser_id, "xlsx-parser-v1");
        assert_eq!(artifacts[0].parser_version, "v1");
        assert_eq!(artifacts[0].quality_level, "complete");
        let chunks = database.list_knowledge_chunks(version.id)?;
        assert!(!chunks.is_empty(), "XLSX 正式版本必须生成可检索分块");
        assert!(chunks
            .iter()
            .any(|chunk| chunk.content.contains("订单退款必须校验状态")));

        let matching_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "订单退款必须校验状态".to_string(),
            project_ids: vec![project.id],
            release_ids: vec![target_release.id],
            source_ids: Vec::new(),
            document_types: vec!["xlsx".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(matching_hits.len(), 1);
        assert_eq!(
            matching_hits[0].citation.document_version_id,
            Some(version.id)
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "订单退款必须校验状态".to_string(),
                    project_ids: vec![project.id],
                    release_ids: vec![other_release.id],
                    source_ids: Vec::new(),
                    document_types: vec!["xlsx".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "绑定具体项目版本的 XLSX 不得被其他版本检索到"
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "untrusted".to_string(),
                    project_ids: vec![project.id],
                    release_ids: vec![target_release.id],
                    source_ids: Vec::new(),
                    document_types: vec!["xlsx".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "XLSX 外部关系不得进入正式正文或全文索引"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_import_indexes_managed_pptx_asset_in_its_bound_release_only(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-pptx-upload-job-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root)?;
        let database_path = root.join("knowledge.db");
        let database = Database::init(database_path.to_str().ok_or("测试路径必须为 UTF-8")?)?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "pptx-upload-job".to_string(),
                name: "PPTX 上传项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let target_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "release/v1.0.0".to_string(),
            commit_sha: "3".repeat(40),
            description: "PPTX 归属版本".to_string(),
            released_at: None,
        })?;
        let other_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v2.0.0".to_string(),
            tag_name: "v2.0.0".to_string(),
            branch: "release/v2.0.0".to_string(),
            commit_sha: "4".repeat(40),
            description: "不应命中的其他版本".to_string(),
            released_at: None,
        })?;
        let other_project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "pptx-upload-job-isolation".to_string(),
                name: "PPTX 跨项目隔离项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let other_project_release =
            database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
                id: None,
                project_id: other_project.id,
                version: "v1.0.0".to_string(),
                tag_name: "v1.0.0".to_string(),
                branch: "release/v1.0.0".to_string(),
                commit_sha: "5".repeat(40),
                description: "跨项目隔离版本".to_string(),
                released_at: None,
            })?;
        let source = root.join("发布验证说明.pptx");
        let file = fs::File::create(&source)?;
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in [
            ("[Content_Types].xml", r#"<Types/>"#),
            (
                "_rels/.rels",
                r#"<Relationships><Relationship Id="rIdOffice" Type="officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
            ),
            (
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rIdSlide"/></p:sldIdLst></p:presentation>"#,
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships><Relationship Id="rIdSlide" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>发布验证说明</a:t></a:r></a:p><a:p><a:r><a:t>发版前必须校验 API 兼容性</a:t></a:r></a:p></p:txBody><p:blipFill><a:blip r:link="rIdExternalImage"/></p:blipFill></p:sp></p:spTree></p:cSld></p:sld>"#,
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                r#"<Relationships><Relationship Id="rIdExternalLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://untrusted.invalid/release-guide" TargetMode="External"/><Relationship Id="rIdExternalImage" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="https://untrusted.invalid/remote-image.png" TargetMode="External"/></Relationships>"#,
            ),
        ] {
            archive.start_file(name, options)?;
            archive.write_all(content.as_bytes())?;
        }
        archive.finish()?;

        let grants = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &grants,
            source.to_str().ok_or("测试路径必须为 UTF-8")?,
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &grants,
            crate::models::UploadKnowledgeAssetInput {
                project_id: project.id,
                project_version_id: Some(target_release.id),
                cross_version_scope: None,
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        // 登记后删除用户原路径，验证后台任务只消费内容寻址的受管资产，
        // 不会在未来回读或解析用户可替换的源文件。
        fs::remove_file(&source)?;

        let finished = super::KnowledgeUploadImportJobService::run_upload_import_job(
            &database,
            &root,
            upload.import_job_id,
        )?;

        assert_eq!(finished.status, "completed");
        let document = database
            .get_knowledge_document_by_id(upload.document_id)?
            .expect("PPTX 上传成功后必须保留文档记录");
        assert_eq!(document.status, "active");
        assert_eq!(document.doc_type, "pptx");
        let version = database
            .list_knowledge_document_versions(upload.document_id)?
            .into_iter()
            .next()
            .expect("PPTX 上传成功后必须生成正式版本");
        assert_eq!(document.latest_version_id, Some(version.id));
        assert_eq!(version.release_id, Some(target_release.id));
        let bindings = database.list_knowledge_document_version_bindings(version.id)?;
        assert_eq!(bindings.len(), 1, "正式版本必须只有一个明确范围绑定");
        assert_eq!(bindings[0].release_id, Some(target_release.id));
        assert!(bindings[0].cross_version_scope.is_empty());
        assert_eq!(
            version.mime_type,
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        );
        assert!(version.content.contains("发版前必须校验 API 兼容性"));
        assert!(!version.content.contains("untrusted.invalid"));
        assert!(!version.content.contains("rIdExternalImage"));
        assert_eq!(version.parsed_meta["parserId"], "pptx-parser-v1");
        let artifacts = database.list_knowledge_document_comparison_artifacts(version.id)?;
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].parser_id, "pptx-parser-v1");
        assert_eq!(artifacts[0].parser_version, "v1");
        assert_eq!(artifacts[0].quality_level, "complete");
        // 比较视图刻意不返回内部 assetId 与结构 JSON；这里通过独立只读连接核验
        // 任务提交的真实持久化记录，确保上传资产与解析结构没有在事务中脱钩。
        let (artifact_asset_id, structure_json): (Option<i64>, String) =
            rusqlite::Connection::open(&database_path)?.query_row(
                "SELECT asset_id, structure_json
                 FROM knowledge_document_parse_artifacts
                 WHERE document_version_id = ?1",
                [version.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
        assert_eq!(artifact_asset_id, Some(upload.asset_id));
        assert!(!structure_json.contains("untrusted.invalid"));
        assert!(!structure_json.contains("rIdExternalImage"));
        let structure: serde_json::Value = serde_json::from_str(&structure_json)?;
        assert_eq!(structure["frontMatter"]["slideCount"], 1);
        assert!(structure["blocks"]
            .as_array()
            .is_some_and(|blocks| blocks.iter().any(|block| {
                block["blockType"] == "slide"
                    && block["metadata"]["slideNumber"] == 1
                    && block["content"]
                        .as_str()
                        .is_some_and(|content| content.contains("发版前必须校验 API 兼容性"))
            })));
        let chunks = database.list_knowledge_chunks(version.id)?;
        assert!(!chunks.is_empty(), "PPTX 正式版本必须生成可检索分块");
        assert!(chunks
            .iter()
            .any(|chunk| chunk.content.contains("校验 API 兼容性")));

        let matching_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "发版前必须校验 API 兼容性".to_string(),
            project_ids: vec![project.id],
            release_ids: vec![target_release.id],
            source_ids: Vec::new(),
            document_types: vec!["pptx".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(matching_hits.len(), 1);
        assert_eq!(
            matching_hits[0].citation.document_version_id,
            Some(version.id)
        );
        assert_eq!(matching_hits[0].citation.project_id, Some(project.id));
        assert_eq!(
            matching_hits[0].citation.release_id,
            Some(target_release.id)
        );
        let cited_chunk_id = matching_hits[0]
            .citation
            .chunk_id
            .expect("全文检索必须返回实际命中分块的引用");
        assert!(chunks.iter().any(|chunk| chunk.id == cited_chunk_id));
        assert_eq!(
            matching_hits[0].citation.citation_key,
            format!(
                "document:{}:version:{}:chunk:{cited_chunk_id}",
                document.id, version.id
            )
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "发版前必须校验 API 兼容性".to_string(),
                    project_ids: vec![project.id],
                    release_ids: vec![other_release.id],
                    source_ids: Vec::new(),
                    document_types: vec!["pptx".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "绑定具体项目版本的 PPTX 不得被其他版本检索到"
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "发版前必须校验 API 兼容性".to_string(),
                    project_ids: vec![other_project.id],
                    release_ids: Vec::new(),
                    source_ids: Vec::new(),
                    document_types: vec!["pptx".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "PPTX 不得跨项目被检索，项目过滤本身必须生效"
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "发版前必须校验 API 兼容性".to_string(),
                    project_ids: vec![other_project.id],
                    release_ids: vec![other_project_release.id],
                    source_ids: Vec::new(),
                    document_types: vec!["pptx".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "PPTX 不得跨项目被检索，即使筛选了另一个项目的有效版本"
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "untrusted".to_string(),
                    project_ids: vec![project.id],
                    release_ids: vec![target_release.id],
                    source_ids: Vec::new(),
                    document_types: vec!["pptx".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "PPTX 外部关系不得进入正式正文或全文索引"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn upload_import_indexes_managed_pdf_asset_in_its_bound_release_only(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-pdf-upload-job-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root)?;
        let database_path = root.join("knowledge.db");
        let database = Database::init(database_path.to_str().ok_or("测试路径必须为 UTF-8")?)?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "pdf-upload-job".to_string(),
                name: "PDF 上传项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let target_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "release/v1.0.0".to_string(),
            commit_sha: "6".repeat(40),
            description: "PDF 归属版本".to_string(),
            released_at: None,
        })?;
        let other_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v2.0.0".to_string(),
            tag_name: "v2.0.0".to_string(),
            branch: "release/v2.0.0".to_string(),
            commit_sha: "7".repeat(40),
            description: "不应命中的其他版本".to_string(),
            released_at: None,
        })?;

        // 使用含文字层的最小 PDF，确保这是一条真实的二进制上传解析路径；链接注释
        // 刻意带有外部 URI，用于验证解析器不会把不可执行的 PDF 动作写入知识正文。
        let source = root.join("发布验收说明.pdf");
        let mut pdf = lopdf::Document::with_version("1.5");
        let pages_id = pdf.new_object_id();
        let font_id = pdf.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
        });
        let resources_id = pdf.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });
        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("BT", vec![]),
                lopdf::content::Operation::new("Tf", vec!["F1".into(), 18.into()]),
                lopdf::content::Operation::new("Td", vec![72.into(), 720.into()]),
                lopdf::content::Operation::new(
                    "Tj",
                    vec![lopdf::Object::string_literal(
                        "Release verification requires database compatibility",
                    )],
                ),
                lopdf::content::Operation::new("ET", vec![]),
            ],
        };
        let content_id = pdf.add_object(lopdf::Stream::new(dictionary! {}, content.encode()?));
        let external_link_id = pdf.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
            "A" => dictionary! {
                "S" => "URI",
                "URI" => lopdf::Object::string_literal("https://untrusted.invalid/pdf-action"),
            },
        });
        let page_id = pdf.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Annots" => vec![external_link_id.into()],
        });
        pdf.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = pdf.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages_id,
        });
        pdf.trailer.set("Root", catalog_id);
        let mut pdf_bytes = Vec::new();
        pdf.save_to(&mut pdf_bytes)?;
        fs::write(&source, pdf_bytes)?;

        let grants = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &grants,
            source.to_str().ok_or("测试路径必须为 UTF-8")?,
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &grants,
            crate::models::UploadKnowledgeAssetInput {
                project_id: project.id,
                project_version_id: Some(target_release.id),
                cross_version_scope: None,
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        // 受管资产完成内容哈希校验后，用户原路径不能再影响后台解析结果。
        fs::remove_file(&source)?;

        let finished = super::KnowledgeUploadImportJobService::run_upload_import_job(
            &database,
            &root,
            upload.import_job_id,
        )?;

        assert_eq!(finished.status, "completed");
        let document = database
            .get_knowledge_document_by_id(upload.document_id)?
            .expect("PDF 上传成功后必须保留文档记录");
        assert_eq!(document.status, "active");
        assert_eq!(document.doc_type, "pdf");
        let version = database
            .list_knowledge_document_versions(upload.document_id)?
            .into_iter()
            .next()
            .expect("PDF 上传成功后必须生成正式版本");
        assert_eq!(document.latest_version_id, Some(version.id));
        assert_eq!(version.release_id, Some(target_release.id));
        assert_eq!(version.mime_type, "application/pdf");
        assert!(version
            .content
            .contains("Release verification requires database compatibility"));
        assert!(!version.content.contains("untrusted.invalid"));
        assert_eq!(version.parsed_meta["parserId"], "pdf-parser-v1");
        let bindings = database.list_knowledge_document_version_bindings(version.id)?;
        assert_eq!(bindings.len(), 1, "正式版本必须只有一个明确范围绑定");
        assert_eq!(bindings[0].release_id, Some(target_release.id));
        assert!(bindings[0].cross_version_scope.is_empty());

        let artifacts = database.list_knowledge_document_comparison_artifacts(version.id)?;
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].parser_id, "pdf-parser-v1");
        assert_eq!(artifacts[0].parser_version, "v1");
        assert_eq!(artifacts[0].quality_level, "complete");
        let (artifact_asset_id, structure_json): (Option<i64>, String) =
            rusqlite::Connection::open(&database_path)?.query_row(
                "SELECT asset_id, structure_json
                 FROM knowledge_document_parse_artifacts
                 WHERE document_version_id = ?1",
                [version.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
        assert_eq!(artifact_asset_id, Some(upload.asset_id));
        assert!(!structure_json.contains("untrusted.invalid"));
        let structure: serde_json::Value = serde_json::from_str(&structure_json)?;
        assert_eq!(structure["frontMatter"]["pageCount"], 1);
        assert!(structure["blocks"]
            .as_array()
            .is_some_and(|blocks| blocks.iter().any(|block| {
                block["blockType"] == "pdf_page"
                    && block["metadata"]["pageNumber"] == 1
                    && block["metadata"]["requiresOcr"] == false
                    && block["content"].as_str().is_some_and(|content| {
                        content.contains("Release verification requires database compatibility")
                    })
            })));
        let chunks = database.list_knowledge_chunks(version.id)?;
        assert!(!chunks.is_empty(), "PDF 正式版本必须生成可检索分块");
        assert!(chunks
            .iter()
            .any(|chunk| chunk.content.contains("database compatibility")));

        let matching_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "Release verification requires database compatibility".to_string(),
            project_ids: vec![project.id],
            release_ids: vec![target_release.id],
            source_ids: Vec::new(),
            document_types: vec!["pdf".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(matching_hits.len(), 1);
        assert_eq!(
            matching_hits[0].citation.document_version_id,
            Some(version.id)
        );
        assert_eq!(matching_hits[0].citation.project_id, Some(project.id));
        assert_eq!(
            matching_hits[0].citation.release_id,
            Some(target_release.id)
        );
        let cited_chunk_id = matching_hits[0]
            .citation
            .chunk_id
            .expect("全文检索必须返回实际命中分块的引用");
        assert!(chunks.iter().any(|chunk| chunk.id == cited_chunk_id));
        assert_eq!(
            matching_hits[0].citation.citation_key,
            format!(
                "document:{}:version:{}:chunk:{cited_chunk_id}",
                document.id, version.id
            )
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "Release verification requires database compatibility".to_string(),
                    project_ids: vec![project.id],
                    release_ids: vec![other_release.id],
                    source_ids: Vec::new(),
                    document_types: vec!["pdf".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "绑定具体项目版本的 PDF 不得被其他版本检索到"
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "untrusted".to_string(),
                    project_ids: vec![project.id],
                    release_ids: vec![target_release.id],
                    source_ids: Vec::new(),
                    document_types: vec!["pdf".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "PDF 外部动作不得进入正式正文或全文索引"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn image_upload_without_remote_ocr_consent_degrades_to_metadata_when_local_ocr_cannot_read_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-image-ocr-policy-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root)?;
        let database_path = root.join("knowledge.db");
        let database = Database::init(database_path.to_str().ok_or("测试路径必须为 UTF-8")?)?;
        let project = crate::services::knowledge::KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "image-ocr-policy".to_string(),
                name: "图片 OCR 策略".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            },
        )?;
        let target_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "release/v1.0.0".to_string(),
            commit_sha: "8".repeat(40),
            description: "图片 OCR 策略归属版本".to_string(),
            released_at: None,
        })?;
        let other_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v2.0.0".to_string(),
            tag_name: "v2.0.0".to_string(),
            branch: "release/v2.0.0".to_string(),
            commit_sha: "9".repeat(40),
            description: "不应命中的其他版本".to_string(),
            released_at: None,
        })?;
        let source = root.join("流程图.png");
        // 使用真实可解码的 1×1 PNG。图中没有可识别文字，使本机 OCR 的缺失能力和
        // 远程 OCR 未授权的降级路径均能在受管二进制资产上被稳定验证。
        fs::write(
            &source,
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\0\0\0\rIDATx\xdac\xf8\xcf\xc0\xf0\x1f\0\x05\0\x01\xff\x89\x99=\x1d\0\0\0\0IEND\xaeB`\x82",
        )?;
        let grants = KnowledgeUploadGrantRegistry::default();
        let prepared = KnowledgeDocumentService::prepare_upload_file(
            &grants,
            source.to_str().ok_or("测试路径必须为 UTF-8")?,
        )?;
        let upload = KnowledgeDocumentService::create_upload_import(
            &database,
            &root,
            &grants,
            crate::models::UploadKnowledgeAssetInput {
                project_id: project.id,
                project_version_id: Some(target_release.id),
                cross_version_scope: None,
                file_handle: prepared.file_handle,
                display_name: None,
                source_folder_name: None,
                allow_remote_ocr: false,
                ocr_provider_key: None,
            },
        )?;
        // 导入登记后删除用户原路径，证明任务只依赖已哈希校验并内容寻址的受管资产。
        fs::remove_file(&source)?;

        let finished = super::KnowledgeUploadImportJobService::run_upload_import_job(
            &database,
            &root,
            upload.import_job_id,
        )?;

        assert_eq!(finished.status, "completed");
        let document = database
            .get_knowledge_document_by_id(upload.document_id)?
            .expect("图片上传成功后必须保留文档记录");
        assert_eq!(document.status, "active");
        assert_eq!(document.doc_type, "image");
        let version = database
            .list_knowledge_document_versions(upload.document_id)?
            .into_iter()
            .next()
            .expect("本机 OCR 无法读取时图片也必须保留正式版本");
        assert_eq!(document.latest_version_id, Some(version.id));
        assert_eq!(version.release_id, Some(target_release.id));
        assert_eq!(version.mime_type, "image/png");
        assert!(version.content.contains("流程图.png"));
        assert!(version.content.contains("1 × 1"));
        let bindings = database.list_knowledge_document_version_bindings(version.id)?;
        assert_eq!(bindings.len(), 1, "正式版本必须只有一个明确范围绑定");
        assert_eq!(bindings[0].release_id, Some(target_release.id));
        assert!(bindings[0].cross_version_scope.is_empty());
        let artifacts = database.list_knowledge_document_comparison_artifacts(version.id)?;
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].parser_id, "image-metadata-parser-v1");
        assert_eq!(artifacts[0].quality_level, "partial");
        // 比较视图不暴露受管资产主键；通过只读连接核验解析产物仍绑定本次上传的资产。
        let (artifact_asset_id, structure_json): (Option<i64>, String) =
            rusqlite::Connection::open(&database_path)?.query_row(
                "SELECT asset_id, structure_json
                 FROM knowledge_document_parse_artifacts
                 WHERE document_version_id = ?1",
                [version.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
        assert_eq!(artifact_asset_id, Some(upload.asset_id));
        let structure: serde_json::Value = serde_json::from_str(&structure_json)?;
        assert_eq!(structure["frontMatter"]["assetKind"], "image");
        assert_eq!(structure["frontMatter"]["width"], 1);
        assert_eq!(structure["frontMatter"]["height"], 1);
        assert_eq!(structure["frontMatter"]["textExtraction"], "unavailable");
        assert_eq!(version.parsed_meta["ocr"]["mode"], "local_unavailable");
        assert_ne!(version.parsed_meta["ocr"]["mode"], "remote");
        let chunks = database.list_knowledge_chunks(version.id)?;
        assert!(!chunks.is_empty(), "图片元数据正式版本必须生成可检索分块");
        assert!(chunks
            .iter()
            .any(|chunk| chunk.content.contains("未获得 OCR 正文")));

        let matching_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "未获得".to_string(),
            project_ids: vec![project.id],
            release_ids: vec![target_release.id],
            source_ids: Vec::new(),
            document_types: vec!["image".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(matching_hits.len(), 1);
        assert_eq!(
            matching_hits[0].citation.document_version_id,
            Some(version.id)
        );
        assert_eq!(matching_hits[0].citation.project_id, Some(project.id));
        assert_eq!(
            matching_hits[0].citation.release_id,
            Some(target_release.id)
        );
        let cited_chunk_id = matching_hits[0]
            .citation
            .chunk_id
            .expect("全文检索必须返回实际命中图片元数据分块的引用");
        assert!(chunks
            .iter()
            .any(|chunk| chunk.id == cited_chunk_id && chunk.content.contains("未获得 OCR 正文")));
        assert_eq!(
            matching_hits[0].citation.citation_key,
            format!(
                "document:{}:version:{}:chunk:{cited_chunk_id}",
                document.id, version.id
            )
        );
        assert!(
            database
                .search_knowledge_fts(&KnowledgeSearchInput {
                    query: "未获得".to_string(),
                    project_ids: vec![project.id],
                    release_ids: vec![other_release.id],
                    source_ids: Vec::new(),
                    document_types: vec!["image".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(10),
                    include_context: Some(true),
                })?
                .is_empty(),
            "绑定具体项目版本的图片元数据不得被其他版本检索到"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
