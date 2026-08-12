use crate::database::knowledge_domain::search::normalize_knowledge_title;
use crate::database::Database;
use crate::error::AppError;
use crate::models::knowledge_domain::terminology::{
    KnowledgeProjectTerm, KnowledgeProjectTermExpansion, UpsertKnowledgeProjectTermInput,
};
use crate::services::knowledge::{audit_knowledge, required_text, validate_positive_id};
use crate::services::knowledge_rollout::KnowledgeRolloutService;

const MAX_TERM_LENGTH: usize = 80;
const MAX_ALIAS_LENGTH: usize = 120;
const MAX_ALIAS_COUNT: usize = 12;

/// 项目术语是人工确认的检索辅助信息，不生成翻译，也不从 AI 或其他项目自动学习映射。
pub struct KnowledgeProjectTerminologyService;

impl KnowledgeProjectTerminologyService {
    pub fn list(db: &Database, project_id: i64) -> Result<Vec<KnowledgeProjectTerm>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        Self::ensure_project(db, project_id)?;
        db.list_knowledge_project_terms(project_id)
    }

    pub fn upsert(
        db: &Database,
        mut input: UpsertKnowledgeProjectTermInput,
    ) -> Result<KnowledgeProjectTerm, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        Self::ensure_project(db, input.project_id)?;
        if let Some(id) = input.id {
            validate_positive_id(id, "项目术语 ID")?;
            let existing = db
                .get_knowledge_project_term(id)?
                .ok_or_else(|| AppError::NotFound(format!("项目术语不存在: {id}")))?;
            if existing.project_id != input.project_id {
                return Err(AppError::InvalidInput("项目术语不属于当前项目".to_string()));
            }
        }
        input.term = required_text(&input.term, "用户术语")?;
        if input.term.chars().count() > MAX_TERM_LENGTH {
            return Err(AppError::InvalidInput(
                "用户术语不能超过 80 个字符".to_string(),
            ));
        }
        let normalized_term = normalize_knowledge_title(&input.term);
        if normalized_term.is_empty() {
            return Err(AppError::InvalidInput("用户术语不能为空".to_string()));
        }
        input.aliases = normalize_aliases(input.aliases)?;
        if input.aliases.is_empty() {
            return Err(AppError::InvalidInput(
                "请至少填写一个代码或业务别名".to_string(),
            ));
        }
        input.confirmation_note = required_text(&input.confirmation_note, "确认说明")?;
        if input.confirmation_note.chars().count() > 500 {
            return Err(AppError::InvalidInput(
                "确认说明不能超过 500 个字符".to_string(),
            ));
        }
        let created_by = input
            .created_by
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("本地用户");
        if created_by.chars().count() > 80 {
            return Err(AppError::InvalidInput(
                "确认人不能超过 80 个字符".to_string(),
            ));
        }
        let aliases_json = serde_json::to_string(&input.aliases)?;
        let result =
            db.upsert_knowledge_project_term(&input, &normalized_term, &aliases_json, created_by)?;
        audit_knowledge(
            db,
            "knowledge_project_term_upsert",
            "L1",
            "成功",
            "保存项目术语映射",
            serde_json::json!({"projectId": result.project_id, "termId": result.id}),
        );
        Ok(result)
    }

    pub fn delete(db: &Database, project_id: i64, id: i64) -> Result<(), AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        Self::ensure_project(db, project_id)?;
        validate_positive_id(id, "项目术语 ID")?;
        let term = db
            .get_knowledge_project_term(id)?
            .ok_or_else(|| AppError::NotFound(format!("项目术语不存在: {id}")))?;
        if term.project_id != project_id {
            return Err(AppError::InvalidInput("项目术语不属于当前项目".to_string()));
        }
        db.soft_delete_knowledge_project_term(id)?;
        audit_knowledge(
            db,
            "knowledge_project_term_delete",
            "L2",
            "成功",
            "删除项目术语映射",
            serde_json::json!({"projectId": term.project_id, "termId": id}),
        );
        Ok(())
    }

    pub(crate) fn expand_query(
        db: &Database,
        project_id: i64,
        query: &str,
    ) -> Result<Vec<KnowledgeProjectTermExpansion>, AppError> {
        let normalized_query = normalize_knowledge_title(query);
        if normalized_query.is_empty() {
            return Ok(Vec::new());
        }
        Ok(db
            .list_knowledge_project_terms(project_id)?
            .into_iter()
            .filter_map(|term| {
                normalized_query
                    .contains(&normalize_knowledge_title(&term.term))
                    .then_some(KnowledgeProjectTermExpansion {
                        term: term.term,
                        aliases: term.aliases,
                    })
            })
            .collect())
    }

    fn ensure_project(db: &Database, project_id: i64) -> Result<(), AppError> {
        validate_positive_id(project_id, "项目 ID")?;
        if !db.knowledge_project_exists(project_id)? {
            return Err(AppError::NotFound(format!("知识项目不存在: {project_id}")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::KnowledgeProjectTerminologyService;
    use crate::database::Database;
    use crate::models::{UpsertKnowledgeProjectInput, UpsertKnowledgeProjectTermInput};

    #[test]
    fn confirmed_terms_are_project_scoped_and_soft_delete_stops_expansion(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "term-project".to_string(),
            name: "术语项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        let other_project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "other-term-project".to_string(),
            name: "其他术语项目".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: String::new(),
            enabled: true,
        })?;
        let term = KnowledgeProjectTerminologyService::upsert(
            &database,
            UpsertKnowledgeProjectTermInput {
                id: None,
                project_id: project.id,
                term: "工单".to_string(),
                aliases: vec!["WorkOrder".to_string(), "work_order".to_string()],
                confirmation_note: "已由项目负责人确认工单对应代码模型。".to_string(),
                created_by: Some("测试用户".to_string()),
            },
        )?;
        let expansions = KnowledgeProjectTerminologyService::expand_query(
            &database,
            project.id,
            "查询工单状态",
        )?;
        assert_eq!(expansions.len(), 1);
        assert_eq!(expansions[0].aliases, vec!["WorkOrder", "work_order"]);
        assert!(KnowledgeProjectTerminologyService::expand_query(
            &database,
            other_project.id,
            "查询工单状态",
        )?
        .is_empty());
        assert!(KnowledgeProjectTerminologyService::upsert(
            &database,
            UpsertKnowledgeProjectTermInput {
                id: None,
                project_id: project.id,
                term: "订单".to_string(),
                aliases: vec!["Order".to_string()],
                confirmation_note: " ".to_string(),
                created_by: None,
            },
        )
        .is_err());
        KnowledgeProjectTerminologyService::delete(&database, project.id, term.id)?;
        assert!(KnowledgeProjectTerminologyService::expand_query(
            &database,
            project.id,
            "查询工单状态",
        )?
        .is_empty());
        Ok(())
    }
}

fn normalize_aliases(values: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut aliases = Vec::new();
    for raw in values {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > MAX_ALIAS_LENGTH {
            return Err(AppError::InvalidInput(
                "代码或业务别名不能超过 120 个字符".to_string(),
            ));
        }
        if !aliases
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(value))
        {
            aliases.push(value.to_string());
        }
    }
    if aliases.len() > MAX_ALIAS_COUNT {
        return Err(AppError::InvalidInput(
            "最多可填写 12 个代码或业务别名".to_string(),
        ));
    }
    Ok(aliases)
}
