use rusqlite::{params, OptionalExtension};

use crate::database::Database;
use crate::error::AppError;
use crate::models::knowledge_domain::qa::{
    KnowledgeQaMessage, KnowledgeQaSession, KnowledgeQaSessionDetail, PersistKnowledgeQaRoundInput,
};

pub(crate) const DOMAIN: &str = "qa";

impl Database {
    pub(crate) fn list_knowledge_qa_sessions(
        &self,
        project_id: i64,
    ) -> Result<Vec<KnowledgeQaSession>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT session.id, session.project_id, session.project_version_id,
                    session.release_commit_sha,
                    session.provider_key, session.model, session.title,
                    COUNT(message.id), session.created_at, session.updated_at
             FROM knowledge_qa_sessions session
             LEFT JOIN knowledge_qa_messages message ON message.session_id = session.id
             WHERE session.project_id = ?1 AND session.deleted_at IS NULL
             GROUP BY session.id
             ORDER BY session.updated_at DESC, session.id DESC",
        )?;
        let sessions = statement
            .query_map([project_id], map_session)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(sessions)
    }

    pub(crate) fn get_knowledge_qa_session_detail(
        &self,
        session_id: i64,
    ) -> Result<Option<KnowledgeQaSessionDetail>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let session = conn
            .query_row(
                "SELECT session.id, session.project_id, session.project_version_id,
                        session.release_commit_sha,
                        session.provider_key, session.model, session.title,
                        COUNT(message.id), session.created_at, session.updated_at
                 FROM knowledge_qa_sessions session
                 LEFT JOIN knowledge_qa_messages message ON message.session_id = session.id
                 WHERE session.id = ?1 AND session.deleted_at IS NULL
                 GROUP BY session.id",
                [session_id],
                map_session,
            )
            .optional()?;
        let Some(session) = session else {
            return Ok(None);
        };
        let mut statement = conn.prepare(
            "SELECT id, session_id, role, content, evidence_only, answer_json, created_at
             FROM knowledge_qa_messages
             WHERE session_id = ?1
             ORDER BY sequence_no ASC, id ASC",
        )?;
        let messages = statement
            .query_map([session_id], map_message)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(KnowledgeQaSessionDetail { session, messages }))
    }

    pub(crate) fn persist_knowledge_qa_round(
        &self,
        input: &PersistKnowledgeQaRoundInput,
        title: &str,
        release_commit_sha: &str,
    ) -> Result<i64, AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let session_id = if let Some(session_id) = input.session_id {
            let scope = tx
                .query_row(
                    "SELECT project_id, project_version_id, release_commit_sha,
                            provider_key, model
                     FROM knowledge_qa_sessions
                     WHERE id = ?1 AND deleted_at IS NULL",
                    [session_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("问答会话不存在: {session_id}")))?;
            if scope
                != (
                    input.project_id,
                    input.project_version_id,
                    release_commit_sha.to_string(),
                    input.provider_key.clone(),
                    input.model.clone(),
                )
            {
                return Err(AppError::InvalidInput(
                    "问答会话范围与当前项目、版本或模型不一致".to_string(),
                ));
            }
            session_id
        } else {
            tx.execute(
                "INSERT INTO knowledge_qa_sessions
                    (project_id, project_version_id, release_commit_sha,
                     provider_key, model, title)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    input.project_id,
                    input.project_version_id,
                    release_commit_sha,
                    input.provider_key,
                    input.model,
                    title,
                ],
            )?;
            tx.last_insert_rowid()
        };
        let next_sequence: i64 = tx.query_row(
            "SELECT COALESCE(MAX(sequence_no), 0) + 1
             FROM knowledge_qa_messages WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO knowledge_qa_messages
                (session_id, sequence_no, role, content, evidence_only, answer_json)
             VALUES (?1, ?2, 'user', ?3, ?4, NULL)",
            params![
                session_id,
                next_sequence,
                input.question,
                input.evidence_only as i64,
            ],
        )?;
        tx.execute(
            "INSERT INTO knowledge_qa_messages
                (session_id, sequence_no, role, content, evidence_only, answer_json)
             VALUES (?1, ?2, 'assistant', ?3, ?4, ?5)",
            params![
                session_id,
                next_sequence + 1,
                input.answer.answer,
                input.evidence_only as i64,
                serde_json::to_string(&input.answer)?,
            ],
        )?;
        tx.execute(
            "UPDATE knowledge_qa_sessions
             SET updated_at = datetime('now', 'localtime') WHERE id = ?1",
            [session_id],
        )?;
        tx.commit()?;
        Ok(session_id)
    }

    pub(crate) fn soft_delete_knowledge_qa_session(
        &self,
        project_id: i64,
        session_id: i64,
    ) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let affected = conn.execute(
            "UPDATE knowledge_qa_sessions
             SET deleted_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND project_id = ?2 AND deleted_at IS NULL",
            params![session_id, project_id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("问答会话不存在: {session_id}")));
        }
        Ok(())
    }
}

fn map_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeQaSession> {
    Ok(KnowledgeQaSession {
        id: row.get(0)?,
        project_id: row.get(1)?,
        project_version_id: row.get(2)?,
        release_commit_sha: row.get(3)?,
        provider_key: row.get(4)?,
        model: row.get(5)?,
        title: row.get(6)?,
        message_count: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn map_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeQaMessage> {
    let answer_json: Option<String> = row.get(5)?;
    let answer = answer_json
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()?;
    Ok(KnowledgeQaMessage {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        evidence_only: row.get::<_, i64>(4)? != 0,
        answer,
        created_at: row.get(6)?,
    })
}
