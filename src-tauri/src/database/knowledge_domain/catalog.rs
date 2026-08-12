use std::collections::HashSet;

use rusqlite::{params, types::Type, OptionalExtension};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    KnowledgeGitRefType, KnowledgeProjectVersionCompleteness,
    KnowledgeProjectVersionStageCompleteness, KnowledgeRelease, KnowledgeReleaseRepositoryManifest,
    KnowledgeRepositoryBinding, KnowledgeRepositoryBindingInput, KnowledgeVersionStrategy,
    RepositoryBindingInput,
};

/// Service 已完成白名单 Git 解析后的内部写入值；绝不直接接受 IPC 的 Commit SHA。
pub(crate) struct NewKnowledgeReleaseRepositoryManifest {
    pub repository_binding_id: i64,
    pub requested_ref_type: KnowledgeGitRefType,
    pub requested_ref_name: String,
    pub resolved_commit_sha: String,
    pub inclusion_status: String,
    pub exclusion_reason: String,
    pub worktree_dirty: bool,
}

impl Database {
    pub fn knowledge_project_name_taken(
        &self,
        name: &str,
        except_id: Option<i64>,
    ) -> Result<bool, AppError> {
        let normalized_name = name.trim();
        if normalized_name.is_empty() {
            return Ok(false);
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM knowledge_projects
                 WHERE deleted_at IS NULL AND lower(trim(name)) = lower(trim(?1))
                   AND (?2 IS NULL OR id != ?2)
             )",
            params![normalized_name, except_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }

    pub fn knowledge_project_exists(&self, id: i64) -> Result<bool, AppError> {
        if id <= 0 {
            return Ok(false);
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_projects WHERE id = ?1 AND deleted_at IS NULL)",
            [id],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }

    /// 目录领域只保存已登记工作区的稳定 key；工作区是否存在由 Service 在写入前核验。
    pub fn replace_knowledge_project_repository_bindings(
        &self,
        input: &KnowledgeRepositoryBindingInput,
    ) -> Result<Vec<KnowledgeRepositoryBinding>, AppError> {
        if input.project_id <= 0 {
            return Err(AppError::InvalidInput("项目 ID 必须为正数".into()));
        }
        if input.repositories.is_empty() {
            return Err(AppError::InvalidInput("项目至少需要关联一个仓库".into()));
        }
        let mut workspace_keys = HashSet::new();
        for repository in &input.repositories {
            let key = repository.workspace_key.trim();
            if key.is_empty() {
                return Err(AppError::InvalidInput("仓库工作区不能为空".into()));
            }
            if !workspace_keys.insert(key.to_string()) {
                return Err(AppError::InvalidInput(format!("重复关联仓库: {key}")));
            }
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE knowledge_project_repository_bindings
             SET enabled = 0, deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE project_id = ?1 AND deleted_at IS NULL",
            [input.project_id],
        )?;
        for repository in &input.repositories {
            upsert_binding(&tx, input.project_id, repository)?;
        }
        tx.commit()?;
        drop(conn);
        self.list_knowledge_project_repository_bindings(input.project_id)
    }

    pub fn list_knowledge_project_repository_bindings(
        &self,
        project_id: i64,
    ) -> Result<Vec<KnowledgeRepositoryBinding>, AppError> {
        if project_id <= 0 {
            return Err(AppError::InvalidInput("项目 ID 必须为正数".into()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, project_id, workspace_key, alias, repository_role, default_branch,
                    version_strategy, enabled, deleted_at
             FROM knowledge_project_repository_bindings
             WHERE project_id = ?1 AND deleted_at IS NULL
             ORDER BY id",
        )?;
        let bindings = statement
            .query_map([project_id], map_binding)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(bindings)
    }

    pub fn get_knowledge_project_repository_binding(
        &self,
        id: i64,
    ) -> Result<Option<KnowledgeRepositoryBinding>, AppError> {
        if id <= 0 {
            return Err(AppError::InvalidInput("仓库关联 ID 必须为正数".into()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, project_id, workspace_key, alias, repository_role, default_branch,
                    version_strategy, enabled, deleted_at
             FROM knowledge_project_repository_bindings
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
            map_binding,
        )
        .optional()
        .map_err(Into::into)
    }

    /// 解除关联只停用当前绑定；历史版本清单、文档绑定和审计仍持有原绑定 ID，不能物理删除。
    pub fn deactivate_knowledge_project_repository_binding(&self, id: i64) -> Result<(), AppError> {
        if id <= 0 {
            return Err(AppError::InvalidInput("仓库关联 ID 必须为正数".into()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let changed = conn.execute(
            "UPDATE knowledge_project_repository_bindings
             SET enabled = 0, deleted_at = datetime('now', 'localtime'), updated_at = datetime('now', 'localtime')
             WHERE id = ?1 AND deleted_at IS NULL",
            [id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound(format!("当前仓库关联不存在: {id}")));
        }
        Ok(())
    }

    /// 版本清单一经写入即不可替换；刷新移动分支必须创建新的发布版本而非篡改历史 Commit。
    pub(crate) fn insert_knowledge_release_repository_manifests(
        &self,
        release_id: i64,
        manifests: &[NewKnowledgeReleaseRepositoryManifest],
    ) -> Result<Vec<KnowledgeReleaseRepositoryManifest>, AppError> {
        if release_id <= 0 || manifests.is_empty() {
            return Err(AppError::InvalidInput("版本清单不能为空".to_string()));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        for manifest in manifests {
            tx.execute(
                "INSERT INTO knowledge_release_repository_manifests
                     (release_id, repository_binding_id, requested_ref_type, requested_ref_name,
                      resolved_commit_sha, capture_kind, inclusion_status, exclusion_reason,
                      worktree_dirty, captured_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'git', ?6, ?7, ?8,
                         CASE WHEN ?6 = 'ready' THEN datetime('now', 'localtime') ELSE NULL END,
                         datetime('now', 'localtime'), datetime('now', 'localtime'))",
                params![
                    release_id,
                    manifest.repository_binding_id,
                    manifest.requested_ref_type.as_str(),
                    manifest.requested_ref_name,
                    manifest.resolved_commit_sha,
                    manifest.inclusion_status,
                    manifest.exclusion_reason,
                    manifest.worktree_dirty as i64,
                ],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.list_knowledge_release_repository_manifests(release_id)
    }

    /// 创建发布记录与仓库清单使用同一事务：任一仓库未能落盘时不得留下“已创建”但无证据的版本。
    pub(crate) fn create_knowledge_release_with_repository_manifests(
        &self,
        project_id: i64,
        version: &str,
        manifests: &[NewKnowledgeReleaseRepositoryManifest],
    ) -> Result<(KnowledgeRelease, Vec<KnowledgeReleaseRepositoryManifest>), AppError> {
        if project_id <= 0 || version.trim().is_empty() || manifests.is_empty() {
            return Err(AppError::InvalidInput(
                "项目版本及其仓库清单不能为空".to_string(),
            ));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO knowledge_releases
                 (project_id, version, tag_name, branch, commit_sha, description, released_at,
                  created_at, updated_at, deleted_at)
             VALUES (?1, ?2, '', '', '', '', NULL, datetime('now', 'localtime'),
                     datetime('now', 'localtime'), NULL)",
            params![project_id, version.trim()],
        )?;
        let release_id = tx.last_insert_rowid();
        for manifest in manifests {
            tx.execute(
                "INSERT INTO knowledge_release_repository_manifests
                     (release_id, repository_binding_id, requested_ref_type, requested_ref_name,
                      resolved_commit_sha, capture_kind, inclusion_status, exclusion_reason,
                      worktree_dirty, captured_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'git', ?6, ?7, ?8,
                         CASE WHEN ?6 = 'ready' THEN datetime('now', 'localtime') ELSE NULL END,
                         datetime('now', 'localtime'), datetime('now', 'localtime'))",
                params![
                    release_id,
                    manifest.repository_binding_id,
                    manifest.requested_ref_type.as_str(),
                    manifest.requested_ref_name,
                    manifest.resolved_commit_sha,
                    manifest.inclusion_status,
                    manifest.exclusion_reason,
                    manifest.worktree_dirty as i64,
                ],
            )?;
        }
        let release = tx.query_row(
            "SELECT id, project_id, version, tag_name, branch, commit_sha, description,
                    released_at, created_at, updated_at, deleted_at
             FROM knowledge_releases WHERE id = ?1",
            [release_id],
            |row| {
                Ok(KnowledgeRelease {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    version: row.get(2)?,
                    tag_name: row.get(3)?,
                    branch: row.get(4)?,
                    commit_sha: row.get(5)?,
                    description: row.get(6)?,
                    released_at: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    deleted_at: row.get(10)?,
                })
            },
        )?;
        let mut statement = tx.prepare(
            "SELECT id, release_id, repository_binding_id, requested_ref_type, requested_ref_name,
                    resolved_commit_sha, capture_kind, inclusion_status, exclusion_reason,
                    worktree_dirty, captured_at
             FROM knowledge_release_repository_manifests
             WHERE release_id = ?1 ORDER BY repository_binding_id, id",
        )?;
        let saved = statement
            .query_map([release_id], map_release_manifest)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        tx.commit()?;
        Ok((release, saved))
    }

    pub fn list_knowledge_release_repository_manifests(
        &self,
        release_id: i64,
    ) -> Result<Vec<KnowledgeReleaseRepositoryManifest>, AppError> {
        if release_id <= 0 {
            return Err(AppError::InvalidInput("版本 ID 必须为正数".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, release_id, repository_binding_id, requested_ref_type, requested_ref_name,
                    resolved_commit_sha, capture_kind, inclusion_status, exclusion_reason,
                    worktree_dirty, captured_at
             FROM knowledge_release_repository_manifests
             WHERE release_id = ?1 ORDER BY repository_binding_id, id",
        )?;
        let manifests = statement
            .query_map([release_id], map_release_manifest)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(manifests)
    }

    /// 同一项目版本的清单是不可变事实。创建重试需要先按版本取得既有事实，再由
    /// Service 对比完整的仓库引用，不能以“版本已存在”笼统拒绝安全重试。
    pub(crate) fn get_knowledge_release_by_project_and_version(
        &self,
        project_id: i64,
        version: &str,
    ) -> Result<Option<KnowledgeRelease>, AppError> {
        if project_id <= 0 || version.trim().is_empty() {
            return Err(AppError::InvalidInput("项目版本不能为空".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.query_row(
            "SELECT id, project_id, version, tag_name, branch, commit_sha, description,
                    released_at, created_at, updated_at, deleted_at
             FROM knowledge_releases
             WHERE project_id = ?1 AND version = ?2 COLLATE NOCASE AND deleted_at IS NULL
             ORDER BY id
             LIMIT 1",
            params![project_id, version.trim()],
            |row| {
                Ok(KnowledgeRelease {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    version: row.get(2)?,
                    tag_name: row.get(3)?,
                    branch: row.get(4)?,
                    commit_sha: row.get(5)?,
                    description: row.get(6)?,
                    released_at: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    deleted_at: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// 版本完整度仅汇总已经持久化的事实。没有可计算的预期数量时使用“未开始”，不把
    /// 空集合显示为已完成，避免误导用户把尚未执行的分析、图谱或向量当作就绪。
    pub(crate) fn get_knowledge_project_version_completeness(
        &self,
        release_id: i64,
    ) -> Result<KnowledgeProjectVersionCompleteness, AppError> {
        if release_id <= 0 {
            return Err(AppError::InvalidInput("版本 ID 必须为正数".to_string()));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let release = conn
            .query_row(
                "SELECT id, project_id, version FROM knowledge_releases
                 WHERE id = ?1 AND deleted_at IS NULL",
                [release_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
        let manifest_total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_release_repository_manifests WHERE release_id = ?1",
            [release_id],
            |row| row.get(0),
        )?;
        let manifest_ready: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_release_repository_manifests
             WHERE release_id = ?1 AND inclusion_status IN ('ready', 'excluded')",
            [release_id],
            |row| row.get(0),
        )?;
        let document_versions: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_document_versions version
             JOIN knowledge_documents document ON document.id = version.document_id
             WHERE (version.release_id = ?1 OR EXISTS (
                    SELECT 1 FROM knowledge_document_version_bindings version_binding
                    WHERE version_binding.document_version_id = version.id
                      AND version_binding.cross_version_scope = 'project_all_versions'
             )) AND document.project_id = ?2 AND document.deleted_at IS NULL",
            params![release_id, release.1],
            |row| row.get(0),
        )?;
        let parsed_versions: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT artifact.document_version_id)
             FROM knowledge_document_parse_artifacts artifact
             JOIN knowledge_document_versions version ON version.id = artifact.document_version_id
             JOIN knowledge_documents document ON document.id = version.document_id
             WHERE (version.release_id = ?1 OR EXISTS (
                    SELECT 1 FROM knowledge_document_version_bindings version_binding
                    WHERE version_binding.document_version_id = version.id
                      AND version_binding.cross_version_scope = 'project_all_versions'
             )) AND document.project_id = ?2 AND document.deleted_at IS NULL",
            params![release_id, release.1],
            |row| row.get(0),
        )?;
        let indexed_versions: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT chunk.document_version_id)
             FROM knowledge_chunks chunk
             JOIN knowledge_document_versions version ON version.id = chunk.document_version_id
             JOIN knowledge_documents document ON document.id = version.document_id
             WHERE (version.release_id = ?1 OR EXISTS (
                    SELECT 1 FROM knowledge_document_version_bindings version_binding
                    WHERE version_binding.document_version_id = version.id
                      AND version_binding.cross_version_scope = 'project_all_versions'
             )) AND document.project_id = ?2 AND document.deleted_at IS NULL",
            params![release_id, release.1],
            |row| row.get(0),
        )?;
        let vectorized_versions: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT chunk.document_version_id)
             FROM knowledge_chunk_embeddings embedding
             JOIN knowledge_chunks chunk ON chunk.id = embedding.chunk_id
             JOIN knowledge_document_versions version ON version.id = chunk.document_version_id
             JOIN knowledge_documents document ON document.id = version.document_id
             WHERE (version.release_id = ?1 OR EXISTS (
                    SELECT 1 FROM knowledge_document_version_bindings version_binding
                    WHERE version_binding.document_version_id = version.id
                      AND version_binding.cross_version_scope = 'project_all_versions'
             )) AND document.project_id = ?2 AND document.deleted_at IS NULL",
            params![release_id, release.1],
            |row| row.get(0),
        )?;
        let completed_analysis: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_analysis_runs
             WHERE project_id = ?1 AND release_id = ?2 AND status = 'completed'",
            params![release.1, release_id],
            |row| row.get(0),
        )?;
        let active_graph: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_graph_builds
             WHERE project_id = ?1 AND release_id = ?2 AND status = 'completed' AND is_active = 1",
            params![release.1, release_id],
            |row| row.get(0),
        )?;
        let stages = vec![
            version_stage(
                "repository_capture",
                "仓库采集",
                manifest_ready,
                manifest_total,
                "个仓库清单已冻结",
            ),
            version_stage(
                "document_sync",
                "文档同步",
                document_versions,
                document_versions,
                "个已绑定文档版本",
            ),
            version_stage(
                "parsing",
                "文档解析",
                parsed_versions,
                document_versions,
                "个文档版本已解析",
            ),
            version_stage(
                "indexing",
                "全文索引",
                indexed_versions,
                document_versions,
                "个文档版本已建立索引",
            ),
            version_stage(
                "analysis",
                "代码分析",
                completed_analysis,
                1,
                "次已完成分析",
            ),
            version_stage("graph", "知识图谱", active_graph, 1, "个当前图谱投影"),
            version_stage(
                "vector",
                "向量化",
                vectorized_versions,
                indexed_versions,
                "个文档版本已有本地向量",
            ),
        ];
        let status = if stages.iter().all(|stage| stage.status == "ready") {
            "ready"
        } else {
            "partial"
        };
        Ok(KnowledgeProjectVersionCompleteness {
            release_id: release.0,
            project_id: release.1,
            version: release.2,
            status: status.to_string(),
            stages,
        })
    }
}

fn version_stage(
    stage: &str,
    label: &str,
    completed_count: i64,
    total_count: i64,
    unit: &str,
) -> KnowledgeProjectVersionStageCompleteness {
    let (status, summary) = if total_count <= 0 {
        ("not_started", format!("尚无{unit}"))
    } else if completed_count >= total_count {
        ("ready", format!("{completed_count}/{total_count} {unit}"))
    } else if completed_count > 0 {
        ("partial", format!("{completed_count}/{total_count} {unit}"))
    } else {
        ("pending", format!("等待处理，共 {total_count} {unit}"))
    };
    KnowledgeProjectVersionStageCompleteness {
        stage: stage.to_string(),
        label: label.to_string(),
        status: status.to_string(),
        completed_count,
        total_count,
        summary,
    }
}

fn upsert_binding(
    tx: &rusqlite::Transaction<'_>,
    project_id: i64,
    repository: &RepositoryBindingInput,
) -> Result<(), AppError> {
    let version_strategy = repository.version_strategy.as_str();
    tx.execute(
        "INSERT INTO knowledge_project_repository_bindings
             (project_id, workspace_key, alias, repository_role, default_branch, version_strategy,
              enabled, created_at, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, datetime('now', 'localtime'), datetime('now', 'localtime'), NULL)
         ON CONFLICT(project_id, workspace_key) DO UPDATE SET
             alias = excluded.alias,
             repository_role = excluded.repository_role,
             default_branch = excluded.default_branch,
             version_strategy = excluded.version_strategy,
             enabled = 1,
             updated_at = datetime('now', 'localtime'),
             deleted_at = NULL",
        params![
            project_id,
            repository.workspace_key.trim(),
            repository.alias.as_deref().unwrap_or("").trim(),
            repository.role.as_deref().unwrap_or("service").trim(),
            repository.default_branch.as_deref().unwrap_or("").trim(),
            version_strategy,
        ],
    )?;
    Ok(())
}

fn map_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeRepositoryBinding> {
    let version_strategy: String = row.get(6)?;
    let version_strategy =
        KnowledgeVersionStrategy::from_persisted(&version_strategy).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                Type::Text,
                format!("知识库仓库关联存在未知版本策略: {version_strategy}").into(),
            )
        })?;
    Ok(KnowledgeRepositoryBinding {
        id: row.get(0)?,
        project_id: row.get(1)?,
        workspace_key: row.get(2)?,
        alias: row.get(3)?,
        repository_role: row.get(4)?,
        default_branch: row.get(5)?,
        version_strategy,
        enabled: row.get::<_, i64>(7)? != 0,
        deleted_at: row.get(8)?,
    })
}

fn map_release_manifest(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<KnowledgeReleaseRepositoryManifest> {
    let requested_ref_type: String = row.get(3)?;
    let requested_ref_type =
        KnowledgeGitRefType::from_persisted(&requested_ref_type).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                Type::Text,
                format!("知识版本清单存在未知引用类型: {requested_ref_type}").into(),
            )
        })?;
    Ok(KnowledgeReleaseRepositoryManifest {
        id: row.get(0)?,
        release_id: row.get(1)?,
        repository_binding_id: row.get(2)?,
        requested_ref_type,
        requested_ref_name: row.get(4)?,
        resolved_commit_sha: row.get(5)?,
        capture_kind: row.get(6)?,
        inclusion_status: row.get(7)?,
        exclusion_reason: row.get(8)?,
        worktree_dirty: row.get::<_, i64>(9)? != 0,
        captured_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;

    use super::Database;
    use crate::database::schema;
    use crate::models::{
        KnowledgeRepositoryBindingInput, KnowledgeVersionStrategy, RepositoryBindingInput,
    };

    fn database() -> Result<Database, Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        Ok(Database {
            conn: Mutex::new(connection),
        })
    }

    #[test]
    fn repository_binding_replaces_active_set_without_deleting_history(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let initial = KnowledgeRepositoryBindingInput {
            project_id: 1,
            repositories: vec![RepositoryBindingInput {
                workspace_key: "repo-a".into(),
                alias: Some("服务 A".into()),
                role: None,
                default_branch: Some("main".into()),
                version_strategy: KnowledgeVersionStrategy::TagOrBranch,
            }],
        };
        let first = database.replace_knowledge_project_repository_bindings(&initial)?;
        let repeated = database.replace_knowledge_project_repository_bindings(&initial)?;
        assert_eq!(first.len(), 1);
        assert_eq!(repeated.len(), 1);
        assert_eq!(
            first[0].id, repeated[0].id,
            "重复关联应幂等更新而非插入新行"
        );
        let replacement = KnowledgeRepositoryBindingInput {
            project_id: 1,
            repositories: vec![RepositoryBindingInput {
                workspace_key: "repo-b".into(),
                alias: None,
                role: None,
                default_branch: None,
                version_strategy: KnowledgeVersionStrategy::Manual,
            }],
        };
        let active = database.replace_knowledge_project_repository_bindings(&replacement)?;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].workspace_key, "repo-b");
        let connection = database.conn.lock().map_err(|error| error.to_string())?;
        let historical: i64 = connection.query_row(
            "SELECT COUNT(*) FROM knowledge_project_repository_bindings
             WHERE project_id = 1 AND workspace_key = 'repo-a' AND deleted_at IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(historical, 1);
        Ok(())
    }

    #[test]
    fn version_completeness_keeps_empty_processing_stages_not_started(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            conn.execute(
                "INSERT INTO knowledge_releases (project_id, version, tag_name, branch, commit_sha, description)
                 VALUES (1, 'v1.0.0', '', '', '', '')",
                [],
            )?;
        }
        let completeness = database.get_knowledge_project_version_completeness(1)?;
        assert_eq!(completeness.status, "partial");
        assert!(completeness
            .stages
            .iter()
            .filter(|stage| stage.stage != "analysis" && stage.stage != "graph")
            .all(|stage| stage.status == "not_started"));
        assert!(completeness
            .stages
            .iter()
            .filter(|stage| stage.stage == "analysis" || stage.stage == "graph")
            .all(|stage| stage.status == "pending"));
        Ok(())
    }

    #[test]
    fn version_completeness_counts_project_wide_document_binding(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = database()?;
        let conn = database.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "INSERT INTO knowledge_releases (project_id, version, tag_name, branch, commit_sha, description)
             VALUES (1, 'v1.0.0', '', '', '', '')",
            [],
        )?;
        conn.execute(
            "INSERT INTO knowledge_documents
                (document_key, project_id, source_id, doc_type, title, logical_path, status,
                 sensitivity, tags_json, allow_ai, allow_mcp)
             VALUES ('shared-doc', 1, NULL, 'markdown', '通用说明', 'docs/shared.md', 'active',
                     'internal', '[]', 1, 0)",
            [],
        )?;
        conn.execute(
            "INSERT INTO knowledge_document_versions
                (document_id, release_id, version_label, git_branch, commit_sha, source_path,
                 mime_type, content, content_hash, parsed_meta_json, token_estimate, valid)
             VALUES (1, NULL, '通用版本', '', '', 'docs/shared.md', 'text/markdown', '通用正文',
                     'shared-hash', '{}', 1, 1)",
            [],
        )?;
        conn.execute(
            "UPDATE knowledge_documents SET latest_version_id = 1 WHERE id = 1",
            [],
        )?;
        conn.execute(
            "INSERT INTO knowledge_document_version_bindings
                (document_version_id, release_id, repository_binding_id, cross_version_scope)
             VALUES (1, NULL, NULL, 'project_all_versions')",
            [],
        )?;
        drop(conn);

        let completeness = database.get_knowledge_project_version_completeness(1)?;
        let document_sync = completeness
            .stages
            .iter()
            .find(|stage| stage.stage == "document_sync")
            .expect("文档同步阶段必须存在");
        assert_eq!(document_sync.completed_count, 1);
        assert_eq!(document_sync.total_count, 1);
        Ok(())
    }
}
