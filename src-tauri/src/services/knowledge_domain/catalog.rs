use std::path::Path;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use crate::database::knowledge_domain::catalog::NewKnowledgeReleaseRepositoryManifest;
use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    KnowledgeGitRefType, KnowledgeListInput, KnowledgePage, KnowledgeProject,
    KnowledgeProjectVersionCompleteness, KnowledgeProjectVersionManifestInput,
    KnowledgeProjectVersionManifestResult, KnowledgeRelease, KnowledgeRepositoryAvailability,
    KnowledgeRepositoryBinding, KnowledgeRepositoryBindingInput, ProjectVersionRepositoryRefInput,
    UpsertKnowledgeProjectInput, UpsertKnowledgeReleaseInput,
};
use crate::services::knowledge::{
    audit_knowledge, empty_list_input, normalize_key, normalized_unique_values, required_text,
    validate_positive_id,
};
use crate::services::knowledge_rollout::KnowledgeRolloutService;

/// 项目与多仓库目录的唯一业务入口。旧 `KnowledgeService` 仅保留兼容转发，避免两个
/// Service 各自实现项目校验、审计和发布开关。
pub struct KnowledgeCatalogService;

impl KnowledgeCatalogService {
    pub fn list_projects(
        db: &Database,
        input: Option<KnowledgeListInput>,
    ) -> Result<KnowledgePage<KnowledgeProject>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        db.list_knowledge_projects(&input.unwrap_or_else(empty_list_input))
    }

    pub fn upsert_project(
        db: &Database,
        mut input: UpsertKnowledgeProjectInput,
    ) -> Result<KnowledgeProject, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        input.project_key = normalize_key(&input.project_key, "项目标识")?;
        input.name = required_text(&input.name, "项目名称")?;
        if let Some(project) = db.get_knowledge_project_by_key(&input.project_key)? {
            if let Some(id) = input.id {
                if id != project.id {
                    return Err(AppError::InvalidInput(
                        "项目 ID 与项目标识不匹配，不能覆盖其他项目".to_string(),
                    ));
                }
            }
            // 客户端收到写入成功前断开时会带着原 projectKey 重试。先归并为已有 ID，
            // 让后续同名校验排除自己，而不是把安全重试误判为重名新建。
            input.id = Some(project.id);
        } else if let Some(id) = input.id {
            let project = db
                .get_knowledge_project_by_id(id)?
                .ok_or_else(|| AppError::NotFound(format!("知识项目不存在: {id}")))?;
            if project.project_key != input.project_key {
                return Err(AppError::InvalidInput(
                    "项目 ID 与项目标识不匹配，不能修改已有项目的稳定标识".to_string(),
                ));
            }
        }
        if db.knowledge_project_name_taken(&input.name, input.id)? {
            return Err(AppError::InvalidInput(
                "项目名称已存在，请修改名称后再保存".to_string(),
            ));
        }
        input.aliases = normalized_unique_values(input.aliases);
        input.git_workspace_keys = normalized_unique_values(input.git_workspace_keys);
        input.git_workspace_key = input.git_workspace_key.trim().to_string();
        if input.git_workspace_keys.is_empty() && !input.git_workspace_key.is_empty() {
            input.git_workspace_keys = vec![input.git_workspace_key.clone()];
        }
        for workspace_key in &input.git_workspace_keys {
            if db.get_git_workspace(workspace_key)?.is_none() {
                return Err(AppError::NotFound(format!(
                    "Git 工作区不存在或尚未加载: {workspace_key}"
                )));
            }
        }
        input.git_workspace_key = input
            .git_workspace_keys
            .first()
            .cloned()
            .unwrap_or_default();
        input.default_branch = input.default_branch.trim().to_string();
        let project = db.upsert_knowledge_project(&input)?;
        audit_knowledge(
            db,
            "knowledge_project_upsert",
            "L1",
            "成功",
            "保存知识项目配置",
            serde_json::json!({"projectId": project.id, "projectKey": project.project_key}),
        );
        Ok(project)
    }

    pub fn delete_project(db: &Database, id: i64) -> Result<(), AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(id, "项目 ID")?;
        db.soft_delete_knowledge_project(id)?;
        audit_knowledge(
            db,
            "knowledge_project_delete",
            "L2",
            "成功",
            "删除知识项目",
            serde_json::json!({"projectId": id}),
        );
        Ok(())
    }

    pub fn list_releases(
        db: &Database,
        project_id: i64,
    ) -> Result<Vec<KnowledgeRelease>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(project_id, "项目 ID")?;
        db.list_knowledge_releases(project_id)
    }

    /// 重新打开已创建的版本时，只从不可变清单读取仓库 Commit 证据，不能用旧发布表的
    /// tag/branch/commit 字段猜测多仓库范围。
    pub fn get_project_version_manifest(
        db: &Database,
        release_id: i64,
    ) -> Result<KnowledgeProjectVersionManifestResult, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(release_id, "版本 ID")?;
        let release = db
            .get_knowledge_release_by_id(release_id)?
            .ok_or_else(|| AppError::NotFound(format!("知识版本不存在: {release_id}")))?;
        let repositories = db.list_knowledge_release_repository_manifests(release_id)?;
        if repositories.is_empty() {
            return Err(AppError::NotFound(format!(
                "项目版本尚未创建多仓库清单: {release_id}"
            )));
        }
        let status = if repositories
            .iter()
            .all(|manifest| matches!(manifest.inclusion_status.as_str(), "ready" | "excluded"))
        {
            "ready"
        } else {
            "partial"
        };
        Ok(KnowledgeProjectVersionManifestResult {
            release_id: release.id,
            project_id: release.project_id,
            version: release.version,
            status: status.to_string(),
            repositories,
        })
    }

    pub fn get_project_version_completeness(
        db: &Database,
        release_id: i64,
    ) -> Result<KnowledgeProjectVersionCompleteness, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(release_id, "版本 ID")?;
        db.get_knowledge_project_version_completeness(release_id)
    }

    pub fn upsert_release(
        db: &Database,
        mut input: UpsertKnowledgeReleaseInput,
    ) -> Result<KnowledgeRelease, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(input.project_id, "项目 ID")?;
        input.version = required_text(&input.version, "版本号")?;
        input.tag_name = input.tag_name.trim().to_string();
        input.branch = input.branch.trim().to_string();
        input.commit_sha = input.commit_sha.trim().to_string();
        if input.version.eq_ignore_ascii_case("unversioned") {
            input.tag_name.clear();
            input.commit_sha.clear();
        }
        let release = db.upsert_knowledge_release(&input)?;
        audit_knowledge(
            db,
            "knowledge_release_upsert",
            "L1",
            "成功",
            "保存知识版本配置",
            serde_json::json!({"releaseId": release.id, "projectId": release.project_id, "version": release.version}),
        );
        Ok(release)
    }

    pub fn delete_release(db: &Database, id: i64) -> Result<(), AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(id, "版本 ID")?;
        db.soft_delete_knowledge_release(id)?;
        audit_knowledge(
            db,
            "knowledge_release_delete",
            "L2",
            "成功",
            "删除知识版本",
            serde_json::json!({"releaseId": id}),
        );
        Ok(())
    }

    /// 只有已登记的本地 Git 工作区可以进入项目目录，路径不会从 IPC 传入。
    pub fn replace_repository_bindings(
        db: &Database,
        input: KnowledgeRepositoryBindingInput,
    ) -> Result<Vec<KnowledgeRepositoryBinding>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(input.project_id, "项目 ID")?;
        if !db.knowledge_project_exists(input.project_id)? {
            return Err(AppError::NotFound(format!(
                "知识项目不存在: {}",
                input.project_id
            )));
        }
        for repository in &input.repositories {
            let workspace_key = required_text(&repository.workspace_key, "Git 工作区标识")?;
            if db.get_git_workspace(&workspace_key)?.is_none() {
                return Err(AppError::NotFound(format!(
                    "Git 工作区不存在或尚未加载: {workspace_key}"
                )));
            }
        }
        let project_id = input.project_id;
        let bindings = db.replace_knowledge_project_repository_bindings(&input)?;
        audit_knowledge(
            db,
            "knowledge_project_repository_bindings_replace",
            "L1",
            "成功",
            "更新项目关联仓库",
            serde_json::json!({"projectId": project_id, "repositoryCount": bindings.len()}),
        );
        Ok(bindings)
    }

    pub fn list_repository_bindings(
        db: &Database,
        project_id: i64,
    ) -> Result<Vec<KnowledgeRepositoryBinding>, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(project_id, "项目 ID")?;
        db.list_knowledge_project_repository_bindings(project_id)
    }

    pub fn unlink_repository_binding(
        db: &Database,
        repository_binding_id: i64,
    ) -> Result<(), AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(repository_binding_id, "仓库关联 ID")?;
        let binding = db
            .get_knowledge_project_repository_binding(repository_binding_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("仓库关联不存在: {repository_binding_id}"))
            })?;
        db.deactivate_knowledge_project_repository_binding(repository_binding_id)?;
        audit_knowledge(
            db,
            "knowledge_project_repository_binding_unlink",
            "L2",
            "成功",
            "解除项目仓库关联",
            serde_json::json!({"projectId": binding.project_id, "repositoryBindingId": repository_binding_id}),
        );
        Ok(())
    }

    /// 只对当前有效关联执行固定的 Git 只读命令，既不接收命令参数，也不执行 Hook、脚本、
    /// 检出、暂存、重置、清理、拉取或推送。
    pub async fn inspect_repository_binding(
        db: &Database,
        repository_binding_id: i64,
    ) -> Result<KnowledgeRepositoryAvailability, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(repository_binding_id, "仓库关联 ID")?;
        let binding = db
            .get_knowledge_project_repository_binding(repository_binding_id)?
            .ok_or_else(|| {
                AppError::NotFound(format!("仓库关联不存在: {repository_binding_id}"))
            })?;
        let workspace = db
            .get_git_workspace(&binding.workspace_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("Git 工作区不存在: {}", binding.workspace_key))
            })?;
        let repo = Path::new(&workspace.repo_path);
        if !repo.is_dir() || !repo.join(".git").exists() {
            return Ok(unavailable_repository(
                &binding,
                "仓库目录不可用或不再是 Git 仓库",
            ));
        }
        if read_catalog_git(repo, CatalogGitReadOperation::Probe)
            .await
            .is_err()
        {
            return Ok(unavailable_repository(&binding, "Git 仓库无法读取"));
        }

        let status = read_catalog_git(repo, CatalogGitReadOperation::Status).await?;
        let branch = read_catalog_git(repo, CatalogGitReadOperation::Branch)
            .await
            .unwrap_or_default()
            .trim()
            .to_string();
        let head_commit = read_catalog_git(repo, CatalogGitReadOperation::Head)
            .await
            .unwrap_or_default()
            .trim()
            .to_string();
        let changed_file_count = porcelain_record_count(status.as_bytes());
        Ok(KnowledgeRepositoryAvailability {
            repository_binding_id: binding.id,
            workspace_key: binding.workspace_key,
            available: true,
            branch,
            head_commit,
            dirty: changed_file_count > 0,
            changed_file_count,
            message: if changed_file_count > 0 {
                "仓库可读取，存在未提交改动；创建版本时将提示确认。".to_string()
            } else {
                "仓库可读取，工作区无未提交改动。".to_string()
            },
        })
    }

    /// 以项目当前活动仓库为全集创建不可变版本清单。分支只在此刻解析为 Commit；后续分支
    /// 前进不会回写历史清单。任何未映射、跨项目映射或 Git 解析失败都会在写库前拒绝。
    pub async fn create_project_version_manifest(
        db: &Database,
        input: KnowledgeProjectVersionManifestInput,
    ) -> Result<KnowledgeProjectVersionManifestResult, AppError> {
        KnowledgeRolloutService::require(db, "catalog")?;
        validate_positive_id(input.project_id, "项目 ID")?;
        let version = required_text(&input.version, "版本号")?;
        if !db.knowledge_project_exists(input.project_id)? {
            return Err(AppError::NotFound(format!(
                "知识项目不存在: {}",
                input.project_id
            )));
        }

        let mut requested = normalized_manifest_references(&input.repositories)?;
        let requested_for_retry = requested.clone();
        if let Some(existing) =
            existing_manifest_retry_result(db, input.project_id, &version, &requested_for_retry)?
        {
            return Ok(existing);
        }

        let bindings = db.list_knowledge_project_repository_bindings(input.project_id)?;
        validate_complete_manifest_binding_coverage(&bindings, &requested)?;
        let mut manifests = Vec::with_capacity(bindings.len());
        for binding in bindings {
            let repository = requested
                .remove(&binding.id)
                .ok_or_else(|| AppError::InvalidInput("版本清单缺少仓库映射".to_string()))?;
            if repository.excluded {
                manifests.push(NewKnowledgeReleaseRepositoryManifest {
                    repository_binding_id: binding.id,
                    requested_ref_type: repository.ref_type,
                    requested_ref_name: repository.ref_name,
                    resolved_commit_sha: String::new(),
                    inclusion_status: "excluded".to_string(),
                    exclusion_reason: "用户在版本清单中明确排除该仓库".to_string(),
                    worktree_dirty: false,
                });
                continue;
            }

            let workspace = db
                .get_git_workspace(&binding.workspace_key)?
                .ok_or_else(|| {
                    AppError::NotFound(format!("Git 工作区不存在: {}", binding.workspace_key))
                })?;
            let repo = Path::new(&workspace.repo_path);
            if !repo.is_dir() || !repo.join(".git").exists() {
                return Err(AppError::InvalidInput(format!(
                    "仓库“{}”目录不可用或不再是 Git 仓库",
                    binding.alias
                )));
            }
            let resolved_commit_sha =
                resolve_catalog_git_commit(repo, repository.ref_type, &repository.ref_name).await?;
            let status = read_catalog_git(repo, CatalogGitReadOperation::Status).await?;
            let worktree_dirty = porcelain_record_count(status.as_bytes()) > 0;
            manifests.push(NewKnowledgeReleaseRepositoryManifest {
                repository_binding_id: binding.id,
                requested_ref_type: repository.ref_type,
                requested_ref_name: repository.ref_name,
                resolved_commit_sha,
                inclusion_status: "ready".to_string(),
                exclusion_reason: String::new(),
                worktree_dirty,
            });
        }

        if !manifests
            .iter()
            .any(|manifest| manifest.inclusion_status == "ready")
        {
            return Err(AppError::InvalidInput(
                "项目版本至少需要包含一个未排除的仓库".to_string(),
            ));
        }
        let (release, repositories) = match db.create_knowledge_release_with_repository_manifests(
            input.project_id,
            &version,
            &manifests,
        ) {
            Ok(created) => created,
            Err(error) => {
                // 两个客户端同时提交相同版本时，其中一个事务可能在首次读取后才遇到
                // 唯一键冲突。重新读取并严格比对请求，只有完全一致时才能恢复成功。
                if let Some(existing) = existing_manifest_retry_result(
                    db,
                    input.project_id,
                    &version,
                    &requested_for_retry,
                )? {
                    return Ok(existing);
                }
                return Err(error);
            }
        };
        audit_knowledge(
            db,
            "knowledge_project_version_manifest_create",
            "L1",
            "成功",
            "创建项目版本不可变清单",
            serde_json::json!({"projectId": input.project_id, "releaseId": release.id, "version": version}),
        );
        Ok(KnowledgeProjectVersionManifestResult {
            release_id: release.id,
            project_id: release.project_id,
            version: release.version,
            status: "ready".to_string(),
            repositories,
        })
    }
}

#[derive(Debug, Clone)]
struct NormalizedManifestReference {
    ref_type: KnowledgeGitRefType,
    ref_name: String,
    excluded: bool,
}

/// 将客户端输入收敛为可持久化比对的版本身份。已创建版本的重试不应再次访问 Git，
/// 因此这里仅执行纯输入校验与规范化；Git 解析仍只发生在首次创建路径。
fn normalized_manifest_references(
    repositories: &[ProjectVersionRepositoryRefInput],
) -> Result<std::collections::HashMap<i64, NormalizedManifestReference>, AppError> {
    let mut requested = std::collections::HashMap::with_capacity(repositories.len());
    for repository in repositories {
        validate_positive_id(repository.repository_binding_id, "仓库关联 ID")?;
        let ref_name = repository.ref_name.trim().to_string();
        if repository.excluded {
            if !ref_name.is_empty() {
                return Err(AppError::InvalidInput(
                    "已排除仓库不能再填写 Git 引用".to_string(),
                ));
            }
        } else {
            // 保持与首次创建完全相同的引用规范化，避免空白或不安全引用在重试时
            // 被误认为另一个版本身份。
            validate_manifest_git_reference(repository.ref_type, &ref_name)?;
        }
        if requested
            .insert(
                repository.repository_binding_id,
                NormalizedManifestReference {
                    ref_type: repository.ref_type,
                    ref_name,
                    excluded: repository.excluded,
                },
            )
            .is_some()
        {
            return Err(AppError::InvalidInput(
                "一个仓库只能配置一次版本引用".to_string(),
            ));
        }
    }
    Ok(requested)
}

/// 版本清单的仓库 ID 必须与创建时项目的全部活动关联严格相等。排除只是不读取该
/// 仓库，不能省略它；这样重试不会借由旧清单遗漏后来关联的仓库范围。
fn validate_complete_manifest_binding_coverage(
    bindings: &[KnowledgeRepositoryBinding],
    requested: &std::collections::HashMap<i64, NormalizedManifestReference>,
) -> Result<(), AppError> {
    if bindings.is_empty() {
        return Err(AppError::InvalidInput(
            "请先为项目关联至少一个 Git 仓库，再创建版本".to_string(),
        ));
    }
    if requested.len() != bindings.len() {
        return Err(AppError::InvalidInput(
            "每个当前关联仓库都需要选择引用或明确排除".to_string(),
        ));
    }
    if bindings
        .iter()
        .any(|binding| !requested.contains_key(&binding.id))
        || requested
            .keys()
            .any(|id| !bindings.iter().any(|binding| binding.id == *id))
    {
        return Err(AppError::InvalidInput(
            "版本清单只能覆盖当前项目的全部活动仓库".to_string(),
        ));
    }
    Ok(())
}

/// 仅当完整的仓库引用与已冻结清单一致时，才把重试归并为既有成功结果。任何差异都
/// 明确拒绝，既不重新解析移动分支，也不修改历史 Commit 或仓库包含范围。
fn existing_manifest_retry_result(
    db: &Database,
    project_id: i64,
    version: &str,
    requested: &std::collections::HashMap<i64, NormalizedManifestReference>,
) -> Result<Option<KnowledgeProjectVersionManifestResult>, AppError> {
    // 必须在每一次“已有清单重试”判断前读取当前活动绑定。否则旧/异常的部分清单
    // 会在项目已新增仓库后被错误地当作完整范围复用；此校验只访问 SQLite，不读 Git。
    let bindings = db.list_knowledge_project_repository_bindings(project_id)?;
    validate_complete_manifest_binding_coverage(&bindings, requested)?;
    let Some(release) = db.get_knowledge_release_by_project_and_version(project_id, version)?
    else {
        return Ok(None);
    };
    let repositories = db.list_knowledge_release_repository_manifests(release.id)?;
    let references_match = repositories.len() == requested.len()
        && repositories.iter().all(|manifest| {
            let Some(reference) = requested.get(&manifest.repository_binding_id) else {
                return false;
            };
            let excluded = match manifest.inclusion_status.as_str() {
                "ready" => false,
                "excluded" => true,
                _ => return false,
            };
            reference.ref_type == manifest.requested_ref_type
                && reference.ref_name == manifest.requested_ref_name
                && reference.excluded == excluded
        });
    if !references_match {
        return Err(AppError::InvalidInput(format!(
            "项目版本已存在且仓库引用不一致: {}；请使用新的版本号创建新清单",
            release.version
        )));
    }

    // 旧发布记录可能没有多仓库清单。它不能被当前接口补写，否则会把历史版本从
    // “未知范围”伪造为当前范围；因此只有完整冻结清单才具备可重试资格。
    if repositories.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "项目版本已存在但不是可重试的不可变清单: {}；请使用新的版本号创建新清单",
            release.version
        )));
    }
    let status = if repositories
        .iter()
        .all(|manifest| matches!(manifest.inclusion_status.as_str(), "ready" | "excluded"))
    {
        "ready"
    } else {
        "partial"
    };
    Ok(Some(KnowledgeProjectVersionManifestResult {
        release_id: release.id,
        project_id: release.project_id,
        version: release.version,
        status: status.to_string(),
        repositories,
    }))
}

/// 仅枚举不可变的只读 Git 子命令；未通过显式引用校验的 IPC 字符串不得进入 `Command` 参数。
enum CatalogGitReadOperation {
    Probe,
    Status,
    Branch,
    Head,
}

async fn read_catalog_git(
    repo: &Path,
    operation: CatalogGitReadOperation,
) -> Result<String, AppError> {
    let args: &[&str] = match operation {
        CatalogGitReadOperation::Probe => &["rev-parse", "--is-inside-work-tree"],
        CatalogGitReadOperation::Status => {
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"]
        }
        CatalogGitReadOperation::Branch => &["branch", "--show-current"],
        CatalogGitReadOperation::Head => &["rev-parse", "--verify", "HEAD^{commit}"],
    };
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .kill_on_drop(true);
    let output = timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| AppError::Custom("读取 Git 状态超时".to_string()))??;
    if !output.status.success() {
        return Err(AppError::Custom("读取 Git 状态失败".to_string()));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| AppError::Custom("Git 状态输出不是有效 UTF-8".to_string()))
}

fn validate_manifest_git_reference(
    ref_type: KnowledgeGitRefType,
    value: &str,
) -> Result<String, AppError> {
    let value = required_text(value, "Git 引用")?;
    let safe_common = !value.starts_with('-')
        && !value.contains("..")
        && !value.contains("@{")
        && !value.contains('\\')
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control());
    let valid = match ref_type {
        KnowledgeGitRefType::Commit => {
            value.len() >= 7
                && value.len() <= 64
                && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        }
        KnowledgeGitRefType::Branch | KnowledgeGitRefType::Tag => {
            safe_common
                && !value.starts_with('/')
                && !value.ends_with('/')
                && !value.ends_with('.')
                && !value.contains("//")
                && !value
                    .chars()
                    .any(|character| matches!(character, '~' | '^' | ':' | '?' | '*' | '['))
        }
    };
    if !valid {
        return Err(AppError::InvalidInput(
            "Git 引用格式不安全或不完整".to_string(),
        ));
    }
    Ok(value)
}

async fn resolve_catalog_git_commit(
    repo: &Path,
    ref_type: KnowledgeGitRefType,
    ref_name: &str,
) -> Result<String, AppError> {
    let qualified_ref = match ref_type {
        // 工作区首次登记可能尚未完成状态刷新，此时历史数据会以 HEAD 表示“当前
        // 检出位置”。HEAD 不是 refs/heads 下的分支名，必须直接解析，避免把
        // 普通用户的安全默认值误判为不存在的分支。
        KnowledgeGitRefType::Branch if ref_name == "HEAD" => "HEAD^{commit}".to_string(),
        KnowledgeGitRefType::Branch => format!("refs/heads/{ref_name}^{{commit}}"),
        KnowledgeGitRefType::Tag => format!("refs/tags/{ref_name}^{{commit}}"),
        KnowledgeGitRefType::Commit => format!("{ref_name}^{{commit}}"),
    };
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .arg("-C")
        .arg(repo)
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            qualified_ref.as_str(),
        ])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .kill_on_drop(true);
    let output = timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| AppError::Custom("解析 Git 引用超时".to_string()))??;
    if !output.status.success() {
        return Err(AppError::InvalidInput(format!(
            "Git 中找不到所选引用: {ref_name}"
        )));
    }
    let commit_sha = String::from_utf8(output.stdout)
        .map_err(|_| AppError::Custom("Git Commit 输出不是有效 UTF-8".to_string()))?
        .trim()
        .to_string();
    if commit_sha.len() != 40 || !commit_sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::Custom("Git 未返回有效 Commit SHA".to_string()));
    }
    Ok(commit_sha)
}

fn porcelain_record_count(output: &[u8]) -> u32 {
    output
        .split(|byte| *byte == 0)
        .filter(|record| record.len() >= 3 && (record[0] != b' ' || record[1] != b' '))
        .count()
        .try_into()
        .unwrap_or(u32::MAX)
}

fn unavailable_repository(
    binding: &KnowledgeRepositoryBinding,
    message: &str,
) -> KnowledgeRepositoryAvailability {
    KnowledgeRepositoryAvailability {
        repository_binding_id: binding.id,
        workspace_key: binding.workspace_key.clone(),
        available: false,
        branch: String::new(),
        head_commit: String::new(),
        dirty: false,
        changed_file_count: 0,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::KnowledgeCatalogService;
    use crate::database::Database;
    use crate::models::{
        KnowledgeGitRefType, KnowledgeProjectVersionManifestInput, KnowledgeRepositoryBindingInput,
        KnowledgeVersionStrategy, ProjectVersionRepositoryRefInput, RepositoryBindingInput,
        UpsertGitWorkspaceInput, UpsertKnowledgeProjectInput, UpsertKnowledgeReleaseInput,
    };
    use crate::services::knowledge::KnowledgeService;

    static GIT_FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn project_input() -> UpsertKnowledgeProjectInput {
        UpsertKnowledgeProjectInput {
            id: None,
            project_key: "order-center".to_string(),
            name: "订单中心".to_string(),
            aliases: vec![],
            description: String::new(),
            git_workspace_key: String::new(),
            git_workspace_keys: vec![],
            default_branch: "main".to_string(),
            enabled: true,
        }
    }

    fn run_git(repo: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "Git fixture command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }

    fn create_git_fixture() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "tauri-knowledge-catalog-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            GIT_FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&root)?;
        run_git(&root, &["init", "--quiet"])?;
        run_git(
            &root,
            &["config", "user.email", "catalog-test@example.invalid"],
        )?;
        run_git(&root, &["config", "user.name", "Catalog Test"])?;
        fs::write(root.join("README.md"), "# fixture\n")?;
        run_git(&root, &["add", "README.md"])?;
        run_git(&root, &["commit", "--quiet", "-m", "initial"])?;
        Ok(root)
    }

    #[test]
    fn legacy_project_facade_and_new_binding_service_share_the_same_catalog_rules(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        let project = KnowledgeService::upsert_project(&db, project_input())?;
        db.upsert_git_workspace(
            &UpsertGitWorkspaceInput {
                id: None,
                workspace_key: "orders-api".to_string(),
                name: "订单服务".to_string(),
                repo_path: "/tmp/orders-api".to_string(),
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
            &db,
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
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].workspace_key, "orders-api");
        assert_eq!(
            bindings[0].version_strategy,
            KnowledgeVersionStrategy::TagOrBranch
        );

        // 旧兼容方法读取的是同一个项目和 ID，不需要迁移调用方的 JSON 外观。
        let legacy_page = KnowledgeService::list_projects(&db, None)?;
        assert_eq!(legacy_page.items[0].id, project.id);
        assert_eq!(
            KnowledgeCatalogService::list_repository_bindings(&db, project.id)?.len(),
            1
        );
        let legacy_release = KnowledgeService::upsert_release(
            &db,
            UpsertKnowledgeReleaseInput {
                id: None,
                project_id: project.id,
                version: "v1.0.0".to_string(),
                tag_name: "v1.0.0".to_string(),
                branch: "main".to_string(),
                commit_sha: "".to_string(),
                description: "旧接口创建的版本".to_string(),
                released_at: None,
            },
        )?;
        let legacy_releases = KnowledgeService::list_releases(&db, project.id)?;
        assert_eq!(legacy_releases[0].id, legacy_release.id);
        let project_json = serde_json::to_value(&project)?;
        let release_json = serde_json::to_value(&legacy_release)?;
        assert!(project_json.get("projectKey").is_some());
        assert!(project_json.get("gitWorkspaceKeys").is_some());
        assert!(project_json.get("project_key").is_none());
        assert!(release_json.get("projectId").is_some());
        assert!(release_json.get("tagName").is_some());
        assert!(release_json.get("project_id").is_none());
        KnowledgeCatalogService::unlink_repository_binding(&db, bindings[0].id)?;
        assert!(KnowledgeCatalogService::list_repository_bindings(&db, project.id)?.is_empty());
        Ok(())
    }

    #[test]
    fn unregistered_workspace_is_rejected_before_active_bindings_change(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        let project = KnowledgeCatalogService::upsert_project(&db, project_input())?;
        let error = KnowledgeCatalogService::replace_repository_bindings(
            &db,
            KnowledgeRepositoryBindingInput {
                project_id: project.id,
                repositories: vec![RepositoryBindingInput {
                    workspace_key: "missing-workspace".to_string(),
                    alias: None,
                    role: None,
                    default_branch: None,
                    version_strategy: KnowledgeVersionStrategy::Manual,
                }],
            },
        )
        .expect_err("未登记工作区不应写入项目目录");
        assert!(error.to_string().contains("Git 工作区不存在"));
        assert!(KnowledgeCatalogService::list_repository_bindings(&db, project.id)?.is_empty());
        Ok(())
    }

    #[test]
    fn project_key_retry_reuses_id_and_same_name_with_other_key_is_rejected_without_data_loss(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::init(":memory:")?;
        let project = KnowledgeCatalogService::upsert_project(&db, project_input())?;
        let retry = KnowledgeCatalogService::upsert_project(&db, project_input())?;
        assert_eq!(
            retry.id, project.id,
            "同 projectKey 且未携带 ID 的重试必须复用项目"
        );
        assert_eq!(
            KnowledgeCatalogService::list_projects(&db, None)?.total,
            1,
            "安全重试不能创建第二个项目"
        );
        let duplicate_error = KnowledgeCatalogService::upsert_project(
            &db,
            UpsertKnowledgeProjectInput {
                project_key: "order-center-copy".to_string(),
                ..project_input()
            },
        )
        .expect_err("相同项目名称必须被拒绝");
        assert!(duplicate_error.to_string().contains("项目名称"));

        db.upsert_git_workspace(
            &UpsertGitWorkspaceInput {
                id: None,
                workspace_key: "orders-api".to_string(),
                name: "订单服务".to_string(),
                repo_path: "/tmp/orders-api".to_string(),
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
        let original = KnowledgeCatalogService::replace_repository_bindings(
            &db,
            KnowledgeRepositoryBindingInput {
                project_id: project.id,
                repositories: vec![RepositoryBindingInput {
                    workspace_key: "orders-api".to_string(),
                    alias: None,
                    role: None,
                    default_branch: None,
                    version_strategy: KnowledgeVersionStrategy::Manual,
                }],
            },
        )?;
        let partial_error = KnowledgeCatalogService::replace_repository_bindings(
            &db,
            KnowledgeRepositoryBindingInput {
                project_id: project.id,
                repositories: vec![
                    RepositoryBindingInput {
                        workspace_key: "orders-api".to_string(),
                        alias: None,
                        role: None,
                        default_branch: None,
                        version_strategy: KnowledgeVersionStrategy::Manual,
                    },
                    RepositoryBindingInput {
                        workspace_key: "missing-api".to_string(),
                        alias: None,
                        role: None,
                        default_branch: None,
                        version_strategy: KnowledgeVersionStrategy::Manual,
                    },
                ],
            },
        )
        .expect_err("批量关联必须先完成全部工作区校验");
        assert!(partial_error.to_string().contains("Git 工作区不存在"));
        let retained = KnowledgeCatalogService::list_repository_bindings(&db, project.id)?;
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].id, original[0].id);
        Ok(())
    }

    #[tokio::test]
    async fn readonly_repository_inspection_reports_dirty_state_without_changing_worktree(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let repo = create_git_fixture()?;
        let result = async {
            let db = Database::init(":memory:")?;
            let project = KnowledgeCatalogService::upsert_project(&db, project_input())?;
            db.upsert_git_workspace(
                &UpsertGitWorkspaceInput {
                    id: None,
                    workspace_key: "orders-api".to_string(),
                    name: "订单服务".to_string(),
                    repo_path: repo.to_string_lossy().to_string(),
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
            let binding = KnowledgeCatalogService::replace_repository_bindings(
                &db,
                KnowledgeRepositoryBindingInput {
                    project_id: project.id,
                    repositories: vec![RepositoryBindingInput {
                        workspace_key: "orders-api".to_string(),
                        alias: None,
                        role: None,
                        default_branch: None,
                        version_strategy: KnowledgeVersionStrategy::Branch,
                    }],
                },
            )?
            .remove(0);
            fs::write(repo.join("README.md"), "# fixture\nchanged\n")?;
            let before = run_git(&repo, &["status", "--porcelain=v1"])?;
            let inspected =
                KnowledgeCatalogService::inspect_repository_binding(&db, binding.id).await?;
            let after = run_git(&repo, &["status", "--porcelain=v1"])?;
            assert!(inspected.available);
            assert!(inspected.dirty);
            assert_eq!(inspected.changed_file_count, 1);
            assert_eq!(before, after, "只读探测不得改变工作区状态");
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        .await;
        fs::remove_dir_all(&repo)?;
        result
    }

    #[tokio::test]
    async fn project_version_manifest_resolves_tag_and_keeps_branch_history_immutable(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let first_repo = create_git_fixture()?;
        let second_repo = create_git_fixture()?;
        let result = async {
            let db = Database::init(":memory:")?;
            let project = KnowledgeCatalogService::upsert_project(&db, project_input())?;
            for (key, name, repo) in [
                ("orders-api", "订单服务", &first_repo),
                ("gateway-api", "网关服务", &second_repo),
            ] {
                db.upsert_git_workspace(
                    &UpsertGitWorkspaceInput {
                        id: None,
                        workspace_key: key.to_string(),
                        name: name.to_string(),
                        repo_path: repo.to_string_lossy().to_string(),
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
                run_git(repo, &["tag", "v1.0.0"])?;
            }
            let bindings = KnowledgeCatalogService::replace_repository_bindings(
                &db,
                KnowledgeRepositoryBindingInput {
                    project_id: project.id,
                    repositories: vec![
                        RepositoryBindingInput {
                            workspace_key: "orders-api".to_string(),
                            alias: None,
                            role: None,
                            default_branch: None,
                            version_strategy: KnowledgeVersionStrategy::TagOrBranch,
                        },
                        RepositoryBindingInput {
                            workspace_key: "gateway-api".to_string(),
                            alias: None,
                            role: None,
                            default_branch: None,
                            version_strategy: KnowledgeVersionStrategy::TagOrBranch,
                        },
                    ],
                },
            )?;
            let tagged = KnowledgeCatalogService::create_project_version_manifest(
                &db,
                KnowledgeProjectVersionManifestInput {
                    project_id: project.id,
                    version: "v1.0.0".to_string(),
                    repositories: bindings
                        .iter()
                        .map(|binding| ProjectVersionRepositoryRefInput {
                            repository_binding_id: binding.id,
                            ref_type: KnowledgeGitRefType::Tag,
                            ref_name: "v1.0.0".to_string(),
                            excluded: false,
                        })
                        .collect(),
                },
            )
            .await?;
            assert_eq!(tagged.status, "ready");
            assert!(tagged
                .repositories
                .iter()
                .all(|item| item.resolved_commit_sha.len() == 40));
            let reopened =
                KnowledgeCatalogService::get_project_version_manifest(&db, tagged.release_id)?;
            assert_eq!(reopened.repositories.len(), tagged.repositories.len());
            assert!(reopened
                .repositories
                .iter()
                .zip(&tagged.repositories)
                .all(|(reopened, created)| reopened.resolved_commit_sha
                    == created.resolved_commit_sha));

            // 已登记工作区的异步状态刷新尚未完成时会保留 HEAD；创建项目的默认流程
            // 仍必须把它冻结为当前 Commit，而不能查找不存在的 refs/heads/HEAD。
            let head_manifest = KnowledgeCatalogService::create_project_version_manifest(
                &db,
                KnowledgeProjectVersionManifestInput {
                    project_id: project.id,
                    version: "head-current".to_string(),
                    repositories: vec![
                        ProjectVersionRepositoryRefInput {
                            repository_binding_id: bindings[0].id,
                            ref_type: KnowledgeGitRefType::Branch,
                            ref_name: "HEAD".to_string(),
                            excluded: false,
                        },
                        ProjectVersionRepositoryRefInput {
                            repository_binding_id: bindings[1].id,
                            ref_type: KnowledgeGitRefType::Tag,
                            ref_name: "v1.0.0".to_string(),
                            excluded: false,
                        },
                    ],
                },
            )
            .await?;
            assert_eq!(
                head_manifest.repositories[0].resolved_commit_sha,
                run_git(&first_repo, &["rev-parse", "HEAD"])?.trim(),
                "HEAD 默认值必须冻结当前 Commit"
            );
            let legacy_update = KnowledgeService::upsert_release(
                &db,
                UpsertKnowledgeReleaseInput {
                    id: Some(tagged.release_id),
                    project_id: project.id,
                    version: "overwritten-version".to_string(),
                    tag_name: String::new(),
                    branch: String::new(),
                    commit_sha: String::new(),
                    description: "不应修改已冻结清单".to_string(),
                    released_at: None,
                },
            )
            .expect_err("旧发布更新不得改写已冻结清单");
            assert!(legacy_update.to_string().contains("已冻结项目版本"));
            let legacy_delete = KnowledgeService::delete_release(&db, tagged.release_id)
                .expect_err("旧发布删除不得隐藏已冻结清单");
            assert!(legacy_delete.to_string().contains("已冻结项目版本"));

            let branch = run_git(&first_repo, &["branch", "--show-current"])?
                .trim()
                .to_string();
            let before = KnowledgeCatalogService::create_project_version_manifest(
                &db,
                KnowledgeProjectVersionManifestInput {
                    project_id: project.id,
                    version: "branch-before".to_string(),
                    repositories: vec![
                        ProjectVersionRepositoryRefInput {
                            repository_binding_id: bindings[0].id,
                            ref_type: KnowledgeGitRefType::Branch,
                            ref_name: branch.clone(),
                            excluded: false,
                        },
                        ProjectVersionRepositoryRefInput {
                            repository_binding_id: bindings[1].id,
                            ref_type: KnowledgeGitRefType::Tag,
                            ref_name: "v1.0.0".to_string(),
                            excluded: false,
                        },
                    ],
                },
            )
            .await?;
            fs::write(first_repo.join("README.md"), "# fixture\nnext\n")?;
            run_git(&first_repo, &["add", "README.md"])?;
            run_git(&first_repo, &["commit", "--quiet", "-m", "next"])?;
            let after = KnowledgeCatalogService::create_project_version_manifest(
                &db,
                KnowledgeProjectVersionManifestInput {
                    project_id: project.id,
                    version: "branch-after".to_string(),
                    repositories: vec![
                        ProjectVersionRepositoryRefInput {
                            repository_binding_id: bindings[0].id,
                            ref_type: KnowledgeGitRefType::Branch,
                            ref_name: branch,
                            excluded: false,
                        },
                        ProjectVersionRepositoryRefInput {
                            repository_binding_id: bindings[1].id,
                            ref_type: KnowledgeGitRefType::Tag,
                            ref_name: "v1.0.0".to_string(),
                            excluded: false,
                        },
                    ],
                },
            )
            .await?;
            assert_ne!(
                before.repositories[0].resolved_commit_sha,
                after.repositories[0].resolved_commit_sha,
                "再次创建分支版本必须保存新的 Commit，而不是改写旧清单"
            );
            let original = db.list_knowledge_release_repository_manifests(before.release_id)?;
            assert_eq!(
                original[0].resolved_commit_sha, before.repositories[0].resolved_commit_sha,
                "历史版本清单必须保持最初解析的 Commit"
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        .await;
        fs::remove_dir_all(&first_repo)?;
        fs::remove_dir_all(&second_repo)?;
        result
    }

    #[tokio::test]
    async fn project_version_manifest_retry_reuses_identical_frozen_manifest_without_reopening_git(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let repo = create_git_fixture()?;
        let result = async {
            let db = Database::init(":memory:")?;
            let project = KnowledgeCatalogService::upsert_project(&db, project_input())?;
            db.upsert_git_workspace(
                &UpsertGitWorkspaceInput {
                    id: None,
                    workspace_key: "orders-api".to_string(),
                    name: "订单服务".to_string(),
                    repo_path: repo.to_string_lossy().to_string(),
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
            db.upsert_git_workspace(
                &UpsertGitWorkspaceInput {
                    id: None,
                    workspace_key: "archived-api".to_string(),
                    name: "历史服务".to_string(),
                    // 该仓库在本次版本中明确排除，因此不会读取它；共用夹具路径只用于
                    // 验证排除标记也是不可变版本身份的一部分。
                    repo_path: repo.to_string_lossy().to_string(),
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
                &db,
                KnowledgeRepositoryBindingInput {
                    project_id: project.id,
                    repositories: vec![
                        RepositoryBindingInput {
                            workspace_key: "orders-api".to_string(),
                            alias: None,
                            role: None,
                            default_branch: None,
                            version_strategy: KnowledgeVersionStrategy::Branch,
                        },
                        RepositoryBindingInput {
                            workspace_key: "archived-api".to_string(),
                            alias: None,
                            role: None,
                            default_branch: None,
                            version_strategy: KnowledgeVersionStrategy::Tag,
                        },
                    ],
                },
            )?;
            let branch = run_git(&repo, &["branch", "--show-current"])?
                .trim()
                .to_string();
            let request = KnowledgeProjectVersionManifestInput {
                project_id: project.id,
                version: "retry-after-response-loss".to_string(),
                repositories: vec![
                    ProjectVersionRepositoryRefInput {
                        repository_binding_id: bindings[0].id,
                        ref_type: KnowledgeGitRefType::Branch,
                        ref_name: branch.clone(),
                        excluded: false,
                    },
                    ProjectVersionRepositoryRefInput {
                        repository_binding_id: bindings[1].id,
                        ref_type: KnowledgeGitRefType::Tag,
                        ref_name: String::new(),
                        excluded: true,
                    },
                ],
            };

            // 模拟首次请求已经提交事务、但客户端在收到响应前断开。随后分支继续前进，
            // 重试仍必须返回最初冻结的 Commit，而不是重新解析当前分支。
            let created =
                KnowledgeCatalogService::create_project_version_manifest(&db, request.clone())
                    .await?;
            fs::write(repo.join("README.md"), "# fixture\nretry\n")?;
            run_git(&repo, &["add", "README.md"])?;
            run_git(&repo, &["commit", "--quiet", "-m", "advance branch"])?;
            let retried =
                KnowledgeCatalogService::create_project_version_manifest(&db, request.clone())
                    .await?;
            assert_eq!(retried.release_id, created.release_id);
            assert_eq!(
                retried.repositories[0].resolved_commit_sha,
                created.repositories[0].resolved_commit_sha,
                "相同请求重试必须复用已冻结的清单，不能重新解析移动分支"
            );
            assert_eq!(
                KnowledgeCatalogService::list_releases(&db, project.id)?.len(),
                1,
                "响应丢失后的重试不得创建第二个项目版本"
            );

            let mismatch = KnowledgeProjectVersionManifestInput {
                project_id: project.id,
                version: request.version.clone(),
                repositories: vec![
                    ProjectVersionRepositoryRefInput {
                        repository_binding_id: bindings[0].id,
                        ref_type: KnowledgeGitRefType::Commit,
                        ref_name: created.repositories[0].resolved_commit_sha.clone(),
                        excluded: false,
                    },
                    ProjectVersionRepositoryRefInput {
                        repository_binding_id: bindings[1].id,
                        ref_type: KnowledgeGitRefType::Tag,
                        ref_name: "v-does-not-matter".to_string(),
                        excluded: false,
                    },
                ],
            };
            let error = KnowledgeCatalogService::create_project_version_manifest(&db, mismatch)
                .await
                .expect_err("同版本的不同引用不得覆盖既有清单");
            assert!(error.to_string().contains("仓库引用不一致"));
            let persisted = db.list_knowledge_release_repository_manifests(created.release_id)?;
            assert_eq!(persisted.len(), 2);
            assert_eq!(
                persisted[0].requested_ref_type,
                KnowledgeGitRefType::Branch,
                "不一致重试不得篡改原始引用类型"
            );
            assert_eq!(
                persisted[0].resolved_commit_sha, created.repositories[0].resolved_commit_sha,
                "不一致重试不得篡改历史 Commit"
            );
            assert_eq!(persisted[1].inclusion_status, "excluded");
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        .await;
        fs::remove_dir_all(&repo)?;
        result
    }

    #[tokio::test]
    async fn project_version_manifest_retry_rejects_legacy_partial_manifest_after_binding_is_added(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let repo = create_git_fixture()?;
        let result = async {
            let db = Database::init(":memory:")?;
            let project = KnowledgeCatalogService::upsert_project(&db, project_input())?;
            db.upsert_git_workspace(
                &UpsertGitWorkspaceInput {
                    id: None,
                    workspace_key: "orders-api".to_string(),
                    name: "订单服务".to_string(),
                    repo_path: repo.to_string_lossy().to_string(),
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
            db.upsert_git_workspace(
                &UpsertGitWorkspaceInput {
                    id: None,
                    workspace_key: "billing-api".to_string(),
                    name: "计费服务".to_string(),
                    // 新关联在本次重试中故意漏传；若错误地进入首次创建路径，会暴露出
                    // 不可用目录，而正确实现必须在读取 Git 前拒绝该请求。
                    repo_path: repo
                        .join("not-a-git-repository")
                        .to_string_lossy()
                        .to_string(),
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
            let first_binding = KnowledgeCatalogService::replace_repository_bindings(
                &db,
                KnowledgeRepositoryBindingInput {
                    project_id: project.id,
                    repositories: vec![RepositoryBindingInput {
                        workspace_key: "orders-api".to_string(),
                        alias: None,
                        role: None,
                        default_branch: None,
                        version_strategy: KnowledgeVersionStrategy::Branch,
                    }],
                },
            )?
            .remove(0);
            let branch = run_git(&repo, &["branch", "--show-current"])?
                .trim()
                .to_string();
            let legacy_request = KnowledgeProjectVersionManifestInput {
                project_id: project.id,
                version: "legacy-partial-retry".to_string(),
                repositories: vec![ProjectVersionRepositoryRefInput {
                    repository_binding_id: first_binding.id,
                    ref_type: KnowledgeGitRefType::Branch,
                    ref_name: branch,
                    excluded: false,
                }],
            };
            let created = KnowledgeCatalogService::create_project_version_manifest(
                &db,
                legacy_request.clone(),
            )
            .await?;
            assert_eq!(created.repositories.len(), 1);

            let active_bindings = KnowledgeCatalogService::replace_repository_bindings(
                &db,
                KnowledgeRepositoryBindingInput {
                    project_id: project.id,
                    repositories: vec![
                        RepositoryBindingInput {
                            workspace_key: "orders-api".to_string(),
                            alias: None,
                            role: None,
                            default_branch: None,
                            version_strategy: KnowledgeVersionStrategy::Branch,
                        },
                        RepositoryBindingInput {
                            workspace_key: "billing-api".to_string(),
                            alias: None,
                            role: None,
                            default_branch: None,
                            version_strategy: KnowledgeVersionStrategy::Tag,
                        },
                    ],
                },
            )?;
            assert_eq!(active_bindings.len(), 2);
            assert!(active_bindings
                .iter()
                .any(|binding| binding.id == first_binding.id));

            let error =
                KnowledgeCatalogService::create_project_version_manifest(&db, legacy_request)
                    .await
                    .expect_err("遗漏当前新增仓库的旧清单不得被重试复用");
            assert!(error.to_string().contains("每个当前关联仓库"));
            assert_eq!(
                KnowledgeCatalogService::list_releases(&db, project.id)?.len(),
                1,
                "被拒绝的重试不得创建或覆盖版本"
            );
            let persisted = db.list_knowledge_release_repository_manifests(created.release_id)?;
            assert_eq!(persisted.len(), 1, "被拒绝的重试不得补写历史部分清单");
            assert_eq!(
                persisted[0].resolved_commit_sha, created.repositories[0].resolved_commit_sha,
                "被拒绝的重试不得改写已冻结 Commit"
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        .await;
        fs::remove_dir_all(&repo)?;
        result
    }

    #[tokio::test]
    async fn project_version_manifest_requires_complete_mapping_or_explicit_exclusion(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let repo = create_git_fixture()?;
        let result = async {
            let db = Database::init(":memory:")?;
            let project = KnowledgeCatalogService::upsert_project(&db, project_input())?;
            db.upsert_git_workspace(
                &UpsertGitWorkspaceInput {
                    id: None,
                    workspace_key: "orders-api".to_string(),
                    name: "订单服务".to_string(),
                    repo_path: repo.to_string_lossy().to_string(),
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
            let binding = KnowledgeCatalogService::replace_repository_bindings(
                &db,
                KnowledgeRepositoryBindingInput {
                    project_id: project.id,
                    repositories: vec![RepositoryBindingInput {
                        workspace_key: "orders-api".to_string(),
                        alias: None,
                        role: None,
                        default_branch: None,
                        version_strategy: KnowledgeVersionStrategy::Tag,
                    }],
                },
            )?
            .remove(0);
            let missing_tag = KnowledgeCatalogService::create_project_version_manifest(
                &db,
                KnowledgeProjectVersionManifestInput {
                    project_id: project.id,
                    version: "missing-tag".to_string(),
                    repositories: vec![ProjectVersionRepositoryRefInput {
                        repository_binding_id: binding.id,
                        ref_type: KnowledgeGitRefType::Tag,
                        ref_name: "v-does-not-exist".to_string(),
                        excluded: false,
                    }],
                },
            )
            .await
            .expect_err("缺失 Tag 不得自动退回 HEAD");
            assert!(missing_tag.to_string().contains("找不到所选引用"));
            let excluded = KnowledgeCatalogService::create_project_version_manifest(
                &db,
                KnowledgeProjectVersionManifestInput {
                    project_id: project.id,
                    version: "all-excluded".to_string(),
                    repositories: vec![ProjectVersionRepositoryRefInput {
                        repository_binding_id: binding.id,
                        ref_type: KnowledgeGitRefType::Tag,
                        ref_name: String::new(),
                        excluded: true,
                    }],
                },
            )
            .await
            .expect_err("全部仓库排除时不应创建就绪版本");
            assert!(excluded.to_string().contains("至少需要包含一个"));
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        .await;
        fs::remove_dir_all(&repo)?;
        result
    }
}
