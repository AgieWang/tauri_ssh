use rusqlite::{params, OptionalExtension};

use crate::database::Database;
use crate::error::AppError;

pub(crate) const DOMAIN: &str = "governance";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeFeatureFlagRecord {
    pub id: i64,
    pub feature_key: String,
    pub project_id: Option<i64>,
    pub enabled: bool,
}

/// 完整性巡检只报告可修复的领域数据缺口，不会在读取或启动期间删除任何历史数据。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct KnowledgePlatformIntegrityReport {
    pub orphan_assets: i64,
    pub invalid_draft_parents: i64,
    pub unbound_document_versions: i64,
    pub invalid_release_manifests: i64,
    pub invalid_graph_edges: i64,
    pub invalid_active_profiles: i64,
    pub invalid_relation_references: i64,
}

impl KnowledgePlatformIntegrityReport {
    pub(crate) const fn is_valid(&self) -> bool {
        self.orphan_assets == 0
            && self.invalid_draft_parents == 0
            && self.unbound_document_versions == 0
            && self.invalid_release_manifests == 0
            && self.invalid_graph_edges == 0
            && self.invalid_active_profiles == 0
            && self.invalid_relation_references == 0
    }
}

impl Database {
    pub(crate) fn set_knowledge_feature_flag(
        &self,
        feature_key: &str,
        project_id: Option<i64>,
        enabled: bool,
    ) -> Result<KnowledgeFeatureFlagRecord, AppError> {
        if feature_key.trim().is_empty() || project_id.is_some_and(|id| id <= 0) {
            return Err(AppError::InvalidInput(
                "功能开关名称或项目范围无效".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        if let Some(project_id) = project_id {
            conn.execute(
                "INSERT INTO knowledge_feature_flags(feature_key, project_id, enabled, updated_at)
                 VALUES (?1, ?2, ?3, datetime('now', 'localtime'))
                 ON CONFLICT(feature_key, project_id) DO UPDATE SET enabled = excluded.enabled,
                     updated_at = excluded.updated_at",
                params![feature_key.trim(), project_id, enabled as i64],
            )?;
        } else {
            // SQLite 的 UNIQUE 对 NULL 不视为相等，不能依赖 ON CONFLICT 去重全局开关。
            let changed = conn.execute(
                "UPDATE knowledge_feature_flags
                 SET enabled = ?2, updated_at = datetime('now', 'localtime')
                 WHERE feature_key = ?1 AND project_id IS NULL",
                params![feature_key.trim(), enabled as i64],
            )?;
            if changed == 0 {
                conn.execute(
                    "INSERT INTO knowledge_feature_flags(feature_key, project_id, enabled, updated_at)
                     VALUES (?1, NULL, ?2, datetime('now', 'localtime'))",
                    params![feature_key.trim(), enabled as i64],
                )?;
            }
        }
        get_feature_flag(&conn, feature_key.trim(), project_id)?
            .ok_or_else(|| AppError::Custom("保存功能开关后未找到记录".to_string()))
    }

    pub(crate) fn get_knowledge_feature_flag(
        &self,
        feature_key: &str,
        project_id: Option<i64>,
    ) -> Result<Option<KnowledgeFeatureFlagRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_feature_flag(&conn, feature_key.trim(), project_id)
    }

    pub(crate) fn insert_knowledge_document_coverage_snapshot(
        &self,
        project_id: i64,
        release_id: Option<i64>,
        repository_binding_id: Option<i64>,
        document_type: &str,
        metrics_json: &str,
    ) -> Result<i64, AppError> {
        if project_id <= 0 {
            return Err(AppError::InvalidInput("覆盖报告必须指定项目".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_document_coverage_snapshots
                (project_id, release_id, repository_binding_id, document_type, metrics_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                project_id,
                release_id,
                repository_binding_id,
                document_type.trim(),
                metrics_json
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 为迁移、回填和发布门禁提供只读巡检。迁移中的旧文档若已有 release_id，视为具备
    /// 兼容范围；新文档版本则必须通过 v37 绑定表或显式跨版本范围归属。
    pub(crate) fn inspect_knowledge_platform_integrity(
        &self,
    ) -> Result<KnowledgePlatformIntegrityReport, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        Ok(KnowledgePlatformIntegrityReport {
            orphan_assets: count(&conn, "
                SELECT COUNT(*) FROM knowledge_assets asset
                WHERE asset.deleted_at IS NULL AND asset.reference_count = 0
                  AND NOT EXISTS (
                    SELECT 1 FROM knowledge_document_parse_artifacts artifact
                    WHERE artifact.asset_id = asset.id
                  )")?,
            invalid_draft_parents: count(&conn, "
                SELECT COUNT(*) FROM knowledge_document_drafts draft
                WHERE draft.deleted_at IS NULL AND (
                    (draft.document_id IS NOT NULL AND NOT EXISTS (
                        SELECT 1 FROM knowledge_documents document WHERE document.id = draft.document_id
                    )) OR (draft.base_version_id IS NOT NULL AND NOT EXISTS (
                        SELECT 1 FROM knowledge_document_versions version WHERE version.id = draft.base_version_id
                    ))
                )")?,
            unbound_document_versions: count(&conn, "
                SELECT COUNT(*) FROM knowledge_document_versions version
                WHERE version.release_id IS NULL
                  AND NOT EXISTS (
                    SELECT 1 FROM knowledge_document_version_bindings binding
                    WHERE binding.document_version_id = version.id
                  )")?,
            invalid_release_manifests: count(&conn, "
                SELECT COUNT(*) FROM knowledge_release_repository_manifests manifest
                WHERE NOT EXISTS (SELECT 1 FROM knowledge_releases release WHERE release.id = manifest.release_id)
                   OR NOT EXISTS (
                    SELECT 1 FROM knowledge_project_repository_bindings binding
                    WHERE binding.id = manifest.repository_binding_id
                   )")?,
            invalid_graph_edges: count(&conn, "
                SELECT COUNT(*) FROM knowledge_graph_edges edge
                WHERE trim(edge.evidence_ref) = ''
                   OR NOT EXISTS (
                    SELECT 1 FROM knowledge_graph_nodes node
                    WHERE node.id = edge.from_node_id AND node.graph_build_id = edge.graph_build_id
                   )
                   OR NOT EXISTS (
                    SELECT 1 FROM knowledge_graph_nodes node
                    WHERE node.id = edge.to_node_id AND node.graph_build_id = edge.graph_build_id
                   )")?,
            invalid_active_profiles: count(&conn, "
                SELECT COUNT(*) FROM knowledge_embedding_profiles
                WHERE is_active = 1 AND status NOT IN ('active', 'ready')")?,
            invalid_relation_references: count(&conn, "
                SELECT COUNT(*) FROM knowledge_relations relation
                WHERE relation.document_version_id > 0 AND NOT EXISTS (
                    SELECT 1 FROM knowledge_document_versions version
                    WHERE version.id = relation.document_version_id
                )")?,
        })
    }
}

fn count(conn: &rusqlite::Connection, sql: &str) -> Result<i64, AppError> {
    conn.query_row(sql, [], |row| row.get(0))
        .map_err(Into::into)
}

fn get_feature_flag(
    conn: &rusqlite::Connection,
    feature_key: &str,
    project_id: Option<i64>,
) -> Result<Option<KnowledgeFeatureFlagRecord>, AppError> {
    conn.query_row(
        "SELECT id, feature_key, project_id, enabled
         FROM knowledge_feature_flags
         WHERE feature_key = ?1 AND project_id IS ?2",
        params![feature_key, project_id],
        |row| {
            Ok(KnowledgeFeatureFlagRecord {
                id: row.get(0)?,
                feature_key: row.get(1)?,
                project_id: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
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
    fn feature_flags_keep_global_and_project_scopes_separate(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        let global = database.set_knowledge_feature_flag("graph", None, false)?;
        let global_updated = database.set_knowledge_feature_flag("graph", None, true)?;
        assert_eq!(global.id, global_updated.id, "全局 NULL 范围必须幂等");
        let project = database.set_knowledge_feature_flag("graph", Some(1), false)?;
        assert_ne!(global.id, project.id);
        assert!(
            database
                .get_knowledge_feature_flag("graph", None)?
                .expect("全局开关")
                .enabled
        );
        assert!(
            !database
                .get_knowledge_feature_flag("graph", Some(1))?
                .expect("项目开关")
                .enabled
        );
        let coverage_id = database.insert_knowledge_document_coverage_snapshot(
            1,
            Some(2),
            None,
            "docx",
            "{\"parsed\":1}",
        )?;
        assert!(coverage_id > 0);
        Ok(())
    }

    #[test]
    fn integrity_report_exposes_orphan_and_scope_violations_without_deleting_data(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute_batch(
                "
                INSERT INTO knowledge_assets
                    (asset_key, content_hash, storage_key, original_name, normalized_name, mime_type, size_bytes)
                VALUES ('orphan', 'orphan-hash', 'sha256/orphan', 'orphan.md', 'orphan.md', 'text/markdown', 1);
                INSERT INTO knowledge_document_drafts(project_id, title, content, base_version_id)
                VALUES (1, '孤立草稿', '正文', 999);
                INSERT INTO knowledge_documents(document_key, doc_type, title)
                VALUES ('unbound-document', 'markdown', '未绑定文档');
                INSERT INTO knowledge_document_versions(document_id, content, content_hash)
                VALUES (1, '正文', 'unbound-hash');
                INSERT INTO knowledge_release_repository_manifests
                    (release_id, repository_binding_id, requested_ref_type, requested_ref_name)
                VALUES (999, 999, 'tag', 'v1');
                INSERT INTO knowledge_graph_builds(build_key, project_id, release_id, projection_version, source_hash)
                VALUES ('invalid-graph', 1, 1, 'v1', 'graph-hash');
                INSERT INTO knowledge_graph_edges
                    (graph_build_id, from_node_id, relation_type, to_node_id, evidence_ref)
                VALUES (1, 999, 'depends_on', 998, '');
                INSERT INTO knowledge_embedding_profiles
                    (profile_key, name, mode, model, fingerprint, status, is_active)
                VALUES ('invalid-profile', '无效活动配置', 'local', 'model', 'invalid-profile', 'draft', 1);
                INSERT INTO knowledge_relations
                    (project_id, release_id, document_version_id, snapshot_id, sensitivity, scope_status,
                     from_type, from_key, relation_type, to_type, to_key)
                VALUES (1, 1, 999, 0, 'internal', 'scoped', 'document', 'a', 'references', 'document', 'b');
                ",
            )?;
        }
        let report = database.inspect_knowledge_platform_integrity()?;
        assert!(!report.is_valid());
        assert_eq!(report.orphan_assets, 1);
        assert_eq!(report.invalid_draft_parents, 1);
        assert_eq!(report.unbound_document_versions, 1);
        assert_eq!(report.invalid_release_manifests, 1);
        assert_eq!(report.invalid_graph_edges, 1);
        assert_eq!(report.invalid_active_profiles, 1);
        assert_eq!(report.invalid_relation_references, 1);
        let persisted_assets: i64 = database
            .conn
            .lock()
            .map_err(|error| error.to_string())?
            .query_row("SELECT COUNT(*) FROM knowledge_assets", [], |row| {
                row.get(0)
            })?;
        assert_eq!(persisted_assets, 1, "巡检不得清理或修改历史数据");
        Ok(())
    }
}
