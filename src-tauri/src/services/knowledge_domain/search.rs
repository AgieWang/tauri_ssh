use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::database::Database;
use crate::error::AppError;
use crate::models::knowledge_domain::search::{
    KnowledgeCatalogSearchInput, KnowledgeCatalogSearchPage,
};
use crate::models::knowledge_domain::terminology::KnowledgeProjectTermExpansion;
use crate::services::knowledge::{normalize_key, validate_positive_id};
use crate::services::knowledge_domain::terminology::KnowledgeProjectTerminologyService;
use crate::services::knowledge_rollout::KnowledgeRolloutService;

pub(crate) const DOMAIN: &str = "search";

const DEFAULT_PAGE_LIMIT: u32 = 20;
const MAX_PAGE_LIMIT: u32 = 50;

/// 项目搜索的领域入口。游标仅包含排序位置与查询/结果快照摘要，不携带正文或文件路径；
/// 这样同一个链接不能被挪用到其他项目、版本或筛选范围。
pub struct KnowledgeCatalogSearchService;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogSearchCursor {
    input_fingerprint: String,
    result_snapshot: String,
    last_title_rank: i64,
    last_document_id: i64,
}

/// 术语扩展只作为本次查询的受控召回计划：原关键词始终保留，页面只展示已经人工确认并
/// 实际命中的映射，避免不透明的同义词或跨项目词典改变用户的搜索范围。
struct CatalogSearchPlan {
    terms: Vec<String>,
    applied_terms: Vec<KnowledgeProjectTermExpansion>,
}

impl KnowledgeCatalogSearchService {
    pub fn search(
        db: &Database,
        mut input: KnowledgeCatalogSearchInput,
    ) -> Result<KnowledgeCatalogSearchPage, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        Self::validate_and_normalize_input(db, &mut input)?;
        let input_fingerprint = catalog_input_fingerprint(&input)?;
        let search_plan = Self::build_search_plan(db, &input)?;
        let current_snapshot = snapshot_fingerprint(&format!(
            "{}:{}",
            db.get_knowledge_catalog_search_snapshot(&input)?,
            db.get_knowledge_project_term_snapshot(input.project_id)?,
        ));
        let cursor = input.cursor.as_deref().map(decode_cursor).transpose()?;
        if let Some(cursor) = &cursor {
            if cursor.input_fingerprint != input_fingerprint {
                return Err(AppError::InvalidInput(
                    "搜索条件已变化，请重新搜索".to_string(),
                ));
            }
            if cursor.result_snapshot != current_snapshot {
                return Ok(KnowledgeCatalogSearchPage {
                    items: Vec::new(),
                    next_cursor: None,
                    result_snapshot: current_snapshot,
                    snapshot_changed: true,
                    applied_terms: search_plan.applied_terms,
                });
            }
        }

        let limit = input.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
        let mut items = db.search_knowledge_catalog_page(
            &input,
            &search_plan.terms,
            cursor.as_ref().map(|value| value.last_title_rank),
            cursor.as_ref().map(|value| value.last_document_id),
            limit,
        )?;
        let has_more = items.len() > limit as usize;
        if has_more {
            items.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            let last = items
                .last()
                .ok_or_else(|| AppError::Custom("搜索分页缺少最后一条结果".to_string()))?;
            let last_title_rank = last
                .diagnostics
                .get("titleRank")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| AppError::Custom("搜索结果缺少稳定排序键".to_string()))?;
            let last_document_id = last.citation.document_id.ok_or_else(|| {
                AppError::Custom("搜索结果缺少文档标识，不能继续翻页".to_string())
            })?;
            Some(encode_cursor(&CatalogSearchCursor {
                input_fingerprint,
                result_snapshot: current_snapshot.clone(),
                last_title_rank,
                last_document_id,
            })?)
        } else {
            None
        };
        Ok(KnowledgeCatalogSearchPage {
            items,
            next_cursor,
            result_snapshot: current_snapshot,
            snapshot_changed: false,
            applied_terms: search_plan.applied_terms,
        })
    }

    fn build_search_plan(
        db: &Database,
        input: &KnowledgeCatalogSearchInput,
    ) -> Result<CatalogSearchPlan, AppError> {
        let applied_terms =
            KnowledgeProjectTerminologyService::expand_query(db, input.project_id, &input.query)?;
        let mut terms = vec![input.query.clone()];
        for expansion in &applied_terms {
            for alias in &expansion.aliases {
                if !terms.iter().any(|term| term.eq_ignore_ascii_case(alias)) {
                    terms.push(alias.clone());
                }
            }
        }
        Ok(CatalogSearchPlan {
            terms,
            applied_terms,
        })
    }

    fn validate_and_normalize_input(
        db: &Database,
        input: &mut KnowledgeCatalogSearchInput,
    ) -> Result<(), AppError> {
        validate_positive_id(input.project_id, "项目 ID")?;
        if !db.knowledge_project_exists(input.project_id)? {
            return Err(AppError::NotFound(format!(
                "知识项目不存在: {}",
                input.project_id
            )));
        }
        input.query = input.query.trim().to_string();
        if input.query.is_empty() {
            return Err(AppError::InvalidInput("搜索关键词不能为空".to_string()));
        }
        if let Some(project_version_id) = input.project_version_id {
            validate_positive_id(project_version_id, "项目版本 ID")?;
            let release = db
                .get_knowledge_release_by_id(project_version_id)?
                .ok_or_else(|| {
                    AppError::NotFound(format!("知识版本不存在: {project_version_id}"))
                })?;
            if release.project_id != input.project_id {
                return Err(AppError::InvalidInput(
                    "项目版本不属于当前项目，不能跨项目搜索".to_string(),
                ));
            }
        }
        input.repository_binding_ids.sort_unstable();
        input.repository_binding_ids.dedup();
        for binding_id in &input.repository_binding_ids {
            validate_positive_id(*binding_id, "仓库关联 ID")?;
            let binding = db
                .get_knowledge_project_repository_binding(*binding_id)?
                .ok_or_else(|| AppError::NotFound(format!("仓库关联不存在: {binding_id}")))?;
            if binding.project_id != input.project_id
                || !binding.enabled
                || binding.deleted_at.is_some()
            {
                return Err(AppError::InvalidInput(
                    "仓库关联不属于当前项目或已停用，请刷新筛选条件".to_string(),
                ));
            }
        }
        input.document_types = input
            .document_types
            .drain(..)
            .map(|value| normalize_key(&value, "文档类型"))
            .collect::<Result<Vec<_>, _>>()?;
        input.document_types.sort();
        input.document_types.dedup();
        input.limit = Some(
            input
                .limit
                .unwrap_or(DEFAULT_PAGE_LIMIT)
                .clamp(1, MAX_PAGE_LIMIT),
        );
        Ok(())
    }
}

fn catalog_input_fingerprint(input: &KnowledgeCatalogSearchInput) -> Result<String, AppError> {
    let value = serde_json::json!({
        "projectId": input.project_id,
        "projectVersionId": input.project_version_id,
        // FTS 按用户输入的原词切分；全角与半角等标题归一化等价形式在 FTS 中未必等价。
        // 因此游标范围必须保留原始关键词，不能只使用标题索引的规范化值。
        "query": input.query,
        "repositoryBindingIds": input.repository_binding_ids,
        "documentTypes": input.document_types,
    });
    let encoded = serde_json::to_vec(&value)?;
    Ok(snapshot_fingerprint(&String::from_utf8_lossy(&encoded)))
}

fn snapshot_fingerprint(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn encode_cursor(cursor: &CatalogSearchCursor) -> Result<String, AppError> {
    let value = serde_json::to_vec(cursor)?;
    Ok(URL_SAFE_NO_PAD.encode(value))
}

fn decode_cursor(value: &str) -> Result<CatalogSearchCursor, AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::InvalidInput("搜索游标无效，请重新搜索".to_string()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| AppError::InvalidInput("搜索游标无效，请重新搜索".to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        catalog_input_fingerprint, decode_cursor, encode_cursor, CatalogSearchCursor,
        KnowledgeCatalogSearchInput, KnowledgeCatalogSearchService,
    };
    use crate::database::Database;
    use crate::models::{
        CreateKnowledgeDocumentVersionInput, KnowledgeChunkWriteInput,
        UpsertKnowledgeDocumentInput, UpsertKnowledgeProjectInput, UpsertKnowledgeProjectTermInput,
    };
    use crate::services::knowledge_domain::terminology::KnowledgeProjectTerminologyService;

    #[test]
    fn cursor_round_trip_preserves_scope_snapshot_and_position() {
        let cursor = CatalogSearchCursor {
            input_fingerprint: "scope".to_string(),
            result_snapshot: "snapshot".to_string(),
            last_title_rank: 2,
            last_document_id: 9,
        };
        let encoded = encode_cursor(&cursor).expect("编码游标");
        assert_eq!(
            decode_cursor(&encoded).expect("解码游标").last_document_id,
            9
        );
    }

    #[test]
    fn cursor_scope_keeps_fts_sensitive_full_width_query_distinct() {
        let full_width = KnowledgeCatalogSearchInput {
            project_id: 1,
            project_version_id: None,
            query: "ＡＰＩ".to_string(),
            repository_binding_ids: Vec::new(),
            document_types: Vec::new(),
            cursor: None,
            limit: Some(20),
        };
        let half_width = KnowledgeCatalogSearchInput {
            query: "API".to_string(),
            ..full_width.clone()
        };
        assert_ne!(
            catalog_input_fingerprint(&full_width).expect("全角关键词指纹"),
            catalog_input_fingerprint(&half_width).expect("半角关键词指纹"),
            "FTS 不保证全角与半角等价，游标不能在两者之间复用"
        );
    }

    #[test]
    fn confirmed_project_term_adds_aliases_without_cross_project_leakage(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "search-term-project".to_string(),
            name: "搜索术语项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        KnowledgeProjectTerminologyService::upsert(
            &database,
            UpsertKnowledgeProjectTermInput {
                id: None,
                project_id: project.id,
                term: "工单".to_string(),
                aliases: vec!["WorkOrder".to_string(), "work_order".to_string()],
                confirmation_note: "负责人已确认。".to_string(),
                created_by: None,
            },
        )?;
        let plan = KnowledgeCatalogSearchService::build_search_plan(
            &database,
            &KnowledgeCatalogSearchInput {
                project_id: project.id,
                project_version_id: None,
                query: "工单查询".to_string(),
                repository_binding_ids: Vec::new(),
                document_types: Vec::new(),
                cursor: None,
                limit: None,
            },
        )?;
        assert_eq!(plan.terms, vec!["工单查询", "WorkOrder", "work_order"]);
        assert_eq!(plan.applied_terms.len(), 1);
        Ok(())
    }

    #[test]
    fn catalog_search_returns_code_document_for_confirmed_chinese_term(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "search-term-result-project".to_string(),
            name: "术语召回项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "work-order-code".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "code".to_string(),
            title: "WorkOrder 领域模型".to_string(),
            logical_path: "src/WorkOrder.ts".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: false,
            allow_mcp: false,
        })?;
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v1".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: "src/WorkOrder.ts".to_string(),
                mime_type: "text/plain".to_string(),
                content: "export class WorkOrder {}".to_string(),
                content_hash: "work-order-content".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "WorkOrder".to_string(),
                content: "export class WorkOrder {}".to_string(),
                content_hash: "work-order-chunk".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;
        KnowledgeProjectTerminologyService::upsert(
            &database,
            UpsertKnowledgeProjectTermInput {
                id: None,
                project_id: project.id,
                term: "工单".to_string(),
                aliases: vec!["WorkOrder".to_string()],
                confirmation_note: "负责人已确认。".to_string(),
                created_by: None,
            },
        )?;
        let page = KnowledgeCatalogSearchService::search(
            &database,
            KnowledgeCatalogSearchInput {
                project_id: project.id,
                project_version_id: None,
                query: "工单".to_string(),
                repository_binding_ids: Vec::new(),
                document_types: Vec::new(),
                cursor: None,
                limit: Some(20),
            },
        )?;
        assert_eq!(page.applied_terms[0].term, "工单");
        assert_eq!(page.items[0].citation.title, "WorkOrder 领域模型");
        assert!(page.items[0].channels.contains(&"title".to_string()));
        Ok(())
    }
}
