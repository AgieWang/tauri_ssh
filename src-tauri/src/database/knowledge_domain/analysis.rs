use rusqlite::{params, OptionalExtension};

use crate::database::Database;
use crate::error::AppError;

pub(crate) const DOMAIN: &str = "analysis";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewKnowledgeAnalysisRun {
    pub run_key: String,
    pub project_id: i64,
    pub release_id: i64,
    pub manifest_hash: String,
    pub analyzer_version: String,
    pub include_rules_json: String,
    pub exclude_rules_json: String,
    pub snapshot_ids_json: String,
    pub evidence_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeAnalysisRunRecord {
    pub id: i64,
    pub run_key: String,
    pub project_id: i64,
    pub release_id: i64,
    pub manifest_hash: String,
    pub analyzer_version: String,
    pub snapshot_ids_json: String,
    pub evidence_hash: String,
    pub status: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewKnowledgeAnalysisDraft {
    pub analysis_run_id: i64,
    pub provider_key: String,
    pub model: String,
    pub template_key: String,
    pub content: String,
    pub claim_refs_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeAnalysisDraftRecord {
    pub id: i64,
    pub analysis_run_id: i64,
    pub provider_key: String,
    pub model: String,
    pub template_key: String,
    pub content: String,
    pub claim_refs_json: String,
    pub status: String,
    pub confirmed_version_id: Option<i64>,
}

impl Database {
    pub(crate) fn create_knowledge_analysis_run(
        &self,
        run: &NewKnowledgeAnalysisRun,
    ) -> Result<KnowledgeAnalysisRunRecord, AppError> {
        if run.run_key.trim().is_empty()
            || run.project_id <= 0
            || run.release_id <= 0
            || run.manifest_hash.trim().is_empty()
            || run.analyzer_version.trim().is_empty()
            || run.evidence_hash.trim().is_empty()
        {
            return Err(AppError::InvalidInput(
                "分析运行缺少项目、版本或证据标识".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_analysis_runs
                (run_key, project_id, release_id, manifest_hash, analyzer_version,
                 include_rules_json, exclude_rules_json, snapshot_ids_json, evidence_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(run_key) DO NOTHING",
            params![
                run.run_key.trim(),
                run.project_id,
                run.release_id,
                run.manifest_hash.trim(),
                run.analyzer_version.trim(),
                run.include_rules_json,
                run.exclude_rules_json,
                run.snapshot_ids_json,
                run.evidence_hash.trim(),
            ],
        )?;
        get_analysis_run_by_key(&conn, run.run_key.trim())?
            .ok_or_else(|| AppError::Custom("创建分析运行后未找到记录".to_string()))
    }

    pub(crate) fn get_knowledge_analysis_run_by_key(
        &self,
        run_key: &str,
    ) -> Result<Option<KnowledgeAnalysisRunRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_analysis_run_by_key(&conn, run_key)
    }

    pub(crate) fn get_knowledge_analysis_run_by_id(
        &self,
        run_id: i64,
    ) -> Result<Option<KnowledgeAnalysisRunRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, run_key, project_id, release_id, manifest_hash, analyzer_version,
                    snapshot_ids_json, evidence_hash, status, finished_at
             FROM knowledge_analysis_runs WHERE id = ?1",
            [run_id],
            map_analysis_run,
        )
        .optional()
        .map_err(Into::into)
    }

    pub(crate) fn update_knowledge_analysis_run_status(
        &self,
        run_id: i64,
        status: &str,
    ) -> Result<(), AppError> {
        if run_id <= 0 || status.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "分析运行状态更新缺少有效参数".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_analysis_runs SET status = ?2,
                 finished_at = CASE WHEN ?2 IN ('completed', 'failed', 'cancelled')
                                    THEN datetime('now', 'localtime') ELSE NULL END
             WHERE id = ?1",
            params![run_id, status.trim()],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("分析运行不存在: {run_id}")));
        }
        Ok(())
    }

    /// 远程调用前原子领取一次运行权。其他窗口只能观察“正在生成”，不能重复发送相同
    /// 固定快照上下文，从而避免覆盖草稿或重复消耗 Provider 配额。
    pub(crate) fn claim_knowledge_analysis_run(&self, run_id: i64) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        Ok(conn.execute(
            "UPDATE knowledge_analysis_runs
             SET status = 'running', finished_at = NULL
             WHERE id = ?1 AND status IN ('queued', 'failed')",
            [run_id],
        )? == 1)
    }

    pub(crate) fn upsert_knowledge_analysis_draft(
        &self,
        draft: &NewKnowledgeAnalysisDraft,
    ) -> Result<KnowledgeAnalysisDraftRecord, AppError> {
        if draft.analysis_run_id <= 0 || draft.template_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "分析草稿缺少运行或模板标识".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_analysis_drafts
                (analysis_run_id, provider_key, model, template_key, content, claim_refs_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(analysis_run_id, template_key) DO UPDATE SET
                provider_key = excluded.provider_key, model = excluded.model, content = excluded.content,
                claim_refs_json = excluded.claim_refs_json, status = 'draft',
                updated_at = datetime('now', 'localtime')
             WHERE knowledge_analysis_drafts.status != 'confirmed'",
            params![
                draft.analysis_run_id, draft.provider_key.trim(), draft.model.trim(),
                draft.template_key.trim(), draft.content, draft.claim_refs_json,
            ],
        )?;
        conn.query_row(
            "SELECT id, analysis_run_id, provider_key, model, template_key, content,
                    claim_refs_json, status, confirmed_version_id
             FROM knowledge_analysis_drafts
             WHERE analysis_run_id = ?1 AND template_key = ?2",
            params![draft.analysis_run_id, draft.template_key.trim()],
            map_analysis_draft,
        )
        .map_err(Into::into)
    }

    /// 在写入正式知识文档前原子领取确认权，防止两个窗口并发确认时重复创建不可变版本。
    pub(crate) fn claim_knowledge_analysis_draft_confirmation(
        &self,
        draft_id: i64,
    ) -> Result<KnowledgeAnalysisDraftRecord, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_analysis_drafts
             SET status = 'confirming', updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status IN ('draft', 'reviewing')",
            [draft_id],
        )?;
        if changed != 1 {
            return Err(AppError::InvalidInput(
                "分析草稿不存在或正在确认，不能重复提交".to_string(),
            ));
        }
        conn.query_row(
            "SELECT id, analysis_run_id, provider_key, model, template_key, content,
                    claim_refs_json, status, confirmed_version_id
             FROM knowledge_analysis_drafts WHERE id = ?1",
            [draft_id],
            map_analysis_draft,
        )
        .map_err(Into::into)
    }

    /// 仅允许已经领取确认权的调用落库，确保正式文档与草稿审计关联不会被后来者覆盖。
    pub(crate) fn confirm_knowledge_analysis_draft(
        &self,
        draft_id: i64,
        document_version_id: i64,
    ) -> Result<(), AppError> {
        if draft_id <= 0 || document_version_id <= 0 {
            return Err(AppError::InvalidInput(
                "分析草稿和确认文档版本必须有效".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_analysis_drafts
             SET status = 'confirmed', confirmed_version_id = ?2,
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status = 'confirming'",
            params![draft_id, document_version_id],
        )?;
        if changed == 0 {
            return Err(AppError::InvalidInput(
                "分析草稿不存在或当前状态不能确认".to_string(),
            ));
        }
        Ok(())
    }

    /// 文档草稿保存或提交失败时释放确认权，用户可修正输入后再次确认；已确认的草稿
    /// 不会被这个恢复操作覆盖。
    pub(crate) fn release_knowledge_analysis_draft_confirmation(
        &self,
        draft_id: i64,
    ) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "UPDATE knowledge_analysis_drafts
             SET status = 'draft', updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status = 'confirming'",
            [draft_id],
        )?;
        Ok(())
    }

    /// 应用进程中断后，运行中的远程调用不可能继续执行，重置为 failed 以允许用户重试。
    /// 对确认中的草稿，按版本行中仅内部写入的唯一草稿关联查找已经创建的不可变版本：
    /// 找得到就补齐 confirmed 关联，找不到才释放回 draft，避免重启后重复生成正式文档。
    pub(crate) fn recover_interrupted_knowledge_analysis_state(
        &self,
    ) -> Result<(i64, i64), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let recovered_runs = conn.execute(
            "UPDATE knowledge_analysis_runs
             SET status = 'failed', finished_at = datetime('now', 'localtime')
             WHERE status = 'running'",
            [],
        )?;
        let confirming_ids = conn
            .prepare("SELECT id FROM knowledge_analysis_drafts WHERE status = 'confirming'")?
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for draft_id in &confirming_ids {
            let document_version_id = conn
                .query_row(
                    "SELECT id FROM knowledge_document_versions
                     WHERE valid = 1 AND analysis_draft_id = ?1",
                    [draft_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            if let Some(document_version_id) = document_version_id {
                conn.execute(
                    "UPDATE knowledge_analysis_drafts
                     SET status = 'confirmed', confirmed_version_id = ?2,
                         updated_at = datetime('now', 'localtime')
                     WHERE id = ?1 AND status = 'confirming'",
                    params![draft_id, document_version_id],
                )?;
            } else {
                conn.execute(
                    "UPDATE knowledge_analysis_drafts
                     SET status = 'draft', updated_at = datetime('now', 'localtime')
                     WHERE id = ?1 AND status = 'confirming'",
                    [draft_id],
                )?;
            }
        }
        Ok((
            i64::try_from(recovered_runs).unwrap_or(i64::MAX),
            i64::try_from(confirming_ids.len()).unwrap_or(i64::MAX),
        ))
    }

    pub(crate) fn get_knowledge_analysis_draft_by_run_and_template(
        &self,
        analysis_run_id: i64,
        template_key: &str,
    ) -> Result<Option<KnowledgeAnalysisDraftRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, analysis_run_id, provider_key, model, template_key, content,
                    claim_refs_json, status, confirmed_version_id
             FROM knowledge_analysis_drafts
             WHERE analysis_run_id = ?1 AND template_key = ?2",
            params![analysis_run_id, template_key.trim()],
            map_analysis_draft,
        )
        .optional()
        .map_err(Into::into)
    }

    pub(crate) fn get_knowledge_analysis_draft_by_id(
        &self,
        draft_id: i64,
    ) -> Result<Option<KnowledgeAnalysisDraftRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, analysis_run_id, provider_key, model, template_key, content,
                    claim_refs_json, status, confirmed_version_id
             FROM knowledge_analysis_drafts WHERE id = ?1",
            [draft_id],
            map_analysis_draft,
        )
        .optional()
        .map_err(Into::into)
    }
}

fn map_analysis_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeAnalysisRunRecord> {
    Ok(KnowledgeAnalysisRunRecord {
        id: row.get(0)?,
        run_key: row.get(1)?,
        project_id: row.get(2)?,
        release_id: row.get(3)?,
        manifest_hash: row.get(4)?,
        analyzer_version: row.get(5)?,
        snapshot_ids_json: row.get(6)?,
        evidence_hash: row.get(7)?,
        status: row.get(8)?,
        finished_at: row.get(9)?,
    })
}

fn get_analysis_run_by_key(
    conn: &rusqlite::Connection,
    run_key: &str,
) -> Result<Option<KnowledgeAnalysisRunRecord>, AppError> {
    conn.query_row(
        "SELECT id, run_key, project_id, release_id, manifest_hash, analyzer_version,
                snapshot_ids_json, evidence_hash, status, finished_at
         FROM knowledge_analysis_runs WHERE run_key = ?1",
        [run_key],
        map_analysis_run,
    )
    .optional()
    .map_err(Into::into)
}

fn map_analysis_draft(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeAnalysisDraftRecord> {
    Ok(KnowledgeAnalysisDraftRecord {
        id: row.get(0)?,
        analysis_run_id: row.get(1)?,
        provider_key: row.get(2)?,
        model: row.get(3)?,
        template_key: row.get(4)?,
        content: row.get(5)?,
        claim_refs_json: row.get(6)?,
        status: row.get(7)?,
        confirmed_version_id: row.get(8)?,
    })
}

pub(crate) fn analysis_draft_commit_message(draft_id: i64) -> String {
    format!("确认 AI 代码分析草稿 #{draft_id} 并入库")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;

    use super::{Database, NewKnowledgeAnalysisDraft, NewKnowledgeAnalysisRun};
    use crate::database::schema;

    #[test]
    fn analysis_run_is_idempotent_and_draft_requires_explicit_confirmation(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        let run = NewKnowledgeAnalysisRun {
            run_key: "analysis:project:release".into(),
            project_id: 1,
            release_id: 2,
            manifest_hash: "manifest".into(),
            analyzer_version: "v1".into(),
            include_rules_json: "[]".into(),
            exclude_rules_json: "[]".into(),
            snapshot_ids_json: "[1,2]".into(),
            evidence_hash: "evidence".into(),
        };
        let first = database.create_knowledge_analysis_run(&run)?;
        let repeated = database.create_knowledge_analysis_run(&run)?;
        assert_eq!(first.id, repeated.id);
        assert_eq!(first.snapshot_ids_json, "[1,2]");
        assert!(database.claim_knowledge_analysis_run(first.id)?);
        assert!(
            !database.claim_knowledge_analysis_run(first.id)?,
            "并发调用不能重复领取远程 AI 运行权"
        );
        database.update_knowledge_analysis_run_status(first.id, "failed")?;
        let draft = database.upsert_knowledge_analysis_draft(&NewKnowledgeAnalysisDraft {
            analysis_run_id: first.id,
            provider_key: "local".into(),
            model: "model".into(),
            template_key: "project-summary".into(),
            content: "带证据的摘要".into(),
            claim_refs_json: "[\"file:1\"]".into(),
        })?;
        assert_eq!(draft.status, "draft");
        database.claim_knowledge_analysis_draft_confirmation(draft.id)?;
        assert!(
            database
                .claim_knowledge_analysis_draft_confirmation(draft.id)
                .is_err(),
            "并发调用不能重复领取草稿确认权"
        );
        database.confirm_knowledge_analysis_draft(draft.id, 11)?;
        assert!(database
            .confirm_knowledge_analysis_draft(draft.id, 12)
            .is_err());
        Ok(())
    }

    #[test]
    fn interrupted_analysis_state_recovers_runs_and_uncommitted_drafts(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        let run = database.create_knowledge_analysis_run(&NewKnowledgeAnalysisRun {
            run_key: "analysis:interrupted".into(),
            project_id: 1,
            release_id: 2,
            manifest_hash: "manifest".into(),
            analyzer_version: "v1".into(),
            include_rules_json: "[]".into(),
            exclude_rules_json: "[]".into(),
            snapshot_ids_json: "[1]".into(),
            evidence_hash: "evidence".into(),
        })?;
        assert!(database.claim_knowledge_analysis_run(run.id)?);
        let draft = database.upsert_knowledge_analysis_draft(&NewKnowledgeAnalysisDraft {
            analysis_run_id: run.id,
            provider_key: "local".into(),
            model: "model".into(),
            template_key: "project-summary".into(),
            content: "带证据的摘要".into(),
            claim_refs_json: "[\"code:1:file:2\"]".into(),
        })?;
        database.claim_knowledge_analysis_draft_confirmation(draft.id)?;
        assert_eq!(
            database.recover_interrupted_knowledge_analysis_state()?,
            (1, 1)
        );
        assert_eq!(
            database
                .get_knowledge_analysis_run_by_id(run.id)?
                .expect("运行记录存在")
                .status,
            "failed"
        );
        assert_eq!(
            database
                .get_knowledge_analysis_draft_by_id(draft.id)?
                .expect("草稿存在")
                .status,
            "draft"
        );
        Ok(())
    }

    #[test]
    fn interrupted_confirmation_recovers_only_the_internal_draft_link(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        let run = database.create_knowledge_analysis_run(&NewKnowledgeAnalysisRun {
            run_key: "analysis:interrupted-confirmation".into(),
            project_id: 1,
            release_id: 2,
            manifest_hash: "manifest".into(),
            analyzer_version: "v1".into(),
            include_rules_json: "[]".into(),
            exclude_rules_json: "[]".into(),
            snapshot_ids_json: "[1]".into(),
            evidence_hash: "evidence".into(),
        })?;
        let draft = database.upsert_knowledge_analysis_draft(&NewKnowledgeAnalysisDraft {
            analysis_run_id: run.id,
            provider_key: "local".into(),
            model: "model".into(),
            template_key: "project-summary".into(),
            content: "带证据的摘要".into(),
            claim_refs_json: "[\"code:1:file:2\"]".into(),
        })?;
        database.claim_knowledge_analysis_draft_confirmation(draft.id)?;
        let version_id = {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_document_versions
                    (document_id, content, content_hash, commit_message, analysis_draft_id)
                 VALUES (1, '已写入正文', 'analysis-confirmed-hash',
                         '确认 AI 代码分析草稿 #999 并入库', ?1)",
                [draft.id],
            )?;
            conn.last_insert_rowid()
        };

        assert_eq!(
            database.recover_interrupted_knowledge_analysis_state()?,
            (0, 1)
        );
        let recovered = database
            .get_knowledge_analysis_draft_by_id(draft.id)?
            .expect("草稿存在");
        assert_eq!(recovered.status, "confirmed");
        assert_eq!(recovered.confirmed_version_id, Some(version_id));
        Ok(())
    }
}
