use std::path::Path;

use crate::database::Database;
use crate::error::AppError;
use crate::models::knowledge_domain::qa::{
    KnowledgeQaSession, KnowledgeQaSessionDetail, KnowledgeScopedQuestionInput,
    PersistKnowledgeQaRoundInput,
};
use crate::models::{
    KnowledgeAskInput, KnowledgeAskResult, KnowledgeConversationMessage, KnowledgeSearchInput,
};
use crate::services::knowledge::validate_positive_id;
use crate::services::knowledge_domain::terminology::KnowledgeProjectTerminologyService;
use crate::services::knowledge_embedding::KnowledgeEmbeddingService;
use crate::services::knowledge_retrieval::KnowledgeRetrievalService;
use crate::services::knowledge_rollout::KnowledgeRolloutService;

pub(crate) const DOMAIN: &str = "qa";

const MAX_CONVERSATION_MESSAGES: usize = 12;
const MAX_CONVERSATION_MESSAGE_CHARS: usize = 8_000;
const MAX_CONVERSATION_CHARS: usize = 48_000;
const RELEASE_REQUIREMENT_COVERAGE_MODE: &str = "releaseRequirementCoverage";

/// 仅查看本地证据时不得生成活动向量。活动 Profile 可能指向远程 Provider，因而这是
/// 数据外发边界，而不是单纯的召回策略开关。
fn should_generate_query_embedding(evidence_only: bool) -> bool {
    !evidence_only
}

/// 项目工作台问答的作用域边界。它只负责将受限的项目/版本/仓库请求转换为既有检索
/// 引擎的硬过滤条件；模型调用、来源授权、拒答和引用校验仍由检索服务统一执行。
pub struct KnowledgeScopedQuestionService;

impl KnowledgeScopedQuestionService {
    pub fn list_sessions(
        db: &Database,
        project_id: i64,
    ) -> Result<Vec<KnowledgeQaSession>, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        validate_positive_id(project_id, "项目 ID")?;
        if !db.knowledge_project_exists(project_id)? {
            return Err(AppError::NotFound(format!("知识项目不存在: {project_id}")));
        }
        db.list_knowledge_qa_sessions(project_id)
    }

    pub fn get_session(
        db: &Database,
        project_id: i64,
        session_id: i64,
    ) -> Result<KnowledgeQaSessionDetail, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        validate_positive_id(project_id, "项目 ID")?;
        validate_positive_id(session_id, "问答会话 ID")?;
        let detail = db
            .get_knowledge_qa_session_detail(session_id)?
            .ok_or_else(|| AppError::NotFound(format!("问答会话不存在: {session_id}")))?;
        if detail.session.project_id != project_id {
            return Err(AppError::InvalidInput("问答会话不属于当前项目".to_string()));
        }
        Ok(detail)
    }

    pub fn persist_round(
        db: &Database,
        mut input: PersistKnowledgeQaRoundInput,
    ) -> Result<KnowledgeQaSessionDetail, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        validate_positive_id(input.project_id, "项目 ID")?;
        validate_positive_id(input.project_version_id, "项目版本 ID")?;
        if let Some(session_id) = input.session_id {
            validate_positive_id(session_id, "问答会话 ID")?;
        }
        input.question = input.question.trim().to_string();
        if input.question.is_empty() || input.question.chars().count() > 2_000 {
            return Err(AppError::InvalidInput(
                "项目问题不能为空且不能超过 2000 个字符".to_string(),
            ));
        }
        if input.answer.answer.trim().is_empty() {
            return Err(AppError::InvalidInput("助手回答不能为空".to_string()));
        }
        input.provider_key = input.provider_key.trim().to_string();
        input.model = input.model.trim().to_string();
        let release = db
            .get_knowledge_release_by_id(input.project_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识版本不存在: {}", input.project_version_id))
            })?;
        if release.project_id != input.project_id {
            return Err(AppError::InvalidInput(
                "项目版本不属于当前项目，不能保存问答会话".to_string(),
            ));
        }
        let title: String = input.question.chars().take(60).collect();
        let session_id = db.persist_knowledge_qa_round(&input, &title, &release.commit_sha)?;
        db.get_knowledge_qa_session_detail(session_id)?
            .ok_or_else(|| AppError::Custom("保存问答会话后无法读取结果".to_string()))
    }

    pub fn delete_session(db: &Database, project_id: i64, session_id: i64) -> Result<(), AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        validate_positive_id(project_id, "项目 ID")?;
        validate_positive_id(session_id, "问答会话 ID")?;
        db.soft_delete_knowledge_qa_session(project_id, session_id)
    }

    pub async fn ask(
        db: &Database,
        app_data_dir: &Path,
        mut input: KnowledgeScopedQuestionInput,
    ) -> Result<KnowledgeAskResult, AppError> {
        KnowledgeRolloutService::require(db, "hybrid_rag")?;
        validate_positive_id(input.project_id, "项目 ID")?;
        validate_positive_id(input.project_version_id, "项目版本 ID")?;
        input.question = input.question.trim().to_string();
        if input.question.is_empty() {
            return Err(AppError::InvalidInput("请输入项目问题".to_string()));
        }
        if input.question.chars().count() > 2_000 {
            return Err(AppError::InvalidInput(
                "项目问题不能超过 2000 个字符".to_string(),
            ));
        }
        input.conversation = normalize_conversation(input.conversation)?;
        let project = db
            .get_knowledge_project_by_id(input.project_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识项目不存在: {}", input.project_id)))?;
        let release = db
            .get_knowledge_release_by_id(input.project_version_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("知识版本不存在: {}", input.project_version_id))
            })?;
        if release.project_id != input.project_id {
            return Err(AppError::InvalidInput(
                "项目版本不属于当前项目，不能跨项目问答".to_string(),
            ));
        }
        if !input.repository_binding_ids.is_empty() {
            return Err(AppError::InvalidInput(
                "项目问答当前按完整版本范围检索，请清除仓库筛选后重试".to_string(),
            ));
        }
        if !input.evidence_only
            && (input.provider_key.trim().is_empty() || input.model.trim().is_empty())
        {
            return Err(AppError::InvalidInput(
                "请选择已配置的 AI 服务商和模型，或仅查看本地证据".to_string(),
            ));
        }

        // “查看本地证据”是明确的无外发操作：即使当前活动 Profile 是远程向量化方案，
        // 也不能为了补充语义召回而发送用户问题。该模式仍通过本地 FTS 和关系检索返回
        // 可追溯证据；只有用户选择“基于证据回答”时才允许沿用活动向量方案。
        let query_vector = if should_generate_query_embedding(input.evidence_only) {
            KnowledgeEmbeddingService::generate_active_query_embedding(
                db,
                app_data_dir,
                &input.question,
            )
            .await?
        } else {
            None
        };
        // 仅使用当前项目中人工确认的术语别名补充召回词，例如“明日工作计划”可以
        // 额外检索 `tomorrowWorkPlan`。生成规则问题会使用更精确的代码符号召回，
        // 原始问题则通过 original_question 完整传给模型；项目与版本过滤不变。
        let answer_mode = answer_mode_for_question(&input.question);
        let retrieval_question = if answer_mode.is_some() {
            release_requirement_coverage_query(
                &input.question,
                &project.name,
                &release.version,
                &input.conversation,
            )
        } else {
            input.question.clone()
        };
        let retrieval_query =
            expand_project_term_aliases(db, input.project_id, &retrieval_question)?;
        KnowledgeRetrievalService::ask_with_query_vector(
            db,
            KnowledgeAskInput {
                search: KnowledgeSearchInput {
                    query: retrieval_query,
                    project_ids: vec![input.project_id],
                    release_ids: vec![input.project_version_id],
                    source_ids: Vec::new(),
                    document_types: Vec::new(),
                    sensitivities: Vec::new(),
                    snapshot_id: None,
                    limit: Some(20),
                    include_context: Some(true),
                },
                original_question: Some(input.question.clone()),
                answer_mode,
                provider_key: input.provider_key.trim().to_string(),
                model: input.model.trim().to_string(),
                evidence_only: Some(input.evidence_only),
                conversation: input.conversation,
            },
            query_vector,
        )
        .await
    }
}

/// “当前版本实现了哪些需求”不是普通相似度问答：它需要同时保留需求全集和代码证据。
/// 模式识别只影响检索规划，不改变项目、版本和权限硬过滤。
fn answer_mode_for_question(question: &str) -> Option<String> {
    let normalized = question.trim().to_ascii_lowercase();
    let asks_requirement = ["需求", "功能", "事项", "req", "story"]
        .iter()
        .any(|token| normalized.contains(token));
    let asks_implementation = ["实现", "完成", "未做", "进度", "覆盖"]
        .iter()
        .any(|token| normalized.contains(token));
    (asks_requirement && asks_implementation).then(|| RELEASE_REQUIREMENT_COVERAGE_MODE.to_string())
}

/// 连续追问时只复用最近一条“需求范围”用户问题来改写检索词。助手历史仍只用于指代
/// 消解，不能成为证据；发布版本由当前页面选择值补入，用户无需重复输入版本号。
fn release_requirement_coverage_query(
    _question: &str,
    project_name: &str,
    release_version: &str,
    conversation: &[KnowledgeConversationMessage],
) -> String {
    let previous_requirement_scope = conversation.iter().rev().find_map(|message| {
        if !message.role.eq_ignore_ascii_case("user") {
            return None;
        }
        let content = message.content.trim();
        ["需求", "功能", "req", "story"]
            .iter()
            .any(|token| content.to_ascii_lowercase().contains(token))
            .then_some(content)
    });
    previous_requirement_scope
        .map(|scope| format!("{} {}", scope, release_version.trim()))
        .unwrap_or_else(|| {
            format!(
                "{} {} 需求文档",
                project_name.trim(),
                release_version.trim()
            )
        })
}

/// 会话上下文只保留最近几轮，并在进入检索/Provider 链路前限制总长度。历史消息
/// 用于理解“这个方法/上面的 SQL”等指代，不属于当前轮证据，不能无限增长或改变
/// 项目、版本硬过滤条件。
fn normalize_conversation(
    messages: Vec<KnowledgeConversationMessage>,
) -> Result<Vec<KnowledgeConversationMessage>, AppError> {
    let mut normalized = Vec::new();
    for message in messages.into_iter().rev().take(MAX_CONVERSATION_MESSAGES) {
        let role = message.role.trim().to_ascii_lowercase();
        if !matches!(role.as_str(), "user" | "assistant") {
            return Err(AppError::InvalidInput(
                "项目问答历史消息角色只能是 user 或 assistant".to_string(),
            ));
        }
        let content = message.content.trim();
        if content.is_empty() {
            continue;
        }
        normalized.push(KnowledgeConversationMessage {
            role,
            content: content
                .chars()
                .take(MAX_CONVERSATION_MESSAGE_CHARS)
                .collect(),
        });
    }
    normalized.reverse();

    while normalized
        .iter()
        .map(|message| message.content.chars().count())
        .sum::<usize>()
        > MAX_CONVERSATION_CHARS
    {
        if normalized.len() <= 1 {
            break;
        }
        normalized.remove(0);
    }
    // 截断后避免上下文从 assistant 回答开始，减少模型失去当前会话起点的概率。
    if normalized
        .first()
        .is_some_and(|message| message.role == "assistant")
    {
        normalized.remove(0);
    }
    Ok(normalized)
}

fn expand_project_term_aliases(
    db: &Database,
    project_id: i64,
    question: &str,
) -> Result<String, AppError> {
    let focused_rule_query = focused_business_rule_query(question);
    let mut terms = focused_rule_query
        .clone()
        .map(|term| vec![term])
        .unwrap_or_else(|| vec![question.trim().to_string()]);
    // “明日工作计划”在当前业务的页面文案中常带“工作”，而 Java 类、方法和注释
    // 统一使用“明日计划”。FTS5 trigram 不会跨越这个插入词，因此补入已确认的
    // 短语映射；不对任意“工作计划”做泛化替换，以免把其他计划概念误召回。
    if focused_rule_query.is_none() {
        for variant in compact_business_query_variants(question) {
            if !terms.iter().any(|term| term == &variant) {
                terms.push(variant);
            }
        }
    }
    for expansion in KnowledgeProjectTerminologyService::expand_query(db, project_id, question)? {
        for alias in expansion.aliases {
            if !terms.iter().any(|term| term.eq_ignore_ascii_case(&alias)) {
                terms.push(alias);
            }
        }
    }
    Ok(terms.join(" "))
}

fn focused_business_rule_query(question: &str) -> Option<String> {
    if !question.contains("明日工作计划") || !question.contains("生成") {
        return None;
    }
    // 仅对已确认的页面/代码同义短语做聚焦。当前生成入口的稳定方法名是
    // `generateTomorrowPlan`；直接检索该符号可避免“生成”一词把检查点、SQL 等
    // 周边实现排在主规则之前。其他“工作计划”概念不会触发该映射。
    Some("generateTomorrowPlan".to_string())
}

fn compact_business_query_variants(question: &str) -> Vec<String> {
    let compact_plan = question.replace("明日工作计划", "明日计划");
    (compact_plan != question)
        .then_some(compact_plan)
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        answer_mode_for_question, expand_project_term_aliases, focused_business_rule_query,
        normalize_conversation, release_requirement_coverage_query,
        should_generate_query_embedding, KnowledgeScopedQuestionService,
    };
    use crate::database::Database;
    use crate::models::knowledge_domain::qa::{
        KnowledgeScopedQuestionInput, PersistKnowledgeQaRoundInput,
    };
    use crate::models::{
        CreateKnowledgeDocumentVersionInput, KnowledgeAskResult, KnowledgeChunkWriteInput,
        KnowledgeConversationMessage, UpsertKnowledgeDocumentInput,
        UpsertKnowledgeEmbeddingProfileInput, UpsertKnowledgeProjectInput,
        UpsertKnowledgeProjectTermInput, UpsertKnowledgeReleaseInput,
    };
    use crate::services::knowledge_domain::terminology::KnowledgeProjectTerminologyService;
    use crate::services::knowledge_embedding::KnowledgeEmbeddingService;

    #[tokio::test]
    async fn evidence_answer_uses_the_requested_historical_project_version(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        database.ensure_knowledge_fts()?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "qa-history".to_string(),
            name: "问答历史版本".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        let old_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: String::new(),
            branch: String::new(),
            commit_sha: String::new(),
            description: String::new(),
            released_at: None,
        })?;
        let new_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v2.0.0".to_string(),
            tag_name: String::new(),
            branch: String::new(),
            commit_sha: String::new(),
            description: String::new(),
            released_at: None,
        })?;
        let document = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "inventory-rule".to_string(),
            project_id: Some(project.id),
            source_id: None,
            doc_type: "markdown".to_string(),
            title: "库存规则".to_string(),
            logical_path: "docs/inventory.md".to_string(),
            sensitivity: "internal".to_string(),
            tags: Vec::new(),
            allow_ai: true,
            allow_mcp: false,
        })?;
        let old_version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: Some(old_release.id),
                version_label: "v1".to_string(),
                git_branch: String::new(),
                commit_sha: "old".to_string(),
                source_path: "docs/inventory.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "历史库存规则：库存不足时拒绝创建订单。".to_string(),
                content_hash: "old-content".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "库存规则".to_string(),
                content: "历史库存规则：库存不足时拒绝创建订单。".to_string(),
                content_hash: "old-chunk".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;
        let new_version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: document.id,
                release_id: Some(new_release.id),
                version_label: "v2".to_string(),
                git_branch: String::new(),
                commit_sha: "new".to_string(),
                source_path: "docs/inventory.md".to_string(),
                mime_type: "text/markdown".to_string(),
                content: "新版库存规则：允许预占库存。".to_string(),
                content_hash: "new-content".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "库存规则".to_string(),
                content: "新版库存规则：允许预占库存。".to_string(),
                content_hash: "new-chunk".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;

        // 即使已有远程 Profile 且没有可用的远程 Provider，“查看本地证据”也必须继续
        // 走 FTS，不应因为向量能力默认可用而把本地证据查询变成远程请求。
        let remote_profile =
            database.upsert_knowledge_embedding_profile(&UpsertKnowledgeEmbeddingProfileInput {
                id: None,
                profile_key: "qa-remote-profile".to_string(),
                name: "问答远程 Profile".to_string(),
                mode: "remote".to_string(),
                provider_key: "unconfigured-remote-provider".to_string(),
                model: "remote-embedding-model".to_string(),
                model_revision: String::new(),
                dimension: 2,
                normalized: true,
                config: serde_json::json!({}),
                fingerprint: "qa-remote-profile-v1".to_string(),
            })?;
        database.begin_knowledge_embedding_profile_build(remote_profile.id)?;
        for version in [&old_version, &new_version] {
            let chunk = database.list_knowledge_chunks(version.id)?[0].clone();
            database.upsert_knowledge_chunk_embedding(
                chunk.id,
                remote_profile.id,
                &chunk.content_hash,
                &[1.0, 0.0],
            )?;
        }
        assert!(
            database
                .complete_knowledge_embedding_profile_build(remote_profile.id)?
                .complete
        );
        database.activate_knowledge_embedding_profile(remote_profile.id)?;
        assert!(KnowledgeEmbeddingService::remote_embedding_enabled(
            &database
        )?);

        let result = KnowledgeScopedQuestionService::ask(
            &database,
            std::path::Path::new("/tmp/unused-knowledge-qa"),
            KnowledgeScopedQuestionInput {
                project_id: project.id,
                project_version_id: old_release.id,
                question: "历史库存规则".to_string(),
                repository_binding_ids: Vec::new(),
                provider_key: String::new(),
                model: String::new(),
                evidence_only: true,
                conversation: Vec::new(),
            },
        )
        .await?;
        assert_eq!(result.citation_validation, "notApplicable");
        assert_eq!(result.citations.len(), 1);
        assert_eq!(
            result.citations[0].document_version_id,
            Some(old_version.id)
        );
        assert!(result.citations[0].excerpt.contains("历史库存规则"));
        Ok(())
    }

    #[test]
    fn evidence_only_never_generates_a_query_embedding() {
        assert!(!should_generate_query_embedding(true));
        assert!(should_generate_query_embedding(false));
    }

    #[test]
    fn conversation_is_bounded_and_keeps_a_user_turn_at_the_front() {
        let messages = (0..20)
            .map(|index| KnowledgeConversationMessage {
                role: if index % 2 == 0 {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                },
                content: format!("消息 {index}"),
            })
            .collect();
        let normalized = normalize_conversation(messages).expect("历史消息应被截断");
        assert!(normalized.len() <= 12);
        assert_eq!(
            normalized.first().map(|item| item.role.as_str()),
            Some("user")
        );
        assert_eq!(
            normalized.last().map(|item| item.role.as_str()),
            Some("assistant")
        );
    }

    #[test]
    fn conversation_rejects_unknown_roles() {
        let error = normalize_conversation(vec![KnowledgeConversationMessage {
            role: "system".to_string(),
            content: "不应由前端注入系统消息".to_string(),
        }])
        .expect_err("未知角色必须被拒绝");
        assert!(error.to_string().contains("user 或 assistant"));
    }

    #[test]
    fn saved_round_can_be_restored_and_soft_deleted() -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "qa-session".to_string(),
            name: "会话持久化".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        let release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: String::new(),
            branch: String::new(),
            commit_sha: "fixed-sha".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let detail = KnowledgeScopedQuestionService::persist_round(
            &database,
            PersistKnowledgeQaRoundInput {
                session_id: None,
                project_id: project.id,
                project_version_id: release.id,
                provider_key: "chat".to_string(),
                model: "chat-model".to_string(),
                question: "库存不足怎么办？".to_string(),
                answer: KnowledgeAskResult {
                    answer: "拒绝创建订单。".to_string(),
                    citation_validation: "verified".to_string(),
                    citations: Vec::new(),
                    conflicts: Vec::new(),
                    evidence_gaps: Vec::new(),
                    retrieval_diagnostics: serde_json::json!({}),
                },
                evidence_only: false,
            },
        )?;
        assert_eq!(detail.messages.len(), 2);
        assert_eq!(detail.session.message_count, 2);
        assert_eq!(detail.messages[0].role, "user");
        assert_eq!(detail.messages[1].role, "assistant");
        assert_eq!(
            detail.messages[1]
                .answer
                .as_ref()
                .map(|answer| answer.answer.as_str()),
            Some("拒绝创建订单。")
        );

        let restored =
            KnowledgeScopedQuestionService::get_session(&database, project.id, detail.session.id)?;
        assert_eq!(restored.messages.len(), 2);
        KnowledgeScopedQuestionService::delete_session(&database, project.id, detail.session.id)?;
        assert!(KnowledgeScopedQuestionService::list_sessions(&database, project.id)?.is_empty());
        Ok(())
    }

    #[test]
    fn confirmed_project_aliases_extend_a_natural_language_question(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "qa-terminology".to_string(),
            name: "项目术语问答".to_string(),
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
                term: "明日工作计划".to_string(),
                aliases: vec!["tomorrowWorkPlan".to_string()],
                confirmation_note: "已由项目维护者确认对应代码模块".to_string(),
                created_by: Some("测试用户".to_string()),
            },
        )?;

        let query = expand_project_term_aliases(
            &database,
            project.id,
            "全业务工单中，明日工作计划生成的逻辑是什么？",
        )?;

        assert!(query.starts_with("generateTomorrowPlan"));
        assert!(query.contains("tomorrowWorkPlan"));
        Ok(())
    }

    #[test]
    fn focused_business_rule_query_removes_project_noise() {
        assert_eq!(
            focused_business_rule_query("全业务工单的明日工作计划的生成规则是什么？"),
            Some("generateTomorrowPlan".to_string())
        );
        assert_eq!(
            focused_business_rule_query("生成明日工作计划的规则"),
            Some("generateTomorrowPlan".to_string())
        );
        assert_eq!(
            focused_business_rule_query(
                "全业务工单中明日工作计划生成时，工单进入计划的逻辑是什么？"
            ),
            Some("generateTomorrowPlan".to_string())
        );
        assert_eq!(focused_business_rule_query("明日工作计划如何确认？"), None);
        assert_eq!(focused_business_rule_query("系统能否生成工作计划？"), None);
    }

    #[test]
    fn release_coverage_questions_use_the_dedicated_answer_mode() {
        assert_eq!(
            answer_mode_for_question("v1.2.0 实现了哪些需求，还有哪些未实现？").as_deref(),
            Some("releaseRequirementCoverage")
        );
        assert_eq!(
            answer_mode_for_question("当前版本完成了哪些功能？").as_deref(),
            Some("releaseRequirementCoverage")
        );
        assert_eq!(answer_mode_for_question("明日计划如何生成？"), None);
    }

    #[test]
    fn release_coverage_query_reuses_only_the_previous_user_requirement_scope() {
        let query = release_requirement_coverage_query(
            "现在实现了哪些需求？",
            "全业务工单中心",
            "v1.2.0",
            &[
                KnowledgeConversationMessage {
                    role: "user".to_string(),
                    content: "全业务工单 v1.2.0 的需求是什么？".to_string(),
                },
                KnowledgeConversationMessage {
                    role: "assistant".to_string(),
                    content: "模型曾经猜测了一个不存在的功能".to_string(),
                },
            ],
        );

        assert!(query.contains("全业务工单 v1.2.0 的需求是什么？"));
        assert!(query.contains("v1.2.0"));
        assert!(!query.contains("不存在的功能"));
    }

    #[tokio::test]
    async fn natural_language_plan_question_recalls_the_version_scoped_generator_code(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        database.ensure_knowledge_fts()?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "qa-plan-generation".to_string(),
            name: "计划生成问答".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        let release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v1.0.0".to_string(),
            tag_name: String::new(),
            branch: String::new(),
            commit_sha: "generator-commit".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let later_release = database.upsert_knowledge_release(&UpsertKnowledgeReleaseInput {
            id: None,
            project_id: project.id,
            version: "v2.0.0".to_string(),
            tag_name: String::new(),
            branch: String::new(),
            commit_sha: "later-generator-commit".to_string(),
            description: String::new(),
            released_at: None,
        })?;
        let generator = database.upsert_knowledge_document(&UpsertKnowledgeDocumentInput {
            id: None,
            document_key: "tomorrow-plan-generator".to_string(),
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
        let version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: generator.id,
                release_id: Some(release.id),
                version_label: "v1.0.0".to_string(),
                git_branch: "release/v1.0.0".to_string(),
                commit_sha: "generator-commit".to_string(),
                source_path: "provider/TomorrowPlanApiImpl.java".to_string(),
                mime_type: "text/plain".to_string(),
                content: "public void generateTomorrowPlan() { /* 明日计划生成 */ }".to_string(),
                content_hash: "tomorrow-plan-generator-v1".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "TomorrowPlanApiImpl#generateTomorrowPlan".to_string(),
                content: "generateTomorrowPlan：执行明日计划生成。".to_string(),
                content_hash: "tomorrow-plan-generator-chunk-v1".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;
        let later_version = database.create_knowledge_document_version(
            &CreateKnowledgeDocumentVersionInput {
                document_id: generator.id,
                release_id: Some(later_release.id),
                version_label: "v2.0.0".to_string(),
                git_branch: "release/v2.0.0".to_string(),
                commit_sha: "later-generator-commit".to_string(),
                source_path: "provider/TomorrowPlanApiImpl.java".to_string(),
                mime_type: "text/plain".to_string(),
                content: "public void generateTomorrowPlan() { /* 新版明日计划生成 */ }"
                    .to_string(),
                content_hash: "tomorrow-plan-generator-v2".to_string(),
                parsed_meta: serde_json::json!({}),
                token_estimate: 10,
            },
            &[KnowledgeChunkWriteInput {
                chunk_index: 0,
                heading_path: "TomorrowPlanApiImpl#generateTomorrowPlan".to_string(),
                content: "新版 generateTomorrowPlan：执行明日计划生成。".to_string(),
                content_hash: "tomorrow-plan-generator-chunk-v2".to_string(),
                location: serde_json::json!({"startLine": 1, "endLine": 1}),
                token_estimate: 10,
            }],
        )?;

        let result = KnowledgeScopedQuestionService::ask(
            &database,
            std::path::Path::new("/tmp/unused-knowledge-qa"),
            KnowledgeScopedQuestionInput {
                project_id: project.id,
                project_version_id: release.id,
                question: "明日工作计划的生成规则是什么？".to_string(),
                repository_binding_ids: Vec::new(),
                provider_key: String::new(),
                model: String::new(),
                evidence_only: true,
                conversation: Vec::new(),
            },
        )
        .await?;

        assert_eq!(result.citations.len(), 1);
        assert_eq!(result.citations[0].document_version_id, Some(version.id));
        assert_eq!(result.citations[0].release_id, Some(release.id));
        assert_ne!(
            result.citations[0].document_version_id,
            Some(later_version.id)
        );
        assert_eq!(result.citations[0].title, "TomorrowPlanApiImpl.java");
        assert!(result.citations[0]
            .heading_path
            .contains("generateTomorrowPlan"));
        Ok(())
    }
}
