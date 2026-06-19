pub mod schema;

use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::AppError;
use crate::models::AppConfig;

/// 数据库封装，线程安全
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// 初始化数据库（创建或打开 + 自动迁移）
    pub fn init(db_path: &str) -> Result<Self, AppError> {
        let conn = Connection::open(db_path)?;

        // 启用 WAL 模式提升并发性能
        conn.pragma_update(None, "journal_mode", "WAL")?;

        // 设置忙等待超时，防止并发写入死锁
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        // 执行 Schema 迁移
        schema::migrate(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ─── 配置 DAO ────────────────────────────────────

    /// 获取所有配置（排除已软删除的）
    pub fn get_all_config(&self) -> Result<Vec<AppConfig>, AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT key, value FROM app_config WHERE deleted_at IS NULL ORDER BY key",
        )?;
        let configs = stmt
            .query_map([], |row| {
                Ok(AppConfig {
                    key: row.get(0)?,
                    value: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(configs)
    }

    /// 获取单个配置（排除已软删除的）
    pub fn get_config(&self, key: &str) -> Result<Option<String>, AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT value FROM app_config WHERE key = ?1 AND deleted_at IS NULL",
        )?;
        let result = stmt
            .query_row([key], |row| row.get::<_, String>(0))
            .ok();
        Ok(result)
    }

    /// 设置配置（upsert）
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "INSERT INTO app_config (key, value, updated_at)
             VALUES (?1, ?2, datetime('now', 'localtime'))
             ON CONFLICT(key) DO UPDATE SET
               value = excluded.value,
               updated_at = excluded.updated_at",
            [key, value],
        )?;
        Ok(())
    }

    /// 软删除配置（设置 deleted_at 而非物理删除）
    pub fn delete_config(&self, key: &str) -> Result<bool, AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE app_config SET deleted_at = datetime('now', 'localtime') WHERE key = ?1 AND deleted_at IS NULL",
            [key],
        )?;
        Ok(affected > 0)
    }

    /// 物理删除配置（永久删除，不可恢复）
    pub fn hard_delete_config(&self, key: &str) -> Result<bool, AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute("DELETE FROM app_config WHERE key = ?1", [key])?;
        Ok(affected > 0)
    }

    /// 恢复已软删除的配置
    pub fn restore_config(&self, key: &str) -> Result<bool, AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE app_config SET deleted_at = NULL WHERE key = ?1 AND deleted_at IS NOT NULL",
            [key],
        )?;
        Ok(affected > 0)
    }
}
