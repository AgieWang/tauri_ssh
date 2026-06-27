use rusqlite::Connection;

use crate::error::AppError;

/// 当前 Schema 版本
pub const SCHEMA_VERSION: i32 = 15;

/// 获取数据库版本
pub fn get_version(conn: &Connection) -> Result<i32, AppError> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(version)
}

/// 设置数据库版本
pub fn set_version(conn: &Connection, version: i32) -> Result<(), AppError> {
    conn.pragma_update(None, "user_version", version)?;
    Ok(())
}

/// 执行数据库迁移
pub fn migrate(conn: &Connection) -> Result<(), AppError> {
    let mut version = get_version(conn)?;

    if version > SCHEMA_VERSION {
        return Err(AppError::Custom(format!(
            "数据库版本({})高于应用支持的版本({}), 请升级应用",
            version, SCHEMA_VERSION
        )));
    }

    while version < SCHEMA_VERSION {
        match version {
            0 => migrate_v0_to_v1(conn)?,
            1 => migrate_v1_to_v2(conn)?,
            2 => migrate_v2_to_v3(conn)?,
            3 => migrate_v3_to_v4(conn)?,
            4 => migrate_v4_to_v5(conn)?,
            5 => migrate_v5_to_v6(conn)?,
            6 => migrate_v6_to_v7(conn)?,
            7 => migrate_v7_to_v8(conn)?,
            8 => migrate_v8_to_v9(conn)?,
            9 => migrate_v9_to_v10(conn)?,
            10 => migrate_v10_to_v11(conn)?,
            11 => migrate_v11_to_v12(conn)?,
            12 => migrate_v12_to_v13(conn)?,
            13 => migrate_v13_to_v14(conn)?,
            14 => migrate_v14_to_v15(conn)?,
            _ => {
                return Err(AppError::Custom(format!("未知的数据库版本: {}", version)));
            }
        }
        version = get_version(conn)?;
    }

    log::info!("数据库迁移完成, 当前版本: {}", version);
    Ok(())
}

/// v14 -> v15: 资源监控阈值规则与告警事件
fn migrate_v14_to_v15(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v14 -> v15（资源监控告警）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS resource_alert_rules (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            target_type     TEXT NOT NULL,
            target_key      TEXT NOT NULL DEFAULT '*',
            metric_key      TEXT NOT NULL,
            operator        TEXT NOT NULL DEFAULT '>',
            threshold_value REAL NOT NULL,
            severity        TEXT NOT NULL DEFAULT 'warning',
            enabled         INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at      TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_resource_alert_rules_target
            ON resource_alert_rules(target_type, target_key, enabled);

        CREATE TABLE IF NOT EXISTS resource_alert_events (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id         INTEGER NOT NULL,
            target_type     TEXT NOT NULL,
            target_key      TEXT NOT NULL,
            severity        TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'open',
            metric_key      TEXT NOT NULL,
            metric_value    REAL NOT NULL,
            threshold_value REAL NOT NULL,
            message         TEXT NOT NULL,
            first_seen_at   TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            last_seen_at    TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            resolved_at     TEXT DEFAULT NULL,
            snapshot_id     INTEGER DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_resource_alert_events_status
            ON resource_alert_events(status, last_seen_at DESC);
        CREATE INDEX IF NOT EXISTS idx_resource_alert_events_target
            ON resource_alert_events(target_type, target_key, status);

        INSERT INTO resource_alert_rules (target_type, target_key, metric_key, operator, threshold_value, severity)
        SELECT 'server', '*', 'cpuUsagePercent', '>', 90, 'critical'
        WHERE NOT EXISTS (SELECT 1 FROM resource_alert_rules WHERE target_type='server' AND metric_key='cpuUsagePercent' AND deleted_at IS NULL);
        INSERT INTO resource_alert_rules (target_type, target_key, metric_key, operator, threshold_value, severity)
        SELECT 'server', '*', 'memoryUsagePercent', '>', 90, 'critical'
        WHERE NOT EXISTS (SELECT 1 FROM resource_alert_rules WHERE target_type='server' AND metric_key='memoryUsagePercent' AND deleted_at IS NULL);
        INSERT INTO resource_alert_rules (target_type, target_key, metric_key, operator, threshold_value, severity)
        SELECT 'server', '*', 'diskUsagePercent', '>', 90, 'critical'
        WHERE NOT EXISTS (SELECT 1 FROM resource_alert_rules WHERE target_type='server' AND metric_key='diskUsagePercent' AND deleted_at IS NULL);
        INSERT INTO resource_alert_rules (target_type, target_key, metric_key, operator, threshold_value, severity)
        SELECT 'mysql', '*', 'connectionUsagePercent', '>', 80, 'warning'
        WHERE NOT EXISTS (SELECT 1 FROM resource_alert_rules WHERE target_type='mysql' AND metric_key='connectionUsagePercent' AND deleted_at IS NULL);
        INSERT INTO resource_alert_rules (target_type, target_key, metric_key, operator, threshold_value, severity)
        SELECT 'postgresql', '*', 'lockWaits', '>', 0, 'warning'
        WHERE NOT EXISTS (SELECT 1 FROM resource_alert_rules WHERE target_type='postgresql' AND metric_key='lockWaits' AND deleted_at IS NULL);
        INSERT INTO resource_alert_rules (target_type, target_key, metric_key, operator, threshold_value, severity)
        SELECT 'redis', '*', 'memoryUsagePercent', '>', 90, 'critical'
        WHERE NOT EXISTS (SELECT 1 FROM resource_alert_rules WHERE target_type='redis' AND metric_key='memoryUsagePercent' AND deleted_at IS NULL);
        ",
    )?;

    set_version(conn, 15)?;
    Ok(())
}

/// v11 -> v12: AI Skill 管理、经验库和 Runbook
fn migrate_v11_to_v12(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v11 -> v12（AI Skill 管理）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS ai_skills (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            skill_key        TEXT NOT NULL UNIQUE,
            name             TEXT NOT NULL,
            description      TEXT NOT NULL DEFAULT '',
            content          TEXT NOT NULL,
            scopes           TEXT NOT NULL DEFAULT '[\"global\"]',
            trigger_words    TEXT NOT NULL DEFAULT '[]',
            tags             TEXT NOT NULL DEFAULT '[]',
            priority         INTEGER NOT NULL DEFAULT 0,
            enabled          INTEGER NOT NULL DEFAULT 1,
            builtin          INTEGER NOT NULL DEFAULT 0,
            source           TEXT NOT NULL DEFAULT 'user',
            source_path      TEXT NOT NULL DEFAULT '',
            content_hash     TEXT NOT NULL DEFAULT '',
            missing          INTEGER NOT NULL DEFAULT 0,
            builtin_version  INTEGER NOT NULL DEFAULT 1,
            builtin_content  TEXT NOT NULL DEFAULT '',
            user_overridden  INTEGER NOT NULL DEFAULT 0,
            allow_mcp        INTEGER NOT NULL DEFAULT 1,
            created_at       TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at       TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ai_skills_enabled_priority
            ON ai_skills(enabled, priority);
        CREATE INDEX IF NOT EXISTS idx_ai_skills_source
            ON ai_skills(source);

        CREATE TABLE IF NOT EXISTS ai_experiences (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            experience_key  TEXT NOT NULL UNIQUE,
            title           TEXT NOT NULL,
            symptom         TEXT NOT NULL DEFAULT '',
            cause           TEXT NOT NULL DEFAULT '',
            solution        TEXT NOT NULL DEFAULT '',
            scenario        TEXT NOT NULL DEFAULT '',
            source          TEXT NOT NULL DEFAULT 'user',
            tags            TEXT NOT NULL DEFAULT '[]',
            references_json TEXT NOT NULL DEFAULT '[]',
            enabled         INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at      TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ai_experiences_enabled
            ON ai_experiences(enabled, updated_at);

        CREATE TABLE IF NOT EXISTS ai_runbooks (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            runbook_key  TEXT NOT NULL UNIQUE,
            name         TEXT NOT NULL,
            description  TEXT NOT NULL DEFAULT '',
            scenario     TEXT NOT NULL DEFAULT '',
            tags         TEXT NOT NULL DEFAULT '[]',
            steps_json   TEXT NOT NULL DEFAULT '[]',
            enabled      INTEGER NOT NULL DEFAULT 1,
            allow_mcp    INTEGER NOT NULL DEFAULT 0,
            created_at   TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at   TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at   TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ai_runbooks_enabled
            ON ai_runbooks(enabled, updated_at);
        ",
    )?;

    set_version(conn, 12)?;
    Ok(())
}

/// v12 -> v13: 经验库 Markdown 文件路径
fn migrate_v12_to_v13(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v12 -> v13（经验库 Markdown 文件路径）");

    conn.execute_batch(
        "
        ALTER TABLE ai_experiences ADD COLUMN markdown_path TEXT NOT NULL DEFAULT '';
        ",
    )?;

    set_version(conn, 13)?;
    Ok(())
}

/// v13 -> v14: 服务器/数据库/Redis 资源监控目标与快照
fn migrate_v13_to_v14(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v13 -> v14（资源监控）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS resource_monitor_targets (
            id                   INTEGER PRIMARY KEY AUTOINCREMENT,
            target_type          TEXT NOT NULL,
            target_key           TEXT NOT NULL,
            display_name         TEXT NOT NULL,
            enabled              INTEGER NOT NULL DEFAULT 1,
            collect_interval_sec INTEGER NOT NULL DEFAULT 60,
            last_status          TEXT NOT NULL DEFAULT 'unknown',
            last_collected_at    TEXT DEFAULT NULL,
            last_error           TEXT DEFAULT NULL,
            created_at           TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at           TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at           TEXT DEFAULT NULL,
            UNIQUE(target_type, target_key)
        );

        CREATE INDEX IF NOT EXISTS idx_resource_monitor_targets_type
            ON resource_monitor_targets(target_type, enabled);

        CREATE TABLE IF NOT EXISTS resource_metric_snapshots (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            target_type    TEXT NOT NULL,
            target_key     TEXT NOT NULL,
            status         TEXT NOT NULL,
            collected_at   TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            duration_ms    INTEGER NOT NULL DEFAULT 0,
            summary_json   TEXT NOT NULL,
            metrics_json   TEXT NOT NULL,
            error          TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_resource_metric_target_time
            ON resource_metric_snapshots(target_type, target_key, collected_at DESC);
        ",
    )?;

    set_version(conn, 14)?;
    Ok(())
}

/// v0 -> v1: 初始化表结构
fn migrate_v0_to_v1(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v0 -> v1");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS app_config (
            key         TEXT PRIMARY KEY,
            value       TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        -- 插入默认配置
        INSERT OR IGNORE INTO app_config (key, value) VALUES ('theme', 'light');
        INSERT OR IGNORE INTO app_config (key, value) VALUES ('language', 'zh-CN');
        INSERT OR IGNORE INTO app_config (key, value) VALUES ('sidebar_collapsed', 'false');
        ",
    )?;

    set_version(conn, 1)?;
    Ok(())
}

/// v1 -> v2: 添加软删除支持
fn migrate_v1_to_v2(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v1 -> v2（软删除支持）");

    conn.execute_batch(
        "
        -- app_config 添加 deleted_at 软删除字段
        ALTER TABLE app_config ADD COLUMN deleted_at TEXT DEFAULT NULL;
        ",
    )?;

    set_version(conn, 2)?;
    Ok(())
}

/// v2 -> v3: AI Provider 管理
fn migrate_v2_to_v3(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v2 -> v3（AI Provider 管理）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS ai_providers (
            key               TEXT PRIMARY KEY,
            name              TEXT NOT NULL,
            region            TEXT NOT NULL,
            protocol          TEXT NOT NULL,
            default_model     TEXT NOT NULL,
            status            TEXT NOT NULL,
            endpoint          TEXT NOT NULL,
            auth_type         TEXT NOT NULL,
            secret_nonce      TEXT DEFAULT NULL,
            secret_ciphertext TEXT DEFAULT NULL,
            latency_ms        INTEGER DEFAULT NULL,
            cost_level        TEXT NOT NULL,
            capabilities      TEXT NOT NULL DEFAULT '[]',
            models            TEXT NOT NULL DEFAULT '[]',
            scenario_fit      TEXT NOT NULL DEFAULT '[]',
            fallback          TEXT NOT NULL DEFAULT '',
            enabled           INTEGER NOT NULL DEFAULT 1,
            created_at        TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at        TEXT DEFAULT NULL
        );

        CREATE TABLE IF NOT EXISTS ai_provider_routes (
            scenario              TEXT PRIMARY KEY,
            primary_provider_key  TEXT NOT NULL,
            fallback_provider_key TEXT NOT NULL,
            requirement           TEXT NOT NULL,
            created_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE INDEX IF NOT EXISTS idx_ai_providers_region ON ai_providers(region);
        CREATE INDEX IF NOT EXISTS idx_ai_providers_status ON ai_providers(status);
        ",
    )?;

    set_version(conn, 3)?;
    Ok(())
}

/// v3 -> v4: 移除 AI Provider 后端预置数据
fn migrate_v3_to_v4(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v3 -> v4（移除 AI Provider 预置数据）");

    conn.execute_batch(
        "
        DELETE FROM ai_provider_routes
        WHERE scenario IN (
            'command_generation',
            'high_risk_review',
            'log_explanation',
            'mcp_tool_calling',
            'china_general_chat'
        );

        DELETE FROM ai_providers
        WHERE key IN (
            'openai',
            'anthropic',
            'gemini',
            'deepseek',
            'glm',
            'kimi',
            'minimax',
            'xiaomi'
        )
        AND secret_ciphertext IS NULL
        AND secret_nonce IS NULL;
        ",
    )?;

    set_version(conn, 4)?;
    Ok(())
}

/// v4 -> v5: SSH 服务器管理
fn migrate_v4_to_v5(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v4 -> v5（SSH 服务器管理）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS ssh_servers (
            alias             TEXT PRIMARY KEY,
            group_name        TEXT NOT NULL,
            host              TEXT NOT NULL,
            port              INTEGER NOT NULL DEFAULT 22,
            username          TEXT NOT NULL DEFAULT '',
            source            TEXT NOT NULL DEFAULT 'manual',
            auth_type         TEXT NOT NULL DEFAULT 'key',
            auth_ref          TEXT NOT NULL DEFAULT '',
            identity_file     TEXT NOT NULL DEFAULT '',
            proxy_jump        TEXT NOT NULL DEFAULT '',
            ai_policy         TEXT NOT NULL DEFAULT 'L2',
            status            TEXT NOT NULL DEFAULT 'unknown',
            enabled           INTEGER NOT NULL DEFAULT 1,
            last_connected_at TEXT DEFAULT NULL,
            created_at        TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at        TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ssh_servers_group ON ssh_servers(group_name);
        CREATE INDEX IF NOT EXISTS idx_ssh_servers_status ON ssh_servers(status);
        CREATE INDEX IF NOT EXISTS idx_ssh_servers_source ON ssh_servers(source);
        ",
    )?;

    set_version(conn, 5)?;
    Ok(())
}

/// v5 -> v6: SSH 服务器直接密码加密存储
fn migrate_v5_to_v6(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v5 -> v6（SSH 服务器直接密码）");

    conn.execute_batch(
        "
        ALTER TABLE ssh_servers ADD COLUMN password_nonce TEXT DEFAULT NULL;
        ALTER TABLE ssh_servers ADD COLUMN password_ciphertext TEXT DEFAULT NULL;
        UPDATE ssh_servers SET auth_type = 'password_ref' WHERE auth_type = 'password';
        ",
    )?;

    set_version(conn, 6)?;
    Ok(())
}

/// v6 -> v7: 凭据保险库
fn migrate_v6_to_v7(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v6 -> v7（凭据保险库）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS credential_vault (
            key               TEXT PRIMARY KEY,
            credential_type   TEXT NOT NULL,
            scope             TEXT NOT NULL DEFAULT '',
            status            TEXT NOT NULL DEFAULT 'normal',
            description       TEXT NOT NULL DEFAULT '',
            secret_nonce      TEXT DEFAULT NULL,
            secret_ciphertext TEXT DEFAULT NULL,
            enabled           INTEGER NOT NULL DEFAULT 1,
            rotated_at        TEXT DEFAULT NULL,
            created_at        TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at        TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_credential_vault_type ON credential_vault(credential_type);
        CREATE INDEX IF NOT EXISTS idx_credential_vault_status ON credential_vault(status);
        CREATE INDEX IF NOT EXISTS idx_credential_vault_scope ON credential_vault(scope);
        ",
    )?;

    set_version(conn, 7)?;
    Ok(())
}

/// v7 -> v8: 审批队列
fn migrate_v7_to_v8(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v7 -> v8（审批队列）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS approval_requests (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            source        TEXT NOT NULL,
            requester     TEXT NOT NULL DEFAULT '',
            server_alias  TEXT NOT NULL DEFAULT '',
            action        TEXT NOT NULL,
            risk          TEXT NOT NULL DEFAULT 'L2',
            status        TEXT NOT NULL DEFAULT 'pending',
            command       TEXT NOT NULL DEFAULT '',
            resource      TEXT NOT NULL DEFAULT '',
            reason        TEXT NOT NULL DEFAULT '',
            summary       TEXT NOT NULL DEFAULT '',
            payload_json  TEXT NOT NULL DEFAULT '{}',
            decision_note TEXT NOT NULL DEFAULT '',
            decided_by    TEXT NOT NULL DEFAULT '',
            decided_at    TEXT DEFAULT NULL,
            expires_at    TEXT DEFAULT NULL,
            created_at    TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at    TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_approval_requests_status ON approval_requests(status);
        CREATE INDEX IF NOT EXISTS idx_approval_requests_source ON approval_requests(source);
        CREATE INDEX IF NOT EXISTS idx_approval_requests_server ON approval_requests(server_alias);
        CREATE INDEX IF NOT EXISTS idx_approval_requests_created ON approval_requests(created_at);
        ",
    )?;

    set_version(conn, 8)?;
    Ok(())
}

/// v8 -> v9: 堡垒机 Web SSH 会话入口
fn migrate_v8_to_v9(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v8 -> v9（堡垒机会话）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS jumpserver_sessions (
            key               TEXT PRIMARY KEY,
            name              TEXT NOT NULL,
            endpoint          TEXT NOT NULL,
            web_url           TEXT NOT NULL,
            session_ref       TEXT NOT NULL DEFAULT '',
            group_name        TEXT NOT NULL DEFAULT '堡垒机',
            account_hint      TEXT NOT NULL DEFAULT '',
            asset_hint        TEXT NOT NULL DEFAULT '',
            protocol          TEXT NOT NULL DEFAULT 'web_ssh',
            ai_mode           TEXT NOT NULL DEFAULT 'suggest_only',
            status            TEXT NOT NULL DEFAULT 'unknown',
            notes             TEXT NOT NULL DEFAULT '',
            enabled           INTEGER NOT NULL DEFAULT 1,
            last_opened_at    TEXT DEFAULT NULL,
            created_at        TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at        TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at        TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_jumpserver_sessions_group ON jumpserver_sessions(group_name);
        CREATE INDEX IF NOT EXISTS idx_jumpserver_sessions_status ON jumpserver_sessions(status);
        CREATE INDEX IF NOT EXISTS idx_jumpserver_sessions_enabled ON jumpserver_sessions(enabled);
        ",
    )?;

    set_version(conn, 9)?;
    Ok(())
}

/// v9 -> v10: 审计日志
fn migrate_v9_to_v10(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v9 -> v10（审计日志）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS audit_logs (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            occurred_at  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            actor        TEXT NOT NULL DEFAULT '',
            source       TEXT NOT NULL DEFAULT '',
            server_alias TEXT NOT NULL DEFAULT '',
            action       TEXT NOT NULL,
            risk         TEXT NOT NULL DEFAULT 'readonly',
            result       TEXT NOT NULL DEFAULT '',
            summary      TEXT NOT NULL DEFAULT '',
            detail_json  TEXT NOT NULL DEFAULT '{}',
            request_id   TEXT NOT NULL DEFAULT '',
            approval_id  INTEGER DEFAULT NULL,
            created_at   TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at   TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_audit_logs_occurred ON audit_logs(occurred_at);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_actor ON audit_logs(actor);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_source ON audit_logs(source);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_server ON audit_logs(server_alias);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_risk ON audit_logs(risk);
        CREATE INDEX IF NOT EXISTS idx_audit_logs_result ON audit_logs(result);
        ",
    )?;

    set_version(conn, 10)?;
    Ok(())
}

/// v10 -> v11: 数据库管理运维
fn migrate_v10_to_v11(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v10 -> v11（数据库管理运维）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS database_connections (
            key                 TEXT PRIMARY KEY,
            name                TEXT NOT NULL,
            group_name          TEXT NOT NULL DEFAULT '默认分组',
            db_type             TEXT NOT NULL,
            connection_mode     TEXT NOT NULL DEFAULT 'direct',
            host                TEXT NOT NULL DEFAULT '',
            port                INTEGER NOT NULL DEFAULT 0,
            database_name       TEXT NOT NULL DEFAULT '',
            username            TEXT NOT NULL DEFAULT '',
            auth_type           TEXT NOT NULL DEFAULT 'direct_password',
            credential_ref      TEXT NOT NULL DEFAULT '',
            password_nonce      TEXT DEFAULT NULL,
            password_ciphertext TEXT DEFAULT NULL,
            ssh_server_alias    TEXT NOT NULL DEFAULT '',
            security_mode       TEXT NOT NULL DEFAULT 'approval_all',
            ai_policy           TEXT NOT NULL DEFAULT 'L2',
            page_size           INTEGER NOT NULL DEFAULT 500,
            status              TEXT NOT NULL DEFAULT 'unknown',
            enabled             INTEGER NOT NULL DEFAULT 1,
            last_connected_at   TEXT DEFAULT NULL,
            notes               TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_database_connections_type ON database_connections(db_type);
        CREATE INDEX IF NOT EXISTS idx_database_connections_group ON database_connections(group_name);
        CREATE INDEX IF NOT EXISTS idx_database_connections_status ON database_connections(status);
        CREATE INDEX IF NOT EXISTS idx_database_connections_enabled ON database_connections(enabled);

        CREATE TABLE IF NOT EXISTS database_query_history (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_key  TEXT NOT NULL,
            db_type         TEXT NOT NULL DEFAULT '',
            sql_text        TEXT NOT NULL,
            risk_level      TEXT NOT NULL DEFAULT 'readonly',
            row_count       INTEGER NOT NULL DEFAULT 0,
            duration_ms     INTEGER NOT NULL DEFAULT 0,
            result          TEXT NOT NULL DEFAULT '',
            error_message   TEXT NOT NULL DEFAULT '',
            actor           TEXT NOT NULL DEFAULT '',
            source          TEXT NOT NULL DEFAULT 'ui',
            approval_id     INTEGER DEFAULT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at      TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_database_query_history_connection ON database_query_history(connection_key);
        CREATE INDEX IF NOT EXISTS idx_database_query_history_created ON database_query_history(created_at);
        CREATE INDEX IF NOT EXISTS idx_database_query_history_risk ON database_query_history(risk_level);

        CREATE TABLE IF NOT EXISTS database_saved_queries (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_key  TEXT NOT NULL DEFAULT '',
            title           TEXT NOT NULL,
            sql_text        TEXT NOT NULL,
            tags            TEXT NOT NULL DEFAULT '[]',
            created_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at      TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_database_saved_queries_connection ON database_saved_queries(connection_key);
        CREATE INDEX IF NOT EXISTS idx_database_saved_queries_updated ON database_saved_queries(updated_at);

        CREATE TABLE IF NOT EXISTS database_export_tasks (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_key  TEXT NOT NULL,
            task_type       TEXT NOT NULL DEFAULT 'query_csv',
            status          TEXT NOT NULL DEFAULT 'pending',
            file_path       TEXT NOT NULL DEFAULT '',
            summary         TEXT NOT NULL DEFAULT '',
            error_message   TEXT NOT NULL DEFAULT '',
            created_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at      TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_database_export_tasks_connection ON database_export_tasks(connection_key);
        CREATE INDEX IF NOT EXISTS idx_database_export_tasks_status ON database_export_tasks(status);
        ",
    )?;

    set_version(conn, 11)?;
    Ok(())
}
