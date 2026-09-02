use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use chrono::Utc;
use globset::{Glob, GlobMatcher};
use regex::Regex;
use reqwest::Url;
use sha2::{Digest, Sha256};
use tauri::{Emitter, Manager};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::{sleep_until, timeout, Instant};

use crate::database::knowledge_domain::documents::parse_artifact_from_result;
use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AnalyzeKnowledgeCodeImpactInput, BuildKnowledgeEmbeddingBatchInput,
    CaptureKnowledgeDirtyWorktreeSnapshotInput, CaptureKnowledgeGitSnapshotInput,
    CaptureKnowledgeLocalDirectorySnapshotInput, CompareKnowledgeCodeSnapshotsInput,
    CompareKnowledgeDocumentVersionsInput, CreateAuditLogInput, CreateKnowledgeCodeSnapshotInput,
    CreateKnowledgeJobInput, GenerateKnowledgeCodeDocumentsInput,
    GenerateKnowledgeCodeDocumentsResult, GenerateZentaoAiSummaryInput,
    GenerateZentaoAiSummaryResult, GenerateZentaoKnowledgeDocumentsInput,
    GenerateZentaoKnowledgeDocumentsResult, ImportKnowledgeCommitRelationsInput,
    ImportKnowledgeDocumentRelationsInput, ImportKnowledgeExperiencesInput,
    ImportKnowledgeExperiencesResult, KnowledgeAskInput, KnowledgeChunkWriteInput,
    KnowledgeCitation, KnowledgeCitationDetail, KnowledgeCodeAnalysisResult,
    KnowledgeCodeCallGraph, KnowledgeCodeCallGraphInput, KnowledgeCodeFile,
    KnowledgeCodeFileChange, KnowledgeCodeFileContent, KnowledgeCodeFileWriteInput,
    KnowledgeCodeRelationWriteInput, KnowledgeCodeSnapshot, KnowledgeCodeSnapshotComparison,
    KnowledgeCodeSource, KnowledgeCodeSymbol, KnowledgeCodeSymbolWriteInput, KnowledgeDocument,
    KnowledgeDocumentComparison, KnowledgeDocumentDetail, KnowledgeDocumentVersion,
    KnowledgeErrorDetail, KnowledgeFtsCapability, KnowledgeGitRef, KnowledgeJob,
    KnowledgeJobProgress, KnowledgeListInput, KnowledgePage, KnowledgeParseAndChunkInput,
    KnowledgeParseAndChunkResult, KnowledgeParseInput, KnowledgeProject, KnowledgeRelation,
    KnowledgeRelease, KnowledgeSearchInput, KnowledgeSource, KnowledgeSourceScopeEntry,
    KnowledgeSourceScopePreview, KnowledgeSourceSyncResult, ListKnowledgeRelationsInput,
    SearchKnowledgeCodeSymbolsInput, SecureCredentialHttpRequestResult,
    SetSecureCredentialEnabledInput, StartKnowledgeSourceSyncInput, SyncKnowledgeGitSourceInput,
    SyncKnowledgeLocalSourceInput, SyncZentaoMappingInput, UpsertKnowledgeCodeSourceInput,
    UpsertKnowledgeDocumentInput, UpsertKnowledgeProjectInput, UpsertKnowledgeRelationInput,
    UpsertKnowledgeReleaseInput, UpsertKnowledgeSourceInput, UpsertZentaoConnectionInput,
    UpsertZentaoEntityInput, UpsertZentaoEntityRelationInput, UpsertZentaoProjectMappingInput,
    ZentaoCapabilityProbeResult, ZentaoConnection, ZentaoEntity, ZentaoEntityRelation,
    ZentaoProjectMapping, ZentaoRemoteScopeItem, ZentaoSyncCursorUpdateInput, ZentaoSyncResult,
};
use crate::services::audit::AuditService;
use crate::services::knowledge_code_analyzer::{
    AnalyzedCodeSymbol, CodeAnalysisResult, P0LanguageAnalyzer,
};
use crate::services::knowledge_domain::documents::KnowledgeDocumentService;
use crate::services::knowledge_domain::jobs::{
    KnowledgeDocumentJobService, KnowledgeUploadImportJobService,
};
use crate::services::knowledge_embedding::KnowledgeEmbeddingService;
use crate::services::knowledge_parser::{is_markdown_path, KnowledgeParserService};
use crate::services::knowledge_policy::{detect_sensitive_content, KnowledgePolicyService};
use crate::services::knowledge_retrieval::KnowledgeRetrievalService;
use crate::services::knowledge_rollout::KnowledgeRolloutService;
use crate::services::secure_credential::SecureCredentialService;
use crate::state::AppState;

pub struct KnowledgeService;

const SOURCE_PREVIEW_MAX_VISITED: usize = 5_000;
const SOURCE_PREVIEW_MAX_ENTRIES: usize = 500;
const GIT_SYNC_MAX_FILES: usize = 5_000;
const GIT_SYNC_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const KNOWLEDGE_JOB_HEARTBEAT_SECONDS: u64 = 5;
const KNOWLEDGE_JOB_CANCELLED: &str = "KNOWLEDGE_JOB_CANCELLED";
/// 每个禅道连接独立预约下一个只读请求时间，避免探测和同步绕过连接级限流。
static ZENTAO_REQUEST_SCHEDULES: OnceLock<Mutex<HashMap<i64, Instant>>> = OnceLock::new();
const DEFAULT_EXCLUDE_GLOBS: &[&str] = &[
    ".git/**",
    "**/.git/**",
    ".idea/**",
    "**/.idea/**",
    ".vscode/**",
    "**/.vscode/**",
    "node_modules/**",
    "**/node_modules/**",
    "target/**",
    "**/target/**",
    "dist/**",
    "**/dist/**",
    "build/**",
    "**/build/**",
    "coverage/**",
    "**/coverage/**",
    "vendor/**",
    "**/vendor/**",
    "out/**",
    "**/out/**",
];

struct ZentaoProbeCandidate {
    name: &'static str,
    path: &'static str,
}

/// 每个 Profile 的实体端点都必须在连接探测时单独确认。这里的路径只是候选路径，
/// 不是对任意禅道版本都成立的协议假设；未探测成功的实体绝不会出现在同步能力中。
fn zentao_entity_endpoint_candidates(profile: &str) -> &'static [(&'static str, &'static str)] {
    match profile {
        "zentao-rest-v1" => &[
            ("stories", "/api.php/v1/stories"),
            ("story_changes", "/api.php/v1/storychanges"),
            ("tasks", "/api.php/v1/tasks"),
            ("worklogs", "/api.php/v1/worklogs"),
            ("bugs", "/api.php/v1/bugs"),
            ("test_cases", "/api.php/v1/testcases"),
            ("test_tasks", "/api.php/v1/testtasks"),
            ("test_runs", "/api.php/v1/testruns"),
            ("builds", "/api.php/v1/builds"),
            ("releases", "/api.php/v1/releases"),
        ],
        "zentao-legacy-module" => &[
            ("stories", "/api.php?m=story&f=browse"),
            ("tasks", "/api.php?m=task&f=browse"),
            ("bugs", "/api.php?m=bug&f=browse"),
        ],
        _ => &[],
    }
}

fn zentao_sync_endpoint(profile: &str, entity_type: &str) -> Result<&'static str, AppError> {
    zentao_entity_endpoint_candidates(profile)
        .iter()
        .find_map(|(candidate_type, path)| (*candidate_type == entity_type).then_some(*path))
        .ok_or_else(|| {
            AppError::InvalidInput(format!(
                "已探测端点配置 '{profile}' 不支持增量同步实体 '{entity_type}'"
            ))
        })
}

/// 禅道 REST 与传统模块路由在不同版本/部署中并不等价。探测只使用受控的、只读 GET
/// 路径，成功后记录实际命中的 profile，后续同步不得任意拼接未探测过的端点。
fn zentao_probe_candidates(preferred_profile: &str) -> Vec<ZentaoProbeCandidate> {
    let candidates = [
        ZentaoProbeCandidate {
            name: "zentao-rest-v1",
            path: "/api.php/v1/products?limit=1",
        },
        ZentaoProbeCandidate {
            name: "zentao-legacy-module",
            path: "/api.php?m=project&f=all",
        },
    ];
    let preferred_profile = preferred_profile.trim();
    let mut ordered = candidates.into_iter().collect::<Vec<_>>();
    if !preferred_profile.is_empty() {
        ordered.sort_by_key(|candidate| (candidate.name != preferred_profile) as u8);
    }
    ordered
}

fn zentao_discovery_paths(profile: &str) -> Result<Vec<(&'static str, &'static str)>, AppError> {
    match profile {
        "zentao-rest-v1" => Ok(vec![
            ("product", "/api.php/v1/products?limit=200"),
            ("project", "/api.php/v1/projects?limit=200"),
            ("execution", "/api.php/v1/executions?limit=200"),
        ]),
        "zentao-legacy-module" => Ok(vec![("project", "/api.php?m=project&f=all")]),
        _ => Err(AppError::InvalidInput(
            "当前禅道连接未选择受支持的已探测端点配置".to_string(),
        )),
    }
}

fn zentao_fact_document_templates(
    project: &KnowledgeProject,
    mapping: &ZentaoProjectMapping,
    entities: &[ZentaoEntity],
) -> Vec<(&'static str, String, String)> {
    let count = |entity_type: &str| {
        entities
            .iter()
            .filter(|entity| entity.entity_type == entity_type)
            .count()
    };
    let entity_rows = |predicate: &dyn Fn(&ZentaoEntity) -> bool| {
        let rows = entities
            .iter()
            .filter(|entity| predicate(entity))
            .map(|entity| {
                format!(
                    "| {} | {} | {} | {} | {} |",
                    entity.external_key,
                    markdown_cell(&entity.title),
                    entity.normalized_status,
                    entity.assignee_external_id,
                    entity.source_updated_at.as_deref().unwrap_or("未提供"),
                )
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            "| 无 | - | - | - | - |".to_string()
        } else {
            rows.join("\n")
        }
    };
    let table_header = "| 实体 | 标题 | 状态 | 负责人 | 来源更新时间 |\n|---|---|---|---|---|";
    let overview = format!(
        "# {} 项目概览\n\n- 映射：{}\n- 规范化实体总数：{}\n- 需求：{}；任务：{}；缺陷：{}；测试：{}\n\n## 实体状态\n\n{}\n{}",
        project.name,
        mapping.id,
        entities.len(),
        count("stories"),
        count("tasks"),
        count("bugs"),
        count("tests"),
        table_header,
        entity_rows(&|_| true),
    );
    let requirements = format!(
        "# {} 需求基线\n\n{}\n{}",
        project.name,
        table_header,
        entity_rows(&|entity| entity.entity_type == "stories"),
    );
    let traceability = format!(
        "# {} 追踪矩阵\n\n> 本矩阵仅列出已同步的禅道事实；Commit、代码和测试关联缺失时明确保留为空，不能推断。\n\n| 需求/任务实体 | 父实体 | 状态 | 证据键 | 远端地址 |\n|---|---|---|---|---|\n{}",
        project.name,
        entities
            .iter()
            .map(|entity| format!(
                "| {} | {} | {} | {} | {} |",
                entity.external_key,
                entity.parent_external_key,
                entity.normalized_status,
                entity.external_key,
                entity.remote_url,
            ))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let tasks = format!(
        "# {} 任务执行总结\n\n{}\n{}",
        project.name,
        table_header,
        entity_rows(&|entity| entity.entity_type == "tasks" || entity.entity_type == "worklogs"),
    );
    let tests = format!(
        "# {} 测试质量报告\n\n{}\n{}",
        project.name,
        table_header,
        entity_rows(&|entity| entity.entity_type == "tests" || entity.entity_type == "test_cases"),
    );
    let changes = format!(
        "# {} 变更记录\n\n{}\n{}",
        project.name,
        table_header,
        entity_rows(&|entity| entity.source_updated_at.is_some()),
    );
    let risks = format!(
        "# {} 开放风险与遗留\n\n{}\n{}",
        project.name,
        table_header,
        entity_rows(&|entity| !matches!(
            entity.normalized_status.as_str(),
            "closed" | "done" | "resolved"
        )),
    );
    vec![
        ("project-overview", "项目概览".to_string(), overview),
        ("release-requirements", "需求基线".to_string(), requirements),
        ("traceability", "追踪矩阵".to_string(), traceability),
        ("task-execution", "任务执行总结".to_string(), tasks),
        ("test-quality", "测试质量报告".to_string(), tests),
        ("change-log", "变更记录".to_string(), changes),
        ("open-risks", "开放风险与遗留".to_string(), risks),
    ]
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn ensure_analyzed_code_snapshot(
    db: &Database,
    snapshot_id: i64,
) -> Result<KnowledgeCodeSnapshot, AppError> {
    validate_positive_id(snapshot_id, "源码快照 ID")?;
    let snapshot = db
        .get_knowledge_code_snapshot_by_id(snapshot_id)?
        .ok_or_else(|| AppError::NotFound("源码快照不存在".to_string()))?;
    if snapshot.status != "analyzed" {
        return Err(AppError::InvalidInput(
            "源码快照尚未完成分析，不能用于查询或影响分析".to_string(),
        ));
    }
    Ok(snapshot)
}

fn bounded_code_graph(
    symbols: &[KnowledgeCodeSymbol],
    all_edges: &[crate::models::KnowledgeCodeRelation],
    roots: &[String],
    max_depth: i64,
    reverse: bool,
    include_unconfirmed: bool,
) -> (
    Vec<KnowledgeCodeSymbol>,
    Vec<crate::models::KnowledgeCodeRelation>,
    bool,
) {
    const MAX_GRAPH_NODES: usize = 500;
    const MAX_GRAPH_EDGES: usize = 1_000;
    let symbols_by_key = symbols
        .iter()
        .map(|symbol| (symbol.symbol_key.as_str(), symbol))
        .collect::<std::collections::HashMap<_, _>>();
    let mut visited = roots
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut queue =
        std::collections::VecDeque::from_iter(roots.iter().cloned().map(|root| (root, 0_i64)));
    let mut selected_edges = Vec::new();
    let mut truncated = false;
    while let Some((symbol_key, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for edge in all_edges.iter().filter(|edge| {
            (include_unconfirmed || edge.confirmed)
                && if reverse {
                    edge.to_symbol_key == symbol_key
                } else {
                    edge.from_symbol_key == symbol_key
                }
        }) {
            if edge.to_symbol_key.is_empty() || edge.from_symbol_key.is_empty() {
                continue;
            }
            if selected_edges.len() >= MAX_GRAPH_EDGES {
                truncated = true;
                break;
            }
            let adjacent = if reverse {
                &edge.from_symbol_key
            } else {
                &edge.to_symbol_key
            };
            if !symbols_by_key.contains_key(adjacent.as_str()) {
                continue;
            }
            selected_edges.push(edge.clone());
            if visited.insert(adjacent.clone()) {
                if visited.len() > MAX_GRAPH_NODES {
                    truncated = true;
                    break;
                }
                queue.push_back((adjacent.clone(), depth + 1));
            }
        }
        if truncated {
            break;
        }
    }
    let nodes = visited
        .into_iter()
        .filter_map(|key| symbols_by_key.get(key.as_str()).cloned())
        .cloned()
        .collect();
    (nodes, selected_edges, truncated)
}

fn compare_code_file_sets(
    from_files: &[KnowledgeCodeFile],
    to_files: &[KnowledgeCodeFile],
) -> Vec<KnowledgeCodeFileChange> {
    let from_active = from_files
        .iter()
        .filter(|file| file.status == "active")
        .map(|file| (file.relative_path.as_str(), file))
        .collect::<std::collections::BTreeMap<_, _>>();
    let to_active = to_files
        .iter()
        .filter(|file| file.status == "active")
        .map(|file| (file.relative_path.as_str(), file))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut changes = Vec::new();
    let mut unmatched_from = Vec::new();
    let mut unmatched_to = Vec::new();
    for (path, from_file) in &from_active {
        match to_active.get(path) {
            Some(to_file) if to_file.content_hash != from_file.content_hash => {
                changes.push(KnowledgeCodeFileChange {
                    change_type: "modified".to_string(),
                    from_path: (*path).to_string(),
                    to_path: (*path).to_string(),
                    content_hash: to_file.content_hash.clone(),
                });
            }
            Some(_) => {}
            None => unmatched_from.push((*from_file).clone()),
        }
    }
    for (path, to_file) in &to_active {
        if !from_active.contains_key(path) {
            unmatched_to.push((*to_file).clone());
        }
    }
    let mut consumed_to = std::collections::BTreeSet::new();
    for from_file in unmatched_from {
        if let Some(to_file) = unmatched_to.iter().find(|to_file| {
            to_file.content_hash == from_file.content_hash
                && !consumed_to.contains(to_file.relative_path.as_str())
        }) {
            consumed_to.insert(to_file.relative_path.clone());
            changes.push(KnowledgeCodeFileChange {
                change_type: "renamed".to_string(),
                from_path: from_file.relative_path.clone(),
                to_path: to_file.relative_path.clone(),
                content_hash: from_file.content_hash.clone(),
            });
        } else {
            changes.push(KnowledgeCodeFileChange {
                change_type: "deleted".to_string(),
                from_path: from_file.relative_path.clone(),
                to_path: String::new(),
                content_hash: from_file.content_hash.clone(),
            });
        }
    }
    for to_file in unmatched_to {
        if !consumed_to.contains(to_file.relative_path.as_str()) {
            changes.push(KnowledgeCodeFileChange {
                change_type: "added".to_string(),
                from_path: String::new(),
                to_path: to_file.relative_path.clone(),
                content_hash: to_file.content_hash.clone(),
            });
        }
    }
    changes.sort_by(|left, right| {
        (
            left.change_type.as_str(),
            left.from_path.as_str(),
            left.to_path.as_str(),
        )
            .cmp(&(
                right.change_type.as_str(),
                right.from_path.as_str(),
                right.to_path.as_str(),
            ))
    });
    changes
}

/// 以相邻快照的路径和内容哈希生成可审计的增量变更。内容哈希相同而路径变化时明确
/// 记录 rename；其余新增、删除、同路径修改均独立保存，供关系失效和后续影响分析使用。
fn classify_code_snapshot_changes(
    previous_files: &[KnowledgeCodeFile],
    current_file_hashes: &std::collections::HashMap<String, String>,
) -> Vec<(String, String, String, String, serde_json::Value)> {
    let previous_by_path = previous_files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.content_hash.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let previous_paths = previous_by_path
        .keys()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let current_paths = current_file_hashes
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let mut changes = Vec::new();

    for path in previous_paths.intersection(&current_paths) {
        let previous_hash = previous_by_path[path];
        let current_hash = &current_file_hashes[*path];
        if previous_hash != current_hash {
            changes.push((
                "modified".to_string(),
                (*path).to_string(),
                (*path).to_string(),
                current_hash.clone(),
                serde_json::json!({ "kind": "content_hash", "previousHash": previous_hash }),
            ));
        }
    }

    let removed_paths = previous_paths
        .difference(&current_paths)
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let added_paths = current_paths
        .difference(&previous_paths)
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut matched_added = std::collections::BTreeSet::new();
    for old_path in &removed_paths {
        let old_hash = previous_by_path[old_path];
        if let Some(new_path) = added_paths
            .iter()
            .find(|new_path| current_file_hashes[**new_path] == old_hash)
        {
            matched_added.insert(*new_path);
            changes.push((
                "renamed".to_string(),
                (*old_path).to_string(),
                (*new_path).to_string(),
                old_hash.to_string(),
                serde_json::json!({ "kind": "content_hash_rename" }),
            ));
        } else {
            changes.push((
                "deleted".to_string(),
                (*old_path).to_string(),
                String::new(),
                old_hash.to_string(),
                serde_json::json!({ "kind": "content_hash" }),
            ));
        }
    }
    for new_path in added_paths.difference(&matched_added) {
        changes.push((
            "added".to_string(),
            String::new(),
            (*new_path).to_string(),
            current_file_hashes[*new_path].clone(),
            serde_json::json!({ "kind": "content_hash" }),
        ));
    }
    changes.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    changes
}

/// 代码报告只使用当前快照内的已持久化分析结果。每个模板的输入排序稳定，故相同快照
/// 反复生成不会产生新版本；报告中的关系候选均明确标记，避免误导为运行时调用事实。
fn code_snapshot_report_templates(
    snapshot: &KnowledgeCodeSnapshot,
    files: &[KnowledgeCodeFile],
    symbols: &[KnowledgeCodeSymbol],
    relations: &[crate::models::KnowledgeCodeRelation],
) -> Vec<(&'static str, String, String)> {
    let active_files = files
        .iter()
        .filter(|file| file.status == "active" && file.sensitivity != "restricted")
        .collect::<Vec<_>>();
    let confirmed_relations = relations
        .iter()
        .filter(|relation| relation.confirmed)
        .collect::<Vec<_>>();
    let candidate_relations = relations.len().saturating_sub(confirmed_relations.len());
    let file_rows = active_files
        .iter()
        .map(|file| {
            format!(
                "| {} | {} | {} | {} |",
                markdown_cell(&file.relative_path),
                file.language,
                file.analysis_level,
                if file.is_test { "是" } else { "否" },
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let symbol_rows = symbols
        .iter()
        .take(500)
        .map(|symbol| {
            format!(
                "| {} | {} | {} | L{}-L{} |",
                symbol.symbol_kind,
                markdown_cell(&symbol.qualified_name),
                markdown_cell(&symbol.signature),
                symbol.start_line,
                symbol.end_line,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let relation_rows = relations
        .iter()
        .take(500)
        .map(|relation| {
            format!(
                "| {} | {} | {} | {} | {} |",
                markdown_cell(&relation.from_symbol_key),
                relation.relation_type,
                markdown_cell(if relation.to_symbol_key.is_empty() {
                    &relation.to_external_key
                } else {
                    &relation.to_symbol_key
                }),
                relation.resolver,
                if relation.confirmed {
                    "已确认"
                } else {
                    "候选"
                },
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let report_scope = format!(
        "- 快照：`{}`\n- 类型：`{}`\n- Commit：`{}`\n- 基线 Commit：`{}`\n- 分支：`{}`\n- 工作树脏状态：{}\n- 活动文件：{}；符号：{}；关系：{}（已确认 {}，候选 {}）",
        snapshot.snapshot_key,
        snapshot.snapshot_type,
        if snapshot.commit_sha.is_empty() { "无" } else { &snapshot.commit_sha },
        if snapshot.base_commit_sha.is_empty() { "无" } else { &snapshot.base_commit_sha },
        if snapshot.branch_name.is_empty() { "无" } else { &snapshot.branch_name },
        if snapshot.worktree_dirty { "是；仅本地观察，不能作为发布事实" } else { "否" },
        active_files.len(),
        symbols.len(),
        relations.len(),
        confirmed_relations.len(),
        candidate_relations,
    );
    let module_rows = active_files
        .iter()
        .map(|file| {
            file.relative_path
                .split('/')
                .next()
                .unwrap_or(".")
                .to_string()
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|module| {
            let count = active_files
                .iter()
                .filter(|file| {
                    file.relative_path == module
                        || file.relative_path.starts_with(&(module.clone() + "/"))
                })
                .count();
            format!("| {} | {} |", markdown_cell(&module), count)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let api_symbols = symbols
        .iter()
        .filter(|symbol| {
            matches!(
                symbol.symbol_kind.as_str(),
                "tauri_command" | "route" | "controller" | "api"
            )
        })
        .collect::<Vec<_>>();
    let database_symbols = symbols
        .iter()
        .filter(|symbol| matches!(symbol.symbol_kind.as_str(), "table" | "column" | "mapper"))
        .collect::<Vec<_>>();
    let config_symbols = symbols
        .iter()
        .filter(|symbol| symbol.symbol_kind == "config_key")
        .collect::<Vec<_>>();
    let test_files = active_files
        .iter()
        .filter(|file| file.is_test)
        .collect::<Vec<_>>();
    let api_rows = api_symbols
        .iter()
        .map(|symbol| {
            format!(
                "| {} | {} |",
                markdown_cell(&symbol.name),
                markdown_cell(&symbol.signature)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let database_rows = database_symbols
        .iter()
        .map(|symbol| {
            format!(
                "| {} | {} |",
                symbol.symbol_kind,
                markdown_cell(&symbol.qualified_name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let config_rows = config_symbols
        .iter()
        .map(|symbol| {
            format!(
                "| {} | {} |",
                markdown_cell(&symbol.name),
                markdown_cell(&symbol.signature)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let test_rows = test_files
        .iter()
        .map(|file| {
            format!(
                "| {} | {} |",
                markdown_cell(&file.relative_path),
                file.analysis_level
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let missing = "未检测到";
    vec![
        ("repository-overview", "仓库概览".to_string(), format!("# 仓库概览\n\n{}\n\n## 文件清单\n\n| 路径 | 语言 | 分析级别 | 测试 |\n|---|---|---|---|\n{}", report_scope, if file_rows.is_empty() { missing } else { &file_rows })),
        ("module-map", "模块说明".to_string(), format!("# 模块说明\n\n{}\n\n| 顶层模块 | 活动文件数 |\n|---|---|\n{}\n\n## 符号\n\n| 类型 | 限定名 | 签名 | 位置 |\n|---|---|---|---|\n{}", report_scope, if module_rows.is_empty() { missing } else { &module_rows }, if symbol_rows.is_empty() { missing } else { &symbol_rows })),
        ("api-ipc", "API 与 IPC".to_string(), format!("# API 与 IPC\n\n{}\n\n| 名称 | 签名 |\n|---|---|\n{}\n\n> 路由/IPC 关系仅在分析器提取到明确证据后列出；候选关系不代表已验证运行时调用。", report_scope, if api_rows.is_empty() { missing } else { &api_rows })),
        ("database", "数据库说明".to_string(), format!("# 数据库说明\n\n{}\n\n| 类型 | 标识 |\n|---|---|\n{}", report_scope, if database_rows.is_empty() { missing } else { &database_rows })),
        ("call-chain", "调用链".to_string(), format!("# 调用链\n\n{}\n\n| 调用方 | 关系 | 目标 | 解析器 | 状态 |\n|---|---|---|---|---|\n{}", report_scope, if relation_rows.is_empty() { missing } else { &relation_rows })),
        ("config", "配置说明".to_string(), format!("# 配置说明\n\n{}\n\n| 配置键 | 签名/位置 |\n|---|---|\n{}", report_scope, if config_rows.is_empty() { missing } else { &config_rows })),
        ("test-map", "测试映射".to_string(), format!("# 测试映射\n\n{}\n\n| 测试文件 | 分析级别 |\n|---|---|\n{}\n\n> 未发现测试文件不等于没有测试证据。", report_scope, if test_rows.is_empty() { missing } else { &test_rows })),
        ("commit-change", "Commit 变更".to_string(), format!("# Commit 变更\n\n{}\n\n> 本报告描述当前快照可见内容；未提供父快照比较时，不推断具体新增、修改、删除或重命名文件。", report_scope)),
        ("release-implementation", "版本实现".to_string(), format!("# 版本实现\n\n{}\n\n> 需求、禅道任务、测试与 Commit 的关联需要显式关系或人工确认；缺失证据不得补全。", report_scope)),
        ("impact-analysis", "影响分析".to_string(), format!("# 影响分析\n\n{}\n\n> 影响范围仅来自已确认关系；候选关系需人工确认后才可作为影响结论。已确认关系数：{}。", report_scope, confirmed_relations.len())),
    ]
}

fn persist_code_snapshot_reports(
    db: &Database,
    snapshot: &KnowledgeCodeSnapshot,
    source: &KnowledgeCodeSource,
    files: &[KnowledgeCodeFile],
    symbols: &[KnowledgeCodeSymbol],
    relations: &[crate::models::KnowledgeCodeRelation],
) -> Result<Vec<i64>, AppError> {
    let mut version_ids = Vec::new();
    for (kind, title, content) in
        code_snapshot_report_templates(snapshot, files, symbols, relations)
    {
        let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: format!("code-report:{}:{kind}", source.source.source_key),
            project_id: snapshot.project_id,
            source_id: Some(source.source.id),
            doc_type: "code_report".to_string(),
            title: title.clone(),
            logical_path: format!("code-reports/{kind}.md"),
            sensitivity: "internal".to_string(),
            tags: vec!["code".to_string(), "report".to_string(), kind.to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        let content_hash = sha256_hex(content.as_bytes());
        if db.knowledge_document_version_exists(
            document.id,
            &snapshot.snapshot_key,
            &content_hash,
            &document.logical_path,
        )? {
            continue;
        }
        let version = db.create_knowledge_document_version(
            &crate::models::CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: snapshot.release_id,
                version_label: snapshot.snapshot_key.clone(),
                git_branch: snapshot.branch_name.clone(),
                commit_sha: snapshot.commit_sha.clone(),
                source_path: document.logical_path.clone(),
                mime_type: "text/markdown".to_string(),
                content: content.clone(),
                content_hash: content_hash.clone(),
                parsed_meta: serde_json::json!({
                    "generatorId": "knowledge-code-reports-v1",
                    "snapshotId": snapshot.id,
                    "template": kind,
                }),
                token_estimate: 0,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: title,
                content: content.clone(),
                content_hash: content_hash.clone(),
                location: serde_json::json!({
                    "snapshotId": snapshot.id,
                    "language": "generated_report",
                    "path": document.logical_path,
                    "startLine": 1,
                    "endLine": i64::try_from(content.lines().count()).unwrap_or(i64::MAX).max(1),
                    "sensitivity": "internal",
                }),
                token_estimate: i64::try_from(content.chars().count().div_ceil(4))
                    .unwrap_or(i64::MAX),
            }],
        )?;
        version_ids.push(version.id);
    }
    Ok(version_ids)
}

async fn request_zentao_readonly_json(
    db: &Database,
    connection: &ZentaoConnection,
    path: &str,
) -> Result<serde_json::Value, AppError> {
    validate_zentao_insecure_http_policy(
        db,
        &connection.base_url,
        connection.allow_insecure_http,
        connection.tls_verify,
    )?;
    wait_for_zentao_request_turn(connection.id, connection.rate_limit_per_second).await;
    let result = request_zentao_readonly(db, connection, path, "zentao_readonly_request").await?;
    if !(200..300).contains(&result.status_code) {
        return Err(AppError::Custom(format!(
            "禅道只读请求失败: HTTP {}",
            result.status_code
        )));
    }
    Ok(result.body)
}

/// 认证模式仅在这个入口分派，避免同步、范围发现和能力探测各自走出不同的安全边界。
async fn request_zentao_readonly(
    db: &Database,
    connection: &ZentaoConnection,
    path: &str,
    audit_action: &str,
) -> Result<SecureCredentialHttpRequestResult, AppError> {
    match connection.auth_mode.as_str() {
        "bearer" | "auto" => {
            SecureCredentialService::http_readonly_request_for_same_origin_service(
                db,
                &connection.credential_key,
                &connection.base_url,
                path,
                connection.request_timeout_seconds as u64,
                audit_action,
            )
            .await
        }
        _ => Err(AppError::InvalidInput(
            "禅道认证模式仅支持 API Token 或自动探测（按 API Token）".into(),
        )),
    }
}

/// 在真正发起网络请求前再次读取策略，避免连接保存后清空域名白名单而让明文 HTTP
/// 例外继续生效。企业内网地址可能通过非 RFC1918 的 NAT 或 DNS 暴露，因此采用
/// 管理员精确配置的 HTTP 域名/IP 白名单作为可审计信任边界，不猜测网络拓扑。
fn validate_zentao_insecure_http_policy(
    db: &Database,
    base_url: &str,
    allow_insecure_http: bool,
    tls_verify: bool,
) -> Result<(), AppError> {
    if !base_url.starts_with("http://") {
        return Ok(());
    }
    if !allow_insecure_http || tls_verify {
        return Err(AppError::InvalidInput(
            "HTTP 禅道连接未完成显式风险授权".to_string(),
        ));
    }
    let host = Url::parse(base_url)
        .map_err(|_| AppError::InvalidInput("禅道地址必须是有效 URL".to_string()))?
        .host_str()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| AppError::InvalidInput("禅道地址必须包含主机".to_string()))?;
    let allowed_domains = db
        .get_secure_credential_policy_settings()?
        .http_allowed_domains;
    if allowed_domains.is_empty() || !zentao_http_host_is_allowlisted(&host, &allowed_domains) {
        return Err(AppError::InvalidInput(
            "HTTP 禅道地址必须精确加入安全凭据策略的 HTTP 域名白名单".to_string(),
        ));
    }
    Ok(())
}

fn zentao_http_host_is_allowlisted(host: &str, allowed_domains: &[String]) -> bool {
    let normalized_host = host.trim().to_ascii_lowercase();
    allowed_domains.iter().any(|domain| {
        let normalized_domain = domain.trim().trim_start_matches("*.").to_ascii_lowercase();
        !normalized_domain.is_empty()
            && (normalized_host == normalized_domain
                || normalized_host.ends_with(&format!(".{normalized_domain}")))
    })
}

async fn wait_for_zentao_request_turn(connection_id: i64, rate_limit_per_second: f64) {
    let interval = Duration::from_secs_f64(1.0 / rate_limit_per_second.clamp(0.1, 30.0));
    let now = Instant::now();
    let schedules = ZENTAO_REQUEST_SCHEDULES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut schedules = schedules.lock().await;
    let next_allowed = schedules.entry(connection_id).or_insert(now);
    let scheduled_at = (*next_allowed).max(now);
    *next_allowed = scheduled_at + interval;
    drop(schedules);
    if scheduled_at > now {
        sleep_until(scheduled_at).await;
    }
}

fn parse_zentao_scope_items(
    entity_type: &str,
    value: &serde_json::Value,
) -> Vec<ZentaoRemoteScopeItem> {
    let records = value
        .get("data")
        .or_else(|| value.get("items"))
        .or_else(|| value.get("projects"))
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.as_array())
        .cloned()
        .unwrap_or_default();
    records
        .iter()
        .filter_map(|record| {
            let object = record.as_object()?;
            let external_id = object
                .get("id")
                .or_else(|| object.get("project"))
                .or_else(|| object.get("code"))
                .and_then(json_scalar_string)?;
            let name = object
                .get("name")
                .or_else(|| object.get("title"))
                .or_else(|| object.get("text"))
                .and_then(json_scalar_string)
                .unwrap_or_else(|| external_id.clone());
            Some(ZentaoRemoteScopeItem {
                entity_type: entity_type.to_string(),
                external_id,
                name,
                parent_external_id: object
                    .get("product")
                    .or_else(|| object.get("parent"))
                    .or_else(|| object.get("project"))
                    .and_then(json_scalar_string)
                    .unwrap_or_default(),
                status: object
                    .get("status")
                    .and_then(json_scalar_string)
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn parse_zentao_records(value: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    value
        .get("data")
        .or_else(|| value.get("items"))
        .or_else(|| value.get("stories"))
        .or_else(|| value.get("tasks"))
        .or_else(|| value.get("bugs"))
        .or_else(|| value.get("tests"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .or_else(|| value.as_array().cloned())
}

fn normalize_zentao_entity(
    mapping: &ZentaoProjectMapping,
    connection: &ZentaoConnection,
    entity_type: &str,
    record: &serde_json::Value,
) -> Result<UpsertZentaoEntityInput, AppError> {
    let object = record.as_object().ok_or_else(|| {
        AppError::InvalidInput(format!("禅道 {entity_type} 记录不是对象，已中止本实体同步"))
    })?;
    let external_id = object
        .get("id")
        .or_else(|| object.get(entity_type.trim_end_matches('s')))
        .and_then(json_scalar_string)
        .ok_or_else(|| AppError::InvalidInput(format!("禅道 {entity_type} 记录缺少 ID")))?;
    let title = object
        .get("title")
        .or_else(|| object.get("name"))
        .or_else(|| object.get("summary"))
        .and_then(json_scalar_string)
        .unwrap_or_else(|| format!("{entity_type} #{external_id}"));
    let body = object
        .get("spec")
        .or_else(|| object.get("description"))
        .or_else(|| object.get("content"))
        .or_else(|| object.get("steps"))
        .and_then(json_scalar_string)
        .map(|value| strip_zentao_html(&value))
        .unwrap_or_default();
    let original_status = object
        .get("status")
        .and_then(json_scalar_string)
        .unwrap_or_default();
    let source_updated_at = object
        .get("lastEditedDate")
        .or_else(|| object.get("updated_at"))
        .or_else(|| object.get("updatedDate"))
        .or_else(|| object.get("date"))
        .and_then(json_scalar_string);
    let source_created_at = object
        .get("createdDate")
        .or_else(|| object.get("created_at"))
        .and_then(json_scalar_string);
    let raw_snapshot = record.clone();
    let raw_json_hash = sha256_hex(raw_snapshot.to_string().as_bytes());
    let normalized_content = serde_json::json!({
        "type": entity_type, "id": external_id, "title": title, "body": body,
        "status": original_status, "updatedAt": source_updated_at
    });
    Ok(UpsertZentaoEntityInput {
        connection_id: connection.id,
        mapping_id: mapping.id,
        knowledge_project_id: mapping.knowledge_project_id,
        release_id: None,
        entity_type: entity_type.to_string(),
        external_key: format!("zentao:{}:{}:{}", connection.id, entity_type, external_id),
        external_id,
        title,
        body_markdown: body,
        original_status: original_status.clone(),
        normalized_status: original_status.to_ascii_lowercase(),
        assignee_external_id: object
            .get("assignedTo")
            .or_else(|| object.get("assignee"))
            .and_then(json_scalar_string)
            .unwrap_or_default(),
        parent_external_key: zentao_parent_external_key(entity_type, connection.id, object),
        remote_url: String::new(),
        content_hash: sha256_hex(normalized_content.to_string().as_bytes()),
        raw_json_hash,
        raw_snapshot: Some(raw_snapshot),
        source_created_at,
        source_updated_at,
    })
}

/// 禅道经常只返回父实体裸 ID。将字段语义固化进稳定键，避免 `story=1` 与同映射中
/// `bug#1`、`project#1` 等异构实体误关联；无法确定父类型时宁可不生成关系。
fn zentao_parent_external_key(
    entity_type: &str,
    connection_id: i64,
    object: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let candidates: &[(&str, &str)] = match entity_type {
        "tasks" => &[
            ("story", "stories"),
            ("execution", "executions"),
            ("project", "projects"),
        ],
        "bugs" => &[
            ("story", "stories"),
            ("task", "tasks"),
            ("project", "projects"),
        ],
        "tests" | "test_cases" | "test_runs" => &[
            ("story", "stories"),
            ("task", "tasks"),
            ("case", "test_cases"),
        ],
        "story_changes" => &[("story", "stories")],
        "worklogs" => &[("task", "tasks")],
        _ => &[],
    };
    candidates
        .iter()
        .find_map(|(field, parent_type)| {
            object
                .get(*field)
                .and_then(json_scalar_string)
                .map(|id| format!("zentao:{connection_id}:{parent_type}:{id}"))
        })
        .unwrap_or_default()
}

fn strip_zentao_html(value: &str) -> String {
    // Rust regex 不支持反向引用。分别匹配 script/style，避免在处理禅道 HTML 时因
    // `Regex::new` 失败而 panic；脚本与样式正文也必须在入库前移除。
    Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>|<style\b[^>]*>.*?</style\s*>|<[^>]+>")
        .expect("固定禅道 HTML 清理正则必须有效")
        .replace_all(value, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn json_scalar_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) if !value.trim().is_empty() => {
            Some(value.trim().to_string())
        }
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn normalize_zentao_base_url(value: &str, allow_insecure_http: bool) -> Result<String, AppError> {
    let mut url = Url::parse(value.trim())
        .map_err(|_| AppError::InvalidInput("禅道地址必须是有效 URL".to_string()))?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(AppError::InvalidInput(
            "禅道地址仅支持带主机的 HTTPS 或 HTTP URL".to_string(),
        ));
    }
    // HTTP 不具备 TLS 的机密性和完整性。仅允许用户在单个连接上明确授权，且不得
    // 把 HTTP 例外误用于 HTTPS 地址，避免安全配置的含义被静默弱化。
    if url.scheme() == "http" && !allow_insecure_http {
        return Err(AppError::InvalidInput(
            "HTTP 禅道地址必须显式开启“允许内网 HTTP”".to_string(),
        ));
    }
    if url.scheme() == "https" && allow_insecure_http {
        return Err(AppError::InvalidInput(
            "仅 HTTP 禅道地址可以开启“允许内网 HTTP”".to_string(),
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::InvalidInput(
            "禅道地址不能包含凭据、查询参数或片段".to_string(),
        ));
    }
    url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
    Ok(url.to_string())
}

/// 在 Rust 信任边界校验传输配置，不能只依赖前端自动关闭开关。HTTP 没有可验证的
/// 证书，HTTPS 则必须保持验证开启，避免把单个内网例外扩散为通用降级。
fn validate_zentao_transport(base_url: &str, tls_verify: bool) -> Result<String, AppError> {
    let transport = Url::parse(base_url)
        .map_err(|_| AppError::InvalidInput("禅道地址必须是有效 URL".to_string()))?
        .scheme()
        .to_string();
    if transport == "http" && tls_verify {
        return Err(AppError::InvalidInput(
            "HTTP 禅道地址不支持证书校验；请关闭“校验证书”".to_string(),
        ));
    }
    if transport == "https" && !tls_verify {
        return Err(AppError::InvalidInput(
            "HTTPS 禅道连接必须启用 TLS 证书校验".to_string(),
        ));
    }
    Ok(transport)
}

fn normalize_zentao_option(value: &str, fallback: &str) -> Result<String, AppError> {
    let value = value.trim();
    let normalized = if value.is_empty() { fallback } else { value }.to_ascii_lowercase();
    if normalized.len() > 80
        || !normalized.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(AppError::InvalidInput("禅道连接选项格式无效".to_string()));
    }
    Ok(normalized)
}

impl KnowledgeService {
    fn require_rollout(db: &Database, stage: &str) -> Result<(), AppError> {
        KnowledgeRolloutService::require(db, stage)
    }

    /// 禅道连接只保存安全凭据引用，连接表和返回值均不包含秘密材料。
    pub fn upsert_zentao_connection(
        db: &Database,
        mut input: UpsertZentaoConnectionInput,
    ) -> Result<ZentaoConnection, AppError> {
        Self::require_rollout(db, "zentao")?;
        input.connection_key = normalize_key(&input.connection_key, "禅道连接标识")?;
        input.name = required_text(&input.name, "禅道连接名称")?;
        input.base_url = normalize_zentao_base_url(&input.base_url, input.allow_insecure_http)?;
        let transport = validate_zentao_transport(&input.base_url, input.tls_verify)?;
        if transport == "http" {
            validate_zentao_insecure_http_policy(
                db,
                &input.base_url,
                input.allow_insecure_http,
                input.tls_verify,
            )?;
        }
        input.auth_mode = normalize_zentao_option(&input.auth_mode, "bearer")?;
        if !["bearer", "auto"].contains(&input.auth_mode.as_str()) {
            return Err(AppError::InvalidInput(
                "禅道认证模式仅支持 API Token 或自动探测（按 API Token）".into(),
            ));
        }
        input.credential_key = required_text(&input.credential_key, "安全凭据引用")?;
        let credential = db
            .get_secure_credential(&input.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭据 '{}' 不存在", input.credential_key))
            })?;
        if !credential.enabled || credential.status != "active" || !credential.has_secret {
            return Err(AppError::InvalidInput(
                "禅道连接必须引用已启用且已保存秘密材料的安全凭据".to_string(),
            ));
        }
        if !matches!(credential.provider.as_str(), "http_api" | "custom") {
            return Err(AppError::InvalidInput(
                "禅道连接目前仅支持 http_api 或 custom 类型的安全凭据引用".to_string(),
            ));
        }
        input.api_version = normalize_zentao_option(&input.api_version, "auto")?;
        input.endpoint_profile = input.endpoint_profile.trim().to_string();
        input.request_timeout_seconds = input.request_timeout_seconds.clamp(3, 120);
        input.page_size = input.page_size.clamp(1, 200);
        if !input.rate_limit_per_second.is_finite() || input.rate_limit_per_second <= 0.0 {
            return Err(AppError::InvalidInput("禅道连接限流必须为正数".to_string()));
        }
        input.rate_limit_per_second = input.rate_limit_per_second.min(30.0);
        let connection = db.upsert_zentao_connection(&input)?;
        audit_knowledge(
            db,
            "knowledge_zentao_connection_upsert",
            if transport == "http" { "L3" } else { "L2" },
            "成功",
            if transport == "http" {
                "保存已显式确认风险的内网 HTTP 禅道连接"
            } else {
                "保存 HTTPS 禅道连接"
            },
            serde_json::json!({
                "connectionId": connection.id,
                "transport": transport,
                "insecureHttpExplicitlyAllowed": connection.allow_insecure_http,
            }),
        );
        Ok(connection)
    }

    pub fn list_zentao_connections(db: &Database) -> Result<Vec<ZentaoConnection>, AppError> {
        Self::require_rollout(db, "zentao")?;
        db.list_zentao_connections()
    }

    pub fn delete_zentao_connection(db: &Database, id: i64) -> Result<(), AppError> {
        Self::require_rollout(db, "zentao")?;
        validate_positive_id(id, "禅道连接 ID")?;
        let connection = db
            .get_zentao_connection_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("禅道连接不存在: {id}")))?;
        if connection.auth_mode == "password_session" {
            let expected_key = format!("zentao-session-{}", connection.connection_key);
            if connection.credential_key != expected_key {
                return Err(AppError::InvalidInput(
                    "历史禅道会话连接的安全凭据引用异常，已拒绝删除以保护凭据".into(),
                ));
            }
            // 清理历史账号密码会话连接遗留的专用凭据；若撤销失败，连接仍保持可用而不会
            // 留下失控的已删除连接凭据。
            SecureCredentialService::set_enabled(
                db,
                SetSecureCredentialEnabledInput {
                    credential_key: expected_key,
                    enabled: false,
                },
            )?;
        }
        db.soft_delete_zentao_connection(id)
    }

    /// 仅做 GET 能力探测，不跟随跨主机重定向，也不写入远端数据。使用候选端点矩阵而非
    /// 假设所有禅道版本都具备同一 API 路径；响应正文不会持久化或记录到日志。
    pub async fn probe_zentao_connection(
        db: &Database,
        connection_id: i64,
    ) -> Result<ZentaoCapabilityProbeResult, AppError> {
        Self::require_rollout(db, "zentao")?;
        validate_positive_id(connection_id, "禅道连接 ID")?;
        let connection = db
            .get_zentao_connection_by_id(connection_id)?
            .ok_or_else(|| AppError::NotFound(format!("禅道连接不存在: {connection_id}")))?;
        if !connection.enabled {
            return Err(AppError::InvalidInput("禅道连接已禁用".to_string()));
        }
        validate_zentao_insecure_http_policy(
            db,
            &connection.base_url,
            connection.allow_insecure_http,
            connection.tls_verify,
        )?;
        let candidates = zentao_probe_candidates(&connection.endpoint_profile);
        let mut observed = Vec::new();
        for candidate in candidates {
            wait_for_zentao_request_turn(connection.id, connection.rate_limit_per_second).await;
            let response =
                request_zentao_readonly(db, &connection, candidate.path, "zentao_capability_probe")
                    .await?;
            let status = response.status_code;
            observed.push(serde_json::json!({"profile": candidate.name, "status": status}));
            if (200..300).contains(&status) {
                let mut entities = Vec::new();
                let mut entity_endpoints = serde_json::Map::new();
                for (entity_type, entity_path) in zentao_entity_endpoint_candidates(candidate.name)
                {
                    let separator = if entity_path.contains('?') { '&' } else { '?' };
                    let probe_path = format!("{entity_path}{separator}limit=1&page=1");
                    wait_for_zentao_request_turn(connection.id, connection.rate_limit_per_second)
                        .await;
                    match request_zentao_readonly(
                        db,
                        &connection,
                        &probe_path,
                        "zentao_entity_capability_probe",
                    )
                    .await
                    {
                        Ok(entity_response)
                            if (200..300).contains(&entity_response.status_code) =>
                        {
                            entities.push((*entity_type).to_string());
                            entity_endpoints.insert(
                                (*entity_type).to_string(),
                                serde_json::Value::String((*entity_path).to_string()),
                            );
                            observed.push(serde_json::json!({
                                "profile": candidate.name,
                                "entity": entity_type,
                                "status": entity_response.status_code,
                            }));
                        }
                        Ok(entity_response) => observed.push(serde_json::json!({
                            "profile": candidate.name,
                            "entity": entity_type,
                            "status": entity_response.status_code,
                        })),
                        // 一个可选实体端点失败不应覆盖连接已经成功的只读能力探测。完整的
                        // 请求错误只记录脱敏状态，后续同步会依据 entities 拒绝该实体类型。
                        Err(_) => observed.push(serde_json::json!({
                            "profile": candidate.name,
                            "entity": entity_type,
                            "status": "unavailable",
                        })),
                    }
                }
                let capabilities = serde_json::json!({
                    "probeVersion": 1,
                    "endpointProfile": candidate.name,
                    "readOnly": true,
                    "entities": entities,
                    "entityEndpoints": entity_endpoints,
                    "observed": observed,
                });
                db.update_zentao_connection_probe(
                    connection.id,
                    "detected",
                    "bearer",
                    candidate.name,
                    &capabilities,
                    "success",
                    None,
                )?;
                return Ok(ZentaoCapabilityProbeResult {
                    connection_id: connection.id,
                    api_version: "detected".to_string(),
                    auth_mode: "bearer".to_string(),
                    endpoint_profile: candidate.name.to_string(),
                    capabilities,
                    status: "success".to_string(),
                    message: "禅道只读能力探测成功".to_string(),
                });
            }
        }
        let capabilities = serde_json::json!({"probeVersion": 1, "observed": observed});
        db.update_zentao_connection_probe(
            connection.id,
            "unknown",
            "unknown",
            "",
            &capabilities,
            "failed",
            Some("未发现可访问的只读禅道 API 端点"),
        )?;
        Err(AppError::Custom(
            "禅道能力探测失败：认证、权限或 API 端点不可用".to_string(),
        ))
    }

    /// 读取产品、项目和执行的最小范围树，返回值只用于用户确认映射；不得根据名称自动
    /// 绑定本地项目或版本。所有请求仍经过已探测 profile 的固定只读端点。
    pub async fn discover_zentao_remote_scopes(
        db: &Database,
        connection_id: i64,
    ) -> Result<Vec<ZentaoRemoteScopeItem>, AppError> {
        Self::require_rollout(db, "zentao")?;
        validate_positive_id(connection_id, "禅道连接 ID")?;
        let connection = db
            .get_zentao_connection_by_id(connection_id)?
            .ok_or_else(|| AppError::NotFound(format!("禅道连接不存在: {connection_id}")))?;
        if !connection.enabled || connection.last_test_status != "success" {
            return Err(AppError::InvalidInput(
                "请先成功完成禅道只读能力探测，再发现远程范围".to_string(),
            ));
        }
        let paths = zentao_discovery_paths(&connection.endpoint_profile)?;
        let mut scopes = Vec::new();
        for (entity_type, path) in paths {
            let body = request_zentao_readonly_json(db, &connection, path).await?;
            scopes.extend(parse_zentao_scope_items(entity_type, &body));
        }
        scopes.sort_by(|left, right| {
            left.entity_type
                .cmp(&right.entity_type)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.external_id.cmp(&right.external_id))
        });
        scopes.dedup_by(|left, right| {
            left.entity_type == right.entity_type && left.external_id == right.external_id
        });
        Ok(scopes)
    }

    pub fn upsert_zentao_project_mapping(
        db: &Database,
        mut input: UpsertZentaoProjectMappingInput,
    ) -> Result<ZentaoProjectMapping, AppError> {
        Self::require_rollout(db, "zentao")?;
        validate_positive_id(input.connection_id, "禅道连接 ID")?;
        validate_positive_id(input.knowledge_project_id, "知识项目 ID")?;
        if db
            .get_zentao_connection_by_id(input.connection_id)?
            .is_none()
        {
            return Err(AppError::NotFound("禅道连接不存在".to_string()));
        }
        if db
            .list_knowledge_projects(&KnowledgeListInput {
                project_id: Some(input.knowledge_project_id),
                release_id: None,
                source_id: None,
                keyword: None,
                status: None,
                offset: Some(0),
                limit: Some(1),
            })?
            .items
            .is_empty()
        {
            return Err(AppError::NotFound("知识项目不存在".to_string()));
        }
        input.remote_project_id = required_text(&input.remote_project_id, "禅道远程项目 ID")?;
        input.remote_product_id = input.remote_product_id.trim().to_string();
        input.remote_execution_ids = normalized_unique_values(input.remote_execution_ids);
        if !input.release_mapping.is_object() || !input.sync_scope.is_object() {
            return Err(AppError::InvalidInput(
                "禅道版本映射与同步范围必须为对象".to_string(),
            ));
        }
        let mapping = db.upsert_zentao_project_mapping(&input)?;
        audit_knowledge(
            db,
            "knowledge_zentao_mapping_upsert",
            "L2",
            "成功",
            "保存禅道知识映射",
            serde_json::json!({"mappingId": mapping.id, "connectionId": mapping.connection_id, "projectId": mapping.knowledge_project_id}),
        );
        Ok(mapping)
    }

    pub fn list_zentao_project_mappings(
        db: &Database,
        connection_id: Option<i64>,
    ) -> Result<Vec<ZentaoProjectMapping>, AppError> {
        Self::require_rollout(db, "zentao")?;
        if let Some(connection_id) = connection_id {
            validate_positive_id(connection_id, "禅道连接 ID")?;
        }
        db.list_zentao_project_mappings(connection_id)
    }

    /// 同步只读取已经通过能力探测的固定端点。每页先写入幂等实体和检查点；全部分页
    /// 成功后才确认缺失并推进成功游标，因此网络中断不会造成远端数据被误删或漏读。
    pub async fn sync_zentao_mapping(
        db: &Database,
        input: SyncZentaoMappingInput,
    ) -> Result<Vec<ZentaoSyncResult>, AppError> {
        Self::require_rollout(db, "zentao")?;
        validate_positive_id(input.mapping_id, "禅道项目映射 ID")?;
        let mapping = db
            .get_zentao_project_mapping_by_id(input.mapping_id)?
            .ok_or_else(|| AppError::NotFound("禅道项目映射不存在".to_string()))?;
        if !mapping.enabled {
            return Err(AppError::InvalidInput("禅道项目映射已禁用".to_string()));
        }
        let connection = db
            .get_zentao_connection_by_id(mapping.connection_id)?
            .ok_or_else(|| AppError::NotFound("禅道连接不存在".to_string()))?;
        if !connection.enabled || connection.last_test_status != "success" {
            return Err(AppError::InvalidInput(
                "请先完成禅道只读能力探测后再同步".to_string(),
            ));
        }
        let supported = connection
            .capabilities
            .get("entities")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let requested = if input.entity_types.is_empty() {
            supported.clone()
        } else {
            normalized_unique_values(input.entity_types)
        };
        let mut results = Vec::new();
        for entity_type in &requested {
            if !supported.iter().any(|item| item == entity_type) {
                return Err(AppError::InvalidInput(format!(
                    "当前禅道端点配置未声明支持实体类型: {entity_type}"
                )));
            }
            results.push(
                Self::sync_zentao_entity_type(db, &connection, &mapping, &entity_type).await?,
            );
        }
        let relation_count = Self::rebuild_zentao_entity_relations(db, mapping.id)?.len();
        audit_knowledge(
            db,
            "knowledge_zentao_sync",
            "readonly",
            "成功",
            "完成禅道知识同步",
            serde_json::json!({"mappingId": mapping.id, "connectionId": connection.id, "entityTypes": requested, "resultCount": results.len(), "relationCount": relation_count}),
        );
        Ok(results)
    }

    /// 从已同步实体中读取禅道明确返回的父级 ID，建立可验证关系；不从标题、正文或模型
    /// 输出推断关系。Commit 标识关系由 `import_commit_message_relations` 单独处理。
    pub fn rebuild_zentao_entity_relations(
        db: &Database,
        mapping_id: i64,
    ) -> Result<Vec<ZentaoEntityRelation>, AppError> {
        validate_positive_id(mapping_id, "禅道项目映射 ID")?;
        let entities = db.list_zentao_entities_for_mapping(mapping_id)?;
        let mut relations = Vec::new();
        for child in &entities {
            let parent_reference = child.parent_external_key.trim();
            if parent_reference.is_empty() {
                continue;
            }
            let Some(parent) = entities
                .iter()
                .find(|candidate| candidate.external_key == parent_reference)
            else {
                continue;
            };
            let relation_type = match child.entity_type.as_str() {
                "tasks" => "decomposed_to",
                "bugs" => "has_bug",
                "tests" | "test_cases" | "test_runs" => "verified_by",
                _ => "related_to",
            };
            relations.push(
                db.upsert_zentao_entity_relation(&UpsertZentaoEntityRelationInput {
                    from_external_key: parent.external_key.clone(),
                    relation_type: relation_type.to_string(),
                    to_external_key: child.external_key.clone(),
                    evidence: serde_json::json!({
                        "kind": "zentao_parent_field",
                        "mappingId": mapping_id,
                        "parentFieldValue": parent_reference,
                        "parentEntityType": parent.entity_type,
                        "childEntityType": child.entity_type,
                        "sourceUpdatedAt": child.source_updated_at,
                    }),
                    source: "zentao_parent_field".to_string(),
                    confidence: 1.0,
                    confirmed: true,
                })?,
            );
        }
        Ok(relations)
    }

    /// 依据已持久化的规范化禅道事实生成固定模板 Markdown；该流程不请求远端、不调用
    /// 模型，生成物与普通文档走同一版本、分块、FTS 与向量管道。
    pub fn generate_zentao_fact_documents(
        db: &Database,
        input: GenerateZentaoKnowledgeDocumentsInput,
    ) -> Result<GenerateZentaoKnowledgeDocumentsResult, AppError> {
        Self::require_rollout(db, "zentao")?;
        validate_positive_id(input.mapping_id, "禅道项目映射 ID")?;
        let mapping = db
            .get_zentao_project_mapping_by_id(input.mapping_id)?
            .ok_or_else(|| AppError::NotFound("禅道项目映射不存在".to_string()))?;
        if !mapping.enabled {
            return Err(AppError::InvalidInput("禅道项目映射已禁用".to_string()));
        }
        let project = db
            .list_knowledge_projects(&KnowledgeListInput {
                project_id: Some(mapping.knowledge_project_id),
                release_id: None,
                source_id: None,
                keyword: None,
                status: None,
                offset: Some(0),
                limit: Some(1),
            })?
            .items
            .into_iter()
            .next()
            .ok_or_else(|| AppError::NotFound("禅道映射的知识项目不存在".to_string()))?;
        let entities = db.list_zentao_entities_for_mapping(mapping.id)?;
        let source = db.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            id: None,
            source_key: format!("zentao-generated-{}", mapping.id),
            project_id: Some(project.id),
            source_type: "zentao".to_string(),
            display_name: format!("禅道事实文档: {}", project.name),
            root_path: String::new(),
            git_workspace_key: String::new(),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            version_strategy: "zentao_mapping".to_string(),
            sync_mode: "manual".to_string(),
            allow_remote_embedding: mapping.allow_remote_embedding,
            enabled: true,
        })?;
        let mut version_ids = Vec::new();
        for (kind, title, content) in zentao_fact_document_templates(&project, &mapping, &entities)
        {
            let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: format!("zentao-{}-{kind}", mapping.id),
                project_id: Some(project.id),
                source_id: Some(source.id),
                doc_type: "zentao_report".to_string(),
                title,
                logical_path: format!("zentao/{}/{}.md", mapping.id, kind),
                sensitivity: "internal".to_string(),
                tags: vec!["zentao".to_string(), "facts".to_string(), kind.to_string()],
                allow_ai: true,
                allow_mcp: false,
            })?;
            let content_hash = sha256_hex(content.as_bytes());
            let version_label = format!("zentao-mapping-{}-facts-v1", mapping.id);
            if db.knowledge_document_version_exists(
                document.id,
                &version_label,
                &content_hash,
                &document.logical_path,
            )? {
                continue;
            }
            let version = db.create_knowledge_document_version(
                &crate::models::CreateKnowledgeDocumentVersionInput {
                    document_id: document.id,
                    release_id: None,
                    version_label,
                    git_branch: String::new(),
                    commit_sha: String::new(),
                    source_path: document.logical_path,
                    mime_type: "text/markdown".to_string(),
                    content_hash,
                    content,
                    parsed_meta: serde_json::json!({
                        "generatorId": "zentao-facts-v1",
                        "mappingId": mapping.id,
                        "template": kind,
                        "entityCount": entities.len(),
                    }),
                    token_estimate: 0,
                },
                &[],
            )?;
            Self::parse_and_index_document_version(db, version.id, None)?;
            version_ids.push(version.id);
        }
        let result = GenerateZentaoKnowledgeDocumentsResult {
            mapping_id: mapping.id,
            source_id: source.id,
            generated_document_version_ids: version_ids,
            entity_count: i64::try_from(entities.len()).unwrap_or(i64::MAX),
        };
        audit_knowledge(
            db,
            "knowledge_zentao_document_generate",
            "readonly",
            "成功",
            "生成禅道事实文档",
            serde_json::json!({"mappingId": result.mapping_id, "sourceId": result.source_id, "generatedVersionCount": result.generated_document_version_ids.len(), "entityCount": result.entity_count}),
        );
        Ok(result)
    }

    /// 在已生成的禅道事实文档之上生成独立 AI 摘要。摘要绝不改写事实文档；只有通过
    /// 既有 RAG 引用校验的结论才会落盘，并完整记录 Provider、模型和引用键。
    pub async fn generate_zentao_ai_summary(
        db: &Database,
        input: GenerateZentaoAiSummaryInput,
    ) -> Result<GenerateZentaoAiSummaryResult, AppError> {
        Self::require_rollout(db, "zentao")?;
        validate_positive_id(input.mapping_id, "禅道项目映射 ID")?;
        let mapping = db
            .get_zentao_project_mapping_by_id(input.mapping_id)?
            .ok_or_else(|| AppError::NotFound("禅道项目映射不存在".to_string()))?;
        if !mapping.enabled || !mapping.allow_remote_ai {
            return Err(AppError::InvalidInput(
                "禅道映射未显式允许远程 AI 摘要".to_string(),
            ));
        }
        let provider_key = required_text(&input.provider_key, "AI Provider Key")?;
        let model = required_text(&input.model, "AI 模型")?;
        let prompt = required_text(&input.prompt, "AI 摘要提示词")?;
        let source = db
            .list_knowledge_sources(Some(mapping.knowledge_project_id))?
            .into_iter()
            .find(|item| item.source_key == format!("zentao-generated-{}", mapping.id))
            .ok_or_else(|| {
                AppError::InvalidInput("请先生成禅道事实文档后再生成 AI 摘要".to_string())
            })?;
        let answer = KnowledgeRetrievalService::ask(
            db,
            KnowledgeAskInput {
                search: KnowledgeSearchInput {
                    query: format!("禅道项目概览 需求 任务 测试 风险 {prompt}"),
                    project_ids: vec![mapping.knowledge_project_id],
                    release_ids: Vec::new(),
                    source_ids: vec![source.id],
                    document_types: vec!["zentao_report".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: None,
                    limit: Some(50),
                    include_context: Some(true),
                },
                original_question: None,
                answer_mode: None,
                provider_key: provider_key.clone(),
                model: model.clone(),
                evidence_only: Some(true),
                conversation: Vec::new(),
            },
        )
        .await?;
        if answer.citations.is_empty() {
            return Err(AppError::InvalidInput(
                "AI 摘要缺少可校验事实引用，未写入知识库".to_string(),
            ));
        }
        let citation_lines = answer
            .citations
            .iter()
            .map(|citation| format!("- `{}`：{}", citation.citation_key, citation.title))
            .collect::<Vec<_>>()
            .join("\n");
        let content = format!(
            "# {} 禅道 AI 摘要\n\n> 本文是基于已生成禅道事实文档的 AI 解释，不属于事实区域；每项结论须由下列引用复核。\n\n{}\n\n## 可核验引用\n\n{}",
            source.display_name, answer.answer, citation_lines
        );
        let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: format!("zentao-ai-summary-{}", mapping.id),
            project_id: Some(mapping.knowledge_project_id),
            source_id: Some(source.id),
            doc_type: "zentao_ai_summary".to_string(),
            title: format!("禅道 AI 摘要: {}", source.display_name),
            logical_path: format!("zentao/{}/ai-summary.md", mapping.id),
            sensitivity: "internal".to_string(),
            tags: vec!["zentao".to_string(), "ai-summary".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        let content_hash = sha256_hex(content.as_bytes());
        let version = db.create_knowledge_document_version(
            &crate::models::CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: format!("zentao-mapping-{}-ai-summary-v1", mapping.id),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: document.logical_path.clone(),
                mime_type: "text/markdown".to_string(),
                content,
                content_hash,
                parsed_meta: serde_json::json!({
                    "generatorId": "zentao-ai-summary-v1",
                    "mappingId": mapping.id,
                    "providerKey": provider_key,
                    "model": model,
                    "citationKeys": answer.citations.iter().map(|item| item.citation_key.clone()).collect::<Vec<_>>(),
                    "factDocumentSourceId": source.id,
                }),
                token_estimate: 0,
            },
            &[],
        )?;
        Self::parse_and_index_document_version(db, version.id, None)?;
        audit_knowledge(
            db,
            "knowledge_zentao_ai_summary_generate",
            "remote_ai",
            "成功",
            "生成带引用的禅道 AI 摘要",
            serde_json::json!({"mappingId": mapping.id, "providerKey": provider_key, "model": model, "citationCount": answer.citations.len()}),
        );
        Ok(GenerateZentaoAiSummaryResult {
            mapping_id: mapping.id,
            document_version_id: version.id,
            citation_count: i64::try_from(answer.citations.len()).unwrap_or(i64::MAX),
            provider_key,
            model,
        })
    }

    /// 现有经验库始终是来源事实；此处仅做幂等投影，不反向更新它的内容或状态。
    /// 受限经验不保存正文，因此不会进入分块、FTS、向量、远程 Provider 或正文接口。
    pub fn import_ai_experiences(
        db: &Database,
        input: ImportKnowledgeExperiencesInput,
    ) -> Result<ImportKnowledgeExperiencesResult, AppError> {
        if let Some(project_id) = input.project_id {
            validate_positive_id(project_id, "知识项目 ID")?;
        }
        if let Some(release_id) = input.release_id {
            let release = db
                .get_knowledge_release_by_id(release_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
            if input
                .project_id
                .is_some_and(|project_id| project_id != release.project_id)
            {
                return Err(AppError::InvalidInput(
                    "经验导入的发布版本不属于当前知识项目".to_string(),
                ));
            }
        }
        let source = Self::upsert_source(
            db,
            UpsertKnowledgeSourceInput {
                id: None,
                source_key: "ai-experiences".to_string(),
                project_id: input.project_id,
                source_type: "experience".to_string(),
                display_name: "现有 AI 经验库".to_string(),
                root_path: "ai_experiences".to_string(),
                git_workspace_key: String::new(),
                include_globs: Vec::new(),
                exclude_globs: Vec::new(),
                version_strategy: "manual".to_string(),
                sync_mode: "manual".to_string(),
                allow_remote_embedding: false,
                enabled: true,
            },
        )?;
        let mut result = ImportKnowledgeExperiencesResult {
            source_id: source.id,
            scanned_count: 0,
            imported_count: 0,
            unchanged_count: 0,
            restricted_count: 0,
            generated_document_version_ids: Vec::new(),
        };
        for experience in db
            .list_ai_experiences(None)?
            .into_iter()
            .filter(|item| item.enabled)
        {
            result.scanned_count += 1;
            let logical_path = format!("experiences/{}.md", experience.experience_key);
            let content = format!(
                "# {}\n\n## 问题现象\n\n{}\n\n## 根因分析\n\n{}\n\n## 解决方案\n\n{}\n\n## 场景\n\n{}\n\n## 标签\n\n{}\n\n## 来源\n\n{}",
                experience.title, experience.symptom, experience.cause, experience.solution,
                experience.scenario, experience.tags.join(", "), experience.source,
            );
            let content_hash = sha256_hex(content.as_bytes());
            let restricted_rule = detect_sensitive_content(&content);
            let document = Self::upsert_document(
                db,
                UpsertKnowledgeDocumentInput {
                    id: None,
                    document_key: format!("experience:{}", experience.experience_key),
                    project_id: input.project_id,
                    source_id: Some(source.id),
                    doc_type: "experience".to_string(),
                    title: experience.title.clone(),
                    logical_path: logical_path.clone(),
                    sensitivity: if restricted_rule.is_some() {
                        "restricted".to_string()
                    } else {
                        "internal".to_string()
                    },
                    tags: experience.tags.clone(),
                    allow_ai: restricted_rule.is_none(),
                    allow_mcp: false,
                },
            )?;
            let version_label = format!("experience-{}", experience.updated_at);
            if db.knowledge_document_version_exists(
                document.id,
                &version_label,
                &content_hash,
                &logical_path,
            )? {
                result.unchanged_count += 1;
                continue;
            }
            if let Some(rule) = restricted_rule {
                db.create_knowledge_document_version(
                    &crate::models::CreateKnowledgeDocumentVersionInput {
                        document_id: document.id, release_id: input.release_id, version_label,
                        git_branch: String::new(), commit_sha: String::new(), source_path: logical_path,
                        mime_type: "text/markdown".to_string(), content: String::new(), content_hash,
                        parsed_meta: serde_json::json!({"source":"ai_experiences","experienceId":experience.id,"restricted":true,"skipReason":format!("sensitive_content:{rule}")}),
                        token_estimate: 0,
                    }, &[],
                )?;
                result.restricted_count += 1;
                continue;
            }
            let version = db.create_knowledge_document_version(
                &crate::models::CreateKnowledgeDocumentVersionInput {
                    document_id: document.id, release_id: input.release_id, version_label,
                    git_branch: String::new(), commit_sha: String::new(), source_path: logical_path,
                    mime_type: "text/markdown".to_string(), content, content_hash,
                    parsed_meta: serde_json::json!({"source":"ai_experiences","experienceId":experience.id}),
                    token_estimate: 0,
                }, &[],
            )?;
            Self::parse_and_index_document_version(db, version.id, None)?;
            result.imported_count += 1;
            result.generated_document_version_ids.push(version.id);
        }
        Ok(result)
    }

    /// 将已分析的代码快照编译为固定模板工程报告。报告只汇总受策略允许的元数据、符号和
    /// 关系证据，不执行源码、不请求 Git/禅道/模型，也不把未确认关系表述为事实。
    pub fn generate_code_snapshot_documents(
        db: &Database,
        input: GenerateKnowledgeCodeDocumentsInput,
    ) -> Result<GenerateKnowledgeCodeDocumentsResult, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        validate_positive_id(input.snapshot_id, "源码快照 ID")?;
        let snapshot = db
            .get_knowledge_code_snapshot_by_id(input.snapshot_id)?
            .ok_or_else(|| AppError::NotFound("源码快照不存在".to_string()))?;
        if snapshot.status != "analyzed" {
            return Err(AppError::InvalidInput(
                "仅能为已完成分析的源码快照生成工程报告".to_string(),
            ));
        }
        let source = db
            .list_knowledge_code_sources()?
            .into_iter()
            .find(|item| item.source.id == snapshot.source_id)
            .ok_or_else(|| AppError::NotFound("源码快照对应的知识源不存在".to_string()))?;
        let files = db.list_knowledge_code_files(snapshot.id)?;
        let symbols = db.list_knowledge_code_symbols(snapshot.id, None)?;
        let relations = db.list_knowledge_code_relations(snapshot.id, None, Some(1_000))?;
        let generated_document_version_ids =
            persist_code_snapshot_reports(db, &snapshot, &source, &files, &symbols, &relations)?;
        Ok(GenerateKnowledgeCodeDocumentsResult {
            snapshot_id: snapshot.id,
            source_id: source.source.id,
            generated_document_version_ids,
            file_count: i64::try_from(files.len()).unwrap_or(i64::MAX),
            symbol_count: i64::try_from(symbols.len()).unwrap_or(i64::MAX),
            relation_count: i64::try_from(relations.len()).unwrap_or(i64::MAX),
        })
    }

    /// 搜索仅限已完成分析的单个快照，避免同名符号在不同历史版本或工作树间串用。
    pub fn search_code_symbols(
        db: &Database,
        input: SearchKnowledgeCodeSymbolsInput,
    ) -> Result<Vec<KnowledgeCodeSymbol>, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        ensure_analyzed_code_snapshot(db, input.snapshot_id)?;
        db.list_knowledge_code_symbols(input.snapshot_id, input.keyword.as_deref())
    }

    /// 文件列表返回已完成分析快照的全部审计元数据，使跳过数量和原因对用户可见。
    /// 正文读取仍由 `get_code_file_content` 单独限制为 active + internal，不能通过
    /// 列表接口绕过受限内容边界。
    pub fn list_code_files(
        db: &Database,
        snapshot_id: i64,
    ) -> Result<Vec<KnowledgeCodeFile>, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        ensure_analyzed_code_snapshot(db, snapshot_id)?;
        db.list_knowledge_code_files(snapshot_id)
    }

    pub fn get_code_file_content(
        db: &Database,
        snapshot_id: i64,
        file_id: i64,
    ) -> Result<KnowledgeCodeFileContent, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        validate_positive_id(file_id, "代码文件 ID")?;
        ensure_analyzed_code_snapshot(db, snapshot_id)?;
        let file = db
            .list_knowledge_code_files(snapshot_id)?
            .into_iter()
            .find(|file| file.id == file_id)
            .ok_or_else(|| AppError::NotFound("代码文件不属于指定快照".to_string()))?;
        if file.status != "active" || file.sensitivity != "internal" {
            return Err(AppError::InvalidInput(
                "代码文件已跳过、失效或受敏感策略限制，不能查看正文".to_string(),
            ));
        }
        let document_version_id = file
            .document_version_id
            .ok_or_else(|| AppError::NotFound("代码文件没有关联可读取的文档版本".to_string()))?;
        let version = db
            .get_knowledge_document_version_by_id(document_version_id)?
            .ok_or_else(|| AppError::NotFound("代码文档版本不存在".to_string()))?;
        if !version.valid {
            return Err(AppError::InvalidInput(
                "代码文档版本已失效，不能查看历史正文".to_string(),
            ));
        }
        let document = db
            .get_knowledge_document_by_id(version.document_id)?
            .ok_or_else(|| AppError::NotFound("代码知识文档不存在".to_string()))?;
        if document.sensitivity != "internal" {
            return Err(AppError::InvalidInput(
                "代码文档受当前知识策略限制，不能返回正文".to_string(),
            ));
        }
        Ok(KnowledgeCodeFileContent {
            file,
            content: version.content,
        })
    }

    /// 返回单向、深度受限的调用/关系图。默认只使用人工确认边；显式允许时才附带候选边，
    /// 其 `confirmed=false` 状态会原样返回而不升级为事实。
    pub fn code_call_graph(
        db: &Database,
        input: KnowledgeCodeCallGraphInput,
    ) -> Result<KnowledgeCodeCallGraph, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        ensure_analyzed_code_snapshot(db, input.snapshot_id)?;
        let root_symbol_key = required_text(&input.symbol_key, "根符号键")?;
        let max_depth = input.max_depth.unwrap_or(2).clamp(1, 5);
        let include_unconfirmed = input.include_unconfirmed.unwrap_or(false);
        let symbols = db.list_knowledge_code_symbols(input.snapshot_id, None)?;
        if !symbols
            .iter()
            .any(|symbol| symbol.symbol_key == root_symbol_key)
        {
            return Err(AppError::NotFound("根符号不属于该代码快照".to_string()));
        }
        let all_edges = db.list_knowledge_code_relations(input.snapshot_id, None, Some(1_000))?;
        let (nodes, edges, truncated) = bounded_code_graph(
            &symbols,
            &all_edges,
            &[root_symbol_key.clone()],
            max_depth,
            false,
            include_unconfirmed,
        );
        Ok(KnowledgeCodeCallGraph {
            snapshot_id: input.snapshot_id,
            root_symbol_key,
            nodes,
            edges,
            max_depth,
            truncated,
        })
    }

    /// 快照比较依据不可变文件哈希，只有同一代码源内的快照可比较。相同哈希的路径变化
    /// 被记录为重命名；其余变化严格区分新增、删除与修改。
    pub fn compare_code_snapshots(
        db: &Database,
        input: CompareKnowledgeCodeSnapshotsInput,
    ) -> Result<KnowledgeCodeSnapshotComparison, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        let from_snapshot = ensure_analyzed_code_snapshot(db, input.from_snapshot_id)?;
        let to_snapshot = ensure_analyzed_code_snapshot(db, input.to_snapshot_id)?;
        if from_snapshot.source_id != to_snapshot.source_id {
            return Err(AppError::InvalidInput(
                "只能比较同一源码知识源的代码快照".to_string(),
            ));
        }
        let from_files = db.list_knowledge_code_files(from_snapshot.id)?;
        let to_files = db.list_knowledge_code_files(to_snapshot.id)?;
        let file_changes = compare_code_file_sets(&from_files, &to_files);
        let from_symbols = db
            .list_knowledge_code_symbols(from_snapshot.id, None)?
            .into_iter()
            .map(|symbol| symbol.symbol_key)
            .collect::<std::collections::BTreeSet<_>>();
        let to_symbols = db
            .list_knowledge_code_symbols(to_snapshot.id, None)?
            .into_iter()
            .map(|symbol| symbol.symbol_key)
            .collect::<std::collections::BTreeSet<_>>();
        Ok(KnowledgeCodeSnapshotComparison {
            from_snapshot,
            to_snapshot,
            file_changes,
            added_symbol_keys: to_symbols.difference(&from_symbols).cloned().collect(),
            removed_symbol_keys: from_symbols.difference(&to_symbols).cloned().collect(),
            retained_symbol_keys: from_symbols.intersection(&to_symbols).cloned().collect(),
        })
    }

    /// 影响分析仅沿已确认的反向关系扩展，候选关系不会放大影响范围。
    pub fn analyze_code_impact(
        db: &Database,
        input: AnalyzeKnowledgeCodeImpactInput,
    ) -> Result<KnowledgeCodeCallGraph, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        ensure_analyzed_code_snapshot(db, input.snapshot_id)?;
        let roots = input
            .symbol_keys
            .into_iter()
            .map(|key| required_text(&key, "影响分析符号键"))
            .collect::<Result<Vec<_>, _>>()?;
        if roots.is_empty() {
            return Err(AppError::InvalidInput(
                "至少需要一个影响分析符号键".to_string(),
            ));
        }
        let max_depth = input.max_depth.unwrap_or(2).clamp(1, 5);
        let symbols = db.list_knowledge_code_symbols(input.snapshot_id, None)?;
        let known = symbols
            .iter()
            .map(|symbol| symbol.symbol_key.as_str())
            .collect::<std::collections::HashSet<_>>();
        if roots.iter().any(|root| !known.contains(root.as_str())) {
            return Err(AppError::NotFound(
                "影响分析包含不属于该快照的符号".to_string(),
            ));
        }
        let all_edges = db.list_knowledge_code_relations(input.snapshot_id, None, Some(1_000))?;
        let (nodes, edges, truncated) =
            bounded_code_graph(&symbols, &all_edges, &roots, max_depth, true, false);
        Ok(KnowledgeCodeCallGraph {
            snapshot_id: input.snapshot_id,
            root_symbol_key: roots.join(","),
            nodes,
            edges,
            max_depth,
            truncated,
        })
    }

    async fn sync_zentao_entity_type(
        db: &Database,
        connection: &ZentaoConnection,
        mapping: &ZentaoProjectMapping,
        entity_type: &str,
    ) -> Result<ZentaoSyncResult, AppError> {
        let endpoint = zentao_sync_endpoint(&connection.endpoint_profile, entity_type)?;
        let page_size = connection.page_size.clamp(1, 200) as usize;
        let prior_cursor = db.get_zentao_sync_cursor(mapping.id, entity_type)?;
        let mut page = prior_cursor
            .as_ref()
            .and_then(|cursor| cursor.checkpoint.get("nextPage"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1)
            .clamp(1, 10_000);
        let mut fetched_count = 0_i64;
        let mut changed_count = 0_i64;
        let mut unchanged_count = 0_i64;
        let mut seen_external_ids = Vec::new();
        let mut last_updated_at = prior_cursor
            .as_ref()
            .map(|cursor| cursor.last_updated_at.clone())
            .unwrap_or_default();
        let mut last_external_id = prior_cursor
            .as_ref()
            .map(|cursor| cursor.last_external_id.clone())
            .unwrap_or_default();

        loop {
            // 删除连接或知识项目会事务化禁用映射；分页之间再次检查，避免旧内存快照
            // 继续发送远程请求。实体/游标 DAO 也会在写事务内重复校验。
            db.ensure_zentao_mapping_sync_active(mapping.id)?;
            let path = format!("{endpoint}?limit={page_size}&page={page}");
            let value = request_zentao_readonly_json(db, connection, &path).await?;
            let records = parse_zentao_records(&value).ok_or_else(|| {
                AppError::Custom(format!("禅道 {entity_type} 分页响应缺少可读取数组"))
            })?;
            for record in &records {
                let normalized = normalize_zentao_entity(mapping, connection, entity_type, record)?;
                last_updated_at = normalized
                    .source_updated_at
                    .clone()
                    .unwrap_or(last_updated_at);
                last_external_id = normalized.external_id.clone();
                seen_external_ids.push(normalized.external_id.clone());
                let (_, changed) = db.upsert_zentao_entity(&normalized)?;
                fetched_count += 1;
                if changed {
                    changed_count += 1;
                } else {
                    unchanged_count += 1;
                }
            }
            let complete = records.len() < page_size;
            let cursor = db.upsert_zentao_sync_cursor(&ZentaoSyncCursorUpdateInput {
                mapping_id: mapping.id,
                entity_type: entity_type.to_string(),
                last_updated_at: last_updated_at.clone(),
                last_external_id: last_external_id.clone(),
                checkpoint: serde_json::json!({"nextPage": if complete { 1 } else { page + 1 }}),
                completed_full_sync: complete,
            })?;
            if complete {
                let missing_confirmed_count = db.confirm_zentao_missing_entities(
                    mapping.id,
                    entity_type,
                    &seen_external_ids,
                )?;
                return Ok(ZentaoSyncResult {
                    mapping_id: mapping.id,
                    entity_type: entity_type.to_string(),
                    fetched_count,
                    changed_count,
                    unchanged_count,
                    missing_confirmed_count,
                    cursor,
                });
            }
            page += 1;
            if page > 10_000 {
                return Err(AppError::Custom(
                    "禅道分页超过安全上限，未推进成功游标".to_string(),
                ));
            }
        }
    }
    pub fn list_projects(
        db: &Database,
        input: Option<KnowledgeListInput>,
    ) -> Result<KnowledgePage<KnowledgeProject>, AppError> {
        crate::services::knowledge_domain::catalog::KnowledgeCatalogService::list_projects(
            db, input,
        )
    }

    pub fn upsert_project(
        db: &Database,
        input: UpsertKnowledgeProjectInput,
    ) -> Result<KnowledgeProject, AppError> {
        crate::services::knowledge_domain::catalog::KnowledgeCatalogService::upsert_project(
            db, input,
        )
    }

    pub fn delete_project(db: &Database, id: i64) -> Result<(), AppError> {
        crate::services::knowledge_domain::catalog::KnowledgeCatalogService::delete_project(db, id)
    }

    pub fn list_releases(
        db: &Database,
        project_id: i64,
    ) -> Result<Vec<KnowledgeRelease>, AppError> {
        crate::services::knowledge_domain::catalog::KnowledgeCatalogService::list_releases(
            db, project_id,
        )
    }

    pub fn upsert_release(
        db: &Database,
        input: UpsertKnowledgeReleaseInput,
    ) -> Result<KnowledgeRelease, AppError> {
        crate::services::knowledge_domain::catalog::KnowledgeCatalogService::upsert_release(
            db, input,
        )
    }

    pub fn delete_release(db: &Database, id: i64) -> Result<(), AppError> {
        crate::services::knowledge_domain::catalog::KnowledgeCatalogService::delete_release(db, id)
    }

    pub async fn discover_git_refs(
        db: &Database,
        workspace_key: &str,
    ) -> Result<Vec<KnowledgeGitRef>, AppError> {
        Self::require_rollout(db, "catalog")?;
        let workspace_key = required_text(workspace_key, "Git 工作区标识")?;
        let workspace = db
            .get_git_workspace(&workspace_key)?
            .ok_or_else(|| AppError::NotFound(format!("Git 工作区不存在: {workspace_key}")))?;
        let repo = Path::new(&workspace.repo_path);
        if !repo.is_dir() || !repo.join(".git").exists() {
            return Err(AppError::InvalidInput(format!(
                "Git 工作区目录无效: {}",
                workspace.repo_path
            )));
        }

        let current_branch = run_readonly_git(repo, &["branch", "--show-current"])
            .await?
            .trim()
            .to_string();
        let output = run_readonly_git(
            repo,
            &[
                "for-each-ref",
                "--sort=-creatordate",
                "--format=%(refname)%1f%(objectname)%1f%(creatordate:iso-strict)%1f%(subject)",
                "refs/heads",
                "refs/tags",
            ],
        )
        .await?;
        let mut refs = parse_git_refs(&output, &current_branch);

        let head =
            run_readonly_git(repo, &["show", "-s", "--format=%H%x1f%cI%x1f%s", "HEAD"]).await?;
        if let Some(head) = parse_head_ref(&head, &current_branch) {
            refs.insert(0, head);
        }
        Ok(refs)
    }

    pub fn list_sources(
        db: &Database,
        project_id: Option<i64>,
    ) -> Result<Vec<KnowledgeSource>, AppError> {
        Self::require_rollout(db, "catalog")?;
        if let Some(project_id) = project_id {
            validate_positive_id(project_id, "项目 ID")?;
        }
        db.list_knowledge_sources(project_id)
    }

    pub fn upsert_source(
        db: &Database,
        input: UpsertKnowledgeSourceInput,
    ) -> Result<KnowledgeSource, AppError> {
        Self::require_rollout(db, "catalog")?;
        let input = normalize_knowledge_source_input(input)?;
        db.upsert_knowledge_source(&input)
    }

    /// 为一个多选 Git 工作区请求创建独立来源，并以单个 SQLite 事务保证全有或全无。
    pub fn upsert_sources_atomically(
        db: &Database,
        inputs: Vec<UpsertKnowledgeSourceInput>,
    ) -> Result<Vec<KnowledgeSource>, AppError> {
        Self::require_rollout(db, "catalog")?;
        if inputs.len() < 2 {
            return Err(AppError::InvalidInput(
                "批量保存至少需要两个知识来源".to_string(),
            ));
        }
        let mut source_keys = std::collections::BTreeSet::new();
        let mut normalized_inputs = Vec::with_capacity(inputs.len());
        for input in inputs {
            let input = normalize_knowledge_source_input(input)?;
            if !source_keys.insert(input.source_key.clone()) {
                return Err(AppError::InvalidInput(format!(
                    "批量保存中存在重复的知识源标识: {}",
                    input.source_key
                )));
            }
            normalized_inputs.push(input);
        }
        let sources = db.upsert_knowledge_sources_atomically(&normalized_inputs)?;
        audit_knowledge(
            db,
            "knowledge_sources_batch_upsert",
            "L1",
            "成功",
            "原子保存多个知识来源",
            serde_json::json!({"sourceIds": sources.iter().map(|source| source.id).collect::<Vec<_>>() }),
        );
        Ok(sources)
    }

    pub fn delete_source(db: &Database, id: i64) -> Result<(), AppError> {
        Self::require_rollout(db, "catalog")?;
        validate_positive_id(id, "知识源 ID")?;
        db.soft_delete_knowledge_source(id)
    }

    /// 创建源码分析来源时复用通用来源的规范路径、符号链接和包含/排除规则校验，并额外
    /// 固化未跟踪文件、语言白名单与远程处理的显式授权。
    pub fn upsert_code_source(
        db: &Database,
        mut input: UpsertKnowledgeCodeSourceInput,
    ) -> Result<KnowledgeCodeSource, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        input.source.source_type = normalize_source_type(&input.source.source_type)?;
        if !matches!(
            input.source.source_type.as_str(),
            "git_workspace" | "local_directory"
        ) {
            return Err(AppError::InvalidInput(
                "源码知识源仅支持已登记 Git 工作区或用户授权本地目录".to_string(),
            ));
        }
        if !(4 * 1024..=50 * 1024 * 1024).contains(&input.max_file_size_bytes) {
            return Err(AppError::InvalidInput(
                "源码文件大小上限必须在 4KB 到 50MB 之间".to_string(),
            ));
        }
        input.allowed_languages = normalized_unique_values(input.allowed_languages);
        if input.allowed_languages.iter().any(|language| {
            !language
                .chars()
                .all(|character| character.is_ascii_alphabetic() || character == '_')
        }) {
            return Err(AppError::InvalidInput(
                "源码语言白名单只能包含英文字母和下划线".to_string(),
            ));
        }
        // 远程 AI 分析默认可用，不再要求来源额外授权。保留该字段仅为兼容已有
        // SQLite/IPC 数据；远程 Embedding 仍由 source.allow_remote_embedding 单独控制。
        input.allow_remote_processing = true;
        // 预览仅静态枚举元数据，借此在写入前验证根目录、边界和符号链接策略。
        Self::preview_source_scope(db, input.source.clone())?;
        let source = Self::upsert_source(db, input.source.clone())?;
        input.source.id = Some(source.id);
        db.upsert_knowledge_code_source(&input)
    }

    pub fn list_code_sources(db: &Database) -> Result<Vec<KnowledgeCodeSource>, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        db.list_knowledge_code_sources()
    }

    /// 源码范围预览在通用根目录校验之上增加二进制、敏感路径、文件大小与语言边界。
    /// 预览只读取少量字节判断文本属性，不返回或持久化任何源码正文。
    pub fn preview_code_source_scope(
        db: &Database,
        source_id: i64,
    ) -> Result<KnowledgeSourceScopePreview, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        validate_positive_id(source_id, "源码知识源 ID")?;
        let code_source = db
            .list_knowledge_code_sources()?
            .into_iter()
            .find(|item| item.source.id == source_id)
            .ok_or_else(|| AppError::NotFound(format!("源码知识源不存在: {source_id}")))?;
        let source = &code_source.source;
        let mut preview = Self::preview_source_scope(
            db,
            UpsertKnowledgeSourceInput {
                id: Some(source.id),
                source_key: source.source_key.clone(),
                project_id: source.project_id,
                source_type: source.source_type.clone(),
                display_name: source.display_name.clone(),
                root_path: source.root_path.clone(),
                git_workspace_key: source.git_workspace_key.clone(),
                include_globs: source.include_globs.clone(),
                exclude_globs: source.exclude_globs.clone(),
                version_strategy: source.version_strategy.clone(),
                sync_mode: source.sync_mode.clone(),
                allow_remote_embedding: source.allow_remote_embedding,
                enabled: source.enabled,
            },
        )?;
        for entry in &mut preview.entries {
            if entry.decision != "included" || entry.entry_type != "file" {
                continue;
            }
            let path = Path::new(&preview.canonical_root).join(&entry.relative_path);
            let reason = if entry.size_bytes > code_source.settings.max_file_size_bytes {
                Some("file_too_large")
            } else if looks_sensitive_code_path(&entry.relative_path) {
                Some("sensitive_path")
            } else if !is_code_language_allowed(
                &code_source.settings.allowed_languages,
                code_language_for_path(&entry.relative_path),
            ) {
                Some("language_not_allowed")
            } else if is_binary_file(&path)? {
                Some("binary_content")
            } else {
                None
            };
            if let Some(reason) = reason {
                entry.decision = "skipped".to_string();
                entry.reason = reason.to_string();
                preview.included_files = preview.included_files.saturating_sub(1);
                preview.included_bytes = preview.included_bytes.saturating_sub(entry.size_bytes);
                preview.skipped_entries += 1;
            }
        }
        preview.warnings.push(
            "源码预览默认跳过依赖/生成目录、符号链接、二进制、敏感路径和不在语言白名单内的文件。"
                .to_string(),
        );
        Ok(preview)
    }

    /// 通过 Git 对象数据库捕获不可变 Commit 快照；严禁 checkout、stash、reset 或切换分支。
    pub async fn capture_git_snapshot(
        db: &Database,
        input: CaptureKnowledgeGitSnapshotInput,
    ) -> Result<KnowledgeCodeSnapshot, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        validate_positive_id(input.source_id, "源码知识源 ID")?;
        let code_source = db
            .list_knowledge_code_sources()?
            .into_iter()
            .find(|item| item.source.id == input.source_id)
            .ok_or_else(|| AppError::NotFound(format!("源码知识源不存在: {}", input.source_id)))?;
        if code_source.source.source_type != "git_workspace" {
            return Err(AppError::InvalidInput(
                "不可变 Git 快照仅支持 git_workspace 源码知识源".to_string(),
            ));
        }
        if !code_source.source.enabled {
            return Err(AppError::InvalidInput("源码知识源已禁用".to_string()));
        }
        if let Some(release_id) = input.release_id {
            let release = db
                .get_knowledge_release_by_id(release_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
            if code_source.source.project_id != Some(release.project_id) {
                return Err(AppError::InvalidInput(
                    "源码知识源与目标版本不属于同一项目".to_string(),
                ));
            }
        }
        let workspace = db
            .get_git_workspace(&code_source.source.git_workspace_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "Git 工作区不存在: {}",
                    code_source.source.git_workspace_key
                ))
            })?;
        let repo = Path::new(&workspace.repo_path);
        if !repo.is_dir() || !repo.join(".git").exists() {
            return Err(AppError::InvalidInput("Git 工作区目录无效".to_string()));
        }
        let git_ref = validate_git_ref(&input.git_ref)?;
        let branch_name = run_readonly_git(repo, &["branch", "--show-current"])
            .await?
            .trim()
            .to_string();
        let commit_expression = format!("{git_ref}^{{commit}}");
        let commit_sha = match run_readonly_git(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &commit_expression,
            ],
        )
        .await
        {
            Ok(commit_sha) => commit_sha.trim().to_string(),
            Err(AppError::Custom(message)) if is_missing_git_ref_error(&message) => {
                let current_branch = if branch_name.is_empty() {
                    "HEAD（分离状态）"
                } else {
                    branch_name.as_str()
                };
                return Err(AppError::InvalidInput(format!(
                    "Git 引用不存在于工作区“{}”：{}。当前检出分支为“{}”，请先在该仓库拉取或创建此引用，或改选该仓库已有的分支、Tag 或 Commit SHA。",
                    code_source.source.git_workspace_key, git_ref, current_branch
                )));
            }
            Err(error) => return Err(error),
        };
        if commit_sha.len() != 40 || !commit_sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AppError::Custom("Git 未返回有效 Commit SHA".to_string()));
        }
        // 路径可能含有中文、空格甚至换行；文本模式会受 Git 的 quotePath 转义影响，
        // 后续分析必须使用 NUL 分隔的原始 tree 条目，避免快照计数与实际读取不一致。
        let tree_entries = list_git_tree(repo, &commit_sha).await?;
        let file_count = i64::try_from(tree_entries.len())
            .map_err(|_| AppError::Custom("Git 快照文件数量超出范围".to_string()))?;
        // 同一 Commit 可以同时作为不同项目版本的固定证据。快照身份必须包含 release，
        // 否则先前的未绑定/旧版本快照会被静默复用，破坏版本隔离。
        let release_scope = input
            .release_id
            .map(|release_id| release_id.to_string())
            .unwrap_or_else(|| "unbound".to_string());
        let snapshot_key = format!(
            "git:{}:{}:{}",
            code_source.source.source_key,
            release_scope,
            sha256_hex(commit_sha.as_bytes())
        );
        db.upsert_knowledge_code_snapshot(&CreateKnowledgeCodeSnapshotInput {
            snapshot_key,
            source_id: code_source.source.id,
            project_id: code_source.source.project_id,
            release_id: input.release_id,
            snapshot_type: "git_commit".to_string(),
            ref_name: git_ref,
            commit_sha,
            base_commit_sha: String::new(),
            branch_name,
            worktree_dirty: false,
            dirty_state: serde_json::json!({}),
            captured_at: Utc::now().to_rfc3339(),
            file_count,
            analyzer_version: "knowledge-code-snapshot-v1".to_string(),
            status: "captured".to_string(),
        })
    }

    /// 采集当前工作树的只读隔离快照。状态与哈希均来自当前本地目录，不能映射为发布事实。
    pub async fn capture_dirty_worktree_snapshot(
        db: &Database,
        input: CaptureKnowledgeDirtyWorktreeSnapshotInput,
    ) -> Result<KnowledgeCodeSnapshot, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        validate_positive_id(input.source_id, "源码知识源 ID")?;
        let code_source = db
            .list_knowledge_code_sources()?
            .into_iter()
            .find(|item| item.source.id == input.source_id)
            .ok_or_else(|| AppError::NotFound(format!("源码知识源不存在: {}", input.source_id)))?;
        if code_source.source.source_type != "git_workspace" || !code_source.source.enabled {
            return Err(AppError::InvalidInput(
                "工作树快照仅支持已启用的 Git 工作区源码知识源".to_string(),
            ));
        }
        if let Some(release_id) = input.release_id {
            let release = db
                .get_knowledge_release_by_id(release_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
            if code_source.source.project_id != Some(release.project_id) {
                return Err(AppError::InvalidInput(
                    "源码知识源与目标版本不属于同一项目".to_string(),
                ));
            }
        }
        let workspace = db
            .get_git_workspace(&code_source.source.git_workspace_key)?
            .ok_or_else(|| AppError::NotFound("Git 工作区不存在".to_string()))?;
        let repo = fs::canonicalize(&workspace.repo_path)?;
        if !repo.join(".git").exists() {
            return Err(AppError::InvalidInput("Git 工作区目录无效".to_string()));
        }
        let baseline_commit =
            run_readonly_git(repo.as_path(), &["rev-parse", "--verify", "HEAD^{commit}"])
                .await?
                .trim()
                .to_string();
        let branch_name = run_readonly_git(repo.as_path(), &["branch", "--show-current"])
            .await?
            .trim()
            .to_string();
        let status = run_readonly_git_bytes(
            repo.as_path(),
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )
        .await?;
        let changed_entries = parse_worktree_status(&status)?;
        let untracked = changed_entries
            .iter()
            .filter(|entry| entry.untracked)
            .map(|entry| entry.path.clone())
            .collect::<std::collections::HashSet<_>>();
        let include_matchers = compile_globs(&code_source.source.include_globs, "包含规则")?;
        let mut excludes = DEFAULT_EXCLUDE_GLOBS
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        excludes.extend(code_source.source.exclude_globs.clone());
        let exclude_matchers = compile_globs(&excludes, "排除规则")?;
        let files = collect_local_files(&repo, &include_matchers, &exclude_matchers)?;
        let hashes = files
            .files
            .into_iter()
            .filter(|file| {
                code_source.settings.include_untracked || !untracked.contains(&file.relative_path)
            })
            .filter(|file| file.size <= code_source.settings.max_file_size_bytes as u64)
            .filter_map(|file| {
                let language = code_language_for_path(&file.relative_path);
                (!language.is_empty()
                    && is_code_language_allowed(&code_source.settings.allowed_languages, language))
                .then(|| {
                    fs::read(&file.absolute_path).ok().map(|bytes| {
                    serde_json::json!({ "path": file.relative_path, "sha256": sha256_hex(&bytes) })
                })
                })
                .flatten()
            })
            .collect::<Vec<_>>();
        let dirty_state = serde_json::json!({
            "schemaVersion": 1,
            "baselineCommit": baseline_commit,
            "branch": branch_name,
            "includeUntracked": code_source.settings.include_untracked,
            "changes": changed_entries,
            "files": hashes,
            "semantics": "local_worktree_observation_not_release_fact",
        });
        let snapshot_key = format!(
            "worktree:{}:{}",
            code_source.source.source_key,
            sha256_hex(serde_json::to_string(&dirty_state)?.as_bytes())
        );
        let file_count = dirty_state["files"]
            .as_array()
            .map_or(0, |items| items.len() as i64);
        db.upsert_knowledge_code_snapshot(&CreateKnowledgeCodeSnapshotInput {
            snapshot_key,
            source_id: code_source.source.id,
            project_id: code_source.source.project_id,
            release_id: input.release_id,
            snapshot_type: "git_worktree".to_string(),
            ref_name: "WORKTREE".to_string(),
            commit_sha: String::new(),
            base_commit_sha: baseline_commit,
            branch_name,
            worktree_dirty: !changed_entries.is_empty(),
            dirty_state,
            captured_at: Utc::now().to_rfc3339(),
            file_count,
            analyzer_version: "knowledge-code-worktree-snapshot-v1".to_string(),
            status: "captured".to_string(),
        })
    }

    /// 对用户明确授权的非 Git 本地目录建立当前哈希快照，不读取根目录之外的内容。
    pub fn capture_local_directory_snapshot(
        db: &Database,
        input: CaptureKnowledgeLocalDirectorySnapshotInput,
    ) -> Result<KnowledgeCodeSnapshot, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        validate_positive_id(input.source_id, "源码知识源 ID")?;
        let code_source = db
            .list_knowledge_code_sources()?
            .into_iter()
            .find(|item| item.source.id == input.source_id)
            .ok_or_else(|| AppError::NotFound(format!("源码知识源不存在: {}", input.source_id)))?;
        if code_source.source.source_type != "local_directory" || !code_source.source.enabled {
            return Err(AppError::InvalidInput(
                "本地目录快照仅支持已启用的 local_directory 源码知识源".to_string(),
            ));
        }
        if input.release_id.is_some() {
            return Err(AppError::InvalidInput(
                "非 Git 本地目录快照没有发布版本或历史 Commit 语义".to_string(),
            ));
        }
        let configured_root = required_text(&code_source.source.root_path, "源码目录路径")?;
        let root = fs::canonicalize(&configured_root).map_err(|error| {
            AppError::InvalidInput(format!("源码目录无法访问: {configured_root}: {error}"))
        })?;
        if !root.is_dir() {
            return Err(AppError::InvalidInput("源码目录必须是目录".to_string()));
        }
        let include_matchers = compile_globs(&code_source.source.include_globs, "包含规则")?;
        let mut excludes = DEFAULT_EXCLUDE_GLOBS
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        excludes.extend(code_source.source.exclude_globs.clone());
        let exclude_matchers = compile_globs(&excludes, "排除规则")?;
        let collection = collect_local_files(&root, &include_matchers, &exclude_matchers)?;
        let hashes = collection
            .files
            .into_iter()
            .filter(|file| file.size <= code_source.settings.max_file_size_bytes as u64)
            .filter_map(|file| {
                let language = code_language_for_path(&file.relative_path);
                (!language.is_empty()
                    && is_code_language_allowed(&code_source.settings.allowed_languages, language))
                .then(|| {
                    fs::read(&file.absolute_path).ok().map(|bytes| {
                    serde_json::json!({ "path": file.relative_path, "sha256": sha256_hex(&bytes) })
                })
                })
                .flatten()
            })
            .collect::<Vec<_>>();
        let dirty_state = serde_json::json!({
            "schemaVersion": 1,
            "canonicalRoot": root,
            "files": hashes,
            "skippedEntries": collection.skipped,
            "truncated": collection.truncated,
            "semantics": "non_historical_local_directory",
        });
        let snapshot_key = format!(
            "local-directory:{}:{}",
            code_source.source.source_key,
            sha256_hex(serde_json::to_string(&dirty_state)?.as_bytes())
        );
        let file_count = dirty_state["files"]
            .as_array()
            .map_or(0, |items| items.len() as i64);
        db.upsert_knowledge_code_snapshot(&CreateKnowledgeCodeSnapshotInput {
            snapshot_key,
            source_id: code_source.source.id,
            project_id: code_source.source.project_id,
            release_id: None,
            snapshot_type: "local_directory".to_string(),
            ref_name: String::new(),
            commit_sha: String::new(),
            base_commit_sha: String::new(),
            branch_name: String::new(),
            worktree_dirty: false,
            dirty_state,
            captured_at: Utc::now().to_rfc3339(),
            file_count,
            analyzer_version: "knowledge-code-local-directory-snapshot-v1".to_string(),
            status: "captured".to_string(),
        })
    }

    /// 将一个已创建的源码快照转化为可检索的文件、符号和代码片段。Git 快照只读取
    /// 对象数据库，目录/工作树快照只读取已授权根目录，整个过程不会 checkout 或改变用户文件。
    pub async fn analyze_code_snapshot(
        db: &Database,
        snapshot_id: i64,
    ) -> Result<KnowledgeCodeAnalysisResult, AppError> {
        Self::require_rollout(db, "code_analysis")?;
        validate_positive_id(snapshot_id, "源码快照 ID")?;
        db.set_knowledge_code_snapshot_analysis_status(snapshot_id, "analyzing", None)?;
        db.deactivate_knowledge_relations_for_snapshot(snapshot_id)?;
        match Self::analyze_code_snapshot_inner(db, snapshot_id).await {
            Ok(mut result) => {
                db.set_knowledge_code_snapshot_analysis_status(snapshot_id, "analyzed", None)?;
                // 返回给页面的快照必须反映已经持久化的最终状态；否则前端会收到进入分析
                // 前读取的 `analyzing` 快照，错误禁用后续的报告生成和 AI 草稿操作。
                result.snapshot = db
                    .get_knowledge_code_snapshot_by_id(snapshot_id)?
                    .ok_or_else(|| AppError::Custom("更新分析状态后未找到代码快照".to_string()))?;
                // 只有完整快照已进入 analyzed 状态后才公开统一证据链。这样失败或部分
                // 分析的快照不会遗留可被关系召回当成事实的 Commit/符号关系。
                Self::link_confirmed_code_snapshot_evidence(db, snapshot_id)?;
                audit_knowledge(
                    db,
                    "knowledge_code_snapshot_analyze",
                    "readonly",
                    "成功",
                    "完成代码快照分析",
                    serde_json::json!({"snapshotId": snapshot_id, "analyzedFiles": result.analyzed_files, "skippedFiles": result.skipped_files, "symbols": result.symbols, "documents": result.documents}),
                );
                Ok(result)
            }
            Err(error) => {
                // 状态持久化失败不得掩盖原始分析错误；failed 快照会被 FTS/向量通道排除。
                let _ = db.set_knowledge_code_snapshot_analysis_status(
                    snapshot_id,
                    "failed",
                    Some(&truncate_error(&error.to_string(), 500)),
                );
                audit_knowledge(
                    db,
                    "knowledge_code_snapshot_analyze",
                    "blocked",
                    "失败",
                    "代码快照分析失败",
                    serde_json::json!({"snapshotId": snapshot_id}),
                );
                Err(error)
            }
        }
    }

    async fn analyze_code_snapshot_inner(
        db: &Database,
        snapshot_id: i64,
    ) -> Result<KnowledgeCodeAnalysisResult, AppError> {
        validate_positive_id(snapshot_id, "源码快照 ID")?;
        let snapshot = db
            .get_knowledge_code_snapshot_by_id(snapshot_id)?
            .ok_or_else(|| AppError::NotFound(format!("源码快照不存在: {snapshot_id}")))?;
        let code_source = db
            .list_knowledge_code_sources()?
            .into_iter()
            .find(|item| item.source.id == snapshot.source_id)
            .ok_or_else(|| AppError::NotFound("源码快照对应的知识源不存在".to_string()))?;
        if !code_source.source.enabled {
            return Err(AppError::InvalidInput(
                "源码知识源已禁用，不能继续分析".to_string(),
            ));
        }
        db.ensure_knowledge_fts()?;
        let source_files = read_code_snapshot_files(db, &code_source, &snapshot).await?;
        let previous_snapshot = db
            .list_knowledge_code_snapshots(Some(snapshot.source_id))?
            .into_iter()
            .find(|candidate| candidate.id != snapshot.id && candidate.status == "analyzed");
        let previous_files = if let Some(previous_snapshot) = &previous_snapshot {
            db.list_knowledge_code_files(previous_snapshot.id)?
        } else {
            Vec::new()
        };
        let reusable_analysis = if let Some(previous_snapshot) = &previous_snapshot {
            let previous_symbols = db.list_knowledge_code_symbols(previous_snapshot.id, None)?;
            let symbols_by_file = previous_symbols.into_iter().fold(
                std::collections::HashMap::<i64, Vec<KnowledgeCodeSymbol>>::new(),
                |mut grouped, symbol| {
                    grouped.entry(symbol.file_id).or_default().push(symbol);
                    grouped
                },
            );
            previous_files
                .iter()
                .cloned()
                .map(|file| {
                    let symbols = symbols_by_file.get(&file.id).cloned().unwrap_or_default();
                    (file.relative_path.clone(), (file, symbols))
                })
                .collect::<std::collections::HashMap<_, _>>()
        } else {
            std::collections::HashMap::new()
        };
        // Git 历史快照使用对象级 Diff 识别变更/删除/重命名；具体符号仍由下方内容哈希
        // 复用决定，避免仅凭文件名或 Diff 状态错误跳过解析。
        let git_diff_summary = if snapshot.snapshot_type == "git_commit" {
            let predecessor = db
                .list_knowledge_code_snapshots(Some(snapshot.source_id))?
                .into_iter()
                .find(|candidate| {
                    candidate.id != snapshot.id
                        && candidate.status == "analyzed"
                        && candidate.snapshot_type == "git_commit"
                        && !candidate.commit_sha.is_empty()
                });
            if let Some(predecessor) = predecessor {
                let workspace = db
                    .get_git_workspace(&code_source.source.git_workspace_key)?
                    .ok_or_else(|| AppError::NotFound("Git 工作区不存在".to_string()))?;
                let diff = diff_git_paths(
                    Path::new(&workspace.repo_path),
                    &predecessor.commit_sha,
                    &snapshot.commit_sha,
                )
                .await?;
                Some((
                    diff.current_paths.len(),
                    diff.deleted_paths.len(),
                    diff.renamed_paths.len(),
                ))
            } else {
                None
            }
        } else {
            None
        };
        let current_paths = source_files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect::<std::collections::HashSet<_>>();
        let current_file_hashes = source_files
            .iter()
            .map(|file| (file.relative_path.clone(), sha256_hex(&file.bytes)))
            .collect::<std::collections::HashMap<_, _>>();
        let mut analyzed_files = 0_i64;
        let mut skipped_files = 0_i64;
        let mut symbols = 0_i64;
        let mut documents = 0_i64;
        let mut warnings = Vec::new();
        let mut reused_file_analyses = 0_i64;
        let mut renamed_file_analyses = 0_i64;
        let mut reused_embeddings = 0_i64;
        let mut relation_evidence = Vec::<(String, String)>::new();

        for source_file in source_files {
            let content_hash = sha256_hex(&source_file.bytes);
            let language = code_language_for_path(&source_file.relative_path).to_string();
            let is_test = is_test_code_path(&source_file.relative_path);
            let is_generated = is_generated_code_path(&source_file.relative_path);
            let skip_reason = if source_file.bytes.contains(&0) {
                Some("binary_content")
            } else if language.is_empty() {
                Some("unsupported_language")
            } else if !is_code_language_allowed(&code_source.settings.allowed_languages, &language)
            {
                Some("language_not_allowed")
            } else if source_file.bytes.len() > code_source.settings.max_file_size_bytes as usize {
                Some("file_too_large")
            } else if looks_sensitive_code_path(&source_file.relative_path) {
                Some("sensitive_file_name")
            } else {
                None
            };

            if let Some(skip_reason) = skip_reason {
                db.replace_knowledge_code_file_analysis(
                    &KnowledgeCodeFileWriteInput {
                        snapshot_id,
                        document_version_id: None,
                        relative_path: source_file.relative_path,
                        language,
                        file_size: i64::try_from(source_file.bytes.len()).unwrap_or(i64::MAX),
                        content_hash,
                        analysis_level: "skipped".to_string(),
                        is_generated,
                        is_test,
                        sensitivity: if skip_reason.starts_with("sensitive") {
                            "restricted".to_string()
                        } else {
                            "internal".to_string()
                        },
                        status: "skipped".to_string(),
                        skip_reason: skip_reason.to_string(),
                    },
                    &[],
                )?;
                skipped_files += 1;
                continue;
            }

            let raw_content = match String::from_utf8(source_file.bytes) {
                Ok(content) => content,
                Err(_) => {
                    db.replace_knowledge_code_file_analysis(
                        &KnowledgeCodeFileWriteInput {
                            snapshot_id,
                            document_version_id: None,
                            relative_path: source_file.relative_path,
                            language,
                            file_size: 0,
                            content_hash,
                            analysis_level: "skipped".to_string(),
                            is_generated,
                            is_test,
                            sensitivity: "internal".to_string(),
                            status: "skipped".to_string(),
                            skip_reason: "non_utf8_content".to_string(),
                        },
                        &[],
                    )?;
                    skipped_files += 1;
                    continue;
                }
            };
            let (content, redaction_rule) = match detect_sensitive_content(&raw_content) {
                Some(rule @ ("private_key" | "certificate")) => {
                    db.replace_knowledge_code_file_analysis(
                        &KnowledgeCodeFileWriteInput {
                            snapshot_id,
                            document_version_id: None,
                            relative_path: source_file.relative_path,
                            language,
                            file_size: i64::try_from(raw_content.len()).unwrap_or(i64::MAX),
                            content_hash,
                            analysis_level: "skipped".to_string(),
                            is_generated,
                            is_test,
                            sensitivity: "restricted".to_string(),
                            status: "skipped".to_string(),
                            skip_reason: format!("sensitive_content:{rule}"),
                        },
                        &[],
                    )?;
                    skipped_files += 1;
                    continue;
                }
                Some(rule) => (
                    KnowledgePolicyService::sanitize_remote_ai_context(&raw_content)?,
                    Some(rule),
                ),
                None => (raw_content, None),
            };
            let indexed_content_hash = sha256_hex(content.as_bytes());
            if let Some(rule) = redaction_rule {
                warnings.push(format!(
                    "{} 中命中的 {} 已脱敏后建立索引",
                    source_file.relative_path, rule
                ));
            }

            // 优先按原路径复用；路径不存在时再仅按内容哈希、语言和活动状态匹配，
            // 使 Git rename / 本地重命名不会重复执行解析。写入时仍重新计算当前路径的
            // symbol key，因而不会把旧路径的引用误带入新快照。
            let prior_analysis = reusable_analysis
                .get(&source_file.relative_path)
                .filter(|(previous_file, _)| {
                    previous_file.status == "active"
                        && previous_file.content_hash == content_hash
                        && previous_file.language == language
                })
                .or_else(|| {
                    reusable_analysis.values().find(|(previous_file, _)| {
                        previous_file.status == "active"
                            && previous_file.relative_path != source_file.relative_path
                            && previous_file.content_hash == content_hash
                            && previous_file.language == language
                    })
                });
            let analysis = prior_analysis
                .map(|(previous_file, previous_symbols)| {
                    reused_file_analyses += 1;
                    if previous_file.relative_path != source_file.relative_path {
                        renamed_file_analyses += 1;
                    }
                    CodeAnalysisResult {
                        language: previous_file.language.clone(),
                        analysis_level: previous_file.analysis_level.clone(),
                        parser_error: (!previous_file.skip_reason.is_empty())
                            .then(|| previous_file.skip_reason.clone()),
                        symbols: previous_symbols
                            .iter()
                            .map(|symbol| AnalyzedCodeSymbol {
                                kind: symbol.symbol_kind.clone(),
                                name: symbol.name.clone(),
                                qualified_name: symbol.qualified_name.clone(),
                                signature: symbol.signature.clone(),
                                start_line: symbol.start_line,
                                end_line: symbol.end_line,
                            })
                            .collect(),
                    }
                })
                .unwrap_or_else(|| {
                    P0LanguageAnalyzer::analyze_path(&source_file.relative_path, &content)
                });
            let is_markdown = analysis.language == "markdown";
            let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                id: None,
                document_key: format!(
                    "code-{}-{}",
                    code_source.source.source_key,
                    sha256_hex(source_file.relative_path.as_bytes())
                ),
                project_id: snapshot.project_id,
                source_id: Some(code_source.source.id),
                doc_type: if is_markdown {
                    "markdown".to_string()
                } else {
                    "code".to_string()
                },
                title: Path::new(&source_file.relative_path)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| source_file.relative_path.clone()),
                logical_path: source_file.relative_path.clone(),
                sensitivity: "internal".to_string(),
                tags: vec![
                    if is_markdown {
                        "markdown".to_string()
                    } else {
                        "code".to_string()
                    },
                    analysis.language.clone(),
                    snapshot.snapshot_type.clone(),
                ],
                // 远程访问仍必须通过来源授权和 PolicyService；本字段仅控制受控知识召回。
                allow_ai: true,
                allow_mcp: false,
            })?;
            let (chunks, parsed_meta, markdown_warning) = if is_markdown {
                markdown_document_chunks(&source_file.relative_path, &content)
            } else {
                (
                    code_symbol_chunks(
                        snapshot.id,
                        snapshot.project_id,
                        &analysis.language,
                        "internal",
                        &content,
                        &source_file.relative_path,
                        &analysis.symbols,
                    ),
                    serde_json::json!({
                        "parserId": "knowledge-code-analyzer-v1",
                        "analysisLevel": analysis.analysis_level,
                        "parserError": analysis.parser_error,
                        "snapshotId": snapshot.id,
                    }),
                    None,
                )
            };
            if let Some(warning) = markdown_warning {
                warnings.push(warning);
            }
            let version = db.create_knowledge_document_version(
                &crate::models::CreateKnowledgeDocumentVersionInput {
                    document_id: document.id,
                    release_id: snapshot.release_id,
                    version_label: snapshot.snapshot_key.clone(),
                    git_branch: snapshot.branch_name.clone(),
                    commit_sha: snapshot.commit_sha.clone(),
                    source_path: source_file.relative_path.clone(),
                    mime_type: if is_markdown {
                        "text/markdown".to_string()
                    } else {
                        "text/plain".to_string()
                    },
                    content: content.clone(),
                    content_hash: indexed_content_hash,
                    parsed_meta,
                    token_estimate: chunks.iter().map(|chunk| chunk.token_estimate).sum(),
                },
                &chunks,
            )?;
            if let Some((previous_file, _)) = prior_analysis {
                if let Some(previous_version_id) = previous_file.document_version_id {
                    reused_embeddings += db.copy_knowledge_chunk_embeddings_by_content_hash(
                        version.id,
                        previous_version_id,
                    )?;
                }
            }
            let written_symbols = analysis
                .symbols
                .iter()
                .map(|symbol| KnowledgeCodeSymbolWriteInput {
                    symbol_key: code_symbol_key(
                        &source_file.relative_path,
                        &symbol.qualified_name,
                        symbol.start_line,
                    ),
                    symbol_kind: symbol.kind.clone(),
                    name: symbol.name.clone(),
                    qualified_name: symbol.qualified_name.clone(),
                    signature: symbol.signature.clone(),
                    visibility: visibility_from_signature(&symbol.signature),
                    parent_symbol_key: String::new(),
                    start_line: symbol.start_line,
                    start_column: 0,
                    end_line: symbol.end_line,
                    end_column: 0,
                    doc_comment: String::new(),
                    content_hash: sha256_hex(symbol.signature.as_bytes()),
                    analysis_level: analysis.analysis_level.clone(),
                })
                .collect::<Vec<_>>();
            let relation_path = source_file.relative_path.clone();
            db.replace_knowledge_code_file_analysis(
                &KnowledgeCodeFileWriteInput {
                    snapshot_id,
                    document_version_id: Some(version.id),
                    relative_path: source_file.relative_path,
                    language: analysis.language,
                    file_size: i64::try_from(content.len()).unwrap_or(i64::MAX),
                    content_hash,
                    analysis_level: analysis.analysis_level,
                    is_generated,
                    is_test,
                    sensitivity: "internal".to_string(),
                    status: "active".to_string(),
                    skip_reason: redaction_rule
                        .map(|rule| format!("redacted_sensitive_content:{rule}"))
                        .or(analysis.parser_error)
                        .unwrap_or_default(),
                },
                &written_symbols,
            )?;
            if !is_markdown {
                relation_evidence.push((relation_path, content));
            }
            analyzed_files += 1;
            symbols += i64::try_from(written_symbols.len()).unwrap_or(i64::MAX);
            documents += 1;
        }
        let files = db.list_knowledge_code_files(snapshot_id)?;
        let all_symbols = db.list_knowledge_code_symbols(snapshot_id, None)?;
        let relations = resolve_snapshot_code_relations(&relation_evidence, &files, &all_symbols);
        db.replace_knowledge_code_relations(snapshot_id, &relations)?;
        let snapshot_changes =
            classify_code_snapshot_changes(&previous_files, &current_file_hashes);
        db.replace_knowledge_code_snapshot_changes(
            snapshot_id,
            previous_snapshot.as_ref().map(|item| item.id),
            &snapshot_changes,
        )?;
        let snapshot = db
            .get_knowledge_code_snapshot_by_id(snapshot_id)?
            .ok_or_else(|| AppError::NotFound(format!("源码快照不存在: {snapshot_id}")))?;
        if snapshot.file_count > analyzed_files + skipped_files {
            warnings.push("快照文件清单包含未进入有效分析范围的文件".to_string());
        }
        if reused_file_analyses > 0 {
            warnings.push(format!(
                "按内容哈希复用 {} 个未变文件的符号分析；关系已在新快照中重新计算",
                reused_file_analyses
            ));
        }
        if reused_embeddings > 0 {
            warnings.push(format!(
                "按片段内容哈希复用 {} 条前序快照向量；未变代码不需要重新向量化",
                reused_embeddings
            ));
        }
        if renamed_file_analyses > 0 {
            warnings.push(format!(
                "检测到 {} 个内容哈希相同的重命名文件；已重建路径相关符号键和关系",
                renamed_file_analyses
            ));
        }
        if let Some((changed, deleted, renamed)) = git_diff_summary {
            warnings.push(format!(
                "Git 对象 Diff：{} 个当前变更、{} 个删除、{} 个重命名；已按内容哈希复用并重算关系",
                changed, deleted, renamed
            ));
        }
        let removed_paths = reusable_analysis
            .keys()
            .filter(|path| !current_paths.contains(path.as_str()))
            .count();
        if removed_paths > 0 {
            warnings.push(format!(
                "检测到 {} 个已删除文件；已通过快照级关系替换使相关关系失效",
                removed_paths
            ));
        }
        let generated_reports = persist_code_snapshot_reports(
            db,
            &snapshot,
            &code_source,
            &files,
            &all_symbols,
            &db.list_knowledge_code_relations(snapshot_id, None, Some(1_000))?,
        )?;
        documents += i64::try_from(generated_reports.len()).unwrap_or(i64::MAX);
        Ok(KnowledgeCodeAnalysisResult {
            snapshot,
            analyzed_files,
            skipped_files,
            symbols,
            documents,
            warnings,
        })
    }

    pub fn preview_source_scope(
        db: &Database,
        mut input: UpsertKnowledgeSourceInput,
    ) -> Result<KnowledgeSourceScopePreview, AppError> {
        Self::require_rollout(db, "catalog")?;
        input.source_type = normalize_source_type(&input.source_type)?;
        input.include_globs = normalized_unique_values(input.include_globs);
        input.exclude_globs = normalized_unique_values(input.exclude_globs);
        if matches!(
            input.source_type.as_str(),
            "manual_markdown" | "experience" | "zentao"
        ) {
            return Ok(KnowledgeSourceScopePreview {
                source_type: input.source_type,
                canonical_root: String::new(),
                include_globs: input.include_globs,
                exclude_globs: input.exclude_globs,
                allow_remote_embedding: input.allow_remote_embedding,
                included_files: 0,
                skipped_entries: 0,
                included_bytes: 0,
                truncated: false,
                warnings: remote_processing_warnings(input.allow_remote_embedding),
                entries: Vec::new(),
            });
        }

        let configured_root = if input.source_type == "git_workspace" {
            let workspace_key = required_text(&input.git_workspace_key, "Git 工作区标识")?;
            db.get_git_workspace(&workspace_key)?
                .ok_or_else(|| AppError::NotFound(format!("Git 工作区不存在: {workspace_key}")))?
                .repo_path
        } else {
            required_text(&input.root_path, "知识源路径")?
        };
        let canonical_root = fs::canonicalize(&configured_root).map_err(|error| {
            AppError::InvalidInput(format!("知识源路径无法访问: {configured_root}: {error}"))
        })?;
        if input.source_type == "single_file" && !canonical_root.is_file() {
            return Err(AppError::InvalidInput(
                "单文件知识源必须指向普通文件".to_string(),
            ));
        }
        if input.source_type != "single_file" && !canonical_root.is_dir() {
            return Err(AppError::InvalidInput("目录知识源必须指向目录".to_string()));
        }

        let include_matchers = compile_globs(&input.include_globs, "包含规则")?;
        let mut effective_excludes = DEFAULT_EXCLUDE_GLOBS
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        effective_excludes.extend(input.exclude_globs.clone());
        effective_excludes.sort();
        effective_excludes.dedup();
        let exclude_matchers = compile_globs(&effective_excludes, "排除规则")?;
        let mut preview = KnowledgeSourceScopePreview {
            source_type: input.source_type,
            canonical_root: canonical_root.to_string_lossy().to_string(),
            include_globs: input.include_globs,
            exclude_globs: effective_excludes,
            allow_remote_embedding: input.allow_remote_embedding,
            included_files: 0,
            skipped_entries: 0,
            included_bytes: 0,
            truncated: false,
            warnings: remote_processing_warnings(input.allow_remote_embedding),
            entries: Vec::new(),
        };

        if canonical_root.is_file() {
            preview_path(
                &canonical_root,
                canonical_root.parent().unwrap_or(&canonical_root),
                &include_matchers,
                &exclude_matchers,
                &mut preview,
            )?;
            return Ok(preview);
        }

        let mut pending = vec![canonical_root.clone()];
        let mut visited = 0_usize;
        while let Some(directory) = pending.pop() {
            let mut entries = fs::read_dir(&directory)
                .map_err(|error| AppError::Io(error))?
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                visited += 1;
                if visited > SOURCE_PREVIEW_MAX_VISITED {
                    preview.truncated = true;
                    break;
                }
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path)?;
                let relative = relative_path(&canonical_root, &path)?;
                if metadata.file_type().is_symlink() {
                    push_scope_entry(
                        &mut preview,
                        KnowledgeSourceScopeEntry {
                            relative_path: relative,
                            entry_type: "symlink".to_string(),
                            decision: "skipped".to_string(),
                            reason: "symlink_not_followed".to_string(),
                            size_bytes: 0,
                        },
                    );
                    continue;
                }
                if metadata.is_dir() {
                    if matches_any(&exclude_matchers, &format!("{relative}/")) {
                        push_scope_entry(
                            &mut preview,
                            KnowledgeSourceScopeEntry {
                                relative_path: relative,
                                entry_type: "directory".to_string(),
                                decision: "skipped".to_string(),
                                reason: "excluded_by_rule".to_string(),
                                size_bytes: 0,
                            },
                        );
                    } else {
                        pending.push(path);
                    }
                    continue;
                }
                preview_path(
                    &path,
                    &canonical_root,
                    &include_matchers,
                    &exclude_matchers,
                    &mut preview,
                )?;
            }
            if preview.truncated {
                break;
            }
        }
        Ok(preview)
    }

    pub async fn sync_git_source(
        db: &Database,
        input: SyncKnowledgeGitSourceInput,
    ) -> Result<KnowledgeSourceSyncResult, AppError> {
        Self::require_rollout(db, "catalog")?;
        Self::sync_git_source_with_runtime(db, input, None).await
    }

    async fn sync_git_source_with_runtime(
        db: &Database,
        input: SyncKnowledgeGitSourceInput,
        runtime: Option<&KnowledgeJobRuntime<'_>>,
    ) -> Result<KnowledgeSourceSyncResult, AppError> {
        validate_positive_id(input.source_id, "知识源 ID")?;
        let source = db
            .get_knowledge_source_by_id(input.source_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {}", input.source_id)))?;
        if source.source_type != "git_workspace" {
            return Err(AppError::InvalidInput(
                "仅 git_workspace 知识源支持 Git 历史同步".to_string(),
            ));
        }
        if !source.enabled {
            return Err(AppError::InvalidInput("知识源已禁用".to_string()));
        }
        if let Some(release_id) = input.release_id {
            let release = db
                .get_knowledge_release_by_id(release_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
            if source.project_id != Some(release.project_id) {
                return Err(AppError::InvalidInput(
                    "知识源与目标版本不属于同一项目".to_string(),
                ));
            }
        }
        let git_ref = validate_git_ref(&input.git_ref)?;
        db.update_knowledge_source_sync_state(source.id, &source.last_commit_sha, "running", None)?;

        let mut result =
            sync_git_source_inner(db, &source, input.release_id, &git_ref, runtime).await;
        if let Ok(sync_result) = &mut result {
            let index_warnings = dispatch_pending_document_index_jobs(
                db,
                source.id,
                runtime.map(|value| value.app),
            )?;
            sync_result.warnings.extend(index_warnings);
        }
        match &result {
            Ok(result) => {
                db.update_knowledge_source_sync_state(
                    source.id,
                    &result.commit_sha,
                    "success",
                    None,
                )?;
                audit_knowledge(
                    db,
                    "knowledge_source_sync",
                    "readonly",
                    "成功",
                    "完成 Git 知识源同步",
                    serde_json::json!({"sourceId": source.id, "scannedFiles": result.scanned_files, "createdVersions": result.created_versions, "unchangedFiles": result.unchanged_files, "deletedPaths": result.deleted_paths, "skippedFiles": result.skipped_files}),
                );
            }
            Err(error) => {
                let message = truncate_error(&error.to_string(), 500);
                let status = if runtime.is_some()
                    && db.is_knowledge_job_cancel_requested(
                        runtime.map_or(0, |value| value.job_id),
                    )? {
                    "cancelled"
                } else {
                    "failed"
                };
                db.update_knowledge_source_sync_state(
                    source.id,
                    &source.last_commit_sha,
                    status,
                    Some(&message),
                )?;
                audit_knowledge(
                    db,
                    "knowledge_source_sync",
                    "blocked",
                    "失败",
                    "Git 知识源同步失败",
                    serde_json::json!({"sourceId": source.id, "status": status}),
                );
            }
        }
        result
    }

    pub fn sync_local_source(
        db: &Database,
        input: SyncKnowledgeLocalSourceInput,
    ) -> Result<KnowledgeSourceSyncResult, AppError> {
        Self::require_rollout(db, "catalog")?;
        let source_id = input.source_id;
        let result = Self::sync_local_source_with_runtime(db, input, None);
        if let Ok(sync) = &result {
            audit_knowledge(
                db,
                "knowledge_source_sync",
                "readonly",
                "成功",
                "完成本地知识源同步",
                serde_json::json!({"sourceId": source_id, "scannedFiles": sync.scanned_files, "createdVersions": sync.created_versions, "unchangedFiles": sync.unchanged_files, "deletedPaths": sync.deleted_paths, "skippedFiles": sync.skipped_files}),
            );
        }
        result
    }

    /// 将现有 ai_experiences 投影到统一知识文档管道。经验库仍由原有 Command/MCP
    /// 管理，此处只读取已启用的结构化经验并按内容哈希增量创建可检索版本。
    pub fn sync_experience_source(
        db: &Database,
        source_id: i64,
        release_id: Option<i64>,
    ) -> Result<KnowledgeSourceSyncResult, AppError> {
        Self::require_rollout(db, "catalog")?;
        validate_positive_id(source_id, "经验知识源 ID")?;
        let source = db
            .get_knowledge_source_by_id(source_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {source_id}")))?;
        if source.source_type != "experience" {
            return Err(AppError::InvalidInput(
                "仅 experience 知识源可同步现有经验库".to_string(),
            ));
        }
        if !source.enabled {
            return Err(AppError::InvalidInput("知识源已禁用".to_string()));
        }
        if let Some(release_id) = release_id {
            let release = db
                .get_knowledge_release_by_id(release_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
            if source.project_id != Some(release.project_id) {
                return Err(AppError::InvalidInput(
                    "同步版本不属于当前经验知识源项目".to_string(),
                ));
            }
        }

        db.update_knowledge_source_sync_state(source.id, "", "running", None)?;
        let result: Result<KnowledgeSourceSyncResult, AppError> = (|| {
            let experiences = db.list_ai_experiences(None)?;
            let mut created_versions = 0_i64;
            let mut unchanged_files = 0_i64;
            for experience in experiences.into_iter().filter(|item| item.enabled) {
                let content = render_experience_knowledge_document(&experience);
                let content_hash = sha256_hex(content.as_bytes());
                let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
                    id: None,
                    document_key: format!("experience-{}", experience.experience_key),
                    project_id: source.project_id,
                    source_id: Some(source.id),
                    doc_type: "experience".to_string(),
                    title: experience.title.clone(),
                    logical_path: format!("experience/{}.md", experience.experience_key),
                    sensitivity: "internal".to_string(),
                    tags: {
                        let mut tags = experience.tags.clone();
                        tags.push("experience".to_string());
                        normalized_unique_values(tags)
                    },
                    allow_ai: true,
                    allow_mcp: false,
                })?;
                let version_label = format!("experience:{}", experience.updated_at);
                if db.knowledge_document_version_exists(
                    document.id,
                    &version_label,
                    &content_hash,
                    &document.logical_path,
                )? {
                    unchanged_files += 1;
                    continue;
                }
                let version = db.create_knowledge_document_version(
                    &crate::models::CreateKnowledgeDocumentVersionInput {
                        document_id: document.id,
                        release_id,
                        version_label,
                        git_branch: String::new(),
                        commit_sha: String::new(),
                        source_path: document.logical_path.clone(),
                        mime_type: "text/markdown".to_string(),
                        content,
                        content_hash,
                        parsed_meta: serde_json::json!({
                            "generatorId": "ai-experience-projection-v1",
                            "experienceKey": experience.experience_key,
                            "experienceUpdatedAt": experience.updated_at,
                        }),
                        token_estimate: 0,
                    },
                    &[],
                )?;
                Self::parse_and_index_document_version(db, version.id, None)?;
                created_versions += 1;
            }
            Ok(KnowledgeSourceSyncResult {
                source_id: source.id,
                commit_sha: String::new(),
                scanned_files: i64::try_from(db.list_ai_experiences(None)?.len())
                    .unwrap_or(i64::MAX),
                created_versions,
                unchanged_files,
                deleted_paths: 0,
                skipped_files: 0,
                warnings: Vec::new(),
            })
        })();
        match &result {
            Ok(_) => db.update_knowledge_source_sync_state(source.id, "", "success", None)?,
            Err(error) => db.update_knowledge_source_sync_state(
                source.id,
                "",
                "failed",
                Some(&truncate_error(&error.to_string(), 500)),
            )?,
        }
        result
    }

    fn sync_local_source_with_runtime(
        db: &Database,
        input: SyncKnowledgeLocalSourceInput,
        runtime: Option<&KnowledgeJobRuntime<'_>>,
    ) -> Result<KnowledgeSourceSyncResult, AppError> {
        validate_positive_id(input.source_id, "知识源 ID")?;
        let source = db
            .get_knowledge_source_by_id(input.source_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {}", input.source_id)))?;
        if !matches!(
            source.source_type.as_str(),
            "local_directory" | "single_file"
        ) {
            return Err(AppError::InvalidInput(
                "仅 local_directory 或 single_file 知识源支持本地同步".to_string(),
            ));
        }
        if !source.enabled {
            return Err(AppError::InvalidInput("知识源已禁用".to_string()));
        }
        if let Some(release_id) = input.release_id {
            let release = db
                .get_knowledge_release_by_id(release_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
            if source.project_id != Some(release.project_id) {
                return Err(AppError::InvalidInput(
                    "知识源与目标版本不属于同一项目".to_string(),
                ));
            }
        }

        db.update_knowledge_source_sync_state(source.id, &source.last_commit_sha, "running", None)?;
        let mut result = sync_local_source_inner(db, &source, input.release_id, runtime);
        if let Ok(sync_result) = &mut result {
            let index_warnings = dispatch_pending_document_index_jobs(
                db,
                source.id,
                runtime.map(|value| value.app),
            )?;
            sync_result.warnings.extend(index_warnings);
        }
        match &result {
            Ok(_) => db.update_knowledge_source_sync_state(
                source.id,
                &source.last_commit_sha,
                "success",
                None,
            )?,
            Err(error) => {
                let message = truncate_error(&error.to_string(), 500);
                let status = if runtime.is_some()
                    && db.is_knowledge_job_cancel_requested(
                        runtime.map_or(0, |value| value.job_id),
                    )? {
                    "cancelled"
                } else {
                    "failed"
                };
                db.update_knowledge_source_sync_state(
                    source.id,
                    &source.last_commit_sha,
                    status,
                    Some(&message),
                )?;
            }
        }
        result
    }

    pub fn start_source_sync_job(
        app: tauri::AppHandle,
        input: StartKnowledgeSourceSyncInput,
    ) -> Result<KnowledgeJob, AppError> {
        validate_positive_id(input.source_id, "知识源 ID")?;
        let state = app.state::<AppState>();
        let source = state
            .db
            .get_knowledge_source_by_id(input.source_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {}", input.source_id)))?;
        if !matches!(
            source.source_type.as_str(),
            "git_workspace" | "local_directory" | "single_file"
        ) {
            return Err(AppError::InvalidInput(
                "当前知识源类型尚不支持后台同步".to_string(),
            ));
        }
        let frozen_git_ref = if let Some(release_id) = input.release_id {
            let release = state
                .db
                .get_knowledge_release_by_id(release_id)?
                .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
            if source.project_id != Some(release.project_id) {
                return Err(AppError::InvalidInput(
                    "知识源与目标版本不属于同一项目".to_string(),
                ));
            }
            if source.source_type != "git_workspace" {
                None
            } else {
                let mut frozen_commit = None;
                for manifest in state
                    .db
                    .list_knowledge_release_repository_manifests(release_id)?
                    .into_iter()
                {
                    if manifest.inclusion_status != "ready"
                        || manifest.resolved_commit_sha.trim().is_empty()
                    {
                        continue;
                    }
                    let binding = state
                        .db
                        .get_knowledge_project_repository_binding_including_history(
                            manifest.repository_binding_id,
                        )?;
                    if binding.is_some_and(|binding| {
                        binding.project_id == release.project_id
                            && binding.workspace_key == source.git_workspace_key
                    }) {
                        frozen_commit = Some(manifest.resolved_commit_sha);
                        break;
                    }
                }
                Some(frozen_commit.ok_or_else(|| {
                    AppError::InvalidInput(
                        "目标版本清单没有该仓库的冻结 Commit，不能同步".to_string(),
                    )
                })?)
            }
        } else {
            None
        };
        if let Some(active) = state
            .db
            .find_active_knowledge_job("source_sync", Some(source.id))?
        {
            return Ok(active);
        }

        let mut normalized_input = input;
        normalized_input.git_ref = if let Some(frozen_ref) = frozen_git_ref {
            if let Some(requested_ref) = normalized_input.git_ref.as_deref() {
                if requested_ref.trim() != frozen_ref {
                    return Err(AppError::InvalidInput(
                        "请求 Git 引用与目标版本冻结 Commit 不一致".to_string(),
                    ));
                }
            }
            Some(frozen_ref)
        } else if source.source_type == "git_workspace" {
            Some(validate_git_ref(
                normalized_input.git_ref.as_deref().unwrap_or("HEAD"),
            )?)
        } else {
            None
        };
        let job_key = format!(
            "knowledge-source-sync-{}-{}-{}",
            source.id,
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        );
        let checkpoint = knowledge_sync_checkpoint(&normalized_input, "queued", 0, 0, None);
        let job = state.db.create_knowledge_job(&CreateKnowledgeJobInput {
            job_key,
            job_type: "source_sync".to_string(),
            source_id: Some(source.id),
            profile_id: None,
            message: "知识源同步已进入队列".to_string(),
            checkpoint,
        })?;
        emit_knowledge_job_progress(&app, &job);

        let task_app = app.clone();
        let task_input = normalized_input;
        tauri::async_runtime::spawn(async move {
            if let Err(error) = run_source_sync_job(task_app, job.id, task_input).await {
                log::warn!("知识源后台同步任务执行异常: {}", error);
            }
        });
        Ok(job)
    }

    pub fn get_job(db: &Database, job_key: &str) -> Result<KnowledgeJob, AppError> {
        Self::require_rollout(db, "catalog")?;
        let job_key = required_text(job_key, "知识任务标识")?;
        db.get_knowledge_job(&job_key)?
            .ok_or_else(|| AppError::NotFound(format!("知识任务不存在: {job_key}")))
    }

    pub fn list_jobs(db: &Database, limit: Option<i64>) -> Result<Vec<KnowledgeJob>, AppError> {
        Self::require_rollout(db, "catalog")?;
        db.list_knowledge_jobs(limit.unwrap_or(50))
    }

    pub fn cancel_job(app: &tauri::AppHandle, job_key: &str) -> Result<KnowledgeJob, AppError> {
        let state = app.state::<AppState>();
        let job = Self::get_job(&state.db, job_key)?;
        let job = if job.job_type == "upload_import" {
            state.db.request_knowledge_document_upload_cancel(job.id)?
        } else if job.job_type == "embedding_build" {
            state
                .db
                .cancel_knowledge_embedding_job_and_fail_profile(job.id)?
        } else {
            state.db.request_knowledge_job_cancel(job.id)?
        };
        emit_knowledge_job_progress(app, &job);
        Ok(job)
    }

    pub fn retry_job(app: tauri::AppHandle, job_key: &str) -> Result<KnowledgeJob, AppError> {
        let state = app.state::<AppState>();
        let job = Self::get_job(&state.db, job_key)?;
        match job.job_type.as_str() {
            "source_sync" => {
                let input =
                    serde_json::from_value::<StartKnowledgeSourceSyncInput>(job.checkpoint.clone())
                        .map_err(|_| {
                            AppError::Custom("知识任务检查点缺少可恢复的同步参数".to_string())
                        })?;
                let restarted = state.db.restart_knowledge_job(job.id)?;
                emit_knowledge_job_progress(&app, &restarted);
                let task_app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = run_source_sync_job(task_app, restarted.id, input).await {
                        log::warn!("知识源重试任务执行异常: {}", error);
                    }
                });
                Ok(restarted)
            }
            "embedding_build" => {
                let profile_id = job.profile_id.ok_or_else(|| {
                    AppError::Custom("向量构建任务缺少 Embedding Profile".to_string())
                })?;
                let profile = state
                    .db
                    .get_knowledge_embedding_profile_by_id(profile_id)?
                    .ok_or_else(|| {
                        AppError::NotFound("向量构建任务对应的 Profile 不存在".to_string())
                    })?;
                // 真实构建错误会将非活动蓝绿 Profile 标记为 failed。用户选择重试时
                // 必须显式把它重新置为 building，随后才允许复用持久化 checkpoint；
                // interrupted/cancelled 情况保留原 building 状态，不重复改写生命周期。
                if profile.status == "failed" {
                    state
                        .db
                        .begin_knowledge_embedding_profile_build(profile_id)?;
                }
                if !matches!(profile.mode.as_str(), "local" | "remote") {
                    return Err(AppError::InvalidInput(
                        "Embedding Profile 模式仅支持 local 或 remote".to_string(),
                    ));
                }
                let restarted = state.db.restart_knowledge_job(job.id)?;
                emit_knowledge_job_progress(&app, &restarted);
                let task_app = app.clone();
                let retry_key = job.job_key.clone();
                let batch_input = BuildKnowledgeEmbeddingBatchInput {
                    profile_id,
                    job_key: Some(retry_key),
                    batch_size: None,
                };
                if profile.mode == "remote" {
                    tauri::async_runtime::spawn(async move {
                        let state = task_app.state::<AppState>();
                        if let Err(error) = KnowledgeEmbeddingService::build_remote_embedding_batch(
                            &state.db,
                            batch_input,
                        )
                        .await
                        {
                            log::warn!("远程向量构建重试任务执行异常: {}", error);
                        }
                    });
                } else {
                    tauri::async_runtime::spawn_blocking(move || {
                        let state = task_app.state::<AppState>();
                        let result = task_app
                            .path()
                            .app_data_dir()
                            .map_err(|error| {
                                AppError::Custom(format!("无法获取应用数据目录: {error}"))
                            })
                            .and_then(|app_data_dir| {
                                KnowledgeEmbeddingService::build_local_embedding_batch(
                                    &state.db,
                                    &app_data_dir,
                                    batch_input,
                                )
                            });
                        if let Err(error) = result {
                            log::warn!("本地向量构建重试任务执行异常: {}", error);
                        }
                    });
                }
                Ok(restarted)
            }
            "document_index" => KnowledgeDocumentJobService::retry_document_index_job(app, job.id),
            "project_version_backfill" => {
                KnowledgeDocumentJobService::retry_project_version_backfill(app, job)
            }
            "upload_import" => {
                KnowledgeUploadImportJobService::retry_upload_import_job(app, job.id)
            }
            _ => Err(AppError::InvalidInput(
                "当前知识任务类型尚不支持重试".to_string(),
            )),
        }
    }

    pub fn recover_interrupted_jobs(db: &Database) -> Result<i64, AppError> {
        db.recover_interrupted_knowledge_jobs(0)
    }

    pub fn list_documents(
        db: &Database,
        input: Option<KnowledgeListInput>,
    ) -> Result<KnowledgePage<KnowledgeDocument>, AppError> {
        KnowledgeDocumentService::list_documents(db, input)
    }

    pub fn get_document_detail(
        db: &Database,
        document_id: i64,
    ) -> Result<KnowledgeDocumentDetail, AppError> {
        KnowledgeDocumentService::get_document_detail(db, document_id)
    }

    pub fn list_document_versions(
        db: &Database,
        document_id: i64,
    ) -> Result<Vec<KnowledgeDocumentVersion>, AppError> {
        KnowledgeDocumentService::list_document_versions(db, document_id)
    }

    pub fn list_document_chunks(
        db: &Database,
        document_version_id: i64,
    ) -> Result<Vec<crate::models::KnowledgeChunk>, AppError> {
        Self::require_rollout(db, "catalog")?;
        validate_positive_id(document_version_id, "知识文档版本 ID")?;
        let version = db
            .get_knowledge_document_version_by_id(document_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档版本不存在: {document_version_id}"))
            })?;
        let document = db
            .get_knowledge_document_by_id(version.document_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档不存在: {}", version.document_id))
            })?;
        KnowledgePolicyService::authorize_content_output(&document)?;
        db.list_knowledge_chunks(document_version_id)
    }

    pub fn compare_document_versions(
        db: &Database,
        input: CompareKnowledgeDocumentVersionsInput,
    ) -> Result<KnowledgeDocumentComparison, AppError> {
        KnowledgeDocumentService::compare_document_versions(db, input)
    }

    pub fn get_citation_detail(
        db: &Database,
        chunk_id: i64,
    ) -> Result<KnowledgeCitationDetail, AppError> {
        Self::require_rollout(db, "catalog")?;
        validate_positive_id(chunk_id, "知识片段 ID")?;
        let chunk = db
            .get_knowledge_chunk_by_id(chunk_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识片段不存在: {chunk_id}")))?;
        let version = db
            .get_knowledge_document_version_by_id(chunk.document_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档版本不存在: {}", chunk.document_version_id))
            })?;
        let document = db
            .get_knowledge_document_by_id(version.document_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档不存在: {}", version.document_id))
            })?;
        KnowledgePolicyService::authorize_content_output(&document)?;
        if !version.valid
            || document.deleted_at.is_some()
            || document.status != "active"
            || !document.allow_ai
        {
            return Err(AppError::NotFound("该引用当前不可读取".to_string()));
        }
        if let Some(source_id) = document.source_id {
            let source = db
                .get_knowledge_source_by_id(source_id)?
                .ok_or_else(|| AppError::NotFound("引用所属知识源不存在".to_string()))?;
            if !source.enabled {
                return Err(AppError::NotFound("引用所属知识源已禁用".to_string()));
            }
        }
        let start_line = chunk
            .location
            .get("startLine")
            .and_then(serde_json::Value::as_i64);
        let end_line = chunk
            .location
            .get("endLine")
            .and_then(serde_json::Value::as_i64);
        let snapshot_id = chunk
            .location
            .get("snapshotId")
            .and_then(serde_json::Value::as_i64);
        if let Some(snapshot_id) = snapshot_id {
            let snapshot = db
                .get_knowledge_code_snapshot_by_id(snapshot_id)?
                .ok_or_else(|| AppError::NotFound("代码引用所属快照不存在".to_string()))?;
            let code_file_is_readable = document.doc_type == "code_report"
                || db
                    .list_knowledge_code_files(snapshot_id)?
                    .iter()
                    .any(|file| {
                        file.document_version_id == Some(version.id)
                            && file.status == "active"
                            && file.sensitivity == "internal"
                    });
            if snapshot.status != "analyzed" || !code_file_is_readable {
                return Err(AppError::NotFound("代码引用当前不可读取".to_string()));
            }
        }
        let symbol_key = chunk
            .location
            .get("symbolKey")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let is_code = snapshot_id.is_some();
        let citation = KnowledgeCitation {
            citation_key: if let Some(snapshot_id) = snapshot_id {
                format!("code:snapshot:{snapshot_id}:chunk:{}", chunk.id)
            } else {
                format!(
                    "document:{}:version:{}:chunk:{}",
                    document.id, version.id, chunk.id
                )
            },
            source_type: if is_code {
                "code_snapshot".to_string()
            } else {
                "knowledge_document".to_string()
            },
            document_id: Some(document.id),
            document_version_id: Some(version.id),
            chunk_id: Some(chunk.id),
            project_id: document.project_id,
            release_id: version.release_id,
            title: document.title.clone(),
            logical_path: if version.source_path.trim().is_empty() {
                document.logical_path.clone()
            } else {
                version.source_path.clone()
            },
            heading_path: chunk.heading_path.clone(),
            commit_sha: version.commit_sha.clone(),
            external_key: String::new(),
            snapshot_id,
            symbol_key,
            start_line,
            end_line,
            excerpt: chunk.content.chars().take(400).collect(),
        };
        Ok(KnowledgeCitationDetail {
            citation,
            document,
            version,
            chunk,
        })
    }

    /// 写入前约束实体类型、关系类型和来源，避免把任意 JSON 或用户展示文本误当作
    /// 可遍历的事实边。未确认 AI 建议可保存，但检索服务只会把 confirmed 关系作为证据。
    pub fn upsert_relation(
        db: &Database,
        mut input: UpsertKnowledgeRelationInput,
    ) -> Result<KnowledgeRelation, AppError> {
        Self::require_rollout(db, "hybrid_rag")?;
        input.from_type = relation_token(&input.from_type, "关系起点类型")?;
        input.from_key = relation_key(&input.from_key, "关系起点标识")?;
        input.relation_type = relation_token(&input.relation_type, "关系类型")?;
        input.to_type = relation_token(&input.to_type, "关系终点类型")?;
        input.to_key = relation_key(&input.to_key, "关系终点标识")?;
        input.source = relation_token(&input.source, "关系来源")?;
        if !input.confidence.is_finite() || !(0.0..=1.0).contains(&input.confidence) {
            return Err(AppError::InvalidInput(
                "关系置信度必须是 0 到 1 之间的有限数".to_string(),
            ));
        }
        if !input.evidence.is_object() && !input.evidence.is_array() {
            return Err(AppError::InvalidInput(
                "关系证据必须是结构化对象或数组".to_string(),
            ));
        }
        input.sensitivity = normalize_sensitivity(&input.sensitivity)?;
        if input.project_id.is_none()
            && input.release_id.is_none()
            && input.document_version_id.is_none()
            && input.snapshot_id.is_none()
            && input.sensitivity != "restricted"
        {
            return Err(AppError::InvalidInput(
                "没有可验证项目、版本、文档或快照归属的关系只能保存为 restricted".to_string(),
            ));
        }
        validate_relation_scope(db, &mut input)?;
        db.upsert_knowledge_relation(&input)
    }

    pub fn list_relations(
        db: &Database,
        mut input: ListKnowledgeRelationsInput,
    ) -> Result<Vec<KnowledgeRelation>, AppError> {
        Self::require_rollout(db, "hybrid_rag")?;
        match (&input.entity_type, &input.entity_key) {
            (Some(entity_type), Some(entity_key)) => {
                input.entity_type = Some(relation_token(entity_type, "实体类型")?);
                input.entity_key = Some(relation_key(entity_key, "实体标识")?);
            }
            (None, None) => {}
            _ => {
                return Err(AppError::InvalidInput(
                    "查询关系时实体类型和实体标识必须同时提供".to_string(),
                ));
            }
        }
        input.project_ids.retain(|id| *id > 0);
        input.project_ids.sort_unstable();
        input.project_ids.dedup();
        input.release_ids.retain(|id| *id > 0);
        input.release_ids.sort_unstable();
        input.release_ids.dedup();
        input.sensitivities = if input.sensitivities.is_empty() {
            vec!["public".to_string(), "internal".to_string()]
        } else {
            input
                .sensitivities
                .iter()
                .map(|value| normalize_sensitivity(value))
                .collect::<Result<Vec<_>, _>>()?
        };
        db.list_knowledge_relations(&input)
    }

    pub fn confirm_relation(
        db: &Database,
        id: i64,
        confirmed: bool,
    ) -> Result<KnowledgeRelation, AppError> {
        validate_positive_id(id, "知识关系 ID")?;
        let relation = db.confirm_knowledge_relation(id, confirmed)?;
        audit_knowledge(
            db,
            "knowledge_relation_confirm",
            "L2",
            "成功",
            if confirmed {
                "确认知识关系"
            } else {
                "撤销确认知识关系"
            },
            serde_json::json!({"relationId": relation.id, "confirmed": relation.confirmed, "source": relation.source}),
        );
        Ok(relation)
    }

    /// 仅导入 Markdown front matter 中列出的显式关系。格式为
    /// `relationships: [{fromType, fromKey, relationType, toType, toKey, confidence, confirmed}]`；
    /// 缺失字段会拒绝整个调用，避免部分导入制造不可追溯的事实链。
    pub fn import_document_front_matter_relations(
        db: &Database,
        input: ImportKnowledgeDocumentRelationsInput,
    ) -> Result<Vec<KnowledgeRelation>, AppError> {
        validate_positive_id(input.document_version_id, "知识文档版本 ID")?;
        let version = db
            .get_knowledge_document_version_by_id(input.document_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档版本不存在: {}", input.document_version_id))
            })?;
        let document = db
            .get_knowledge_document_by_id(version.document_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档不存在: {}", version.document_id))
            })?;
        let relationships = version
            .parsed_meta
            .get("frontMatter")
            .and_then(|front_matter| front_matter.get("relationships"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut imported = Vec::with_capacity(relationships.len());
        for (index, relation) in relationships.into_iter().enumerate() {
            let object = relation.as_object().ok_or_else(|| {
                AppError::InvalidInput(format!("front matter relationships[{index}] 必须是对象"))
            })?;
            let text = |key: &str| -> Result<String, AppError> {
                object
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
                    .ok_or_else(|| {
                        AppError::InvalidInput(format!(
                            "front matter relationships[{index}].{key} 必须是非空字符串"
                        ))
                    })
            };
            let confidence = object
                .get("confidence")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0);
            let confirmed = object
                .get("confirmed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            imported.push(Self::upsert_relation(
                db,
                UpsertKnowledgeRelationInput {
                    id: None,
                    project_id: document.project_id,
                    release_id: version.release_id,
                    document_version_id: Some(version.id),
                    snapshot_id: None,
                    sensitivity: document.sensitivity.clone(),
                    from_type: text("fromType")?,
                    from_key: text("fromKey")?,
                    relation_type: text("relationType")?,
                    to_type: text("toType")?,
                    to_key: text("toKey")?,
                    evidence: serde_json::json!({
                        "kind": "front_matter",
                        "documentVersionId": version.id,
                        "sourcePath": version.source_path,
                        "index": index,
                    }),
                    confidence,
                    confirmed,
                    source: "front_matter".to_string(),
                },
            )?);
        }
        Ok(imported)
    }

    /// 只识别 Commit message 中由调用方配置的需求、任务和缺陷标识。没有显式标识时不建立
    /// 关系；默认作为已确认的 Git 事实，调用方可显式降级为未确认候选。
    pub fn import_commit_message_relations(
        db: &Database,
        input: ImportKnowledgeCommitRelationsInput,
    ) -> Result<Vec<KnowledgeRelation>, AppError> {
        let commit_sha = input.commit_sha.trim().to_ascii_lowercase();
        if !Regex::new(r"^[0-9a-f]{7,40}$")
            .expect("静态 Commit SHA 正则必须有效")
            .is_match(&commit_sha)
        {
            return Err(AppError::InvalidInput(
                "Commit SHA 必须是 7 到 40 位十六进制字符".to_string(),
            ));
        }
        let message = input.commit_message.trim();
        if message.is_empty() || message.len() > 8 * 1024 || message.chars().any(char::is_control) {
            return Err(AppError::InvalidInput(
                "Commit message 不能为空、不能含控制字符且不能超过 8KB".to_string(),
            ));
        }
        let configured_prefixes = input.entity_prefixes.unwrap_or_else(|| {
            vec![
                "req".to_string(),
                "story".to_string(),
                "task".to_string(),
                "bug".to_string(),
                "test".to_string(),
            ]
        });
        let prefixes = configured_prefixes
            .into_iter()
            .map(|prefix| prefix.trim().to_ascii_lowercase())
            .filter(|prefix| !prefix.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        if prefixes.is_empty()
            || prefixes.len() > 8
            || prefixes.iter().any(|prefix| {
                prefix.len() > 32
                    || !prefix
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '_')
            })
        {
            return Err(AppError::InvalidInput(
                "Commit 实体标识前缀必须为 1 至 8 个字母、数字或下划线组成的值".to_string(),
            ));
        }
        let prefix_pattern = prefixes
            .iter()
            .map(|prefix| regex::escape(prefix))
            .collect::<Vec<_>>()
            .join("|");
        let pattern = Regex::new(&format!(r"(?i)\b({prefix_pattern})[#_ -]?(\d+)\b"))
            .expect("静态禅道标识正则必须有效");
        let snapshot_scope = if let Some(snapshot_id) = input.snapshot_id {
            validate_positive_id(snapshot_id, "代码快照 ID")?;
            let snapshot = db
                .get_knowledge_code_snapshot_by_id(snapshot_id)?
                .ok_or_else(|| AppError::NotFound(format!("代码快照不存在: {snapshot_id}")))?;
            if snapshot.status != "analyzed" {
                return Err(AppError::InvalidInput(
                    "只有已完成分析的代码快照可以导入 Commit 证据关系".to_string(),
                ));
            }
            if snapshot.commit_sha.is_empty()
                || !(snapshot.commit_sha.starts_with(&commit_sha)
                    || commit_sha.starts_with(&snapshot.commit_sha))
            {
                return Err(AppError::InvalidInput(
                    "Commit SHA 与代码快照不一致，不能建立跨范围关系".to_string(),
                ));
            }
            Some(snapshot)
        } else {
            None
        };
        let mut imported = Vec::new();
        for capture in pattern.captures_iter(message) {
            let Some(kind) = capture.get(1) else { continue };
            let Some(id) = capture.get(2) else { continue };
            let entity_type = match kind.as_str().to_ascii_lowercase().as_str() {
                "req" | "story" => "requirement",
                "task" => "task",
                "bug" => "bug",
                "test" | "case" => "test",
                _ => continue,
            };
            let (project_id, release_id, snapshot_id, sensitivity, from_type, from_key) =
                if let Some(snapshot) = &snapshot_scope {
                    let candidate_types: &[&str] = match entity_type {
                        "requirement" => &["stories", "story_changes"],
                        "task" => &["tasks", "worklogs"],
                        "bug" => &["bugs"],
                        "test" => &["tests", "test_cases", "test_runs"],
                        _ => &[],
                    };
                    // 未映射发布版本的快照只能保留显式 trailer 证据，不能凭项目内同号
                    // 实体猜测其属于任意 release；这会把后续版本资料串入历史/本地快照。
                    let entities = snapshot
                        .release_id
                        .zip(snapshot.project_id)
                        .map(|(release_id, project_id)| {
                            db.find_zentao_entities_by_scope_and_external_id(
                                project_id,
                                Some(release_id),
                                candidate_types,
                                id.as_str(),
                            )
                        })
                        .transpose()?
                        .unwrap_or_default();
                    if entities.len() > 1 {
                        return Err(AppError::InvalidInput(format!(
                            "Commit 标识 {}-{} 在当前项目/版本匹配多个禅道实体，请先收窄映射",
                            kind.as_str(),
                            id.as_str()
                        )));
                    }
                    if let Some(entity) = entities.into_iter().next() {
                        (
                            snapshot.project_id,
                            snapshot.release_id,
                            Some(snapshot.id),
                            "internal".to_string(),
                            "zentao_entity".to_string(),
                            entity.external_key,
                        )
                    } else {
                        // 明确 Commit trailer 仍是来源事实，但没有本地禅道实体时不能伪造
                        // 已映射关系；保留在快照作用域中，供后续同步后人工确认。
                        (
                            snapshot.project_id,
                            snapshot.release_id,
                            Some(snapshot.id),
                            "internal".to_string(),
                            entity_type.to_string(),
                            format!("{}-{}", kind.as_str().to_ascii_uppercase(), id.as_str()),
                        )
                    }
                } else {
                    (
                        None,
                        None,
                        None,
                        "restricted".to_string(),
                        entity_type.to_string(),
                        format!("{}-{}", kind.as_str().to_ascii_uppercase(), id.as_str()),
                    )
                };
            imported.push(Self::upsert_relation(
                db,
                UpsertKnowledgeRelationInput {
                    id: None,
                    project_id,
                    release_id,
                    document_version_id: None,
                    snapshot_id,
                    sensitivity,
                    from_type,
                    from_key,
                    relation_type: if entity_type == "test" {
                        "verified_by".to_string()
                    } else {
                        "implemented_by".to_string()
                    },
                    to_type: "commit".to_string(),
                    to_key: commit_sha.clone(),
                    evidence: serde_json::json!({
                        "kind": "commit_message",
                        "commit": commit_sha,
                        "messageSha256": format!("{:x}", Sha256::digest(message.as_bytes())),
                        "matchedIdentifier": capture.get(0).map(|matched| matched.as_str()),
                    }),
                    confidence: 1.0,
                    confirmed: input.confirmed.unwrap_or(true),
                    source: "commit_trailer".to_string(),
                },
            )?);
        }
        Ok(imported)
    }

    /// 将已完成分析的快照中可直接核验的事实投影到通用关系表。这里不提升静态推断
    /// 的 calls/imports 边：只有快照—Commit、发布—快照和符号属于确定性元数据事实。
    fn link_confirmed_code_snapshot_evidence(
        db: &Database,
        snapshot_id: i64,
    ) -> Result<(), AppError> {
        let snapshot = db
            .get_knowledge_code_snapshot_by_id(snapshot_id)?
            .ok_or_else(|| AppError::NotFound(format!("代码快照不存在: {snapshot_id}")))?;
        if snapshot.status != "analyzed" {
            return Err(AppError::InvalidInput(
                "未完成分析的代码快照不能建立证据链".to_string(),
            ));
        }
        let snapshot_key = snapshot.id.to_string();
        if !snapshot.commit_sha.is_empty() {
            Self::upsert_relation(
                db,
                UpsertKnowledgeRelationInput {
                    id: None,
                    project_id: snapshot.project_id,
                    release_id: snapshot.release_id,
                    document_version_id: None,
                    snapshot_id: Some(snapshot.id),
                    sensitivity: "internal".to_string(),
                    from_type: "code_snapshot".to_string(),
                    from_key: snapshot_key.clone(),
                    relation_type: "captured_from".to_string(),
                    to_type: "git_commit".to_string(),
                    to_key: snapshot.commit_sha.clone(),
                    evidence: serde_json::json!({
                        "kind": "code_snapshot_commit",
                        "snapshotId": snapshot.id,
                        "snapshotType": snapshot.snapshot_type,
                        "refName": snapshot.ref_name,
                    }),
                    confidence: 1.0,
                    confirmed: true,
                    source: "code_snapshot".to_string(),
                },
            )?;
        }
        if let Some(release_id) = snapshot.release_id {
            Self::upsert_relation(
                db,
                UpsertKnowledgeRelationInput {
                    id: None,
                    project_id: snapshot.project_id,
                    release_id: Some(release_id),
                    document_version_id: None,
                    snapshot_id: Some(snapshot.id),
                    sensitivity: "internal".to_string(),
                    from_type: "release".to_string(),
                    from_key: release_id.to_string(),
                    relation_type: "implemented_in".to_string(),
                    to_type: "code_snapshot".to_string(),
                    to_key: snapshot_key.clone(),
                    evidence: serde_json::json!({
                        "kind": "release_code_snapshot",
                        "snapshotId": snapshot.id,
                    }),
                    confidence: 1.0,
                    confirmed: true,
                    source: "code_snapshot".to_string(),
                },
            )?;
        }
        for symbol in db.list_knowledge_code_symbols(snapshot.id, None)? {
            Self::upsert_relation(
                db,
                UpsertKnowledgeRelationInput {
                    id: None,
                    project_id: snapshot.project_id,
                    release_id: snapshot.release_id,
                    document_version_id: None,
                    snapshot_id: Some(snapshot.id),
                    sensitivity: "internal".to_string(),
                    from_type: "code_snapshot".to_string(),
                    from_key: snapshot_key.clone(),
                    relation_type: "contains".to_string(),
                    to_type: "code_symbol".to_string(),
                    to_key: symbol.symbol_key,
                    evidence: serde_json::json!({
                        "kind": "code_symbol_declaration",
                        "snapshotId": snapshot.id,
                        "fileId": symbol.file_id,
                        "startLine": symbol.start_line,
                        "endLine": symbol.end_line,
                    }),
                    confidence: 1.0,
                    confirmed: true,
                    source: "code_snapshot".to_string(),
                },
            )?;
        }
        Ok(())
    }

    pub fn preview_parse_and_chunk(
        input: KnowledgeParseAndChunkInput,
    ) -> Result<KnowledgeParseAndChunkResult, AppError> {
        KnowledgeParserService::parse_and_chunk(input)
    }

    pub fn parse_and_index_document_version(
        db: &Database,
        document_version_id: i64,
        options: Option<crate::models::KnowledgeChunkOptions>,
    ) -> Result<KnowledgeParseAndChunkResult, AppError> {
        Self::require_rollout(db, "catalog")?;
        validate_positive_id(document_version_id, "知识文档版本 ID")?;
        let version = db
            .get_knowledge_document_version_by_id(document_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档版本不存在: {document_version_id}"))
            })?;
        let result = KnowledgeParserService::parse_and_chunk(KnowledgeParseAndChunkInput {
            document: KnowledgeParseInput {
                source_path: version.source_path,
                mime_type: version.mime_type,
                content: version.content,
                binary_content: None,
            },
            options,
        })?;
        let token_estimate = result
            .chunks
            .iter()
            .map(|chunk| chunk.token_estimate)
            .sum::<i64>();
        let parsed_meta = serde_json::json!({
            "parserId": result.parsed.parser_id,
            "normalizationVersion": result.parsed.normalization_version,
            "chunkStrategyId": result.chunk_strategy_id,
            "frontMatter": result.parsed.front_matter,
            "warnings": result.parsed.warnings,
        });
        let parse_artifact = parse_artifact_from_result(document_version_id, None, &result)?;
        db.replace_knowledge_document_chunks_with_parse_artifact(
            document_version_id,
            &parsed_meta,
            token_estimate,
            &result.chunks,
            &parse_artifact,
        )?;
        Ok(result)
    }

    pub fn upsert_document(
        db: &Database,
        mut input: UpsertKnowledgeDocumentInput,
    ) -> Result<KnowledgeDocument, AppError> {
        Self::require_rollout(db, "catalog")?;
        input.document_key = normalize_key(&input.document_key, "文档标识")?;
        input.title = required_text(&input.title, "文档标题")?;
        input.doc_type = required_text(&input.doc_type, "文档类型")?.to_lowercase();
        input.logical_path = input.logical_path.trim().to_string();
        input.sensitivity = normalize_sensitivity(&input.sensitivity)?;
        input.tags = normalized_unique_values(input.tags);
        db.upsert_knowledge_document(&input)
    }

    pub fn delete_document(db: &Database, id: i64) -> Result<(), AppError> {
        KnowledgeDocumentService::soft_delete(db, id).map(|_| ())
    }

    pub fn ensure_fts(db: &Database) -> Result<KnowledgeFtsCapability, AppError> {
        Self::require_rollout(db, "catalog")?;
        db.ensure_knowledge_fts()
    }

    pub fn rebuild_fts(db: &Database) -> Result<i64, AppError> {
        Self::require_rollout(db, "catalog")?;
        db.rebuild_knowledge_fts()
    }
}

pub(crate) fn empty_list_input() -> KnowledgeListInput {
    KnowledgeListInput {
        project_id: None,
        release_id: None,
        source_id: None,
        keyword: None,
        status: None,
        offset: None,
        limit: None,
    }
}

pub(crate) fn validate_positive_id(value: i64, field: &str) -> Result<(), AppError> {
    if value <= 0 {
        return Err(AppError::InvalidInput(format!("{field} 必须大于 0")));
    }
    Ok(())
}

pub(crate) fn required_text(value: &str, field: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::InvalidInput(format!("{field}不能为空")));
    }
    Ok(value.to_string())
}

/// 关系表为兼容历史数据保留了软引用字段而不是数据库外键，因此在写入信任边界把项目、
/// 版本、文档版本和快照归属归并校验，避免伪造的组合 ID 进入后续图谱或检索证据。
fn validate_relation_scope(
    db: &Database,
    input: &mut UpsertKnowledgeRelationInput,
) -> Result<(), AppError> {
    let mut project_id = input.project_id;
    if let Some(value) = project_id {
        validate_positive_id(value, "关系项目 ID")?;
        if !db.knowledge_project_exists(value)? {
            return Err(AppError::NotFound(format!("知识项目不存在: {value}")));
        }
    }
    if let Some(release_id) = input.release_id {
        validate_positive_id(release_id, "关系项目版本 ID")?;
        let release = db
            .get_knowledge_release_by_id(release_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
        merge_relation_project_scope(&mut project_id, release.project_id, "项目版本")?;
    }
    if let Some(document_version_id) = input.document_version_id {
        validate_positive_id(document_version_id, "关系文档版本 ID")?;
        let version = db
            .get_knowledge_document_version_by_id(document_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档版本不存在: {document_version_id}"))
            })?;
        let document = db
            .get_knowledge_document_by_id(version.document_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识文档不存在: {}", version.document_id))
            })?;
        let document_project_id = document.project_id.ok_or_else(|| {
            AppError::InvalidInput("关系文档版本尚未归属知识项目，不能作为范围证据".to_string())
        })?;
        merge_relation_project_scope(&mut project_id, document_project_id, "文档版本")?;
        if let Some(release_id) = input.release_id {
            let bindings = db.list_knowledge_document_version_bindings(document_version_id)?;
            if !bindings.iter().any(|binding| {
                binding.release_id == Some(release_id)
                    || binding.cross_version_scope == "project_all_versions"
            }) {
                return Err(AppError::InvalidInput(
                    "关系文档版本不属于所选项目版本，不能混合引用".to_string(),
                ));
            }
        }
    }
    if let Some(snapshot_id) = input.snapshot_id {
        validate_positive_id(snapshot_id, "关系代码快照 ID")?;
        let snapshot = db
            .get_knowledge_code_snapshot_by_id(snapshot_id)?
            .ok_or_else(|| AppError::NotFound(format!("代码快照不存在: {snapshot_id}")))?;
        if let Some(snapshot_project_id) = snapshot.project_id {
            merge_relation_project_scope(&mut project_id, snapshot_project_id, "代码快照")?;
        }
        if let (Some(release_id), Some(snapshot_release_id)) =
            (input.release_id, snapshot.release_id)
        {
            if release_id != snapshot_release_id {
                return Err(AppError::InvalidInput(
                    "代码快照不属于所选项目版本，不能混合引用".to_string(),
                ));
            }
        }
    }
    input.project_id = project_id;
    Ok(())
}

fn merge_relation_project_scope(
    project_id: &mut Option<i64>,
    scoped_project_id: i64,
    source: &str,
) -> Result<(), AppError> {
    if let Some(current) = *project_id {
        if current != scoped_project_id {
            return Err(AppError::InvalidInput(format!(
                "关系项目与{source}归属不一致，不能混合引用"
            )));
        }
    } else {
        *project_id = Some(scoped_project_id);
    }
    Ok(())
}

fn relation_token(value: &str, field: &str) -> Result<String, AppError> {
    let value = required_text(value, field)?;
    let valid = Regex::new(r"^[a-z][a-z0-9_-]{0,63}$")
        .expect("静态关系 token 正则必须有效")
        .is_match(&value);
    if !valid {
        return Err(AppError::InvalidInput(format!(
            "{field}只能使用小写字母、数字、短横线和下划线，且必须以字母开头"
        )));
    }
    Ok(value)
}

fn relation_key(value: &str, field: &str) -> Result<String, AppError> {
    let value = required_text(value, field)?;
    if value.len() > 512 || value.chars().any(char::is_control) {
        return Err(AppError::InvalidInput(format!(
            "{field}不能包含控制字符且长度不能超过 512"
        )));
    }
    Ok(value)
}

pub(crate) fn normalize_key(value: &str, field: &str) -> Result<String, AppError> {
    let value = required_text(value, field)?.to_lowercase();
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(AppError::InvalidInput(format!(
            "{field}只能包含字母、数字、点、下划线和连字符"
        )));
    }
    Ok(value)
}

fn normalize_source_type(value: &str) -> Result<String, AppError> {
    let value = value.trim().to_lowercase();
    let allowed = [
        "git_workspace",
        "local_directory",
        "single_file",
        "manual_markdown",
        "experience",
        "zentao",
    ];
    if !allowed.contains(&value.as_str()) {
        return Err(AppError::InvalidInput(format!(
            "不支持的知识源类型: {value}"
        )));
    }
    Ok(value)
}

fn normalize_knowledge_source_input(
    mut input: UpsertKnowledgeSourceInput,
) -> Result<UpsertKnowledgeSourceInput, AppError> {
    input.source_key = normalize_key(&input.source_key, "知识源标识")?;
    input.display_name = required_text(&input.display_name, "知识源名称")?;
    input.source_type = normalize_source_type(&input.source_type)?;
    input.version_strategy = normalize_version_strategy(&input.version_strategy)?;
    input.sync_mode = normalize_sync_mode(&input.sync_mode)?;
    input.include_globs = normalized_unique_values(input.include_globs);
    input.exclude_globs = normalized_unique_values(input.exclude_globs);
    Ok(input)
}

/// 经验正文由结构化字段确定性生成，避免读取或修改经验库的 Markdown 文件；引用只保留
/// 已有 references_json 元数据，仍由原经验库命令负责展示和维护。
fn render_experience_knowledge_document(experience: &crate::models::AiExperience) -> String {
    format!(
        "# {}\n\n## 症状\n{}\n\n## 原因\n{}\n\n## 方案\n{}\n\n## 场景\n{}\n\n## 标签\n{}\n\n## 来源\n{}\n\n## 引用元数据\n```json\n{}\n```\n",
        experience.title,
        experience.symptom,
        experience.cause,
        experience.solution,
        experience.scenario,
        experience.tags.join(", "),
        experience.source,
        experience.references_json,
    )
}

/// 知识库操作审计只保存稳定标识、计数和策略结果，绝不携带文档正文、凭据引用值或
/// 远端响应。审计失败不能阻断主业务，但任何调用方都必须在成功后显式记录事件。
pub(crate) fn audit_knowledge(
    db: &Database,
    action: &str,
    risk: &str,
    result: &str,
    summary: &str,
    detail: serde_json::Value,
) {
    let _ = AuditService::create(
        db,
        CreateAuditLogInput {
            actor: "local-user".to_string(),
            source: "knowledge".to_string(),
            server_alias: String::new(),
            action: action.to_string(),
            risk: risk.to_string(),
            result: result.to_string(),
            summary: summary.to_string(),
            detail_json: Some(detail.to_string()),
            request_id: None,
            approval_id: None,
        },
    );
}

fn normalize_version_strategy(value: &str) -> Result<String, AppError> {
    let value = value.trim().to_lowercase();
    // `unversioned` 是目录与手工知识的显式安全语义：没有证据时绝不推断为最新版本。
    // `release_mapping` 和 `incremental` 是前端登记来源时提供的稳定策略名；保留旧值，
    // 使已存在来源与新的 UI 都能被同一服务边界接受。
    if !matches!(
        value.as_str(),
        "unversioned" | "manual" | "git_ref" | "release_mapping" | "zentao_mapping"
    ) {
        return Err(AppError::InvalidInput(format!("不支持的版本策略: {value}")));
    }
    Ok(value)
}

fn normalize_sync_mode(value: &str) -> Result<String, AppError> {
    let value = value.trim().to_lowercase();
    if !matches!(
        value.as_str(),
        "incremental" | "manual" | "scheduled" | "on_change"
    ) {
        return Err(AppError::InvalidInput(format!("不支持的同步模式: {value}")));
    }
    Ok(value)
}

fn normalize_sensitivity(value: &str) -> Result<String, AppError> {
    let value = value.trim().to_lowercase();
    if !matches!(
        value.as_str(),
        "public" | "internal" | "confidential" | "restricted"
    ) {
        return Err(AppError::InvalidInput(format!("不支持的敏感级别: {value}")));
    }
    Ok(value)
}

pub(crate) fn normalized_unique_values(values: Vec<String>) -> Vec<String> {
    let mut normalized = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn compile_globs(patterns: &[String], field: &str) -> Result<Vec<GlobMatcher>, AppError> {
    patterns
        .iter()
        .map(|pattern| {
            Glob::new(pattern)
                .map(|glob| glob.compile_matcher())
                .map_err(|error| AppError::InvalidInput(format!("{field}无效: {pattern}: {error}")))
        })
        .collect()
}

fn matches_any(matchers: &[GlobMatcher], relative_path: &str) -> bool {
    matchers
        .iter()
        .any(|matcher| matcher.is_match(relative_path))
}

fn preview_path(
    path: &Path,
    canonical_root: &Path,
    include_matchers: &[GlobMatcher],
    exclude_matchers: &[GlobMatcher],
    preview: &mut KnowledgeSourceScopePreview,
) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        push_scope_entry(
            preview,
            KnowledgeSourceScopeEntry {
                relative_path: relative_path(canonical_root, path)?,
                entry_type: "symlink".to_string(),
                decision: "skipped".to_string(),
                reason: "symlink_not_followed".to_string(),
                size_bytes: 0,
            },
        );
        return Ok(());
    }
    if !metadata.is_file() {
        push_scope_entry(
            preview,
            KnowledgeSourceScopeEntry {
                relative_path: relative_path(canonical_root, path)?,
                entry_type: "other".to_string(),
                decision: "skipped".to_string(),
                reason: "not_regular_file".to_string(),
                size_bytes: 0,
            },
        );
        return Ok(());
    }
    KnowledgePolicyService::authorize_local_file(canonical_root, path)?;
    let relative = relative_path(canonical_root, path)?;
    let included_by_rule = include_matchers.is_empty() || matches_any(include_matchers, &relative);
    let excluded_by_rule = matches_any(exclude_matchers, &relative);
    let size_bytes = i64::try_from(metadata.len()).unwrap_or(i64::MAX);
    if included_by_rule && !excluded_by_rule {
        preview.included_files += 1;
        preview.included_bytes = preview.included_bytes.saturating_add(size_bytes);
        push_scope_entry(
            preview,
            KnowledgeSourceScopeEntry {
                relative_path: relative,
                entry_type: "file".to_string(),
                decision: "included".to_string(),
                reason: "within_effective_scope".to_string(),
                size_bytes,
            },
        );
    } else {
        push_scope_entry(
            preview,
            KnowledgeSourceScopeEntry {
                relative_path: relative,
                entry_type: "file".to_string(),
                decision: "skipped".to_string(),
                reason: if excluded_by_rule {
                    "excluded_by_rule".to_string()
                } else {
                    "not_included_by_rule".to_string()
                },
                size_bytes,
            },
        );
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> Result<String, AppError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| AppError::InvalidInput(format!("路径越出授权根目录: {}", path.display())))?;
    let normalized = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Ok(if normalized.is_empty() {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        normalized
    })
}

fn push_scope_entry(preview: &mut KnowledgeSourceScopePreview, entry: KnowledgeSourceScopeEntry) {
    if entry.decision == "skipped" {
        preview.skipped_entries += 1;
    }
    if preview.entries.len() < SOURCE_PREVIEW_MAX_ENTRIES {
        preview.entries.push(entry);
    } else {
        preview.truncated = true;
    }
}

fn remote_processing_warnings(allow_remote_embedding: bool) -> Vec<String> {
    if allow_remote_embedding {
        vec!["该知识源允许远程 Embedding；实际发送前仍需通过敏感级别和内容安全检查。".to_string()]
    } else {
        Vec::new()
    }
}

struct KnowledgeJobRuntime<'a> {
    job_id: i64,
    app: &'a tauri::AppHandle,
    input: &'a StartKnowledgeSourceSyncInput,
}

/// 同步只负责冻结正文；所有新版本在此处统一进入文档索引任务。后台同步使用
/// `spawn_blocking`，手动调用旧同步 Command 没有 AppHandle 时则在当前调用中完成，
/// 两条入口都不会留下“版本已写入但永远没有 chunks”的半成品状态。
fn dispatch_pending_document_index_jobs(
    db: &Database,
    source_id: i64,
    app: Option<&tauri::AppHandle>,
) -> Result<Vec<String>, AppError> {
    let jobs = db.queue_knowledge_document_index_jobs_for_source(source_id)?;
    let mut warnings = Vec::new();
    for (document_version_id, job_id) in jobs {
        if let Some(app) = app {
            KnowledgeDocumentJobService::spawn_document_index_job(
                app.clone(),
                document_version_id,
                job_id,
            );
            continue;
        }
        let job =
            KnowledgeDocumentJobService::run_document_index_job(db, document_version_id, job_id)?;
        if job.status == "failed" {
            warnings.push(format!(
                "文档版本 {document_version_id} 已进入索引任务，但解析失败，可在任务列表中重试"
            ));
        }
    }
    Ok(warnings)
}

async fn run_source_sync_job(
    app: tauri::AppHandle,
    job_id: i64,
    input: StartKnowledgeSourceSyncInput,
) -> Result<(), AppError> {
    let initial_checkpoint = knowledge_sync_checkpoint(&input, "sync", 0, 0, None);
    {
        let state = app.state::<AppState>();
        let running = state.db.mark_knowledge_job_running(
            job_id,
            "sync",
            "正在同步知识源",
            &initial_checkpoint,
        )?;
        emit_knowledge_job_progress(&app, &running);
    }

    let heartbeat_app = app.clone();
    let heartbeat_task = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(KNOWLEDGE_JOB_HEARTBEAT_SECONDS)).await;
            let state = heartbeat_app.state::<AppState>();
            match state.db.touch_knowledge_job_heartbeat(job_id) {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    log::warn!("更新知识任务心跳失败: {}", error);
                    break;
                }
            }
        }
    });

    let result = {
        let state = app.state::<AppState>();
        let runtime = KnowledgeJobRuntime {
            job_id,
            app: &app,
            input: &input,
        };
        let source = state
            .db
            .get_knowledge_source_by_id(input.source_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识源不存在: {}", input.source_id)))?;
        match source.source_type.as_str() {
            "git_workspace" => {
                KnowledgeService::sync_git_source_with_runtime(
                    &state.db,
                    SyncKnowledgeGitSourceInput {
                        source_id: input.source_id,
                        release_id: input.release_id,
                        git_ref: input.git_ref.clone().unwrap_or_else(|| "HEAD".to_string()),
                    },
                    Some(&runtime),
                )
                .await
            }
            "local_directory" | "single_file" => KnowledgeService::sync_local_source_with_runtime(
                &state.db,
                SyncKnowledgeLocalSourceInput {
                    source_id: input.source_id,
                    release_id: input.release_id,
                },
                Some(&runtime),
            ),
            _ => Err(AppError::InvalidInput(
                "当前知识源类型尚不支持后台同步".to_string(),
            )),
        }
    };
    heartbeat_task.abort();

    let state = app.state::<AppState>();
    let cancelled = state.db.is_knowledge_job_cancel_requested(job_id)?;
    let (status, message, error, checkpoint) = if cancelled {
        (
            "cancelled",
            "知识源同步已安全取消".to_string(),
            None,
            knowledge_sync_checkpoint(&input, "cancelled", 0, 0, None),
        )
    } else {
        match result {
            Ok(sync_result) => (
                "completed",
                "知识源同步完成".to_string(),
                None,
                serde_json::json!({
                    "sourceId": input.source_id,
                    "releaseId": input.release_id,
                    "gitRef": input.git_ref,
                    "stage": "completed",
                    "current": sync_result.scanned_files,
                    "total": sync_result.scanned_files,
                    "result": sync_result,
                }),
            ),
            Err(error) => {
                let sanitized = truncate_error(&error.to_string(), 500);
                (
                    "failed",
                    "知识源同步失败".to_string(),
                    Some(sanitized.clone()),
                    serde_json::json!({
                        "sourceId": input.source_id,
                        "releaseId": input.release_id,
                        "gitRef": input.git_ref,
                        "stage": "failed",
                        "error": sanitized,
                    }),
                )
            }
        }
    };
    let finished =
        state
            .db
            .finish_knowledge_job(job_id, status, &message, error.as_deref(), &checkpoint)?;
    emit_knowledge_job_progress(&app, &finished);
    Ok(())
}

fn knowledge_sync_checkpoint(
    input: &StartKnowledgeSourceSyncInput,
    stage: &str,
    current: i64,
    total: i64,
    last_path: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "sourceId": input.source_id,
        "releaseId": input.release_id,
        "gitRef": input.git_ref,
        "stage": stage,
        "current": current,
        "total": total,
        "lastPath": last_path,
    })
}

fn report_knowledge_job_progress(
    db: &Database,
    runtime: Option<&KnowledgeJobRuntime<'_>>,
    stage: &str,
    current: i64,
    total: i64,
    last_path: Option<&str>,
) -> Result<(), AppError> {
    let Some(runtime) = runtime else {
        return Ok(());
    };
    if db.is_knowledge_job_cancel_requested(runtime.job_id)? {
        return Err(AppError::Custom(KNOWLEDGE_JOB_CANCELLED.to_string()));
    }
    let checkpoint = knowledge_sync_checkpoint(runtime.input, stage, current, total, last_path);
    let message = last_path.map_or_else(
        || format!("正在处理知识源（{current}/{total}）"),
        |path| format!("正在处理 {path}（{current}/{total}）"),
    );
    if !db.update_knowledge_job_progress(runtime.job_id, current, total, &message, &checkpoint)? {
        if db.is_knowledge_job_cancel_requested(runtime.job_id)? {
            return Err(AppError::Custom(KNOWLEDGE_JOB_CANCELLED.to_string()));
        }
        return Err(AppError::Custom("知识任务进度状态已失效".to_string()));
    }
    let job = db
        .get_knowledge_job_by_id(runtime.job_id)?
        .ok_or_else(|| AppError::NotFound(format!("知识任务不存在: {}", runtime.job_id)))?;
    emit_knowledge_job_progress(runtime.app, &job);
    Ok(())
}

pub(crate) fn emit_knowledge_job_progress(app: &tauri::AppHandle, job: &KnowledgeJob) {
    let stage = job
        .checkpoint
        .get("stage")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&job.status)
        .to_string();
    let error = job.error.as_ref().map(|message| KnowledgeErrorDetail {
        code: format!("KNOWLEDGE_JOB_{}", job.status.to_uppercase()),
        message: message.clone(),
        stage: stage.clone(),
        source_key: job
            .source_id
            .map(|source_id| source_id.to_string())
            .unwrap_or_default(),
        retryable: matches!(job.status.as_str(), "failed" | "interrupted"),
        sanitized_details: serde_json::json!({}),
    });
    let payload = KnowledgeJobProgress {
        job_key: job.job_key.clone(),
        status: job.status.clone(),
        stage,
        current: job.progress_current,
        total: job.progress_total,
        message: job.message.clone(),
        can_cancel: matches!(job.status.as_str(), "queued" | "running") && !job.cancel_requested,
        error,
    };
    if let Err(error) = app.emit("knowledge-job-progress", payload) {
        log::warn!("知识任务进度事件推送失败: {}", error);
    }
}

async fn run_readonly_git(repo: &Path, args: &[&str]) -> Result<String, AppError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).args(args).kill_on_drop(true);
    let output = timeout(Duration::from_secs(15), command.output())
        .await
        .map_err(|_| AppError::Custom("读取 Git 引用超时".to_string()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Custom(format!(
            "读取 Git 引用失败: {}",
            stderr.trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| AppError::Custom("Git 输出不是有效 UTF-8".to_string()))
}

async fn run_readonly_git_bytes(repo: &Path, args: &[&str]) -> Result<Vec<u8>, AppError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).args(args).kill_on_drop(true);
    let output = timeout(Duration::from_secs(30), command.output())
        .await
        .map_err(|_| AppError::Custom("读取 Git 对象超时".to_string()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Custom(format!(
            "读取 Git 对象失败: {}",
            truncate_error(stderr.trim(), 500)
        )));
    }
    Ok(output.stdout)
}

async fn sync_git_source_inner(
    db: &Database,
    source: &KnowledgeSource,
    release_id: Option<i64>,
    git_ref: &str,
    runtime: Option<&KnowledgeJobRuntime<'_>>,
) -> Result<KnowledgeSourceSyncResult, AppError> {
    let workspace = db
        .get_git_workspace(&source.git_workspace_key)?
        .ok_or_else(|| {
            AppError::NotFound(format!("Git 工作区不存在: {}", source.git_workspace_key))
        })?;
    let repo = fs::canonicalize(&workspace.repo_path)?;
    if !repo.join(".git").exists() {
        return Err(AppError::InvalidInput("Git 工作区不是有效仓库".to_string()));
    }
    let commit_expression = format!("{git_ref}^{{commit}}");
    let commit_sha = run_readonly_git(
        &repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &commit_expression,
        ],
    )
    .await?
    .trim()
    .to_string();
    if commit_sha.len() < 7
        || !commit_sha
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(AppError::Custom("Git Ref 未解析为有效 Commit".to_string()));
    }

    let include_matchers = compile_globs(&source.include_globs, "包含规则")?;
    let mut exclude_patterns = DEFAULT_EXCLUDE_GLOBS
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    exclude_patterns.extend(source.exclude_globs.clone());
    let exclude_matchers = compile_globs(&exclude_patterns, "排除规则")?;

    let mut sync_warnings = Vec::new();
    let (tree_entries, diff_paths) = if release_id.is_some() {
        // 显式版本同步必须冻结该版本的完整仓库内容。仅扫描 Commit Diff 会漏掉
        // “上个版本未变、但当前版本仍需要绑定”的文件，导致按版本检索缺少证据。
        // 读取整棵树后仍由 version_label + content_hash 检查保持幂等；同时保留 Diff
        // 的重命名元数据，让路径变化继续复用原文档，不会凭空产生重复文档。
        let tree = list_git_tree(&repo, &commit_sha).await?;
        let diff = if source.last_commit_sha.is_empty() || source.last_commit_sha == commit_sha {
            GitDiffPaths::default()
        } else if source.last_commit_sha.len() < 7
            || !source
                .last_commit_sha
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            // 历史来源可能没有可用的 Commit 游标；完整树仍可安全导入，
            // 只是无法可靠推断重命名关系。
            GitDiffPaths::default()
        } else {
            let previous_commit = format!("{}^{{commit}}", source.last_commit_sha);
            match run_readonly_git(&repo, &["cat-file", "-e", &previous_commit]).await {
                Ok(_) => match run_readonly_git(
                    &repo,
                    &[
                        "merge-base",
                        "--is-ancestor",
                        &source.last_commit_sha,
                        &commit_sha,
                    ],
                )
                .await
                {
                    Ok(_) => {
                        match diff_git_paths(&repo, &source.last_commit_sha, &commit_sha).await {
                            Ok(diff) => diff,
                            Err(error) => {
                                let warning = format!(
                                "无法读取 Git 版本差异，已继续导入完整版本树并跳过重命名识别: {}",
                                truncate_error(&error.to_string(), 300)
                            );
                                log::warn!("{warning}");
                                sync_warnings.push(warning);
                                GitDiffPaths::default()
                            }
                        }
                    }
                    Err(error) => {
                        let warning = format!(
                            "历史 Git 游标与目标版本没有祖先关系，已继续导入完整版本树并跳过重命名识别: {}",
                            truncate_error(&error.to_string(), 300)
                        );
                        log::warn!("{warning}");
                        sync_warnings.push(warning);
                        GitDiffPaths::default()
                    }
                },
                Err(error) => {
                    let warning = format!(
                        "历史 Git Commit 不可达，已继续导入完整版本树并跳过重命名识别: {}",
                        truncate_error(&error.to_string(), 300)
                    );
                    log::warn!("{warning}");
                    sync_warnings.push(warning);
                    GitDiffPaths::default()
                }
            }
        };
        (tree, diff)
    } else if source.last_commit_sha.is_empty() {
        (
            list_git_tree(&repo, &commit_sha).await?,
            GitDiffPaths::default(),
        )
    } else if source.last_commit_sha == commit_sha {
        // 未绑定版本的增量同步可以沿用 Commit 游标，避免重复读取整棵树。
        (Vec::new(), GitDiffPaths::default())
    } else {
        let diff = diff_git_paths(&repo, &source.last_commit_sha, &commit_sha).await?;
        let tree = list_git_tree(&repo, &commit_sha).await?;
        let changed_tree = tree
            .into_iter()
            .filter(|entry| diff.current_paths.contains(&entry.path))
            .collect();
        (changed_tree, diff)
    };

    let version_label = if let Some(release_id) = release_id {
        db.get_knowledge_release_by_id(release_id)?
            .map(|release| release.version)
            .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?
    } else {
        "unversioned".to_string()
    };
    let mut result = KnowledgeSourceSyncResult {
        source_id: source.id,
        commit_sha: commit_sha.clone(),
        scanned_files: 0,
        created_versions: 0,
        unchanged_files: 0,
        deleted_paths: 0,
        skipped_files: 0,
        warnings: sync_warnings,
    };
    let progress_total = i64::try_from(tree_entries.len()).unwrap_or(i64::MAX);
    report_knowledge_job_progress(db, runtime, "read_git_objects", 0, progress_total, None)?;

    // 版本化同步只建立不可变版本范围，不能把某个发布版本中不存在的路径
    // 全局标记为 deleted，否则历史版本的文档也会从检索中消失。未绑定版本的
    // 增量同步仍保留原有删除语义。重命名也延后到当前树条目写入时处理：先根据
    // Diff 找到旧文档，再由 upsert 更新当前逻辑路径；旧版本的 source_path 不会被
    // 全局重写，引用历史版本时仍能显示旧路径。
    if release_id.is_none() {
        for path in diff_paths.deleted_paths {
            if db.mark_knowledge_document_path_deleted(source.id, &path)? {
                result.deleted_paths += 1;
            }
        }
    }
    for (index, entry) in tree_entries
        .into_iter()
        .take(GIT_SYNC_MAX_FILES)
        .enumerate()
    {
        let progress_current = i64::try_from(index + 1).unwrap_or(i64::MAX);
        result.scanned_files += 1;
        // 重命名元数据必须在所有跳过分支之前解析。否则新路径命中敏感规则时，
        // 旧路径文档不会进入策略清理，旧版本的 FTS/向量仍可能继续被检索到。
        let renamed_from_path = diff_paths
            .renamed_paths
            .iter()
            .find_map(|(old_path, new_path)| {
                (new_path == &entry.path).then_some(old_path.as_str())
            });
        if entry.size > GIT_SYNC_MAX_FILE_BYTES
            || (!include_matchers.is_empty() && !matches_any(&include_matchers, &entry.path))
            || matches_any(&exclude_matchers, &entry.path)
        {
            result.skipped_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_git_objects",
                progress_current,
                progress_total,
                Some(&entry.path),
            )?;
            continue;
        }
        let object_spec = format!("{}:{}", commit_sha, entry.path);
        let bytes = run_readonly_git_bytes(&repo, &["show", "--no-ext-diff", &object_spec]).await?;
        if bytes.len() as u64 > GIT_SYNC_MAX_FILE_BYTES {
            result.skipped_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_git_objects",
                progress_current,
                progress_total,
                Some(&entry.path),
            )?;
            continue;
        }
        let Ok(content) = String::from_utf8(bytes) else {
            result.skipped_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_git_objects",
                progress_current,
                progress_total,
                Some(&entry.path),
            )?;
            continue;
        };
        if content.contains('\0') {
            result.skipped_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_git_objects",
                progress_current,
                progress_total,
                Some(&entry.path),
            )?;
            continue;
        }

        if let Some(rule) = detect_sensitive_content(&content) {
            let mut document_ids = Vec::new();
            if let Some(document) =
                db.get_knowledge_document_by_source_path(source.id, &entry.path)?
            {
                document_ids.push(document.id);
            }
            if let Some(old_path) = renamed_from_path {
                if let Some(document) =
                    db.get_knowledge_document_by_source_path(source.id, old_path)?
                {
                    if !document_ids.contains(&document.id) {
                        document_ids.push(document.id);
                    }
                }
            }
            for document_id in document_ids {
                db.restrict_knowledge_document(document_id)?;
            }
            result.skipped_files += 1;
            result.warnings.push(format!(
                "Git 文件 {} 因敏感内容规则 {rule} 被阻断，未保存正文",
                entry.path
            ));
            report_knowledge_job_progress(
                db,
                runtime,
                "read_git_objects",
                progress_current,
                progress_total,
                Some(&entry.path),
            )?;
            continue;
        }

        let content_hash = sha256_hex(content.as_bytes());
        let mut existing_document =
            db.get_knowledge_document_by_source_path(source.id, &entry.path)?;
        if existing_document.is_none() {
            if let Some(old_path) = renamed_from_path {
                existing_document =
                    db.get_knowledge_document_by_source_path(source.id, old_path)?;
            }
        }
        let document_key = existing_document.as_ref().map_or_else(
            || {
                format!(
                    "git:{}:{}",
                    source.source_key,
                    sha256_hex(entry.path.as_bytes())
                )
            },
            |document| document.document_key.clone(),
        );
        let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: existing_document.map(|document| document.id),
            document_key,
            project_id: source.project_id,
            source_id: Some(source.id),
            doc_type: document_type_for_path(&entry.path).to_string(),
            title: Path::new(&entry.path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| entry.path.clone()),
            logical_path: entry.path.clone(),
            sensitivity: "internal".to_string(),
            tags: vec!["git".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        if db.knowledge_document_version_exists(
            document.id,
            &version_label,
            &content_hash,
            &entry.path,
        )? {
            result.unchanged_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_git_objects",
                progress_current,
                progress_total,
                Some(&entry.path),
            )?;
            continue;
        }
        db.create_knowledge_document_version(
            &crate::models::CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id,
                version_label: version_label.clone(),
                git_branch: if git_ref == commit_sha {
                    String::new()
                } else {
                    git_ref.to_string()
                },
                commit_sha: commit_sha.clone(),
                source_path: entry.path.clone(),
                mime_type: document_mime_type_for_path(&entry.path).to_string(),
                content,
                content_hash,
                parsed_meta: serde_json::json!({
                    "source": "git_object",
                    "syncStrategy": "read_only",
                }),
                token_estimate: 0,
            },
            &[],
        )?;
        result.created_versions += 1;
        report_knowledge_job_progress(
            db,
            runtime,
            "read_git_objects",
            progress_current,
            progress_total,
            Some(&document.logical_path),
        )?;
    }
    if result.scanned_files >= GIT_SYNC_MAX_FILES as i64 {
        result.warnings.push(format!(
            "本次同步达到文件上限 {GIT_SYNC_MAX_FILES}，需要缩小 include 范围后重试"
        ));
    }
    Ok(result)
}

#[derive(Debug)]
struct GitTreeEntry {
    path: String,
    size: u64,
}

#[derive(Debug, Default)]
struct GitDiffPaths {
    current_paths: std::collections::HashSet<String>,
    deleted_paths: Vec<String>,
    renamed_paths: Vec<(String, String)>,
}

async fn list_git_tree(repo: &Path, commit_sha: &str) -> Result<Vec<GitTreeEntry>, AppError> {
    let output =
        run_readonly_git_bytes(repo, &["ls-tree", "-r", "-z", "--long", commit_sha]).await?;
    let mut entries = Vec::new();
    for record in output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let Some(tab_index) = record.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let header = String::from_utf8_lossy(&record[..tab_index]);
        let path = String::from_utf8_lossy(&record[tab_index + 1..]).to_string();
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 4 || fields[1] != "blob" {
            continue;
        }
        let Ok(size) = fields[3].parse::<u64>() else {
            continue;
        };
        entries.push(GitTreeEntry { path, size });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

async fn diff_git_paths(
    repo: &Path,
    previous_commit: &str,
    commit_sha: &str,
) -> Result<GitDiffPaths, AppError> {
    let output = run_readonly_git_bytes(
        repo,
        &[
            "diff",
            "--name-status",
            "-z",
            "--find-renames",
            previous_commit,
            commit_sha,
            "--",
        ],
    )
    .await?;
    parse_git_diff_paths(&output)
}

fn parse_git_diff_paths(output: &[u8]) -> Result<GitDiffPaths, AppError> {
    let fields = output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| {
            String::from_utf8(field.to_vec())
                .map_err(|_| AppError::Custom("Git Diff 路径不是有效 UTF-8".to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut result = GitDiffPaths::default();
    let mut index = 0_usize;
    while index < fields.len() {
        let status = &fields[index];
        index += 1;
        if status.starts_with('R') || status.starts_with('C') {
            if index + 1 >= fields.len() {
                return Err(AppError::Custom("Git Diff 重命名记录不完整".to_string()));
            }
            let old_path = fields[index].clone();
            let new_path = fields[index + 1].clone();
            index += 2;
            if status.starts_with('R') {
                result.renamed_paths.push((old_path, new_path.clone()));
            }
            result.current_paths.insert(new_path);
            continue;
        }
        if index >= fields.len() {
            return Err(AppError::Custom("Git Diff 路径记录不完整".to_string()));
        }
        let path = fields[index].clone();
        index += 1;
        if status.starts_with('D') {
            result.deleted_paths.push(path);
        } else {
            result.current_paths.insert(path);
        }
    }
    Ok(result)
}

fn validate_git_ref(value: &str) -> Result<String, AppError> {
    let value = required_text(value, "Git Ref")?;
    if value.starts_with('-')
        || value.contains("..")
        || value.contains("@{")
        || value.chars().any(char::is_whitespace)
    {
        return Err(AppError::InvalidInput("Git Ref 格式不安全".to_string()));
    }
    Ok(value)
}

fn is_missing_git_ref_error(message: &str) -> bool {
    [
        "Needed a single revision",
        "unknown revision",
        "ambiguous argument",
        "not a valid object name",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

fn document_type_for_path(path: &str) -> &'static str {
    if is_markdown_path(path) {
        return "markdown";
    }
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "sql" => "sql",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "rs" | "ts" | "tsx" | "js" | "jsx" | "vue" | "java" => "code",
        _ => "text",
    }
}

fn document_mime_type_for_path(path: &str) -> &'static str {
    if is_markdown_path(path) {
        "text/markdown"
    } else {
        "text/plain"
    }
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn truncate_error(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[derive(Debug)]
struct LocalFileEntry {
    absolute_path: PathBuf,
    relative_path: String,
    size: u64,
}

#[derive(Debug, Default)]
struct LocalFileCollection {
    files: Vec<LocalFileEntry>,
    skipped: i64,
    truncated: bool,
}

/// Git porcelain v1 的脱敏状态记录；路径用于定位已授权文件，绝不包含文件正文。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WorktreeChange {
    path: String,
    index_status: String,
    worktree_status: String,
    untracked: bool,
}

fn parse_worktree_status(output: &[u8]) -> Result<Vec<WorktreeChange>, AppError> {
    let records = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect::<Vec<_>>();
    let mut changes = Vec::new();
    let mut index = 0_usize;
    while index < records.len() {
        let record = records[index];
        if record.len() < 4 || record[2] != b' ' {
            return Err(AppError::Custom("Git 工作树状态格式无效".to_string()));
        }
        let index_status = record[0] as char;
        let worktree_status = record[1] as char;
        let path = String::from_utf8(record[3..].to_vec())
            .map_err(|_| AppError::Custom("Git 工作树路径不是有效 UTF-8".to_string()))?;
        if path.starts_with('/') || path.contains("../") || path == ".." {
            return Err(AppError::InvalidInput(
                "Git 工作树路径越出仓库根目录".to_string(),
            ));
        }
        changes.push(WorktreeChange {
            path,
            index_status: index_status.to_string(),
            worktree_status: worktree_status.to_string(),
            untracked: index_status == '?' && worktree_status == '?',
        });
        // porcelain -z 对重命名/复制紧接着写入原路径；它仅作状态证据，不应再误解析为状态记录。
        if matches!(index_status, 'R' | 'C') || matches!(worktree_status, 'R' | 'C') {
            index += 1;
        }
        index += 1;
    }
    changes.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(changes)
}

fn code_language_for_path(path: &str) -> &'static str {
    if is_markdown_path(path) {
        return "markdown";
    }
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "ts" | "tsx" | "js" | "jsx" => "typescript",
        "vue" => "vue",
        "java" => "java",
        "sql" => "sql",
        // MyBatis Mapper XML 是 SQL 的承载格式。将它单独标注，便于检索与展示，同时
        // 兼容既有的 `sql` 白名单，避免已有源码源需要重新配置才能获得 SQL 证据。
        "xml" => "mybatis_xml",
        _ => "",
    }
}

fn is_code_language_allowed(allowed_languages: &[String], language: &str) -> bool {
    // Markdown 是代码/文档联合分析的基础说明材料。即使是历史来源配置，也必须
    // 自动纳入，避免用户保存旧配置后 README、设计说明等仍被静默跳过。
    if language == "markdown" {
        return true;
    }
    allowed_languages.is_empty()
        || allowed_languages.iter().any(|allowed| {
            allowed == language
                || (language == "typescript" && allowed == "javascript")
                || (language == "mybatis_xml" && matches!(allowed.as_str(), "sql" | "xml"))
        })
}

#[derive(Debug)]
struct SnapshotCodeFile {
    relative_path: String,
    bytes: Vec<u8>,
}

async fn read_code_snapshot_files(
    db: &Database,
    source: &KnowledgeCodeSource,
    snapshot: &KnowledgeCodeSnapshot,
) -> Result<Vec<SnapshotCodeFile>, AppError> {
    let include_matchers = compile_globs(&source.source.include_globs, "包含规则")?;
    let mut excluded = DEFAULT_EXCLUDE_GLOBS
        .iter()
        .map(|pattern| (*pattern).to_string())
        .collect::<Vec<_>>();
    excluded.extend(source.source.exclude_globs.clone());
    let exclude_matchers = compile_globs(&excluded, "排除规则")?;
    let included = |path: &str| {
        !path.starts_with('/')
            && !path.split('/').any(|part| part == "..")
            && (include_matchers.is_empty() || matches_any(&include_matchers, path))
            && !matches_any(&exclude_matchers, path)
    };

    if snapshot.snapshot_type == "git_commit" {
        let workspace = source.source.git_workspace_key.trim();
        if workspace.is_empty() {
            return Err(AppError::InvalidInput(
                "Git 快照缺少 Git 工作区标识".to_string(),
            ));
        }
        let workspace = db
            .get_git_workspace(workspace)?
            .ok_or_else(|| AppError::NotFound("Git 工作区不存在".to_string()))?;
        let root = fs::canonicalize(&workspace.repo_path)?;
        // 不要使用 `git ls-tree --name-only` 的文本输出：当 core.quotePath 生效时，
        // 中文路径会变成带引号和八进制转义的文本，既不能被排除规则匹配，也无法
        // 作为 `commit:path` 重新读取。list_git_tree 解析 NUL 分隔的原始字节路径。
        let tree_entries = list_git_tree(&root, &snapshot.commit_sha).await?;
        let mut files = Vec::new();
        for entry in tree_entries {
            if !included(&entry.path) {
                continue;
            }
            let object = format!("{}:{}", snapshot.commit_sha, entry.path);
            let bytes = run_readonly_git_bytes(&root, &["show", "--no-ext-diff", &object]).await?;
            files.push(SnapshotCodeFile {
                relative_path: entry.path,
                bytes,
            });
        }
        return Ok(files);
    }

    let configured_root = if source.source.source_type == "git_workspace" {
        db.get_git_workspace(&source.source.git_workspace_key)?
            .ok_or_else(|| AppError::NotFound("Git 工作区不存在".to_string()))?
            .repo_path
    } else {
        required_text(&source.source.root_path, "源码目录路径")?
    };
    let root = fs::canonicalize(&configured_root).map_err(|error| {
        AppError::InvalidInput(format!("源码目录无法访问: {configured_root}: {error}"))
    })?;
    // 本地目录和工作树快照不是“稍后再扫描当前目录”的别名。捕获时记录的清单及哈希
    // 是唯一允许读取的内容；任一文件的增删改都会使快照失效，调用方必须重新捕获。
    let expected = snapshot
        .dirty_state
        .get("files")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| AppError::InvalidInput("本地源码快照缺少受控文件清单".to_string()))?
        .iter()
        .filter_map(|entry| {
            Some((
                entry.get("path")?.as_str()?.to_string(),
                entry.get("sha256")?.as_str()?.to_string(),
            ))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let collection = collect_local_files(&root, &include_matchers, &exclude_matchers)?;
    let mut files = Vec::new();
    for entry in collection
        .files
        .into_iter()
        .filter(|entry| included(&entry.relative_path))
    {
        let Some(expected_hash) = expected.get(&entry.relative_path) else {
            continue;
        };
        let bytes = fs::read(entry.absolute_path)?;
        if sha256_hex(&bytes) != *expected_hash {
            return Err(AppError::InvalidInput(format!(
                "本地源码快照已变化: {}，请重新捕获后再分析",
                entry.relative_path
            )));
        }
        files.push(SnapshotCodeFile {
            relative_path: entry.relative_path,
            bytes,
        });
    }
    if files.len() != expected.len() {
        return Err(AppError::InvalidInput(
            "本地源码快照文件清单已变化，请重新捕获后再分析".to_string(),
        ));
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn code_symbol_chunks(
    snapshot_id: i64,
    project_id: Option<i64>,
    language: &str,
    sensitivity: &str,
    content: &str,
    relative_path: &str,
    symbols: &[crate::services::knowledge_code_analyzer::AnalyzedCodeSymbol],
) -> Vec<KnowledgeChunkWriteInput> {
    let lines = content.lines().collect::<Vec<_>>();
    if symbols.is_empty() {
        return vec![KnowledgeChunkWriteInput {
            chunk_index: 0,
            heading_path: relative_path.to_string(),
            content: content.to_string(),
            content_hash: sha256_hex(content.as_bytes()),
            location: serde_json::json!({
                "snapshotId": snapshot_id,
                "projectId": project_id,
                "language": language,
                "path": relative_path,
                "sensitivity": sensitivity,
                "startLine": 1,
                "endLine": lines.len().max(1),
            }),
            token_estimate: i64::try_from(content.chars().count().div_ceil(4)).unwrap_or(i64::MAX),
        }];
    }
    symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| {
            let start = symbol.start_line.max(1) as usize;
            let next_start = symbols
                .get(index + 1)
                .map(|next| next.start_line.max(1) as usize)
                .unwrap_or_else(|| lines.len().saturating_add(1));
            let end = next_start.saturating_sub(1).max(start).min(lines.len().max(1));
            let snippet = lines
                .get(start.saturating_sub(1)..end)
                .map(|slice| slice.join("\n"))
                .unwrap_or_default();
            KnowledgeChunkWriteInput {
                chunk_index: i64::try_from(index).unwrap_or(i64::MAX),
                heading_path: format!("{}#{}", relative_path, symbol.qualified_name),
                content_hash: sha256_hex(snippet.as_bytes()),
                token_estimate: i64::try_from(snippet.chars().count().div_ceil(4)).unwrap_or(i64::MAX),
                content: snippet,
                location: serde_json::json!({
                    "snapshotId": snapshot_id,
                    "projectId": project_id,
                    "language": language,
                    "path": relative_path,
                    "sensitivity": sensitivity,
                    "startLine": start,
                    "endLine": end,
                    "symbolKey": code_symbol_key(relative_path, &symbol.qualified_name, symbol.start_line),
                    "symbol": symbol.qualified_name,
                    "signature": symbol.signature,
                }),
            }
        })
        .collect()
}

/// Markdown 在联合源码分析中使用普通文档解析链路，而不是带 `snapshotId` 的代码
/// 片段链路。这样既能让 README/设计说明参与检索，也能保证引用仍是普通文档引用，
/// 不会被检索层误判为 `code_snapshot` 证据。
fn markdown_document_chunks(
    relative_path: &str,
    content: &str,
) -> (
    Vec<KnowledgeChunkWriteInput>,
    serde_json::Value,
    Option<String>,
) {
    let input = KnowledgeParseAndChunkInput {
        document: KnowledgeParseInput {
            source_path: relative_path.to_string(),
            mime_type: "text/markdown".to_string(),
            content: content.to_string(),
            binary_content: None,
        },
        options: None,
    };
    match KnowledgeParserService::parse_and_chunk(input) {
        Ok(result) if !result.chunks.is_empty() => {
            let parsed_meta = serde_json::json!({
                "parserId": result.parsed.parser_id,
                "normalizationVersion": result.parsed.normalization_version,
                "chunkStrategyId": result.chunk_strategy_id,
                "frontMatter": result.parsed.front_matter,
                "warnings": result.parsed.warnings,
                "analysisSource": "code_snapshot_markdown",
            });
            (result.chunks, parsed_meta, None)
        }
        Ok(_) => (
            Vec::new(),
            serde_json::json!({
                "parserId": "markdown-parser-v1",
                "analysisSource": "code_snapshot_markdown",
                "warnings": ["Markdown 文档为空，未生成结构化片段"],
            }),
            None,
        ),
        Err(error) => {
            let safe_error = truncate_error(&error.to_string(), 300);
            let lines = content.lines().count().max(1);
            let chunk = KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: relative_path.to_string(),
                content: content.to_string(),
                content_hash: sha256_hex(content.as_bytes()),
                location: serde_json::json!({
                    "language": "markdown",
                    "path": relative_path,
                    "startLine": 1,
                    "endLine": lines,
                    "parserId": "markdown-parser-fallback-v1",
                }),
                token_estimate: i64::try_from(content.chars().count().div_ceil(4))
                    .unwrap_or(i64::MAX),
            };
            (
                vec![chunk],
                serde_json::json!({
                    "parserId": "markdown-parser-fallback-v1",
                    "analysisSource": "code_snapshot_markdown",
                    "parserError": safe_error,
                }),
                Some(format!(
                    "Markdown 文件 {relative_path} 解析失败，已降级为纯文本片段"
                )),
            )
        }
    }
}

fn code_symbol_key(relative_path: &str, qualified_name: &str, start_line: i64) -> String {
    format!("{}::{}@{}", relative_path, qualified_name, start_line)
}

/// 仅解析文本中可验证的静态线索。无法唯一匹配的调用不会伪造成内部边，而是保留为
/// 外部目标或直接忽略；这让调用图可以安全降级而不会把动态分派表述成确定事实。
fn resolve_snapshot_code_relations(
    evidence: &[(String, String)],
    files: &[KnowledgeCodeFile],
    symbols: &[KnowledgeCodeSymbol],
) -> Vec<KnowledgeCodeRelationWriteInput> {
    use std::collections::{BTreeMap, HashMap};

    let file_ids = files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.id))
        .collect::<HashMap<_, _>>();
    let mut symbols_by_file = BTreeMap::<i64, Vec<&KnowledgeCodeSymbol>>::new();
    let mut targets = BTreeMap::<String, Vec<&KnowledgeCodeSymbol>>::new();
    for symbol in symbols {
        symbols_by_file
            .entry(symbol.file_id)
            .or_default()
            .push(symbol);
        targets.entry(symbol.name.clone()).or_default().push(symbol);
    }
    for file_symbols in symbols_by_file.values_mut() {
        file_symbols.sort_by_key(|symbol| symbol.start_line);
    }
    let call_pattern =
        Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("静态调用关系正则必须有效");
    let invoke_pattern =
        Regex::new(r#"\binvoke\s*\(\s*[\"']([^\"']+)[\"']"#).expect("Tauri IPC 关系正则必须有效");
    let event_pattern = Regex::new(r#"\b(?:emit|listen)\s*\(\s*[\"']([^\"']+)"#)
        .expect("Tauri 事件关系正则必须有效");
    let route_pattern = Regex::new(r#"(?i)\b(?:get|post|put|delete|patch)\s*\(\s*[\"'](/[^\"']+)"#)
        .expect("HTTP 路由关系正则必须有效");
    let import_pattern = Regex::new(
        r#"(?im)^\s*(?:use|import)\s+(?:[^;\n]*?\s+from\s+)?[\"']?([A-Za-z_][A-Za-z0-9_./:-]*)"#,
    )
    .expect("导入关系正则必须有效");
    let extends_pattern = Regex::new(
        r"(?im)\b(?:class|interface)\s+[A-Za-z_][A-Za-z0-9_]*\s+extends\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .expect("继承关系正则必须有效");
    let implements_pattern = Regex::new(
        r"(?im)\bclass\s+[A-Za-z_][A-Za-z0-9_]*\s+implements\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .expect("接口实现关系正则必须有效");
    let config_pattern =
        Regex::new(r#"(?im)\b(?:var|env|getItem|setItem|load)\s*\(\s*[\"']([^\"']+)"#)
            .expect("配置键关系正则必须有效");
    let feign_pattern = Regex::new(r#"(?im)@FeignClient\s*\(\s*(?:name\s*=\s*)?[\"']([^\"')]+)"#)
        .expect("Feign 关系正则必须有效");
    let component_pattern =
        Regex::new(r"(?im)@(Service|Mapper)\b").expect("Service Mapper 关系正则必须有效");
    let table_pattern =
        Regex::new(r"(?i)\b(?:from|join|into|update|table)\s+`?([A-Za-z_][A-Za-z0-9_]*)`?")
            .expect("SQL 表关系正则必须有效");
    let ignored_calls = [
        "if", "for", "while", "match", "loop", "fn", "function", "return", "new", "catch", "await",
        "select", "insert", "update", "delete",
    ];
    let file_is_test = files
        .iter()
        .map(|file| (file.id, file.is_test))
        .collect::<HashMap<_, _>>();
    let mut relations = Vec::new();
    for (path, content) in evidence {
        let Some(file_id) = file_ids.get(path.as_str()).copied() else {
            continue;
        };
        let file_symbols = symbols_by_file
            .get(&file_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let caller_at = |offset: usize| {
            let line = code_line_at(content, offset);
            file_symbols
                .iter()
                .copied()
                .filter(|symbol| symbol.start_line <= line)
                .max_by_key(|symbol| symbol.start_line)
                .map(|symbol| (symbol, line))
                .or_else(|| file_symbols.first().map(|symbol| (*symbol, line)))
        };
        for capture in call_pattern.captures_iter(content) {
            let Some(name) = capture.get(1) else {
                continue;
            };
            if ignored_calls.contains(&name.as_str()) {
                continue;
            }
            let Some((caller, line)) = caller_at(name.start()) else {
                continue;
            };
            let Some(target) = resolve_code_symbol_target(
                &targets,
                name.as_str(),
                &["function", "method", "command"],
            ) else {
                continue;
            };
            if target.symbol_key == caller.symbol_key {
                continue;
            }
            relations.push(code_relation(
                caller,
                "calls",
                &target.symbol_key,
                "",
                "",
                file_id,
                line,
                name.as_str(),
                "static_call_name",
                0.75,
            ));
            if file_is_test.get(&file_id).copied().unwrap_or(false) {
                relations.push(code_relation_from_key(
                    &target.symbol_key,
                    "tested_by",
                    &caller.symbol_key,
                    "",
                    "",
                    file_id,
                    line,
                    name.as_str(),
                    "test_static_call_name",
                    0.75,
                ));
            }
        }
        for capture in invoke_pattern.captures_iter(content) {
            let Some(command) = capture.get(1) else {
                continue;
            };
            let Some((caller, line)) = caller_at(command.start()) else {
                continue;
            };
            let target =
                resolve_code_symbol_target(&targets, command.as_str(), &["command", "function"])
                    .map(|symbol| symbol.symbol_key.as_str())
                    .unwrap_or_default();
            relations.push(code_relation(
                caller,
                "tauri_ipc",
                target,
                if target.is_empty() {
                    "tauri_command"
                } else {
                    ""
                },
                if target.is_empty() {
                    command.as_str()
                } else {
                    ""
                },
                file_id,
                line,
                command.as_str(),
                "tauri_invoke_literal",
                if target.is_empty() { 0.65 } else { 0.95 },
            ));
        }
        for capture in event_pattern.captures_iter(content) {
            let Some(event) = capture.get(1) else {
                continue;
            };
            let Some((caller, line)) = caller_at(event.start()) else {
                continue;
            };
            relations.push(code_relation(
                caller,
                "tauri_event",
                "",
                "tauri_event",
                event.as_str(),
                file_id,
                line,
                event.as_str(),
                "tauri_event_literal",
                0.65,
            ));
        }
        for capture in route_pattern.captures_iter(content) {
            let Some(route) = capture.get(1) else {
                continue;
            };
            let Some((caller, line)) = caller_at(route.start()) else {
                continue;
            };
            relations.push(code_relation(
                caller,
                "http_route",
                "",
                "http_route",
                route.as_str(),
                file_id,
                line,
                route.as_str(),
                "http_route_literal",
                0.85,
            ));
        }
        for capture in import_pattern.captures_iter(content) {
            let Some(import_path) = capture.get(1) else {
                continue;
            };
            let Some((caller, line)) = caller_at(import_path.start()) else {
                continue;
            };
            relations.push(code_relation(
                caller,
                "imports",
                "",
                "import_path",
                import_path.as_str(),
                file_id,
                line,
                import_path.as_str(),
                "import_literal",
                0.65,
            ));
        }
        for (pattern, relation_type, preferred_kinds) in [
            (&extends_pattern, "extends", &["class", "interface"][..]),
            (
                &implements_pattern,
                "implements",
                &["interface", "class"][..],
            ),
        ] {
            for capture in pattern.captures_iter(content) {
                let Some(target_name) = capture.get(1) else {
                    continue;
                };
                let Some((caller, line)) = caller_at(target_name.start()) else {
                    continue;
                };
                let target =
                    resolve_code_symbol_target(&targets, target_name.as_str(), preferred_kinds);
                relations.push(code_relation(
                    caller,
                    relation_type,
                    target
                        .map(|symbol| symbol.symbol_key.as_str())
                        .unwrap_or_default(),
                    if target.is_some() { "" } else { "code_type" },
                    if target.is_some() {
                        ""
                    } else {
                        target_name.as_str()
                    },
                    file_id,
                    line,
                    target_name.as_str(),
                    "type_declaration_literal",
                    0.7,
                ));
            }
        }
        for capture in config_pattern.captures_iter(content) {
            let Some(config_key) = capture.get(1) else {
                continue;
            };
            let Some((caller, line)) = caller_at(config_key.start()) else {
                continue;
            };
            relations.push(code_relation(
                caller,
                "config_uses",
                "",
                "config_key",
                config_key.as_str(),
                file_id,
                line,
                config_key.as_str(),
                "config_literal",
                0.7,
            ));
        }
        for capture in feign_pattern.captures_iter(content) {
            let Some(service) = capture.get(1) else {
                continue;
            };
            let Some((caller, line)) = caller_at(service.start()) else {
                continue;
            };
            relations.push(code_relation(
                caller,
                "feign_client",
                "",
                "feign_service",
                service.as_str(),
                file_id,
                line,
                service.as_str(),
                "feign_annotation_literal",
                0.75,
            ));
        }
        for capture in component_pattern.captures_iter(content) {
            let Some(component_type) = capture.get(1) else {
                continue;
            };
            let Some((caller, line)) = caller_at(component_type.start()) else {
                continue;
            };
            let component_role = component_type.as_str().to_ascii_lowercase();
            relations.push(code_relation(
                caller,
                "component_role",
                "",
                "spring_component",
                &component_role,
                file_id,
                line,
                component_type.as_str(),
                "spring_component_annotation",
                0.8,
            ));
        }
        for capture in table_pattern.captures_iter(content) {
            let Some(table) = capture.get(1) else {
                continue;
            };
            let Some((caller, line)) = caller_at(table.start()) else {
                continue;
            };
            relations.push(code_relation(
                caller,
                "sql_table",
                "",
                "sql_table",
                table.as_str(),
                file_id,
                line,
                table.as_str(),
                "sql_table_literal",
                0.85,
            ));
        }
    }
    for symbols_in_file in symbols_by_file.values() {
        let mut current_table: Option<&KnowledgeCodeSymbol> = None;
        for symbol in symbols_in_file {
            if symbol.symbol_kind == "table" {
                current_table = Some(symbol);
            } else if symbol.symbol_kind == "column" {
                if let Some(table) = current_table {
                    relations.push(code_relation_from_key(
                        &table.symbol_key,
                        "contains",
                        &symbol.symbol_key,
                        "",
                        "",
                        symbol.file_id,
                        symbol.start_line,
                        &symbol.name,
                        "sql_table_column_structure",
                        0.8,
                    ));
                }
            }
        }
    }
    relations
}

fn resolve_code_symbol_target<'a>(
    targets: &'a std::collections::BTreeMap<String, Vec<&'a KnowledgeCodeSymbol>>,
    name: &str,
    preferred_kinds: &[&str],
) -> Option<&'a KnowledgeCodeSymbol> {
    let candidates = targets.get(name)?;
    let preferred = candidates
        .iter()
        .copied()
        .filter(|candidate| preferred_kinds.contains(&candidate.symbol_kind.as_str()))
        .collect::<Vec<_>>();
    let effective = if preferred.is_empty() {
        candidates
    } else {
        &preferred
    };
    let distinct = effective
        .iter()
        .map(|candidate| candidate.symbol_key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if distinct.len() == 1 {
        effective.first().copied()
    } else {
        None
    }
}

fn code_relation(
    caller: &KnowledgeCodeSymbol,
    relation_type: &str,
    to_symbol_key: &str,
    to_external_type: &str,
    to_external_key: &str,
    file_id: i64,
    line: i64,
    evidence_text: &str,
    resolver: &str,
    confidence: f64,
) -> KnowledgeCodeRelationWriteInput {
    code_relation_from_key(
        &caller.symbol_key,
        relation_type,
        to_symbol_key,
        to_external_type,
        to_external_key,
        file_id,
        line,
        evidence_text,
        resolver,
        confidence,
    )
}

fn code_relation_from_key(
    from_symbol_key: &str,
    relation_type: &str,
    to_symbol_key: &str,
    to_external_type: &str,
    to_external_key: &str,
    file_id: i64,
    line: i64,
    evidence_text: &str,
    resolver: &str,
    confidence: f64,
) -> KnowledgeCodeRelationWriteInput {
    KnowledgeCodeRelationWriteInput {
        from_symbol_key: from_symbol_key.to_string(),
        relation_type: relation_type.to_string(),
        to_symbol_key: to_symbol_key.to_string(),
        to_external_type: to_external_type.to_string(),
        to_external_key: to_external_key.to_string(),
        evidence_file_id: Some(file_id),
        evidence_start_line: Some(line),
        evidence_end_line: Some(line),
        evidence_text: evidence_text.to_string(),
        resolver: resolver.to_string(),
        confidence,
        // 正则模式只提供待核实的静态线索，绝不在未经 AST 证明或人工确认时作为事实边
        // 参与关系召回或影响分析。
        confirmed: false,
    }
}

fn code_line_at(content: &str, byte_index: usize) -> i64 {
    i64::try_from(
        content[..byte_index]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1,
    )
    .unwrap_or(i64::MAX)
}

fn visibility_from_signature(signature: &str) -> String {
    if signature.trim_start().starts_with("pub ") || signature.contains(" public ") {
        "public".to_string()
    } else if signature.contains(" private ") {
        "private".to_string()
    } else {
        "internal".to_string()
    }
}

fn is_test_code_path(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    normalized.contains("/test/")
        || normalized.contains("/tests/")
        || normalized.ends_with("_test.rs")
        || normalized.ends_with(".test.ts")
        || normalized.ends_with(".spec.ts")
}

fn is_generated_code_path(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase();
    normalized.contains("/generated/")
        || normalized.ends_with(".generated.ts")
        || normalized.ends_with("_generated.rs")
}

fn looks_sensitive_code_path(relative_path: &str) -> bool {
    let file_name = Path::new(relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    file_name == ".env"
        || file_name.starts_with(".env.")
        || matches!(
            file_name.as_str(),
            "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519" | "credentials" | "credentials.json"
        )
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
        || file_name.ends_with(".p12")
        || file_name.ends_with(".pfx")
}

fn is_binary_file(path: &Path) -> Result<bool, AppError> {
    use std::io::Read;

    let mut file = fs::File::open(path)?;
    let mut buffer = [0_u8; 8192];
    let read = file.read(&mut buffer)?;
    Ok(buffer[..read].contains(&0))
}

fn sync_local_source_inner(
    db: &Database,
    source: &KnowledgeSource,
    release_id: Option<i64>,
    runtime: Option<&KnowledgeJobRuntime<'_>>,
) -> Result<KnowledgeSourceSyncResult, AppError> {
    let configured_root = required_text(&source.root_path, "知识源路径")?;
    let root = fs::canonicalize(&configured_root).map_err(|error| {
        AppError::InvalidInput(format!("知识源路径无法访问: {configured_root}: {error}"))
    })?;
    if source.source_type == "single_file" && !root.is_file() {
        return Err(AppError::InvalidInput(
            "单文件知识源必须指向普通文件".to_string(),
        ));
    }
    if source.source_type == "local_directory" && !root.is_dir() {
        return Err(AppError::InvalidInput("目录知识源必须指向目录".to_string()));
    }

    let include_matchers = compile_globs(&source.include_globs, "包含规则")?;
    let mut exclude_patterns = DEFAULT_EXCLUDE_GLOBS
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    exclude_patterns.extend(source.exclude_globs.clone());
    let exclude_matchers = compile_globs(&exclude_patterns, "排除规则")?;
    let collection = collect_local_files(&root, &include_matchers, &exclude_matchers)?;
    let current_paths = collection
        .files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<std::collections::HashSet<_>>();
    let states = db.list_knowledge_document_sync_states(source.id)?;
    let mut seen_document_ids = std::collections::HashSet::new();

    let version_label = if let Some(release_id) = release_id {
        db.get_knowledge_release_by_id(release_id)?
            .map(|release| release.version)
            .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?
    } else {
        "unversioned".to_string()
    };
    let mut result = KnowledgeSourceSyncResult {
        source_id: source.id,
        commit_sha: String::new(),
        scanned_files: 0,
        created_versions: 0,
        unchanged_files: 0,
        deleted_paths: 0,
        skipped_files: collection.skipped,
        warnings: Vec::new(),
    };
    if collection.truncated {
        result.warnings.push(format!(
            "本次同步达到文件上限 {GIT_SYNC_MAX_FILES}，需要缩小 include 范围后重试"
        ));
    }

    let progress_total = i64::try_from(collection.files.len()).unwrap_or(i64::MAX);
    report_knowledge_job_progress(db, runtime, "read_local_files", 0, progress_total, None)?;
    for (index, file) in collection.files.into_iter().enumerate() {
        let progress_current = i64::try_from(index + 1).unwrap_or(i64::MAX);
        result.scanned_files += 1;
        if file.size > GIT_SYNC_MAX_FILE_BYTES {
            result.skipped_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_local_files",
                progress_current,
                progress_total,
                Some(&file.relative_path),
            )?;
            continue;
        }
        let bytes = fs::read(&file.absolute_path)?;
        let Ok(content) = String::from_utf8(bytes) else {
            result.skipped_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_local_files",
                progress_current,
                progress_total,
                Some(&file.relative_path),
            )?;
            continue;
        };
        if content.contains('\0') {
            result.skipped_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_local_files",
                progress_current,
                progress_total,
                Some(&file.relative_path),
            )?;
            continue;
        }
        if let Some(rule) = detect_sensitive_content(&content) {
            if let Some(state) = states
                .iter()
                .find(|state| state.logical_path == file.relative_path)
            {
                db.restrict_knowledge_document(state.id)?;
                // 此路径仍存在，只是被策略阻断；不能在循环结束时当成删除项覆盖掉
                // restricted 元数据，避免丢失可追溯的跳过状态。
                seen_document_ids.insert(state.id);
            }
            result.skipped_files += 1;
            result.warnings.push(format!(
                "文件 {} 因敏感内容规则 {rule} 被阻断，未保存正文",
                file.relative_path
            ));
            report_knowledge_job_progress(
                db,
                runtime,
                "read_local_files",
                progress_current,
                progress_total,
                Some(&file.relative_path),
            )?;
            continue;
        }
        let content_hash = sha256_hex(content.as_bytes());
        let existing_by_path = states
            .iter()
            .find(|state| state.logical_path == file.relative_path);
        let renamed_state = if existing_by_path.is_none() {
            states.iter().find(|state| {
                state.content_hash == content_hash
                    && !current_paths.contains(&state.logical_path)
                    && !seen_document_ids.contains(&state.id)
            })
        } else {
            None
        };
        if let Some(state) = renamed_state {
            db.rename_knowledge_document_path(source.id, &state.logical_path, &file.relative_path)?;
        }
        let existing = existing_by_path.or(renamed_state);
        let document_key = existing.map_or_else(
            || {
                format!(
                    "local:{}:{}",
                    source.source_key,
                    sha256_hex(file.relative_path.as_bytes())
                )
            },
            |state| state.document_key.clone(),
        );
        let document = db.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: existing.map(|state| state.id),
            document_key,
            project_id: source.project_id,
            source_id: Some(source.id),
            doc_type: document_type_for_path(&file.relative_path).to_string(),
            title: Path::new(&file.relative_path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| file.relative_path.clone()),
            logical_path: file.relative_path.clone(),
            sensitivity: "internal".to_string(),
            tags: vec!["local".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        seen_document_ids.insert(document.id);
        if db.knowledge_document_version_exists(
            document.id,
            &version_label,
            &content_hash,
            &file.relative_path,
        )? {
            result.unchanged_files += 1;
            report_knowledge_job_progress(
                db,
                runtime,
                "read_local_files",
                progress_current,
                progress_total,
                Some(&file.relative_path),
            )?;
            continue;
        }
        db.create_knowledge_document_version(
            &crate::models::CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id,
                version_label: version_label.clone(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: file.relative_path.clone(),
                mime_type: document_mime_type_for_path(&file.relative_path).to_string(),
                content,
                content_hash,
                parsed_meta: serde_json::json!({
                    "source": "local_filesystem",
                    "historical": false,
                }),
                token_estimate: 0,
            },
            &[],
        )?;
        result.created_versions += 1;
        report_knowledge_job_progress(
            db,
            runtime,
            "read_local_files",
            progress_current,
            progress_total,
            Some(&document.logical_path),
        )?;
    }

    for state in states {
        if !seen_document_ids.contains(&state.id)
            && state.status != "deleted"
            && db.mark_knowledge_document_path_deleted(source.id, &state.logical_path)?
        {
            result.deleted_paths += 1;
        }
    }
    Ok(result)
}

fn collect_local_files(
    root: &Path,
    include_matchers: &[GlobMatcher],
    exclude_matchers: &[GlobMatcher],
) -> Result<LocalFileCollection, AppError> {
    let canonical_root = fs::canonicalize(root)?;
    if root.is_file() {
        let canonical_path = KnowledgePolicyService::authorize_local_file(root, root)?;
        let metadata = fs::metadata(&canonical_path)?;
        return Ok(LocalFileCollection {
            files: vec![LocalFileEntry {
                absolute_path: canonical_path,
                relative_path: root
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                size: metadata.len(),
            }],
            skipped: 0,
            truncated: false,
        });
    }

    let mut collection = LocalFileCollection::default();
    let mut pending = vec![canonical_root.clone()];
    let mut visited = 0_usize;
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            visited += 1;
            if visited > SOURCE_PREVIEW_MAX_VISITED || collection.files.len() >= GIT_SYNC_MAX_FILES
            {
                collection.truncated = true;
                return Ok(collection);
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            let relative = relative_path(&canonical_root, &path)?;
            if metadata.file_type().is_symlink() {
                collection.skipped += 1;
                continue;
            }
            if metadata.is_dir() {
                if matches_any(exclude_matchers, &format!("{relative}/")) {
                    collection.skipped += 1;
                } else {
                    pending.push(path);
                }
                continue;
            }
            if !metadata.is_file()
                || (!include_matchers.is_empty() && !matches_any(include_matchers, &relative))
                || matches_any(exclude_matchers, &relative)
            {
                collection.skipped += 1;
                continue;
            }
            let canonical_path =
                KnowledgePolicyService::authorize_local_file(&canonical_root, &path)?;
            collection.files.push(LocalFileEntry {
                absolute_path: canonical_path,
                relative_path: relative,
                size: metadata.len(),
            });
        }
    }
    collection
        .files
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(collection)
}

fn parse_git_refs(output: &str, current_branch: &str) -> Vec<KnowledgeGitRef> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\u{1f}');
            let full_name = fields.next()?.trim();
            let commit_sha = fields.next()?.trim();
            let committed_at = fields.next()?.trim();
            let subject = fields.next().unwrap_or_default().trim();
            let (ref_type, name) = if let Some(name) = full_name.strip_prefix("refs/heads/") {
                ("branch", name)
            } else if let Some(name) = full_name.strip_prefix("refs/tags/") {
                ("tag", name)
            } else {
                return None;
            };
            Some(KnowledgeGitRef {
                ref_type: ref_type.to_string(),
                name: name.to_string(),
                commit_sha: commit_sha.to_string(),
                subject: subject.to_string(),
                committed_at: committed_at.to_string(),
                current: ref_type == "branch" && name == current_branch,
            })
        })
        .collect()
}

fn parse_head_ref(output: &str, current_branch: &str) -> Option<KnowledgeGitRef> {
    let mut fields = output.trim().splitn(3, '\u{1f}');
    let commit_sha = fields.next()?.trim();
    let committed_at = fields.next()?.trim();
    let subject = fields.next().unwrap_or_default().trim();
    if commit_sha.is_empty() {
        return None;
    }
    Some(KnowledgeGitRef {
        ref_type: "commit".to_string(),
        name: if current_branch.is_empty() {
            "HEAD".to_string()
        } else {
            format!("HEAD ({current_branch})")
        },
        commit_sha: commit_sha.to_string(),
        subject: subject.to_string(),
        committed_at: committed_at.to_string(),
        current: true,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command as StdCommand;
    use std::time::Duration;

    use super::{
        classify_code_snapshot_changes, code_language_for_path, code_symbol_chunks,
        is_code_language_allowed, is_missing_git_ref_error, normalize_key, normalize_sensitivity,
        normalize_sync_mode, normalize_version_strategy, normalize_zentao_base_url,
        normalized_unique_values, parse_git_refs, parse_head_ref, parse_zentao_scope_items,
        strip_zentao_html, validate_zentao_transport, wait_for_zentao_request_turn,
        zentao_entity_endpoint_candidates, zentao_http_host_is_allowlisted,
        zentao_probe_candidates, zentao_sync_endpoint, KnowledgeService,
    };
    use crate::database::knowledge_domain::documents::{
        NewKnowledgeAsset, NewKnowledgeDocumentParseArtifact,
    };
    use crate::database::Database;
    use crate::models::{
        CaptureKnowledgeDirtyWorktreeSnapshotInput, CaptureKnowledgeGitSnapshotInput,
        CaptureKnowledgeLocalDirectorySnapshotInput, CompareKnowledgeDocumentVersionsInput,
        CreateKnowledgeDocumentVersionInput, GenerateZentaoAiSummaryInput,
        GenerateZentaoKnowledgeDocumentsInput, ImportKnowledgeCommitRelationsInput,
        ImportKnowledgeDocumentRelationsInput, KnowledgeChunkWriteInput, KnowledgeCodeFile,
        KnowledgeListInput, KnowledgeSearchInput, KnowledgeSourceScopePreview, ListAuditLogsInput,
        ListKnowledgeRelationsInput, SyncKnowledgeGitSourceInput, SyncKnowledgeLocalSourceInput,
        UpsertGitWorkspaceInput, UpsertKnowledgeCodeSourceInput, UpsertKnowledgeDocumentInput,
        UpsertKnowledgeProjectInput, UpsertKnowledgeReleaseInput, UpsertKnowledgeSourceInput,
        UpsertZentaoConnectionInput, UpsertZentaoEntityInput, UpsertZentaoProjectMappingInput,
    };
    use crate::services::knowledge_code_analyzer::P0LanguageAnalyzer;

    #[test]
    fn recognizes_git_ref_resolution_failures() {
        assert!(is_missing_git_ref_error("fatal: Needed a single revision"));
        assert!(is_missing_git_ref_error(
            "fatal: ambiguous argument 'missing'"
        ));
        assert!(!is_missing_git_ref_error("fatal: not a git repository"));
    }

    #[test]
    fn normalizes_keys_and_unique_values() {
        assert_eq!(
            normalize_key(" Project_A ", "项目标识").ok().as_deref(),
            Some("project_a")
        );
        assert!(normalize_key("项目 A", "项目标识").is_err());
        assert_eq!(
            normalized_unique_values(vec![" b ".to_string(), "a".to_string(), "a".to_string()]),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn accepts_catalog_source_version_and_sync_strategies() {
        for strategy in [
            "unversioned",
            "manual",
            "git_ref",
            "release_mapping",
            "zentao_mapping",
        ] {
            assert_eq!(
                normalize_version_strategy(strategy).ok().as_deref(),
                Some(strategy)
            );
        }
        assert!(normalize_version_strategy("latest").is_err());

        for mode in ["incremental", "manual", "scheduled", "on_change"] {
            assert_eq!(normalize_sync_mode(mode).ok().as_deref(), Some(mode));
        }
        assert!(normalize_sync_mode("continuous").is_err());
    }

    #[test]
    fn rejects_unknown_sensitivity() {
        assert!(normalize_sensitivity("secret").is_err());
        assert_eq!(
            normalize_sensitivity(" INTERNAL ").ok().as_deref(),
            Some("internal")
        );
    }

    #[test]
    fn knowledge_configuration_and_relation_confirmation_emit_sanitized_audits(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-audit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(&root)?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let project = KnowledgeService::upsert_project(
            &database,
            UpsertKnowledgeProjectInput {
                id: None,
                project_key: "audit-project".to_string(),
                name: "审计项目".to_string(),
                aliases: Vec::new(),
                description: "password=must-not-enter-audit".to_string(),
                git_workspace_keys: Vec::new(),
                git_workspace_key: String::new(),
                default_branch: "main".to_string(),
                enabled: true,
            },
        )?;
        assert!(project.id > 0);
        let events = database.list_audit_logs(&ListAuditLogsInput {
            actor: None,
            source: Some("knowledge".to_string()),
            server_alias: None,
            action: Some("knowledge_project_upsert".to_string()),
            risk: None,
            result: None,
            keyword: None,
            limit: Some(10),
        })?;
        assert_eq!(events.len(), 1);
        assert!(!events[0].detail_json.contains("password"));
        assert!(!events[0].summary.contains("password"));
        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn classifies_added_modified_deleted_and_renamed_code_files() {
        let previous_files = vec![
            KnowledgeCodeFile {
                id: 1,
                snapshot_id: 10,
                document_version_id: Some(101),
                relative_path: "src/changed.rs".to_string(),
                language: "rust".to_string(),
                file_size: 1,
                content_hash: "old".to_string(),
                analysis_level: "structured_fallback".to_string(),
                is_generated: false,
                is_test: false,
                sensitivity: "internal".to_string(),
                status: "active".to_string(),
                skip_reason: String::new(),
                created_at: String::new(),
            },
            KnowledgeCodeFile {
                id: 2,
                snapshot_id: 10,
                document_version_id: Some(102),
                relative_path: "src/old_name.rs".to_string(),
                language: "rust".to_string(),
                file_size: 1,
                content_hash: "same".to_string(),
                analysis_level: "structured_fallback".to_string(),
                is_generated: false,
                is_test: false,
                sensitivity: "internal".to_string(),
                status: "active".to_string(),
                skip_reason: String::new(),
                created_at: String::new(),
            },
            KnowledgeCodeFile {
                id: 3,
                snapshot_id: 10,
                document_version_id: Some(103),
                relative_path: "src/deleted.rs".to_string(),
                language: "rust".to_string(),
                file_size: 1,
                content_hash: "gone".to_string(),
                analysis_level: "structured_fallback".to_string(),
                is_generated: false,
                is_test: false,
                sensitivity: "internal".to_string(),
                status: "active".to_string(),
                skip_reason: String::new(),
                created_at: String::new(),
            },
        ];
        let current = std::collections::HashMap::from([
            ("src/changed.rs".to_string(), "new".to_string()),
            ("src/new_name.rs".to_string(), "same".to_string()),
            ("src/added.rs".to_string(), "added".to_string()),
        ]);
        let changes = classify_code_snapshot_changes(&previous_files, &current);
        assert!(changes.iter().any(|change| {
            change.0 == "modified" && change.1 == "src/changed.rs" && change.2 == "src/changed.rs"
        }));
        assert!(changes.iter().any(|change| {
            change.0 == "renamed" && change.1 == "src/old_name.rs" && change.2 == "src/new_name.rs"
        }));
        assert!(changes
            .iter()
            .any(|change| change.0 == "deleted" && change.1 == "src/deleted.rs"));
        assert!(changes
            .iter()
            .any(|change| change.0 == "added" && change.2 == "src/added.rs"));
    }

    #[test]
    fn zentao_probe_registry_never_uses_a_single_universal_endpoint() {
        let candidates = zentao_probe_candidates("");
        assert!(candidates.len() >= 2);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.name == "zentao-rest-v1"));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.name == "zentao-legacy-module"));
        let preferred = zentao_probe_candidates("zentao-legacy-module");
        assert_eq!(preferred[0].name, "zentao-legacy-module");
        assert_eq!(
            normalize_zentao_base_url("https://zentao.example.test/zentao", false)
                .ok()
                .as_deref(),
            Some("https://zentao.example.test/zentao/")
        );
        assert!(
            normalize_zentao_base_url("https://user:secret@zentao.example.test/", false).is_err()
        );
        assert!(normalize_zentao_base_url("http://zentao.example.test/", false).is_err());
        assert_eq!(
            normalize_zentao_base_url("http://192.162.11.133:9090/zentao", true)
                .ok()
                .as_deref(),
            Some("http://192.162.11.133:9090/zentao/")
        );
        assert!(normalize_zentao_base_url("https://zentao.example.test/", true).is_err());
        assert_eq!(
            validate_zentao_transport("http://zentao.example.test/", false)
                .ok()
                .as_deref(),
            Some("http")
        );
        assert!(validate_zentao_transport("http://zentao.example.test/", true).is_err());
        assert!(validate_zentao_transport("https://zentao.example.test/", false).is_err());
        assert!(zentao_http_host_is_allowlisted(
            "192.162.11.133",
            &["192.162.11.133".to_string()]
        ));
        assert!(!zentao_http_host_is_allowlisted(
            "zentao.example.test",
            &["internal.example.test".to_string()]
        ));
    }

    #[tokio::test]
    async fn zentao_connection_rate_limiter_spaces_requests() {
        let connection_id = -i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after epoch")
                .as_nanos()
                % 1_000_000_000,
        )
        .expect("nanoseconds modulo i64 should fit");
        wait_for_zentao_request_turn(connection_id, 30.0).await;
        let started = std::time::Instant::now();
        wait_for_zentao_request_turn(connection_id, 30.0).await;
        assert!(
            started.elapsed() >= Duration::from_millis(20),
            "同一连接的第二个请求必须等待限流间隔"
        );
    }

    #[test]
    fn zentao_entity_endpoints_are_profile_scoped_and_include_incremental_fact_types() {
        let rest = zentao_entity_endpoint_candidates("zentao-rest-v1");
        let types = rest
            .iter()
            .map(|(entity_type, _)| *entity_type)
            .collect::<Vec<_>>();
        for expected in [
            "stories",
            "story_changes",
            "tasks",
            "worklogs",
            "bugs",
            "test_cases",
            "test_tasks",
            "test_runs",
            "builds",
            "releases",
        ] {
            assert!(types.contains(&expected), "缺少候选实体端点: {expected}");
            assert!(zentao_sync_endpoint("zentao-rest-v1", expected).is_ok());
        }
        assert!(zentao_sync_endpoint("zentao-legacy-module", "worklogs").is_err());
        assert!(zentao_sync_endpoint("unknown", "stories").is_err());
    }

    #[test]
    fn zentao_scope_parser_normalizes_ids_without_persisting_remote_payload() {
        let items = parse_zentao_scope_items(
            "project",
            &serde_json::json!({"data": [
                {"id": 12, "name": "研发迭代", "product": 3, "status": "doing"},
                {"id": "13", "title": "发布准备"}
            ]}),
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].external_id, "12");
        assert_eq!(items[0].parent_external_id, "3");
        assert_eq!(items[1].name, "发布准备");
    }

    #[test]
    fn zentao_html_sanitizer_removes_tags_script_and_style_without_panicking() {
        let text = strip_zentao_html(
            "<p>需求 <b>正文</b></p><script>secret()</script><style>.hidden{}</style>",
        );
        assert_eq!(text, "需求 正文");
    }

    #[test]
    fn document_history_comparison_and_citation_are_consistent(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-document-api-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "refund-requirement".to_string(),
            project_id: None,
            source_id: None,
            doc_type: "requirement".to_string(),
            title: "退款审批需求".to_string(),
            logical_path: "requirements/refund.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: vec!["refund".to_string()],
            allow_ai: true,
            allow_mcp: false,
        })?;
        let first = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v1.0.0".to_string(),
                git_branch: "main".to_string(),
                commit_sha: "commit-v1".to_string(),
                source_path: "requirements/refund.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# 退款审批\n共同说明\n旧实现".to_string(),
                content_hash: "refund-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 8,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "退款审批 > 实现".to_string(),
                content: "旧实现".to_string(),
                content_hash: "refund-v1-chunk".to_string(),
                location: serde_json::json!({"startLine": 3, "endLine": 3}),
                token_estimate: 2,
            }],
        )?;
        let second = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v1.1.0".to_string(),
                git_branch: "main".to_string(),
                commit_sha: "commit-v2".to_string(),
                source_path: "requirements/refund.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# 退款审批\n共同说明\n新实现".to_string(),
                content_hash: "refund-v2".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 8,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "退款审批 > 实现".to_string(),
                content: "新实现".to_string(),
                content_hash: "refund-v2-chunk".to_string(),
                location: serde_json::json!({"startLine": 3, "endLine": 3}),
                token_estimate: 2,
            }],
        )?;
        let first_asset = database.upsert_knowledge_asset(&NewKnowledgeAsset {
            asset_key: "refund-v1-docx".to_string(),
            content_hash: "asset-refund-v1".to_string(),
            storage_key: "sha256/asset-refund-v1".to_string(),
            original_name: "退款审批-v1.docx".to_string(),
            normalized_name: "退款审批-v1.docx".to_string(),
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .to_string(),
            size_bytes: 128,
        })?;
        let second_asset = database.upsert_knowledge_asset(&NewKnowledgeAsset {
            asset_key: "refund-v2-docx".to_string(),
            content_hash: "asset-refund-v2".to_string(),
            storage_key: "sha256/asset-refund-v2".to_string(),
            original_name: "退款审批-v2.docx".to_string(),
            normalized_name: "退款审批-v2.docx".to_string(),
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .to_string(),
            size_bytes: 160,
        })?;
        database.insert_knowledge_document_parse_artifact(&NewKnowledgeDocumentParseArtifact {
            document_version_id: first.id,
            asset_id: Some(first_asset.id),
            parser_id: "docx".to_string(),
            parser_version: "1.0.0".to_string(),
            quality_level: "complete".to_string(),
            warning_json: "[]".to_string(),
            normalized_hash: "normalized-refund-v1".to_string(),
            structure_json: "[]".to_string(),
        })?;
        database.insert_knowledge_document_parse_artifact(&NewKnowledgeDocumentParseArtifact {
            document_version_id: second.id,
            asset_id: Some(second_asset.id),
            parser_id: "docx".to_string(),
            parser_version: "2.0.0".to_string(),
            quality_level: "complete".to_string(),
            warning_json: "[]".to_string(),
            normalized_hash: "normalized-refund-v2".to_string(),
            structure_json: "[]".to_string(),
        })?;

        let detail = super::KnowledgeService::get_document_detail(&database, document.id)?;
        assert_eq!(detail.versions.len(), 2);
        assert_eq!(detail.document.latest_version_id, Some(second.id));

        let comparison = super::KnowledgeService::compare_document_versions(
            &database,
            CompareKnowledgeDocumentVersionsInput {
                from_version_id: first.id,
                to_version_id: second.id,
            },
        )?;
        assert!(!comparison.unchanged);
        assert!(comparison.content_changed);
        assert!(comparison.asset_changed);
        assert!(comparison.parser_changed);
        assert_eq!(comparison.common_prefix_lines, 2);
        assert_eq!(comparison.removed_lines, vec!["旧实现"]);
        assert_eq!(comparison.added_lines, vec!["新实现"]);
        assert_eq!(comparison.from_asset_hashes, vec!["asset-refund-v1"]);
        assert_eq!(comparison.to_asset_hashes, vec!["asset-refund-v2"]);
        assert_eq!(comparison.from_parse_artifacts[0].parser_version, "1.0.0");
        assert_eq!(comparison.to_parse_artifacts[0].parser_version, "2.0.0");

        let third = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "v1.1.1".to_string(),
                git_branch: "main".to_string(),
                commit_sha: "commit-v3".to_string(),
                source_path: "requirements/refund.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# 退款审批\n共同说明\n新实现".to_string(),
                content_hash: "refund-v2".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 8,
            },
            &[],
        )?;
        database.insert_knowledge_document_parse_artifact(&NewKnowledgeDocumentParseArtifact {
            document_version_id: third.id,
            asset_id: Some(second_asset.id),
            parser_id: "docx".to_string(),
            parser_version: "3.0.0".to_string(),
            quality_level: "complete".to_string(),
            warning_json: "[]".to_string(),
            normalized_hash: "normalized-refund-v2".to_string(),
            structure_json: "[]".to_string(),
        })?;
        let parser_only_comparison = super::KnowledgeService::compare_document_versions(
            &database,
            CompareKnowledgeDocumentVersionsInput {
                from_version_id: second.id,
                to_version_id: third.id,
            },
        )?;
        assert!(!parser_only_comparison.content_changed);
        assert!(!parser_only_comparison.asset_changed);
        assert!(parser_only_comparison.parser_changed);
        assert!(!parser_only_comparison.unchanged);

        let chunks = super::KnowledgeService::list_document_chunks(&database, second.id)?;
        let citation = super::KnowledgeService::get_citation_detail(&database, chunks[0].id)?;
        assert_eq!(citation.citation.document_id, Some(document.id));
        assert_eq!(citation.citation.document_version_id, Some(second.id));
        assert_eq!(citation.citation.start_line, Some(3));
        assert_eq!(citation.citation.end_line, Some(3));
        assert_eq!(citation.citation.commit_sha, "commit-v2");

        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn parses_git_refs_without_changing_worktree() {
        let refs = parse_git_refs(
            "refs/heads/main\u{1f}abc123\u{1f}2026-07-30T01:00:00+08:00\u{1f}main commit\n\
             refs/tags/v1.0.0\u{1f}def456\u{1f}2026-07-29T01:00:00+08:00\u{1f}release",
            "main",
        );
        assert_eq!(refs.len(), 2);
        assert!(refs[0].current);
        assert_eq!(refs[1].ref_type, "tag");

        let head = parse_head_ref(
            "abc123\u{1f}2026-07-30T01:00:00+08:00\u{1f}main commit",
            "main",
        );
        assert_eq!(head.map(|value| value.name).as_deref(), Some("HEAD (main)"));
    }

    #[test]
    fn parses_incremental_git_diff_with_renames_and_deletions(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let diff = super::parse_git_diff_paths(
            b"M\0src/lib.rs\0D\0old.sql\0R100\0old.ts\0new.ts\0A\0README.md\0",
        )?;
        assert!(diff.current_paths.contains("src/lib.rs"));
        assert!(diff.current_paths.contains("new.ts"));
        assert!(diff.current_paths.contains("README.md"));
        assert_eq!(diff.deleted_paths, vec!["old.sql"]);
        assert_eq!(
            diff.renamed_paths,
            vec![("old.ts".to_string(), "new.ts".to_string())]
        );
        Ok(())
    }

    #[test]
    fn source_scope_preview_helpers_keep_paths_bounded() -> Result<(), Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("tauri-ssh-knowledge-scope-{}", std::process::id()));
        fs::create_dir_all(root.join("src"))?;
        fs::write(root.join("src/lib.rs"), "fn main() {}")?;
        let root = fs::canonicalize(root)?;
        let mut preview = KnowledgeSourceScopePreview {
            source_type: "local_directory".to_string(),
            canonical_root: root.to_string_lossy().to_string(),
            include_globs: vec!["**/*.rs".to_string()],
            exclude_globs: Vec::new(),
            allow_remote_embedding: false,
            included_files: 0,
            skipped_entries: 0,
            included_bytes: 0,
            truncated: false,
            warnings: Vec::new(),
            entries: Vec::new(),
        };
        let includes = super::compile_globs(&preview.include_globs, "包含规则")?;
        super::preview_path(
            &root.join("src/lib.rs"),
            &root,
            &includes,
            &[],
            &mut preview,
        )?;
        assert_eq!(preview.included_files, 1);
        assert_eq!(preview.entries[0].relative_path, "src/lib.rs");
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn generates_idempotent_zentao_fact_documents_through_common_index_pipeline(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-zentao-facts-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "zentao-facts".to_string(),
            name: "禅道事实项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let connection = database.upsert_zentao_connection(&UpsertZentaoConnectionInput {
            id: None,
            connection_key: "zentao-facts".to_string(),
            name: "禅道测试连接".to_string(),
            base_url: "https://zentao.example.test/".to_string(),
            api_version: "v1".to_string(),
            auth_mode: "token".to_string(),
            endpoint_profile: "zentao-rest-v1".to_string(),
            credential_key: "credential-reference-only".to_string(),
            tls_verify: true,
            allow_insecure_http: false,
            request_timeout_seconds: 15,
            page_size: 20,
            rate_limit_per_second: 2.0,
            enabled: true,
        })?;
        let mapping = database.upsert_zentao_project_mapping(&UpsertZentaoProjectMappingInput {
            id: None,
            connection_id: connection.id,
            knowledge_project_id: project.id,
            remote_product_id: "product-1".to_string(),
            remote_project_id: "project-1".to_string(),
            remote_execution_ids: vec!["execution-1".to_string()],
            release_mapping: serde_json::json!({}),
            sync_scope: serde_json::json!({}),
            sync_since: None,
            include_comments: false,
            include_worklogs: true,
            include_attachment_metadata: false,
            allow_remote_embedding: false,
            allow_remote_ai: false,
            enabled: true,
        })?;
        let second_project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "zentao-facts-secondary".to_string(),
            name: "禅道事实项目二".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        assert!(database
            .upsert_zentao_project_mapping(&UpsertZentaoProjectMappingInput {
                id: None,
                connection_id: connection.id,
                knowledge_project_id: second_project.id,
                remote_product_id: "product-1".to_string(),
                remote_project_id: "project-1".to_string(),
                remote_execution_ids: Vec::new(),
                release_mapping: serde_json::json!({}),
                sync_scope: serde_json::json!({}),
                sync_since: None,
                include_comments: false,
                include_worklogs: false,
                include_attachment_metadata: false,
                allow_remote_embedding: false,
                allow_remote_ai: false,
                enabled: true,
            })
            .is_err());
        let normalized_task = super::normalize_zentao_entity(
            &mapping,
            &connection,
            "tasks",
            &serde_json::json!({"id": "10", "title": "订单审批任务", "story": "101"}),
        )?;
        assert_eq!(
            normalized_task.parent_external_key,
            format!("zentao:{}:stories:101", connection.id)
        );
        for (entity_type, external_id, parent_key, status) in [
            ("stories", "101", "", "active"),
            ("tasks", "201", "S101", "done"),
            ("tests", "301", "S101", "passed"),
        ] {
            database.upsert_zentao_entity(&UpsertZentaoEntityInput {
                connection_id: connection.id,
                mapping_id: mapping.id,
                knowledge_project_id: project.id,
                release_id: None,
                entity_type: entity_type.to_string(),
                external_id: external_id.to_string(),
                external_key: format!("{}{}", &entity_type[..1].to_ascii_uppercase(), external_id),
                title: format!("{entity_type} 订单审批"),
                body_markdown: "订单审批事实正文".to_string(),
                original_status: status.to_string(),
                normalized_status: status.to_string(),
                assignee_external_id: "tester".to_string(),
                parent_external_key: parent_key.to_string(),
                remote_url: format!("https://zentao.example.test/{entity_type}/{external_id}"),
                content_hash: format!("{entity_type}-{external_id}-hash"),
                raw_json_hash: format!("{entity_type}-{external_id}-raw-hash"),
                raw_snapshot: None,
                source_created_at: Some("2026-07-31T00:00:00Z".to_string()),
                source_updated_at: Some("2026-07-31T00:00:00Z".to_string()),
            })?;
        }
        let entity_relations =
            KnowledgeService::rebuild_zentao_entity_relations(&database, mapping.id)?;
        assert_eq!(entity_relations.len(), 2);
        assert!(entity_relations.iter().any(|relation| {
            relation.from_external_key == "S101"
                && relation.to_external_key == "T201"
                && relation.relation_type == "decomposed_to"
                && relation.confirmed
        }));

        let first = KnowledgeService::generate_zentao_fact_documents(
            &database,
            GenerateZentaoKnowledgeDocumentsInput {
                mapping_id: mapping.id,
            },
        )?;
        assert_eq!(first.generated_document_version_ids.len(), 7);
        let documents = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project.id),
            release_id: None,
            source_id: Some(first.source_id),
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(documents.total, 7);
        let traceability = documents
            .items
            .iter()
            .find(|document| document.logical_path.ends_with("traceability.md"))
            .ok_or("missing traceability document")?;
        let versions = database.list_knowledge_document_versions(traceability.id)?;
        assert_eq!(versions.len(), 1);
        assert!(versions[0].content.contains("不能推断"));
        assert!(!database.list_knowledge_chunks(versions[0].id)?.is_empty());

        let second = KnowledgeService::generate_zentao_fact_documents(
            &database,
            GenerateZentaoKnowledgeDocumentsInput {
                mapping_id: mapping.id,
            },
        )?;
        assert!(second.generated_document_version_ids.is_empty());
        assert_eq!(
            database
                .list_knowledge_document_versions(traceability.id)?
                .len(),
            1
        );
        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn zentao_ai_summary_requires_explicit_remote_ai_authorization(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tauri-ssh-zentao-ai-summary-{unique}"));
        fs::create_dir_all(&root)?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "zentao-ai-summary".to_string(),
            name: "禅道 AI 摘要项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let connection = database.upsert_zentao_connection(&UpsertZentaoConnectionInput {
            id: None,
            connection_key: "zentao-ai-summary".to_string(),
            name: "禅道摘要测试连接".to_string(),
            base_url: "https://zentao.example.test/".to_string(),
            api_version: "v1".to_string(),
            auth_mode: "token".to_string(),
            endpoint_profile: "zentao-rest-v1".to_string(),
            credential_key: "credential-reference-only".to_string(),
            tls_verify: true,
            allow_insecure_http: false,
            request_timeout_seconds: 15,
            page_size: 20,
            rate_limit_per_second: 2.0,
            enabled: true,
        })?;
        let mapping = database.upsert_zentao_project_mapping(&UpsertZentaoProjectMappingInput {
            id: None,
            connection_id: connection.id,
            knowledge_project_id: project.id,
            remote_product_id: "product-1".to_string(),
            remote_project_id: "project-1".to_string(),
            remote_execution_ids: Vec::new(),
            release_mapping: serde_json::json!({}),
            sync_scope: serde_json::json!({}),
            sync_since: None,
            include_comments: false,
            include_worklogs: false,
            include_attachment_metadata: false,
            allow_remote_embedding: false,
            allow_remote_ai: false,
            enabled: true,
        })?;

        let error = KnowledgeService::generate_zentao_ai_summary(
            &database,
            GenerateZentaoAiSummaryInput {
                mapping_id: mapping.id,
                provider_key: "provider-key".to_string(),
                model: "model".to_string(),
                prompt: "归纳当前风险".to_string(),
            },
        )
        .await
        .expect_err("未授权映射不得触发远程 AI");
        assert!(error.to_string().contains("未显式允许远程 AI"));

        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn git_source_sync_reads_objects_without_changing_worktree(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-git-sync-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("repo/docs"))?;
        run_git(&root.join("repo"), &["init"])?;
        run_git(
            &root.join("repo"),
            &["config", "user.email", "test@example.com"],
        )?;
        run_git(
            &root.join("repo"),
            &["config", "user.name", "Knowledge Test"],
        )?;
        fs::write(root.join("repo/docs/requirement.md"), "# v1\n退款审批")?;
        run_git(&root.join("repo"), &["add", "docs/requirement.md"])?;
        run_git(&root.join("repo"), &["commit", "-m", "add requirement"])?;

        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        database.upsert_git_workspace(
            &UpsertGitWorkspaceInput {
                id: None,
                workspace_key: "workspace-a".to_string(),
                name: "Workspace A".to_string(),
                repo_path: root.join("repo").to_string_lossy().to_string(),
                credential_key: None,
                description: None,
            },
            "master",
            "",
            "clean",
            0,
            0,
            0,
        )?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "project-a".to_string(),
            name: "项目 A".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: vec!["workspace-a".to_string()],
            git_workspace_key: "workspace-a".to_string(),
            default_branch: "master".to_string(),
            enabled: true,
        })?;
        let source = database.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            id: None,
            source_key: "source-a".to_string(),
            project_id: Some(project.id),
            source_type: "git_workspace".to_string(),
            display_name: "Git 文档".to_string(),
            root_path: String::new(),
            git_workspace_key: "workspace-a".to_string(),
            include_globs: vec!["docs/**".to_string()],
            exclude_globs: Vec::new(),
            version_strategy: "git_ref".to_string(),
            sync_mode: "manual".to_string(),
            allow_remote_embedding: false,
            enabled: true,
        })?;

        let release_a = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "master".to_string(),
            commit_sha: String::new(),
            description: String::new(),
            released_at: None,
        })?;
        let release_b = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.1".to_string(),
            tag_name: "v1.0.1".to_string(),
            branch: "master".to_string(),
            commit_sha: String::new(),
            description: String::new(),
            released_at: None,
        })?;

        let before_status = run_git(&root.join("repo"), &["status", "--porcelain"])?;
        let first = super::KnowledgeService::sync_git_source(
            &database,
            SyncKnowledgeGitSourceInput {
                source_id: source.id,
                release_id: Some(release_a.id),
                git_ref: "HEAD".to_string(),
            },
        )
        .await?;
        assert_eq!(first.created_versions, 1);
        let first_document = database
            .get_knowledge_document_by_source_path(source.id, "docs/requirement.md")?
            .ok_or("首次同步应创建 requirement.md 文档")?;
        let first_version = database
            .list_knowledge_document_versions(first_document.id)?
            .into_iter()
            .find(|version| version.release_id == Some(release_a.id))
            .ok_or("首次同步应创建 v1.0.0 文档版本")?;
        KnowledgeService::parse_and_index_document_version(&database, first_version.id, None)?;
        let first_chunk = database
            .list_knowledge_chunks(first_version.id)?
            .into_iter()
            .next()
            .ok_or("首次同步应生成可引用片段")?;
        assert_eq!(
            run_git(&root.join("repo"), &["status", "--porcelain"])?,
            before_status
        );

        // 同一 Commit 绑定到新的项目版本时，必须重新读取 Git 对象并创建新的
        // document_version；只比较 source.last_commit_sha 会把新版本漏掉。
        let same_commit_other_release = super::KnowledgeService::sync_git_source(
            &database,
            SyncKnowledgeGitSourceInput {
                source_id: source.id,
                release_id: Some(release_b.id),
                git_ref: "HEAD".to_string(),
            },
        )
        .await?;
        assert_eq!(same_commit_other_release.created_versions, 1);

        // 不同 Commit 绑定同一个发布版本时，未变化的旧文件和新增文件都必须进入
        // 当前版本；只读取 Diff 会漏掉未变化的 requirement.md。
        fs::write(
            root.join("repo/docs/plan.md"),
            "# v1.0.1\n明日工作计划生成规则",
        )?;
        run_git(&root.join("repo"), &["add", "docs/plan.md"])?;
        run_git(&root.join("repo"), &["commit", "-m", "add plan"])?;
        let different_commit_same_release = super::KnowledgeService::sync_git_source(
            &database,
            SyncKnowledgeGitSourceInput {
                source_id: source.id,
                release_id: Some(release_b.id),
                git_ref: "HEAD".to_string(),
            },
        )
        .await?;
        assert_eq!(different_commit_same_release.created_versions, 1);
        assert_eq!(different_commit_same_release.unchanged_files, 1);

        fs::rename(
            root.join("repo/docs/requirement.md"),
            root.join("repo/docs/refund.md"),
        )?;
        run_git(&root.join("repo"), &["add", "-A"])?;
        run_git(&root.join("repo"), &["commit", "-m", "rename requirement"])?;
        let second = super::KnowledgeService::sync_git_source(
            &database,
            SyncKnowledgeGitSourceInput {
                source_id: source.id,
                release_id: Some(release_b.id),
                git_ref: "HEAD".to_string(),
            },
        )
        .await?;
        // 同一发布版本中仅发生路径重命名时，内容哈希相同但 source_path 不同，
        // 仍需创建不可变新版本；否则当前版本引用会继续显示旧路径。
        assert_eq!(second.created_versions, 1);
        assert_eq!(second.unchanged_files, 1);
        assert_eq!(second.deleted_paths, 0);
        let documents = database.list_knowledge_documents(&crate::models::KnowledgeListInput {
            project_id: Some(project.id),
            release_id: None,
            source_id: Some(source.id),
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(documents.total, 2);
        assert!(documents
            .items
            .iter()
            .any(|document| document.logical_path == "docs/refund.md"));
        assert!(documents
            .items
            .iter()
            .any(|document| document.logical_path == "docs/plan.md"));
        let refund_document = documents
            .items
            .iter()
            .find(|document| document.logical_path == "docs/refund.md")
            .expect("重命名后的文档应复用原文档");
        assert_eq!(
            database
                .list_knowledge_document_versions(refund_document.id)?
                .len(),
            3
        );
        let plan_document = documents
            .items
            .iter()
            .find(|document| document.logical_path == "docs/plan.md")
            .expect("新增文档应保留在发布版本");
        assert_eq!(
            database
                .list_knowledge_document_versions(plan_document.id)?
                .len(),
            1
        );
        let historical_citation = KnowledgeService::get_citation_detail(&database, first_chunk.id)?;
        assert_eq!(
            historical_citation.citation.logical_path,
            "docs/requirement.md"
        );
        let historical_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "退款审批".to_string(),
            project_ids: vec![project.id],
            release_ids: vec![release_a.id],
            source_ids: vec![source.id],
            document_types: Vec::new(),
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert!(historical_hits
            .iter()
            .any(|hit| hit.citation.logical_path == "docs/requirement.md"));
        let current_release_hits = database.search_knowledge_fts(&KnowledgeSearchInput {
            query: "退款审批".to_string(),
            project_ids: vec![project.id],
            release_ids: vec![release_b.id],
            source_ids: vec![source.id],
            document_types: Vec::new(),
            sensitivities: vec!["internal".to_string()],
            snapshot_id: None,
            limit: Some(10),
            include_context: Some(true),
        })?;
        assert!(current_release_hits
            .iter()
            .any(|hit| hit.citation.logical_path == "docs/refund.md"));

        // 重命名后的新路径如果在读取正文后命中敏感规则，必须回查旧路径并清理
        // 旧文档的正文、分块和索引；否则旧路径会继续成为可检索的敏感证据。
        fs::write(
            root.join("repo/docs/credentials.md"),
            "# 凭据说明\n仅用于敏感重命名回归测试\n保留重命名上下文\n本文件不包含真实凭据",
        )?;
        run_git(&root.join("repo"), &["add", "docs/credentials.md"])?;
        run_git(&root.join("repo"), &["commit", "-m", "add credentials doc"])?;
        let seeded_sensitive_rename = super::KnowledgeService::sync_git_source(
            &database,
            SyncKnowledgeGitSourceInput {
                source_id: source.id,
                release_id: Some(release_b.id),
                git_ref: "HEAD".to_string(),
            },
        )
        .await?;
        assert_eq!(seeded_sensitive_rename.created_versions, 1);
        let credentials_document = database
            .get_knowledge_document_by_source_path(source.id, "docs/credentials.md")?
            .ok_or("敏感重命名测试应先创建旧路径文档")?;
        let credentials_version = database
            .list_knowledge_document_versions(credentials_document.id)?
            .into_iter()
            .find(|version| version.release_id == Some(release_b.id))
            .ok_or("敏感重命名测试应创建发布版本")?;
        KnowledgeService::parse_and_index_document_version(
            &database,
            credentials_version.id,
            None,
        )?;
        assert!(!database
            .list_knowledge_chunks(credentials_version.id)?
            .is_empty());

        fs::rename(
            root.join("repo/docs/credentials.md"),
            root.join("repo/docs/secrets.md"),
        )?;
        fs::write(
            root.join("repo/docs/secrets.md"),
            "# 凭据说明\n仅用于敏感重命名回归测试\n保留重命名上下文\npassword=must-not-be-indexed",
        )?;
        run_git(&root.join("repo"), &["add", "-A"])?;
        run_git(
            &root.join("repo"),
            &["commit", "-m", "rename credentials to secret"],
        )?;
        let blocked_sensitive_rename = super::KnowledgeService::sync_git_source(
            &database,
            SyncKnowledgeGitSourceInput {
                source_id: source.id,
                release_id: Some(release_b.id),
                git_ref: "HEAD".to_string(),
            },
        )
        .await?;
        assert_eq!(blocked_sensitive_rename.skipped_files, 1);
        assert!(database
            .get_knowledge_document_by_source_path(source.id, "docs/secrets.md")?
            .is_none());
        let restricted_credentials = database
            .get_knowledge_document_by_source_path(source.id, "docs/credentials.md")?
            .ok_or("敏感重命名后旧路径文档必须保留审计元数据")?;
        assert_eq!(restricted_credentials.sensitivity, "restricted");
        assert!(!restricted_credentials.allow_ai);
        let restricted_version = database
            .list_knowledge_document_versions(restricted_credentials.id)?
            .into_iter()
            .find(|version| version.id == credentials_version.id)
            .ok_or("敏感重命名后旧版本应保留哈希审计记录")?;
        assert!(!restricted_version.valid);
        assert!(restricted_version.content.is_empty());
        assert!(database
            .list_knowledge_chunks(restricted_version.id)?
            .is_empty());

        // 旧同步游标即使是合法十六进制字符串，也可能因仓库重写或 GC 已不可达。
        // 这时完整版本树仍应可导入，并明确向调用方报告重命名识别被跳过。
        database.update_knowledge_source_sync_state(source.id, &"a".repeat(40), "success", None)?;
        let stale_cursor = super::KnowledgeService::sync_git_source(
            &database,
            SyncKnowledgeGitSourceInput {
                source_id: source.id,
                release_id: Some(release_b.id),
                git_ref: "HEAD".to_string(),
            },
        )
        .await?;
        assert_eq!(stale_cursor.created_versions, 0);
        assert_eq!(stale_cursor.unchanged_files, 2);
        assert!(stale_cursor
            .warnings
            .iter()
            .any(|warning| warning.contains("历史 Git Commit 不可达")));
        assert!(run_git(&root.join("repo"), &["status", "--porcelain"])?.is_empty());

        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn captures_git_commit_snapshot_without_changing_worktree(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-code-snapshot-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("repo/src"))?;
        fs::create_dir_all(root.join("repo/.codex/docs/brainstorm"))?;
        run_git(&root.join("repo"), &["init"])?;
        run_git(
            &root.join("repo"),
            &["config", "user.email", "test@example.com"],
        )?;
        run_git(
            &root.join("repo"),
            &["config", "user.name", "Knowledge Test"],
        )?;
        fs::write(root.join("repo/src/lib.rs"), "pub fn snapshot() {}")?;
        // Git 默认会转义中文路径。该文件必须仍能先被排除规则识别，
        // 而不是将转义后的文本拼入 `commit:path` 后再读取失败。
        fs::write(
            root.join("repo/.codex/docs/brainstorm/统计指标.md"),
            "# 不应纳入源码分析\n",
        )?;
        run_git(
            &root.join("repo"),
            &["add", "src/lib.rs", ".codex/docs/brainstorm/统计指标.md"],
        )?;
        run_git(&root.join("repo"), &["commit", "-m", "capture source"])?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "code-snapshot-project".to_string(),
            name: "代码快照项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: vec!["workspace-snapshot".to_string()],
            git_workspace_key: "workspace-snapshot".to_string(),
            default_branch: "master".to_string(),
            enabled: true,
        })?;
        let release_a = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: "v1.0.0".to_string(),
            branch: "master".to_string(),
            commit_sha: String::new(),
            description: String::new(),
            released_at: None,
        })?;
        let release_b = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.1".to_string(),
            tag_name: "v1.0.1".to_string(),
            branch: "master".to_string(),
            commit_sha: String::new(),
            description: String::new(),
            released_at: None,
        })?;
        database.upsert_git_workspace(
            &UpsertGitWorkspaceInput {
                id: None,
                workspace_key: "workspace-snapshot".to_string(),
                name: "Workspace Snapshot".to_string(),
                repo_path: root.join("repo").to_string_lossy().to_string(),
                credential_key: None,
                description: None,
            },
            "master",
            "",
            "clean",
            0,
            0,
            0,
        )?;
        let source = database.upsert_knowledge_code_source(&UpsertKnowledgeCodeSourceInput {
            source: UpsertKnowledgeSourceInput {
                id: None,
                source_key: "code-snapshot".to_string(),
                project_id: Some(project.id),
                source_type: "git_workspace".to_string(),
                display_name: "Git 源码".to_string(),
                root_path: String::new(),
                git_workspace_key: "workspace-snapshot".to_string(),
                include_globs: vec!["**/*".to_string()],
                exclude_globs: vec![".codex/**".to_string()],
                version_strategy: "git_ref".to_string(),
                sync_mode: "manual".to_string(),
                allow_remote_embedding: false,
                enabled: true,
            },
            include_untracked: true,
            max_file_size_bytes: 1024 * 1024,
            allowed_languages: vec!["rust".to_string()],
            allow_remote_processing: false,
        })?;
        let before_status = run_git(&root.join("repo"), &["status", "--porcelain"])?;
        let missing_ref_error = KnowledgeService::capture_git_snapshot(
            &database,
            CaptureKnowledgeGitSnapshotInput {
                source_id: source.source.id,
                git_ref: "feature/missing-branch".to_string(),
                release_id: None,
            },
        )
        .await
        .expect_err("不存在的 Git 引用应返回可操作的错误");
        assert!(missing_ref_error
            .to_string()
            .contains("Git 引用不存在于工作区"));
        let snapshot = KnowledgeService::capture_git_snapshot(
            &database,
            CaptureKnowledgeGitSnapshotInput {
                source_id: source.source.id,
                git_ref: "HEAD".to_string(),
                release_id: Some(release_a.id),
            },
        )
        .await?;
        assert_eq!(snapshot.snapshot_type, "git_commit");
        assert_eq!(snapshot.file_count, 2);
        let same_commit_other_release = KnowledgeService::capture_git_snapshot(
            &database,
            CaptureKnowledgeGitSnapshotInput {
                source_id: source.source.id,
                git_ref: "HEAD".to_string(),
                release_id: Some(release_b.id),
            },
        )
        .await?;
        assert_ne!(snapshot.id, same_commit_other_release.id);
        assert_eq!(same_commit_other_release.release_id, Some(release_b.id));
        let analysis = KnowledgeService::analyze_code_snapshot(&database, snapshot.id).await?;
        assert_eq!(analysis.snapshot.status, "analyzed");
        assert_eq!(analysis.analyzed_files, 1);
        assert_eq!(
            database
                .list_knowledge_code_symbols(snapshot.id, Some("snapshot"))?
                .len(),
            1
        );
        let snapshot_evidence = KnowledgeService::list_relations(
            &database,
            ListKnowledgeRelationsInput {
                entity_type: Some("code_snapshot".to_string()),
                entity_key: Some(snapshot.id.to_string()),
                project_ids: Vec::new(),
                release_ids: Vec::new(),
                sensitivities: Vec::new(),
                confirmed_only: Some(true),
                limit: Some(20),
            },
        )?;
        assert!(snapshot_evidence.iter().any(|relation| {
            relation.relation_type == "captured_from"
                && relation.to_type == "git_commit"
                && relation.to_key == snapshot.commit_sha
                && relation.confirmed
        }));
        assert!(snapshot_evidence.iter().any(|relation| {
            relation.relation_type == "contains" && relation.to_type == "code_symbol"
        }));
        assert_eq!(
            run_git(&root.join("repo"), &["status", "--porcelain"])?,
            before_status
        );
        fs::write(root.join("repo/src/lib.rs"), "pub fn snapshot() {}")?;
        fs::rename(
            root.join("repo/src/lib.rs"),
            root.join("repo/src/renamed.rs"),
        )?;
        run_git(&root.join("repo"), &["add", "-A"])?;
        run_git(
            &root.join("repo"),
            &["commit", "-m", "rename and change code"],
        )?;
        let next_snapshot = KnowledgeService::capture_git_snapshot(
            &database,
            CaptureKnowledgeGitSnapshotInput {
                source_id: source.source.id,
                git_ref: "HEAD".to_string(),
                release_id: None,
            },
        )
        .await?;
        let next_analysis =
            KnowledgeService::analyze_code_snapshot(&database, next_snapshot.id).await?;
        assert!(next_analysis
            .warnings
            .iter()
            .any(|warning| warning.starts_with("Git 对象 Diff：")));
        let git_comparison = KnowledgeService::compare_code_snapshots(
            &database,
            crate::models::CompareKnowledgeCodeSnapshotsInput {
                from_snapshot_id: snapshot.id,
                to_snapshot_id: next_snapshot.id,
            },
        )?;
        assert!(git_comparison.file_changes.iter().any(|change| {
            change.change_type == "renamed"
                && change.from_path == "src/lib.rs"
                && change.to_path == "src/renamed.rs"
        }));
        fs::write(
            root.join("repo/src/renamed.rs"),
            "pub fn snapshot() { /* dirty */ }",
        )?;
        fs::write(root.join("repo/src/staged.rs"), "pub fn staged() {}")?;
        run_git(&root.join("repo"), &["add", "src/staged.rs"])?;
        fs::write(root.join("repo/src/untracked.rs"), "pub fn untracked() {}")?;
        let dirty_status = run_git(&root.join("repo"), &["status", "--porcelain"])?;
        let dirty_snapshot = KnowledgeService::capture_dirty_worktree_snapshot(
            &database,
            CaptureKnowledgeDirtyWorktreeSnapshotInput {
                source_id: source.source.id,
                release_id: None,
            },
        )
        .await?;
        assert_eq!(dirty_snapshot.snapshot_type, "git_worktree");
        assert!(dirty_snapshot.worktree_dirty);
        assert!(dirty_snapshot.commit_sha.is_empty());
        assert!(!dirty_snapshot.base_commit_sha.is_empty());
        assert_eq!(
            dirty_snapshot.dirty_state["semantics"],
            "local_worktree_observation_not_release_fact"
        );
        assert_eq!(
            dirty_snapshot.dirty_state["changes"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
        assert_eq!(
            dirty_snapshot.dirty_state["files"].as_array().map(Vec::len),
            Some(3)
        );
        assert_eq!(
            run_git(&root.join("repo"), &["status", "--porcelain"])?,
            dirty_status
        );
        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn captures_authorized_local_directory_without_historical_claims(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-local-code-snapshot-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("source/src"))?;
        fs::write(root.join("source/src/lib.rs"), "pub fn local_snapshot() {}")?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let source = database.upsert_knowledge_code_source(&UpsertKnowledgeCodeSourceInput {
            source: UpsertKnowledgeSourceInput {
                id: None,
                source_key: "local-code-snapshot".to_string(),
                project_id: None,
                source_type: "local_directory".to_string(),
                display_name: "本地源码快照".to_string(),
                root_path: root.join("source").to_string_lossy().to_string(),
                git_workspace_key: String::new(),
                include_globs: vec!["src/**".to_string()],
                exclude_globs: Vec::new(),
                version_strategy: "unversioned".to_string(),
                sync_mode: "manual".to_string(),
                allow_remote_embedding: false,
                enabled: true,
            },
            include_untracked: false,
            max_file_size_bytes: 1024 * 1024,
            allowed_languages: vec!["rust".to_string()],
            allow_remote_processing: false,
        })?;
        let snapshot = KnowledgeService::capture_local_directory_snapshot(
            &database,
            CaptureKnowledgeLocalDirectorySnapshotInput {
                source_id: source.source.id,
                release_id: None,
            },
        )?;
        assert_eq!(snapshot.snapshot_type, "local_directory");
        assert!(snapshot.commit_sha.is_empty());
        assert!(snapshot.base_commit_sha.is_empty());
        assert!(!snapshot.worktree_dirty);
        assert_eq!(snapshot.file_count, 1);
        assert_eq!(
            snapshot.dirty_state["semantics"],
            "non_historical_local_directory"
        );
        assert!(KnowledgeService::capture_local_directory_snapshot(
            &database,
            CaptureKnowledgeLocalDirectorySnapshotInput {
                source_id: source.source.id,
                release_id: Some(1),
            },
        )
        .is_err());
        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn analyzes_local_code_snapshot_into_symbols_and_exact_chunks(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-code-analysis-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(root.join("source/src"))?;
        fs::write(
            root.join("source/src/lib.rs"),
            "pub struct OrderService {}\n\npub fn submit_order() {\n  let value = 1;\n}\n",
        )?;
        fs::write(
            root.join("source/src/caller.rs"),
            "pub fn dispatch_order() {\n  submit_order();\n  invoke(\"submit_order\");\n  emit(\"order-updated\");\n  let _ = std::env::var(\"ORDER_MODE\");\n}\n",
        )?;
        fs::write(
            root.join("source/src/secret.rs"),
            "const KEY: &str = \"-----BEGIN PRIVATE KEY-----secret\";",
        )?;
        fs::write(
            root.join("source/src/redacted.rs"),
            "// password: correct horse battery staple # local only\n// blCancelToken: true\n// token: response.data\npub fn connect() {}\n",
        )?;
        fs::write(
            root.join("source/src/README.md"),
            "# 订单源码说明\n\n这里说明 submit_order 的业务约束。",
        )?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let source = database.upsert_knowledge_code_source(&UpsertKnowledgeCodeSourceInput {
            source: UpsertKnowledgeSourceInput {
                id: None,
                source_key: "code-analysis".to_string(),
                project_id: None,
                source_type: "local_directory".to_string(),
                display_name: "源码分析".to_string(),
                root_path: root.join("source").to_string_lossy().to_string(),
                git_workspace_key: String::new(),
                include_globs: vec!["src/**".to_string()],
                exclude_globs: Vec::new(),
                version_strategy: "unversioned".to_string(),
                sync_mode: "manual".to_string(),
                allow_remote_embedding: false,
                enabled: true,
            },
            include_untracked: false,
            max_file_size_bytes: 1024 * 1024,
            allowed_languages: vec!["rust".to_string()],
            allow_remote_processing: false,
        })?;
        let snapshot = KnowledgeService::capture_local_directory_snapshot(
            &database,
            CaptureKnowledgeLocalDirectorySnapshotInput {
                source_id: source.source.id,
                release_id: None,
            },
        )?;
        let result = KnowledgeService::analyze_code_snapshot(&database, snapshot.id).await?;
        assert_eq!(result.analyzed_files, 4);
        assert_eq!(result.skipped_files, 1);
        assert!(result.symbols >= 3);
        assert_eq!(result.documents, 14);
        let generated_reports = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: None,
            release_id: None,
            source_id: Some(source.source.id),
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(generated_reports.total, 14);
        let markdown_document = generated_reports
            .items
            .iter()
            .find(|document| document.logical_path == "src/README.md")
            .ok_or("missing markdown source document")?;
        assert_eq!(markdown_document.doc_type, "markdown");
        let markdown_version = database
            .list_knowledge_document_versions(markdown_document.id)?
            .into_iter()
            .next()
            .ok_or("missing markdown source version")?;
        assert_eq!(markdown_version.mime_type, "text/markdown");
        let markdown_chunk = database
            .list_knowledge_chunks(markdown_version.id)?
            .into_iter()
            .next()
            .ok_or("missing markdown source chunk")?;
        assert!(markdown_chunk.location.get("snapshotId").is_none());
        let repository_report = generated_reports
            .items
            .iter()
            .find(|document| document.logical_path == "code-reports/repository-overview.md")
            .ok_or("missing generated repository report")?;
        let report_versions = database.list_knowledge_document_versions(repository_report.id)?;
        assert_eq!(report_versions.len(), 1);
        let report_chunk = database
            .list_knowledge_chunks(report_versions[0].id)?
            .into_iter()
            .next()
            .ok_or("missing generated report chunk")?;
        assert_eq!(
            KnowledgeService::get_citation_detail(&database, report_chunk.id)?
                .citation
                .snapshot_id,
            Some(snapshot.id)
        );
        let report_search =
            crate::services::knowledge_retrieval::KnowledgeRetrievalService::search_fts(
                &database,
                crate::models::KnowledgeSearchInput {
                    query: "仓库概览".to_string(),
                    project_ids: Vec::new(),
                    release_ids: Vec::new(),
                    source_ids: Vec::new(),
                    document_types: vec!["code_report".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: Some(snapshot.id),
                    limit: Some(10),
                    include_context: Some(true),
                },
            )?;
        assert!(!report_search.is_empty());
        assert!(KnowledgeService::generate_code_snapshot_documents(
            &database,
            crate::models::GenerateKnowledgeCodeDocumentsInput {
                snapshot_id: snapshot.id,
            },
        )?
        .generated_document_version_ids
        .is_empty());
        let files = database.list_knowledge_code_files(snapshot.id)?;
        assert!(files
            .iter()
            .any(|file| file.relative_path == "src/secret.rs" && file.sensitivity == "restricted"));
        let redacted_file = files
            .iter()
            .find(|file| file.relative_path == "src/redacted.rs")
            .ok_or("missing redacted code file")?;
        assert_eq!(redacted_file.status, "active");
        assert_eq!(
            redacted_file.skip_reason,
            "redacted_sensitive_content:credential_or_connection_string"
        );
        let redacted_version = database
            .get_knowledge_document_version_by_id(
                redacted_file
                    .document_version_id
                    .ok_or("missing redacted document version")?,
            )?
            .ok_or("missing redacted document content")?;
        assert!(redacted_version.content.contains("[REDACTED]"));
        assert!(!redacted_version
            .content
            .contains("correct horse battery staple"));
        assert!(!redacted_version.content.contains("horse"));
        assert!(redacted_version.content.contains("blCancelToken: true"));
        assert!(redacted_version.content.contains("token: response.data"));
        let symbols = database.list_knowledge_code_symbols(snapshot.id, Some("submit"))?;
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].start_line, 3);
        let version_id = files
            .iter()
            .find(|file| file.relative_path == "src/lib.rs")
            .and_then(|file| file.document_version_id)
            .ok_or("missing code document version")?;
        let chunks = database.list_knowledge_chunks(version_id)?;
        let submit_chunk = chunks
            .iter()
            .find(|chunk| chunk.location["symbol"] == "submit_order")
            .ok_or("missing submit_order code chunk")?;
        assert_eq!(submit_chunk.location["snapshotId"], snapshot.id);
        assert_eq!(submit_chunk.location["startLine"], 3);
        assert_eq!(submit_chunk.location["language"], "rust");
        assert_eq!(submit_chunk.location["path"], "src/lib.rs");
        assert_eq!(submit_chunk.location["sensitivity"], "internal");
        assert!(submit_chunk.location["signature"]
            .as_str()
            .is_some_and(|signature| signature.contains("submit_order")));
        let fts_hits = crate::services::knowledge_retrieval::KnowledgeRetrievalService::search_fts(
            &database,
            crate::models::KnowledgeSearchInput {
                query: "submit_order".to_string(),
                project_ids: Vec::new(),
                release_ids: Vec::new(),
                source_ids: Vec::new(),
                document_types: vec!["code".to_string()],
                sensitivities: vec!["internal".to_string()],
                snapshot_id: Some(snapshot.id),
                limit: Some(10),
                include_context: Some(true),
            },
        )?;
        let symbol_hit = fts_hits
            .iter()
            .find(|hit| hit.citation.symbol_key == symbols[0].symbol_key)
            .ok_or("missing exact code symbol FTS hit")?;
        assert_eq!(symbol_hit.citation.snapshot_id, Some(snapshot.id));
        assert_eq!(symbol_hit.citation.start_line, Some(3));
        let detail = KnowledgeService::get_citation_detail(&database, submit_chunk.id)?;
        assert_eq!(detail.citation.source_type, "code_snapshot");
        assert_eq!(detail.citation.snapshot_id, Some(snapshot.id));
        let relations = database.list_knowledge_code_relations(snapshot.id, None, None)?;
        assert!(relations.iter().any(|relation| {
            relation.relation_type == "calls"
                && relation.from_symbol_key.contains("dispatch_order")
                && relation.to_symbol_key.contains("submit_order")
                && relation.evidence_start_line == Some(2)
                && !relation.confirmed
        }));
        assert!(relations.iter().any(|relation| {
            relation.relation_type == "tauri_ipc"
                && relation.from_symbol_key.contains("dispatch_order")
                && relation.to_symbol_key.contains("submit_order")
                && !relation.confirmed
        }));
        assert!(relations.iter().any(|relation| {
            relation.relation_type == "tauri_event"
                && relation.to_external_key == "order-updated"
                && !relation.confirmed
        }));
        assert!(relations.iter().any(|relation| {
            relation.relation_type == "config_uses"
                && relation.to_external_key == "ORDER_MODE"
                && !relation.confirmed
        }));
        let call_relation = relations
            .iter()
            .find(|relation| relation.relation_type == "calls")
            .ok_or("missing code call relation")?;
        database.confirm_knowledge_code_relation(call_relation.id, true)?;
        let symbol_search = KnowledgeService::search_code_symbols(
            &database,
            crate::models::SearchKnowledgeCodeSymbolsInput {
                snapshot_id: snapshot.id,
                keyword: Some("submit_order".to_string()),
            },
        )?;
        assert_eq!(symbol_search.len(), 1);
        let call_graph = KnowledgeService::code_call_graph(
            &database,
            crate::models::KnowledgeCodeCallGraphInput {
                snapshot_id: snapshot.id,
                symbol_key: call_relation.from_symbol_key.clone(),
                max_depth: Some(2),
                include_unconfirmed: Some(false),
            },
        )?;
        assert!(call_graph
            .nodes
            .iter()
            .any(|symbol| symbol.symbol_key == call_relation.to_symbol_key));
        assert!(call_graph.edges.iter().all(|relation| relation.confirmed));
        let impact = KnowledgeService::analyze_code_impact(
            &database,
            crate::models::AnalyzeKnowledgeCodeImpactInput {
                snapshot_id: snapshot.id,
                symbol_keys: vec![call_relation.to_symbol_key.clone()],
                max_depth: Some(2),
            },
        )?;
        assert!(impact
            .nodes
            .iter()
            .any(|symbol| symbol.symbol_key == call_relation.from_symbol_key));
        let hybrid =
            crate::services::knowledge_retrieval::KnowledgeRetrievalService::search_hybrid(
                &database,
                crate::models::KnowledgeHybridSearchInput {
                    filters: crate::models::KnowledgeSearchInput {
                        query: "submit_order".to_string(),
                        project_ids: Vec::new(),
                        release_ids: Vec::new(),
                        source_ids: Vec::new(),
                        document_types: vec!["code".to_string()],
                        sensitivities: vec!["internal".to_string()],
                        snapshot_id: Some(snapshot.id),
                        limit: Some(10),
                        include_context: Some(true),
                    },
                    query_vector: None,
                    relation_depth: Some(1),
                },
            )?;
        assert!(hybrid.hits.iter().any(|hit| {
            hit.citation.symbol_key.contains("dispatch_order")
                && hit.channels.iter().any(|channel| channel == "relation")
        }));
        KnowledgeService::analyze_code_snapshot(&database, snapshot.id).await?;
        assert!(database
            .list_knowledge_code_relations(snapshot.id, None, None)?
            .iter()
            .any(|relation| {
                relation.relation_type == "calls"
                    && relation.from_symbol_key == call_relation.from_symbol_key
                    && relation.to_symbol_key == call_relation.to_symbol_key
                    && relation.confirmed
            }));
        fs::write(
            root.join("source/src/lib.rs"),
            "pub fn submit_order() { /* changed after capture */ }",
        )?;
        let changed_snapshot = KnowledgeService::capture_local_directory_snapshot(
            &database,
            CaptureKnowledgeLocalDirectorySnapshotInput {
                source_id: source.source.id,
                release_id: None,
            },
        )?;
        let changed_analysis =
            KnowledgeService::analyze_code_snapshot(&database, changed_snapshot.id).await?;
        assert!(changed_analysis
            .warnings
            .iter()
            .any(|warning| warning.contains("按内容哈希复用")));
        let comparison = KnowledgeService::compare_code_snapshots(
            &database,
            crate::models::CompareKnowledgeCodeSnapshotsInput {
                from_snapshot_id: snapshot.id,
                to_snapshot_id: changed_snapshot.id,
            },
        )?;
        assert!(comparison.file_changes.iter().any(|change| {
            change.change_type == "modified"
                && change.from_path == "src/lib.rs"
                && change.to_path == "src/lib.rs"
        }));
        fs::rename(
            root.join("source/src/caller.rs"),
            root.join("source/src/renamed_caller.rs"),
        )?;
        let renamed_snapshot = KnowledgeService::capture_local_directory_snapshot(
            &database,
            CaptureKnowledgeLocalDirectorySnapshotInput {
                source_id: source.source.id,
                release_id: None,
            },
        )?;
        let renamed_analysis =
            KnowledgeService::analyze_code_snapshot(&database, renamed_snapshot.id).await?;
        assert!(renamed_analysis
            .warnings
            .iter()
            .any(|warning| warning.contains("重命名文件")));
        let renamed_comparison = KnowledgeService::compare_code_snapshots(
            &database,
            crate::models::CompareKnowledgeCodeSnapshotsInput {
                from_snapshot_id: changed_snapshot.id,
                to_snapshot_id: renamed_snapshot.id,
            },
        )?;
        assert!(renamed_comparison.file_changes.iter().any(|change| {
            change.change_type == "renamed"
                && change.from_path == "src/caller.rs"
                && change.to_path == "src/renamed_caller.rs"
        }));
        assert!(database
            .list_knowledge_code_relations(renamed_snapshot.id, None, None)?
            .iter()
            .all(|relation| !relation.from_symbol_key.contains("src/caller.rs")));
        assert!(
            KnowledgeService::analyze_code_snapshot(&database, snapshot.id)
                .await
                .is_err()
        );
        assert_eq!(
            database
                .get_knowledge_code_snapshot_by_id(snapshot.id)?
                .ok_or("missing changed snapshot")?
                .status,
            "failed"
        );
        assert!(KnowledgeService::list_relations(
            &database,
            ListKnowledgeRelationsInput {
                entity_type: Some("code_snapshot".to_string()),
                entity_key: Some(snapshot.id.to_string()),
                project_ids: Vec::new(),
                release_ids: Vec::new(),
                sensitivities: Vec::new(),
                confirmed_only: Some(true),
                limit: Some(20),
            },
        )?
        .is_empty());
        assert!(
            crate::services::knowledge_retrieval::KnowledgeRetrievalService::search_fts(
                &database,
                crate::models::KnowledgeSearchInput {
                    query: "submit_order".to_string(),
                    project_ids: Vec::new(),
                    release_ids: Vec::new(),
                    source_ids: Vec::new(),
                    document_types: vec!["code".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: Some(snapshot.id),
                    limit: Some(10),
                    include_context: Some(true),
                },
            )?
            .is_empty()
        );
        assert!(KnowledgeService::get_citation_detail(&database, submit_chunk.id).is_err());
        assert!(KnowledgeService::get_citation_detail(&database, report_chunk.id).is_err());
        assert!(
            crate::services::knowledge_retrieval::KnowledgeRetrievalService::search_fts(
                &database,
                crate::models::KnowledgeSearchInput {
                    query: "仓库概览".to_string(),
                    project_ids: Vec::new(),
                    release_ids: Vec::new(),
                    source_ids: Vec::new(),
                    document_types: vec!["code_report".to_string()],
                    sensitivities: vec!["internal".to_string()],
                    snapshot_id: Some(snapshot.id),
                    limit: Some(10),
                    include_context: Some(true),
                },
            )?
            .is_empty()
        );
        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn code_scope_preview_explains_sensitive_binary_size_and_language_skips(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-code-scope-preview-{}",
            std::process::id()
        ));
        let source_root = root.join("source/src");
        fs::create_dir_all(source_root.join("node_modules/dependency"))?;
        fs::write(source_root.join("ok.rs"), "pub fn allowed() {}")?;
        fs::write(source_root.join(".env"), "TOKEN=not-indexed")?;
        fs::write(source_root.join("binary.rs"), b"not\0source")?;
        fs::write(source_root.join("large.rs"), vec![b'x'; 4097])?;
        fs::write(source_root.join("notes.txt"), "not an allowed language")?;
        fs::write(
            source_root.join("node_modules/dependency/index.rs"),
            "pub fn dependency() {}",
        )?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let source = database.upsert_knowledge_code_source(&UpsertKnowledgeCodeSourceInput {
            source: UpsertKnowledgeSourceInput {
                id: None,
                source_key: "code-preview".to_string(),
                project_id: None,
                source_type: "local_directory".to_string(),
                display_name: "源码范围预览".to_string(),
                root_path: root.join("source").to_string_lossy().to_string(),
                git_workspace_key: String::new(),
                include_globs: vec!["src/**".to_string()],
                exclude_globs: Vec::new(),
                version_strategy: "unversioned".to_string(),
                sync_mode: "manual".to_string(),
                allow_remote_embedding: false,
                enabled: true,
            },
            include_untracked: false,
            max_file_size_bytes: 4096,
            allowed_languages: vec!["rust".to_string()],
            allow_remote_processing: false,
        })?;
        let preview = KnowledgeService::preview_code_source_scope(&database, source.source.id)?;
        let reasons = preview
            .entries
            .iter()
            .map(|entry| (entry.relative_path.as_str(), entry.reason.as_str()))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(reasons.get("src/.env"), Some(&"sensitive_path"));
        assert_eq!(reasons.get("src/binary.rs"), Some(&"binary_content"));
        assert_eq!(reasons.get("src/large.rs"), Some(&"file_too_large"));
        assert_eq!(reasons.get("src/notes.txt"), Some(&"language_not_allowed"));
        assert_eq!(reasons.get("src/ok.rs"), Some(&"within_effective_scope"));
        assert!(preview
            .entries
            .iter()
            .any(|entry| entry.relative_path == "src/node_modules"
                && entry.reason == "excluded_by_rule"));
        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn indexes_mybatis_mapper_statement_with_existing_sql_allowlist() {
        let mapper = r#"<mapper namespace="example.OrderMapper">
            <select id="selectCandidateWorkOrdersByUserIds" resultMap="BaseResultMap">
                SELECT d.id
                FROM bda_work_order_receive_uid aru
                STRAIGHT_JOIN bda_work_order_detail d ON d.app_task_id = aru.app_task_id
                WHERE aru.receive_uid IN <foreach>#{uid}</foreach>
                  AND d.is_deleted = 0
                  AND d.state_id = 0
                LIMIT #{limit}
            </select>
            <update id="markHandled">UPDATE bda_work_order_detail SET state_id = 1</update>
        </mapper>"#;
        assert_eq!(
            code_language_for_path("mapper/BdaWorkOrderDetailMapper.xml"),
            "mybatis_xml"
        );
        assert!(is_code_language_allowed(
            &["java".to_string(), "sql".to_string()],
            "mybatis_xml"
        ));
        assert!(!is_code_language_allowed(
            &["java".to_string()],
            "mybatis_xml"
        ));

        let analysis = P0LanguageAnalyzer::analyze_path("mapper/OrderMapper.xml", mapper);
        let chunks = code_symbol_chunks(
            2,
            Some(1),
            &analysis.language,
            "internal",
            mapper,
            "mapper/OrderMapper.xml",
            &analysis.symbols,
        );
        let candidate_chunk = chunks
            .iter()
            .find(|chunk| {
                chunk
                    .heading_path
                    .ends_with("#selectCandidateWorkOrdersByUserIds")
            })
            .expect("候选工单 SQL 应形成独立的可检索片段");
        for condition in [
            "receive_uid",
            "is_deleted = 0",
            "state_id = 0",
            "LIMIT #{limit}",
        ] {
            assert!(candidate_chunk.content.contains(condition));
        }
    }

    #[test]
    fn local_source_sync_preserves_identity_and_history() -> Result<(), Box<dyn std::error::Error>>
    {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-local-sync-{}-{unique}",
            std::process::id()
        ));
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root)?;
        fs::write(source_root.join("requirement.md"), "# v1\n退款审批")?;

        #[cfg(unix)]
        {
            let outside = root.join("outside.md");
            fs::write(&outside, "不应跟随符号链接")?;
            std::os::unix::fs::symlink(&outside, source_root.join("outside-link.md"))?;
        }

        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "local-project".to_string(),
            name: "本地项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let source = database.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            id: None,
            source_key: "local-docs".to_string(),
            project_id: Some(project.id),
            source_type: "local_directory".to_string(),
            display_name: "本地文档".to_string(),
            root_path: source_root.to_string_lossy().to_string(),
            git_workspace_key: String::new(),
            include_globs: vec!["**/*.md".to_string()],
            exclude_globs: Vec::new(),
            version_strategy: "unversioned".to_string(),
            sync_mode: "manual".to_string(),
            allow_remote_embedding: false,
            enabled: true,
        })?;
        let sync_input = SyncKnowledgeLocalSourceInput {
            source_id: source.id,
            release_id: None,
        };

        let first = super::KnowledgeService::sync_local_source(&database, sync_input.clone())?;
        assert_eq!(first.created_versions, 1);
        let first_document = database
            .list_knowledge_documents(&KnowledgeListInput {
                project_id: Some(project.id),
                release_id: None,
                source_id: Some(source.id),
                keyword: None,
                status: None,
                offset: None,
                limit: None,
            })?
            .items
            .into_iter()
            .next()
            .ok_or("missing synced document")?;
        let first_version = database
            .list_knowledge_document_versions(first_document.id)?
            .into_iter()
            .next()
            .ok_or("missing synced document version")?;
        assert!(!database.list_knowledge_chunks(first_version.id)?.is_empty());
        assert_eq!(
            database
                .list_knowledge_jobs(20)?
                .iter()
                .filter(|job| job.job_type == "document_index")
                .count(),
            1
        );
        #[cfg(unix)]
        assert_eq!(first.skipped_files, 1);

        let second = super::KnowledgeService::sync_local_source(&database, sync_input.clone())?;
        assert_eq!(second.created_versions, 0);
        assert_eq!(second.unchanged_files, 1);

        fs::write(source_root.join("requirement.md"), "# v2\n退款审批流程")?;
        let third = super::KnowledgeService::sync_local_source(&database, sync_input.clone())?;
        assert_eq!(third.created_versions, 1);

        let before_rename = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project.id),
            release_id: None,
            source_id: Some(source.id),
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(before_rename.total, 1);
        let document_id = before_rename.items[0].id;
        assert_eq!(
            database
                .list_knowledge_document_versions(document_id)?
                .len(),
            2
        );

        fs::rename(
            source_root.join("requirement.md"),
            source_root.join("refund.md"),
        )?;
        let renamed = super::KnowledgeService::sync_local_source(&database, sync_input.clone())?;
        // 本地重命名同样属于不可变来源路径变化：保留旧版本路径，同时为当前路径
        // 建立新版本，避免同一内容的引用在历史和当前同步之间混用。
        assert_eq!(renamed.created_versions, 1);
        assert_eq!(renamed.unchanged_files, 0);
        assert_eq!(renamed.deleted_paths, 0);
        let after_rename = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project.id),
            release_id: None,
            source_id: Some(source.id),
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(after_rename.total, 1);
        assert_eq!(after_rename.items[0].id, document_id);
        assert_eq!(after_rename.items[0].logical_path, "refund.md");
        assert_eq!(
            database
                .list_knowledge_document_versions(document_id)?
                .len(),
            3
        );

        fs::remove_file(source_root.join("refund.md"))?;
        let deleted = super::KnowledgeService::sync_local_source(&database, sync_input)?;
        assert_eq!(deleted.deleted_paths, 1);
        let deleted_documents = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project.id),
            release_id: None,
            source_id: Some(source.id),
            keyword: None,
            status: Some("deleted".to_string()),
            offset: None,
            limit: None,
        })?;
        assert_eq!(deleted_documents.total, 1);
        assert_eq!(
            database
                .list_knowledge_document_versions(document_id)?
                .len(),
            3
        );

        let single_path = root.join("single.md");
        fs::write(&single_path, "# 单文件")?;
        let single_source = database.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            id: None,
            source_key: "single-doc".to_string(),
            project_id: Some(project.id),
            source_type: "single_file".to_string(),
            display_name: "单文件".to_string(),
            root_path: single_path.to_string_lossy().to_string(),
            git_workspace_key: String::new(),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            version_strategy: "unversioned".to_string(),
            sync_mode: "manual".to_string(),
            allow_remote_embedding: false,
            enabled: true,
        })?;
        let single = super::KnowledgeService::sync_local_source(
            &database,
            SyncKnowledgeLocalSourceInput {
                source_id: single_source.id,
                release_id: None,
            },
        )?;
        assert_eq!(single.created_versions, 1);
        assert_eq!(single.scanned_files, 1);

        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn local_sync_purges_previously_indexed_content_when_it_becomes_sensitive(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-sensitive-sync-{}-{unique}",
            std::process::id()
        ));
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root)?;
        let path = source_root.join("requirement.md");
        fs::write(&path, "# 退款审批\n正常需求正文")?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;
        let source = database.upsert_knowledge_source(&UpsertKnowledgeSourceInput {
            id: None,
            source_key: "sensitive-local-docs".to_string(),
            project_id: None,
            source_type: "local_directory".to_string(),
            display_name: "敏感同步测试".to_string(),
            root_path: source_root.to_string_lossy().to_string(),
            git_workspace_key: String::new(),
            include_globs: vec!["**/*.md".to_string()],
            exclude_globs: Vec::new(),
            version_strategy: "unversioned".to_string(),
            sync_mode: "manual".to_string(),
            allow_remote_embedding: false,
            enabled: true,
        })?;
        let input = SyncKnowledgeLocalSourceInput {
            source_id: source.id,
            release_id: None,
        };
        KnowledgeService::sync_local_source(&database, input.clone())?;
        let document = database
            .get_knowledge_document_by_source_path(source.id, "requirement.md")?
            .expect("首次同步应创建文档");
        let version = database.list_knowledge_document_versions(document.id)?[0].clone();
        KnowledgeService::parse_and_index_document_version(&database, version.id, None)?;
        assert!(!database.list_knowledge_chunks(version.id)?.is_empty());

        fs::write(&path, "password=must-not-be-indexed")?;
        let blocked = KnowledgeService::sync_local_source(&database, input)?;
        assert_eq!(blocked.created_versions, 0);
        assert_eq!(blocked.skipped_files, 1);
        let restricted = database
            .get_knowledge_document_by_id(document.id)?
            .expect("文档元数据应保留用于审计");
        assert_eq!(restricted.sensitivity, "restricted");
        assert!(!restricted.allow_ai && !restricted.allow_mcp);
        let retained_version = database.list_knowledge_document_versions(document.id)?[0].clone();
        assert!(!retained_version.valid);
        assert!(retained_version.content.is_empty());
        assert!(database
            .list_knowledge_chunks(retained_version.id)?
            .is_empty());
        assert!(KnowledgeService::get_document_detail(&database, document.id).is_err());

        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn source_sync_failure_preserves_history_and_rejects_cross_project_release(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-knowledge-source-failure-{}-{unique}",
            std::process::id()
        ));
        let source_root = root.join("docs");
        fs::create_dir_all(&source_root)?;
        fs::write(source_root.join("requirement.md"), "# 已发布需求")?;
        let database = Database::init(root.join("knowledge.sqlite").to_string_lossy().as_ref())?;

        let project_a = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "failure-project-a".to_string(),
            name: "失败测试项目 A".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let project_b = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "failure-project-b".to_string(),
            name: "失败测试项目 B".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let release_b = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project_b.id,
            version: "v2.0.0".to_string(),
            tag_name: "v2.0.0".to_string(),
            branch: "release/v2.0.0".to_string(),
            commit_sha: String::new(),
            description: String::new(),
            released_at: None,
        })?;
        let mut source_input = UpsertKnowledgeSourceInput {
            id: None,
            source_key: "failure-local-docs".to_string(),
            project_id: Some(project_a.id),
            source_type: "local_directory".to_string(),
            display_name: "失败测试本地文档".to_string(),
            root_path: source_root.to_string_lossy().to_string(),
            git_workspace_key: String::new(),
            include_globs: vec!["**/*.md".to_string()],
            exclude_globs: Vec::new(),
            version_strategy: "unversioned".to_string(),
            sync_mode: "manual".to_string(),
            allow_remote_embedding: false,
            enabled: true,
        };
        let source = database.upsert_knowledge_source(&source_input)?;
        let first = super::KnowledgeService::sync_local_source(
            &database,
            SyncKnowledgeLocalSourceInput {
                source_id: source.id,
                release_id: None,
            },
        )?;
        assert_eq!(first.created_versions, 1);

        let cross_project = super::KnowledgeService::sync_local_source(
            &database,
            SyncKnowledgeLocalSourceInput {
                source_id: source.id,
                release_id: Some(release_b.id),
            },
        );
        assert!(cross_project.is_err());

        source_input.id = Some(source.id);
        source_input.root_path = root.join("missing").to_string_lossy().to_string();
        database.upsert_knowledge_source(&source_input)?;
        let failed = super::KnowledgeService::sync_local_source(
            &database,
            SyncKnowledgeLocalSourceInput {
                source_id: source.id,
                release_id: None,
            },
        );
        assert!(failed.is_err());
        let failed_source = database
            .get_knowledge_source_by_id(source.id)?
            .ok_or("同步失败后的知识源不存在")?;
        assert_eq!(failed_source.last_sync_status, "failed");
        assert!(failed_source.last_error.is_some());

        let documents = database.list_knowledge_documents(&KnowledgeListInput {
            project_id: Some(project_a.id),
            release_id: None,
            source_id: Some(source.id),
            keyword: None,
            status: None,
            offset: None,
            limit: None,
        })?;
        assert_eq!(documents.total, 1);
        assert_eq!(
            database
                .list_knowledge_document_versions(documents.items[0].id)?
                .len(),
            1
        );

        drop(database);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn imports_explicit_front_matter_and_commit_identifier_relations(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "relation-fixture-project".to_string(),
            name: "关系夹具项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "relation-fixture".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "requirement".to_string(),
            title: "关系夹具".to_string(),
            logical_path: "relations.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: None,
                version_label: "unversioned".to_string(),
                git_branch: String::new(),
                commit_sha: String::new(),
                source_path: "relations.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "# relations".to_string(),
                content_hash: "relation-fixture-v1".to_string(),
                parsed_meta: serde_json::json!({
                    "frontMatter": {
                        "relationships": [{
                            "fromType": "requirement",
                            "fromKey": "REQ-42",
                            "relationType": "implemented_by",
                            "toType": "commit",
                            "toKey": "abcdef1",
                        }]
                    }
                }),
                token_estimate: 1,
            },
            &[],
        )?;
        let front_matter = KnowledgeService::import_document_front_matter_relations(
            &database,
            ImportKnowledgeDocumentRelationsInput {
                document_version_id: version.id,
            },
        )?;
        assert_eq!(front_matter.len(), 1);
        assert!(front_matter[0].confirmed);
        assert_eq!(front_matter[0].project_id, Some(project.id));
        let commits = KnowledgeService::import_commit_message_relations(
            &database,
            ImportKnowledgeCommitRelationsInput {
                commit_sha: "abcdef1".to_string(),
                commit_message: "feat: approve refund REQ-42 Task-8".to_string(),
                entity_prefixes: Some(vec!["req".to_string(), "task".to_string()]),
                confirmed: Some(false),
                snapshot_id: None,
            },
        )?;
        assert_eq!(commits.len(), 2);
        assert!(commits.iter().all(|relation| !relation.confirmed));
        Ok(())
    }

    fn run_git(repo: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let output = StdCommand::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "git {} 失败: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }
}
