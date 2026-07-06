use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{process::Command, time::timeout};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AiProviderAskInput, CodeReviewBatchItem, CodeReviewBatchParseResult, CodeReviewChangedFile,
    CodeReviewCommit, CodeReviewTask, CreateAuditLogInput, CreateCodeReviewBatchTasksInput,
    CreateCodeReviewTaskInput, GitWorkspace, ListCodeReviewTasksInput, ParseCodeReviewBatchInput,
    RunCodeReviewAiInput, SecureCredential,
};
use crate::services::{
    ai_provider::AiProviderService, audit::AuditService, git_workspace::GitWorkspaceService,
    secure_credential::SecureCredentialService,
};

const MAX_DIFF_FILES: usize = 40;
const MAX_DIFF_CHARS: usize = 120_000;

pub struct CodeReviewService;

impl CodeReviewService {
    pub fn list(
        db: &Database,
        input: Option<ListCodeReviewTasksInput>,
    ) -> Result<Vec<CodeReviewTask>, AppError> {
        db.list_code_review_tasks(&input.unwrap_or(ListCodeReviewTasksInput {
            workspace_key: None,
            status: None,
            keyword: None,
            limit: Some(100),
        }))
    }

    pub fn get(db: &Database, task_key: &str) -> Result<CodeReviewTask, AppError> {
        db.get_code_review_task(task_key.trim())?
            .ok_or_else(|| AppError::NotFound(format!("代码审核任务 '{}' 不存在", task_key)))
    }

    pub fn create(
        db: &Database,
        input: CreateCodeReviewTaskInput,
    ) -> Result<CodeReviewTask, AppError> {
        validate_task_input(&input)?;
        let workspace = load_workspace(db, &input.workspace_key)?;
        ensure_git_repo(Path::new(&workspace.repo_path))?;
        let task_key = create_key("cr");
        let task = db.create_code_review_task(&task_key, &workspace, &input)?;
        audit(
            db,
            "code_review_task_create",
            "readonly",
            "成功",
            &format!(
                "创建代码审核任务 {}: {} -> {}",
                task.workspace_name, task.source_branch, task.target_branch
            ),
            json!({
                "taskKey": task.task_key,
                "workspaceKey": task.workspace_key,
                "sourceBranch": task.source_branch,
                "targetBranch": task.target_branch
            }),
        );
        Ok(task)
    }

    pub fn create_batch_tasks(
        db: &Database,
        input: CreateCodeReviewBatchTasksInput,
    ) -> Result<Vec<CodeReviewTask>, AppError> {
        let batch_key = input.batch_key.trim();
        if batch_key.is_empty() {
            return Err(AppError::InvalidInput("批次 Key 不能为空".into()));
        }
        if input.items.is_empty() {
            return Err(AppError::InvalidInput("批量任务不能为空".into()));
        }
        let mut tasks = Vec::new();
        for item in input.items {
            let task = Self::create(
                db,
                CreateCodeReviewTaskInput {
                    workspace_key: item.workspace_key,
                    source_branch: item.source_branch,
                    target_branch: item.target_branch,
                    batch_key: Some(batch_key.to_string()),
                },
            )?;
            tasks.push(task);
        }
        audit(
            db,
            "code_review_batch_create",
            "readonly",
            "成功",
            &format!("创建批量代码审核任务 {} 项", tasks.len()),
            json!({ "batchKey": batch_key, "count": tasks.len() }),
        );
        Ok(tasks)
    }

    pub async fn prepare_diff(db: &Database, task_key: &str) -> Result<CodeReviewTask, AppError> {
        let task = Self::get(db, task_key)?;
        let workspace = load_workspace(db, &task.workspace_key)?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        ensure_clean_worktree(repo).await?;
        fetch_prune(db, &workspace, repo).await?;

        let source_ref = resolve_branch_ref(repo, &task.source_branch).await?;
        let target_ref = resolve_branch_ref(repo, &task.target_branch).await?;
        let merge_base = git_output(
            repo,
            &["merge-base", &source_ref, &target_ref],
            Duration::from_secs(10),
        )
        .await?
        .trim()
        .to_string();
        let source_head = rev_parse(repo, &source_ref).await?;
        let target_head = rev_parse(repo, &target_ref).await?;
        let commits = read_commits(repo, &merge_base, &source_ref).await?;
        let changed_files = read_changed_files(repo, &merge_base, &source_ref).await?;
        let diff_stat = read_diff_stat(repo, &merge_base, &source_ref).await?;
        let diff_excerpt =
            read_diff_excerpt(repo, &merge_base, &source_ref, &changed_files).await?;

        let task = db.update_code_review_task_diff(
            &task.task_key,
            "diff_ready",
            "medium",
            &merge_base,
            &source_head,
            &target_head,
            &serde_json::to_string(&diff_stat)?,
            &serde_json::to_string(&changed_files)?,
            &serde_json::to_string(&commits)?,
            &serde_json::to_string(&diff_excerpt)?,
            "",
        )?;
        audit(
            db,
            "code_review_diff_prepare",
            "readonly",
            "成功",
            &format!("生成代码审核 diff: {}", task.task_key),
            json!({
                "taskKey": task.task_key,
                "workspaceKey": task.workspace_key,
                "changedFiles": changed_files.len(),
                "commitCount": commits.len()
            }),
        );
        Ok(task)
    }

    pub async fn run_ai(
        db: &Database,
        input: RunCodeReviewAiInput,
    ) -> Result<CodeReviewTask, AppError> {
        let task = Self::get(db, &input.task_key)?;
        if task.status != "diff_ready" && task.status != "review_ready" {
            return Err(AppError::InvalidInput(
                "请先生成代码差异后再运行 AI 审查".into(),
            ));
        }
        let prompt = build_review_prompt(&task);
        let started = Instant::now();
        let result = AiProviderService::ask(
            db,
            AiProviderAskInput {
                prompt,
                provider_key: input.provider_key,
                system_prompt: Some(
                    "你是严谨的代码审查助手。请使用简体中文输出 Markdown 审查报告，并在末尾提供 JSON 结构化结论。不要输出凭据、密钥或无关内容。"
                        .into(),
                ),
                skill_scope: Some("code-review".into()),
                use_skill_trigger: Some(true),
            },
        )
        .await?;
        let (risk_level, ai_json) = build_ai_review_json(
            &result.answer,
            result.latency_ms,
            &result.provider_name,
            &result.model,
        );
        let task = db.update_code_review_ai_result(
            &task.task_key,
            "review_ready",
            &risk_level,
            &result.provider_name,
            &result.model,
            &result.answer,
            &serde_json::to_string(&ai_json)?,
            "",
        )?;
        audit(
            db,
            "code_review_ai_run",
            "readonly",
            "成功",
            &format!("AI 代码审查完成: {}", task.task_key),
            json!({
                "taskKey": task.task_key,
                "provider": result.provider_name,
                "model": result.model,
                "latencyMs": started.elapsed().as_millis() as i64,
                "riskLevel": task.risk_level
            }),
        );
        Ok(task)
    }

    pub async fn merge(db: &Database, task_key: &str) -> Result<CodeReviewTask, AppError> {
        let task = Self::get(db, task_key)?;
        if task.status != "diff_ready"
            && task.status != "review_ready"
            && task.status != "merge_failed"
        {
            return Err(AppError::InvalidInput(
                "请先生成代码差异后再确认合并".into(),
            ));
        }
        let workspace = load_workspace(db, &task.workspace_key)?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        ensure_clean_worktree(repo).await?;
        fetch_prune(db, &workspace, repo).await?;
        if let Err(error) = ensure_review_snapshot_fresh(repo, &task).await {
            let task = db.update_code_review_task_status(
                &task.task_key,
                "stale",
                &error.to_string(),
                false,
            )?;
            audit(
                db,
                "code_review_merge_stale",
                "medium",
                "失败",
                &format!("代码审核任务已过期: {}", task.task_key),
                json!({ "taskKey": task.task_key, "error": error.to_string() }),
            );
            return Err(error);
        }

        checkout_branch(repo, &task.target_branch).await?;
        let source_ref = resolve_branch_ref(repo, &task.source_branch).await?;
        match git_output(
            repo,
            &["merge", "--no-edit", &source_ref],
            Duration::from_secs(120),
        )
        .await
        {
            Ok(_) => {
                let task = db.update_code_review_task_status(&task.task_key, "merged", "", true)?;
                let stale_count = db.mark_older_duplicate_code_review_tasks_stale(
                    &task,
                    "已有更新的同分支审查任务完成本地合并，本任务已过期",
                )?;
                audit(
                    db,
                    "code_review_merge_success",
                    "medium",
                    "成功",
                    &format!(
                        "本地合并完成: {} -> {}",
                        task.source_branch, task.target_branch
                    ),
                    json!({
                        "taskKey": task.task_key,
                        "workspaceKey": task.workspace_key,
                        "sourceBranch": task.source_branch,
                        "targetBranch": task.target_branch,
                        "staleDuplicateTasks": stale_count,
                        "highRiskTargetBranch": is_high_risk_target_branch(&task.target_branch)
                    }),
                );
                Ok(task)
            }
            Err(error) => {
                let conflicts = git_output(
                    repo,
                    &["diff", "--name-only", "--diff-filter=U"],
                    Duration::from_secs(5),
                )
                .await
                .unwrap_or_default();
                let status = if conflicts.trim().is_empty() {
                    "merge_failed"
                } else {
                    "conflict"
                };
                let task = db.update_code_review_task_status(
                    &task.task_key,
                    status,
                    &error.to_string(),
                    false,
                )?;
                audit(
                    db,
                    if status == "conflict" {
                        "code_review_merge_conflict"
                    } else {
                        "code_review_merge_failed"
                    },
                    "high",
                    "失败",
                    &format!("本地合并失败: {}", task.task_key),
                    json!({
                        "taskKey": task.task_key,
                        "error": error.to_string(),
                        "highRiskTargetBranch": is_high_risk_target_branch(&task.target_branch),
                        "conflicts": conflicts.lines().collect::<Vec<_>>()
                    }),
                );
                Err(error)
            }
        }
    }

    pub async fn push(db: &Database, task_key: &str) -> Result<CodeReviewTask, AppError> {
        let task = Self::get(db, task_key)?;
        if task.status != "merged" {
            return Err(AppError::InvalidInput(
                "只有本地已合并的任务才能推送远程".into(),
            ));
        }
        let workspace = load_workspace(db, &task.workspace_key)?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let _ = db.update_code_review_push_status(&task.task_key, "pushing", "")?;
        let result = git_output_with_workspace_credential(
            db,
            &workspace,
            repo,
            &["push"],
            Duration::from_secs(120),
        )
        .await;
        match result {
            Ok(_) => {
                let mut task = db.update_code_review_push_status(&task.task_key, "pushed", "")?;
                let stale_count = db.mark_older_duplicate_code_review_tasks_stale(
                    &task,
                    "已有更新的同分支审查任务完成远程推送，本任务已过期",
                )?;
                let switch_result = checkout_branch(repo, &task.source_branch).await;
                let switched_back_to_source = switch_result.is_ok();
                if let Err(error) = switch_result {
                    let message = format!("远程已推送，但切回源分支失败：{}", error);
                    task = db.update_code_review_push_status(&task.task_key, "pushed", &message)?;
                    audit(
                        db,
                        "code_review_push_return_branch_failed",
                        "medium",
                        "警告",
                        &format!(
                            "代码审核任务已推送远程，但切回源分支失败: {}",
                            task.task_key
                        ),
                        json!({
                            "taskKey": task.task_key,
                            "workspaceKey": task.workspace_key,
                            "sourceBranch": task.source_branch,
                            "targetBranch": task.target_branch,
                            "error": message
                        }),
                    );
                } else if let Err(error) =
                    GitWorkspaceService::refresh(db, &workspace.workspace_key).await
                {
                    audit(
                        db,
                        "code_review_push_workspace_refresh_failed",
                        "low",
                        "警告",
                        &format!("代码审核任务推送后刷新 Git 工作区失败: {}", task.task_key),
                        json!({
                            "taskKey": task.task_key,
                            "workspaceKey": task.workspace_key,
                            "error": error.to_string()
                        }),
                    );
                }
                audit(
                    db,
                    "code_review_push_success",
                    "medium",
                    "成功",
                    &format!("代码审核任务已推送远程: {}", task.task_key),
                    json!({
                        "taskKey": task.task_key,
                        "workspaceKey": task.workspace_key,
                        "staleDuplicateTasks": stale_count,
                        "sourceBranch": task.source_branch,
                        "targetBranch": task.target_branch,
                        "switchedBackToSource": switched_back_to_source
                    }),
                );
                Ok(task)
            }
            Err(error) => {
                let message = normalize_push_error(&error.to_string());
                let task =
                    db.update_code_review_push_status(&task.task_key, "push_failed", &message)?;
                audit(
                    db,
                    "code_review_push_failed",
                    "high",
                    "失败",
                    &format!("代码审核任务推送失败: {}", task.task_key),
                    json!({ "taskKey": task.task_key, "error": message }),
                );
                Err(AppError::Custom(message))
            }
        }
    }

    pub async fn abort_merge(db: &Database, task_key: &str) -> Result<CodeReviewTask, AppError> {
        let task = Self::get(db, task_key)?;
        if task.status == "merged" || task.push_status == "pushed" {
            return Err(AppError::InvalidInput(
                "本地已合并或已推送的任务不能中止合并".into(),
            ));
        }
        let workspace = load_workspace(db, &task.workspace_key)?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        if !has_merge_in_progress(repo).await {
            return Err(AppError::InvalidInput(
                "当前 Git 工作区没有可中止的合并；请在 Git 工作区处理未提交改动后再继续".into(),
            ));
        }
        git_output(repo, &["merge", "--abort"], Duration::from_secs(30)).await?;
        let restored_status = if task.ai_review_markdown.trim().is_empty() {
            "diff_ready"
        } else {
            "review_ready"
        };
        let task = db.update_code_review_task_status(
            &task.task_key,
            restored_status,
            "已中止本次合并",
            false,
        )?;
        audit(
            db,
            "code_review_merge_abort",
            "medium",
            "成功",
            &format!("已中止代码审核合并: {}", task.task_key),
            json!({
                "taskKey": task.task_key,
                "workspaceKey": task.workspace_key,
                "sourceBranch": task.source_branch,
                "targetBranch": task.target_branch
            }),
        );
        Ok(task)
    }

    pub fn cancel(db: &Database, task_key: &str) -> Result<CodeReviewTask, AppError> {
        let task = Self::get(db, task_key)?;
        if task.status == "merged" {
            return Err(AppError::InvalidInput(
                "本地已合并的任务不能直接放弃，请通过 Git 工作区处理后续状态".into(),
            ));
        }
        let task =
            db.update_code_review_task_status(&task.task_key, "cancelled", "用户放弃任务", false)?;
        audit(
            db,
            "code_review_task_cancel",
            "readonly",
            "成功",
            &format!("放弃代码审核任务: {}", task.task_key),
            json!({ "taskKey": task.task_key, "workspaceKey": task.workspace_key }),
        );
        Ok(task)
    }

    pub async fn parse_batch(
        db: &Database,
        input: ParseCodeReviewBatchInput,
    ) -> Result<CodeReviewBatchParseResult, AppError> {
        let raw = input.raw_text.trim();
        if raw.is_empty() {
            return Err(AppError::InvalidInput("批量解析文本不能为空".into()));
        }
        let mut items = parse_batch_rules(db, raw)?;
        let mut warnings = Vec::new();
        if items.is_empty()
            || items
                .iter()
                .any(|item| item.confidence < 0.85 || item.matched_workspace_key.is_none())
        {
            match parse_batch_with_ai(db, raw).await {
                Ok(ai_items) => {
                    if !ai_items.is_empty() {
                        if items.is_empty() {
                            warnings.push("规则未解析到完整任务，已使用 AI 解析结果".into());
                        } else {
                            warnings.push("规则解析结果置信度不足，已使用 AI 校验补全结果".into());
                        }
                        items = ai_items;
                    }
                }
                Err(error) => warnings.push(format!("AI 解析兜底失败: {}", error)),
            }
        }
        let batch_key = create_key("crb");
        if items.is_empty() {
            warnings.push("未解析到有效项目，请手动编辑任务列表".into());
        }
        let payload = json!({ "items": items, "warnings": warnings });
        let _ =
            db.create_code_review_batch(&batch_key, raw, &payload.to_string(), items.len() as i64)?;
        audit(
            db,
            "code_review_batch_parse",
            "readonly",
            "成功",
            &format!("解析代码审核批量任务 {} 项", items.len()),
            json!({ "batchKey": batch_key, "count": items.len() }),
        );
        Ok(CodeReviewBatchParseResult {
            batch_key,
            items,
            warnings,
        })
    }
}

fn validate_task_input(input: &CreateCodeReviewTaskInput) -> Result<(), AppError> {
    if input.workspace_key.trim().is_empty() {
        return Err(AppError::InvalidInput("Git 工作区不能为空".into()));
    }
    if input.source_branch.trim().is_empty() || input.target_branch.trim().is_empty() {
        return Err(AppError::InvalidInput("源分支和目标分支不能为空".into()));
    }
    if input.source_branch.trim() == input.target_branch.trim() {
        return Err(AppError::InvalidInput("源分支和目标分支不能相同".into()));
    }
    Ok(())
}

fn load_workspace(db: &Database, workspace_key: &str) -> Result<GitWorkspace, AppError> {
    db.get_git_workspace(workspace_key.trim())?
        .ok_or_else(|| AppError::NotFound(format!("Git 工作区 '{}' 不存在", workspace_key)))
}

fn ensure_git_repo(repo: &Path) -> Result<(), AppError> {
    if !repo.join(".git").exists() {
        return Err(AppError::InvalidInput("工作区路径不是有效 Git 仓库".into()));
    }
    Ok(())
}

async fn ensure_clean_worktree(repo: &Path) -> Result<(), AppError> {
    let porcelain = git_output(
        repo,
        &["-c", "core.quotepath=false", "status", "--porcelain"],
        Duration::from_secs(5),
    )
    .await?;
    if !porcelain.trim().is_empty() {
        return Err(AppError::InvalidInput(format!(
            "当前工作区有未提交改动，请先处理后再生成审查或合并：{}",
            porcelain.lines().take(8).collect::<Vec<_>>().join("；")
        )));
    }
    Ok(())
}

async fn has_merge_in_progress(repo: &Path) -> bool {
    git_output(
        repo,
        &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
        Duration::from_secs(5),
    )
    .await
    .is_ok()
}

async fn fetch_prune(db: &Database, workspace: &GitWorkspace, repo: &Path) -> Result<(), AppError> {
    git_output_with_workspace_credential(
        db,
        workspace,
        repo,
        &["fetch", "--prune"],
        Duration::from_secs(90),
    )
    .await
    .map(|_| ())
}

async fn ensure_review_snapshot_fresh(repo: &Path, task: &CodeReviewTask) -> Result<(), AppError> {
    let source_ref = resolve_branch_ref(repo, &task.source_branch).await?;
    let target_ref = resolve_branch_ref(repo, &task.target_branch).await?;
    let merge_base = git_output(
        repo,
        &["merge-base", &source_ref, &target_ref],
        Duration::from_secs(10),
    )
    .await?
    .trim()
    .to_string();
    let source_head = rev_parse(repo, &source_ref).await?;
    let target_head = rev_parse(repo, &target_ref).await?;
    if source_head != task.source_head
        || target_head != task.target_head
        || merge_base != task.merge_base
    {
        return Err(AppError::InvalidInput(
            "源分支、目标分支或 merge-base 已变化，请重新生成 diff 和 AI 审查".into(),
        ));
    }
    Ok(())
}

async fn checkout_branch(repo: &Path, branch: &str) -> Result<(), AppError> {
    let local = git_output(
        repo,
        &["branch", "--format=%(refname:short)"],
        Duration::from_secs(5),
    )
    .await
    .unwrap_or_default();
    if local.lines().map(str::trim).any(|item| item == branch) {
        git_output(repo, &["checkout", branch], Duration::from_secs(30)).await?;
        return Ok(());
    }
    let remote_ref = format!("origin/{}", branch);
    let resolved = resolve_branch_ref(repo, branch).await?;
    if resolved != remote_ref {
        return Err(AppError::NotFound(format!("分支 '{}' 不存在", branch)));
    }
    git_output(
        repo,
        &["checkout", "-B", branch, "--track", &remote_ref],
        Duration::from_secs(30),
    )
    .await?;
    Ok(())
}

async fn resolve_branch_ref(repo: &Path, branch: &str) -> Result<String, AppError> {
    let branch = branch.trim();
    let local = git_output(
        repo,
        &["branch", "--format=%(refname:short)"],
        Duration::from_secs(5),
    )
    .await
    .unwrap_or_default();
    if local.lines().map(str::trim).any(|item| item == branch) {
        return Ok(branch.to_string());
    }
    let remote_ref = format!("origin/{}", branch);
    let remote = git_output(
        repo,
        &["branch", "--remotes", "--format=%(refname:short)"],
        Duration::from_secs(5),
    )
    .await
    .unwrap_or_default();
    if remote.lines().map(str::trim).any(|item| item == remote_ref) {
        return Ok(remote_ref);
    }
    Err(AppError::NotFound(format!("分支 '{}' 不存在", branch)))
}

async fn rev_parse(repo: &Path, reference: &str) -> Result<String, AppError> {
    Ok(
        git_output(repo, &["rev-parse", reference], Duration::from_secs(5))
            .await?
            .trim()
            .to_string(),
    )
}

async fn read_commits(
    repo: &Path,
    base: &str,
    source_ref: &str,
) -> Result<Vec<CodeReviewCommit>, AppError> {
    let range = format!("{}..{}", base, source_ref);
    let output = git_output(
        repo,
        &["log", "--pretty=format:%h%x1f%an%x1f%ci%x1f%s", &range],
        Duration::from_secs(10),
    )
    .await
    .unwrap_or_default();
    Ok(output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\x1f');
            Some(CodeReviewCommit {
                hash: parts.next()?.to_string(),
                author: parts.next().unwrap_or("").to_string(),
                date: parts.next().unwrap_or("").to_string(),
                message: parts.next().unwrap_or("").to_string(),
            })
        })
        .collect())
}

async fn read_changed_files(
    repo: &Path,
    base: &str,
    source_ref: &str,
) -> Result<Vec<CodeReviewChangedFile>, AppError> {
    let range = format!("{}..{}", base, source_ref);
    let name_status = git_output(
        repo,
        &["diff", "--name-status", &range],
        Duration::from_secs(10),
    )
    .await
    .unwrap_or_default();
    let numstat = git_output(
        repo,
        &["diff", "--numstat", &range],
        Duration::from_secs(10),
    )
    .await
    .unwrap_or_default();
    let mut stats = std::collections::HashMap::new();
    for line in numstat.lines() {
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() >= 3 {
            let additions = parts[0].parse::<i64>().unwrap_or(0);
            let deletions = parts[1].parse::<i64>().unwrap_or(0);
            stats.insert(parts[2].to_string(), (additions, deletions));
        }
    }
    Ok(name_status
        .lines()
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            if parts.len() < 2 {
                return None;
            }
            let path = parts.last().unwrap_or(&"").to_string();
            let (additions, deletions) = stats.get(&path).copied().unwrap_or((0, 0));
            Some(CodeReviewChangedFile {
                path,
                status: parts[0].to_string(),
                additions,
                deletions,
            })
        })
        .collect())
}

async fn read_diff_stat(repo: &Path, base: &str, source_ref: &str) -> Result<Value, AppError> {
    let range = format!("{}..{}", base, source_ref);
    let shortstat = git_output(
        repo,
        &["diff", "--shortstat", &range],
        Duration::from_secs(10),
    )
    .await
    .unwrap_or_default();
    Ok(json!({ "summary": shortstat.trim() }))
}

async fn read_diff_excerpt(
    repo: &Path,
    base: &str,
    source_ref: &str,
    files: &[CodeReviewChangedFile],
) -> Result<Value, AppError> {
    let range = format!("{}..{}", base, source_ref);
    let mut excerpts = Vec::new();
    let mut used = 0usize;
    for file in files.iter().take(MAX_DIFF_FILES) {
        if is_sensitive_path(&file.path) {
            excerpts.push(json!({
                "path": file.path,
                "truncated": true,
                "content": "敏感文件内容已跳过"
            }));
            continue;
        }
        let diff = git_output(
            repo,
            &["diff", "--unified=80", &range, "--", &file.path],
            Duration::from_secs(10),
        )
        .await
        .unwrap_or_default();
        let remaining = MAX_DIFF_CHARS.saturating_sub(used);
        if remaining == 0 {
            break;
        }
        let content = if diff.len() > remaining {
            diff.chars().take(remaining).collect::<String>()
        } else {
            diff
        };
        used += content.len();
        excerpts.push(json!({
            "path": file.path,
            "truncated": used >= MAX_DIFF_CHARS,
            "content": redact_sensitive_text(&content)
        }));
        if used >= MAX_DIFF_CHARS {
            break;
        }
    }
    Ok(json!(excerpts))
}

fn build_review_prompt(task: &CodeReviewTask) -> String {
    format!(
        r#"请审查以下 Git 分支合并差异。

项目: {workspace}
源分支: {source}
目标分支: {target}
merge-base: {base}
源分支 HEAD: {source_head}
目标分支 HEAD: {target_head}

提交列表:
{commits}

文件变更:
{files}

Diff 摘要:
{diff_stat}

Diff 片段:
{diff_excerpt}

请输出 Markdown 审查报告，必须包含：
1. 总体结论和风险等级。
2. 阻塞问题。
3. 警告和建议。
4. 建议测试命令。
5. 是否建议合并。

末尾附加严格 JSON，字段包含 riskLevel、summary、blockingIssues、warnings、testSuggestions、mergeRecommendation。
riskLevel 只能取 low、medium、high、critical；无法确认时使用 medium，不能输出 unknown。
"#,
        workspace = task.workspace_name,
        source = task.source_branch,
        target = task.target_branch,
        base = task.merge_base,
        source_head = task.source_head,
        target_head = task.target_head,
        commits = serde_json::to_string_pretty(&task.commits).unwrap_or_default(),
        files = serde_json::to_string_pretty(&task.changed_files).unwrap_or_default(),
        diff_stat = serde_json::to_string_pretty(&task.diff_stat).unwrap_or_default(),
        diff_excerpt = serde_json::to_string_pretty(&task.diff_excerpt).unwrap_or_default(),
    )
}

fn infer_risk_level(answer: &str) -> String {
    let lower = answer.to_lowercase();
    if lower.contains("critical") || answer.contains("严重") || answer.contains("阻塞") {
        "critical"
    } else if lower.contains("high") || answer.contains("高风险") {
        "high"
    } else if lower.contains("medium") || answer.contains("中风险") {
        "medium"
    } else if lower.contains("low") || answer.contains("低风险") {
        "low"
    } else {
        "medium"
    }
    .into()
}

fn build_ai_review_json(
    answer: &str,
    latency_ms: i64,
    provider_name: &str,
    model: &str,
) -> (String, Value) {
    let mut parsed = extract_json_object(answer)
        .ok()
        .and_then(|json_text| serde_json::from_str::<Value>(&json_text).ok())
        .filter(|value| value.is_object())
        .unwrap_or_else(|| {
            json!({
                "riskLevel": infer_risk_level(answer),
                "summary": "",
                "blockingIssues": [],
                "warnings": [],
                "testSuggestions": [],
                "mergeRecommendation": "manual_review"
            })
        });

    let risk_level = parsed
        .get("riskLevel")
        .and_then(|value| value.as_str())
        .map(normalize_risk_level)
        .unwrap_or_else(|| infer_risk_level(answer));

    if let Some(object) = parsed.as_object_mut() {
        object.insert("riskLevel".into(), json!(risk_level));
        object.insert("latencyMs".into(), json!(latency_ms));
        object.insert("provider".into(), json!(provider_name));
        object.insert("model".into(), json!(model));
        object.insert("reviewedAt".into(), json!(now_text()));
        for key in ["blockingIssues", "warnings", "testSuggestions"] {
            if !object
                .get(key)
                .map(|value| value.is_array())
                .unwrap_or(false)
            {
                object.insert(key.into(), json!([]));
            }
        }
        object
            .entry("mergeRecommendation")
            .or_insert_with(|| json!("manual_review"));
        object.entry("summary").or_insert_with(|| json!(""));
    }

    (risk_level, parsed)
}

fn normalize_risk_level(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "critical" | "严重" | "阻塞" => "critical",
        "high" | "高" | "高风险" => "high",
        "medium" | "中" | "中风险" => "medium",
        "low" | "低" | "低风险" => "low",
        _ => "medium",
    }
    .into()
}

fn is_high_risk_target_branch(branch: &str) -> bool {
    let normalized = branch.trim().to_lowercase();
    normalized == "main"
        || normalized == "master"
        || normalized == "production"
        || normalized.starts_with("release/")
        || normalized.starts_with("prod/")
}

fn parse_batch_rules(db: &Database, raw: &str) -> Result<Vec<CodeReviewBatchItem>, AppError> {
    let workspaces = db.list_git_workspaces(&crate::models::ListGitWorkspacesInput {
        keyword: None,
        credential_key: None,
    })?;
    let target_branch = parse_target_branch(raw).unwrap_or_else(|| "dev".into());
    let mut items = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !(line.contains("项目") && line.contains("分支")) {
            continue;
        }
        let group = if line.contains("前端") {
            "frontend"
        } else if line.contains("后端") {
            "backend"
        } else {
            "unknown"
        };
        let Some((projects_part, branch_part)) = line.split_once("分支") else {
            continue;
        };
        let source_branch = branch_part
            .trim_start_matches(|value| value == ':' || value == '：')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if source_branch.is_empty() {
            continue;
        }
        let projects_text = projects_part
            .split_once('：')
            .or_else(|| projects_part.split_once(':'))
            .map(|(_, right)| right)
            .unwrap_or(projects_part);
        for project in projects_text
            .split(|value| value == '、' || value == ',' || value == '，')
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.ends_with("项目"))
        {
            let matches = match_workspace(project, &workspaces);
            items.push(CodeReviewBatchItem {
                project_name: project.to_string(),
                source_branch: source_branch.clone(),
                target_branch: target_branch.clone(),
                group: group.into(),
                confidence: if matches.len() == 1 { 0.95 } else { 0.6 },
                matched_workspace_key: if matches.len() == 1 {
                    Some(matches[0].workspace_key.clone())
                } else {
                    None
                },
                status: if matches.len() == 1 {
                    "matched".into()
                } else if matches.is_empty() {
                    "unmatched".into()
                } else {
                    "multiple_candidates".into()
                },
                warnings: if matches.len() == 1 {
                    Vec::new()
                } else {
                    vec!["需要手动选择 Git 工作区".into()]
                },
            });
        }
    }
    Ok(items)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiBatchParsePayload {
    target_branch: Option<String>,
    items: Vec<AiBatchParseItem>,
    warnings: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiBatchParseItem {
    project_name: String,
    source_branch: String,
    target_branch: Option<String>,
    group: Option<String>,
    confidence: Option<f64>,
}

async fn parse_batch_with_ai(
    db: &Database,
    raw: &str,
) -> Result<Vec<CodeReviewBatchItem>, AppError> {
    let prompt = format!(
        r#"请从下面的中文合并说明中解析 Git 分支合并任务，只输出严格 JSON，不要输出 Markdown。

输出格式:
{{
  "targetBranch": "dev",
  "items": [
    {{
      "projectName": "fj-example",
      "sourceBranch": "dev-v1",
      "targetBranch": "dev",
      "group": "frontend",
      "confidence": 0.92
    }}
  ],
  "warnings": []
}}

合并说明:
{raw}
"#
    );
    let answer = AiProviderService::ask(
        db,
        AiProviderAskInput {
            prompt,
            provider_key: None,
            system_prompt: Some(
                "你是分支合并需求解析器。只输出严格 JSON，字段名使用 camelCase。".into(),
            ),
            skill_scope: Some("git".into()),
            use_skill_trigger: Some(false),
        },
    )
    .await?
    .answer;
    let json_text = extract_json_object(&answer)?;
    let payload: AiBatchParsePayload = serde_json::from_str(&json_text)
        .map_err(|error| AppError::Custom(format!("AI 解析结果不是有效 JSON: {}", error)))?;
    let workspaces = db.list_git_workspaces(&crate::models::ListGitWorkspacesInput {
        keyword: None,
        credential_key: None,
    })?;
    let default_target = payload.target_branch.unwrap_or_else(|| "dev".into());
    let mut items = Vec::new();
    for item in payload.items {
        let project = item.project_name.trim();
        let source_branch = item.source_branch.trim();
        if project.is_empty() || source_branch.is_empty() {
            continue;
        }
        let target_branch = item
            .target_branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&default_target)
            .to_string();
        let matches = match_workspace(project, &workspaces);
        let mut warnings = payload.warnings.clone().unwrap_or_default();
        if matches.len() != 1 {
            warnings.push("需要手动选择 Git 工作区".into());
        }
        let confidence = item.confidence.unwrap_or(0.8).clamp(0.0, 1.0);
        let matched_workspace_key = if matches.len() == 1 {
            Some(matches[0].workspace_key.clone())
        } else {
            None
        };
        let status = if confidence < 0.85 {
            "needs_confirmation"
        } else if matches.len() == 1 {
            "matched"
        } else if matches.is_empty() {
            "unmatched"
        } else {
            "multiple_candidates"
        };
        items.push(CodeReviewBatchItem {
            project_name: project.to_string(),
            source_branch: source_branch.to_string(),
            target_branch,
            group: item.group.unwrap_or_else(|| "unknown".into()),
            confidence,
            matched_workspace_key,
            status: status.into(),
            warnings,
        });
    }
    Ok(items)
}

fn extract_json_object(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Ok(trimmed.to_string());
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| AppError::Custom("AI 解析结果未包含 JSON 对象".into()))?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| AppError::Custom("AI 解析结果未包含 JSON 对象结束符".into()))?;
    Ok(trimmed[start..=end].to_string())
}

fn parse_target_branch(raw: &str) -> Option<String> {
    for marker in ["合并到", "合并", "合个", "合到"] {
        if let Some((_, right)) = raw.rsplit_once(marker) {
            let branch = right
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|value| {
                    value == '。'
                        || value == '.'
                        || value == '，'
                        || value == ','
                        || value == '分'
                        || value == '支'
                })
                .trim();
            if !branch.is_empty() {
                return Some(branch.to_string());
            }
        }
    }
    None
}

fn match_workspace<'a>(project: &str, workspaces: &'a [GitWorkspace]) -> Vec<&'a GitWorkspace> {
    let project_lower = project.to_lowercase();
    workspaces
        .iter()
        .filter(|workspace| {
            workspace.name.eq_ignore_ascii_case(project)
                || Path::new(&workspace.repo_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(|name| name.eq_ignore_ascii_case(project))
                    .unwrap_or(false)
                || workspace.remote_url.to_lowercase().contains(&project_lower)
        })
        .collect()
}

async fn git_output(repo: &Path, args: &[&str], duration: Duration) -> Result<String, AppError> {
    git_output_with_env(repo, args, duration, &[], &[]).await
}

async fn git_output_with_workspace_credential(
    db: &Database,
    workspace: &GitWorkspace,
    repo: &Path,
    args: &[&str],
    duration: Duration,
) -> Result<String, AppError> {
    let credential_key = workspace.credential_key.trim();
    if credential_key.is_empty() {
        return git_output(repo, args, duration).await;
    }
    let credential =
        SecureCredentialService::resolve_git_credential(db, credential_key, &workspace.remote_url)?;
    validate_git_credential(&credential)?;
    let secret = SecureCredentialService::get_secret(db, &credential.credential_key)?;
    let username = git_username_for_credential(&credential);
    let askpass = write_askpass_script()?;
    let envs = vec![
        ("GIT_ASKPASS", askpass.to_string_lossy().to_string()),
        ("GIT_WORKSPACE_USERNAME", username),
        ("GIT_WORKSPACE_TOKEN", secret.clone()),
    ];
    let result = git_output_with_env(repo, args, duration, &envs, &[secret]).await;
    let _ = fs::remove_file(&askpass);
    result
}

async fn git_output_with_env(
    repo: &Path,
    args: &[&str],
    duration: Duration,
    envs: &[(&str, String)],
    sensitive_values: &[String],
) -> Result<String, AppError> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .kill_on_drop(true);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = timeout(duration, command.output())
        .await
        .map_err(|_| AppError::Custom(format!("git {:?} 执行超时", args)))??;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Custom(redact_git_message(
            &message,
            sensitive_values,
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn validate_git_credential(credential: &SecureCredential) -> Result<(), AppError> {
    if !credential.enabled || credential.status != "active" {
        return Err(AppError::InvalidInput(format!(
            "安全凭证 '{}' 当前不可用",
            credential.credential_key
        )));
    }
    if !credential.has_secret {
        return Err(AppError::InvalidInput(format!(
            "安全凭证 '{}' 未保存密钥内容",
            credential.credential_key
        )));
    }
    if !["github", "gitlab", "gitcode", "gitee"].contains(&credential.provider.as_str()) {
        return Err(AppError::InvalidInput(format!(
            "安全凭证 '{}' 不是 Git Provider 凭证",
            credential.credential_key
        )));
    }
    Ok(())
}

fn git_username_for_credential(credential: &SecureCredential) -> String {
    let account_name = credential.account_name.trim();
    if !account_name.is_empty() {
        return account_name.to_string();
    }
    match credential.provider.as_str() {
        "github" => "x-access-token",
        "gitlab" | "gitcode" | "gitee" => "oauth2",
        _ => "git",
    }
    .into()
}

fn write_askpass_script() -> Result<PathBuf, AppError> {
    let path = std::env::temp_dir().join(format!(
        "tauri-ssh-code-review-askpass-{}-{}.{}",
        std::process::id(),
        chrono::Local::now().timestamp_millis(),
        if cfg!(windows) { "bat" } else { "sh" }
    ));
    let content = if cfg!(windows) {
        "@echo off\r\necho %1 | findstr /I \"Username\" >NUL\r\nif %ERRORLEVEL%==0 (\r\n  echo %GIT_WORKSPACE_USERNAME%\r\n) else (\r\n  echo %GIT_WORKSPACE_TOKEN%\r\n)\r\n"
    } else {
        "#!/bin/sh\ncase \"$1\" in\n  *Username*) printf '%s\\n' \"$GIT_WORKSPACE_USERNAME\" ;;\n  *) printf '%s\\n' \"$GIT_WORKSPACE_TOKEN\" ;;\nesac\n"
    };
    fs::write(&path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&path, permissions)?;
    }
    Ok(path)
}

fn is_sensitive_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".env")
        || lower.contains("secret")
        || lower.contains("private")
        || lower.contains("token")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
}

fn redact_sensitive_text(value: &str) -> String {
    value
        .lines()
        .map(|line| {
            let lower = line.to_lowercase();
            if lower.contains("password")
                || lower.contains("token")
                || lower.contains("secret")
                || lower.contains("api_key")
            {
                "[REDACTED]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_git_message(message: &str, sensitive_values: &[String]) -> String {
    let mut output = message.to_string();
    for value in sensitive_values {
        let value = value.trim();
        if !value.is_empty() {
            output = output.replace(value, "***");
        }
    }
    output
}

fn normalize_push_error(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("permission")
        || lower.contains("protected branch")
        || lower.contains("not allowed")
        || lower.contains("403")
    {
        "当前凭证没有推送目标分支权限，合并已在本地完成但无法推送远程".into()
    } else {
        error.into()
    }
}

fn create_key(prefix: &str) -> String {
    format!("{}_{}", prefix, chrono::Local::now().timestamp_millis())
}

fn now_text() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn audit(db: &Database, action: &str, risk: &str, result: &str, summary: &str, detail: Value) {
    let _ = AuditService::create(
        db,
        CreateAuditLogInput {
            actor: "local-user".into(),
            source: "code_review".into(),
            server_alias: String::new(),
            action: action.into(),
            risk: risk.into(),
            result: result.into(),
            summary: summary.into(),
            detail_json: Some(detail.to_string()),
            request_id: None,
            approval_id: None,
        },
    );
}
