pub mod schema;

use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppError;
use crate::models::{
    AiProvider, AiProviderRoute, AppConfig, ApprovalRequest, AuditLog, CreateApprovalRequestInput,
    CreateAuditLogInput, CredentialVaultItem, DecideApprovalRequestInput, JumpServerSession,
    ListApprovalRequestsInput, ListAuditLogsInput, SshServer, UpsertAiProviderInput,
    UpsertAiProviderRouteInput, UpsertCredentialInput, UpsertJumpServerSessionInput,
    UpsertSshServerInput,
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
}
