use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AiCommitGitWorkspaceInput, AiCommitGitWorkspaceResult, AiProviderAskInput,
    CommitGitWorkspaceInput, CommitGitWorkspaceResult, GitWorkspace, GitWorkspaceBranch,
    GitWorkspaceDetail, GitWorkspaceDiffInput, GitWorkspaceDiffResult, GitWorkspaceScanJobStatus,
    GitWorkspaceScanStartResult, GitWorkspaceStatusResult, ListGitWorkspacesInput,
    MergeGitWorkspaceBranchInput, ScanGitWorkspaceRootInput, ScanGitWorkspaceRootResult,
    SecureCredential, StageGitWorkspaceFilesInput, SwitchGitWorkspaceBranchInput,
    UpsertGitWorkspaceInput,
};
use crate::services::ai_provider::AiProviderService;
use crate::services::secure_credential::SecureCredentialService;
use crate::state::AppState;
use std::collections::HashMap;
use tauri::Manager;

pub struct GitWorkspaceService;

const MAX_SCAN_ENTRIES: usize = 1000;
const MAX_SCAN_REPOS: usize = 200;
const MAX_SCAN_DURATION: Duration = Duration::from_secs(3);
const SKIP_DIRS: &[&str] = &[
    ".cache",
    ".git",
    ".Trash",
    "Applications",
    "Library",
    "System",
    "Volumes",
    "build",
    "dist",
    "node_modules",
    "target",
];

static SCAN_JOBS: OnceLock<Mutex<HashMap<String, GitWorkspaceScanJobStatus>>> = OnceLock::new();

#[derive(Debug, Clone)]
struct GitSnapshot {
    branch: String,
    remote_url: String,
    status: String,
    changed_files: i64,
    ahead: i64,
    behind: i64,
}

impl GitWorkspaceService {
    pub fn list(
        db: &Database,
        input: Option<ListGitWorkspacesInput>,
    ) -> Result<Vec<GitWorkspace>, AppError> {
        let mut items = db.list_git_workspaces(&input.unwrap_or(ListGitWorkspacesInput {
            keyword: None,
            credential_key: None,
        }))?;
        items.sort_by(|left, right| {
            let left_bound = !left.credential_key.trim().is_empty();
            let right_bound = !right.credential_key.trim().is_empty();
            right_bound
                .cmp(&left_bound)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.workspace_key.cmp(&right.workspace_key))
        });
        Ok(items)
    }

    pub async fn upsert(
        db: &Database,
        input: UpsertGitWorkspaceInput,
    ) -> Result<GitWorkspace, AppError> {
        validate_upsert(&input)?;
        let repo_path = canonical_repo_path(&input.repo_path)?;
        let snapshot = inspect_git_repo(&repo_path).await?;
        let normalized = UpsertGitWorkspaceInput {
            repo_path: repo_path.to_string_lossy().to_string(),
            ..input
        };
        db.upsert_git_workspace(
            &normalized,
            &snapshot.branch,
            &snapshot.remote_url,
            &snapshot.status,
            snapshot.changed_files,
            snapshot.ahead,
            snapshot.behind,
        )
    }

    pub fn delete(db: &Database, workspace_key: &str) -> Result<(), AppError> {
        if !db.delete_git_workspace(workspace_key.trim())? {
            return Err(AppError::NotFound(format!(
                "Git 工作区 '{}' 不存在",
                workspace_key
            )));
        }
        Ok(())
    }

    pub async fn refresh(db: &Database, workspace_key: &str) -> Result<GitWorkspace, AppError> {
        let workspace = db
            .get_git_workspace(workspace_key.trim())?
            .ok_or_else(|| AppError::NotFound(format!("Git 工作区 '{}' 不存在", workspace_key)))?;
        let snapshot = inspect_git_repo(Path::new(&workspace.repo_path)).await?;
        db.update_git_workspace_scan(
            &workspace.workspace_key,
            &snapshot.branch,
            &snapshot.remote_url,
            &snapshot.status,
            snapshot.changed_files,
            snapshot.ahead,
            snapshot.behind,
        )
    }

    pub async fn detail(
        db: &Database,
        workspace_key: &str,
    ) -> Result<GitWorkspaceDetail, AppError> {
        let workspace = Self::refresh(db, workspace_key).await?;
        let status_text = git_output(Path::new(&workspace.repo_path), &["status", "--short"])
            .await
            .unwrap_or_else(|error| format!("读取状态失败: {}", error));
        let recent_log = git_output(
            Path::new(&workspace.repo_path),
            &["log", "--oneline", "--decorate", "-n", "8"],
        )
        .await
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
        Ok(GitWorkspaceDetail {
            workspace,
            status_text,
            recent_log,
        })
    }

    pub async fn scan_root(
        db: &Database,
        input: ScanGitWorkspaceRootInput,
    ) -> Result<ScanGitWorkspaceRootResult, AppError> {
        scan_root_inner(db, input, None).await
    }

    pub fn start_scan_root(
        app: tauri::AppHandle,
        input: ScanGitWorkspaceRootInput,
    ) -> Result<GitWorkspaceScanStartResult, AppError> {
        let job_id = create_scan_job_id();
        let started_at = now_text();
        let task_started_at = started_at.clone();
        let status = GitWorkspaceScanJobStatus {
            job_id: job_id.clone(),
            status: "running".into(),
            message: "扫描任务已启动，正在检查一级目录中的 Git 仓库。".into(),
            started_at,
            finished_at: None,
            result: None,
            error: None,
        };
        set_scan_job(status);

        let task_job_id = job_id.clone();
        tauri::async_runtime::spawn(async move {
            let result = run_scan_root_job(app, input, Some(task_job_id.clone())).await;
            match result {
                Ok(scan_result) => set_scan_job(GitWorkspaceScanJobStatus {
                    job_id: task_job_id,
                    status: "completed".into(),
                    message: scan_result.message.clone(),
                    started_at: task_started_at,
                    finished_at: Some(now_text()),
                    result: Some(scan_result),
                    error: None,
                }),
                Err(error) => set_scan_job(GitWorkspaceScanJobStatus {
                    job_id: task_job_id,
                    status: "failed".into(),
                    message: "扫描任务执行失败".into(),
                    started_at: task_started_at,
                    finished_at: Some(now_text()),
                    result: None,
                    error: Some(error.to_string()),
                }),
            }
        });

        Ok(GitWorkspaceScanStartResult {
            job_id,
            status: "running".into(),
            message: "扫描任务已在后台启动".into(),
        })
    }

    pub fn get_scan_status(job_id: &str) -> Result<GitWorkspaceScanJobStatus, AppError> {
        let jobs = scan_jobs()
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        jobs.get(job_id.trim())
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Git 工作区扫描任务 '{}' 不存在", job_id)))
    }

    pub async fn ai_commit(
        db: &Database,
        input: AiCommitGitWorkspaceInput,
    ) -> Result<AiCommitGitWorkspaceResult, AppError> {
        let workspace = db
            .get_git_workspace(input.workspace_key.trim())?
            .ok_or_else(|| {
                AppError::NotFound(format!("Git 工作区 '{}' 不存在", input.workspace_key))
            })?;
        if workspace.credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput("请先为工作区绑定 Git 凭证".into()));
        }
        let repo = Path::new(&workspace.repo_path);
        if !is_git_repo(repo) {
            return Err(AppError::InvalidInput("工作区路径不是有效 Git 仓库".into()));
        }

        let porcelain = git_output(repo, &["status", "--porcelain"]).await?;
        if porcelain.trim().is_empty() {
            return Err(AppError::InvalidInput("当前工作区没有可提交的改动".into()));
        }
        let summary = build_commit_summary(repo, &porcelain).await;
        let ai_result = AiProviderService::ask(
            db,
            AiProviderAskInput {
                prompt: build_commit_prompt(&workspace, &summary),
                provider_key: None,
                system_prompt: Some(
                    "你是严谨的 Git 提交信息助手。只输出一条提交信息，第一行必须是简洁的 Conventional Commit 标题，不要输出 Markdown 代码块。提交说明的自然语言内容默认使用中文，Conventional Commit 类型、scope、文件名和代码标识保持原格式。"
                        .into(),
                ),
                skill_scope: Some("git".into()),
                use_skill_trigger: Some(true),
            },
        )
        .await?;
        let commit_message = normalize_commit_message(&ai_result.answer);
        if commit_message.is_empty() {
            return Err(AppError::Custom("AI 未生成有效提交信息".into()));
        }

        git_command_output(repo, &["add", "-A"], Duration::from_secs(20)).await?;
        git_commit(repo, &commit_message).await?;
        let commit_hash = git_output(repo, &["rev-parse", "--short", "HEAD"])
            .await
            .unwrap_or_default()
            .trim()
            .to_string();
        let refreshed = Self::refresh(db, &workspace.workspace_key).await?;
        Ok(AiCommitGitWorkspaceResult {
            workspace: refreshed,
            commit_message,
            commit_hash,
            provider_name: ai_result.provider_name,
            model: ai_result.model,
        })
    }

    pub async fn status(
        db: &Database,
        workspace_key: &str,
    ) -> Result<GitWorkspaceStatusResult, AppError> {
        let workspace = Self::refresh(db, workspace_key).await?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let porcelain = git_output(repo, &["status", "--porcelain"]).await?;
        let head_commit = git_output(repo, &["rev-parse", "--short", "HEAD"])
            .await
            .unwrap_or_default()
            .trim()
            .to_string();
        let (staged_files, unstaged_files, untracked_files) = parse_porcelain_files(&porcelain);
        Ok(GitWorkspaceStatusResult {
            workspace,
            head_commit,
            porcelain,
            staged_files,
            unstaged_files,
            untracked_files,
        })
    }

    pub async fn diff(
        db: &Database,
        input: GitWorkspaceDiffInput,
    ) -> Result<GitWorkspaceDiffResult, AppError> {
        let workspace = load_workspace(db, input.workspace_key.trim())?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let staged = input.staged.unwrap_or(false);
        let mut args = if staged {
            vec!["diff", "--cached", "--"]
        } else {
            vec!["diff", "--"]
        };
        let path = input
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(path) = path {
            validate_git_relative_path(path)?;
            args.push(path);
        }
        let diff = git_output(repo, &args).await?;
        let max_chars = input.max_chars.unwrap_or(20000).clamp(1000, 100000);
        let (diff, truncated) = truncate_with_flag(&diff, max_chars);
        Ok(GitWorkspaceDiffResult {
            workspace_key: workspace.workspace_key,
            staged,
            path: path.map(str::to_string),
            diff,
            truncated,
        })
    }

    pub async fn stage_files(
        db: &Database,
        input: StageGitWorkspaceFilesInput,
    ) -> Result<GitWorkspaceStatusResult, AppError> {
        let workspace = load_workspace(db, input.workspace_key.trim())?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let paths = normalize_git_paths(&input.paths)?;
        let mut args = vec!["add".to_string(), "--".to_string()];
        args.extend(paths);
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        git_command_output(repo, &arg_refs, Duration::from_secs(30)).await?;
        Self::status(db, &workspace.workspace_key).await
    }

    pub async fn commit(
        db: &Database,
        input: CommitGitWorkspaceInput,
    ) -> Result<CommitGitWorkspaceResult, AppError> {
        let workspace = load_workspace(db, input.workspace_key.trim())?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let message = normalize_commit_message(&input.message);
        if message.trim().is_empty() {
            return Err(AppError::InvalidInput("提交信息不能为空".into()));
        }
        if let Some(paths) = input.paths.as_ref() {
            let paths = normalize_git_paths(paths)?;
            let mut args = vec!["add".to_string(), "--".to_string()];
            args.extend(paths);
            let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            git_command_output(repo, &arg_refs, Duration::from_secs(30)).await?;
        }
        let staged = git_output(repo, &["diff", "--cached", "--name-only"]).await?;
        if staged.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "没有已暂存改动，请先调用 git_workspace_stage_files 或传入 paths".into(),
            ));
        }
        git_commit(repo, &message).await?;
        let commit_hash = git_output(repo, &["rev-parse", "--short", "HEAD"])
            .await
            .unwrap_or_default()
            .trim()
            .to_string();
        let refreshed = Self::refresh(db, &workspace.workspace_key).await?;
        Ok(CommitGitWorkspaceResult {
            workspace: refreshed,
            commit_message: message,
            commit_hash,
        })
    }

    pub async fn pull(db: &Database, workspace_key: &str) -> Result<GitWorkspace, AppError> {
        let workspace = load_workspace(db, workspace_key)?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        git_command_output_with_workspace_credential(
            db,
            &workspace,
            repo,
            &["pull", "--ff-only"],
            Duration::from_secs(90),
        )
        .await?;
        Self::refresh(db, &workspace.workspace_key).await
    }

    pub async fn push(db: &Database, workspace_key: &str) -> Result<GitWorkspace, AppError> {
        let workspace = load_workspace(db, workspace_key)?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let branch = git_output(repo, &["branch", "--show-current"])
            .await
            .unwrap_or_default()
            .trim()
            .to_string();
        if branch.is_empty() {
            return Err(AppError::InvalidInput(
                "当前处于 detached HEAD，无法直接推送".into(),
            ));
        }
        let upstream = git_output(
            repo,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        )
        .await
        .unwrap_or_default();
        if upstream.trim().is_empty() {
            git_command_output_with_workspace_credential(
                db,
                &workspace,
                repo,
                &["push", "-u", "origin", branch.as_str()],
                Duration::from_secs(120),
            )
            .await?;
        } else {
            git_command_output_with_workspace_credential(
                db,
                &workspace,
                repo,
                &["push"],
                Duration::from_secs(120),
            )
            .await?;
        }
        Self::refresh(db, &workspace.workspace_key).await
    }

    pub async fn branches(
        db: &Database,
        workspace_key: &str,
    ) -> Result<Vec<GitWorkspaceBranch>, AppError> {
        let workspace = load_workspace(db, workspace_key)?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let current = git_output(repo, &["branch", "--show-current"])
            .await
            .unwrap_or_default()
            .trim()
            .to_string();
        let output = git_output(repo, &["branch", "--all", "--format=%(refname:short)"]).await?;
        let mut branches = Vec::new();
        for line in output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if line == "origin/HEAD" || line.ends_with("/HEAD") {
                continue;
            }
            let is_remote = line.starts_with("origin/");
            let name = if is_remote {
                line.trim_start_matches("origin/").to_string()
            } else {
                line.to_string()
            };
            if branches
                .iter()
                .any(|branch: &GitWorkspaceBranch| branch.name == name && !branch.is_remote)
                && is_remote
            {
                continue;
            }
            branches.push(GitWorkspaceBranch {
                display_name: if is_remote {
                    format!("{} (origin)", name)
                } else {
                    name.clone()
                },
                is_current: !current.is_empty() && current == name,
                is_remote,
                last_commit_hash: git_branch_last_commit(repo, line, "%h")
                    .await
                    .unwrap_or_default(),
                last_commit_message: git_branch_last_commit(repo, line, "%s")
                    .await
                    .unwrap_or_default(),
                last_commit_at: git_branch_last_commit(repo, line, "%ci")
                    .await
                    .unwrap_or_default(),
                name,
            });
        }
        branches.sort_by(|left, right| {
            right
                .is_current
                .cmp(&left.is_current)
                .then_with(|| left.is_remote.cmp(&right.is_remote))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(branches)
    }

    pub async fn switch_branch(
        db: &Database,
        input: SwitchGitWorkspaceBranchInput,
    ) -> Result<GitWorkspace, AppError> {
        let workspace = load_workspace(db, input.workspace_key.trim())?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let branch = input.branch.trim();
        if branch.is_empty() {
            return Err(AppError::InvalidInput("目标分支不能为空".into()));
        }
        let porcelain = git_output(repo, &["status", "--porcelain"]).await?;
        if !porcelain.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "当前工作区有未提交改动，请先提交或处理后再切换分支".into(),
            ));
        }
        let local_branches = git_output(repo, &["branch", "--format=%(refname:short)"])
            .await
            .unwrap_or_default();
        let has_local = local_branches
            .lines()
            .map(str::trim)
            .any(|item| item == branch);
        if has_local {
            git_command_output(repo, &["checkout", branch], Duration::from_secs(30)).await?;
        } else {
            let remote_ref = format!("origin/{}", branch);
            let remote_branches =
                git_output(repo, &["branch", "--remotes", "--format=%(refname:short)"])
                    .await
                    .unwrap_or_default();
            let has_remote = remote_branches
                .lines()
                .map(str::trim)
                .any(|item| item == remote_ref);
            if !has_remote {
                return Err(AppError::NotFound(format!("分支 '{}' 不存在", branch)));
            }
            git_checkout_tracking_branch(repo, branch, &remote_ref).await?;
        }
        Self::refresh(db, &workspace.workspace_key).await
    }

    pub async fn merge_branch(
        db: &Database,
        input: MergeGitWorkspaceBranchInput,
    ) -> Result<GitWorkspace, AppError> {
        let workspace = load_workspace(db, input.workspace_key.trim())?;
        let repo = Path::new(&workspace.repo_path);
        ensure_git_repo(repo)?;
        let source_branch = input.source_branch.trim();
        let target_branch = input.target_branch.trim();
        if source_branch.is_empty() || target_branch.is_empty() {
            return Err(AppError::InvalidInput("源分支和目标分支不能为空".into()));
        }
        if source_branch == target_branch {
            return Err(AppError::InvalidInput("源分支和目标分支不能相同".into()));
        }
        ensure_clean_worktree(repo, "当前工作区有未提交改动，请先提交或处理后再合并分支").await?;
        checkout_branch(repo, target_branch).await?;
        let source_ref = resolve_branch_ref(repo, source_branch).await?;
        git_command_output(
            repo,
            &["merge", "--no-edit", source_ref.as_str()],
            Duration::from_secs(120),
        )
        .await?;
        Self::refresh(db, &workspace.workspace_key).await
    }
}

fn load_workspace(db: &Database, workspace_key: &str) -> Result<GitWorkspace, AppError> {
    db.get_git_workspace(workspace_key.trim())?
        .ok_or_else(|| AppError::NotFound(format!("Git 工作区 '{}' 不存在", workspace_key)))
}

fn ensure_git_repo(repo: &Path) -> Result<(), AppError> {
    if !is_git_repo(repo) {
        return Err(AppError::InvalidInput("工作区路径不是有效 Git 仓库".into()));
    }
    Ok(())
}

fn parse_porcelain_files(porcelain: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();
    for line in porcelain.lines() {
        if line.len() < 3 {
            continue;
        }
        let status = &line[..2];
        let path = line[3..]
            .split(" -> ")
            .last()
            .unwrap_or("")
            .trim()
            .to_string();
        if path.is_empty() {
            continue;
        }
        if status == "??" {
            untracked.push(path);
            continue;
        }
        let mut chars = status.chars();
        let index_status = chars.next().unwrap_or(' ');
        let worktree_status = chars.next().unwrap_or(' ');
        if index_status != ' ' {
            staged.push(path.clone());
        }
        if worktree_status != ' ' {
            unstaged.push(path);
        }
    }
    staged.sort();
    staged.dedup();
    unstaged.sort();
    unstaged.dedup();
    untracked.sort();
    untracked.dedup();
    (staged, unstaged, untracked)
}

fn validate_git_relative_path(path: &str) -> Result<(), AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("文件路径不能为空".into()));
    }
    let value = Path::new(trimmed);
    if value.is_absolute() || trimmed.contains('\0') {
        return Err(AppError::InvalidInput("只能使用仓库内相对路径".into()));
    }
    if value
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::InvalidInput("文件路径不能包含 ..".into()));
    }
    Ok(())
}

fn normalize_git_paths(paths: &[String]) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();
    for path in paths {
        let trimmed = path.trim();
        validate_git_relative_path(trimmed)?;
        if !normalized.iter().any(|item: &String| item == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }
    if normalized.is_empty() {
        return Err(AppError::InvalidInput("文件路径列表不能为空".into()));
    }
    Ok(normalized)
}

fn truncate_with_flag(value: &str, max_chars: usize) -> (String, bool) {
    if value.chars().count() <= max_chars {
        return (value.to_string(), false);
    }
    let mut output = value.chars().take(max_chars).collect::<String>();
    output.push_str("\n...已截断...");
    (output, true)
}

async fn ensure_clean_worktree(repo: &Path, message: &str) -> Result<(), AppError> {
    let porcelain = git_output(repo, &["status", "--porcelain"]).await?;
    if !porcelain.trim().is_empty() {
        return Err(AppError::InvalidInput(message.into()));
    }
    Ok(())
}

async fn checkout_branch(repo: &Path, branch: &str) -> Result<(), AppError> {
    let local_branches = git_output(repo, &["branch", "--format=%(refname:short)"])
        .await
        .unwrap_or_default();
    let has_local = local_branches
        .lines()
        .map(str::trim)
        .any(|item| item == branch);
    if has_local {
        git_command_output(repo, &["checkout", branch], Duration::from_secs(30)).await?;
    } else {
        let remote_ref = format!("origin/{}", branch);
        let resolved = resolve_branch_ref(repo, branch).await?;
        if resolved != remote_ref {
            return Err(AppError::NotFound(format!("分支 '{}' 不存在", branch)));
        }
        git_checkout_tracking_branch(repo, branch, &remote_ref).await?;
    }
    Ok(())
}

async fn resolve_branch_ref(repo: &Path, branch: &str) -> Result<String, AppError> {
    let local_branches = git_output(repo, &["branch", "--format=%(refname:short)"])
        .await
        .unwrap_or_default();
    if local_branches
        .lines()
        .map(str::trim)
        .any(|item| item == branch)
    {
        return Ok(branch.to_string());
    }
    let remote_ref = format!("origin/{}", branch);
    let remote_branches = git_output(repo, &["branch", "--remotes", "--format=%(refname:short)"])
        .await
        .unwrap_or_default();
    if remote_branches
        .lines()
        .map(str::trim)
        .any(|item| item == remote_ref)
    {
        return Ok(remote_ref);
    }
    Err(AppError::NotFound(format!("分支 '{}' 不存在", branch)))
}

async fn run_scan_root_job(
    app: tauri::AppHandle,
    input: ScanGitWorkspaceRootInput,
    job_id: Option<String>,
) -> Result<ScanGitWorkspaceRootResult, AppError> {
    let state = app.state::<AppState>();
    scan_root_inner(&state.db, input, job_id).await
}

fn scan_jobs() -> &'static Mutex<HashMap<String, GitWorkspaceScanJobStatus>> {
    SCAN_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_scan_job(status: GitWorkspaceScanJobStatus) {
    match scan_jobs().lock() {
        Ok(mut jobs) => {
            jobs.insert(status.job_id.clone(), status);
        }
        Err(error) => log::error!("更新 Git 工作区扫描任务状态失败: {}", error),
    }
}

fn update_scan_job_message(job_id: &str, message: String) {
    match scan_jobs().lock() {
        Ok(mut jobs) => {
            if let Some(status) = jobs.get_mut(job_id) {
                status.message = message;
            }
        }
        Err(error) => log::error!("更新 Git 工作区扫描任务进度失败: {}", error),
    }
}

fn create_scan_job_id() -> String {
    format!("git_scan_{}", chrono::Local::now().timestamp_millis())
}

fn now_text() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn scan_root_inner(
    db: &Database,
    input: ScanGitWorkspaceRootInput,
    job_id: Option<String>,
) -> Result<ScanGitWorkspaceRootResult, AppError> {
    let root = PathBuf::from(input.root_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err(AppError::InvalidInput("扫描根目录不存在或不是目录".into()));
    }
    let discovery_job_id = job_id.clone();
    let discovery = tokio::task::spawn_blocking(move || discover_git_repos(root, discovery_job_id))
        .await
        .map_err(|error| AppError::Custom(format!("Git 工作区扫描任务失败: {}", error)))??;
    let discovered = discovery.discovered;
    let scanned_entries = discovery.scanned_entries;
    let skipped_entries = discovery.skipped_entries;
    let limited = discovery.limited;

    if let Some(job_id) = &job_id {
        update_scan_job_message(
            job_id,
            format!("已发现 {} 个仓库，正在写入工作区列表。", discovered),
        );
    }

    let mut saved = Vec::new();
    let credentials = db.list_secure_credentials().unwrap_or_else(|error| {
        log::warn!("读取安全凭证用于 Git 工作区自动匹配失败: {}", error);
        Vec::new()
    });
    for (index, repo) in discovery.repos.into_iter().enumerate() {
        let name = repo
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("git-workspace")
            .to_string();
        if let Some(job_id) = &job_id {
            update_scan_job_message(
                job_id,
                format!("正在写入第 {}/{} 个仓库：{}", index + 1, discovered, name),
            );
        }
        let snapshot = inspect_git_repo_light(&repo).await;
        let credential_key = input
            .credential_key
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| match_git_credential(&snapshot.remote_url, &credentials));
        let input = UpsertGitWorkspaceInput {
            id: None,
            workspace_key: normalize_workspace_key(&name),
            name,
            repo_path: repo.to_string_lossy().to_string(),
            credential_key,
            description: Some("扫描根目录自动添加".into()),
        };
        match upsert_discovered_workspace(db, input, &snapshot) {
            Ok(workspace) => saved.push(workspace),
            Err(error) => log::warn!("扫描 Git 工作区失败 {}: {}", repo.display(), error),
        }
    }
    let message = if limited {
        format!(
            "已扫描 {} 个一级目录项，跳过 {} 个目录项，发现 {} 个仓库；本次扫描达到数量或时间上限。",
            scanned_entries, skipped_entries, discovered
        )
    } else {
        format!(
            "已扫描 {} 个一级目录项，跳过 {} 个目录项，发现 {} 个仓库。",
            scanned_entries, skipped_entries, discovered
        )
    };
    Ok(ScanGitWorkspaceRootResult {
        workspaces: saved,
        discovered: discovered as i64,
        scanned_entries: scanned_entries as i64,
        skipped_entries: skipped_entries as i64,
        limited,
        message,
    })
}

#[derive(Debug)]
struct GitRepoDiscovery {
    repos: Vec<PathBuf>,
    discovered: usize,
    scanned_entries: usize,
    skipped_entries: usize,
    limited: bool,
}

fn discover_git_repos(root: PathBuf, job_id: Option<String>) -> Result<GitRepoDiscovery, AppError> {
    let mut repos = Vec::new();
    let mut scanned_entries = 0usize;
    let mut skipped_entries = 0usize;
    let mut limited = false;
    let started_at = Instant::now();
    if is_git_repo(&root) {
        repos.push(root.clone());
    }

    let entries = fs::read_dir(&root)?;
    for entry in entries {
        if scanned_entries >= MAX_SCAN_ENTRIES || repos.len() >= MAX_SCAN_REPOS {
            limited = true;
            break;
        }
        if started_at.elapsed() >= MAX_SCAN_DURATION {
            limited = true;
            break;
        }
        scanned_entries += 1;
        if scanned_entries % 100 == 0 {
            if let Some(job_id) = &job_id {
                update_scan_job_message(
                    job_id,
                    format!(
                        "已扫描 {} 个一级目录项，发现 {} 个仓库。",
                        scanned_entries,
                        repos.len()
                    ),
                );
            }
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                skipped_entries += 1;
                log::warn!("读取 Git 工作区扫描目录项失败: {}", error);
                continue;
            }
        };
        let path = entry.path();
        if should_skip_scan_path(&path) {
            skipped_entries += 1;
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                skipped_entries += 1;
                log::warn!(
                    "读取 Git 工作区目录项类型失败 {}: {}",
                    path.display(),
                    error
                );
                continue;
            }
        };
        if file_type.is_dir() && is_git_repo(&path) {
            repos.push(path);
        }
    }

    let discovered = repos.len();
    Ok(GitRepoDiscovery {
        repos,
        discovered,
        scanned_entries,
        skipped_entries,
        limited,
    })
}

fn upsert_discovered_workspace(
    db: &Database,
    input: UpsertGitWorkspaceInput,
    snapshot: &GitSnapshot,
) -> Result<GitWorkspace, AppError> {
    validate_upsert(&input)?;
    let normalized = UpsertGitWorkspaceInput {
        repo_path: input.repo_path.trim().to_string(),
        ..input
    };
    // 扫描根目录时只读取分支和 remote，不执行 git status/log 等慢命令；
    // 用户打开详情或点击刷新时再读取完整 Git 状态。
    db.upsert_git_workspace(
        &normalized,
        &snapshot.branch,
        &snapshot.remote_url,
        "unknown",
        0,
        0,
        0,
    )
}

fn should_skip_scan_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return true;
    };
    SKIP_DIRS.iter().any(|item| item.eq_ignore_ascii_case(name))
}

fn validate_upsert(input: &UpsertGitWorkspaceInput) -> Result<(), AppError> {
    if input.workspace_key.trim().is_empty() {
        return Err(AppError::InvalidInput("工作区 Key 不能为空".into()));
    }
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput("工作区名称不能为空".into()));
    }
    if input.repo_path.trim().is_empty() {
        return Err(AppError::InvalidInput("仓库路径不能为空".into()));
    }
    Ok(())
}

fn canonical_repo_path(path: &str) -> Result<PathBuf, AppError> {
    let repo = fs::canonicalize(PathBuf::from(path.trim()))?;
    if !is_git_repo(&repo) {
        return Err(AppError::InvalidInput("请选择有效的 Git 仓库目录".into()));
    }
    Ok(repo)
}

fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

async fn inspect_git_repo(repo: &Path) -> Result<GitSnapshot, AppError> {
    let branch = git_output(repo, &["branch", "--show-current"])
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let branch = if branch.is_empty() {
        git_output(repo, &["rev-parse", "--short", "HEAD"])
            .await
            .unwrap_or_else(|_| "HEAD".into())
            .trim()
            .to_string()
    } else {
        branch
    };
    let remote_url = sanitize_remote_url(
        git_output(repo, &["config", "--get", "remote.origin.url"])
            .await
            .unwrap_or_default()
            .trim(),
    );
    let changed_files = git_output(repo, &["status", "--porcelain"])
        .await
        .unwrap_or_default()
        .lines()
        .count() as i64;
    let (ahead, behind) = parse_ahead_behind(
        &git_output(
            repo,
            &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
        )
        .await
        .unwrap_or_default(),
    );
    let status = if changed_files > 0 {
        "dirty"
    } else if ahead > 0 && behind > 0 {
        "diverged"
    } else if ahead > 0 {
        "ahead"
    } else if behind > 0 {
        "behind"
    } else {
        "clean"
    }
    .to_string();
    Ok(GitSnapshot {
        branch,
        remote_url,
        status,
        changed_files,
        ahead,
        behind,
    })
}

async fn inspect_git_repo_light(repo: &Path) -> GitSnapshot {
    let branch = quick_git_output(repo, &["branch", "--show-current"])
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let branch = if branch.is_empty() {
        quick_git_output(repo, &["rev-parse", "--short", "HEAD"])
            .await
            .unwrap_or_else(|_| "HEAD".into())
            .trim()
            .to_string()
    } else {
        branch
    };
    let remote_url = sanitize_remote_url(
        quick_git_output(repo, &["config", "--get", "remote.origin.url"])
            .await
            .unwrap_or_default()
            .trim(),
    );
    GitSnapshot {
        branch,
        remote_url,
        status: "unknown".into(),
        changed_files: 0,
        ahead: 0,
        behind: 0,
    }
}

async fn quick_git_output(repo: &Path, args: &[&str]) -> Result<String, AppError> {
    run_git_output(repo, args, Duration::from_secs(1)).await
}

async fn git_output(repo: &Path, args: &[&str]) -> Result<String, AppError> {
    run_git_output(repo, args, Duration::from_secs(5)).await
}

async fn run_git_output(
    repo: &Path,
    args: &[&str],
    duration: Duration,
) -> Result<String, AppError> {
    run_git_output_with_env(repo, args, duration, &[], &[]).await
}

async fn run_git_output_with_env(
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
        let message = redact_git_sensitive_message(&message, sensitive_values);
        return Err(AppError::Custom(if message.is_empty() {
            format!("git {:?} 执行失败", args)
        } else {
            message
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn git_command_output(
    repo: &Path,
    args: &[&str],
    duration: Duration,
) -> Result<String, AppError> {
    run_git_output(repo, args, duration).await
}

async fn git_command_output_with_workspace_credential(
    db: &Database,
    workspace: &GitWorkspace,
    repo: &Path,
    args: &[&str],
    duration: Duration,
) -> Result<String, AppError> {
    let credentials = db.list_secure_credentials().unwrap_or_else(|error| {
        log::warn!("读取安全凭证用于 Git 工作区命令自动匹配失败: {}", error);
        Vec::new()
    });
    let matched_credential_key = workspace
        .credential_key
        .trim()
        .to_string()
        .is_empty()
        .then(|| match_git_credential(&workspace.remote_url, &credentials))
        .flatten();
    let credential_key = if workspace.credential_key.trim().is_empty() {
        matched_credential_key.as_deref().unwrap_or("")
    } else {
        workspace.credential_key.trim()
    };
    if credential_key.is_empty() {
        return git_command_output(repo, args, duration)
            .await
            .map_err(|error| normalize_git_auth_error(error, workspace, None));
    }

    let credential =
        SecureCredentialService::resolve_git_credential(db, credential_key, &workspace.remote_url)?;
    validate_git_workspace_credential(&credential)?;
    let secret = SecureCredentialService::get_secret(db, &credential.credential_key)?;
    let username = git_username_for_credential(&credential);
    let askpass = write_git_workspace_askpass_script()?;
    let envs = vec![
        ("GIT_ASKPASS", askpass.to_string_lossy().to_string()),
        ("GIT_WORKSPACE_USERNAME", username),
        ("GIT_WORKSPACE_TOKEN", secret.clone()),
    ];
    let result = run_git_output_with_env(repo, args, duration, &envs, &[secret])
        .await
        .map_err(|error| normalize_git_auth_error(error, workspace, Some(credential_key)));
    let _ = fs::remove_file(&askpass);
    result
}

async fn git_commit(repo: &Path, message: &str) -> Result<(), AppError> {
    let mut args = vec!["commit".to_string()];
    let mut lines = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let title = lines.next().unwrap_or("chore: update workspace");
    args.push("-m".into());
    args.push(title.to_string());
    let body = lines.collect::<Vec<_>>().join("\n");
    if !body.trim().is_empty() {
        args.push("-m".into());
        args.push(body);
    }
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    git_command_output(repo, &arg_refs, Duration::from_secs(30))
        .await
        .map(|_| ())
}

async fn git_branch_last_commit(
    repo: &Path,
    branch_ref: &str,
    format: &str,
) -> Result<String, AppError> {
    let output = git_output(
        repo,
        &["log", "-1", &format!("--format={}", format), branch_ref],
    )
    .await?;
    Ok(output.trim().to_string())
}

async fn git_checkout_tracking_branch(
    repo: &Path,
    branch: &str,
    remote_ref: &str,
) -> Result<(), AppError> {
    let args = [
        "checkout".to_string(),
        "-b".to_string(),
        branch.to_string(),
        "--track".to_string(),
        remote_ref.to_string(),
    ];
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    git_command_output(repo, &arg_refs, Duration::from_secs(30))
        .await
        .map(|_| ())
}

async fn build_commit_summary(repo: &Path, porcelain: &str) -> String {
    let stat = git_output(repo, &["diff", "--stat"])
        .await
        .unwrap_or_default();
    let staged_stat = git_output(repo, &["diff", "--cached", "--stat"])
        .await
        .unwrap_or_default();
    let names = git_output(repo, &["diff", "--name-status"])
        .await
        .unwrap_or_default();
    [
        format!("status --porcelain:\n{}", porcelain.trim()),
        format!("diff --name-status:\n{}", names.trim()),
        format!("diff --stat:\n{}", stat.trim()),
        format!("staged diff --stat:\n{}", staged_stat.trim()),
    ]
    .join("\n\n")
}

fn build_commit_prompt(workspace: &GitWorkspace, summary: &str) -> String {
    format!(
        "请为下面 Git 工作区生成一条中文提交信息。\n\n工作区: {}\n分支: {}\n路径: {}\n\n变更摘要:\n{}\n\n要求:\n1. 只输出提交信息，不要解释。\n2. 第一行使用 Conventional Commit，type 保持英文，说明内容使用中文，例如 feat: 增加工作区推送能力 / fix: 修复状态刷新超时。\n3. 如需正文，空一行后用 1-3 条中文短句描述关键变更。\n4. 文件名、命令、代码标识、scope 保持原格式。\n5. 不要包含 Markdown 代码块。",
        workspace.name,
        if workspace.branch.trim().is_empty() {
            "HEAD"
        } else {
            workspace.branch.as_str()
        },
        workspace.repo_path,
        truncate_text(summary, 6000)
    )
}

fn normalize_commit_message(value: &str) -> String {
    let mut text = value
        .trim()
        .trim_matches('`')
        .replace("\r\n", "\n")
        .replace("\r", "\n");
    if text.starts_with("```") {
        text = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n");
    }
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output = value.chars().take(max_chars).collect::<String>();
    output.push_str("\n...已截断...");
    output
}

fn match_git_credential(remote_url: &str, credentials: &[SecureCredential]) -> Option<String> {
    let provider = provider_from_remote(remote_url)?;
    let remote_host = normalize_remote_host(remote_url).unwrap_or_default();
    credentials
        .iter()
        .filter(|credential| {
            credential.enabled
                && credential.status == "active"
                && credential.has_secret
                && credential.provider == provider
                && ["github", "gitlab", "gitcode", "gitee"].contains(&credential.provider.as_str())
        })
        .find(|credential| {
            let base_host = normalize_remote_host(&credential.base_url).unwrap_or_default();
            base_host.is_empty() || remote_host.is_empty() || base_host == remote_host
        })
        .map(|credential| credential.credential_key.clone())
}

fn provider_from_remote(remote_url: &str) -> Option<&'static str> {
    let value = remote_url.to_lowercase();
    if value.contains("github.com") {
        Some("github")
    } else if value.contains("gitlab") {
        Some("gitlab")
    } else if value.contains("gitcode") {
        Some("gitcode")
    } else if value.contains("gitee") {
        Some("gitee")
    } else {
        None
    }
}

fn normalize_remote_host(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.split_once("://").map(|(_, rest)| rest) {
        let host = rest
            .split('/')
            .next()
            .unwrap_or("")
            .split('@')
            .next_back()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .trim();
        return (!host.is_empty()).then(|| host.to_lowercase());
    }
    if let Some((host, _)) = trimmed.split_once(':') {
        let host = host
            .split('@')
            .next_back()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        return (!host.is_empty()).then_some(host);
    }
    let host = trimmed
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    (!host.is_empty()).then_some(host)
}

fn validate_git_workspace_credential(credential: &SecureCredential) -> Result<(), AppError> {
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

fn normalize_git_auth_error(
    error: AppError,
    workspace: &GitWorkspace,
    credential_key: Option<&str>,
) -> AppError {
    let message = error.to_string();
    if !message.contains("terminal prompts disabled")
        && !message.contains("could not read Username")
        && !message.contains("Authentication failed")
    {
        return error;
    }

    let provider = provider_from_remote(&workspace.remote_url)
        .map(git_provider_display_name)
        .unwrap_or("Git");
    let bind_hint = if credential_key.is_none() {
        "当前工作区未绑定安全凭证，也未自动匹配到可用凭证。"
    } else if workspace.credential_key.trim().is_empty() {
        "当前工作区未绑定安全凭证，自动匹配到的安全凭证未通过远程仓库认证。"
    } else {
        "当前工作区绑定的安全凭证未通过远程仓库认证。"
    };
    AppError::InvalidInput(format!(
        "{} 仓库拉取需要认证。{}请在「安全 -> Git 工作区」为该仓库绑定可读取仓库的 {} 凭证后重试。",
        provider, bind_hint, provider
    ))
}

fn git_provider_display_name(provider: &str) -> &'static str {
    match provider {
        "github" => "GitHub",
        "gitlab" => "GitLab",
        "gitcode" => "GitCode",
        "gitee" => "Gitee",
        _ => "Git",
    }
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

fn write_git_workspace_askpass_script() -> Result<PathBuf, AppError> {
    let path = std::env::temp_dir().join(format!(
        "tauri-ssh-git-workspace-askpass-{}-{}.{}",
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

fn redact_git_sensitive_message(message: &str, sensitive_values: &[String]) -> String {
    let mut output = sanitize_remote_url(message);
    for value in sensitive_values {
        let value = value.trim();
        if !value.is_empty() {
            output = output.replace(value, "***");
        }
    }
    output
}

fn parse_ahead_behind(value: &str) -> (i64, i64) {
    let mut parts = value.split_whitespace();
    let ahead = parts.next().and_then(|item| item.parse().ok()).unwrap_or(0);
    let behind = parts.next().and_then(|item| item.parse().ok()).unwrap_or(0);
    (ahead, behind)
}

fn sanitize_remote_url(value: &str) -> String {
    if let Some((scheme, rest)) = value.split_once("://") {
        if let Some((_, host_path)) = rest.split_once('@') {
            return format!("{}://***@{}", scheme, host_path);
        }
    }
    value.to_string()
}

fn normalize_workspace_key(name: &str) -> String {
    let normalized = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let key = normalized.trim_matches('_').to_string();
    if key.is_empty() {
        "git_workspace".into()
    } else {
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UpsertGitWorkspaceInput;

    fn run_git(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn test_repo_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tauri-ssh-{}-{}-{}",
            name,
            std::process::id(),
            chrono::Local::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
        ))
    }

    #[tokio::test]
    async fn status_stage_and_commit_explicit_files() {
        let repo = test_repo_path("git-workspace");
        fs::create_dir_all(&repo).expect("create repo dir");
        run_git(&repo, &["init"]);
        run_git(&repo, &["config", "user.email", "test@example.local"]);
        run_git(&repo, &["config", "user.name", "Tauri SSH Test"]);
        fs::write(repo.join("README.md"), "initial\n").expect("write initial");
        run_git(&repo, &["add", "--", "README.md"]);
        run_git(&repo, &["commit", "-m", "chore: initial"]);
        fs::write(repo.join("README.md"), "initial\nchanged\n").expect("write changed");

        let db = Database::init(":memory:").expect("init db");
        let workspace = GitWorkspaceService::upsert(
            &db,
            UpsertGitWorkspaceInput {
                id: None,
                workspace_key: "test_repo".into(),
                name: "Test Repo".into(),
                repo_path: repo.to_string_lossy().to_string(),
                credential_key: None,
                description: None,
            },
        )
        .await
        .expect("upsert workspace");

        let status = GitWorkspaceService::status(&db, &workspace.workspace_key)
            .await
            .expect("status");
        assert!(!status.head_commit.is_empty());
        assert!(status.unstaged_files.contains(&"README.md".into()));

        let diff = GitWorkspaceService::diff(
            &db,
            GitWorkspaceDiffInput {
                workspace_key: workspace.workspace_key.clone(),
                staged: Some(false),
                path: Some("README.md".into()),
                max_chars: Some(10000),
            },
        )
        .await
        .expect("diff");
        assert!(diff.diff.contains("+changed"));

        let staged = GitWorkspaceService::stage_files(
            &db,
            StageGitWorkspaceFilesInput {
                workspace_key: workspace.workspace_key.clone(),
                paths: vec!["README.md".into()],
            },
        )
        .await
        .expect("stage files");
        assert!(staged.staged_files.contains(&"README.md".into()));

        let committed = GitWorkspaceService::commit(
            &db,
            CommitGitWorkspaceInput {
                workspace_key: workspace.workspace_key,
                message: "test: commit explicit file".into(),
                paths: None,
            },
        )
        .await
        .expect("commit");
        assert_eq!(committed.commit_message, "test: commit explicit file");
        assert!(!committed.commit_hash.is_empty());

        let _ = fs::remove_dir_all(&repo);
    }
}
