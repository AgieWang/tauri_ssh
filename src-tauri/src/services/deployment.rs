use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rand::{distributions::Alphanumeric, Rng};
use serde_json::{json, Value};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AiProviderAskInput, CreateApprovalRequestInput, CreateAuditLogInput,
    CreateDeploymentDryRunInput, CreateDeploymentRollbackDryRunInput,
    CreateSecureCredentialSessionInput, DatabaseQueryInput, DeploymentAiAdviceInput,
    DeploymentAiAdviceResult, DeploymentCandidate, DeploymentDetectionResult,
    DeploymentEnvironmentProbe, DeploymentEnvironmentProfile, DeploymentGroup,
    DeploymentImageStoreApp, DeploymentImageStoreEnv, DeploymentPlan, DeploymentPlanStage,
    DeploymentProbeCheck, DeploymentRun, DeploymentRunDetail, DeploymentTarget, DeploymentTemplate,
    DetectDeploymentProjectInput, ExecuteDeploymentRollbackInput, ExecuteDeploymentRunInput,
    InstallImageStoreAppInput, ListDeploymentRunsInput, SftpTransferPathInput,
    TerminalCommandInput, UpsertDatabaseConnectionInput, UpsertDeploymentGroupInput,
    UpsertDeploymentTargetInput,
};
use crate::services::ai_provider::AiProviderService;
use crate::services::approval::ApprovalService;
use crate::services::audit::AuditService;
use crate::services::database_ops::DatabaseOpsService;
use crate::services::secure_credential::SecureCredentialService;
use crate::services::sftp::SftpService;
use crate::services::terminal::TerminalService;

pub struct DeploymentService;

struct ExecutionOutcome {
    has_approval: bool,
    has_failed: bool,
}

impl DeploymentService {
    pub fn list_templates() -> Vec<DeploymentTemplate> {
        vec![
            DeploymentTemplate {
                key: "1panel-app".into(),
                name: "1panel-app".into(),
                description:
                    "1Panel 托管应用部署，按 1Panel 目录约定上传产物并重启对应 compose 服务。"
                        .into(),
                scenario: "1Panel 托管应用".into(),
                risk: "high".into(),
                supported_sources: vec!["local".into(), "git".into()],
                required_profiles: vec!["1panel".into(), "docker".into()],
            },
            DeploymentTemplate {
                key: "dockerfile-service".into(),
                name: "Dockerfile 服务".into(),
                description: "识别 Dockerfile，支持远程构建或本地构建镜像后上传。".into(),
                scenario: "单服务容器部署".into(),
                risk: "review".into(),
                supported_sources: vec!["local".into(), "git".into()],
                required_profiles: vec!["dockerfile-service".into()],
            },
            DeploymentTemplate {
                key: "docker-compose".into(),
                name: "Docker Compose 栈".into(),
                description: "识别 compose 文件，将栈托管到部署根目录。".into(),
                scenario: "多容器编排".into(),
                risk: "review".into(),
                supported_sources: vec!["local".into(), "git".into()],
                required_profiles: vec!["docker-compose".into()],
            },
            DeploymentTemplate {
                key: "static-openresty".into(),
                name: "前端静态站".into(),
                description: "构建前端产物，上传静态资源，并预留 HTTPS 和 API 反代配置。".into(),
                scenario: "React/Vue/Vite/Uniapp 静态站".into(),
                risk: "review".into(),
                supported_sources: vec!["local".into(), "git".into()],
                required_profiles: vec!["static-openresty".into()],
            },
            DeploymentTemplate {
                key: "static-nginx".into(),
                name: "Nginx 静态站".into(),
                description: "前端静态站部署到 nginx，使用 releases 软链原子切换并 reload nginx。"
                    .into(),
                scenario: "React/Vue/Vite/Uniapp 静态站".into(),
                risk: "review".into(),
                supported_sources: vec!["local".into(), "git".into()],
                required_profiles: vec!["static-nginx".into()],
            },
            DeploymentTemplate {
                key: "node-pm2".into(),
                name: "Node PM2 服务".into(),
                description: "Node 后端服务上传 release 后由 PM2 托管。".into(),
                scenario: "Node API 服务".into(),
                risk: "high".into(),
                supported_sources: vec!["local".into(), "git".into()],
                required_profiles: vec!["node-pm2".into()],
            },
            DeploymentTemplate {
                key: "systemd-binary".into(),
                name: "Systemd 二进制服务".into(),
                description: "Java/Go/二进制产物上传后由 systemd 托管。".into(),
                scenario: "JAR、Go、二进制服务".into(),
                risk: "high".into(),
                supported_sources: vec!["local".into(), "git".into()],
                required_profiles: vec!["systemd-binary".into()],
            },
            DeploymentTemplate {
                key: "custom-script".into(),
                name: "自定义脚本".into(),
                description: "兜底部署方案，所有命令强制 dry-run、危险命令扫描和审批。".into(),
                scenario: "非标准项目".into(),
                risk: "high".into(),
                supported_sources: vec!["local".into(), "git".into()],
                required_profiles: vec![],
            },
            DeploymentTemplate {
                key: "image-store".into(),
                name: "镜像商店应用".into(),
                description: "从内置镜像商店选择常用应用，自动生成 docker-compose.yml 并部署。"
                    .into(),
                scenario: "中间件、可视化面板、运维工具一键安装".into(),
                risk: "high".into(),
                supported_sources: vec!["image-store".into()],
                required_profiles: vec!["docker-compose".into()],
            },
        ]
    }

    pub fn list_image_store_apps() -> Vec<DeploymentImageStoreApp> {
        image_store_catalog()
    }

    pub fn install_image_store_app(
        db: &Database,
        input: InstallImageStoreAppInput,
    ) -> Result<DeploymentTarget, AppError> {
        let app = image_store_catalog()
            .into_iter()
            .find(|item| item.key == input.app_key)
            .ok_or_else(|| {
                AppError::NotFound(format!("镜像商店应用 '{}' 不存在", input.app_key))
            })?;
        let target_key = normalize_key(&input.target_key)?;
        validate_required(&input.name, "部署目标名称")?;
        if input.server_alias.trim().is_empty() {
            return Err(AppError::InvalidInput("目标服务器不能为空".into()));
        }
        let port = input.port.or(app.default_port);
        let tag = input
            .image_tag
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&app.tag)
            .to_string();
        let deploy_root = input
            .deploy_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("/opt/tauri-ssh/stacks/{}", target_key));
        let env_overrides = input
            .env_json
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| serde_json::from_str::<Value>(value))
            .transpose()
            .map_err(|error| AppError::InvalidInput(format!("环境变量 JSON 无效: {}", error)))?
            .unwrap_or_else(|| json!({}));
        let env = image_store_env_values(&app, &env_overrides);
        let compose = image_store_compose_yaml(&app, &target_key, &tag, port, &deploy_root, &env);
        let config_json = json!({
            "deploymentProfile": "image-store",
            "imageStore": {
                "appKey": app.key,
                "appName": app.name,
                "image": app.image,
                "tag": tag,
                "containerPort": app.container_port,
                "hostPort": port,
                "volumePath": app.volume_path,
                "env": env,
                "compose": compose
            }
        })
        .to_string();

        Self::upsert_target(
            db,
            UpsertDeploymentTargetInput {
                id: None,
                target_key,
                name: input.name,
                server_alias: input.server_alias,
                recipe: "image-store".into(),
                source_type: "image-store".into(),
                project_path: Some(String::new()),
                git_url: Some(String::new()),
                git_ref: Some(String::new()),
                git_credential_key: Some(String::new()),
                docker_build_mode: Some("remote".into()),
                workdir: Some(".".into()),
                deploy_root: Some(deploy_root),
                domain: Some(String::new()),
                https_enabled: Some(false),
                port,
                health_check_url: Some(String::new()),
                config_json: Some(config_json),
                enabled: input.enabled,
            },
        )
    }

    pub fn list_environment_profiles() -> Vec<DeploymentEnvironmentProfile> {
        vec![
            DeploymentEnvironmentProfile {
                key: "1panel-app".into(),
                name: "1panel-app".into(),
                description: "1panel 托管应用部署（按 1panel 目录约定上传产物 + 重启对应 compose/服务）。".into(),
                category: "基础模板".into(),
                checks: vec!["1panel".into(), "docker".into()],
            },
            DeploymentEnvironmentProfile {
                key: "custom-script".into(),
                name: "custom-script".into(),
                description: "兜底万能配方，各阶段命令全部自定义（artifact 模式仍走 releases 软链原子切换）。".into(),
                category: "基础模板".into(),
                checks: vec!["custom".into()],
            },
            DeploymentEnvironmentProfile {
                key: "docker-compose".into(),
                name: "docker-compose".into(),
                description: "Docker Compose 部署（拉镜像 / 重建容器；后端反代 + HTTPS 由部署引擎统一接管，配了域名即自动配反代）。".into(),
                category: "基础模板".into(),
                checks: vec!["docker".into()],
            },
            DeploymentEnvironmentProfile {
                key: "node-pm2".into(),
                name: "node-pm2".into(),
                description: "Node 后端用 pm2 部署（releases + 软链原子切换，pm2 reload 平滑重启）。".into(),
                category: "基础模板".into(),
                checks: vec!["node".into(), "backend".into()],
            },
            DeploymentEnvironmentProfile {
                key: "static-nginx".into(),
                name: "static-nginx".into(),
                description: "前端静态站部署到 nginx（releases + 软链原子切换，reload nginx）。".into(),
                category: "基础模板".into(),
                checks: vec!["static".into(), "frontend".into()],
            },
            DeploymentEnvironmentProfile {
                key: "static-openresty".into(),
                name: "static-openresty".into(),
                description: "前端静态站部署到 OpenResty（releases + current 软链原子切换；建站 + HTTPS 由部署引擎复用「网站」系统统一接管，不手写 conf）。".into(),
                category: "基础模板".into(),
                checks: vec!["static".into(), "frontend".into()],
            },
            DeploymentEnvironmentProfile {
                key: "systemd-binary".into(),
                name: "systemd-binary".into(),
                description: "Java jar / Go / 二进制用 systemd 部署（releases + 软链原子切换，systemctl restart）。".into(),
                category: "基础模板".into(),
                checks: vec!["java".into(), "go".into(), "binary".into(), "backend".into()],
            },
            DeploymentEnvironmentProfile {
                key: "static-openresty-https".into(),
                name: "前端静态站 + HTTPS".into(),
                description: "适合 Vite/React/Vue/Uniapp 等纯前端项目，默认使用 OpenResty 静态站、80 端口健康检查，并预留域名 HTTPS 配置。".into(),
                category: "组合方案".into(),
                checks: vec!["static".into(), "openresty".into(), "https".into()],
            },
            DeploymentEnvironmentProfile {
                key: "springboot-mysql-redis".into(),
                name: "Spring Boot + MySQL + Redis".into(),
                description: "适合 Java 后端服务，默认使用 systemd 托管，并在扩展配置中预置 MySQL/Redis 专属账号创建结构。".into(),
                category: "组合方案".into(),
                checks: vec!["java".into(), "systemd".into(), "mysql".into(), "redis".into()],
            },
            DeploymentEnvironmentProfile {
                key: "compose-db-redis".into(),
                name: "Docker Compose + 数据库/Redis".into(),
                description: "适合多容器应用复用宿主共享 MySQL/Redis，默认使用 Compose 配方，并预置数据库和 Redis 专属账号配置。".into(),
                category: "组合方案".into(),
                checks: vec!["docker".into(), "compose".into(), "mysql".into(), "redis".into()],
            },
            DeploymentEnvironmentProfile {
                key: "frontend-api-same-domain".into(),
                name: "前后端同域部署".into(),
                description: "适合 SPA 前端和后端 API 同域发布，默认使用 OpenResty 静态站并预置 API 反代前缀和后端端口配置。".into(),
                category: "组合方案".into(),
                checks: vec!["static".into(), "openresty".into(), "api-proxy".into(), "https".into()],
            },
            DeploymentEnvironmentProfile {
                key: "1panel-app-db".into(),
                name: "1Panel 应用 + 共享数据库".into(),
                description: "适合 1Panel 托管应用复用应用内数据库/Redis 资源，默认使用 1Panel 配方并预置专属账号结构。".into(),
                category: "组合方案".into(),
                checks: vec!["1panel".into(), "docker".into(), "mysql".into(), "redis".into()],
            },
        ]
    }

    pub fn list_targets(db: &Database) -> Result<Vec<DeploymentTarget>, AppError> {
        db.list_deployment_targets()
    }

    pub fn upsert_target(
        db: &Database,
        mut input: UpsertDeploymentTargetInput,
    ) -> Result<DeploymentTarget, AppError> {
        input.target_key = normalize_key(&input.target_key)?;
        validate_required(&input.name, "部署目标名称")?;
        validate_recipe(&input.recipe)?;
        validate_source_type(&input.source_type)?;
        if input.server_alias.trim().is_empty() {
            return Err(AppError::InvalidInput("目标服务器不能为空".into()));
        }
        if input.deploy_root.as_deref().unwrap_or("").trim().is_empty() {
            input.deploy_root = Some(format!("/opt/tauri-ssh/stacks/{}", input.target_key));
        }
        if input.docker_build_mode.is_none() {
            input.docker_build_mode = Some("remote".into());
        }
        db.upsert_deployment_target(&input)
    }

    pub fn delete_target(db: &Database, target_key: &str) -> Result<(), AppError> {
        let target_key = normalize_key(target_key)?;
        if !db.delete_deployment_target(&target_key)? {
            return Err(AppError::NotFound(format!(
                "部署目标 '{}' 不存在",
                target_key
            )));
        }
        Ok(())
    }

    pub fn list_groups(db: &Database) -> Result<Vec<DeploymentGroup>, AppError> {
        db.list_deployment_groups()
    }

    pub fn upsert_group(
        db: &Database,
        mut input: UpsertDeploymentGroupInput,
    ) -> Result<DeploymentGroup, AppError> {
        input.group_key = normalize_key(&input.group_key)?;
        validate_required(&input.name, "部署组名称")?;
        for target in &mut input.targets {
            target.target_key = normalize_key(&target.target_key)?;
        }
        db.upsert_deployment_group(&input)
    }

    pub fn delete_group(db: &Database, group_key: &str) -> Result<(), AppError> {
        let group_key = normalize_key(group_key)?;
        if !db.delete_deployment_group(&group_key)? {
            return Err(AppError::NotFound(format!("部署组 '{}' 不存在", group_key)));
        }
        Ok(())
    }

    pub fn detect_project(
        db: &Database,
        input: DetectDeploymentProjectInput,
    ) -> Result<DeploymentDetectionResult, AppError> {
        validate_source_type(&input.source_type)?;
        let (project_root, commit, warnings) = if input.source_type == "git" {
            checkout_git_source(db, &input)?
        } else {
            let path = input
                .project_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::InvalidInput("本地项目目录不能为空".into()))?;
            (PathBuf::from(path), String::new(), Vec::new())
        };

        if !project_root.is_dir() {
            return Err(AppError::InvalidInput(format!(
                "项目目录不存在或不可读取: {}",
                project_root.display()
            )));
        }

        let mut candidates = Vec::new();
        let mut seen = HashSet::new();
        scan_candidates(
            &project_root,
            &project_root,
            0,
            &input.source_type,
            &mut seen,
            &mut candidates,
        )?;

        Ok(DeploymentDetectionResult {
            source_type: input.source_type,
            project_root: project_root.to_string_lossy().to_string(),
            git_url: input.git_url.unwrap_or_default(),
            git_ref: input.git_ref.unwrap_or_default(),
            commit,
            candidates,
            warnings,
        })
    }

    pub async fn ai_advice(
        db: &Database,
        input: DeploymentAiAdviceInput,
    ) -> Result<DeploymentAiAdviceResult, AppError> {
        let plan = match input.plan {
            Some(plan) => plan,
            None => {
                Self::create_dry_run(
                    db,
                    CreateDeploymentDryRunInput {
                        target_key: input.target_key,
                        group_key: input.group_key,
                    },
                )
                .await?
            }
        };
        let plan_json = serde_json::to_string_pretty(&plan)?;
        let user_prompt = input.prompt.unwrap_or_else(|| {
            "请基于以下自动部署 dry-run 计划，给出部署建议、风险解释、审批关注点和执行前检查清单。".into()
        });
        let prompt = format!(
            "{user_prompt}\n\n要求：\n1. 只能基于 dry-run 计划和环境探测结果分析，不要假装已经执行部署。\n2. 明确指出 high/review/readonly 阶段的风险。\n3. 如果发现 Docker、Compose、域名、端口、磁盘、数据库或 Redis 风险，给出处理建议。\n4. 输出 Markdown。\n\nDry-run 计划 JSON：\n```json\n{plan_json}\n```"
        );
        let result = AiProviderService::ask(
            db,
            AiProviderAskInput {
                prompt,
                provider_key: input.provider_key,
                system_prompt: Some("你是 Tauri SSH 的自动部署风险顾问，专注 Linux 服务部署、Docker、OpenResty/Nginx、数据库/Redis 账号、审批和回滚。".into()),
                skill_scope: Some("global".into()),
                use_skill_trigger: Some(true),
            },
        )
        .await?;
        Ok(DeploymentAiAdviceResult {
            provider_key: result.provider_key,
            provider_name: result.provider_name,
            model: result.model,
            answer: result.answer,
            latency_ms: result.latency_ms,
            generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }

    pub async fn create_dry_run(
        db: &Database,
        input: CreateDeploymentDryRunInput,
    ) -> Result<DeploymentPlan, AppError> {
        let target = if let Some(target_key) = input
            .target_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            db.get_deployment_target(target_key)?
                .ok_or_else(|| AppError::NotFound(format!("部署目标 '{}' 不存在", target_key)))?
        } else if let Some(group_key) = input
            .group_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let group = db
                .get_deployment_group(group_key)?
                .ok_or_else(|| AppError::NotFound(format!("部署组 '{}' 不存在", group_key)))?;
            let first_target = group
                .targets
                .iter()
                .find(|item| item.enabled)
                .ok_or_else(|| AppError::InvalidInput("部署组中没有启用的部署目标".into()))?;
            db.get_deployment_target(&first_target.target_key)?
                .ok_or_else(|| {
                    AppError::NotFound(format!("部署目标 '{}' 不存在", first_target.target_key))
                })?
        } else {
            return Err(AppError::InvalidInput("请选择部署目标或部署组".into()));
        };

        let environment = Self::probe_environment(db, &target).await?;
        let stages = build_plan_stages(&target, &environment);
        let approval_required = stages.iter().any(|stage| stage.approval_required);
        let risk = if stages.iter().any(|stage| stage.risk == "high") {
            "high"
        } else {
            "review"
        }
        .to_string();
        let warnings = build_plan_warnings(&target, &environment);
        let plan = DeploymentPlan {
            plan_id: format!("dryrun-{}", chrono::Local::now().format("%Y%m%d%H%M%S")),
            target_key: target.target_key.clone(),
            group_key: input.group_key.unwrap_or_default(),
            title: format!("{} dry-run", target.name),
            recipe: target.recipe.clone(),
            server_alias: target.server_alias.clone(),
            status: "ready".into(),
            risk,
            approval_required,
            environment,
            stages,
            warnings,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };

        let _ = AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "deployment".into(),
                server_alias: target.server_alias.clone(),
                action: "deployment.dry_run".into(),
                risk: plan.risk.clone(),
                result: "成功".into(),
                summary: format!("生成自动部署 dry-run 计划：{}", target.name),
                detail_json: Some(
                    json!({
                        "planId": plan.plan_id,
                        "targetKey": plan.target_key,
                        "recipe": plan.recipe,
                        "approvalRequired": plan.approval_required,
                        "stageCount": plan.stages.len()
                    })
                    .to_string(),
                ),
                request_id: Some(plan.plan_id.clone()),
                approval_id: None,
            },
        );

        Ok(plan)
    }

    pub fn list_runs(
        db: &Database,
        mut input: ListDeploymentRunsInput,
    ) -> Result<Vec<DeploymentRun>, AppError> {
        input.status = input
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| Some("all".into()));
        db.list_deployment_runs(&input)
    }

    pub fn get_run_detail(db: &Database, run_id: &str) -> Result<DeploymentRunDetail, AppError> {
        db.get_deployment_run_detail(run_id)?
            .ok_or_else(|| AppError::NotFound(format!("部署运行 '{}' 不存在", run_id)))
    }

    pub async fn create_rollback_dry_run(
        db: &Database,
        input: CreateDeploymentRollbackDryRunInput,
    ) -> Result<DeploymentPlan, AppError> {
        let target_key = input.target_key.trim();
        if target_key.is_empty() {
            return Err(AppError::InvalidInput("请选择回滚目标".into()));
        }
        let target = db
            .get_deployment_target(target_key)?
            .ok_or_else(|| AppError::NotFound(format!("部署目标 '{}' 不存在", target_key)))?;
        let environment = Self::probe_environment(db, &target).await?;
        let mut stages = vec![
            stage(
                "rollback_probe",
                "回滚环境探测",
                "readonly",
                false,
                "",
                "确认目标服务器、部署根目录和 current/releases 结构。",
            ),
            stage(
                "rollback",
                "切换到上一 release",
                "high",
                true,
                "ln -sfn <previous-release> current && reload/restart",
                "将 current 软链接切换到上一版本；容器/静态站会执行对应 reload 或 compose up。",
            ),
            stage(
                "health_check",
                "回滚后健康检查",
                "readonly",
                false,
                health_check_preview(&target),
                "检查回滚后的服务可用性。",
            ),
        ];
        if environment
            .checks
            .iter()
            .any(|item| item.status == "warning")
        {
            stages.insert(
                1,
                stage(
                    "environment_warnings",
                    "环境风险提示",
                    "review",
                    false,
                    "",
                    "环境探测发现风险，回滚前需要处理或确认。",
                ),
            );
        }
        let warnings = build_plan_warnings(&target, &environment);
        Ok(DeploymentPlan {
            plan_id: format!("rollback-{}", chrono::Local::now().format("%Y%m%d%H%M%S")),
            target_key: target.target_key.clone(),
            group_key: String::new(),
            title: format!("{} rollback dry-run", target.name),
            recipe: target.recipe.clone(),
            server_alias: target.server_alias.clone(),
            status: "ready".into(),
            risk: "high".into(),
            approval_required: true,
            environment,
            stages,
            warnings,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }

    pub async fn execute_rollback(
        db: &Database,
        input: ExecuteDeploymentRollbackInput,
    ) -> Result<DeploymentRunDetail, AppError> {
        let created_by = input.created_by.unwrap_or_else(|| "local-user".into());
        let plan = Self::create_rollback_dry_run(
            db,
            CreateDeploymentRollbackDryRunInput {
                target_key: input.target_key.clone(),
                run_id: input.run_id.clone(),
            },
        )
        .await?;
        let target = db
            .get_deployment_target(&plan.target_key)?
            .ok_or_else(|| AppError::NotFound(format!("部署目标 '{}' 不存在", plan.target_key)))?;
        let run_id = unique_run_id("rollback");
        let plan_json = serde_json::to_string(&plan)?;
        db.create_deployment_run(
            &run_id,
            &target.target_key,
            "",
            "running",
            "回滚运行已开始",
            &plan_json,
            &created_by,
        )?;
        let release_id = rollback_release_marker();
        let outcome = Self::execute_plan_into_run(
            db,
            &run_id,
            &plan,
            &target,
            &created_by,
            None,
            &release_id,
        )
        .await?;
        let (status, summary, finished) = summarize_outcome(&outcome, "回滚执行完成。");
        let run = db.update_deployment_run_status(&run_id, status, &summary, finished)?;
        Self::audit_run(db, &run, status, &summary, None)?;
        Self::get_run_detail(db, &run.run_id)
    }

    pub async fn execute_run(
        db: &Database,
        input: ExecuteDeploymentRunInput,
    ) -> Result<DeploymentRunDetail, AppError> {
        if let Some(run_id) = input
            .continue_run_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Self::continue_run(db, run_id).await;
        }

        let created_by = input.created_by.unwrap_or_else(|| "local-user".into());
        if let Some(group_key) = input
            .group_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .filter(|_| input.target_key.as_deref().unwrap_or("").trim().is_empty())
        {
            return Self::execute_group_run(db, group_key, &created_by).await;
        }

        let target_key = input
            .target_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("请选择部署目标或部署组".into()))?;
        Self::execute_target_run(db, target_key, "", &created_by).await
    }

    async fn execute_group_run(
        db: &Database,
        group_key: &str,
        created_by: &str,
    ) -> Result<DeploymentRunDetail, AppError> {
        let group = db
            .get_deployment_group(group_key)?
            .ok_or_else(|| AppError::NotFound(format!("部署组 '{}' 不存在", group_key)))?;
        let enabled_targets: Vec<_> = group.targets.iter().filter(|item| item.enabled).collect();
        if enabled_targets.is_empty() {
            return Err(AppError::InvalidInput("部署组中没有启用的部署目标".into()));
        }
        let run_id = unique_run_id("deploy-group");
        let plan_json = json!({
            "type": "group",
            "groupKey": group.group_key,
            "targets": enabled_targets.iter().map(|item| item.target_key.clone()).collect::<Vec<_>>()
        })
        .to_string();
        db.create_deployment_run(
            &run_id,
            "",
            &group.group_key,
            "running",
            "部署组运行已开始",
            &plan_json,
            created_by,
        )?;
        let mut final_outcome = ExecutionOutcome {
            has_approval: false,
            has_failed: false,
        };
        for item in enabled_targets {
            let plan = Self::create_dry_run(
                db,
                CreateDeploymentDryRunInput {
                    target_key: Some(item.target_key.clone()),
                    group_key: None,
                },
            )
            .await?;
            let target = db.get_deployment_target(&item.target_key)?.ok_or_else(|| {
                AppError::NotFound(format!("部署目标 '{}' 不存在", item.target_key))
            })?;
            let release_id = release_id_for(&run_id, &target.target_key);
            let outcome = Self::execute_plan_into_run(
                db,
                &run_id,
                &plan,
                &target,
                created_by,
                Some(&target.target_key),
                &release_id,
            )
            .await?;
            final_outcome.has_approval |= outcome.has_approval;
            final_outcome.has_failed |= outcome.has_failed;
            if outcome.has_failed || outcome.has_approval {
                break;
            }
        }
        let (status, summary, finished) = summarize_outcome(&final_outcome, "部署组执行完成。");
        let run = db.update_deployment_run_status(&run_id, status, &summary, finished)?;
        Self::audit_run(db, &run, status, &summary, None)?;
        Self::get_run_detail(db, &run.run_id)
    }

    async fn execute_target_run(
        db: &Database,
        target_key: &str,
        group_key: &str,
        created_by: &str,
    ) -> Result<DeploymentRunDetail, AppError> {
        let plan = Self::create_dry_run(
            db,
            CreateDeploymentDryRunInput {
                target_key: Some(target_key.to_string()),
                group_key: None,
            },
        )
        .await?;
        let target = db
            .get_deployment_target(&plan.target_key)?
            .ok_or_else(|| AppError::NotFound(format!("部署目标 '{}' 不存在", plan.target_key)))?;
        if !target.enabled {
            return Err(AppError::InvalidInput("部署目标已禁用".into()));
        }
        let run_id = unique_run_id("deploy");
        let plan_json = serde_json::to_string(&plan)?;
        db.create_deployment_run(
            &run_id,
            &plan.target_key,
            group_key,
            "running",
            "部署运行已开始",
            &plan_json,
            created_by,
        )?;
        let release_id = release_id_for(&run_id, &target.target_key);
        let outcome =
            Self::execute_plan_into_run(db, &run_id, &plan, &target, created_by, None, &release_id)
                .await?;
        let (status, summary, finished) = summarize_outcome(&outcome, "部署执行完成。");
        let run = db.update_deployment_run_status(&run_id, status, &summary, finished)?;
        Self::audit_run(db, &run, status, &summary, None)?;
        Self::get_run_detail(db, &run.run_id)
    }

    async fn execute_plan_into_run(
        db: &Database,
        run_id: &str,
        plan: &DeploymentPlan,
        target: &DeploymentTarget,
        created_by: &str,
        step_prefix: Option<&str>,
        release_id: &str,
    ) -> Result<ExecutionOutcome, AppError> {
        let mut has_approval = false;
        let mut has_failed = false;
        for stage in &plan.stages {
            let prefixed_key = prefixed_step_key(step_prefix, &stage.key);
            let prefixed_title = prefixed_step_title(step_prefix, &stage.title);
            let command = execution_command_for_stage(target, stage, release_id);
            if stage.approval_required {
                let approval = ApprovalService::create(
                    db,
                    CreateApprovalRequestInput {
                        source: "deployment".into(),
                        requester: created_by.to_string(),
                        server_alias: target.server_alias.clone(),
                        action: "deployment_run_step".into(),
                        risk: stage.risk.clone(),
                        command: command
                            .clone()
                            .unwrap_or_else(|| stage.command_preview.clone()),
                        resource: target.deploy_root.clone(),
                        reason: format!("自动部署 '{}' 阶段 '{}'", target.name, stage.title),
                        summary: format!("等待审批后执行部署阶段：{}", prefixed_title),
                        payload_json: Some(
                            json!({
                                "runId": run_id,
                                "targetKey": target.target_key,
                                "stageKey": stage.key,
                                "stageTitle": stage.title,
                                "command": command
                            })
                            .to_string(),
                        ),
                        expires_at: None,
                    },
                )?;
                db.create_deployment_run_step(
                    run_id,
                    &prefixed_key,
                    &prefixed_title,
                    "approval_required",
                    command.as_deref().unwrap_or(&stage.command_preview),
                    Some(approval.id),
                )?;
                has_approval = true;
                continue;
            }

            let step = db.create_deployment_run_step(
                &run_id,
                &prefixed_key,
                &prefixed_title,
                "running",
                command.as_deref().unwrap_or(&stage.command_preview),
                None,
            )?;
            if stage.key == "source" && should_upload_source_locally(target) {
                match upload_source_release(db, target, release_id).await {
                    Ok(summary) => {
                        db.update_deployment_run_step_result(
                            step.id,
                            "success",
                            &summary,
                            "",
                            Some(0),
                            None,
                        )?;
                    }
                    Err(error) => {
                        has_failed = true;
                        db.update_deployment_run_step_result(
                            step.id,
                            "failed",
                            "",
                            &trim_preview(&error.to_string()),
                            Some(1),
                            None,
                        )?;
                        break;
                    }
                }
                continue;
            }
            if stage.key == "build"
                && target.recipe == "dockerfile-service"
                && target.docker_build_mode == "local_upload"
            {
                match build_upload_local_docker_image(db, target, release_id).await {
                    Ok(summary) => {
                        db.update_deployment_run_step_result(
                            step.id,
                            "success",
                            &summary,
                            "",
                            Some(0),
                            None,
                        )?;
                    }
                    Err(error) => {
                        has_failed = true;
                        db.update_deployment_run_step_result(
                            step.id,
                            "failed",
                            "",
                            &trim_preview(&error.to_string()),
                            Some(1),
                            None,
                        )?;
                        break;
                    }
                }
                continue;
            }
            if stage.key == "service_accounts" {
                match execute_service_accounts(db, target).await {
                    Ok(summary) => {
                        db.update_deployment_run_step_result(
                            step.id,
                            "success",
                            &summary,
                            "",
                            Some(0),
                            None,
                        )?;
                    }
                    Err(error) => {
                        has_failed = true;
                        db.update_deployment_run_step_result(
                            step.id,
                            "failed",
                            "",
                            &trim_preview(&error.to_string()),
                            Some(1),
                            None,
                        )?;
                        break;
                    }
                }
                continue;
            }
            match command {
                Some(command) if !command.trim().is_empty() => {
                    let result = TerminalService::execute(
                        db,
                        TerminalCommandInput {
                            server_alias: target.server_alias.clone(),
                            command,
                            timeout_secs: Some(stage_timeout_secs(stage)),
                            initiated_by_ai: Some(false),
                        },
                    )
                    .await?;
                    let status = if result.blocked {
                        "blocked"
                    } else if result.exit_status == 0 {
                        "success"
                    } else {
                        "failed"
                    };
                    if status != "success" {
                        has_failed = true;
                    }
                    db.update_deployment_run_step_result(
                        step.id,
                        status,
                        &trim_preview(&result.stdout),
                        &trim_preview(&result.stderr),
                        Some(result.exit_status as i64),
                        None,
                    )?;
                    if status == "failed" || status == "blocked" {
                        break;
                    }
                }
                _ => {
                    db.update_deployment_run_step_result(
                        step.id,
                        "success",
                        "无需远程命令，阶段已记录。",
                        "",
                        Some(0),
                        None,
                    )?;
                }
            }
        }
        Ok(ExecutionOutcome {
            has_approval,
            has_failed,
        })
    }

    async fn continue_run(db: &Database, run_id: &str) -> Result<DeploymentRunDetail, AppError> {
        let detail = Self::get_run_detail(db, run_id)?;
        let mut has_waiting_approval = false;
        let mut has_failed = false;
        for step in detail
            .steps
            .iter()
            .filter(|item| item.status == "approval_required")
        {
            if let Some(approval_id) = step.approval_id {
                let approval = db
                    .get_approval_request(approval_id)?
                    .ok_or_else(|| AppError::NotFound(format!("审批 {} 不存在", approval_id)))?;
                if approval.status != "approved" {
                    has_waiting_approval = true;
                    continue;
                }
            } else {
                has_waiting_approval = true;
                continue;
            }
            let target_key = step_target_key(step, &detail.run)?;
            let target = db
                .get_deployment_target(&target_key)?
                .ok_or_else(|| AppError::NotFound(format!("部署目标 '{}' 不存在", target_key)))?;
            let command = step.command_preview.clone();
            let result = TerminalService::execute(
                db,
                TerminalCommandInput {
                    server_alias: target.server_alias.clone(),
                    command,
                    timeout_secs: Some(120),
                    initiated_by_ai: Some(false),
                },
            )
            .await?;
            let status = if result.blocked {
                "blocked"
            } else if result.exit_status == 0 {
                "success"
            } else {
                "failed"
            };
            if status != "success" {
                has_failed = true;
            }
            db.update_deployment_run_step_result(
                step.id,
                status,
                &trim_preview(&result.stdout),
                &trim_preview(&result.stderr),
                Some(result.exit_status as i64),
                step.approval_id,
            )?;
            if has_failed {
                break;
            }
        }
        let (status, summary, finished) = if has_failed {
            ("failed", "审批步骤执行失败，请查看步骤日志。", true)
        } else if has_waiting_approval {
            ("approval_required", "仍有步骤等待审批。", false)
        } else {
            ("success", "部署执行完成。", true)
        };
        let run = db.update_deployment_run_status(run_id, status, summary, finished)?;
        Self::audit_run(db, &run, status, summary, None)?;
        Self::get_run_detail(db, run_id)
    }

    fn audit_run(
        db: &Database,
        run: &DeploymentRun,
        result: &str,
        summary: &str,
        approval_id: Option<i64>,
    ) -> Result<(), AppError> {
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: run.created_by.clone(),
                source: "deployment".into(),
                server_alias: String::new(),
                action: "deployment.run".into(),
                risk: if result == "success" {
                    "review"
                } else {
                    "high"
                }
                .into(),
                result: result.into(),
                summary: summary.into(),
                detail_json: Some(
                    json!({
                        "runId": run.run_id,
                        "targetKey": run.target_key,
                        "groupKey": run.group_key,
                        "status": run.status
                    })
                    .to_string(),
                ),
                request_id: Some(run.run_id.clone()),
                approval_id,
            },
        )?;
        Ok(())
    }

    async fn probe_environment(
        db: &Database,
        target: &DeploymentTarget,
    ) -> Result<DeploymentEnvironmentProbe, AppError> {
        if target.server_alias.trim().is_empty() {
            return Err(AppError::InvalidInput("部署目标未配置服务器".into()));
        }
        let command = build_probe_command(target);
        let output = TerminalService::execute(
            db,
            TerminalCommandInput {
                server_alias: target.server_alias.clone(),
                command,
                timeout_secs: Some(15),
                initiated_by_ai: Some(false),
            },
        )
        .await?;
        if output.blocked {
            return Err(AppError::InvalidInput(format!(
                "环境探测命令被策略阻断: {}",
                output.message
            )));
        }
        Ok(parse_probe_output(target, &output.stdout, &output.stderr))
    }
}

fn validate_required(value: &str, label: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::InvalidInput(format!("{}不能为空", label)));
    }
    Ok(())
}

fn normalize_key(value: &str) -> Result<String, AppError> {
    let normalized = value.trim().to_ascii_lowercase().replace(' ', "-");
    if normalized.is_empty() {
        return Err(AppError::InvalidInput("Key 不能为空".into()));
    }
    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(AppError::InvalidInput(
            "Key 只能包含字母、数字、-、_".into(),
        ));
    }
    Ok(normalized)
}

fn validate_recipe(recipe: &str) -> Result<(), AppError> {
    if DeploymentService::list_templates()
        .iter()
        .any(|item| item.key == recipe)
    {
        return Ok(());
    }
    Err(AppError::InvalidInput(format!(
        "不支持的部署配方: {}",
        recipe
    )))
}

fn validate_source_type(source_type: &str) -> Result<(), AppError> {
    match source_type {
        "local" | "git" | "image-store" => Ok(()),
        _ => Err(AppError::InvalidInput(
            "项目来源只能是 local、git 或 image-store".into(),
        )),
    }
}

fn checkout_git_source(
    db: &Database,
    input: &DetectDeploymentProjectInput,
) -> Result<(PathBuf, String, Vec<String>), AppError> {
    let git_url = input
        .git_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::InvalidInput("Git 仓库 URL 不能为空".into()))?;
    let credential_key = input
        .git_credential_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let slug = sanitize_path_segment(git_url);
    let checkout_root = std::env::temp_dir().join("tauri-ssh-deploy-checkouts");
    fs::create_dir_all(&checkout_root)?;
    let checkout_dir = checkout_root.join(slug);
    if checkout_dir.exists() {
        fs::remove_dir_all(&checkout_dir)?;
    }

    let mut clone_args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(git_ref) = input
        .git_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        clone_args.extend(["--branch".to_string(), git_ref.to_string()]);
    }
    clone_args.extend([
        git_url.to_string(),
        checkout_dir.to_string_lossy().to_string(),
    ]);
    let mut warnings = Vec::new();
    if let Some(credential_key) = credential_key {
        let session = SecureCredentialService::create_session(
            db,
            CreateSecureCredentialSessionInput {
                credential_key: credential_key.clone(),
                caller: Some("deployment-detect".into()),
                scopes: Vec::new(),
                ttl_minutes: Some(15),
            },
        )?;
        let secret = SecureCredentialService::get_secret(db, &credential_key)?;
        let username = git_username_for_provider(&session.provider);
        let askpass = write_git_askpass_script()?;
        let envs = vec![
            ("GIT_TERMINAL_PROMPT", "0".to_string()),
            ("GIT_ASKPASS", askpass.to_string_lossy().to_string()),
            ("GIT_DEPLOY_USERNAME", username),
            ("GIT_DEPLOY_TOKEN", secret),
        ];
        let clone_result = run_git_with_env(&clone_args, None, &envs);
        let _ = fs::remove_file(&askpass);
        clone_result?;
        warnings.push(format!(
            "已通过安全凭证 '{}' 创建短期会话 {} 完成 Git 检测，未返回凭证明文。",
            credential_key, session.session_id
        ));
    } else {
        run_git(&clone_args, None)?;
    }

    let commit = run_git(&["rev-parse".into(), "HEAD".into()], Some(&checkout_dir))?;
    Ok((checkout_dir, commit.trim().to_string(), warnings))
}

fn run_git(args: &[String], current_dir: Option<&Path>) -> Result<String, AppError> {
    run_git_with_env(args, current_dir, &[])
}

fn run_git_with_env(
    args: &[String],
    current_dir: Option<&Path>,
    envs: &[(&str, String)],
) -> Result<String, AppError> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().map_err(|error| {
        AppError::Custom(format!("执行 git 失败，请确认本机已安装 Git: {}", error))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Custom(if stderr.is_empty() {
            "Git 命令执行失败".into()
        } else {
            stderr
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_username_for_provider(provider: &str) -> String {
    match provider {
        "github" => "x-access-token",
        "gitlab" | "gitcode" | "gitee" => "oauth2",
        _ => "git",
    }
    .into()
}

fn write_git_askpass_script() -> Result<PathBuf, AppError> {
    let path = std::env::temp_dir().join(format!(
        "tauri-ssh-git-askpass-{}.{}",
        chrono::Local::now().format("%Y%m%d%H%M%S%3f"),
        if cfg!(windows) { "bat" } else { "sh" }
    ));
    let content = if cfg!(windows) {
        "@echo off\r\necho %GIT_DEPLOY_TOKEN%\r\n"
    } else {
        "#!/bin/sh\ncase \"$1\" in\n  *Username*) printf '%s\\n' \"$GIT_DEPLOY_USERNAME\" ;;\n  *) printf '%s\\n' \"$GIT_DEPLOY_TOKEN\" ;;\nesac\n"
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

fn sanitize_path_segment(value: &str) -> String {
    let mut output: String = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    output.truncate(80);
    if output.is_empty() {
        "repo".into()
    } else {
        output
    }
}

fn scan_candidates(
    root: &Path,
    dir: &Path,
    depth: usize,
    source_type: &str,
    seen: &mut HashSet<String>,
    candidates: &mut Vec<DeploymentCandidate>,
) -> Result<(), AppError> {
    if depth > 2 {
        return Ok(());
    }
    detect_dir(root, dir, source_type, seen, candidates)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || should_skip_dir(&path) {
            continue;
        }
        scan_candidates(root, &path, depth + 1, source_type, seen, candidates)?;
    }
    Ok(())
}

fn detect_dir(
    root: &Path,
    dir: &Path,
    source_type: &str,
    seen: &mut HashSet<String>,
    candidates: &mut Vec<DeploymentCandidate>,
) -> Result<(), AppError> {
    let relative = dir.strip_prefix(root).unwrap_or(dir);
    let workdir = if relative.as_os_str().is_empty() {
        ".".to_string()
    } else {
        relative.to_string_lossy().to_string()
    };
    let dir_name = dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("app")
        .to_string();
    let raw_key = if workdir == "." {
        dir_name.clone()
    } else {
        workdir.replace('/', "-")
    };
    let base_key = normalize_key(&raw_key)?;

    let dockerfile = find_named_file(dir, &["Dockerfile"]);
    let compose = find_named_file(
        dir,
        &[
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ],
    );
    let package_json = dir.join("package.json");
    let pom = dir.join("pom.xml");
    let gradle = dir.join("build.gradle");
    let go_mod = dir.join("go.mod");

    if let Some(compose_file) = compose {
        push_candidate(
            seen,
            candidates,
            DeploymentCandidate {
                key: unique_key(&base_key, "compose"),
                name: format!("{} Compose 栈", readable_name(&dir_name)),
                recipe: "docker-compose".into(),
                confidence: 95,
                source_type: source_type.into(),
                workdir: workdir.clone(),
                build_command: String::new(),
                start_command: "docker compose up -d".into(),
                artifact_dir: String::new(),
                dockerfile: String::new(),
                compose_file: compose_file.to_string_lossy().to_string(),
                exposed_ports: extract_ports_from_text(
                    &fs::read_to_string(&compose_file).unwrap_or_default(),
                ),
                env_files: find_env_files(dir),
                detected_frameworks: vec!["docker-compose".into()],
                warnings: Vec::new(),
                config_json: json!({ "composeFile": compose_file.to_string_lossy() }).to_string(),
            },
        );
    }

    if let Some(dockerfile_path) = dockerfile {
        push_candidate(
            seen,
            candidates,
            DeploymentCandidate {
                key: unique_key(&base_key, "docker"),
                name: format!("{} Dockerfile 服务", readable_name(&dir_name)),
                recipe: "dockerfile-service".into(),
                confidence: 90,
                source_type: source_type.into(),
                workdir: workdir.clone(),
                build_command: "docker build".into(),
                start_command: "docker compose up -d".into(),
                artifact_dir: String::new(),
                dockerfile: dockerfile_path.to_string_lossy().to_string(),
                compose_file: String::new(),
                exposed_ports: extract_expose_ports(&dockerfile_path),
                env_files: find_env_files(dir),
                detected_frameworks: detect_frameworks(dir),
                warnings: Vec::new(),
                config_json: json!({ "buildMode": "remote", "dockerfile": dockerfile_path.to_string_lossy() }).to_string(),
            },
        );
    }

    if package_json.exists() {
        let package = fs::read_to_string(&package_json).unwrap_or_default();
        let frameworks = detect_package_frameworks(&package);
        let is_frontend = frameworks.iter().any(|item| {
            ["vite", "react", "vue", "nuxt", "next", "uniapp"].contains(&item.as_str())
        });
        let recipe = if is_frontend {
            "static-openresty"
        } else {
            "node-pm2"
        };
        push_candidate(
            seen,
            candidates,
            DeploymentCandidate {
                key: unique_key(&base_key, recipe),
                name: format!(
                    "{} {}",
                    readable_name(&dir_name),
                    if is_frontend {
                        "前端静态站"
                    } else {
                        "Node 服务"
                    }
                ),
                recipe: recipe.into(),
                confidence: if is_frontend { 85 } else { 70 },
                source_type: source_type.into(),
                workdir: workdir.clone(),
                build_command: default_node_build_command(dir, is_frontend),
                start_command: if is_frontend {
                    String::new()
                } else {
                    "pm2 reload ecosystem.config.js".into()
                },
                artifact_dir: if is_frontend {
                    default_artifact_dir(dir)
                } else {
                    String::new()
                },
                dockerfile: String::new(),
                compose_file: String::new(),
                exposed_ports: Vec::new(),
                env_files: find_env_files(dir),
                detected_frameworks: frameworks,
                warnings: Vec::new(),
                config_json: json!({ "packageJson": package_json.to_string_lossy() }).to_string(),
            },
        );
    }

    if pom.exists() || gradle.exists() || go_mod.exists() {
        push_candidate(
            seen,
            candidates,
            DeploymentCandidate {
                key: unique_key(&base_key, "systemd"),
                name: format!("{} Systemd 服务", readable_name(&dir_name)),
                recipe: "systemd-binary".into(),
                confidence: 65,
                source_type: source_type.into(),
                workdir,
                build_command: default_binary_build_command(dir),
                start_command: "systemctl restart <service>".into(),
                artifact_dir: "target".into(),
                dockerfile: String::new(),
                compose_file: String::new(),
                exposed_ports: Vec::new(),
                env_files: find_env_files(dir),
                detected_frameworks: detect_frameworks(dir),
                warnings: vec!["Systemd 服务写入和重启需要审批".into()],
                config_json: json!({ "serviceManager": "systemd" }).to_string(),
            },
        );
    }

    Ok(())
}

fn push_candidate(
    seen: &mut HashSet<String>,
    candidates: &mut Vec<DeploymentCandidate>,
    mut candidate: DeploymentCandidate,
) {
    if !seen.insert(candidate.key.clone()) {
        candidate.key = format!("{}-{}", candidate.key, candidates.len() + 1);
        seen.insert(candidate.key.clone());
    }
    candidates.push(candidate);
}

fn unique_key(base: &str, suffix: &str) -> String {
    if base.ends_with(suffix) {
        base.to_string()
    } else {
        format!("{}-{}", base, suffix)
    }
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some(".git" | "node_modules" | "target" | "dist" | "build" | ".next" | ".output")
    )
}

fn find_named_file(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

fn find_env_files(dir: &Path) -> Vec<String> {
    [".env", ".env.example"]
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

fn extract_expose_ports(path: &Path) -> Vec<i64> {
    fs::read_to_string(path)
        .map(|content| {
            content
                .lines()
                .filter_map(|line| line.trim().strip_prefix("EXPOSE "))
                .flat_map(|ports| ports.split_whitespace())
                .filter_map(|port| port.split('/').next())
                .filter_map(|port| port.parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn extract_ports_from_text(content: &str) -> Vec<i64> {
    content
        .lines()
        .filter_map(|line| line.split(':').next_back())
        .map(str::trim)
        .filter_map(|value| value.trim_matches('"').parse::<i64>().ok())
        .collect()
}

fn detect_frameworks(dir: &Path) -> Vec<String> {
    let mut frameworks = Vec::new();
    if dir.join("pom.xml").exists() {
        frameworks.push("maven".into());
    }
    if dir.join("build.gradle").exists() {
        frameworks.push("gradle".into());
    }
    if dir.join("go.mod").exists() {
        frameworks.push("go".into());
    }
    if dir.join("requirements.txt").exists() || dir.join("pyproject.toml").exists() {
        frameworks.push("python".into());
    }
    frameworks
}

fn detect_package_frameworks(content: &str) -> Vec<String> {
    let mut frameworks = Vec::new();
    for (needle, label) in [
        ("\"vite\"", "vite"),
        ("\"react\"", "react"),
        ("\"vue\"", "vue"),
        ("\"nuxt\"", "nuxt"),
        ("\"next\"", "next"),
        ("\"@dcloudio", "uniapp"),
        ("\"express\"", "express"),
        ("\"@nestjs", "nestjs"),
    ] {
        if content.contains(needle) {
            frameworks.push(label.to_string());
        }
    }
    if frameworks.is_empty() {
        frameworks.push("node".into());
    }
    frameworks
}

fn default_node_build_command(dir: &Path, frontend: bool) -> String {
    let script = if frontend { "build" } else { "build" };
    if dir.join("pnpm-lock.yaml").exists() {
        format!("pnpm install && pnpm run {}", script)
    } else if dir.join("yarn.lock").exists() {
        format!("yarn install && yarn {}", script)
    } else {
        format!("npm install && npm run {}", script)
    }
}

fn default_artifact_dir(dir: &Path) -> String {
    for name in ["dist", "build", ".output/public", "unpackage/dist/build/h5"] {
        if dir.join(name).exists() {
            return name.into();
        }
    }
    "dist".into()
}

fn default_binary_build_command(dir: &Path) -> String {
    if dir.join("pom.xml").exists() {
        "mvn clean package -DskipTests".into()
    } else if dir.join("build.gradle").exists() {
        "gradle bootJar".into()
    } else if dir.join("go.mod").exists() {
        "go build ./...".into()
    } else {
        String::new()
    }
}

fn readable_name(value: &str) -> String {
    value.replace(['-', '_'], " ")
}

fn build_probe_command(target: &DeploymentTarget) -> String {
    let port = target.port.unwrap_or(0);
    let domain = shell_single_quote(&target.domain);
    format!(
        r#"set +e
echo "__TAURI_DEPLOY_PROBE_START__"
echo "OS=$(uname -s 2>/dev/null)"
echo "ARCH=$(uname -m 2>/dev/null)"
echo "USER=$(id -un 2>/dev/null)"
echo "DISK_AVAILABLE_KB=$(df -Pk / 2>/dev/null | awk 'NR==2 {{print $4}}')"
echo "DOCKER_VERSION=$(docker --version 2>/dev/null || true)"
echo "COMPOSE_VERSION=$(docker compose version 2>/dev/null || docker-compose --version 2>/dev/null || true)"
echo "NGINX_VERSION=$(nginx -v 2>&1 || true)"
echo "OPENRESTY_VERSION=$(openresty -v 2>&1 || true)"
echo "GIT_VERSION=$(git --version 2>/dev/null || true)"
if [ {port} -gt 0 ]; then
  if (ss -ltn 2>/dev/null || netstat -ltn 2>/dev/null) | grep -E '[:.]'{port}'[[:space:]]' >/dev/null; then
    echo "PORT_{port}=busy"
  else
    echo "PORT_{port}=free"
  fi
else
  echo "PORT_0=unknown"
fi
if [ -n {domain} ]; then
  if getent hosts {domain} >/dev/null 2>&1 || nslookup {domain} >/dev/null 2>&1; then
    echo "DOMAIN_RESOLVED=yes"
  else
    echo "DOMAIN_RESOLVED=no"
  fi
else
  echo "DOMAIN_RESOLVED=unknown"
fi
echo "__TAURI_DEPLOY_PROBE_END__"
"#
    )
}

fn shell_single_quote(value: &str) -> String {
    if value.trim().is_empty() {
        "''".into()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

fn docker_image_ref(target: &DeploymentTarget, release_id: &str) -> String {
    let name = target
        .target_key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    format!(
        "{}:{}",
        if name.is_empty() {
            "tauri-ssh-app"
        } else {
            &name
        },
        release_id
    )
}

fn parse_probe_output(
    target: &DeploymentTarget,
    stdout: &str,
    stderr: &str,
) -> DeploymentEnvironmentProbe {
    let get = |key: &str| -> String {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{}=", key)))
            .unwrap_or("")
            .trim()
            .to_string()
    };

    let os = get("OS");
    let arch = get("ARCH");
    let user = get("USER");
    let disk_available_kb = get("DISK_AVAILABLE_KB").parse::<i64>().ok();
    let docker_version = get("DOCKER_VERSION");
    let compose_version = get("COMPOSE_VERSION");
    let nginx_version = get("NGINX_VERSION");
    let openresty_version = get("OPENRESTY_VERSION");
    let git_version = get("GIT_VERSION");
    let port_status = get(&format!("PORT_{}", target.port.unwrap_or(0)));
    let port_available = match port_status.as_str() {
        "free" => Some(true),
        "busy" => Some(false),
        _ => None,
    };
    let domain_resolved = match get("DOMAIN_RESOLVED").as_str() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    };

    let mut checks = vec![
        check(
            "linux",
            "Linux 目标服务器",
            if os.to_ascii_lowercase().contains("linux") {
                "ok"
            } else {
                "warning"
            },
            if os.is_empty() {
                "无法读取系统类型"
            } else {
                &os
            },
        ),
        check(
            "docker",
            "Docker",
            if docker_version.is_empty() {
                "warning"
            } else {
                "ok"
            },
            if docker_version.is_empty() {
                "未检测到 Docker"
            } else {
                &docker_version
            },
        ),
        check(
            "compose",
            "Docker Compose",
            if compose_version.is_empty() {
                "warning"
            } else {
                "ok"
            },
            if compose_version.is_empty() {
                "未检测到 Docker Compose"
            } else {
                &compose_version
            },
        ),
        check(
            "git",
            "Git",
            if git_version.is_empty() {
                "warning"
            } else {
                "ok"
            },
            if git_version.is_empty() {
                "未检测到 Git"
            } else {
                &git_version
            },
        ),
    ];

    if target.recipe == "static-openresty" || target.https_enabled {
        let web_status = if !openresty_version.is_empty() || !nginx_version.is_empty() {
            "ok"
        } else {
            "warning"
        };
        let web_message = if !openresty_version.is_empty() {
            openresty_version.as_str()
        } else if !nginx_version.is_empty() {
            nginx_version.as_str()
        } else {
            "未检测到 OpenResty/Nginx"
        };
        checks.push(check("web", "OpenResty/Nginx", web_status, web_message));
    }

    if let Some(available) = port_available {
        checks.push(check(
            "port",
            "端口占用",
            if available { "ok" } else { "warning" },
            if available {
                "目标端口空闲"
            } else {
                "目标端口已被监听"
            },
        ));
    }

    if let Some(resolved) = domain_resolved {
        checks.push(check(
            "domain",
            "域名解析",
            if resolved { "ok" } else { "warning" },
            if resolved {
                "域名可解析"
            } else {
                "域名暂未解析或目标机无法解析"
            },
        ));
    }

    DeploymentEnvironmentProbe {
        server_alias: target.server_alias.clone(),
        os,
        arch,
        user,
        disk_available_kb,
        docker_version,
        compose_version,
        nginx_version,
        openresty_version,
        git_version,
        port_available,
        domain_resolved,
        checks,
        raw_output: if stderr.trim().is_empty() {
            stdout.to_string()
        } else {
            format!("{}\n{}", stdout, stderr)
        },
    }
}

fn check(key: &str, label: &str, status: &str, message: &str) -> DeploymentProbeCheck {
    DeploymentProbeCheck {
        key: key.into(),
        label: label.into(),
        status: status.into(),
        message: message.into(),
    }
}

fn image_store_catalog() -> Vec<DeploymentImageStoreApp> {
    vec![
        image_store_app(
            "nginx",
            "Nginx",
            "Web 服务 / 静态文件服务",
            "nginx",
            "latest",
            Some(8080),
            Some(80),
            "/usr/share/nginx/html",
            vec![],
            vec!["默认映射到宿主 8080 端口，可后续挂载站点目录。"],
        ),
        image_store_app(
            "mysql",
            "MySQL",
            "关系型数据库",
            "mysql",
            "8.4",
            Some(3306),
            Some(3306),
            "/var/lib/mysql",
            vec![image_store_env(
                "MYSQL_ROOT_PASSWORD",
                "Root 密码",
                "ChangeMe_123456",
                true,
                true,
            )],
            vec!["生产环境请修改默认 Root 密码。"],
        ),
        image_store_app(
            "postgres",
            "PostgreSQL",
            "关系型数据库",
            "postgres",
            "16",
            Some(5432),
            Some(5432),
            "/var/lib/postgresql/data",
            vec![
                image_store_env("POSTGRES_USER", "用户名", "postgres", true, false),
                image_store_env("POSTGRES_PASSWORD", "密码", "ChangeMe_123456", true, true),
            ],
            vec!["生产环境请修改默认数据库密码。"],
        ),
        image_store_app(
            "redis",
            "Redis",
            "缓存 / KV 存储",
            "redis",
            "7",
            Some(6379),
            Some(6379),
            "/data",
            vec![],
            vec!["默认未开启密码，生产环境建议在配置中补充 requirepass。"],
        ),
        image_store_app(
            "portainer",
            "Portainer",
            "Docker 可视化管理",
            "portainer/portainer-ce",
            "latest",
            Some(9000),
            Some(9000),
            "/data",
            vec![],
            vec!["会挂载 /var/run/docker.sock，请仅安装在可信服务器。"],
        ),
        image_store_app(
            "minio",
            "MinIO",
            "S3 兼容对象存储",
            "minio/minio",
            "latest",
            Some(9001),
            Some(9001),
            "/data",
            vec![
                image_store_env("MINIO_ROOT_USER", "Root 用户", "minioadmin", true, false),
                image_store_env(
                    "MINIO_ROOT_PASSWORD",
                    "Root 密码",
                    "ChangeMe_123456",
                    true,
                    true,
                ),
            ],
            vec!["API 默认容器端口 9000，控制台默认映射到宿主 9001。"],
        ),
        image_store_app(
            "gitea",
            "Gitea",
            "轻量 Git 服务",
            "gitea/gitea",
            "latest",
            Some(3000),
            Some(3000),
            "/data",
            vec![],
            vec!["SSH 端口未默认映射，可在配置 JSON 中扩展 compose。"],
        ),
        image_store_app(
            "grafana",
            "Grafana",
            "监控可视化",
            "grafana/grafana",
            "latest",
            Some(3001),
            Some(3000),
            "/var/lib/grafana",
            vec![
                image_store_env("GF_SECURITY_ADMIN_USER", "管理员", "admin", true, false),
                image_store_env(
                    "GF_SECURITY_ADMIN_PASSWORD",
                    "管理员密码",
                    "ChangeMe_123456",
                    true,
                    true,
                ),
            ],
            vec!["Grafana 容器端口是 3000，默认映射到宿主 3001。"],
        ),
        image_store_app(
            "rabbitmq",
            "RabbitMQ",
            "消息队列",
            "rabbitmq",
            "3-management",
            Some(15672),
            Some(15672),
            "/var/lib/rabbitmq",
            vec![],
            vec!["AMQP 5672 未默认作为主端口展示，可在 compose 中扩展映射。"],
        ),
        image_store_app(
            "mongo",
            "MongoDB",
            "文档数据库",
            "mongo",
            "7",
            Some(27017),
            Some(27017),
            "/data/db",
            vec![],
            vec!["默认未启用认证，生产环境请补充账号密码配置。"],
        ),
        image_store_app(
            "rocketmq-namesrv",
            "RocketMQ NameServer",
            "RocketMQ 注册中心",
            "apache/rocketmq",
            "5.3.0",
            Some(9876),
            Some(9876),
            "/home/rocketmq/logs",
            vec![image_store_env(
                "JAVA_OPT_EXT",
                "JVM 参数",
                "-server -Xms256m -Xmx256m",
                false,
                false,
            )],
            vec!["默认启动 NameServer；Broker 请使用 RocketMQ Broker 镜像项单独安装。"],
        ),
        image_store_app(
            "rocketmq-broker",
            "RocketMQ Broker",
            "RocketMQ 消息 Broker",
            "apache/rocketmq",
            "5.3.0",
            Some(10911),
            Some(10911),
            "/home/rocketmq/store",
            vec![
                image_store_env(
                    "NAMESRV_ADDR",
                    "NameServer 地址",
                    "127.0.0.1:9876",
                    true,
                    false,
                ),
                image_store_env(
                    "JAVA_OPT_EXT",
                    "JVM 参数",
                    "-server -Xms512m -Xmx512m",
                    false,
                    false,
                ),
            ],
            vec!["默认按单机快速部署生成；生产环境请把 NAMESRV_ADDR 改为真实 NameServer 地址。"],
        ),
        image_store_app(
            "elasticsearch",
            "Elasticsearch",
            "搜索引擎 / 日志检索",
            "docker.elastic.co/elasticsearch/elasticsearch",
            "8.15.3",
            Some(9200),
            Some(9200),
            "/usr/share/elasticsearch/data",
            vec![
                image_store_env("discovery.type", "发现模式", "single-node", true, false),
                image_store_env("xpack.security.enabled", "安全认证", "false", true, false),
                image_store_env(
                    "ES_JAVA_OPTS",
                    "JVM 参数",
                    "-Xms512m -Xmx512m",
                    false,
                    false,
                ),
            ],
            vec!["生产环境建议开启认证，并提前设置 vm.max_map_count。"],
        ),
        image_store_app(
            "skywalking-oap",
            "SkyWalking OAP",
            "APM 后端分析服务",
            "apache/skywalking-oap-server",
            "10.0.1",
            Some(12800),
            Some(12800),
            "/skywalking/ext-config",
            vec![image_store_env("SW_STORAGE", "存储类型", "h2", true, false)],
            vec!["默认使用 H2 便于快速体验；生产环境建议切换到 Elasticsearch 存储。"],
        ),
        image_store_app(
            "skywalking-ui",
            "SkyWalking UI",
            "APM 可视化界面",
            "apache/skywalking-ui",
            "10.0.1",
            Some(8080),
            Some(8080),
            "/skywalking/ext-config",
            vec![image_store_env(
                "SW_OAP_ADDRESS",
                "OAP 地址",
                "http://127.0.0.1:12800",
                true,
                false,
            )],
            vec!["如果 OAP 不在同一宿主机，请修改 SW_OAP_ADDRESS。"],
        ),
        image_store_app(
            "elk",
            "ELK",
            "Elasticsearch + Logstash + Kibana",
            "docker.elastic.co/elasticsearch/elasticsearch",
            "8.15.3",
            Some(5601),
            Some(5601),
            "/usr/share/elasticsearch/data",
            vec![
                image_store_env(
                    "ELASTIC_PASSWORD",
                    "Elastic 密码",
                    "ChangeMe_123456",
                    false,
                    true,
                ),
                image_store_env(
                    "ES_JAVA_OPTS",
                    "ES JVM 参数",
                    "-Xms512m -Xmx512m",
                    false,
                    false,
                ),
            ],
            vec!["会生成三容器 compose，默认开放 Kibana 5601、Elasticsearch 9200、Logstash 5044。"],
        ),
    ]
}

fn image_store_app(
    key: &str,
    name: &str,
    description: &str,
    image: &str,
    tag: &str,
    default_port: Option<i64>,
    container_port: Option<i64>,
    volume_path: &str,
    env: Vec<DeploymentImageStoreEnv>,
    notes: Vec<&str>,
) -> DeploymentImageStoreApp {
    DeploymentImageStoreApp {
        key: key.into(),
        name: name.into(),
        description: description.into(),
        category: "常用镜像".into(),
        image: image.into(),
        tag: tag.into(),
        default_port,
        container_port,
        volume_path: volume_path.into(),
        env,
        notes: notes.into_iter().map(str::to_string).collect(),
    }
}

fn image_store_env(
    key: &str,
    label: &str,
    default_value: &str,
    required: bool,
    secret: bool,
) -> DeploymentImageStoreEnv {
    DeploymentImageStoreEnv {
        key: key.into(),
        label: label.into(),
        default_value: default_value.into(),
        required,
        secret,
    }
}

fn image_store_env_values(
    app: &DeploymentImageStoreApp,
    overrides: &Value,
) -> Vec<(String, String)> {
    app.env
        .iter()
        .map(|item| {
            let value = overrides
                .get(&item.key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| item.default_value.clone());
            (item.key.clone(), value)
        })
        .collect()
}

fn image_store_compose_yaml(
    app: &DeploymentImageStoreApp,
    target_key: &str,
    tag: &str,
    port: Option<i64>,
    deploy_root: &str,
    env: &[(String, String)],
) -> String {
    if app.key == "elk" {
        return image_store_elk_compose_yaml(target_key, tag, port, deploy_root, env);
    }

    let image = format!("{}:{}", app.image, tag);
    let data_dir = format!("{}/data", deploy_root.trim_end_matches('/'));
    let mut lines = vec![
        "services:".to_string(),
        format!("  {}:", target_key),
        format!("    image: {}", image),
        format!("    container_name: {}", target_key),
        "    restart: unless-stopped".into(),
    ];
    if let (Some(host_port), Some(container_port)) = (port, app.container_port) {
        lines.push("    ports:".into());
        lines.push(format!(
            "      - \"127.0.0.1:{}:{}\"",
            host_port, container_port
        ));
    }
    if !env.is_empty() {
        lines.push("    environment:".into());
        for (key, value) in env {
            lines.push(format!("      {}: {}", key, yaml_string(value)));
        }
    }
    lines.push("    volumes:".into());
    if app.key == "portainer" {
        lines.push("      - /var/run/docker.sock:/var/run/docker.sock".into());
    }
    lines.push(format!("      - {}:{}", data_dir, app.volume_path));
    if app.key == "minio" {
        lines.push("    command: server /data --console-address \":9001\"".into());
    }
    if app.key == "rocketmq-namesrv" {
        lines.push("    command: sh mqnamesrv".into());
    }
    if app.key == "rocketmq-broker" {
        let namesrv_addr = env
            .iter()
            .find(|(key, _)| key == "NAMESRV_ADDR")
            .map(|(_, value)| value.as_str())
            .unwrap_or("127.0.0.1:9876");
        lines.push(format!("    command: sh mqbroker -n {}", namesrv_addr));
    }
    lines.join("\n")
}

fn image_store_elk_compose_yaml(
    target_key: &str,
    tag: &str,
    port: Option<i64>,
    deploy_root: &str,
    env: &[(String, String)],
) -> String {
    let root = deploy_root.trim_end_matches('/');
    let kibana_port = port.unwrap_or(5601);
    let es_java_opts = env
        .iter()
        .find(|(key, _)| key == "ES_JAVA_OPTS")
        .map(|(_, value)| value.as_str())
        .unwrap_or("-Xms512m -Xmx512m");
    vec![
        "services:".to_string(),
        format!("  {}-elasticsearch:", target_key),
        format!(
            "    image: docker.elastic.co/elasticsearch/elasticsearch:{}",
            tag
        ),
        format!("    container_name: {}-elasticsearch", target_key),
        "    restart: unless-stopped".into(),
        "    environment:".into(),
        "      discovery.type: single-node".into(),
        "      xpack.security.enabled: \"false\"".into(),
        format!("      ES_JAVA_OPTS: {}", yaml_string(es_java_opts)),
        "    ports:".into(),
        "      - \"127.0.0.1:9200:9200\"".into(),
        "    volumes:".into(),
        format!(
            "      - {}/elasticsearch:/usr/share/elasticsearch/data",
            root
        ),
        format!("  {}-logstash:", target_key),
        format!("    image: docker.elastic.co/logstash/logstash:{}", tag),
        format!("    container_name: {}-logstash", target_key),
        "    restart: unless-stopped".into(),
        "    depends_on:".into(),
        format!("      - {}-elasticsearch", target_key),
        "    ports:".into(),
        "      - \"127.0.0.1:5044:5044\"".into(),
        format!("  {}-kibana:", target_key),
        format!("    image: docker.elastic.co/kibana/kibana:{}", tag),
        format!("    container_name: {}-kibana", target_key),
        "    restart: unless-stopped".into(),
        "    depends_on:".into(),
        format!("      - {}-elasticsearch", target_key),
        "    environment:".into(),
        format!(
            "      ELASTICSEARCH_HOSTS: {}",
            yaml_string(&format!("http://{}-elasticsearch:9200", target_key))
        ),
        "    ports:".into(),
        format!("      - \"127.0.0.1:{}:5601\"", kibana_port),
    ]
    .join("\n")
}

fn build_plan_stages(
    target: &DeploymentTarget,
    environment: &DeploymentEnvironmentProbe,
) -> Vec<DeploymentPlanStage> {
    let mut stages = vec![
        stage(
            "probe",
            "环境探测",
            "readonly",
            false,
            "",
            "读取 Linux、Docker、Compose、Web 服务、端口、域名和磁盘信息。",
        ),
        stage(
            "prepare",
            "准备部署目录",
            "review",
            false,
            &format!("mkdir -p {}", target.deploy_root),
            "创建部署根目录并检查写入权限；真实执行阶段会受策略和审批控制。",
        ),
    ];

    if target.recipe == "image-store" {
        stages.push(stage(
            "source",
            "读取镜像商店配置",
            "review",
            false,
            "render image-store docker-compose.yml",
            "从镜像商店应用配置生成 docker-compose.yml，不需要上传本地项目。",
        ));
    } else if target.source_type == "git" {
        stages.push(stage(
            "source",
            "拉取 Git 仓库",
            "review",
            false,
            &format!(
                "git clone --branch {} <repo> {}",
                if target.git_ref.is_empty() {
                    "<default>"
                } else {
                    &target.git_ref
                },
                target.deploy_root
            ),
            "使用安全凭证短期会话拉取仓库，不在日志中输出 Token 或私钥。",
        ));
    } else {
        stages.push(stage(
            "source",
            "上传本地项目",
            "review",
            false,
            &format!(
                "sftp upload {} -> {}",
                target.project_path, target.deploy_root
            ),
            "打包并上传本地项目目录，上传清单会在真实 dry-run 中展开。",
        ));
    }

    match target.recipe.as_str() {
        "dockerfile-service" => {
            if target.docker_build_mode == "local_upload" {
                stages.push(stage(
                    "build",
                    "本地构建镜像并上传",
                    "review",
                    false,
                    "docker build -t <image>:<version> . && docker save <image> | gzip",
                    "本地构建镜像 tar 包后上传目标服务器，再执行 docker load。",
                ));
            } else {
                stages.push(stage(
                    "build",
                    "远程构建镜像",
                    "review",
                    false,
                    "docker build -t <image>:<version> .",
                    "默认远程构建 Docker 镜像，适合目标服务器能访问依赖源的场景。",
                ));
            }
            stages.push(stage(
                "configure",
                "生成 Compose 托管配置",
                "review",
                false,
                "write docker-compose.yml and .env",
                "生成容器名称、端口、环境变量、卷挂载和资源限制。",
            ));
            stages.push(stage(
                "deploy",
                "启动容器",
                "high",
                true,
                "docker compose up -d",
                "启动或重建容器，属于高风险步骤，需要审批或二次确认。",
            ));
        }
        "docker-compose" => {
            stages.push(stage(
                "validate_compose",
                "检查 Compose 配置",
                "review",
                false,
                "docker compose config",
                "检查端口、volume、privileged、host network 和 env 缺失风险。",
            ));
            stages.push(stage(
                "deploy",
                "启动 Compose 栈",
                "high",
                true,
                "docker compose up -d",
                "启动或更新 compose 栈，涉及容器重建，需要审批或二次确认。",
            ));
        }
        "1panel-app" => {
            stages.push(stage(
                "validate_compose",
                "检查 1Panel Compose 配置",
                "review",
                false,
                "docker compose config",
                "按 1Panel 应用目录约定检查 compose 配置、环境变量和服务名。",
            ));
            stages.push(stage(
                "deploy",
                "重启 1Panel 托管服务",
                "high",
                true,
                "docker compose up -d",
                "上传产物后重建或重启对应 1Panel compose 服务，需要审批或二次确认。",
            ));
        }
        "static-openresty" | "static-nginx" => {
            stages.push(stage(
                "build",
                "构建前端产物",
                "review",
                false,
                "pnpm install && pnpm run build",
                "构建 dist/build 等静态产物，真实执行会记录产物摘要。",
            ));
            stages.push(stage(
                "configure_web",
                "写入静态站配置",
                "high",
                true,
                "write nginx/openresty site config && reload",
                "写入站点配置并 reload Web 服务，需要审批。",
            ));
        }
        "node-pm2" => {
            stages.push(stage(
                "deploy",
                "PM2 发布",
                "high",
                true,
                "pm2 reload ecosystem.config.js",
                "重载 Node 服务，需要审批或二次确认。",
            ));
        }
        "systemd-binary" => {
            stages.push(stage(
                "deploy",
                "Systemd 发布",
                "high",
                true,
                "systemctl daemon-reload && systemctl restart <service>",
                "写入 unit 或重启服务，需要审批。",
            ));
        }
        "image-store" => {
            stages.push(stage(
                "pull_image",
                "拉取镜像",
                "review",
                false,
                "docker pull <image>:<tag>",
                "从镜像仓库拉取应用镜像，失败时保留远端输出。",
            ));
            stages.push(stage(
                "configure",
                "生成 Compose 配置",
                "review",
                false,
                "write image-store docker-compose.yml",
                "根据端口、volume 和环境变量生成 docker-compose.yml。",
            ));
            stages.push(stage(
                "deploy",
                "启动镜像应用",
                "high",
                true,
                "docker compose up -d",
                "启动或更新镜像商店应用容器，需要审批或二次确认。",
            ));
        }
        "custom-script" => {
            let custom_stages = custom_script_stages(&target.config_json);
            if custom_stages.is_empty() {
                stages.push(stage(
                    "custom_config_required",
                    "配置自定义脚本",
                    "review",
                    false,
                    "",
                    "请在扩展配置 JSON 中配置 customStages 或 customCommand 后再执行。",
                ));
            } else {
                stages.extend(custom_stages);
            }
        }
        _ => {
            stages.push(stage(
                "unsupported_recipe",
                "不支持的部署配方",
                "review",
                false,
                "",
                "当前部署配方未匹配到内置执行计划。",
            ));
        }
    }

    if target.https_enabled && !target.domain.trim().is_empty() {
        stages.push(stage(
            "https",
            "HTTPS 自动签证书",
            "high",
            true,
            "issue certificate for domain and configure renewal hook",
            "检查域名解析和 80/443 连通后签发证书，写证书和 reload Web 服务需要审批。",
        ));
    }

    if config_requests_service_account(&target.config_json) {
        stages.push(stage(
            "service_accounts",
            "数据库/Redis 专属账号",
            "high",
            true,
            "CREATE DATABASE / CREATE USER / GRANT / Redis ACL SETUSER",
            "根据配置生成专属库、账号、授权和凭证登记计划，需要审批。",
        ));
    } else {
        stages.push(stage(
            "service_accounts_preview",
            "数据库/Redis 专属账号检查",
            "review",
            false,
            "",
            "当前目标未声明数据库/Redis 专属账号配置，后续可在扩展配置 JSON 中补充。",
        ));
    }

    stages.push(stage(
        "health_check",
        "健康检查",
        "readonly",
        false,
        health_check_preview(target),
        "使用 HTTP/TCP/命令检查服务状态；失败时保留运行日志和现场。",
    ));
    stages.push(stage(
        "audit",
        "审计收尾",
        "readonly",
        false,
        "",
        "记录计划、审批、步骤和风险摘要。",
    ));

    if environment
        .checks
        .iter()
        .any(|item| item.status == "warning")
    {
        stages.insert(
            1,
            stage(
                "environment_warnings",
                "环境风险提示",
                "review",
                false,
                "",
                "环境探测发现风险，真实执行前需要处理或确认。",
            ),
        );
    }

    stages
}

fn stage(
    key: &str,
    title: &str,
    risk: &str,
    approval_required: bool,
    command_preview: &str,
    summary: &str,
) -> DeploymentPlanStage {
    DeploymentPlanStage {
        key: key.into(),
        title: title.into(),
        risk: risk.into(),
        approval_required,
        command_preview: command_preview.into(),
        summary: summary.into(),
        status: "planned".into(),
    }
}

fn custom_script_stages(config_json: &str) -> Vec<DeploymentPlanStage> {
    let Ok(value) = serde_json::from_str::<Value>(config_json) else {
        return Vec::new();
    };
    if let Some(stages) = value
        .get("customStages")
        .or_else(|| value.get("stages"))
        .and_then(Value::as_array)
    {
        return stages
            .iter()
            .enumerate()
            .filter_map(|(index, item)| custom_script_stage_from_value(index, item))
            .collect();
    }
    json_string(&value, "customCommand")
        .or_else(|| json_string(&value, "command"))
        .map(|command| {
            vec![stage(
                "custom_script",
                "执行自定义部署脚本",
                "high",
                true,
                &command,
                "自定义脚本会进入危险命令扫描和审批流程。",
            )]
        })
        .unwrap_or_default()
}

fn custom_script_stage_from_value(index: usize, value: &Value) -> Option<DeploymentPlanStage> {
    let command = json_string(value, "command")
        .or_else(|| json_string(value, "script"))
        .or_else(|| json_string(value, "shell"))?;
    let raw_key = json_string(value, "key").unwrap_or_else(|| format!("step_{}", index + 1));
    let key = format!("custom_{}", sanitize_stage_key(&raw_key));
    let title = json_string(value, "title").unwrap_or_else(|| format!("自定义脚本 {}", index + 1));
    let risk = json_string(value, "risk").unwrap_or_else(|| "high".into());
    let approval_required = value
        .get("approvalRequired")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let summary = json_string(value, "summary")
        .unwrap_or_else(|| "自定义脚本会进入危险命令扫描，并按风险级别进入审批或二次确认。".into());
    Some(stage(
        &key,
        &title,
        &risk,
        approval_required,
        &command,
        &summary,
    ))
}

fn sanitize_stage_key(value: &str) -> String {
    let mut output: String = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    output.truncate(48);
    if output.is_empty() {
        "step".into()
    } else {
        output
    }
}

fn build_plan_warnings(
    target: &DeploymentTarget,
    environment: &DeploymentEnvironmentProbe,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if !environment.os.to_ascii_lowercase().contains("linux") {
        warnings.push("首版仅支持 Linux 目标服务器。".into());
    }
    if matches!(
        target.recipe.as_str(),
        "1panel-app" | "dockerfile-service" | "docker-compose" | "image-store"
    ) && environment.docker_version.trim().is_empty()
    {
        warnings.push("目标服务器未检测到 Docker，真实执行前需要安装或选择其他配方。".into());
    }
    if matches!(target.recipe.as_str(), "1panel-app" | "docker-compose")
        && environment.compose_version.trim().is_empty()
    {
        warnings.push("目标服务器未检测到 Docker Compose。".into());
    }
    if target.https_enabled && environment.domain_resolved == Some(false) {
        warnings.push("域名暂未解析，HTTPS 自动签证书会失败。".into());
    }
    if target.https_enabled && target.domain.trim().is_empty() {
        warnings.push("已启用 HTTPS，但未填写域名，证书签发阶段不会生成。".into());
    }
    if environment.port_available == Some(false) {
        warnings.push("目标端口已被监听，执行前需要调整端口或停止占用进程。".into());
    }
    warnings
}

fn should_upload_source_locally(target: &DeploymentTarget) -> bool {
    target.recipe != "image-store" && target.source_type == "local"
        || (target.source_type == "git" && !target.git_credential_key.trim().is_empty())
}

async fn upload_source_release(
    db: &Database,
    target: &DeploymentTarget,
    release_id: &str,
) -> Result<String, AppError> {
    let (source_dir, source_label) = if target.source_type == "git" {
        let (checkout_dir, commit, warnings) = checkout_git_source(
            db,
            &DetectDeploymentProjectInput {
                source_type: "git".into(),
                project_path: None,
                git_url: Some(target.git_url.clone()),
                git_ref: if target.git_ref.trim().is_empty() {
                    None
                } else {
                    Some(target.git_ref.clone())
                },
                git_credential_key: Some(target.git_credential_key.clone()),
            },
        )?;
        let warning_text = if warnings.is_empty() {
            String::new()
        } else {
            format!("；{}", warnings.join("；"))
        };
        (
            checkout_dir,
            format!("Git checkout {}{}", commit, warning_text),
        )
    } else {
        let project_path = PathBuf::from(target.project_path.trim());
        if !project_path.is_dir() {
            return Err(AppError::InvalidInput(format!(
                "本地项目目录不存在或不可读取: {}",
                target.project_path
            )));
        }
        (project_path, "本地项目目录".into())
    };

    let archive_path = temp_deploy_file(&format!(
        "{}-{}-source.tar.gz",
        target.target_key, release_id
    ))?;
    create_source_archive(&source_dir, &archive_path)?;
    let remote_archive = format!(
        "{}/{}-source.tar.gz",
        releases_root(target),
        sanitize_path_segment(release_id)
    );
    SftpService::upload(
        db,
        SftpTransferPathInput {
            server_alias: target.server_alias.clone(),
            local_path: archive_path.to_string_lossy().to_string(),
            remote_path: remote_archive.clone(),
        },
    )?;
    let command = format!(
        "rm -rf {release} && mkdir -p {release} && tar -xzf {archive} -C {release} && rm -f {archive}",
        release = shell_single_quote(&release_root(target, release_id)),
        archive = shell_single_quote(&remote_archive)
    );
    let result = TerminalService::execute(
        db,
        TerminalCommandInput {
            server_alias: target.server_alias.clone(),
            command,
            timeout_secs: Some(120),
            initiated_by_ai: Some(false),
        },
    )
    .await?;
    let _ = fs::remove_file(&archive_path);
    if result.blocked || result.exit_status != 0 {
        return Err(AppError::Custom(format!(
            "远端解包项目失败: {}{}",
            trim_preview(&result.stderr),
            trim_preview(&result.stdout)
        )));
    }
    Ok(format!(
        "{}已归档上传并解包到 {}。",
        source_label,
        release_root(target, release_id)
    ))
}

async fn build_upload_local_docker_image(
    db: &Database,
    target: &DeploymentTarget,
    release_id: &str,
) -> Result<String, AppError> {
    if target.source_type != "local" {
        return Err(AppError::InvalidInput(
            "本地构建镜像上传模式需要本地项目目录；Git 仓库部署请使用远程构建。".into(),
        ));
    }
    let project_dir = PathBuf::from(target.project_path.trim());
    if !project_dir.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "本地 Docker 构建目录不存在: {}",
            project_dir.display()
        )));
    }

    let image = docker_image_ref(target, release_id);
    let mut build = Command::new("docker");
    build
        .arg("build")
        .arg("-t")
        .arg(&image)
        .arg(".")
        .current_dir(&project_dir);
    run_local_command(&mut build, "本地 docker build 失败")?;

    let image_archive =
        temp_deploy_file(&format!("{}-{}-image.tar", target.target_key, release_id))?;
    let mut save = Command::new("docker");
    save.arg("save").arg("-o").arg(&image_archive).arg(&image);
    run_local_command(&mut save, "本地 docker save 失败")?;

    let remote_archive = format!(
        "{}/{}-image.tar",
        release_root(target, release_id),
        sanitize_path_segment(&target.target_key)
    );
    SftpService::upload(
        db,
        SftpTransferPathInput {
            server_alias: target.server_alias.clone(),
            local_path: image_archive.to_string_lossy().to_string(),
            remote_path: remote_archive.clone(),
        },
    )?;
    let load_result = TerminalService::execute(
        db,
        TerminalCommandInput {
            server_alias: target.server_alias.clone(),
            command: format!(
                "docker load -i {archive} && rm -f {archive}",
                archive = shell_single_quote(&remote_archive)
            ),
            timeout_secs: Some(180),
            initiated_by_ai: Some(false),
        },
    )
    .await?;
    let _ = fs::remove_file(&image_archive);
    if load_result.blocked || load_result.exit_status != 0 {
        return Err(AppError::Custom(format!(
            "远端 docker load 失败: {}{}",
            trim_preview(&load_result.stderr),
            trim_preview(&load_result.stdout)
        )));
    }
    Ok(format!("本地镜像 {} 已构建、上传并在远端加载。", image))
}

fn create_source_archive(source_dir: &Path, archive_path: &Path) -> Result<(), AppError> {
    let mut archive = Command::new("tar");
    archive
        .arg("-czf")
        .arg(archive_path)
        .arg("-C")
        .arg(source_dir)
        .arg(".");
    run_local_command(&mut archive, "本地项目归档失败")
}

fn run_local_command(command: &mut Command, label: &str) -> Result<(), AppError> {
    let output = command
        .output()
        .map_err(|error| AppError::Custom(format!("{}: {}", label, error)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(AppError::Custom(format!("{}: {}{}", label, stderr, stdout)));
    }
    Ok(())
}

fn temp_deploy_file(file_name: &str) -> Result<PathBuf, AppError> {
    let dir = std::env::temp_dir().join("tauri-ssh-deploy-artifacts");
    fs::create_dir_all(&dir)?;
    Ok(dir.join(sanitize_path_segment(file_name)))
}

fn config_requests_service_account(config_json: &str) -> bool {
    serde_json::from_str::<Value>(config_json)
        .ok()
        .map(|value| service_account_config_enabled(&value))
        .unwrap_or(false)
}

fn service_account_config_enabled(value: &Value) -> bool {
    if value
        .get("enabled")
        .and_then(Value::as_bool)
        .is_some_and(|enabled| !enabled)
    {
        return false;
    }
    if json_string(value, "connectionKey")
        .or_else(|| json_string(value, "connection"))
        .is_some()
    {
        return true;
    }
    [
        "database",
        "databases",
        "redis",
        "redises",
        "serviceAccounts",
    ]
    .iter()
    .filter_map(|key| value.get(key))
    .any(|item| match item {
        Value::Array(items) => items.iter().any(service_account_config_enabled),
        Value::Object(_) => service_account_config_enabled(item),
        _ => false,
    })
}

#[derive(Debug, Clone)]
struct ServiceAccountSpec {
    kind: String,
    connection_key: String,
    database_name: String,
    username: String,
    credential_key: String,
}

struct ImageStoreRuntimeConfig {
    app_key: String,
    image: String,
    tag: String,
    compose: String,
}

fn image_store_config(config_json: &str) -> Option<ImageStoreRuntimeConfig> {
    let value: Value = serde_json::from_str(config_json).ok()?;
    let image_store = value.get("imageStore")?;
    Some(ImageStoreRuntimeConfig {
        app_key: json_string(image_store, "appKey").unwrap_or_default(),
        image: json_string(image_store, "image")?,
        tag: json_string(image_store, "tag").unwrap_or_else(|| "latest".into()),
        compose: json_string(image_store, "compose")?,
    })
}

async fn execute_service_accounts(
    db: &Database,
    target: &DeploymentTarget,
) -> Result<String, AppError> {
    let specs = parse_service_account_specs(target)?;
    if specs.is_empty() {
        return Ok("未声明数据库/Redis 专属账号配置，跳过。".into());
    }
    let mut messages = Vec::new();
    for spec in specs {
        let password = generate_account_password();
        match spec.kind.as_str() {
            "database" => {
                let connection = db
                    .get_database_connection(&spec.connection_key)?
                    .ok_or_else(|| {
                        AppError::NotFound(format!("数据库连接 '{}' 不存在", spec.connection_key))
                    })?;
                if !["mysql", "postgresql"].contains(&connection.db_type.as_str()) {
                    return Err(AppError::InvalidInput(format!(
                        "连接 '{}' 不是 MySQL/PostgreSQL",
                        spec.connection_key
                    )));
                }
                create_sql_service_account(db, &connection, &spec, &password).await?;
                DatabaseOpsService::upsert_connection(
                    db,
                    UpsertDatabaseConnectionInput {
                        key: spec.credential_key.clone(),
                        name: format!("{} 专属数据库账号", target.name),
                        group_name: "自动部署".into(),
                        db_type: connection.db_type.clone(),
                        connection_mode: connection.connection_mode.clone(),
                        host: connection.host.clone(),
                        port: connection.port,
                        database_name: spec.database_name.clone(),
                        username: spec.username.clone(),
                        auth_type: "direct_password".into(),
                        credential_ref: String::new(),
                        password: Some(password),
                        clear_password: Some(false),
                        ssh_server_alias: connection.ssh_server_alias.clone(),
                        security_mode: connection.security_mode.clone(),
                        ai_policy: connection.ai_policy.clone(),
                        page_size: connection.page_size,
                        status: Some("online".into()),
                        enabled: true,
                        notes: format!(
                            "自动部署目标 {} 创建的专属数据库账号，密码已加密保存。",
                            target.target_key
                        ),
                    },
                )?;
                messages.push(format!(
                    "已创建/登记数据库账号 {} -> {}",
                    spec.username, spec.credential_key
                ));
            }
            "redis" => {
                let connection = db
                    .get_database_connection(&spec.connection_key)?
                    .ok_or_else(|| {
                        AppError::NotFound(format!("Redis 连接 '{}' 不存在", spec.connection_key))
                    })?;
                if connection.db_type != "redis" {
                    return Err(AppError::InvalidInput(format!(
                        "连接 '{}' 不是 Redis",
                        spec.connection_key
                    )));
                }
                DatabaseOpsService::create_redis_acl_user(
                    db,
                    &spec.connection_key,
                    Some(spec.database_name.clone()),
                    &spec.username,
                    &password,
                )
                .await?;
                DatabaseOpsService::upsert_connection(
                    db,
                    UpsertDatabaseConnectionInput {
                        key: spec.credential_key.clone(),
                        name: format!("{} 专属 Redis 账号", target.name),
                        group_name: "自动部署".into(),
                        db_type: "redis".into(),
                        connection_mode: connection.connection_mode.clone(),
                        host: connection.host.clone(),
                        port: connection.port,
                        database_name: spec.database_name.clone(),
                        username: spec.username.clone(),
                        auth_type: "direct_password".into(),
                        credential_ref: String::new(),
                        password: Some(password),
                        clear_password: Some(false),
                        ssh_server_alias: connection.ssh_server_alias.clone(),
                        security_mode: connection.security_mode.clone(),
                        ai_policy: connection.ai_policy.clone(),
                        page_size: connection.page_size,
                        status: Some("online".into()),
                        enabled: true,
                        notes: format!(
                            "自动部署目标 {} 创建的专属 Redis ACL 账号，密码已加密保存。",
                            target.target_key
                        ),
                    },
                )?;
                messages.push(format!(
                    "已创建/登记 Redis ACL 账号 {} -> {}",
                    spec.username, spec.credential_key
                ));
            }
            _ => {}
        }
    }
    Ok(messages.join("\n"))
}

async fn create_sql_service_account(
    db: &Database,
    connection: &crate::models::DatabaseConnection,
    spec: &ServiceAccountSpec,
    password: &str,
) -> Result<(), AppError> {
    let statements = match connection.db_type.as_str() {
        "mysql" => vec![
            format!(
                "CREATE DATABASE IF NOT EXISTS {}",
                mysql_identifier(&spec.database_name)?
            ),
            format!(
                "CREATE USER IF NOT EXISTS {}@'%' IDENTIFIED BY {}",
                mysql_string(&spec.username),
                sql_string(password)
            ),
            format!(
                "ALTER USER {}@'%' IDENTIFIED BY {}",
                mysql_string(&spec.username),
                sql_string(password)
            ),
            format!(
                "GRANT ALL PRIVILEGES ON {}.* TO {}@'%'",
                mysql_identifier(&spec.database_name)?,
                mysql_string(&spec.username)
            ),
            "FLUSH PRIVILEGES".into(),
        ],
        "postgresql" => vec![
            format!("CREATE DATABASE {}", pg_identifier(&spec.database_name)?),
            format!(
                "DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = {user}) THEN CREATE ROLE {ident} LOGIN PASSWORD {password}; ELSE ALTER ROLE {ident} WITH LOGIN PASSWORD {password}; END IF; END $$",
                user = sql_string(&spec.username),
                ident = pg_identifier(&spec.username)?,
                password = sql_string(password)
            ),
            format!(
                "GRANT ALL PRIVILEGES ON DATABASE {} TO {}",
                pg_identifier(&spec.database_name)?,
                pg_identifier(&spec.username)?
            ),
        ],
        _ => return Err(AppError::InvalidInput("数据库类型无效".into())),
    };
    for statement in statements {
        match DatabaseOpsService::execute_sql(
            db,
            DatabaseQueryInput {
                connection_key: connection.key.clone(),
                database_name: None,
                sql: statement,
                page: Some(1),
                page_size: Some(1),
            },
        )
        .await
        {
            Ok(_) => {}
            Err(error)
                if connection.db_type == "postgresql"
                    && error.to_string().to_lowercase().contains("already exists") => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn parse_service_account_specs(
    target: &DeploymentTarget,
) -> Result<Vec<ServiceAccountSpec>, AppError> {
    let value: Value = serde_json::from_str(&target.config_json).map_err(|error| {
        AppError::InvalidInput(format!("扩展配置 JSON 无效，无法创建专属账号: {}", error))
    })?;
    let mut specs = Vec::new();
    collect_service_account_specs(&mut specs, target, "database", value.get("database"))?;
    collect_service_account_specs(&mut specs, target, "database", value.get("databases"))?;
    collect_service_account_specs(&mut specs, target, "redis", value.get("redis"))?;
    collect_service_account_specs(&mut specs, target, "redis", value.get("redises"))?;
    if let Some(service_accounts) = value.get("serviceAccounts") {
        collect_service_account_specs(
            &mut specs,
            target,
            "database",
            service_accounts
                .get("database")
                .or_else(|| service_accounts.get("databases")),
        )?;
        collect_service_account_specs(
            &mut specs,
            target,
            "redis",
            service_accounts
                .get("redis")
                .or_else(|| service_accounts.get("redises")),
        )?;
    }
    Ok(specs)
}

fn collect_service_account_specs(
    specs: &mut Vec<ServiceAccountSpec>,
    target: &DeploymentTarget,
    kind: &str,
    value: Option<&Value>,
) -> Result<(), AppError> {
    match value {
        Some(Value::Array(items)) => {
            for item in items {
                if service_account_item_disabled(item) {
                    continue;
                }
                specs.push(parse_service_account_spec(target, kind, item)?);
            }
        }
        Some(Value::Object(_)) => {
            let item = value.unwrap();
            if !service_account_item_disabled(item) {
                specs.push(parse_service_account_spec(target, kind, item)?)
            }
        }
        _ => {}
    }
    Ok(())
}

fn service_account_item_disabled(value: &Value) -> bool {
    value
        .get("enabled")
        .and_then(Value::as_bool)
        .is_some_and(|enabled| !enabled)
}

fn parse_service_account_spec(
    target: &DeploymentTarget,
    kind: &str,
    value: &Value,
) -> Result<ServiceAccountSpec, AppError> {
    let connection_key = json_string(value, "connectionKey")
        .or_else(|| json_string(value, "connection"))
        .ok_or_else(|| AppError::InvalidInput(format!("{} 专属账号缺少 connectionKey", kind)))?;
    let database_name = json_string(value, "databaseName")
        .or_else(|| json_string(value, "dbName"))
        .or_else(|| json_string(value, "database"))
        .or_else(|| json_string(value, "db"))
        .unwrap_or_else(|| {
            if kind == "redis" {
                "0".into()
            } else {
                target.target_key.clone()
            }
        });
    let username = json_string(value, "username")
        .or_else(|| json_string(value, "user"))
        .unwrap_or_else(|| sanitize_account_name(&format!("{}_{}", target.target_key, kind)));
    let credential_key = json_string(value, "credentialKey").unwrap_or_else(|| {
        format!(
            "{}_{}_{}",
            target.target_key,
            kind,
            sanitize_account_name(&database_name)
        )
    });
    Ok(ServiceAccountSpec {
        kind: kind.into(),
        connection_key,
        database_name,
        username,
        credential_key,
    })
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
}

fn generate_account_password() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

fn sanitize_account_name(value: &str) -> String {
    let mut output: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    output.truncate(48);
    if output.is_empty() {
        "app_user".into()
    } else {
        output
    }
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

fn yaml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn mysql_string(value: &str) -> String {
    sql_string(value)
}

fn mysql_identifier(value: &str) -> Result<String, AppError> {
    validate_identifier(value)?;
    Ok(format!("`{}`", value.replace('`', "``")))
}

fn pg_identifier(value: &str) -> Result<String, AppError> {
    validate_identifier(value)?;
    Ok(format!("\"{}\"", value.replace('"', "\"\"")))
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("数据库名/用户名不能为空".into()));
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(AppError::InvalidInput(format!(
            "数据库名/用户名 '{}' 只能包含字母、数字、下划线或中划线",
            trimmed
        )));
    }
    Ok(())
}

fn health_check_preview(target: &DeploymentTarget) -> &str {
    if !target.health_check_url.trim().is_empty() {
        "curl -fsS <health-check-url>"
    } else if target.port.is_some() {
        "tcp check <host>:<port>"
    } else {
        "manual health check"
    }
}

fn stage_timeout_secs(stage: &DeploymentPlanStage) -> u64 {
    match stage.key.as_str() {
        "build" => 120,
        "source" => 90,
        "validate_compose" => 60,
        "health_check" => 30,
        _ => 45,
    }
}

fn trim_preview(value: &str) -> String {
    const MAX: usize = 4000;
    let trimmed = value.trim();
    if trimmed.len() <= MAX {
        trimmed.to_string()
    } else {
        format!(
            "{}...\n[输出已截断，仅保留前 {} 字节]",
            &trimmed[..MAX],
            MAX
        )
    }
}

fn execution_command_for_stage(
    target: &DeploymentTarget,
    stage: &DeploymentPlanStage,
    release_id: &str,
) -> Option<String> {
    match stage.key.as_str() {
        "prepare" => Some(format!(
            "mkdir -p {releases} {release}",
            releases = shell_single_quote(&releases_root(target)),
            release = shell_single_quote(&release_root(target, release_id))
        )),
        "source" => source_command(target, release_id),
        "pull_image" => image_store_pull_command(target),
        "build" => build_command(target, release_id),
        "configure" => dockerfile_compose_command(target, release_id),
        "validate_compose" => Some(format!(
            "cd {} && docker compose config",
            shell_single_quote(&remote_workdir(target, release_id))
        )),
        "deploy" => deploy_command(target, release_id),
        "configure_web" => static_web_config_command(target, release_id),
        "https" => https_command(target),
        "rollback" => rollback_command(target),
        "service_accounts" => service_account_command(target),
        "health_check" => health_check_command(target),
        key if target.recipe == "custom-script" && key.starts_with("custom_") => {
            Some(stage.command_preview.clone())
        }
        _ => None,
    }
}

fn unique_run_id(prefix: &str) -> String {
    format!(
        "{}-{}",
        prefix,
        chrono::Local::now().format("%Y%m%d%H%M%S%3f")
    )
}

fn release_id_for(run_id: &str, target_key: &str) -> String {
    sanitize_path_segment(&format!("{}-{}", run_id, target_key))
}

fn rollback_release_marker() -> String {
    "rollback".into()
}

fn summarize_outcome(
    outcome: &ExecutionOutcome,
    success_summary: &str,
) -> (&'static str, String, bool) {
    if outcome.has_failed {
        ("failed", "部署执行失败，请查看步骤日志。".into(), true)
    } else if outcome.has_approval {
        (
            "approval_required",
            "已执行可自动步骤，后续步骤等待审批。".into(),
            false,
        )
    } else {
        ("success", success_summary.into(), true)
    }
}

fn prefixed_step_key(prefix: Option<&str>, key: &str) -> String {
    prefix
        .map(|value| format!("{}::{}", value, key))
        .unwrap_or_else(|| key.to_string())
}

fn prefixed_step_title(prefix: Option<&str>, title: &str) -> String {
    prefix
        .map(|value| format!("{} / {}", value, title))
        .unwrap_or_else(|| title.to_string())
}

fn step_target_key(
    step: &crate::models::DeploymentRunStep,
    run: &DeploymentRun,
) -> Result<String, AppError> {
    if let Some((target_key, _)) = step.step_key.split_once("::") {
        return Ok(target_key.to_string());
    }
    if !run.target_key.trim().is_empty() {
        return Ok(run.target_key.clone());
    }
    Err(AppError::InvalidInput(
        "无法从运行步骤中识别部署目标".into(),
    ))
}

fn releases_root(target: &DeploymentTarget) -> String {
    format!("{}/releases", target.deploy_root.trim_end_matches('/'))
}

fn current_root(target: &DeploymentTarget) -> String {
    format!("{}/current", target.deploy_root.trim_end_matches('/'))
}

fn release_root(target: &DeploymentTarget, release_id: &str) -> String {
    format!("{}/{}", releases_root(target), release_id)
}

fn remote_workdir(target: &DeploymentTarget, release_id: &str) -> String {
    let workdir = target.workdir.trim();
    let root = if release_id == "rollback" {
        current_root(target)
    } else {
        release_root(target, release_id)
    };
    if workdir.is_empty() || workdir == "." {
        root
    } else {
        format!(
            "{}/{}",
            root.trim_end_matches('/'),
            workdir.trim_start_matches('/')
        )
    }
}

fn source_command(target: &DeploymentTarget, release_id: &str) -> Option<String> {
    if target.recipe == "image-store" {
        return None;
    }
    if target.source_type == "git" {
        if !target.git_credential_key.trim().is_empty() {
            return Some(format!(
                "secure git checkout with credential ref {} && upload archive -> {}",
                shell_single_quote(&target.git_credential_key),
                shell_single_quote(&release_root(target, release_id))
            ));
        }
        let git_ref = if target.git_ref.trim().is_empty() {
            "main"
        } else {
            target.git_ref.trim()
        };
        if target.git_url.trim().is_empty() {
            return None;
        }
        let root = release_root(target, release_id);
        Some(format!(
            "rm -rf {root} && git clone --branch {git_ref} {url} {root}",
            root = shell_single_quote(&root),
            git_ref = shell_single_quote(git_ref),
            url = shell_single_quote(&target.git_url),
        ))
    } else {
        Some(format!(
            "archive {} && upload/extract -> {}",
            shell_single_quote(&target.project_path),
            shell_single_quote(&release_root(target, release_id))
        ))
    }
}

fn build_command(target: &DeploymentTarget, release_id: &str) -> Option<String> {
    let dir = shell_single_quote(&remote_workdir(target, release_id));
    match target.recipe.as_str() {
        "dockerfile-service" if target.docker_build_mode == "remote" => Some(format!(
            "cd {dir} && docker build -t {image}:{tag} .",
            dir = dir,
            image = target.target_key,
            tag = release_id
        )),
        "static-openresty" | "static-nginx" => Some(format!(
            "cd {dir} && if [ -f pnpm-lock.yaml ]; then pnpm install --frozen-lockfile && pnpm run build; elif [ -f yarn.lock ]; then yarn install --frozen-lockfile && yarn build; else npm install && npm run build; fi",
            dir = dir
        )),
        _ => None,
    }
}

fn image_store_pull_command(target: &DeploymentTarget) -> Option<String> {
    let config = image_store_config(&target.config_json)?;
    if config.app_key == "elk" {
        let images = [
            "docker.elastic.co/elasticsearch/elasticsearch",
            "docker.elastic.co/logstash/logstash",
            "docker.elastic.co/kibana/kibana",
        ]
        .into_iter()
        .map(|image| shell_single_quote(&format!("{}:{}", image, config.tag)))
        .map(|image| format!("docker pull {}", image))
        .collect::<Vec<_>>()
        .join(" && ");
        return Some(images);
    }
    Some(format!(
        "docker pull {}",
        shell_single_quote(&format!("{}:{}", config.image, config.tag))
    ))
}

fn dockerfile_compose_command(target: &DeploymentTarget, release_id: &str) -> Option<String> {
    if target.recipe == "image-store" {
        return image_store_compose_command(target, release_id);
    }
    if target.recipe != "dockerfile-service" {
        return None;
    }
    let port = target.port.unwrap_or(8080);
    let compose = format!(
        r#"services:
  {name}:
    image: {name}:{tag}
    container_name: {name}
    restart: unless-stopped
    ports:
      - "127.0.0.1:{port}:{port}"
"#,
        name = target.target_key,
        tag = release_id,
        port = port
    );
    Some(format!(
        "cat > {}/docker-compose.yml <<'EOF'\n{}EOF",
        shell_single_quote(&release_root(target, release_id)),
        compose
    ))
}

fn image_store_compose_command(target: &DeploymentTarget, release_id: &str) -> Option<String> {
    let config = image_store_config(&target.config_json)?;
    Some(format!(
        "cat > {}/docker-compose.yml <<'EOF'\n{}\nEOF",
        shell_single_quote(&release_root(target, release_id)),
        config.compose
    ))
}

fn deploy_command(target: &DeploymentTarget, release_id: &str) -> Option<String> {
    let release_root = release_root(target, release_id);
    let current_root = current_root(target);
    match target.recipe.as_str() {
        "dockerfile-service" | "image-store" => Some(format!(
            "ln -sfn {release} {current} && cd {current} && docker compose up -d",
            release = shell_single_quote(&release_root),
            current = shell_single_quote(&current_root)
        )),
        "1panel-app" | "docker-compose" => Some(format!(
            "ln -sfn {release} {current} && cd {workdir} && docker compose up -d",
            release = shell_single_quote(&release_root),
            current = shell_single_quote(&current_root),
            workdir = shell_single_quote(&remote_workdir(target, release_id))
        )),
        "node-pm2" => Some(format!(
            "ln -sfn {release} {current} && cd {workdir} && pm2 reload ecosystem.config.js --update-env",
            release = shell_single_quote(&release_root),
            current = shell_single_quote(&current_root),
            workdir = shell_single_quote(&remote_workdir(target, release_id))
        )),
        "systemd-binary" => Some(format!("systemctl restart {}", shell_single_quote(&target.target_key))),
        _ => None,
    }
}

fn static_web_config_command(target: &DeploymentTarget, release_id: &str) -> Option<String> {
    if target.domain.trim().is_empty() {
        return None;
    }
    let site_root = format!("{}/dist", current_root(target));
    let config = format!(
        r#"server {{
    listen 80;
    server_name {domain};
    root {site_root};
    index index.html;
    location / {{
        try_files $uri $uri/ /index.html;
    }}
}}
"#,
        domain = target.domain,
        site_root = site_root
    );
    Some(format!(
        "ln -sfn {release} {current} && cat > /etc/nginx/conf.d/{domain}.conf <<'EOF'\n{config}EOF\nnginx -t && nginx -s reload",
        release = shell_single_quote(&release_root(target, release_id)),
        current = shell_single_quote(&current_root(target)),
        domain = target.domain,
        config = config
    ))
}

fn rollback_command(target: &DeploymentTarget) -> Option<String> {
    let reload = match target.recipe.as_str() {
        "1panel-app" | "dockerfile-service" | "docker-compose" | "image-store" => {
            format!(
                "cd {} && docker compose up -d",
                shell_single_quote(&current_root(target))
            )
        }
        "static-openresty" | "static-nginx" => "nginx -t && nginx -s reload".into(),
        "node-pm2" => format!(
            "cd {} && pm2 reload ecosystem.config.js --update-env",
            shell_single_quote(&current_root(target))
        ),
        "systemd-binary" => format!(
            "systemctl restart {}",
            shell_single_quote(&target.target_key)
        ),
        _ => "true".into(),
    };
    Some(format!(
        r#"PREV=$(ls -1dt {releases}/* 2>/dev/null | grep -v "$(readlink -f {current} 2>/dev/null)" | head -n 1)
if [ -z "$PREV" ]; then echo "未找到可回滚 release"; exit 1; fi
ln -sfn "$PREV" {current}
{reload}"#,
        releases = shell_single_quote(&releases_root(target)),
        current = shell_single_quote(&current_root(target)),
        reload = reload
    ))
}

fn https_command(target: &DeploymentTarget) -> Option<String> {
    if target.domain.trim().is_empty() {
        return None;
    }
    let email = format!("admin@{}", target.domain);
    Some(format!(
        "certbot --nginx -d {} --non-interactive --agree-tos -m {} && nginx -s reload",
        shell_single_quote(&target.domain),
        shell_single_quote(&email)
    ))
}

fn service_account_command(target: &DeploymentTarget) -> Option<String> {
    if !config_requests_service_account(&target.config_json) {
        return None;
    }
    Some(format!(
        "echo {}",
        shell_single_quote("数据库/Redis 专属账号创建需要读取应用本地连接配置，当前执行阶段已创建审批记录，后续由数据库专用执行器处理。")
    ))
}

fn health_check_command(target: &DeploymentTarget) -> Option<String> {
    if !target.health_check_url.trim().is_empty() {
        Some(format!(
            "curl -fsS {}",
            shell_single_quote(&target.health_check_url)
        ))
    } else {
        target.port.map(|port| format!("nc -z 127.0.0.1 {}", port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_target(recipe: &str, config_json: &str) -> DeploymentTarget {
        DeploymentTarget {
            id: 1,
            target_key: "demo-app".into(),
            name: "Demo App".into(),
            server_alias: "demo-server".into(),
            recipe: recipe.into(),
            source_type: "local".into(),
            project_path: "/tmp/demo".into(),
            git_url: String::new(),
            git_ref: "main".into(),
            git_credential_key: String::new(),
            docker_build_mode: "remote".into(),
            workdir: ".".into(),
            deploy_root: "/opt/tauri-ssh/stacks/demo-app".into(),
            domain: String::new(),
            https_enabled: false,
            port: Some(8080),
            health_check_url: String::new(),
            config_json: config_json.into(),
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn test_environment() -> DeploymentEnvironmentProbe {
        DeploymentEnvironmentProbe {
            server_alias: "demo-server".into(),
            os: "Linux".into(),
            arch: "x86_64".into(),
            user: "root".into(),
            disk_available_kb: Some(1024 * 1024),
            docker_version: "Docker version 27".into(),
            compose_version: "Docker Compose version v2".into(),
            nginx_version: String::new(),
            openresty_version: String::new(),
            git_version: "git version 2".into(),
            port_available: Some(true),
            domain_resolved: None,
            checks: Vec::new(),
            raw_output: String::new(),
        }
    }

    #[test]
    fn environment_profiles_include_base_and_composed_profiles() {
        let template_keys: HashSet<String> = DeploymentService::list_templates()
            .into_iter()
            .map(|item| item.key)
            .collect();
        let profiles = DeploymentService::list_environment_profiles();
        let profile_keys: Vec<String> = profiles.iter().map(|item| item.key.clone()).collect();

        assert_eq!(
            profile_keys,
            vec![
                "1panel-app",
                "custom-script",
                "docker-compose",
                "node-pm2",
                "static-nginx",
                "static-openresty",
                "systemd-binary",
                "static-openresty-https",
                "springboot-mysql-redis",
                "compose-db-redis",
                "frontend-api-same-domain",
                "1panel-app-db",
            ]
        );
        for key in profile_keys.iter().take(7) {
            assert!(
                template_keys.contains(key.as_str()),
                "环境方案 {} 没有对应部署模板",
                key
            );
            assert!(
                validate_recipe(&key).is_ok(),
                "环境方案 {} 不能通过后端配方校验",
                key
            );
        }
        assert_eq!(
            profiles
                .iter()
                .filter(|item| item.category == "组合方案")
                .count(),
            5
        );
    }

    #[test]
    fn custom_script_generates_executable_stage_from_config_json() {
        let target = test_target(
            "custom-script",
            r#"{
              "deploymentProfile": "custom-script",
              "customStages": [
                {
                  "key": "restart app",
                  "title": "重启应用",
                  "command": "systemctl restart demo-app",
                  "risk": "high",
                  "approvalRequired": true,
                  "summary": "重启 systemd 服务"
                }
              ]
            }"#,
        );
        let stages = build_plan_stages(&target, &test_environment());
        let custom_stage = stages
            .iter()
            .find(|stage| stage.key == "custom_restart_app")
            .expect("应从 customStages 生成自定义阶段");

        assert_eq!(custom_stage.command_preview, "systemctl restart demo-app");
        assert!(custom_stage.approval_required);
        assert_eq!(
            execution_command_for_stage(&target, custom_stage, "release-1").as_deref(),
            Some("systemctl restart demo-app")
        );
    }

    #[test]
    fn https_stage_requires_domain() {
        let mut target = test_target(
            "static-openresty",
            r#"{"deploymentProfile":"static-openresty"}"#,
        );
        target.https_enabled = true;
        target.domain = String::new();

        let stages = build_plan_stages(&target, &test_environment());
        let warnings = build_plan_warnings(&target, &test_environment());

        assert!(
            stages.iter().all(|stage| stage.key != "https"),
            "未填写域名时不应生成不可执行的 HTTPS 签证书阶段"
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("未填写域名")),
            "启用 HTTPS 但未填写域名时应给出明确提示"
        );
    }

    #[test]
    fn disabled_service_account_template_does_not_request_execution() {
        let config_json = r#"{
          "deploymentProfile": "springboot-mysql-redis",
          "serviceAccounts": {
            "database": {
              "enabled": false,
              "connectionKey": "",
              "databaseName": "demo_app",
              "username": "demo_app"
            },
            "redis": {
              "enabled": false,
              "connectionKey": "",
              "databaseName": "0",
              "username": "demo_redis"
            }
          }
        }"#;

        assert!(!config_requests_service_account(config_json));
    }

    #[test]
    fn filled_service_account_config_requests_execution() {
        let config_json = r#"{
          "serviceAccounts": {
            "database": {
              "connectionKey": "mysql-main",
              "databaseName": "demo_app",
              "username": "demo_app"
            }
          }
        }"#;

        assert!(config_requests_service_account(config_json));
    }

    #[test]
    fn image_store_catalog_contains_common_apps() {
        let keys: HashSet<String> = DeploymentService::list_image_store_apps()
            .into_iter()
            .map(|item| item.key)
            .collect();

        for key in [
            "nginx",
            "mysql",
            "postgres",
            "redis",
            "portainer",
            "minio",
            "rocketmq-namesrv",
            "rocketmq-broker",
            "elasticsearch",
            "skywalking-oap",
            "skywalking-ui",
            "elk",
        ] {
            assert!(keys.contains(key), "镜像商店缺少 {}", key);
        }
    }

    #[test]
    fn image_store_compose_contains_image_port_and_volume() {
        let app = image_store_catalog()
            .into_iter()
            .find(|item| item.key == "mysql")
            .expect("mysql 应存在");
        let compose = image_store_compose_yaml(
            &app,
            "img-mysql",
            "8.4",
            Some(3306),
            "/opt/tauri-ssh/stacks/img-mysql",
            &[("MYSQL_ROOT_PASSWORD".into(), "secret".into())],
        );

        assert!(compose.contains("image: mysql:8.4"));
        assert!(compose.contains("\"127.0.0.1:3306:3306\""));
        assert!(compose.contains("/opt/tauri-ssh/stacks/img-mysql/data:/var/lib/mysql"));
        assert!(compose.contains("MYSQL_ROOT_PASSWORD: \"secret\""));
    }

    #[test]
    fn image_store_compose_supports_rocketmq_commands() {
        let app = image_store_catalog()
            .into_iter()
            .find(|item| item.key == "rocketmq-broker")
            .expect("rocketmq-broker 应存在");
        let compose = image_store_compose_yaml(
            &app,
            "img-rocketmq-broker",
            "5.3.0",
            Some(10911),
            "/opt/tauri-ssh/stacks/img-rocketmq-broker",
            &[("NAMESRV_ADDR".into(), "127.0.0.1:9876".into())],
        );

        assert!(compose.contains("image: apache/rocketmq:5.3.0"));
        assert!(compose.contains("command: sh mqbroker -n 127.0.0.1:9876"));
    }

    #[test]
    fn image_store_compose_supports_elk_stack() {
        let app = image_store_catalog()
            .into_iter()
            .find(|item| item.key == "elk")
            .expect("elk 应存在");
        let compose = image_store_compose_yaml(
            &app,
            "img-elk",
            "8.15.3",
            Some(5601),
            "/opt/tauri-ssh/stacks/img-elk",
            &[("ES_JAVA_OPTS".into(), "-Xms512m -Xmx512m".into())],
        );

        assert!(compose.contains("img-elk-elasticsearch"));
        assert!(compose.contains("docker.elastic.co/logstash/logstash:8.15.3"));
        assert!(compose.contains("docker.elastic.co/kibana/kibana:8.15.3"));
        assert!(compose.contains("\"127.0.0.1:5601:5601\""));
    }
}
