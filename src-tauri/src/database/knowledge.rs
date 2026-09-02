use rusqlite::{
    params, params_from_iter, types::Value, Connection, OptionalExtension, Row, Transaction,
};

use super::Database;
use crate::database::knowledge_domain::documents::{
    insert_knowledge_document_parse_artifact_in_transaction, NewKnowledgeDocumentParseArtifact,
};
use crate::database::knowledge_domain::search::{
    append_selected_document_version_filter, rebuild_knowledge_document_title_index_in_transaction,
    release_scope_visibility_predicate, sync_knowledge_document_title_index,
};
use crate::error::AppError;
use crate::models::{
    CreateKnowledgeCodeSnapshotInput, CreateKnowledgeDocumentVersionInput, CreateKnowledgeJobInput,
    KnowledgeChunk, KnowledgeChunkEmbedding, KnowledgeChunkWriteInput, KnowledgeCitation,
    KnowledgeCodeFile, KnowledgeCodeFileWriteInput, KnowledgeCodeRelation,
    KnowledgeCodeRelationWriteInput, KnowledgeCodeSnapshot, KnowledgeCodeSource,
    KnowledgeCodeSourceSettings, KnowledgeCodeSymbol, KnowledgeCodeSymbolWriteInput,
    KnowledgeDocument, KnowledgeDocumentDeletionImpactPreview, KnowledgeDocumentParseSummary,
    KnowledgeDocumentProcessingSummary, KnowledgeDocumentProcessingTaskSummary,
    KnowledgeDocumentVersion, KnowledgeEmbeddingIndexValidation, KnowledgeEmbeddingProfile,
    KnowledgeFtsCapability, KnowledgeJob, KnowledgeListInput, KnowledgePage, KnowledgeProject,
    KnowledgeRelation, KnowledgeRelease, KnowledgeRetrievalEvaluationRun, KnowledgeSearchHit,
    KnowledgeSearchInput, KnowledgeSource, ListKnowledgeRelationsInput,
    UpsertKnowledgeCodeSourceInput, UpsertKnowledgeDocumentInput,
    UpsertKnowledgeEmbeddingProfileInput, UpsertKnowledgeProjectInput,
    UpsertKnowledgeRelationInput, UpsertKnowledgeReleaseInput, UpsertKnowledgeSourceInput,
    UpsertZentaoConnectionInput, UpsertZentaoEntityInput, UpsertZentaoEntityRelationInput,
    UpsertZentaoProjectMappingInput, ZentaoConnection, ZentaoEntity, ZentaoEntityRelation,
    ZentaoProjectMapping, ZentaoSyncCursor, ZentaoSyncCursorUpdateInput,
};

const DEFAULT_PAGE_LIMIT: i64 = 50;
const MAX_PAGE_LIMIT: i64 = 200;

/// 仅供后台文档索引任务使用的原子完成载荷。分块写入与任务终态共用同一事务，调用方
/// 不得拆成“先写入、再完成”两次数据库操作。
pub(crate) struct CompleteKnowledgeDocumentIndexJobInput<'a> {
    pub document_version_id: i64,
    pub parsed_meta: &'a serde_json::Value,
    pub token_estimate: i64,
    pub chunks: &'a [KnowledgeChunkWriteInput],
    pub parse_artifact: &'a NewKnowledgeDocumentParseArtifact,
    pub job_id: i64,
    pub message: &'a str,
    pub checkpoint: &'a serde_json::Value,
}

/// 文档索引解析失败后的终态收尾载荷。失败只可在没有取消请求时写入；已经提交的取消
/// 请求优先转换为 cancelled，避免界面显示与用户操作相反的失败状态。
pub(crate) struct FailKnowledgeDocumentIndexJobInput<'a> {
    pub job_id: i64,
    pub error: &'a str,
    pub failed_checkpoint: &'a serde_json::Value,
    pub cancelled_checkpoint: &'a serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct KnowledgeDocumentSyncState {
    pub id: i64,
    pub document_key: String,
    pub logical_path: String,
    pub content_hash: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct KnowledgeVectorCandidate {
    pub profile_id: i64,
    pub profile_dimension: i64,
    pub chunk_id: i64,
    pub document_version_id: i64,
    pub document_id: i64,
    pub project_id: Option<i64>,
    pub release_id: Option<i64>,
    pub source_id: Option<i64>,
    pub doc_type: String,
    pub sensitivity: String,
    pub title: String,
    pub logical_path: String,
    pub heading_path: String,
    pub commit_sha: String,
    pub content: String,
    pub location: serde_json::Value,
    pub vector: Vec<f32>,
    pub vector_norm: f64,
}

/// 仅供 Embedding Service 在进度预估中使用的内部记录，绝不能直接穿透 Command 或 Dev API。
#[derive(Debug, Clone)]
pub struct KnowledgeEmbeddingRebuildCandidate {
    pub chunk_id: i64,
    pub document_id: i64,
    pub source_id: Option<i64>,
    pub source_key: String,
    pub source_name: String,
    pub source_enabled: bool,
    pub source_allows_remote_embedding: bool,
    pub sensitivity: String,
    pub content: String,
    pub content_hash: String,
    pub existing_embedding_content_hash: Option<String>,
}

impl Database {
    pub fn upsert_zentao_connection(
        &self,
        input: &UpsertZentaoConnectionInput,
    ) -> Result<ZentaoConnection, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        if let Some(id) = input.id {
            let changed = conn.execute(
                "UPDATE zentao_connections SET
                    connection_key = ?1, name = ?2, base_url = ?3, api_version = ?4,
                    auth_mode = ?5, endpoint_profile = ?6, credential_key = ?7,
                    tls_verify = ?8, allow_insecure_http = ?9, request_timeout_seconds = ?10, page_size = ?11,
                    rate_limit_per_second = ?12, enabled = ?13,
                    updated_at = datetime('now', 'localtime'), deleted_at = NULL
                 WHERE id = ?14",
                params![
                    input.connection_key.trim(),
                    input.name.trim(),
                    input.base_url.trim(),
                    input.api_version.trim(),
                    input.auth_mode.trim(),
                    input.endpoint_profile.trim(),
                    input.credential_key.trim(),
                    input.tls_verify as i64,
                    input.allow_insecure_http as i64,
                    input.request_timeout_seconds,
                    input.page_size,
                    input.rate_limit_per_second,
                    input.enabled as i64,
                    id,
                ],
            )?;
            if changed == 0 {
                return Err(AppError::NotFound(format!("禅道连接不存在: {id}")));
            }
            return get_zentao_connection(&conn, id);
        }
        conn.execute(
            "INSERT INTO zentao_connections
             (connection_key, name, base_url, api_version, auth_mode, endpoint_profile,
              credential_key, tls_verify, allow_insecure_http, request_timeout_seconds, page_size,
              rate_limit_per_second, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(connection_key) DO UPDATE SET
                name = excluded.name, base_url = excluded.base_url, api_version = excluded.api_version,
                auth_mode = excluded.auth_mode, endpoint_profile = excluded.endpoint_profile,
                credential_key = excluded.credential_key, tls_verify = excluded.tls_verify,
                allow_insecure_http = excluded.allow_insecure_http,
                request_timeout_seconds = excluded.request_timeout_seconds, page_size = excluded.page_size,
                rate_limit_per_second = excluded.rate_limit_per_second, enabled = excluded.enabled,
                updated_at = datetime('now', 'localtime'), deleted_at = NULL",
            params![
                input.connection_key.trim(), input.name.trim(), input.base_url.trim(),
                input.api_version.trim(), input.auth_mode.trim(), input.endpoint_profile.trim(),
                input.credential_key.trim(), input.tls_verify as i64,
                input.allow_insecure_http as i64, input.request_timeout_seconds, input.page_size, input.rate_limit_per_second,
                input.enabled as i64,
            ],
        )?;
        let id = conn.query_row(
            "SELECT id FROM zentao_connections WHERE connection_key = ?1",
            [input.connection_key.trim()],
            |row| row.get(0),
        )?;
        get_zentao_connection(&conn, id)
    }

    pub fn list_zentao_connections(&self) -> Result<Vec<ZentaoConnection>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let items = conn
            .prepare(ZENTAO_CONNECTION_SELECT)?
            .query_map([], map_zentao_connection)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(items)
    }

    pub fn get_zentao_connection_by_id(
        &self,
        id: i64,
    ) -> Result<Option<ZentaoConnection>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_zentao_connection(&conn, id)
            .map(Some)
            .or_else(|error| match error {
                AppError::Database(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                error => Err(error),
            })
    }

    pub fn update_zentao_connection_probe(
        &self,
        id: i64,
        api_version: &str,
        auth_mode: &str,
        endpoint_profile: &str,
        capabilities: &serde_json::Value,
        status: &str,
        error: Option<&str>,
    ) -> Result<ZentaoConnection, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|lock_error| AppError::Custom(lock_error.to_string()))?;
        let changed = conn.execute(
            "UPDATE zentao_connections SET api_version = ?1, auth_mode = ?2, endpoint_profile = ?3,
                capabilities_json = ?4, last_test_status = ?5, last_tested_at = datetime('now', 'localtime'),
                last_error = ?6, updated_at = datetime('now', 'localtime')
             WHERE id = ?7 AND deleted_at IS NULL",
            params![api_version, auth_mode, endpoint_profile, serde_json::to_string(capabilities)?, status, error, id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("禅道连接不存在: {id}")));
        }
        get_zentao_connection(&conn, id)
    }

    pub fn soft_delete_zentao_connection(&self, id: i64) -> Result<(), AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let changed = transaction.execute(
            "UPDATE zentao_connections SET deleted_at = datetime('now', 'localtime'), enabled = 0,
                updated_at = datetime('now', 'localtime') WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("禅道连接不存在: {id}")));
        }
        // 保留映射历史，但被删除的连接绝不能继续作为同步入口。
        transaction.execute(
            "UPDATE zentao_project_mappings SET enabled = 0,
                updated_at = datetime('now', 'localtime')
             WHERE connection_id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn upsert_zentao_project_mapping(
        &self,
        input: &UpsertZentaoProjectMappingInput,
    ) -> Result<ZentaoProjectMapping, AppError> {
        let executions = serde_json::to_string(&input.remote_execution_ids)?;
        let release_mapping = serde_json::to_string(&input.release_mapping)?;
        let sync_scope = serde_json::to_string(&input.sync_scope)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        // 实体的远端 ID 以连接为幂等范围；同一远程项目不能拆映射到多个知识项目，
        // 否则后一次同步会覆盖前一映射的实体归属。
        let conflicting_mapping = transaction
            .query_row(
                "SELECT id FROM zentao_project_mappings
                 WHERE connection_id = ?1 AND remote_project_id = ?2 AND deleted_at IS NULL
                   AND id != ?3 LIMIT 1",
                params![
                    input.connection_id,
                    input.remote_project_id.trim(),
                    input.id.unwrap_or(-1),
                ],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if conflicting_mapping.is_some() {
            return Err(AppError::InvalidInput(
                "同一禅道远程项目不能同时映射到多个知识项目".to_string(),
            ));
        }
        let connection_exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM zentao_connections
             WHERE id = ?1 AND enabled = 1 AND deleted_at IS NULL)",
            [input.connection_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !connection_exists {
            return Err(AppError::NotFound("可用禅道连接不存在".to_string()));
        }
        let project_exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_projects
             WHERE id = ?1 AND enabled = 1 AND deleted_at IS NULL)",
            [input.knowledge_project_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !project_exists {
            return Err(AppError::NotFound("可用知识项目不存在".to_string()));
        }
        if let Some(id) = input.id {
            let changed = transaction.execute(
                "UPDATE zentao_project_mappings SET connection_id = ?1, knowledge_project_id = ?2,
                    remote_product_id = ?3, remote_project_id = ?4, remote_execution_ids_json = ?5,
                    release_mapping_json = ?6, sync_scope_json = ?7, sync_since = ?8,
                    include_comments = ?9, include_worklogs = ?10, include_attachment_metadata = ?11,
                    allow_remote_embedding = ?12, allow_remote_ai = ?13, enabled = ?14,
                    updated_at = datetime('now', 'localtime'), deleted_at = NULL WHERE id = ?15",
                params![input.connection_id, input.knowledge_project_id, input.remote_product_id.trim(),
                    input.remote_project_id.trim(), executions, release_mapping, sync_scope, input.sync_since,
                    input.include_comments as i64, input.include_worklogs as i64,
                    input.include_attachment_metadata as i64, input.allow_remote_embedding as i64,
                    input.allow_remote_ai as i64, input.enabled as i64, id],
            )?;
            if changed == 0 {
                return Err(AppError::NotFound(format!("禅道项目映射不存在: {id}")));
            }
            let mapping = get_zentao_project_mapping(&transaction, id)?;
            transaction.commit()?;
            return Ok(mapping);
        }
        transaction.execute(
            "INSERT INTO zentao_project_mappings (connection_id, knowledge_project_id, remote_product_id,
              remote_project_id, remote_execution_ids_json, release_mapping_json, sync_scope_json, sync_since,
              include_comments, include_worklogs, include_attachment_metadata, allow_remote_embedding,
              allow_remote_ai, enabled) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(connection_id, knowledge_project_id, remote_project_id) DO UPDATE SET
              remote_product_id=excluded.remote_product_id, remote_execution_ids_json=excluded.remote_execution_ids_json,
              release_mapping_json=excluded.release_mapping_json, sync_scope_json=excluded.sync_scope_json,
              sync_since=excluded.sync_since, include_comments=excluded.include_comments,
              include_worklogs=excluded.include_worklogs, include_attachment_metadata=excluded.include_attachment_metadata,
              allow_remote_embedding=excluded.allow_remote_embedding, allow_remote_ai=excluded.allow_remote_ai,
              enabled=excluded.enabled, updated_at=datetime('now', 'localtime'), deleted_at=NULL",
            params![input.connection_id, input.knowledge_project_id, input.remote_product_id.trim(),
                input.remote_project_id.trim(), executions, release_mapping, sync_scope, input.sync_since,
                input.include_comments as i64, input.include_worklogs as i64,
                input.include_attachment_metadata as i64, input.allow_remote_embedding as i64,
                input.allow_remote_ai as i64, input.enabled as i64],
        )?;
        let id = transaction.query_row(
            "SELECT id FROM zentao_project_mappings WHERE connection_id = ?1 AND knowledge_project_id = ?2 AND remote_project_id = ?3",
            params![input.connection_id, input.knowledge_project_id, input.remote_project_id.trim()], |row| row.get(0))?;
        let mapping = get_zentao_project_mapping(&transaction, id)?;
        transaction.commit()?;
        Ok(mapping)
    }

    pub fn get_zentao_project_mapping_by_id(
        &self,
        id: i64,
    ) -> Result<Option<ZentaoProjectMapping>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_zentao_project_mapping(&conn, id)
            .map(Some)
            .or_else(|error| match error {
                AppError::Database(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                error => Err(error),
            })
    }

    /// 映射是同步、事实文档和 UI 状态的唯一入口。只返回未软删除记录，避免浏览器
    /// Dev API 为了展示映射而绕过 Service 直接查询 SQLite。
    pub fn list_zentao_project_mappings(
        &self,
        connection_id: Option<i64>,
    ) -> Result<Vec<ZentaoProjectMapping>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, connection_id, knowledge_project_id, remote_product_id, remote_project_id,
                    remote_execution_ids_json, release_mapping_json, sync_scope_json, sync_since,
                    include_comments, include_worklogs, include_attachment_metadata, allow_remote_embedding,
                    allow_remote_ai, enabled, created_at, updated_at, deleted_at
             FROM zentao_project_mappings
             WHERE deleted_at IS NULL AND (?1 IS NULL OR connection_id = ?1)
             ORDER BY connection_id, remote_project_id, id",
        )?;
        let mappings = statement
            .query_map([connection_id], map_zentao_project_mapping)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(mappings)
    }

    /// 在每个远端分页请求前确认映射、连接和知识项目仍同时有效。删除操作会在同一
    /// SQLite 事务中禁用映射，运行中的同步因此能在下一安全边界停止。
    pub fn ensure_zentao_mapping_sync_active(&self, mapping_id: i64) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        ensure_zentao_mapping_sync_active(&conn, mapping_id)
    }

    /// 每种禅道实体有独立游标。调用者可在分页中持续保存 checkpoint；只有完整成功时
    /// 才允许推进成功游标，避免部分失败造成数据窗口丢失。
    pub fn upsert_zentao_sync_cursor(
        &self,
        input: &ZentaoSyncCursorUpdateInput,
    ) -> Result<ZentaoSyncCursor, AppError> {
        let checkpoint_json = serde_json::to_string(&input.checkpoint)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        ensure_zentao_mapping_sync_active(&transaction, input.mapping_id)?;
        transaction.execute(
            "INSERT INTO zentao_sync_cursors (mapping_id, entity_type, last_updated_at,
                last_external_id, checkpoint_json, last_success_at, last_full_sync_at)
             VALUES (?1, ?2, ?3, ?4, ?5,
                CASE WHEN ?6 THEN datetime('now', 'localtime') ELSE NULL END,
                CASE WHEN ?6 THEN datetime('now', 'localtime') ELSE NULL END)
             ON CONFLICT(mapping_id, entity_type) DO UPDATE SET
                last_updated_at = CASE WHEN ?6 THEN excluded.last_updated_at
                    ELSE zentao_sync_cursors.last_updated_at END,
                last_external_id = CASE WHEN ?6 THEN excluded.last_external_id
                    ELSE zentao_sync_cursors.last_external_id END,
                checkpoint_json = excluded.checkpoint_json,
                last_success_at = CASE WHEN ?6 THEN datetime('now', 'localtime')
                    ELSE zentao_sync_cursors.last_success_at END,
                last_full_sync_at = CASE WHEN ?6 THEN datetime('now', 'localtime')
                    ELSE zentao_sync_cursors.last_full_sync_at END,
                updated_at = datetime('now', 'localtime')",
            params![
                input.mapping_id,
                input.entity_type.trim(),
                input.last_updated_at.trim(),
                input.last_external_id.trim(),
                checkpoint_json,
                input.completed_full_sync as i64,
            ],
        )?;
        let cursor =
            get_zentao_sync_cursor(&transaction, input.mapping_id, input.entity_type.trim())?;
        transaction.commit()?;
        Ok(cursor)
    }

    pub fn get_zentao_sync_cursor(
        &self,
        mapping_id: i64,
        entity_type: &str,
    ) -> Result<Option<ZentaoSyncCursor>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_zentao_sync_cursor(&conn, mapping_id, entity_type)
            .map(Some)
            .or_else(|error| match error {
                AppError::Database(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                error => Err(error),
            })
    }

    /// 以连接、实体类型和外部 ID 为幂等键。内容未变时只更新同步时间和缺失状态，避免
    /// 触发重复文档生成或向量化；正文变化才覆盖当前规范化快照。
    pub fn upsert_zentao_entity(
        &self,
        input: &UpsertZentaoEntityInput,
    ) -> Result<(ZentaoEntity, bool), AppError> {
        let raw_snapshot_json = input
            .raw_snapshot
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        ensure_zentao_mapping_sync_active(&transaction, input.mapping_id)?;
        let previous_hash = transaction
            .query_row(
                "SELECT content_hash FROM zentao_entities
                 WHERE connection_id = ?1 AND entity_type = ?2 AND external_id = ?3",
                params![
                    input.connection_id,
                    input.entity_type.trim(),
                    input.external_id.trim()
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let changed = previous_hash.as_deref() != Some(input.content_hash.as_str());
        transaction.execute(
            "INSERT INTO zentao_entities (connection_id, mapping_id, knowledge_project_id, release_id,
                entity_type, external_id, external_key, title, body_markdown, original_status,
                normalized_status, assignee_external_id, parent_external_key, remote_url, content_hash,
                raw_json_hash, raw_snapshot_json, source_created_at, source_updated_at, missing_count,
                status, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                ?17, ?18, ?19, 0, 'active', NULL)
             ON CONFLICT(connection_id, entity_type, external_id) DO UPDATE SET
                mapping_id = excluded.mapping_id, knowledge_project_id = excluded.knowledge_project_id,
                release_id = excluded.release_id, external_key = excluded.external_key,
                title = CASE WHEN zentao_entities.content_hash != excluded.content_hash THEN excluded.title ELSE zentao_entities.title END,
                body_markdown = CASE WHEN zentao_entities.content_hash != excluded.content_hash THEN excluded.body_markdown ELSE zentao_entities.body_markdown END,
                original_status = excluded.original_status, normalized_status = excluded.normalized_status,
                assignee_external_id = excluded.assignee_external_id, parent_external_key = excluded.parent_external_key,
                remote_url = excluded.remote_url, content_hash = excluded.content_hash,
                raw_json_hash = excluded.raw_json_hash,
                raw_snapshot_json = CASE WHEN zentao_entities.content_hash != excluded.content_hash THEN excluded.raw_snapshot_json ELSE zentao_entities.raw_snapshot_json END,
                source_created_at = excluded.source_created_at, source_updated_at = excluded.source_updated_at,
                last_synced_at = datetime('now', 'localtime'), missing_count = 0, status = 'active', deleted_at = NULL",
            params![
                input.connection_id, input.mapping_id, input.knowledge_project_id, input.release_id,
                input.entity_type.trim(), input.external_id.trim(), input.external_key.trim(),
                input.title.trim(), input.body_markdown, input.original_status.trim(), input.normalized_status.trim(),
                input.assignee_external_id.trim(), input.parent_external_key.trim(), input.remote_url.trim(),
                input.content_hash.trim(), input.raw_json_hash.trim(), raw_snapshot_json,
                input.source_created_at, input.source_updated_at,
            ],
        )?;
        let entity = get_zentao_entity(
            &transaction,
            input.connection_id,
            input.entity_type.trim(),
            input.external_id.trim(),
        )?;
        transaction.commit()?;
        Ok((entity, changed))
    }

    /// 仅在某实体类型全量分页完成后调用。连续两次完整同步均不可见才标记删除，短暂权限
    /// 或网络异常不会清除历史事实。
    pub fn confirm_zentao_missing_entities(
        &self,
        mapping_id: i64,
        entity_type: &str,
        seen_external_ids: &[String],
    ) -> Result<i64, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        ensure_zentao_mapping_sync_active(&transaction, mapping_id)?;
        let mut sql = String::from(
            "UPDATE zentao_entities SET missing_count = missing_count + 1,
                status = CASE WHEN missing_count + 1 >= 2 THEN 'deleted' ELSE 'missing' END,
                deleted_at = CASE WHEN missing_count + 1 >= 2 THEN datetime('now', 'localtime') ELSE deleted_at END
             WHERE mapping_id = ? AND entity_type = ? AND deleted_at IS NULL",
        );
        let mut values = vec![
            Value::Integer(mapping_id),
            Value::Text(entity_type.trim().to_string()),
        ];
        if !seen_external_ids.is_empty() {
            sql.push_str(" AND external_id NOT IN (");
            sql.push_str(
                &std::iter::repeat_n("?", seen_external_ids.len())
                    .collect::<Vec<_>>()
                    .join(","),
            );
            sql.push(')');
            values.extend(seen_external_ids.iter().cloned().map(Value::Text));
        }
        let changed = transaction.execute(&sql, params_from_iter(values))?;
        transaction.commit()?;
        Ok(changed as i64)
    }

    /// 生成事实文档只能读取当前映射下仍有效的规范化实体，不读取远端响应或凭据。
    pub fn list_zentao_entities_for_mapping(
        &self,
        mapping_id: i64,
    ) -> Result<Vec<ZentaoEntity>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, connection_id, mapping_id, knowledge_project_id, release_id, entity_type,
                    external_id, external_key, title, body_markdown, original_status,
                    normalized_status, assignee_external_id, parent_external_key, remote_url,
                    content_hash, raw_json_hash, raw_snapshot_json, source_created_at,
                    source_updated_at, first_synced_at, last_synced_at, missing_count, status,
                    deleted_at
             FROM zentao_entities
             WHERE mapping_id = ?1 AND status = 'active' AND deleted_at IS NULL
             ORDER BY entity_type, external_key",
        )?;
        let entities = statement
            .query_map([mapping_id], map_zentao_entity)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(entities)
    }

    /// 仅保存具有明确来源字段证据的禅道实体关系。唯一键使重复同步可重放而不制造关系副本。
    pub fn upsert_zentao_entity_relation(
        &self,
        input: &UpsertZentaoEntityRelationInput,
    ) -> Result<ZentaoEntityRelation, AppError> {
        let evidence_json = serde_json::to_string(&input.evidence)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO zentao_entity_relations
             (from_external_key, relation_type, to_external_key, evidence_json, source, confidence, confirmed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(from_external_key, relation_type, to_external_key, source) DO UPDATE SET
                evidence_json = excluded.evidence_json, confidence = excluded.confidence,
                confirmed = excluded.confirmed, updated_at = datetime('now', 'localtime'), deleted_at = NULL",
            params![
                input.from_external_key.trim(),
                input.relation_type.trim(),
                input.to_external_key.trim(),
                evidence_json,
                input.source.trim(),
                input.confidence,
                input.confirmed as i64,
            ],
        )?;
        conn.query_row(
            "SELECT id, from_external_key, relation_type, to_external_key, evidence_json, source,
                    confidence, confirmed, created_at, updated_at, deleted_at
             FROM zentao_entity_relations
             WHERE from_external_key = ?1 AND relation_type = ?2 AND to_external_key = ?3
               AND source = ?4 AND deleted_at IS NULL",
            params![
                input.from_external_key.trim(),
                input.relation_type.trim(),
                input.to_external_key.trim(),
                input.source.trim(),
            ],
            map_zentao_entity_relation,
        )
        .map_err(AppError::from)
    }

    pub fn upsert_knowledge_code_snapshot(
        &self,
        input: &CreateKnowledgeCodeSnapshotInput,
    ) -> Result<KnowledgeCodeSnapshot, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_code_snapshots
             (snapshot_key, source_id, project_id, release_id, snapshot_type, ref_name,
              commit_sha, base_commit_sha, branch_name, worktree_dirty, captured_at, file_count,
              dirty_state_json, analyzer_version, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(snapshot_key) DO UPDATE SET
                file_count = excluded.file_count,
                dirty_state_json = excluded.dirty_state_json,
                analyzer_version = excluded.analyzer_version,
                status = excluded.status,
                error = NULL,
                updated_at = datetime('now', 'localtime')",
            params![
                input.snapshot_key.trim(),
                input.source_id,
                input.project_id,
                input.release_id,
                input.snapshot_type.trim(),
                input.ref_name.trim(),
                input.commit_sha.trim(),
                input.base_commit_sha.trim(),
                input.branch_name.trim(),
                input.worktree_dirty as i64,
                input.captured_at.trim(),
                input.file_count,
                serde_json::to_string(&input.dirty_state)?,
                input.analyzer_version.trim(),
                input.status.trim(),
            ],
        )?;
        let id = conn.query_row(
            "SELECT id FROM knowledge_code_snapshots WHERE snapshot_key = ?1",
            [input.snapshot_key.trim()],
            |row| row.get(0),
        )?;
        get_knowledge_code_snapshot(&conn, id)
    }

    pub fn list_knowledge_code_snapshots(
        &self,
        source_id: Option<i64>,
    ) -> Result<Vec<KnowledgeCodeSnapshot>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, snapshot_key, source_id, project_id, release_id, snapshot_type,
                    ref_name, commit_sha, base_commit_sha, branch_name, worktree_dirty,
                    dirty_state_json, captured_at, file_count, symbol_count, relation_count,
                    analyzer_version, status, error, created_at, updated_at
             FROM knowledge_code_snapshots
             WHERE (?1 IS NULL OR source_id = ?1)
             ORDER BY captured_at DESC, id DESC",
        )?;
        let snapshots = statement
            .query_map([source_id], map_knowledge_code_snapshot)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(snapshots)
    }

    pub fn get_knowledge_code_snapshot_by_id(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeCodeSnapshot>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_knowledge_code_snapshot(&conn, id)
            .map(Some)
            .or_else(|error| match error {
                AppError::Database(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                error => Err(error),
            })
    }

    /// 以“快照 + 相对路径”覆盖一份代码文件分析结果。先替换该文件的符号，再更新快照
    /// 统计，确保删除或重新分析时不会留下上一次的失效符号。
    pub fn replace_knowledge_code_file_analysis(
        &self,
        input: &KnowledgeCodeFileWriteInput,
        symbols: &[KnowledgeCodeSymbolWriteInput],
    ) -> Result<KnowledgeCodeFile, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let previous_document_version_id = transaction
            .query_row(
                "SELECT document_version_id FROM knowledge_code_files
                 WHERE snapshot_id = ?1 AND relative_path = ?2",
                params![input.snapshot_id, input.relative_path.trim()],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten();
        // 再次分析若因秘密、二进制或范围规则而跳过，旧的正文版本必须立即失效并退出
        // FTS/向量候选，而不是仅删除符号后仍被普通代码文档检索命中。
        if input.document_version_id.is_none() {
            if let Some(version_id) = previous_document_version_id {
                transaction
                    .execute(
                        "DELETE FROM knowledge_chunks_fts
                     WHERE CAST(chunk_id AS INTEGER) IN (
                        SELECT id FROM knowledge_chunks WHERE document_version_id = ?1
                     )",
                        [version_id],
                    )
                    .or_else(|error| match error {
                        rusqlite::Error::SqliteFailure(_, Some(message))
                            if message.contains("no such table") =>
                        {
                            Ok(0)
                        }
                        error => Err(error),
                    })?;
                transaction.execute(
                    "DELETE FROM knowledge_chunk_embeddings
                     WHERE chunk_id IN (
                        SELECT id FROM knowledge_chunks WHERE document_version_id = ?1
                     )",
                    [version_id],
                )?;
                // restricted 内容不只是“不可检索”：持久层也不保留其旧正文或片段，
                // 仅保留版本哈希及代码文件上的 skip reason 作为可审计元数据。
                transaction.execute(
                    "DELETE FROM knowledge_chunks WHERE document_version_id = ?1",
                    [version_id],
                )?;
                transaction.execute(
                    "UPDATE knowledge_document_versions SET valid = 0, content = '' WHERE id = ?1",
                    [version_id],
                )?;
                transaction.execute(
                    "UPDATE knowledge_documents
                     SET sensitivity = 'restricted', allow_ai = 0, allow_mcp = 0,
                         updated_at = datetime('now', 'localtime')
                     WHERE id = (SELECT document_id FROM knowledge_document_versions WHERE id = ?1)",
                    [version_id],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO knowledge_code_files
             (snapshot_id, document_version_id, relative_path, language, file_size, content_hash,
              analysis_level, is_generated, is_test, sensitivity, status, skip_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(snapshot_id, relative_path) DO UPDATE SET
                document_version_id = excluded.document_version_id,
                language = excluded.language, file_size = excluded.file_size,
                content_hash = excluded.content_hash, analysis_level = excluded.analysis_level,
                is_generated = excluded.is_generated, is_test = excluded.is_test,
                sensitivity = excluded.sensitivity, status = excluded.status,
                skip_reason = excluded.skip_reason",
            params![
                input.snapshot_id,
                input.document_version_id,
                input.relative_path.trim(),
                input.language.trim(),
                input.file_size,
                input.content_hash.trim(),
                input.analysis_level.trim(),
                input.is_generated as i64,
                input.is_test as i64,
                input.sensitivity.trim(),
                input.status.trim(),
                input.skip_reason.trim(),
            ],
        )?;
        let file_id = transaction.query_row(
            "SELECT id FROM knowledge_code_files WHERE snapshot_id = ?1 AND relative_path = ?2",
            params![input.snapshot_id, input.relative_path.trim()],
            |row| row.get::<_, i64>(0),
        )?;
        transaction.execute(
            "DELETE FROM knowledge_code_symbols WHERE snapshot_id = ?1 AND file_id = ?2",
            params![input.snapshot_id, file_id],
        )?;
        for symbol in symbols {
            transaction.execute(
                "INSERT INTO knowledge_code_symbols
                 (snapshot_id, file_id, symbol_key, symbol_kind, name, qualified_name, signature,
                  visibility, parent_symbol_key, start_line, start_column, end_line, end_column,
                  doc_comment, content_hash, analysis_level)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                 ON CONFLICT(snapshot_id, symbol_key) DO UPDATE SET
                    file_id = excluded.file_id, symbol_kind = excluded.symbol_kind,
                    name = excluded.name, qualified_name = excluded.qualified_name,
                    signature = excluded.signature, visibility = excluded.visibility,
                    parent_symbol_key = excluded.parent_symbol_key,
                    start_line = excluded.start_line, start_column = excluded.start_column,
                    end_line = excluded.end_line, end_column = excluded.end_column,
                    doc_comment = excluded.doc_comment, content_hash = excluded.content_hash,
                    analysis_level = excluded.analysis_level",
                params![
                    input.snapshot_id,
                    file_id,
                    symbol.symbol_key.trim(),
                    symbol.symbol_kind.trim(),
                    symbol.name.trim(),
                    symbol.qualified_name.trim(),
                    symbol.signature.trim(),
                    symbol.visibility.trim(),
                    symbol.parent_symbol_key.trim(),
                    symbol.start_line,
                    symbol.start_column,
                    symbol.end_line,
                    symbol.end_column,
                    symbol.doc_comment.trim(),
                    symbol.content_hash.trim(),
                    symbol.analysis_level.trim(),
                ],
            )?;
        }
        transaction.execute(
            "UPDATE knowledge_code_snapshots
             SET symbol_count = (SELECT COUNT(*) FROM knowledge_code_symbols WHERE snapshot_id = ?1),
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            [input.snapshot_id],
        )?;
        let file = transaction.query_row(
            "SELECT id, snapshot_id, document_version_id, relative_path, language, file_size,
                    content_hash, analysis_level, is_generated, is_test, sensitivity, status,
                    skip_reason, created_at
             FROM knowledge_code_files WHERE id = ?1",
            [file_id],
            map_knowledge_code_file,
        )?;
        transaction.commit()?;
        Ok(file)
    }

    pub fn list_knowledge_code_files(
        &self,
        snapshot_id: i64,
    ) -> Result<Vec<KnowledgeCodeFile>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let files = conn
            .prepare(
                "SELECT id, snapshot_id, document_version_id, relative_path, language, file_size,
                    content_hash, analysis_level, is_generated, is_test, sensitivity, status,
                    skip_reason, created_at
             FROM knowledge_code_files WHERE snapshot_id = ?1 ORDER BY relative_path",
            )?
            .query_map([snapshot_id], map_knowledge_code_file)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    pub fn list_knowledge_code_symbols(
        &self,
        snapshot_id: i64,
        keyword: Option<&str>,
    ) -> Result<Vec<KnowledgeCodeSymbol>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let pattern = keyword.map(|value| format!("%{}%", value.trim()));
        let symbols = conn
            .prepare(
                "SELECT id, snapshot_id, file_id, symbol_key, symbol_kind, name, qualified_name,
                    signature, visibility, parent_symbol_key, start_line, start_column, end_line,
                    end_column, doc_comment, content_hash, analysis_level, created_at
             FROM knowledge_code_symbols
             WHERE snapshot_id = ?1 AND (?2 IS NULL OR name LIKE ?2 OR qualified_name LIKE ?2)
             ORDER BY start_line, symbol_key",
            )?
            .query_map(params![snapshot_id, pattern], map_knowledge_code_symbol)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(symbols)
    }

    /// 覆盖保存相邻快照的结构化变更。调用方只传递路径、内容哈希和脱敏 Diff 元数据；
    /// 旧记录在同一事务中替换，避免失败重试累积过期的重命名或删除结论。
    pub fn replace_knowledge_code_snapshot_changes(
        &self,
        snapshot_id: i64,
        previous_snapshot_id: Option<i64>,
        changes: &[(String, String, String, String, serde_json::Value)],
    ) -> Result<(), AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "DELETE FROM knowledge_code_snapshot_changes WHERE snapshot_id = ?1",
            [snapshot_id],
        )?;
        for (change_type, from_path, to_path, content_hash, evidence) in changes {
            transaction.execute(
                "INSERT INTO knowledge_code_snapshot_changes
                 (snapshot_id, previous_snapshot_id, change_type, from_path, to_path,
                  content_hash, evidence_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    snapshot_id,
                    previous_snapshot_id,
                    change_type,
                    from_path,
                    to_path,
                    content_hash,
                    serde_json::to_string(evidence)?,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// 新快照的片段正文哈希与前序快照一致时复制同 Profile 的已有向量。片段仍属于新的
    /// 文档版本，因此 FTS 和引用位置保持快照隔离；只复用语义空间中确实相同的向量。
    pub fn copy_knowledge_chunk_embeddings_by_content_hash(
        &self,
        target_document_version_id: i64,
        previous_document_version_id: i64,
    ) -> Result<i64, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO knowledge_chunk_embeddings
                 (chunk_id, profile_id, dimension, vector_blob, vector_norm, content_hash, created_at)
             SELECT target.id, previous_embedding.profile_id, previous_embedding.dimension,
                    previous_embedding.vector_blob, previous_embedding.vector_norm,
                    target.content_hash, previous_embedding.created_at
             FROM knowledge_chunks target
             JOIN knowledge_chunks previous
               ON previous.document_version_id = ?2
              AND previous.content_hash = target.content_hash
             JOIN knowledge_chunk_embeddings previous_embedding
               ON previous_embedding.chunk_id = previous.id
             WHERE target.document_version_id = ?1",
            params![target_document_version_id, previous_document_version_id],
        )?;
        Ok(i64::try_from(inserted).unwrap_or(i64::MAX))
    }

    /// 每次快照分析都完整重算已支持的确定性关系；替换在一个事务中完成，避免删除
    /// 文件后仍可从调用图读取到过期边。
    pub fn replace_knowledge_code_relations(
        &self,
        snapshot_id: i64,
        relations: &[KnowledgeCodeRelationWriteInput],
    ) -> Result<(), AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        // 人工确认属于同一不可变快照的稳定边属性；重新分析仍解析到相同边时保留确认，
        // 仅新增、删除或证据行变化的边回到待确认状态。
        let preserved_confirmations = transaction
            .prepare(
                "SELECT from_symbol_key, relation_type, to_symbol_key, to_external_type,
                        to_external_key, evidence_start_line
                 FROM knowledge_code_relations
                 WHERE snapshot_id = ?1 AND confirmed = 1",
            )?
            .query_map([snapshot_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })?
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        transaction.execute(
            "DELETE FROM knowledge_code_relations WHERE snapshot_id = ?1",
            [snapshot_id],
        )?;
        for relation in relations {
            let identity = (
                relation.from_symbol_key.trim().to_string(),
                relation.relation_type.trim().to_string(),
                relation.to_symbol_key.trim().to_string(),
                relation.to_external_type.trim().to_string(),
                relation.to_external_key.trim().to_string(),
                relation.evidence_start_line,
            );
            let confirmed = relation.confirmed || preserved_confirmations.contains(&identity);
            transaction.execute(
                "INSERT OR IGNORE INTO knowledge_code_relations
                 (snapshot_id, from_symbol_key, relation_type, to_symbol_key,
                  to_external_type, to_external_key, evidence_file_id, evidence_start_line,
                  evidence_end_line, evidence_text, resolver, confidence, confirmed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    snapshot_id,
                    relation.from_symbol_key.trim(),
                    relation.relation_type.trim(),
                    relation.to_symbol_key.trim(),
                    relation.to_external_type.trim(),
                    relation.to_external_key.trim(),
                    relation.evidence_file_id,
                    relation.evidence_start_line,
                    relation.evidence_end_line,
                    relation.evidence_text.trim(),
                    relation.resolver.trim(),
                    relation.confidence.clamp(0.0, 1.0),
                    confirmed as i64,
                ],
            )?;
        }
        transaction.execute(
            "UPDATE knowledge_code_snapshots
             SET relation_count = (
                    SELECT COUNT(*) FROM knowledge_code_relations WHERE snapshot_id = ?1
                 ), updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            [snapshot_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_knowledge_code_relations(
        &self,
        snapshot_id: i64,
        symbol_key: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<KnowledgeCodeRelation>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let pattern = symbol_key
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("%{}%", value.trim()));
        let mut statement = conn.prepare(
            "SELECT id, snapshot_id, from_symbol_key, relation_type, to_symbol_key,
                    to_external_type, to_external_key, evidence_file_id, evidence_start_line,
                    evidence_end_line, evidence_text, resolver, confidence, confirmed, created_at
             FROM knowledge_code_relations
             WHERE snapshot_id = ?1
               AND (?2 IS NULL OR from_symbol_key LIKE ?2 OR to_symbol_key LIKE ?2)
             ORDER BY confidence DESC, id
             LIMIT ?3",
        )?;
        let relations = statement
            .query_map(
                params![snapshot_id, pattern, limit.unwrap_or(200).clamp(1, 1_000)],
                map_knowledge_code_relation,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(relations)
    }

    /// 自动分析产出的关系默认未确认；只有人工确认后才允许它参与混合检索的关系通道。
    pub fn confirm_knowledge_code_relation(
        &self,
        id: i64,
        confirmed: bool,
    ) -> Result<KnowledgeCodeRelation, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_code_relations SET confirmed = ?1 WHERE id = ?2",
            params![confirmed as i64, id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("源码关系不存在: {id}")));
        }
        get_knowledge_code_relation(&conn, id)
    }

    /// 仅由完整分析流程在全部文件、符号和关系均成功持久化后调用。进行中的或失败的
    /// 快照保留为非 analyzed 状态，查询层因此不会把部分结果作为证据返回。
    pub fn set_knowledge_code_snapshot_analysis_status(
        &self,
        snapshot_id: i64,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        if !matches!(status, "captured" | "analyzing" | "analyzed" | "failed") {
            return Err(AppError::InvalidInput("无效的源码快照状态".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|value| AppError::Custom(value.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_code_snapshots
             SET status = ?1, error = ?2, updated_at = datetime('now', 'localtime')
             WHERE id = ?3",
            params![status, error, snapshot_id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("源码快照不存在: {snapshot_id}")));
        }
        Ok(())
    }

    pub fn upsert_knowledge_code_source(
        &self,
        input: &UpsertKnowledgeCodeSourceInput,
    ) -> Result<KnowledgeCodeSource, AppError> {
        let source = self.upsert_knowledge_source(&input.source)?;
        let languages_json = serde_json::to_string(&input.allowed_languages)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_code_source_settings
             (source_id, include_untracked, max_file_size_bytes, allowed_languages_json,
              allow_remote_processing)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_id) DO UPDATE SET
                include_untracked = excluded.include_untracked,
                max_file_size_bytes = excluded.max_file_size_bytes,
                allowed_languages_json = excluded.allowed_languages_json,
                allow_remote_processing = excluded.allow_remote_processing,
                updated_at = datetime('now', 'localtime')",
            params![
                source.id,
                input.include_untracked as i64,
                input.max_file_size_bytes,
                languages_json,
                input.allow_remote_processing as i64,
            ],
        )?;
        let settings = get_knowledge_code_source_settings(&conn, source.id)?;
        Ok(KnowledgeCodeSource { source, settings })
    }

    pub fn list_knowledge_code_sources(&self) -> Result<Vec<KnowledgeCodeSource>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT s.id, s.source_key, s.project_id, s.source_type, s.display_name,
                    s.root_path, s.git_workspace_key, s.include_globs_json,
                    s.exclude_globs_json, s.version_strategy, s.sync_mode,
                    s.allow_remote_embedding, s.enabled, s.last_commit_sha,
                    s.last_sync_status, s.last_synced_at, s.last_error, s.created_at,
                    s.updated_at, s.deleted_at
             FROM knowledge_sources s
             JOIN knowledge_code_source_settings c ON c.source_id = s.id
             WHERE s.deleted_at IS NULL
             ORDER BY s.updated_at DESC, s.id DESC",
        )?;
        let sources = statement
            .query_map([], map_knowledge_source)?
            .collect::<Result<Vec<_>, _>>()?;
        sources
            .into_iter()
            .map(|source| {
                Ok(KnowledgeCodeSource {
                    settings: get_knowledge_code_source_settings(&conn, source.id)?,
                    source,
                })
            })
            .collect()
    }

    pub fn upsert_knowledge_project(
        &self,
        input: &UpsertKnowledgeProjectInput,
    ) -> Result<KnowledgeProject, AppError> {
        let aliases_json = serde_json::to_string(&input.aliases)?;
        let git_workspace_keys_json = serde_json::to_string(&input.git_workspace_keys)?;
        let git_workspace_key = input
            .git_workspace_keys
            .first()
            .map(String::as_str)
            .unwrap_or_else(|| input.git_workspace_key.trim());
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let resolved_id = resolve_knowledge_project_upsert_target(&conn, input)?;
        if let Some(id) = resolved_id {
            let changed = conn.execute(
                "UPDATE knowledge_projects SET
                    project_key = ?1, name = ?2, aliases_json = ?3, description = ?4,
                    git_workspace_key = ?5, git_workspace_keys_json = ?6,
                    default_branch = ?7, enabled = ?8,
                    updated_at = datetime('now', 'localtime'), deleted_at = NULL
                 WHERE id = ?9",
                params![
                    input.project_key.trim(),
                    input.name.trim(),
                    aliases_json,
                    input.description.trim(),
                    git_workspace_key,
                    git_workspace_keys_json,
                    input.default_branch.trim(),
                    input.enabled as i64,
                    id
                ],
            )?;
            if changed == 0 {
                return Err(AppError::NotFound(format!("知识项目不存在: {id}")));
            }
            return get_knowledge_project(&conn, id);
        }

        conn.execute(
            "INSERT INTO knowledge_projects
             (project_key, name, aliases_json, description, git_workspace_key,
              git_workspace_keys_json, default_branch, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                input.project_key.trim(),
                input.name.trim(),
                aliases_json,
                input.description.trim(),
                git_workspace_key,
                git_workspace_keys_json,
                input.default_branch.trim(),
                input.enabled as i64,
            ],
        )?;
        let id = conn.last_insert_rowid();
        get_knowledge_project(&conn, id)
    }

    /// 稳定项目标识用于判定客户端重试是否命中同一项目，包含已删除记录以避免复用旧身份。
    pub fn get_knowledge_project_by_key(
        &self,
        project_key: &str,
    ) -> Result<Option<KnowledgeProject>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, project_key, name, aliases_json, description, git_workspace_key,
                    git_workspace_keys_json, default_branch, enabled, created_at, updated_at, deleted_at
             FROM knowledge_projects WHERE project_key = ?1",
            [project_key.trim()],
            map_knowledge_project,
        )
        .optional()
        .map_err(AppError::from)
    }

    /// 供 Service 在校验 ID 与稳定项目标识是否同属一个实体时使用；不对软删除过滤，
    /// 因为同一项目的重试允许恢复该记录，不能被误判成一个新项目。
    pub fn get_knowledge_project_by_id(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeProject>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, project_key, name, aliases_json, description, git_workspace_key,
                    git_workspace_keys_json, default_branch, enabled, created_at, updated_at, deleted_at
             FROM knowledge_projects WHERE id = ?1",
            [id],
            map_knowledge_project,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn list_knowledge_projects(
        &self,
        input: &KnowledgeListInput,
    ) -> Result<KnowledgePage<KnowledgeProject>, AppError> {
        let (offset, limit) = normalized_page(input.offset, input.limit);
        let keyword = normalized_keyword(input.keyword.as_deref());
        let status = input.status.as_deref();
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let total = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_projects
             WHERE deleted_at IS NULL
               AND (?1 IS NULL OR name LIKE ?1 OR project_key LIKE ?1 OR aliases_json LIKE ?1)
               AND (?2 IS NULL OR
                    (?2 = 'enabled' AND enabled = 1) OR
                    (?2 = 'disabled' AND enabled = 0))
               AND (?3 IS NULL OR id = ?3)",
            params![keyword, status, input.project_id],
            |row| row.get(0),
        )?;
        let items = conn
            .prepare(
                "SELECT id, project_key, name, aliases_json, description, git_workspace_key,
                        git_workspace_keys_json, default_branch, enabled, created_at, updated_at, deleted_at
                 FROM knowledge_projects
                 WHERE deleted_at IS NULL
                   AND (?1 IS NULL OR name LIKE ?1 OR project_key LIKE ?1 OR aliases_json LIKE ?1)
                   AND (?2 IS NULL OR
                        (?2 = 'enabled' AND enabled = 1) OR
                        (?2 = 'disabled' AND enabled = 0))
                   AND (?3 IS NULL OR id = ?3)
                 ORDER BY updated_at DESC, id DESC
                 LIMIT ?4 OFFSET ?5",
            )?
            .query_map(
                params![keyword, status, input.project_id, limit, offset],
                map_knowledge_project,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(KnowledgePage {
            items,
            total,
            offset,
            limit,
        })
    }

    pub fn soft_delete_knowledge_project(&self, id: i64) -> Result<(), AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let changed = transaction.execute(
            "UPDATE knowledge_projects SET deleted_at = datetime('now', 'localtime'),
                enabled = 0, updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("知识记录不存在: {id}")));
        }
        // 保留映射的审计历史，同时禁止其继续同步到已删除项目。
        transaction.execute(
            "UPDATE zentao_project_mappings SET enabled = 0,
                updated_at = datetime('now', 'localtime')
             WHERE knowledge_project_id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn upsert_knowledge_release(
        &self,
        input: &UpsertKnowledgeReleaseInput,
    ) -> Result<KnowledgeRelease, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        if let Some(id) = input.id {
            if knowledge_release_has_repository_manifest(&conn, id)? {
                return Err(AppError::InvalidInput(
                    "已冻结项目版本不能通过旧发布接口修改，请创建新的版本清单".to_string(),
                ));
            }
            let changed = conn.execute(
                "UPDATE knowledge_releases SET
                    project_id = ?1, version = ?2, tag_name = ?3, branch = ?4,
                    commit_sha = ?5, description = ?6, released_at = ?7,
                    updated_at = datetime('now', 'localtime'), deleted_at = NULL
                 WHERE id = ?8",
                params![
                    input.project_id,
                    input.version.trim(),
                    input.tag_name.trim(),
                    input.branch.trim(),
                    input.commit_sha.trim(),
                    input.description.trim(),
                    input.released_at,
                    id
                ],
            )?;
            if changed == 0 {
                return Err(AppError::NotFound(format!("知识版本不存在: {id}")));
            }
            return get_knowledge_release(&conn, id);
        }
        let existing_id = conn
            .query_row(
                "SELECT id FROM knowledge_releases WHERE project_id = ?1 AND version = ?2",
                params![input.project_id, input.version.trim()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(existing_id) = existing_id {
            if knowledge_release_has_repository_manifest(&conn, existing_id)? {
                return Err(AppError::InvalidInput(
                    "已冻结项目版本不能通过旧发布接口修改，请创建新的版本清单".to_string(),
                ));
            }
        }
        conn.execute(
            "INSERT INTO knowledge_releases
             (project_id, version, tag_name, branch, commit_sha, description, released_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(project_id, version) DO UPDATE SET
                tag_name = excluded.tag_name,
                branch = excluded.branch,
                commit_sha = excluded.commit_sha,
                description = excluded.description,
                released_at = excluded.released_at,
                updated_at = datetime('now', 'localtime'),
                deleted_at = NULL",
            params![
                input.project_id,
                input.version.trim(),
                input.tag_name.trim(),
                input.branch.trim(),
                input.commit_sha.trim(),
                input.description.trim(),
                input.released_at
            ],
        )?;
        let id = conn.query_row(
            "SELECT id FROM knowledge_releases WHERE project_id = ?1 AND version = ?2",
            params![input.project_id, input.version.trim()],
            |row| row.get(0),
        )?;
        get_knowledge_release(&conn, id)
    }

    pub fn list_knowledge_releases(
        &self,
        project_id: i64,
    ) -> Result<Vec<KnowledgeRelease>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let items = conn
            .prepare(
                "SELECT id, project_id, version, tag_name, branch, commit_sha, description,
                        released_at, created_at, updated_at, deleted_at
                 FROM knowledge_releases
                 WHERE project_id = ?1 AND deleted_at IS NULL
                 ORDER BY released_at DESC, version DESC",
            )?
            .query_map([project_id], map_knowledge_release)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    /// 仅在已确定的知识项目（及可选版本）内按类型和外部编号查找禅道实体。Commit
    /// trailer 的 `Task-42` 这类标识不能跨项目裸匹配，否则会把同号需求错误串联。
    pub fn find_zentao_entities_by_scope_and_external_id(
        &self,
        project_id: i64,
        release_id: Option<i64>,
        entity_types: &[&str],
        external_id: &str,
    ) -> Result<Vec<ZentaoEntity>, AppError> {
        if project_id <= 0 || entity_types.is_empty() || external_id.trim().is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(entity_types.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT id, connection_id, mapping_id, knowledge_project_id, release_id, entity_type,
                    external_id, external_key, title, body_markdown, original_status, normalized_status,
                    assignee_external_id, parent_external_key, remote_url, content_hash, raw_json_hash,
                    raw_snapshot_json, source_created_at, source_updated_at, first_synced_at, last_synced_at,
                    missing_count, status, deleted_at
             FROM zentao_entities
             WHERE knowledge_project_id = ?1 AND (?2 IS NULL OR release_id = ?2)
               AND external_id = ?3 AND entity_type IN ({placeholders})
               AND status = 'active' AND deleted_at IS NULL
             ORDER BY id",
        );
        let mut values = vec![
            Value::Integer(project_id),
            release_id.map(Value::Integer).unwrap_or(Value::Null),
            Value::Text(external_id.trim().to_string()),
        ];
        values.extend(
            entity_types
                .iter()
                .map(|item| Value::Text((*item).to_string())),
        );
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let entities = conn
            .prepare(&sql)?
            .query_map(params_from_iter(values.iter()), map_zentao_entity)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(entities)
    }

    pub fn get_knowledge_release_by_id(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeRelease>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, project_id, version, tag_name, branch, commit_sha, description,
                    released_at, created_at, updated_at, deleted_at
             FROM knowledge_releases
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            map_knowledge_release,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn soft_delete_knowledge_release(&self, id: i64) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        if knowledge_release_has_repository_manifest(&conn, id)? {
            return Err(AppError::InvalidInput(
                "已冻结项目版本不能删除，请创建新的版本替代历史清单".to_string(),
            ));
        }
        let changed = conn.execute(
            "UPDATE knowledge_releases
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("知识记录不存在: {id}")));
        }
        Ok(())
    }

    pub fn upsert_knowledge_source(
        &self,
        input: &UpsertKnowledgeSourceInput,
    ) -> Result<KnowledgeSource, AppError> {
        let include_globs_json = serde_json::to_string(&input.include_globs)?;
        let exclude_globs_json = serde_json::to_string(&input.exclude_globs)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let resolved_id = resolve_knowledge_source_upsert_target(&conn, input)?;
        if let Some(id) = resolved_id {
            let changed = conn.execute(
                "UPDATE knowledge_sources SET
                    source_key = ?1, project_id = ?2, source_type = ?3, display_name = ?4,
                    root_path = ?5, git_workspace_key = ?6, include_globs_json = ?7,
                    exclude_globs_json = ?8, version_strategy = ?9, sync_mode = ?10,
                    allow_remote_embedding = ?11, enabled = ?12,
                    updated_at = datetime('now', 'localtime'), deleted_at = NULL
                 WHERE id = ?13",
                params![
                    input.source_key.trim(),
                    input.project_id,
                    input.source_type.trim(),
                    input.display_name.trim(),
                    input.root_path.trim(),
                    input.git_workspace_key.trim(),
                    include_globs_json,
                    exclude_globs_json,
                    input.version_strategy.trim(),
                    input.sync_mode.trim(),
                    input.allow_remote_embedding as i64,
                    input.enabled as i64,
                    id
                ],
            )?;
            if changed == 0 {
                return Err(AppError::NotFound(format!("知识源不存在: {id}")));
            }
            return get_knowledge_source(&conn, id);
        }
        conn.execute(
            "INSERT INTO knowledge_sources
             (source_key, project_id, source_type, display_name, root_path, git_workspace_key,
              include_globs_json, exclude_globs_json, version_strategy, sync_mode,
              allow_remote_embedding, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                input.source_key.trim(),
                input.project_id,
                input.source_type.trim(),
                input.display_name.trim(),
                input.root_path.trim(),
                input.git_workspace_key.trim(),
                include_globs_json,
                exclude_globs_json,
                input.version_strategy.trim(),
                input.sync_mode.trim(),
                input.allow_remote_embedding as i64,
                input.enabled as i64
            ],
        )?;
        let id = conn.last_insert_rowid();
        get_knowledge_source(&conn, id)
    }

    /// 多个来源必须作为一个整体写入：派生标识与已有来源冲突时不能悄悄覆盖，任一失败
    /// 都会回滚本批已写入的记录，避免留下用户误以为保存失败的半成品来源。
    pub fn upsert_knowledge_sources_atomically(
        &self,
        inputs: &[UpsertKnowledgeSourceInput],
    ) -> Result<Vec<KnowledgeSource>, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let mut sources = Vec::with_capacity(inputs.len());

        for input in inputs {
            let include_globs_json = serde_json::to_string(&input.include_globs)?;
            let exclude_globs_json = serde_json::to_string(&input.exclude_globs)?;
            let resolved_id = resolve_knowledge_source_upsert_target(&transaction, input)?;
            let id = if let Some(id) = resolved_id {
                let changed = transaction.execute(
                    "UPDATE knowledge_sources SET
                        source_key = ?1, project_id = ?2, source_type = ?3, display_name = ?4,
                        root_path = ?5, git_workspace_key = ?6, include_globs_json = ?7,
                        exclude_globs_json = ?8, version_strategy = ?9, sync_mode = ?10,
                        allow_remote_embedding = ?11, enabled = ?12,
                        updated_at = datetime('now', 'localtime'), deleted_at = NULL
                     WHERE id = ?13",
                    params![
                        input.source_key.trim(),
                        input.project_id,
                        input.source_type.trim(),
                        input.display_name.trim(),
                        input.root_path.trim(),
                        input.git_workspace_key.trim(),
                        include_globs_json,
                        exclude_globs_json,
                        input.version_strategy.trim(),
                        input.sync_mode.trim(),
                        input.allow_remote_embedding as i64,
                        input.enabled as i64,
                        id
                    ],
                )?;
                if changed == 0 {
                    return Err(AppError::NotFound(format!("知识源不存在: {id}")));
                }
                id
            } else {
                transaction.execute(
                    "INSERT INTO knowledge_sources
                     (source_key, project_id, source_type, display_name, root_path, git_workspace_key,
                      include_globs_json, exclude_globs_json, version_strategy, sync_mode,
                      allow_remote_embedding, enabled)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        input.source_key.trim(), input.project_id, input.source_type.trim(),
                        input.display_name.trim(), input.root_path.trim(), input.git_workspace_key.trim(),
                        include_globs_json, exclude_globs_json, input.version_strategy.trim(),
                        input.sync_mode.trim(), input.allow_remote_embedding as i64, input.enabled as i64
                    ],
                )?;
                transaction.last_insert_rowid()
            };
            sources.push(get_knowledge_source(&transaction, id)?);
        }
        transaction.commit()?;
        Ok(sources)
    }

    pub fn list_knowledge_sources(
        &self,
        project_id: Option<i64>,
    ) -> Result<Vec<KnowledgeSource>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let items = conn
            .prepare(
                "SELECT id, source_key, project_id, source_type, display_name, root_path,
                        git_workspace_key, include_globs_json, exclude_globs_json,
                        version_strategy, sync_mode, allow_remote_embedding, enabled,
                        last_commit_sha, last_sync_status, last_synced_at, last_error,
                        created_at, updated_at, deleted_at
                 FROM knowledge_sources
                 WHERE deleted_at IS NULL AND (?1 IS NULL OR project_id = ?1)
                 ORDER BY display_name, id",
            )?
            .query_map([project_id], map_knowledge_source)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn get_knowledge_source_by_id(&self, id: i64) -> Result<Option<KnowledgeSource>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, source_key, project_id, source_type, display_name, root_path,
                    git_workspace_key, include_globs_json, exclude_globs_json,
                    version_strategy, sync_mode, allow_remote_embedding, enabled,
                    last_commit_sha, last_sync_status, last_synced_at, last_error,
                    created_at, updated_at, deleted_at
             FROM knowledge_sources
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            map_knowledge_source,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn update_knowledge_source_sync_state(
        &self,
        id: i64,
        commit_sha: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|lock_error| AppError::Custom(lock_error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_sources SET
                last_commit_sha = ?1,
                last_sync_status = ?2,
                last_error = ?3,
                last_synced_at = CASE WHEN ?2 = 'success'
                    THEN datetime('now', 'localtime') ELSE last_synced_at END,
                updated_at = datetime('now', 'localtime')
             WHERE id = ?4 AND deleted_at IS NULL",
            params![commit_sha, status, error, id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("知识源不存在: {id}")));
        }
        Ok(())
    }

    pub fn mark_knowledge_document_path_deleted(
        &self,
        source_id: i64,
        logical_path: &str,
    ) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_documents SET
                status = 'deleted',
                updated_at = datetime('now', 'localtime')
             WHERE source_id = ?1 AND logical_path = ?2
               AND deleted_at IS NULL AND status != 'deleted'",
            params![source_id, logical_path],
        )?;
        Ok(changed > 0)
    }

    /// 将已入库文档提升为 restricted 并擦除可读正文。该操作用于同步时新内容命中
    /// 秘密规则的场景：保留文档标识和内容哈希以支持审计/去重，但不保留片段、FTS、
    /// 向量或版本正文，避免旧索引在任意输出通道中复活。
    pub fn restrict_knowledge_document(&self, document_id: i64) -> Result<(), AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        transaction
            .execute(
                "DELETE FROM knowledge_chunks_fts
                 WHERE CAST(chunk_id AS INTEGER) IN (
                    SELECT id FROM knowledge_chunks
                    WHERE document_version_id IN (
                        SELECT id FROM knowledge_document_versions WHERE document_id = ?1
                    )
                 )",
                [document_id],
            )
            .or_else(|error| match error {
                rusqlite::Error::SqliteFailure(_, Some(message))
                    if message.contains("no such table") =>
                {
                    Ok(0)
                }
                error => Err(error),
            })?;
        transaction.execute(
            "DELETE FROM knowledge_chunk_embeddings
             WHERE chunk_id IN (
                SELECT id FROM knowledge_chunks
                WHERE document_version_id IN (
                    SELECT id FROM knowledge_document_versions WHERE document_id = ?1
                )
             )",
            [document_id],
        )?;
        transaction.execute(
            "DELETE FROM knowledge_chunks
             WHERE document_version_id IN (
                SELECT id FROM knowledge_document_versions WHERE document_id = ?1
             )",
            [document_id],
        )?;
        transaction.execute(
            "UPDATE knowledge_document_versions SET valid = 0, content = '' WHERE document_id = ?1",
            [document_id],
        )?;
        transaction.execute(
            "UPDATE knowledge_documents
             SET sensitivity = 'restricted', allow_ai = 0, allow_mcp = 0,
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            [document_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get_knowledge_document_by_source_path(
        &self,
        source_id: i64,
        logical_path: &str,
    ) -> Result<Option<KnowledgeDocument>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT d.id, d.document_key, d.project_id, d.source_id, d.doc_type, d.title, d.logical_path,
                    d.status, d.sensitivity, d.tags_json, d.latest_version_id, d.allow_ai, d.allow_mcp,
                    d.created_at, d.updated_at, d.deleted_at,
                    (SELECT upload.source_folder_name
                     FROM knowledge_document_uploads upload
                     WHERE upload.document_id = d.id
                     ORDER BY upload.id DESC LIMIT 1)
             FROM knowledge_documents d
             WHERE d.source_id = ?1 AND d.logical_path = ?2 AND d.deleted_at IS NULL",
            params![source_id, logical_path],
            map_knowledge_document,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn get_knowledge_document_by_id(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeDocument>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT d.id, d.document_key, d.project_id, d.source_id, d.doc_type, d.title, d.logical_path,
                    d.status, d.sensitivity, d.tags_json, d.latest_version_id, d.allow_ai, d.allow_mcp,
                    d.created_at, d.updated_at, d.deleted_at,
                    (SELECT upload.source_folder_name
                     FROM knowledge_document_uploads upload
                     WHERE upload.document_id = d.id
                     ORDER BY upload.id DESC LIMIT 1)
             FROM knowledge_documents d WHERE d.id = ?1 AND d.deleted_at IS NULL",
            [id],
            map_knowledge_document,
        )
        .optional()
        .map_err(AppError::from)
    }

    /// 恢复流程需要读取已软删除文档的最小元数据并做策略校验；普通查询仍必须使用
    /// `get_knowledge_document_by_id`，避免删除文档重新出现在目录、检索或问答中。
    pub(crate) fn get_knowledge_document_including_deleted_by_id(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeDocument>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT d.id, d.document_key, d.project_id, d.source_id, d.doc_type, d.title, d.logical_path,
                    d.status, d.sensitivity, d.tags_json, d.latest_version_id, d.allow_ai, d.allow_mcp,
                    d.created_at, d.updated_at, d.deleted_at,
                    (SELECT upload.source_folder_name
                     FROM knowledge_document_uploads upload
                     WHERE upload.document_id = d.id
                     ORDER BY upload.id DESC LIMIT 1)
             FROM knowledge_documents d WHERE d.id = ?1",
            [id],
            map_knowledge_document,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn get_knowledge_document_by_key(
        &self,
        document_key: &str,
    ) -> Result<Option<KnowledgeDocument>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT d.id, d.document_key, d.project_id, d.source_id, d.doc_type, d.title, d.logical_path,
                    d.status, d.sensitivity, d.tags_json, d.latest_version_id, d.allow_ai, d.allow_mcp,
                    d.created_at, d.updated_at, d.deleted_at,
                    (SELECT upload.source_folder_name
                     FROM knowledge_document_uploads upload
                     WHERE upload.document_id = d.id
                     ORDER BY upload.id DESC LIMIT 1)
             FROM knowledge_documents d WHERE d.document_key = ?1 AND d.deleted_at IS NULL",
            [document_key.trim()],
            map_knowledge_document,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn save_knowledge_retrieval_evaluation_run(
        &self,
        fixture_version: &str,
        profile_id: Option<i64>,
        top_k: i64,
        case_count: i64,
        recall_at_k: f64,
        mrr: f64,
        citation_accuracy: f64,
        version_leakage_rate: f64,
        refusal_accuracy: f64,
        p50_latency_ms: i64,
        p95_latency_ms: i64,
        details: &serde_json::Value,
    ) -> Result<KnowledgeRetrievalEvaluationRun, AppError> {
        let details_json = serde_json::to_string(details)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_retrieval_evaluation_runs
             (fixture_version, profile_id, top_k, case_count, recall_at_k, mrr,
              citation_accuracy, version_leakage_rate, refusal_accuracy, p50_latency_ms,
              p95_latency_ms, details_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                fixture_version,
                profile_id,
                top_k,
                case_count,
                recall_at_k,
                mrr,
                citation_accuracy,
                version_leakage_rate,
                refusal_accuracy,
                p50_latency_ms,
                p95_latency_ms,
                details_json,
            ],
        )?;
        get_knowledge_retrieval_evaluation_run(&conn, conn.last_insert_rowid())
    }

    pub fn list_knowledge_retrieval_evaluation_runs(
        &self,
        limit: i64,
    ) -> Result<Vec<KnowledgeRetrievalEvaluationRun>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, fixture_version, profile_id, top_k, case_count, recall_at_k, mrr,
                    citation_accuracy, version_leakage_rate, refusal_accuracy, p50_latency_ms,
                    p95_latency_ms, details_json, created_at
             FROM knowledge_retrieval_evaluation_runs
             ORDER BY created_at DESC, id DESC LIMIT ?1",
        )?;
        let runs = statement
            .query_map(
                [limit.clamp(1, 100)],
                map_knowledge_retrieval_evaluation_run,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(runs)
    }

    pub fn rename_knowledge_document_path(
        &self,
        source_id: i64,
        old_path: &str,
        new_path: &str,
    ) -> Result<bool, AppError> {
        let title = std::path::Path::new(new_path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| new_path.to_string());
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_documents SET
                logical_path = ?1,
                title = ?2,
                status = 'active',
                updated_at = datetime('now', 'localtime')
             WHERE source_id = ?3 AND logical_path = ?4 AND deleted_at IS NULL",
            params![new_path, title, source_id, old_path],
        )?;
        if changed > 0 {
            let document_ids = conn
                .prepare(
                    "SELECT id FROM knowledge_documents
                     WHERE source_id = ?1 AND logical_path = ?2 AND deleted_at IS NULL",
                )?
                .query_map(params![source_id, new_path], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for document_id in document_ids {
                sync_knowledge_document_title_index(&conn, document_id)?;
            }
        }
        Ok(changed > 0)
    }

    pub fn list_knowledge_document_sync_states(
        &self,
        source_id: i64,
    ) -> Result<Vec<KnowledgeDocumentSyncState>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let items = conn
            .prepare(
                "SELECT d.id, d.document_key, d.logical_path,
                        COALESCE(v.content_hash, ''), d.status
                 FROM knowledge_documents d
                 LEFT JOIN knowledge_document_versions v ON v.id = d.latest_version_id
                 WHERE d.source_id = ?1 AND d.deleted_at IS NULL
                 ORDER BY d.id",
            )?
            .query_map([source_id], |row| {
                Ok(KnowledgeDocumentSyncState {
                    id: row.get(0)?,
                    document_key: row.get(1)?,
                    logical_path: row.get(2)?,
                    content_hash: row.get(3)?,
                    status: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn soft_delete_knowledge_source(&self, id: i64) -> Result<(), AppError> {
        self.soft_delete_knowledge_record("knowledge_sources", id)
    }

    pub fn upsert_knowledge_document(
        &self,
        input: &UpsertKnowledgeDocumentInput,
    ) -> Result<KnowledgeDocument, AppError> {
        let tags_json = serde_json::to_string(&input.tags)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        if let Some(id) = input.id {
            let changed = conn.execute(
                "UPDATE knowledge_documents SET
                    document_key = ?1, project_id = ?2, source_id = ?3, doc_type = ?4,
                    title = ?5, logical_path = ?6, sensitivity = ?7, tags_json = ?8,
                    allow_ai = ?9, allow_mcp = ?10, status = 'active',
                    updated_at = datetime('now', 'localtime'), deleted_at = NULL
                 WHERE id = ?11",
                params![
                    input.document_key.trim(),
                    input.project_id,
                    input.source_id,
                    input.doc_type.trim(),
                    input.title.trim(),
                    input.logical_path.trim(),
                    input.sensitivity.trim(),
                    tags_json,
                    input.allow_ai as i64,
                    input.allow_mcp as i64,
                    id
                ],
            )?;
            if changed == 0 {
                return Err(AppError::NotFound(format!("知识文档不存在: {id}")));
            }
            sync_knowledge_document_title_index(&conn, id)?;
            return get_knowledge_document(&conn, id);
        }
        conn.execute(
            "INSERT INTO knowledge_documents
             (document_key, project_id, source_id, doc_type, title, logical_path,
              sensitivity, tags_json, allow_ai, allow_mcp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(document_key) DO UPDATE SET
                project_id = excluded.project_id,
                source_id = excluded.source_id,
                doc_type = excluded.doc_type,
                title = excluded.title,
                logical_path = excluded.logical_path,
                sensitivity = excluded.sensitivity,
                tags_json = excluded.tags_json,
                allow_ai = excluded.allow_ai,
                allow_mcp = excluded.allow_mcp,
                status = 'active',
                updated_at = datetime('now', 'localtime'),
                deleted_at = NULL",
            params![
                input.document_key.trim(),
                input.project_id,
                input.source_id,
                input.doc_type.trim(),
                input.title.trim(),
                input.logical_path.trim(),
                input.sensitivity.trim(),
                tags_json,
                input.allow_ai as i64,
                input.allow_mcp as i64
            ],
        )?;
        let id = conn.query_row(
            "SELECT id FROM knowledge_documents WHERE document_key = ?1",
            [input.document_key.trim()],
            |row| row.get(0),
        )?;
        sync_knowledge_document_title_index(&conn, id)?;
        get_knowledge_document(&conn, id)
    }

    pub fn list_knowledge_documents(
        &self,
        input: &KnowledgeListInput,
    ) -> Result<KnowledgePage<KnowledgeDocument>, AppError> {
        self.list_knowledge_documents_with_visibility(input, true, false)
    }

    /// 面向普通目录、浏览器开发 API 与桌面 Command 的列表不暴露 restricted 文档；
    /// 后端迁移、清理和审计仍可显式调用完整内部查询，避免用 UI 规则破坏保留义务。
    pub(crate) fn list_visible_knowledge_documents(
        &self,
        input: &KnowledgeListInput,
    ) -> Result<KnowledgePage<KnowledgeDocument>, AppError> {
        self.list_knowledge_documents_with_visibility(input, false, false)
    }

    /// 回收站只返回已经软删除且不受限的文档元数据；正文与资产仍通过原有详情授权链路保护。
    pub(crate) fn list_deleted_visible_knowledge_documents(
        &self,
        input: &KnowledgeListInput,
    ) -> Result<KnowledgePage<KnowledgeDocument>, AppError> {
        self.list_knowledge_documents_with_visibility(input, false, true)
    }

    fn list_knowledge_documents_with_visibility(
        &self,
        input: &KnowledgeListInput,
        include_restricted: bool,
        deleted_only: bool,
    ) -> Result<KnowledgePage<KnowledgeDocument>, AppError> {
        let (offset, limit) = normalized_page(input.offset, input.limit);
        let keyword = normalized_keyword(input.keyword.as_deref());
        let deletion_predicate = if deleted_only {
            "d.deleted_at IS NOT NULL"
        } else {
            "d.deleted_at IS NULL"
        };
        let release_visibility_predicate =
            release_scope_visibility_predicate("d", "v", "requested_release.id = ?2");
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let total_sql = format!(
            "SELECT COUNT(*) FROM knowledge_documents d
             WHERE {deletion_predicate}
               AND (?1 IS NULL OR d.project_id = ?1)
               AND (?2 IS NULL OR (
                    EXISTS (
                        SELECT 1 FROM knowledge_document_versions v
                        WHERE v.document_id = d.id
                          AND {release_visibility_predicate}
                    )
               ))
               AND (?3 IS NULL OR d.source_id = ?3)
               AND (?4 IS NULL OR d.status = ?4)
               AND (?5 IS NULL OR d.title LIKE ?5 OR d.logical_path LIKE ?5 OR d.tags_json LIKE ?5)
               AND (?6 = 1 OR d.sensitivity != 'restricted')"
        );
        let total = conn.query_row(
            &total_sql,
            params![
                input.project_id,
                input.release_id,
                input.source_id,
                input.status,
                keyword,
                include_restricted as i64,
            ],
            |row| row.get(0),
        )?;
        let items_sql = format!(
            "SELECT d.id, d.document_key, d.project_id, d.source_id, d.doc_type, d.title, d.logical_path,
                        d.status, d.sensitivity, d.tags_json, d.latest_version_id, d.allow_ai, d.allow_mcp,
                        d.created_at, d.updated_at, d.deleted_at,
                        (SELECT upload.source_folder_name
                         FROM knowledge_document_uploads upload
                         WHERE upload.document_id = d.id
                         ORDER BY upload.id DESC LIMIT 1)
                 FROM knowledge_documents d
                 WHERE {deletion_predicate}
                   AND (?1 IS NULL OR d.project_id = ?1)
                   AND (?2 IS NULL OR (
                        EXISTS (
                            SELECT 1 FROM knowledge_document_versions v
                            WHERE v.document_id = d.id
                              AND {release_visibility_predicate}
                        )
                   ))
                   AND (?3 IS NULL OR d.source_id = ?3)
                   AND (?4 IS NULL OR d.status = ?4)
                   AND (?5 IS NULL OR d.title LIKE ?5 OR d.logical_path LIKE ?5 OR d.tags_json LIKE ?5)
                   AND (?6 = 1 OR d.sensitivity != 'restricted')
                 ORDER BY d.updated_at DESC, d.id DESC
                 LIMIT ?7 OFFSET ?8"
        );
        let items = conn
            .prepare(&items_sql)?
            .query_map(
                params![
                    input.project_id,
                    input.release_id,
                    input.source_id,
                    input.status,
                    keyword,
                    include_restricted as i64,
                    limit,
                    offset
                ],
                map_knowledge_document,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(KnowledgePage {
            items,
            total,
            offset,
            limit,
        })
    }

    /// 汇总上传/索引任务与最近解析产物。详情只返回分类后的安全失败原因，不返回任务
    /// 检查点、任意底层错误、错误堆栈或资产路径，避免形成绕过正文/路径权限的侧通道。
    pub(crate) fn get_knowledge_document_processing_summary(
        &self,
        document: &KnowledgeDocument,
    ) -> Result<KnowledgeDocumentProcessingSummary, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let task_with_error = conn
            .query_row(
                "SELECT task_id, job_key, job_type, status, progress_current, progress_total,
                        message, error, cancel_requested
                 FROM (
                    SELECT job.id AS task_id, job.job_key, job.job_type, job.status,
                           job.progress_current, job.progress_total, job.message,
                           job.error, job.cancel_requested
                    FROM knowledge_document_uploads upload
                    JOIN knowledge_jobs job ON job.id = upload.import_job_id
                    WHERE upload.document_id = ?1
                    UNION ALL
                    SELECT job.id AS task_id, job.job_key, job.job_type, job.status,
                           job.progress_current, job.progress_total, job.message,
                           job.error, job.cancel_requested
                    FROM knowledge_document_versions version
                    JOIN knowledge_jobs job ON job.id = version.index_job_id
                    WHERE version.document_id = ?1
                 )
                 ORDER BY task_id DESC LIMIT 1",
                [document.id],
                |row| {
                    Ok((
                        KnowledgeDocumentProcessingTaskSummary {
                            id: row.get(0)?,
                            job_key: row.get(1)?,
                            job_type: row.get(2)?,
                            status: row.get(3)?,
                            progress_current: row.get(4)?,
                            progress_total: row.get(5)?,
                            message: row.get(6)?,
                            cancel_requested: row.get::<_, i64>(8)? != 0,
                        },
                        row.get::<_, Option<String>>(7)?,
                    ))
                },
            )
            .optional()?;
        let task = task_with_error.as_ref().map(|(summary, _)| summary);
        let parser = conn
            .query_row(
                "SELECT artifact.parser_id, artifact.parser_version, artifact.quality_level,
                        artifact.warning_json
                 FROM knowledge_document_parse_artifacts artifact
                 JOIN knowledge_document_versions version
                   ON version.id = artifact.document_version_id
                 WHERE version.document_id = ?1
                 ORDER BY artifact.id DESC LIMIT 1",
                [document.id],
                |row| {
                    let warnings =
                        serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default();
                    Ok(KnowledgeDocumentParseSummary {
                        parser_id: row.get(0)?,
                        parser_version: row.get(1)?,
                        quality_level: row.get(2)?,
                        warnings,
                    })
                },
            )
            .optional()?;

        let status = processing_status(document, task, parser.as_ref());
        let content_available = document.status == "active"
            && !matches!(
                status.as_str(),
                "processing" | "failed" | "cancelled" | "interrupted"
            );
        let message = processing_message(&status, task, parser.as_ref());
        let failure_reason = processing_failure_reason(
            &status,
            task_with_error
                .as_ref()
                .and_then(|(_, error)| error.as_deref()),
        );
        Ok(KnowledgeDocumentProcessingSummary {
            available_actions: processing_actions(&status, content_available, task),
            status,
            message,
            failure_reason,
            content_available,
            task: task_with_error.map(|(summary, _)| summary),
            parser,
        })
    }

    pub fn soft_delete_knowledge_document(&self, id: i64) -> Result<(), AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let changed = transaction.execute(
            "UPDATE knowledge_documents
             SET deleted_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("知识文档不存在: {id}")));
        }
        let fts_exists = transaction
            .query_row(
                "SELECT 1 FROM sqlite_master
                 WHERE type = 'table' AND name = 'knowledge_chunks_fts'",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if fts_exists {
            transaction.execute(
                "DELETE FROM knowledge_chunks_fts
                 WHERE CAST(chunk_id AS INTEGER) IN (
                    SELECT c.id
                    FROM knowledge_chunks c
                    JOIN knowledge_document_versions v ON v.id = c.document_version_id
                    WHERE v.document_id = ?1
                 )",
                [id],
            )?;
        }
        transaction.execute(
            "DELETE FROM knowledge_document_title_index WHERE document_id = ?1",
            [id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// 预览只统计本地派生数据与历史事实，不返回正文、资产路径或永久删除能力。
    pub(crate) fn preview_knowledge_document_deletion(
        &self,
        document_id: i64,
    ) -> Result<KnowledgeDocumentDeletionImpactPreview, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let title = conn
            .query_row(
                "SELECT title FROM knowledge_documents WHERE id = ?1",
                [document_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("知识文档不存在: {document_id}")))?;
        let count = |sql: &str| -> Result<i64, AppError> {
            conn.query_row(sql, [document_id], |row| row.get(0))
                .map_err(Into::into)
        };
        let fts_entry_count = if conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'knowledge_chunks_fts'",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            count(
                "SELECT COUNT(*) FROM knowledge_chunks_fts
                 WHERE CAST(chunk_id AS INTEGER) IN (
                    SELECT chunk.id FROM knowledge_chunks chunk
                    JOIN knowledge_document_versions version ON version.id = chunk.document_version_id
                    WHERE version.document_id = ?1
                 )",
            )?
        } else {
            0
        };
        Ok(KnowledgeDocumentDeletionImpactPreview {
            document_id,
            title,
            version_count: count(
                "SELECT COUNT(*) FROM knowledge_document_versions WHERE document_id = ?1",
            )?,
            chunk_count: count(
                "SELECT COUNT(*) FROM knowledge_chunks chunk
                 JOIN knowledge_document_versions version ON version.id = chunk.document_version_id
                 WHERE version.document_id = ?1",
            )?,
            vector_count: count(
                "SELECT COUNT(*) FROM knowledge_chunk_embeddings embedding
                 JOIN knowledge_chunks chunk ON chunk.id = embedding.chunk_id
                 JOIN knowledge_document_versions version ON version.id = chunk.document_version_id
                 WHERE version.document_id = ?1",
            )?,
            relation_count: count(
                "SELECT COUNT(*) FROM knowledge_relations relation
                 WHERE relation.document_version_id IN (
                    SELECT id FROM knowledge_document_versions WHERE document_id = ?1
                 ) AND relation.deleted_at IS NULL",
            )?,
            asset_count: count(
                "SELECT COUNT(*) FROM knowledge_document_uploads WHERE document_id = ?1",
            )?,
            fts_entry_count,
            permanent_deletion_enabled: false,
            permanent_deletion_block_reason: "永久删除尚未启用；历史版本、引用和受控资产将继续保留"
                .to_string(),
        })
    }

    /// 恢复只反转逻辑删除并回填全部有效版本的全文和当前标题索引；不会改写历史版本、
    /// 关系、向量或资产。默认检索仍会按请求范围选择版本，历史版本只在显式版本查询中返回。
    pub(crate) fn restore_knowledge_document(
        &self,
        document_id: i64,
    ) -> Result<crate::models::RestoreKnowledgeDocumentResult, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE knowledge_documents
             SET deleted_at = NULL,
                 status = CASE WHEN status = 'deleted' THEN 'active' ELSE status END,
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NOT NULL",
            [document_id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!(
                "未找到可恢复的知识文档: {document_id}"
            )));
        }
        let before = fts_entry_count(&tx, document_id)?;
        if let Some(current_version_id) = tx.query_row(
            "SELECT latest_version_id FROM knowledge_documents WHERE id = ?1",
            [document_id],
            |row| row.get::<_, Option<i64>>(0),
        )? {
            sync_document_fts_if_available(&tx, document_id, current_version_id)?;
        }
        sync_knowledge_document_title_index(&tx, document_id)?;
        let after = fts_entry_count(&tx, document_id)?;
        tx.commit()?;
        drop(conn);
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        Ok(crate::models::RestoreKnowledgeDocumentResult {
            document: get_knowledge_document(&conn, document_id)?,
            rebuilt_fts_entries: (after - before).max(0),
        })
    }

    pub fn create_knowledge_document_version(
        &self,
        input: &CreateKnowledgeDocumentVersionInput,
        chunks: &[KnowledgeChunkWriteInput],
    ) -> Result<KnowledgeDocumentVersion, AppError> {
        let parsed_meta_json = serde_json::to_string(&input.parsed_meta)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO knowledge_document_versions
             (document_id, release_id, version_label, git_branch, commit_sha, source_path,
              mime_type, content, content_hash, parsed_meta_json, token_estimate)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                input.document_id,
                input.release_id,
                input.version_label,
                input.git_branch,
                input.commit_sha,
                input.source_path,
                input.mime_type,
                input.content,
                input.content_hash,
                parsed_meta_json,
                input.token_estimate
            ],
        )?;
        let version_id = transaction.query_row(
            "SELECT id FROM knowledge_document_versions
             WHERE document_id = ?1 AND version_label = ?2 AND content_hash = ?3
               AND source_path = ?4",
            params![
                input.document_id,
                input.version_label,
                input.content_hash,
                input.source_path
            ],
            |row| row.get::<_, i64>(0),
        )?;
        // 旧来源同步、代码快照与生成链路仍经由这个兼容 DAO 创建版本。只要调用方给出
        // 不可变项目版本，就在同一事务中补写范围绑定；未选择版本的来源同步则绑定到
        // 明确的来源范围，不能让新旧入口产生“最新版本”的隐式语义。
        if let Some(release_id) = input.release_id {
            transaction.execute(
                "INSERT OR IGNORE INTO knowledge_document_version_bindings
                    (document_version_id, release_id, repository_binding_id, cross_version_scope)
                 VALUES (?1, ?2, NULL, '')",
                params![version_id, release_id],
            )?;
        } else if let Some(source_id) = transaction.query_row(
            "SELECT source_id FROM knowledge_documents WHERE id = ?1",
            [input.document_id],
            |row| row.get::<_, Option<i64>>(0),
        )? {
            transaction.execute(
                "INSERT OR IGNORE INTO knowledge_document_version_bindings
                    (document_version_id, release_id, repository_binding_id, cross_version_scope)
                 VALUES (?1, NULL, NULL, ?2)",
                params![version_id, format!("source:{source_id}")],
            )?;
        }
        let existing_chunks = transaction.query_row(
            "SELECT COUNT(*) FROM knowledge_chunks WHERE document_version_id = ?1",
            [version_id],
            |row| row.get::<_, i64>(0),
        )?;
        if existing_chunks == 0 {
            insert_chunks(&transaction, version_id, chunks)?;
        }
        transaction.execute(
            "UPDATE knowledge_documents
             SET latest_version_id = ?1, updated_at = datetime('now', 'localtime')
             WHERE id = ?2",
            params![version_id, input.document_id],
        )?;
        sync_document_fts_if_available(&transaction, input.document_id, version_id)?;
        sync_knowledge_document_title_index(&transaction, input.document_id)?;
        transaction.commit()?;
        drop(conn);

        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_knowledge_document_version(&conn, version_id)
    }

    pub fn knowledge_document_version_exists(
        &self,
        document_id: i64,
        version_label: &str,
        content_hash: &str,
        source_path: &str,
    ) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        Ok(conn
            .query_row(
                "SELECT 1 FROM knowledge_document_versions
                 WHERE document_id = ?1 AND version_label = ?2 AND content_hash = ?3
                   AND source_path = ?4",
                params![document_id, version_label, content_hash, source_path],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn list_knowledge_document_versions(
        &self,
        document_id: i64,
    ) -> Result<Vec<KnowledgeDocumentVersion>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let items = conn
            .prepare(
                "SELECT id, document_id, release_id, version_label, git_branch, commit_sha,
                        source_path, mime_type, content, content_hash, parsed_meta_json,
                        token_estimate, valid, created_at
                 FROM knowledge_document_versions
                 WHERE document_id = ?1
                 ORDER BY created_at DESC, id DESC",
            )?
            .query_map([document_id], map_knowledge_document_version)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn get_knowledge_document_version_by_id(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeDocumentVersion>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, document_id, release_id, version_label, git_branch, commit_sha,
                    source_path, mime_type, content, content_hash, parsed_meta_json,
                    token_estimate, valid, created_at
             FROM knowledge_document_versions WHERE id = ?1",
            [id],
            map_knowledge_document_version,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn list_knowledge_chunks(
        &self,
        document_version_id: i64,
    ) -> Result<Vec<KnowledgeChunk>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let items = conn
            .prepare(
                "SELECT id, document_version_id, chunk_index, heading_path, content,
                        content_hash, location_json, token_estimate, embedding_status,
                        created_at, updated_at
                 FROM knowledge_chunks
                 WHERE document_version_id = ?1
                 ORDER BY chunk_index",
            )?
            .query_map([document_version_id], map_knowledge_chunk)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn get_knowledge_chunk_by_id(&self, id: i64) -> Result<Option<KnowledgeChunk>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, document_version_id, chunk_index, heading_path, content,
                    content_hash, location_json, token_estimate, embedding_status,
                    created_at, updated_at
             FROM knowledge_chunks WHERE id = ?1",
            [id],
            map_knowledge_chunk,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn replace_knowledge_document_chunks(
        &self,
        document_version_id: i64,
        parsed_meta: &serde_json::Value,
        token_estimate: i64,
        chunks: &[KnowledgeChunkWriteInput],
    ) -> Result<Vec<KnowledgeChunk>, AppError> {
        let parsed_meta_json = serde_json::to_string(parsed_meta)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        replace_knowledge_document_chunks_in_transaction(
            &transaction,
            document_version_id,
            &parsed_meta_json,
            token_estimate,
            chunks,
        )?;
        transaction.commit()?;
        drop(conn);
        self.list_knowledge_chunks(document_version_id)
    }

    /// 用于手动解析和历史回填。分块、全文索引与解析产物必须一次提交，避免完整度
    /// 将“已有分块但没有解析产物”永久显示为未完成。
    pub fn replace_knowledge_document_chunks_with_parse_artifact(
        &self,
        document_version_id: i64,
        parsed_meta: &serde_json::Value,
        token_estimate: i64,
        chunks: &[KnowledgeChunkWriteInput],
        parse_artifact: &NewKnowledgeDocumentParseArtifact,
    ) -> Result<Vec<KnowledgeChunk>, AppError> {
        let parsed_meta_json = serde_json::to_string(parsed_meta)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        replace_knowledge_document_chunks_in_transaction(
            &transaction,
            document_version_id,
            &parsed_meta_json,
            token_estimate,
            chunks,
        )?;
        insert_knowledge_document_parse_artifact_in_transaction(
            &transaction,
            document_version_id,
            parse_artifact,
        )?;
        transaction.commit()?;
        drop(conn);
        self.list_knowledge_chunks(document_version_id)
    }

    /// 文档索引写入与任务完成必须在同一事务中提交：取消请求若先线性化，分块和 FTS
    /// 写入会一起回滚；完成若先线性化，后续取消会被明确拒绝，避免“已入索引却显示取消”。
    pub fn replace_knowledge_document_chunks_and_finish_job(
        &self,
        input: CompleteKnowledgeDocumentIndexJobInput<'_>,
    ) -> Result<KnowledgeJob, AppError> {
        let parsed_meta_json = serde_json::to_string(input.parsed_meta)?;
        let checkpoint_json = serde_json::to_string(input.checkpoint)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        replace_knowledge_document_chunks_in_transaction(
            &transaction,
            input.document_version_id,
            &parsed_meta_json,
            input.token_estimate,
            input.chunks,
        )?;
        insert_knowledge_document_parse_artifact_in_transaction(
            &transaction,
            input.document_version_id,
            input.parse_artifact,
        )?;
        let changed = transaction.execute(
            "UPDATE knowledge_jobs SET
                status = 'completed', message = ?1, error = NULL, checkpoint_json = ?2,
                heartbeat_at = datetime('now', 'localtime'),
                finished_at = datetime('now', 'localtime')
             WHERE id = ?3 AND status = 'running' AND cancel_requested = 0",
            params![input.message, checkpoint_json, input.job_id],
        )?;
        if changed != 1 {
            return Err(AppError::InvalidInput(
                "知识任务已取消或当前状态不允许完成".to_string(),
            ));
        }
        transaction.commit()?;
        get_knowledge_job_by_id(&conn, input.job_id)
    }

    /// “失败”与“取消”在同一事务内仲裁。失败先线性化时，后续取消会被任务终态拒绝；
    /// 取消先线性化时，失败写入不会覆盖它，而是以 cancelled 终态收尾。
    pub fn fail_knowledge_document_index_job_or_cancel(
        &self,
        input: FailKnowledgeDocumentIndexJobInput<'_>,
    ) -> Result<KnowledgeJob, AppError> {
        let failed_checkpoint_json = serde_json::to_string(input.failed_checkpoint)?;
        let cancelled_checkpoint_json = serde_json::to_string(input.cancelled_checkpoint)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let failed = transaction.execute(
            "UPDATE knowledge_jobs SET
                status = 'failed', message = '文档索引失败', error = ?1,
                checkpoint_json = ?2, heartbeat_at = datetime('now', 'localtime'),
                finished_at = datetime('now', 'localtime')
             WHERE id = ?3 AND status IN ('queued', 'running', 'interrupted')
               AND cancel_requested = 0",
            params![input.error, failed_checkpoint_json, input.job_id],
        )?;
        if failed == 0 {
            let cancelled = transaction.execute(
                "UPDATE knowledge_jobs SET
                    status = 'cancelled', message = '文档索引已安全取消', error = NULL,
                    checkpoint_json = ?1, heartbeat_at = datetime('now', 'localtime'),
                    finished_at = datetime('now', 'localtime')
                 WHERE id = ?2 AND status IN ('queued', 'running', 'interrupted')
                   AND cancel_requested = 1",
                params![cancelled_checkpoint_json, input.job_id],
            )?;
            if cancelled != 1 {
                return Err(AppError::InvalidInput(
                    "知识任务当前状态不允许结束".to_string(),
                ));
            }
        }
        transaction.commit()?;
        get_knowledge_job_by_id(&conn, input.job_id)
    }

    pub fn ensure_knowledge_fts(&self) -> Result<KnowledgeFtsCapability, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let existing_sql = conn
            .query_row(
                "SELECT sql FROM sqlite_master
                 WHERE type = 'table' AND name = 'knowledge_chunks_fts'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(sql) = existing_sql {
            // 历史版本问答依赖每个有效版本的 FTS 片段。旧库只写入最新版本时，在首次
            // 检索前原子回填；默认查询仍由版本选择谓词限制在当前版本，不会扩大日常结果。
            if knowledge_fts_needs_history_backfill(&conn)? {
                let transaction = conn.transaction()?;
                transaction.execute("DELETE FROM knowledge_chunks_fts", [])?;
                transaction.execute(
                    "INSERT INTO knowledge_chunks_fts(chunk_id, title, heading_path, content)
                     SELECT CAST(c.id AS TEXT), d.title, c.heading_path, c.content
                     FROM knowledge_chunks c
                     JOIN knowledge_document_versions v ON v.id = c.document_version_id
                     JOIN knowledge_documents d ON d.id = v.document_id
                     WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'",
                    [],
                )?;
                transaction.commit()?;
            }
            let tokenizer = if sql.to_lowercase().contains("trigram") {
                "trigram"
            } else {
                "unicode61"
            };
            return Ok(KnowledgeFtsCapability {
                fts5_available: true,
                trigram_available: probe_fts_tokenizer(&conn, "trigram"),
                active_tokenizer: tokenizer.to_string(),
            });
        }

        let trigram_available = probe_fts_tokenizer(&conn, "trigram");
        let tokenizer = if trigram_available {
            "trigram"
        } else if probe_fts_tokenizer(&conn, "unicode61") {
            "unicode61"
        } else {
            return Err(AppError::Custom(
                "当前 SQLite 运行时不支持 FTS5，无法创建知识索引".to_string(),
            ));
        };
        let transaction = conn.transaction()?;
        transaction.execute_batch(&format!(
            "CREATE VIRTUAL TABLE knowledge_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                title,
                heading_path,
                content,
                tokenize = '{tokenizer}'
            );"
        ))?;
        transaction.execute(
            "INSERT INTO knowledge_chunks_fts(chunk_id, title, heading_path, content)
             SELECT CAST(c.id AS TEXT), d.title, c.heading_path, c.content
             FROM knowledge_chunks c
             JOIN knowledge_document_versions v ON v.id = c.document_version_id
             JOIN knowledge_documents d ON d.id = v.document_id
             WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'",
            [],
        )?;
        transaction.commit()?;
        Ok(KnowledgeFtsCapability {
            fts5_available: true,
            trigram_available,
            active_tokenizer: tokenizer.to_string(),
        })
    }

    /// 搜索路径只确认索引可立即使用，绝不在用户请求内执行全量回填。旧库索引缺失时由
    /// 显式“重建全文索引”操作恢复，页面可显示可读错误而不会长时间停留在加载状态。
    pub fn ensure_knowledge_fts_ready_for_search(
        &self,
    ) -> Result<KnowledgeFtsCapability, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let existing_sql = conn
            .query_row(
                "SELECT sql FROM sqlite_master
                 WHERE type = 'table' AND name = 'knowledge_chunks_fts'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(sql) = existing_sql else {
            // 新库或首次导入尚未建表时没有旧索引可误用，允许按既有逻辑创建并填充；
            // 只有“表已存在但内容失配”的历史库才必须避免在搜索请求中全量重建。
            drop(conn);
            return self.ensure_knowledge_fts();
        };
        if knowledge_fts_needs_history_backfill(&conn)? {
            return Err(AppError::KnowledgeFtsRebuildRequired);
        }
        let tokenizer = if sql.to_lowercase().contains("trigram") {
            "trigram"
        } else {
            "unicode61"
        };
        Ok(KnowledgeFtsCapability {
            fts5_available: true,
            trigram_available: probe_fts_tokenizer(&conn, "trigram"),
            active_tokenizer: tokenizer.to_string(),
        })
    }

    pub fn rebuild_knowledge_fts(&self) -> Result<i64, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let existing_sql = conn
            .query_row(
                "SELECT sql FROM sqlite_master
                 WHERE type = 'table' AND name = 'knowledge_chunks_fts'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let transaction = conn.transaction()?;
        if existing_sql.is_none() {
            let tokenizer = if probe_fts_tokenizer(&transaction, "trigram") {
                "trigram"
            } else if probe_fts_tokenizer(&transaction, "unicode61") {
                "unicode61"
            } else {
                return Err(AppError::Custom(
                    "当前 SQLite 运行时不支持 FTS5，无法创建知识索引".to_string(),
                ));
            };
            transaction.execute_batch(&format!(
                "CREATE VIRTUAL TABLE knowledge_chunks_fts USING fts5(
                    chunk_id UNINDEXED,
                    title,
                    heading_path,
                    content,
                    tokenize = '{tokenizer}'
                );"
            ))?;
        }
        // 显式恢复始终在一个事务内完成一次清空和一次回填。不能先调用
        // `ensure_knowledge_fts`，否则旧库失配时会把全量写入执行两遍。
        transaction.execute("DELETE FROM knowledge_chunks_fts", [])?;
        let inserted = transaction.execute(
            "INSERT INTO knowledge_chunks_fts(chunk_id, title, heading_path, content)
             SELECT CAST(c.id AS TEXT), d.title, c.heading_path, c.content
             FROM knowledge_chunks c
             JOIN knowledge_document_versions v ON v.id = c.document_version_id
             JOIN knowledge_documents d ON d.id = v.document_id
             WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'",
            [],
        )?;
        rebuild_knowledge_document_title_index_in_transaction(&transaction)?;
        transaction.commit()?;
        Ok(inserted as i64)
    }

    /// FTS 通道只接收已经通过检索层硬过滤器的输入；SQL 再次落实项目、版本、来源、
    /// 文档类型、敏感级别与来源启用状态，避免调用方遗漏过滤条件导致历史串用。
    pub fn search_knowledge_fts(
        &self,
        input: &KnowledgeSearchInput,
    ) -> Result<Vec<KnowledgeSearchHit>, AppError> {
        self.ensure_knowledge_fts_ready_for_search()?;
        let fts_query = fts_query_from_text(&input.query);
        let short_cjk_terms = chinese_short_terms(&input.query);
        let has_short_cjk_terms = !short_cjk_terms.is_empty();
        let use_short_cjk_fallback = fts_query.is_empty() && has_short_cjk_terms;
        if fts_query.is_empty() && !use_short_cjk_fallback {
            return Ok(Vec::new());
        }
        let mut sql = if use_short_cjk_fallback {
            // FTS5 trigram 不能检索少于三个字的中文词，例如“工单”。在仍保持项目、
            // 版本和敏感级别等硬过滤的前提下，退化为受限 LIKE 检索，避免常用短词静默
            // 变成空查询。长中文自然语言仍优先走 trigram，以避免全表扫描。
            String::from(
                "SELECT c.id, c.document_version_id, c.heading_path, c.content, c.location_json,
                    v.release_id, v.commit_sha, d.id, d.project_id, d.doc_type, d.sensitivity,
                    d.title, COALESCE(NULLIF(v.source_path, ''), d.logical_path), 0.0
             FROM knowledge_chunks c
             JOIN knowledge_document_versions v ON v.id = c.document_version_id
             JOIN knowledge_documents d ON d.id = v.document_id
             LEFT JOIN knowledge_sources s ON s.id = d.source_id
             LEFT JOIN knowledge_code_files cf ON cf.document_version_id = v.id
             LEFT JOIN knowledge_code_snapshots cs ON cs.id = cf.snapshot_id
             WHERE ",
            )
        } else {
            String::from(
                "SELECT c.id, c.document_version_id, c.heading_path, c.content, c.location_json,
                    v.release_id, v.commit_sha, d.id, d.project_id, d.doc_type, d.sensitivity,
                    d.title, COALESCE(NULLIF(v.source_path, ''), d.logical_path), bm25(knowledge_chunks_fts)
             FROM knowledge_chunks_fts
             JOIN knowledge_chunks c ON c.id = CAST(knowledge_chunks_fts.chunk_id AS INTEGER)
             JOIN knowledge_document_versions v ON v.id = c.document_version_id
             JOIN knowledge_documents d ON d.id = v.document_id
             LEFT JOIN knowledge_sources s ON s.id = d.source_id
             LEFT JOIN knowledge_code_files cf ON cf.document_version_id = v.id
             LEFT JOIN knowledge_code_snapshots cs ON cs.id = cf.snapshot_id
             WHERE knowledge_chunks_fts MATCH ?
               AND v.valid = 1
               AND d.deleted_at IS NULL AND d.status = 'active'
               AND d.allow_ai = 1 AND COALESCE(s.enabled, 1) = 1
               AND (cf.id IS NULL OR (cf.status = 'active' AND cs.status = 'analyzed'))
               AND (json_extract(c.location_json, '$.snapshotId') IS NULL OR EXISTS (
                    SELECT 1 FROM knowledge_code_snapshots report_snapshot
                    WHERE report_snapshot.id = CAST(json_extract(c.location_json, '$.snapshotId') AS INTEGER)
                      AND report_snapshot.status = 'analyzed'
               ))",
            )
        };
        let mut values = if use_short_cjk_fallback {
            Vec::new()
        } else {
            vec![Value::Text(fts_query)]
        };
        if has_short_cjk_terms {
            if !use_short_cjk_fallback {
                sql.push_str(" AND ");
            }
            for (index, _) in short_cjk_terms.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" AND ");
                }
                sql.push_str(
                    "(d.title LIKE ? ESCAPE '\\' OR c.heading_path LIKE ? ESCAPE '\\' OR c.content LIKE ? ESCAPE '\\')",
                );
            }
            // 为每个短中文片段绑定三列 LIKE 参数；参数化可防止用户输入改变 SQL 语义。
            short_cjk_terms
                .iter()
                .flat_map(|term| {
                    let pattern = Value::Text(format!("%{}%", escape_like_pattern(term)));
                    [pattern.clone(), pattern.clone(), pattern]
                })
                .for_each(|pattern| values.push(pattern));
        }
        if use_short_cjk_fallback {
            sql.push_str(
                " AND v.valid = 1
               AND d.deleted_at IS NULL AND d.status = 'active'
               AND d.allow_ai = 1 AND COALESCE(s.enabled, 1) = 1
               AND (cf.id IS NULL OR (cf.status = 'active' AND cs.status = 'analyzed'))
               AND (json_extract(c.location_json, '$.snapshotId') IS NULL OR EXISTS (
                    SELECT 1 FROM knowledge_code_snapshots report_snapshot
                    WHERE report_snapshot.id = CAST(json_extract(c.location_json, '$.snapshotId') AS INTEGER)
                      AND report_snapshot.status = 'analyzed'
               ))",
            );
        }
        append_in_filter(&mut sql, &mut values, "d.project_id", &input.project_ids);
        append_selected_document_version_filter(
            &mut sql,
            &mut values,
            "d",
            "v",
            &input.release_ids,
        );
        append_in_filter(&mut sql, &mut values, "d.source_id", &input.source_ids);
        append_text_in_filter(&mut sql, &mut values, "d.doc_type", &input.document_types);
        append_text_in_filter(&mut sql, &mut values, "d.sensitivity", &input.sensitivities);
        if let Some(snapshot_id) = input.snapshot_id {
            // 代码片段与普通文档共用 FTS 表；快照归属只存在片段位置元数据中。使用
            // json_extract 在数据库侧再次硬过滤，避免不同 Git/工作树快照的同名符号串用。
            sql.push_str(" AND CAST(json_extract(c.location_json, '$.snapshotId') AS INTEGER) = ?");
            values.push(Value::Integer(snapshot_id));
        }
        if use_short_cjk_fallback {
            sql.push_str(" ORDER BY c.id LIMIT ?");
        } else {
            sql.push_str(" ORDER BY bm25(knowledge_chunks_fts), c.id LIMIT ?");
        }
        values.push(Value::Integer(input.limit.unwrap_or(20).clamp(1, 100)));
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let rows = conn
            .prepare(&sql)?
            .query_map(params_from_iter(values.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, f64>(13)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let location: serde_json::Value = serde_json::from_str(&row.4).unwrap_or_default();
                let excerpt = row.3.chars().take(400).collect::<String>();
                let snapshot_id = location
                    .get("snapshotId")
                    .and_then(serde_json::Value::as_i64);
                let symbol_key = location
                    .get("symbolKey")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let is_code = snapshot_id.is_some();
                KnowledgeSearchHit {
                    score: -row.13,
                    channels: vec!["fts".to_string()],
                    citation: KnowledgeCitation {
                        citation_key: if let Some(snapshot_id) = snapshot_id {
                            format!("code:snapshot:{snapshot_id}:chunk:{}", row.0)
                        } else {
                            format!("document:{}:version:{}:chunk:{}", row.7, row.1, row.0)
                        },
                        source_type: if is_code {
                            "code_snapshot".to_string()
                        } else {
                            "knowledge_document".to_string()
                        },
                        document_id: Some(row.7),
                        document_version_id: Some(row.1),
                        chunk_id: Some(row.0),
                        project_id: row.8,
                        release_id: row.5,
                        title: row.11,
                        logical_path: row.12,
                        heading_path: row.2,
                        commit_sha: row.6,
                        external_key: String::new(),
                        snapshot_id,
                        symbol_key,
                        start_line: location
                            .get("startLine")
                            .and_then(serde_json::Value::as_i64),
                        end_line: location.get("endLine").and_then(serde_json::Value::as_i64),
                        excerpt,
                    },
                    content: if input.include_context.unwrap_or(false) {
                        row.3
                    } else {
                        String::new()
                    },
                    diagnostics: serde_json::json!({ "rank": row.13 }),
                }
            })
            .collect())
    }

    pub fn upsert_knowledge_chunk_embedding(
        &self,
        chunk_id: i64,
        profile_id: i64,
        content_hash: &str,
        vector: &[f32],
    ) -> Result<KnowledgeChunkEmbedding, AppError> {
        if vector.is_empty() {
            return Err(AppError::InvalidInput("向量不能为空".to_string()));
        }
        let dimension = i64::try_from(vector.len())
            .map_err(|_| AppError::InvalidInput("向量维度超出范围".to_string()))?;
        let vector_norm = vector_norm(vector);
        if !vector_norm.is_finite() || vector_norm <= 0.0 {
            return Err(AppError::InvalidInput(
                "向量范数必须是大于 0 的有限数".to_string(),
            ));
        }
        let vector_blob = encode_vector_blob(vector);
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let (profile_dimension, profile_status, profile_is_active) = transaction
            .query_row(
                "SELECT dimension, status, is_active
                 FROM knowledge_embedding_profiles WHERE id = ?1",
                [profile_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("向量化方案不存在: {profile_id}")))?;
        if profile_status != "building" || profile_is_active {
            return Err(AppError::InvalidInput(
                "仅允许向非活动的 building Profile 写入向量".to_string(),
            ));
        }
        if profile_dimension != 0 && profile_dimension != dimension {
            return Err(AppError::InvalidInput(format!(
                "向量维度不匹配: Profile={profile_dimension}, 实际={dimension}"
            )));
        }
        transaction.execute(
            "INSERT INTO knowledge_chunk_embeddings
             (chunk_id, profile_id, dimension, vector_blob, vector_norm, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(chunk_id, profile_id) DO UPDATE SET
                dimension = excluded.dimension,
                vector_blob = excluded.vector_blob,
                vector_norm = excluded.vector_norm,
                content_hash = excluded.content_hash,
                created_at = datetime('now', 'localtime')",
            params![
                chunk_id,
                profile_id,
                dimension,
                vector_blob,
                vector_norm,
                content_hash
            ],
        )?;
        let embedding = transaction.query_row(
            "SELECT chunk_id, profile_id, dimension, vector_norm, content_hash, created_at
             FROM knowledge_chunk_embeddings
             WHERE chunk_id = ?1 AND profile_id = ?2",
            params![chunk_id, profile_id],
            map_chunk_embedding,
        )?;
        transaction.commit()?;
        Ok(embedding)
    }

    pub fn get_knowledge_chunk_vector(
        &self,
        chunk_id: i64,
        profile_id: i64,
    ) -> Result<Option<Vec<f32>>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let row = conn
            .query_row(
                "SELECT dimension, vector_blob FROM knowledge_chunk_embeddings
                 WHERE chunk_id = ?1 AND profile_id = ?2",
                params![chunk_id, profile_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        row.map(|(dimension, blob)| decode_vector_blob(&blob, dimension))
            .transpose()
    }

    pub fn get_active_knowledge_embedding_profile(
        &self,
    ) -> Result<Option<KnowledgeEmbeddingProfile>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, profile_key, name, mode, provider_key, model, model_revision,
                    dimension, normalized, config_json, fingerprint, status, is_active,
                    created_at, updated_at
             FROM knowledge_embedding_profiles
             WHERE is_active = 1 AND status = 'active'",
            [],
            |row| {
                let config_json = row.get::<_, String>(9)?;
                Ok(KnowledgeEmbeddingProfile {
                    id: row.get(0)?,
                    profile_key: row.get(1)?,
                    name: row.get(2)?,
                    mode: row.get(3)?,
                    provider_key: row.get(4)?,
                    model: row.get(5)?,
                    model_revision: row.get(6)?,
                    dimension: row.get(7)?,
                    normalized: row.get::<_, i64>(8)? != 0,
                    config: serde_json::from_str(&config_json).unwrap_or_default(),
                    fingerprint: row.get(10)?,
                    status: row.get(11)?,
                    is_active: row.get::<_, i64>(12)? != 0,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
                })
            },
        )
        .optional()
        .map_err(AppError::from)
    }

    /// 返回所有 Profile（包括已退役索引），供用户确认蓝绿重建及回滚目标；不返回任何
    /// Provider 凭据，Profile 仅保存凭据引用键。
    pub fn list_knowledge_embedding_profiles(
        &self,
    ) -> Result<Vec<KnowledgeEmbeddingProfile>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, profile_key, name, mode, provider_key, model, model_revision,
                    dimension, normalized, config_json, fingerprint, status, is_active,
                    created_at, updated_at
             FROM knowledge_embedding_profiles
             ORDER BY is_active DESC, updated_at DESC, id DESC",
        )?;
        let profiles = statement
            .query_map([], |row| {
                let config_json = row.get::<_, String>(9)?;
                Ok(KnowledgeEmbeddingProfile {
                    id: row.get(0)?,
                    profile_key: row.get(1)?,
                    name: row.get(2)?,
                    mode: row.get(3)?,
                    provider_key: row.get(4)?,
                    model: row.get(5)?,
                    model_revision: row.get(6)?,
                    dimension: row.get(7)?,
                    normalized: row.get::<_, i64>(8)? != 0,
                    config: serde_json::from_str(&config_json).unwrap_or_default(),
                    fingerprint: row.get(10)?,
                    status: row.get(11)?,
                    is_active: row.get::<_, i64>(12)? != 0,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(profiles)
    }

    pub fn get_knowledge_embedding_profile_by_id(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeEmbeddingProfile>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_embedding_profile(&conn, id)
            .map(Some)
            .or_else(|error| match error {
                AppError::Database(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                error => Err(error),
            })
    }

    pub fn upsert_knowledge_embedding_profile(
        &self,
        input: &UpsertKnowledgeEmbeddingProfileInput,
    ) -> Result<KnowledgeEmbeddingProfile, AppError> {
        let config_json = serde_json::to_string(&input.config)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        if let Some(id) = input.id {
            let existing = get_embedding_profile(&transaction, id)?;
            if existing.is_active || existing.status != "draft" {
                return Err(AppError::InvalidInput(
                    "仅允许修改尚未构建的草稿向量化方案；已构建方案必须新建以保持向量空间不可变"
                        .into(),
                ));
            }
            let changed = transaction.execute(
                "UPDATE knowledge_embedding_profiles SET
                    profile_key = ?1, name = ?2, mode = ?3, provider_key = ?4,
                    model = ?5, model_revision = ?6, dimension = ?7, normalized = ?8,
                    config_json = ?9, fingerprint = ?10,
                    updated_at = datetime('now', 'localtime')
                 WHERE id = ?11 AND is_active = 0",
                params![
                    input.profile_key.trim(),
                    input.name.trim(),
                    input.mode.trim(),
                    input.provider_key.trim(),
                    input.model.trim(),
                    input.model_revision.trim(),
                    input.dimension,
                    input.normalized as i64,
                    config_json,
                    input.fingerprint.trim(),
                    id,
                ],
            )?;
            if changed == 0 {
                return Err(AppError::InvalidInput(
                    "向量化方案不存在或正在活动，不能直接修改".into(),
                ));
            }
            let profile = get_embedding_profile(&transaction, id)?;
            transaction.commit()?;
            return Ok(profile);
        }
        transaction.execute(
            "INSERT INTO knowledge_embedding_profiles
             (profile_key, name, mode, provider_key, model, model_revision, dimension,
              normalized, config_json, fingerprint, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'draft')",
            params![
                input.profile_key.trim(),
                input.name.trim(),
                input.mode.trim(),
                input.provider_key.trim(),
                input.model.trim(),
                input.model_revision.trim(),
                input.dimension,
                input.normalized as i64,
                config_json,
                input.fingerprint.trim(),
            ],
        )?;
        let profile = get_embedding_profile(&transaction, transaction.last_insert_rowid())?;
        transaction.commit()?;
        Ok(profile)
    }

    /// 流式遍历重建预估候选，避免把大规模知识正文一次性加载到内存。
    pub fn visit_knowledge_embedding_rebuild_candidates<F>(
        &self,
        profile_id: i64,
        include_content: bool,
        mut visitor: F,
    ) -> Result<(), AppError>
    where
        F: FnMut(KnowledgeEmbeddingRebuildCandidate) -> Result<(), AppError>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT c.id, d.id, d.source_id, COALESCE(s.source_key, ''),
                    COALESCE(s.display_name, ''), COALESCE(s.enabled, 0),
                    COALESCE(s.allow_remote_embedding, 0), d.sensitivity,
                    c.id, CASE WHEN ?2 = 1 THEN c.content ELSE '' END,
                    c.content_hash, e.content_hash
             FROM knowledge_chunks c
             JOIN knowledge_document_versions v ON v.id = c.document_version_id
             JOIN knowledge_documents d ON d.id = v.document_id
             LEFT JOIN knowledge_sources s ON s.id = d.source_id AND s.deleted_at IS NULL
             LEFT JOIN knowledge_chunk_embeddings e
               ON e.chunk_id = c.id AND e.profile_id = ?1
             WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
             ORDER BY d.id, c.id",
        )?;
        let candidates =
            statement.query_map(params![profile_id, include_content as i64], |row| {
                Ok(KnowledgeEmbeddingRebuildCandidate {
                    chunk_id: row.get(0)?,
                    document_id: row.get(1)?,
                    source_id: row.get(2)?,
                    source_key: row.get(3)?,
                    source_name: row.get(4)?,
                    source_enabled: row.get::<_, i64>(5)? != 0,
                    source_allows_remote_embedding: row.get::<_, i64>(6)? != 0,
                    sensitivity: row.get(7)?,
                    content: row.get(9)?,
                    content_hash: row.get(10)?,
                    existing_embedding_content_hash: row.get(11)?,
                })
            })?;
        for candidate in candidates {
            visitor(candidate?)?;
        }
        Ok(())
    }

    pub fn list_active_knowledge_vector_candidates(
        &self,
        max_candidates: i64,
    ) -> Result<Vec<KnowledgeVectorCandidate>, AppError> {
        self.list_active_knowledge_vector_candidates_with_filters(max_candidates, None)
    }

    /// 向量候选必须在 SQLite 中先应用与 FTS 相同的硬过滤，再限制扫描规模；否则目标
    /// 项目或快照排在全局前 5 万条之后会被静默丢弃。
    pub fn list_active_knowledge_vector_candidates_filtered(
        &self,
        max_candidates: i64,
        filters: &KnowledgeSearchInput,
    ) -> Result<Vec<KnowledgeVectorCandidate>, AppError> {
        self.list_active_knowledge_vector_candidates_with_filters(max_candidates, Some(filters))
    }

    fn list_active_knowledge_vector_candidates_with_filters(
        &self,
        max_candidates: i64,
        filters: Option<&KnowledgeSearchInput>,
    ) -> Result<Vec<KnowledgeVectorCandidate>, AppError> {
        let max_candidates = max_candidates.clamp(1, 50_000);
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut sql = String::from(
            "SELECT p.id, p.dimension, e.chunk_id, e.vector_blob, e.vector_norm,
                        c.document_version_id, c.heading_path, c.content, c.location_json,
                        v.release_id, v.commit_sha, d.id, d.project_id, d.source_id,
                        d.doc_type, d.sensitivity, d.title,
                        COALESCE(NULLIF(v.source_path, ''), d.logical_path)
                 FROM knowledge_embedding_profiles p
                 JOIN knowledge_chunk_embeddings e ON e.profile_id = p.id
                 JOIN knowledge_chunks c ON c.id = e.chunk_id
                 JOIN knowledge_document_versions v ON v.id = c.document_version_id
                 JOIN knowledge_documents d ON d.id = v.document_id
                 LEFT JOIN knowledge_sources s ON s.id = d.source_id
                 LEFT JOIN knowledge_code_files cf ON cf.document_version_id = v.id
                 LEFT JOIN knowledge_code_snapshots cs ON cs.id = cf.snapshot_id
                 WHERE p.is_active = 1 AND p.status = 'active'
                   AND v.valid = 1
                   AND d.deleted_at IS NULL AND d.status = 'active'
                   AND d.allow_ai = 1 AND COALESCE(s.enabled, 1) = 1
                   AND (cf.id IS NULL OR (cf.status = 'active' AND cs.status = 'analyzed'))
                   AND (json_extract(c.location_json, '$.snapshotId') IS NULL OR EXISTS (
                        SELECT 1 FROM knowledge_code_snapshots report_snapshot
                        WHERE report_snapshot.id = CAST(json_extract(c.location_json, '$.snapshotId') AS INTEGER)
                          AND report_snapshot.status = 'analyzed'
                   ))",
        );
        let mut values = Vec::new();
        if let Some(filters) = filters {
            append_in_filter(&mut sql, &mut values, "d.project_id", &filters.project_ids);
            append_selected_document_version_filter(
                &mut sql,
                &mut values,
                "d",
                "v",
                &filters.release_ids,
            );
            append_in_filter(&mut sql, &mut values, "d.source_id", &filters.source_ids);
            append_text_in_filter(&mut sql, &mut values, "d.doc_type", &filters.document_types);
            append_text_in_filter(
                &mut sql,
                &mut values,
                "d.sensitivity",
                &filters.sensitivities,
            );
            if let Some(snapshot_id) = filters.snapshot_id {
                sql.push_str(
                    " AND CAST(json_extract(c.location_json, '$.snapshotId') AS INTEGER) = ?",
                );
                values.push(Value::Integer(snapshot_id));
            }
        }
        sql.push_str(" ORDER BY e.chunk_id LIMIT ?");
        values.push(Value::Integer(max_candidates));
        let raw = conn
            .prepare(&sql)?
            .query_map(params_from_iter(values.iter()), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, String>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, String>(17)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(
                |(
                    profile_id,
                    profile_dimension,
                    chunk_id,
                    vector_blob,
                    vector_norm,
                    document_version_id,
                    heading_path,
                    content,
                    location_json,
                    release_id,
                    commit_sha,
                    document_id,
                    project_id,
                    source_id,
                    doc_type,
                    sensitivity,
                    title,
                    logical_path,
                )| {
                    Ok(KnowledgeVectorCandidate {
                        profile_id,
                        profile_dimension,
                        chunk_id,
                        document_version_id,
                        document_id,
                        project_id,
                        release_id,
                        source_id,
                        doc_type,
                        sensitivity,
                        title,
                        logical_path,
                        heading_path,
                        commit_sha,
                        content,
                        location: serde_json::from_str(&location_json).unwrap_or_default(),
                        vector: decode_vector_blob(&vector_blob, profile_dimension)?,
                        vector_norm,
                    })
                },
            )
            .collect()
    }

    pub fn activate_knowledge_embedding_profile(
        &self,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingProfile, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let profile = get_embedding_profile(&transaction, profile_id)?;
        if profile.status != "ready" || profile.dimension <= 0 || profile.is_active {
            return Err(AppError::InvalidInput(
                "仅允许激活已完成独立构建的非活动 ready Profile".to_string(),
            ));
        }
        let validation = calculate_embedding_index_validation(&transaction, &profile)?;
        if !validation.complete {
            return Err(AppError::InvalidInput(format!(
                "Profile 索引尚不完整，拒绝切换：应有 {} 个片段，已索引 {} 个，过期 {} 个，维度不匹配 {} 个，无效向量 {} 个",
                validation.expected_chunks,
                validation.indexed_chunks,
                validation.stale_chunks,
                validation.dimension_mismatch_chunks,
                validation.invalid_vector_chunks,
            )));
        }
        transaction.execute(
            "UPDATE knowledge_embedding_profiles
             SET is_active = 0,
                 status = CASE WHEN status = 'active' THEN 'ready' ELSE status END,
                 updated_at = datetime('now', 'localtime')
             WHERE is_active = 1",
            [],
        )?;
        transaction.execute(
            "UPDATE knowledge_embedding_profiles
             SET is_active = 1, status = 'active',
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            [profile_id],
        )?;
        transaction.commit()?;
        drop(conn);

        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_embedding_profile(&conn, profile_id)
    }

    /// 将非活动 Profile 标记为构建中；构建期间旧活动索引保持不变。
    pub fn begin_knowledge_embedding_profile_build(
        &self,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingProfile, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let existing = get_embedding_profile(&conn, profile_id)?;
        if !existing.is_active && existing.status == "building" {
            // 应用重启后的 interrupted 向量任务仍保留 building Profile；再次进入页面时
            // 必须允许同一 Profile 幂等恢复，由任务状态机决定从检查点继续还是拒绝并发。
            return Ok(existing);
        }
        let changed = conn.execute(
            "UPDATE knowledge_embedding_profiles
             SET status = 'building', updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND is_active = 0 AND status IN ('draft', 'failed', 'ready')",
            [profile_id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "仅允许开始构建非活动的 draft、failed 或 ready Profile".into(),
            ));
        }
        get_embedding_profile(&conn, profile_id)
    }

    /// 构建任务因模型、Provider、维度或持久化错误结束时，目标 Profile 也必须退出
    /// `building`。旧活动 Profile 不受影响；失败 Profile 可由显式“重新构建”操作再次
    /// 进入 building，避免任务已经失败而索引状态永久显示构建中的假象。
    pub fn fail_knowledge_embedding_profile_for_job(&self, job_id: i64) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "UPDATE knowledge_embedding_profiles
             SET status = 'failed', updated_at = datetime('now', 'localtime')
             WHERE id = (
                SELECT profile_id FROM knowledge_jobs WHERE id = ?1
             )
               AND is_active = 0 AND status = 'building'",
            [job_id],
        )?;
        Ok(())
    }

    /// 仅当 Profile 覆盖全部当前有效片段并且所有向量兼容时，才允许标记为 ready。
    pub fn complete_knowledge_embedding_profile_build(
        &self,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingIndexValidation, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let profile = get_embedding_profile(&transaction, profile_id)?;
        if profile.is_active || profile.status != "building" {
            return Err(AppError::InvalidInput(
                "仅允许完成正在独立构建的非活动 Profile".into(),
            ));
        }
        let validation = calculate_embedding_index_validation(&transaction, &profile)?;
        if !validation.complete {
            transaction.execute(
                "UPDATE knowledge_embedding_profiles
                 SET status = 'failed', updated_at = datetime('now', 'localtime')
                 WHERE id = ?1",
                [profile_id],
            )?;
            transaction.commit()?;
            return Err(AppError::InvalidInput(format!(
                "Profile 构建未完成：应有 {} 个片段，已索引 {} 个，无效向量 {} 个",
                validation.expected_chunks,
                validation.indexed_chunks,
                validation.invalid_vector_chunks,
            )));
        }
        transaction.execute(
            "UPDATE knowledge_embedding_profiles
             SET status = 'ready', updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            [profile_id],
        )?;
        transaction.commit()?;
        Ok(validation)
    }

    pub fn validate_knowledge_embedding_profile(
        &self,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingIndexValidation, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let profile = get_embedding_profile(&conn, profile_id)?;
        calculate_embedding_index_validation(&conn, &profile)
    }

    /// 显式退休一个非活动索引并删除其向量；活动和最近保留索引由调用者在 UI 中明确确认后才可清理。
    pub fn retire_knowledge_embedding_profile(
        &self,
        profile_id: i64,
    ) -> Result<KnowledgeEmbeddingProfile, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let profile = get_embedding_profile(&transaction, profile_id)?;
        if profile.is_active || profile.status == "building" {
            return Err(AppError::InvalidInput(
                "不能清理当前活动或正在构建的向量化方案".into(),
            ));
        }
        transaction.execute(
            "DELETE FROM knowledge_chunk_embeddings WHERE profile_id = ?1",
            [profile_id],
        )?;
        transaction.execute(
            "UPDATE knowledge_embedding_profiles
             SET status = 'retired', updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            [profile_id],
        )?;
        transaction.commit()?;
        get_embedding_profile(&conn, profile_id)
    }

    /// 关系的业务唯一键由两端实体、类型和来源组成；重复导入更新证据与置信度，
    /// 不会制造无法解释的平行事实。人工确认只会提升同一条关系，不会覆盖来源证据。
    pub fn upsert_knowledge_relation(
        &self,
        input: &UpsertKnowledgeRelationInput,
    ) -> Result<KnowledgeRelation, AppError> {
        let evidence_json = serde_json::to_string(&input.evidence)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        if let Some(id) = input.id {
            let changed = conn.execute(
                "UPDATE knowledge_relations SET
                    project_id = ?1, release_id = ?2, document_version_id = ?3, snapshot_id = ?4,
                    sensitivity = ?5, from_type = ?6, from_key = ?7, relation_type = ?8,
                    to_type = ?9, to_key = ?10, evidence_json = ?11, confidence = ?12,
                    confirmed = ?13, source = ?14, deleted_at = NULL,
                    updated_at = datetime('now', 'localtime') WHERE id = ?15",
                params![
                    input.project_id.unwrap_or(0),
                    input.release_id.unwrap_or(0),
                    input.document_version_id.unwrap_or(0),
                    input.snapshot_id.unwrap_or(0),
                    input.sensitivity,
                    input.from_type,
                    input.from_key,
                    input.relation_type,
                    input.to_type,
                    input.to_key,
                    evidence_json,
                    input.confidence,
                    input.confirmed as i64,
                    input.source,
                    id,
                ],
            )?;
            if changed == 0 {
                return Err(AppError::NotFound(format!("知识关系不存在: {id}")));
            }
            return get_knowledge_relation(&conn, id);
        }
        conn.execute(
            "INSERT INTO knowledge_relations
                 (project_id, release_id, document_version_id, snapshot_id, sensitivity,
                  scope_status, from_type, from_key, relation_type, to_type, to_key, evidence_json,
                  confidence, confirmed, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(project_id, release_id, document_version_id, snapshot_id,
                         from_type, from_key, relation_type, to_type, to_key, source)
             DO UPDATE SET evidence_json = excluded.evidence_json,
                           confidence = excluded.confidence,
                           confirmed = MAX(knowledge_relations.confirmed, excluded.confirmed),
                           deleted_at = NULL,
                           updated_at = datetime('now', 'localtime')",
            params![
                input.project_id.unwrap_or(0),
                input.release_id.unwrap_or(0),
                input.document_version_id.unwrap_or(0),
                input.snapshot_id.unwrap_or(0),
                input.sensitivity,
                if input.project_id.is_some()
                    || input.release_id.is_some()
                    || input.document_version_id.is_some()
                    || input.snapshot_id.is_some()
                {
                    "scoped"
                } else {
                    "needs_rebuild"
                },
                input.from_type,
                input.from_key,
                input.relation_type,
                input.to_type,
                input.to_key,
                evidence_json,
                input.confidence,
                input.confirmed as i64,
                input.source,
            ],
        )?;
        let id = conn.query_row(
            "SELECT id FROM knowledge_relations
             WHERE project_id = ?1 AND release_id = ?2 AND document_version_id = ?3
               AND snapshot_id = ?4 AND from_type = ?5 AND from_key = ?6
               AND relation_type = ?7 AND to_type = ?8 AND to_key = ?9 AND source = ?10",
            params![
                input.project_id.unwrap_or(0),
                input.release_id.unwrap_or(0),
                input.document_version_id.unwrap_or(0),
                input.snapshot_id.unwrap_or(0),
                input.from_type,
                input.from_key,
                input.relation_type,
                input.to_type,
                input.to_key,
                input.source,
            ],
            |row| row.get::<_, i64>(0),
        )?;
        get_knowledge_relation(&conn, id)
    }

    pub fn list_knowledge_relations(
        &self,
        input: &ListKnowledgeRelationsInput,
    ) -> Result<Vec<KnowledgeRelation>, AppError> {
        let mut sql = String::from(
            "SELECT id, project_id, release_id, document_version_id, snapshot_id, sensitivity,
                    scope_status, from_type, from_key, relation_type, to_type, to_key, evidence_json,
                    confidence, confirmed, source, created_at, updated_at, deleted_at
             FROM knowledge_relations WHERE deleted_at IS NULL
               AND (snapshot_id = 0 OR EXISTS (
                    SELECT 1 FROM knowledge_code_snapshots snapshot
                    WHERE snapshot.id = knowledge_relations.snapshot_id
                      AND snapshot.status = 'analyzed'
               ))",
        );
        let mut values = Vec::<Value>::new();
        if let (Some(entity_type), Some(entity_key)) = (&input.entity_type, &input.entity_key) {
            sql.push_str(" AND ((from_type = ? AND from_key = ?) OR (to_type = ? AND to_key = ?))");
            values.extend([
                Value::Text(entity_type.clone()),
                Value::Text(entity_key.clone()),
                Value::Text(entity_type.clone()),
                Value::Text(entity_key.clone()),
            ]);
        }
        if input.confirmed_only.unwrap_or(false) {
            sql.push_str(" AND confirmed = 1");
        }
        append_in_filter(&mut sql, &mut values, "project_id", &input.project_ids);
        append_in_filter(&mut sql, &mut values, "release_id", &input.release_ids);
        append_text_in_filter(&mut sql, &mut values, "sensitivity", &input.sensitivities);
        sql.push_str(" ORDER BY confirmed DESC, confidence DESC, id DESC LIMIT ?");
        values.push(Value::Integer(input.limit.unwrap_or(100).clamp(1, 500)));
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let relations = conn
            .prepare(&sql)?
            .query_map(params_from_iter(values.iter()), map_knowledge_relation)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(relations)
    }

    /// 代码快照重新分析前撤销此前投影到通用关系图的证据。分析成功会在同一快照再次
    /// 建立确定性边；失败则保持撤销状态，避免旧证据绕过快照状态和内容隔离。
    pub fn deactivate_knowledge_relations_for_snapshot(
        &self,
        snapshot_id: i64,
    ) -> Result<i64, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_relations
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE snapshot_id = ?1 AND deleted_at IS NULL",
            [snapshot_id],
        )?;
        Ok(i64::try_from(changed).unwrap_or(i64::MAX))
    }

    pub fn confirm_knowledge_relation(
        &self,
        id: i64,
        confirmed: bool,
    ) -> Result<KnowledgeRelation, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_relations SET confirmed = ?1, updated_at = datetime('now', 'localtime')
             WHERE id = ?2 AND deleted_at IS NULL",
            params![confirmed as i64, id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("知识关系不存在: {id}")));
        }
        get_knowledge_relation(&conn, id)
    }

    pub fn create_knowledge_job(
        &self,
        input: &CreateKnowledgeJobInput,
    ) -> Result<KnowledgeJob, AppError> {
        let checkpoint_json = serde_json::to_string(&input.checkpoint)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_jobs
             (job_key, job_type, source_id, profile_id, status, message,
              checkpoint_json, heartbeat_at)
             VALUES (?1, ?2, ?3, ?4, 'queued', ?5, ?6, datetime('now', 'localtime'))",
            params![
                input.job_key,
                input.job_type,
                input.source_id,
                input.profile_id,
                input.message,
                checkpoint_json
            ],
        )?;
        get_knowledge_job_by_id(&conn, conn.last_insert_rowid())
    }

    pub fn find_active_knowledge_job(
        &self,
        job_type: &str,
        source_id: Option<i64>,
    ) -> Result<Option<KnowledgeJob>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, job_key, job_type, source_id, profile_id, status,
                    progress_current, progress_total, message, error,
                    checkpoint_json, heartbeat_at, cancel_requested,
                    started_at, finished_at
             FROM knowledge_jobs
             WHERE job_type = ?1 AND source_id IS ?2
               AND status IN ('queued', 'running')
             ORDER BY id DESC LIMIT 1",
            params![job_type, source_id],
            map_knowledge_job,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn get_knowledge_job(&self, job_key: &str) -> Result<Option<KnowledgeJob>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, job_key, job_type, source_id, profile_id, status,
                    progress_current, progress_total, message, error,
                    checkpoint_json, heartbeat_at, cancel_requested,
                    started_at, finished_at
             FROM knowledge_jobs WHERE job_key = ?1",
            [job_key],
            map_knowledge_job,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn get_knowledge_job_by_id(&self, id: i64) -> Result<Option<KnowledgeJob>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, job_key, job_type, source_id, profile_id, status,
                    progress_current, progress_total, message, error,
                    checkpoint_json, heartbeat_at, cancel_requested,
                    started_at, finished_at
             FROM knowledge_jobs WHERE id = ?1",
            [id],
            map_knowledge_job,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn list_knowledge_jobs(&self, limit: i64) -> Result<Vec<KnowledgeJob>, AppError> {
        let limit = limit.clamp(1, MAX_PAGE_LIMIT);
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let items = conn
            .prepare(
                "SELECT id, job_key, job_type, source_id, profile_id, status,
                        progress_current, progress_total, message, error,
                        checkpoint_json, heartbeat_at, cancel_requested,
                        started_at, finished_at
                 FROM knowledge_jobs ORDER BY id DESC LIMIT ?1",
            )?
            .query_map([limit], map_knowledge_job)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    pub fn mark_knowledge_job_running(
        &self,
        id: i64,
        stage: &str,
        message: &str,
        checkpoint: &serde_json::Value,
    ) -> Result<KnowledgeJob, AppError> {
        let checkpoint_json = serde_json::to_string(checkpoint)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET
                status = 'running', message = ?1, error = NULL,
                checkpoint_json = ?2, heartbeat_at = datetime('now', 'localtime'),
                cancel_requested = 0, finished_at = NULL
             WHERE id = ?3 AND status = 'queued'",
            params![format!("{stage}: {message}"), checkpoint_json, id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "知识任务当前状态不允许启动".to_string(),
            ));
        }
        get_knowledge_job_by_id(&conn, id)
    }

    pub fn update_knowledge_job_progress(
        &self,
        id: i64,
        current: i64,
        total: i64,
        message: &str,
        checkpoint: &serde_json::Value,
    ) -> Result<bool, AppError> {
        let checkpoint_json = serde_json::to_string(checkpoint)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET
                progress_current = ?1, progress_total = ?2, message = ?3,
                checkpoint_json = ?4, heartbeat_at = datetime('now', 'localtime')
             WHERE id = ?5 AND status = 'running' AND cancel_requested = 0",
            params![current, total, message, checkpoint_json, id],
        )?;
        Ok(changed > 0)
    }

    /// 向量构建由多个短 Command 批次组成；每批落盘后回到 queued，下一批再原子地
    /// 领取为 running。这样页面刷新不会把已经安全保存检查点的任务永久卡在 running。
    pub fn queue_knowledge_job_next_batch(&self, id: i64) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET
                status = 'queued',
                message = '等待下一批向量构建',
                heartbeat_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status = 'running' AND cancel_requested = 0",
            [id],
        )?;
        Ok(changed > 0)
    }

    pub fn touch_knowledge_job_heartbeat(&self, id: i64) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET heartbeat_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status = 'running'",
            [id],
        )?;
        Ok(changed > 0)
    }

    pub fn request_knowledge_job_cancel(&self, id: i64) -> Result<KnowledgeJob, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET
                cancel_requested = 1,
                status = CASE
                    WHEN status IN ('queued', 'interrupted') THEN 'cancelled'
                    ELSE status
                END,
                message = CASE
                    WHEN status IN ('queued', 'interrupted') THEN '任务已取消'
                    ELSE '已请求取消，正在安全停止'
                END,
                finished_at = CASE
                    WHEN status IN ('queued', 'interrupted')
                    THEN datetime('now', 'localtime')
                    ELSE finished_at
                END
             WHERE id = ?1 AND status IN ('queued', 'running', 'interrupted')",
            [id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "知识任务已结束或当前状态不允许取消".to_string(),
            ));
        }
        get_knowledge_job_by_id(&conn, id)
    }

    /// 停止非活动向量构建时，取消标记与 Profile 禁写必须原子提交。否则远程请求刚好
    /// 返回的窗口可能继续写入即将删除的副本向量。
    pub fn cancel_knowledge_embedding_job_and_fail_profile(
        &self,
        id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let transaction = conn.transaction()?;
        let profile_id: i64 = transaction
            .query_row(
                "SELECT profile_id FROM knowledge_jobs
                 WHERE id = ?1 AND job_type = 'embedding_build' AND profile_id IS NOT NULL",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                AppError::InvalidInput("向量构建任务不存在或缺少方案引用".to_string())
            })?;
        let changed = transaction.execute(
            "UPDATE knowledge_jobs SET
                cancel_requested = 1,
                status = 'cancelled',
                message = '远程向量构建已取消',
                error = NULL,
                finished_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status IN ('queued', 'running', 'interrupted')",
            [id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "知识任务已结束或当前状态不允许取消".to_string(),
            ));
        }
        transaction.execute(
            "UPDATE knowledge_embedding_profiles
             SET status = 'failed', updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND is_active = 0 AND status = 'building'",
            [profile_id],
        )?;
        let job = get_knowledge_job_by_id(&transaction, id)?;
        transaction.commit()?;
        Ok(job)
    }

    pub fn is_knowledge_job_cancel_requested(&self, id: i64) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT cancel_requested FROM knowledge_jobs WHERE id = ?1",
            [id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("知识任务不存在: {id}")))
    }

    pub fn finish_knowledge_job(
        &self,
        id: i64,
        status: &str,
        message: &str,
        error: Option<&str>,
        checkpoint: &serde_json::Value,
    ) -> Result<KnowledgeJob, AppError> {
        if !matches!(status, "completed" | "failed" | "cancelled" | "interrupted") {
            return Err(AppError::InvalidInput("未知知识任务终态".to_string()));
        }
        let checkpoint_json = serde_json::to_string(checkpoint)?;
        let conn = self
            .conn
            .lock()
            .map_err(|lock_error| AppError::Custom(lock_error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET
                status = ?1, message = ?2, error = ?3, checkpoint_json = ?4,
                heartbeat_at = datetime('now', 'localtime'),
                finished_at = datetime('now', 'localtime')
             WHERE id = ?5 AND status IN ('queued', 'running', 'interrupted')
               -- 完成态必须与取消请求做 compare-and-set，防止最后一次写入后用户点击
               -- 取消仍被 completed 覆盖；失败/中断/取消收尾仍允许保存检查点。
               AND (?1 <> 'completed' OR cancel_requested = 0)",
            params![status, message, error, checkpoint_json, id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "知识任务当前状态不允许结束".to_string(),
            ));
        }
        get_knowledge_job_by_id(&conn, id)
    }

    /// 失败与取消的收尾必须比较并交换：用户已经请求取消时，解析错误不能把任务重新
    /// 标记为失败，避免界面显示与实际用户操作相反的终态。
    pub fn finish_knowledge_job_failed_or_cancel(
        &self,
        id: i64,
        failure_message: &str,
        error: &str,
        failed_checkpoint: &serde_json::Value,
        cancelled_message: &str,
        cancelled_checkpoint: &serde_json::Value,
    ) -> Result<KnowledgeJob, AppError> {
        let failed_checkpoint_json = serde_json::to_string(failed_checkpoint)?;
        let cancelled_checkpoint_json = serde_json::to_string(cancelled_checkpoint)?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|lock_error| AppError::Custom(lock_error.to_string()))?;
        let transaction = conn.transaction()?;
        let failed = transaction.execute(
            "UPDATE knowledge_jobs SET
                status = 'failed', message = ?1, error = ?2, checkpoint_json = ?3,
                heartbeat_at = datetime('now', 'localtime'),
                finished_at = datetime('now', 'localtime')
             WHERE id = ?4 AND status IN ('queued', 'running', 'interrupted')
               AND cancel_requested = 0",
            params![failure_message, error, failed_checkpoint_json, id],
        )?;
        if failed == 0 {
            let cancelled = transaction.execute(
                "UPDATE knowledge_jobs SET
                    status = 'cancelled', message = ?1, error = NULL, checkpoint_json = ?2,
                    heartbeat_at = datetime('now', 'localtime'),
                    finished_at = datetime('now', 'localtime')
                 WHERE id = ?3 AND status IN ('queued', 'running', 'interrupted')
                   AND cancel_requested = 1",
                params![cancelled_message, cancelled_checkpoint_json, id],
            )?;
            if cancelled == 0 {
                return Err(AppError::InvalidInput(
                    "知识任务当前状态不允许结束".to_string(),
                ));
            }
        }
        transaction.commit()?;
        get_knowledge_job_by_id(&conn, id)
    }

    pub fn restart_knowledge_job(&self, id: i64) -> Result<KnowledgeJob, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET
                status = 'queued', message = '任务已进入重试队列', error = NULL,
                cancel_requested = 0, heartbeat_at = datetime('now', 'localtime'),
                finished_at = NULL
             WHERE id = ?1 AND status IN ('failed', 'cancelled', 'interrupted')",
            [id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "仅失败、取消或中断任务允许重试".to_string(),
            ));
        }
        get_knowledge_job_by_id(&conn, id)
    }

    /// Embedding 的任务键按 Profile 复用；当最后一批已写完但客户端在完成
    /// Profile 重建前退出时，Profile 仍会是 building，而任务已是 completed。此时从
    /// 零检查点重启可以复用已有向量的 content_hash，重新覆盖新增或变更片段，同时不
    /// 影响其他类型的已完成任务。
    pub fn restart_completed_knowledge_embedding_job(
        &self,
        id: i64,
        profile_id: i64,
    ) -> Result<KnowledgeJob, AppError> {
        let checkpoint = serde_json::json!({
            "profileId": profile_id,
            "lastChunkId": 0,
            "processed": 0,
            "embedded": 0,
            "skipped": 0,
            "blocked": 0,
        });
        let checkpoint_json = serde_json::to_string(&checkpoint)?;
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET
                status = 'queued', message = '任务已重新进入向量构建队列', error = NULL,
                progress_current = 0, progress_total = 0, checkpoint_json = ?1,
                cancel_requested = 0, heartbeat_at = datetime('now', 'localtime'),
                finished_at = NULL
             WHERE id = ?2 AND job_type = 'embedding_build'
               AND profile_id = ?3 AND status = 'completed'",
            params![checkpoint_json, id, profile_id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "仅已完成的同一向量 Profile 任务允许重新构建".to_string(),
            ));
        }
        get_knowledge_job_by_id(&conn, id)
    }

    pub fn recover_interrupted_knowledge_jobs(
        &self,
        stale_after_seconds: i64,
    ) -> Result<i64, AppError> {
        let stale_after_seconds = stale_after_seconds.max(0);
        let modifier = format!("-{stale_after_seconds} seconds");
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_jobs SET
                status = 'interrupted',
                message = '应用退出或任务心跳中断，可从检查点重试',
                error = '任务心跳已中断',
                finished_at = datetime('now', 'localtime')
             WHERE status IN ('queued', 'running')
               AND (heartbeat_at IS NULL
                    OR heartbeat_at <= datetime('now', 'localtime', ?1))",
            [modifier],
        )?;
        Ok(changed as i64)
    }

    fn soft_delete_knowledge_record(&self, table: &str, id: i64) -> Result<(), AppError> {
        let allowed = [
            "knowledge_projects",
            "knowledge_releases",
            "knowledge_sources",
            "knowledge_documents",
        ];
        if !allowed.contains(&table) {
            return Err(AppError::InvalidInput("不允许软删除未知知识表".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let sql = format!(
            "UPDATE {table}
             SET deleted_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NULL"
        );
        let changed = conn.execute(&sql, [id])?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("知识记录不存在: {id}")));
        }
        Ok(())
    }
}

pub fn encode_vector_blob(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

pub fn decode_vector_blob(blob: &[u8], dimension: i64) -> Result<Vec<f32>, AppError> {
    let dimension = usize::try_from(dimension)
        .map_err(|_| AppError::InvalidInput("向量维度不能为负数".to_string()))?;
    let expected_bytes = dimension
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| AppError::InvalidInput("向量维度超出范围".to_string()))?;
    if dimension == 0 || blob.len() != expected_bytes {
        return Err(AppError::InvalidInput(format!(
            "向量 BLOB 长度不匹配: dimension={dimension}, bytes={}",
            blob.len()
        )));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect())
}

fn vector_norm(vector: &[f32]) -> f64 {
    vector
        .iter()
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt()
}

fn normalized_page(offset: Option<i64>, limit: Option<i64>) -> (i64, i64) {
    (
        offset.unwrap_or(0).max(0),
        limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT),
    )
}

fn normalized_keyword(keyword: Option<&str>) -> Option<String> {
    keyword
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"))
}

pub(crate) fn insert_chunks(
    conn: &Connection,
    document_version_id: i64,
    chunks: &[KnowledgeChunkWriteInput],
) -> Result<(), AppError> {
    let mut statement = conn.prepare(
        "INSERT INTO knowledge_chunks
         (document_version_id, chunk_index, heading_path, content, content_hash,
          location_json, token_estimate)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for chunk in chunks {
        statement.execute(params![
            document_version_id,
            chunk.chunk_index,
            chunk.heading_path,
            chunk.content,
            chunk.content_hash,
            serde_json::to_string(&chunk.location)?,
            chunk.token_estimate
        ])?;
    }
    Ok(())
}

fn replace_knowledge_document_chunks_in_transaction(
    transaction: &Transaction<'_>,
    document_version_id: i64,
    parsed_meta_json: &str,
    token_estimate: i64,
    chunks: &[KnowledgeChunkWriteInput],
) -> Result<(), AppError> {
    let document_id = transaction
        .query_row(
            "SELECT document_id FROM knowledge_document_versions WHERE id = ?1",
            [document_version_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("知识文档版本不存在: {document_version_id}")))?;
    let existing = transaction
        .prepare(
            "SELECT chunk_index, content_hash FROM knowledge_chunks
             WHERE document_version_id = ?1 ORDER BY chunk_index",
        )?
        .query_map([document_version_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let incoming = chunks
        .iter()
        .map(|chunk| (chunk.chunk_index, chunk.content_hash.clone()))
        .collect::<Vec<_>>();

    transaction.execute(
        "UPDATE knowledge_document_versions SET
            parsed_meta_json = ?1, token_estimate = ?2
         WHERE id = ?3",
        params![parsed_meta_json, token_estimate, document_version_id],
    )?;
    if existing == incoming {
        return Ok(());
    }

    transaction.execute(
        "DELETE FROM knowledge_chunk_embeddings
         WHERE chunk_id IN (
            SELECT id FROM knowledge_chunks WHERE document_version_id = ?1
         )",
        [document_version_id],
    )?;
    let fts_exists = transaction
        .query_row(
            "SELECT 1 FROM sqlite_master
             WHERE type = 'table' AND name = 'knowledge_chunks_fts'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if fts_exists {
        transaction.execute(
            "DELETE FROM knowledge_chunks_fts
             WHERE CAST(chunk_id AS INTEGER) IN (
                SELECT id FROM knowledge_chunks WHERE document_version_id = ?1
             )",
            [document_version_id],
        )?;
    }
    transaction.execute(
        "DELETE FROM knowledge_chunks WHERE document_version_id = ?1",
        [document_version_id],
    )?;
    insert_chunks(transaction, document_version_id, chunks)?;
    sync_document_fts_if_available(transaction, document_id, document_version_id)
}

pub(crate) fn sync_document_fts_if_available(
    conn: &Connection,
    document_id: i64,
    _document_version_id: i64,
) -> Result<(), AppError> {
    let fts_exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master
             WHERE type = 'table' AND name = 'knowledge_chunks_fts'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !fts_exists {
        return Ok(());
    }
    conn.execute(
        "DELETE FROM knowledge_chunks_fts
         WHERE CAST(chunk_id AS INTEGER) IN (
            SELECT chunk.id FROM knowledge_chunks chunk
            JOIN knowledge_document_versions version ON version.id = chunk.document_version_id
            WHERE version.document_id = ?1
         )",
        [document_id],
    )?;
    conn.execute(
        "INSERT INTO knowledge_chunks_fts(chunk_id, title, heading_path, content)
         SELECT CAST(c.id AS TEXT), d.title, c.heading_path, c.content
         FROM knowledge_documents d
         JOIN knowledge_document_versions v ON v.document_id = d.id AND v.valid = 1
         JOIN knowledge_chunks c ON c.document_version_id = v.id
         WHERE d.id = ?1",
        params![document_id],
    )?;
    Ok(())
}

/// 仅比较当前可见有效片段与 FTS 条目数量；不读取正文。旧索引缺少历史版本或清理后
/// 留下计数偏差时，`ensure_knowledge_fts` 会在同一数据库事务中重建派生索引。
fn knowledge_fts_needs_history_backfill(conn: &Connection) -> Result<bool, AppError> {
    let expected = knowledge_fts_expected_entry_count(conn)?;
    let actual = conn.query_row("SELECT COUNT(*) FROM knowledge_chunks_fts", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(expected != actual)
}

fn knowledge_fts_expected_entry_count(conn: &Connection) -> Result<i64, AppError> {
    conn.query_row(
        "SELECT COUNT(*)
         FROM knowledge_chunks c
         JOIN knowledge_document_versions v ON v.id = c.document_version_id
         JOIN knowledge_documents d ON d.id = v.document_id
         WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map_err(Into::into)
}

/// 统计逻辑文档当前留存的 FTS 行。FTS 是可重建派生索引，缺失时按零处理，
/// 因而预览和恢复都不依赖某个 SQLite 构建是否启用了 FTS5。
fn fts_entry_count(conn: &Connection, document_id: i64) -> Result<i64, AppError> {
    let fts_exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master
             WHERE type = 'table' AND name = 'knowledge_chunks_fts'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !fts_exists {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*) FROM knowledge_chunks_fts
         WHERE CAST(chunk_id AS INTEGER) IN (
            SELECT chunk.id FROM knowledge_chunks chunk
            JOIN knowledge_document_versions version ON version.id = chunk.document_version_id
            WHERE version.document_id = ?1
         )",
        [document_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn probe_fts_tokenizer(conn: &Connection, tokenizer: &str) -> bool {
    if !matches!(tokenizer, "trigram" | "unicode61") {
        return false;
    }
    let sql = format!(
        "CREATE VIRTUAL TABLE temp.__knowledge_fts_probe
         USING fts5(content, tokenize='{tokenizer}');
         DROP TABLE temp.__knowledge_fts_probe;"
    );
    conn.execute_batch(&sql).is_ok()
}

fn fts_query_from_text(query: &str) -> String {
    let exact_terms = query
        .split_whitespace()
        .map(|part| part.replace('"', ""))
        // 含中文的无空格片段不能作为精确短语保留，否则会重新引入整句匹配的
        // 召回死角；独立的英文 API、类名或文件名仍按精确词参与检索。
        .filter(|part| !part.is_empty() && !part.chars().any(is_cjk))
        .collect::<Vec<_>>();
    let chinese_terms = chinese_trigram_terms(query);
    if chinese_terms.is_empty() {
        return exact_terms
            .into_iter()
            .map(|term| format!("\"{term}\""))
            .collect::<Vec<_>>()
            .join(" AND ");
    }

    // 中文提问通常没有空格，若将整句作为一个 FTS 短语，会要求源码或文档原样出现
    // “明日工作计划生成的逻辑是什么”之类的完整句子。这里保留原有项目/版本等 SQL
    // 硬过滤，仅把中文连续片段拆成 FTS5 trigram 可召回的查询词，并与显式英文标识符
    // 并列召回；后续 BM25、融合和引用校验仍决定最终可回答证据。
    chinese_terms
        .into_iter()
        .chain(exact_terms)
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn chinese_trigram_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = Vec::new();
    let mut append_current = |characters: &mut Vec<char>| {
        for window in characters.windows(3) {
            let term = window.iter().collect::<String>();
            if !terms.contains(&term) {
                terms.push(term);
            }
        }
        characters.clear();
    };
    for character in query.chars() {
        if is_cjk(character) {
            current.push(character);
        } else {
            append_current(&mut current);
        }
    }
    append_current(&mut current);
    terms
}

/// 为 1-2 字中文片段提供受限 LIKE 回退；三字及以上仍由 FTS5 trigram 负责。
fn chinese_short_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = Vec::new();
    let mut append_current = |characters: &mut Vec<char>| {
        if (1..=2).contains(&characters.len()) {
            let term = characters.iter().collect::<String>();
            if !terms.contains(&term) {
                terms.push(term);
            }
        }
        characters.clear();
    };
    for character in query.chars() {
        if is_cjk(character) {
            current.push(character);
        } else {
            append_current(&mut current);
        }
    }
    append_current(&mut current);
    terms
}

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn is_cjk(character: char) -> bool {
    matches!(character,
        '\u{3400}'..='\u{4dbf}'
            | '\u{4e00}'..='\u{9fff}'
            | '\u{f900}'..='\u{faff}'
    )
}

fn append_in_filter(sql: &mut String, values: &mut Vec<Value>, column: &str, ids: &[i64]) {
    if ids.is_empty() {
        return;
    }
    sql.push_str(" AND ");
    sql.push_str(column);
    sql.push_str(" IN (");
    sql.push_str(
        &std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(","),
    );
    sql.push(')');
    values.extend(ids.iter().copied().map(Value::Integer));
}

fn append_text_in_filter(
    sql: &mut String,
    values: &mut Vec<Value>,
    column: &str,
    texts: &[String],
) {
    if texts.is_empty() {
        return;
    }
    sql.push_str(" AND ");
    sql.push_str(column);
    sql.push_str(" IN (");
    sql.push_str(
        &std::iter::repeat_n("?", texts.len())
            .collect::<Vec<_>>()
            .join(","),
    );
    sql.push(')');
    values.extend(texts.iter().cloned().map(Value::Text));
}

/// `project_key` 是项目的稳定身份，ID 更新请求不能借用另一个 key 覆盖其记录；而无 ID
/// 的同 key 请求被视为客户端在响应丢失后的安全重试，应该更新原记录而非插入第二个项目。
fn resolve_knowledge_project_upsert_target(
    conn: &Connection,
    input: &UpsertKnowledgeProjectInput,
) -> Result<Option<i64>, AppError> {
    let existing_by_key = conn
        .query_row(
            "SELECT id, project_key, name, aliases_json, description, git_workspace_key,
                    git_workspace_keys_json, default_branch, enabled, created_at, updated_at, deleted_at
             FROM knowledge_projects WHERE project_key = ?1",
            [input.project_key.trim()],
            map_knowledge_project,
        )
        .optional()?;

    let Some(id) = input.id else {
        return Ok(existing_by_key.map(|project| project.id));
    };
    let existing_by_id = conn
        .query_row(
            "SELECT id, project_key, name, aliases_json, description, git_workspace_key,
                    git_workspace_keys_json, default_branch, enabled, created_at, updated_at, deleted_at
             FROM knowledge_projects WHERE id = ?1",
            [id],
            map_knowledge_project,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("知识项目不存在: {id}")))?;

    if existing_by_id.project_key != input.project_key.trim() {
        return Err(AppError::InvalidInput(
            "项目 ID 与项目标识不匹配，不能修改已有项目的稳定标识".to_string(),
        ));
    }
    if existing_by_key.is_some_and(|project| project.id != id) {
        return Err(AppError::InvalidInput(
            "项目 ID 与项目标识不匹配，不能覆盖其他项目".to_string(),
        ));
    }
    Ok(Some(id))
}

/// `source_key` 只可指向一个既有来源。无 ID 的同 key 请求是可恢复的批量重试，但
/// 必须保持原项目归属；携带 ID 的请求则同时校验 ID、key 和归属，避免跨项目挪用来源。
fn resolve_knowledge_source_upsert_target(
    conn: &Connection,
    input: &UpsertKnowledgeSourceInput,
) -> Result<Option<i64>, AppError> {
    let existing_by_key = conn
        .query_row(
            "SELECT id, source_key, project_id, source_type, display_name, root_path,
                    git_workspace_key, include_globs_json, exclude_globs_json,
                    version_strategy, sync_mode, allow_remote_embedding, enabled,
                    last_commit_sha, last_sync_status, last_synced_at, last_error,
                    created_at, updated_at, deleted_at
             FROM knowledge_sources WHERE source_key = ?1",
            [input.source_key.trim()],
            map_knowledge_source,
        )
        .optional()?;

    let Some(id) = input.id else {
        if let Some(source) = existing_by_key {
            if source.project_id != input.project_id {
                return Err(AppError::InvalidInput(format!(
                    "知识源标识已被其他项目占用: {}",
                    input.source_key
                )));
            }
            return Ok(Some(source.id));
        }
        return Ok(None);
    };

    let existing_by_id = conn
        .query_row(
            "SELECT id, source_key, project_id, source_type, display_name, root_path,
                    git_workspace_key, include_globs_json, exclude_globs_json,
                    version_strategy, sync_mode, allow_remote_embedding, enabled,
                    last_commit_sha, last_sync_status, last_synced_at, last_error,
                    created_at, updated_at, deleted_at
             FROM knowledge_sources WHERE id = ?1",
            [id],
            map_knowledge_source,
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {id}")))?;

    if existing_by_id.source_key != input.source_key.trim()
        || existing_by_id.project_id != input.project_id
    {
        return Err(AppError::InvalidInput(
            "知识源 ID、标识与项目归属不匹配，不能覆盖其他来源".to_string(),
        ));
    }
    if existing_by_key.is_some_and(|source| source.id != id) {
        return Err(AppError::InvalidInput(
            "知识源 ID 与知识源标识不匹配，不能覆盖其他来源".to_string(),
        ));
    }
    Ok(Some(id))
}

fn get_knowledge_project(conn: &Connection, id: i64) -> Result<KnowledgeProject, AppError> {
    conn.query_row(
        "SELECT id, project_key, name, aliases_json, description, git_workspace_key,
                git_workspace_keys_json, default_branch, enabled, created_at, updated_at, deleted_at
         FROM knowledge_projects WHERE id = ?1",
        [id],
        map_knowledge_project,
    )
    .map_err(AppError::from)
}

fn map_knowledge_project(row: &Row<'_>) -> rusqlite::Result<KnowledgeProject> {
    let aliases_json = row.get::<_, String>(3)?;
    let git_workspace_key = row.get::<_, String>(5)?;
    let git_workspace_keys_json = row.get::<_, String>(6)?;
    let git_workspace_keys = serde_json::from_str(&git_workspace_keys_json).unwrap_or_else(|_| {
        if git_workspace_key.trim().is_empty() {
            Vec::new()
        } else {
            vec![git_workspace_key.clone()]
        }
    });
    Ok(KnowledgeProject {
        id: row.get(0)?,
        project_key: row.get(1)?,
        name: row.get(2)?,
        aliases: serde_json::from_str(&aliases_json).unwrap_or_default(),
        description: row.get(4)?,
        git_workspace_keys,
        git_workspace_key,
        default_branch: row.get(7)?,
        enabled: row.get::<_, i64>(8)? != 0,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        deleted_at: row.get(11)?,
    })
}

fn get_knowledge_release(conn: &Connection, id: i64) -> Result<KnowledgeRelease, AppError> {
    conn.query_row(
        "SELECT id, project_id, version, tag_name, branch, commit_sha, description,
                released_at, created_at, updated_at, deleted_at
         FROM knowledge_releases WHERE id = ?1",
        [id],
        map_knowledge_release,
    )
    .map_err(AppError::from)
}

/// 新版多仓库清单以 release_id 为历史证据主键；一旦存在清单，旧发布 CRUD 不得改写或
/// 隐藏该 release，避免文档、分析与检索失去可复核的 Commit 作用域。
fn knowledge_release_has_repository_manifest(conn: &Connection, id: i64) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM knowledge_release_repository_manifests WHERE release_id = ?1
         )",
        [id],
        |row| row.get(0),
    )
    .map_err(AppError::from)
}

fn map_knowledge_release(row: &Row<'_>) -> rusqlite::Result<KnowledgeRelease> {
    Ok(KnowledgeRelease {
        id: row.get(0)?,
        project_id: row.get(1)?,
        version: row.get(2)?,
        tag_name: row.get(3)?,
        branch: row.get(4)?,
        commit_sha: row.get(5)?,
        description: row.get(6)?,
        released_at: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        deleted_at: row.get(10)?,
    })
}

fn get_knowledge_source(conn: &Connection, id: i64) -> Result<KnowledgeSource, AppError> {
    conn.query_row(
        "SELECT id, source_key, project_id, source_type, display_name, root_path,
                git_workspace_key, include_globs_json, exclude_globs_json,
                version_strategy, sync_mode, allow_remote_embedding, enabled,
                last_commit_sha, last_sync_status, last_synced_at, last_error,
                created_at, updated_at, deleted_at
         FROM knowledge_sources WHERE id = ?1",
        [id],
        map_knowledge_source,
    )
    .map_err(AppError::from)
}

fn map_knowledge_source(row: &Row<'_>) -> rusqlite::Result<KnowledgeSource> {
    let include_globs_json = row.get::<_, String>(7)?;
    let exclude_globs_json = row.get::<_, String>(8)?;
    Ok(KnowledgeSource {
        id: row.get(0)?,
        source_key: row.get(1)?,
        project_id: row.get(2)?,
        source_type: row.get(3)?,
        display_name: row.get(4)?,
        root_path: row.get(5)?,
        git_workspace_key: row.get(6)?,
        include_globs: serde_json::from_str(&include_globs_json).unwrap_or_default(),
        exclude_globs: serde_json::from_str(&exclude_globs_json).unwrap_or_default(),
        version_strategy: row.get(9)?,
        sync_mode: row.get(10)?,
        allow_remote_embedding: row.get::<_, i64>(11)? != 0,
        enabled: row.get::<_, i64>(12)? != 0,
        last_commit_sha: row.get(13)?,
        last_sync_status: row.get(14)?,
        last_synced_at: row.get(15)?,
        last_error: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
        deleted_at: row.get(19)?,
    })
}

fn get_knowledge_document(conn: &Connection, id: i64) -> Result<KnowledgeDocument, AppError> {
    conn.query_row(
        "SELECT d.id, d.document_key, d.project_id, d.source_id, d.doc_type, d.title, d.logical_path,
                d.status, d.sensitivity, d.tags_json, d.latest_version_id, d.allow_ai, d.allow_mcp,
                d.created_at, d.updated_at, d.deleted_at,
                (SELECT upload.source_folder_name
                 FROM knowledge_document_uploads upload
                 WHERE upload.document_id = d.id
                 ORDER BY upload.id DESC LIMIT 1)
         FROM knowledge_documents d WHERE d.id = ?1",
        [id],
        map_knowledge_document,
    )
    .map_err(AppError::from)
}

fn map_knowledge_document(row: &Row<'_>) -> rusqlite::Result<KnowledgeDocument> {
    let tags_json = row.get::<_, String>(9)?;
    Ok(KnowledgeDocument {
        id: row.get(0)?,
        document_key: row.get(1)?,
        project_id: row.get(2)?,
        source_id: row.get(3)?,
        doc_type: row.get(4)?,
        title: row.get(5)?,
        logical_path: row.get(6)?,
        source_folder_name: row.get(16)?,
        status: row.get(7)?,
        sensitivity: row.get(8)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        latest_version_id: row.get(10)?,
        allow_ai: row.get::<_, i64>(11)? != 0,
        allow_mcp: row.get::<_, i64>(12)? != 0,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        deleted_at: row.get(15)?,
    })
}

fn get_knowledge_document_version(
    conn: &Connection,
    id: i64,
) -> Result<KnowledgeDocumentVersion, AppError> {
    conn.query_row(
        "SELECT id, document_id, release_id, version_label, git_branch, commit_sha,
                source_path, mime_type, content, content_hash, parsed_meta_json,
                token_estimate, valid, created_at
         FROM knowledge_document_versions WHERE id = ?1",
        [id],
        map_knowledge_document_version,
    )
    .map_err(AppError::from)
}

fn map_knowledge_document_version(row: &Row<'_>) -> rusqlite::Result<KnowledgeDocumentVersion> {
    let parsed_meta_json = row.get::<_, String>(10)?;
    Ok(KnowledgeDocumentVersion {
        id: row.get(0)?,
        document_id: row.get(1)?,
        release_id: row.get(2)?,
        version_label: row.get(3)?,
        git_branch: row.get(4)?,
        commit_sha: row.get(5)?,
        source_path: row.get(6)?,
        mime_type: row.get(7)?,
        content: row.get(8)?,
        content_hash: row.get(9)?,
        parsed_meta: serde_json::from_str(&parsed_meta_json).unwrap_or_default(),
        token_estimate: row.get(11)?,
        valid: row.get::<_, i64>(12)? != 0,
        created_at: row.get(13)?,
    })
}

fn map_knowledge_chunk(row: &Row<'_>) -> rusqlite::Result<KnowledgeChunk> {
    let location_json = row.get::<_, String>(6)?;
    Ok(KnowledgeChunk {
        id: row.get(0)?,
        document_version_id: row.get(1)?,
        chunk_index: row.get(2)?,
        heading_path: row.get(3)?,
        content: row.get(4)?,
        content_hash: row.get(5)?,
        location: serde_json::from_str(&location_json).unwrap_or_default(),
        token_estimate: row.get(7)?,
        embedding_status: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn get_knowledge_retrieval_evaluation_run(
    conn: &Connection,
    id: i64,
) -> Result<KnowledgeRetrievalEvaluationRun, AppError> {
    conn.query_row(
        "SELECT id, fixture_version, profile_id, top_k, case_count, recall_at_k, mrr,
                citation_accuracy, version_leakage_rate, refusal_accuracy, p50_latency_ms,
                p95_latency_ms, details_json, created_at
         FROM knowledge_retrieval_evaluation_runs WHERE id = ?1",
        [id],
        map_knowledge_retrieval_evaluation_run,
    )
    .map_err(AppError::from)
}

fn map_knowledge_retrieval_evaluation_run(
    row: &Row<'_>,
) -> rusqlite::Result<KnowledgeRetrievalEvaluationRun> {
    let details_json: String = row.get(12)?;
    Ok(KnowledgeRetrievalEvaluationRun {
        id: row.get(0)?,
        fixture_version: row.get(1)?,
        profile_id: row.get(2)?,
        top_k: row.get(3)?,
        case_count: row.get(4)?,
        recall_at_k: row.get(5)?,
        mrr: row.get(6)?,
        citation_accuracy: row.get(7)?,
        version_leakage_rate: row.get(8)?,
        refusal_accuracy: row.get(9)?,
        p50_latency_ms: row.get(10)?,
        p95_latency_ms: row.get(11)?,
        details: serde_json::from_str(&details_json).unwrap_or_else(|_| serde_json::json!([])),
        created_at: row.get(13)?,
    })
}

fn map_chunk_embedding(row: &Row<'_>) -> rusqlite::Result<KnowledgeChunkEmbedding> {
    Ok(KnowledgeChunkEmbedding {
        chunk_id: row.get(0)?,
        profile_id: row.get(1)?,
        dimension: row.get(2)?,
        vector_norm: row.get(3)?,
        content_hash: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn get_knowledge_relation(conn: &Connection, id: i64) -> Result<KnowledgeRelation, AppError> {
    conn.query_row(
        "SELECT id, project_id, release_id, document_version_id, snapshot_id, sensitivity,
                scope_status, from_type, from_key, relation_type, to_type, to_key, evidence_json,
                confidence, confirmed, source, created_at, updated_at, deleted_at
         FROM knowledge_relations WHERE id = ?1 AND deleted_at IS NULL",
        [id],
        map_knowledge_relation,
    )
    .map_err(AppError::from)
}

const ZENTAO_CONNECTION_SELECT: &str =
    "SELECT id, connection_key, name, base_url, api_version, auth_mode, endpoint_profile,
        credential_key, tls_verify, allow_insecure_http, request_timeout_seconds, page_size, rate_limit_per_second,
        capabilities_json, enabled, last_test_status, last_tested_at, last_error, created_at,
        updated_at, deleted_at FROM zentao_connections WHERE deleted_at IS NULL ORDER BY name, id";

fn get_zentao_connection(conn: &Connection, id: i64) -> Result<ZentaoConnection, AppError> {
    conn.query_row(
        "SELECT id, connection_key, name, base_url, api_version, auth_mode, endpoint_profile,
            credential_key, tls_verify, allow_insecure_http, request_timeout_seconds, page_size, rate_limit_per_second,
            capabilities_json, enabled, last_test_status, last_tested_at, last_error, created_at,
            updated_at, deleted_at FROM zentao_connections WHERE id = ?1 AND deleted_at IS NULL",
        [id],
        map_zentao_connection,
    )
    .map_err(AppError::from)
}

fn map_zentao_connection(row: &Row<'_>) -> rusqlite::Result<ZentaoConnection> {
    let credential_key: String = row.get(7)?;
    let capabilities_json: String = row.get(13)?;
    Ok(ZentaoConnection {
        id: row.get(0)?,
        connection_key: row.get(1)?,
        name: row.get(2)?,
        base_url: row.get(3)?,
        api_version: row.get(4)?,
        auth_mode: row.get(5)?,
        endpoint_profile: row.get(6)?,
        credential_configured: !credential_key.trim().is_empty(),
        credential_key,
        tls_verify: row.get::<_, i64>(8)? != 0,
        allow_insecure_http: row.get::<_, i64>(9)? != 0,
        request_timeout_seconds: row.get(10)?,
        page_size: row.get(11)?,
        rate_limit_per_second: row.get(12)?,
        capabilities: serde_json::from_str(&capabilities_json)
            .unwrap_or_else(|_| serde_json::json!({})),
        enabled: row.get::<_, i64>(14)? != 0,
        last_test_status: row.get(15)?,
        last_tested_at: row.get(16)?,
        last_error: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
        deleted_at: row.get(20)?,
    })
}

fn get_zentao_project_mapping(
    conn: &Connection,
    id: i64,
) -> Result<ZentaoProjectMapping, AppError> {
    conn.query_row(
        "SELECT id, connection_id, knowledge_project_id, remote_product_id, remote_project_id,
            remote_execution_ids_json, release_mapping_json, sync_scope_json, sync_since,
            include_comments, include_worklogs, include_attachment_metadata, allow_remote_embedding,
            allow_remote_ai, enabled, created_at, updated_at, deleted_at
         FROM zentao_project_mappings WHERE id = ?1 AND deleted_at IS NULL",
        [id],
        map_zentao_project_mapping,
    )
    .map_err(AppError::from)
}

fn ensure_zentao_mapping_sync_active(conn: &Connection, mapping_id: i64) -> Result<(), AppError> {
    let active = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM zentao_project_mappings m
            JOIN zentao_connections c ON c.id = m.connection_id
            JOIN knowledge_projects p ON p.id = m.knowledge_project_id
            WHERE m.id = ?1 AND m.enabled = 1 AND m.deleted_at IS NULL
              AND c.enabled = 1 AND c.deleted_at IS NULL
              AND p.enabled = 1 AND p.deleted_at IS NULL
        )",
        [mapping_id],
        |row| row.get::<_, bool>(0),
    )?;
    if !active {
        return Err(AppError::InvalidInput(
            "禅道同步已停止：映射、连接或知识项目已禁用或删除".to_string(),
        ));
    }
    Ok(())
}

fn get_zentao_sync_cursor(
    conn: &Connection,
    mapping_id: i64,
    entity_type: &str,
) -> Result<ZentaoSyncCursor, AppError> {
    conn.query_row(
        "SELECT id, mapping_id, entity_type, last_updated_at, last_external_id, checkpoint_json,
                last_success_at, last_full_sync_at, updated_at
         FROM zentao_sync_cursors WHERE mapping_id = ?1 AND entity_type = ?2",
        params![mapping_id, entity_type],
        map_zentao_sync_cursor,
    )
    .map_err(AppError::from)
}

fn map_zentao_sync_cursor(row: &Row<'_>) -> rusqlite::Result<ZentaoSyncCursor> {
    let checkpoint_json: String = row.get(5)?;
    Ok(ZentaoSyncCursor {
        id: row.get(0)?,
        mapping_id: row.get(1)?,
        entity_type: row.get(2)?,
        last_updated_at: row.get(3)?,
        last_external_id: row.get(4)?,
        checkpoint: serde_json::from_str(&checkpoint_json)
            .unwrap_or_else(|_| serde_json::json!({})),
        last_success_at: row.get(6)?,
        last_full_sync_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn get_zentao_entity(
    conn: &Connection,
    connection_id: i64,
    entity_type: &str,
    external_id: &str,
) -> Result<ZentaoEntity, AppError> {
    conn.query_row(
        "SELECT id, connection_id, mapping_id, knowledge_project_id, release_id, entity_type,
                external_id, external_key, title, body_markdown, original_status,
                normalized_status, assignee_external_id, parent_external_key, remote_url,
                content_hash, raw_json_hash, raw_snapshot_json, source_created_at,
                source_updated_at, first_synced_at, last_synced_at, missing_count, status, deleted_at
         FROM zentao_entities WHERE connection_id = ?1 AND entity_type = ?2 AND external_id = ?3",
        params![connection_id, entity_type, external_id],
        map_zentao_entity,
    )
    .map_err(AppError::from)
}

fn map_zentao_entity(row: &Row<'_>) -> rusqlite::Result<ZentaoEntity> {
    let raw_snapshot_json: Option<String> = row.get(17)?;
    Ok(ZentaoEntity {
        id: row.get(0)?,
        connection_id: row.get(1)?,
        mapping_id: row.get(2)?,
        knowledge_project_id: row.get(3)?,
        release_id: row.get(4)?,
        entity_type: row.get(5)?,
        external_id: row.get(6)?,
        external_key: row.get(7)?,
        title: row.get(8)?,
        body_markdown: row.get(9)?,
        original_status: row.get(10)?,
        normalized_status: row.get(11)?,
        assignee_external_id: row.get(12)?,
        parent_external_key: row.get(13)?,
        remote_url: row.get(14)?,
        content_hash: row.get(15)?,
        raw_json_hash: row.get(16)?,
        raw_snapshot: raw_snapshot_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok()),
        source_created_at: row.get(18)?,
        source_updated_at: row.get(19)?,
        first_synced_at: row.get(20)?,
        last_synced_at: row.get(21)?,
        missing_count: row.get(22)?,
        status: row.get(23)?,
        deleted_at: row.get(24)?,
    })
}

fn map_zentao_entity_relation(row: &Row<'_>) -> rusqlite::Result<ZentaoEntityRelation> {
    let evidence_json: String = row.get(4)?;
    Ok(ZentaoEntityRelation {
        id: row.get(0)?,
        from_external_key: row.get(1)?,
        relation_type: row.get(2)?,
        to_external_key: row.get(3)?,
        evidence: serde_json::from_str(&evidence_json).unwrap_or_else(|_| serde_json::json!({})),
        source: row.get(5)?,
        confidence: row.get(6)?,
        confirmed: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        deleted_at: row.get(10)?,
    })
}

fn map_zentao_project_mapping(row: &Row<'_>) -> rusqlite::Result<ZentaoProjectMapping> {
    let execution_json: String = row.get(5)?;
    let release_mapping_json: String = row.get(6)?;
    let sync_scope_json: String = row.get(7)?;
    Ok(ZentaoProjectMapping {
        id: row.get(0)?,
        connection_id: row.get(1)?,
        knowledge_project_id: row.get(2)?,
        remote_product_id: row.get(3)?,
        remote_project_id: row.get(4)?,
        remote_execution_ids: serde_json::from_str(&execution_json).unwrap_or_default(),
        release_mapping: serde_json::from_str(&release_mapping_json)
            .unwrap_or_else(|_| serde_json::json!({})),
        sync_scope: serde_json::from_str(&sync_scope_json)
            .unwrap_or_else(|_| serde_json::json!({})),
        sync_since: row.get(8)?,
        include_comments: row.get::<_, i64>(9)? != 0,
        include_worklogs: row.get::<_, i64>(10)? != 0,
        include_attachment_metadata: row.get::<_, i64>(11)? != 0,
        allow_remote_embedding: row.get::<_, i64>(12)? != 0,
        allow_remote_ai: row.get::<_, i64>(13)? != 0,
        enabled: row.get::<_, i64>(14)? != 0,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        deleted_at: row.get(17)?,
    })
}

fn map_knowledge_relation(row: &Row<'_>) -> rusqlite::Result<KnowledgeRelation> {
    let evidence_json = row.get::<_, String>(12)?;
    Ok(KnowledgeRelation {
        id: row.get(0)?,
        project_id: positive_id_or_none(row.get(1)?),
        release_id: positive_id_or_none(row.get(2)?),
        document_version_id: positive_id_or_none(row.get(3)?),
        snapshot_id: positive_id_or_none(row.get(4)?),
        sensitivity: row.get(5)?,
        scope_status: row.get(6)?,
        from_type: row.get(7)?,
        from_key: row.get(8)?,
        relation_type: row.get(9)?,
        to_type: row.get(10)?,
        to_key: row.get(11)?,
        evidence: serde_json::from_str(&evidence_json).unwrap_or_default(),
        confidence: row.get(13)?,
        confirmed: row.get::<_, i64>(14)? != 0,
        source: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        deleted_at: row.get(18)?,
    })
}

fn positive_id_or_none(id: i64) -> Option<i64> {
    (id > 0).then_some(id)
}

fn get_knowledge_job_by_id(conn: &Connection, id: i64) -> Result<KnowledgeJob, AppError> {
    conn.query_row(
        "SELECT id, job_key, job_type, source_id, profile_id, status,
                progress_current, progress_total, message, error,
                checkpoint_json, heartbeat_at, cancel_requested,
                started_at, finished_at
         FROM knowledge_jobs WHERE id = ?1",
        [id],
        map_knowledge_job,
    )
    .map_err(AppError::from)
}

fn map_knowledge_job(row: &Row<'_>) -> rusqlite::Result<KnowledgeJob> {
    let checkpoint_json = row.get::<_, String>(10)?;
    Ok(KnowledgeJob {
        id: row.get(0)?,
        job_key: row.get(1)?,
        job_type: row.get(2)?,
        source_id: row.get(3)?,
        profile_id: row.get(4)?,
        status: row.get(5)?,
        progress_current: row.get(6)?,
        progress_total: row.get(7)?,
        message: row.get(8)?,
        error: row.get(9)?,
        checkpoint: serde_json::from_str(&checkpoint_json).unwrap_or_default(),
        heartbeat_at: row.get(11)?,
        cancel_requested: row.get::<_, i64>(12)? != 0,
        started_at: row.get(13)?,
        finished_at: row.get(14)?,
    })
}

fn processing_status(
    document: &KnowledgeDocument,
    task: Option<&KnowledgeDocumentProcessingTaskSummary>,
    parser: Option<&KnowledgeDocumentParseSummary>,
) -> String {
    if let Some(task) = task {
        match task.status.as_str() {
            // 已提交手工文档即使索引仍在排队，正文也已正式可读；只有尚无正式版本的
            // processing 文档才把排队/运行任务展示为“处理中”。
            "queued" | "running" if document.status == "processing" => {
                return "processing".to_string()
            }
            "failed" => return "failed".to_string(),
            "cancelled" => return "cancelled".to_string(),
            "interrupted" => return "interrupted".to_string(),
            _ => {}
        }
    }
    if let Some(parser) = parser {
        match parser.quality_level.as_str() {
            "failed" => return "failed".to_string(),
            "partial" => return "partial".to_string(),
            _ => {}
        }
    }
    document.status.clone()
}

fn processing_message(
    status: &str,
    task: Option<&KnowledgeDocumentProcessingTaskSummary>,
    parser: Option<&KnowledgeDocumentParseSummary>,
) -> String {
    match status {
        "processing" => task
            .map(|item| item.message.clone())
            .filter(|message| !message.trim().is_empty())
            .unwrap_or_else(|| "文档正在处理，请稍后查看".to_string()),
        "partial" => {
            let warning_count = parser.map_or(0, |item| item.warnings.len());
            if warning_count > 0 {
                format!("文档已部分解析，另有 {warning_count} 项需要注意")
            } else {
                "文档已部分解析，可查看已提取内容".to_string()
            }
        }
        "failed" => "文档处理失败，可重新尝试；不会显示不完整正文".to_string(),
        "cancelled" => "文档处理已取消，可重新尝试".to_string(),
        "interrupted" => "文档处理已中断，可从检查点重新尝试".to_string(),
        _ => "文档内容已可查看".to_string(),
    }
}

/// 详情页只返回可行动的安全原因，不透出绝对路径、检查点或任意底层错误文本。
fn processing_failure_reason(status: &str, task_error: Option<&str>) -> Option<String> {
    if status != "failed" {
        return None;
    }
    let error = task_error.unwrap_or_default();
    if let Some(detail) = error
        .strip_prefix("参数无效: Markdown front matter 解析失败: ")
        .or_else(|| error.strip_prefix("Markdown front matter 解析失败: "))
    {
        return Some(format!(
            "Markdown Front Matter 格式不正确：{detail}。Markdown 链接等特殊值请使用引号。"
        ));
    }
    if error.contains("Markdown front matter 缺少结束分隔符") {
        return Some("Markdown Front Matter 缺少结束分隔符 `---`。".to_string());
    }
    Some("文档处理失败，请检查文件格式后重新处理。".to_string())
}

fn processing_actions(
    status: &str,
    content_available: bool,
    task: Option<&KnowledgeDocumentProcessingTaskSummary>,
) -> Vec<String> {
    match status {
        "processing" if task.is_some_and(|item| !item.cancel_requested) => {
            vec!["取消处理".to_string()]
        }
        "failed" | "cancelled" | "interrupted" => vec!["重新尝试".to_string()],
        "partial" if content_available => {
            vec!["查看已解析内容".to_string(), "重新处理".to_string()]
        }
        "partial" => vec!["重新处理".to_string()],
        _ if content_available => vec!["查看内容".to_string()],
        _ => Vec::new(),
    }
}

fn get_knowledge_code_source_settings(
    conn: &Connection,
    source_id: i64,
) -> Result<KnowledgeCodeSourceSettings, AppError> {
    conn.query_row(
        "SELECT source_id, include_untracked, max_file_size_bytes, allowed_languages_json,
                allow_remote_processing, created_at, updated_at
         FROM knowledge_code_source_settings WHERE source_id = ?1",
        [source_id],
        |row| {
            let allowed_languages_json = row.get::<_, String>(3)?;
            Ok(KnowledgeCodeSourceSettings {
                source_id: row.get(0)?,
                include_untracked: row.get::<_, i64>(1)? != 0,
                max_file_size_bytes: row.get(2)?,
                allowed_languages: serde_json::from_str(&allowed_languages_json)
                    .unwrap_or_default(),
                allow_remote_processing: row.get::<_, i64>(4)? != 0,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .map_err(AppError::from)
}

fn get_knowledge_code_snapshot(
    conn: &Connection,
    id: i64,
) -> Result<KnowledgeCodeSnapshot, AppError> {
    conn.query_row(
        "SELECT id, snapshot_key, source_id, project_id, release_id, snapshot_type,
                ref_name, commit_sha, base_commit_sha, branch_name, worktree_dirty,
                dirty_state_json, captured_at, file_count, symbol_count, relation_count,
                analyzer_version, status, error, created_at, updated_at
         FROM knowledge_code_snapshots WHERE id = ?1",
        [id],
        map_knowledge_code_snapshot,
    )
    .map_err(AppError::from)
}

fn map_knowledge_code_snapshot(row: &Row<'_>) -> rusqlite::Result<KnowledgeCodeSnapshot> {
    Ok(KnowledgeCodeSnapshot {
        id: row.get(0)?,
        snapshot_key: row.get(1)?,
        source_id: row.get(2)?,
        project_id: row.get(3)?,
        release_id: row.get(4)?,
        snapshot_type: row.get(5)?,
        ref_name: row.get(6)?,
        commit_sha: row.get(7)?,
        base_commit_sha: row.get(8)?,
        branch_name: row.get(9)?,
        worktree_dirty: row.get::<_, i64>(10)? != 0,
        dirty_state: serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or_default(),
        captured_at: row.get(12)?,
        file_count: row.get(13)?,
        symbol_count: row.get(14)?,
        relation_count: row.get(15)?,
        analyzer_version: row.get(16)?,
        status: row.get(17)?,
        error: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
    })
}

fn map_knowledge_code_file(row: &Row<'_>) -> rusqlite::Result<KnowledgeCodeFile> {
    Ok(KnowledgeCodeFile {
        id: row.get(0)?,
        snapshot_id: row.get(1)?,
        document_version_id: row.get(2)?,
        relative_path: row.get(3)?,
        language: row.get(4)?,
        file_size: row.get(5)?,
        content_hash: row.get(6)?,
        analysis_level: row.get(7)?,
        is_generated: row.get::<_, i64>(8)? != 0,
        is_test: row.get::<_, i64>(9)? != 0,
        sensitivity: row.get(10)?,
        status: row.get(11)?,
        skip_reason: row.get(12)?,
        created_at: row.get(13)?,
    })
}

fn map_knowledge_code_symbol(row: &Row<'_>) -> rusqlite::Result<KnowledgeCodeSymbol> {
    Ok(KnowledgeCodeSymbol {
        id: row.get(0)?,
        snapshot_id: row.get(1)?,
        file_id: row.get(2)?,
        symbol_key: row.get(3)?,
        symbol_kind: row.get(4)?,
        name: row.get(5)?,
        qualified_name: row.get(6)?,
        signature: row.get(7)?,
        visibility: row.get(8)?,
        parent_symbol_key: row.get(9)?,
        start_line: row.get(10)?,
        start_column: row.get(11)?,
        end_line: row.get(12)?,
        end_column: row.get(13)?,
        doc_comment: row.get(14)?,
        content_hash: row.get(15)?,
        analysis_level: row.get(16)?,
        created_at: row.get(17)?,
    })
}

fn map_knowledge_code_relation(row: &Row<'_>) -> rusqlite::Result<KnowledgeCodeRelation> {
    Ok(KnowledgeCodeRelation {
        id: row.get(0)?,
        snapshot_id: row.get(1)?,
        from_symbol_key: row.get(2)?,
        relation_type: row.get(3)?,
        to_symbol_key: row.get(4)?,
        to_external_type: row.get(5)?,
        to_external_key: row.get(6)?,
        evidence_file_id: row.get(7)?,
        evidence_start_line: row.get(8)?,
        evidence_end_line: row.get(9)?,
        evidence_text: row.get(10)?,
        resolver: row.get(11)?,
        confidence: row.get(12)?,
        confirmed: row.get::<_, i64>(13)? != 0,
        created_at: row.get(14)?,
    })
}

fn get_knowledge_code_relation(
    conn: &Connection,
    id: i64,
) -> Result<KnowledgeCodeRelation, AppError> {
    conn.query_row(
        "SELECT id, snapshot_id, from_symbol_key, relation_type, to_symbol_key,
                to_external_type, to_external_key, evidence_file_id, evidence_start_line,
                evidence_end_line, evidence_text, resolver, confidence, confirmed, created_at
         FROM knowledge_code_relations WHERE id = ?1",
        [id],
        map_knowledge_code_relation,
    )
    .map_err(AppError::from)
}

fn get_embedding_profile(
    conn: &Connection,
    id: i64,
) -> Result<KnowledgeEmbeddingProfile, AppError> {
    conn.query_row(
        "SELECT id, profile_key, name, mode, provider_key, model, model_revision,
                dimension, normalized, config_json, fingerprint, status, is_active,
                created_at, updated_at
         FROM knowledge_embedding_profiles WHERE id = ?1",
        [id],
        |row| {
            let config_json = row.get::<_, String>(9)?;
            Ok(KnowledgeEmbeddingProfile {
                id: row.get(0)?,
                profile_key: row.get(1)?,
                name: row.get(2)?,
                mode: row.get(3)?,
                provider_key: row.get(4)?,
                model: row.get(5)?,
                model_revision: row.get(6)?,
                dimension: row.get(7)?,
                normalized: row.get::<_, i64>(8)? != 0,
                config: serde_json::from_str(&config_json).unwrap_or_default(),
                fingerprint: row.get(10)?,
                status: row.get(11)?,
                is_active: row.get::<_, i64>(12)? != 0,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        },
    )
    .map_err(AppError::from)
}

fn calculate_embedding_index_validation(
    conn: &Connection,
    profile: &KnowledgeEmbeddingProfile,
) -> Result<KnowledgeEmbeddingIndexValidation, AppError> {
    let expected_chunks = conn.query_row(
        "SELECT COUNT(*)
         FROM knowledge_chunks c
         JOIN knowledge_document_versions v ON v.id = c.document_version_id
         JOIN knowledge_documents d ON d.id = v.document_id
         WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
           AND TRIM(c.content, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)) <> ''",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let indexed_chunks = conn.query_row(
        "SELECT COUNT(*)
         FROM knowledge_chunks c
         JOIN knowledge_document_versions v ON v.id = c.document_version_id
         JOIN knowledge_documents d ON d.id = v.document_id
         JOIN knowledge_chunk_embeddings e ON e.chunk_id = c.id AND e.profile_id = ?1
         WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
           AND TRIM(c.content, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)) <> ''
           AND e.content_hash = c.content_hash AND e.dimension = ?2",
        params![profile.id, profile.dimension],
        |row| row.get::<_, i64>(0),
    )?;
    let stale_chunks = conn.query_row(
        "SELECT COUNT(*)
         FROM knowledge_chunks c
         JOIN knowledge_document_versions v ON v.id = c.document_version_id
         JOIN knowledge_documents d ON d.id = v.document_id
         JOIN knowledge_chunk_embeddings e ON e.chunk_id = c.id AND e.profile_id = ?1
         WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
           AND TRIM(c.content, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)) <> ''
           AND e.content_hash <> c.content_hash",
        [profile.id],
        |row| row.get::<_, i64>(0),
    )?;
    let dimension_mismatch_chunks = conn.query_row(
        "SELECT COUNT(*)
         FROM knowledge_chunks c
         JOIN knowledge_document_versions v ON v.id = c.document_version_id
         JOIN knowledge_documents d ON d.id = v.document_id
         JOIN knowledge_chunk_embeddings e ON e.chunk_id = c.id AND e.profile_id = ?1
         WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
           AND TRIM(c.content, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)) <> ''
           AND e.dimension <> ?2",
        params![profile.id, profile.dimension],
        |row| row.get::<_, i64>(0),
    )?;
    let invalid_vector_chunks = count_invalid_embedding_vectors(conn, profile)?;
    Ok(KnowledgeEmbeddingIndexValidation {
        profile_id: profile.id,
        profile_key: profile.profile_key.clone(),
        expected_chunks,
        indexed_chunks,
        stale_chunks,
        dimension_mismatch_chunks,
        invalid_vector_chunks,
        complete: expected_chunks == indexed_chunks
            && stale_chunks == 0
            && dimension_mismatch_chunks == 0
            && invalid_vector_chunks == 0,
    })
}

/// 校验实际存储的二进制向量，防止损坏 BLOB、非有限数或错误范数在激活后才暴露。
fn count_invalid_embedding_vectors(
    conn: &Connection,
    profile: &KnowledgeEmbeddingProfile,
) -> Result<i64, AppError> {
    let mut statement = conn.prepare(
        "SELECT e.dimension, e.vector_blob, e.vector_norm
         FROM knowledge_chunks c
         JOIN knowledge_document_versions v ON v.id = c.document_version_id
         JOIN knowledge_documents d ON d.id = v.document_id
         JOIN knowledge_chunk_embeddings e ON e.chunk_id = c.id AND e.profile_id = ?1
         WHERE v.valid = 1 AND d.deleted_at IS NULL AND d.status = 'active'
           AND TRIM(c.content, ' ' || char(9) || char(10) || char(11) || char(12) || char(13)) <> ''
           AND e.content_hash = c.content_hash AND e.dimension = ?2",
    )?;
    let rows = statement.query_map(params![profile.id, profile.dimension], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;
    let mut invalid = 0_i64;
    for row in rows {
        let (dimension, blob, stored_norm) = row?;
        let valid = decode_vector_blob(&blob, dimension)
            .ok()
            .filter(|vector| vector.iter().all(|value| value.is_finite()))
            .map(|vector| {
                let calculated_norm = vector_norm(&vector);
                stored_norm.is_finite()
                    && stored_norm > 0.0
                    && calculated_norm.is_finite()
                    && calculated_norm > 0.0
                    && (calculated_norm - stored_norm).abs()
                        <= 0.000_001_f64 * calculated_norm.max(1.0)
            })
            .unwrap_or(false);
        if !valid {
            invalid = invalid
                .checked_add(1)
                .ok_or_else(|| AppError::Custom("无效向量计数超出范围".into()))?;
        }
    }
    Ok(invalid)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::{params, Connection};

    use super::{decode_vector_blob, fts_query_from_text, processing_failure_reason, Database};
    use crate::database::schema;
    use crate::error::AppError;
    use crate::models::{
        CreateKnowledgeDocumentVersionInput, CreateKnowledgeJobInput, KnowledgeChunkWriteInput,
        KnowledgeListInput, KnowledgeSearchInput, KnowledgeVectorSearchInput,
        ListKnowledgeRelationsInput, UpsertKnowledgeCodeSourceInput, UpsertKnowledgeDocumentInput,
        UpsertKnowledgeEmbeddingProfileInput, UpsertKnowledgeProjectInput,
        UpsertKnowledgeRelationInput, UpsertKnowledgeReleaseInput, UpsertKnowledgeSourceInput,
        UpsertZentaoConnectionInput, UpsertZentaoEntityInput, UpsertZentaoProjectMappingInput,
        ZentaoSyncCursorUpdateInput,
    };
    use crate::services::knowledge_embedding::KnowledgeEmbeddingService;

    fn test_database() -> Result<Database, Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        schema::migrate(&conn)?;
        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    #[test]
    fn chinese_natural_language_question_uses_relaxed_trigram_terms() {
        let query = fts_query_from_text("全业务工单中，明日工作计划生成的逻辑是什么？");

        assert!(query.contains("\"明日工\""));
        assert!(query.contains("\"工作计\""));
        assert!(query.contains(" OR "));
        assert!(!query.contains("\"全业务工单中，明日工作计划生成的逻辑是什么？\""));
    }

    #[test]
    fn mixed_chinese_question_quotes_version_number_once() -> Result<(), Box<dyn std::error::Error>>
    {
        let query = fts_query_from_text("企业务工单 1.2.0 版本的需求是什么");

        assert!(query.contains("\"1.2.0\""));
        assert!(!query.contains("\"\"1.2.0\"\""));

        let connection = Connection::open_in_memory()?;
        connection.execute_batch(
            "CREATE VIRTUAL TABLE documents USING fts5(content, tokenize='trigram');
             INSERT INTO documents(content) VALUES ('企业务工单 1.2.0 版本需求');",
        )?;
        let hit_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM documents WHERE documents MATCH ?1",
            [query],
            |row| row.get(0),
        )?;
        assert_eq!(hit_count, 1);

        Ok(())
    }

    #[test]
    fn markdown_front_matter_failure_is_safe_and_actionable() {
        let reason = processing_failure_reason(
            "failed",
            Some(
                "参数无效: Markdown front matter 解析失败: did not find expected key at line 3 column 50",
            ),
        );

        assert_eq!(
            reason.as_deref(),
            Some(
                "Markdown Front Matter 格式不正确：did not find expected key at line 3 column 50。Markdown 链接等特殊值请使用引号。"
            )
        );
        assert_eq!(processing_failure_reason("active", None), None);
    }

    fn zentao_entity_input(
        connection_id: i64,
        mapping_id: i64,
        knowledge_project_id: i64,
        external_id: &str,
    ) -> UpsertZentaoEntityInput {
        UpsertZentaoEntityInput {
            connection_id,
            mapping_id,
            knowledge_project_id,
            release_id: None,
            entity_type: "stories".into(),
            external_id: external_id.into(),
            external_key: format!("zentao:{connection_id}:stories:{external_id}"),
            title: "需求 A".into(),
            body_markdown: "正文".into(),
            original_status: "active".into(),
            normalized_status: "active".into(),
            assignee_external_id: String::new(),
            parent_external_key: String::new(),
            remote_url: String::new(),
            content_hash: format!("content-{external_id}"),
            raw_json_hash: format!("raw-{external_id}"),
            raw_snapshot: Some(serde_json::json!({"id": external_id})),
            source_created_at: None,
            source_updated_at: Some("2026-07-31T10:00:00Z".into()),
        }
    }

    #[test]
    fn project_crud_uses_pagination_and_soft_deletion() -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "project-a".to_string(),
            name: "项目 A".to_string(),
            aliases: vec!["A 项目".to_string()],
            description: "测试项目".to_string(),
            git_workspace_keys: vec!["workspace-a".to_string()],
            git_workspace_key: "workspace-a".to_string(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        assert_eq!(project.git_workspace_keys, vec!["workspace-a"]);
        assert_eq!(project.git_workspace_key, "workspace-a");
        let page = database.list_knowledge_projects(&KnowledgeListInput {
            project_id: None,
            release_id: None,
            source_id: None,
            keyword: Some("A 项目".to_string()),
            status: Some("enabled".to_string()),
            offset: Some(0),
            limit: Some(20),
        })?;
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, project.id);
        assert_eq!(page.items[0].git_workspace_keys, vec!["workspace-a"]);

        database.soft_delete_knowledge_project(project.id)?;
        let page = database.list_knowledge_projects(&KnowledgeListInput {
            project_id: None,
            release_id: None,
            source_id: None,
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert!(page.items.is_empty());
        Ok(())
    }

    #[test]
    fn document_list_filters_by_bound_project_release_without_duplicates(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "document-release-project".to_string(),
            name: "文档版本项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let release_v1 = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "main".to_string(),
            commit_sha: "commit-v1".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let release_v2 = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v2.0.0".to_string(),
            tag_name: "v2.0.0".to_string(),
            branch: "main".to_string(),
            commit_sha: "commit-v2".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let create_document = |document_key: &str, title: &str| {
            database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: document_key.to_string(),
                project_id: Some(project.id),
                source_id: None,
                doc_type: "markdown".to_string(),
                title: title.to_string(),
                logical_path: format!("docs/{document_key}.md"),
                sensitivity: "internal".to_string(),
                tags: Vec::new(),
                allow_ai: true,
                allow_mcp: false,
            })
        };
        let create_version = |document_id: i64, release_id: i64, label: &str| {
            database.create_knowledge_document_version(
                &CreateKnowledgeDocumentVersionInput {
                    document_id,
                    release_id: Some(release_id),
                    version_label: label.to_string(),
                    git_branch: "main".to_string(),
                    commit_sha: format!("{label}-commit"),
                    source_path: format!("docs/{label}.md"),
                    mime_type: "text/markdown".to_string(),
                    content: format!("{label} 文档正文"),
                    content_hash: format!("{label}-hash"),
                    parsed_meta: serde_json::json!({}),
                    token_estimate: 1,
                },
                &[],
            )
        };
        let v1_document = create_document("guide-v1", "v1 使用说明")?;
        let v2_document = create_document("guide-v2", "v2 使用说明")?;
        create_version(v1_document.id, release_v1.id, "v1")?;
        create_version(v2_document.id, release_v2.id, "v2")?;

        let v1_page = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project.id),
            release_id: Some(release_v1.id),
            source_id: None,
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(v1_page.total, 1);
        assert_eq!(v1_page.items[0].id, v1_document.id);

        let v2_page = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project.id),
            release_id: Some(release_v2.id),
            source_id: None,
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(v2_page.total, 1);
        assert_eq!(v2_page.items[0].id, v2_document.id);

        database.soft_delete_knowledge_document(v1_document.id)?;
        let visible_after_delete = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project.id),
            release_id: Some(release_v1.id),
            source_id: None,
            keyword: None,
            status: None,
            offset: Some(0),
            limit: Some(20),
        })?;
        assert!(visible_after_delete.items.is_empty());
        let recycle_bin =
            database.list_deleted_visible_knowledge_documents(&KnowledgeListInput {
                project_id: Some(project.id),
                release_id: Some(release_v1.id),
                source_id: None,
                keyword: None,
                status: None,
                offset: Some(0),
                limit: Some(20),
            })?;
        assert_eq!(recycle_bin.total, 1);
        assert_eq!(recycle_bin.items[0].id, v1_document.id);
        assert!(database
            .get_knowledge_document_by_id(v1_document.id)?
            .is_none());
        Ok(())
    }

    #[test]
    fn project_wide_document_binding_is_visible_in_every_selected_release(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "project-wide-document".to_string(),
            name: "跨版本项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        let release_v1 = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "main".to_string(),
            commit_sha: "commit-v1".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let release_v2 = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v2.0.0".to_string(),
            tag_name: "v2.0.0".to_string(),
            branch: "main".to_string(),
            commit_sha: "commit-v2".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "shared-guide".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "通用部署说明".to_string(),
            logical_path: "docs/deployment.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "通用版本".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: "docs/deployment.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "通用部署步骤与回滚说明。".to_string(),
                content_hash: "shared-guide-hash".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "部署".to_string(),
                content: "通用部署步骤与回滚说明。".to_string(),
                content_hash: "shared-guide-chunk-hash".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_document_version_bindings
                    (document_version_id, release_id, repository_binding_id, cross_version_scope)
                 VALUES (?1, NULL, NULL, 'project_all_versions')",
                [version.id],
            )?;
            conn.execute(
                "INSERT INTO knowledge_embedding_profiles
                    (profile_key, name, mode, model, dimension, fingerprint, status)
                 VALUES ('shared-guide-profile', 'Shared Guide', 'local', 'model-a', 2,
                         'shared-guide-fingerprint', 'building')",
                [],
            )?;
        }
        let chunk = database.list_knowledge_chunks(version.id)?[0].clone();
        database.upsert_knowledge_chunk_embedding(chunk.id, 1, &chunk.content_hash, &[1.0, 0.0])?;
        database.complete_knowledge_embedding_profile_build(1)?;
        database.activate_knowledge_embedding_profile(1)?;

        for release_id in [release_v1.id, release_v2.id] {
            let documents = database.list_knowledge_documents(&KnowledgeListInput {
                project_id: Some(project.id),
                release_id: Some(release_id),
                source_id: None,
                keyword: None,
                status: None,
                offset: None,
                limit: None,
            })?;
            assert_eq!(
                documents.items.len(),
                1,
                "发布版本 {release_id} 应包含通用文档"
            );

            let filter = KnowledgeSearchInput {
                query: "通用部署说明".to_string(),
                project_ids: vec![project.id],
                release_ids: vec![release_id],
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: Vec::new(),
                snapshot_id: None,
                limit: Some(10),
                include_context: Some(true),
            };
            assert_eq!(
                database
                    .search_knowledge_document_title_hits(&filter)?
                    .len(),
                1
            );
            assert_eq!(database.search_knowledge_fts(&filter)?.len(), 1);
            assert_eq!(
                database
                    .list_active_knowledge_vector_candidates_filtered(10, &filter)?
                    .len(),
                1
            );
            assert_eq!(
                KnowledgeEmbeddingService::search_active_vectors(
                    &database,
                    KnowledgeVectorSearchInput {
                        query_vector: vec![1.0, 0.0],
                        filters: filter.clone(),
                    },
                )?
                .len(),
                1,
                "发布版本 {release_id} 的真实向量检索应包含通用文档"
            );
        }
        let unrelated_project =
            database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
                id: None,
                project_key: "unrelated-project-wide-document".to_string(),
                name: "无关项目".to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: String::new(),
                enabled: true,
            })?;
        let unrelated_release =
            database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
                id: None,
                project_id: unrelated_project.id,
                version: "v9.0.0".to_string(),
                tag_name: "v9.0.0".to_string(),
                branch: "main".to_string(),
                commit_sha: "unrelated-commit".to_string(),
                description: String::new(),
                released_at: None,
            })?;
        let unrelated_document =
            database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: "unrelated-shared-guide".to_string(),
                project_id: Some(unrelated_project.id),
                source_id: None,
                doc_type: "markdown".to_string(),
                title: "无关项目通用说明".to_string(),
                logical_path: "docs/unrelated.md".to_string(),
                sensitivity: "internal".to_string(),
                tags: Vec::new(),
                allow_ai: true,
                allow_mcp: false,
            })?;
        let unrelated_version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: unrelated_document.id,
                release_id: None,
                version_label: "通用版本".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: "docs/unrelated.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "无关项目正文".to_string(),
                content_hash: "unrelated-guide-hash".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[],
        )?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_document_version_bindings
                    (document_version_id, release_id, repository_binding_id, cross_version_scope)
                 VALUES (?1, NULL, NULL, 'project_all_versions')",
                [unrelated_version.id],
            )?;
        }
        let release_only_documents = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: None,
            release_id: Some(release_v1.id),
            source_id: None,
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(release_only_documents.items.len(), 1);
        assert_eq!(release_only_documents.items[0].id, document.id);

        // 版本范围既不能按 ID 跨项目串用，也不能因为 project_all_versions 绑定而放宽
        // 项目边界：这些断言会在旧的 FTS、标题和向量 SQL 上全部失败。
        let unrelated_filter = KnowledgeSearchInput {
            query: "通用部署说明".to_string(),
            project_ids: Vec::new(),
            release_ids: vec![unrelated_release.id],
            source_ids: Vec::new(),
            document_types: Vec::new(),
            sensitivities: Vec::new(),
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        };
        assert!(database
            .search_knowledge_document_title_hits(&unrelated_filter)?
            .is_empty());
        assert!(database.search_knowledge_fts(&unrelated_filter)?.is_empty());
        assert!(database
            .list_active_knowledge_vector_candidates_filtered(10, &unrelated_filter)?
            .is_empty());
        assert!(KnowledgeEmbeddingService::search_active_vectors(
            &database,
            KnowledgeVectorSearchInput {
                query_vector: vec![1.0, 0.0],
                filters: unrelated_filter.clone(),
            },
        )?
        .is_empty());
        let mismatched_project_page = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project.id),
            release_id: Some(unrelated_release.id),
            source_id: None,
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(mismatched_project_page.total, 0);
        assert!(mismatched_project_page.items.is_empty());
        Ok(())
    }

    #[test]
    fn atomic_source_batch_rejects_cross_project_conflicts_without_partial_writes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let project = |project_key: &str, name: &str| {
            database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
                id: None,
                project_key: project_key.to_string(),
                name: name.to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: "main".to_string(),
                enabled: true,
            })
        };
        let first_project = project("source-batch-first", "来源批量项目一")?;
        let second_project = project("source-batch-second", "来源批量项目二")?;
        let source_input = |source_key: &str, project_id: i64| UpsertKnowledgeSourceInput {
            id: None,
            source_key: source_key.to_string(),
            project_id: Some(project_id),
            source_type: "git_workspace".to_string(),
            display_name: source_key.to_string(),
            root_path: String::new(),
            git_workspace_key: source_key.to_string(),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            version_strategy: "unversioned".to_string(),
            sync_mode: "incremental".to_string(),
            allow_remote_embedding: false,
            enabled: true,
        };
        database.upsert_knowledge_source(&source_input("existing-source", first_project.id))?;

        let error = database
            .upsert_knowledge_sources_atomically(&[
                source_input("new-source", first_project.id),
                source_input("existing-source", second_project.id),
            ])
            .expect_err("冲突来源必须拒绝整批写入");

        assert!(error.to_string().contains("已被其他项目占用"));
        let sources = database.list_knowledge_sources(None)?;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_key, "existing-source");
        assert_eq!(sources[0].project_id, Some(first_project.id));
        Ok(())
    }

    #[test]
    fn atomic_source_batch_retries_stable_keys_and_rolls_back_scope_or_id_conflicts(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let project = |project_key: &str, name: &str| {
            database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
                id: None,
                project_key: project_key.to_string(),
                name: name.to_string(),
                aliases: Vec::new(),
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: "main".to_string(),
                enabled: true,
            })
        };
        let first_project = project("source-retry-first", "来源重试项目一")?;
        let second_project = project("source-retry-second", "来源重试项目二")?;
        let source_input = |source_key: &str, project_id: i64| UpsertKnowledgeSourceInput {
            id: None,
            source_key: source_key.to_string(),
            project_id: Some(project_id),
            source_type: "git_workspace".to_string(),
            display_name: source_key.to_string(),
            root_path: String::new(),
            git_workspace_key: source_key.to_string(),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            version_strategy: "unversioned".to_string(),
            sync_mode: "incremental".to_string(),
            allow_remote_embedding: false,
            enabled: true,
        };
        let first_request = vec![
            source_input("retry-source-one", first_project.id),
            source_input("retry-source-two", first_project.id),
        ];
        let first = database.upsert_knowledge_sources_atomically(&first_request)?;
        let retry = database.upsert_knowledge_sources_atomically(&first_request)?;
        assert_eq!(
            retry.iter().map(|source| source.id).collect::<Vec<_>>(),
            first.iter().map(|source| source.id).collect::<Vec<_>>(),
            "同项目、同 sourceKey 且未携带 ID 的重试必须复用原来源"
        );

        let cross_project_error = database
            .upsert_knowledge_sources_atomically(&[
                source_input("should-not-persist-cross-project", first_project.id),
                source_input("retry-source-one", second_project.id),
            ])
            .expect_err("不同项目不能借用已有 sourceKey");
        assert!(cross_project_error.to_string().contains("已被其他项目占用"));
        assert!(database
            .list_knowledge_sources(None)?
            .iter()
            .all(|source| source.source_key != "should-not-persist-cross-project"));

        let mut mismatched_id = source_input("different-source-key", first_project.id);
        mismatched_id.id = Some(first[0].id);
        let id_mismatch_error = database
            .upsert_knowledge_sources_atomically(&[
                source_input("should-not-persist-id-mismatch", first_project.id),
                mismatched_id,
            ])
            .expect_err("ID 与 sourceKey 不匹配不能覆盖已有来源");
        assert!(id_mismatch_error
            .to_string()
            .contains("ID、标识与项目归属不匹配"));

        let sources = database.list_knowledge_sources(None)?;
        assert_eq!(sources.len(), 2, "两次失败批量均必须完整回滚");
        assert_eq!(
            sources.iter().map(|source| source.id).collect::<Vec<_>>(),
            first.iter().map(|source| source.id).collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn zentao_connection_persists_only_credential_reference_and_masks_it_on_output(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let connection = database.upsert_zentao_connection(&UpsertZentaoConnectionInput {
            id: None,
            connection_key: "zentao-test".to_string(),
            name: "禅道测试".to_string(),
            base_url: "https://zentao.example.test/".to_string(),
            api_version: "auto".to_string(),
            auth_mode: "auto".to_string(),
            endpoint_profile: String::new(),
            credential_key: "credential-ref-only".to_string(),
            tls_verify: true,
            allow_insecure_http: false,
            request_timeout_seconds: 30,
            page_size: 100,
            rate_limit_per_second: 5.0,
            enabled: true,
        })?;
        assert!(connection.credential_configured);
        let serialized = serde_json::to_string(&connection)?;
        assert!(!serialized.contains("credential-ref-only"));
        assert!(!serialized.contains("credentialKey"));
        assert!(!connection.allow_insecure_http);
        database.soft_delete_zentao_connection(connection.id)?;
        assert!(database.list_zentao_connections()?.is_empty());
        Ok(())
    }

    #[test]
    fn zentao_mapping_requires_live_targets_and_is_disabled_with_its_owner(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "zentao-project".into(),
            name: "禅道知识项目".into(),
            aliases: vec![],
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".into(),
            enabled: true,
        })?;
        let connection = database.upsert_zentao_connection(&UpsertZentaoConnectionInput {
            id: None,
            connection_key: "zentao-mapping-a".into(),
            name: "禅道映射 A".into(),
            base_url: "https://zentao.example.test/".into(),
            api_version: "auto".into(),
            auth_mode: "auto".into(),
            endpoint_profile: String::new(),
            credential_key: "credential-a".into(),
            tls_verify: true,
            allow_insecure_http: false,
            request_timeout_seconds: 30,
            page_size: 100,
            rate_limit_per_second: 5.0,
            enabled: true,
        })?;
        let mapping_input = UpsertZentaoProjectMappingInput {
            id: None,
            connection_id: connection.id,
            knowledge_project_id: project.id,
            remote_product_id: "1".into(),
            remote_project_id: "2".into(),
            remote_execution_ids: vec![],
            release_mapping: serde_json::json!({}),
            sync_scope: serde_json::json!({}),
            sync_since: None,
            include_comments: false,
            include_worklogs: true,
            include_attachment_metadata: true,
            allow_remote_embedding: false,
            allow_remote_ai: false,
            enabled: true,
        };
        let mapping = database.upsert_zentao_project_mapping(&mapping_input)?;
        database.soft_delete_zentao_connection(connection.id)?;
        assert!(
            !database
                .get_zentao_project_mapping_by_id(mapping.id)?
                .expect("mapping should retain history")
                .enabled
        );
        assert!(database
            .upsert_zentao_project_mapping(&mapping_input)
            .is_err());
        let cursor_input = ZentaoSyncCursorUpdateInput {
            mapping_id: mapping.id,
            entity_type: "stories".into(),
            last_updated_at: String::new(),
            last_external_id: String::new(),
            checkpoint: serde_json::json!({"nextPage": 2}),
            completed_full_sync: false,
        };
        assert!(database
            .upsert_zentao_entity(&zentao_entity_input(
                connection.id,
                mapping.id,
                project.id,
                "101"
            ))
            .is_err());
        assert!(database.upsert_zentao_sync_cursor(&cursor_input).is_err());
        assert!(database
            .confirm_zentao_missing_entities(mapping.id, "stories", &[])
            .is_err());

        let second_connection =
            database.upsert_zentao_connection(&UpsertZentaoConnectionInput {
                id: None,
                connection_key: "zentao-mapping-b".into(),
                name: "禅道映射 B".into(),
                base_url: "https://zentao.example.test/".into(),
                api_version: "auto".into(),
                auth_mode: "auto".into(),
                endpoint_profile: String::new(),
                credential_key: "credential-b".into(),
                tls_verify: true,
                allow_insecure_http: false,
                request_timeout_seconds: 30,
                page_size: 100,
                rate_limit_per_second: 5.0,
                enabled: true,
            })?;
        let second_mapping =
            database.upsert_zentao_project_mapping(&UpsertZentaoProjectMappingInput {
                connection_id: second_connection.id,
                remote_project_id: "3".into(),
                ..mapping_input
            })?;
        database.soft_delete_knowledge_project(project.id)?;
        assert!(
            !database
                .get_zentao_project_mapping_by_id(second_mapping.id)?
                .expect("mapping should retain history")
                .enabled
        );
        let project_deleted_cursor = ZentaoSyncCursorUpdateInput {
            mapping_id: second_mapping.id,
            ..cursor_input
        };
        assert!(database
            .upsert_zentao_entity(&zentao_entity_input(
                second_connection.id,
                second_mapping.id,
                project.id,
                "102",
            ))
            .is_err());
        assert!(database
            .upsert_zentao_sync_cursor(&project_deleted_cursor)
            .is_err());
        assert!(database
            .confirm_zentao_missing_entities(second_mapping.id, "stories", &[])
            .is_err());
        Ok(())
    }

    #[test]
    fn zentao_entity_upsert_and_cursor_recovery_are_idempotent(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_projects (id, project_key, name, aliases_json, enabled)
                 VALUES (1, 'zentao-cursor-project', '游标项目', '[]', 1)",
                [],
            )?;
            conn.execute(
                "INSERT INTO zentao_connections (id, connection_key, name, base_url, credential_key, enabled)
                 VALUES (1, 'zentao-cursor-connection', '游标连接', 'https://zentao.example.test/', 'credential', 1)",
                [],
            )?;
            conn.execute(
                "INSERT INTO zentao_project_mappings (id, connection_id, knowledge_project_id, remote_project_id, enabled)
                 VALUES (1, 1, 1, '1', 1)",
                [],
            )?;
        }
        let input = zentao_entity_input(1, 1, 1, "101");
        assert!(database.upsert_zentao_entity(&input)?.1);
        assert!(!database.upsert_zentao_entity(&input)?.1);
        let checkpoint = database.upsert_zentao_sync_cursor(&ZentaoSyncCursorUpdateInput {
            mapping_id: 1,
            entity_type: "stories".into(),
            last_updated_at: "2026-07-31T10:00:00Z".into(),
            last_external_id: "101".into(),
            checkpoint: serde_json::json!({"nextPage": 2}),
            completed_full_sync: false,
        })?;
        assert!(checkpoint.last_success_at.is_none());
        let completed = database.upsert_zentao_sync_cursor(&ZentaoSyncCursorUpdateInput {
            checkpoint: serde_json::json!({"nextPage": 1}),
            completed_full_sync: true,
            ..ZentaoSyncCursorUpdateInput {
                mapping_id: 1,
                entity_type: "stories".into(),
                last_updated_at: "2026-07-31T10:00:00Z".into(),
                last_external_id: "101".into(),
                checkpoint: serde_json::json!({}),
                completed_full_sync: false,
            }
        })?;
        assert!(completed.last_success_at.is_some());
        assert_eq!(
            database.confirm_zentao_missing_entities(1, "stories", &[])?,
            1
        );
        assert_eq!(
            database.confirm_zentao_missing_entities(1, "stories", &[])?,
            1
        );
        let conn = database.conn.lock().map_err(|error| error.to_string())?;
        let status: String = conn.query_row(
            "SELECT status FROM zentao_entities WHERE external_key = 'zentao:1:stories:101'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(status, "deleted");
        Ok(())
    }

    #[test]
    fn document_version_chunks_and_fts_are_transactional() -> Result<(), Box<dyn std::error::Error>>
    {
        let database = test_database()?;
        database.ensure_knowledge_fts()?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "doc-a".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "requirement".to_string(),
            title: "退款审批需求".to_string(),
            logical_path: "requirements/refund.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: vec!["REQ-1042".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v2.3.1".to_string(),
                git_branch: "release/v2.3.1".to_string(),
                commit_sha: "abc123".to_string(),
                source_path: "requirements/refund.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# 退款审批\n支付项目增加退款审批，调用 API。".to_string(),
                content_hash: "hash-v1".to_string(),
                parsed_meta: serde_json::json!({"parser": "markdown-v1"}),
                token_estimate: 20,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "退款审批".to_string(),
                content: "支付项目增加退款审批，调用 API。".to_string(),
                content_hash: "chunk-hash".to_string(),
                location: serde_json::json!({"startLine": 2, "endLine": 2}),
                token_estimate: 10,
            }],
        )?;
        assert_eq!(database.list_knowledge_chunks(version.id)?.len(), 1);
        let original_chunk_id = database.list_knowledge_chunks(version.id)?[0].id;

        let conn = database.conn.lock().map_err(|error| error.to_string())?;
        let hits = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_chunks_fts
             WHERE knowledge_chunks_fts MATCH '退款审批'",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        assert_eq!(hits, 1);
        drop(conn);
        let fts_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "退款审批".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["requirement".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(fts_hits.len(), 1);
        assert_eq!(fts_hits[0].channels, vec!["fts"]);
        assert_eq!(fts_hits[0].citation.chunk_id, Some(original_chunk_id));
        assert!(fts_hits[0].content.contains("支付项目"));

        // FTS5 trigram 无法直接匹配少于三个字的中文，常用短词必须通过受限 LIKE
        // 回退命中正文，且仍复用相同的文档类型和敏感级别硬过滤。
        let short_cjk_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "退款".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["requirement".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(short_cjk_hits.len(), 1);
        assert_eq!(short_cjk_hits[0].citation.chunk_id, Some(original_chunk_id));
        assert!(short_cjk_hits[0].content.contains("退款审批"));

        let one_character_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "退".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["requirement".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(false),
        })?;
        assert_eq!(one_character_hits.len(), 1);
        assert_eq!(
            one_character_hits[0].citation.chunk_id,
            Some(original_chunk_id)
        );

        let api_only_document =
            database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: "api-only".to_string(),
                project_id: None,
                source_id: None,
                doc_type: "requirement".to_string(),
                title: "API 接口说明".to_string(),
                logical_path: "requirements/api.md".to_string(),
                sensitivity: "internal".to_string(),
                tags: Vec::new(),
                allow_ai: true,
                allow_mcp: false,
            })?;
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: api_only_document.id,
                release_id: None,
                version_label: "v2.3.1".to_string(),
                git_branch: "release/v2.3.1".to_string(),
                commit_sha: "def456".to_string(),
                source_path: "requirements/api.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# API\n仅记录 API 协议。".to_string(),
                content_hash: "api-only-v1".to_string(),
                parsed_meta: serde_json::json!({"parser": "markdown-v1"}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "API".to_string(),
                content: "仅记录 API 协议。".to_string(),
                content_hash: "api-only-chunk".to_string(),
                location: serde_json::json!({"startLine": 2, "endLine": 2}),
                token_estimate: 6,
            }],
        )?;
        let mixed_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "退款 API".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["requirement".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(mixed_hits.len(), 1);
        assert_eq!(mixed_hits[0].citation.chunk_id, Some(original_chunk_id));

        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute("DELETE FROM knowledge_chunks_fts", [])?;
        }
        assert!(
            database.ensure_knowledge_fts_ready_for_search().is_err(),
            "搜索不得在请求中同步回填不完整的全文索引"
        );
        assert_eq!(database.rebuild_knowledge_fts()?, 2);
        assert!(
            database
                .ensure_knowledge_fts_ready_for_search()?
                .fts5_available
        );

        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_embedding_profiles
                 (profile_key, name, mode, model, dimension, fingerprint, status)
                 VALUES ('replace-profile', 'Replace Profile', 'local', 'model-a', 3,
                         'replace-fingerprint', 'building')",
                [],
            )?;
        }
        database.upsert_knowledge_chunk_embedding(
            original_chunk_id,
            1,
            "chunk-hash",
            &[0.2, 0.4, 0.8],
        )?;
        let replacement = vec![
            KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "退款审批 > API".to_string(),
                content: "新增 POST /refund/approve。".to_string(),
                content_hash: "replacement-0".to_string(),
                location: serde_json::json!({"startLine": 3, "endLine": 3}),
                token_estimate: 8,
            },
            KnowledgeChunkWriteInput {
                chunk_index: 1,
                heading_path: "退款审批 > 数据库".to_string(),
                content: "写入 refund_approval 表。".to_string(),
                content_hash: "replacement-1".to_string(),
                location: serde_json::json!({"startLine": 4, "endLine": 4}),
                token_estimate: 8,
            },
        ];
        let replaced = database.replace_knowledge_document_chunks(
            version.id,
            &serde_json::json!({"parserId": "markdown-parser-v1"}),
            16,
            &replacement,
        )?;
        assert_eq!(replaced.len(), 2);
        assert!(database
            .get_knowledge_chunk_vector(original_chunk_id, 1)?
            .is_none());
        database.upsert_knowledge_chunk_embedding(
            replaced[0].id,
            1,
            "replacement-0",
            &[0.1, 0.3, 0.9],
        )?;
        let conn = database.conn.lock().map_err(|error| error.to_string())?;
        let replacement_hits = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_chunks_fts
             WHERE knowledge_chunks_fts MATCH 'approve'",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        assert_eq!(replacement_hits, 1);
        drop(conn);

        let duplicate_indexes = vec![replacement[0].clone(), replacement[0].clone()];
        let failed = database.replace_knowledge_document_chunks(
            version.id,
            &serde_json::json!({"parserId": "broken"}),
            16,
            &duplicate_indexes,
        );
        assert!(failed.is_err());
        assert_eq!(database.list_knowledge_chunks(version.id)?.len(), 2);

        let history_version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v2.3.2".to_string(),
                git_branch: "release/v2.3.2".to_string(),
                commit_sha: "def456".to_string(),
                source_path: "requirements/refund.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# 退款审批\n历史版本仍可检索。".to_string(),
                content_hash: "hash-v2".to_string(),
                parsed_meta: serde_json::json!({"parser": "markdown-v1"}),
                token_estimate: 12,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "退款审批 > 历史".to_string(),
                content: "历史版本仍可检索。".to_string(),
                content_hash: "history-chunk".to_string(),
                location: serde_json::json!({"startLine": 2, "endLine": 2}),
                token_estimate: 6,
            }],
        )?;
        let stale_full_text_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "refund_approval".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["requirement".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert!(stale_full_text_hits.is_empty());
        let title_hits = database.search_knowledge_document_title_hits(&KnowledgeSearchInput {
            query: "退款审批需求".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["requirement".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(title_hits.len(), 1);
        assert_eq!(
            title_hits[0].citation.document_version_id,
            Some(history_version.id)
        );
        assert_eq!(title_hits[0].channels, vec!["title"]);

        database.soft_delete_knowledge_document(document.id)?;
        let conn = database.conn.lock().map_err(|error| error.to_string())?;
        let hits_after_delete = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_chunks_fts
             WHERE knowledge_chunks_fts MATCH '退款审批'",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        assert_eq!(hits_after_delete, 0);
        drop(conn);

        let preview = database.preview_knowledge_document_deletion(document.id)?;
        assert_eq!(preview.version_count, 2);
        assert_eq!(preview.chunk_count, 3);
        assert_eq!(preview.vector_count, 1);
        assert_eq!(preview.fts_entry_count, 0);
        assert!(!preview.permanent_deletion_enabled);
        assert!(preview.permanent_deletion_block_reason.contains("尚未启用"));
        assert_eq!(
            database
                .list_knowledge_document_versions(document.id)?
                .len(),
            2
        );
        assert_eq!(database.list_knowledge_chunks(version.id)?.len(), 2);
        assert_eq!(database.list_knowledge_chunks(history_version.id)?.len(), 1);

        let restored = database.restore_knowledge_document(document.id)?;
        assert_eq!(restored.document.id, document.id);
        assert_eq!(restored.rebuilt_fts_entries, 3);
        assert!(database
            .get_knowledge_chunk_vector(replaced[0].id, 1)?
            .is_some());
        let hits_after_restore = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "退款审批".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["requirement".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(hits_after_restore.len(), 1);
        assert!(database.restore_knowledge_document(document.id).is_err());
        Ok(())
    }

    #[test]
    fn first_fts_creation_backfills_current_document_versions(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "first-fts-backfill".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "首次索引回填".to_string(),
            logical_path: "docs/first-fts.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v1".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: document.logical_path.clone(),
                mime_type: "text/markdown".to_string(),
                content: "首次创建 FTS 时必须回填这段正文".to_string(),
                content_hash: "first-fts-backfill-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "正文".to_string(),
                content: "首次创建 FTS 时必须回填这段正文".to_string(),
                content_hash: "first-fts-backfill-chunk".to_string(),
                location: serde_json::json!({}),
                token_estimate: 10,
            }],
        )?;
        let hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "必须回填".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["markdown".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("必须回填"));
        assert_eq!(database.rebuild_knowledge_fts()?, 1);
        Ok(())
    }

    #[test]
    fn title_hits_bound_unparsed_document_content_to_an_excerpt(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "title-excerpt".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "超长正文标题".to_string(),
            logical_path: "docs/title-excerpt.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let content = "正文".repeat(600);
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v1".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: document.logical_path.clone(),
                mime_type: "text/markdown".to_string(),
                content,
                content_hash: "title-excerpt-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 1_200,
            },
            &[],
        )?;
        let hits = database.search_knowledge_document_title_hits(&KnowledgeSearchInput {
            query: "超长正文标题".to_string(),
            project_ids: Vec::new(),
            release_ids: Vec::new(),
            source_ids: Vec::new(),
            document_types: vec!["markdown".to_string()],
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].citation.chunk_id.is_none());
        assert_eq!(hits[0].content.chars().count(), 400);
        assert_eq!(hits[0].citation.excerpt.chars().count(), 400);
        Ok(())
    }

    #[test]
    fn document_title_update_replaces_the_search_index_entry(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "renamed-title".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "旧标题".to_string(),
            logical_path: "docs/renamed-title.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v1".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: document.logical_path.clone(),
                mime_type: "text/markdown".to_string(),
                content: "标题变更不应创建新版本".to_string(),
                content_hash: "renamed-title-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[],
        )?;
        database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: Some(document.id),
            document_key: document.document_key.clone(),
            project_id: document.project_id,
            source_id: document.source_id,
            doc_type: document.doc_type.clone(),
            title: "新标题".to_string(),
            logical_path: document.logical_path.clone(),
            sensitivity: document.sensitivity.clone(),
            tags: document.tags.clone(),
            allow_ai: document.allow_ai,
            allow_mcp: document.allow_mcp,
        })?;
        let search = |query: &str| {
            database.search_knowledge_document_title_hits(&KnowledgeSearchInput {
                query: query.to_string(),
                project_ids: Vec::new(),
                release_ids: Vec::new(),
                source_ids: Vec::new(),
                document_types: vec!["markdown".to_string()],
                sensitivities: vec!["internal".to_string()],
                snapshot_id: None,
                limit: Some(10),
                include_context: Some(false),
            })
        };
        assert!(search("旧标题")?.is_empty());
        assert_eq!(search("新标题")?.len(), 1);
        Ok(())
    }

    #[test]
    fn vector_round_trip_is_profile_scoped_and_dimension_checked(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_embedding_profiles
                 (profile_key, name, mode, model, dimension, fingerprint, status)
                 VALUES ('profile-a', 'Profile A', 'local', 'model-a', 3, 'fp-a', 'building')",
                [],
            )?;
        }
        let metadata =
            database.upsert_knowledge_chunk_embedding(7, 1, "chunk-hash", &[0.2, 0.4, 0.8])?;
        assert_eq!(metadata.dimension, 3);
        let decoded = database
            .get_knowledge_chunk_vector(7, 1)?
            .ok_or("缺少向量")?;
        assert_eq!(decoded, vec![0.2, 0.4, 0.8]);

        let invalid = database.upsert_knowledge_chunk_embedding(7, 1, "chunk-hash", &[0.2, 0.4]);
        assert!(invalid.is_err());
        assert!(decode_vector_blob(&[0, 0, 0, 0], 2).is_err());
        Ok(())
    }

    #[test]
    fn embedding_validation_ignores_blank_chunks() -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "blank-chunk-document".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "空白片段校验".to_string(),
            logical_path: "blank-chunk.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "unversioned".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: document.logical_path.clone(),
                mime_type: "text/markdown".to_string(),
                content: "有效正文".to_string(),
                content_hash: "blank-chunk-version".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 2,
            },
            &[
                KnowledgeChunkWriteInput {
                    chunk_index: 0,
                    heading_path: "空白".to_string(),
                    content: " \n\t".to_string(),
                    content_hash: "blank-chunk-hash".to_string(),
                    location: serde_json::json!({"startLine": 1, "endLine": 1}),
                    token_estimate: 0,
                },
                KnowledgeChunkWriteInput {
                    chunk_index: 1,
                    heading_path: "正文".to_string(),
                    content: "有效正文".to_string(),
                    content_hash: "valid-chunk-hash".to_string(),
                    location: serde_json::json!({"startLine": 2, "endLine": 2}),
                    token_estimate: 2,
                },
            ],
        )?;
        let profile =
            database.upsert_knowledge_embedding_profile(&UpsertKnowledgeEmbeddingProfileInput {
                id: None,
                profile_key: "blank-chunk-profile".to_string(),
                name: "空白片段 Profile".to_string(),
                mode: "local".to_string(),
                provider_key: "local".to_string(),
                model: "test-model".to_string(),
                model_revision: String::new(),
                dimension: 2,
                normalized: true,
                config: serde_json::json!({}),
                fingerprint: "blank-chunk-fingerprint".to_string(),
            })?;
        database.begin_knowledge_embedding_profile_build(profile.id)?;
        let chunks = database.list_knowledge_chunks(version.id)?;
        let valid_chunk = chunks
            .iter()
            .find(|chunk| chunk.content == "有效正文")
            .ok_or("缺少有效片段")?;
        database.upsert_knowledge_chunk_embedding(
            valid_chunk.id,
            profile.id,
            &valid_chunk.content_hash,
            &[1.0, 0.0],
        )?;

        let validation = database.validate_knowledge_embedding_profile(profile.id)?;
        assert_eq!(validation.expected_chunks, 1);
        assert_eq!(validation.indexed_chunks, 1);
        assert!(validation.complete);
        Ok(())
    }

    #[test]
    fn blue_green_profile_lifecycle_preserves_old_index_and_requires_complete_build(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "blue-green-document".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "requirement".to_string(),
            title: "蓝绿索引".to_string(),
            logical_path: "blue-green.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "unversioned".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: document.logical_path.clone(),
                mime_type: "text/markdown".to_string(),
                content: "蓝绿索引正文".to_string(),
                content_hash: "blue-green-version".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 4,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "蓝绿索引".to_string(),
                content: "蓝绿索引正文".to_string(),
                content_hash: "blue-green-chunk".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 4,
            }],
        )?;
        let chunks = database.list_knowledge_chunks(version.id)?;
        let profile_input = |key: &str, fingerprint: &str| UpsertKnowledgeEmbeddingProfileInput {
            id: None,
            profile_key: key.to_string(),
            name: key.to_string(),
            mode: "local".to_string(),
            provider_key: "local".to_string(),
            model: "model-a".to_string(),
            model_revision: String::new(),
            dimension: 2,
            normalized: true,
            config: serde_json::json!({}),
            fingerprint: fingerprint.to_string(),
        };
        let old =
            database.upsert_knowledge_embedding_profile(&profile_input("profile-old", "fp-old"))?;
        database.begin_knowledge_embedding_profile_build(old.id)?;
        database.upsert_knowledge_chunk_embedding(
            chunks[0].id,
            old.id,
            &chunks[0].content_hash,
            &[1.0, 0.0],
        )?;
        assert!(
            database
                .complete_knowledge_embedding_profile_build(old.id)?
                .complete
        );
        database.activate_knowledge_embedding_profile(old.id)?;

        let mut changed_old_input = profile_input("profile-old", "fp-old-revised");
        changed_old_input.id = Some(old.id);
        assert!(database
            .upsert_knowledge_embedding_profile(&changed_old_input)
            .is_err());

        let next = database
            .upsert_knowledge_embedding_profile(&profile_input("profile-next", "fp-next"))?;
        database.begin_knowledge_embedding_profile_build(next.id)?;
        assert!(database
            .retire_knowledge_embedding_profile(next.id)
            .is_err());
        assert!(database
            .complete_knowledge_embedding_profile_build(next.id)
            .is_err());
        assert_eq!(
            database
                .get_active_knowledge_embedding_profile()?
                .map(|profile| profile.id),
            Some(old.id)
        );
        assert_eq!(
            database
                .get_knowledge_embedding_profile_by_id(next.id)?
                .ok_or("新 Profile 不存在")?
                .status,
            "failed"
        );

        let corrupted = database
            .upsert_knowledge_embedding_profile(&profile_input("profile-corrupt", "fp-corrupt"))?;
        database.begin_knowledge_embedding_profile_build(corrupted.id)?;
        database.upsert_knowledge_chunk_embedding(
            chunks[0].id,
            corrupted.id,
            &chunks[0].content_hash,
            &[1.0, 0.0],
        )?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE knowledge_chunk_embeddings
                 SET vector_blob = X'00'
                 WHERE chunk_id = ?1 AND profile_id = ?2",
                params![chunks[0].id, corrupted.id],
            )?;
        }
        let corrupted_validation = database.validate_knowledge_embedding_profile(corrupted.id)?;
        assert_eq!(corrupted_validation.invalid_vector_chunks, 1);
        assert!(!corrupted_validation.complete);
        assert!(database
            .complete_knowledge_embedding_profile_build(corrupted.id)
            .is_err());

        database.begin_knowledge_embedding_profile_build(next.id)?;
        database.upsert_knowledge_chunk_embedding(
            chunks[0].id,
            next.id,
            &chunks[0].content_hash,
            &[0.0, 1.0],
        )?;
        assert!(
            database
                .complete_knowledge_embedding_profile_build(next.id)?
                .complete
        );
        database.activate_knowledge_embedding_profile(next.id)?;
        assert_eq!(
            database
                .get_active_knowledge_embedding_profile()?
                .map(|profile| profile.id),
            Some(next.id)
        );
        assert!(database
            .get_knowledge_chunk_vector(chunks[0].id, old.id)?
            .is_some());

        database.activate_knowledge_embedding_profile(old.id)?;
        let retired = database.retire_knowledge_embedding_profile(next.id)?;
        assert_eq!(retired.status, "retired");
        assert!(database
            .get_knowledge_chunk_vector(chunks[0].id, next.id)?
            .is_none());
        Ok(())
    }

    #[test]
    fn failed_embedding_job_marks_only_its_building_profile_failed(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let profile =
            database.upsert_knowledge_embedding_profile(&UpsertKnowledgeEmbeddingProfileInput {
                id: None,
                profile_key: "failed-job-profile".to_string(),
                name: "失败任务 Profile".to_string(),
                mode: "local".to_string(),
                provider_key: "local".to_string(),
                model: "test-model".to_string(),
                model_revision: String::new(),
                dimension: 2,
                normalized: true,
                config: serde_json::json!({}),
                fingerprint: "failed-job-fingerprint".to_string(),
            })?;
        database.begin_knowledge_embedding_profile_build(profile.id)?;
        let job = database.create_knowledge_job(&CreateKnowledgeJobInput {
            job_key: "embedding-build-failure".to_string(),
            job_type: "embedding_build".to_string(),
            source_id: None,
            profile_id: Some(profile.id),
            message: "测试失败收尾".to_string(),
            checkpoint: serde_json::json!({"lastChunkId": 3}),
        })?;

        database.fail_knowledge_embedding_profile_for_job(job.id)?;
        assert_eq!(
            database
                .get_knowledge_embedding_profile_by_id(profile.id)?
                .ok_or("Profile 不存在")?
                .status,
            "failed"
        );
        assert!(database.get_active_knowledge_embedding_profile()?.is_none());
        Ok(())
    }

    /// 以 10 万片段模拟一个真实规模索引。该验收刻意绕过模型推理，仅写入已归一化的
    /// 固定向量，从而把验证范围限定为 SQLite 蓝绿索引的完整性、原子切换、回滚和清理。
    /// 模型质量与真实推理仍由独立的 Embedding Spike 负责，不能用本测试替代。
    #[test]
    #[ignore = "realistic-volume blue-green acceptance; run explicitly before release"]
    fn blue_green_profile_lifecycle_at_100k_chunks() -> Result<(), Box<dyn std::error::Error>> {
        const CHUNK_COUNT: usize = 100_000;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tauri-ssh-blue-green-scale-{unique}"));
        std::fs::create_dir_all(&root)?;
        let database_path = root.join("knowledge.sqlite");
        let database = Database::init(database_path.to_string_lossy().as_ref())?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "blue-green-scale-document".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "requirement".to_string(),
            title: "蓝绿索引规模验收".to_string(),
            logical_path: "acceptance/blue-green-scale.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let chunks = (0..CHUNK_COUNT)
            .map(|index| KnowledgeChunkWriteInput {
                chunk_index: index as i64,
                heading_path: "规模验收".to_string(),
                content: format!("蓝绿索引规模验收片段 {index}"),
                content_hash: format!("blue-green-scale-chunk-{index:06}"),
                location: serde_json::json!({"startLine": index + 1, "endLine": index + 1}),
                token_estimate: 1,
            })
            .collect::<Vec<_>>();
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "unversioned".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: document.logical_path.clone(),
                mime_type: "text/markdown".to_string(),
                content: "蓝绿索引规模验收".to_string(),
                content_hash: "blue-green-scale-version".to_string(),
                parsed_meta: serde_json::json!({"acceptance": "100k-blue-green"}),
                token_estimate: i64::try_from(CHUNK_COUNT)?,
            },
            &chunks,
        )?;
        let profile_input = |key: &str, fingerprint: &str| UpsertKnowledgeEmbeddingProfileInput {
            id: None,
            profile_key: key.to_string(),
            name: key.to_string(),
            mode: "local".to_string(),
            provider_key: "local".to_string(),
            model: "acceptance-vector".to_string(),
            model_revision: String::new(),
            dimension: 2,
            normalized: true,
            config: serde_json::json!({"acceptance": "100k-blue-green"}),
            fingerprint: fingerprint.to_string(),
        };
        let write_vectors =
            |profile_id: i64, take: usize, vector: [f32; 2]| -> Result<(), AppError> {
                let mut conn = database
                    .conn
                    .lock()
                    .map_err(|error| AppError::Custom(error.to_string()))?;
                let transaction = conn.transaction()?;
                let pairs = transaction
                    .prepare(
                        "SELECT id, content_hash FROM knowledge_chunks
                     WHERE document_version_id = ?1 ORDER BY id LIMIT ?2",
                    )?
                    .query_map(
                        params![version.id, i64::try_from(take).unwrap_or(i64::MAX)],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                    )?
                    .collect::<Result<Vec<_>, _>>()?;
                let vector_blob = vector
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>();
                let mut statement = transaction.prepare(
                    "INSERT OR REPLACE INTO knowledge_chunk_embeddings
                 (chunk_id, profile_id, dimension, vector_blob, vector_norm, content_hash)
                 VALUES (?1, ?2, 2, ?3, 1.0, ?4)",
                )?;
                for (chunk_id, content_hash) in pairs {
                    statement.execute(params![chunk_id, profile_id, vector_blob, content_hash])?;
                }
                drop(statement);
                transaction.commit()?;
                Ok(())
            };

        let old = database.upsert_knowledge_embedding_profile(&profile_input(
            "blue-green-scale-old",
            "blue-green-scale-old-fingerprint",
        ))?;
        database.begin_knowledge_embedding_profile_build(old.id)?;
        write_vectors(old.id, CHUNK_COUNT, [1.0, 0.0])?;
        assert_eq!(
            database
                .complete_knowledge_embedding_profile_build(old.id)?
                .expected_chunks,
            i64::try_from(CHUNK_COUNT)?
        );
        database.activate_knowledge_embedding_profile(old.id)?;

        let next = database.upsert_knowledge_embedding_profile(&profile_input(
            "blue-green-scale-next",
            "blue-green-scale-next-fingerprint",
        ))?;
        database.begin_knowledge_embedding_profile_build(next.id)?;
        write_vectors(next.id, CHUNK_COUNT / 2, [0.0, 1.0])?;
        let last_persisted_chunk_id = database
            .list_knowledge_chunks(version.id)?
            .get(CHUNK_COUNT / 2 - 1)
            .ok_or("缺少半量重建检查点片段")?
            .id;
        let rebuild_job = database.create_knowledge_job(&CreateKnowledgeJobInput {
            job_key: "blue-green-scale-rebuild".to_string(),
            // 必须使用生产重试分派识别的真实任务类型；不能用只在测试中存在的名称
            // 冒充可恢复的构建任务。
            job_type: "embedding_build".to_string(),
            source_id: None,
            profile_id: Some(next.id),
            message: "100k 索引重建中".to_string(),
            checkpoint: serde_json::json!({
                "profileId": next.id,
                "lastChunkId": last_persisted_chunk_id,
                "processed": CHUNK_COUNT / 2,
                "embedded": CHUNK_COUNT / 2,
                "skipped": 0,
                "blocked": 0,
            }),
        })?;
        database.mark_knowledge_job_running(
            rebuild_job.id,
            "embedding",
            "模拟应用中断",
            &serde_json::json!({
                "profileId": next.id,
                "lastChunkId": last_persisted_chunk_id,
                "processed": CHUNK_COUNT / 2,
                "embedded": CHUNK_COUNT / 2,
                "skipped": 0,
                "blocked": 0,
            }),
        )?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE knowledge_jobs
                 SET heartbeat_at = datetime('now', 'localtime', '-2 minutes')
                 WHERE id = ?1",
                [rebuild_job.id],
            )?;
        }
        assert_eq!(database.recover_interrupted_knowledge_jobs(30)?, 1);
        assert_eq!(
            database
                .get_knowledge_job_by_id(rebuild_job.id)?
                .ok_or("重建任务不存在")?
                .status,
            "interrupted"
        );
        assert_eq!(
            database
                .get_active_knowledge_embedding_profile()?
                .map(|profile| profile.id),
            Some(old.id)
        );
        assert_eq!(
            database.restart_knowledge_job(rebuild_job.id)?.status,
            "queued"
        );
        assert!(database
            .complete_knowledge_embedding_profile_build(next.id)
            .is_err());
        assert_eq!(
            database
                .get_active_knowledge_embedding_profile()?
                .map(|profile| profile.id),
            Some(old.id)
        );

        database.begin_knowledge_embedding_profile_build(next.id)?;
        write_vectors(next.id, CHUNK_COUNT, [0.0, 1.0])?;
        let next_validation = database.complete_knowledge_embedding_profile_build(next.id)?;
        assert!(next_validation.complete);
        assert_eq!(next_validation.indexed_chunks, i64::try_from(CHUNK_COUNT)?);
        database.activate_knowledge_embedding_profile(next.id)?;
        assert_eq!(
            database
                .get_active_knowledge_embedding_profile()?
                .map(|profile| profile.id),
            Some(next.id)
        );

        database.activate_knowledge_embedding_profile(old.id)?;
        let retired = database.retire_knowledge_embedding_profile(next.id)?;
        assert_eq!(retired.status, "retired");
        let remaining_next_vectors = {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.query_row(
                "SELECT COUNT(*) FROM knowledge_chunk_embeddings WHERE profile_id = ?1",
                [next.id],
                |row| row.get::<_, i64>(0),
            )?
        };
        assert_eq!(remaining_next_vectors, 0);
        assert!(std::fs::metadata(&database_path)?.len() > 0);
        drop(database);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn knowledge_jobs_persist_checkpoint_cancel_and_recover(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let checkpoint = serde_json::json!({
            "sourceId": 7,
            "releaseId": null,
            "gitRef": "HEAD",
            "stage": "queued",
        });
        let job = database.create_knowledge_job(&CreateKnowledgeJobInput {
            job_key: "job-source-sync-1".to_string(),
            job_type: "source_sync".to_string(),
            source_id: Some(7),
            profile_id: None,
            message: "排队".to_string(),
            checkpoint: checkpoint.clone(),
        })?;
        assert_eq!(job.status, "queued");
        assert_eq!(
            database
                .find_active_knowledge_job("source_sync", Some(7))?
                .map(|value| value.id),
            Some(job.id)
        );

        let running = database.mark_knowledge_job_running(job.id, "sync", "开始", &checkpoint)?;
        assert_eq!(running.status, "running");
        let progress_checkpoint = serde_json::json!({
            "sourceId": 7,
            "releaseId": null,
            "gitRef": "HEAD",
            "stage": "read_local_files",
            "current": 2,
            "total": 5,
            "lastPath": "docs/requirement.md",
        });
        assert!(database.update_knowledge_job_progress(
            job.id,
            2,
            5,
            "处理文档",
            &progress_checkpoint,
        )?);
        assert!(database.update_knowledge_job_progress(
            job.id,
            2,
            5,
            "重复检查点不产生新任务",
            &progress_checkpoint,
        )?);
        let polled = database
            .get_knowledge_job("job-source-sync-1")?
            .ok_or("知识任务不存在")?;
        assert_eq!(polled.progress_current, 2);
        assert_eq!(polled.checkpoint["lastPath"], "docs/requirement.md");
        assert_eq!(database.list_knowledge_jobs(10)?.len(), 1);

        let cancelling = database.request_knowledge_job_cancel(job.id)?;
        assert!(cancelling.cancel_requested);
        assert_eq!(cancelling.status, "running");
        // 模拟最后一个批次检查完取消标志、但写 completed 前用户发起取消的窗口：完成
        // 操作必须 compare-and-set 失败，随后由调用方写入 cancelled 检查点。
        assert!(database
            .finish_knowledge_job(
                job.id,
                "completed",
                "不得覆盖取消",
                None,
                &progress_checkpoint,
            )
            .is_err());
        let cancelled = database.finish_knowledge_job(
            job.id,
            "cancelled",
            "安全取消",
            None,
            &progress_checkpoint,
        )?;
        assert_eq!(cancelled.status, "cancelled");

        let queued = database.restart_knowledge_job(job.id)?;
        assert_eq!(queued.status, "queued");
        assert!(!queued.cancel_requested);
        database.mark_knowledge_job_running(job.id, "sync", "重新开始", &progress_checkpoint)?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE knowledge_jobs
                 SET heartbeat_at = datetime('now', 'localtime', '-2 minutes')
                 WHERE id = ?1",
                [job.id],
            )?;
        }
        assert_eq!(database.recover_interrupted_knowledge_jobs(30)?, 1);
        let interrupted = database
            .get_knowledge_job_by_id(job.id)?
            .ok_or("中断任务不存在")?;
        assert_eq!(interrupted.status, "interrupted");
        assert_eq!(interrupted.checkpoint["lastPath"], "docs/requirement.md");
        Ok(())
    }

    #[test]
    fn failed_job_completion_yields_to_concurrent_cancellation(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let checkpoint = serde_json::json!({"releaseId": 7, "stage": "backfill"});
        let job = database.create_knowledge_job(&CreateKnowledgeJobInput {
            job_key: "project-version-backfill-cancel".to_string(),
            job_type: "project_version_backfill".to_string(),
            source_id: None,
            profile_id: None,
            message: "排队".to_string(),
            checkpoint: checkpoint.clone(),
        })?;
        database.mark_knowledge_job_running(job.id, "backfill", "开始", &checkpoint)?;
        database.request_knowledge_job_cancel(job.id)?;

        let finished = database.finish_knowledge_job_failed_or_cancel(
            job.id,
            "回填失败",
            "不应覆盖取消",
            &checkpoint,
            "回填已取消",
            &checkpoint,
        )?;
        assert_eq!(finished.status, "cancelled");
        assert!(finished.error.is_none());
        Ok(())
    }

    #[test]
    fn cancelling_a_queued_job_prevents_a_stale_runner_from_starting_it(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let checkpoint = serde_json::json!({"stage": "queued"});
        let job = database.create_knowledge_job(&CreateKnowledgeJobInput {
            job_key: "job-cancel-before-start".to_string(),
            job_type: "upload_import".to_string(),
            source_id: None,
            profile_id: None,
            message: "等待导入".to_string(),
            checkpoint: checkpoint.clone(),
        })?;
        let stale_runner_job_id = job.id;

        let cancelled = database.request_knowledge_job_cancel(job.id)?;
        assert_eq!(cancelled.status, "cancelled");
        assert!(database
            .mark_knowledge_job_running(stale_runner_job_id, "parse", "不应重新启动", &checkpoint)
            .is_err());
        let persisted = database
            .get_knowledge_job_by_id(job.id)?
            .expect("已取消的任务仍应可查询");
        assert_eq!(persisted.status, "cancelled");
        assert!(persisted.cancel_requested);
        Ok(())
    }

    #[test]
    fn active_vector_search_applies_metadata_filters_and_cosine_order(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "vector-project".to_string(),
            name: "向量项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "vector-doc".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "requirement".to_string(),
            title: "退款审批".to_string(),
            logical_path: "REQ-1042.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "unversioned".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: "REQ-1042.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "退款审批".to_string(),
                content_hash: "vector-version".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 4,
            },
            &[
                KnowledgeChunkWriteInput {
                    chunk_index: 0,
                    heading_path: "退款审批".to_string(),
                    content: "大额退款需要人工审批".to_string(),
                    content_hash: "vector-chunk-a".to_string(),
                    location: serde_json::json!({"startLine": 1, "endLine": 1}),
                    token_estimate: 4,
                },
                KnowledgeChunkWriteInput {
                    chunk_index: 1,
                    heading_path: "其他".to_string(),
                    content: "无关内容".to_string(),
                    content_hash: "vector-chunk-b".to_string(),
                    location: serde_json::json!({"startLine": 2, "endLine": 2}),
                    token_estimate: 2,
                },
            ],
        )?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_embedding_profiles
                 (profile_key, name, mode, model, dimension, fingerprint, status)
                 VALUES ('search-profile', 'Search Profile', 'local', 'model-a', 2,
                         'search-fingerprint', 'building')",
                [],
            )?;
        }
        let chunks = database.list_knowledge_chunks(version.id)?;
        database.upsert_knowledge_chunk_embedding(
            chunks[0].id,
            1,
            "vector-chunk-a",
            &[1.0, 0.0],
        )?;
        database.upsert_knowledge_chunk_embedding(
            chunks[1].id,
            1,
            "vector-chunk-b",
            &[0.0, 1.0],
        )?;
        database.complete_knowledge_embedding_profile_build(1)?;
        database.activate_knowledge_embedding_profile(1)?;
        let hits = KnowledgeEmbeddingService::search_active_vectors(
            &database,
            KnowledgeVectorSearchInput {
                query_vector: vec![0.9, 0.1],
                filters: KnowledgeSearchInput {
                    query: "退款审批".to_string(),
                    project_ids: vec![project.id],
                    release_ids: Vec::new(),
                    source_ids: Vec::new(),
                    document_types: vec!["requirement".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(1),
                    include_context: Some(true),
                },
            },
        )?;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].citation.chunk_id, Some(chunks[0].id));
        assert!(hits[0].content.contains("人工审批"));
        assert_eq!(hits[0].channels, vec!["vector"]);
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE knowledge_documents SET allow_ai = 0 WHERE id = ?1",
                [document.id],
            )?;
        }
        assert!(database
            .list_active_knowledge_vector_candidates(10)?
            .is_empty());
        Ok(())
    }

    #[test]
    fn code_source_settings_are_upserted_with_its_knowledge_source(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let code_source =
            database.upsert_knowledge_code_source(&UpsertKnowledgeCodeSourceInput {
                source: UpsertKnowledgeSourceInput {
                    id: None,
                    source_key: "code-local".to_string(),
                    project_id: None,
                    source_type: "local_directory".to_string(),
                    display_name: "本地源码".to_string(),
                    root_path: "/authorized/code".to_string(),
                    git_workspace_key: String::new(),
                    include_globs: vec!["**/*.rs".to_string()],
                    exclude_globs: vec!["target/**".to_string()],
                    version_strategy: "manual".to_string(),
                    sync_mode: "manual".to_string(),
                    allow_remote_embedding: false,
                    enabled: true,
                },
                include_untracked: false,
                max_file_size_bytes: 2 * 1024 * 1024,
                allowed_languages: vec!["rust".to_string(), "typescript".to_string()],
                allow_remote_processing: false,
            })?;
        assert_eq!(code_source.source.source_key, "code-local");
        assert!(!code_source.settings.include_untracked);
        assert_eq!(code_source.settings.allowed_languages.len(), 2);
        assert_eq!(database.list_knowledge_code_sources()?.len(), 1);
        Ok(())
    }

    #[test]
    fn knowledge_relations_are_idempotent_and_confirmation_is_explicit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = test_database()?;
        let input = UpsertKnowledgeRelationInput {
            id: None,
            project_id: Some(1),
            release_id: None,
            document_version_id: None,
            snapshot_id: None,
            sensitivity: "internal".to_string(),
            from_type: "requirement".to_string(),
            from_key: "REQ-1042".to_string(),
            relation_type: "implemented_by".to_string(),
            to_type: "commit".to_string(),
            to_key: "a1b2c3d".to_string(),
            evidence: serde_json::json!({"source": "git", "commit": "a1b2c3d"}),
            confidence: 0.72,
            confirmed: false,
            source: "commit_trailer".to_string(),
        };
        let created = database.upsert_knowledge_relation(&input)?;
        assert!(!created.confirmed);
        let updated = database.upsert_knowledge_relation(&UpsertKnowledgeRelationInput {
            confidence: 0.91,
            confirmed: true,
            ..input
        })?;
        assert_eq!(created.id, updated.id);
        assert!(updated.confirmed);
        assert_eq!(updated.confidence, 0.91);
        let listed = database.list_knowledge_relations(&ListKnowledgeRelationsInput {
            entity_type: Some("requirement".to_string()),
            entity_key: Some("REQ-1042".to_string()),
            project_ids: vec![1],
            release_ids: Vec::new(),
            sensitivities: vec!["internal".to_string()],
            confirmed_only: Some(true),
            limit: Some(10),
        })?;
        assert_eq!(listed.len(), 1);
        assert!(
            database
                .confirm_knowledge_relation(updated.id, false)?
                .confirmed
                == false
        );
        Ok(())
    }
}
