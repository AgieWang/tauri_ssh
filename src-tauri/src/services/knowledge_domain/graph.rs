use std::collections::{BTreeMap, BTreeSet, VecDeque};

use sha2::{Digest, Sha256};

use crate::database::knowledge_domain::graph::{
    KnowledgeGraphEdgeRecord, KnowledgeGraphNodeRecord, KnowledgeGraphSourceDocument,
    KnowledgeGraphSourceRelation, NewKnowledgeGraphBuild, NewKnowledgeGraphEdge,
    NewKnowledgeGraphNode,
};
use crate::database::Database;
use crate::error::AppError;
use crate::models::knowledge_domain::graph::{
    KnowledgeGraphBuildInput, KnowledgeGraphBuildResult, KnowledgeGraphEdge, KnowledgeGraphNode,
    KnowledgeGraphProjection, KnowledgeGraphQueryInput,
};
use crate::services::knowledge::validate_positive_id;
use crate::services::knowledge_rollout::KnowledgeRolloutService;

pub(crate) const DOMAIN: &str = "graph";

const PROJECTION_VERSION: &str = "documents-explicit-relations-v1";
const MAX_DEPTH: u8 = 4;
const MAX_NODE_LIMIT: u32 = 300;

/// 本地图谱只投影已经入库、可见且具备明确证据的关系。它不会用模型推断正文含义，
/// 因而用户能够从每条边回溯到原关系证据，并且不会因为未授权的远程调用泄露内容。
pub struct KnowledgeGraphService;

impl KnowledgeGraphService {
    pub fn build(
        db: &Database,
        input: KnowledgeGraphBuildInput,
    ) -> Result<KnowledgeGraphBuildResult, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_scope(db, input.project_id, input.project_version_id)?;

        let documents =
            db.list_knowledge_graph_source_documents(input.project_id, input.project_version_id)?;
        let relations = db.list_knowledge_graph_source_relations(
            input.project_id,
            input.project_version_id,
            input.include_unconfirmed,
        )?;
        let source_hash = source_hash(&documents, &relations, input.include_unconfirmed)?;
        let build_key = format!(
            "graph:{}:{}:{}",
            input.project_id, input.project_version_id, source_hash
        );

        if let Some(existing) = db.get_knowledge_graph_build_by_key(&build_key)? {
            if existing.is_active && existing.status == "completed" {
                let (_, nodes, edges) = db
                    .get_active_knowledge_graph_projection(
                        input.project_id,
                        input.project_version_id,
                    )?
                    .ok_or_else(|| AppError::Custom("当前图谱投影缺少已启用构建".to_string()))?;
                return Ok(KnowledgeGraphBuildResult {
                    build_id: existing.id,
                    build_key,
                    project_id: input.project_id,
                    project_version_id: input.project_version_id,
                    node_count: saturating_count(nodes.len()),
                    edge_count: saturating_count(edges.len()),
                    reused: true,
                });
            }
        }

        let (nodes, relation_edges) = projection_inputs(&documents, &relations)?;
        let build = db.create_knowledge_graph_build(&NewKnowledgeGraphBuild {
            build_key: build_key.clone(),
            project_id: input.project_id,
            release_id: input.project_version_id,
            projection_version: PROJECTION_VERSION.to_string(),
            source_hash,
        })?;
        // 先写入完整节点集合但不启用该构建；任意后续失败都不会影响上一版已启用投影。
        let checkpoint = serde_json::json!({
            "source": "local_documents_and_explicit_relations",
            "nodeCount": nodes.len(),
            "relationCount": relation_edges.len(),
            "includeUnconfirmed": input.include_unconfirmed,
        });
        db.replace_knowledge_graph_projection(
            build.id,
            &nodes,
            &[],
            &serde_json::to_string(&checkpoint)?,
        )?;
        let node_ids = db
            .list_knowledge_graph_nodes_for_build(build.id)?
            .into_iter()
            .map(|node| ((node.entity_type, node.entity_key), node.id))
            .collect::<BTreeMap<_, _>>();
        for edge in &relation_edges {
            let from_node_id = node_ids
                .get(&(edge.from_type.clone(), edge.from_key.clone()))
                .copied()
                .ok_or_else(|| AppError::Custom("图谱起点节点未写入".to_string()))?;
            let to_node_id = node_ids
                .get(&(edge.to_type.clone(), edge.to_key.clone()))
                .copied()
                .ok_or_else(|| AppError::Custom("图谱终点节点未写入".to_string()))?;
            db.insert_knowledge_graph_edge(
                build.id,
                &NewKnowledgeGraphEdge {
                    from_node_id,
                    relation_type: edge.relation_type.clone(),
                    to_node_id,
                    evidence_ref: serde_json::to_string(&serde_json::json!({
                        "relationId": edge.id,
                        "documentVersionId": edge.document_version_id,
                        "evidence": edge.evidence,
                    }))?,
                    confidence: edge.confidence,
                    confirmed: edge.confirmed,
                    source_relation_ref: format!("relation:{}", edge.id),
                },
            )?;
        }
        db.activate_knowledge_graph_build(build.id)?;
        Ok(KnowledgeGraphBuildResult {
            build_id: build.id,
            build_key,
            project_id: input.project_id,
            project_version_id: input.project_version_id,
            node_count: saturating_count(nodes.len()),
            edge_count: saturating_count(relation_edges.len()),
            reused: false,
        })
    }

    pub fn query(
        db: &Database,
        mut input: KnowledgeGraphQueryInput,
    ) -> Result<KnowledgeGraphProjection, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_scope(db, input.project_id, input.project_version_id)?;
        input.depth = input.depth.clamp(0, MAX_DEPTH);
        input.node_limit = input.node_limit.clamp(1, MAX_NODE_LIMIT);
        let root = input
            .root_entity_key
            .take()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let root_type = input
            .root_entity_type
            .take()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let (build, nodes, edges) = db
            .get_active_knowledge_graph_projection(input.project_id, input.project_version_id)?
            .ok_or_else(|| {
                AppError::NotFound("当前项目版本还没有已生成的知识图谱，请先生成图谱".to_string())
            })?;
        project_subgraph(
            build,
            nodes,
            edges,
            root.as_deref(),
            root_type.as_deref(),
            input.depth,
            input.node_limit,
            input.include_unconfirmed,
        )
    }
}

#[derive(Debug, Clone)]
struct ProjectionRelation {
    id: i64,
    from_type: String,
    from_key: String,
    relation_type: String,
    to_type: String,
    to_key: String,
    evidence: serde_json::Value,
    confidence: f64,
    confirmed: bool,
    document_version_id: Option<i64>,
}

fn validate_scope(db: &Database, project_id: i64, release_id: i64) -> Result<(), AppError> {
    validate_positive_id(project_id, "项目 ID")?;
    validate_positive_id(release_id, "项目版本 ID")?;
    if !db.knowledge_project_exists(project_id)? {
        return Err(AppError::NotFound(format!("知识项目不存在: {project_id}")));
    }
    let release = db
        .get_knowledge_release_by_id(release_id)?
        .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
    if release.project_id != project_id {
        return Err(AppError::InvalidInput(
            "项目版本不属于当前项目，不能生成或查看跨项目图谱".to_string(),
        ));
    }
    Ok(())
}

fn projection_inputs(
    documents: &[KnowledgeGraphSourceDocument],
    relations: &[KnowledgeGraphSourceRelation],
) -> Result<(Vec<NewKnowledgeGraphNode>, Vec<ProjectionRelation>), AppError> {
    let mut labels = BTreeMap::<(String, String), String>::new();
    for document in documents {
        labels.insert(
            ("document".to_string(), document.document_id.to_string()),
            document.title.clone(),
        );
    }
    let mut projection_relations = Vec::with_capacity(relations.len());
    for relation in relations {
        labels
            .entry((relation.from_type.clone(), relation.from_key.clone()))
            .or_insert_with(|| format!("{}: {}", relation.from_type, relation.from_key));
        labels
            .entry((relation.to_type.clone(), relation.to_key.clone()))
            .or_insert_with(|| format!("{}: {}", relation.to_type, relation.to_key));
        projection_relations.push(ProjectionRelation {
            id: relation.id,
            from_type: relation.from_type.clone(),
            from_key: relation.from_key.clone(),
            relation_type: relation.relation_type.clone(),
            to_type: relation.to_type.clone(),
            to_key: relation.to_key.clone(),
            evidence: relation.evidence.clone(),
            confidence: relation.confidence,
            confirmed: relation.confirmed,
            document_version_id: relation.document_version_id,
        });
    }
    let nodes = labels
        .into_iter()
        .map(|((entity_type, entity_key), label)| {
            let metadata_hash = format!(
                "{:x}",
                Sha256::digest(format!("{entity_type}\u{0}{entity_key}\u{0}{label}").as_bytes())
            );
            NewKnowledgeGraphNode {
                entity_type,
                entity_key,
                label,
                metadata_hash,
            }
        })
        .collect();
    Ok((nodes, projection_relations))
}

fn source_hash(
    documents: &[KnowledgeGraphSourceDocument],
    relations: &[KnowledgeGraphSourceRelation],
    include_unconfirmed: bool,
) -> Result<String, AppError> {
    let value = serde_json::json!({
        "projectionVersion": PROJECTION_VERSION,
        "includeUnconfirmed": include_unconfirmed,
        "documents": documents.iter().map(|document| serde_json::json!({
            "documentId": document.document_id,
            "documentVersionId": document.document_version_id,
            "contentHash": document.content_hash,
            "title": document.title,
        })).collect::<Vec<_>>(),
        "relations": relations.iter().map(|relation| serde_json::json!({
            "id": relation.id,
            "fromType": relation.from_type,
            "fromKey": relation.from_key,
            "relationType": relation.relation_type,
            "toType": relation.to_type,
            "toKey": relation.to_key,
            "evidence": relation.evidence,
            "confidence": relation.confidence,
            "confirmed": relation.confirmed,
            "documentVersionId": relation.document_version_id,
            "updatedAt": relation.updated_at,
        })).collect::<Vec<_>>(),
    });
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value)?.as_slice())
    ))
}

fn project_subgraph(
    build: crate::database::knowledge_domain::graph::KnowledgeGraphBuildRecord,
    nodes: Vec<KnowledgeGraphNodeRecord>,
    edges: Vec<KnowledgeGraphEdgeRecord>,
    root: Option<&str>,
    root_type: Option<&str>,
    depth: u8,
    node_limit: u32,
    include_unconfirmed: bool,
) -> Result<KnowledgeGraphProjection, AppError> {
    let nodes_by_id = nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<BTreeMap<_, _>>();
    let visible_edges = edges
        .into_iter()
        .filter(|edge| include_unconfirmed || edge.confirmed)
        .collect::<Vec<_>>();
    let roots = if let Some(root) = root {
        let roots = nodes
            .iter()
            .filter(|node| {
                node.entity_key == root
                    && root_type
                        .map(|entity_type| node.entity_type == entity_type)
                        .unwrap_or(true)
            })
            .map(|node| node.id)
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return Err(AppError::NotFound(format!("当前图谱未找到实体: {root}")));
        }
        roots
    } else {
        nodes.iter().map(|node| node.id).collect()
    };
    let mut selected = BTreeSet::new();
    let mut queue = VecDeque::new();
    for root_id in roots {
        if selected.len() >= node_limit as usize {
            break;
        }
        selected.insert(root_id);
        queue.push_back((root_id, 0_u8));
    }
    while let Some((node_id, current_depth)) = queue.pop_front() {
        if current_depth >= depth || selected.len() >= node_limit as usize {
            continue;
        }
        for edge in &visible_edges {
            let neighbor = if edge.from_node_id == node_id {
                Some(edge.to_node_id)
            } else if edge.to_node_id == node_id {
                Some(edge.from_node_id)
            } else {
                None
            };
            if let Some(neighbor) = neighbor {
                if selected.len() < node_limit as usize && selected.insert(neighbor) {
                    queue.push_back((neighbor, current_depth + 1));
                }
            }
        }
    }
    let projected_nodes = selected
        .iter()
        .filter_map(|node_id| nodes_by_id.get(node_id))
        .map(|node| KnowledgeGraphNode {
            id: node.id,
            entity_type: node.entity_type.clone(),
            entity_key: node.entity_key.clone(),
            label: node.label.clone(),
        })
        .collect::<Vec<_>>();
    let projected_edges = visible_edges
        .iter()
        .filter(|edge| selected.contains(&edge.from_node_id) && selected.contains(&edge.to_node_id))
        .map(graph_edge)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(KnowledgeGraphProjection {
        build_id: build.id,
        build_key: build.build_key,
        project_id: build.project_id,
        project_version_id: build.release_id,
        truncated: projected_nodes.len() < nodes.len(),
        nodes: projected_nodes,
        edges: projected_edges,
    })
}

fn graph_edge(edge: &KnowledgeGraphEdgeRecord) -> Result<KnowledgeGraphEdge, AppError> {
    let evidence = serde_json::from_str(&edge.evidence_ref).map_err(|_| {
        AppError::Custom("图谱边证据已损坏，请重新生成该项目版本的图谱".to_string())
    })?;
    Ok(KnowledgeGraphEdge {
        id: edge.id,
        from_node_id: edge.from_node_id,
        relation_type: edge.relation_type.clone(),
        to_node_id: edge.to_node_id,
        evidence,
        confidence: edge.confidence,
        confirmed: edge.confirmed,
        source_relation_ref: edge.source_relation_ref.clone(),
    })
}

fn saturating_count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{KnowledgeGraphBuildInput, KnowledgeGraphQueryInput, KnowledgeGraphService};
    use crate::database::Database;
    use crate::models::{
        CreateKnowledgeDocumentVersionInput, KnowledgeChunkWriteInput,
        UpsertKnowledgeDocumentInput, UpsertKnowledgeProjectInput, UpsertKnowledgeRelationInput,
        UpsertKnowledgeReleaseInput,
    };
    use crate::services::knowledge::KnowledgeService;

    fn setup_scope() -> Result<(Database, i64, i64), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "orders".to_string(),
            name: "订单中心".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: String::new(),
            branch: "main".to_string(),
            commit_sha: "a".repeat(40),
            description: String::new(),
            released_at: None,
        })?;
        Ok((database, project.id, release.id))
    }

    #[test]
    fn builds_scoped_projection_reuses_same_source_and_keeps_evidence(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (database, project_id, release_id) = setup_scope()?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "orders-api".to_string(),
            project_id: Some(project_id),
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "订单接口说明".to_string(),
            logical_path: "docs/orders.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: Some(release_id),
                version_label: "v1".to_string(),
                git_branch: "main".to_string(),
                commit_sha: "a".repeat(40),
                source_path: "docs/orders.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# 订单接口".to_string(),
                content_hash: "document-v1".to_string(),
                parsed_meta: json!({}),
                token_estimate: 4,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "订单接口".to_string(),
                content: "创建订单".to_string(),
                content_hash: "chunk-v1".to_string(),
                location: json!({}),
                token_estimate: 2,
            }],
        )?;
        database.upsert_knowledge_relation(&UpsertKnowledgeRelationInput {
            id: None,
            project_id: Some(project_id),
            release_id: Some(release_id),
            document_version_id: Some(version.id),
            snapshot_id: None,
            sensitivity: "internal".to_string(),
            from_type: "document".to_string(),
            from_key: document.id.to_string(),
            relation_type: "describes".to_string(),
            to_type: "api".to_string(),
            to_key: "create-order".to_string(),
            evidence: json!({"kind": "front_matter", "documentVersionId": version.id}),
            confidence: 1.0,
            confirmed: true,
            source: "front_matter".to_string(),
        })?;

        let first = KnowledgeGraphService::build(
            &database,
            KnowledgeGraphBuildInput {
                project_id,
                project_version_id: release_id,
                include_unconfirmed: false,
            },
        )?;
        assert_eq!((first.node_count, first.edge_count), (2, 1));
        assert!(!first.reused);

        let graph = KnowledgeGraphService::query(
            &database,
            KnowledgeGraphQueryInput {
                project_id,
                project_version_id: release_id,
                root_entity_key: Some(document.id.to_string()),
                root_entity_type: Some("document".to_string()),
                depth: 1,
                node_limit: 10,
                include_unconfirmed: false,
            },
        )?;
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].evidence["documentVersionId"], version.id);
        assert!(
            graph.nodes.iter().any(|node| node.label == "订单接口说明"),
            "文档节点应保留可读标题"
        );

        // 后续版本会更新 documents.latest_version_id；重建 v1 时仍必须从 v1 的绑定版本
        // 读取，不能让最新版本覆盖或挤掉历史投影。
        let second_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id,
            version: "v2.0.0".to_string(),
            tag_name: String::new(),
            branch: "main".to_string(),
            commit_sha: "b".repeat(40),
            description: String::new(),
            released_at: None,
        })?;
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: Some(second_release.id),
                version_label: "v2".to_string(),
                git_branch: "main".to_string(),
                commit_sha: "b".repeat(40),
                source_path: "docs/orders.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# 订单接口 v2".to_string(),
                content_hash: "document-v2".to_string(),
                parsed_meta: json!({}),
                token_estimate: 5,
            },
            &[],
        )?;

        let reused = KnowledgeGraphService::build(
            &database,
            KnowledgeGraphBuildInput {
                project_id,
                project_version_id: release_id,
                include_unconfirmed: false,
            },
        )?;
        assert!(reused.reused);
        assert_eq!(reused.build_id, first.build_id);

        let foreign_project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "foreign".to_string(),
            name: "外部项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let foreign_document =
            database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: "foreign-doc".to_string(),
                project_id: Some(foreign_project.id),
                source_id: None,
                doc_type: "markdown".to_string(),
                title: "外部资料".to_string(),
                logical_path: "docs/foreign.md".to_string(),
                sensitivity: "internal".to_string(),
                tags: Vec::new(),
                allow_ai: true,
                allow_mcp: false,
            })?;
        let foreign_version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: foreign_document.id,
                release_id: Some(second_release.id),
                version_label: "bad".to_string(),
                git_branch: "main".to_string(),
                commit_sha: "c".repeat(40),
                source_path: "docs/foreign.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "外部内容".to_string(),
                content_hash: "foreign-v1".to_string(),
                parsed_meta: json!({}),
                token_estimate: 2,
            },
            &[],
        )?;
        let mixed_input = UpsertKnowledgeRelationInput {
            id: None,
            project_id: Some(project_id),
            release_id: Some(release_id),
            document_version_id: Some(foreign_version.id),
            snapshot_id: None,
            sensitivity: "internal".to_string(),
            from_type: "document".to_string(),
            from_key: foreign_document.id.to_string(),
            relation_type: "leaks".to_string(),
            to_type: "api".to_string(),
            to_key: "foreign-api".to_string(),
            evidence: json!({"documentVersionId": foreign_version.id}),
            confidence: 1.0,
            confirmed: true,
            source: "manual".to_string(),
        };
        assert!(
            KnowledgeService::upsert_relation(&database, mixed_input.clone()).is_err(),
            "公开关系写入必须拒绝跨项目文档版本"
        );
        // 仍用 DAO 写入模拟旧库残留的脏关系，验证图谱读取端不会信任它。
        let mixed_relation = database.upsert_knowledge_relation(&mixed_input)?;
        let after_mixed_relation = KnowledgeGraphService::build(
            &database,
            KnowledgeGraphBuildInput {
                project_id,
                project_version_id: release_id,
                include_unconfirmed: false,
            },
        )?;
        assert!(
            after_mixed_relation.reused,
            "跨项目证据不能改变当前图谱来源"
        );
        let stable_graph = KnowledgeGraphService::query(
            &database,
            KnowledgeGraphQueryInput {
                project_id,
                project_version_id: release_id,
                root_entity_key: None,
                root_entity_type: None,
                depth: 1,
                node_limit: 10,
                include_unconfirmed: false,
            },
        )?;
        assert!(stable_graph
            .edges
            .iter()
            .all(|edge| edge.source_relation_ref != format!("relation:{}", mixed_relation.id)));
        Ok(())
    }

    #[test]
    fn rejects_version_from_another_project_before_graph_reading(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (database, project_id, _) = setup_scope()?;
        let another_project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "inventory".to_string(),
            name: "库存中心".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let another_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: another_project.id,
            version: "v1.0.0".to_string(),
            tag_name: String::new(),
            branch: "main".to_string(),
            commit_sha: "b".repeat(40),
            description: String::new(),
            released_at: None,
        })?;

        let error = KnowledgeGraphService::build(
            &database,
            KnowledgeGraphBuildInput {
                project_id,
                project_version_id: another_release.id,
                include_unconfirmed: false,
            },
        )
        .expect_err("跨项目版本必须被拒绝");
        assert!(error.to_string().contains("不属于当前项目"));
        Ok(())
    }
}
