use rusqlite::{params, OptionalExtension};

use crate::database::Database;
use crate::error::AppError;

pub(crate) const DOMAIN: &str = "jobs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeBackfillRunRecord {
    pub id: i64,
    pub backfill_type: String,
    pub checkpoint_json: String,
    pub status: String,
    pub processed_count: i64,
    pub failed_count: i64,
}

impl Database {
    /// 回填执行记录与任务执行分离：重试复用检查点，不依赖 UI 再次点击。
    pub(crate) fn create_knowledge_backfill_run(
        &self,
        backfill_type: &str,
        checkpoint_json: &str,
    ) -> Result<KnowledgeBackfillRunRecord, AppError> {
        if backfill_type.trim().is_empty() {
            return Err(AppError::InvalidInput("回填类型不能为空".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_backfill_runs(backfill_type, checkpoint_json)
             VALUES (?1, ?2)",
            params![backfill_type.trim(), checkpoint_json],
        )?;
        get_backfill_run(&conn, conn.last_insert_rowid())?
            .ok_or_else(|| AppError::Custom("创建回填记录后未找到记录".to_string()))
    }

    pub(crate) fn update_knowledge_backfill_run(
        &self,
        id: i64,
        checkpoint_json: &str,
        status: &str,
        processed_count: i64,
        failed_count: i64,
    ) -> Result<KnowledgeBackfillRunRecord, AppError> {
        if id <= 0 || status.trim().is_empty() || processed_count < 0 || failed_count < 0 {
            return Err(AppError::InvalidInput("回填状态或计数无效".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_backfill_runs
             SET checkpoint_json = ?2, status = ?3, processed_count = ?4, failed_count = ?5,
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            params![
                id,
                checkpoint_json,
                status.trim(),
                processed_count,
                failed_count
            ],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("回填记录不存在: {id}")));
        }
        get_backfill_run(&conn, id)?
            .ok_or_else(|| AppError::NotFound(format!("回填记录不存在: {id}")))
    }
}

fn get_backfill_run(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<Option<KnowledgeBackfillRunRecord>, AppError> {
    conn.query_row(
        "SELECT id, backfill_type, checkpoint_json, status, processed_count, failed_count
         FROM knowledge_backfill_runs WHERE id = ?1",
        [id],
        |row| {
            Ok(KnowledgeBackfillRunRecord {
                id: row.get(0)?,
                backfill_type: row.get(1)?,
                checkpoint_json: row.get(2)?,
                status: row.get(3)?,
                processed_count: row.get(4)?,
                failed_count: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;

    use super::Database;
    use crate::database::schema;

    #[test]
    fn backfill_checkpoint_survives_failed_run_and_retry_update(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        let run = database.create_knowledge_backfill_run("title-index", "{\"lastId\":10}")?;
        let failed =
            database.update_knowledge_backfill_run(run.id, "{\"lastId\":12}", "failed", 12, 1)?;
        assert_eq!(failed.checkpoint_json, "{\"lastId\":12}");
        let retried = database.update_knowledge_backfill_run(
            run.id,
            failed.checkpoint_json.as_str(),
            "queued",
            12,
            1,
        )?;
        assert_eq!(retried.status, "queued");
        assert_eq!(retried.processed_count, 12);
        Ok(())
    }
}
