use rusqlite::{params, OptionalExtension};

use crate::database::Database;
use crate::error::AppError;

pub(crate) const DOMAIN: &str = "graph";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewKnowledgeGraphBuild {
    pub build_key: String,
    pub project_id: i64,
    pub release_id: i64,
    pub projection_version: String,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeGraphBuildRecord {
    pub id: i64,
    pub build_key: String,
    pub project_id: i64,
    pub release_id: i64,
    pub projection_version: String,
    pub source_hash: String,
    pub status: String,
    pub is_active: bool,
    pub checkpoint_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewKnowledgeGraphNode {
    pub entity_type: String,
    pub entity_key: String,
    pub label: String,
    pub metadata_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewKnowledgeGraphEdge {
    pub from_node_id: i64,
    pub relation_type: String,
    pub to_node_id: i64,
    pub evidence_ref: String,
    pub confidence: f64,
    pub confirmed: bool,
    pub source_relation_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeGraphSourceDocument {
    pub document_id: i64,
    pub document_version_id: i64,
    pub title: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KnowledgeGraphSourceRelation {
    pub id: i64,
    pub from_type: String,
    pub from_key: String,
    pub relation_type: String,
    pub to_type: String,
    pub to_key: String,
    pub evidence: serde_json::Value,
    pub confidence: f64,
    pub confirmed: bool,
    pub document_version_id: Option<i64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnowledgeGraphNodeRecord {
    pub id: i64,
    pub entity_type: String,
    pub entity_key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KnowledgeGraphEdgeRecord {
    pub id: i64,
    pub from_node_id: i64,
    pub relation_type: String,
    pub to_node_id: i64,
    pub evidence_ref: String,
    pub confidence: f64,
    pub confirmed: bool,
    pub source_relation_ref: String,
}

impl Database {
    /// 只取当前项目版本可见的文档版本。这里复用正式文档的版本绑定语义，避免项目全局
    /// 文档或另一版本的文档在图谱中串入当前版本。
    pub(crate) fn list_knowledge_graph_source_documents(
        &self,
        project_id: i64,
        release_id: i64,
    ) -> Result<Vec<KnowledgeGraphSourceDocument>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT d.id, v.id, d.title, v.content_hash
             FROM knowledge_documents d
             JOIN knowledge_document_versions v ON v.id = (
                 SELECT candidate.id
                 FROM knowledge_document_versions candidate
                 WHERE candidate.document_id = d.id AND candidate.valid = 1
                   AND EXISTS (
                        SELECT 1 FROM knowledge_document_version_bindings binding
                        WHERE binding.document_version_id = candidate.id
                          AND (binding.release_id = ?2
                               OR binding.cross_version_scope = 'project_all_versions')
                   )
                 ORDER BY CASE WHEN EXISTS (
                              SELECT 1 FROM knowledge_document_version_bindings exact_binding
                              WHERE exact_binding.document_version_id = candidate.id
                                AND exact_binding.release_id = ?2
                          ) THEN 0 ELSE 1 END,
                          candidate.id DESC
                 LIMIT 1
             )
             WHERE d.project_id = ?1
               AND d.deleted_at IS NULL AND d.status = 'active'
               AND d.sensitivity != 'restricted' AND d.allow_ai = 1
             ORDER BY d.id",
        )?;
        let documents = statement
            .query_map(params![project_id, release_id], |row| {
                Ok(KnowledgeGraphSourceDocument {
                    document_id: row.get(0)?,
                    document_version_id: row.get(1)?,
                    title: row.get(2)?,
                    content_hash: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(documents)
    }

    /// 关系的项目、版本和文档版本必须彼此一致。历史关系表没有外键，因此读取端也要
    /// 防御性验证文档真实归属和版本可见性，不能只相信关系行里可被旧数据污染的 ID。
    pub(crate) fn list_knowledge_graph_source_relations(
        &self,
        project_id: i64,
        release_id: i64,
        include_unconfirmed: bool,
    ) -> Result<Vec<KnowledgeGraphSourceRelation>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT r.id, r.from_type, r.from_key, r.relation_type, r.to_type, r.to_key,
                    r.evidence_json, r.confidence, r.confirmed, r.document_version_id, r.updated_at
             FROM knowledge_relations r
             WHERE r.project_id = ?1 AND r.deleted_at IS NULL AND r.scope_status = 'scoped'
               AND r.sensitivity != 'restricted'
               AND (?3 = 1 OR r.confirmed = 1)
               AND (r.snapshot_id = 0 OR EXISTS (
                    SELECT 1 FROM knowledge_code_snapshots snapshot
                    WHERE snapshot.id = r.snapshot_id AND snapshot.status = 'analyzed'
               ))
               AND (r.release_id = 0 OR r.release_id = ?2)
               AND (r.release_id = ?2 OR r.document_version_id != 0)
               AND (r.document_version_id = 0 OR EXISTS (
                    SELECT 1
                    FROM knowledge_document_versions document_version
                    JOIN knowledge_documents document ON document.id = document_version.document_id
                    WHERE document_version.id = r.document_version_id
                      AND document.project_id = r.project_id
                      AND document.deleted_at IS NULL AND document_version.valid = 1
                      AND EXISTS (
                           SELECT 1 FROM knowledge_document_version_bindings binding
                           WHERE binding.document_version_id = document_version.id
                             AND (binding.release_id = ?2
                                  OR binding.cross_version_scope = 'project_all_versions')
                      )
               ))
             ORDER BY r.id",
        )?;
        let relations = statement
            .query_map(
                params![project_id, release_id, include_unconfirmed as i64],
                |row| {
                    let evidence_json: String = row.get(6)?;
                    let evidence = serde_json::from_str(&evidence_json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    let version_id: i64 = row.get(9)?;
                    Ok(KnowledgeGraphSourceRelation {
                        id: row.get(0)?,
                        from_type: row.get(1)?,
                        from_key: row.get(2)?,
                        relation_type: row.get(3)?,
                        to_type: row.get(4)?,
                        to_key: row.get(5)?,
                        evidence,
                        confidence: row.get(7)?,
                        confirmed: row.get::<_, i64>(8)? != 0,
                        document_version_id: (version_id > 0).then_some(version_id),
                        updated_at: row.get(10)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(relations)
    }

    /// 新投影默认不可见。只有完整构建后才可通过 activate 原子切换，避免用户看到半图谱。
    pub(crate) fn create_knowledge_graph_build(
        &self,
        build: &NewKnowledgeGraphBuild,
    ) -> Result<KnowledgeGraphBuildRecord, AppError> {
        if build.build_key.trim().is_empty()
            || build.project_id <= 0
            || build.release_id <= 0
            || build.projection_version.trim().is_empty()
            || build.source_hash.trim().is_empty()
        {
            return Err(AppError::InvalidInput(
                "图谱构建缺少项目、版本或投影标识".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        conn.execute(
            "INSERT INTO knowledge_graph_builds
                (build_key, project_id, release_id, projection_version, source_hash)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(build_key) DO NOTHING",
            params![
                build.build_key.trim(),
                build.project_id,
                build.release_id,
                build.projection_version.trim(),
                build.source_hash.trim(),
            ],
        )?;
        get_graph_build_by_key(&conn, build.build_key.trim())?
            .ok_or_else(|| AppError::Custom("创建图谱构建后未找到记录".to_string()))
    }

    pub(crate) fn get_knowledge_graph_build_by_key(
        &self,
        build_key: &str,
    ) -> Result<Option<KnowledgeGraphBuildRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        get_graph_build_by_key(&conn, build_key)
    }

    pub(crate) fn list_knowledge_graph_nodes_for_build(
        &self,
        graph_build_id: i64,
    ) -> Result<Vec<KnowledgeGraphNodeRecord>, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let mut statement = conn.prepare(
            "SELECT id, entity_type, entity_key, label
             FROM knowledge_graph_nodes WHERE graph_build_id = ?1 ORDER BY id",
        )?;
        let nodes = statement
            .query_map([graph_build_id], |row| {
                Ok(KnowledgeGraphNodeRecord {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    entity_key: row.get(2)?,
                    label: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(nodes)
    }

    pub(crate) fn get_active_knowledge_graph_projection(
        &self,
        project_id: i64,
        release_id: i64,
    ) -> Result<
        Option<(
            KnowledgeGraphBuildRecord,
            Vec<KnowledgeGraphNodeRecord>,
            Vec<KnowledgeGraphEdgeRecord>,
        )>,
        AppError,
    > {
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let build = conn
            .query_row(
                "SELECT id, build_key, project_id, release_id, projection_version, source_hash,
                        status, is_active, checkpoint_json
                 FROM knowledge_graph_builds
                 WHERE project_id = ?1 AND release_id = ?2 AND is_active = 1 AND status = 'completed'",
                params![project_id, release_id],
                |row| {
                    Ok(KnowledgeGraphBuildRecord {
                        id: row.get(0)?,
                        build_key: row.get(1)?,
                        project_id: row.get(2)?,
                        release_id: row.get(3)?,
                        projection_version: row.get(4)?,
                        source_hash: row.get(5)?,
                        status: row.get(6)?,
                        is_active: row.get::<_, i64>(7)? != 0,
                        checkpoint_json: row.get(8)?,
                    })
                },
            )
            .optional()?;
        let Some(build) = build else { return Ok(None) };
        let nodes = conn
            .prepare(
                "SELECT id, entity_type, entity_key, label
                 FROM knowledge_graph_nodes WHERE graph_build_id = ?1 ORDER BY id",
            )?
            .query_map([build.id], |row| {
                Ok(KnowledgeGraphNodeRecord {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    entity_key: row.get(2)?,
                    label: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let edges = conn
            .prepare(
                "SELECT id, from_node_id, relation_type, to_node_id, evidence_ref, confidence,
                        confirmed, source_relation_ref
                 FROM knowledge_graph_edges WHERE graph_build_id = ?1 ORDER BY id",
            )?
            .query_map([build.id], |row| {
                Ok(KnowledgeGraphEdgeRecord {
                    id: row.get(0)?,
                    from_node_id: row.get(1)?,
                    relation_type: row.get(2)?,
                    to_node_id: row.get(3)?,
                    evidence_ref: row.get(4)?,
                    confidence: row.get(5)?,
                    confirmed: row.get::<_, i64>(6)? != 0,
                    source_relation_ref: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some((build, nodes, edges)))
    }

    pub(crate) fn replace_knowledge_graph_projection(
        &self,
        graph_build_id: i64,
        nodes: &[NewKnowledgeGraphNode],
        edges: &[NewKnowledgeGraphEdge],
        checkpoint_json: &str,
    ) -> Result<(), AppError> {
        if graph_build_id <= 0 {
            return Err(AppError::InvalidInput("图谱构建 ID 必须为正数".to_string()));
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_graph_builds WHERE id = ?1)",
            [graph_build_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AppError::NotFound(format!(
                "图谱构建不存在: {graph_build_id}"
            )));
        }
        tx.execute(
            "DELETE FROM knowledge_graph_edges WHERE graph_build_id = ?1",
            [graph_build_id],
        )?;
        tx.execute(
            "DELETE FROM knowledge_graph_nodes WHERE graph_build_id = ?1",
            [graph_build_id],
        )?;
        for node in nodes {
            if node.entity_type.trim().is_empty()
                || node.entity_key.trim().is_empty()
                || node.label.trim().is_empty()
                || node.metadata_hash.trim().is_empty()
            {
                return Err(AppError::InvalidInput(
                    "图谱节点缺少稳定实体或标签".to_string(),
                ));
            }
            tx.execute(
                "INSERT INTO knowledge_graph_nodes
                    (graph_build_id, entity_type, entity_key, label, metadata_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    graph_build_id,
                    node.entity_type.trim(),
                    node.entity_key.trim(),
                    node.label.trim(),
                    node.metadata_hash.trim()
                ],
            )?;
        }
        for edge in edges {
            if edge.from_node_id <= 0
                || edge.to_node_id <= 0
                || edge.relation_type.trim().is_empty()
                || edge.evidence_ref.trim().is_empty()
                || !(0.0..=1.0).contains(&edge.confidence)
            {
                return Err(AppError::InvalidInput(
                    "图谱关系边缺少证据或置信度无效".to_string(),
                ));
            }
            let endpoint_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM knowledge_graph_nodes
                 WHERE graph_build_id = ?1 AND id IN (?2, ?3)",
                params![graph_build_id, edge.from_node_id, edge.to_node_id],
                |row| row.get(0),
            )?;
            if endpoint_count != 2 {
                return Err(AppError::InvalidInput(
                    "图谱关系边必须连接当前构建中的两个节点".to_string(),
                ));
            }
            tx.execute(
                "INSERT INTO knowledge_graph_edges
                    (graph_build_id, from_node_id, relation_type, to_node_id, evidence_ref,
                     confidence, confirmed, source_relation_ref)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    graph_build_id,
                    edge.from_node_id,
                    edge.relation_type.trim(),
                    edge.to_node_id,
                    edge.evidence_ref.trim(),
                    edge.confidence,
                    edge.confirmed as i64,
                    edge.source_relation_ref.trim(),
                ],
            )?;
        }
        tx.execute(
            "UPDATE knowledge_graph_builds
             SET checkpoint_json = ?2, status = 'completed', finished_at = datetime('now', 'localtime')
             WHERE id = ?1",
            params![graph_build_id, checkpoint_json],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn activate_knowledge_graph_build(
        &self,
        graph_build_id: i64,
    ) -> Result<(), AppError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let tx = conn.transaction()?;
        let (project_id, release_id, status): (i64, i64, String) = tx
            .query_row(
                "SELECT project_id, release_id, status FROM knowledge_graph_builds WHERE id = ?1",
                [graph_build_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("图谱构建不存在: {graph_build_id}")))?;
        if status != "completed" {
            return Err(AppError::InvalidInput(
                "只有已完成的图谱构建可以启用".to_string(),
            ));
        }
        tx.execute(
            "UPDATE knowledge_graph_builds SET is_active = 0
             WHERE project_id = ?1 AND release_id = ?2 AND is_active = 1",
            params![project_id, release_id],
        )?;
        tx.execute(
            "UPDATE knowledge_graph_builds SET is_active = 1 WHERE id = ?1",
            [graph_build_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 增量构建器可在已写入节点后补充关系；端点仍必须属于同一图谱构建，避免跨版本串边。
    pub(crate) fn insert_knowledge_graph_edge(
        &self,
        graph_build_id: i64,
        edge: &NewKnowledgeGraphEdge,
    ) -> Result<i64, AppError> {
        if graph_build_id <= 0
            || edge.from_node_id <= 0
            || edge.to_node_id <= 0
            || edge.relation_type.trim().is_empty()
            || edge.evidence_ref.trim().is_empty()
            || !(0.0..=1.0).contains(&edge.confidence)
        {
            return Err(AppError::InvalidInput(
                "图谱关系边缺少证据或置信度无效".to_string(),
            ));
        }
        let conn = self
            .conn
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let endpoint_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_graph_nodes
             WHERE graph_build_id = ?1 AND id IN (?2, ?3)",
            params![graph_build_id, edge.from_node_id, edge.to_node_id],
            |row| row.get(0),
        )?;
        if endpoint_count != 2 {
            return Err(AppError::InvalidInput(
                "图谱关系边必须连接当前构建中的两个节点".to_string(),
            ));
        }
        conn.execute(
            "INSERT INTO knowledge_graph_edges
                (graph_build_id, from_node_id, relation_type, to_node_id, evidence_ref,
                 confidence, confirmed, source_relation_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(graph_build_id, from_node_id, relation_type, to_node_id, evidence_ref)
             DO UPDATE SET confidence = excluded.confidence, confirmed = excluded.confirmed,
                 source_relation_ref = excluded.source_relation_ref",
            params![
                graph_build_id,
                edge.from_node_id,
                edge.relation_type.trim(),
                edge.to_node_id,
                edge.evidence_ref.trim(),
                edge.confidence,
                edge.confirmed as i64,
                edge.source_relation_ref.trim(),
            ],
        )?;
        conn.query_row(
            "SELECT id FROM knowledge_graph_edges
             WHERE graph_build_id = ?1 AND from_node_id = ?2 AND relation_type = ?3
               AND to_node_id = ?4 AND evidence_ref = ?5",
            params![
                graph_build_id,
                edge.from_node_id,
                edge.relation_type.trim(),
                edge.to_node_id,
                edge.evidence_ref.trim(),
            ],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }
}

fn get_graph_build_by_key(
    conn: &rusqlite::Connection,
    build_key: &str,
) -> Result<Option<KnowledgeGraphBuildRecord>, AppError> {
    conn.query_row(
        "SELECT id, build_key, project_id, release_id, projection_version, source_hash,
                status, is_active, checkpoint_json
         FROM knowledge_graph_builds WHERE build_key = ?1",
        [build_key],
        |row| {
            Ok(KnowledgeGraphBuildRecord {
                id: row.get(0)?,
                build_key: row.get(1)?,
                project_id: row.get(2)?,
                release_id: row.get(3)?,
                projection_version: row.get(4)?,
                source_hash: row.get(5)?,
                status: row.get(6)?,
                is_active: row.get::<_, i64>(7)? != 0,
                checkpoint_json: row.get(8)?,
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

    use super::{Database, NewKnowledgeGraphBuild, NewKnowledgeGraphEdge, NewKnowledgeGraphNode};
    use crate::database::schema;

    #[test]
    fn completed_graph_projection_activates_without_exposing_half_build(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::open_in_memory()?;
        schema::migrate(&connection)?;
        let database = Database {
            conn: Mutex::new(connection),
        };
        let first = database.create_knowledge_graph_build(&NewKnowledgeGraphBuild {
            build_key: "graph:1:2:a".into(),
            project_id: 1,
            release_id: 2,
            projection_version: "v1".into(),
            source_hash: "source-a".into(),
        })?;
        assert!(database.activate_knowledge_graph_build(first.id).is_err());
        database.replace_knowledge_graph_projection(
            first.id,
            &[
                NewKnowledgeGraphNode {
                    entity_type: "document".into(),
                    entity_key: "document:1".into(),
                    label: "设计文档".into(),
                    metadata_hash: "n1".into(),
                },
                NewKnowledgeGraphNode {
                    entity_type: "api".into(),
                    entity_key: "api:1".into(),
                    label: "查询接口".into(),
                    metadata_hash: "n2".into(),
                },
            ],
            &[],
            "{\"phase\":\"nodes\"}",
        )?;
        let node_ids = {
            let conn = database.conn.lock().map_err(|error| error.to_string())?;
            let mut statement = conn.prepare(
                "SELECT id FROM knowledge_graph_nodes WHERE graph_build_id = ?1 ORDER BY id",
            )?;
            let node_ids = statement
                .query_map([first.id], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            node_ids
        };
        let edge_id = database.insert_knowledge_graph_edge(
            first.id,
            &NewKnowledgeGraphEdge {
                from_node_id: node_ids[0],
                relation_type: "implements".into(),
                to_node_id: node_ids[1],
                evidence_ref: "document:1#section:1".into(),
                confidence: 1.0,
                confirmed: true,
                source_relation_ref: "relation:1".into(),
            },
        )?;
        assert!(edge_id > 0);
        database.activate_knowledge_graph_build(first.id)?;
        let active: i64 = database
            .conn
            .lock()
            .map_err(|error| error.to_string())?
            .query_row(
                "SELECT is_active FROM knowledge_graph_builds WHERE id = ?1",
                [first.id],
                |row| row.get(0),
            )?;
        assert_eq!(active, 1);
        Ok(())
    }
}
