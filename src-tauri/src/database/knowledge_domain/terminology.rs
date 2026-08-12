use rusqlite::{params, OptionalExtension};

use crate::database::Database;
use crate::error::AppError;
use crate::models::knowledge_domain::terminology::{
    KnowledgeProjectTerm, UpsertKnowledgeProjectTermInput,
};

impl Database {
    pub(crate) fn list_knowledge_project_terms(
        &self,
        project_id: i64,
    ) -> Result<Vec<KnowledgeProjectTerm>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, project_id, term, aliases_json, confirmation_note, created_by,
                    created_at, updated_at
             FROM knowledge_project_terms
             WHERE project_id = ?1 AND deleted_at IS NULL
             ORDER BY lower(term) ASC, id ASC",
        )?;
        let terms = statement
            .query_map([project_id], map_knowledge_project_term)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(terms)
    }

    pub(crate) fn get_knowledge_project_term(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeProjectTerm>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, project_id, term, aliases_json, confirmation_note, created_by,
                    created_at, updated_at
             FROM knowledge_project_terms
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            map_knowledge_project_term,
        )
        .optional()
        .map_err(Into::into)
    }

    pub(crate) fn upsert_knowledge_project_term(
        &self,
        input: &UpsertKnowledgeProjectTermInput,
        normalized_term: &str,
        aliases_json: &str,
        created_by: &str,
    ) -> Result<KnowledgeProjectTerm, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let id = if let Some(id) = input.id {
            let affected = conn.execute(
                "UPDATE knowledge_project_terms
                 SET term = ?1, normalized_term = ?2, aliases_json = ?3,
                     confirmation_note = ?4, created_by = ?5,
                     updated_at = datetime('now', 'localtime')
                 WHERE id = ?6 AND project_id = ?7 AND deleted_at IS NULL",
                params![
                    input.term,
                    normalized_term,
                    aliases_json,
                    input.confirmation_note,
                    created_by,
                    id,
                    input.project_id,
                ],
            )?;
            if affected == 0 {
                return Err(AppError::NotFound(format!("项目术语不存在: {id}")));
            }
            id
        } else {
            conn.execute(
                "INSERT INTO knowledge_project_terms
                    (project_id, term, normalized_term, aliases_json, confirmation_note, created_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    input.project_id,
                    input.term,
                    normalized_term,
                    aliases_json,
                    input.confirmation_note,
                    created_by,
                ],
            )?;
            conn.last_insert_rowid()
        };
        drop(conn);
        self.get_knowledge_project_term(id)?
            .ok_or_else(|| AppError::Custom("保存项目术语后无法读取结果".to_string()))
    }

    pub(crate) fn soft_delete_knowledge_project_term(&self, id: i64) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let affected = conn.execute(
            "UPDATE knowledge_project_terms
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("项目术语不存在: {id}")));
        }
        Ok(())
    }

    /// 分页快照覆盖术语新增、修改和删除；不包含正文或用户身份之外的任何敏感内容。
    pub(crate) fn get_knowledge_project_term_snapshot(
        &self,
        project_id: i64,
    ) -> Result<String, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT COALESCE(GROUP_CONCAT(snapshot_part, '|'), '') FROM (
                 SELECT id || ':' || normalized_term || ':' || aliases_json || ':' || updated_at
                 AS snapshot_part
                 FROM knowledge_project_terms
                 WHERE project_id = ?1 AND deleted_at IS NULL
                 ORDER BY id ASC
             )",
            [project_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }
}

fn map_knowledge_project_term(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeProjectTerm> {
    let aliases_json: String = row.get(3)?;
    let aliases = serde_json::from_str(&aliases_json).unwrap_or_default();
    Ok(KnowledgeProjectTerm {
        id: row.get(0)?,
        project_id: row.get(1)?,
        term: row.get(2)?,
        aliases,
        confirmation_note: row.get(4)?,
        created_by: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}
