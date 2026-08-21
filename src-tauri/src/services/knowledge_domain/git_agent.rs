use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::process::Command;
use tokio::time::timeout;

use crate::database::Database;
use crate::error::AppError;
use crate::models::{KnowledgeAskResult, KnowledgeCitation};
use crate::services::knowledge::audit_knowledge;

const GIT_TIMEOUT: Duration = Duration::from_secs(10);
const COMMIT_COUNT_TOOL_KEY: &str = "git.commit_count";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitAgentIntent {
    CommitCount,
}

#[derive(Debug)]
struct RepositoryCommitCount {
    binding_id: i64,
    workspace_key: String,
    display_name: String,
    commit_sha: String,
    count: i64,
    duration_ms: u128,
}

#[derive(Debug)]
struct RepositoryFailure {
    target_key: String,
    message: String,
    duration_ms: u128,
}

/// 项目证据 Agent 的首个只读工具。是否执行 Git、仓库路径和冻结提交均由后端确定，
/// 前端与模型不能提交命令、参数或本地路径。
pub struct KnowledgeGitAgentService;

impl KnowledgeGitAgentService {
    pub async fn try_answer(
        db: &Database,
        project_id: i64,
        release_id: i64,
        release_version: &str,
        question: &str,
    ) -> Result<Option<KnowledgeAskResult>, AppError> {
        let Some(GitAgentIntent::CommitCount) = git_agent_intent(question) else {
            return Ok(None);
        };

        let manifests = db.list_knowledge_release_repository_manifests(release_id)?;
        if manifests.is_empty() {
            let result = no_evidence_result(
                "当前项目版本没有冻结的仓库清单，无法安全统计 Git 提交次数。",
                Vec::new(),
                Vec::new(),
            );
            audit_git_agent(db, project_id, release_id, &result);
            return Ok(Some(result));
        }

        let mut successes = Vec::new();
        let mut failures = Vec::new();
        for manifest in manifests {
            let started_at = Instant::now();
            let binding = match db
                .get_knowledge_project_repository_binding(manifest.repository_binding_id)?
            {
                Some(binding) if binding.project_id == project_id => binding,
                _ => {
                    failures.push(RepositoryFailure {
                        target_key: format!("repository-{}", manifest.repository_binding_id),
                        message: "版本清单中的仓库关联已不可用".to_string(),
                        duration_ms: started_at.elapsed().as_millis(),
                    });
                    continue;
                }
            };
            let target_key = binding.workspace_key.clone();
            if manifest.inclusion_status != "ready" {
                failures.push(RepositoryFailure {
                    target_key,
                    message: "该仓库未进入当前版本的可用冻结清单".to_string(),
                    duration_ms: started_at.elapsed().as_millis(),
                });
                continue;
            }
            if !is_full_commit_sha(&manifest.resolved_commit_sha) {
                failures.push(RepositoryFailure {
                    target_key,
                    message: "冻结提交标识无效".to_string(),
                    duration_ms: started_at.elapsed().as_millis(),
                });
                continue;
            }
            let Some(workspace) = db.get_git_workspace(&binding.workspace_key)? else {
                failures.push(RepositoryFailure {
                    target_key,
                    message: "关联的 Git 工作区不存在".to_string(),
                    duration_ms: started_at.elapsed().as_millis(),
                });
                continue;
            };
            match read_commit_count(
                Path::new(&workspace.repo_path),
                &manifest.resolved_commit_sha,
            )
            .await
            {
                Ok(count) => successes.push(RepositoryCommitCount {
                    binding_id: binding.id,
                    workspace_key: binding.workspace_key,
                    display_name: if binding.alias.trim().is_empty() {
                        workspace.name
                    } else {
                        binding.alias
                    },
                    commit_sha: manifest.resolved_commit_sha,
                    count,
                    duration_ms: started_at.elapsed().as_millis(),
                }),
                Err(message) => failures.push(RepositoryFailure {
                    target_key,
                    message,
                    duration_ms: started_at.elapsed().as_millis(),
                }),
            }
        }

        if successes.is_empty() {
            let gaps = failures
                .iter()
                .map(|failure| format!("仓库“{}”：{}。", failure.target_key, failure.message))
                .collect();
            let result = no_evidence_result(
                "当前版本的关联仓库均未能完成只读 Git 统计，未返回可能误导的 0 次结果。",
                gaps,
                failures,
            );
            audit_git_agent(db, project_id, release_id, &result);
            return Ok(Some(result));
        }

        let total: i64 = successes.iter().map(|item| item.count).sum();
        let citations = successes
            .iter()
            .map(|item| commit_count_citation(project_id, release_id, item))
            .collect::<Vec<_>>();
        let answer = render_commit_count_answer(release_id, release_version, &successes, total);
        let evidence_gaps = failures
            .iter()
            .map(|failure| format!("仓库“{}”：{}。", failure.target_key, failure.message))
            .collect::<Vec<_>>();
        let steps = successes
            .iter()
            .map(|item| {
                json!({
                    "toolKey": COMMIT_COUNT_TOOL_KEY,
                    "targetKey": item.workspace_key,
                    "status": "succeeded",
                    "durationMs": item.duration_ms,
                })
            })
            .chain(failures.iter().map(|item| {
                json!({
                    "toolKey": COMMIT_COUNT_TOOL_KEY,
                    "targetKey": item.target_key,
                    "status": "failed",
                    "durationMs": item.duration_ms,
                })
            }))
            .collect::<Vec<_>>();

        let result = KnowledgeAskResult {
            answer,
            citation_validation: "notApplicable".to_string(),
            citations,
            conflicts: Vec::new(),
            evidence_gaps,
            retrieval_diagnostics: json!({
                "queryMode": "gitAgent",
                "agent": {
                    "intent": COMMIT_COUNT_TOOL_KEY,
                    "status": if failures.is_empty() { "completed" } else { "partial" },
                    "repositoryCount": successes.len() + failures.len(),
                    "succeededCount": successes.len(),
                    "failedCount": failures.len(),
                    "scope": "selectedRelease",
                    "includeMerges": true,
                    "totalCommitCount": total,
                    "steps": steps,
                }
            }),
        };
        audit_git_agent(db, project_id, release_id, &result);
        Ok(Some(result))
    }
}

fn git_agent_intent(question: &str) -> Option<GitAgentIntent> {
    let normalized = question.trim().to_ascii_lowercase();
    let explicit_technical_context = normalized.contains("git")
        || normalized.contains("commit")
        || normalized.contains("仓库提交")
        || normalized.contains("代码提交");
    let business_submission_object = ["需求", "申请", "表单", "文件", "工单", "数据"]
        .iter()
        .any(|token| normalized.contains(token));
    let version_commit_phrase = normalized.contains("提交")
        && !business_submission_object
        && (normalized.contains("版本")
            || normalized
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '.')
                .any(|token| {
                    token.starts_with('v')
                        && token[1..]
                            .chars()
                            .any(|character| character.is_ascii_digit())
                }));
    let mentions_git_or_commit = explicit_technical_context || version_commit_phrase;
    let asks_count = ["多少次", "几次", "次数", "数量", "总数", "多少个", "统计"]
        .iter()
        .any(|token| normalized.contains(token));
    let asks_verification = ["验证", "校验", "证明", "通过了吗", "关联"]
        .iter()
        .any(|token| normalized.contains(token));
    (mentions_git_or_commit && asks_count && !asks_verification)
        .then_some(GitAgentIntent::CommitCount)
}

fn is_full_commit_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 只执行固定的 `rev-list --count`，并关闭 hook、交互和可选锁。错误只返回安全分类，
/// 不把绝对路径、完整命令或 stderr 暴露到回答和持久化诊断中。
async fn read_commit_count(repo: &Path, commit_sha: &str) -> Result<i64, String> {
    if !repo.is_dir() || !is_full_commit_sha(commit_sha) {
        return Err("仓库目录或冻结提交不可用".to_string());
    }
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .arg("-C")
        .arg(repo)
        .args(["rev-list", "--count", commit_sha])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .kill_on_drop(true);
    let output = timeout(GIT_TIMEOUT, command.output())
        .await
        .map_err(|_| "Git 只读统计超时".to_string())?
        .map_err(|_| "无法启动 Git 只读统计".to_string())?;
    if !output.status.success() {
        return Err("冻结提交在本地仓库中不可达".to_string());
    }
    let raw = String::from_utf8(output.stdout).map_err(|_| "Git 统计结果编码无效".to_string())?;
    raw.trim()
        .parse::<i64>()
        .ok()
        .filter(|count| *count >= 0)
        .ok_or_else(|| "Git 统计结果格式无效".to_string())
}

fn commit_count_citation(
    project_id: i64,
    release_id: i64,
    item: &RepositoryCommitCount,
) -> KnowledgeCitation {
    let short_sha = &item.commit_sha[..7];
    KnowledgeCitation {
        citation_key: format!(
            "tool:git_commit_count:release:{release_id}:repository:{}",
            item.binding_id
        ),
        source_type: "git_statistics".to_string(),
        document_id: None,
        document_version_id: None,
        chunk_id: None,
        project_id: Some(project_id),
        release_id: Some(release_id),
        title: format!("{} Git 提交统计", item.display_name),
        logical_path: format!("git/{}", item.workspace_key),
        heading_path: "提交统计".to_string(),
        commit_sha: item.commit_sha.clone(),
        external_key: item.workspace_key.clone(),
        snapshot_id: None,
        symbol_key: COMMIT_COUNT_TOOL_KEY.to_string(),
        start_line: None,
        end_line: None,
        excerpt: format!(
            "截至所选版本冻结提交 {short_sha}，可达提交 {} 次，包含合并提交。",
            item.count
        ),
    }
}

fn render_commit_count_answer(
    release_id: i64,
    release_version: &str,
    items: &[RepositoryCommitCount],
    total: i64,
) -> String {
    let mut lines = vec![
        format!(
            "已按当前选择的 **{}** 冻结版本查询 {} 个关联 Git 仓库。",
            release_version.trim(),
            items.len()
        ),
        String::new(),
        "| 仓库 | 截止提交 | 提交数 |".to_string(),
        "|---|---|---:|".to_string(),
    ];
    lines.extend(items.iter().map(|item| {
        let citation_key = format!(
            "tool:git_commit_count:release:{release_id}:repository:{}",
            item.binding_id
        );
        format!(
            "| {} | `{}` | {} [{}] |",
            item.display_name,
            &item.commit_sha[..7],
            item.count,
            citation_key
        )
    }));
    let mut answer = lines.join("\n");
    answer.push_str(&format!(
        "\n\n合计 **{total} 次**。该结果为逐仓库可达提交数的算术和，包含合并提交，不是跨仓库去重结果。"
    ));
    answer
}

fn no_evidence_result(
    message: &str,
    evidence_gaps: Vec<String>,
    failures: Vec<RepositoryFailure>,
) -> KnowledgeAskResult {
    let steps = failures
        .iter()
        .map(|failure| {
            json!({
                "toolKey": COMMIT_COUNT_TOOL_KEY,
                "targetKey": failure.target_key,
                "status": "failed",
                "durationMs": failure.duration_ms,
            })
        })
        .collect::<Vec<_>>();
    KnowledgeAskResult {
        answer: message.to_string(),
        citation_validation: "notApplicable".to_string(),
        citations: Vec::new(),
        conflicts: Vec::new(),
        evidence_gaps,
        retrieval_diagnostics: json!({
            "queryMode": "gitAgent",
            "agent": {
                "intent": COMMIT_COUNT_TOOL_KEY,
                "status": "failed",
                "repositoryCount": failures.len(),
                "succeededCount": 0,
                "failedCount": failures.len(),
                "scope": "selectedRelease",
                "includeMerges": true,
                "steps": steps,
            }
        }),
    }
}

fn audit_git_agent(db: &Database, project_id: i64, release_id: i64, result: &KnowledgeAskResult) {
    let agent = &result.retrieval_diagnostics["agent"];
    audit_knowledge(
        db,
        "knowledge_git_agent_ask",
        "readonly",
        if agent["status"] == "failed" {
            "失败"
        } else if agent["status"] == "partial" {
            "部分成功"
        } else {
            "成功"
        },
        "执行知识库 Git 只读统计",
        json!({
            "projectId": project_id,
            "releaseId": release_id,
            "toolKey": COMMIT_COUNT_TOOL_KEY,
            "status": agent["status"],
            "repositoryCount": agent["repositoryCount"],
            "succeededCount": agent["succeededCount"],
            "failedCount": agent["failedCount"],
        }),
    );
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command as StdCommand;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{git_agent_intent, read_commit_count, GitAgentIntent, KnowledgeGitAgentService};
    use crate::database::Database;
    use crate::models::{
        KnowledgeGitRefType, KnowledgeProjectVersionManifestInput, KnowledgeRepositoryBindingInput,
        KnowledgeVersionStrategy, ListAuditLogsInput, ProjectVersionRepositoryRefInput,
        RepositoryBindingInput, UpsertGitWorkspaceInput, UpsertKnowledgeProjectInput,
    };
    use crate::services::knowledge_domain::catalog::KnowledgeCatalogService;

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn run_git(repo: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let output = StdCommand::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }

    fn create_git_fixture() -> Result<(PathBuf, String), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-git-agent-{}-{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root)?;
        run_git(&root, &["init", "--quiet"])?;
        run_git(
            &root,
            &["config", "user.email", "agent-test@example.invalid"],
        )?;
        run_git(&root, &["config", "user.name", "Agent Test"])?;
        for index in 1..=2 {
            fs::write(root.join("README.md"), format!("commit {index}\n"))?;
            run_git(&root, &["add", "README.md"])?;
            run_git(
                &root,
                &["commit", "--quiet", "-m", &format!("commit {index}")],
            )?;
        }
        let sha = run_git(&root, &["rev-parse", "HEAD"])?;
        Ok((root, sha.trim().to_string()))
    }

    #[test]
    fn detects_commit_count_questions_without_misrouting_verification_questions() {
        assert_eq!(
            git_agent_intent("全业务工单开发以来进行了多少次 git 提交？"),
            Some(GitAgentIntent::CommitCount)
        );
        assert_eq!(
            git_agent_intent("v1.2.0 有多少个提交？"),
            Some(GitAgentIntent::CommitCount)
        );
        assert_eq!(
            git_agent_intent("统计当前版本各仓库提交数量"),
            Some(GitAgentIntent::CommitCount)
        );
        assert_eq!(git_agent_intent("这个功能通过代码提交验证了吗？"), None);
        assert_eq!(git_agent_intent("当前版本提交了多少个需求？"), None);
        assert_eq!(git_agent_intent("提交了多少份申请？"), None);
        assert_eq!(git_agent_intent("需求实现在哪个文件？"), None);
        assert_eq!(git_agent_intent("最近构建为什么失败？"), None);
    }

    #[tokio::test]
    async fn counts_only_commits_reachable_from_the_frozen_sha(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (repo, frozen_sha) = create_git_fixture()?;
        fs::write(repo.join("README.md"), "commit 3\n")?;
        run_git(&repo, &["add", "README.md"])?;
        run_git(&repo, &["commit", "--quiet", "-m", "commit 3"])?;

        assert_eq!(read_commit_count(&repo, &frozen_sha).await?, 2);
        let _ = fs::remove_dir_all(repo);
        Ok(())
    }

    #[tokio::test]
    async fn answers_with_dynamic_evidence_without_exposing_the_repository_path(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (repo, frozen_sha) = create_git_fixture()?;
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "git-agent-evidence".to_string(),
            name: "Git Agent 证据".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        database.upsert_git_workspace(
            &UpsertGitWorkspaceInput {
                id: None,
                workspace_key: "orders-api".to_string(),
                name: "订单服务".to_string(),
                repo_path: repo.to_string_lossy().into_owned(),
                credential_key: None,
                description: None,
            },
            "main",
            "",
            "clean",
            0,
            0,
            0,
        )?;
        let bindings = KnowledgeCatalogService::replace_repository_bindings(
            &database,
            KnowledgeRepositoryBindingInput {
                project_id: project.id,
                repositories: vec![RepositoryBindingInput {
                    workspace_key: "orders-api".to_string(),
                    alias: Some("订单服务".to_string()),
                    role: Some("service".to_string()),
                    default_branch: Some("main".to_string()),
                    version_strategy: KnowledgeVersionStrategy::TagOrBranch,
                }],
            },
        )?;
        let release = KnowledgeCatalogService::create_project_version_manifest(
            &database,
            KnowledgeProjectVersionManifestInput {
                project_id: project.id,
                version: "v1.2.0".to_string(),
                repositories: vec![ProjectVersionRepositoryRefInput {
                    repository_binding_id: bindings[0].id,
                    ref_type: KnowledgeGitRefType::Commit,
                    ref_name: frozen_sha.clone(),
                    excluded: false,
                }],
            },
        )
        .await?;

        // 冻结版本后再前进一个提交，回答仍必须停在清单记录的 SHA。
        fs::write(repo.join("README.md"), "commit 3\n")?;
        run_git(&repo, &["add", "README.md"])?;
        run_git(&repo, &["commit", "--quiet", "-m", "commit 3"])?;
        let result = KnowledgeGitAgentService::try_answer(
            &database,
            project.id,
            release.release_id,
            "v1.2.0",
            "开发以来进行了多少次 git 提交？",
        )
        .await?
        .expect("提交统计问题应由 Git Agent 接管");

        assert!(result.answer.contains("合计 **2 次**"));
        assert!(result.answer.contains("包含合并提交"));
        assert_eq!(result.citations.len(), 1);
        assert_eq!(result.citations[0].source_type, "git_statistics");
        assert_eq!(result.citations[0].commit_sha, frozen_sha);
        assert_eq!(result.retrieval_diagnostics["agent"]["totalCommitCount"], 2);
        assert!(!serde_json::to_string(&result)?.contains(&repo.to_string_lossy().as_ref()));
        let audits = database.list_audit_logs(&ListAuditLogsInput {
            actor: None,
            source: None,
            server_alias: None,
            action: Some("knowledge_git_agent_ask".to_string()),
            risk: None,
            result: None,
            keyword: None,
            limit: Some(10),
        })?;
        assert_eq!(audits.len(), 1);
        assert_eq!(audits[0].risk, "readonly");
        assert!(!audits[0]
            .detail_json
            .contains(&repo.to_string_lossy().as_ref()));
        let _ = fs::remove_dir_all(repo);
        Ok(())
    }

    #[tokio::test]
    async fn all_failed_repositories_keep_real_counts_and_safe_steps(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (repo, frozen_sha) = create_git_fixture()?;
        let database = Database::init(":memory:")?;
        let project = database.upsert_knowledge_project(&UpsertKnowledgeProjectInput {
            id: None,
            project_key: "git-agent-failed".to_string(),
            name: "Git Agent 失败统计".to_string(),
            aliases: Vec::new(),
            description: String::new(),
            git_workspace_keys: Vec::new(),
            git_workspace_key: String::new(),
            default_branch: "main".to_string(),
            enabled: true,
        })?;
        database.upsert_git_workspace(
            &UpsertGitWorkspaceInput {
                id: None,
                workspace_key: "missing-orders".to_string(),
                name: "缺失仓库".to_string(),
                repo_path: repo.to_string_lossy().into_owned(),
                credential_key: None,
                description: None,
            },
            "main",
            "",
            "clean",
            0,
            0,
            0,
        )?;
        let bindings = KnowledgeCatalogService::replace_repository_bindings(
            &database,
            KnowledgeRepositoryBindingInput {
                project_id: project.id,
                repositories: vec![RepositoryBindingInput {
                    workspace_key: "missing-orders".to_string(),
                    alias: None,
                    role: None,
                    default_branch: Some("main".to_string()),
                    version_strategy: KnowledgeVersionStrategy::TagOrBranch,
                }],
            },
        )?;
        let release = KnowledgeCatalogService::create_project_version_manifest(
            &database,
            KnowledgeProjectVersionManifestInput {
                project_id: project.id,
                version: "v1.0.0".to_string(),
                repositories: vec![ProjectVersionRepositoryRefInput {
                    repository_binding_id: bindings[0].id,
                    ref_type: KnowledgeGitRefType::Commit,
                    ref_name: frozen_sha,
                    excluded: false,
                }],
            },
        )
        .await?;
        fs::remove_dir_all(&repo)?;

        let result = KnowledgeGitAgentService::try_answer(
            &database,
            project.id,
            release.release_id,
            "v1.0.0",
            "git 提交一共多少次？",
        )
        .await?
        .expect("Git 问题应返回结构化失败");
        assert_eq!(result.retrieval_diagnostics["agent"]["repositoryCount"], 1);
        assert_eq!(result.retrieval_diagnostics["agent"]["failedCount"], 1);
        assert_eq!(
            result.retrieval_diagnostics["agent"]["steps"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert!(result.answer.contains("未返回可能误导的 0 次结果"));
        Ok(())
    }
}
