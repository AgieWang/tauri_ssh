use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AiExperience, AiExperienceMatch, AiExperienceRecallInput, AiRunbook, AiRunbookRunResult,
    AiRunbookStep, AiRunbookStepResult, AiSkill, AiSkillMatch, AiSkillPromptPreviewInput,
    AiSkillPromptPreviewResult, AiSkillTriggerInput, AiSkillTriggerResult, ListAiSkillsInput,
    ListAiSkillsResult, RunAiRunbookInput, SyncBuiltinAiSkillsResult, UpsertAiExperienceInput,
    UpsertAiRunbookInput, UpsertAiSkillInput,
};
use crate::services::approval::ApprovalService;
use crate::services::database_ops::DatabaseOpsService;
use crate::services::sftp::SftpService;
use crate::services::terminal::TerminalService;

pub struct AiSkillService;

struct ParsedSkill {
    key: String,
    name: String,
    description: String,
    content: String,
    trigger_words: Vec<String>,
    tags: Vec<String>,
    scopes: Vec<String>,
    priority: i64,
    source_path: String,
    hash: String,
}

impl AiSkillService {
    pub fn sync_builtin(
        app: &AppHandle,
        db: &Database,
    ) -> Result<SyncBuiltinAiSkillsResult, AppError> {
        let skills_dir = Self::resolve_builtin_skills_dir(app)?;
        let mut scanned = 0;
        let mut inserted = 0;
        let mut updated = 0;
        let mut active_paths = Vec::new();

        if !skills_dir.exists() {
            return Err(AppError::NotFound(format!(
                "内置 Skill 资源目录不存在: {}",
                skills_dir.display()
            )));
        }

        for entry in fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let skill_path = entry.path().join("SKILL.md");
            if !skill_path.exists() {
                continue;
            }
            scanned += 1;
            let parsed = Self::parse_skill_file(&skills_dir, &skill_path)?;
            active_paths.push(parsed.source_path.clone());
            match db.upsert_builtin_ai_skill(
                &parsed.key,
                &parsed.name,
                &parsed.description,
                &parsed.content,
                &parsed.scopes,
                &parsed.trigger_words,
                &parsed.tags,
                parsed.priority,
                &parsed.source_path,
                &parsed.hash,
            )? {
                action if action == "inserted" => inserted += 1,
                action if action == "updated" => updated += 1,
                _ => {}
            }
        }

        let missing = db.mark_missing_builtin_ai_skills(&active_paths)?;
        Ok(SyncBuiltinAiSkillsResult {
            scanned,
            inserted,
            updated,
            missing,
        })
    }

    pub fn list(db: &Database, input: ListAiSkillsInput) -> Result<ListAiSkillsResult, AppError> {
        let items = db.list_ai_skills(&input)?;
        let stats = db.ai_skill_stats()?;
        Ok(ListAiSkillsResult { items, stats })
    }

    pub fn upsert(db: &Database, input: UpsertAiSkillInput) -> Result<AiSkill, AppError> {
        Self::validate_skill_input(&input)?;
        db.upsert_user_ai_skill(&input)
    }

    pub fn set_enabled(db: &Database, id: i64, enabled: bool) -> Result<AiSkill, AppError> {
        db.set_ai_skill_enabled(id, enabled)
    }

    pub fn copy_skill(db: &Database, id: i64) -> Result<AiSkill, AppError> {
        let skill = db
            .get_ai_skill_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("Skill {} 不存在", id)))?;
        let input = UpsertAiSkillInput {
            id: None,
            skill_key: Some(format!(
                "{}-copy-{}",
                skill.skill_key,
                chrono::Utc::now().timestamp_millis()
            )),
            name: format!("{} 副本", skill.name),
            description: Some(skill.description),
            content: skill.content,
            scopes: skill.scopes,
            trigger_words: Some(skill.trigger_words),
            tags: Some(skill.tags),
            priority: Some(skill.priority),
            enabled: Some(false),
            allow_mcp: Some(skill.allow_mcp),
        };
        db.upsert_user_ai_skill(&input)
    }

    pub fn delete(db: &Database, id: i64) -> Result<(), AppError> {
        db.delete_ai_skill(id)
    }

    pub fn restore_builtin(db: &Database, id: i64) -> Result<AiSkill, AppError> {
        db.restore_builtin_ai_skill(id)
    }

    pub fn test_trigger(
        db: &Database,
        input: AiSkillTriggerInput,
    ) -> Result<AiSkillTriggerResult, AppError> {
        let scope = input
            .scope
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("all")
            .to_string();
        let matches = Self::resolve_matches(
            db,
            input.prompt.as_str(),
            &scope,
            input.include_global.unwrap_or(true),
            false,
        )?;
        let experiences = Self::recall_experiences(
            db,
            AiExperienceRecallInput {
                prompt: input.prompt.clone(),
                scope: Some(scope.clone()),
                limit: Some(5),
            },
        )?;
        Ok(AiSkillTriggerResult {
            prompt: input.prompt,
            scope,
            matches,
            experiences,
        })
    }

    pub fn prompt_preview(
        db: &Database,
        input: AiSkillPromptPreviewInput,
    ) -> Result<AiSkillPromptPreviewResult, AppError> {
        let skills = Self::resolve_prompt_skills(
            db,
            input.prompt.as_deref().unwrap_or(""),
            &input.scope,
            input.include_global.unwrap_or(true),
        )?;
        let experiences = Self::recall_experiences(
            db,
            AiExperienceRecallInput {
                prompt: input.prompt.clone().unwrap_or_default(),
                scope: Some(input.scope.clone()),
                limit: Some(5),
            },
        )?;
        let prompt_fragment = Self::build_prompt_fragment(&skills, &experiences);
        Ok(AiSkillPromptPreviewResult {
            scope: input.scope,
            skills,
            experiences,
            prompt_fragment,
        })
    }

    pub fn build_prompt_for_ai(
        db: &Database,
        scope: &str,
        prompt: &str,
    ) -> Result<String, AppError> {
        let skills = Self::resolve_prompt_skills(db, prompt, scope, true)?;
        let experiences = Self::recall_experiences(
            db,
            AiExperienceRecallInput {
                prompt: prompt.to_string(),
                scope: Some(scope.to_string()),
                limit: Some(5),
            },
        )?;
        Ok(Self::build_prompt_fragment(&skills, &experiences))
    }

    pub fn list_experiences(
        db: &Database,
        keyword: Option<String>,
    ) -> Result<Vec<AiExperience>, AppError> {
        db.list_ai_experiences(keyword.as_deref())
    }

    pub fn recall_experiences(
        db: &Database,
        input: AiExperienceRecallInput,
    ) -> Result<Vec<AiExperienceMatch>, AppError> {
        let prompt = input.prompt.trim();
        let scope = input
            .scope
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("all");
        let limit = input.limit.unwrap_or(5).clamp(1, 10) as usize;
        if prompt.is_empty() {
            return Ok(Vec::new());
        }

        let prompt_words = Self::tokenize_recall_text(prompt);
        let lowered_prompt = prompt.to_lowercase();
        let mut matches = db
            .list_ai_experiences(None)?
            .into_iter()
            .filter(|experience| {
                experience.enabled
                    && (scope == "all"
                        || experience.scenario == scope
                        || experience.tags.iter().any(|tag| tag == scope))
            })
            .filter_map(|experience| {
                let searchable = format!(
                    "{}\n{}\n{}\n{}\n{}\n{}",
                    experience.title,
                    experience.symptom,
                    experience.cause,
                    experience.solution,
                    experience.scenario,
                    experience.tags.join(" ")
                )
                .to_lowercase();
                let mut matched_words = prompt_words
                    .iter()
                    .filter(|word| searchable.contains(word.as_str()))
                    .cloned()
                    .collect::<Vec<_>>();
                matched_words.sort();
                matched_words.dedup();

                let mut score = matched_words.len() as i64 * 100;
                if !experience.scenario.is_empty() && experience.scenario == scope {
                    score += 60;
                }
                if lowered_prompt.contains(&experience.title.to_lowercase()) {
                    score += 80;
                }
                if experience
                    .tags
                    .iter()
                    .any(|tag| !tag.is_empty() && lowered_prompt.contains(&tag.to_lowercase()))
                {
                    score += 40;
                }
                if score <= 0 {
                    return None;
                }
                let summary = Self::experience_summary(&experience);
                Some(AiExperienceMatch {
                    experience,
                    matched_words,
                    score,
                    summary,
                })
            })
            .collect::<Vec<_>>();

        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.experience.updated_at.cmp(&a.experience.updated_at))
        });
        matches.truncate(limit);
        Ok(matches)
    }

    pub fn upsert_experience(
        app: &AppHandle,
        db: &Database,
        mut input: UpsertAiExperienceInput,
    ) -> Result<AiExperience, AppError> {
        if input.title.trim().is_empty() {
            return Err(AppError::InvalidInput("经验标题不能为空".into()));
        }
        let experience_key = input
            .experience_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("exp-{}", chrono::Utc::now().timestamp_millis()));
        input.experience_key = Some(experience_key.clone());

        let markdown_path = Self::write_experience_markdown(app, &experience_key, &input)?;
        input.markdown_path = Some(markdown_path.to_string_lossy().into_owned());
        input.references_json = Some(serde_json::to_string(&serde_json::json!([
            {
                "type": "markdown_file",
                "path": markdown_path.to_string_lossy(),
            }
        ]))?);

        db.upsert_ai_experience(&input)
    }

    pub fn delete_experience(db: &Database, id: i64) -> Result<(), AppError> {
        db.delete_ai_experience(id)
    }

    pub fn list_runbooks(
        db: &Database,
        keyword: Option<String>,
    ) -> Result<Vec<AiRunbook>, AppError> {
        db.list_ai_runbooks(keyword.as_deref())
    }

    pub fn upsert_runbook(
        db: &Database,
        input: UpsertAiRunbookInput,
    ) -> Result<AiRunbook, AppError> {
        if input.name.trim().is_empty() {
            return Err(AppError::InvalidInput("Runbook 名称不能为空".into()));
        }
        db.upsert_ai_runbook(&input)
    }

    pub fn delete_runbook(db: &Database, id: i64) -> Result<(), AppError> {
        db.delete_ai_runbook(id)
    }

    pub async fn run_runbook(
        db: &Database,
        input: RunAiRunbookInput,
    ) -> Result<AiRunbookRunResult, AppError> {
        let runbook = Self::resolve_runbook(db, &input)?;
        if !runbook.enabled {
            return Err(AppError::InvalidInput("Runbook 已禁用".into()));
        }
        let dry_run = input.dry_run.unwrap_or(false);
        let mut results = Vec::new();
        let mut has_error = false;
        let mut has_approval = false;

        for step in &runbook.steps {
            let result = Self::run_runbook_step(db, &runbook, step, &input, dry_run).await;
            match result {
                Ok(item) => {
                    has_error |= item.status == "error" || item.status == "blocked";
                    has_approval |= item.status == "approval_required";
                    results.push(item);
                }
                Err(error) => {
                    has_error = true;
                    results.push(AiRunbookStepResult {
                        step_id: step.id.clone(),
                        title: step.title.clone(),
                        step_type: step.step_type.clone(),
                        risk_level: step.risk_level.clone(),
                        status: "error".into(),
                        message: error.to_string(),
                        output: serde_json::json!({}),
                        approval_id: None,
                        duration_ms: 0,
                    });
                    break;
                }
            }
        }

        let status = if has_error {
            "error"
        } else if has_approval {
            "approval_required"
        } else if dry_run {
            "planned"
        } else {
            "success"
        }
        .to_string();
        let message = match status.as_str() {
            "error" => "Runbook 执行中断，请查看失败步骤".into(),
            "approval_required" => "Runbook 已执行可自动步骤，并创建待审批步骤".into(),
            "planned" => "Runbook 预演完成，未执行真实动作".into(),
            _ => "Runbook 执行完成".into(),
        };
        Ok(AiRunbookRunResult {
            runbook,
            status,
            message,
            steps: results,
        })
    }

    fn resolve_prompt_skills(
        db: &Database,
        prompt: &str,
        scope: &str,
        include_global: bool,
    ) -> Result<Vec<AiSkill>, AppError> {
        let matches = Self::resolve_matches(db, prompt, scope, include_global, true)?;
        Ok(matches.into_iter().map(|item| item.skill).collect())
    }

    pub(crate) fn resolve_runbook(
        db: &Database,
        input: &RunAiRunbookInput,
    ) -> Result<AiRunbook, AppError> {
        let items = db.list_ai_runbooks(None)?;
        if let Some(id) = input.id {
            return items
                .into_iter()
                .find(|item| item.id == id)
                .ok_or_else(|| AppError::NotFound(format!("Runbook {} 不存在", id)));
        }
        if let Some(key) = input
            .runbook_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return items
                .into_iter()
                .find(|item| item.runbook_key == key)
                .ok_or_else(|| AppError::NotFound(format!("Runbook '{}' 不存在", key)));
        }
        Err(AppError::InvalidInput("请选择要执行的 Runbook".into()))
    }

    async fn run_runbook_step(
        db: &Database,
        runbook: &AiRunbook,
        step: &AiRunbookStep,
        input: &RunAiRunbookInput,
        dry_run: bool,
    ) -> Result<AiRunbookStepResult, AppError> {
        let started = std::time::Instant::now();
        if step.risk_level == "blocked" {
            return Ok(Self::step_result(
                step,
                "blocked",
                "步骤风险级别为禁止，未执行",
                serde_json::json!({}),
                None,
                started,
            ));
        }
        if dry_run {
            return Ok(Self::step_result(
                step,
                "planned",
                "预演模式：未执行真实动作",
                serde_json::json!({ "content": step.content }),
                None,
                started,
            ));
        }

        match step.step_type.as_str() {
            "note" => Ok(Self::step_result(
                step,
                "success",
                "说明步骤已跳过执行",
                serde_json::json!({ "note": step.content }),
                None,
                started,
            )),
            "readonly_command" => {
                let command = Self::content_string(&step.content, "command")
                    .unwrap_or_else(|| step.content.trim().to_string());
                Self::validate_readonly_command(&command)?;
                let server_alias = Self::content_string(&step.content, "serverAlias")
                    .or_else(|| input.server_alias.clone())
                    .ok_or_else(|| AppError::InvalidInput("命令步骤缺少 serverAlias".into()))?;
                let timeout_secs = Self::content_u64(&step.content, "timeoutSecs").or(Some(30));
                let result = TerminalService::execute(
                    db,
                    crate::models::TerminalCommandInput {
                        server_alias,
                        command,
                        timeout_secs,
                        initiated_by_ai: None,
                    },
                )
                .await?;
                let message = result.message.clone();
                Ok(Self::step_result(
                    step,
                    if result.blocked { "blocked" } else { "success" },
                    &message,
                    serde_json::to_value(result)?,
                    None,
                    started,
                ))
            }
            "approval_command" => {
                let command = Self::content_string(&step.content, "command")
                    .unwrap_or_else(|| step.content.trim().to_string());
                let server_alias = Self::content_string(&step.content, "serverAlias")
                    .or_else(|| input.server_alias.clone())
                    .unwrap_or_default();
                let approval = ApprovalService::create(
                    db,
                    crate::models::CreateApprovalRequestInput {
                        source: "runbook".into(),
                        requester: input
                            .requester
                            .clone()
                            .unwrap_or_else(|| "local-user".into()),
                        server_alias: server_alias.clone(),
                        action: "terminal_execute".into(),
                        risk: step.risk_level.clone(),
                        command: command.clone(),
                        resource: String::new(),
                        reason: format!("Runbook '{}' 步骤 '{}'", runbook.name, step.title),
                        summary: format!("执行远程命令：{}", command),
                        payload_json: Some(
                            serde_json::json!({
                                "runbookId": runbook.id,
                                "stepId": step.id,
                                "serverAlias": server_alias,
                                "command": command
                            })
                            .to_string(),
                        ),
                        expires_at: None,
                    },
                )?;
                let approval_id = approval.id;
                Ok(Self::step_result(
                    step,
                    "approval_required",
                    "已创建命令审批请求",
                    serde_json::to_value(approval)?,
                    Some(approval_id),
                    started,
                ))
            }
            "sql" => Self::run_sql_step(db, step, input, started).await,
            "redis" => Self::run_redis_step(db, step, input, started).await,
            "file" => Self::run_file_step(db, runbook, step, input, started),
            _ => Ok(Self::step_result(
                step,
                "error",
                "不支持的 Runbook 步骤类型",
                serde_json::json!({ "stepType": step.step_type }),
                None,
                started,
            )),
        }
    }

    async fn run_sql_step(
        db: &Database,
        step: &AiRunbookStep,
        input: &RunAiRunbookInput,
        started: std::time::Instant,
    ) -> Result<AiRunbookStepResult, AppError> {
        let sql = Self::content_string(&step.content, "sql")
            .unwrap_or_else(|| step.content.trim().to_string());
        let connection_key = Self::content_string(&step.content, "connectionKey")
            .or_else(|| input.database_connection_key.clone())
            .ok_or_else(|| AppError::InvalidInput("SQL 步骤缺少 connectionKey".into()))?;
        let database_name = Self::content_string(&step.content, "databaseName")
            .or_else(|| input.database_name.clone());
        if !Self::is_readonly_sql(&sql) || matches!(step.risk_level.as_str(), "high" | "blocked") {
            let approval = ApprovalService::create(
                db,
                crate::models::CreateApprovalRequestInput {
                    source: "runbook".into(),
                    requester: input
                        .requester
                        .clone()
                        .unwrap_or_else(|| "local-user".into()),
                    server_alias: String::new(),
                    action: "database_execute".into(),
                    risk: step.risk_level.clone(),
                    command: sql.clone(),
                    resource: connection_key.clone(),
                    reason: format!("Runbook SQL 步骤 '{}'", step.title),
                    summary: format!("执行数据库 SQL：{}", Self::trim_content(&sql, 160)),
                    payload_json: Some(
                        serde_json::json!({
                            "connectionKey": connection_key,
                            "databaseName": database_name,
                            "sql": sql
                        })
                        .to_string(),
                    ),
                    expires_at: None,
                },
            )?;
            let approval_id = approval.id;
            return Ok(Self::step_result(
                step,
                "approval_required",
                "SQL 非只读或风险较高，已创建审批请求",
                serde_json::to_value(approval)?,
                Some(approval_id),
                started,
            ));
        }
        let result = DatabaseOpsService::execute_sql(
            db,
            crate::models::DatabaseQueryInput {
                connection_key,
                database_name,
                sql,
                page: Self::content_i64(&step.content, "page").or(Some(1)),
                page_size: Self::content_i64(&step.content, "pageSize").or(Some(100)),
            },
        )
        .await?;
        let message = result.message.clone();
        Ok(Self::step_result(
            step,
            "success",
            &message,
            serde_json::to_value(result)?,
            None,
            started,
        ))
    }

    async fn run_redis_step(
        db: &Database,
        step: &AiRunbookStep,
        input: &RunAiRunbookInput,
        started: std::time::Instant,
    ) -> Result<AiRunbookStepResult, AppError> {
        let connection_key = Self::content_string(&step.content, "connectionKey")
            .or_else(|| input.database_connection_key.clone())
            .ok_or_else(|| AppError::InvalidInput("Redis 步骤缺少 connectionKey".into()))?;
        let database_name = Self::content_string(&step.content, "databaseName")
            .or_else(|| input.database_name.clone());
        if let Some(key) = Self::content_string(&step.content, "key") {
            let result = DatabaseOpsService::get_redis_value_preview(
                db,
                crate::models::RedisValuePreviewInput {
                    connection_key,
                    database_name,
                    key,
                },
            )
            .await?;
            return Ok(Self::step_result(
                step,
                "success",
                "Redis Key 预览完成",
                serde_json::to_value(result)?,
                None,
                started,
            ));
        }
        let result = DatabaseOpsService::scan_redis_keys(
            db,
            crate::models::RedisScanInput {
                connection_key,
                database_name,
                pattern: Self::content_string(&step.content, "pattern").or(Some("*".into())),
                cursor: Self::content_u64(&step.content, "cursor"),
                count: Self::content_i64(&step.content, "count").or(Some(100)),
            },
        )
        .await?;
        Ok(Self::step_result(
            step,
            "success",
            "Redis Key 扫描完成",
            serde_json::to_value(result)?,
            None,
            started,
        ))
    }

    fn run_file_step(
        db: &Database,
        runbook: &AiRunbook,
        step: &AiRunbookStep,
        input: &RunAiRunbookInput,
        started: std::time::Instant,
    ) -> Result<AiRunbookStepResult, AppError> {
        let operation =
            Self::content_string(&step.content, "operation").unwrap_or_else(|| "read".into());
        let server_alias = Self::content_string(&step.content, "serverAlias")
            .or_else(|| input.server_alias.clone())
            .ok_or_else(|| AppError::InvalidInput("文件步骤缺少 serverAlias".into()))?;
        let path = Self::content_string(&step.content, "path")
            .ok_or_else(|| AppError::InvalidInput("文件步骤缺少 path".into()))?;
        match operation.as_str() {
            "list" => {
                let result =
                    SftpService::list(db, crate::models::SftpListInput { server_alias, path })?;
                Ok(Self::step_result(
                    step,
                    "success",
                    "远程目录读取完成",
                    serde_json::to_value(result)?,
                    None,
                    started,
                ))
            }
            "read" | "read_text" => {
                let result = SftpService::read_text(
                    db,
                    crate::models::SftpReadTextInput {
                        server_alias,
                        path,
                        max_bytes: Self::content_u64(&step.content, "maxBytes"),
                    },
                )?;
                Ok(Self::step_result(
                    step,
                    "success",
                    "远程文本读取完成",
                    serde_json::to_value(result)?,
                    None,
                    started,
                ))
            }
            _ => {
                let approval = ApprovalService::create(
                    db,
                    crate::models::CreateApprovalRequestInput {
                        source: "runbook".into(),
                        requester: input
                            .requester
                            .clone()
                            .unwrap_or_else(|| "local-user".into()),
                        server_alias: server_alias.clone(),
                        action: format!("sftp_{}", operation),
                        risk: step.risk_level.clone(),
                        command: String::new(),
                        resource: path.clone(),
                        reason: format!("Runbook '{}' 文件步骤 '{}'", runbook.name, step.title),
                        summary: format!("SFTP {}：{}", operation, path),
                        payload_json: Some(step.content.clone()),
                        expires_at: None,
                    },
                )?;
                let approval_id = approval.id;
                Ok(Self::step_result(
                    step,
                    "approval_required",
                    "文件写入/变更类动作已创建审批请求",
                    serde_json::to_value(approval)?,
                    Some(approval_id),
                    started,
                ))
            }
        }
    }

    fn step_result(
        step: &AiRunbookStep,
        status: &str,
        message: &str,
        output: serde_json::Value,
        approval_id: Option<i64>,
        started: std::time::Instant,
    ) -> AiRunbookStepResult {
        AiRunbookStepResult {
            step_id: step.id.clone(),
            title: step.title.clone(),
            step_type: step.step_type.clone(),
            risk_level: step.risk_level.clone(),
            status: status.into(),
            message: message.into(),
            output,
            approval_id,
            duration_ms: started.elapsed().as_millis() as i64,
        }
    }

    fn content_json(content: &str) -> Option<serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(content.trim()).ok()
    }

    fn content_string(content: &str, key: &str) -> Option<String> {
        Self::content_json(content)?
            .get(key)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    fn content_i64(content: &str, key: &str) -> Option<i64> {
        Self::content_json(content)?.get(key)?.as_i64()
    }

    fn content_u64(content: &str, key: &str) -> Option<u64> {
        Self::content_json(content)?.get(key)?.as_u64()
    }

    fn is_readonly_sql(sql: &str) -> bool {
        let normalized = sql
            .trim()
            .trim_end_matches(';')
            .trim_start_matches('(')
            .trim()
            .to_lowercase();
        ["select", "show", "describe", "desc", "explain", "with"]
            .iter()
            .any(|prefix| normalized.starts_with(prefix))
    }

    fn validate_readonly_command(command: &str) -> Result<(), AppError> {
        let normalized = command.trim();
        if normalized.is_empty() {
            return Err(AppError::InvalidInput("命令不能为空".into()));
        }
        let lower = normalized.to_lowercase();
        let blocked_patterns = [
            ">",
            ">>",
            "&&",
            "||",
            ";",
            "`",
            "$(",
            " rm ",
            "rm -",
            "mv ",
            "cp ",
            "chmod ",
            "chown ",
            "sudo ",
            "su ",
            "kill ",
            "pkill ",
            "reboot",
            "shutdown",
            "systemctl restart",
            "systemctl stop",
            "service ",
            "docker rm",
            "docker stop",
            "kubectl delete",
            "truncate ",
            "mkfs",
            "dd ",
            "tee ",
            "sed -i",
            "perl -pi",
            "npm uninstall",
            "nvm uninstall",
        ];
        if blocked_patterns
            .iter()
            .any(|pattern| lower.contains(pattern))
        {
            return Err(AppError::InvalidInput(
                "只读命令步骤包含写入/控制/危险片段".into(),
            ));
        }
        let allowed = [
            "ls",
            "pwd",
            "whoami",
            "id",
            "hostname",
            "uname",
            "date",
            "uptime",
            "df",
            "du",
            "free",
            "top",
            "ps",
            "pgrep",
            "netstat",
            "ss",
            "ip",
            "ifconfig",
            "cat",
            "head",
            "tail",
            "grep",
            "egrep",
            "fgrep",
            "rg",
            "awk",
            "wc",
            "sort",
            "uniq",
            "find",
            "stat",
            "file",
            "env",
            "printenv",
            "which",
            "whereis",
            "systemctl",
            "journalctl",
            "docker",
            "kubectl",
            "curl",
        ];
        let first = normalized
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-');
        if !allowed.contains(&first) {
            return Err(AppError::InvalidInput(format!(
                "只读命令白名单不包含 '{}'",
                first
            )));
        }
        if (first == "systemctl"
            && !lower.starts_with("systemctl status")
            && !lower.starts_with("systemctl list"))
            || (first == "docker"
                && !lower.starts_with("docker ps")
                && !lower.starts_with("docker logs")
                && !lower.starts_with("docker inspect"))
            || (first == "kubectl"
                && !lower.starts_with("kubectl get")
                && !lower.starts_with("kubectl describe")
                && !lower.starts_with("kubectl logs"))
        {
            return Err(AppError::InvalidInput("该子命令不属于只读范围".into()));
        }
        Ok(())
    }

    fn resolve_matches(
        db: &Database,
        prompt: &str,
        scope: &str,
        include_global: bool,
        include_zero_match: bool,
    ) -> Result<Vec<AiSkillMatch>, AppError> {
        let input = ListAiSkillsInput {
            keyword: None,
            source: None,
            show_builtin: Some(true),
            scope: Some(scope.into()),
        };
        let lowered_prompt = prompt.to_lowercase();
        let mut matches: Vec<AiSkillMatch> = db
            .list_ai_skills(&input)?
            .into_iter()
            .filter(|skill| {
                skill.enabled
                    && !skill.missing
                    && (scope == "all"
                        || skill.scopes.iter().any(|item| item == scope)
                        || (include_global && skill.scopes.iter().any(|item| item == "global")))
                    && (scope != "mcp" || skill.allow_mcp)
            })
            .filter_map(|skill| {
                let matched_words = skill
                    .trigger_words
                    .iter()
                    .filter(|word| {
                        let word = word.trim().to_lowercase();
                        !word.is_empty() && lowered_prompt.contains(&word)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if !include_zero_match && matched_words.is_empty() {
                    return None;
                }
                let source_bonus = if skill.source == "user" { 20 } else { 0 };
                let scope_bonus = if skill.scopes.iter().any(|item| item == scope) {
                    10
                } else {
                    0
                };
                let score =
                    matched_words.len() as i64 * 100 + source_bonus + scope_bonus + skill.priority;
                Some(AiSkillMatch {
                    skill,
                    matched_words,
                    score,
                })
            })
            .collect();
        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.skill.priority.cmp(&a.skill.priority))
                .then_with(|| b.skill.updated_at.cmp(&a.skill.updated_at))
        });
        matches.truncate(8);
        Ok(matches)
    }

    fn build_prompt_fragment(skills: &[AiSkill], experiences: &[AiExperienceMatch]) -> String {
        if skills.is_empty() && experiences.is_empty() {
            return String::new();
        }
        let mut out = String::from("以下是本应用按当前场景注入的 Skill 和历史经验。不得输出或索要凭证明文；历史经验只作为参考，必须结合当前真实上下文判断。\n");
        if !skills.is_empty() {
            out.push_str("\n# Skill 规则\n");
            for skill in skills.iter().take(8) {
                out.push_str(&format!(
                    "\n## Skill: {} ({})\n{}\n",
                    skill.name,
                    skill.skill_key,
                    Self::trim_content(&skill.content, 2400)
                ));
            }
        }
        if !experiences.is_empty() {
            out.push_str("\n# 历史经验库命中\n");
            for item in experiences.iter().take(5) {
                let experience = &item.experience;
                out.push_str(&format!(
                    "\n## 经验: {} ({})\n场景：{}\n标签：{}\n命中词：{}\n问题现象：{}\n根因分析：{}\n解决方案：{}\n",
                    experience.title,
                    experience.experience_key,
                    experience.scenario,
                    experience.tags.join(", "),
                    item.matched_words.join(", "),
                    Self::trim_content(&experience.symptom, 800),
                    Self::trim_content(&experience.cause, 800),
                    Self::trim_content(&experience.solution, 1600),
                ));
            }
        }
        out
    }

    fn tokenize_recall_text(value: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();
        for ch in value.to_lowercase().chars() {
            if ch.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch) {
                current.push(ch);
            } else if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
        }
        if !current.is_empty() {
            words.push(current);
        }
        words
            .into_iter()
            .filter(|word| word.chars().count() >= 2)
            .take(80)
            .collect()
    }

    fn experience_summary(experience: &AiExperience) -> String {
        [
            (!experience.symptom.trim().is_empty())
                .then(|| format!("现象：{}", Self::trim_content(&experience.symptom, 160))),
            (!experience.cause.trim().is_empty())
                .then(|| format!("根因：{}", Self::trim_content(&experience.cause, 160))),
            (!experience.solution.trim().is_empty())
                .then(|| format!("方案：{}", Self::trim_content(&experience.solution, 220))),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
    }

    fn trim_content(content: &str, max_chars: usize) -> String {
        let mut text = content.trim().to_string();
        if text.chars().count() > max_chars {
            text = text.chars().take(max_chars).collect::<String>();
            text.push_str("\n...（内容已截断）");
        }
        text
    }

    fn validate_skill_input(input: &UpsertAiSkillInput) -> Result<(), AppError> {
        if input.name.trim().is_empty() {
            return Err(AppError::InvalidInput("Skill 名称不能为空".into()));
        }
        if input.content.trim().is_empty() {
            return Err(AppError::InvalidInput("Skill 内容不能为空".into()));
        }
        if input.scopes.is_empty() {
            return Err(AppError::InvalidInput("至少选择一个作用域".into()));
        }
        Ok(())
    }

    fn resolve_builtin_skills_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
        if let Ok(resource_dir) = app.path().resource_dir() {
            let candidate = resource_dir.join("resources").join("skills");
            if candidate.exists() {
                return Ok(candidate);
            }
            let candidate = resource_dir.join("skills");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        let cwd = std::env::current_dir()?;
        for candidate in [
            cwd.join("src-tauri").join("resources").join("skills"),
            cwd.join("resources").join("skills"),
        ] {
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        Ok(PathBuf::from("src-tauri/resources/skills"))
    }

    fn parse_skill_file(root: &Path, path: &Path) -> Result<ParsedSkill, AppError> {
        let raw = fs::read_to_string(path)?;
        let hash = format!("{:x}", Sha256::digest(raw.as_bytes()));
        let (frontmatter, body) = Self::split_frontmatter(&raw);
        let directory_key = path
            .parent()
            .and_then(Path::file_name)
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_else(|| "skill".into());
        let key = Self::frontmatter_value(&frontmatter, "name").unwrap_or(directory_key);
        let description = Self::frontmatter_value(&frontmatter, "description").unwrap_or_default();
        let trigger_words =
            Self::split_words(&Self::frontmatter_value(&frontmatter, "触发词").unwrap_or_default());
        let mut tags = trigger_words.iter().take(6).cloned().collect::<Vec<_>>();
        tags.sort();
        tags.dedup();
        let scopes = Self::infer_scopes(&key, &description, &trigger_words);
        let priority = if scopes.iter().any(|scope| scope == "global") {
            100
        } else {
            80
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        Ok(ParsedSkill {
            key: key.clone(),
            name: key,
            description,
            content: body.trim().to_string(),
            trigger_words,
            tags,
            scopes,
            priority,
            source_path: format!("resources/skills/{}", relative),
            hash,
        })
    }

    fn split_frontmatter(raw: &str) -> (String, String) {
        let normalized = raw.strip_prefix('\u{feff}').unwrap_or(raw);
        if !normalized.starts_with("---") {
            return (String::new(), normalized.to_string());
        }
        let mut lines = normalized.lines();
        let _ = lines.next();
        let mut frontmatter = Vec::new();
        let mut body = Vec::new();
        let mut in_body = false;
        for line in lines {
            if !in_body && line.trim() == "---" {
                in_body = true;
                continue;
            }
            if in_body {
                body.push(line);
            } else {
                frontmatter.push(line);
            }
        }
        (frontmatter.join("\n"), body.join("\n"))
    }

    fn frontmatter_value(frontmatter: &str, key: &str) -> Option<String> {
        frontmatter.lines().find_map(|line| {
            let (left, right) = line.split_once(':')?;
            if left.trim() == key {
                Some(
                    right
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string(),
                )
            } else {
                None
            }
        })
    }

    fn split_words(value: &str) -> Vec<String> {
        value
            .split(|ch| ch == ',' || ch == '，' || ch == '、')
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    fn infer_scopes(key: &str, description: &str, trigger_words: &[String]) -> Vec<String> {
        let text = format!(
            "{} {} {}",
            key.to_lowercase(),
            description.to_lowercase(),
            trigger_words.join(" ").to_lowercase()
        );
        let mut scopes = Vec::new();
        if text.contains("mysql")
            || text.contains("postgres")
            || text.contains("redis")
            || text.contains("sql")
            || text.contains("db")
        {
            scopes.push("sql".into());
        }
        if text.contains("log") || text.contains("日志") || text.contains("loki") {
            scopes.push("logs".into());
        }
        if text.contains("ssh")
            || text.contains("linux")
            || text.contains("systemd")
            || text.contains("nginx")
            || text.contains("docker")
            || text.contains("kubernetes")
            || text.contains("端口")
            || text.contains("进程")
        {
            scopes.push("terminal".into());
        }
        if text.contains("nginx") || text.contains("配置") || text.contains("yaml") {
            scopes.push("sftp".into());
        }
        if text.contains("mcp") || text.contains("agent") {
            scopes.push("mcp".into());
        }
        if scopes.is_empty() {
            scopes.push("global".into());
        }
        scopes.sort();
        scopes.dedup();
        scopes
    }

    fn write_experience_markdown(
        app: &AppHandle,
        experience_key: &str,
        input: &UpsertAiExperienceInput,
    ) -> Result<PathBuf, AppError> {
        let base_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| AppError::Custom(format!("获取应用数据目录失败: {}", error)))?
            .join("experiences");
        fs::create_dir_all(&base_dir)?;

        let file_name = format!("{}.md", Self::safe_file_stem(experience_key));
        let path = base_dir.join(file_name);
        let tags = input.tags.clone().unwrap_or_default();
        let markdown = Self::build_experience_markdown(input, &tags);
        fs::write(&path, markdown)?;
        Ok(path)
    }

    fn build_experience_markdown(input: &UpsertAiExperienceInput, tags: &[String]) -> String {
        let title = input.title.trim();
        let source = input.source.as_deref().unwrap_or("user");
        let scenario = input.scenario.as_deref().unwrap_or("");
        let symptom = input.symptom.as_deref().unwrap_or("");
        let cause = input.cause.as_deref().unwrap_or("");
        let solution = input.solution.as_deref().unwrap_or("");
        let tag_line = if tags.is_empty() {
            "无".to_string()
        } else {
            tags.iter()
                .map(|tag| format!("`{}`", tag))
                .collect::<Vec<_>>()
                .join(" ")
        };

        [
            format!("# {}", title),
            format!(
                "> 来源：{}  \n> 场景：{}  \n> 生成时间：{}",
                source,
                scenario,
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            ),
            format!("## 问题现象\n\n{}", Self::markdown_or_empty(symptom)),
            format!("## 根因分析\n\n{}", Self::markdown_or_empty(cause)),
            format!("## 解决方案\n\n{}", Self::markdown_or_empty(solution)),
            format!("## 标签\n\n{}", tag_line),
        ]
        .join("\n\n")
    }

    fn markdown_or_empty(value: &str) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            "未填写".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn safe_file_stem(value: &str) -> String {
        let mut out = String::new();
        for ch in value.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                out.push(ch);
            }
        }
        if out.is_empty() {
            format!("exp-{}", chrono::Utc::now().timestamp_millis())
        } else {
            out
        }
    }
}
