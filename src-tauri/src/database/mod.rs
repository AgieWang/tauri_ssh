pub mod schema;

use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppError;
use crate::models::{
    AiExperience, AiProvider, AiProviderRoute, AiRunbook, AiRunbookStep, AiSkill, AiSkillStats,
    AppConfig, ApprovalRequest, AuditLog, CreateApprovalRequestInput, CreateAuditLogInput,
    CredentialVaultItem, DatabaseConnection, DecideApprovalRequestInput, JumpServerSession,
    ListAiSkillsInput, ListApprovalRequestsInput, ListAuditLogsInput, ListResourceAlertEventsInput,
    ListResourceAlertRulesInput, ResourceAlertEvent, ResourceAlertRule, ResourceMetricSnapshot,
    ResourceMonitorTarget, ResourceSnapshotListInput, SshServer, UpsertAiExperienceInput,
    UpsertAiProviderInput, UpsertAiProviderRouteInput, UpsertAiRunbookInput, UpsertAiSkillInput,
    UpsertCredentialInput, UpsertDatabaseConnectionInput, UpsertJumpServerSessionInput,
    UpsertResourceAlertRuleInput, UpsertResourceMonitorTargetInput, UpsertSshServerInput,
};

pub struct AiProviderSecretRow {
    pub provider: AiProvider,
    pub secret_nonce: Option<String>,
    pub secret_ciphertext: Option<String>,
}

pub struct SshServerSecretRow {
    pub server: SshServer,
    pub password_nonce: Option<String>,
    pub password_ciphertext: Option<String>,
}

pub struct CredentialSecretRow {
    pub secret_nonce: Option<String>,
    pub secret_ciphertext: Option<String>,
}

#[allow(dead_code)]
pub struct DatabaseConnectionSecretRow {
    pub connection: DatabaseConnection,
    pub password_nonce: Option<String>,
    pub password_ciphertext: Option<String>,
}

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
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM app_config WHERE deleted_at IS NULL ORDER BY key")?;
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
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt =
            conn.prepare("SELECT value FROM app_config WHERE key = ?1 AND deleted_at IS NULL")?;
        let result = stmt.query_row([key], |row| row.get::<_, String>(0)).ok();
        Ok(result)
    }

    /// 设置配置（upsert）
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
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
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE app_config SET deleted_at = datetime('now', 'localtime') WHERE key = ?1 AND deleted_at IS NULL",
            [key],
        )?;
        Ok(affected > 0)
    }

    /// 物理删除配置（永久删除，不可恢复）
    #[allow(dead_code)]
    pub fn hard_delete_config(&self, key: &str) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute("DELETE FROM app_config WHERE key = ?1", [key])?;
        Ok(affected > 0)
    }

    /// 恢复已软删除的配置
    #[allow(dead_code)]
    pub fn restore_config(&self, key: &str) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE app_config SET deleted_at = NULL WHERE key = ?1 AND deleted_at IS NOT NULL",
            [key],
        )?;
        Ok(affected > 0)
    }

    // ─── SSH 服务器 DAO ───────────────────────────────

    pub fn list_ssh_servers(&self) -> Result<Vec<SshServer>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT alias, group_name, host, port, username, source, auth_type, auth_ref,
                    identity_file, password_ciphertext IS NOT NULL AS has_password,
                    proxy_jump, ai_policy, status, enabled, last_connected_at, updated_at
             FROM ssh_servers
             WHERE deleted_at IS NULL
             ORDER BY group_name, alias",
        )?;
        let rows = stmt
            .query_map([], |row| self.map_ssh_server_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_ssh_server(&self, alias: &str) -> Result<Option<SshServer>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT alias, group_name, host, port, username, source, auth_type, auth_ref,
                    identity_file, password_ciphertext IS NOT NULL AS has_password,
                    proxy_jump, ai_policy, status, enabled, last_connected_at, updated_at
             FROM ssh_servers
             WHERE alias = ?1 AND deleted_at IS NULL",
            [alias],
            |row| self.map_ssh_server_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn get_ssh_server_secret_row(
        &self,
        alias: &str,
    ) -> Result<Option<SshServerSecretRow>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT alias, group_name, host, port, username, source, auth_type, auth_ref,
                    identity_file, password_ciphertext IS NOT NULL AS has_password,
                    proxy_jump, ai_policy, status, enabled, last_connected_at, updated_at,
                    password_nonce, password_ciphertext
             FROM ssh_servers
             WHERE alias = ?1 AND deleted_at IS NULL",
            [alias],
            |row| {
                Ok(SshServerSecretRow {
                    server: self.map_ssh_server_row(row)?,
                    password_nonce: row.get(16)?,
                    password_ciphertext: row.get(17)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn upsert_ssh_server(
        &self,
        input: &UpsertSshServerInput,
        encrypted_password: Option<(&str, &str)>,
        clear_password: bool,
    ) -> Result<SshServer, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let enabled = if input.enabled { 1 } else { 0 };
        let status = input.status.as_deref().unwrap_or("unknown");
        conn.execute(
            "INSERT INTO ssh_servers
             (alias, group_name, host, port, username, source, auth_type, auth_ref, identity_file,
              password_nonce, password_ciphertext, proxy_jump, ai_policy, status, enabled, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, datetime('now', 'localtime'), NULL)
             ON CONFLICT(alias) DO UPDATE SET
               group_name = excluded.group_name,
               host = excluded.host,
               port = excluded.port,
               username = excluded.username,
               source = excluded.source,
               auth_type = excluded.auth_type,
               auth_ref = excluded.auth_ref,
               identity_file = excluded.identity_file,
               password_nonce = CASE
                 WHEN ?16 THEN NULL
                 WHEN ?10 IS NOT NULL THEN excluded.password_nonce
                 ELSE ssh_servers.password_nonce
               END,
               password_ciphertext = CASE
                 WHEN ?16 THEN NULL
                 WHEN ?11 IS NOT NULL THEN excluded.password_ciphertext
                 ELSE ssh_servers.password_ciphertext
               END,
               proxy_jump = excluded.proxy_jump,
               ai_policy = excluded.ai_policy,
               status = excluded.status,
               enabled = excluded.enabled,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                input.alias,
                input.group_name,
                input.host,
                input.port,
                input.username,
                input.source,
                input.auth_type,
                input.auth_ref,
                input.identity_file,
                encrypted_password.map(|v| v.0),
                encrypted_password.map(|v| v.1),
                input.proxy_jump,
                input.ai_policy,
                status,
                enabled,
                clear_password
            ],
        )?;
        drop(conn);

        self.get_ssh_server(&input.alias)?
            .ok_or_else(|| AppError::NotFound(format!("SSH 服务器 '{}' 不存在", input.alias)))
    }

    pub fn delete_ssh_server(&self, alias: &str) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE ssh_servers
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE alias = ?1 AND deleted_at IS NULL",
            [alias],
        )?;
        Ok(affected > 0)
    }

    pub fn update_ssh_server_status(
        &self,
        alias: &str,
        status: &str,
        connected: bool,
    ) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        if connected {
            conn.execute(
                "UPDATE ssh_servers
                 SET status = ?2, last_connected_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
                 WHERE alias = ?1 AND deleted_at IS NULL",
                params![alias, status],
            )?;
        } else {
            conn.execute(
                "UPDATE ssh_servers
                 SET status = ?2, updated_at = datetime('now', 'localtime')
                 WHERE alias = ?1 AND deleted_at IS NULL",
                params![alias, status],
            )?;
        }
        Ok(())
    }

    // ─── 凭据保险库 DAO ───────────────────────────────

    pub fn list_credentials(&self) -> Result<Vec<CredentialVaultItem>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT key, credential_type, scope, status, description,
                    secret_ciphertext IS NOT NULL AS has_secret,
                    enabled, rotated_at, updated_at
             FROM credential_vault
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC, key",
        )?;
        let rows = stmt
            .query_map([], |row| self.map_credential_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_credential(&self, key: &str) -> Result<Option<CredentialVaultItem>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT key, credential_type, scope, status, description,
                    secret_ciphertext IS NOT NULL AS has_secret,
                    enabled, rotated_at, updated_at
             FROM credential_vault
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
            |row| self.map_credential_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn upsert_credential(
        &self,
        input: &UpsertCredentialInput,
        encrypted_secret: Option<(&str, &str)>,
        clear_secret: bool,
    ) -> Result<CredentialVaultItem, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let enabled = if input.enabled { 1 } else { 0 };
        let status = input.status.as_deref().unwrap_or("normal");
        conn.execute(
            "INSERT INTO credential_vault
             (key, credential_type, scope, status, description, secret_nonce, secret_ciphertext,
              enabled, rotated_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                     CASE WHEN ?7 IS NOT NULL THEN datetime('now', 'localtime') ELSE NULL END,
                     datetime('now', 'localtime'), NULL)
             ON CONFLICT(key) DO UPDATE SET
               credential_type = excluded.credential_type,
               scope = excluded.scope,
               status = excluded.status,
               description = excluded.description,
               secret_nonce = CASE
                 WHEN ?9 THEN NULL
                 WHEN ?6 IS NOT NULL THEN excluded.secret_nonce
                 ELSE credential_vault.secret_nonce
               END,
               secret_ciphertext = CASE
                 WHEN ?9 THEN NULL
                 WHEN ?7 IS NOT NULL THEN excluded.secret_ciphertext
                 ELSE credential_vault.secret_ciphertext
               END,
               enabled = excluded.enabled,
               rotated_at = CASE
                 WHEN ?9 THEN NULL
                 WHEN ?7 IS NOT NULL THEN datetime('now', 'localtime')
                 ELSE credential_vault.rotated_at
               END,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                input.key,
                input.credential_type,
                input.scope,
                status,
                input.description,
                encrypted_secret.map(|v| v.0),
                encrypted_secret.map(|v| v.1),
                enabled,
                clear_secret
            ],
        )?;
        drop(conn);

        self.get_credential(&input.key)?
            .ok_or_else(|| AppError::NotFound(format!("凭据 '{}' 不存在", input.key)))
    }

    pub fn get_credential_secret_row(
        &self,
        key: &str,
    ) -> Result<Option<CredentialSecretRow>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT secret_nonce, secret_ciphertext
             FROM credential_vault
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
            |row| {
                Ok(CredentialSecretRow {
                    secret_nonce: row.get(0)?,
                    secret_ciphertext: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn authorize_credential(
        &self,
        key: &str,
        scope: &str,
    ) -> Result<CredentialVaultItem, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE credential_vault
             SET scope = ?2, updated_at = datetime('now', 'localtime')
             WHERE key = ?1 AND deleted_at IS NULL",
            params![key, scope],
        )?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NotFound(format!("凭据 '{}' 不存在", key)));
        }
        self.get_credential(key)?
            .ok_or_else(|| AppError::NotFound(format!("凭据 '{}' 不存在", key)))
    }

    pub fn rotate_credential(
        &self,
        key: &str,
        encrypted_secret: (&str, &str),
    ) -> Result<CredentialVaultItem, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE credential_vault
             SET secret_nonce = ?2,
                 secret_ciphertext = ?3,
                 status = 'normal',
                 rotated_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime')
             WHERE key = ?1 AND deleted_at IS NULL",
            params![key, encrypted_secret.0, encrypted_secret.1],
        )?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NotFound(format!("凭据 '{}' 不存在", key)));
        }
        self.get_credential(key)?
            .ok_or_else(|| AppError::NotFound(format!("凭据 '{}' 不存在", key)))
    }

    pub fn delete_credential(&self, key: &str) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE credential_vault
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
        )?;
        Ok(affected > 0)
    }

    // ─── 数据库管理 DAO ───────────────────────────────

    pub fn list_database_connections(&self) -> Result<Vec<DatabaseConnection>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT key, name, group_name, db_type, connection_mode, host, port, database_name,
                    username, auth_type, credential_ref, password_ciphertext IS NOT NULL AS has_password,
                    ssh_server_alias, security_mode, ai_policy, page_size, status, enabled,
                    last_connected_at, notes, updated_at
             FROM database_connections
             WHERE deleted_at IS NULL
             ORDER BY group_name, name",
        )?;
        let rows = stmt
            .query_map([], |row| self.map_database_connection_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_database_connection(
        &self,
        key: &str,
    ) -> Result<Option<DatabaseConnection>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT key, name, group_name, db_type, connection_mode, host, port, database_name,
                    username, auth_type, credential_ref, password_ciphertext IS NOT NULL AS has_password,
                    ssh_server_alias, security_mode, ai_policy, page_size, status, enabled,
                    last_connected_at, notes, updated_at
             FROM database_connections
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
            |row| self.map_database_connection_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    #[allow(dead_code)]
    pub fn get_database_connection_secret_row(
        &self,
        key: &str,
    ) -> Result<Option<DatabaseConnectionSecretRow>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT key, name, group_name, db_type, connection_mode, host, port, database_name,
                    username, auth_type, credential_ref, password_ciphertext IS NOT NULL AS has_password,
                    ssh_server_alias, security_mode, ai_policy, page_size, status, enabled,
                    last_connected_at, notes, updated_at, password_nonce, password_ciphertext
             FROM database_connections
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
            |row| {
                Ok(DatabaseConnectionSecretRow {
                    connection: self.map_database_connection_row(row)?,
                    password_nonce: row.get(21)?,
                    password_ciphertext: row.get(22)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn upsert_database_connection(
        &self,
        input: &UpsertDatabaseConnectionInput,
        encrypted_password: Option<(&str, &str)>,
        clear_password: bool,
    ) -> Result<DatabaseConnection, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let enabled = if input.enabled { 1 } else { 0 };
        let status = input.status.as_deref().unwrap_or("unknown");
        conn.execute(
            "INSERT INTO database_connections
             (key, name, group_name, db_type, connection_mode, host, port, database_name, username,
              auth_type, credential_ref, password_nonce, password_ciphertext, ssh_server_alias,
              security_mode, ai_policy, page_size, status, enabled, notes, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20, datetime('now', 'localtime'), NULL)
             ON CONFLICT(key) DO UPDATE SET
               name = excluded.name,
               group_name = excluded.group_name,
               db_type = excluded.db_type,
               connection_mode = excluded.connection_mode,
               host = excluded.host,
               port = excluded.port,
               database_name = excluded.database_name,
               username = excluded.username,
               auth_type = excluded.auth_type,
               credential_ref = excluded.credential_ref,
               password_nonce = CASE
                 WHEN ?21 THEN NULL
                 WHEN ?12 IS NOT NULL THEN excluded.password_nonce
                 ELSE database_connections.password_nonce
               END,
               password_ciphertext = CASE
                 WHEN ?21 THEN NULL
                 WHEN ?13 IS NOT NULL THEN excluded.password_ciphertext
                 ELSE database_connections.password_ciphertext
               END,
               ssh_server_alias = excluded.ssh_server_alias,
               security_mode = excluded.security_mode,
               ai_policy = excluded.ai_policy,
               page_size = excluded.page_size,
               status = excluded.status,
               enabled = excluded.enabled,
               notes = excluded.notes,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                input.key,
                input.name,
                input.group_name,
                input.db_type,
                input.connection_mode,
                input.host,
                input.port,
                input.database_name,
                input.username,
                input.auth_type,
                input.credential_ref,
                encrypted_password.map(|v| v.0),
                encrypted_password.map(|v| v.1),
                input.ssh_server_alias,
                input.security_mode,
                input.ai_policy,
                input.page_size,
                status,
                enabled,
                input.notes,
                clear_password
            ],
        )?;
        drop(conn);

        self.get_database_connection(&input.key)?
            .ok_or_else(|| AppError::NotFound(format!("数据库连接 '{}' 不存在", input.key)))
    }

    pub fn delete_database_connection(&self, key: &str) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE database_connections
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
        )?;
        Ok(affected > 0)
    }

    pub fn update_database_connection_status(
        &self,
        key: &str,
        status: &str,
        connected: bool,
    ) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        if connected {
            conn.execute(
                "UPDATE database_connections
                 SET status = ?2, last_connected_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
                 WHERE key = ?1 AND deleted_at IS NULL",
                params![key, status],
            )?;
        } else {
            conn.execute(
                "UPDATE database_connections
                 SET status = ?2, updated_at = datetime('now', 'localtime')
                 WHERE key = ?1 AND deleted_at IS NULL",
                params![key, status],
            )?;
        }
        Ok(())
    }

    // ─── 审批队列 DAO ───────────────────────────────

    pub fn list_approval_requests(
        &self,
        input: &ListApprovalRequestsInput,
    ) -> Result<Vec<ApprovalRequest>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let status = input.status.as_deref().unwrap_or("all");
        let limit = input.limit.unwrap_or(100).clamp(1, 500);
        if status == "all" {
            let mut stmt = conn.prepare(
                "SELECT id, source, requester, server_alias, action, risk, status, command,
                        resource, reason, summary, payload_json, decision_note, decided_by,
                        decided_at, expires_at, created_at, updated_at
                 FROM approval_requests
                 WHERE deleted_at IS NULL
                 ORDER BY
                   CASE status WHEN 'pending' THEN 0 ELSE 1 END,
                   created_at DESC
                 LIMIT ?1",
            )?;
            let rows = stmt
                .query_map([limit], |row| self.map_approval_request_row(row))?
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(rows);
        }

        let mut stmt = conn.prepare(
            "SELECT id, source, requester, server_alias, action, risk, status, command,
                    resource, reason, summary, payload_json, decision_note, decided_by,
                    decided_at, expires_at, created_at, updated_at
             FROM approval_requests
             WHERE deleted_at IS NULL AND status = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![status, limit], |row| {
                self.map_approval_request_row(row)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_approval_request(&self, id: i64) -> Result<Option<ApprovalRequest>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, source, requester, server_alias, action, risk, status, command,
                    resource, reason, summary, payload_json, decision_note, decided_by,
                    decided_at, expires_at, created_at, updated_at
             FROM approval_requests
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |row| self.map_approval_request_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn create_approval_request(
        &self,
        input: &CreateApprovalRequestInput,
    ) -> Result<ApprovalRequest, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let payload_json = input.payload_json.as_deref().unwrap_or("{}");
        conn.execute(
            "INSERT INTO approval_requests
             (source, requester, server_alias, action, risk, status, command, resource, reason,
              summary, payload_json, expires_at, created_at, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, ?9, ?10, ?11,
                     datetime('now', 'localtime'), datetime('now', 'localtime'), NULL)",
            params![
                input.source,
                input.requester,
                input.server_alias,
                input.action,
                input.risk,
                input.command,
                input.resource,
                input.reason,
                input.summary,
                payload_json,
                input.expires_at
            ],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_approval_request(id)?
            .ok_or_else(|| AppError::NotFound(format!("审批请求 '{}' 不存在", id)))
    }

    pub fn decide_approval_request(
        &self,
        input: &DecideApprovalRequestInput,
    ) -> Result<ApprovalRequest, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE approval_requests
             SET status = ?2,
                 decision_note = ?3,
                 decided_by = ?4,
                 decided_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status = 'pending' AND deleted_at IS NULL",
            params![input.id, input.decision, input.note, input.decided_by],
        )?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::InvalidInput(
                "审批请求不存在，或当前状态已不可决策".into(),
            ));
        }
        self.get_approval_request(input.id)?
            .ok_or_else(|| AppError::NotFound(format!("审批请求 '{}' 不存在", input.id)))
    }

    // ─── 堡垒机会话 DAO ───────────────────────────────

    pub fn list_jumpserver_sessions(&self) -> Result<Vec<JumpServerSession>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT key, name, endpoint, web_url, session_ref, group_name, account_hint,
                    asset_hint, protocol, ai_mode, status, notes, enabled, last_opened_at, updated_at
             FROM jumpserver_sessions
             WHERE deleted_at IS NULL
             ORDER BY enabled DESC, group_name, name",
        )?;
        let rows = stmt
            .query_map([], |row| self.map_jumpserver_session_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_jumpserver_session(&self, key: &str) -> Result<Option<JumpServerSession>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT key, name, endpoint, web_url, session_ref, group_name, account_hint,
                    asset_hint, protocol, ai_mode, status, notes, enabled, last_opened_at, updated_at
             FROM jumpserver_sessions
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
            |row| self.map_jumpserver_session_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn upsert_jumpserver_session(
        &self,
        input: &UpsertJumpServerSessionInput,
    ) -> Result<JumpServerSession, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let enabled = if input.enabled { 1 } else { 0 };
        let status = input.status.as_deref().unwrap_or("unknown");
        conn.execute(
            "INSERT INTO jumpserver_sessions
             (key, name, endpoint, web_url, session_ref, group_name, account_hint, asset_hint,
              protocol, ai_mode, status, notes, enabled, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     datetime('now', 'localtime'), NULL)
             ON CONFLICT(key) DO UPDATE SET
               name = excluded.name,
               endpoint = excluded.endpoint,
               web_url = excluded.web_url,
               session_ref = excluded.session_ref,
               group_name = excluded.group_name,
               account_hint = excluded.account_hint,
               asset_hint = excluded.asset_hint,
               protocol = excluded.protocol,
               ai_mode = excluded.ai_mode,
               status = excluded.status,
               notes = excluded.notes,
               enabled = excluded.enabled,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                input.key,
                input.name,
                input.endpoint,
                input.web_url,
                input.session_ref,
                input.group_name,
                input.account_hint,
                input.asset_hint,
                input.protocol,
                input.ai_mode,
                status,
                input.notes,
                enabled
            ],
        )?;
        drop(conn);
        self.get_jumpserver_session(&input.key)?
            .ok_or_else(|| AppError::NotFound(format!("堡垒机会话 '{}' 不存在", input.key)))
    }

    pub fn mark_jumpserver_session_opened(&self, key: &str) -> Result<JumpServerSession, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE jumpserver_sessions
             SET status = 'opened',
                 last_opened_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime')
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
        )?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NotFound(format!("堡垒机会话 '{}' 不存在", key)));
        }
        self.get_jumpserver_session(key)?
            .ok_or_else(|| AppError::NotFound(format!("堡垒机会话 '{}' 不存在", key)))
    }

    pub fn delete_jumpserver_session(&self, key: &str) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE jumpserver_sessions
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
        )?;
        Ok(affected > 0)
    }

    // ─── 资源监控 DAO ───────────────────────────────

    pub fn upsert_resource_monitor_target(
        &self,
        input: &UpsertResourceMonitorTargetInput,
        fallback_name: &str,
    ) -> Result<ResourceMonitorTarget, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let display_name = input
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_name);
        let enabled = input.enabled.unwrap_or(true);
        let interval = input.collect_interval_sec.unwrap_or(60).clamp(30, 86400);
        conn.execute(
            "INSERT INTO resource_monitor_targets
             (target_type, target_key, display_name, enabled, collect_interval_sec, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now', 'localtime'), NULL)
             ON CONFLICT(target_type, target_key) DO UPDATE SET
               display_name = excluded.display_name,
               enabled = excluded.enabled,
               collect_interval_sec = excluded.collect_interval_sec,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                input.target_type,
                input.target_key,
                display_name,
                if enabled { 1 } else { 0 },
                interval
            ],
        )?;
        drop(conn);
        self.get_resource_monitor_target(&input.target_type, &input.target_key)?
            .ok_or_else(|| AppError::NotFound("资源监控目标不存在".into()))
    }

    pub fn ensure_resource_monitor_target(
        &self,
        target_type: &str,
        target_key: &str,
        display_name: &str,
    ) -> Result<ResourceMonitorTarget, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "INSERT INTO resource_monitor_targets
             (target_type, target_key, display_name, enabled, collect_interval_sec)
             VALUES (?1, ?2, ?3, 1, 60)
             ON CONFLICT(target_type, target_key) DO UPDATE SET
               display_name = CASE
                 WHEN resource_monitor_targets.display_name = '' THEN excluded.display_name
                 ELSE resource_monitor_targets.display_name
               END,
               deleted_at = NULL",
            params![target_type, target_key, display_name],
        )?;
        drop(conn);
        self.get_resource_monitor_target(target_type, target_key)?
            .ok_or_else(|| AppError::NotFound("资源监控目标不存在".into()))
    }

    pub fn get_resource_monitor_target(
        &self,
        target_type: &str,
        target_key: &str,
    ) -> Result<Option<ResourceMonitorTarget>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, target_type, target_key, display_name, enabled, collect_interval_sec,
                    last_status, last_collected_at, last_error, updated_at
             FROM resource_monitor_targets
             WHERE target_type = ?1 AND target_key = ?2 AND deleted_at IS NULL",
            params![target_type, target_key],
            |row| self.map_resource_monitor_target_row(row, "", None),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn list_resource_monitor_targets(&self) -> Result<Vec<ResourceMonitorTarget>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, target_type, target_key, display_name, enabled, collect_interval_sec,
                    last_status, last_collected_at, last_error, updated_at
             FROM resource_monitor_targets
             WHERE deleted_at IS NULL
             ORDER BY target_type, display_name",
        )?;
        let rows = stmt
            .query_map([], |row| {
                self.map_resource_monitor_target_row(row, "", None)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_resource_monitor_target(
        &self,
        target_type: &str,
        target_key: &str,
    ) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE resource_monitor_targets
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE target_type = ?1 AND target_key = ?2 AND deleted_at IS NULL",
            params![target_type, target_key],
        )?;
        Ok(affected > 0)
    }

    pub fn save_resource_metric_snapshot(
        &self,
        target_type: &str,
        target_key: &str,
        status: &str,
        duration_ms: i64,
        summary: &serde_json::Value,
        metrics: &serde_json::Value,
        error: Option<&str>,
    ) -> Result<ResourceMetricSnapshot, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let summary_json = serde_json::to_string(summary)?;
        let metrics_json = serde_json::to_string(metrics)?;
        conn.execute(
            "INSERT INTO resource_metric_snapshots
             (target_type, target_key, status, duration_ms, summary_json, metrics_json, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                target_type,
                target_key,
                status,
                duration_ms,
                summary_json,
                metrics_json,
                error
            ],
        )?;
        let id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE resource_monitor_targets
             SET last_status = ?3,
                 last_collected_at = (SELECT collected_at FROM resource_metric_snapshots WHERE id = ?4),
                 last_error = ?5,
                 updated_at = datetime('now', 'localtime')
             WHERE target_type = ?1 AND target_key = ?2 AND deleted_at IS NULL",
            params![target_type, target_key, status, id, error],
        )?;
        drop(conn);
        self.get_resource_metric_snapshot(id)?
            .ok_or_else(|| AppError::NotFound(format!("资源快照 '{}' 不存在", id)))
    }

    pub fn get_resource_metric_snapshot(
        &self,
        id: i64,
    ) -> Result<Option<ResourceMetricSnapshot>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, target_type, target_key, status, collected_at, duration_ms,
                    summary_json, metrics_json, error
             FROM resource_metric_snapshots
             WHERE id = ?1",
            [id],
            |row| self.map_resource_metric_snapshot_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn get_latest_resource_metric_snapshot(
        &self,
        target_type: &str,
        target_key: &str,
    ) -> Result<Option<ResourceMetricSnapshot>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, target_type, target_key, status, collected_at, duration_ms,
                    summary_json, metrics_json, error
             FROM resource_metric_snapshots
             WHERE target_type = ?1 AND target_key = ?2
             ORDER BY id DESC
             LIMIT 1",
            params![target_type, target_key],
            |row| self.map_resource_metric_snapshot_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn list_resource_metric_snapshots(
        &self,
        input: &ResourceSnapshotListInput,
    ) -> Result<Vec<ResourceMetricSnapshot>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let limit = input.limit.unwrap_or(200).clamp(1, 5000);
        let mut stmt = conn.prepare(
            "SELECT id, target_type, target_key, status, collected_at, duration_ms,
                    summary_json, metrics_json, error
             FROM resource_metric_snapshots
             WHERE (?1 IS NULL OR target_type = ?1)
               AND (?2 IS NULL OR target_key = ?2)
             ORDER BY id DESC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![input.target_type, input.target_key, limit], |row| {
                self.map_resource_metric_snapshot_row(row)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn list_resource_alert_rules(
        &self,
        input: &ListResourceAlertRulesInput,
    ) -> Result<Vec<ResourceAlertRule>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let enabled = input.enabled.map(|value| if value { 1 } else { 0 });
        let mut stmt = conn.prepare(
            "SELECT id, target_type, target_key, metric_key, operator, threshold_value,
                    severity, enabled, updated_at
             FROM resource_alert_rules
             WHERE deleted_at IS NULL
               AND (?1 IS NULL OR target_type = ?1)
               AND (?2 IS NULL OR target_key = ?2 OR target_key = '*')
               AND (?3 IS NULL OR enabled = ?3)
             ORDER BY target_type, target_key, metric_key",
        )?;
        let rows = stmt
            .query_map(
                params![input.target_type, input.target_key, enabled],
                |row| self.map_resource_alert_rule_row(row),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn upsert_resource_alert_rule(
        &self,
        input: &UpsertResourceAlertRuleInput,
    ) -> Result<ResourceAlertRule, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let target_key = input.target_key.as_deref().unwrap_or("*");
        let enabled = input.enabled.unwrap_or(true);
        let id = input.id.unwrap_or(0);
        if id > 0 {
            conn.execute(
                "UPDATE resource_alert_rules
                 SET target_type = ?2, target_key = ?3, metric_key = ?4, operator = ?5,
                     threshold_value = ?6, severity = ?7, enabled = ?8,
                     updated_at = datetime('now', 'localtime')
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![
                    id,
                    input.target_type,
                    target_key,
                    input.metric_key,
                    input.operator,
                    input.threshold_value,
                    input.severity,
                    if enabled { 1 } else { 0 }
                ],
            )?;
        } else {
            conn.execute(
                "INSERT INTO resource_alert_rules
                 (target_type, target_key, metric_key, operator, threshold_value, severity, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    input.target_type,
                    target_key,
                    input.metric_key,
                    input.operator,
                    input.threshold_value,
                    input.severity,
                    if enabled { 1 } else { 0 }
                ],
            )?;
        }
        let rule_id = if id > 0 { id } else { conn.last_insert_rowid() };
        drop(conn);
        self.get_resource_alert_rule(rule_id)?
            .ok_or_else(|| AppError::NotFound(format!("告警规则 '{}' 不存在", rule_id)))
    }

    pub fn delete_resource_alert_rule(&self, id: i64) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE resource_alert_rules
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        Ok(affected > 0)
    }

    pub fn get_resource_alert_rule(&self, id: i64) -> Result<Option<ResourceAlertRule>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, target_type, target_key, metric_key, operator, threshold_value,
                    severity, enabled, updated_at
             FROM resource_alert_rules
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |row| self.map_resource_alert_rule_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn open_or_update_resource_alert_event(
        &self,
        rule: &ResourceAlertRule,
        target_type: &str,
        target_key: &str,
        metric_value: f64,
        message: &str,
        snapshot_id: i64,
    ) -> Result<ResourceAlertEvent, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let existing_id = conn
            .query_row(
                "SELECT id FROM resource_alert_events
                 WHERE rule_id = ?1 AND target_type = ?2 AND target_key = ?3 AND status = 'open'
                 ORDER BY id DESC LIMIT 1",
                params![rule.id, target_type, target_key],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let id = if let Some(id) = existing_id {
            conn.execute(
                "UPDATE resource_alert_events
                 SET metric_value = ?2, threshold_value = ?3, message = ?4,
                     last_seen_at = datetime('now', 'localtime'), snapshot_id = ?5
                 WHERE id = ?1",
                params![id, metric_value, rule.threshold_value, message, snapshot_id],
            )?;
            id
        } else {
            conn.execute(
                "INSERT INTO resource_alert_events
                 (rule_id, target_type, target_key, severity, status, metric_key, metric_value,
                  threshold_value, message, snapshot_id)
                 VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6, ?7, ?8, ?9)",
                params![
                    rule.id,
                    target_type,
                    target_key,
                    rule.severity,
                    rule.metric_key,
                    metric_value,
                    rule.threshold_value,
                    message,
                    snapshot_id
                ],
            )?;
            conn.last_insert_rowid()
        };
        drop(conn);
        self.get_resource_alert_event(id)?
            .ok_or_else(|| AppError::NotFound(format!("告警事件 '{}' 不存在", id)))
    }

    pub fn auto_resolve_resource_alert_event(
        &self,
        rule_id: i64,
        target_type: &str,
        target_key: &str,
    ) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "UPDATE resource_alert_events
             SET status = 'resolved', resolved_at = datetime('now', 'localtime'),
                 last_seen_at = datetime('now', 'localtime')
             WHERE rule_id = ?1 AND target_type = ?2 AND target_key = ?3 AND status = 'open'",
            params![rule_id, target_type, target_key],
        )?;
        Ok(())
    }

    pub fn get_resource_alert_event(
        &self,
        id: i64,
    ) -> Result<Option<ResourceAlertEvent>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, rule_id, target_type, target_key, severity, status, metric_key,
                    metric_value, threshold_value, message, first_seen_at, last_seen_at,
                    resolved_at, snapshot_id
             FROM resource_alert_events
             WHERE id = ?1",
            [id],
            |row| self.map_resource_alert_event_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn resolve_resource_alert_event(&self, id: i64) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE resource_alert_events
             SET status = 'resolved', resolved_at = datetime('now', 'localtime'),
                 last_seen_at = datetime('now', 'localtime')
             WHERE id = ?1 AND status = 'open'",
            [id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!(
                "打开的告警事件 '{}' 不存在",
                id
            )));
        }
        Ok(())
    }

    pub fn list_resource_alert_events(
        &self,
        input: &ListResourceAlertEventsInput,
    ) -> Result<Vec<ResourceAlertEvent>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let limit = input.limit.unwrap_or(100).clamp(1, 1000);
        let mut stmt = conn.prepare(
            "SELECT id, rule_id, target_type, target_key, severity, status, metric_key,
                    metric_value, threshold_value, message, first_seen_at, last_seen_at,
                    resolved_at, snapshot_id
             FROM resource_alert_events
             WHERE (?1 IS NULL OR status = ?1)
               AND (?2 IS NULL OR target_type = ?2)
               AND (?3 IS NULL OR target_key = ?3)
             ORDER BY CASE status WHEN 'open' THEN 0 ELSE 1 END, last_seen_at DESC
             LIMIT ?4",
        )?;
        let rows = stmt
            .query_map(
                params![input.status, input.target_type, input.target_key, limit],
                |row| self.map_resource_alert_event_row(row),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn count_open_resource_alert_events(&self) -> Result<i64, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT COUNT(*) FROM resource_alert_events WHERE status = 'open'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.into())
    }

    // ─── 审计日志 DAO ───────────────────────────────

    pub fn list_audit_logs(&self, input: &ListAuditLogsInput) -> Result<Vec<AuditLog>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let keyword = input
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{}%", value));
        let limit = input.limit.unwrap_or(200);
        let mut stmt = conn.prepare(
            "SELECT id, occurred_at, actor, source, server_alias, action, risk, result,
                    summary, detail_json, request_id, approval_id, created_at
             FROM audit_logs
             WHERE deleted_at IS NULL
               AND (?1 IS NULL OR actor = ?1)
               AND (?2 IS NULL OR source = ?2)
               AND (?3 IS NULL OR server_alias = ?3)
               AND (?4 IS NULL OR action = ?4)
               AND (?5 IS NULL OR risk = ?5)
               AND (?6 IS NULL OR result = ?6)
               AND (
                 ?7 IS NULL
                 OR actor LIKE ?7
                 OR source LIKE ?7
                 OR server_alias LIKE ?7
                 OR action LIKE ?7
                 OR result LIKE ?7
                 OR summary LIKE ?7
                 OR request_id LIKE ?7
                 OR detail_json LIKE ?7
               )
             ORDER BY id DESC
             LIMIT ?8",
        )?;
        let rows = stmt
            .query_map(
                params![
                    input.actor,
                    input.source,
                    input.server_alias,
                    input.action,
                    input.risk,
                    input.result,
                    keyword,
                    limit
                ],
                |row| self.map_audit_log_row(row),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn create_audit_log(&self, input: &CreateAuditLogInput) -> Result<AuditLog, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let detail_json = input.detail_json.as_deref().unwrap_or("{}");
        let request_id = input.request_id.as_deref().unwrap_or("");
        conn.execute(
            "INSERT INTO audit_logs
             (actor, source, server_alias, action, risk, result, summary, detail_json, request_id, approval_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                input.actor,
                input.source,
                input.server_alias,
                input.action,
                input.risk,
                input.result,
                input.summary,
                detail_json,
                request_id,
                input.approval_id
            ],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.get_audit_log(id)?
            .ok_or_else(|| AppError::NotFound(format!("审计日志 '{}' 不存在", id)))
    }

    pub fn get_audit_log(&self, id: i64) -> Result<Option<AuditLog>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, occurred_at, actor, source, server_alias, action, risk, result,
                    summary, detail_json, request_id, approval_id, created_at
             FROM audit_logs
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |row| self.map_audit_log_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    // ─── AI Provider DAO ───────────────────────────────

    pub fn list_ai_providers(&self) -> Result<Vec<AiProvider>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT key, name, region, protocol, default_model, status, endpoint, auth_type,
                    secret_ciphertext IS NOT NULL AS has_api_key,
                    latency_ms, cost_level, capabilities, models, scenario_fit, fallback, enabled, updated_at
             FROM ai_providers
             WHERE deleted_at IS NULL
             ORDER BY
               CASE region WHEN 'china' THEN 0 WHEN 'global' THEN 1 WHEN 'gateway' THEN 2 ELSE 3 END,
               name",
        )?;

        let rows = stmt
            .query_map([], |row| self.map_ai_provider_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_ai_provider(&self, key: &str) -> Result<Option<AiProvider>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT key, name, region, protocol, default_model, status, endpoint, auth_type,
                    secret_ciphertext IS NOT NULL AS has_api_key,
                    latency_ms, cost_level, capabilities, models, scenario_fit, fallback, enabled, updated_at
             FROM ai_providers
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
            |row| self.map_ai_provider_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn get_ai_provider_secret_row(
        &self,
        key: &str,
    ) -> Result<Option<AiProviderSecretRow>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT key, name, region, protocol, default_model, status, endpoint, auth_type,
                    secret_ciphertext IS NOT NULL AS has_api_key,
                    latency_ms, cost_level, capabilities, models, scenario_fit, fallback, enabled, updated_at,
                    secret_nonce, secret_ciphertext
             FROM ai_providers
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
            |row| {
                Ok(AiProviderSecretRow {
                    provider: self.map_ai_provider_row(row)?,
                    secret_nonce: row.get(17)?,
                    secret_ciphertext: row.get(18)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn upsert_ai_provider(
        &self,
        input: &UpsertAiProviderInput,
        encrypted_secret: Option<(&str, &str)>,
        clear_api_key: bool,
    ) -> Result<AiProvider, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let capabilities = serde_json::to_string(&input.capabilities)?;
        let models = serde_json::to_string(&input.models)?;
        let scenario_fit = serde_json::to_string(&input.scenario_fit)?;
        let enabled = if input.enabled { 1 } else { 0 };

        conn.execute(
            "INSERT INTO ai_providers
             (key, name, region, protocol, default_model, status, endpoint, auth_type,
              secret_nonce, secret_ciphertext, cost_level, capabilities, models, scenario_fit, fallback, enabled, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, datetime('now', 'localtime'), NULL)
             ON CONFLICT(key) DO UPDATE SET
               name = excluded.name,
               region = excluded.region,
               protocol = excluded.protocol,
               default_model = excluded.default_model,
               status = excluded.status,
               endpoint = excluded.endpoint,
               auth_type = excluded.auth_type,
               secret_nonce = CASE
                 WHEN ?17 THEN NULL
                 WHEN ?9 IS NOT NULL THEN excluded.secret_nonce
                 ELSE ai_providers.secret_nonce
               END,
               secret_ciphertext = CASE
                 WHEN ?17 THEN NULL
                 WHEN ?10 IS NOT NULL THEN excluded.secret_ciphertext
                 ELSE ai_providers.secret_ciphertext
               END,
               cost_level = excluded.cost_level,
               capabilities = excluded.capabilities,
               models = excluded.models,
               scenario_fit = excluded.scenario_fit,
               fallback = excluded.fallback,
               enabled = excluded.enabled,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                input.key,
                input.name,
                input.region,
                input.protocol,
                input.default_model,
                input.status,
                input.endpoint,
                input.auth_type,
                encrypted_secret.map(|v| v.0),
                encrypted_secret.map(|v| v.1),
                input.cost_level,
                capabilities,
                models,
                scenario_fit,
                input.fallback,
                enabled,
                clear_api_key
            ],
        )?;
        drop(conn);

        self.get_ai_provider(&input.key)?
            .ok_or_else(|| AppError::NotFound(format!("AI Provider '{}' 不存在", input.key)))
    }

    pub fn delete_ai_provider(&self, key: &str) -> Result<bool, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let affected = conn.execute(
            "UPDATE ai_providers
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE key = ?1 AND deleted_at IS NULL",
            [key],
        )?;
        Ok(affected > 0)
    }

    pub fn update_ai_provider_latency(
        &self,
        key: &str,
        latency_ms: i64,
        status: &str,
    ) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "UPDATE ai_providers
             SET latency_ms = ?2, status = ?3, updated_at = datetime('now', 'localtime')
             WHERE key = ?1 AND deleted_at IS NULL",
            params![key, latency_ms, status],
        )?;
        Ok(())
    }

    pub fn list_ai_provider_routes(&self) -> Result<Vec<AiProviderRoute>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT scenario, primary_provider_key, fallback_provider_key, requirement, updated_at
             FROM ai_provider_routes
             ORDER BY scenario",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AiProviderRoute {
                    scenario: row.get(0)?,
                    primary_provider_key: row.get(1)?,
                    fallback_provider_key: row.get(2)?,
                    requirement: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn upsert_ai_provider_route(
        &self,
        input: &UpsertAiProviderRouteInput,
    ) -> Result<AiProviderRoute, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "INSERT INTO ai_provider_routes
             (scenario, primary_provider_key, fallback_provider_key, requirement, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now', 'localtime'))
             ON CONFLICT(scenario) DO UPDATE SET
               primary_provider_key = excluded.primary_provider_key,
               fallback_provider_key = excluded.fallback_provider_key,
               requirement = excluded.requirement,
               updated_at = excluded.updated_at",
            params![
                input.scenario,
                input.primary_provider_key,
                input.fallback_provider_key,
                input.requirement
            ],
        )?;
        drop(conn);

        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT scenario, primary_provider_key, fallback_provider_key, requirement, updated_at
             FROM ai_provider_routes WHERE scenario = ?1",
            [&input.scenario],
            |row| {
                Ok(AiProviderRoute {
                    scenario: row.get(0)?,
                    primary_provider_key: row.get(1)?,
                    fallback_provider_key: row.get(2)?,
                    requirement: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .map_err(|e| e.into())
    }

    // ─── AI Skill DAO ───────────────────────────────

    pub fn list_ai_skills(&self, input: &ListAiSkillsInput) -> Result<Vec<AiSkill>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, skill_key, name, description, content, scopes, trigger_words, tags,
                    priority, enabled, builtin, source, source_path, content_hash, missing,
                    builtin_version, user_overridden, allow_mcp, created_at, updated_at
             FROM ai_skills
             WHERE deleted_at IS NULL
             ORDER BY builtin ASC, priority DESC, updated_at DESC, name ASC",
        )?;
        let mut items = stmt
            .query_map([], |row| self.map_ai_skill_row(row))?
            .collect::<Result<Vec<_>, _>>()?;

        if input.show_builtin == Some(false) {
            items.retain(|item| !item.builtin);
        }
        if let Some(source) = input
            .source
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            items.retain(|item| item.source == source);
        }
        if let Some(scope) = input
            .scope
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            items.retain(|item| item.scopes.iter().any(|s| s == scope || s == "global"));
        }
        if let Some(keyword) = input
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            let lowered = keyword.to_lowercase();
            items.retain(|item| {
                item.name.to_lowercase().contains(&lowered)
                    || item.skill_key.to_lowercase().contains(&lowered)
                    || item.description.to_lowercase().contains(&lowered)
                    || item
                        .trigger_words
                        .iter()
                        .any(|word| word.to_lowercase().contains(&lowered))
                    || item
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&lowered))
            });
        }
        Ok(items)
    }

    pub fn get_ai_skill_by_id(&self, id: i64) -> Result<Option<AiSkill>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, skill_key, name, description, content, scopes, trigger_words, tags,
                    priority, enabled, builtin, source, source_path, content_hash, missing,
                    builtin_version, user_overridden, allow_mcp, created_at, updated_at
             FROM ai_skills
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |row| self.map_ai_skill_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn get_ai_skill_by_key(&self, skill_key: &str) -> Result<Option<AiSkill>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, skill_key, name, description, content, scopes, trigger_words, tags,
                    priority, enabled, builtin, source, source_path, content_hash, missing,
                    builtin_version, user_overridden, allow_mcp, created_at, updated_at
             FROM ai_skills
             WHERE skill_key = ?1 AND deleted_at IS NULL",
            [skill_key],
            |row| self.map_ai_skill_row(row),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn get_ai_skill_builtin_content(
        &self,
        skill_key: &str,
    ) -> Result<Option<String>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT builtin_content FROM ai_skills
             WHERE skill_key = ?1 AND builtin = 1 AND deleted_at IS NULL",
            [skill_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn ai_skill_stats(&self) -> Result<AiSkillStats, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN source = 'user' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN builtin = 1 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN enabled = 1 THEN 1 ELSE 0 END)
             FROM ai_skills
             WHERE deleted_at IS NULL",
            [],
            |row| {
                Ok(AiSkillStats {
                    total: row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    user: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    builtin: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    enabled: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                })
            },
        )
        .map_err(|e| e.into())
    }

    pub fn upsert_user_ai_skill(&self, input: &UpsertAiSkillInput) -> Result<AiSkill, AppError> {
        let skill_key = input
            .skill_key
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Self::slugify_key(&input.name));
        let description = input.description.clone().unwrap_or_default();
        let scopes_json = serde_json::to_string(&input.scopes)?;
        let trigger_words_json =
            serde_json::to_string(&input.trigger_words.clone().unwrap_or_default())?;
        let tags_json = serde_json::to_string(&input.tags.clone().unwrap_or_default())?;
        let priority = input.priority.unwrap_or(0);
        let enabled = if input.enabled.unwrap_or(true) { 1 } else { 0 };
        let allow_mcp = if input.allow_mcp.unwrap_or(true) {
            1
        } else {
            0
        };
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;

        if let Some(id) = input.id {
            let builtin: i64 = conn
                .query_row(
                    "SELECT builtin FROM ai_skills WHERE id = ?1 AND deleted_at IS NULL",
                    [id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Skill {} 不存在", id)))?;
            if builtin != 0 {
                conn.execute(
                    "UPDATE ai_skills
                     SET name = ?1, description = ?2, content = ?3, scopes = ?4,
                         trigger_words = ?5, tags = ?6, priority = ?7, enabled = ?8,
                         allow_mcp = ?9, user_overridden = 1, updated_at = datetime('now', 'localtime')
                     WHERE id = ?10 AND deleted_at IS NULL",
                    params![
                        input.name,
                        description,
                        input.content,
                        scopes_json,
                        trigger_words_json,
                        tags_json,
                        priority,
                        enabled,
                        allow_mcp,
                        id
                    ],
                )?;
            } else {
                conn.execute(
                    "UPDATE ai_skills
                     SET skill_key = ?1, name = ?2, description = ?3, content = ?4, scopes = ?5,
                         trigger_words = ?6, tags = ?7, priority = ?8, enabled = ?9,
                         allow_mcp = ?10, updated_at = datetime('now', 'localtime')
                     WHERE id = ?11 AND deleted_at IS NULL",
                    params![
                        skill_key,
                        input.name,
                        description,
                        input.content,
                        scopes_json,
                        trigger_words_json,
                        tags_json,
                        priority,
                        enabled,
                        allow_mcp,
                        id
                    ],
                )?;
            }
            drop(conn);
            return self
                .get_ai_skill_by_id(id)?
                .ok_or_else(|| AppError::NotFound(format!("Skill {} 不存在", id)));
        }

        conn.execute(
            "INSERT INTO ai_skills
             (skill_key, name, description, content, scopes, trigger_words, tags, priority,
              enabled, builtin, source, allow_mcp, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 'user', ?10, datetime('now', 'localtime'), NULL)
             ON CONFLICT(skill_key) DO UPDATE SET
               name = excluded.name,
               description = excluded.description,
               content = excluded.content,
               scopes = excluded.scopes,
               trigger_words = excluded.trigger_words,
               tags = excluded.tags,
               priority = excluded.priority,
               enabled = excluded.enabled,
               allow_mcp = excluded.allow_mcp,
               source = 'user',
               builtin = 0,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                skill_key,
                input.name,
                description,
                input.content,
                scopes_json,
                trigger_words_json,
                tags_json,
                priority,
                enabled,
                allow_mcp
            ],
        )?;
        drop(conn);
        self.get_ai_skill_by_key(&skill_key)?
            .ok_or_else(|| AppError::NotFound(format!("Skill '{}' 不存在", skill_key)))
    }

    pub fn upsert_builtin_ai_skill(
        &self,
        skill_key: &str,
        name: &str,
        description: &str,
        content: &str,
        scopes: &[String],
        trigger_words: &[String],
        tags: &[String],
        priority: i64,
        source_path: &str,
        content_hash: &str,
    ) -> Result<String, AppError> {
        let scopes_json = serde_json::to_string(scopes)?;
        let trigger_words_json = serde_json::to_string(trigger_words)?;
        let tags_json = serde_json::to_string(tags)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let existing: Option<(i64, String, i64)> = conn
            .query_row(
                "SELECT id, content_hash, user_overridden FROM ai_skills WHERE skill_key = ?1 AND deleted_at IS NULL",
                [skill_key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let action = if let Some((id, old_hash, user_overridden)) = existing {
            if old_hash == content_hash {
                if user_overridden == 0 {
                    conn.execute(
                        "UPDATE ai_skills
                         SET name = ?1, description = ?2, scopes = ?3, trigger_words = ?4,
                             tags = ?5, priority = ?6, missing = 0, source_path = ?7,
                             updated_at = datetime('now', 'localtime')
                         WHERE id = ?8",
                        params![
                            name,
                            description,
                            scopes_json,
                            trigger_words_json,
                            tags_json,
                            priority,
                            source_path,
                            id
                        ],
                    )?;
                } else {
                    conn.execute(
                        "UPDATE ai_skills
                         SET missing = 0, source_path = ?1, updated_at = datetime('now', 'localtime')
                         WHERE id = ?2",
                        params![source_path, id],
                    )?;
                }
                "unchanged".to_string()
            } else {
                if user_overridden == 0 {
                    conn.execute(
                        "UPDATE ai_skills
                         SET name = ?1, description = ?2, content = ?3, scopes = ?4,
                             trigger_words = ?5, tags = ?6, priority = ?7,
                             source_path = ?8, content_hash = ?9, builtin_content = ?10,
                             missing = 0, updated_at = datetime('now', 'localtime')
                         WHERE id = ?11",
                        params![
                            name,
                            description,
                            content,
                            scopes_json,
                            trigger_words_json,
                            tags_json,
                            priority,
                            source_path,
                            content_hash,
                            content,
                            id
                        ],
                    )?;
                } else {
                    conn.execute(
                        "UPDATE ai_skills
                         SET builtin_content = ?1, content_hash = ?2, source_path = ?3,
                             missing = 0, updated_at = datetime('now', 'localtime')
                         WHERE id = ?4",
                        params![content, content_hash, source_path, id],
                    )?;
                }
                "updated".to_string()
            }
        } else {
            conn.execute(
                "INSERT INTO ai_skills
                 (skill_key, name, description, content, scopes, trigger_words, tags, priority,
                  enabled, builtin, source, source_path, content_hash, missing, builtin_version,
                  builtin_content, user_overridden, allow_mcp, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 1, 'builtin', ?9, ?10, 0, 1,
                         ?11, 0, 1, datetime('now', 'localtime'), NULL)",
                params![
                    skill_key,
                    name,
                    description,
                    content,
                    scopes_json,
                    trigger_words_json,
                    tags_json,
                    priority,
                    source_path,
                    content_hash,
                    content
                ],
            )?;
            "inserted".to_string()
        };
        Ok(action)
    }

    pub fn mark_missing_builtin_ai_skills(&self, source_paths: &[String]) -> Result<i64, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT source_path FROM ai_skills
             WHERE builtin = 1 AND deleted_at IS NULL",
        )?;
        let existing = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut missing = 0;
        for path in existing {
            if !source_paths.iter().any(|item| item == &path) {
                missing += conn.execute(
                    "UPDATE ai_skills
                     SET missing = 1, enabled = 0, updated_at = datetime('now', 'localtime')
                     WHERE source_path = ?1 AND builtin = 1",
                    [path],
                )?;
            }
        }
        Ok(missing as i64)
    }

    pub fn set_ai_skill_enabled(&self, id: i64, enabled: bool) -> Result<AiSkill, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "UPDATE ai_skills
             SET enabled = ?1, updated_at = datetime('now', 'localtime')
             WHERE id = ?2 AND deleted_at IS NULL",
            params![if enabled { 1 } else { 0 }, id],
        )?;
        drop(conn);
        self.get_ai_skill_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("Skill {} 不存在", id)))
    }

    pub fn delete_ai_skill(&self, id: i64) -> Result<(), AppError> {
        let existing = self
            .get_ai_skill_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("Skill {} 不存在", id)))?;
        if existing.builtin {
            return Err(AppError::InvalidInput(
                "内置 Skill 不允许删除，只能停用".into(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "UPDATE ai_skills
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND builtin = 0",
            [id],
        )?;
        Ok(())
    }

    pub fn restore_builtin_ai_skill(&self, id: i64) -> Result<AiSkill, AppError> {
        let existing = self
            .get_ai_skill_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("Skill {} 不存在", id)))?;
        if !existing.builtin {
            return Err(AppError::InvalidInput("只有内置 Skill 可以恢复默认".into()));
        }
        let builtin_content = self
            .get_ai_skill_builtin_content(&existing.skill_key)?
            .unwrap_or_default();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "UPDATE ai_skills
             SET content = ?1, user_overridden = 0, enabled = 1,
                 updated_at = datetime('now', 'localtime')
             WHERE id = ?2 AND builtin = 1",
            params![builtin_content, id],
        )?;
        drop(conn);
        self.get_ai_skill_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("Skill {} 不存在", id)))
    }

    pub fn list_ai_experiences(
        &self,
        keyword: Option<&str>,
    ) -> Result<Vec<AiExperience>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, experience_key, title, symptom, cause, solution, scenario, source,
                    tags, references_json, markdown_path, enabled, created_at, updated_at
             FROM ai_experiences
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC, id DESC",
        )?;
        let mut items = stmt
            .query_map([], |row| self.map_ai_experience_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(keyword) = keyword.map(str::trim).filter(|v| !v.is_empty()) {
            let lowered = keyword.to_lowercase();
            items.retain(|item| {
                item.title.to_lowercase().contains(&lowered)
                    || item.symptom.to_lowercase().contains(&lowered)
                    || item.cause.to_lowercase().contains(&lowered)
                    || item.solution.to_lowercase().contains(&lowered)
                    || item
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&lowered))
            });
        }
        Ok(items)
    }

    pub fn upsert_ai_experience(
        &self,
        input: &UpsertAiExperienceInput,
    ) -> Result<AiExperience, AppError> {
        let experience_key = input
            .experience_key
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Self::slugify_key(&input.title));
        let tags_json = serde_json::to_string(&input.tags.clone().unwrap_or_default())?;
        let references_json = input
            .references_json
            .clone()
            .unwrap_or_else(|| "[]".to_string());
        let markdown_path = input.markdown_path.clone().unwrap_or_default();
        let enabled = if input.enabled.unwrap_or(true) { 1 } else { 0 };
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        if let Some(id) = input.id {
            conn.execute(
                "UPDATE ai_experiences
                 SET title = ?1, symptom = ?2, cause = ?3, solution = ?4, scenario = ?5,
                     source = ?6, tags = ?7, references_json = ?8, markdown_path = ?9,
                     enabled = ?10, updated_at = datetime('now', 'localtime')
                 WHERE id = ?11 AND deleted_at IS NULL",
                params![
                    input.title,
                    input.symptom.clone().unwrap_or_default(),
                    input.cause.clone().unwrap_or_default(),
                    input.solution.clone().unwrap_or_default(),
                    input.scenario.clone().unwrap_or_default(),
                    input.source.clone().unwrap_or_else(|| "user".into()),
                    tags_json,
                    references_json,
                    markdown_path,
                    enabled,
                    id
                ],
            )?;
            drop(conn);
            return self.get_ai_experience_by_id(id);
        }
        conn.execute(
            "INSERT INTO ai_experiences
             (experience_key, title, symptom, cause, solution, scenario, source, tags,
              references_json, markdown_path, enabled, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now', 'localtime'), NULL)
             ON CONFLICT(experience_key) DO UPDATE SET
               title = excluded.title,
               symptom = excluded.symptom,
               cause = excluded.cause,
               solution = excluded.solution,
               scenario = excluded.scenario,
               source = excluded.source,
               tags = excluded.tags,
               references_json = excluded.references_json,
               markdown_path = excluded.markdown_path,
               enabled = excluded.enabled,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                experience_key,
                input.title,
                input.symptom.clone().unwrap_or_default(),
                input.cause.clone().unwrap_or_default(),
                input.solution.clone().unwrap_or_default(),
                input.scenario.clone().unwrap_or_default(),
                input.source.clone().unwrap_or_else(|| "user".into()),
                tags_json,
                references_json,
                markdown_path,
                enabled
            ],
        )?;
        drop(conn);
        self.get_ai_experience_by_key(&experience_key)
    }

    pub fn delete_ai_experience(&self, id: i64) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "UPDATE ai_experiences
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn list_ai_runbooks(&self, keyword: Option<&str>) -> Result<Vec<AiRunbook>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT id, runbook_key, name, description, scenario, tags, steps_json,
                    enabled, allow_mcp, created_at, updated_at
             FROM ai_runbooks
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC, id DESC",
        )?;
        let mut items = stmt
            .query_map([], |row| self.map_ai_runbook_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(keyword) = keyword.map(str::trim).filter(|v| !v.is_empty()) {
            let lowered = keyword.to_lowercase();
            items.retain(|item| {
                item.name.to_lowercase().contains(&lowered)
                    || item.description.to_lowercase().contains(&lowered)
                    || item.scenario.to_lowercase().contains(&lowered)
                    || item
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&lowered))
            });
        }
        Ok(items)
    }

    pub fn upsert_ai_runbook(&self, input: &UpsertAiRunbookInput) -> Result<AiRunbook, AppError> {
        let runbook_key = input
            .runbook_key
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Self::slugify_key(&input.name));
        let tags_json = serde_json::to_string(&input.tags.clone().unwrap_or_default())?;
        let steps_json = serde_json::to_string(&input.steps.clone().unwrap_or_default())?;
        let enabled = if input.enabled.unwrap_or(true) { 1 } else { 0 };
        let allow_mcp = if input.allow_mcp.unwrap_or(false) {
            1
        } else {
            0
        };
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        if let Some(id) = input.id {
            conn.execute(
                "UPDATE ai_runbooks
                 SET name = ?1, description = ?2, scenario = ?3, tags = ?4, steps_json = ?5,
                     enabled = ?6, allow_mcp = ?7, updated_at = datetime('now', 'localtime')
                 WHERE id = ?8 AND deleted_at IS NULL",
                params![
                    input.name,
                    input.description.clone().unwrap_or_default(),
                    input.scenario.clone().unwrap_or_default(),
                    tags_json,
                    steps_json,
                    enabled,
                    allow_mcp,
                    id
                ],
            )?;
            drop(conn);
            return self.get_ai_runbook_by_id(id);
        }
        conn.execute(
            "INSERT INTO ai_runbooks
             (runbook_key, name, description, scenario, tags, steps_json, enabled, allow_mcp,
              updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now', 'localtime'), NULL)
             ON CONFLICT(runbook_key) DO UPDATE SET
               name = excluded.name,
               description = excluded.description,
               scenario = excluded.scenario,
               tags = excluded.tags,
               steps_json = excluded.steps_json,
               enabled = excluded.enabled,
               allow_mcp = excluded.allow_mcp,
               updated_at = excluded.updated_at,
               deleted_at = NULL",
            params![
                runbook_key,
                input.name,
                input.description.clone().unwrap_or_default(),
                input.scenario.clone().unwrap_or_default(),
                tags_json,
                steps_json,
                enabled,
                allow_mcp
            ],
        )?;
        drop(conn);
        self.get_ai_runbook_by_key(&runbook_key)
    }

    pub fn delete_ai_runbook(&self, id: i64) -> Result<(), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.execute(
            "UPDATE ai_runbooks
             SET deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    fn get_ai_experience_by_id(&self, id: i64) -> Result<AiExperience, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, experience_key, title, symptom, cause, solution, scenario, source,
                    tags, references_json, markdown_path, enabled, created_at, updated_at
             FROM ai_experiences
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |row| self.map_ai_experience_row(row),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("经验 {} 不存在", id)))
    }

    fn get_ai_experience_by_key(&self, key: &str) -> Result<AiExperience, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, experience_key, title, symptom, cause, solution, scenario, source,
                    tags, references_json, markdown_path, enabled, created_at, updated_at
             FROM ai_experiences
             WHERE experience_key = ?1 AND deleted_at IS NULL",
            [key],
            |row| self.map_ai_experience_row(row),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("经验 '{}' 不存在", key)))
    }

    fn get_ai_runbook_by_id(&self, id: i64) -> Result<AiRunbook, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, runbook_key, name, description, scenario, tags, steps_json,
                    enabled, allow_mcp, created_at, updated_at
             FROM ai_runbooks
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            |row| self.map_ai_runbook_row(row),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("Runbook {} 不存在", id)))
    }

    fn get_ai_runbook_by_key(&self, key: &str) -> Result<AiRunbook, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        conn.query_row(
            "SELECT id, runbook_key, name, description, scenario, tags, steps_json,
                    enabled, allow_mcp, created_at, updated_at
             FROM ai_runbooks
             WHERE runbook_key = ?1 AND deleted_at IS NULL",
            [key],
            |row| self.map_ai_runbook_row(row),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("Runbook '{}' 不存在", key)))
    }

    fn slugify_key(value: &str) -> String {
        let mut out = String::new();
        for ch in value.trim().to_lowercase().chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch);
            } else if ch.is_whitespace() || ch == '-' || ch == '_' || ch == '.' {
                if !out.ends_with('-') {
                    out.push('-');
                }
            }
        }
        let trimmed = out.trim_matches('-').to_string();
        if trimmed.is_empty() {
            format!("item-{}", chrono::Utc::now().timestamp_millis())
        } else {
            trimmed
        }
    }

    fn parse_json_vec<T: serde::de::DeserializeOwned>(value: &str) -> Vec<T> {
        serde_json::from_str(value).unwrap_or_default()
    }

    fn map_ai_skill_row(&self, row: &rusqlite::Row<'_>) -> Result<AiSkill, rusqlite::Error> {
        let scopes_json: String = row.get(5)?;
        let trigger_words_json: String = row.get(6)?;
        let tags_json: String = row.get(7)?;
        Ok(AiSkill {
            id: row.get(0)?,
            skill_key: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            content: row.get(4)?,
            scopes: Self::parse_json_vec(&scopes_json),
            trigger_words: Self::parse_json_vec(&trigger_words_json),
            tags: Self::parse_json_vec(&tags_json),
            priority: row.get(8)?,
            enabled: row.get::<_, i64>(9)? != 0,
            builtin: row.get::<_, i64>(10)? != 0,
            source: row.get(11)?,
            source_path: row.get(12)?,
            content_hash: row.get(13)?,
            missing: row.get::<_, i64>(14)? != 0,
            builtin_version: row.get(15)?,
            user_overridden: row.get::<_, i64>(16)? != 0,
            allow_mcp: row.get::<_, i64>(17)? != 0,
            created_at: row.get(18)?,
            updated_at: row.get(19)?,
        })
    }

    fn map_ai_experience_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> Result<AiExperience, rusqlite::Error> {
        let tags_json: String = row.get(8)?;
        Ok(AiExperience {
            id: row.get(0)?,
            experience_key: row.get(1)?,
            title: row.get(2)?,
            symptom: row.get(3)?,
            cause: row.get(4)?,
            solution: row.get(5)?,
            scenario: row.get(6)?,
            source: row.get(7)?,
            tags: Self::parse_json_vec(&tags_json),
            references_json: row.get(9)?,
            markdown_path: row.get(10)?,
            enabled: row.get::<_, i64>(11)? != 0,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }

    fn map_ai_runbook_row(&self, row: &rusqlite::Row<'_>) -> Result<AiRunbook, rusqlite::Error> {
        let tags_json: String = row.get(5)?;
        let steps_json: String = row.get(6)?;
        Ok(AiRunbook {
            id: row.get(0)?,
            runbook_key: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            scenario: row.get(4)?,
            tags: Self::parse_json_vec(&tags_json),
            steps: Self::parse_json_vec::<AiRunbookStep>(&steps_json),
            enabled: row.get::<_, i64>(7)? != 0,
            allow_mcp: row.get::<_, i64>(8)? != 0,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }

    fn map_ai_provider_row(&self, row: &rusqlite::Row<'_>) -> Result<AiProvider, rusqlite::Error> {
        let capabilities_json: String = row.get(11)?;
        let models_json: String = row.get(12)?;
        let scenario_fit_json: String = row.get(13)?;
        let has_api_key: bool = row.get(8)?;
        Ok(AiProvider {
            key: row.get(0)?,
            name: row.get(1)?,
            region: row.get(2)?,
            protocol: row.get(3)?,
            default_model: row.get(4)?,
            status: row.get(5)?,
            endpoint: row.get(6)?,
            auth_type: row.get(7)?,
            api_key_masked: if has_api_key {
                Some("••••••••".into())
            } else {
                None
            },
            has_api_key,
            latency_ms: row.get(9)?,
            cost_level: row.get(10)?,
            capabilities: serde_json::from_str(&capabilities_json).unwrap_or_default(),
            models: serde_json::from_str(&models_json).unwrap_or_default(),
            scenario_fit: serde_json::from_str(&scenario_fit_json).unwrap_or_default(),
            fallback: row.get(14)?,
            enabled: row.get::<_, i64>(15)? != 0,
            updated_at: row.get(16)?,
        })
    }

    fn map_ssh_server_row(&self, row: &rusqlite::Row<'_>) -> Result<SshServer, rusqlite::Error> {
        let has_password: bool = row.get(9)?;
        Ok(SshServer {
            alias: row.get(0)?,
            group_name: row.get(1)?,
            host: row.get(2)?,
            port: row.get(3)?,
            username: row.get(4)?,
            source: row.get(5)?,
            auth_type: row.get(6)?,
            auth_ref: row.get(7)?,
            identity_file: row.get(8)?,
            password_masked: if has_password {
                Some("••••••••".into())
            } else {
                None
            },
            has_password,
            proxy_jump: row.get(10)?,
            ai_policy: row.get(11)?,
            status: row.get(12)?,
            enabled: row.get::<_, i64>(13)? != 0,
            last_connected_at: row.get(14)?,
            updated_at: row.get(15)?,
        })
    }

    fn map_credential_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> Result<CredentialVaultItem, rusqlite::Error> {
        let has_secret: bool = row.get(5)?;
        Ok(CredentialVaultItem {
            key: row.get(0)?,
            credential_type: row.get(1)?,
            scope: row.get(2)?,
            status: row.get(3)?,
            description: row.get(4)?,
            secret_masked: if has_secret {
                Some("••••••••".into())
            } else {
                None
            },
            has_secret,
            enabled: row.get::<_, i64>(6)? != 0,
            rotated_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }

    fn map_database_connection_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> Result<DatabaseConnection, rusqlite::Error> {
        let has_password: bool = row.get(11)?;
        Ok(DatabaseConnection {
            key: row.get(0)?,
            name: row.get(1)?,
            group_name: row.get(2)?,
            db_type: row.get(3)?,
            connection_mode: row.get(4)?,
            host: row.get(5)?,
            port: row.get(6)?,
            database_name: row.get(7)?,
            username: row.get(8)?,
            auth_type: row.get(9)?,
            credential_ref: row.get(10)?,
            password_masked: if has_password {
                Some("••••••••".into())
            } else {
                None
            },
            has_password,
            ssh_server_alias: row.get(12)?,
            security_mode: row.get(13)?,
            ai_policy: row.get(14)?,
            page_size: row.get(15)?,
            status: row.get(16)?,
            enabled: row.get::<_, i64>(17)? != 0,
            last_connected_at: row.get(18)?,
            notes: row.get(19)?,
            updated_at: row.get(20)?,
        })
    }

    fn map_approval_request_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> Result<ApprovalRequest, rusqlite::Error> {
        Ok(ApprovalRequest {
            id: row.get(0)?,
            source: row.get(1)?,
            requester: row.get(2)?,
            server_alias: row.get(3)?,
            action: row.get(4)?,
            risk: row.get(5)?,
            status: row.get(6)?,
            command: row.get(7)?,
            resource: row.get(8)?,
            reason: row.get(9)?,
            summary: row.get(10)?,
            payload_json: row.get(11)?,
            decision_note: row.get(12)?,
            decided_by: row.get(13)?,
            decided_at: row.get(14)?,
            expires_at: row.get(15)?,
            created_at: row.get(16)?,
            updated_at: row.get(17)?,
        })
    }

    fn map_jumpserver_session_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> Result<JumpServerSession, rusqlite::Error> {
        Ok(JumpServerSession {
            key: row.get(0)?,
            name: row.get(1)?,
            endpoint: row.get(2)?,
            web_url: row.get(3)?,
            session_ref: row.get(4)?,
            group_name: row.get(5)?,
            account_hint: row.get(6)?,
            asset_hint: row.get(7)?,
            protocol: row.get(8)?,
            ai_mode: row.get(9)?,
            status: row.get(10)?,
            notes: row.get(11)?,
            enabled: row.get::<_, i64>(12)? != 0,
            last_opened_at: row.get(13)?,
            updated_at: row.get(14)?,
        })
    }

    fn map_audit_log_row(&self, row: &rusqlite::Row<'_>) -> Result<AuditLog, rusqlite::Error> {
        Ok(AuditLog {
            id: row.get(0)?,
            occurred_at: row.get(1)?,
            actor: row.get(2)?,
            source: row.get(3)?,
            server_alias: row.get(4)?,
            action: row.get(5)?,
            risk: row.get(6)?,
            result: row.get(7)?,
            summary: row.get(8)?,
            detail_json: row.get(9)?,
            request_id: row.get(10)?,
            approval_id: row.get(11)?,
            created_at: row.get(12)?,
        })
    }

    fn map_resource_monitor_target_row(
        &self,
        row: &rusqlite::Row<'_>,
        group_name: &str,
        latest_snapshot: Option<ResourceMetricSnapshot>,
    ) -> Result<ResourceMonitorTarget, rusqlite::Error> {
        Ok(ResourceMonitorTarget {
            id: row.get(0)?,
            target_type: row.get(1)?,
            target_key: row.get(2)?,
            display_name: row.get(3)?,
            group_name: group_name.to_string(),
            enabled: row.get::<_, i64>(4)? != 0,
            collect_interval_sec: row.get(5)?,
            last_status: row.get(6)?,
            last_collected_at: row.get(7)?,
            last_error: row.get(8)?,
            latest_snapshot,
            updated_at: row.get(9)?,
        })
    }

    fn map_resource_metric_snapshot_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> Result<ResourceMetricSnapshot, rusqlite::Error> {
        let summary_json: String = row.get(6)?;
        let metrics_json: String = row.get(7)?;
        let summary = serde_json::from_str(&summary_json).unwrap_or_else(|_| serde_json::json!({}));
        let metrics = serde_json::from_str(&metrics_json).unwrap_or_else(|_| serde_json::json!({}));
        Ok(ResourceMetricSnapshot {
            id: row.get(0)?,
            target_type: row.get(1)?,
            target_key: row.get(2)?,
            status: row.get(3)?,
            collected_at: row.get(4)?,
            duration_ms: row.get(5)?,
            summary,
            metrics,
            error: row.get(8)?,
        })
    }

    fn map_resource_alert_rule_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> Result<ResourceAlertRule, rusqlite::Error> {
        Ok(ResourceAlertRule {
            id: row.get(0)?,
            target_type: row.get(1)?,
            target_key: row.get(2)?,
            metric_key: row.get(3)?,
            operator: row.get(4)?,
            threshold_value: row.get(5)?,
            severity: row.get(6)?,
            enabled: row.get::<_, i64>(7)? != 0,
            updated_at: row.get(8)?,
        })
    }

    fn map_resource_alert_event_row(
        &self,
        row: &rusqlite::Row<'_>,
    ) -> Result<ResourceAlertEvent, rusqlite::Error> {
        Ok(ResourceAlertEvent {
            id: row.get(0)?,
            rule_id: row.get(1)?,
            target_type: row.get(2)?,
            target_key: row.get(3)?,
            severity: row.get(4)?,
            status: row.get(5)?,
            metric_key: row.get(6)?,
            metric_value: row.get(7)?,
            threshold_value: row.get(8)?,
            message: row.get(9)?,
            first_seen_at: row.get(10)?,
            last_seen_at: row.get(11)?,
            resolved_at: row.get(12)?,
            snapshot_id: row.get(13)?,
        })
    }
}
