use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Instant;

use regex::Regex;
use serde::Deserialize;

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    CreateAuditLogInput, KnowledgeAskInput, KnowledgeAskResult, KnowledgeCitation,
    KnowledgeConversationMessage, KnowledgeHybridSearchInput, KnowledgeHybridSearchResult,
    KnowledgeListInput, KnowledgeQueryAnalysis, KnowledgeRagContextPreview,
    KnowledgeRetrievalEvaluationCaseResult, KnowledgeRetrievalEvaluationRun, KnowledgeSearchHit,
    KnowledgeSearchInput, KnowledgeVectorSearchInput, ListKnowledgeRelationsInput,
    RunKnowledgeRetrievalEvaluationInput,
};
use crate::services::ai_provider::AiProviderService;
use crate::services::audit::AuditService;
use crate::services::knowledge_embedding::KnowledgeEmbeddingService;
use crate::services::knowledge_policy::KnowledgePolicyService;
use crate::services::knowledge_rollout::KnowledgeRolloutService;

/// 检索前的规则解析器。精确标识用于硬过滤或排序加权，绝不替代用户显式传入的过滤条件。
pub struct KnowledgeRetrievalService;

const RELEASE_REQUIREMENT_COVERAGE_MODE: &str = "releaseRequirementCoverage";

fn audit_rag_ask(
    db: &Database,
    mode: &str,
    citation_count: usize,
    conflict_count: usize,
    gap_count: usize,
) {
    let _ = AuditService::create(db, CreateAuditLogInput {
        actor: "local-user".to_string(), source: "knowledge".to_string(), server_alias: String::new(),
        action: "knowledge_rag_ask".to_string(), risk: if mode.starts_with("remote") { "ai" } else { "readonly" }.to_string(),
        result: "成功".to_string(), summary: "执行知识库问答".to_string(),
        detail_json: Some(serde_json::json!({"mode":mode,"citationCount":citation_count,"conflictCount":conflict_count,"evidenceGapCount":gap_count}).to_string()),
        request_id: None, approval_id: None,
    });
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetrievalEvaluationFixture {
    fixture_version: String,
    documents: Vec<RetrievalEvaluationDocument>,
    queries: Vec<RetrievalEvaluationQuery>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetrievalEvaluationDocument {
    document_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetrievalEvaluationQuery {
    id: String,
    query: String,
    #[serde(default)]
    must_match: Vec<String>,
    #[serde(default)]
    must_not_match_as_primary: Vec<String>,
    #[serde(default)]
    expected_refusal: bool,
}

impl KnowledgeRetrievalService {
    pub fn analyze_query(db: &Database, query: &str) -> Result<KnowledgeQueryAnalysis, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        let query = query.trim();
        if query.is_empty() {
            return Err(AppError::InvalidInput("知识检索问题不能为空".to_string()));
        }
        let projects = db.list_knowledge_projects(&KnowledgeListInput {
            project_id: None,
            release_id: None,
            source_id: None,
            keyword: None,
            status: Some("enabled".to_string()),
            offset: Some(0),
            limit: Some(200),
        })?;
        let normalized_query = query.to_lowercase();
        let mut matched_projects = BTreeSet::new();
        for project in projects.items {
            let aliases = std::iter::once(project.project_key.as_str())
                .chain(std::iter::once(project.name.as_str()))
                .chain(project.aliases.iter().map(String::as_str));
            if aliases
                .filter(|alias| alias.len() >= 2)
                .any(|alias| normalized_query.contains(&alias.trim().to_lowercase()))
            {
                matched_projects.insert(project.id);
            }
        }
        let project_ids = matched_projects.into_iter().collect::<Vec<_>>();
        let ambiguous_project_ids = if project_ids.len() > 1 {
            project_ids.clone()
        } else {
            Vec::new()
        };
        Ok(KnowledgeQueryAnalysis {
            query: query.to_string(),
            project_ids,
            ambiguous_project_ids,
            releases: captures(query, r"(?i)v?\d+(?:\.\d+){1,3}(?:[-+][a-z0-9.-]+)?"),
            requirement_ids: captures(
                query,
                r"(?i)(?:\b(?:req|story|task|bug)[#_ -]?\d+\b|(?:需求|任务|缺陷)[#_ -]?\d+)",
            ),
            commit_shas: captures(query, r"(?i)\b[0-9a-f]{7,40}\b"),
            code_symbols: captures(
                query,
                r"\b[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)+\b",
            ),
            paths: captures(query, r"(?:(?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+)"),
            api_routes: captures(query, r"/(?:[A-Za-z0-9_.~!$&'()*+,;=:@%\-]+/?)+"),
            tables: sql_tables(query),
            fields: captures(query, r"\b[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*\b"),
        })
    }

    /// 在 FTS、向量或关系通道运行前统一收敛检索范围。
    ///
    /// 没有明确授权模型的调用方不应把 restricted/confidential 文档当作默认候选；
    /// 版本过滤会同时收敛到所属项目，防止同名版本跨项目串用。
    pub fn apply_hard_filters(
        db: &Database,
        mut input: KnowledgeSearchInput,
    ) -> Result<KnowledgeSearchInput, AppError> {
        input.query = input.query.trim().to_string();
        if input.query.is_empty() {
            return Err(AppError::InvalidInput("知识检索问题不能为空".to_string()));
        }
        normalize_ids(&mut input.project_ids);
        normalize_ids(&mut input.release_ids);
        normalize_ids(&mut input.source_ids);
        if input.project_ids.is_empty() {
            let analysis = Self::analyze_query(db, &input.query)?;
            if !analysis.ambiguous_project_ids.is_empty() {
                return Err(AppError::InvalidInput(
                    "项目别名匹配多个知识项目，请显式选择项目后再检索".to_string(),
                ));
            }
            input.project_ids = analysis.project_ids;
        }

        // 版本文本（例如 v1.6.0）必须在进入任一召回通道前收敛为真实 release ID。
        // 同名版本可跨项目存在，只有项目范围内唯一匹配时才自动选择。
        if input.release_ids.is_empty() {
            let analysis = Self::analyze_query(db, &input.query)?;
            if !analysis.releases.is_empty() {
                let projects = if input.project_ids.is_empty() {
                    db.list_knowledge_projects(&KnowledgeListInput {
                        project_id: None,
                        release_id: None,
                        source_id: None,
                        keyword: None,
                        status: Some("enabled".to_string()),
                        offset: Some(0),
                        limit: Some(200),
                    })?
                    .items
                } else {
                    input
                        .project_ids
                        .iter()
                        .map(|project_id| {
                            db.list_knowledge_projects(&KnowledgeListInput {
                                project_id: Some(*project_id),
                                release_id: None,
                                source_id: None,
                                keyword: None,
                                status: Some("enabled".to_string()),
                                offset: Some(0),
                                limit: Some(1),
                            })?
                            .items
                            .into_iter()
                            .next()
                            .ok_or_else(|| {
                                AppError::NotFound(format!("启用的知识项目不存在: {project_id}"))
                            })
                        })
                        .collect::<Result<Vec<_>, AppError>>()?
                };
                let mut matched_release_ids = BTreeSet::new();
                for project in projects {
                    for release in db.list_knowledge_releases(project.id)? {
                        if analysis.releases.iter().any(|version| {
                            normalized_release_name(&release.version)
                                == normalized_release_name(version)
                                || normalized_release_name(&release.tag_name)
                                    == normalized_release_name(version)
                        }) {
                            matched_release_ids.insert(release.id);
                        }
                    }
                }
                if matched_release_ids.len() > 1 {
                    return Err(AppError::InvalidInput(
                        "版本号匹配多个知识发布版本，请显式选择项目和版本后再检索".to_string(),
                    ));
                }
                input.release_ids = matched_release_ids.into_iter().collect();
            }
        }

        let mut project_ids = input.project_ids.iter().copied().collect::<HashSet<_>>();
        for project_id in &input.project_ids {
            let page = db.list_knowledge_projects(&KnowledgeListInput {
                project_id: Some(*project_id),
                release_id: None,
                source_id: None,
                keyword: None,
                status: Some("enabled".to_string()),
                offset: Some(0),
                limit: Some(1),
            })?;
            if page.items.is_empty() {
                return Err(AppError::NotFound(format!(
                    "启用的知识项目不存在: {project_id}"
                )));
            }
        }
        for release_id in &input.release_ids {
            let release = db
                .get_knowledge_release_by_id(*release_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
            if !input.project_ids.is_empty() && !project_ids.contains(&release.project_id) {
                return Err(AppError::InvalidInput(
                    "选择的发布版本不属于当前项目过滤范围".to_string(),
                ));
            }
            project_ids.insert(release.project_id);
        }
        input.project_ids = project_ids.into_iter().collect();
        input.project_ids.sort_unstable();

        for source_id in &input.source_ids {
            let source = db
                .get_knowledge_source_by_id(*source_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {source_id}")))?;
            if !source.enabled {
                return Err(AppError::InvalidInput(format!("知识源已禁用: {source_id}")));
            }
            if let Some(project_id) = source.project_id {
                if !input.project_ids.is_empty() && !input.project_ids.contains(&project_id) {
                    return Err(AppError::InvalidInput(
                        "知识源不属于当前项目过滤范围".to_string(),
                    ));
                }
            }
        }
        if let Some(snapshot_id) = input.snapshot_id {
            let snapshot = db
                .get_knowledge_code_snapshot_by_id(snapshot_id)?
                .ok_or_else(|| AppError::NotFound(format!("源码快照不存在: {snapshot_id}")))?;
            if let Some(project_id) = snapshot.project_id {
                if !input.project_ids.is_empty() && !input.project_ids.contains(&project_id) {
                    return Err(AppError::InvalidInput(
                        "源码快照不属于当前项目过滤范围".to_string(),
                    ));
                }
            }
            if let Some(release_id) = snapshot.release_id {
                if !input.release_ids.is_empty() && !input.release_ids.contains(&release_id) {
                    return Err(AppError::InvalidInput(
                        "源码快照不属于当前发布版本过滤范围".to_string(),
                    ));
                }
            }
        }
        input.document_types = normalize_text_filters(input.document_types, "文档类型")?;
        input.sensitivities = normalize_text_filters(input.sensitivities, "敏感级别")?;
        if input.sensitivities.is_empty() {
            input.sensitivities = vec!["public".to_string(), "internal".to_string()];
        }
        if input
            .sensitivities
            .iter()
            .any(|value| !matches!(value.as_str(), "public" | "internal"))
        {
            return Err(AppError::InvalidInput(
                "普通知识检索仅允许 public 或 internal 敏感级别".to_string(),
            ));
        }
        input.limit = Some(input.limit.unwrap_or(20).clamp(1, 100));
        Ok(input)
    }

    pub fn search_fts(
        db: &Database,
        input: KnowledgeSearchInput,
    ) -> Result<Vec<crate::models::KnowledgeSearchHit>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        let input = Self::apply_hard_filters(db, input)?;
        Self::search_title_and_fts(db, &input)
    }

    /// 标题和正文都属于本地、可解释的检索通道。标题优先展示，但同一文档当前版本
    /// 只保留一条结果，并在 channels 中保留两种命中原因，避免用户看到重复卡片。
    fn search_title_and_fts(
        db: &Database,
        input: &KnowledgeSearchInput,
    ) -> Result<Vec<KnowledgeSearchHit>, AppError> {
        let title_hits = db.search_knowledge_document_title_hits(input)?;
        let fts_hits = db.search_knowledge_fts(input)?;
        Ok(merge_title_and_fts_hits(
            title_hits,
            fts_hits,
            input.limit.unwrap_or(20).clamp(1, 100) as usize,
        ))
    }

    /// 三通道检索的入口。FTS 与向量查询使用独立工作线程同时调度；SQLite 单连接可能
    /// 使底层数据库访问短暂串行，但任一通道的准备或失败不会阻塞另一通道。关系扩展
    /// 依赖前两通道的种子，因而在二者完成后有界执行。这里绝不因缺少向量而触发模型
    /// 调用或远程回退。
    pub fn search_hybrid(
        db: &Database,
        input: KnowledgeHybridSearchInput,
    ) -> Result<KnowledgeHybridSearchResult, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        let filters = Self::apply_hard_filters(db, input.filters)?;
        let limit = filters.limit.unwrap_or(20).clamp(1, 100) as usize;
        let fts_filters = filters.clone();
        let vector_filters = filters.clone();
        let query_vector = input.query_vector.filter(|vector| !vector.is_empty());
        let (fts_result, fts_duration_ms, vector_result, vector_duration_ms) =
            std::thread::scope(|scope| -> Result<_, AppError> {
                let fts_started = Instant::now();
                let fts_task = scope.spawn(move || {
                    let result = Self::search_title_and_fts(db, &fts_filters);
                    (result, elapsed_millis(fts_started))
                });
                let vector_started = Instant::now();
                let vector_task = scope.spawn(move || {
                    let result = match query_vector {
                        Some(query_vector) => KnowledgeEmbeddingService::search_active_vectors(
                            db,
                            KnowledgeVectorSearchInput {
                                query_vector,
                                filters: vector_filters,
                            },
                        ),
                        None => Ok(Vec::new()),
                    };
                    (result, elapsed_millis(vector_started))
                });
                let (fts_result, fts_duration_ms) = fts_task
                    .join()
                    .map_err(|_| AppError::Custom("知识 FTS 检索工作线程异常结束".to_string()))?;
                let (vector_result, vector_duration_ms) = vector_task
                    .join()
                    .map_err(|_| AppError::Custom("知识向量检索工作线程异常结束".to_string()))?;
                Ok((
                    fts_result,
                    fts_duration_ms,
                    vector_result,
                    vector_duration_ms,
                ))
            })?;
        let fts_hits = fts_result?;
        let (vector_hits, vector_error) = match vector_result {
            Ok(hits) => (hits, None),
            // FTS 仍然有效时，未配置 Profile 或提供了不兼容向量不能阻断精确检索；
            // 诊断仅保存错误类别，不写入查询正文或知识内容。
            Err(AppError::NotFound(_) | AppError::InvalidInput(_)) => {
                (Vec::new(), Some("unavailable"))
            }
            Err(error) => return Err(error),
        };
        let relation_depth = input.relation_depth.unwrap_or(1).clamp(0, 2);
        let relation_started = Instant::now();
        let relation_hits = if relation_depth == 0 {
            Vec::new()
        } else {
            Self::recall_confirmed_relations(
                db,
                &filters,
                &fts_hits,
                &vector_hits,
                relation_depth,
                limit,
            )?
        };
        let relation_duration_ms = elapsed_millis(relation_started);
        let analysis = Self::analyze_query(db, &filters.query)?;
        let hits = apply_fusion_signals(
            fuse_rrf(
                [
                    ("fts", fts_hits.as_slice()),
                    ("vector", vector_hits.as_slice()),
                    ("relation", relation_hits.as_slice()),
                ],
                limit,
            ),
            &filters,
            &analysis,
        );
        Ok(KnowledgeHybridSearchResult {
            diagnostics: serde_json::json!({
                "channels": {
                    "fts": {"candidates": fts_hits.len(), "durationMs": fts_duration_ms, "status": "ok"},
                    "vector": {"candidates": vector_hits.len(), "durationMs": vector_duration_ms, "status": vector_error.unwrap_or("ok")},
                    "relation": {"candidates": relation_hits.len(), "durationMs": relation_duration_ms, "status": if relation_depth == 0 { "disabled" } else { "ok" }},
                },
                "relationDepth": relation_depth,
                "dispatch": "parallel-fts-vector-then-bounded-relations",
                "fusion": "rrf-60",
            }),
            hits,
        })
    }

    /// 返回可复核的证据上下文。该函数不会调用 Embedding、聊天模型或外部网络。
    pub fn preview_rag_context(
        db: &Database,
        search: KnowledgeSearchInput,
    ) -> Result<KnowledgeRagContextPreview, AppError> {
        Self::preview_rag_context_with_query_vector(db, search, None)
    }

    /// 供项目版本问答传入与活动 Profile 同空间的查询向量。向量由上层显式生成，
    /// 检索层不会为了补全语义召回而隐式发送任何远程请求。
    pub fn preview_rag_context_with_query_vector(
        db: &Database,
        mut search: KnowledgeSearchInput,
        query_vector: Option<Vec<f32>>,
    ) -> Result<KnowledgeRagContextPreview, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        search.include_context = Some(true);
        let result = Self::search_hybrid(
            db,
            KnowledgeHybridSearchInput {
                filters: search.clone(),
                query_vector,
                relation_depth: Some(1),
            },
        )?;
        // 精确入口符号通常只会命中编排方法，例如 `generateTomorrowPlan`。业务规则却可能
        // 位于其被调的批处理方法或 Mapper/SQL 中；仅把入口片段交给模型会导致它错误地把
        // 可由源码确认的筛选条件说成“证据不足”。因此在同一项目/版本（且有快照时同一
        // 不变快照）内，最多继续补齐两跳已出现在源码中的调用符号。这里仍是本地 FTS，
        // 不把未确认的静态调用图当作事实，也不会发起额外的向量或远程 AI 请求。
        let primary_hits = result.hits;
        let related_code_hits = Self::recall_called_code_context(db, &search, &primary_hits)?;
        // 调用链片段是补充而非替代：必须先保留用户精确命中的入口，再追加下游细则，
        // 防止调用较多时入口被 12 个上下文槽位挤出。初始结果超过上限时先保留前半
        // 部分入口，再放入两跳结果，最后才用其余初始命中填满槽位，确保 Mapper/SQL
        // 细则不会仅因初始召回过多而永远无法进入模型上下文。
        // 只有实际拼入上下文的片段可被模型引用。不能把第 13 个未发送的命中也暴露为
        // 合法 citation，否则回答会看似可追溯、实际却没有见过对应证据。
        let context_hits = select_context_hits(&primary_hits, &related_code_hits);
        let context_evidence = context_hits.clone();
        let citations = context_hits
            .iter()
            .map(|hit| hit.citation.clone())
            .collect::<Vec<_>>();
        let conflicts = find_conflicts(&context_evidence);
        let evidence_gaps = evidence_gaps(&search, &context_evidence);
        let context_refs = context_hits.iter().collect::<Vec<_>>();
        let context = render_evidence_context(&context_refs);
        Ok(KnowledgeRagContextPreview {
            prompt: search.query,
            context,
            citations,
            conflicts,
            evidence_gaps,
            retrieval_diagnostics: result.diagnostics,
        })
    }

    /// 版本需求覆盖问题需要先得到需求基线，再用每条需求中的稳定业务短语检索代码。
    /// 这不是把相似代码直接提升为“已实现”，而是为模型提供成对的需求与实现候选；
    /// 没有显式关系或直接行为证据时，回答必须保持“待确认”。
    fn preview_release_requirement_coverage_context(
        db: &Database,
        mut search: KnowledgeSearchInput,
        query_vector: Option<Vec<f32>>,
    ) -> Result<KnowledgeRagContextPreview, AppError> {
        search.include_context = Some(true);
        let base_result = Self::search_hybrid(
            db,
            KnowledgeHybridSearchInput {
                filters: search.clone(),
                query_vector,
                relation_depth: Some(1),
            },
        )?;

        let all_requirement_hits = base_result
            .hits
            .iter()
            .filter(|hit| is_requirement_baseline_hit(hit))
            .cloned()
            .collect::<Vec<_>>();
        let release_specific_hits = all_requirement_hits
            .iter()
            .filter(|hit| requirement_hit_matches_release(hit, &search.query))
            .cloned()
            .collect::<Vec<_>>();
        let requirement_pool = if release_specific_hits.is_empty() {
            all_requirement_hits
        } else {
            release_specific_hits
        };
        let mut seen_requirement_content = BTreeSet::new();
        let mut requirement_hits = requirement_pool
            .into_iter()
            .filter(|hit| seen_requirement_content.insert(hit.content.trim().to_string()))
            .take(4)
            .map(|mut hit| {
                hit.diagnostics["coverageRole"] = serde_json::json!("requirementBaseline");
                hit
            })
            .collect::<Vec<_>>();
        // 文档类型来自真实上传格式（Markdown、DOCX、PDF），不能假设都被标成
        // requirement。标题未命中时保留非代码正文作为降级基线，但绝不拿代码报告
        // 冒充需求文档。
        if requirement_hits.is_empty() {
            requirement_hits = base_result
                .hits
                .iter()
                .filter(|hit| {
                    !is_code_evidence_hit(hit)
                        && !hit.citation.logical_path.starts_with("code-reports/")
                })
                .take(4)
                .cloned()
                .map(|mut hit| {
                    hit.diagnostics["coverageRole"] =
                        serde_json::json!("requirementBaselineFallback");
                    hit
                })
                .collect();
        }

        let requirement_candidates = extract_requirement_candidates(&requirement_hits);
        let mut implementation_hits = Vec::new();
        let mut seen_implementation = BTreeSet::new();
        for requirement in requirement_candidates.iter().take(8) {
            let mut code_search = search.clone();
            // 先在更宽的 FTS 候选集中按源码属性过滤，再截取每条需求的前四条。
            // 一条需求常同时覆盖今日、明日等多个页面，只保留两条会把同一需求的
            // 第二个真实实现挤掉；总上下文仍由 12 条全局上限约束。
            // 如果先做全类型 RRF 截断，标题相似的需求和发布文档会把真实代码挤出。
            code_search.limit = Some(100);
            let mut code_hits = Vec::new();
            let mut seen_code_hits = BTreeSet::new();
            for code_query in coverage_code_queries(requirement) {
                code_search.query = code_query;
                for hit in Self::search_fts(db, code_search.clone())?
                    .into_iter()
                    .filter(is_implementation_source_hit)
                {
                    if seen_code_hits.insert(hit.citation.citation_key.clone()) {
                        code_hits.push(hit);
                    }
                }
            }
            code_hits.sort_by(|left, right| {
                implementation_candidate_priority(requirement, left)
                    .cmp(&implementation_candidate_priority(requirement, right))
                    .then_with(|| right.score.total_cmp(&left.score))
                    .then_with(|| left.citation.citation_key.cmp(&right.citation.citation_key))
            });
            append_unique_requirement_candidates(
                requirement,
                code_hits,
                &mut seen_implementation,
                &mut implementation_hits,
            );
        }
        if implementation_hits.is_empty() {
            implementation_hits = base_result
                .hits
                .iter()
                .filter(|hit| is_implementation_source_hit(hit))
                .take(7)
                .cloned()
                .map(|mut hit| {
                    hit.diagnostics["coverageRole"] =
                        serde_json::json!("implementationCandidateFallback");
                    hit
                })
                .collect();
        }

        let mut test_hits = Vec::new();
        let mut seen_tests = BTreeSet::new();
        for requirement in requirement_candidates.iter().take(8) {
            let related_implementations = implementation_hits
                .iter()
                .filter(|hit| {
                    hit.diagnostics
                        .get("coverageRequirement")
                        .and_then(serde_json::Value::as_str)
                        == Some(requirement.as_str())
                })
                .collect::<Vec<_>>();
            let mut test_search = search.clone();
            test_search.limit = Some(100);
            let mut candidates = Vec::new();
            let mut seen_candidates = BTreeSet::new();
            for test_query in coverage_test_queries(requirement, &related_implementations) {
                test_search.query = test_query;
                for hit in Self::search_fts(db, test_search.clone())?
                    .into_iter()
                    .filter(is_test_evidence_hit)
                {
                    if seen_candidates.insert(hit.citation.citation_key.clone()) {
                        candidates.push(hit);
                    }
                }
            }
            candidates.sort_by(|left, right| {
                test_candidate_priority(requirement, &related_implementations, left)
                    .cmp(&test_candidate_priority(
                        requirement,
                        &related_implementations,
                        right,
                    ))
                    .then_with(|| right.score.total_cmp(&left.score))
                    .then_with(|| left.citation.citation_key.cmp(&right.citation.citation_key))
            });
            append_unique_test_candidates(requirement, candidates, &mut seen_tests, &mut test_hits);
        }

        let report_hits = base_result
            .hits
            .iter()
            .filter(|hit| {
                hit.citation
                    .logical_path
                    .ends_with("code-reports/release-implementation.md")
            })
            .take(1)
            .cloned()
            .map(|mut hit| {
                hit.diagnostics["coverageRole"] = serde_json::json!("releaseMetadata");
                hit
            })
            .collect::<Vec<_>>();
        let context_hits = select_coverage_context_hits(
            &requirement_hits,
            &implementation_hits,
            &test_hits,
            &report_hits,
        );
        let citations = context_hits
            .iter()
            .map(|hit| hit.citation.clone())
            .collect::<Vec<_>>();
        let explicit_relations = db
            .list_knowledge_relations(&ListKnowledgeRelationsInput {
                entity_type: None,
                entity_key: None,
                project_ids: search.project_ids.clone(),
                release_ids: search.release_ids.clone(),
                sensitivities: search.sensitivities.clone(),
                confirmed_only: Some(true),
                limit: Some(500),
            })?
            .into_iter()
            .filter(|relation| {
                relation.scope_status == "scoped"
                    && matches!(
                        relation.relation_type.as_str(),
                        "implemented_by" | "verified_by"
                    )
            })
            .collect::<Vec<_>>();
        let implemented_relation_count = explicit_relations
            .iter()
            .filter(|relation| relation.relation_type == "implemented_by")
            .count();
        let verified_relation_count = explicit_relations
            .iter()
            .filter(|relation| relation.relation_type == "verified_by")
            .count();
        let explicit_relation_count = explicit_relations.len();
        let mut evidence_gaps = Vec::new();
        if requirement_hits.is_empty() {
            evidence_gaps.push("未找到当前版本的明确需求基线".to_string());
        }
        if implementation_hits.is_empty() {
            evidence_gaps.push("未找到可与需求逐条核对的源码证据".to_string());
        }
        if test_hits.is_empty() {
            evidence_gaps.push("未找到可与需求逐条核对的测试源码候选".to_string());
        } else if verified_relation_count == 0 {
            evidence_gaps.push(
                "已找到测试源码候选，但未找到测试执行报告或 verified_by 关系；不能据此判定测试已通过"
                    .to_string(),
            );
        }
        if explicit_relation_count == 0 {
            evidence_gaps.push(
                "需求与代码尚未建立显式关系；相似代码只能作为待确认候选，不能据此判定“未实现”"
                    .to_string(),
            );
        }
        let context_refs = context_hits.iter().collect::<Vec<_>>();
        let mut diagnostics = base_result.diagnostics;
        diagnostics["queryMode"] = serde_json::json!(RELEASE_REQUIREMENT_COVERAGE_MODE);
        diagnostics["coverage"] = serde_json::json!({
            "requirementBaselineCount": requirement_hits.len(),
            "requirementCandidateCount": requirement_candidates.len(),
            "implementationCandidateCount": implementation_hits.len(),
            "testCandidateCount": test_hits.len(),
            "implementedRelationCount": implemented_relation_count,
            "verifiedRelationCount": verified_relation_count,
            "explicitRelationCount": explicit_relation_count,
        });
        Ok(KnowledgeRagContextPreview {
            prompt: search.query,
            context: render_evidence_context(&context_refs),
            conflicts: find_conflicts(&context_hits),
            evidence_gaps,
            citations,
            retrieval_diagnostics: diagnostics,
        })
    }

    /// 仅用 `preview_rag_context` 输出的证据调用既有 AI Provider。没有证据时直接拒答；
    /// 这样模型不会以内部常识虚构项目事实。`evidence_only=true` 可用于只返回可审核
    /// 的确定性证据摘要，而不发送任何远程请求。
    pub async fn ask(
        db: &Database,
        input: KnowledgeAskInput,
    ) -> Result<KnowledgeAskResult, AppError> {
        Self::ask_with_query_vector(db, input, None).await
    }

    /// 与普通问答共享完全相同的证据、授权和引用校验，仅允许上层提供已验证维度的
    /// 查询向量。这样项目工作台不会因增加向量通道而绕过既有的拒答规则。
    pub async fn ask_with_query_vector(
        db: &Database,
        input: KnowledgeAskInput,
        query_vector: Option<Vec<f32>>,
    ) -> Result<KnowledgeAskResult, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        let coverage_mode = input.answer_mode.as_deref() == Some(RELEASE_REQUIREMENT_COVERAGE_MODE);
        let mut preview = if coverage_mode {
            Self::preview_release_requirement_coverage_context(db, input.search, query_vector)?
        } else {
            Self::preview_rag_context_with_query_vector(db, input.search, query_vector)?
        };
        let original_question = input
            .original_question
            .as_deref()
            .map(str::trim)
            .filter(|question| !question.is_empty());
        preview.prompt = original_question_or_retrieval_query(original_question, &preview.prompt);
        if let Some(original_question) = original_question {
            // 检索词可能为精确代码符号，不能再以它判断用户是否要求需求/测试证据；
            // 使用原问题过滤不相关的通用缺口，避免已命中代码时仍提示“缺少测试”。
            preview.evidence_gaps = preview
                .evidence_gaps
                .into_iter()
                .filter(|gap| evidence_gap_is_relevant_to_question(gap, original_question))
                .collect();
        }
        if preview.citations.is_empty() {
            audit_rag_ask(
                db,
                "refused_no_evidence",
                0,
                preview.conflicts.len(),
                preview.evidence_gaps.len(),
            );
            return Ok(KnowledgeAskResult {
                answer: "未找到满足当前项目、版本与权限约束的知识证据，因此不能编造内部事实。"
                    .to_string(),
                citation_validation: "notApplicable".to_string(),
                citations: Vec::new(),
                conflicts: preview.conflicts,
                evidence_gaps: with_missing_evidence(
                    preview.evidence_gaps,
                    "未找到可引用的知识片段",
                ),
                retrieval_diagnostics: preview.retrieval_diagnostics,
            });
        }
        if input.evidence_only.unwrap_or(false) {
            audit_rag_ask(
                db,
                "evidence_only",
                preview.citations.len(),
                preview.conflicts.len(),
                preview.evidence_gaps.len(),
            );
            return Ok(KnowledgeAskResult {
                answer: if coverage_mode {
                    render_coverage_evidence_only_answer(&preview)
                } else {
                    render_evidence_only_answer(&preview)
                },
                citation_validation: "notApplicable".to_string(),
                citations: preview.citations,
                conflicts: preview.conflicts,
                evidence_gaps: preview.evidence_gaps,
                retrieval_diagnostics: preview.retrieval_diagnostics,
            });
        }
        let provider_key = input.provider_key.trim();
        if provider_key.is_empty() {
            return Err(AppError::InvalidInput(
                "知识问答必须选择已配置的 AI Provider".to_string(),
            ));
        }
        let requested_model = input.model.trim();
        if requested_model.is_empty() {
            return Err(AppError::InvalidInput(
                "知识问答必须指定模型标识".to_string(),
            ));
        }
        let provider = db
            .get_ai_provider(provider_key)?
            .ok_or_else(|| AppError::NotFound(format!("AI Provider 不存在: {provider_key}")))?;
        if provider.default_model != requested_model {
            return Err(AppError::InvalidInput(
                "选择的知识问答模型与当前 Provider 默认聊天模型不一致；请先在 Provider 设置中切换模型"
                    .to_string(),
            ));
        }
        KnowledgePolicyService::authorize_remote_ai_context(
            db,
            &preview.citations,
            &preview.context,
        )?;
        // Provider 只能收到此处生成的脱敏证据副本，检索预览仍保留本地原始证据用于引用。
        let sanitized_context =
            KnowledgePolicyService::sanitize_remote_ai_context(&preview.context)?;
        // 用户问题同样是远程请求正文的一部分，不能因其不属于检索证据而绕开脱敏。
        let sanitized_prompt = KnowledgePolicyService::sanitize_remote_ai_context(&preview.prompt)?;
        let conversation_context = render_conversation_context(&input.conversation)?;
        let coverage_instruction = if coverage_mode {
            "这是版本需求覆盖问题。必须先按需求基线逐条输出 Markdown 表格，列为“需求、判断、实现证据、验证证据”。\n\
判断仅允许使用：已确认实现、发现实现候选（待确认）、未找到实现证据、明确未实现。\n\
只有显式关系或源码直接展示了该需求的完整行为时才能写“已确认实现”；只有证据明确标记未完成时才能写“明确未实现”。\n\
验证证据必须区分“找到测试源码候选”和“已有执行验证”：测试文件存在只能写“找到测试源码候选（未验证执行结果）”；只有测试运行报告、CI/Jenkins 结果或 confirmed verified_by 关系才能写“已验证通过”。\n\
没有搜到代码必须写“未找到实现证据”，不得改写成“未实现”。同一条需求的每个判断都要引用需求证据和对应代码/测试证据。"
        } else {
            ""
        };
        let evidence_gap_context = if preview.evidence_gaps.is_empty() {
            "（无）".to_string()
        } else {
            preview
                .evidence_gaps
                .iter()
                .map(|gap| format!("- {gap}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let system_prompt = format!(
            "你是团队知识库问答助手。只能根据下方证据回答；每个事实后原样使用证据中的 [citation_key] 引用。\n\
不要添加 `citation:` 前缀；如果历史格式要求添加该前缀，系统会兼容解析。\n\
解释 SQL 时，LIMIT、JOIN、WHERE 等条件只能按证据中展示的查询作用域描述；除非 SQL 明确按用户分组、窗口或子查询限制，\
不得把整批查询的 LIMIT 改写成“每个用户/每个网格员”的限制。\n\
不要输出没有引用的事实性开场段落；每个非标题事实段落（包括开头总结）都必须至少带一个有效引用。\n\
不得把未命中的实现、测试或禅道状态补全为事实；存在冲突时并列说明；用户询问历史版本时，\n\
不得把后续版本表述为当时事实。\n{}\n\n对话历史（仅用于理解连续追问，不是证据；不得从历史回答推断当前事实）：\n{}\n\n本轮证据缺口：\n{}\n\n本轮证据：\n{}",
            coverage_instruction,
            if conversation_context.is_empty() {
                "（无历史消息，这是本轮对话的第一问）"
            } else {
                &conversation_context
            },
            evidence_gap_context,
            sanitized_context,
        );
        let provider_result = AiProviderService::ask(
            db,
            crate::models::AiProviderAskInput {
                prompt: sanitized_prompt,
                provider_key: Some(provider_key.to_string()),
                system_prompt: Some(system_prompt),
                // 仅装配知识库作用域的内置规则，确保远程模型也收到版本隔离、引用和
                // 证据缺口约束；这些规则不替代下方按实际检索结果生成的证据上下文。
                skill_scope: Some("knowledge".into()),
                use_skill_trigger: Some(false),
            },
        )
        .await?;
        let citation_validation =
            citation_validation_status(&provider_result.answer, &preview.citations);
        audit_rag_ask(
            db,
            if citation_validation == "verified" {
                "remote"
            } else {
                "remote_unverified_citation"
            },
            preview.citations.len(),
            preview.conflicts.len(),
            preview.evidence_gaps.len(),
        );
        Ok(KnowledgeAskResult {
            answer: provider_result.answer,
            citation_validation: citation_validation.to_string(),
            citations: preview.citations,
            conflicts: preview.conflicts,
            evidence_gaps: preview.evidence_gaps,
            retrieval_diagnostics: preview.retrieval_diagnostics,
        })
    }

    /// 运行随应用发布的固定评测集。评测只使用本地已索引知识，不调用 Embedding 或聊天
    /// Provider，因此可用于比较 Profile/分块/排序变化前后的召回和历史串用风险。
    pub fn run_fixed_evaluation(
        db: &Database,
        input: RunKnowledgeRetrievalEvaluationInput,
    ) -> Result<KnowledgeRetrievalEvaluationRun, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        let fixture: RetrievalEvaluationFixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/knowledge_retrieval_baseline.json"
        ))?;
        let top_k = input.top_k.unwrap_or(5).clamp(1, 20);
        let document_ids = fixture
            .documents
            .iter()
            .map(|document| {
                Ok((
                    document.document_key.clone(),
                    db.get_knowledge_document_by_key(&document.document_key)?
                        .map(|doc| doc.id),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, AppError>>()?;
        let mut cases = Vec::new();
        for query in fixture.queries {
            let started = Instant::now();
            let result = Self::search_hybrid(
                db,
                KnowledgeHybridSearchInput {
                    filters: KnowledgeSearchInput {
                        query: query.query,
                        project_ids: Vec::new(),
                        release_ids: Vec::new(),
                        source_ids: Vec::new(),
                        document_types: Vec::new(),
                        sensitivities: Vec::new(),
                        snapshot_id: None,
                        limit: Some(top_k),
                        include_context: Some(false),
                    },
                    query_vector: None,
                    relation_depth: Some(0),
                },
            )?;
            let expected = query
                .must_match
                .iter()
                .filter_map(|key| document_ids.get(key).copied().flatten())
                .collect::<BTreeSet<_>>();
            let forbidden = query
                .must_not_match_as_primary
                .iter()
                .filter_map(|key| document_ids.get(key).copied().flatten())
                .collect::<BTreeSet<_>>();
            let actual = result
                .hits
                .iter()
                .filter_map(|hit| hit.citation.document_id)
                .collect::<Vec<_>>();
            let matching = actual.iter().filter(|id| expected.contains(id)).count();
            let recall_at_k = if query.must_match.is_empty() {
                1.0
            } else {
                matching as f64 / query.must_match.len() as f64
            };
            let reciprocal_rank = actual
                .iter()
                .position(|id| expected.contains(id))
                .map(|index| 1.0 / (index + 1) as f64)
                .unwrap_or(0.0);
            let citation_accuracy = if actual.is_empty() {
                f64::from(query.must_match.is_empty())
            } else {
                matching as f64 / actual.len() as f64
            };
            let version_leakage = actual.iter().any(|id| forbidden.contains(id));
            let refusal_expected = query.expected_refusal;
            let refusal_correct = if refusal_expected {
                result.hits.is_empty()
            } else {
                true
            };
            cases.push(KnowledgeRetrievalEvaluationCaseResult {
                fixture_id: query.id,
                hit_count: i64::try_from(result.hits.len()).unwrap_or(i64::MAX),
                recall_at_k,
                reciprocal_rank,
                citation_accuracy,
                version_leakage,
                refusal_expected,
                refusal_correct,
                latency_ms: elapsed_millis(started),
            });
        }
        let case_count = i64::try_from(cases.len())
            .map_err(|_| AppError::Custom("固定评测案例数量超出范围".to_string()))?;
        if case_count == 0 {
            return Err(AppError::InvalidInput("固定检索评测集不能为空".to_string()));
        }
        let divisor = case_count as f64;
        let mut latencies = cases.iter().map(|case| case.latency_ms).collect::<Vec<_>>();
        latencies.sort_unstable();
        let p50_latency_ms = percentile_latency(&latencies, 0.50);
        let p95_latency_ms = percentile_latency(&latencies, 0.95);
        // 当前固定 fixture 只含关键词/版本等可复现实例，未随某个模型 Profile 发布查询
        // 向量。因此本次运行明确是 FTS 基线，不得伪装成活动 Profile 的混合回归结果。
        // 向量基线将在模型 Spike 确定后以 Profile 指纹和查询向量单独加入。
        let profile_id = None;
        db.save_knowledge_retrieval_evaluation_run(
            &fixture.fixture_version,
            profile_id,
            top_k,
            case_count,
            cases.iter().map(|case| case.recall_at_k).sum::<f64>() / divisor,
            cases.iter().map(|case| case.reciprocal_rank).sum::<f64>() / divisor,
            cases.iter().map(|case| case.citation_accuracy).sum::<f64>() / divisor,
            cases.iter().filter(|case| case.version_leakage).count() as f64 / divisor,
            cases.iter().filter(|case| case.refusal_correct).count() as f64 / divisor,
            p50_latency_ms,
            p95_latency_ms,
            &serde_json::to_value(&cases)?,
        )
    }

    fn recall_confirmed_relations(
        db: &Database,
        filters: &KnowledgeSearchInput,
        fts_hits: &[KnowledgeSearchHit],
        vector_hits: &[KnowledgeSearchHit],
        depth: i64,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchHit>, AppError> {
        const RELATION_FANOUT: usize = 8;
        let mut hits =
            Self::recall_confirmed_code_relations(db, filters, fts_hits, vector_hits, limit)?;
        // 未指定项目时，关系通道不能猜测同名外部 ID 的归属；保留 FTS/向量结果即可。
        if filters.project_ids.is_empty() {
            return Ok(hits);
        }
        let mut frontier = BTreeSet::new();
        for hit in fts_hits
            .iter()
            .chain(vector_hits)
            .take(limit.saturating_mul(2))
        {
            if let Some(document_id) = hit.citation.document_id {
                frontier.insert(("document".to_string(), document_id.to_string()));
            }
            if let Some(version_id) = hit.citation.document_version_id {
                frontier.insert(("document_version".to_string(), version_id.to_string()));
            }
            if let Some(chunk_id) = hit.citation.chunk_id {
                frontier.insert(("chunk".to_string(), chunk_id.to_string()));
            }
            if !hit.citation.commit_sha.is_empty() {
                frontier.insert(("commit".to_string(), hit.citation.commit_sha.clone()));
            }
        }
        let mut seen_relations = BTreeSet::new();
        for _ in 0..depth {
            let seeds = frontier.iter().cloned().collect::<Vec<_>>();
            frontier.clear();
            for (entity_type, entity_key) in seeds {
                let relations = db.list_knowledge_relations(&ListKnowledgeRelationsInput {
                    entity_type: Some(entity_type),
                    entity_key: Some(entity_key),
                    project_ids: filters.project_ids.clone(),
                    release_ids: filters.release_ids.clone(),
                    sensitivities: filters.sensitivities.clone(),
                    confirmed_only: Some(true),
                    limit: Some(RELATION_FANOUT as i64),
                })?;
                for relation in relations {
                    if relation.scope_status != "scoped" {
                        continue;
                    }
                    if !seen_relations.insert(relation.id) {
                        continue;
                    }
                    frontier.insert((relation.from_type.clone(), relation.from_key.clone()));
                    frontier.insert((relation.to_type.clone(), relation.to_key.clone()));
                    hits.push(KnowledgeSearchHit {
                        score: relation.confidence,
                        channels: vec!["relation".to_string()],
                        citation: crate::models::KnowledgeCitation {
                            citation_key: format!("relation:{}", relation.id),
                            source_type: "knowledge_relation".to_string(),
                            document_id: None,
                            document_version_id: None,
                            chunk_id: None,
                            project_id: None,
                            release_id: None,
                            title: format!(
                                "{} {} {}",
                                relation.from_key, relation.relation_type, relation.to_key
                            ),
                            logical_path: String::new(),
                            heading_path: String::new(),
                            commit_sha: if relation.from_type == "commit" {
                                relation.from_key.clone()
                            } else if relation.to_type == "commit" {
                                relation.to_key.clone()
                            } else {
                                String::new()
                            },
                            external_key: String::new(),
                            snapshot_id: None,
                            symbol_key: String::new(),
                            start_line: None,
                            end_line: None,
                            excerpt: String::new(),
                        },
                        content: String::new(),
                        diagnostics: serde_json::json!({
                            "relationType": relation.relation_type,
                            "confirmed": true,
                            "confidence": relation.confidence,
                            "source": relation.source,
                        }),
                    });
                    if hits.len() >= limit.saturating_mul(2) {
                        return Ok(hits);
                    }
                }
            }
        }
        Ok(hits)
    }

    /// 源码关系只能在调用方明确选择快照时扩展；这既避免工作树/历史版本串用，也让
    /// 关系表中未确认的正则推断永远不会成为 RAG 事实。关系目标必须回落到有效代码
    /// 片段，不能仅凭关系文本构造无法打开的引用。
    fn recall_confirmed_code_relations(
        db: &Database,
        filters: &KnowledgeSearchInput,
        fts_hits: &[KnowledgeSearchHit],
        vector_hits: &[KnowledgeSearchHit],
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchHit>, AppError> {
        let Some(snapshot_id) = filters.snapshot_id else {
            return Ok(Vec::new());
        };
        if !filters.document_types.is_empty()
            && !filters.document_types.contains(&"code".to_string())
        {
            return Ok(Vec::new());
        }
        if !filters.sensitivities.is_empty()
            && !filters.sensitivities.contains(&"internal".to_string())
        {
            return Ok(Vec::new());
        }
        let snapshot = db
            .get_knowledge_code_snapshot_by_id(snapshot_id)?
            .ok_or_else(|| AppError::NotFound(format!("源码快照不存在: {snapshot_id}")))?;
        if snapshot.status != "analyzed" {
            return Ok(Vec::new());
        }
        if !filters.source_ids.is_empty() && !filters.source_ids.contains(&snapshot.source_id) {
            return Ok(Vec::new());
        }
        let files = db.list_knowledge_code_files(snapshot_id)?;
        let file_by_id = files
            .iter()
            .filter(|file| file.status == "active" && file.sensitivity == "internal")
            .map(|file| (file.id, file))
            .collect::<HashMap<_, _>>();
        let symbols = db.list_knowledge_code_symbols(snapshot_id, None)?;
        let symbol_by_key = symbols
            .iter()
            .map(|symbol| (symbol.symbol_key.as_str(), symbol))
            .collect::<HashMap<_, _>>();
        let mut seen = BTreeSet::new();
        let mut hits = Vec::new();
        for seed in fts_hits.iter().chain(vector_hits).filter(|hit| {
            hit.citation.snapshot_id == Some(snapshot_id) && !hit.citation.symbol_key.is_empty()
        }) {
            let relations = db.list_knowledge_code_relations(
                snapshot_id,
                Some(&seed.citation.symbol_key),
                Some((limit.saturating_mul(2)) as i64),
            )?;
            for relation in relations.into_iter().filter(|relation| relation.confirmed) {
                let target_key = if relation.from_symbol_key == seed.citation.symbol_key {
                    relation.to_symbol_key.as_str()
                } else if relation.to_symbol_key == seed.citation.symbol_key {
                    relation.from_symbol_key.as_str()
                } else {
                    continue;
                };
                if target_key.is_empty() || !seen.insert((relation.id, target_key.to_string())) {
                    continue;
                }
                let Some(symbol) = symbol_by_key.get(target_key) else {
                    continue;
                };
                let Some(file) = file_by_id.get(&symbol.file_id) else {
                    continue;
                };
                let Some(document_version_id) = file.document_version_id else {
                    continue;
                };
                let chunks = db.list_knowledge_chunks(document_version_id)?;
                let Some(chunk) = chunks.iter().find(|chunk| {
                    chunk
                        .location
                        .get("symbolKey")
                        .and_then(serde_json::Value::as_str)
                        == Some(target_key)
                }) else {
                    continue;
                };
                let version = db
                    .get_knowledge_document_version_by_id(document_version_id)?
                    .ok_or_else(|| AppError::NotFound("源码文档版本不存在".to_string()))?;
                if !version.valid {
                    continue;
                }
                let document = db
                    .get_knowledge_document_by_id(version.document_id)?
                    .ok_or_else(|| AppError::NotFound("源码知识文档不存在".to_string()))?;
                hits.push(KnowledgeSearchHit {
                    score: relation.confidence,
                    channels: vec!["relation".to_string()],
                    citation: KnowledgeCitation {
                        citation_key: format!("code:snapshot:{snapshot_id}:chunk:{}", chunk.id),
                        source_type: "code_snapshot".to_string(),
                        document_id: Some(document.id),
                        document_version_id: Some(version.id),
                        chunk_id: Some(chunk.id),
                        project_id: document.project_id,
                        release_id: version.release_id,
                        title: document.title,
                        logical_path: if version.source_path.trim().is_empty() {
                            document.logical_path
                        } else {
                            version.source_path
                        },
                        heading_path: chunk.heading_path.clone(),
                        commit_sha: version.commit_sha,
                        external_key: String::new(),
                        snapshot_id: Some(snapshot_id),
                        symbol_key: symbol.symbol_key.clone(),
                        start_line: Some(symbol.start_line),
                        end_line: Some(symbol.end_line),
                        excerpt: chunk.content.chars().take(400).collect(),
                    },
                    content: if filters.include_context.unwrap_or(false) {
                        chunk.content.clone()
                    } else {
                        String::new()
                    },
                    diagnostics: serde_json::json!({
                        "relationId": relation.id,
                        "relationType": relation.relation_type,
                        "confirmed": true,
                        "confidence": relation.confidence,
                        "resolver": relation.resolver,
                        "evidenceLine": relation.evidence_start_line,
                    }),
                });
                if hits.len() >= limit.saturating_mul(2) {
                    return Ok(hits);
                }
            }
        }
        Ok(hits)
    }

    /// 入口源码内已经出现的调用符号可以作为可复核的二次检索词。与代码关系表不同，
    /// 这里不把“调用边”作为事实输出，而是必须再次命中目标源码片段才会补入上下文。
    /// 这使尚未人工确认的静态关系不会突破证据边界，同时解决入口方法与细则方法分块后
    /// 无法共同回答的问题。
    fn recall_called_code_context(
        db: &Database,
        filters: &KnowledgeSearchInput,
        seed_hits: &[KnowledgeSearchHit],
    ) -> Result<Vec<KnowledgeSearchHit>, AppError> {
        const MAX_DEPTH: usize = 2;
        const MAX_SYMBOLS: usize = 12;
        const MAX_HITS: usize = 12;

        let mut frontier = BTreeSet::new();
        for hit in seed_hits
            .iter()
            .filter(|hit| is_code_evidence_hit(hit) && has_method_symbol(hit))
        {
            for symbol in called_symbol_candidates(&hit.content) {
                // 第一跳只在入口所在文件内找，避免接口声明中的方法名或外部 SDK 调用
                // 抢占上下文；从已命中的批处理方法继续第二跳时才允许跨到 Mapper/SQL。
                frontier.insert((
                    hit.citation.snapshot_id,
                    hit.citation.logical_path.clone(),
                    symbol,
                    true,
                ));
            }
        }

        let mut seen_symbols = BTreeSet::new();
        let mut seen_citations = seed_hits
            .iter()
            .map(|hit| hit.citation.citation_key.clone())
            .collect::<BTreeSet<_>>();
        let mut hits = Vec::new();
        for _ in 0..MAX_DEPTH {
            let mut targets = frontier
                .iter()
                .filter(|target| seen_symbols.insert((*target).clone()))
                .cloned()
                .collect::<Vec<_>>();
            // 限额内优先沿着业务处理/查询调用向下补证据。否则入口中大量日期、集合和
            // 日志工具调用会占满两跳配额，导致真正的 Mapper/SQL 条件无法进入模型上下文。
            targets.sort_by(|left, right| {
                (called_symbol_priority(&left.2), &left.2)
                    .cmp(&(called_symbol_priority(&right.2), &right.2))
            });
            targets.truncate(MAX_SYMBOLS);
            frontier.clear();
            if targets.is_empty() {
                break;
            }
            for (snapshot_id, source_path, symbol, restrict_to_source_path) in targets {
                let mut related_filters = filters.clone();
                related_filters.query = symbol;
                // 快照是不可变的；入口来自源码快照时，后续细则绝不能跨到同版本的另一仓库
                // 快照或历史快照。没有快照的普通文档仍由既有项目/版本硬过滤约束。
                related_filters.snapshot_id = snapshot_id;
                related_filters.limit = Some(3);
                related_filters.include_context = Some(true);
                for hit in Self::search_fts(db, related_filters)? {
                    if !is_code_evidence_hit(&hit)
                        || hit.citation.snapshot_id != snapshot_id
                        || (restrict_to_source_path && hit.citation.logical_path != source_path)
                        || !seen_citations.insert(hit.citation.citation_key.clone())
                    {
                        continue;
                    }
                    for next_symbol in called_symbol_candidates(&hit.content) {
                        frontier.insert((
                            hit.citation.snapshot_id,
                            hit.citation.logical_path.clone(),
                            next_symbol,
                            false,
                        ));
                    }
                    hits.push(hit);
                    if hits.len() >= MAX_HITS {
                        return Ok(hits);
                    }
                }
            }
        }
        Ok(hits)
    }
}

fn is_code_evidence_hit(hit: &KnowledgeSearchHit) -> bool {
    hit.citation.source_type == "code_snapshot" || is_code_path(&hit.citation.logical_path)
}

fn is_implementation_source_hit(hit: &KnowledgeSearchHit) -> bool {
    is_code_evidence_hit(hit)
        && !hit.citation.logical_path.starts_with("code-reports/")
        && !is_test_evidence_hit(hit)
}

fn is_test_evidence_hit(hit: &KnowledgeSearchHit) -> bool {
    let path = hit.citation.logical_path.to_ascii_lowercase();
    path.contains("/src/test/")
        || path.contains("/tests/")
        || path.starts_with("tests/")
        || path.ends_with("test.java")
        || path.ends_with("tests.java")
        || path.contains(".test.")
        || path.contains(".spec.")
        || hit
            .citation
            .logical_path
            .ends_with("code-reports/test-map.md")
}

fn is_requirement_baseline_hit(hit: &KnowledgeSearchHit) -> bool {
    if is_code_evidence_hit(hit) || hit.citation.logical_path.starts_with("code-reports/") {
        return false;
    }
    let searchable = format!(
        "{}\n{}\n{}",
        hit.citation.title, hit.citation.logical_path, hit.citation.heading_path
    )
    .to_ascii_lowercase();
    ["需求", "requirement", "prd", "用户故事", "story"]
        .iter()
        .any(|token| searchable.contains(token))
}

fn requirement_hit_matches_release(hit: &KnowledgeSearchHit, query: &str) -> bool {
    let searchable =
        format!("{}\n{}", hit.citation.title, hit.citation.logical_path).to_ascii_lowercase();
    captures(query, r"(?i)v?\d+(?:\.\d+){1,3}(?:[-+][a-z0-9.-]+)?")
        .into_iter()
        .flat_map(|version| {
            let normalized = normalized_release_name(&version);
            let family = normalized.trim_end_matches(".0").to_string();
            [normalized, family]
        })
        .filter(|version| !version.is_empty())
        .any(|version| contains_version_token(&searchable, &version))
}

fn contains_version_token(text: &str, version: &str) -> bool {
    text.match_indices(version).any(|(start, matched)| {
        let previous = text[..start].chars().next_back();
        let next = text[start + matched.len()..].chars().next();
        !previous.is_some_and(|character| character.is_ascii_digit() || character == '.')
            && !next.is_some_and(|character| character.is_ascii_digit() || character == '.')
    })
}

/// 从需求基线中提取可独立检索的条目。这里只生成检索候选，不生成需求事实；最终
/// 回答仍须引用原需求片段，因此容忍 DOCX、Markdown 和纯文本的常见列表格式。
fn extract_requirement_candidates(hits: &[KnowledgeSearchHit]) -> Vec<String> {
    let mut candidates = Vec::new();
    for line in hits.iter().flat_map(|hit| hit.content.lines()) {
        let normalized = line
            .trim()
            .trim_start_matches(|character: char| {
                character.is_ascii_digit()
                    || matches!(character, '.' | '、' | '-' | '*' | '#' | ')' | '）')
                    || character.is_whitespace()
            })
            .trim();
        let length = normalized.chars().count();
        let has_action = [
            "新增", "优化", "支持", "实现", "修改", "调整", "删除", "同步", "接口", "功能", "修复",
            "改用", "限制",
        ]
        .iter()
        .any(|token| normalized.contains(token));
        if (8..=320).contains(&length)
            && has_action
            && !candidates.iter().any(|item| item == normalized)
        {
            candidates.push(normalized.to_string());
        }
        if candidates.len() >= 12 {
            break;
        }
    }
    candidates
}

fn coverage_code_query(requirement: &str) -> String {
    let mut parts = requirement
        .split(|character| {
            matches!(
                character,
                '，' | '。' | '；' | '：' | '、' | ',' | ';' | ':' | '(' | ')' | '（' | '）'
            )
        })
        .map(str::trim)
        .filter(|part| (3..=40).contains(&part.chars().count()))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    for identifier in captures(requirement, r"\b[A-Za-z_][A-Za-z0-9_]{2,}\b") {
        if !parts
            .iter()
            .any(|part| part.eq_ignore_ascii_case(&identifier))
        {
            parts.insert(0, identifier);
        }
    }
    parts.truncate(8);
    parts.join(" ")
}

fn coverage_code_queries(requirement: &str) -> Vec<String> {
    const FOCUSED_SIGNALS: &[&str] = &[
        "一键删除",
        "批量删除",
        "删除按钮",
        "删除确认",
        "勾选",
        "已确认",
        "未确认",
        "新增或更新",
        "批量插入",
        "批量更新",
        "同步接口",
        "数据同步",
        "上传中台",
    ];
    let mut queries = FOCUSED_SIGNALS
        .iter()
        .filter(|signal| requirement.contains(**signal))
        .take(4)
        .map(|signal| (*signal).to_string())
        .collect::<Vec<_>>();
    let broad_query = coverage_code_query(requirement);
    if !broad_query.is_empty() && !queries.iter().any(|query| query == &broad_query) {
        queries.push(broad_query);
    }
    queries
}

fn coverage_test_queries(
    requirement: &str,
    implementation_hits: &[&KnowledgeSearchHit],
) -> Vec<String> {
    let mut queries: Vec<String> = Vec::new();
    for hit in implementation_hits {
        for identifier in captures(&hit.content, r"\b[A-Za-z_][A-Za-z0-9_]{3,}\b")
            .into_iter()
            .chain(captures(
                &hit.citation.heading_path,
                r"\b[A-Za-z_][A-Za-z0-9_]{3,}\b",
            ))
        {
            if is_test_query_identifier(&identifier)
                && !queries
                    .iter()
                    .any(|query| query.eq_ignore_ascii_case(&identifier))
            {
                queries.push(identifier);
            }
            if queries.len() >= 8 {
                break;
            }
        }
        if queries.len() >= 8 {
            break;
        }
    }
    for query in coverage_code_queries(requirement) {
        if !queries.iter().any(|existing| existing == &query) {
            queries.push(query);
        }
    }
    queries
}

fn is_test_query_identifier(identifier: &str) -> bool {
    const STOP_WORDS: &[&str] = &[
        "public",
        "private",
        "protected",
        "return",
        "const",
        "function",
        "template",
        "class",
        "string",
        "boolean",
        "number",
        "object",
        "undefined",
        "response",
        "request",
        "current",
        "handle",
        "click",
        "change",
        "value",
        "label",
        "option",
        "button",
        "dialog",
        "message",
    ];
    identifier.len() >= 5
        && (identifier.chars().any(char::is_uppercase) || identifier.contains('_'))
        && !STOP_WORDS.contains(&identifier.to_ascii_lowercase().as_str())
}

fn test_candidate_priority(
    requirement: &str,
    implementation_hits: &[&KnowledgeSearchHit],
    hit: &KnowledgeSearchHit,
) -> (u8, u8, u8) {
    let searchable = format!(
        "{}\n{}\n{}\n{}",
        hit.citation.title, hit.citation.logical_path, hit.citation.heading_path, hit.content
    )
    .to_ascii_lowercase();
    let implementation_identifiers = implementation_hits
        .iter()
        .flat_map(|implementation| {
            captures(
                &format!(
                    "{}\n{}\n{}",
                    implementation.citation.logical_path,
                    implementation.citation.heading_path,
                    implementation.content
                ),
                r"\b[A-Za-z_][A-Za-z0-9_]{3,}\b",
            )
        })
        .filter(|identifier| is_test_query_identifier(identifier))
        .collect::<BTreeSet<_>>();
    let identifier_matches = implementation_identifiers
        .iter()
        .filter(|identifier| searchable.contains(&identifier.to_ascii_lowercase()))
        .count()
        .min(u8::MAX as usize) as u8;
    let behavior_matches = coverage_code_queries(requirement)
        .iter()
        .filter(|signal| searchable.contains(&signal.to_ascii_lowercase()))
        .count()
        .min(u8::MAX as usize) as u8;
    (
        u8::MAX.saturating_sub(identifier_matches),
        u8::MAX.saturating_sub(behavior_matches),
        if hit.citation.source_type == "code_snapshot" {
            0
        } else {
            1
        },
    )
}

fn implementation_candidate_priority(requirement: &str, hit: &KnowledgeSearchHit) -> (u8, u8, u8) {
    let requirement_lower = requirement.to_ascii_lowercase();
    let path = hit.citation.logical_path.to_ascii_lowercase();
    let searchable = format!(
        "{}\n{}\n{}",
        hit.citation.title, hit.citation.logical_path, hit.content
    )
    .to_ascii_lowercase();
    let identifiers = captures(requirement, r"\b[A-Za-z_][A-Za-z0-9_]{2,}\b");
    let identifier_match = identifiers
        .iter()
        .filter(|identifier| {
            !matches!(
                identifier.to_ascii_lowercase().as_str(),
                "pms" | "ai" | "api"
            )
        })
        .any(|identifier| searchable.contains(&identifier.to_ascii_lowercase()));
    let asks_frontend = ["按钮", "页面", "列表", "勾选", "弹窗", "助手"]
        .iter()
        .any(|token| requirement_lower.contains(token));
    let is_frontend = [".vue", ".tsx", ".ts", ".jsx", ".js"]
        .iter()
        .any(|extension| path.ends_with(extension));
    let asks_service = ["接口", "同步", "新增", "更新"]
        .iter()
        .any(|token| requirement_lower.contains(token));
    let is_service = ["api", "service", "controller", "mapper"]
        .iter()
        .any(|token| path.contains(token));
    let is_sql = path.ends_with(".sql");

    let semantic_priority = if identifier_match
        || (asks_frontend && is_frontend)
        || (!asks_frontend && asks_service && is_service)
    {
        0
    } else if is_sql
        && !["数据库", "数据表", "建表", "sql"]
            .iter()
            .any(|token| requirement_lower.contains(token))
    {
        3
    } else {
        1
    };
    let source_priority = if hit.citation.source_type == "code_snapshot" {
        0
    } else {
        1
    };
    // 页面入口和埋点也会重复出现“今日工作安排”等模块名称，但只有真实实现通常同时
    // 包含按钮、勾选、确认、批量删除或同步等行为短语。先保留不可变代码快照，再在
    // 同类来源中按行为重合度排序，避免上传原型或通用模块词频挤占两个候选槽位。
    const BEHAVIOR_SIGNALS: &[&str] = &[
        "一键删除",
        "批量删除",
        "删除按钮",
        "删除确认",
        "勾选",
        "多选",
        "已确认",
        "未确认",
        "新增或更新",
        "批量插入",
        "批量更新",
        "同步接口",
        "数据同步",
        "上传中台",
    ];
    let behavior_matches = BEHAVIOR_SIGNALS
        .iter()
        .filter(|signal| requirement_lower.contains(**signal) && searchable.contains(**signal))
        .count()
        .min(u8::MAX as usize) as u8;
    (
        source_priority,
        semantic_priority,
        u8::MAX.saturating_sub(behavior_matches),
    )
}

fn append_unique_requirement_candidates(
    requirement: &str,
    hits: Vec<KnowledgeSearchHit>,
    seen_implementation: &mut BTreeSet<String>,
    implementation_hits: &mut Vec<KnowledgeSearchHit>,
) {
    let mut added = 0;
    for mut hit in hits {
        if !seen_implementation.insert(hit.citation.citation_key.clone()) {
            continue;
        }
        hit.diagnostics["coverageRole"] = serde_json::json!("implementationCandidate");
        hit.diagnostics["coverageRequirement"] = serde_json::json!(requirement);
        implementation_hits.push(hit);
        added += 1;
        if added >= 4 {
            break;
        }
    }
}

fn append_unique_test_candidates(
    requirement: &str,
    hits: Vec<KnowledgeSearchHit>,
    seen_tests: &mut BTreeSet<String>,
    test_hits: &mut Vec<KnowledgeSearchHit>,
) {
    let mut added = 0;
    for mut hit in hits {
        if !seen_tests.insert(hit.citation.citation_key.clone()) {
            continue;
        }
        hit.diagnostics["coverageRole"] = serde_json::json!("testCandidate");
        hit.diagnostics["coverageRequirement"] = serde_json::json!(requirement);
        hit.diagnostics["executionStatus"] = serde_json::json!("notVerified");
        test_hits.push(hit);
        added += 1;
        if added >= 4 {
            break;
        }
    }
}

fn append_round_robin_coverage_hits(
    selected: &mut Vec<KnowledgeSearchHit>,
    citations: &mut BTreeSet<String>,
    hits: &[KnowledgeSearchHit],
    max_additions: usize,
) {
    let mut groups = Vec::<(String, Vec<&KnowledgeSearchHit>)>::new();
    for hit in hits {
        let requirement = hit
            .diagnostics
            .get("coverageRequirement")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("__fallback__");
        if let Some((_, group_hits)) = groups
            .iter_mut()
            .find(|(group_requirement, _)| group_requirement == requirement)
        {
            group_hits.push(hit);
        } else {
            groups.push((requirement.to_string(), vec![hit]));
        }
    }

    let mut additions = 0;
    let mut round = 0;
    while additions < max_additions {
        let mut progressed = false;
        for (_, group_hits) in &groups {
            let Some(hit) = group_hits.get(round) else {
                continue;
            };
            progressed = true;
            if citations.insert(hit.citation.citation_key.clone()) {
                selected.push((*hit).clone());
                additions += 1;
                if additions >= max_additions {
                    break;
                }
            }
        }
        if !progressed {
            break;
        }
        round += 1;
    }
}

fn select_coverage_context_hits(
    requirement_hits: &[KnowledgeSearchHit],
    implementation_hits: &[KnowledgeSearchHit],
    test_hits: &[KnowledgeSearchHit],
    report_hits: &[KnowledgeSearchHit],
) -> Vec<KnowledgeSearchHit> {
    const REQUIREMENT_HIT_LIMIT: usize = 4;
    const IMPLEMENTATION_HIT_LIMIT: usize = 8;
    const TEST_HIT_LIMIT: usize = 8;
    const CONTEXT_HIT_LIMIT: usize = 21;
    let mut selected = Vec::new();
    let mut citations = BTreeSet::new();
    append_context_hits(
        &mut selected,
        &mut citations,
        requirement_hits,
        REQUIREMENT_HIT_LIMIT,
    );
    // 实现与测试候选都按需求轮转，避免第一条需求占满全局上下文。各 8 个槽位
    // 可完整容纳两条需求各四个实现和测试候选；更多需求也会先获得公平首轮。
    append_round_robin_coverage_hits(
        &mut selected,
        &mut citations,
        implementation_hits,
        IMPLEMENTATION_HIT_LIMIT,
    );
    append_round_robin_coverage_hits(&mut selected, &mut citations, test_hits, TEST_HIT_LIMIT);
    append_context_hits(
        &mut selected,
        &mut citations,
        report_hits,
        CONTEXT_HIT_LIMIT,
    );
    selected
}

fn has_method_symbol(hit: &KnowledgeSearchHit) -> bool {
    let symbol = hit
        .citation
        .symbol_key
        .rsplit("::")
        .next()
        .and_then(|symbol| symbol.split('@').next())
        .filter(|symbol| !symbol.is_empty())
        .or_else(|| hit.citation.heading_path.rsplit('#').next())
        .and_then(|symbol| symbol.chars().next());
    symbol.is_some_and(char::is_lowercase)
}

fn called_symbol_candidates(code: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "if",
        "for",
        "while",
        "switch",
        "catch",
        "return",
        "new",
        "try",
        "throw",
        "else",
        "format",
        "size",
        "stream",
        "filter",
        "map",
        "collect",
        "tolist",
        "isempty",
        "isnotempty",
        "get",
        "set",
        "add",
        "remove",
        "split",
        "distinct",
    ];
    let Ok(pattern) = Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(") else {
        return Vec::new();
    };
    pattern
        .captures_iter(code)
        .filter_map(|captures| captures.get(1).map(|capture| capture.as_str()))
        .filter(|symbol| {
            symbol.len() >= 5
                && symbol.chars().any(char::is_uppercase)
                && !STOP_WORDS.contains(&symbol.to_ascii_lowercase().as_str())
        })
        .fold(Vec::new(), |mut symbols, symbol| {
            if !symbols.iter().any(|existing| existing == symbol) {
                symbols.push(symbol.to_string());
            }
            symbols
        })
}

/// 标题用于定位文档，全文片段用于说明“为什么命中”。同一正式版本同时命中两者时，
/// 结果仍按标题优先排序，但必须展示全文命中的段落和行号，不能退化成文档首段摘要。
fn merge_title_and_fts_hits(
    title_hits: Vec<KnowledgeSearchHit>,
    fts_hits: Vec<KnowledgeSearchHit>,
    limit: usize,
) -> Vec<KnowledgeSearchHit> {
    let mut merged = BTreeMap::<String, KnowledgeSearchHit>::new();
    for hit in title_hits.into_iter().chain(fts_hits) {
        let key = hit
            .citation
            .document_version_id
            .map(|version_id| format!("document-version:{version_id}"))
            .unwrap_or_else(|| hit.citation.citation_key.clone());
        let incoming_is_fts = hit.channels.iter().any(|channel| channel == "fts");
        match merged.get_mut(&key) {
            Some(existing) => {
                let existing_is_fts = existing.channels.iter().any(|channel| channel == "fts");
                for channel in hit.channels {
                    if !existing.channels.contains(&channel) {
                        existing.channels.push(channel);
                    }
                }
                existing.score = existing.score.max(hit.score);
                if incoming_is_fts && !existing_is_fts {
                    existing.citation = hit.citation;
                    if !hit.content.is_empty() {
                        existing.content = hit.content;
                    }
                } else if existing.content.is_empty() && !hit.content.is_empty() {
                    existing.content = hit.content;
                    existing.citation.excerpt = hit.citation.excerpt;
                }
            }
            None => {
                merged.insert(key, hit);
            }
        }
    }
    let mut hits = merged.into_values().collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        let left_title_match = left.channels.iter().any(|channel| channel == "title");
        let right_title_match = right.channels.iter().any(|channel| channel == "title");
        right_title_match
            .cmp(&left_title_match)
            .then_with(|| right.score.total_cmp(&left.score))
            .then_with(|| left.citation.citation_key.cmp(&right.citation.citation_key))
    });
    hits.truncate(limit);
    hits
}

fn fuse_rrf(channels: [(&str, &[KnowledgeSearchHit]); 3], limit: usize) -> Vec<KnowledgeSearchHit> {
    const RRF_K: f64 = 60.0;
    let mut fused = BTreeMap::<String, KnowledgeSearchHit>::new();
    for (channel, hits) in channels {
        for (index, hit) in hits.iter().enumerate() {
            let contribution = 1.0 / (RRF_K + (index + 1) as f64);
            let key = hit.citation.citation_key.clone();
            let entry = fused.entry(key).or_insert_with(|| KnowledgeSearchHit {
                score: 0.0,
                channels: Vec::new(),
                citation: hit.citation.clone(),
                content: hit.content.clone(),
                diagnostics: hit.diagnostics.clone(),
            });
            if !entry.diagnostics.is_object() {
                entry.diagnostics = serde_json::json!({});
            }
            if entry.diagnostics.get("rrf").is_none() {
                entry.diagnostics["rrf"] = serde_json::json!({});
            }
            if entry.diagnostics.get("channelDiagnostics").is_none() {
                entry.diagnostics["channelDiagnostics"] = serde_json::json!({});
            }
            entry.diagnostics["channelDiagnostics"][channel] = hit.diagnostics.clone();
            entry.score += contribution;
            if !entry.channels.iter().any(|existing| existing == channel) {
                entry.channels.push(channel.to_string());
            }
            if entry.content.is_empty() && !hit.content.is_empty() {
                entry.content = hit.content.clone();
            }
            entry.diagnostics["rrf"][channel] = serde_json::json!({
                "rank": index + 1,
                "contribution": contribution,
            });
        }
    }
    let mut hits = fused.into_values().collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.citation.citation_key.cmp(&right.citation.citation_key))
    });
    hits.truncate(limit);
    hits
}

fn elapsed_millis(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

fn percentile_latency(sorted: &[i64], percentile: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((sorted.len() as f64 * percentile).ceil() as usize).saturating_sub(1);
    sorted[rank.min(sorted.len() - 1)]
}

/// RRF 负责合并排序名次；下面的规则只使用可验证的查询标识和引用元数据，保持每次
/// 排序稳定且可在诊断中解释。不存在足够元数据的“陈旧”候选不会被伪造处罚。
fn apply_fusion_signals(
    mut hits: Vec<KnowledgeSearchHit>,
    filters: &KnowledgeSearchInput,
    analysis: &KnowledgeQueryAnalysis,
) -> Vec<KnowledgeSearchHit> {
    for hit in &mut hits {
        let mut adjustments = BTreeMap::<String, f64>::new();
        if !filters.project_ids.is_empty()
            && hit
                .citation
                .project_id
                .is_some_and(|project_id| filters.project_ids.contains(&project_id))
        {
            adjustments.insert("exactProject".to_string(), 0.010);
        }
        if !filters.release_ids.is_empty()
            && hit
                .citation
                .release_id
                .is_some_and(|release_id| filters.release_ids.contains(&release_id))
        {
            adjustments.insert("exactVersion".to_string(), 0.014);
        }
        let searchable = format!(
            "{}\n{}\n{}\n{}",
            hit.citation.title, hit.citation.logical_path, hit.citation.symbol_key, hit.content
        )
        .to_ascii_lowercase();
        if analysis.requirement_ids.iter().any(|id| {
            searchable.contains(&id.to_ascii_lowercase())
                || hit.citation.external_key.eq_ignore_ascii_case(id)
        }) {
            adjustments.insert("requirementId".to_string(), 0.020);
        }
        if analysis.code_symbols.iter().any(|symbol| {
            searchable.contains(&symbol.to_ascii_lowercase())
                || hit.citation.symbol_key.eq_ignore_ascii_case(symbol)
        }) {
            adjustments.insert("codeSymbol".to_string(), 0.012);
        }
        if hit.channels.iter().any(|channel| channel == "relation")
            && hit
                .diagnostics
                .get("confirmed")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        {
            adjustments.insert("confirmedRelation".to_string(), 0.016);
        }
        // FTS/向量候选均来自有效文档版本，且查询 SQL 已验证来源处于 enabled 状态。
        if hit.citation.document_version_id.is_some() {
            adjustments.insert("verifiedDocument".to_string(), 0.006);
        }
        if is_high_confidence_title_hit(hit) {
            adjustments.insert("highConfidenceTitle".to_string(), 0.030);
        }
        // 当前 schema 尚未保存可跨通道比较的“陈旧时间线”，因此显式记录 0 而非把未版本化
        // 内容错误认定为陈旧。后续迁移提供版本时间线后可保持该诊断键不变地加入处罚。
        adjustments.insert("stalePenalty".to_string(), 0.0);
        let delta = adjustments.values().sum::<f64>();
        hit.score += delta;
        hit.diagnostics["fusionSignals"] =
            serde_json::to_value(&adjustments).unwrap_or_else(|_| serde_json::json!({}));
    }
    hits.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.citation.citation_key.cmp(&right.citation.citation_key))
    });
    hits
}

fn is_high_confidence_title_hit(hit: &KnowledgeSearchHit) -> bool {
    hit.channels.iter().any(|channel| channel == "title")
        && hit
            .diagnostics
            .get("highConfidenceTitleMatch")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
}

fn render_evidence_context(hits: &[&KnowledgeSearchHit]) -> String {
    hits.iter()
        .map(|hit| {
            let coverage_role = hit
                .diagnostics
                .get("coverageRole")
                .and_then(serde_json::Value::as_str)
                .map(|role| match role {
                    "requirementBaseline" => "需求基线",
                    "requirementBaselineFallback" => "需求基线（降级匹配）",
                    "implementationCandidate" => "实现候选（待核对）",
                    "implementationCandidateFallback" => "实现候选（降级匹配）",
                    "testCandidate" => "测试源码候选（未验证执行结果）",
                    "releaseMetadata" => "版本元数据",
                    _ => role,
                });
            let coverage_requirement = hit
                .diagnostics
                .get("coverageRequirement")
                .and_then(serde_json::Value::as_str);
            format!(
                "[{}]\n项目={}; 版本={}; 来源={}; 位置={}{}{}{}{}\n{}",
                hit.citation.citation_key,
                hit.citation
                    .project_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "未标注".to_string()),
                hit.citation
                    .release_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unversioned".to_string()),
                hit.citation.source_type,
                hit.citation.logical_path,
                if hit.citation.heading_path.is_empty() {
                    ""
                } else {
                    "#"
                },
                hit.citation.heading_path,
                coverage_role
                    .map(|role| format!("; 证据角色={role}"))
                    .unwrap_or_default(),
                coverage_requirement
                    .map(|requirement| format!("; 匹配需求={requirement}"))
                    .unwrap_or_default(),
                hit.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn find_conflicts(hits: &[KnowledgeSearchHit]) -> Vec<String> {
    let mut locations = BTreeMap::<String, BTreeSet<String>>::new();
    for hit in hits {
        if hit.citation.logical_path.is_empty() || hit.citation.commit_sha.is_empty() {
            continue;
        }
        // 每个仓库快照都会生成同名报告；在缺少 source_id 的旧引用模型中按路径比较
        // 会把正常的多仓库证据误报为冲突。源码文件仍保持原有提交冲突检测。
        if hit.citation.logical_path.starts_with("code-reports/") {
            continue;
        }
        locations
            .entry(hit.citation.logical_path.clone())
            .or_default()
            .insert(hit.citation.commit_sha.clone());
    }
    locations
        .into_iter()
        .filter_map(|(path, commits)| {
            (commits.len() > 1).then(|| format!("同一路径存在多个提交版本证据: {path}"))
        })
        .collect()
}

fn select_context_hits(
    primary_hits: &[KnowledgeSearchHit],
    related_code_hits: &[KnowledgeSearchHit],
) -> Vec<KnowledgeSearchHit> {
    const CONTEXT_HIT_LIMIT: usize = 12;
    const PRIMARY_HIT_RESERVE: usize = 6;
    const HIGH_CONFIDENCE_TITLE_RESERVE: usize = 2;
    let mut context_hits = Vec::new();
    let mut context_citations = BTreeSet::new();
    let high_confidence_title_hits = primary_hits
        .iter()
        .filter(|hit| is_high_confidence_title_hit(hit))
        .take(HIGH_CONFIDENCE_TITLE_RESERVE)
        .cloned()
        .collect::<Vec<_>>();
    let primary_reserve = primary_hits.len().min(PRIMARY_HIT_RESERVE);
    // 标题只预留两个槽位，既保证版本化需求文档不被通用正文淹没，也为入口、正文和
    // 两跳调用链留下至少四个位置，避免大量相似标题反向挤掉真正的规则证据。
    append_context_hits(
        &mut context_hits,
        &mut context_citations,
        &high_confidence_title_hits,
        CONTEXT_HIT_LIMIT,
    );
    append_context_hits(
        &mut context_hits,
        &mut context_citations,
        &primary_hits[..primary_reserve],
        CONTEXT_HIT_LIMIT,
    );
    append_context_hits(
        &mut context_hits,
        &mut context_citations,
        related_code_hits,
        CONTEXT_HIT_LIMIT,
    );
    append_context_hits(
        &mut context_hits,
        &mut context_citations,
        &primary_hits[primary_reserve..],
        CONTEXT_HIT_LIMIT,
    );
    context_hits
}

fn append_context_hits(
    context_hits: &mut Vec<KnowledgeSearchHit>,
    context_citations: &mut BTreeSet<String>,
    candidates: &[KnowledgeSearchHit],
    limit: usize,
) {
    for hit in candidates {
        if context_hits.len() >= limit {
            break;
        }
        if hit.citation.chunk_id.is_some()
            && !hit.content.trim().is_empty()
            && context_citations.insert(hit.citation.citation_key.clone())
        {
            context_hits.push(hit.clone());
        }
    }
}

fn called_symbol_priority(symbol: &str) -> u8 {
    let normalized = symbol.to_ascii_lowercase();
    if normalized.starts_with("process")
        || normalized.starts_with("generate")
        || normalized.starts_with("select")
        || normalized.starts_with("query")
        || normalized.starts_with("find")
        || normalized.starts_with("load")
    {
        0
    } else if normalized.starts_with("create")
        || normalized.starts_with("build")
        || normalized.starts_with("preassign")
        || normalized.starts_with("update")
        || normalized.starts_with("save")
    {
        1
    } else {
        2
    }
}

fn evidence_gaps(search: &KnowledgeSearchInput, hits: &[KnowledgeSearchHit]) -> Vec<String> {
    let mut gaps = Vec::new();
    if hits.is_empty() {
        gaps.push("没有匹配当前过滤条件的知识证据".to_string());
        return gaps;
    }
    let text = hits
        .iter()
        .map(|hit| format!("{}\n{}", hit.citation.title, hit.content))
        .collect::<String>()
        .to_ascii_lowercase();
    for (kind, tokens) in [
        ("需求", &["req", "story", "需求"][..]),
        ("实现", &["implementation", "实现", "commit", "代码"][..]),
        ("测试", &["test", "测试", "case"][..]),
    ] {
        let has_code_snapshot = kind == "实现"
            && hits.iter().any(|hit| {
                hit.citation.source_type == "code_snapshot"
                    || is_code_path(&hit.citation.logical_path)
            });
        if !has_code_snapshot && !tokens.iter().any(|token| text.contains(token)) {
            gaps.push(format!("未找到明确的{kind}证据"));
        }
    }
    if !search.release_ids.is_empty() && hits.iter().all(|hit| hit.citation.release_id.is_none()) {
        gaps.push("命中证据未携带发布版本标识".to_string());
    }
    gaps
}

fn is_code_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    [
        ".java", ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".cs", ".vue", ".xml", ".sql",
    ]
    .iter()
    .any(|extension| path.ends_with(extension))
}

fn is_test_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.contains("/src/test/")
        || path.contains("/tests/")
        || path.starts_with("tests/")
        || path.ends_with("test.java")
        || path.ends_with("tests.java")
        || path.contains(".test.")
        || path.contains(".spec.")
}

/// 将当前页面会话的历史消息放进系统提示，而不是拼进本轮用户问题。这样各类
/// Provider 仍然收到当前问题作为 user prompt，同时可以利用“这个方法/上面的 SQL”
/// 等指代；历史内容逐条脱敏，并明确声明不能作为当前事实证据。
fn render_conversation_context(
    messages: &[KnowledgeConversationMessage],
) -> Result<String, AppError> {
    const MAX_MESSAGES: usize = 12;
    const MAX_TOTAL_CHARS: usize = 48_000;
    const MAX_MESSAGE_CHARS: usize = 8_000;

    let mut selected = messages
        .iter()
        .rev()
        .take(MAX_MESSAGES)
        .cloned()
        .collect::<Vec<_>>();
    selected.reverse();
    if selected
        .first()
        .is_some_and(|message| message.role.eq_ignore_ascii_case("assistant"))
    {
        selected.remove(0);
    }

    let mut total_chars = 0;
    let mut sections = Vec::new();
    for message in selected {
        let role = match message.role.trim().to_ascii_lowercase().as_str() {
            "user" => "用户",
            "assistant" => "助手",
            _ => {
                return Err(AppError::InvalidInput(
                    "项目问答历史消息角色只能是 user 或 assistant".to_string(),
                ));
            }
        };
        let content = message
            .content
            .trim()
            .chars()
            .take(MAX_MESSAGE_CHARS)
            .collect::<String>();
        if content.is_empty() {
            continue;
        }
        let sanitized = KnowledgePolicyService::sanitize_remote_ai_context(&content)?;
        let message_chars = sanitized.chars().count();
        if total_chars + message_chars > MAX_TOTAL_CHARS {
            break;
        }
        total_chars += message_chars;
        sections.push(format!("{role}：\n{sanitized}"));
    }
    Ok(sections.join("\n\n"))
}

fn evidence_gap_is_relevant_to_question(gap: &str, question: &str) -> bool {
    let question = question.to_ascii_lowercase();
    if gap.contains("需求") {
        return ["需求", "req", "story", "用户故事"]
            .iter()
            .any(|token| question.contains(token));
    }
    if gap.contains("测试") {
        return ["测试", "test", "用例", "验证"]
            .iter()
            .any(|token| question.contains(token));
    }
    if gap.contains("实现") {
        return ["代码", "实现", "逻辑", "规则", "生成", "如何", "怎么"]
            .iter()
            .any(|token| question.contains(token));
    }
    true
}

fn with_missing_evidence(mut gaps: Vec<String>, missing: &str) -> Vec<String> {
    if !gaps.iter().any(|gap| gap == missing) {
        gaps.push(missing.to_string());
    }
    gaps
}

fn original_question_or_retrieval_query(
    original_question: Option<&str>,
    retrieval_query: &str,
) -> String {
    original_question
        .map(str::trim)
        .filter(|question| !question.is_empty())
        .unwrap_or(retrieval_query)
        .to_string()
}

fn render_evidence_only_answer(preview: &KnowledgeRagContextPreview) -> String {
    let citations = preview
        .citations
        .iter()
        .map(|citation| format!("- [{}] {}", citation.citation_key, citation.title))
        .collect::<Vec<_>>()
        .join("\n");
    format!("以下为已检索到的可引用证据，未调用大模型：\n{}", citations)
}

fn render_coverage_evidence_only_answer(preview: &KnowledgeRagContextPreview) -> String {
    let mut requirement_rows = Vec::new();
    let mut implementation_rows = Vec::new();
    let mut test_rows = Vec::new();
    let mut metadata_rows = Vec::new();
    for citation in &preview.citations {
        let row = format!("- [{}] {}", citation.citation_key, citation.title);
        if citation.logical_path.starts_with("code-reports/") {
            metadata_rows.push(row);
        } else if is_test_path(&citation.logical_path) {
            test_rows.push(row);
        } else if citation.source_type == "code_snapshot" || is_code_path(&citation.logical_path) {
            implementation_rows.push(row);
        } else {
            requirement_rows.push(row);
        }
    }
    format!(
        "已按当前版本执行需求覆盖检索，未调用大模型。以下结果用于人工核对；代码候选不等于已经确认实现，测试源码候选也不等于已经执行通过。\n\n## 需求基线\n{}\n\n## 代码实现候选\n{}\n\n## 测试源码候选（未验证执行结果）\n{}\n\n## 版本元数据\n{}",
        if requirement_rows.is_empty() {
            "- 未找到明确需求基线".to_string()
        } else {
            requirement_rows.join("\n")
        },
        if implementation_rows.is_empty() {
            "- 未找到代码实现候选".to_string()
        } else {
            implementation_rows.join("\n")
        },
        if test_rows.is_empty() {
            "- 未找到测试源码候选".to_string()
        } else {
            test_rows.join("\n")
        },
        if metadata_rows.is_empty() {
            "- 无".to_string()
        } else {
            metadata_rows.join("\n")
        },
    )
}

/// 每个有事实正文的段落都必须绑定至少一个本次上下文中实际发送的 citation。该规则
/// 无法理解自然语言事实边界，但可阻止“一条引用覆盖整段未证实结论”和伪造引用键。
fn answer_has_valid_block_citations(answer: &str, citations: &[KnowledgeCitation]) -> bool {
    let allowed = citations
        .iter()
        .map(|citation| citation.citation_key.as_str())
        .collect::<HashSet<_>>();
    if allowed.is_empty() {
        return false;
    }
    let citation_regex = Regex::new(r"\[([^\]\r\n]+)\]").expect("引用正则必须有效");
    let blocks = answer
        .split("\n\n")
        .map(str::trim)
        .filter(|block| {
            !block.is_empty() && !block.starts_with('#') && !is_non_fact_framing_block(block)
        })
        .collect::<Vec<_>>();
    !blocks.is_empty()
        && blocks.iter().all(|block| {
            let keys = citation_regex
                .captures_iter(block)
                .filter_map(|captures| captures.get(1).map(|key| key.as_str()))
                .collect::<Vec<_>>();
            !keys.is_empty()
                && keys
                    .into_iter()
                    .all(|key| answer_citation_matches(key, &allowed, citations))
        })
}

fn is_non_fact_framing_block(block: &str) -> bool {
    let normalized = block.trim().trim_end_matches([':', '：']).trim();
    normalized.ends_with("如下")
        || matches!(
            normalized,
            "具体逻辑" | "具体逻辑如下" | "具体步骤" | "具体步骤如下" | "如下"
        )
}

fn answer_citation_matches(
    raw_key: &str,
    allowed: &HashSet<&str>,
    citations: &[KnowledgeCitation],
) -> bool {
    let normalized = raw_key.strip_prefix("citation:").unwrap_or(raw_key);
    if allowed.contains(normalized) {
        return true;
    }
    // 兼容 Provider 历史输出的 `[citation:chunk:<id>]` / `[citation:<id>]`。只有
    // 本次上下文中确实存在同一 chunk，才允许这种短格式，避免把任意数字当成证据。
    let chunk_id = normalized
        .strip_prefix("chunk:")
        .or_else(|| {
            normalized
                .chars()
                .all(|character| character.is_ascii_digit())
                .then_some(normalized)
        })
        .and_then(|value| value.parse::<i64>().ok());
    chunk_id.is_some_and(|id| {
        citations
            .iter()
            .any(|citation| citation.chunk_id == Some(id))
    })
}

fn citation_validation_status(answer: &str, citations: &[KnowledgeCitation]) -> &'static str {
    if answer_has_valid_block_citations(answer, citations) {
        "verified"
    } else {
        // 模型原始输出仍应反馈给用户；但它未满足逐段引用约束，不能被界面误认为
        // 已核验的项目事实。引用证据和警告会随结果一并返回，供用户自行复核。
        "unverified"
    }
}

fn normalize_ids(values: &mut Vec<i64>) {
    values.retain(|value| *value > 0);
    values.sort_unstable();
    values.dedup();
}

fn normalize_text_filters(values: Vec<String>, label: &str) -> Result<Vec<String>, AppError> {
    let mut normalized = values
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if normalized
        .iter()
        .any(|value| value.chars().any(char::is_whitespace))
    {
        return Err(AppError::InvalidInput(format!("{label}不能包含空白字符")));
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn captures(query: &str, pattern: &str) -> Vec<String> {
    Regex::new(pattern)
        .expect("静态知识查询解析正则必须有效")
        .find_iter(query)
        .map(|matched| matched.as_str().to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sql_tables(query: &str) -> Vec<String> {
    let expression =
        Regex::new(r"(?i)\b(?:from|join|update|into|table)\s+([A-Za-z_][A-Za-z0-9_]*)")
            .expect("静态 SQL 表解析正则必须有效");
    expression
        .captures_iter(query)
        .filter_map(|captures| captures.get(1).map(|matched| matched.as_str().to_string()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalized_release_name(value: &str) -> String {
    value
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::database::Database;
    use crate::models::{
        CreateKnowledgeDocumentVersionInput, KnowledgeAskInput, KnowledgeChunkWriteInput,
        KnowledgeCitation, KnowledgeConversationMessage, KnowledgeSearchHit, KnowledgeSearchInput,
        RunKnowledgeRetrievalEvaluationInput, UpsertKnowledgeDocumentInput,
        UpsertKnowledgeProjectInput, UpsertKnowledgeReleaseInput,
    };

    use super::{
        answer_has_valid_block_citations, append_unique_requirement_candidates,
        append_unique_test_candidates, called_symbol_candidates, citation_validation_status,
        coverage_code_queries, coverage_test_queries, evidence_gap_is_relevant_to_question,
        evidence_gaps, extract_requirement_candidates, find_conflicts, fuse_rrf,
        implementation_candidate_priority, is_code_evidence_hit, is_test_evidence_hit,
        merge_title_and_fts_hits, original_question_or_retrieval_query,
        render_conversation_context, select_context_hits, select_coverage_context_hits,
        KnowledgeRetrievalService,
    };

    #[tokio::test]
    async fn release_coverage_mode_balances_requirement_and_code_evidence(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        database.ensure_knowledge_fts()?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "coverage-project".to_string(),
            name: "覆盖分析项目".to_string(),
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
            version: "v1.2.0".to_string(),
            tag_name: String::new(),
            branch: "release/v1.2.0".to_string(),
            commit_sha: "coverage-commit".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let requirement = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "coverage-requirements".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "订单中心 v1.2.0 需求文档".to_string(),
            logical_path: "docs/v1.2.0-requirements.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: requirement.id,
                release_id: Some(release.id),
                version_label: "v1.2.0".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: "docs/v1.2.0-requirements.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "1. 新增订单一键删除功能。".to_string(),
                content_hash: "coverage-requirement-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "需求清单".to_string(),
                content: "1. 新增订单一键删除功能。".to_string(),
                content_hash: "coverage-requirement-chunk-v1".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;
        let implementation = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "coverage-code".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "code".to_string(),
            title: "OrderBatchDeleteService.java".to_string(),
            logical_path: "src/OrderBatchDeleteService.java".to_string(),
            sensitivity: "internal".to_string(),
            tags: vec!["code".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: implementation.id,
                release_id: Some(release.id),
                version_label: "v1.2.0".to_string(),
                git_branch: "release/v1.2.0".to_string(),
                commit_sha: "coverage-commit".to_string(),
                source_path: "src/OrderBatchDeleteService.java".to_string(),
                mime_type: "text/plain".to_string(),
                content: "void batchDeleteOrders() { // 订单一键删除 }".to_string(),
                content_hash: "coverage-code-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "OrderBatchDeleteService#batchDeleteOrders".to_string(),
                content: "batchDeleteOrders 实现订单一键删除。".to_string(),
                content_hash: "coverage-code-chunk-v1".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;
        let test_document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "coverage-code-test".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "code".to_string(),
            title: "OrderBatchDeleteServiceTest.java".to_string(),
            logical_path: "src/test/java/OrderBatchDeleteServiceTest.java".to_string(),
            sensitivity: "internal".to_string(),
            tags: vec!["code".to_string(), "test".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: test_document.id,
                release_id: Some(release.id),
                version_label: "v1.2.0".to_string(),
                git_branch: "release/v1.2.0".to_string(),
                commit_sha: "coverage-commit".to_string(),
                source_path: "src/test/java/OrderBatchDeleteServiceTest.java".to_string(),
                mime_type: "text/plain".to_string(),
                content: "batchDeleteOrders 测试订单一键删除。".to_string(),
                content_hash: "coverage-code-test-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "OrderBatchDeleteServiceTest#batchDeleteOrders".to_string(),
                content: "batchDeleteOrders 测试订单一键删除。".to_string(),
                content_hash: "coverage-code-test-chunk-v1".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;

        let result = KnowledgeRetrievalService::ask_with_query_vector(
            &database,
            KnowledgeAskInput {
                search: KnowledgeSearchInput {
                    query: "v1.2.0 实现了哪些需求 需求文档 代码实现".to_string(),
                    project_ids: vec![project.id],
                    release_ids: vec![release.id],
                    source_ids: Vec::new(),
                    document_types: Vec::new(),
                    sensitivities: Vec::new(),
                    snapshot_id: None,
                    limit: Some(20),
                    include_context: Some(true),
                },
                original_question: Some(
                    "请逐条分析 v1.2.0 的需求：哪些已实现，并通过代码和测试验证？".to_string(),
                ),
                answer_mode: Some("releaseRequirementCoverage".to_string()),
                provider_key: String::new(),
                model: String::new(),
                evidence_only: Some(true),
                conversation: Vec::new(),
            },
            None,
        )
        .await?;

        assert_eq!(
            result.retrieval_diagnostics["queryMode"],
            "releaseRequirementCoverage"
        );
        assert!(result
            .citations
            .iter()
            .any(|citation| citation.document_id == Some(requirement.id)));
        assert!(result
            .citations
            .iter()
            .any(|citation| citation.document_id == Some(implementation.id)));
        assert!(result
            .citations
            .iter()
            .any(|citation| citation.document_id == Some(test_document.id)));
        assert!(result.answer.contains("需求基线"));
        assert!(result.answer.contains("代码实现候选"));
        assert!(result.answer.contains("测试源码候选（未验证执行结果）"));
        assert!(
            result.retrieval_diagnostics["coverage"]["testCandidateCount"]
                .as_u64()
                .is_some_and(|count| count > 0)
        );
        assert_eq!(
            result.retrieval_diagnostics["coverage"]["verifiedRelationCount"],
            0
        );
        assert!(result
            .evidence_gaps
            .iter()
            .any(|gap| gap.contains("测试源码候选") && gap.contains("verified_by")));
        Ok(())
    }

    #[test]
    fn generated_reports_from_multiple_snapshots_are_not_source_conflicts() {
        let hits = [1_i64, 2_i64]
            .into_iter()
            .map(|snapshot_id| KnowledgeSearchHit {
                score: 1.0,
                channels: vec!["fts".to_string()],
                citation: KnowledgeCitation {
                    citation_key: format!("code:snapshot:{snapshot_id}:chunk:{snapshot_id}"),
                    source_type: "code_snapshot".to_string(),
                    document_id: Some(snapshot_id),
                    document_version_id: Some(snapshot_id),
                    chunk_id: Some(snapshot_id),
                    project_id: Some(1),
                    release_id: Some(1),
                    title: "版本实现".to_string(),
                    logical_path: "code-reports/release-implementation.md".to_string(),
                    heading_path: String::new(),
                    commit_sha: format!("commit-{snapshot_id}"),
                    external_key: String::new(),
                    snapshot_id: Some(snapshot_id),
                    symbol_key: String::new(),
                    start_line: Some(1),
                    end_line: Some(2),
                    excerpt: "版本元数据".to_string(),
                },
                content: "版本元数据".to_string(),
                diagnostics: serde_json::json!({}),
            })
            .collect::<Vec<_>>();

        assert!(find_conflicts(&hits).is_empty());
    }

    #[test]
    fn requirement_candidate_extraction_keeps_actionable_rows() {
        let hit = KnowledgeSearchHit {
            score: 1.0,
            channels: vec!["fts".to_string()],
            citation: KnowledgeCitation {
                citation_key: "document:1:version:1:chunk:1".to_string(),
                source_type: "knowledge_document".to_string(),
                document_id: Some(1),
                document_version_id: Some(1),
                chunk_id: Some(1),
                project_id: Some(1),
                release_id: Some(1),
                title: "需求".to_string(),
                logical_path: "docs/requirements.md".to_string(),
                heading_path: String::new(),
                commit_sha: String::new(),
                external_key: String::new(),
                snapshot_id: None,
                symbol_key: String::new(),
                start_line: Some(1),
                end_line: Some(3),
                excerpt: String::new(),
            },
            content: "1. 新增一键删除功能。\n背景说明。\n2. 优化同步接口超时限制。".to_string(),
            diagnostics: serde_json::json!({}),
        };
        let candidates = extract_requirement_candidates(&[hit]);
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0].contains("一键删除"));
        assert!(candidates[1].contains("同步接口"));
    }

    #[test]
    fn coverage_prefers_behavior_implementation_over_navigation_entry() {
        let hit = |path: &str, content: &str| KnowledgeSearchHit {
            score: 1.0,
            channels: vec!["fts".to_string()],
            citation: KnowledgeCitation {
                citation_key: format!("code:{path}"),
                source_type: "code_snapshot".to_string(),
                document_id: Some(1),
                document_version_id: Some(1),
                chunk_id: Some(1),
                project_id: Some(1),
                release_id: Some(1),
                title: "index.vue".to_string(),
                logical_path: path.to_string(),
                heading_path: path.to_string(),
                commit_sha: "commit".to_string(),
                external_key: String::new(),
                snapshot_id: Some(1),
                symbol_key: String::new(),
                start_line: Some(1),
                end_line: Some(10),
                excerpt: content.to_string(),
            },
            content: content.to_string(),
            diagnostics: serde_json::json!({}),
        };
        let requirement = "今日工作安排新增一键删除按钮，支持勾选多条后批量删除";
        let implementation = hit(
            "src/views/todayWorkSchedule/index.vue",
            "label: '一键删除'；勾选多条后调用 deleteSelectedWorkOrders 批量删除",
        );
        let navigation = hit(
            "src/views/fullWorkOrderCenter/index.vue",
            "今日工作安排和明日工作计划的页面导航入口",
        );
        let mut uploaded_prototype = hit(
            "upload/js_today-work.js",
            "一键删除、批量删除、删除按钮、删除确认、勾选和多选的交互原型",
        );
        uploaded_prototype.citation.source_type = "knowledge_document".to_string();

        assert!(
            implementation_candidate_priority(requirement, &implementation)
                < implementation_candidate_priority(requirement, &navigation)
        );
        assert!(
            implementation_candidate_priority(requirement, &implementation)
                < implementation_candidate_priority(requirement, &uploaded_prototype)
        );

        let immutable_snapshot_with_weaker_semantics =
            hit("src/domain/workflow.rs", "批量删除流程的不可变源码快照证据");
        let mut uploaded_document_with_stronger_semantics = hit(
            "upload/todayWorkSchedule.vue",
            "一键删除按钮、勾选多条、批量删除和删除确认",
        );
        uploaded_document_with_stronger_semantics
            .citation
            .source_type = "knowledge_document".to_string();
        assert!(
            implementation_candidate_priority(
                requirement,
                &immutable_snapshot_with_weaker_semantics,
            ) < implementation_candidate_priority(
                requirement,
                &uploaded_document_with_stronger_semantics,
            ),
            "不可变代码快照应先于上传文档，再比较语义和行为信号"
        );
    }

    #[test]
    fn coverage_context_round_robins_three_requirements_before_global_limit() {
        let implementation_hits = ["需求一", "需求二", "需求三"]
            .into_iter()
            .flat_map(|requirement| {
                (1..=4).map(move |candidate| KnowledgeSearchHit {
                    score: 1.0,
                    channels: vec!["fts".to_string()],
                    citation: KnowledgeCitation {
                        citation_key: format!("code:{requirement}:{candidate}"),
                        source_type: "code_snapshot".to_string(),
                        document_id: Some(candidate),
                        document_version_id: Some(candidate),
                        chunk_id: Some(candidate),
                        project_id: Some(1),
                        release_id: Some(1),
                        title: format!("{requirement}候选{candidate}"),
                        logical_path: format!("src/{requirement}/{candidate}.rs"),
                        heading_path: String::new(),
                        commit_sha: "commit".to_string(),
                        external_key: String::new(),
                        snapshot_id: Some(1),
                        symbol_key: String::new(),
                        start_line: Some(1),
                        end_line: Some(2),
                        excerpt: String::new(),
                    },
                    content: format!("{requirement}实现候选{candidate}"),
                    diagnostics: serde_json::json!({
                        "coverageRole": "implementationCandidate",
                        "coverageRequirement": requirement,
                    }),
                })
            })
            .collect::<Vec<_>>();

        let selected = select_coverage_context_hits(&[], &implementation_hits, &[], &[]);
        assert_eq!(selected.len(), 8);
        let requirements = selected
            .iter()
            .map(|hit| {
                hit.diagnostics["coverageRequirement"]
                    .as_str()
                    .expect("实现候选应保留需求归属")
            })
            .collect::<Vec<_>>();
        assert_eq!(&requirements[..3], &["需求一", "需求二", "需求三"]);
        assert_eq!(
            requirements
                .iter()
                .filter(|candidate| **candidate == "需求一")
                .count(),
            3
        );
        assert_eq!(
            requirements
                .iter()
                .filter(|candidate| **candidate == "需求二")
                .count(),
            3
        );
        assert_eq!(
            requirements
                .iter()
                .filter(|candidate| **candidate == "需求三")
                .count(),
            2
        );
    }

    #[test]
    fn overlapping_requirement_candidates_scan_past_duplicates_to_fill_four() {
        let candidate = |key: &str| KnowledgeSearchHit {
            score: 1.0,
            channels: vec!["fts".to_string()],
            citation: KnowledgeCitation {
                citation_key: format!("code:{key}"),
                source_type: "code_snapshot".to_string(),
                document_id: Some(1),
                document_version_id: Some(1),
                chunk_id: Some(1),
                project_id: Some(1),
                release_id: Some(1),
                title: key.to_string(),
                logical_path: format!("src/{key}.rs"),
                heading_path: String::new(),
                commit_sha: "commit".to_string(),
                external_key: String::new(),
                snapshot_id: Some(1),
                symbol_key: String::new(),
                start_line: Some(1),
                end_line: Some(2),
                excerpt: String::new(),
            },
            content: key.to_string(),
            diagnostics: serde_json::json!({}),
        };
        let mut seen = BTreeSet::new();
        let mut selected = Vec::new();
        append_unique_requirement_candidates(
            "需求一",
            ["a", "b", "c", "d"].into_iter().map(candidate).collect(),
            &mut seen,
            &mut selected,
        );
        append_unique_requirement_candidates(
            "需求二",
            ["a", "b", "c", "d", "e", "f", "g", "h"]
                .into_iter()
                .map(candidate)
                .collect(),
            &mut seen,
            &mut selected,
        );

        assert_eq!(selected.len(), 8);
        let second_requirement = selected
            .iter()
            .filter(|hit| hit.diagnostics["coverageRequirement"] == "需求二")
            .map(|hit| hit.citation.citation_key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(second_requirement, ["code:e", "code:f", "code:g", "code:h"]);
    }

    #[test]
    fn coverage_test_queries_follow_implementation_symbols_and_keep_test_candidates_separate() {
        let implementation = KnowledgeSearchHit {
            score: 1.0,
            channels: vec!["fts".to_string()],
            citation: KnowledgeCitation {
                citation_key: "code:implementation".to_string(),
                source_type: "code_snapshot".to_string(),
                document_id: Some(1),
                document_version_id: Some(1),
                chunk_id: Some(1),
                project_id: Some(1),
                release_id: Some(1),
                title: "WorkOrderDataSyncJob.java".to_string(),
                logical_path: "src/main/java/WorkOrderDataSyncJob.java".to_string(),
                heading_path: "WorkOrderDataSyncJob#handleBatch".to_string(),
                commit_sha: "commit".to_string(),
                external_key: String::new(),
                snapshot_id: Some(1),
                symbol_key: "WorkOrderDataSyncJob::handleBatch".to_string(),
                start_line: Some(1),
                end_line: Some(20),
                excerpt: String::new(),
            },
            content: "batchInsertOrUpdate(items); updateAfterBatch(checkpoint);".to_string(),
            diagnostics: serde_json::json!({}),
        };
        let queries = coverage_test_queries("新增 pms 数据同步接口并批量更新", &[&implementation]);
        assert!(queries.contains(&"batchInsertOrUpdate".to_string()));
        assert!(queries.contains(&"updateAfterBatch".to_string()));

        let test_hit = KnowledgeSearchHit {
            citation: KnowledgeCitation {
                citation_key: "code:test".to_string(),
                title: "WorkOrderDataSyncJobTest.java".to_string(),
                logical_path: "src/test/java/WorkOrderDataSyncJobTest.java".to_string(),
                ..implementation.citation.clone()
            },
            content: "verify(api).batchInsertOrUpdate(items);".to_string(),
            ..implementation.clone()
        };
        assert!(is_test_evidence_hit(&test_hit));
        assert!(!super::is_implementation_source_hit(&test_hit));

        let mut seen = BTreeSet::new();
        let mut selected = Vec::new();
        append_unique_test_candidates(
            "新增 pms 数据同步接口并批量更新",
            vec![test_hit],
            &mut seen,
            &mut selected,
        );
        assert_eq!(selected[0].diagnostics["coverageRole"], "testCandidate");
        assert_eq!(selected[0].diagnostics["executionStatus"], "notVerified");
    }

    #[test]
    fn coverage_searches_stable_behavior_phrases_before_the_broad_requirement() {
        let queries = coverage_code_queries(
            "明日工作计划新增一键删除按钮，勾选多条后批量删除，已确认后按钮消失",
        );

        assert_eq!(queries[0], "一键删除");
        assert!(queries.contains(&"批量删除".to_string()));
        assert!(queries.contains(&"勾选".to_string()));
        assert!(queries
            .last()
            .is_some_and(|query| query.contains("明日工作计划")));
    }

    #[test]
    fn ordinary_mapper_and_sql_documents_count_as_code_evidence() {
        for path in [
            "mapper/OrderMapper.xml",
            "db/order_query.sql",
            "web/OrderPage.vue",
        ] {
            let hit = KnowledgeSearchHit {
                score: 1.0,
                channels: vec!["fts".to_string()],
                citation: KnowledgeCitation {
                    citation_key: format!("document:{path}"),
                    source_type: "knowledge_document".to_string(),
                    document_id: Some(1),
                    document_version_id: Some(1),
                    chunk_id: Some(1),
                    project_id: Some(1),
                    release_id: Some(1),
                    title: path.to_string(),
                    logical_path: path.to_string(),
                    heading_path: String::new(),
                    commit_sha: String::new(),
                    external_key: String::new(),
                    snapshot_id: None,
                    symbol_key: String::new(),
                    start_line: None,
                    end_line: None,
                    excerpt: "代码证据".to_string(),
                },
                content: "代码证据".to_string(),
                diagnostics: serde_json::json!({}),
            };
            assert!(is_code_evidence_hit(&hit), "{path} 应保留为代码证据");
        }
    }

    #[test]
    fn conversation_context_is_sanitized_and_kept_out_of_current_evidence() {
        let rendered = render_conversation_context(&[
            KnowledgeConversationMessage {
                role: "user".to_string(),
                content: "上面方法的 token=sk-test123 怎么处理？".to_string(),
            },
            KnowledgeConversationMessage {
                role: "assistant".to_string(),
                content: "它会继续使用当前证据。".to_string(),
            },
        ])
        .expect("历史消息应能脱敏");
        assert!(rendered.contains("用户："));
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("sk-test123"));
    }

    #[test]
    fn analysis_extracts_versioned_code_and_sql_identifiers(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        db.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "billing".to_string(),
            name: "计费平台".to_string(),
            aliases: vec!["收费".to_string()],
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let analysis = KnowledgeRetrievalService::analyze_query(
            &db,
            "计费平台 v1.6.0 的 REQ-1042 如何实现？提交 abc1234，路径 src/api/order.ts，路由 /api/orders，OrderService::create，SELECT * FROM orders WHERE o.id = ?",
        )?;
        assert_eq!(analysis.project_ids.len(), 1);
        assert_eq!(analysis.releases, vec!["v1.6.0"]);
        assert_eq!(analysis.requirement_ids, vec!["REQ-1042"]);
        assert_eq!(analysis.commit_shas, vec!["abc1234"]);
        assert!(analysis.paths.contains(&"src/api/order.ts".to_string()));
        assert!(analysis.api_routes.contains(&"/api/orders".to_string()));
        assert!(analysis
            .code_symbols
            .contains(&"OrderService::create".to_string()));
        assert_eq!(analysis.tables, vec!["orders"]);
        assert!(analysis.fields.contains(&"o.id".to_string()));
        Ok(())
    }

    #[test]
    fn hard_filters_scope_release_to_project_and_default_to_safe_sensitivities(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        let project = db.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "billing".to_string(),
            name: "计费平台".to_string(),
            aliases: vec!["收费".to_string()],
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let other = db.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "other".to_string(),
            name: "其他项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let release = db.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.6.0".to_string(),
            tag_name: "v1.6.0".to_string(),
            branch: "release/v1.6.0".to_string(),
            commit_sha: "abc1234".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let scoped = KnowledgeRetrievalService::apply_hard_filters(
            &db,
            KnowledgeSearchInput {
                query: "收费的历史方案".to_string(),
                project_ids: Vec::new(),
                release_ids: vec![release.id],
                source_ids: Vec::new(),
                document_types: vec!["Requirement".to_string()],
                sensitivities: Vec::new(),
                snapshot_id: None,
                limit: Some(500),
                include_context: Some(true),
            },
        )?;
        assert_eq!(scoped.project_ids, vec![project.id]);
        assert_eq!(scoped.sensitivities, vec!["public", "internal"]);
        assert_eq!(scoped.document_types, vec!["requirement"]);
        assert_eq!(scoped.limit, Some(100));
        let version_query = "收费 v1.6.0 的历史方案";
        assert_eq!(
            KnowledgeRetrievalService::analyze_query(&db, version_query)?.releases,
            vec!["v1.6.0"]
        );
        assert_eq!(db.list_knowledge_releases(project.id)?[0].version, "v1.6.0");
        let inferred_release = KnowledgeRetrievalService::apply_hard_filters(
            &db,
            KnowledgeSearchInput {
                query: version_query.to_string(),
                project_ids: Vec::new(),
                release_ids: Vec::new(),
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: Vec::new(),
                snapshot_id: None,
                limit: None,
                include_context: None,
            },
        )?;
        assert_eq!(inferred_release.project_ids, vec![project.id]);
        assert_eq!(inferred_release.release_ids, vec![release.id]);
        assert!(KnowledgeRetrievalService::apply_hard_filters(
            &db,
            KnowledgeSearchInput {
                query: "历史方案".to_string(),
                project_ids: vec![other.id],
                release_ids: vec![release.id],
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: Vec::new(),
                snapshot_id: None,
                limit: None,
                include_context: None,
            },
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn ambiguous_project_alias_refuses_recall_until_user_selects_scope(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        for key in ["billing-a", "billing-b"] {
            db.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
                id: None,
                project_key: key.to_string(),
                name: key.to_string(),
                aliases: vec!["订单".to_string()],
                description: String::new(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: "main".to_string(),
                enabled: true,
            })?;
        }
        let error = KnowledgeRetrievalService::apply_hard_filters(
            &db,
            KnowledgeSearchInput {
                query: "订单历史实现".to_string(),
                project_ids: Vec::new(),
                release_ids: Vec::new(),
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: Vec::new(),
                snapshot_id: None,
                limit: None,
                include_context: None,
            },
        )
        .expect_err("重名项目别名不得自动跨项目召回");
        assert!(error.to_string().contains("显式选择项目"));
        Ok(())
    }

    #[test]
    fn rrf_fusion_preserves_channel_diagnostics_and_stable_order() {
        let hit = |key: &str, channel: &str| KnowledgeSearchHit {
            score: 1.0,
            channels: vec![channel.to_string()],
            citation: KnowledgeCitation {
                citation_key: key.to_string(),
                source_type: "knowledge_document".to_string(),
                document_id: Some(1),
                document_version_id: Some(1),
                chunk_id: Some(1),
                project_id: Some(1),
                release_id: None,
                title: key.to_string(),
                logical_path: String::new(),
                heading_path: String::new(),
                commit_sha: String::new(),
                external_key: String::new(),
                snapshot_id: None,
                symbol_key: String::new(),
                start_line: None,
                end_line: None,
                excerpt: String::new(),
            },
            content: String::new(),
            diagnostics: serde_json::json!({"sourceDiagnostic": channel}),
        };
        let fts = vec![hit("shared", "fts"), hit("fts-only", "fts")];
        let vectors = vec![hit("shared", "vector")];
        let fused = fuse_rrf([("fts", &fts), ("vector", &vectors), ("relation", &[])], 10);
        assert_eq!(fused[0].citation.citation_key, "shared");
        assert_eq!(fused[0].channels, vec!["fts", "vector"]);
        assert_eq!(fused[0].diagnostics["rrf"]["fts"]["rank"], 1);
        assert_eq!(
            fused[0].diagnostics["channelDiagnostics"]["fts"]["sourceDiagnostic"],
            "fts"
        );
        assert_eq!(
            fused[0].diagnostics["channelDiagnostics"]["vector"]["sourceDiagnostic"],
            "vector"
        );
    }

    #[test]
    fn title_and_full_text_match_keeps_relevant_full_text_citation() {
        let hit = |channel: &str,
                   document_version_id: i64,
                   chunk_id: i64,
                   score: f64,
                   content: &str|
         -> KnowledgeSearchHit {
            KnowledgeSearchHit {
                score,
                channels: vec![channel.to_string()],
                citation: KnowledgeCitation {
                    citation_key: format!(
                        "document:1:version:{document_version_id}:chunk:{chunk_id}"
                    ),
                    source_type: "knowledge_document".to_string(),
                    document_id: Some(1),
                    document_version_id: Some(document_version_id),
                    chunk_id: Some(chunk_id),
                    project_id: Some(1),
                    release_id: Some(1),
                    title: "退款审批说明".to_string(),
                    logical_path: "docs/refund.md".to_string(),
                    heading_path: format!("段落 {chunk_id}"),
                    commit_sha: String::new(),
                    external_key: String::new(),
                    snapshot_id: None,
                    symbol_key: String::new(),
                    start_line: Some(chunk_id),
                    end_line: Some(chunk_id),
                    excerpt: content.to_string(),
                },
                content: content.to_string(),
                diagnostics: serde_json::json!({}),
            }
        };

        let merged = merge_title_and_fts_hits(
            vec![hit("title", 7, 1, 1.0, "文档开头的概览。")],
            vec![
                hit("fts", 7, 2, 0.2, "退款超过限额时必须由主管审批。"),
                hit("fts", 8, 3, 99.0, "仅全文命中的其他文档。"),
            ],
            10,
        );

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].citation.document_version_id, Some(7));
        assert_eq!(merged[0].citation.chunk_id, Some(2));
        assert_eq!(merged[0].content, "退款超过限额时必须由主管审批。");
        assert_eq!(merged[0].channels, vec!["title", "fts"]);
        assert_eq!(merged[1].citation.document_version_id, Some(8));
    }

    #[test]
    fn title_and_full_text_match_keeps_full_text_citation_without_context() {
        let hit = |channel: &str, chunk_id: i64, excerpt: &str| KnowledgeSearchHit {
            score: 1.0,
            channels: vec![channel.to_string()],
            citation: KnowledgeCitation {
                citation_key: format!("document:1:version:7:chunk:{chunk_id}"),
                source_type: "knowledge_document".to_string(),
                document_id: Some(1),
                document_version_id: Some(7),
                chunk_id: Some(chunk_id),
                project_id: Some(1),
                release_id: Some(1),
                title: "退款审批说明".to_string(),
                logical_path: "docs/refund.md".to_string(),
                heading_path: format!("段落 {chunk_id}"),
                commit_sha: String::new(),
                external_key: String::new(),
                snapshot_id: None,
                symbol_key: String::new(),
                start_line: Some(chunk_id),
                end_line: Some(chunk_id),
                excerpt: excerpt.to_string(),
            },
            content: String::new(),
            diagnostics: serde_json::json!({}),
        };

        let merged = merge_title_and_fts_hits(
            vec![hit("title", 1, "文档开头的概览。")],
            vec![hit("fts", 2, "退款超过限额时必须由主管审批。")],
            10,
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].citation.chunk_id, Some(2));
        assert_eq!(merged[0].citation.excerpt, "退款超过限额时必须由主管审批。");
        assert!(merged[0].content.is_empty());
        assert_eq!(merged[0].channels, vec!["title", "fts"]);
    }

    #[test]
    fn answer_must_reference_each_fact_block_with_an_allowed_citation_key() {
        let citation = KnowledgeCitation {
            citation_key: "document:1:version:1:chunk:1".to_string(),
            source_type: "knowledge_document".to_string(),
            document_id: Some(1),
            document_version_id: Some(1),
            chunk_id: Some(1),
            project_id: Some(1),
            release_id: Some(1),
            title: "需求".to_string(),
            logical_path: "requirements/a.md".to_string(),
            heading_path: String::new(),
            commit_sha: String::new(),
            external_key: String::new(),
            snapshot_id: None,
            symbol_key: String::new(),
            start_line: None,
            end_line: None,
            excerpt: String::new(),
        };
        assert!(answer_has_valid_block_citations(
            "结论 [document:1:version:1:chunk:1]\n\n另一项结论 [document:1:version:1:chunk:1]",
            &[citation.clone()]
        ));
        assert!(answer_has_valid_block_citations(
            "历史 Provider 格式 [citation:document:1:version:1:chunk:1]",
            &[citation.clone()]
        ));
        assert!(answer_has_valid_block_citations(
            "短格式引用 [citation:chunk:1]",
            &[citation.clone()]
        ));
        let mut code_citation = citation.clone();
        code_citation.citation_key = "code:snapshot:2:chunk:8051".to_string();
        code_citation.chunk_id = Some(8051);
        assert!(answer_has_valid_block_citations(
            "候选 SQL [citation:code:snapshot:2:chunk:8051]",
            &[code_citation.clone()]
        ));
        assert!(answer_has_valid_block_citations(
            "候选 SQL [code:snapshot:2:chunk:8051]",
            &[code_citation]
        ));
        assert!(answer_has_valid_block_citations(
            "具体逻辑如下：\n\n候选 SQL [document:1:version:1:chunk:1]",
            &[citation.clone()]
        ));
        assert!(!answer_has_valid_block_citations(
            "未引用的结论",
            &[citation.clone()]
        ));
        assert!(!answer_has_valid_block_citations(
            "伪造引用 [document:999:version:1:chunk:1]",
            &[citation]
        ));
        assert_eq!(
            citation_validation_status(
                "未引用的模型原始回答",
                &[KnowledgeCitation {
                    citation_key: "document:1:version:1:chunk:1".to_string(),
                    source_type: "knowledge_document".to_string(),
                    document_id: Some(1),
                    document_version_id: Some(1),
                    chunk_id: Some(1),
                    project_id: Some(1),
                    release_id: Some(1),
                    title: "需求".to_string(),
                    logical_path: "requirements/a.md".to_string(),
                    heading_path: String::new(),
                    commit_sha: String::new(),
                    external_key: String::new(),
                    snapshot_id: None,
                    symbol_key: String::new(),
                    start_line: None,
                    end_line: None,
                    excerpt: String::new(),
                }]
            ),
            "unverified"
        );
    }

    #[test]
    fn original_question_is_preserved_for_the_model_prompt() {
        assert_eq!(
            original_question_or_retrieval_query(
                Some("明日工作计划生成时如何校验分公司权限？"),
                "generateTomorrowPlan",
            ),
            "明日工作计划生成时如何校验分公司权限？"
        );
        assert_eq!(
            original_question_or_retrieval_query(Some("  "), "generateTomorrowPlan"),
            "generateTomorrowPlan"
        );
    }

    #[test]
    fn called_symbol_candidates_keep_business_calls_and_skip_language_noise() {
        let symbols = called_symbol_candidates(
            "if (CollUtil.isEmpty(workers)) { return; } processWorkerBatch(batch, planDate);",
        );
        assert!(symbols.contains(&"processWorkerBatch".to_string()));
        assert!(!symbols.contains(&"if".to_string()));
        assert!(!symbols.contains(&"isEmpty".to_string()));
    }

    #[test]
    fn entrypoint_context_includes_called_batch_and_candidate_rule_evidence(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        db.ensure_knowledge_fts()?;
        let project = db.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "tomorrow-plan".to_string(),
            name: "明日计划".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let release = db.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: String::new(),
            branch: "main".to_string(),
            commit_sha: "tomorrow-plan-commit".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "tomorrow-plan-source".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "code".to_string(),
            title: "TomorrowPlanApiImpl.java".to_string(),
            logical_path: "provider/TomorrowPlanApiImpl.java".to_string(),
            sensitivity: "internal".to_string(),
            tags: vec!["code".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        db.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: Some(release.id),
                version_label: "v1.0.0".to_string(),
                git_branch: "main".to_string(),
                commit_sha: "tomorrow-plan-commit".to_string(),
                source_path: "provider/TomorrowPlanApiImpl.java".to_string(),
                mime_type: "text/x-java-source".to_string(),
                content: "tomorrow plan source".to_string(),
                content_hash: "tomorrow-plan-source-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 30,
            },
            &[
                KnowledgeChunkWriteInput {
                    chunk_index: 0,
                    heading_path: "TomorrowPlanApiImpl#generateTomorrowPlan".to_string(),
                    content: "void generateTomorrowPlan() { processWorkerBatch(); }".to_string(),
                    content_hash: "tomorrow-plan-entry".to_string(),
                    location: serde_json::json!({"startLine": 1, "endLine": 1}),
                    token_estimate: 8,
                },
                KnowledgeChunkWriteInput {
                    chunk_index: 1,
                    heading_path: "TomorrowPlanApiImpl#processWorkerBatch".to_string(),
                    content: "void processWorkerBatch() { selectCandidateWorkOrdersByUserIds(); }"
                        .to_string(),
                    content_hash: "tomorrow-plan-batch".to_string(),
                    location: serde_json::json!({"startLine": 2, "endLine": 2}),
                    token_estimate: 8,
                },
                KnowledgeChunkWriteInput {
                    chunk_index: 2,
                    heading_path: "BdaWorkOrderDetailMapper#selectCandidateWorkOrdersByUserIds"
                        .to_string(),
                    content:
                        "selectCandidateWorkOrdersByUserIds：仅查询分配给当前网格员的待办工单。"
                            .to_string(),
                    content_hash: "tomorrow-plan-candidate".to_string(),
                    location: serde_json::json!({"startLine": 3, "endLine": 3}),
                    token_estimate: 10,
                },
            ],
        )?;

        let preview = KnowledgeRetrievalService::preview_rag_context(
            &db,
            KnowledgeSearchInput {
                query: "generateTomorrowPlan".to_string(),
                project_ids: vec![project.id],
                release_ids: vec![release.id],
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: Vec::new(),
                snapshot_id: None,
                limit: Some(20),
                include_context: Some(true),
            },
        )?;

        assert!(preview.context.contains("processWorkerBatch"));
        assert!(preview.context.contains("仅查询分配给当前网格员的待办工单"));
        assert!(preview
            .citations
            .iter()
            .any(|citation| citation.heading_path.contains("processWorkerBatch")));
        assert!(preview.citations.iter().any(|citation| {
            citation
                .heading_path
                .contains("selectCandidateWorkOrdersByUserIds")
        }));
        Ok(())
    }

    #[test]
    fn context_quota_keeps_related_code_evidence_when_primary_hits_are_full() {
        let make_hit = |key: String, heading: String| KnowledgeSearchHit {
            score: 1.0,
            channels: vec!["fts".to_string()],
            citation: KnowledgeCitation {
                citation_key: key,
                source_type: "knowledge_document".to_string(),
                document_id: Some(1),
                document_version_id: Some(1),
                chunk_id: Some(1),
                project_id: Some(1),
                release_id: Some(1),
                title: "测试证据".to_string(),
                logical_path: "src/Test.java".to_string(),
                heading_path: heading,
                commit_sha: "commit".to_string(),
                external_key: String::new(),
                snapshot_id: None,
                symbol_key: String::new(),
                start_line: None,
                end_line: None,
                excerpt: "证据".to_string(),
            },
            content: "证据".to_string(),
            diagnostics: serde_json::json!({}),
        };
        let primary = (0..12)
            .map(|index| make_hit(format!("primary:{index}"), format!("入口{index}")))
            .collect::<Vec<_>>();
        let related = vec![make_hit(
            "related:mapper".to_string(),
            "Mapper 查询规则".to_string(),
        )];
        let context = select_context_hits(&primary, &related);

        assert_eq!(context.len(), 12);
        assert!(context
            .iter()
            .any(|hit| hit.citation.citation_key == "related:mapper"));
    }

    #[test]
    fn high_confidence_titles_do_not_consume_the_related_code_quota() {
        let make_hit = |index: usize| KnowledgeSearchHit {
            score: 1.0,
            channels: vec!["title".to_string()],
            citation: KnowledgeCitation {
                citation_key: format!("title:{index}"),
                source_type: "knowledge_document".to_string(),
                document_id: Some(index as i64 + 1),
                document_version_id: Some(index as i64 + 1),
                chunk_id: Some(index as i64 + 1),
                project_id: Some(1),
                release_id: Some(1),
                title: format!("高置信需求文档 {index}"),
                logical_path: format!("requirements/{index}.md"),
                heading_path: String::new(),
                commit_sha: String::new(),
                external_key: String::new(),
                snapshot_id: None,
                symbol_key: String::new(),
                start_line: None,
                end_line: None,
                excerpt: "需求".to_string(),
            },
            content: "需求".to_string(),
            diagnostics: serde_json::json!({"highConfidenceTitleMatch": true}),
        };
        let primary = (0..12).map(make_hit).collect::<Vec<_>>();
        let mut related = make_hit(99);
        related.channels = vec!["relation".to_string()];
        related.citation.citation_key = "related:implementation".to_string();
        related.diagnostics = serde_json::json!({});

        let context = select_context_hits(&primary, &[related]);

        assert_eq!(context.len(), 12);
        assert!(context
            .iter()
            .any(|hit| hit.citation.citation_key == "related:implementation"));
    }

    #[test]
    fn versioned_requirement_question_keeps_the_matching_title_among_noisy_minutes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        db.ensure_knowledge_fts()?;
        let project = db.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "all-work-orders".to_string(),
            name: "全业务工单中心".to_string(),
            aliases: vec!["全业务工单".to_string()],
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let release = db.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.2.0".to_string(),
            tag_name: "v1.2.0".to_string(),
            branch: "release/v1.2.0".to_string(),
            commit_sha: "all-work-orders-v1.2.0".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let requirement = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "all-work-orders-v1.2-requirements".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "需求与进度_全业务工单1.2版本需求文档".to_string(),
            logical_path: "requirements/all-work-orders-v1.2.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        db.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: requirement.id,
                release_id: Some(release.id),
                version_label: "v1.2.0".to_string(),
                git_branch: "release/v1.2.0".to_string(),
                commit_sha: "all-work-orders-v1.2.0".to_string(),
                source_path: "requirements/all-work-orders-v1.2.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "新增统一工单池，并补充派发、签收和归档流程。".to_string(),
                content_hash: "all-work-orders-requirements-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 20,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "功能范围".to_string(),
                content: "新增统一工单池，并补充派发、签收和归档流程。".to_string(),
                content_hash: "all-work-orders-requirements-chunk-v1".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 20,
            }],
        )?;

        for (index, wrong_version) in ["11.2", "1.20", "1.2.1"].into_iter().enumerate() {
            let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: format!("wrong-version-{index}"),
                project_id: Some(project.id),
                source_id: None,
                doc_type: "markdown".to_string(),
                title: format!("全业务工单{wrong_version}版本需求文档"),
                logical_path: format!("requirements/wrong-{index}.md"),
                sensitivity: "internal".to_string(),
                tags: Vec::new(),
                allow_ai: true,
                allow_mcp: false,
            })?;
            db.create_knowledge_document_version(
                &CreateKnowledgeDocumentVersionInput {
                    document_id: document.id,
                    release_id: Some(release.id),
                    version_label: "测试边界".to_string(),
                    git_branch: String::new(),
                    commit_sha: format!("wrong-version-{index}"),
                    source_path: format!("requirements/wrong-{index}.md"),
                    mime_type: "text/markdown".to_string(),
                    content: "这是其他版本的边界测试文档。".to_string(),
                    content_hash: format!("wrong-version-content-{index}"),
                    parsed_meta: serde_json::json!({}),
                    token_estimate: 10,
                },
                &[],
            )?;
        }

        for question in [
            "全业务工单 v1.2.0 版本的需求是什么",
            "请帮我查看全业务工单 v1.2.0 的主要需求是什么",
            "全业务工单 v1.2.0 的详细需求有哪些",
        ] {
            let title_hits = db.search_knowledge_document_title_hits(&KnowledgeSearchInput {
                query: question.to_string(),
                project_ids: vec![project.id],
                release_ids: vec![release.id],
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: vec!["internal".to_string()],
                snapshot_id: None,
                limit: Some(20),
                include_context: Some(true),
            })?;
            assert_eq!(
                title_hits.len(),
                1,
                "问题未精确命中目标版本标题: {question}"
            );
            assert_eq!(title_hits[0].citation.document_id, Some(requirement.id));
        }

        for index in 0..25 {
            let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: format!("meeting-minutes-{index}"),
                project_id: Some(project.id),
                source_id: None,
                doc_type: "markdown".to_string(),
                title: format!("项目会议纪要 {index}"),
                logical_path: format!("minutes/{index}.md"),
                sensitivity: "internal".to_string(),
                tags: Vec::new(),
                allow_ai: true,
                allow_mcp: false,
            })?;
            db.create_knowledge_document_version(
                &CreateKnowledgeDocumentVersionInput {
                    document_id: document.id,
                    release_id: Some(release.id),
                    version_label: "v1.2.0".to_string(),
                    git_branch: "release/v1.2.0".to_string(),
                    commit_sha: format!("meeting-{index}"),
                    source_path: format!("minutes/{index}.md"),
                    mime_type: "text/markdown".to_string(),
                    content: "全业务工单 v1.2.0 版本的需求是什么，会议暂未形成结论。".to_string(),
                    content_hash: format!("meeting-version-{index}"),
                    parsed_meta: serde_json::json!({}),
                    token_estimate: 20,
                },
                &[KnowledgeChunkWriteInput {
                    chunk_index: 0,
                    heading_path: "讨论记录".to_string(),
                    content: "全业务工单 v1.2.0 版本的需求是什么，会议暂未形成结论。".to_string(),
                    content_hash: format!("meeting-chunk-{index}"),
                    location: serde_json::json!({"startLine": 1, "endLine": 1}),
                    token_estimate: 20,
                }],
            )?;
        }

        let preview = KnowledgeRetrievalService::preview_rag_context(
            &db,
            KnowledgeSearchInput {
                query: "全业务工单 v1.2.0 版本的需求是什么".to_string(),
                project_ids: vec![project.id],
                release_ids: vec![release.id],
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: vec!["internal".to_string()],
                snapshot_id: None,
                limit: Some(20),
                include_context: Some(true),
            },
        )?;

        assert!(preview
            .citations
            .iter()
            .any(|citation| citation.document_id == Some(requirement.id)));
        assert!(preview.context.contains("新增统一工单池"));
        Ok(())
    }

    #[test]
    fn code_snapshot_counts_as_implementation_evidence_and_irrelevant_gaps_are_hidden() {
        let hit = KnowledgeSearchHit {
            score: 1.0,
            channels: vec!["fts".to_string()],
            citation: KnowledgeCitation {
                citation_key: "code:snapshot:2:chunk:885".to_string(),
                source_type: "code_snapshot".to_string(),
                document_id: Some(1),
                document_version_id: Some(1),
                chunk_id: Some(885),
                project_id: Some(1),
                release_id: Some(1),
                title: "TomorrowPlanApiImpl.java".to_string(),
                logical_path: "provider/TomorrowPlanApiImpl.java".to_string(),
                heading_path: "generateTomorrowPlan".to_string(),
                commit_sha: "commit".to_string(),
                external_key: String::new(),
                snapshot_id: Some(2),
                symbol_key: "TomorrowPlanApiImpl::generateTomorrowPlan".to_string(),
                start_line: Some(1),
                end_line: Some(10),
                excerpt: "明日计划生成实现".to_string(),
            },
            content: "public void generateTomorrowPlan() {}".to_string(),
            diagnostics: serde_json::json!({}),
        };
        let gaps = evidence_gaps(
            &KnowledgeSearchInput {
                query: "generateTomorrowPlan".to_string(),
                project_ids: vec![1],
                release_ids: vec![1],
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: Vec::new(),
                snapshot_id: None,
                limit: Some(20),
                include_context: Some(true),
            },
            &[hit],
        );

        assert!(!gaps.iter().any(|gap| gap.contains("实现")));
        assert!(gaps.iter().any(|gap| gap.contains("测试")));
        assert!(!evidence_gap_is_relevant_to_question(
            "未找到明确的测试证据",
            "明日工作计划的生成规则是什么？"
        ));
    }

    #[test]
    fn preview_uses_only_version_scoped_evidence_and_reports_gaps(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        db.ensure_knowledge_fts()?;
        let project = db.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "billing".to_string(),
            name: "计费平台".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let release = db.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.6.0".to_string(),
            tag_name: "v1.6.0".to_string(),
            branch: "release/v1.6.0".to_string(),
            commit_sha: "a1b2c3d".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "billing-req-1042".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "requirement".to_string(),
            title: "REQ-1042 退款审批".to_string(),
            logical_path: "requirements/REQ-1042.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: vec!["REQ-1042".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        db.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: Some(release.id),
                version_label: "v1.6.0".to_string(),
                git_branch: "release/v1.6.0".to_string(),
                commit_sha: "a1b2c3d".to_string(),
                source_path: "requirements/REQ-1042.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "退款审批需求".to_string(),
                content_hash: "req-1042-version".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 5,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "退款审批".to_string(),
                content: "REQ-1042 规定大额退款必须人工审批。".to_string(),
                content_hash: "req-1042-chunk".to_string(),
                location: serde_json::json!({"startLine": 2, "endLine": 2}),
                token_estimate: 5,
            }],
        )?;
        let preview = KnowledgeRetrievalService::preview_rag_context(
            &db,
            KnowledgeSearchInput {
                query: "退款审批".to_string(),
                project_ids: vec![project.id],
                release_ids: vec![release.id],
                source_ids: Vec::new(),
                document_types: Vec::new(),
                sensitivities: vec!["internal".to_string()],
                snapshot_id: None,
                limit: Some(10),
                include_context: None,
            },
        )?;
        assert_eq!(preview.citations.len(), 1);
        assert_eq!(preview.citations[0].release_id, Some(release.id));
        assert!(preview.context.contains("REQ-1042"));
        assert_eq!(
            preview.retrieval_diagnostics["dispatch"],
            "parallel-fts-vector-then-bounded-relations"
        );
        assert!(preview.evidence_gaps.iter().any(|gap| gap.contains("实现")));
        assert!(preview.evidence_gaps.iter().any(|gap| gap.contains("测试")));
        Ok(())
    }

    #[test]
    fn fixed_evaluation_persists_comparable_metrics() -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        let run = KnowledgeRetrievalService::run_fixed_evaluation(
            &db,
            RunKnowledgeRetrievalEvaluationInput { top_k: Some(5) },
        )?;
        assert_eq!(run.fixture_version, "knowledge-retrieval-baseline-v1");
        assert!(run.case_count >= 9);
        assert!((0.0..=1.0).contains(&run.recall_at_k));
        assert_eq!(db.list_knowledge_retrieval_evaluation_runs(10)?.len(), 1);
        Ok(())
    }
}
