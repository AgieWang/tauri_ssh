use rusqlite::{Connection, OptionalExtension};

use crate::error::AppError;

/// 当前 Schema 版本
pub const SCHEMA_VERSION: i32 = 51;

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

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AppError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == column);
    if !exists {
        conn.execute(
            &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition),
            [],
        )?;
    }
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
            15 => migrate_v15_to_v16(conn)?,
            16 => migrate_v16_to_v17(conn)?,
            17 => migrate_v17_to_v18(conn)?,
            18 => migrate_v18_to_v19(conn)?,
            19 => migrate_v19_to_v20(conn)?,
            20 => migrate_v20_to_v21(conn)?,
            21 => migrate_v21_to_v22(conn)?,
            22 => migrate_v22_to_v23(conn)?,
            23 => migrate_v23_to_v24(conn)?,
            24 => migrate_v24_to_v25(conn)?,
            25 => migrate_v25_to_v26(conn)?,
            26 => migrate_v26_to_v27(conn)?,
            27 => migrate_v27_to_v28(conn)?,
            28 => migrate_v28_to_v29(conn)?,
            29 => migrate_v29_to_v30(conn)?,
            30 => migrate_v30_to_v31(conn)?,
            31 => migrate_v31_to_v32(conn)?,
            32 => migrate_v32_to_v33(conn)?,
            33 => migrate_v33_to_v34(conn)?,
            34 => migrate_v34_to_v35(conn)?,
            35 => migrate_v35_to_v36(conn)?,
            36 => migrate_v36_to_v37(conn)?,
            37 => migrate_v37_to_v38(conn)?,
            38 => migrate_v38_to_v39(conn)?,
            39 => migrate_v39_to_v40(conn)?,
            40 => migrate_v40_to_v41(conn)?,
            41 => migrate_v41_to_v42(conn)?,
            42 => migrate_v42_to_v43(conn)?,
            43 => migrate_v43_to_v44(conn)?,
            44 => migrate_v44_to_v45(conn)?,
            45 => migrate_v45_to_v46(conn)?,
            46 => migrate_v46_to_v47(conn)?,
            47 => migrate_v47_to_v48(conn)?,
            48 => migrate_v48_to_v49(conn)?,
            49 => migrate_v49_to_v50(conn)?,
            50 => migrate_v50_to_v51(conn)?,
            _ => {
                return Err(AppError::Custom(format!("未知的数据库版本: {}", version)));
            }
        }
        version = get_version(conn)?;
    }

    log::info!("数据库迁移完成, 当前版本: {}", version);
    Ok(())
}

/// v19 -> v20: Jenkins 构建运维工作台基础表
fn migrate_v19_to_v20(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v19 -> v20（Jenkins 构建运维工作台）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS jenkins_connections (
            id                          INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_key              TEXT NOT NULL UNIQUE,
            config_version              INTEGER NOT NULL DEFAULT 1,
            name                        TEXT NOT NULL,
            base_url                    TEXT NOT NULL,
            credential_key              TEXT NOT NULL DEFAULT '',
            credential_display_name     TEXT NOT NULL DEFAULT '',
            username_masked             TEXT NOT NULL DEFAULT '',
            ssh_server_alias            TEXT NOT NULL DEFAULT '',
            environment                 TEXT NOT NULL DEFAULT 'dev',
            environment_label           TEXT NOT NULL DEFAULT '',
            tls_verify                  INTEGER NOT NULL DEFAULT 1,
            default_view                TEXT NOT NULL DEFAULT '',
            default_folder              TEXT NOT NULL DEFAULT '',
            allow_mcp_read              INTEGER NOT NULL DEFAULT 1,
            allow_mcp_write             INTEGER NOT NULL DEFAULT 0,
            approval_policy             TEXT NOT NULL DEFAULT 'manual',
            parameter_prefill_enabled   INTEGER NOT NULL DEFAULT 1,
            risk_rules_json             TEXT NOT NULL DEFAULT '{}',
            notify_on_success           INTEGER NOT NULL DEFAULT 0,
            notify_on_failure           INTEGER NOT NULL DEFAULT 1,
            notify_on_unstable          INTEGER NOT NULL DEFAULT 1,
            notify_on_aborted           INTEGER NOT NULL DEFAULT 1,
            status                      TEXT NOT NULL DEFAULT 'draft',
            version                     TEXT NOT NULL DEFAULT '',
            capabilities_json           TEXT NOT NULL DEFAULT '{}',
            last_error_code             TEXT NOT NULL DEFAULT '',
            last_error_message          TEXT NOT NULL DEFAULT '',
            description                 TEXT NOT NULL DEFAULT '',
            enabled                     INTEGER NOT NULL DEFAULT 0,
            last_tested_at              TEXT DEFAULT NULL,
            created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at                  TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_jenkins_connections_key
            ON jenkins_connections(connection_key);
        CREATE INDEX IF NOT EXISTS idx_jenkins_connections_enabled
            ON jenkins_connections(enabled, deleted_at);

        CREATE TABLE IF NOT EXISTS jenkins_recent_jobs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_key      TEXT NOT NULL,
            job_full_name       TEXT NOT NULL,
            display_name        TEXT NOT NULL,
            url                 TEXT NOT NULL DEFAULT '',
            job_type            TEXT NOT NULL DEFAULT 'job',
            normalized_status   TEXT NOT NULL DEFAULT 'unknown',
            raw_color           TEXT NOT NULL DEFAULT '',
            buildable           INTEGER NOT NULL DEFAULT 1,
            last_build_number   INTEGER DEFAULT NULL,
            last_build_status   TEXT NOT NULL DEFAULT '',
            favorite            INTEGER NOT NULL DEFAULT 0,
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(connection_key, job_full_name)
        );

        CREATE INDEX IF NOT EXISTS idx_jenkins_recent_jobs_connection
            ON jenkins_recent_jobs(connection_key, updated_at DESC);

        CREATE TABLE IF NOT EXISTS jenkins_build_runs (
            id                              INTEGER PRIMARY KEY AUTOINCREMENT,
            run_key                         TEXT NOT NULL UNIQUE,
            request_id                      TEXT NOT NULL DEFAULT '',
            approval_id                     INTEGER DEFAULT NULL,
            connection_key                  TEXT NOT NULL,
            connection_config_version       INTEGER NOT NULL DEFAULT 1,
            job_full_name                   TEXT NOT NULL,
            queue_id                        TEXT NOT NULL DEFAULT '',
            build_number                    INTEGER DEFAULT NULL,
            status                          TEXT NOT NULL DEFAULT 'queued',
            status_source                   TEXT NOT NULL DEFAULT 'local',
            result                          TEXT NOT NULL DEFAULT '',
            request_hash                    TEXT NOT NULL DEFAULT '',
            parameters_redacted_json        TEXT NOT NULL DEFAULT '{}',
            cause                           TEXT NOT NULL DEFAULT '',
            created_by                      TEXT NOT NULL DEFAULT 'local-user',
            created_at                      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at                      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            started_at                      TEXT DEFAULT NULL,
            finished_at                     TEXT DEFAULT NULL,
            last_error_code                 TEXT NOT NULL DEFAULT '',
            last_error_message              TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_jenkins_build_runs_connection
            ON jenkins_build_runs(connection_key, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_jenkins_build_runs_job
            ON jenkins_build_runs(connection_key, job_full_name, created_at DESC);

        CREATE TABLE IF NOT EXISTS jenkins_recent_parameter_values (
            id                      INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_key          TEXT NOT NULL,
            job_full_name           TEXT NOT NULL,
            parameter_name          TEXT NOT NULL,
            requester               TEXT NOT NULL DEFAULT '__shared__',
            value_kind              TEXT NOT NULL DEFAULT 'plain',
            value_json              TEXT NOT NULL DEFAULT '{}',
            sensitive               INTEGER NOT NULL DEFAULT 0,
            updated_from_run_key    TEXT NOT NULL DEFAULT '',
            updated_at              TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(connection_key, job_full_name, parameter_name, requester)
        );

        CREATE INDEX IF NOT EXISTS idx_jenkins_recent_parameter_values_job
            ON jenkins_recent_parameter_values(connection_key, job_full_name, requester);

        CREATE TABLE IF NOT EXISTS jenkins_artifacts (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            artifact_key        TEXT NOT NULL UNIQUE,
            request_id          TEXT NOT NULL DEFAULT '',
            connection_key      TEXT NOT NULL,
            job_full_name       TEXT NOT NULL,
            build_number        INTEGER NOT NULL,
            file_name           TEXT NOT NULL,
            relative_path       TEXT NOT NULL,
            local_path          TEXT NOT NULL DEFAULT '',
            size_bytes          INTEGER DEFAULT NULL,
            sha256              TEXT NOT NULL DEFAULT '',
            status              TEXT NOT NULL DEFAULT 'recorded',
            downloaded_at       TEXT DEFAULT NULL,
            cleaned_at          TEXT DEFAULT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE INDEX IF NOT EXISTS idx_jenkins_artifacts_build
            ON jenkins_artifacts(connection_key, job_full_name, build_number);
        ",
    )?;

    set_version(conn, 20)?;
    Ok(())
}

/// v22 -> v23: Jenkins 构建参数模板
fn migrate_v22_to_v23(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v22 -> v23（Jenkins 构建参数模板）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS jenkins_parameter_templates (
            id                          INTEGER PRIMARY KEY AUTOINCREMENT,
            template_key                TEXT NOT NULL UNIQUE,
            connection_key              TEXT NOT NULL,
            job_full_name               TEXT NOT NULL,
            name                        TEXT NOT NULL,
            parameters_json             TEXT NOT NULL DEFAULT '{\"parameters\":[]}',
            parameter_definition_hash   TEXT NOT NULL DEFAULT '',
            created_by                  TEXT NOT NULL DEFAULT 'local-user',
            created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(connection_key, job_full_name, name, created_by)
        );

        CREATE INDEX IF NOT EXISTS idx_jenkins_parameter_templates_job
            ON jenkins_parameter_templates(connection_key, job_full_name, created_by, updated_at DESC);
        ",
    )?;
    set_version(conn, 23)?;
    Ok(())
}

/// v23 -> v24: Jenkins Job 收藏标记
fn migrate_v23_to_v24(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v23 -> v24（Jenkins Job 收藏标记）");
    add_column_if_missing(
        conn,
        "jenkins_recent_jobs",
        "favorite",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    set_version(conn, 24)?;
    Ok(())
}

/// v24 -> v25: 团队知识库核心目录、文档、分块、关系和任务
fn migrate_v24_to_v25(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v24 -> v25（团队知识库核心数据）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_projects (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            project_key         TEXT NOT NULL UNIQUE,
            name                TEXT NOT NULL,
            aliases_json        TEXT NOT NULL DEFAULT '[]',
            description         TEXT NOT NULL DEFAULT '',
            git_workspace_key   TEXT NOT NULL DEFAULT '',
            default_branch      TEXT NOT NULL DEFAULT '',
            enabled             INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_projects_enabled
            ON knowledge_projects(enabled, deleted_at);

        CREATE TABLE IF NOT EXISTS knowledge_releases (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id          INTEGER NOT NULL,
            version             TEXT NOT NULL,
            tag_name            TEXT NOT NULL DEFAULT '',
            branch              TEXT NOT NULL DEFAULT '',
            commit_sha          TEXT NOT NULL DEFAULT '',
            description         TEXT NOT NULL DEFAULT '',
            released_at         TEXT DEFAULT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL,
            UNIQUE(project_id, version)
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_releases_project
            ON knowledge_releases(project_id, released_at);

        CREATE TABLE IF NOT EXISTS knowledge_sources (
            id                          INTEGER PRIMARY KEY AUTOINCREMENT,
            source_key                  TEXT NOT NULL UNIQUE,
            project_id                  INTEGER DEFAULT NULL,
            source_type                 TEXT NOT NULL,
            display_name                TEXT NOT NULL,
            root_path                   TEXT NOT NULL DEFAULT '',
            git_workspace_key           TEXT NOT NULL DEFAULT '',
            include_globs_json          TEXT NOT NULL DEFAULT '[]',
            exclude_globs_json          TEXT NOT NULL DEFAULT '[]',
            version_strategy            TEXT NOT NULL DEFAULT 'manual',
            sync_mode                   TEXT NOT NULL DEFAULT 'manual',
            allow_remote_embedding      INTEGER NOT NULL DEFAULT 0,
            enabled                     INTEGER NOT NULL DEFAULT 1,
            last_commit_sha             TEXT NOT NULL DEFAULT '',
            last_sync_status            TEXT NOT NULL DEFAULT 'never',
            last_synced_at              TEXT DEFAULT NULL,
            last_error                  TEXT DEFAULT NULL,
            created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at                  TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_sources_project
            ON knowledge_sources(project_id, enabled, deleted_at);

        CREATE TABLE IF NOT EXISTS knowledge_documents (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            document_key        TEXT NOT NULL UNIQUE,
            project_id          INTEGER DEFAULT NULL,
            source_id           INTEGER DEFAULT NULL,
            doc_type            TEXT NOT NULL,
            title               TEXT NOT NULL,
            logical_path        TEXT NOT NULL DEFAULT '',
            status              TEXT NOT NULL DEFAULT 'active',
            sensitivity         TEXT NOT NULL DEFAULT 'internal',
            tags_json           TEXT NOT NULL DEFAULT '[]',
            latest_version_id   INTEGER DEFAULT NULL,
            allow_ai            INTEGER NOT NULL DEFAULT 1,
            allow_mcp           INTEGER NOT NULL DEFAULT 0,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_documents_project_type
            ON knowledge_documents(project_id, doc_type, status, deleted_at);
        CREATE INDEX IF NOT EXISTS idx_knowledge_documents_source
            ON knowledge_documents(source_id, status, deleted_at);

        CREATE TABLE IF NOT EXISTS knowledge_document_versions (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id         INTEGER NOT NULL,
            release_id          INTEGER DEFAULT NULL,
            version_label       TEXT NOT NULL DEFAULT '',
            git_branch          TEXT NOT NULL DEFAULT '',
            commit_sha          TEXT NOT NULL DEFAULT '',
            source_path         TEXT NOT NULL DEFAULT '',
            mime_type           TEXT NOT NULL DEFAULT 'text/markdown',
            content             TEXT NOT NULL,
            content_hash        TEXT NOT NULL,
            parsed_meta_json    TEXT NOT NULL DEFAULT '{}',
            token_estimate      INTEGER NOT NULL DEFAULT 0,
            valid               INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(document_id, version_label, content_hash, source_path)
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_doc_versions_release
            ON knowledge_document_versions(release_id, document_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_doc_versions_commit
            ON knowledge_document_versions(commit_sha, document_id);

        CREATE TABLE IF NOT EXISTS knowledge_chunks (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            document_version_id   INTEGER NOT NULL,
            chunk_index           INTEGER NOT NULL,
            heading_path          TEXT NOT NULL DEFAULT '',
            content               TEXT NOT NULL,
            content_hash          TEXT NOT NULL,
            location_json         TEXT NOT NULL DEFAULT '{}',
            token_estimate        INTEGER NOT NULL DEFAULT 0,
            embedding_status      TEXT NOT NULL DEFAULT 'pending',
            created_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(document_version_id, chunk_index)
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_version
            ON knowledge_chunks(document_version_id, chunk_index);
        CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_hash
            ON knowledge_chunks(content_hash, embedding_status);

        CREATE TABLE IF NOT EXISTS knowledge_relations (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            from_type           TEXT NOT NULL,
            from_key            TEXT NOT NULL,
            relation_type       TEXT NOT NULL,
            to_type             TEXT NOT NULL,
            to_key              TEXT NOT NULL,
            evidence_json       TEXT NOT NULL DEFAULT '{}',
            confidence          REAL NOT NULL DEFAULT 1.0,
            confirmed           INTEGER NOT NULL DEFAULT 1,
            source              TEXT NOT NULL DEFAULT 'user',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL,
            UNIQUE(from_type, from_key, relation_type, to_type, to_key, source)
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_relations_from
            ON knowledge_relations(from_type, from_key, relation_type, deleted_at);
        CREATE INDEX IF NOT EXISTS idx_knowledge_relations_to
            ON knowledge_relations(to_type, to_key, relation_type, deleted_at);

        CREATE TABLE IF NOT EXISTS knowledge_jobs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            job_key             TEXT NOT NULL UNIQUE,
            job_type            TEXT NOT NULL,
            source_id           INTEGER DEFAULT NULL,
            profile_id          INTEGER DEFAULT NULL,
            status              TEXT NOT NULL,
            progress_current    INTEGER NOT NULL DEFAULT 0,
            progress_total      INTEGER NOT NULL DEFAULT 0,
            message             TEXT NOT NULL DEFAULT '',
            error               TEXT DEFAULT NULL,
            checkpoint_json     TEXT NOT NULL DEFAULT '{}',
            heartbeat_at        TEXT DEFAULT NULL,
            cancel_requested    INTEGER NOT NULL DEFAULT 0,
            started_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            finished_at         TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_jobs_status
            ON knowledge_jobs(status, started_at);

        CREATE TABLE IF NOT EXISTS knowledge_generation_runs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            run_key             TEXT NOT NULL UNIQUE,
            project_id          INTEGER NOT NULL,
            release_id          INTEGER DEFAULT NULL,
            source_id           INTEGER DEFAULT NULL,
            sync_job_id         INTEGER DEFAULT NULL,
            template_version    TEXT NOT NULL,
            document_types_json TEXT NOT NULL DEFAULT '[]',
            input_hash          TEXT NOT NULL,
            status              TEXT NOT NULL,
            generated_count     INTEGER NOT NULL DEFAULT 0,
            skipped_count       INTEGER NOT NULL DEFAULT 0,
            ai_summary_enabled  INTEGER NOT NULL DEFAULT 0,
            ai_provider_key     TEXT NOT NULL DEFAULT '',
            ai_model            TEXT NOT NULL DEFAULT '',
            error               TEXT DEFAULT NULL,
            started_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            finished_at         TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_generation_runs_project
            ON knowledge_generation_runs(project_id, release_id, started_at);
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_generation_runs_input
            ON knowledge_generation_runs(project_id, ifnull(release_id, -1), template_version, input_hash);
        ",
    )?;
    set_version(conn, 25)?;
    Ok(())
}

/// v25 -> v26: Embedding Profile 与本地向量
fn migrate_v25_to_v26(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v25 -> v26（Embedding Profile 与本地向量）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_embedding_profiles (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_key         TEXT NOT NULL UNIQUE,
            name                TEXT NOT NULL,
            mode                TEXT NOT NULL,
            provider_key        TEXT NOT NULL DEFAULT '',
            model               TEXT NOT NULL,
            model_revision      TEXT NOT NULL DEFAULT '',
            dimension           INTEGER NOT NULL DEFAULT 0,
            normalized          INTEGER NOT NULL DEFAULT 1,
            config_json         TEXT NOT NULL DEFAULT '{}',
            fingerprint         TEXT NOT NULL UNIQUE,
            status              TEXT NOT NULL DEFAULT 'draft',
            is_active           INTEGER NOT NULL DEFAULT 0,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_embedding_profiles_active
            ON knowledge_embedding_profiles(is_active, status);
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_embedding_profiles_one_active
            ON knowledge_embedding_profiles(is_active)
            WHERE is_active = 1;

        CREATE TABLE IF NOT EXISTS knowledge_chunk_embeddings (
            chunk_id       INTEGER NOT NULL,
            profile_id     INTEGER NOT NULL,
            dimension      INTEGER NOT NULL CHECK(dimension > 0),
            vector_blob    BLOB NOT NULL,
            vector_norm    REAL NOT NULL CHECK(vector_norm >= 0),
            content_hash   TEXT NOT NULL DEFAULT '',
            created_at     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            PRIMARY KEY(chunk_id, profile_id)
        );

        CREATE INDEX IF NOT EXISTS idx_knowledge_embeddings_profile
            ON knowledge_chunk_embeddings(profile_id, chunk_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_embeddings_reuse
            ON knowledge_chunk_embeddings(profile_id, content_hash);
        ",
    )?;
    set_version(conn, 26)?;
    Ok(())
}

/// v26 -> v27: 禅道连接、映射、游标、实体和关系
fn migrate_v26_to_v27(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v26 -> v27（禅道知识同步）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS zentao_connections (
            id                        INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_key            TEXT NOT NULL UNIQUE,
            name                      TEXT NOT NULL,
            base_url                  TEXT NOT NULL,
            api_version               TEXT NOT NULL DEFAULT 'auto',
            auth_mode                 TEXT NOT NULL DEFAULT 'auto',
            endpoint_profile          TEXT NOT NULL DEFAULT '',
            credential_key            TEXT NOT NULL,
            tls_verify                INTEGER NOT NULL DEFAULT 1,
            request_timeout_seconds   INTEGER NOT NULL DEFAULT 30,
            page_size                 INTEGER NOT NULL DEFAULT 100,
            rate_limit_per_second     REAL NOT NULL DEFAULT 5,
            capabilities_json         TEXT NOT NULL DEFAULT '{}',
            enabled                   INTEGER NOT NULL DEFAULT 1,
            last_test_status          TEXT NOT NULL DEFAULT 'never',
            last_tested_at            TEXT DEFAULT NULL,
            last_error                TEXT DEFAULT NULL,
            created_at                TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at                TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at                TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_zentao_connections_enabled
            ON zentao_connections(enabled, deleted_at);

        CREATE TABLE IF NOT EXISTS zentao_project_mappings (
            id                          INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_id               INTEGER NOT NULL,
            knowledge_project_id        INTEGER NOT NULL,
            remote_product_id           TEXT NOT NULL DEFAULT '',
            remote_project_id           TEXT NOT NULL,
            remote_execution_ids_json   TEXT NOT NULL DEFAULT '[]',
            release_mapping_json        TEXT NOT NULL DEFAULT '{}',
            sync_scope_json              TEXT NOT NULL DEFAULT '{}',
            sync_since                  TEXT DEFAULT NULL,
            include_comments            INTEGER NOT NULL DEFAULT 0,
            include_worklogs            INTEGER NOT NULL DEFAULT 1,
            include_attachment_metadata INTEGER NOT NULL DEFAULT 1,
            allow_remote_embedding      INTEGER NOT NULL DEFAULT 0,
            allow_remote_ai             INTEGER NOT NULL DEFAULT 0,
            enabled                     INTEGER NOT NULL DEFAULT 1,
            created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at                  TEXT DEFAULT NULL,
            UNIQUE(connection_id, knowledge_project_id, remote_project_id)
        );

        CREATE INDEX IF NOT EXISTS idx_zentao_mappings_project
            ON zentao_project_mappings(knowledge_project_id, enabled, deleted_at);

        CREATE TABLE IF NOT EXISTS zentao_sync_cursors (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            mapping_id          INTEGER NOT NULL,
            entity_type         TEXT NOT NULL,
            last_updated_at     TEXT NOT NULL DEFAULT '',
            last_external_id    TEXT NOT NULL DEFAULT '',
            checkpoint_json     TEXT NOT NULL DEFAULT '{}',
            last_success_at     TEXT DEFAULT NULL,
            last_full_sync_at   TEXT DEFAULT NULL,
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(mapping_id, entity_type)
        );

        CREATE TABLE IF NOT EXISTS zentao_entities (
            id                      INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_id           INTEGER NOT NULL,
            mapping_id              INTEGER NOT NULL,
            knowledge_project_id    INTEGER NOT NULL,
            release_id              INTEGER DEFAULT NULL,
            entity_type             TEXT NOT NULL,
            external_id             TEXT NOT NULL,
            external_key            TEXT NOT NULL UNIQUE,
            title                   TEXT NOT NULL DEFAULT '',
            body_markdown           TEXT NOT NULL DEFAULT '',
            original_status         TEXT NOT NULL DEFAULT '',
            normalized_status       TEXT NOT NULL DEFAULT '',
            assignee_external_id    TEXT NOT NULL DEFAULT '',
            parent_external_key     TEXT NOT NULL DEFAULT '',
            remote_url              TEXT NOT NULL DEFAULT '',
            content_hash            TEXT NOT NULL,
            raw_json_hash           TEXT NOT NULL DEFAULT '',
            raw_snapshot_json       TEXT DEFAULT NULL,
            source_created_at       TEXT DEFAULT NULL,
            source_updated_at       TEXT DEFAULT NULL,
            first_synced_at         TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            last_synced_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            missing_count           INTEGER NOT NULL DEFAULT 0,
            status                  TEXT NOT NULL DEFAULT 'active',
            deleted_at              TEXT DEFAULT NULL,
            UNIQUE(connection_id, entity_type, external_id)
        );

        CREATE INDEX IF NOT EXISTS idx_zentao_entities_project_type
            ON zentao_entities(knowledge_project_id, release_id, entity_type, normalized_status);
        CREATE INDEX IF NOT EXISTS idx_zentao_entities_updated
            ON zentao_entities(mapping_id, entity_type, source_updated_at);

        CREATE TABLE IF NOT EXISTS zentao_entity_relations (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            from_external_key   TEXT NOT NULL,
            relation_type       TEXT NOT NULL,
            to_external_key     TEXT NOT NULL,
            evidence_json       TEXT NOT NULL DEFAULT '{}',
            source              TEXT NOT NULL DEFAULT 'zentao',
            confidence          REAL NOT NULL DEFAULT 1.0,
            confirmed           INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL,
            UNIQUE(from_external_key, relation_type, to_external_key, source)
        );

        CREATE INDEX IF NOT EXISTS idx_zentao_relations_from
            ON zentao_entity_relations(from_external_key, relation_type, deleted_at);
        CREATE INDEX IF NOT EXISTS idx_zentao_relations_to
            ON zentao_entity_relations(to_external_key, relation_type, deleted_at);
        ",
    )?;
    set_version(conn, 27)?;
    Ok(())
}

/// v27 -> v28: Git 与本地源码不可变快照、文件、符号和关系
fn migrate_v27_to_v28(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v27 -> v28（源码知识快照）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_code_snapshots (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_key        TEXT NOT NULL UNIQUE,
            source_id           INTEGER NOT NULL,
            project_id          INTEGER DEFAULT NULL,
            release_id          INTEGER DEFAULT NULL,
            snapshot_type       TEXT NOT NULL,
            ref_name            TEXT NOT NULL DEFAULT '',
            commit_sha          TEXT NOT NULL DEFAULT '',
            base_commit_sha     TEXT NOT NULL DEFAULT '',
            branch_name         TEXT NOT NULL DEFAULT '',
            worktree_dirty      INTEGER NOT NULL DEFAULT 0,
            captured_at         TEXT NOT NULL,
            file_count          INTEGER NOT NULL DEFAULT 0,
            symbol_count        INTEGER NOT NULL DEFAULT 0,
            relation_count      INTEGER NOT NULL DEFAULT 0,
            analyzer_version    TEXT NOT NULL,
            status              TEXT NOT NULL,
            error               TEXT DEFAULT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE INDEX IF NOT EXISTS idx_code_snapshots_source
            ON knowledge_code_snapshots(source_id, captured_at);
        CREATE INDEX IF NOT EXISTS idx_code_snapshots_commit
            ON knowledge_code_snapshots(project_id, commit_sha);

        CREATE TABLE IF NOT EXISTS knowledge_code_files (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id           INTEGER NOT NULL,
            document_version_id   INTEGER DEFAULT NULL,
            relative_path         TEXT NOT NULL,
            language              TEXT NOT NULL DEFAULT 'unknown',
            file_size             INTEGER NOT NULL DEFAULT 0,
            content_hash          TEXT NOT NULL,
            analysis_level        TEXT NOT NULL,
            is_generated          INTEGER NOT NULL DEFAULT 0,
            is_test               INTEGER NOT NULL DEFAULT 0,
            sensitivity           TEXT NOT NULL DEFAULT 'internal',
            status                TEXT NOT NULL DEFAULT 'active',
            skip_reason           TEXT NOT NULL DEFAULT '',
            created_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(snapshot_id, relative_path)
        );

        CREATE INDEX IF NOT EXISTS idx_code_files_snapshot_language
            ON knowledge_code_files(snapshot_id, language, status);
        CREATE INDEX IF NOT EXISTS idx_code_files_hash
            ON knowledge_code_files(snapshot_id, content_hash);

        CREATE TABLE IF NOT EXISTS knowledge_code_symbols (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id         INTEGER NOT NULL,
            file_id             INTEGER NOT NULL,
            symbol_key          TEXT NOT NULL,
            symbol_kind         TEXT NOT NULL,
            name                TEXT NOT NULL,
            qualified_name      TEXT NOT NULL DEFAULT '',
            signature           TEXT NOT NULL DEFAULT '',
            visibility          TEXT NOT NULL DEFAULT '',
            parent_symbol_key   TEXT NOT NULL DEFAULT '',
            start_line          INTEGER NOT NULL,
            start_column        INTEGER NOT NULL DEFAULT 0,
            end_line            INTEGER NOT NULL,
            end_column          INTEGER NOT NULL DEFAULT 0,
            doc_comment         TEXT NOT NULL DEFAULT '',
            content_hash        TEXT NOT NULL,
            analysis_level      TEXT NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(snapshot_id, symbol_key)
        );

        CREATE INDEX IF NOT EXISTS idx_code_symbols_name
            ON knowledge_code_symbols(snapshot_id, name, symbol_kind);
        CREATE INDEX IF NOT EXISTS idx_code_symbols_qualified
            ON knowledge_code_symbols(snapshot_id, qualified_name);

        CREATE TABLE IF NOT EXISTS knowledge_code_relations (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id           INTEGER NOT NULL,
            from_symbol_key       TEXT NOT NULL,
            relation_type         TEXT NOT NULL,
            to_symbol_key         TEXT NOT NULL DEFAULT '',
            to_external_type      TEXT NOT NULL DEFAULT '',
            to_external_key       TEXT NOT NULL DEFAULT '',
            evidence_file_id      INTEGER DEFAULT NULL,
            evidence_start_line   INTEGER DEFAULT NULL,
            evidence_end_line     INTEGER DEFAULT NULL,
            evidence_text         TEXT NOT NULL DEFAULT '',
            resolver              TEXT NOT NULL,
            confidence            REAL NOT NULL DEFAULT 1.0,
            confirmed             INTEGER NOT NULL DEFAULT 1,
            created_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(
                snapshot_id,
                from_symbol_key,
                relation_type,
                to_symbol_key,
                to_external_type,
                to_external_key,
                evidence_start_line
            )
        );

        CREATE INDEX IF NOT EXISTS idx_code_relations_from
            ON knowledge_code_relations(snapshot_id, from_symbol_key, relation_type);
        CREATE INDEX IF NOT EXISTS idx_code_relations_to
            ON knowledge_code_relations(snapshot_id, to_symbol_key, relation_type);
        ",
    )?;
    set_version(conn, 28)?;
    Ok(())
}

/// v28 -> v29: AI Provider 独立 Embedding 模型配置
fn migrate_v28_to_v29(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v28 -> v29（AI Provider Embedding 模型）");
    add_column_if_missing(
        conn,
        "ai_providers",
        "embedding_model",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    set_version(conn, 29)?;
    Ok(())
}

/// v29 -> v30: 源码分析来源的独立安全与范围设置。
fn migrate_v29_to_v30(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v29 -> v30（源码知识来源设置）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_code_source_settings (
            source_id                   INTEGER PRIMARY KEY,
            include_untracked           INTEGER NOT NULL DEFAULT 0,
            max_file_size_bytes         INTEGER NOT NULL DEFAULT 1048576,
            allowed_languages_json      TEXT NOT NULL DEFAULT '[]',
            allow_remote_processing     INTEGER NOT NULL DEFAULT 0,
            created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE INDEX IF NOT EXISTS idx_code_source_settings_remote
            ON knowledge_code_source_settings(allow_remote_processing);
        ",
    )?;
    set_version(conn, 30)?;
    Ok(())
}

/// v30 -> v31: 工作树快照仅保存状态与内容哈希，不把未提交内容误标为发布事实。
fn migrate_v30_to_v31(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v30 -> v31（工作树快照状态）");
    add_column_if_missing(
        conn,
        "knowledge_code_snapshots",
        "dirty_state_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    set_version(conn, 31)?;
    Ok(())
}

/// v31 -> v32: 通用关系必须携带可验证的项目/版本/来源归属，防止跨项目或历史版本扩展。
fn migrate_v31_to_v32(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v31 -> v32（知识关系归属与可见性）");
    // 早期 v32 实现会替换并删除原表。一旦中途失败，user_version 仍为 31，但表可能
    // 已经处于半迁移状态。这里通过保留 legacy 表、显式事务和幂等建表来保证重试安全。
    let has_scoped_columns = table_has_column(conn, "knowledge_relations", "scope_status")?;
    let tx = conn.unchecked_transaction()?;
    if !has_scoped_columns {
        tx.execute_batch(
            "
        CREATE TABLE IF NOT EXISTS knowledge_relations_scoped_v32 (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id          INTEGER NOT NULL DEFAULT 0,
            release_id          INTEGER NOT NULL DEFAULT 0,
            document_version_id INTEGER NOT NULL DEFAULT 0,
            snapshot_id         INTEGER NOT NULL DEFAULT 0,
            sensitivity         TEXT NOT NULL DEFAULT 'restricted',
            scope_status        TEXT NOT NULL DEFAULT 'scoped',
            from_type           TEXT NOT NULL,
            from_key            TEXT NOT NULL,
            relation_type       TEXT NOT NULL,
            to_type             TEXT NOT NULL,
            to_key              TEXT NOT NULL,
            evidence_json       TEXT NOT NULL DEFAULT '{}',
            confidence          REAL NOT NULL DEFAULT 1.0,
            confirmed           INTEGER NOT NULL DEFAULT 1,
            source              TEXT NOT NULL DEFAULT 'user',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL,
            UNIQUE(project_id, release_id, document_version_id, snapshot_id,
                   from_type, from_key, relation_type, to_type, to_key, source)
        );

        INSERT OR IGNORE INTO knowledge_relations_scoped_v32
            (id, from_type, from_key, relation_type, to_type, to_key, evidence_json,
             confidence, confirmed, source, scope_status, created_at, updated_at, deleted_at)
        SELECT id, from_type, from_key, relation_type, to_type, to_key, evidence_json,
            confidence, confirmed, source, 'needs_rebuild', created_at, updated_at, deleted_at
        FROM knowledge_relations;

        ALTER TABLE knowledge_relations RENAME TO knowledge_relations_legacy_v31;
        ALTER TABLE knowledge_relations_scoped_v32 RENAME TO knowledge_relations;
        ",
        )?;
    }
    tx.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_knowledge_relations_from
            ON knowledge_relations(project_id, release_id, from_type, from_key, relation_type, deleted_at);
        CREATE INDEX IF NOT EXISTS idx_knowledge_relations_to
            ON knowledge_relations(project_id, release_id, to_type, to_key, relation_type, deleted_at);
        CREATE INDEX IF NOT EXISTS idx_knowledge_relations_visibility
            ON knowledge_relations(project_id, release_id, sensitivity, confirmed, deleted_at);
        ",
    )?;
    tx.pragma_update(None, "user_version", 32)?;
    tx.commit()?;
    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}

/// v32 -> v33: 固定检索评测的可比较运行指标。仅保留脱敏汇总与命中文档 ID，正文仍是
/// 原始知识表的职责，避免评测记录复制敏感内容。
fn migrate_v32_to_v33(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v32 -> v33（知识检索评测）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_retrieval_evaluation_runs (
            id                   INTEGER PRIMARY KEY AUTOINCREMENT,
            fixture_version      TEXT NOT NULL,
            profile_id           INTEGER DEFAULT NULL,
            top_k                INTEGER NOT NULL,
            case_count           INTEGER NOT NULL,
            recall_at_k          REAL NOT NULL,
            mrr                  REAL NOT NULL,
            citation_accuracy    REAL NOT NULL,
            version_leakage_rate REAL NOT NULL,
            refusal_accuracy     REAL NOT NULL,
            p50_latency_ms       INTEGER NOT NULL,
            p95_latency_ms       INTEGER NOT NULL,
            details_json         TEXT NOT NULL DEFAULT '[]',
            created_at           TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_evaluation_runs_created
            ON knowledge_retrieval_evaluation_runs(created_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_knowledge_evaluation_runs_profile
            ON knowledge_retrieval_evaluation_runs(profile_id, created_at DESC);
        ",
    )?;
    set_version(conn, 33)?;
    Ok(())
}

/// v33 -> v34: 持久化相邻源码快照的增量变更，尤其是 Git 确认的重命名证据。该表
/// 不保存源码正文，只保存路径、哈希和结构化 Diff 证据，供影响分析与增量索引审计使用。
fn migrate_v33_to_v34(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v33 -> v34（源码快照增量变更）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_code_snapshot_changes (
            id                      INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id             INTEGER NOT NULL,
            previous_snapshot_id    INTEGER DEFAULT NULL,
            change_type             TEXT NOT NULL,
            from_path               TEXT NOT NULL DEFAULT '',
            to_path                 TEXT NOT NULL DEFAULT '',
            content_hash            TEXT NOT NULL DEFAULT '',
            evidence_json           TEXT NOT NULL DEFAULT '{}',
            created_at              TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(snapshot_id, change_type, from_path, to_path)
        );

        CREATE INDEX IF NOT EXISTS idx_code_snapshot_changes_snapshot
            ON knowledge_code_snapshot_changes(snapshot_id, change_type, id);
        CREATE INDEX IF NOT EXISTS idx_code_snapshot_changes_previous
            ON knowledge_code_snapshot_changes(previous_snapshot_id, change_type, id);
        ",
    )?;
    set_version(conn, 34)?;
    Ok(())
}

/// v34 -> v35: 内网禅道允许在逐连接确认风险后使用 HTTP。该标记不是全局开关，
/// 也不保存任何凭据内容；历史连接继续保持默认 HTTPS-only。
fn migrate_v34_to_v35(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v34 -> v35（禅道内网 HTTP 显式授权）");
    // 历史恢复/最小夹具可能只保留与当前迁移目标无关的表；缺失禅道表时不应让
    // 关系表恢复失败。正常 v34 数据库必定拥有该表，仍会执行增量补列。
    if table_exists(conn, "zentao_connections")? {
        add_column_if_missing(
            conn,
            "zentao_connections",
            "allow_insecure_http",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    set_version(conn, 35)?;
    Ok(())
}

/// v35 -> v36: 一个知识项目可以关联多个已登记 Git 工作区。旧的单值标识迁移为
/// 单元素列表；保留原字段，供旧客户端和依赖单工作区的历史流程继续读取。
fn migrate_v35_to_v36(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v35 -> v36（知识项目多 Git 工作区）");
    if !table_exists(conn, "knowledge_projects")? {
        set_version(conn, 36)?;
        return Ok(());
    }
    add_column_if_missing(
        conn,
        "knowledge_projects",
        "git_workspace_keys_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;

    let legacy_projects = {
        let mut statement = conn.prepare(
            "SELECT id, git_workspace_key FROM knowledge_projects
             WHERE trim(git_workspace_key) <> ''
               AND git_workspace_keys_json = '[]'",
        )?;
        let projects = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        projects
    };
    for (id, workspace_key) in legacy_projects {
        let workspace_keys = serde_json::to_string(&vec![workspace_key.trim().to_string()])?;
        conn.execute(
            "UPDATE knowledge_projects SET git_workspace_keys_json = ?1 WHERE id = ?2",
            rusqlite::params![workspace_keys, id],
        )?;
    }
    set_version(conn, 36)?;
    Ok(())
}

/// v36 -> v37: 为项目多仓库清单、版本化文档、受控资产、分析草稿、图谱投影和评测
/// 补齐独立表族。所有新表均是前向追加，旧项目/发布版本/文档/索引保持原样可读。
fn migrate_v36_to_v37(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v36 -> v37（知识平台领域扩展）");
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_project_repository_bindings (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id          INTEGER NOT NULL,
            workspace_key       TEXT NOT NULL,
            alias               TEXT NOT NULL DEFAULT '',
            repository_role     TEXT NOT NULL DEFAULT 'service',
            default_branch      TEXT NOT NULL DEFAULT '',
            version_strategy    TEXT NOT NULL DEFAULT 'manual',
            enabled             INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL,
            UNIQUE(project_id, workspace_key)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_repository_bindings_project
            ON knowledge_project_repository_bindings(project_id, enabled, deleted_at);

        CREATE TABLE IF NOT EXISTS knowledge_release_repository_manifests (
            id                      INTEGER PRIMARY KEY AUTOINCREMENT,
            release_id              INTEGER NOT NULL,
            repository_binding_id   INTEGER NOT NULL,
            requested_ref_type      TEXT NOT NULL,
            requested_ref_name      TEXT NOT NULL,
            resolved_commit_sha     TEXT NOT NULL DEFAULT '',
            capture_kind            TEXT NOT NULL DEFAULT 'git',
            inclusion_status        TEXT NOT NULL DEFAULT 'pending',
            exclusion_reason        TEXT NOT NULL DEFAULT '',
            worktree_dirty          INTEGER NOT NULL DEFAULT 0,
            captured_at             TEXT DEFAULT NULL,
            created_at              TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at              TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(release_id, repository_binding_id)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_release_manifest_release
            ON knowledge_release_repository_manifests(release_id, inclusion_status);
        CREATE INDEX IF NOT EXISTS idx_knowledge_release_manifest_commit
            ON knowledge_release_repository_manifests(repository_binding_id, resolved_commit_sha);

        CREATE TABLE IF NOT EXISTS knowledge_assets (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            asset_key           TEXT NOT NULL UNIQUE,
            content_hash        TEXT NOT NULL UNIQUE,
            storage_key         TEXT NOT NULL UNIQUE,
            original_name       TEXT NOT NULL,
            normalized_name     TEXT NOT NULL,
            mime_type           TEXT NOT NULL,
            size_bytes          INTEGER NOT NULL CHECK(size_bytes >= 0),
            reference_count     INTEGER NOT NULL DEFAULT 0 CHECK(reference_count >= 0),
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_assets_reference_count
            ON knowledge_assets(reference_count, deleted_at);

        CREATE TABLE IF NOT EXISTS knowledge_document_drafts (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id         INTEGER DEFAULT NULL,
            project_id          INTEGER NOT NULL,
            title               TEXT NOT NULL,
            content             TEXT NOT NULL,
            doc_type            TEXT NOT NULL DEFAULT 'markdown',
            base_version_id     INTEGER DEFAULT NULL,
            revision            INTEGER NOT NULL DEFAULT 1,
            editor_label        TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_document_drafts_project
            ON knowledge_document_drafts(project_id, document_id, deleted_at);

        CREATE TABLE IF NOT EXISTS knowledge_document_version_bindings (
            id                      INTEGER PRIMARY KEY AUTOINCREMENT,
            document_version_id     INTEGER NOT NULL,
            release_id              INTEGER DEFAULT NULL,
            repository_binding_id   INTEGER DEFAULT NULL,
            cross_version_scope     TEXT NOT NULL DEFAULT '',
            created_at              TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(document_version_id, release_id, repository_binding_id, cross_version_scope)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_document_version_bindings_scope
            ON knowledge_document_version_bindings(release_id, repository_binding_id, document_version_id);

        CREATE TABLE IF NOT EXISTS knowledge_document_parse_artifacts (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            document_version_id INTEGER NOT NULL,
            asset_id            INTEGER DEFAULT NULL,
            parser_id           TEXT NOT NULL,
            parser_version      TEXT NOT NULL,
            quality_level       TEXT NOT NULL,
            warning_json        TEXT NOT NULL DEFAULT '[]',
            normalized_hash     TEXT NOT NULL,
            structure_json      TEXT NOT NULL DEFAULT '[]',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(document_version_id, parser_id, parser_version, normalized_hash)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_parse_artifacts_document
            ON knowledge_document_parse_artifacts(document_version_id, quality_level);

        CREATE TABLE IF NOT EXISTS knowledge_analysis_runs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            run_key             TEXT NOT NULL UNIQUE,
            project_id          INTEGER NOT NULL,
            release_id          INTEGER NOT NULL,
            manifest_hash       TEXT NOT NULL,
            analyzer_version    TEXT NOT NULL,
            include_rules_json  TEXT NOT NULL DEFAULT '[]',
            exclude_rules_json  TEXT NOT NULL DEFAULT '[]',
            snapshot_ids_json   TEXT NOT NULL DEFAULT '[]',
            evidence_hash       TEXT NOT NULL,
            status              TEXT NOT NULL DEFAULT 'queued',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            finished_at         TEXT DEFAULT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_analysis_runs_scope
            ON knowledge_analysis_runs(project_id, release_id, status, created_at);

        CREATE TABLE IF NOT EXISTS knowledge_analysis_drafts (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            analysis_run_id     INTEGER NOT NULL,
            provider_key        TEXT NOT NULL DEFAULT '',
            model               TEXT NOT NULL DEFAULT '',
            template_key        TEXT NOT NULL DEFAULT '',
            content             TEXT NOT NULL,
            claim_refs_json     TEXT NOT NULL DEFAULT '[]',
            status              TEXT NOT NULL DEFAULT 'draft',
            confirmed_version_id INTEGER DEFAULT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(analysis_run_id, template_key)
        );

        CREATE TABLE IF NOT EXISTS knowledge_graph_builds (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            build_key           TEXT NOT NULL UNIQUE,
            project_id          INTEGER NOT NULL,
            release_id          INTEGER NOT NULL,
            projection_version  TEXT NOT NULL,
            source_hash         TEXT NOT NULL,
            status              TEXT NOT NULL DEFAULT 'building',
            is_active           INTEGER NOT NULL DEFAULT 0,
            checkpoint_json     TEXT NOT NULL DEFAULT '{}',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            finished_at         TEXT DEFAULT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_graph_builds_active
            ON knowledge_graph_builds(project_id, release_id) WHERE is_active = 1;

        CREATE TABLE IF NOT EXISTS knowledge_graph_nodes (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            graph_build_id      INTEGER NOT NULL,
            entity_type         TEXT NOT NULL,
            entity_key          TEXT NOT NULL,
            label               TEXT NOT NULL,
            metadata_hash       TEXT NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(graph_build_id, entity_type, entity_key)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_graph_nodes_entity
            ON knowledge_graph_nodes(graph_build_id, entity_key);

        CREATE TABLE IF NOT EXISTS knowledge_graph_edges (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            graph_build_id      INTEGER NOT NULL,
            from_node_id        INTEGER NOT NULL,
            relation_type       TEXT NOT NULL,
            to_node_id          INTEGER NOT NULL,
            evidence_ref        TEXT NOT NULL,
            confidence          REAL NOT NULL DEFAULT 1.0,
            confirmed           INTEGER NOT NULL DEFAULT 1,
            source_relation_ref TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(graph_build_id, from_node_id, relation_type, to_node_id, evidence_ref)
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_graph_edges_from
            ON knowledge_graph_edges(graph_build_id, from_node_id, confirmed);
        CREATE INDEX IF NOT EXISTS idx_knowledge_graph_edges_to
            ON knowledge_graph_edges(graph_build_id, to_node_id, confirmed);

        CREATE TABLE IF NOT EXISTS knowledge_document_title_index (
            document_id         INTEGER PRIMARY KEY,
            normalized_title    TEXT NOT NULL,
            current_version_id  INTEGER NOT NULL,
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_document_title_index_title
            ON knowledge_document_title_index(normalized_title);

        CREATE TABLE IF NOT EXISTS knowledge_backfill_runs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            backfill_type       TEXT NOT NULL,
            checkpoint_json     TEXT NOT NULL DEFAULT '{}',
            status              TEXT NOT NULL DEFAULT 'queued',
            processed_count     INTEGER NOT NULL DEFAULT 0,
            failed_count        INTEGER NOT NULL DEFAULT 0,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE TABLE IF NOT EXISTS knowledge_document_coverage_snapshots (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id          INTEGER NOT NULL,
            release_id          INTEGER DEFAULT NULL,
            repository_binding_id INTEGER DEFAULT NULL,
            document_type       TEXT NOT NULL DEFAULT '',
            metrics_json        TEXT NOT NULL DEFAULT '{}',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_coverage_scope
            ON knowledge_document_coverage_snapshots(project_id, release_id, created_at);

        CREATE TABLE IF NOT EXISTS knowledge_feature_flags (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            feature_key         TEXT NOT NULL,
            project_id          INTEGER DEFAULT NULL,
            enabled             INTEGER NOT NULL DEFAULT 0,
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(feature_key, project_id)
        );
        ",
    )?;
    tx.pragma_update(None, "user_version", 37)?;
    tx.commit()?;
    Ok(())
}

/// v37 -> v38: 为人工和生成文档的正式提交补齐不可变谱系与索引任务关联。
/// 历史版本保持原值；新字段只记录提交事实，禁止事后改写版本正文或出处。
fn migrate_v37_to_v38(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v37 -> v38（文档不可变提交）");
    let tx = conn.unchecked_transaction()?;
    // 个别早期数据库只保留了基础配置表。它们进入 v38 时仍需可打开，但不能凭空创建
    // 旧知识表；存在该表时使用可重试的补列，兼容中断后已写入部分列的数据库副本。
    if table_exists(&tx, "knowledge_document_versions")? {
        add_column_if_missing(
            &tx,
            "knowledge_document_versions",
            "parent_version_id",
            "INTEGER DEFAULT NULL",
        )?;
        add_column_if_missing(
            &tx,
            "knowledge_document_versions",
            "author_label",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &tx,
            "knowledge_document_versions",
            "commit_message",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        add_column_if_missing(
            &tx,
            "knowledge_document_versions",
            "index_job_id",
            "INTEGER DEFAULT NULL",
        )?;
        tx.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_knowledge_document_versions_parent
                ON knowledge_document_versions(document_id, parent_version_id);
            CREATE INDEX IF NOT EXISTS idx_knowledge_document_versions_index_job
                ON knowledge_document_versions(index_job_id);

            DROP TRIGGER IF EXISTS prevent_knowledge_document_version_update;
            CREATE TRIGGER prevent_knowledge_document_version_update
            BEFORE UPDATE OF document_id, release_id, version_label, git_branch, commit_sha,
                             source_path, mime_type, content, content_hash, parent_version_id,
                             author_label, commit_message ON knowledge_document_versions
            WHEN NOT (NEW.valid = 0 AND NEW.content = '' AND NEW.content_hash = OLD.content_hash)
            BEGIN
                SELECT RAISE(ABORT, '知识文档版本不可修改，请创建新版本');
            END;
            ",
        )?;
    }
    tx.pragma_update(None, "user_version", 38)?;
    tx.commit()?;
    Ok(())
}

/// v38 -> v39: 上传文件在解析前建立受管资产、逻辑文档和导入任务之间的可审计关联。
fn migrate_v38_to_v39(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v38 -> v39（文档上传导入任务）");
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_document_uploads (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id         INTEGER NOT NULL UNIQUE,
            asset_id            INTEGER NOT NULL,
            release_id          INTEGER DEFAULT NULL,
            import_job_id       INTEGER NOT NULL UNIQUE,
            original_name       TEXT NOT NULL,
            mime_type           TEXT NOT NULL,
            status              TEXT NOT NULL DEFAULT 'queued',
            error_message       TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );
        CREATE INDEX IF NOT EXISTS idx_knowledge_document_uploads_asset
            ON knowledge_document_uploads(asset_id, status);
        CREATE INDEX IF NOT EXISTS idx_knowledge_document_uploads_release
            ON knowledge_document_uploads(release_id, status);
        ",
    )?;
    tx.pragma_update(None, "user_version", 39)?;
    tx.commit()?;
    Ok(())
}

/// v39 -> v40: 图片远程 OCR 的用户授权和 Provider 引用必须在上传排队时冻结，后台
/// 导入任务不能读取前端临时状态，也绝不能持久化任何 Provider 密钥。
fn migrate_v39_to_v40(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v39 -> v40（图片远程 OCR 授权）");
    // 一些旧库回放测试会故意仅回退 user_version，而不回退已存在的表结构；迁移必须
    // 与已有 add_column_if_missing 约定一致，避免“列已存在”阻塞升级或恢复。
    add_column_if_missing(
        conn,
        "knowledge_document_uploads",
        "allow_remote_ocr",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "knowledge_document_uploads",
        "ocr_provider_key",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    set_version(conn, 40)?;
    Ok(())
}

/// v40 -> v41: 上传任务在排队时冻结跨版本范围，解析完成后能与正式版本在同一事务
/// 写入绑定表，避免附件因异步处理而丢失其用户明确选择的范围。
fn migrate_v40_to_v41(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v40 -> v41（上传文档版本范围）");
    add_column_if_missing(
        conn,
        "knowledge_document_uploads",
        "cross_version_scope",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    set_version(conn, 41)?;
    Ok(())
}

/// v41 -> v42: SQLite 的普通 UNIQUE 约束会把 NULL 视为彼此不同，导致同一文档版本的
/// 无发布/无仓库范围可以重复写入。迁移先保留最早记录，再以 NULL 归一化的表达式索引
/// 固化幂等边界，防止后台重试持续制造重复绑定。
fn migrate_v41_to_v42(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v41 -> v42（文档版本范围唯一性）");
    // 个别早期/中断迁移库会保留版本号但没有完整知识表族；延续既有迁移的容错约定，
    // 不凭空创建孤立绑定表，只在该表存在时修复其 NULL 唯一性。
    if !table_exists(conn, "knowledge_document_version_bindings")? {
        set_version(conn, 42)?;
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM knowledge_document_version_bindings
         WHERE id NOT IN (
            SELECT MIN(id)
            FROM knowledge_document_version_bindings
            GROUP BY document_version_id, ifnull(release_id, -1),
                     ifnull(repository_binding_id, -1), cross_version_scope
         )",
        [],
    )?;
    tx.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_document_version_binding_scope
           ON knowledge_document_version_bindings(
                document_version_id,
                ifnull(release_id, -1),
                ifnull(repository_binding_id, -1),
                cross_version_scope
           );",
    )?;
    tx.pragma_update(None, "user_version", 42)?;
    tx.commit()?;
    Ok(())
}

/// v42 -> v43: v42 使用的 -1 空值哨兵会把 NULL 与手工写入的负数 ID 归为同一范围。
/// 改为按四种 NULL 组合分别建部分唯一索引，既保留 NULL 的唯一性，也不改变任意合法
/// 整数范围的语义。再次去重使异常中断或人工移除 v42 索引后的库能够安全恢复。
fn migrate_v42_to_v43(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v42 -> v43（文档版本范围精确唯一性）");
    if !table_exists(conn, "knowledge_document_version_bindings")? {
        set_version(conn, 43)?;
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM knowledge_document_version_bindings
         WHERE id NOT IN (
            SELECT MIN(id)
            FROM knowledge_document_version_bindings
            GROUP BY document_version_id, release_id, repository_binding_id, cross_version_scope
         )",
        [],
    )?;
    tx.execute_batch(
        "
        DROP INDEX IF EXISTS ux_knowledge_document_version_binding_scope;

        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_document_binding_release_repository
            ON knowledge_document_version_bindings(
                document_version_id, release_id, repository_binding_id, cross_version_scope
            )
            WHERE release_id IS NOT NULL AND repository_binding_id IS NOT NULL;
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_document_binding_release_only
            ON knowledge_document_version_bindings(document_version_id, release_id, cross_version_scope)
            WHERE release_id IS NOT NULL AND repository_binding_id IS NULL;
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_document_binding_repository_only
            ON knowledge_document_version_bindings(
                document_version_id, repository_binding_id, cross_version_scope
            )
            WHERE release_id IS NULL AND repository_binding_id IS NOT NULL;
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_document_binding_cross_version_only
            ON knowledge_document_version_bindings(document_version_id, cross_version_scope)
            WHERE release_id IS NULL AND repository_binding_id IS NULL;
        ",
    )?;
    tx.pragma_update(None, "user_version", 43)?;
    tx.commit()?;
    Ok(())
}

/// v43 -> v44: 分析运行必须保存规范化的快照集合。哈希只能证明输入是否变化，无法支撑
/// 人工确认后的证据追溯；该字段不保存源码正文或远程 Provider 的任何秘密。
fn migrate_v43_to_v44(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v43 -> v44（分析快照审计）");
    if table_exists(conn, "knowledge_analysis_runs")? {
        add_column_if_missing(
            conn,
            "knowledge_analysis_runs",
            "snapshot_ids_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
    }
    set_version(conn, 44)?;
    Ok(())
}

/// v44 -> v45: AI 分析草稿确认与正式文档版本之间使用仅内部写入的唯一关联。
/// 不能依赖用户可填写的提交说明恢复中断状态，否则同名手工提交可能被误认成 AI 草稿。
fn migrate_v44_to_v45(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v44 -> v45（分析草稿版本关联）");
    if !table_exists(conn, "knowledge_document_versions")? {
        set_version(conn, 45)?;
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;
    add_column_if_missing(
        &tx,
        "knowledge_document_versions",
        "analysis_draft_id",
        "INTEGER DEFAULT NULL",
    )?;
    tx.execute_batch(
        "
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_document_versions_analysis_draft
            ON knowledge_document_versions(analysis_draft_id)
            WHERE analysis_draft_id IS NOT NULL;

        DROP TRIGGER IF EXISTS prevent_knowledge_document_version_update;
        CREATE TRIGGER prevent_knowledge_document_version_update
        BEFORE UPDATE OF document_id, release_id, version_label, git_branch, commit_sha,
                         source_path, mime_type, content, content_hash, parent_version_id,
                         author_label, commit_message, analysis_draft_id ON knowledge_document_versions
        WHEN NOT (NEW.valid = 0 AND NEW.content = '' AND NEW.content_hash = OLD.content_hash)
        BEGIN
            SELECT RAISE(ABORT, '知识文档版本不可修改，请创建新版本');
        END;
        ",
    )?;
    tx.pragma_update(None, "user_version", 45)?;
    tx.commit()?;
    Ok(())
}

/// v45 -> v46: 项目术语必须与项目隔离，并且仅允许使用已保存的人工确认映射扩展
/// 本地搜索。术语表不保存文档正文，不改变任何现有 FTS 索引，删除采用软删除保留审计。
fn migrate_v45_to_v46(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v45 -> v46（项目术语映射）");
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_project_terms (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            term TEXT NOT NULL,
            normalized_term TEXT NOT NULL,
            aliases_json TEXT NOT NULL DEFAULT '[]',
            confirmation_note TEXT NOT NULL,
            created_by TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at TEXT DEFAULT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_project_terms_active_term
            ON knowledge_project_terms(project_id, normalized_term)
            WHERE deleted_at IS NULL;
        CREATE INDEX IF NOT EXISTS ix_knowledge_project_terms_project_updated
            ON knowledge_project_terms(project_id, updated_at, id)
            WHERE deleted_at IS NULL;
        ",
    )?;
    tx.pragma_update(None, "user_version", 46)?;
    tx.commit()?;
    Ok(())
}

/// v46 -> v47: 文档版本幂等键必须包含来源路径，允许同一文档、发布版本和内容哈希在
/// 不同路径下分别保留版本。SQLite 不能直接修改表级 UNIQUE 约束，因此在单事务中重建
/// 表；显式复制所有字段和 ID，保留历史正文、索引、不可变触发器及自增序列。
fn migrate_v46_to_v47(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v46 -> v47（文档版本来源路径唯一性）");
    if !table_exists(conn, "knowledge_document_versions")? {
        set_version(conn, 47)?;
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;
    let previous_sequence = if table_exists(&tx, "sqlite_sequence")? {
        tx.query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = 'knowledge_document_versions'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    } else {
        None
    };

    tx.execute_batch(
        "
        DROP TRIGGER IF EXISTS prevent_knowledge_document_version_update;
        DROP TABLE IF EXISTS knowledge_document_versions_v47;

        CREATE TABLE knowledge_document_versions_v47 (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id         INTEGER NOT NULL,
            release_id          INTEGER DEFAULT NULL,
            version_label       TEXT NOT NULL DEFAULT '',
            git_branch          TEXT NOT NULL DEFAULT '',
            commit_sha          TEXT NOT NULL DEFAULT '',
            source_path         TEXT NOT NULL DEFAULT '',
            mime_type           TEXT NOT NULL DEFAULT 'text/markdown',
            content             TEXT NOT NULL,
            content_hash        TEXT NOT NULL,
            parsed_meta_json    TEXT NOT NULL DEFAULT '{}',
            token_estimate      INTEGER NOT NULL DEFAULT 0,
            valid               INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            parent_version_id   INTEGER DEFAULT NULL,
            author_label        TEXT NOT NULL DEFAULT '',
            commit_message      TEXT NOT NULL DEFAULT '',
            index_job_id        INTEGER DEFAULT NULL,
            analysis_draft_id   INTEGER DEFAULT NULL,
            UNIQUE(document_id, version_label, content_hash, source_path)
        );

        INSERT INTO knowledge_document_versions_v47 (
            id, document_id, release_id, version_label, git_branch, commit_sha,
            source_path, mime_type, content, content_hash, parsed_meta_json,
            token_estimate, valid, created_at, parent_version_id, author_label,
            commit_message, index_job_id, analysis_draft_id
        )
        SELECT
            id, document_id, release_id, version_label, git_branch, commit_sha,
            source_path, mime_type, content, content_hash, parsed_meta_json,
            token_estimate, valid, created_at, parent_version_id, author_label,
            commit_message, index_job_id, analysis_draft_id
        FROM knowledge_document_versions;

        DROP TABLE knowledge_document_versions;
        ALTER TABLE knowledge_document_versions_v47 RENAME TO knowledge_document_versions;

        CREATE INDEX IF NOT EXISTS idx_knowledge_doc_versions_release
            ON knowledge_document_versions(release_id, document_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_doc_versions_commit
            ON knowledge_document_versions(commit_sha, document_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_document_versions_parent
            ON knowledge_document_versions(document_id, parent_version_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_document_versions_index_job
            ON knowledge_document_versions(index_job_id);
        CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_document_versions_analysis_draft
            ON knowledge_document_versions(analysis_draft_id)
            WHERE analysis_draft_id IS NOT NULL;

        CREATE TRIGGER prevent_knowledge_document_version_update
        BEFORE UPDATE OF document_id, release_id, version_label, git_branch, commit_sha,
                         source_path, mime_type, content, content_hash, parent_version_id,
                         author_label, commit_message, analysis_draft_id ON knowledge_document_versions
        WHEN NOT (NEW.valid = 0 AND NEW.content = '' AND NEW.content_hash = OLD.content_hash)
        BEGIN
            SELECT RAISE(ABORT, '知识文档版本不可修改，请创建新版本');
        END;
        ",
    )?;

    if let Some(previous_sequence) = previous_sequence {
        let updated = tx.execute(
            "UPDATE sqlite_sequence
             SET seq = MAX(seq, ?1)
             WHERE name = 'knowledge_document_versions'",
            [previous_sequence],
        )?;
        if updated == 0 {
            tx.execute(
                "INSERT INTO sqlite_sequence(name, seq)
                 VALUES ('knowledge_document_versions', ?1)",
                [previous_sequence],
            )?;
        }
    }

    tx.pragma_update(None, "user_version", 47)?;
    tx.commit()?;
    Ok(())
}

/// v47 -> v48: 上传任务显式保存来源文件夹名称。旧记录从仍受控的逻辑路径回填，避免
/// 前端用路径哨兵推断文档类型，也让普通文档可以安全使用同名逻辑路径。
fn migrate_v47_to_v48(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v47 -> v48（上传来源文件夹）");
    if !table_exists(conn, "knowledge_document_uploads")? {
        set_version(conn, 48)?;
        return Ok(());
    }
    let has_document_logical_path = table_exists(conn, "knowledge_documents")?
        && conn
            .prepare("PRAGMA table_info(knowledge_documents)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|column| column == "logical_path");

    let tx = conn.unchecked_transaction()?;
    add_column_if_missing(
        &tx,
        "knowledge_document_uploads",
        "source_folder_name",
        "TEXT DEFAULT NULL",
    )?;
    if has_document_logical_path {
        tx.execute(
            "UPDATE knowledge_document_uploads
             SET source_folder_name = (
                 SELECT CASE
                     WHEN document.logical_path LIKE 'upload-folder/%/%' THEN
                         substr(
                             substr(document.logical_path, length('upload-folder/') + 1),
                             1,
                             instr(substr(document.logical_path, length('upload-folder/') + 1), '/') - 1
                         )
                     ELSE NULL
                 END
                 FROM knowledge_documents document
                 WHERE document.id = knowledge_document_uploads.document_id
             )
             WHERE source_folder_name IS NULL
               AND EXISTS (
                   SELECT 1 FROM knowledge_documents document
                   WHERE document.id = knowledge_document_uploads.document_id
                     AND document.logical_path LIKE 'upload-folder/%/%'
               )",
            [],
        )?;
    }
    tx.pragma_update(None, "user_version", 48)?;
    tx.commit()?;
    Ok(())
}

/// v48 -> v49: 项目问答从页面临时状态升级为本地会话。会话绑定项目、不可变版本和
/// 聊天模型；每轮用户与助手消息在同一事务中保存，助手消息保留完整引用结果。
fn migrate_v48_to_v49(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v48 -> v49（项目问答会话）");
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS knowledge_qa_sessions (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id          INTEGER NOT NULL,
            project_version_id  INTEGER NOT NULL,
            release_commit_sha  TEXT NOT NULL DEFAULT '',
            provider_key        TEXT NOT NULL DEFAULT '',
            model               TEXT NOT NULL DEFAULT '',
            title               TEXT NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );
        CREATE INDEX IF NOT EXISTS ix_knowledge_qa_sessions_project_updated
            ON knowledge_qa_sessions(project_id, updated_at DESC, id DESC)
            WHERE deleted_at IS NULL;

        CREATE TABLE IF NOT EXISTS knowledge_qa_messages (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id      INTEGER NOT NULL,
            sequence_no     INTEGER NOT NULL,
            role            TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
            content         TEXT NOT NULL,
            evidence_only   INTEGER NOT NULL DEFAULT 0,
            answer_json     TEXT DEFAULT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(session_id, sequence_no),
            FOREIGN KEY(session_id) REFERENCES knowledge_qa_sessions(id)
        );
        CREATE INDEX IF NOT EXISTS ix_knowledge_qa_messages_session_sequence
            ON knowledge_qa_messages(session_id, sequence_no, id);
        ",
    )?;
    tx.pragma_update(None, "user_version", 49)?;
    tx.commit()?;
    Ok(())
}

/// v49 -> v50: 会话同时固化版本提交 SHA。项目版本记录若被纠正为新提交，旧回答仍可
/// 查看，但不能继续追加到新的证据范围中。`add_column_if_missing` 兼容开发期 v49 库。
fn migrate_v49_to_v50(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v49 -> v50（问答会话版本提交）");
    let tx = conn.unchecked_transaction()?;
    add_column_if_missing(
        &tx,
        "knowledge_qa_sessions",
        "release_commit_sha",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    if table_exists(&tx, "knowledge_releases")? {
        tx.execute(
            "UPDATE knowledge_qa_sessions
             SET release_commit_sha = COALESCE((
                 SELECT release.commit_sha
                 FROM knowledge_releases release
                 WHERE release.id = knowledge_qa_sessions.project_version_id
             ), '')
             WHERE release_commit_sha = ''",
            [],
        )?;
    }
    tx.pragma_update(None, "user_version", 50)?;
    tx.commit()?;
    Ok(())
}

/// v50 -> v51: 允许同一向量兼容配置同时存在多个索引代次，以支持蓝绿重建。
/// `fingerprint` 描述向量空间兼容性，不应作为索引实例的唯一标识；实例仍由
/// `profile_key` 和主键区分，活动索引的唯一性继续由部分唯一索引保证。
fn migrate_v50_to_v51(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v50 -> v51（允许同配置蓝绿向量索引）");
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "
        CREATE TABLE knowledge_embedding_profiles_v51 (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_key         TEXT NOT NULL UNIQUE,
            name                TEXT NOT NULL,
            mode                TEXT NOT NULL,
            provider_key        TEXT NOT NULL DEFAULT '',
            model               TEXT NOT NULL,
            model_revision      TEXT NOT NULL DEFAULT '',
            dimension           INTEGER NOT NULL DEFAULT 0,
            normalized          INTEGER NOT NULL DEFAULT 1,
            config_json         TEXT NOT NULL DEFAULT '{}',
            fingerprint         TEXT NOT NULL,
            status              TEXT NOT NULL DEFAULT 'draft',
            is_active           INTEGER NOT NULL DEFAULT 0,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        INSERT INTO knowledge_embedding_profiles_v51
            (id, profile_key, name, mode, provider_key, model, model_revision, dimension,
             normalized, config_json, fingerprint, status, is_active, created_at, updated_at)
        SELECT id, profile_key, name, mode, provider_key, model, model_revision, dimension,
               normalized, config_json, fingerprint, status, is_active, created_at, updated_at
        FROM knowledge_embedding_profiles;

        DROP TABLE knowledge_embedding_profiles;
        ALTER TABLE knowledge_embedding_profiles_v51 RENAME TO knowledge_embedding_profiles;

        CREATE INDEX idx_knowledge_embedding_profiles_active
            ON knowledge_embedding_profiles(is_active, status);
        CREATE UNIQUE INDEX ux_knowledge_embedding_profiles_one_active
            ON knowledge_embedding_profiles(is_active)
            WHERE is_active = 1;
        ",
    )?;
    tx.pragma_update(None, "user_version", 51)?;
    tx.commit()?;
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// v21 -> v22: Jenkins 构建失败 AI 分析记录
fn migrate_v21_to_v22(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v21 -> v22（Jenkins 构建失败 AI 分析记录）");
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS jenkins_build_analyses (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            analysis_key        TEXT NOT NULL UNIQUE,
            run_key             TEXT NOT NULL DEFAULT '',
            request_id          TEXT NOT NULL DEFAULT '',
            connection_key      TEXT NOT NULL,
            job_full_name       TEXT NOT NULL,
            build_number        INTEGER NOT NULL,
            provider_key        TEXT NOT NULL DEFAULT '',
            provider_name       TEXT NOT NULL DEFAULT '',
            model               TEXT NOT NULL DEFAULT '',
            summary_markdown    TEXT NOT NULL,
            snippet_sha256      TEXT NOT NULL DEFAULT '',
            snippet_start_line  INTEGER NOT NULL DEFAULT 0,
            snippet_end_line    INTEGER NOT NULL DEFAULT 0,
            matched_lines       INTEGER NOT NULL DEFAULT 0,
            created_by          TEXT NOT NULL DEFAULT 'local-user',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE INDEX IF NOT EXISTS idx_jenkins_build_analyses_build
            ON jenkins_build_analyses(connection_key, job_full_name, build_number, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_jenkins_build_analyses_run
            ON jenkins_build_analyses(run_key, created_at DESC);
        ",
    )?;
    set_version(conn, 22)?;
    Ok(())
}

/// v20 -> v21: Jenkins 成功构建通知开关
fn migrate_v20_to_v21(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v20 -> v21（Jenkins 成功构建通知开关）");
    add_column_if_missing(
        conn,
        "jenkins_connections",
        "notify_on_success",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    set_version(conn, 21)?;
    Ok(())
}

/// v18 -> v19: 代码审核与分支合并任务
fn migrate_v18_to_v19(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v18 -> v19（代码审核与分支合并）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS code_review_batches (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_key           TEXT NOT NULL UNIQUE,
            raw_text            TEXT NOT NULL,
            parsed_json         TEXT NOT NULL DEFAULT '{}',
            status              TEXT NOT NULL DEFAULT 'parsed',
            total_count         INTEGER NOT NULL DEFAULT 0,
            success_count       INTEGER NOT NULL DEFAULT 0,
            failed_count        INTEGER NOT NULL DEFAULT 0,
            created_by          TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE TABLE IF NOT EXISTS code_review_tasks (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            task_key            TEXT NOT NULL UNIQUE,
            workspace_key       TEXT NOT NULL,
            workspace_name      TEXT NOT NULL,
            repo_path           TEXT NOT NULL,
            source_branch       TEXT NOT NULL,
            target_branch       TEXT NOT NULL,
            status              TEXT NOT NULL DEFAULT 'draft',
            risk_level          TEXT NOT NULL DEFAULT 'unknown',
            merge_base          TEXT NOT NULL DEFAULT '',
            source_head         TEXT NOT NULL DEFAULT '',
            target_head         TEXT NOT NULL DEFAULT '',
            push_status         TEXT NOT NULL DEFAULT 'not_requested',
            diff_stat_json      TEXT NOT NULL DEFAULT '{}',
            changed_files_json  TEXT NOT NULL DEFAULT '[]',
            commit_list_json    TEXT NOT NULL DEFAULT '[]',
            diff_excerpt_json   TEXT NOT NULL DEFAULT '[]',
            ai_provider         TEXT NOT NULL DEFAULT '',
            ai_model            TEXT NOT NULL DEFAULT '',
            ai_review_markdown  TEXT NOT NULL DEFAULT '',
            ai_review_json      TEXT NOT NULL DEFAULT '{}',
            batch_key           TEXT NOT NULL DEFAULT '',
            created_by          TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            merged_at           TEXT DEFAULT NULL,
            error_message       TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_code_review_tasks_workspace
            ON code_review_tasks(workspace_key, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_code_review_tasks_status
            ON code_review_tasks(status, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_code_review_tasks_batch
            ON code_review_tasks(batch_key);
        ",
    )?;

    set_version(conn, 19)?;
    Ok(())
}

/// v17 -> v18: 安全凭证 Git 工作区
fn migrate_v17_to_v18(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v17 -> v18（Git 工作区）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS git_workspaces (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            workspace_key       TEXT NOT NULL UNIQUE,
            name                TEXT NOT NULL,
            repo_path           TEXT NOT NULL,
            credential_key      TEXT NOT NULL DEFAULT '',
            branch              TEXT NOT NULL DEFAULT '',
            remote_url          TEXT NOT NULL DEFAULT '',
            status              TEXT NOT NULL DEFAULT 'unknown',
            changed_files       INTEGER NOT NULL DEFAULT 0,
            ahead               INTEGER NOT NULL DEFAULT 0,
            behind              INTEGER NOT NULL DEFAULT 0,
            description         TEXT NOT NULL DEFAULT '',
            last_scanned_at     TEXT DEFAULT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_git_workspaces_credential
            ON git_workspaces(credential_key);
        CREATE INDEX IF NOT EXISTS idx_git_workspaces_status
            ON git_workspaces(status);
        CREATE INDEX IF NOT EXISTS idx_git_workspaces_updated
            ON git_workspaces(updated_at DESC);
        ",
    )?;

    set_version(conn, 18)?;
    Ok(())
}

/// v16 -> v17: 服务自动部署基础模型
fn migrate_v16_to_v17(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v16 -> v17（服务自动部署）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS deployment_targets (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            target_key          TEXT NOT NULL UNIQUE,
            name                TEXT NOT NULL,
            server_alias        TEXT NOT NULL DEFAULT '',
            recipe              TEXT NOT NULL,
            source_type         TEXT NOT NULL DEFAULT 'local',
            project_path        TEXT NOT NULL DEFAULT '',
            git_url             TEXT NOT NULL DEFAULT '',
            git_ref             TEXT NOT NULL DEFAULT '',
            git_credential_key  TEXT NOT NULL DEFAULT '',
            docker_build_mode   TEXT NOT NULL DEFAULT 'remote',
            workdir             TEXT NOT NULL DEFAULT '',
            deploy_root         TEXT NOT NULL DEFAULT '',
            domain              TEXT NOT NULL DEFAULT '',
            https_enabled       INTEGER NOT NULL DEFAULT 0,
            port                INTEGER DEFAULT NULL,
            health_check_url    TEXT NOT NULL DEFAULT '',
            config_json         TEXT NOT NULL DEFAULT '{}',
            enabled             INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_deployment_targets_server
            ON deployment_targets(server_alias, enabled);
        CREATE INDEX IF NOT EXISTS idx_deployment_targets_recipe
            ON deployment_targets(recipe);

        CREATE TABLE IF NOT EXISTS deployment_groups (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            group_key           TEXT NOT NULL UNIQUE,
            name                TEXT NOT NULL,
            description         TEXT NOT NULL DEFAULT '',
            enabled             INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );

        CREATE TABLE IF NOT EXISTS deployment_group_targets (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            group_key           TEXT NOT NULL,
            target_key          TEXT NOT NULL,
            sort_order          INTEGER NOT NULL DEFAULT 0,
            enabled             INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            UNIQUE(group_key, target_key)
        );

        CREATE INDEX IF NOT EXISTS idx_deployment_group_targets_group
            ON deployment_group_targets(group_key, sort_order);

        CREATE TABLE IF NOT EXISTS deployment_runs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id              TEXT NOT NULL UNIQUE,
            target_key          TEXT NOT NULL DEFAULT '',
            group_key           TEXT NOT NULL DEFAULT '',
            status              TEXT NOT NULL DEFAULT 'pending',
            version_label       TEXT NOT NULL DEFAULT '',
            summary             TEXT NOT NULL DEFAULT '',
            plan_json           TEXT NOT NULL DEFAULT '{}',
            created_by          TEXT NOT NULL DEFAULT 'local-user',
            started_at          TEXT DEFAULT NULL,
            finished_at         TEXT DEFAULT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE TABLE IF NOT EXISTS deployment_run_steps (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id              TEXT NOT NULL,
            step_key            TEXT NOT NULL,
            title               TEXT NOT NULL,
            status              TEXT NOT NULL DEFAULT 'pending',
            command_preview     TEXT NOT NULL DEFAULT '',
            stdout_preview      TEXT NOT NULL DEFAULT '',
            stderr_preview      TEXT NOT NULL DEFAULT '',
            exit_code           INTEGER DEFAULT NULL,
            approval_id         INTEGER DEFAULT NULL,
            started_at          TEXT DEFAULT NULL,
            finished_at         TEXT DEFAULT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE INDEX IF NOT EXISTS idx_deployment_run_steps_run
            ON deployment_run_steps(run_id, id);
        ",
    )?;

    set_version(conn, 17)?;
    Ok(())
}

/// v15 -> v16: AI/MCP 安全凭证代理
fn migrate_v15_to_v16(conn: &Connection) -> Result<(), AppError> {
    log::info!("数据库迁移: v15 -> v16（安全凭证）");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS secure_credentials (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            credential_key      TEXT NOT NULL UNIQUE,
            display_name        TEXT NOT NULL,
            provider            TEXT NOT NULL,
            credential_type     TEXT NOT NULL,
            account_name        TEXT NOT NULL DEFAULT '',
            base_url            TEXT NOT NULL DEFAULT '',
            scope_json          TEXT NOT NULL DEFAULT '[]',
            tags_json           TEXT NOT NULL DEFAULT '[]',
            folder              TEXT NOT NULL DEFAULT '',
            description         TEXT NOT NULL DEFAULT '',
            status              TEXT NOT NULL DEFAULT 'active',
            enabled             INTEGER NOT NULL DEFAULT 1,
            allow_mcp           INTEGER NOT NULL DEFAULT 0,
            approval_policy     TEXT NOT NULL DEFAULT 'write_requires_approval',
            expires_at          TEXT DEFAULT NULL,
            last_used_at        TEXT DEFAULT NULL,
            usage_count         INTEGER NOT NULL DEFAULT 0,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            deleted_at          TEXT DEFAULT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_secure_credentials_provider
            ON secure_credentials(provider);
        CREATE INDEX IF NOT EXISTS idx_secure_credentials_status
            ON secure_credentials(status);
        CREATE INDEX IF NOT EXISTS idx_secure_credentials_allow_mcp
            ON secure_credentials(allow_mcp, enabled);

        CREATE TABLE IF NOT EXISTS secure_credential_secrets (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            credential_key      TEXT NOT NULL,
            secret_version      INTEGER NOT NULL DEFAULT 1,
            secret_nonce        TEXT NOT NULL,
            secret_ciphertext   TEXT NOT NULL,
            secret_hint         TEXT NOT NULL DEFAULT '',
            active              INTEGER NOT NULL DEFAULT 1,
            rotated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            FOREIGN KEY (credential_key) REFERENCES secure_credentials(credential_key)
        );

        CREATE INDEX IF NOT EXISTS idx_secure_credential_secrets_key
            ON secure_credential_secrets(credential_key, active);

        CREATE TABLE IF NOT EXISTS secure_credential_sessions (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id          TEXT NOT NULL UNIQUE,
            credential_key      TEXT NOT NULL,
            provider            TEXT NOT NULL,
            caller              TEXT NOT NULL DEFAULT 'local-user',
            scope_json          TEXT NOT NULL DEFAULT '[]',
            status              TEXT NOT NULL DEFAULT 'active',
            expires_at          TEXT NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            revoked_at          TEXT DEFAULT NULL,
            last_used_at        TEXT DEFAULT NULL,
            call_count          INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (credential_key) REFERENCES secure_credentials(credential_key)
        );

        CREATE INDEX IF NOT EXISTS idx_secure_credential_sessions_status
            ON secure_credential_sessions(status, expires_at);

        CREATE TABLE IF NOT EXISTS secure_credential_policies (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            policy_key          TEXT NOT NULL UNIQUE,
            policy_json         TEXT NOT NULL,
            enabled             INTEGER NOT NULL DEFAULT 1,
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE TABLE IF NOT EXISTS secure_credential_audit_logs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            actor               TEXT NOT NULL DEFAULT 'local-user',
            source              TEXT NOT NULL DEFAULT 'secure_credential',
            provider            TEXT NOT NULL DEFAULT '',
            credential_key      TEXT NOT NULL DEFAULT '',
            action              TEXT NOT NULL,
            risk                TEXT NOT NULL DEFAULT 'readonly',
            result              TEXT NOT NULL,
            duration_ms         INTEGER NOT NULL DEFAULT 0,
            request_id          TEXT NOT NULL DEFAULT '',
            approval_id         INTEGER DEFAULT NULL,
            detail_json         TEXT NOT NULL DEFAULT '{}',
            created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
        );

        CREATE INDEX IF NOT EXISTS idx_secure_credential_audit_logs_created
            ON secure_credential_audit_logs(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_secure_credential_audit_logs_credential
            ON secure_credential_audit_logs(credential_key, created_at DESC);
        ",
    )?;

    set_version(conn, 16)?;
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::{params, Connection};

    use super::{get_version, migrate, SCHEMA_VERSION};

    #[test]
    fn migrates_complete_knowledge_schema_from_empty_database(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;

        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        let tables = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND (
                    name LIKE 'knowledge_%' OR name LIKE 'zentao_%'
                 )",
            )?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        let expected = [
            "knowledge_projects",
            "knowledge_releases",
            "knowledge_sources",
            "knowledge_documents",
            "knowledge_document_versions",
            "knowledge_chunks",
            "knowledge_relations",
            "knowledge_jobs",
            "knowledge_generation_runs",
            "knowledge_embedding_profiles",
            "knowledge_chunk_embeddings",
            "zentao_connections",
            "zentao_project_mappings",
            "zentao_sync_cursors",
            "zentao_entities",
            "zentao_entity_relations",
            "knowledge_code_snapshots",
            "knowledge_code_files",
            "knowledge_code_symbols",
            "knowledge_code_relations",
            "knowledge_code_snapshot_changes",
            "knowledge_project_repository_bindings",
            "knowledge_release_repository_manifests",
            "knowledge_assets",
            "knowledge_document_drafts",
            "knowledge_document_uploads",
            "knowledge_document_version_bindings",
            "knowledge_document_parse_artifacts",
            "knowledge_analysis_runs",
            "knowledge_analysis_drafts",
            "knowledge_graph_builds",
            "knowledge_graph_nodes",
            "knowledge_graph_edges",
            "knowledge_document_title_index",
            "knowledge_backfill_runs",
            "knowledge_document_coverage_snapshots",
            "knowledge_feature_flags",
            "knowledge_qa_sessions",
            "knowledge_qa_messages",
        ];
        for table in expected {
            assert!(tables.contains(table), "缺少迁移表: {table}");
        }

        let connection_columns = conn
            .prepare("PRAGMA table_info(zentao_connections)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        assert!(connection_columns.contains(&"credential_key".to_string()));
        assert!(connection_columns.contains(&"allow_insecure_http".to_string()));
        let project_columns = conn
            .prepare("PRAGMA table_info(knowledge_projects)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        assert!(project_columns.contains(&"git_workspace_keys_json".to_string()));
        let upload_columns = conn
            .prepare("PRAGMA table_info(knowledge_document_uploads)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        assert!(upload_columns.contains(&"allow_remote_ocr".to_string()));
        assert!(upload_columns.contains(&"ocr_provider_key".to_string()));
        assert!(upload_columns.contains(&"source_folder_name".to_string()));
        let analysis_run_columns = conn
            .prepare("PRAGMA table_info(knowledge_analysis_runs)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        assert!(analysis_run_columns.contains(&"snapshot_ids_json".to_string()));
        assert!(!connection_columns.iter().any(|column| {
            ["password", "token", "cookie", "session"]
                .iter()
                .any(|secret| column.to_lowercase().contains(secret))
        }));
        Ok(())
    }

    #[test]
    fn migrating_v48_creates_retryable_qa_session_schema() -> Result<(), Box<dyn std::error::Error>>
    {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE legacy_v48_sentinel (value TEXT NOT NULL);
             INSERT INTO legacy_v48_sentinel(value) VALUES ('preserved');
             CREATE TABLE knowledge_releases (
                 id INTEGER PRIMARY KEY,
                 commit_sha TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE knowledge_qa_sessions (id INTEGER PRIMARY KEY);
             PRAGMA user_version = 48;",
        )?;

        assert!(migrate(&conn).is_err(), "冲突表结构必须让迁移失败");
        assert_eq!(get_version(&conn)?, 48, "失败不能提前推进 user_version");
        assert_eq!(
            conn.query_row("SELECT value FROM legacy_v48_sentinel", [], |row| row
                .get::<_, String>(0))?,
            "preserved"
        );
        let message_table_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'knowledge_qa_messages'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(message_table_count, 0, "失败事务不能留下半张会话表");

        conn.execute("DROP TABLE knowledge_qa_sessions", [])?;
        migrate(&conn)?;
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        let tables = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name LIKE 'knowledge_qa_%'",
            )?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        assert!(tables.contains("knowledge_qa_sessions"));
        assert!(tables.contains("knowledge_qa_messages"));
        let indexes = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'index' AND name LIKE '%knowledge_qa_%'",
            )?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        assert!(indexes.contains("ix_knowledge_qa_sessions_project_updated"));
        assert!(indexes.contains("ix_knowledge_qa_messages_session_sequence"));

        conn.execute(
            "INSERT INTO knowledge_qa_sessions
                (project_id, project_version_id, release_commit_sha, title)
             VALUES (1, 2, 'fixed-sha', '测试会话')",
            [],
        )?;
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO knowledge_qa_messages
                (session_id, sequence_no, role, content)
             VALUES (?1, 1, 'user', '问题')",
            [session_id],
        )?;
        assert!(conn
            .execute(
                "INSERT INTO knowledge_qa_messages
                    (session_id, sequence_no, role, content)
                 VALUES (?1, 1, 'assistant', '重复序号')",
                [session_id],
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO knowledge_qa_messages
                    (session_id, sequence_no, role, content)
                 VALUES (?1, 2, 'system', '非法角色')",
                [session_id],
            )
            .is_err());
        Ok(())
    }

    #[test]
    fn migrating_v49_backfills_session_release_commit_without_losing_messages(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE knowledge_releases (
                 id INTEGER PRIMARY KEY,
                 commit_sha TEXT NOT NULL DEFAULT ''
             );
             INSERT INTO knowledge_releases(id, commit_sha) VALUES (7, 'release-sha');
             CREATE TABLE knowledge_qa_sessions (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 project_id INTEGER NOT NULL,
                 project_version_id INTEGER NOT NULL,
                 provider_key TEXT NOT NULL DEFAULT '',
                 model TEXT NOT NULL DEFAULT '',
                 title TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                 updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                 deleted_at TEXT DEFAULT NULL
             );
             CREATE TABLE knowledge_qa_messages (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id INTEGER NOT NULL,
                 sequence_no INTEGER NOT NULL,
                 role TEXT NOT NULL,
                 content TEXT NOT NULL,
                 evidence_only INTEGER NOT NULL DEFAULT 0,
                 answer_json TEXT DEFAULT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
             );
             INSERT INTO knowledge_qa_sessions
                 (id, project_id, project_version_id, provider_key, model, title)
             VALUES (3, 1, 7, 'chat', 'model-v1', '历史会话');
             INSERT INTO knowledge_qa_messages
                 (session_id, sequence_no, role, content)
             VALUES (3, 1, 'user', '历史问题');
             PRAGMA user_version = 49;",
        )?;

        migrate(&conn)?;
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        assert_eq!(
            conn.query_row(
                "SELECT release_commit_sha FROM knowledge_qa_sessions WHERE id = 3",
                [],
                |row| row.get::<_, String>(0),
            )?,
            "release-sha"
        );
        assert_eq!(
            conn.query_row(
                "SELECT content FROM knowledge_qa_messages
                 WHERE session_id = 3 AND sequence_no = 1",
                [],
                |row| row.get::<_, String>(0),
            )?,
            "历史问题"
        );
        Ok(())
    }

    #[test]
    fn fresh_schema_document_versions_scope_identity_by_source_path(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;

        for source_path in ["docs/order.md", "src/order.md"] {
            conn.execute(
                "INSERT INTO knowledge_document_versions
                    (document_id, version_label, source_path, content, content_hash)
                 VALUES (1, 'v1.0.0', ?1, '相同正文', 'same-hash')",
                [source_path],
            )?;
        }
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM knowledge_document_versions
                 WHERE document_id = 1 AND version_label = 'v1.0.0'
                   AND content_hash = 'same-hash'",
                [],
                |row| row.get::<_, i64>(0),
            )?,
            2
        );

        assert!(
            conn.execute(
                "INSERT INTO knowledge_document_versions
                    (document_id, version_label, source_path, content, content_hash)
                 VALUES (1, 'v1.0.0', 'docs/order.md', '重复正文', 'same-hash')",
                [],
            )
            .is_err(),
            "同一来源路径的重复版本必须被唯一约束拒绝"
        );
        Ok(())
    }

    #[test]
    fn migrating_v34_zentao_connections_defaults_http_to_disabled(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE zentao_connections (
                id INTEGER PRIMARY KEY,
                connection_key TEXT NOT NULL,
                base_url TEXT NOT NULL,
                tls_verify INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO zentao_connections (id, connection_key, base_url, tls_verify)
            VALUES (1, 'legacy-zentao', 'https://zentao.example.test/', 1);
            PRAGMA user_version = 34;
            ",
        )?;

        migrate(&conn)?;

        let (base_url, tls_verify, allow_insecure_http): (String, i64, i64) = conn.query_row(
            "SELECT base_url, tls_verify, allow_insecure_http
             FROM zentao_connections WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(base_url, "https://zentao.example.test/");
        assert_eq!(tls_verify, 1);
        assert_eq!(allow_insecure_http, 0);
        Ok(())
    }

    #[test]
    fn migrating_v39_uploads_preserves_rows_and_adds_ocr_consent_columns(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE knowledge_document_uploads (
                id INTEGER PRIMARY KEY,
                document_id INTEGER NOT NULL UNIQUE,
                asset_id INTEGER NOT NULL,
                release_id INTEGER DEFAULT NULL,
                import_job_id INTEGER NOT NULL UNIQUE,
                original_name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',
                error_message TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT ''
            );
            INSERT INTO knowledge_document_uploads
                (id, document_id, asset_id, import_job_id, original_name, mime_type)
            VALUES (1, 11, 12, 13, '历史图片.png', 'image/png');
            PRAGMA user_version = 39;
            ",
        )?;

        migrate(&conn)?;

        let row: (i64, String) = conn.query_row(
            "SELECT allow_remote_ocr, ocr_provider_key
             FROM knowledge_document_uploads WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(row, (0, String::new()));
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn migrating_v47_backfills_folder_upload_source_name_from_document_path(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE knowledge_documents (
                id INTEGER PRIMARY KEY,
                logical_path TEXT NOT NULL
            );
            CREATE TABLE knowledge_document_uploads (
                id INTEGER PRIMARY KEY,
                document_id INTEGER NOT NULL UNIQUE
            );
            INSERT INTO knowledge_documents (id, logical_path)
            VALUES (1, 'upload-folder/退款原型/assets/style.css'),
                   (2, 'upload-folder/普通文档/readme.md');
            INSERT INTO knowledge_document_uploads (id, document_id)
            VALUES (1, 1), (2, 2);
            PRAGMA user_version = 47;
            ",
        )?;

        migrate(&conn)?;

        let folder_name: Option<String> = conn.query_row(
            "SELECT source_folder_name FROM knowledge_document_uploads WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(folder_name.as_deref(), Some("退款原型"));
        let ordinary_name: Option<String> = conn.query_row(
            "SELECT source_folder_name FROM knowledge_document_uploads WHERE id = 2",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(ordinary_name, Some("普通文档".to_string()));
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn migrating_v41_deduplicates_nullable_document_version_bindings(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        conn.execute_batch(
            "DROP INDEX IF EXISTS ux_knowledge_document_version_binding_scope;
             DROP INDEX IF EXISTS ux_knowledge_document_binding_release_repository;
             DROP INDEX IF EXISTS ux_knowledge_document_binding_release_only;
             DROP INDEX IF EXISTS ux_knowledge_document_binding_repository_only;
             DROP INDEX IF EXISTS ux_knowledge_document_binding_cross_version_only;
             INSERT INTO knowledge_document_version_bindings
                (document_version_id, release_id, repository_binding_id, cross_version_scope)
             VALUES (7, NULL, NULL, 'project_all_versions');
             INSERT INTO knowledge_document_version_bindings
                (document_version_id, release_id, repository_binding_id, cross_version_scope)
             VALUES (7, NULL, NULL, 'project_all_versions');
             PRAGMA user_version = 41;",
        )?;

        migrate(&conn)?;

        let retained: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_document_version_bindings
             WHERE document_version_id = 7 AND release_id IS NULL
               AND repository_binding_id IS NULL
               AND cross_version_scope = 'project_all_versions'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(retained, 1);
        let duplicate = conn.execute(
            "INSERT INTO knowledge_document_version_bindings
                (document_version_id, release_id, repository_binding_id, cross_version_scope)
             VALUES (7, NULL, NULL, 'project_all_versions')",
            [],
        );
        assert!(duplicate.is_err(), "唯一索引必须拒绝 NULL 范围重复记录");
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn fresh_schema_enforces_each_nullable_document_version_binding_scope(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;

        let index_names = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'index'
                   AND name LIKE 'ux_knowledge_document_binding_%'",
            )?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        for index in [
            "ux_knowledge_document_binding_release_repository",
            "ux_knowledge_document_binding_release_only",
            "ux_knowledge_document_binding_repository_only",
            "ux_knowledge_document_binding_cross_version_only",
        ] {
            assert!(
                index_names.contains(index),
                "缺少 NULL 范围唯一索引: {index}"
            );
        }

        let scopes = [
            (None, None, "all"),
            (Some(11), None, "release"),
            (None, Some(12), "repository"),
            (Some(11), Some(12), "release-repository"),
        ];
        for (release_id, repository_binding_id, cross_version_scope) in scopes {
            conn.execute(
                "INSERT INTO knowledge_document_version_bindings
                    (document_version_id, release_id, repository_binding_id, cross_version_scope)
                 VALUES (?1, ?2, ?3, ?4)",
                params![7, release_id, repository_binding_id, cross_version_scope],
            )?;
            assert!(
                conn.execute(
                    "INSERT INTO knowledge_document_version_bindings
                        (document_version_id, release_id, repository_binding_id, cross_version_scope)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![7, release_id, repository_binding_id, cross_version_scope],
                )
                .is_err(),
                "重复写入必须被拒绝: {cross_version_scope}"
            );
        }
        Ok(())
    }

    #[test]
    fn migrating_v42_preserves_earliest_nullable_binding_and_does_not_merge_negative_id(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        conn.execute_batch(
            "
            DROP INDEX ux_knowledge_document_binding_release_repository;
            DROP INDEX ux_knowledge_document_binding_release_only;
            DROP INDEX ux_knowledge_document_binding_repository_only;
            DROP INDEX ux_knowledge_document_binding_cross_version_only;
            INSERT INTO knowledge_document_version_bindings
                (document_version_id, release_id, repository_binding_id, cross_version_scope)
            VALUES (8, NULL, NULL, 'project_all_versions');
            INSERT INTO knowledge_document_version_bindings
                (document_version_id, release_id, repository_binding_id, cross_version_scope)
            VALUES (8, NULL, NULL, 'project_all_versions');
            INSERT INTO knowledge_document_version_bindings
                (document_version_id, release_id, repository_binding_id, cross_version_scope)
            VALUES (8, -1, NULL, 'project_all_versions');
            PRAGMA user_version = 42;
            ",
        )?;

        let earliest_id: i64 = conn.query_row(
            "SELECT MIN(id) FROM knowledge_document_version_bindings
             WHERE document_version_id = 8 AND release_id IS NULL
               AND repository_binding_id IS NULL
               AND cross_version_scope = 'project_all_versions'",
            [],
            |row| row.get(0),
        )?;
        migrate(&conn)?;

        let retained_nullable_ids = conn
            .prepare(
                "SELECT id FROM knowledge_document_version_bindings
                 WHERE document_version_id = 8 AND release_id IS NULL
                   AND repository_binding_id IS NULL
                   AND cross_version_scope = 'project_all_versions'",
            )?
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(retained_nullable_ids, vec![earliest_id]);
        let negative_id_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_document_version_bindings
             WHERE document_version_id = 8 AND release_id = -1
               AND repository_binding_id IS NULL
               AND cross_version_scope = 'project_all_versions'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(negative_id_rows, 1, "NULL 不能与真实 -1 ID 合并");
        assert!(
            conn.execute(
                "INSERT INTO knowledge_document_version_bindings
                    (document_version_id, release_id, repository_binding_id, cross_version_scope)
                 VALUES (8, NULL, NULL, 'project_all_versions')",
                [],
            )
            .is_err(),
            "升级后必须继续拒绝 NULL 范围重复写入"
        );
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn migrating_v35_knowledge_projects_preserves_legacy_workspace_key(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE knowledge_projects (
                id INTEGER PRIMARY KEY,
                project_key TEXT NOT NULL,
                name TEXT NOT NULL,
                git_workspace_key TEXT NOT NULL DEFAULT ''
            );
            INSERT INTO knowledge_projects (id, project_key, name, git_workspace_key)
            VALUES (1, 'legacy-project', '历史项目', 'workspace-a');
            PRAGMA user_version = 35;
            ",
        )?;

        migrate(&conn)?;

        let workspace_keys_json: String = conn.query_row(
            "SELECT git_workspace_keys_json FROM knowledge_projects WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(workspace_keys_json, r#"["workspace-a"]"#);
        Ok(())
    }

    #[test]
    fn migrating_a_current_database_copy_preserves_legacy_experiences_when_knowledge_is_disabled(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 先构造一份已存在经验库数据的当前数据库，再模拟升级期间应用关闭知识库入口。
        // 迁移只能新增索引相关表，不能删除或改变旧经验的可读性。
        let suffix = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        );
        let source_path = std::env::temp_dir().join(format!("tauri-ssh-legacy-{suffix}.db"));
        let copy_path = std::env::temp_dir().join(format!("tauri-ssh-upgrade-{suffix}.db"));
        {
            let source = Connection::open(&source_path)?;
            migrate(&source)?;
            source.execute(
                "INSERT INTO ai_experiences
                 (experience_key, title, symptom, cause, solution, scenario, markdown_path)
                 VALUES ('legacy-experience', '历史经验', '现象', '原因', '方案', '回归验证',
                         'experiences/legacy.md')",
                [],
            )?;
            source.execute(
                "INSERT INTO app_config (key, value) VALUES ('knowledge.rollout.stage', 'disabled')",
                [],
            )?;
        }
        fs::copy(&source_path, &copy_path)?;
        let conn = Connection::open(&copy_path)?;

        // 以升级前的 v33 复制库为输入，只允许执行 v33 -> v34 的增量迁移。
        conn.pragma_update(None, "user_version", 33)?;
        migrate(&conn)?;

        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        let experience: (String, String) = conn.query_row(
            "SELECT title, markdown_path FROM ai_experiences WHERE experience_key = 'legacy-experience'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(
            experience,
            ("历史经验".to_string(), "experiences/legacy.md".to_string())
        );
        let stage: String = conn.query_row(
            "SELECT value FROM app_config WHERE key = 'knowledge.rollout.stage'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(stage, "disabled");
        let knowledge_tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
             AND name LIKE 'knowledge_%'",
            [],
            |row| row.get(0),
        )?;
        assert!(knowledge_tables > 0, "关闭入口不应通过删除索引表回滚");
        drop(conn);
        fs::remove_file(source_path)?;
        fs::remove_file(copy_path)?;
        Ok(())
    }

    #[test]
    fn enforces_single_active_embedding_profile() -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        conn.execute(
            "INSERT INTO knowledge_embedding_profiles
             (profile_key, name, mode, model, fingerprint, status, is_active)
             VALUES (?1, ?2, 'local', ?3, ?4, 'ready', 1)",
            params![
                "local-e5",
                "Local E5",
                "multilingual-e5-small",
                "fingerprint-1"
            ],
        )?;
        let second_active = conn.execute(
            "INSERT INTO knowledge_embedding_profiles
             (profile_key, name, mode, model, fingerprint, status, is_active)
             VALUES (?1, ?2, 'remote', ?3, ?4, 'ready', 1)",
            params![
                "remote-embedding",
                "Remote",
                "text-embedding",
                "fingerprint-2"
            ],
        );
        assert!(second_active.is_err());
        Ok(())
    }

    #[test]
    fn migrates_embedding_fingerprint_to_allow_blue_green_generations(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE knowledge_embedding_profiles (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                profile_key         TEXT NOT NULL UNIQUE,
                name                TEXT NOT NULL,
                mode                TEXT NOT NULL,
                provider_key        TEXT NOT NULL DEFAULT '',
                model               TEXT NOT NULL,
                model_revision      TEXT NOT NULL DEFAULT '',
                dimension           INTEGER NOT NULL DEFAULT 0,
                normalized          INTEGER NOT NULL DEFAULT 1,
                config_json         TEXT NOT NULL DEFAULT '{}',
                fingerprint         TEXT NOT NULL UNIQUE,
                status              TEXT NOT NULL DEFAULT 'draft',
                is_active           INTEGER NOT NULL DEFAULT 0,
                created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
            );
            CREATE INDEX idx_knowledge_embedding_profiles_active
                ON knowledge_embedding_profiles(is_active, status);
            CREATE UNIQUE INDEX ux_knowledge_embedding_profiles_one_active
                ON knowledge_embedding_profiles(is_active)
                WHERE is_active = 1;
            PRAGMA user_version = 50;
            ",
        )?;
        conn.execute(
            "INSERT INTO knowledge_embedding_profiles
             (profile_key, name, mode, model, fingerprint)
             VALUES ('remote-v1', '远程索引 V1', 'remote', 'text-embedding', 'same-space')",
            [],
        )?;

        migrate(&conn)?;

        conn.execute(
            "INSERT INTO knowledge_embedding_profiles
             (profile_key, name, mode, model, fingerprint)
             VALUES ('remote-v2', '远程索引 V2', 'remote', 'text-embedding', 'same-space')",
            [],
        )?;
        let profile_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_embedding_profiles WHERE fingerprint = 'same-space'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(profile_count, 2);
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);

        conn.execute(
            "UPDATE knowledge_embedding_profiles SET is_active = 1 WHERE profile_key = 'remote-v1'",
            [],
        )?;
        assert!(conn
            .execute(
                "UPDATE knowledge_embedding_profiles SET is_active = 1 WHERE profile_key = 'remote-v2'",
                [],
            )
            .is_err());
        Ok(())
    }

    #[test]
    fn scopes_legacy_relations_as_needing_rebuild() -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE knowledge_relations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                from_type TEXT NOT NULL, from_key TEXT NOT NULL, relation_type TEXT NOT NULL,
                to_type TEXT NOT NULL, to_key TEXT NOT NULL, evidence_json TEXT NOT NULL DEFAULT '{}',
                confidence REAL NOT NULL DEFAULT 1.0, confirmed INTEGER NOT NULL DEFAULT 1,
                source TEXT NOT NULL DEFAULT 'user', created_at TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT '', deleted_at TEXT DEFAULT NULL,
                UNIQUE(from_type, from_key, relation_type, to_type, to_key, source)
            );
            INSERT INTO knowledge_relations
                (from_type, from_key, relation_type, to_type, to_key, source)
            VALUES ('requirement', 'REQ-1', 'implemented_by', 'commit', 'abcdef1', 'legacy');
            PRAGMA user_version = 31;
            ",
        )?;
        migrate(&conn)?;
        let (scope_status, sensitivity): (String, String) = conn.query_row(
            "SELECT scope_status, sensitivity FROM knowledge_relations WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(scope_status, "needs_rebuild");
        assert_eq!(sensitivity, "restricted");
        let legacy_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'knowledge_relations_legacy_v31'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(legacy_count, 1, "迁移必须保留旧关系表以便审计和恢复");
        Ok(())
    }

    #[test]
    fn validates_vector_dimension_and_snapshot_scoped_uniqueness(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        conn.execute(
            "INSERT INTO knowledge_projects(project_key, name) VALUES ('project-a', '项目 A')",
            [],
        )?;
        conn.execute(
            "INSERT INTO knowledge_sources
             (source_key, project_id, source_type, display_name)
             VALUES ('source-a', 1, 'git_workspace', '源码 A')",
            [],
        )?;
        conn.execute(
            "INSERT INTO knowledge_embedding_profiles
             (profile_key, name, mode, model, fingerprint, status)
             VALUES ('profile-a', 'Profile A', 'local', 'model-a', 'fingerprint-a', 'ready')",
            [],
        )?;
        conn.execute(
            "INSERT INTO knowledge_code_snapshots
             (snapshot_key, source_id, project_id, snapshot_type, captured_at,
              analyzer_version, status)
             VALUES ('snapshot-a', 1, 1, 'git_commit', '2026-07-30T00:00:00Z', 'v1', 'ready')",
            [],
        )?;
        conn.execute(
            "INSERT INTO knowledge_code_files
             (snapshot_id, relative_path, content_hash, analysis_level)
             VALUES (1, 'src/lib.rs', 'hash-a', 'ast')",
            [],
        )?;

        let duplicate_path = conn.execute(
            "INSERT INTO knowledge_code_files
             (snapshot_id, relative_path, content_hash, analysis_level)
             VALUES (1, 'src/lib.rs', 'hash-b', 'ast')",
            [],
        );
        assert!(duplicate_path.is_err());

        let invalid_dimension = conn.execute(
            "INSERT INTO knowledge_chunk_embeddings
             (chunk_id, profile_id, dimension, vector_blob, vector_norm, content_hash)
             VALUES (1, 1, 0, x'00000000', 0, 'hash-a')",
            [],
        );
        assert!(invalid_dimension.is_err());
        Ok(())
    }

    #[test]
    fn current_knowledge_refactor_fixture_loads_after_schema_migration(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        conn.execute_batch(include_str!(
            "../../tests/fixtures/knowledge_refactor_current_schema.sql"
        ))?;

        for table in [
            "knowledge_projects",
            "knowledge_releases",
            "knowledge_sources",
            "knowledge_documents",
            "knowledge_document_versions",
            "knowledge_chunks",
            "knowledge_chunk_embeddings",
            "knowledge_relations",
            "knowledge_jobs",
            "knowledge_code_snapshots",
        ] {
            let count: i64 =
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?;
            assert_eq!(count, 1, "夹具应为 {table} 提供一条无敏感数据记录");
        }
        Ok(())
    }

    #[test]
    fn migrating_v36_database_preserves_legacy_rows_and_adds_platform_tables(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        conn.execute(
            "INSERT INTO knowledge_projects(project_key, name) VALUES ('legacy-project', '历史项目')",
            [],
        )?;
        conn.execute_batch(
            "
            DROP TABLE knowledge_feature_flags;
            DROP TABLE knowledge_document_coverage_snapshots;
            DROP TABLE knowledge_backfill_runs;
            DROP TABLE knowledge_document_title_index;
            DROP TABLE knowledge_graph_edges;
            DROP TABLE knowledge_graph_nodes;
            DROP TABLE knowledge_graph_builds;
            DROP TABLE knowledge_analysis_drafts;
            DROP TABLE knowledge_analysis_runs;
            DROP TABLE knowledge_document_parse_artifacts;
            DROP TABLE knowledge_document_version_bindings;
            DROP TABLE knowledge_document_drafts;
            DROP TABLE knowledge_assets;
            DROP TABLE knowledge_release_repository_manifests;
            DROP TABLE knowledge_project_repository_bindings;
            PRAGMA user_version = 36;
            ",
        )?;

        migrate(&conn)?;
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        let legacy_projects: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_projects WHERE project_key = 'legacy-project'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(legacy_projects, 1);
        let graph_tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
             AND name IN ('knowledge_graph_builds', 'knowledge_graph_nodes', 'knowledge_graph_edges')",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(graph_tables, 3);
        Ok(())
    }

    #[test]
    fn migrating_v37_database_adds_immutable_document_commit_columns(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE knowledge_document_versions (
                id INTEGER PRIMARY KEY,
                document_id INTEGER NOT NULL,
                release_id INTEGER DEFAULT NULL,
                version_label TEXT NOT NULL DEFAULT '',
                git_branch TEXT NOT NULL DEFAULT '',
                commit_sha TEXT NOT NULL DEFAULT '',
                source_path TEXT NOT NULL DEFAULT '',
                mime_type TEXT NOT NULL DEFAULT 'text/markdown',
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                parsed_meta_json TEXT NOT NULL DEFAULT '{}',
                token_estimate INTEGER NOT NULL DEFAULT 0,
                valid INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE knowledge_documents (
                id INTEGER PRIMARY KEY,
                latest_version_id INTEGER DEFAULT NULL
            );
            CREATE TABLE knowledge_jobs (id INTEGER PRIMARY KEY);
            PRAGMA user_version = 37;
            ",
        )?;

        migrate(&conn)?;
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        let columns = conn
            .prepare("PRAGMA table_info(knowledge_document_versions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        for column in [
            "parent_version_id",
            "author_label",
            "commit_message",
            "index_job_id",
        ] {
            assert!(
                columns.contains(&column.to_string()),
                "缺少提交字段: {column}"
            );
        }
        conn.execute(
            "INSERT INTO knowledge_document_versions
                (id, document_id, content, content_hash) VALUES (1, 1, '正文', 'hash')",
            [],
        )?;
        assert!(conn
            .execute(
                "UPDATE knowledge_document_versions SET content = '篡改' WHERE id = 1",
                [],
            )
            .is_err());
        Ok(())
    }

    #[test]
    fn migrating_v44_adds_unique_immutable_analysis_draft_link(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE knowledge_document_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id INTEGER NOT NULL,
                release_id INTEGER DEFAULT NULL,
                version_label TEXT NOT NULL DEFAULT '',
                git_branch TEXT NOT NULL DEFAULT '',
                commit_sha TEXT NOT NULL DEFAULT '',
                source_path TEXT NOT NULL DEFAULT '',
                mime_type TEXT NOT NULL DEFAULT 'text/markdown',
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                parsed_meta_json TEXT NOT NULL DEFAULT '{}',
                token_estimate INTEGER NOT NULL DEFAULT 0,
                valid INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT '',
                parent_version_id INTEGER DEFAULT NULL,
                author_label TEXT NOT NULL DEFAULT '',
                commit_message TEXT NOT NULL DEFAULT '',
                index_job_id INTEGER DEFAULT NULL,
                UNIQUE(document_id, version_label, content_hash)
            );
            CREATE TRIGGER prevent_knowledge_document_version_update
            BEFORE UPDATE OF document_id, release_id, version_label, git_branch, commit_sha,
                             source_path, mime_type, content, content_hash, parent_version_id,
                             author_label, commit_message ON knowledge_document_versions
            WHEN NOT (NEW.valid = 0 AND NEW.content = '' AND NEW.content_hash = OLD.content_hash)
            BEGIN
                SELECT RAISE(ABORT, '知识文档版本不可修改，请创建新版本');
            END;
            INSERT INTO knowledge_document_versions
                (document_id, content, content_hash, commit_message)
             VALUES (1, 'v44 既有正文', 'analysis-link-hash-1', '历史提交');
            PRAGMA user_version = 44;
            ",
        )?;

        migrate(&conn)?;
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        let columns = conn
            .prepare("PRAGMA table_info(knowledge_document_versions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        assert!(columns.contains(&"analysis_draft_id".to_string()));
        let old_content: String = conn.query_row(
            "SELECT content FROM knowledge_document_versions WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(old_content, "v44 既有正文");
        conn.execute(
            "INSERT INTO knowledge_document_versions
                (document_id, content, content_hash, analysis_draft_id)
             VALUES (2, 'AI 分析', 'analysis-link-hash-2', 10)",
            [],
        )?;
        assert!(conn
            .execute(
                "INSERT INTO knowledge_document_versions
                    (document_id, content, content_hash, analysis_draft_id)
                 VALUES (3, '重复关联', 'analysis-link-hash-3', 10)",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE knowledge_document_versions SET analysis_draft_id = 11 WHERE id = 1",
                [],
            )
            .is_err());
        Ok(())
    }

    #[test]
    fn migrating_v45_creates_project_scoped_soft_deletable_terms(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA user_version = 45;")?;
        migrate(&conn)?;
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        conn.execute(
            "INSERT INTO knowledge_project_terms
                (project_id, term, normalized_term, aliases_json, confirmation_note)
             VALUES (1, '工单', '工单', '[\"WorkOrder\"]', '已确认')",
            [],
        )?;
        assert!(conn
            .execute(
                "INSERT INTO knowledge_project_terms
                    (project_id, term, normalized_term, aliases_json, confirmation_note)
                 VALUES (1, '工单', '工单', '[\"WorkOrder\"]', '重复')",
                [],
            )
            .is_err());
        conn.execute(
            "UPDATE knowledge_project_terms
             SET deleted_at = datetime('now', 'localtime') WHERE project_id = 1",
            [],
        )?;
        conn.execute(
            "INSERT INTO knowledge_project_terms
                (project_id, term, normalized_term, aliases_json, confirmation_note)
             VALUES (1, '工单', '工单', '[\"WorkOrder\"]', '重新确认')",
            [],
        )?;
        Ok(())
    }

    #[test]
    fn migrating_v46_extends_document_version_identity_with_source_path(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE knowledge_document_versions (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id         INTEGER NOT NULL,
                release_id          INTEGER DEFAULT NULL,
                version_label       TEXT NOT NULL DEFAULT '',
                git_branch          TEXT NOT NULL DEFAULT '',
                commit_sha          TEXT NOT NULL DEFAULT '',
                source_path         TEXT NOT NULL DEFAULT '',
                mime_type           TEXT NOT NULL DEFAULT 'text/markdown',
                content             TEXT NOT NULL,
                content_hash        TEXT NOT NULL,
                parsed_meta_json    TEXT NOT NULL DEFAULT '{}',
                token_estimate      INTEGER NOT NULL DEFAULT 0,
                valid               INTEGER NOT NULL DEFAULT 1,
                created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                parent_version_id   INTEGER DEFAULT NULL,
                author_label        TEXT NOT NULL DEFAULT '',
                commit_message      TEXT NOT NULL DEFAULT '',
                index_job_id        INTEGER DEFAULT NULL,
                analysis_draft_id   INTEGER DEFAULT NULL,
                UNIQUE(document_id, version_label, content_hash)
            );
            CREATE INDEX idx_knowledge_doc_versions_release
                ON knowledge_document_versions(release_id, document_id);
            CREATE INDEX idx_knowledge_doc_versions_commit
                ON knowledge_document_versions(commit_sha, document_id);
            CREATE INDEX idx_knowledge_document_versions_parent
                ON knowledge_document_versions(document_id, parent_version_id);
            CREATE INDEX idx_knowledge_document_versions_index_job
                ON knowledge_document_versions(index_job_id);
            CREATE UNIQUE INDEX ux_knowledge_document_versions_analysis_draft
                ON knowledge_document_versions(analysis_draft_id)
                WHERE analysis_draft_id IS NOT NULL;
            CREATE TRIGGER prevent_knowledge_document_version_update
            BEFORE UPDATE OF document_id, release_id, version_label, git_branch, commit_sha,
                             source_path, mime_type, content, content_hash, parent_version_id,
                             author_label, commit_message, analysis_draft_id ON knowledge_document_versions
            WHEN NOT (NEW.valid = 0 AND NEW.content = '' AND NEW.content_hash = OLD.content_hash)
            BEGIN
                SELECT RAISE(ABORT, '知识文档版本不可修改，请创建新版本');
            END;
            INSERT INTO knowledge_document_versions
                (id, document_id, release_id, version_label, source_path, content, content_hash,
                 analysis_draft_id)
            VALUES (7, 1, 42, 'v1.0.0', 'legacy/order.sql', '历史正文', 'same-hash', 99);
            PRAGMA user_version = 46;
            ",
        )?;

        migrate(&conn)?;
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);

        let preserved: (i64, i64, String, String, String, i64) = conn.query_row(
            "SELECT id, release_id, version_label, source_path, content, analysis_draft_id
             FROM knowledge_document_versions WHERE id = 7",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;
        assert_eq!(
            preserved,
            (
                7,
                42,
                "v1.0.0".to_string(),
                "legacy/order.sql".to_string(),
                "历史正文".to_string(),
                99,
            )
        );

        let indexes = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'index' AND tbl_name = 'knowledge_document_versions'",
            )?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        for index in [
            "idx_knowledge_doc_versions_release",
            "idx_knowledge_doc_versions_commit",
            "idx_knowledge_document_versions_parent",
            "idx_knowledge_document_versions_index_job",
            "ux_knowledge_document_versions_analysis_draft",
        ] {
            assert!(indexes.contains(index), "迁移后缺少索引 {index}");
        }
        let trigger_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'trigger' AND name = 'prevent_knowledge_document_version_update'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(trigger_count, 1);

        conn.execute(
            "INSERT INTO knowledge_document_versions
                (document_id, release_id, version_label, source_path, content, content_hash)
             VALUES (1, 42, 'v1.0.0', 'new/order.sql', '相同正文', 'same-hash')",
            [],
        )?;
        assert!(conn
            .execute(
                "INSERT INTO knowledge_document_versions
                    (document_id, release_id, version_label, source_path, content, content_hash)
                 VALUES (1, 42, 'v1.0.0', 'legacy/order.sql', '重复正文', 'same-hash')",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE knowledge_document_versions SET content = '篡改' WHERE id = 7",
                [],
            )
            .is_err());

        let next_id: i64 = conn.query_row(
            "INSERT INTO knowledge_document_versions
                 (document_id, release_id, version_label, source_path, content, content_hash)
             VALUES (1, 42, 'v1.0.0', 'third/order.sql', '第三份正文', 'same-hash')
             RETURNING id",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(next_id, 9);
        Ok(())
    }

    #[test]
    fn v37_migration_rolls_back_after_schema_failure_and_retries_cleanly(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        conn.execute_batch(
            "
            DROP TABLE knowledge_feature_flags;
            DROP TABLE knowledge_document_coverage_snapshots;
            DROP TABLE knowledge_backfill_runs;
            DROP TABLE knowledge_document_title_index;
            DROP TABLE knowledge_graph_edges;
            DROP TABLE knowledge_graph_nodes;
            DROP TABLE knowledge_graph_builds;
            DROP TABLE knowledge_analysis_drafts;
            DROP TABLE knowledge_analysis_runs;
            DROP TABLE knowledge_document_parse_artifacts;
            DROP TABLE knowledge_document_version_bindings;
            DROP TABLE knowledge_document_drafts;
            DROP TABLE knowledge_assets;
            DROP TABLE knowledge_release_repository_manifests;
            DROP TABLE knowledge_project_repository_bindings;
            CREATE TABLE knowledge_assets (id INTEGER PRIMARY KEY);
            PRAGMA user_version = 36;
            ",
        )?;

        assert!(migrate(&conn).is_err(), "不兼容的既有表必须让迁移失败");
        assert_eq!(get_version(&conn)?, 36, "失败不能提前推进 user_version");
        let binding_table_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'knowledge_project_repository_bindings'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(binding_table_count, 0, "事务失败不得留下部分 v37 表");

        conn.execute("DROP TABLE knowledge_assets", [])?;
        migrate(&conn)?;
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION);
        Ok(())
    }

    #[test]
    fn refuses_future_schema_version_without_touching_database(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)?;
        let error = migrate(&conn).expect_err("未来版本必须拒绝启动");
        assert!(error.to_string().contains("高于应用支持的版本"));
        assert_eq!(get_version(&conn)?, SCHEMA_VERSION + 1);
        Ok(())
    }

    #[test]
    fn migrates_a_v36_copy_without_modifying_the_source_database(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let suffix = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        );
        let source_path = std::env::temp_dir().join(format!("tauri-ssh-v36-source-{suffix}.db"));
        let copy_path = std::env::temp_dir().join(format!("tauri-ssh-v36-copy-{suffix}.db"));
        {
            let source = Connection::open(&source_path)?;
            migrate(&source)?;
            source.execute_batch(include_str!(
                "../../tests/fixtures/knowledge_refactor_current_schema.sql"
            ))?;
            source.execute_batch(
                "
                DROP TABLE knowledge_feature_flags;
                DROP TABLE knowledge_document_coverage_snapshots;
                DROP TABLE knowledge_backfill_runs;
                DROP TABLE knowledge_document_title_index;
                DROP TABLE knowledge_graph_edges;
                DROP TABLE knowledge_graph_nodes;
                DROP TABLE knowledge_graph_builds;
                DROP TABLE knowledge_analysis_drafts;
                DROP TABLE knowledge_analysis_runs;
                DROP TABLE knowledge_document_parse_artifacts;
                DROP TABLE knowledge_document_version_bindings;
                DROP TABLE knowledge_document_drafts;
                DROP TABLE knowledge_assets;
                DROP TABLE knowledge_release_repository_manifests;
                DROP TABLE knowledge_project_repository_bindings;
                PRAGMA user_version = 36;
                ",
            )?;
            assert_eq!(get_version(&source)?, 36);
        }
        fs::copy(&source_path, &copy_path)?;

        let source = Connection::open(&source_path)?;
        let before_projects: i64 =
            source.query_row("SELECT COUNT(*) FROM knowledge_projects", [], |row| {
                row.get(0)
            })?;
        let before_documents: i64 =
            source.query_row("SELECT COUNT(*) FROM knowledge_documents", [], |row| {
                row.get(0)
            })?;
        assert_eq!(get_version(&source)?, 36);
        let copy = Connection::open(&copy_path)?;
        migrate(&copy)?;
        assert_eq!(get_version(&copy)?, SCHEMA_VERSION);
        let after_projects: i64 =
            copy.query_row("SELECT COUNT(*) FROM knowledge_projects", [], |row| {
                row.get(0)
            })?;
        let after_documents: i64 =
            copy.query_row("SELECT COUNT(*) FROM knowledge_documents", [], |row| {
                row.get(0)
            })?;
        assert_eq!(
            (after_projects, after_documents),
            (before_projects, before_documents)
        );
        assert_eq!(get_version(&source)?, 36, "原数据库副本不得被迁移过程修改");
        drop(copy);
        drop(source);
        fs::remove_file(source_path)?;
        fs::remove_file(copy_path)?;
        Ok(())
    }
}
