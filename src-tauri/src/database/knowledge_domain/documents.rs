use rusqlite::{params, OptionalExtension, Transaction};

use crate::database::knowledge::{insert_chunks, sync_document_fts_if_available};
use crate::database::knowledge_domain::search::sync_knowledge_document_title_index;
use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    CreateKnowledgeDocumentVersionInput, KnowledgeChunkWriteInput,
    KnowledgeDocumentComparisonArtifact, KnowledgeJob, KnowledgeParseAndChunkResult,
};
use sha2::{Digest, Sha256};

pub(crate) const DOMAIN: &str = "documents";

/// 原始文件只保存在受控存储中；这里保存可审计的内容寻址元数据，不暴露绝对路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeAssetRecord {
    pub id: i64,
    pub asset_key: String,
    pub content_hash: String,
    pub storage_key: String,
    pub original_name: String,
    pub normalized_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub reference_count: i64,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewKnowledgeAsset {
    pub asset_key: String,
    pub content_hash: String,
    pub storage_key: String,
    pub original_name: String,
    pub normalized_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeDocumentDraftRecord {
    pub id: i64,
    pub document_id: Option<i64>,
    pub project_id: i64,
    pub title: String,
    pub content: String,
    pub doc_type: String,
    pub base_version_id: Option<i64>,
    pub revision: i64,
    pub editor_label: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewKnowledgeDocumentDraft {
    pub document_id: Option<i64>,
    pub project_id: i64,
    pub title: String,
    pub content: String,
    pub doc_type: String,
    pub base_version_id: Option<i64>,
    pub editor_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeDocumentVersionBindingRecord {
    pub id: i64,
    pub document_version_id: i64,
    pub release_id: Option<i64>,
    pub repository_binding_id: Option<i64>,
    pub cross_version_scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewKnowledgeDocumentParseArtifact {
    pub document_version_id: i64,
    pub asset_id: Option<i64>,
    pub parser_id: String,
    pub parser_version: String,
    pub quality_level: String,
    pub warning_json: String,
    pub normalized_hash: String,
    pub structure_json: String,
}

/// 将草稿提交为正式版本时所需的已校验事实。正文、摘要和作者会随版本一并冻结。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommitKnowledgeDocumentDraft {
    pub draft_id: i64,
    pub expected_revision: i64,
    pub version_label: String,
    pub release_id: Option<i64>,
    pub cross_version_scope: String,
    pub commit_message: String,
    pub author_label: String,
    /// 仅 AI 分析确认路径写入。该 ID 由后端在同一受控流程中传递，普通手工提交不开放。
    pub analysis_draft_id: Option<i64>,
    pub content_hash: String,
    pub token_estimate: i64,
}

/// 提交事务创建的稳定标识。索引执行由持久化任务后续消费，提交本身不假装已建立索引。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeDocumentCommitRecord {
    pub document_id: i64,
    pub document_version_id: i64,
    pub parent_version_id: Option<i64>,
    pub content_hash: String,
    pub index_job_id: i64,
    pub index_job_status: String,
}

/// 文件已复制到受控资产目录后的导入登记信息。解析器尚未运行，因此文档明确标记为处理中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreateKnowledgeDocumentUpload {
    pub upload_key: String,
    pub project_id: i64,
    pub release_id: Option<i64>,
    pub cross_version_scope: String,
    pub asset_id: i64,
    pub asset_key: String,
    pub original_name: String,
    /// 已由 Service 规范化的来源文件夹名称；None 表示普通文件上传。
    pub source_folder_name: Option<String>,
    pub mime_type: String,
    pub document_type: String,
    pub title: String,
    pub allow_remote_ocr: bool,
    pub ocr_provider_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeDocumentUploadRecord {
    pub document_id: i64,
    pub asset_id: i64,
    pub import_job_id: i64,
    pub import_job_key: String,
    pub status: String,
}

/// 上传任务执行器只从持久化关联读取受控资产，不接受前端文件路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingKnowledgeDocumentUpload {
    pub document_id: i64,
    pub asset_id: i64,
    pub release_id: Option<i64>,
    pub cross_version_scope: String,
    pub import_job_id: i64,
    pub original_name: String,
    pub source_folder_name: Option<String>,
    pub mime_type: String,
    pub document_type: String,
    pub title: String,
    /// 创建上传文档时已经冻结的逻辑路径；完成导入的版本必须复用它作为来源证据。
    pub logical_path: String,
    pub storage_key: String,
    pub content_hash: String,
    pub size_bytes: i64,
    pub allow_remote_ocr: bool,
    pub ocr_provider_key: String,
}

/// 上传导入成功时必须一起冻结的事实。版本、分块、解析产物、全文索引、文档状态、
/// 上传状态和任务完成状态属于同一个可见结果，不能拆成多个可部分成功的写入。
pub(crate) struct CompleteKnowledgeDocumentUploadImport<'a> {
    pub import_job_id: i64,
    pub version: &'a CreateKnowledgeDocumentVersionInput,
    pub chunks: &'a [KnowledgeChunkWriteInput],
    pub parse_artifact: &'a NewKnowledgeDocumentParseArtifact,
    pub message: &'a str,
    pub checkpoint: &'a serde_json::Value,
}

pub(crate) struct CompleteKnowledgeDocumentUploadImportResult {
    pub document_version_id: i64,
    pub job: KnowledgeJob,
}

impl Database {
    /// 为来源同步创建的、尚未有结构化片段的文档版本幂等排队索引任务。
    ///
    /// 同步写入版本与索引任务是两个职责不同的阶段：版本先冻结，任务再异步解析。
    /// 任务键使用版本 ID，重复点击同步、应用重启恢复或并发轮询都不会重复创建任务。
    pub(crate) fn queue_knowledge_document_index_jobs_for_source(
        &self,
        source_id: i64,
    ) -> Result<Vec<(i64, i64)>, AppError> {
        if source_id <= 0 {
            return Err(AppError::InvalidInput("知识源 ID 必须大于 0".to_string()));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let candidates = {
            let mut statement = tx.prepare(
                "SELECT v.id
                 FROM knowledge_document_versions v
                 JOIN knowledge_documents d ON d.id = v.document_id
                 WHERE d.source_id = ?1
                   AND d.deleted_at IS NULL
                   AND v.valid = 1
                   AND v.content <> ''
                   AND v.index_job_id IS NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM knowledge_chunks c
                       WHERE c.document_version_id = v.id
                   )
                 ORDER BY v.id",
            )?;
            let rows = statement
                .query_map([source_id], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for document_version_id in candidates {
            let job_key = format!(
                "knowledge-document-index-source-{source_id}-version-{document_version_id}"
            );
            let checkpoint = serde_json::json!({
                "sourceId": source_id,
                "documentVersionId": document_version_id,
                "stage": "queued",
            });
            tx.execute(
                "INSERT OR IGNORE INTO knowledge_jobs
                    (job_key, job_type, source_id, status, message, checkpoint_json)
                 VALUES (?1, 'document_index', ?2, 'queued', '等待建立文档索引', ?3)",
                params![job_key, source_id, serde_json::to_string(&checkpoint)?],
            )?;
            let job_id = tx.query_row(
                "SELECT id FROM knowledge_jobs WHERE job_key = ?1",
                [&job_key],
                |row| row.get::<_, i64>(0),
            )?;
            tx.execute(
                "UPDATE knowledge_document_versions
                 SET index_job_id = ?1
                 WHERE id = ?2 AND index_job_id IS NULL",
                params![job_id, document_version_id],
            )?;
        }

        let queued = {
            let mut statement = tx.prepare(
                "SELECT v.id, v.index_job_id
                 FROM knowledge_document_versions v
                 JOIN knowledge_documents d ON d.id = v.document_id
                 JOIN knowledge_jobs j ON j.id = v.index_job_id
                 WHERE d.source_id = ?1
                   AND d.deleted_at IS NULL
                   AND v.valid = 1
                   AND v.content <> ''
                   AND j.job_type = 'document_index'
                   AND j.status = 'queued'
                 ORDER BY v.id",
            )?;
            let rows = statement
                .query_map([source_id], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        tx.commit()?;
        Ok(queued)
    }

    /// 索引任务重试时只能定位到创建该任务的正式文档版本，不能根据“当前版本”猜测，
    /// 否则新版本提交后可能重试并覆盖错误的历史版本索引。
    pub(crate) fn find_knowledge_document_version_id_by_index_job_id(
        &self,
        index_job_id: i64,
    ) -> Result<Option<i64>, AppError> {
        if index_job_id <= 0 {
            return Err(AppError::InvalidInput("索引任务 ID 必须大于 0".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id FROM knowledge_document_versions WHERE index_job_id = ?1",
            [index_job_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    /// 资产的三种稳定键均唯一。重复导入同一内容时复用行并保持引用计数由调用事务管理。
    pub(crate) fn upsert_knowledge_asset(
        &self,
        asset: &NewKnowledgeAsset,
    ) -> Result<KnowledgeAssetRecord, AppError> {
        validate_asset(asset)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_assets
                (asset_key, content_hash, storage_key, original_name, normalized_name, mime_type, size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(content_hash) DO UPDATE SET
                deleted_at = NULL",
            params![
                asset.asset_key.trim(),
                asset.content_hash.trim(),
                asset.storage_key.trim(),
                asset.original_name.trim(),
                asset.normalized_name.trim(),
                asset.mime_type.trim(),
                asset.size_bytes,
            ],
        )?;
        get_asset_by_content_hash(&conn, asset.content_hash.trim())?
            .ok_or_else(|| AppError::Custom("保存资产后未找到对应内容哈希".to_string()))
    }

    /// 引用计数永不允许为负；调用方可在同一业务事务完成版本/资产关系的变更后再更新。
    pub(crate) fn adjust_knowledge_asset_reference_count(
        &self,
        asset_id: i64,
        delta: i64,
    ) -> Result<KnowledgeAssetRecord, AppError> {
        if asset_id <= 0 {
            return Err(AppError::InvalidInput("资产 ID 必须为正数".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_assets
             SET reference_count = reference_count + ?2, deleted_at = NULL
             WHERE id = ?1 AND reference_count + ?2 >= 0",
            params![asset_id, delta],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "资产不存在或引用计数不能为负数".to_string(),
            ));
        }
        get_asset_by_id(&conn, asset_id)?
            .ok_or_else(|| AppError::NotFound(format!("资产不存在: {asset_id}")))
    }

    pub(crate) fn create_knowledge_document_draft(
        &self,
        draft: &NewKnowledgeDocumentDraft,
    ) -> Result<KnowledgeDocumentDraftRecord, AppError> {
        validate_draft(draft)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_document_drafts
                (document_id, project_id, title, content, doc_type, base_version_id, editor_label)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                draft.document_id,
                draft.project_id,
                draft.title.trim(),
                draft.content,
                draft.doc_type.trim(),
                draft.base_version_id,
                draft.editor_label.trim(),
            ],
        )?;
        let id = conn.last_insert_rowid();
        get_draft_by_id(&conn, id)?
            .ok_or_else(|| AppError::Custom("创建草稿后未找到记录".to_string()))
    }

    /// 乐观并发更新：revision 不匹配时不覆盖新内容，并由 Service 读取当前草稿生成冲突响应。
    pub(crate) fn update_knowledge_document_draft(
        &self,
        draft_id: i64,
        expected_revision: i64,
        title: &str,
        content: &str,
        editor_label: &str,
    ) -> Result<Option<KnowledgeDocumentDraftRecord>, AppError> {
        if draft_id <= 0 || expected_revision <= 0 || title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "草稿 ID、修订号和标题必须有效".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_document_drafts
             SET title = ?3, content = ?4, editor_label = ?5, revision = revision + 1,
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND revision = ?2 AND deleted_at IS NULL",
            params![
                draft_id,
                expected_revision,
                title.trim(),
                content,
                editor_label.trim()
            ],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        get_draft_by_id(&conn, draft_id)
    }

    pub(crate) fn get_knowledge_document_draft(
        &self,
        draft_id: i64,
    ) -> Result<Option<KnowledgeDocumentDraftRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_draft_by_id(&conn, draft_id)
    }

    /// 在一个 SQLite 事务中创建（或复用）逻辑文档、不可变正式版本和待执行索引任务，
    /// 最后才归档草稿。任一步失败都会整体回滚，避免出现“当前版本已切换但没有索引任务”
    /// 或“草稿已丢失但版本未创建”的半完成状态。
    pub(crate) fn commit_knowledge_document_draft(
        &self,
        input: &CommitKnowledgeDocumentDraft,
    ) -> Result<KnowledgeDocumentCommitRecord, AppError> {
        validate_commit(input)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let draft = get_draft_by_id(&tx, input.draft_id)?
            .ok_or_else(|| AppError::NotFound(format!("草稿不存在: {}", input.draft_id)))?;
        if draft.deleted_at.is_some() {
            return Err(AppError::InvalidInput(
                "草稿已经提交或删除，不能重复提交".to_string(),
            ));
        }
        if draft.revision != input.expected_revision {
            return Err(AppError::InvalidInput(
                "草稿已被其他操作更新，请先比较后重试".to_string(),
            ));
        }

        let document_id = match draft.document_id {
            Some(document_id) => {
                let project_id: Option<i64> = tx
                    .query_row(
                        "SELECT project_id FROM knowledge_documents
                         WHERE id = ?1 AND deleted_at IS NULL",
                        [document_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if project_id != Some(draft.project_id) {
                    return Err(AppError::InvalidInput("草稿不属于当前项目文档".to_string()));
                }
                document_id
            }
            None => {
                tx.execute(
                    "INSERT INTO knowledge_documents
                        (document_key, project_id, doc_type, title, logical_path, status,
                         sensitivity, tags_json, allow_ai, allow_mcp)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'active', 'internal', '[]', 1, 0)",
                    params![
                        format!("manual-document-{}-{}", draft.project_id, draft.id),
                        draft.project_id,
                        draft.doc_type,
                        draft.title,
                        format!("manual/{}.md", draft.id),
                    ],
                )?;
                tx.last_insert_rowid()
            }
        };

        let parent_version_id = match draft.base_version_id {
            Some(version_id) => Some(version_id),
            None => current_document_version_id(&tx, document_id)?,
        };
        if let Some(parent_version_id) = parent_version_id {
            let parent_document_id: Option<i64> = tx
                .query_row(
                    "SELECT document_id FROM knowledge_document_versions WHERE id = ?1",
                    [parent_version_id],
                    |row| row.get(0),
                )
                .optional()?;
            if parent_document_id != Some(document_id) {
                return Err(AppError::InvalidInput("父版本不属于当前文档".to_string()));
            }
        }

        let parsed_meta_json = serde_json::to_string(&serde_json::json!({
            "origin": "manual",
            "draftId": draft.id,
            "parentVersionId": parent_version_id,
        }))?;
        // 任务先于版本写入，以便版本行从创建起就携带任务标识；不可变触发器因此不需要
        // 为了补写索引任务而放开任何 UPDATE 通道。
        let job_key = format!(
            "document-index:document-{document_id}:draft-{}:revision-{}",
            draft.id, draft.revision
        );
        tx.execute(
            "INSERT INTO knowledge_jobs
                (job_key, job_type, status, message, checkpoint_json)
             VALUES (?1, 'document_index', 'queued', '等待建立文档索引', ?2)",
            params![
                job_key,
                serde_json::to_string(&serde_json::json!({
                    "documentId": document_id,
                    "draftId": draft.id,
                    "contentHash": input.content_hash.trim(),
                }))?,
            ],
        )?;
        let index_job_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO knowledge_document_versions
                (document_id, release_id, version_label, source_path, mime_type, content,
                 content_hash, parsed_meta_json, token_estimate, parent_version_id,
                 author_label, commit_message, analysis_draft_id, index_job_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                document_id,
                input.release_id,
                input.version_label.trim(),
                format!("manual/{}.md", draft.id),
                mime_type_for_document_type(&draft.doc_type),
                draft.content,
                input.content_hash.trim(),
                parsed_meta_json,
                input.token_estimate,
                parent_version_id,
                input.author_label.trim(),
                input.commit_message.trim(),
                input.analysis_draft_id,
                index_job_id,
            ],
        )?;
        let document_version_id = tx.last_insert_rowid();
        insert_document_version_binding_in_transaction(
            &tx,
            document_version_id,
            input.release_id,
            None,
            &input.cross_version_scope,
        )?;
        tx.execute(
            "UPDATE knowledge_documents
             SET latest_version_id = ?1, title = ?2, doc_type = ?3,
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?4",
            params![
                document_version_id,
                draft.title,
                draft.doc_type,
                document_id
            ],
        )?;
        sync_knowledge_document_title_index(&tx, document_id)?;
        let archived = tx.execute(
            "UPDATE knowledge_document_drafts
             SET document_id = ?1, deleted_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?2 AND revision = ?3 AND deleted_at IS NULL",
            params![document_id, draft.id, input.expected_revision],
        )?;
        if archived != 1 {
            return Err(AppError::InvalidInput(
                "草稿已被其他操作更新，请先比较后重试".to_string(),
            ));
        }
        tx.commit()?;
        Ok(KnowledgeDocumentCommitRecord {
            document_id,
            document_version_id,
            parent_version_id,
            content_hash: input.content_hash.trim().to_string(),
            index_job_id,
            index_job_status: "queued".to_string(),
        })
    }

    /// 资产复制成功后，以单个事务登记逻辑文档、导入任务、上传关联和资产引用。解析器只
    /// 消费该任务，不会从前端重取路径；失败时不会留下引用计数或处理中文档的残留行。
    pub(crate) fn create_knowledge_document_upload(
        &self,
        input: &CreateKnowledgeDocumentUpload,
    ) -> Result<KnowledgeDocumentUploadRecord, AppError> {
        validate_upload(input)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO knowledge_documents
                (document_key, project_id, doc_type, title, logical_path, status, sensitivity,
                 tags_json, allow_ai, allow_mcp)
             VALUES (?1, ?2, ?3, ?4, ?5, 'processing', 'internal', '[]', 1, 0)",
            params![
                format!("upload-document-{}", input.upload_key.trim()),
                input.project_id,
                input.document_type.trim(),
                input.title.trim(),
                upload_logical_path(input),
            ],
        )?;
        let document_id = tx.last_insert_rowid();
        let import_job_key = format!("knowledge-upload-import-{}", input.upload_key.trim());
        tx.execute(
            "INSERT INTO knowledge_jobs
                (job_key, job_type, status, message, checkpoint_json)
             VALUES (?1, 'upload_import', 'queued', '等待导入和解析文件', ?2)",
            params![
                import_job_key,
                serde_json::to_string(&serde_json::json!({
                    "documentId": document_id,
                    "assetId": input.asset_id,
                    "releaseId": input.release_id,
                    "crossVersionScope": input.cross_version_scope.trim(),
                    "assetKey": input.asset_key.trim(),
                }))?,
            ],
        )?;
        let import_job_id = tx.last_insert_rowid();
        let adjusted = tx.execute(
            "UPDATE knowledge_assets
             SET reference_count = reference_count + 1, deleted_at = NULL
             WHERE id = ?1",
            [input.asset_id],
        )?;
        if adjusted != 1 {
            return Err(AppError::NotFound(format!(
                "上传资产不存在: {}",
                input.asset_id
            )));
        }
        tx.execute(
            "INSERT INTO knowledge_document_uploads
                (document_id, asset_id, release_id, cross_version_scope, import_job_id, original_name,
                 source_folder_name, mime_type,
                 allow_remote_ocr, ocr_provider_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                document_id,
                input.asset_id,
                input.release_id,
                input.cross_version_scope.trim(),
                import_job_id,
                input.original_name.trim(),
                input.source_folder_name.as_deref(),
                input.mime_type.trim(),
                i64::from(input.allow_remote_ocr),
                input.ocr_provider_key.trim(),
            ],
        )?;
        tx.commit()?;
        Ok(KnowledgeDocumentUploadRecord {
            document_id,
            asset_id: input.asset_id,
            import_job_id,
            import_job_key,
            status: "queued".to_string(),
        })
    }

    pub(crate) fn get_pending_knowledge_document_upload(
        &self,
        import_job_id: i64,
    ) -> Result<Option<PendingKnowledgeDocumentUpload>, AppError> {
        if import_job_id <= 0 {
            return Err(AppError::InvalidInput("导入任务 ID 必须大于 0".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT upload.document_id, upload.asset_id, upload.release_id, upload.cross_version_scope, upload.import_job_id,
                    upload.original_name, upload.source_folder_name, upload.mime_type,
                    document.doc_type, document.title, document.logical_path,
                    asset.storage_key, asset.content_hash, asset.size_bytes,
                    upload.allow_remote_ocr, upload.ocr_provider_key
             FROM knowledge_document_uploads upload
             JOIN knowledge_documents document ON document.id = upload.document_id
             JOIN knowledge_assets asset ON asset.id = upload.asset_id
             WHERE upload.import_job_id = ?1 AND upload.status IN ('queued', 'running')
               AND document.status = 'processing' AND document.deleted_at IS NULL
               AND asset.deleted_at IS NULL",
            [import_job_id],
            |row| {
                Ok(PendingKnowledgeDocumentUpload {
                    document_id: row.get(0)?,
                    asset_id: row.get(1)?,
                    release_id: row.get(2)?,
                    cross_version_scope: row.get(3)?,
                    import_job_id: row.get(4)?,
                    original_name: row.get(5)?,
                    source_folder_name: row.get(6)?,
                    mime_type: row.get(7)?,
                    document_type: row.get(8)?,
                    title: row.get(9)?,
                    logical_path: row.get(10)?,
                    storage_key: row.get(11)?,
                    content_hash: row.get(12)?,
                    size_bytes: row.get(13)?,
                    allow_remote_ocr: row.get::<_, i64>(14)? != 0,
                    ocr_provider_key: row.get(15)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub(crate) fn set_knowledge_document_upload_status(
        &self,
        import_job_id: i64,
        status: &str,
        error_message: &str,
    ) -> Result<(), AppError> {
        if !matches!(status, "completed" | "failed" | "cancelled") {
            return Err(AppError::InvalidInput("未知上传导入状态".to_string()));
        }
        let document_status = if status == "completed" {
            "active"
        } else {
            "failed"
        };
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let document_id = tx
            .query_row(
                "SELECT document_id FROM knowledge_document_uploads WHERE import_job_id = ?1",
                [import_job_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("上传导入任务不存在: {import_job_id}")))?;
        tx.execute(
            "UPDATE knowledge_document_uploads
             SET status = ?1, error_message = ?2, updated_at = datetime('now', 'localtime')
             WHERE import_job_id = ?3",
            params![status, error_message, import_job_id],
        )?;
        tx.execute(
            "UPDATE knowledge_documents SET status = ?1, updated_at = datetime('now', 'localtime')
             WHERE id = ?2 AND status = 'processing'",
            params![document_status, document_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 失败或取消的上传任务再次执行前，恢复上传和逻辑文档的处理中状态；不接收任何
    /// 前端路径，重试仍只能消费已登记的内容寻址资产。
    pub(crate) fn restart_knowledge_document_upload(
        &self,
        import_job_id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        if import_job_id <= 0 {
            return Err(AppError::InvalidInput("导入任务 ID 必须大于 0".to_string()));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let document_id = tx
            .query_row(
                "SELECT document_id FROM knowledge_document_uploads WHERE import_job_id = ?1",
                [import_job_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("上传导入任务不存在: {import_job_id}")))?;
        let changed = tx.execute(
            "UPDATE knowledge_document_uploads
             SET status = 'queued', error_message = '', updated_at = datetime('now', 'localtime')
             WHERE import_job_id = ?1 AND status IN ('failed', 'cancelled', 'queued', 'running')",
            [import_job_id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "仅失败或已取消的上传允许重试".to_string(),
            ));
        }
        tx.execute(
            "UPDATE knowledge_documents
             SET status = 'processing', updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NULL",
            [document_id],
        )?;
        let restarted = tx.execute(
            "UPDATE knowledge_jobs SET
                status = 'queued', message = '任务已进入重试队列', error = NULL,
                cancel_requested = 0, heartbeat_at = datetime('now', 'localtime'),
                finished_at = NULL
             WHERE id = ?1 AND status IN ('failed', 'cancelled', 'interrupted')",
            [import_job_id],
        )?;
        if restarted != 1 {
            return Err(AppError::InvalidInput(
                "仅失败、取消或中断任务允许重试".to_string(),
            ));
        }
        tx.commit()?;
        drop(conn);
        self.get_knowledge_job_by_id(import_job_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识任务不存在: {import_job_id}")))
    }

    /// 上传任务的取消需要同时更新任务、上传登记和逻辑文档。对于已经运行的解析器，只
    /// 设置协作式取消标志；对于尚未开始的任务，直接收尾，避免应用退出后留下处理中。
    pub(crate) fn request_knowledge_document_upload_cancel(
        &self,
        import_job_id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let (document_id, status) = tx
            .query_row(
                "SELECT upload.document_id, job.status
                 FROM knowledge_document_uploads upload
                 JOIN knowledge_jobs job ON job.id = upload.import_job_id
                 WHERE upload.import_job_id = ?1",
                [import_job_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("上传导入任务不存在: {import_job_id}")))?;
        match status.as_str() {
            "queued" | "interrupted" => {
                tx.execute(
                    "UPDATE knowledge_jobs SET
                        cancel_requested = 1, status = 'cancelled', message = '任务已取消',
                        heartbeat_at = datetime('now', 'localtime'),
                        finished_at = datetime('now', 'localtime')
                     WHERE id = ?1 AND status IN ('queued', 'interrupted')",
                    [import_job_id],
                )?;
                tx.execute(
                    "UPDATE knowledge_document_uploads
                     SET status = 'cancelled', error_message = '', updated_at = datetime('now', 'localtime')
                     WHERE import_job_id = ?1 AND status IN ('queued', 'running')",
                    [import_job_id],
                )?;
                tx.execute(
                    "UPDATE knowledge_documents
                     SET status = 'failed', updated_at = datetime('now', 'localtime')
                     WHERE id = ?1 AND status = 'processing'",
                    [document_id],
                )?;
            }
            "running" => {
                tx.execute(
                    "UPDATE knowledge_jobs SET
                        cancel_requested = 1, message = '已请求取消，正在安全停止'
                     WHERE id = ?1 AND status = 'running'",
                    [import_job_id],
                )?;
            }
            _ => {
                return Err(AppError::InvalidInput(
                    "知识任务已结束或当前状态不允许取消".to_string(),
                ));
            }
        }
        tx.commit()?;
        drop(conn);
        self.get_knowledge_job_by_id(import_job_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识任务不存在: {import_job_id}")))
    }

    /// 上传文件解析成功的原子提交。若用户已经请求取消，任务完成 CAS 会失败，整个
    /// 事务回滚，避免“内容可检索但界面显示已取消”的不一致状态。
    pub(crate) fn complete_knowledge_document_upload_import(
        &self,
        input: CompleteKnowledgeDocumentUploadImport<'_>,
    ) -> Result<CompleteKnowledgeDocumentUploadImportResult, AppError> {
        validate_parse_artifact_fields(input.parse_artifact)?;
        if input.import_job_id <= 0 || input.version.document_id <= 0 {
            return Err(AppError::InvalidInput(
                "上传导入缺少有效的任务或文档标识".to_string(),
            ));
        }
        let parsed_meta_json = serde_json::to_string(&input.version.parsed_meta)?;
        let checkpoint_json = serde_json::to_string(input.checkpoint)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let (upload_document_id, upload_release_id, upload_cross_version_scope) = tx
            .query_row(
                "SELECT upload.document_id, upload.release_id, upload.cross_version_scope
                 FROM knowledge_document_uploads upload
                 JOIN knowledge_documents document ON document.id = upload.document_id
                 WHERE upload.import_job_id = ?1 AND upload.status IN ('queued', 'running')
                   AND document.status = 'processing' AND document.deleted_at IS NULL",
                [input.import_job_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("上传任务不是可完成状态".to_string()))?;
        if upload_document_id != input.version.document_id {
            return Err(AppError::InvalidInput(
                "上传任务与文档版本不匹配".to_string(),
            ));
        }
        if upload_release_id != input.version.release_id {
            return Err(AppError::InvalidInput(
                "上传任务与文档版本范围不匹配".to_string(),
            ));
        }
        let asset_id = tx.query_row(
            "SELECT asset_id FROM knowledge_document_uploads WHERE import_job_id = ?1",
            [input.import_job_id],
            |row| row.get::<_, i64>(0),
        )?;
        if input.parse_artifact.asset_id != Some(asset_id) {
            return Err(AppError::InvalidInput(
                "解析产物与上传资产不匹配".to_string(),
            ));
        }
        let active = tx
            .query_row(
                "SELECT 1 FROM knowledge_jobs
             WHERE id = ?1 AND status = 'running' AND cancel_requested = 0",
                [input.import_job_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !active {
            return Err(AppError::InvalidInput(
                "上传任务已取消或不是运行状态".to_string(),
            ));
        }
        tx.execute(
            "INSERT OR IGNORE INTO knowledge_document_versions
             (document_id, release_id, version_label, git_branch, commit_sha, source_path,
              mime_type, content, content_hash, parsed_meta_json, token_estimate)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                input.version.document_id,
                input.version.release_id,
                input.version.version_label,
                input.version.git_branch,
                input.version.commit_sha,
                input.version.source_path,
                input.version.mime_type,
                input.version.content,
                input.version.content_hash,
                parsed_meta_json,
                input.version.token_estimate,
            ],
        )?;
        let document_version_id = tx.query_row(
            "SELECT id FROM knowledge_document_versions
             WHERE document_id = ?1 AND version_label = ?2 AND content_hash = ?3",
            params![
                input.version.document_id,
                input.version.version_label,
                input.version.content_hash,
            ],
            |row| row.get::<_, i64>(0),
        )?;
        insert_document_version_binding_in_transaction(
            &tx,
            document_version_id,
            upload_release_id,
            None,
            &upload_cross_version_scope,
        )?;
        let existing_chunks = tx.query_row(
            "SELECT COUNT(*) FROM knowledge_chunks WHERE document_version_id = ?1",
            [document_version_id],
            |row| row.get::<_, i64>(0),
        )?;
        if existing_chunks == 0 {
            insert_chunks(&tx, document_version_id, input.chunks)?;
        }
        insert_knowledge_document_parse_artifact_in_transaction(
            &tx,
            document_version_id,
            input.parse_artifact,
        )?;
        tx.execute(
            "UPDATE knowledge_documents
             SET latest_version_id = ?1, status = 'active', updated_at = datetime('now', 'localtime')
             WHERE id = ?2 AND status = 'processing' AND deleted_at IS NULL",
            params![document_version_id, input.version.document_id],
        )?;
        sync_document_fts_if_available(&tx, input.version.document_id, document_version_id)?;
        sync_knowledge_document_title_index(&tx, input.version.document_id)?;
        tx.execute(
            "UPDATE knowledge_document_uploads
             SET status = 'completed', error_message = '', updated_at = datetime('now', 'localtime')
             WHERE import_job_id = ?1 AND status IN ('queued', 'running')",
            [input.import_job_id],
        )?;
        let completed = tx.execute(
            "UPDATE knowledge_jobs SET
                status = 'completed', message = ?1, error = NULL, checkpoint_json = ?2,
                heartbeat_at = datetime('now', 'localtime'), finished_at = datetime('now', 'localtime')
             WHERE id = ?3 AND status = 'running' AND cancel_requested = 0",
            params![input.message, checkpoint_json, input.import_job_id],
        )?;
        if completed != 1 {
            return Err(AppError::InvalidInput(
                "上传任务已取消或不能完成".to_string(),
            ));
        }
        tx.commit()?;
        drop(conn);
        let job = self
            .get_knowledge_job_by_id(input.import_job_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识任务不存在: {}", input.import_job_id))
            })?;
        Ok(CompleteKnowledgeDocumentUploadImportResult {
            document_version_id,
            job,
        })
    }

    /// 解析失败时以取消请求为优先级写入终态。这样解析器错误与用户取消并发时，失败不会
    /// 覆盖用户已经明确请求的取消结果。
    pub(crate) fn fail_knowledge_document_upload_import_or_cancel(
        &self,
        import_job_id: i64,
        error_message: &str,
    ) -> Result<KnowledgeJob, AppError> {
        self.finish_knowledge_document_upload_import_terminal(
            import_job_id,
            "failed",
            "上传文档导入失败",
            Some(error_message),
        )
    }

    pub(crate) fn cancel_knowledge_document_upload_import(
        &self,
        import_job_id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        self.finish_knowledge_document_upload_import_terminal(
            import_job_id,
            "cancelled",
            "上传文档导入已安全取消",
            None,
        )
    }

    fn finish_knowledge_document_upload_import_terminal(
        &self,
        import_job_id: i64,
        requested_status: &str,
        requested_message: &str,
        error_message: Option<&str>,
    ) -> Result<KnowledgeJob, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let (document_id, cancelled) = tx
            .query_row(
                "SELECT upload.document_id, job.cancel_requested
                 FROM knowledge_document_uploads upload
                 JOIN knowledge_jobs job ON job.id = upload.import_job_id
                 WHERE upload.import_job_id = ?1
                   AND job.status IN ('queued', 'running', 'interrupted', 'cancelled')",
                [import_job_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)? != 0)),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("上传任务当前状态不允许结束".to_string()))?;
        let status = if cancelled {
            "cancelled"
        } else {
            requested_status
        };
        let message = if cancelled {
            "上传文档导入已安全取消"
        } else {
            requested_message
        };
        let safe_error = if status == "failed" {
            error_message.unwrap_or_default()
        } else {
            ""
        };
        tx.execute(
            "UPDATE knowledge_document_uploads
             SET status = ?1, error_message = ?2, updated_at = datetime('now', 'localtime')
             WHERE import_job_id = ?3",
            params![status, safe_error, import_job_id],
        )?;
        tx.execute(
            "UPDATE knowledge_documents
             SET status = 'failed', updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status = 'processing'",
            [document_id],
        )?;
        tx.execute(
            "UPDATE knowledge_jobs SET
                status = ?1, message = ?2, error = ?3,
                checkpoint_json = ?4, heartbeat_at = datetime('now', 'localtime'),
                finished_at = datetime('now', 'localtime')
             WHERE id = ?5",
            params![
                status,
                message,
                if status == "failed" {
                    error_message
                } else {
                    None
                },
                serde_json::to_string(&serde_json::json!({
                    "documentId": document_id,
                    "stage": status,
                }))?,
                import_job_id,
            ],
        )?;
        tx.commit()?;
        drop(conn);
        self.get_knowledge_job_by_id(import_job_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识任务不存在: {import_job_id}")))
    }

    /// 一个文档版本必须显式落在项目版本、仓库范围或跨版本范围之一，防止静默归入最新版本。
    pub(crate) fn replace_knowledge_document_version_bindings(
        &self,
        document_version_id: i64,
        bindings: &[(Option<i64>, Option<i64>, String)],
    ) -> Result<Vec<KnowledgeDocumentVersionBindingRecord>, AppError> {
        if document_version_id <= 0 || bindings.is_empty() {
            return Err(AppError::InvalidInput(
                "文档版本与其范围绑定不能为空".to_string(),
            ));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM knowledge_document_version_bindings WHERE document_version_id = ?1",
            [document_version_id],
        )?;
        for (release_id, repository_binding_id, cross_version_scope) in bindings {
            insert_document_version_binding_in_transaction(
                &tx,
                document_version_id,
                *release_id,
                *repository_binding_id,
                cross_version_scope,
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.list_knowledge_document_version_bindings(document_version_id)
    }

    pub(crate) fn list_knowledge_document_version_bindings(
        &self,
        document_version_id: i64,
    ) -> Result<Vec<KnowledgeDocumentVersionBindingRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, document_version_id, release_id, repository_binding_id, cross_version_scope
             FROM knowledge_document_version_bindings
             WHERE document_version_id = ?1
             ORDER BY id",
        )?;
        let bindings = statement
            .query_map([document_version_id], map_document_version_binding)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(bindings)
    }

    pub(crate) fn insert_knowledge_document_parse_artifact(
        &self,
        artifact: &NewKnowledgeDocumentParseArtifact,
    ) -> Result<i64, AppError> {
        validate_parse_artifact(artifact)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        insert_knowledge_document_parse_artifact_in_transaction(
            &tx,
            artifact.document_version_id,
            artifact,
        )?;
        let id = tx.query_row(
            "SELECT id FROM knowledge_document_parse_artifacts
             WHERE document_version_id = ?1 AND parser_id = ?2 AND parser_version = ?3
               AND normalized_hash = ?4",
            params![
                artifact.document_version_id,
                artifact.parser_id.trim(),
                artifact.parser_version.trim(),
                artifact.normalized_hash.trim(),
            ],
            |row| row.get(0),
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// 比较接口只需知道可追溯的解析签名与原始资产哈希；存储路径、告警正文和结构 JSON
    /// 都属于内部处理细节，不能随文档内容输出接口泄露。
    pub(crate) fn list_knowledge_document_comparison_artifacts(
        &self,
        document_version_id: i64,
    ) -> Result<Vec<KnowledgeDocumentComparisonArtifact>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT artifact.parser_id, artifact.parser_version, artifact.quality_level,
                    artifact.normalized_hash, asset.content_hash
             FROM knowledge_document_parse_artifacts artifact
             LEFT JOIN knowledge_assets asset ON asset.id = artifact.asset_id
             WHERE artifact.document_version_id = ?1
             ORDER BY artifact.parser_id, artifact.parser_version,
                      artifact.normalized_hash, artifact.id",
        )?;
        let artifacts = statement
            .query_map([document_version_id], |row| {
                Ok(KnowledgeDocumentComparisonArtifact {
                    parser_id: row.get(0)?,
                    parser_version: row.get(1)?,
                    quality_level: row.get(2)?,
                    normalized_hash: row.get(3)?,
                    asset_hash: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(artifacts)
    }

    /// 预览只能读取逻辑文档当前版本关联的图片资产；历史资产、软删除资产和存储路径
    /// 都不通过此查询暴露给调用方。
    pub(crate) fn get_knowledge_document_current_image_asset(
        &self,
        document_id: i64,
    ) -> Result<Option<KnowledgeAssetRecord>, AppError> {
        if document_id <= 0 {
            return Err(AppError::InvalidInput("知识文档 ID 必须为正数".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT asset.id, asset.asset_key, asset.content_hash, asset.storage_key,
                    asset.original_name, asset.normalized_name, asset.mime_type,
                    asset.size_bytes, asset.reference_count, asset.deleted_at
             FROM knowledge_documents document
             JOIN knowledge_document_parse_artifacts artifact
               ON artifact.document_version_id = document.latest_version_id
             JOIN knowledge_assets asset ON asset.id = artifact.asset_id
             WHERE document.id = ?1
               AND document.deleted_at IS NULL
               AND asset.deleted_at IS NULL
               AND asset.mime_type LIKE 'image/%'
             ORDER BY artifact.id DESC
             LIMIT 1",
            [document_id],
            map_asset,
        )
        .optional()
        .map_err(Into::into)
    }
}

fn validate_parse_artifact(artifact: &NewKnowledgeDocumentParseArtifact) -> Result<(), AppError> {
    if artifact.document_version_id <= 0 {
        return Err(AppError::InvalidInput(
            "解析产物缺少必要的版本、解析器或内容哈希".to_string(),
        ));
    }
    validate_parse_artifact_fields(artifact)
}

fn validate_parse_artifact_fields(
    artifact: &NewKnowledgeDocumentParseArtifact,
) -> Result<(), AppError> {
    if artifact.parser_id.trim().is_empty()
        || artifact.parser_version.trim().is_empty()
        || artifact.quality_level.trim().is_empty()
        || artifact.normalized_hash.trim().is_empty()
    {
        return Err(AppError::InvalidInput(
            "解析产物缺少必要的版本、解析器或内容哈希".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn insert_knowledge_document_parse_artifact_in_transaction(
    tx: &Transaction<'_>,
    document_version_id: i64,
    artifact: &NewKnowledgeDocumentParseArtifact,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO knowledge_document_parse_artifacts
            (document_version_id, asset_id, parser_id, parser_version, quality_level,
             warning_json, normalized_hash, structure_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(document_version_id, parser_id, parser_version, normalized_hash)
         DO UPDATE SET asset_id = excluded.asset_id, quality_level = excluded.quality_level,
            warning_json = excluded.warning_json, structure_json = excluded.structure_json",
        params![
            document_version_id,
            artifact.asset_id,
            artifact.parser_id.trim(),
            artifact.parser_version.trim(),
            artifact.quality_level.trim(),
            artifact.warning_json,
            artifact.normalized_hash.trim(),
            artifact.structure_json,
        ],
    )?;
    Ok(())
}

/// 将解析器输出转换为可审计的持久化事实。Git/本地目录来源没有受控资产，故 asset_id
/// 保持为空；上传来源才会提供对应的资产引用。
pub(crate) fn parse_artifact_from_result(
    document_version_id: i64,
    asset_id: Option<i64>,
    result: &KnowledgeParseAndChunkResult,
) -> Result<NewKnowledgeDocumentParseArtifact, AppError> {
    let parser_id = result.parsed.parser_id.clone();
    Ok(NewKnowledgeDocumentParseArtifact {
        document_version_id,
        asset_id,
        parser_version: parser_version(&parser_id),
        parser_id,
        quality_level: if result.parsed.warnings.is_empty() {
            "complete".to_string()
        } else {
            "partial".to_string()
        },
        warning_json: serde_json::to_string(&result.parsed.warnings)?,
        normalized_hash: format!(
            "{:x}",
            Sha256::digest(result.parsed.normalized_content.as_bytes())
        ),
        structure_json: serde_json::to_string(&serde_json::json!({
            "normalizationVersion": result.parsed.normalization_version,
            "frontMatter": result.parsed.front_matter,
            "blocks": result.parsed.blocks,
            "chunkStrategyId": result.chunk_strategy_id,
        }))?,
    })
}

fn parser_version(parser_id: &str) -> String {
    parser_id
        .rsplit_once("-v")
        .map(|(_, version)| format!("v{version}"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn validate_asset(asset: &NewKnowledgeAsset) -> Result<(), AppError> {
    if asset.asset_key.trim().is_empty()
        || asset.content_hash.trim().is_empty()
        || asset.storage_key.trim().is_empty()
        || asset.original_name.trim().is_empty()
        || asset.normalized_name.trim().is_empty()
        || asset.mime_type.trim().is_empty()
        || asset.size_bytes < 0
    {
        return Err(AppError::InvalidInput(
            "资产元数据不完整或文件大小无效".to_string(),
        ));
    }
    Ok(())
}

fn validate_draft(draft: &NewKnowledgeDocumentDraft) -> Result<(), AppError> {
    if draft.project_id <= 0 || draft.title.trim().is_empty() || draft.doc_type.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "草稿必须包含项目、标题和文档类型".to_string(),
        ));
    }
    Ok(())
}

fn validate_commit(input: &CommitKnowledgeDocumentDraft) -> Result<(), AppError> {
    if input.draft_id <= 0
        || input.expected_revision <= 0
        || input.version_label.trim().is_empty()
        || input.version_label.chars().count() > 80
        || input.author_label.trim().is_empty()
        || input.author_label.chars().count() > 80
        || input.commit_message.chars().count() > 500
        || input
            .analysis_draft_id
            .is_some_and(|draft_id| draft_id <= 0)
        || input.content_hash.trim().len() != 64
        || !input
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || input.token_estimate < 0
    {
        return Err(AppError::InvalidInput(
            "文档提交参数不完整或格式不正确".to_string(),
        ));
    }
    if let Some(release_id) = input.release_id {
        if release_id <= 0 {
            return Err(AppError::InvalidInput("项目版本 ID 必须为正数".to_string()));
        }
    }
    if input.release_id.is_none() && input.cross_version_scope.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "提交文档时必须选择项目版本或跨版本范围".to_string(),
        ));
    }
    Ok(())
}

fn validate_upload(input: &CreateKnowledgeDocumentUpload) -> Result<(), AppError> {
    let invalid_folder_name = input.source_folder_name.as_deref().is_some_and(|name| {
        let name = name.trim();
        name.is_empty()
            || name.chars().count() > 180
            || name
                .chars()
                .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':'))
    });
    if input.upload_key.trim().is_empty()
        || input.project_id <= 0
        || input.release_id.is_some_and(|id| id <= 0)
        || (input.release_id.is_none() && input.cross_version_scope.trim().is_empty())
        || input.asset_id <= 0
        || input.asset_key.trim().is_empty()
        || input.original_name.trim().is_empty()
        || input.mime_type.trim().is_empty()
        || input.document_type.trim().is_empty()
        || input.title.trim().is_empty()
        || invalid_folder_name
        || (input.allow_remote_ocr && input.ocr_provider_key.trim().is_empty())
    {
        return Err(AppError::InvalidInput("上传导入元数据不完整".to_string()));
    }
    Ok(())
}

fn upload_logical_path(input: &CreateKnowledgeDocumentUpload) -> String {
    match input
        .source_folder_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(folder_name) => format!(
            "upload-folder/{}/{}",
            folder_name,
            input.original_name.trim()
        ),
        None => format!("upload/{}", input.asset_key.trim()),
    }
}

fn current_document_version_id(
    conn: &rusqlite::Connection,
    document_id: i64,
) -> Result<Option<i64>, AppError> {
    let version_id = conn
        .query_row(
            "SELECT latest_version_id FROM knowledge_documents WHERE id = ?1",
            [document_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?;
    Ok(version_id.flatten())
}

fn mime_type_for_document_type(doc_type: &str) -> &'static str {
    match doc_type {
        "rich_text" => "text/html",
        _ => "text/markdown",
    }
}

fn get_asset_by_id(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<Option<KnowledgeAssetRecord>, AppError> {
    conn.query_row(
        "SELECT id, asset_key, content_hash, storage_key, original_name, normalized_name,
                mime_type, size_bytes, reference_count, deleted_at
         FROM knowledge_assets WHERE id = ?1",
        [id],
        map_asset,
    )
    .optional()
    .map_err(Into::into)
}

fn get_asset_by_content_hash(
    conn: &rusqlite::Connection,
    content_hash: &str,
) -> Result<Option<KnowledgeAssetRecord>, AppError> {
    conn.query_row(
        "SELECT id, asset_key, content_hash, storage_key, original_name, normalized_name,
                mime_type, size_bytes, reference_count, deleted_at
         FROM knowledge_assets WHERE content_hash = ?1",
        [content_hash],
        map_asset,
    )
    .optional()
    .map_err(Into::into)
}

fn get_draft_by_id(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<Option<KnowledgeDocumentDraftRecord>, AppError> {
    conn.query_row(
        "SELECT id, document_id, project_id, title, content, doc_type, base_version_id,
                revision, editor_label, deleted_at
         FROM knowledge_document_drafts WHERE id = ?1",
        [id],
        map_draft,
    )
    .optional()
    .map_err(Into::into)
}

fn map_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeAssetRecord> {
    Ok(KnowledgeAssetRecord {
        id: row.get(0)?,
        asset_key: row.get(1)?,
        content_hash: row.get(2)?,
        storage_key: row.get(3)?,
        original_name: row.get(4)?,
        normalized_name: row.get(5)?,
        mime_type: row.get(6)?,
        size_bytes: row.get(7)?,
        reference_count: row.get(8)?,
        deleted_at: row.get(9)?,
    })
}

fn map_draft(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeDocumentDraftRecord> {
    Ok(KnowledgeDocumentDraftRecord {
        id: row.get(0)?,
        document_id: row.get(1)?,
        project_id: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        doc_type: row.get(5)?,
        base_version_id: row.get(6)?,
        revision: row.get(7)?,
        editor_label: row.get(8)?,
        deleted_at: row.get(9)?,
    })
}

fn map_document_version_binding(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<KnowledgeDocumentVersionBindingRecord> {
    Ok(KnowledgeDocumentVersionBindingRecord {
        id: row.get(0)?,
        document_version_id: row.get(1)?,
        release_id: row.get(2)?,
        repository_binding_id: row.get(3)?,
        cross_version_scope: row.get(4)?,
    })
}

/// 调用方已在 Service 层验证范围归属；DAO 仍在事务内拒绝空范围，确保正式版本与绑定
/// 不会出现一边成功、一边遗漏的状态。
fn insert_document_version_binding_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    document_version_id: i64,
    release_id: Option<i64>,
    repository_binding_id: Option<i64>,
    cross_version_scope: &str,
) -> Result<(), AppError> {
    let cross_version_scope = cross_version_scope.trim();
    if document_version_id <= 0
        || (release_id.is_none()
            && repository_binding_id.is_none()
            && cross_version_scope.is_empty())
    {
        return Err(AppError::InvalidInput(
            "每个文档版本必须明确绑定项目版本、仓库或跨版本范围".to_string(),
        ));
    }
    tx.execute(
        "INSERT OR IGNORE INTO knowledge_document_version_bindings
            (document_version_id, release_id, repository_binding_id, cross_version_scope)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            document_version_id,
            release_id,
            repository_binding_id,
            cross_version_scope,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;

    use super::{
        CommitKnowledgeDocumentDraft, CreateKnowledgeDocumentUpload, Database, NewKnowledgeAsset,
        NewKnowledgeDocumentDraft, NewKnowledgeDocumentParseArtifact,
    };
    use crate::database::schema;

    fn database() -> Result<Database, Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        Ok(Database {
            conn: Mutex::new(connection),
        })
    }

    #[test]
    fn assets_drafts_bindings_and_parse_artifacts_keep_stable_records(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let asset = database.upsert_knowledge_asset(&NewKnowledgeAsset {
            asset_key: "asset-a".into(),
            content_hash: "content-a".into(),
            storage_key: "sha256/content-a".into(),
            original_name: "设计文档.md".into(),
            normalized_name: "设计文档.md".into(),
            mime_type: "text/markdown".into(),
            size_bytes: 12,
        })?;
        let same_asset = database.upsert_knowledge_asset(&NewKnowledgeAsset {
            asset_key: "asset-b".into(),
            content_hash: "content-a".into(),
            storage_key: "sha256/content-a".into(),
            original_name: "设计文档副本.md".into(),
            normalized_name: "设计文档副本.md".into(),
            mime_type: "text/markdown".into(),
            size_bytes: 12,
        })?;
        assert_eq!(asset.id, same_asset.id, "同内容资产应复用内容寻址记录");
        assert_eq!(
            database
                .adjust_knowledge_asset_reference_count(asset.id, 1)?
                .reference_count,
            1
        );
        assert!(database
            .adjust_knowledge_asset_reference_count(asset.id, -2)
            .is_err());

        let draft = database.create_knowledge_document_draft(&NewKnowledgeDocumentDraft {
            document_id: None,
            project_id: 1,
            title: "部署说明".into(),
            content: "第一版".into(),
            doc_type: "markdown".into(),
            base_version_id: None,
            editor_label: "测试用户".into(),
        })?;
        let saved = database
            .update_knowledge_document_draft(
                draft.id,
                draft.revision,
                "部署说明",
                "第二版",
                "测试用户",
            )?
            .expect("正确修订号应保存");
        assert_eq!(saved.revision, 2);
        assert_eq!(saved.content, "第二版");
        assert!(database
            .update_knowledge_document_draft(
                draft.id,
                draft.revision,
                "部署说明",
                "过期",
                "另一个用户"
            )?
            .is_none());

        let bindings = database
            .replace_knowledge_document_version_bindings(101, &[(Some(7), None, String::new())])?;
        assert_eq!(bindings[0].release_id, Some(7));
        assert!(database
            .replace_knowledge_document_version_bindings(101, &[(None, None, String::new())])
            .is_err());
        let artifact_id = database.insert_knowledge_document_parse_artifact(
            &NewKnowledgeDocumentParseArtifact {
                document_version_id: 101,
                asset_id: Some(asset.id),
                parser_id: "markdown".into(),
                parser_version: "1".into(),
                quality_level: "complete".into(),
                warning_json: "[]".into(),
                normalized_hash: "normalized-a".into(),
                structure_json: "[]".into(),
            },
        )?;
        assert!(artifact_id > 0);
        Ok(())
    }

    #[test]
    fn committed_version_rejects_updates_after_index_task_is_linked(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let draft = database.create_knowledge_document_draft(&NewKnowledgeDocumentDraft {
            document_id: None,
            project_id: 1,
            title: "不可变说明".into(),
            content: "已经确认的正文".into(),
            doc_type: "markdown".into(),
            base_version_id: None,
            editor_label: "本地用户".into(),
        })?;
        let committed =
            database.commit_knowledge_document_draft(&CommitKnowledgeDocumentDraft {
                draft_id: draft.id,
                expected_revision: draft.revision,
                version_label: "初始版本".into(),
                release_id: None,
                cross_version_scope: "project_all_versions".into(),
                commit_message: "确认入库".into(),
                author_label: "本地用户".into(),
                analysis_draft_id: None,
                content_hash: "a".repeat(64),
                token_estimate: 4,
            })?;
        let conn = database.conn.lock().map_err(|error| error.to_string())?;
        let error = conn
            .execute(
                "UPDATE knowledge_document_versions SET content = '篡改' WHERE id = ?1",
                [committed.document_version_id],
            )
            .expect_err("已提交版本必须拒绝任何更新");
        assert!(error.to_string().contains("不可修改"));
        Ok(())
    }

    #[test]
    fn upload_registration_rolls_back_document_and_job_when_asset_is_missing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let result = database.create_knowledge_document_upload(&CreateKnowledgeDocumentUpload {
            upload_key: "missing-asset".to_string(),
            project_id: 1,
            release_id: None,
            cross_version_scope: "project_all_versions".into(),
            asset_id: 9_999,
            asset_key: "sha256:missing".to_string(),
            original_name: "缺失资产.md".to_string(),
            source_folder_name: None,
            mime_type: "text/markdown".to_string(),
            document_type: "markdown".to_string(),
            title: "缺失资产".to_string(),
            allow_remote_ocr: false,
            ocr_provider_key: String::new(),
        });
        assert!(result.is_err());

        let conn = database.conn.lock().map_err(|error| error.to_string())?;
        let document_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_documents WHERE document_key = 'upload-document-missing-asset'",
            [],
            |row| row.get(0),
        )?;
        let job_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_jobs WHERE job_key = 'knowledge-upload-import-missing-asset'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(document_count, 0, "上传事务失败不得留下处理中逻辑文档");
        assert_eq!(job_count, 0, "上传事务失败不得留下孤立导入任务");
        Ok(())
    }

    #[test]
    fn retry_upload_restores_processing_state_before_requeueing_job(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let asset = database.upsert_knowledge_asset(&NewKnowledgeAsset {
            asset_key: "retry-asset".into(),
            content_hash: "b".repeat(64),
            storage_key: "sha256/bb/retry".into(),
            original_name: "重试说明.md".into(),
            normalized_name: "重试说明.md".into(),
            mime_type: "text/markdown".into(),
            size_bytes: 8,
        })?;
        let upload = database.create_knowledge_document_upload(&CreateKnowledgeDocumentUpload {
            upload_key: "retry-upload".into(),
            project_id: 1,
            release_id: None,
            cross_version_scope: "project_all_versions".into(),
            asset_id: asset.id,
            asset_key: asset.asset_key,
            original_name: "重试说明.md".into(),
            source_folder_name: None,
            mime_type: "text/markdown".into(),
            document_type: "markdown".into(),
            title: "重试说明".into(),
            allow_remote_ocr: false,
            ocr_provider_key: String::new(),
        })?;
        database.set_knowledge_document_upload_status(
            upload.import_job_id,
            "failed",
            "模拟损坏",
        )?;
        database.finish_knowledge_job(
            upload.import_job_id,
            "failed",
            "上传文档导入失败",
            Some("模拟损坏"),
            &serde_json::json!({"stage": "failed"}),
        )?;

        let restarted = database.restart_knowledge_document_upload(upload.import_job_id)?;

        assert_eq!(restarted.status, "queued");
        assert!(database
            .get_pending_knowledge_document_upload(upload.import_job_id)?
            .is_some());
        Ok(())
    }

    #[test]
    fn upload_failure_does_not_override_a_prior_cancel_request(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let asset = database.upsert_knowledge_asset(&NewKnowledgeAsset {
            asset_key: "cancel-asset".into(),
            content_hash: "c".repeat(64),
            storage_key: "sha256/cc/cancel".into(),
            original_name: "取消说明.md".into(),
            normalized_name: "取消说明.md".into(),
            mime_type: "text/markdown".into(),
            size_bytes: 8,
        })?;
        let upload = database.create_knowledge_document_upload(&CreateKnowledgeDocumentUpload {
            upload_key: "cancel-upload".into(),
            project_id: 1,
            release_id: None,
            cross_version_scope: "project_all_versions".into(),
            asset_id: asset.id,
            asset_key: asset.asset_key,
            original_name: "取消说明.md".into(),
            source_folder_name: None,
            mime_type: "text/markdown".into(),
            document_type: "markdown".into(),
            title: "取消说明".into(),
            allow_remote_ocr: false,
            ocr_provider_key: String::new(),
        })?;
        database.mark_knowledge_job_running(
            upload.import_job_id,
            "parse",
            "正在解析",
            &serde_json::json!({"stage": "parse"}),
        )?;
        database.request_knowledge_job_cancel(upload.import_job_id)?;

        let terminal = database.fail_knowledge_document_upload_import_or_cancel(
            upload.import_job_id,
            "模拟解析错误",
        )?;

        assert_eq!(terminal.status, "cancelled");
        assert!(terminal.error.is_none());
        Ok(())
    }

    #[test]
    fn queued_upload_cancel_finalizes_its_document_lifecycle(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let asset = database.upsert_knowledge_asset(&NewKnowledgeAsset {
            asset_key: "queued-cancel-asset".into(),
            content_hash: "d".repeat(64),
            storage_key: "sha256/dd/queued-cancel".into(),
            original_name: "排队取消说明.md".into(),
            normalized_name: "排队取消说明.md".into(),
            mime_type: "text/markdown".into(),
            size_bytes: 8,
        })?;
        let upload = database.create_knowledge_document_upload(&CreateKnowledgeDocumentUpload {
            upload_key: "queued-cancel-upload".into(),
            project_id: 1,
            release_id: None,
            cross_version_scope: "project_all_versions".into(),
            asset_id: asset.id,
            asset_key: asset.asset_key,
            original_name: "排队取消说明.md".into(),
            source_folder_name: None,
            mime_type: "text/markdown".into(),
            document_type: "markdown".into(),
            title: "排队取消说明".into(),
            allow_remote_ocr: false,
            ocr_provider_key: String::new(),
        })?;
        let terminal = database.request_knowledge_document_upload_cancel(upload.import_job_id)?;

        assert_eq!(terminal.status, "cancelled");
        assert!(database
            .get_pending_knowledge_document_upload(upload.import_job_id)?
            .is_none());
        Ok(())
    }
}
