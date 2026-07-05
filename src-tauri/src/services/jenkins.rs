use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    mpsc::{self, Sender},
    Mutex, OnceLock,
};
use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;
use reqwest::header::{HeaderName, HeaderValue, ACCEPT, CONTENT_LENGTH, LOCATION, USER_AGENT};
use reqwest::multipart;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use ssh2::Session;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    AiProviderAskInput, ApprovalRequest, CleanupJenkinsArtifactInput, CreateApprovalRequestInput,
    CreateAuditLogInput, CreateJenkinsArtifactDeploymentCandidateInput,
    CreateJenkinsBuildDeploymentDryRunInput, DeleteJenkinsParameterTemplateInput,
    DeploymentCandidate, DeploymentPlan, DeploymentTarget, DownloadJenkinsArtifactInput,
    ExecuteJenkinsBuildApprovedInput, ExecuteJenkinsBuildStopApprovedInput,
    ForgetJenkinsRecentParameterValueInput, GenerateJenkinsFailureAnalysisInput,
    GetJenkinsBuildInput, GetJenkinsJobDetailInput, JenkinsArtifact, JenkinsBuild,
    JenkinsBuildAnalysis, JenkinsBuildLogInput, JenkinsBuildLogResult, JenkinsBuildStatusEvent,
    JenkinsBuildStopResult, JenkinsBuildTriggerResult, JenkinsConnection,
    JenkinsConnectionTestResult, JenkinsFileParameterMetadata, JenkinsJob, JenkinsJobDetail,
    JenkinsParameterDefinition, JenkinsParameterDefinitionsResult, JenkinsParameterTemplate,
    JenkinsQueueItem, JenkinsRecentParameterValue, ListJenkinsArtifactsInput,
    ListJenkinsBuildsInput, ListJenkinsConnectionsInput, ListJenkinsJobsInput,
    ListJenkinsParameterTemplatesInput, ListJenkinsParametersInput,
    ListJenkinsRecentParameterValuesInput, PollJenkinsQueueItemInput,
    RecordJenkinsLogCopyAuditInput, SecureCredential, SetJenkinsJobFavoriteInput,
    StopJenkinsBuildInput, TriggerJenkinsBuildInput, UpsertJenkinsConnectionInput,
    UpsertJenkinsParameterTemplateInput,
};
use crate::services::ai_provider::AiProviderService;
use crate::services::approval::ApprovalService;
use crate::services::audit::AuditService;
use crate::services::deployment::DeploymentService;
use crate::services::secure_credential::SecureCredentialService;
use crate::services::terminal::TerminalService;

pub struct JenkinsService;

const JENKINS_ARTIFACT_MAX_BYTES: i64 = 500 * 1024 * 1024;
const JENKINS_PARAMETER_CACHE_TTL_SECS: i64 = 60;
const JENKINS_CRUMB_CACHE_TTL_SECS: u64 = 30 * 60;
const JENKINS_QUEUE_TIMEOUT_SECS: i64 = 10 * 60;
static JENKINS_SENT_NOTIFICATIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static JENKINS_PARAMETER_CACHE: OnceLock<Mutex<HashMap<String, JenkinsParameterCacheEntry>>> =
    OnceLock::new();
static JENKINS_CRUMB_CACHE: OnceLock<Mutex<HashMap<String, JenkinsCrumbCacheEntry>>> =
    OnceLock::new();

struct JenkinsProbeResult {
    version: String,
    capabilities: String,
    credential_display_name: String,
    username_masked: String,
}

#[derive(Clone)]
struct JenkinsParameterCacheEntry {
    parameters: Vec<JenkinsParameterDefinition>,
    parameter_definition_hash: String,
    inserted_at: Instant,
    cached_at: String,
    expires_at: String,
}

struct JenkinsRequestTarget {
    url: String,
    _tunnel: Option<SshTunnelGuard>,
}

#[derive(Debug, Clone)]
struct JenkinsCrumbCacheEntry {
    field: String,
    value: String,
    inserted_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JenkinsRiskRules {
    version: i64,
    fallback_risk: String,
    file_parameter_risk: String,
    environment_risk: String,
    concurrency: JenkinsConcurrencyRules,
    job_rules: Vec<JenkinsPatternRiskRule>,
    parameter_rules: Vec<JenkinsParameterRiskRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JenkinsConcurrencyRules {
    allow_concurrent_builds: bool,
    allow_concurrent_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JenkinsPatternRiskRule {
    pattern: String,
    risk: String,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JenkinsParameterRiskRule {
    name: String,
    value: String,
    risk: String,
    enabled: bool,
}

struct JenkinsStreamResponse {
    response: reqwest::Response,
    _tunnel: Option<SshTunnelGuard>,
}

#[derive(Debug)]
struct JenkinsBuildApprovalContext {
    approval_id: i64,
    request_hash: String,
    connection: JenkinsConnection,
    job_full_name: String,
    parameter_definition_hash: String,
    parameters_json: Value,
    risk_level: String,
    requester: String,
}

#[derive(Debug)]
struct JenkinsBuildStopApprovalContext {
    approval_id: i64,
    request_hash: String,
    connection: JenkinsConnection,
    job_full_name: String,
    build_number: i64,
    risk_level: String,
    requester: String,
}

struct JenkinsBuildTracker;

#[derive(Debug, Clone, PartialEq, Eq)]
enum JenkinsBuildParameter {
    Scalar {
        name: String,
        value: String,
    },
    File {
        name: String,
        file_name: String,
        path: PathBuf,
    },
}

struct SshTunnelGuard {
    shutdown_tx: Sender<()>,
    local_port: u16,
}

impl Drop for SshTunnelGuard {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());
        let _ = TcpStream::connect(("127.0.0.1", self.local_port))
            .and_then(|stream| stream.shutdown(Shutdown::Both));
    }
}

impl Default for JenkinsRiskRules {
    fn default() -> Self {
        Self {
            version: 1,
            fallback_risk: "L2".into(),
            file_parameter_risk: "L3".into(),
            environment_risk: "auto".into(),
            concurrency: JenkinsConcurrencyRules {
                allow_concurrent_builds: false,
                allow_concurrent_patterns: Vec::new(),
            },
            job_rules: vec![
                JenkinsPatternRiskRule {
                    pattern: ".*(prod|production|release).*".into(),
                    risk: "L3".into(),
                    enabled: true,
                },
                JenkinsPatternRiskRule {
                    pattern: ".*(dev|test).*".into(),
                    risk: "L2".into(),
                    enabled: true,
                },
            ],
            parameter_rules: vec![
                JenkinsParameterRiskRule {
                    name: "ENV".into(),
                    value: "prod".into(),
                    risk: "L3".into(),
                    enabled: true,
                },
                JenkinsParameterRiskRule {
                    name: "DEPLOY".into(),
                    value: "true".into(),
                    risk: "L3".into(),
                    enabled: true,
                },
            ],
        }
    }
}

impl JenkinsService {
    pub fn list_connections(
        db: &Database,
        input: ListJenkinsConnectionsInput,
    ) -> Result<Vec<JenkinsConnection>, AppError> {
        db.list_jenkins_connections(&input)
    }

    pub fn upsert_connection(
        db: &Database,
        mut input: UpsertJenkinsConnectionInput,
    ) -> Result<JenkinsConnection, AppError> {
        Self::validate_name(&input.name)?;
        let base_url = Self::normalize_base_url(&input.base_url)?;
        input.approval_policy = Some(Self::normalize_approval_policy(
            input.approval_policy.as_deref(),
        )?);
        input.risk_rules_json = Some(Self::normalize_risk_rules_json(
            input.risk_rules_json.as_deref(),
        )?);
        let connection_key = input
            .connection_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(Self::new_connection_key);
        let existing_connection = db.get_jenkins_connection(&connection_key)?;
        if input.enabled.unwrap_or(false) {
            let Some(existing) = existing_connection.as_ref() else {
                return Err(AppError::InvalidInput(
                    "新建 Jenkins 连接必须先保存并测试成功后才能启用".into(),
                ));
            };
            if existing.status != "ok" || existing.last_tested_at.is_none() {
                return Err(AppError::InvalidInput(
                    "Jenkins 连接启用前必须先测试成功".into(),
                ));
            }
            if existing.deleted_at.is_some() {
                return Err(AppError::InvalidInput(
                    "已删除 Jenkins 连接恢复后必须重新测试成功才能启用".into(),
                ));
            }
        }
        let status = if input
            .credential_key
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            "credential_missing"
        } else if input.enabled.unwrap_or(false)
            && existing_connection
                .as_ref()
                .map(|connection| connection.status.as_str() == "ok")
                .unwrap_or(false)
        {
            "ok"
        } else {
            "draft"
        };
        let connection =
            db.upsert_jenkins_connection(&input, &connection_key, &base_url, status)?;
        Self::audit_connection(
            db,
            "jenkins.connection.upsert",
            &connection,
            "保存 Jenkins 连接",
        )?;
        Ok(connection)
    }

    pub fn normalize_risk_rules_json(input: Option<&str>) -> Result<String, AppError> {
        let value = input.unwrap_or("").trim();
        let rules = if value.is_empty() || value == "{}" || value == "[]" {
            JenkinsRiskRules::default()
        } else {
            serde_json::from_str::<JenkinsRiskRules>(value)
                .map_err(|e| AppError::InvalidInput(format!("风险规则格式无效: {}", e)))?
        };
        Self::validate_risk_rules(&rules)?;
        serde_json::to_string(&rules)
            .map_err(|e| AppError::InvalidInput(format!("风险规则序列化失败: {}", e)))
    }

    #[allow(dead_code)]
    pub fn allow_concurrent_build_for_job(
        connection: &JenkinsConnection,
        job_full_name: &str,
    ) -> Result<bool, AppError> {
        let rules = Self::parse_risk_rules(&connection.risk_rules_json)?;
        if !rules.concurrency.allow_concurrent_builds {
            return Ok(false);
        }
        let job_full_name = job_full_name.trim();
        if job_full_name.is_empty() {
            return Ok(false);
        }
        if rules
            .concurrency
            .allow_concurrent_patterns
            .iter()
            .map(|pattern| pattern.trim())
            .filter(|pattern| !pattern.is_empty())
            .try_fold(false, |matched, pattern| {
                let regex = Regex::new(pattern).map_err(|e| {
                    AppError::InvalidInput(format!("并发白名单正则无效 '{}': {}", pattern, e))
                })?;
                Ok::<bool, AppError>(matched || regex.is_match(job_full_name))
            })?
        {
            return Ok(true);
        }
        Ok(false)
    }

    pub fn delete_connection(db: &Database, connection_key: &str) -> Result<(), AppError> {
        let connection = Self::require_connection(db, connection_key)?;
        if !db.delete_jenkins_connection(connection_key)? {
            return Err(AppError::NotFound("Jenkins 连接不存在或已删除".into()));
        }
        Self::audit_connection(
            db,
            "jenkins.connection.delete",
            &connection,
            "软删除 Jenkins 连接",
        )
    }

    pub fn restore_connection(
        db: &Database,
        connection_key: &str,
    ) -> Result<JenkinsConnection, AppError> {
        if !db.restore_jenkins_connection(connection_key)? {
            return Err(AppError::NotFound("Jenkins 连接不存在或未删除".into()));
        }
        let connection = Self::require_connection(db, connection_key)?;
        Self::audit_connection(
            db,
            "jenkins.connection.restore",
            &connection,
            "恢复 Jenkins 连接",
        )?;
        Ok(connection)
    }

    pub fn duplicate_connection(
        db: &Database,
        connection_key: &str,
    ) -> Result<JenkinsConnection, AppError> {
        let source = Self::require_connection(db, connection_key)?;
        let input = UpsertJenkinsConnectionInput {
            connection_key: None,
            name: format!("{} 副本", source.name),
            base_url: source.base_url.clone(),
            credential_key: None,
            credential_display_name: None,
            username_masked: None,
            ssh_server_alias: Some(source.ssh_server_alias.clone()),
            environment: Some(source.environment.clone()),
            environment_label: Some(source.environment_label.clone()),
            tls_verify: Some(source.tls_verify),
            default_view: Some(source.default_view.clone()),
            default_folder: Some(source.default_folder.clone()),
            allow_mcp_read: Some(source.allow_mcp_read),
            allow_mcp_write: Some(false),
            approval_policy: Some(source.approval_policy.clone()),
            parameter_prefill_enabled: Some(source.parameter_prefill_enabled),
            risk_rules_json: Some(source.risk_rules_json.clone()),
            notify_on_success: Some(source.notify_on_success),
            notify_on_failure: Some(source.notify_on_failure),
            notify_on_unstable: Some(source.notify_on_unstable),
            notify_on_aborted: Some(source.notify_on_aborted),
            description: Some(source.description.clone()),
            enabled: Some(false),
        };
        let copied = Self::upsert_connection(db, input)?;
        Self::audit_connection(
            db,
            "jenkins.connection.duplicate",
            &copied,
            "复制 Jenkins 连接配置",
        )?;
        Ok(copied)
    }

    pub async fn test_connection(
        db: &Database,
        connection_key: &str,
    ) -> Result<JenkinsConnectionTestResult, AppError> {
        let connection = Self::require_connection(db, connection_key)?;
        let started = Instant::now();
        let test_result = Self::probe_connection(db, &connection).await;
        let latency_ms = started.elapsed().as_millis() as i64;

        let (ok, status, version, capabilities, display_name, username_masked, code, message) =
            match test_result {
                Ok(result) => (
                    true,
                    "ok".to_string(),
                    result.version,
                    result.capabilities,
                    result.credential_display_name,
                    result.username_masked,
                    String::new(),
                    "Jenkins 连接测试成功".to_string(),
                ),
                Err(error) => {
                    let message = error.to_string();
                    let status = if connection.credential_key.trim().is_empty() {
                        "credential_missing"
                    } else if message.contains("401") || message.contains("403") {
                        "credential_failed"
                    } else {
                        "failed"
                    };
                    (
                        false,
                        status.to_string(),
                        String::new(),
                        "{}".to_string(),
                        String::new(),
                        String::new(),
                        Self::jenkins_error_code(&message).to_string(),
                        Self::redact_error_message(&message),
                    )
                }
            };

        db.update_jenkins_connection_test_result(
            connection_key,
            &status,
            &version,
            &capabilities,
            &display_name,
            &username_masked,
            &code,
            &message,
        )?;
        Self::audit_connection(
            db,
            "jenkins.connection.test",
            &connection,
            "测试 Jenkins 连接配置",
        )?;
        Ok(JenkinsConnectionTestResult {
            ok,
            connection_key: connection_key.to_string(),
            status,
            version,
            message,
            latency_ms,
        })
    }

    pub async fn list_jobs(
        db: &Database,
        input: ListJenkinsJobsInput,
    ) -> Result<Vec<JenkinsJob>, AppError> {
        let connection = Self::require_connection(db, &input.connection_key)?;
        let cached = db.list_jenkins_recent_jobs(&input.connection_key)?;
        let refresh = input.refresh.unwrap_or(false) || input.force_refresh.unwrap_or(false);
        let mut jobs = Self::fetch_jobs(db, &connection, &input).await?;
        let favorites = cached
            .iter()
            .filter(|job| job.favorite)
            .map(|job| job.job_full_name.clone())
            .collect::<HashSet<_>>();
        Self::apply_job_favorites(&mut jobs, &favorites);
        db.replace_jenkins_recent_jobs(&input.connection_key, &jobs)?;
        if jobs.is_empty() && !refresh {
            return Ok(cached);
        }
        Self::audit_read(
            db,
            "jenkins.jobs.list",
            &connection,
            "刷新 Jenkins Job 列表",
            json!({
                "connectionKey": connection.connection_key,
                "jobCount": jobs.len(),
                "viewName": input.view_name,
                "folder": input.folder,
                "fromCache": false
            }),
            None,
        )?;
        Ok(jobs)
    }

    pub async fn get_job_detail(
        db: &Database,
        input: GetJenkinsJobDetailInput,
    ) -> Result<JenkinsJobDetail, AppError> {
        if input.connection_key.trim().is_empty() || input.job_full_name.trim().is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        let jobs = Self::list_jobs(
            db,
            ListJenkinsJobsInput {
                connection_key: input.connection_key.clone(),
                view_name: None,
                folder: None,
                refresh: input.refresh,
                force_refresh: input.refresh,
                depth: Some(5),
            },
        )
        .await?;
        let job = Self::find_job_in_tree(&jobs, &input.job_full_name).cloned();
        let parameters = Self::list_parameters(
            db,
            ListJenkinsParametersInput {
                connection_key: input.connection_key.clone(),
                job_full_name: input.job_full_name.clone(),
                refresh: input.refresh,
            },
        )
        .await?;
        Ok(JenkinsJobDetail {
            connection_key: input.connection_key,
            job_full_name: input.job_full_name,
            job,
            parameters,
        })
    }

    pub fn set_job_favorite(
        db: &Database,
        input: SetJenkinsJobFavoriteInput,
    ) -> Result<bool, AppError> {
        let connection_key = input.connection_key.trim();
        let job_full_name = input.job_full_name.trim();
        if connection_key.is_empty() || job_full_name.is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        let connection = Self::require_connection(db, connection_key)?;
        let requester = Self::normalize_requester(input.requester.as_deref());
        let changed = db.set_jenkins_job_favorite(&input)?;
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: requester,
                source: "jenkins".into(),
                server_alias: connection.ssh_server_alias,
                action: "jenkins.job.favorite.set".into(),
                risk: "readonly".into(),
                result: if changed { "成功" } else { "未找到" }.into(),
                summary: format!(
                    "{} Jenkins Job 收藏：{}",
                    if input.favorite { "添加" } else { "取消" },
                    job_full_name
                ),
                detail_json: Some(
                    json!({
                        "connectionKey": connection_key,
                        "jobFullName": job_full_name,
                        "favorite": input.favorite,
                        "changed": changed
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        )?;
        Ok(changed)
    }

    pub async fn list_builds(
        db: &Database,
        input: ListJenkinsBuildsInput,
    ) -> Result<Vec<JenkinsBuild>, AppError> {
        let connection = Self::require_connection(db, &input.connection_key)?;
        if input
            .job_full_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        {
            let builds = Self::fetch_builds(db, &connection, &input).await?;
            let mut recorded_builds = Vec::with_capacity(builds.len());
            for build in builds {
                recorded_builds.push(JenkinsBuildTracker::record_observed_build(
                    db,
                    &connection,
                    &build,
                )?);
            }
            Self::audit_read(
                db,
                "jenkins.builds.list",
                &connection,
                "刷新 Jenkins 构建列表",
                json!({
                    "connectionKey": connection.connection_key,
                    "jobFullName": input.job_full_name,
                    "buildCount": recorded_builds.len(),
                    "limit": input.limit.unwrap_or(30).clamp(1, 100)
                }),
                None,
            )?;
            return Ok(recorded_builds);
        }
        db.list_jenkins_build_runs(&input)
    }

    pub async fn list_parameters(
        db: &Database,
        input: ListJenkinsParametersInput,
    ) -> Result<JenkinsParameterDefinitionsResult, AppError> {
        if input.connection_key.trim().is_empty() || input.job_full_name.trim().is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        let connection = Self::require_connection(db, &input.connection_key)?;
        let job_full_name = input.job_full_name.trim().to_string();
        let refresh = input.refresh.unwrap_or(false);
        let cache_key = Self::parameter_cache_key(&connection, &job_full_name);
        if !refresh {
            if let Some(result) =
                Self::get_cached_parameters(&cache_key, &connection, &job_full_name)?
            {
                Self::audit_read(
                    db,
                    "jenkins.parameters.list",
                    &connection,
                    "读取 Jenkins 参数定义",
                    json!({
                        "connectionKey": connection.connection_key,
                        "jobFullName": job_full_name,
                        "parameterCount": result.parameters.len(),
                        "parameterDefinitionHash": result.parameter_definition_hash,
                        "fromCache": true
                    }),
                    None,
                )?;
                return Ok(result);
            }
        }

        let parameters = Self::fetch_parameters(db, &connection, &job_full_name).await?;
        let result = Self::cache_parameters(&cache_key, &connection, &job_full_name, parameters)?;
        Self::audit_read(
            db,
            "jenkins.parameters.list",
            &connection,
            "读取 Jenkins 参数定义",
            json!({
                "connectionKey": connection.connection_key,
                "jobFullName": job_full_name,
                "parameterCount": result.parameters.len(),
                "parameterDefinitionHash": result.parameter_definition_hash,
                "fromCache": false
            }),
            None,
        )?;
        Ok(result)
    }

    pub async fn verify_parameter_definition_hash(
        db: &Database,
        connection_key: &str,
        job_full_name: &str,
        expected_hash: &str,
    ) -> Result<JenkinsParameterDefinitionsResult, AppError> {
        let expected_hash = expected_hash.trim();
        if expected_hash.is_empty() {
            return Err(AppError::InvalidInput(
                "parameterDefinitionHash 不能为空".into(),
            ));
        }
        let connection = Self::require_connection(db, connection_key)?;
        let job_full_name = job_full_name.trim();
        if job_full_name.is_empty() {
            return Err(AppError::InvalidInput("Job 名称不能为空".into()));
        }
        let parameters = Self::fetch_parameters(db, &connection, job_full_name).await?;
        let result = Self::build_parameter_result(&connection, job_full_name, parameters, false)?;
        if result.parameter_definition_hash != expected_hash {
            return Err(AppError::InvalidInput(
                "parameter_definition_changed_after_approval".into(),
            ));
        }
        Ok(result)
    }

    pub fn list_recent_parameter_values(
        db: &Database,
        input: ListJenkinsRecentParameterValuesInput,
    ) -> Result<Vec<JenkinsRecentParameterValue>, AppError> {
        let connection = Self::require_connection(db, &input.connection_key)?;
        let job_full_name = input.job_full_name.trim();
        if job_full_name.is_empty() {
            return Err(AppError::InvalidInput("Job 名称不能为空".into()));
        }
        if !connection.parameter_prefill_enabled {
            return Ok(Vec::new());
        }
        let requester = Self::normalize_requester(input.requester.as_deref());
        let values = db.list_jenkins_recent_parameter_values(
            &connection.connection_key,
            job_full_name,
            &requester,
        )?;
        Self::audit_read(
            db,
            "jenkins.parameters.recent.list",
            &connection,
            "读取 Jenkins 最近参数值",
            json!({
                "connectionKey": connection.connection_key,
                "jobFullName": job_full_name,
                "requester": requester,
                "count": values.len(),
                "prefillEnabled": true
            }),
            None,
        )?;
        Ok(values)
    }

    pub fn forget_recent_parameter_value(
        db: &Database,
        input: ForgetJenkinsRecentParameterValueInput,
    ) -> Result<bool, AppError> {
        let connection = Self::require_connection(db, &input.connection_key)?;
        let job_full_name = input.job_full_name.trim();
        let parameter_name = input.parameter_name.trim();
        if job_full_name.is_empty() || parameter_name.is_empty() {
            return Err(AppError::InvalidInput("Job 和参数名称不能为空".into()));
        }
        let requester = Self::normalize_requester(input.requester.as_deref());
        if requester == "__shared__" {
            return Err(AppError::InvalidInput(
                "共享最近参数值只能由管理员删除".into(),
            ));
        }
        let deleted = db.delete_jenkins_recent_parameter_value(
            &connection.connection_key,
            job_full_name,
            parameter_name,
            &requester,
        )?;
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: requester.clone(),
                source: "jenkins".into(),
                server_alias: connection.ssh_server_alias.clone(),
                action: "jenkins.parameters.recent.forget".into(),
                risk: "L1".into(),
                result: "成功".into(),
                summary: format!(
                    "忘记 Jenkins 最近参数值：{} / {}",
                    job_full_name, parameter_name
                ),
                detail_json: Some(
                    json!({
                        "connectionKey": connection.connection_key,
                        "jobFullName": job_full_name,
                        "parameterName": parameter_name,
                        "requester": requester,
                        "deleted": deleted
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        )?;
        Ok(deleted)
    }

    pub fn list_parameter_templates(
        db: &Database,
        input: ListJenkinsParameterTemplatesInput,
    ) -> Result<Vec<JenkinsParameterTemplate>, AppError> {
        let connection = Self::require_connection(db, &input.connection_key)?;
        let job_full_name = input.job_full_name.trim();
        if job_full_name.is_empty() {
            return Err(AppError::InvalidInput("Job 名称不能为空".into()));
        }
        let requester = Self::normalize_requester(input.requester.as_deref());
        let templates = db.list_jenkins_parameter_templates(
            &connection.connection_key,
            job_full_name,
            &requester,
        )?;
        Self::audit_read(
            db,
            "jenkins.parameters.templates.list",
            &connection,
            "读取 Jenkins 参数模板",
            json!({
                "connectionKey": connection.connection_key,
                "jobFullName": job_full_name,
                "requester": requester,
                "count": templates.len()
            }),
            None,
        )?;
        Ok(templates)
    }

    pub fn upsert_parameter_template(
        db: &Database,
        input: UpsertJenkinsParameterTemplateInput,
    ) -> Result<JenkinsParameterTemplate, AppError> {
        let connection = Self::require_connection(db, &input.connection_key)?;
        let job_full_name = input.job_full_name.trim();
        let name = input.name.trim();
        if job_full_name.is_empty() || name.is_empty() {
            return Err(AppError::InvalidInput("Job 和模板名称不能为空".into()));
        }
        Self::validate_parameter_template_payload(&input.parameters_json)?;
        let requester = Self::normalize_requester(input.requester.as_deref());
        let template_key = input
            .template_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "jenkins-template-{}-{}-{}-{}",
                    sanitize_key_segment(&connection.connection_key),
                    sanitize_key_segment(job_full_name),
                    sanitize_key_segment(&requester),
                    chrono::Utc::now().timestamp_millis()
                )
            });
        let template = JenkinsParameterTemplate {
            id: 0,
            template_key,
            connection_key: connection.connection_key.clone(),
            job_full_name: job_full_name.to_string(),
            name: name.to_string(),
            parameters_json: input.parameters_json,
            parameter_definition_hash: input
                .parameter_definition_hash
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
            created_by: requester.clone(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let saved = db.upsert_jenkins_parameter_template(&template)?;
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: requester,
                source: "jenkins".into(),
                server_alias: connection.ssh_server_alias.clone(),
                action: "jenkins.parameters.template.upsert".into(),
                risk: "L1".into(),
                result: "成功".into(),
                summary: format!("保存 Jenkins 参数模板：{} / {}", job_full_name, saved.name),
                detail_json: Some(
                    json!({
                        "connectionKey": connection.connection_key,
                        "jobFullName": job_full_name,
                        "templateKey": saved.template_key,
                        "name": saved.name,
                        "parameterDefinitionHash": saved.parameter_definition_hash,
                        "parameterCount": saved.parameters_json
                            .get("parameters")
                            .and_then(Value::as_array)
                            .map(|items| items.len())
                            .unwrap_or(0)
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        )?;
        Ok(saved)
    }

    pub fn delete_parameter_template(
        db: &Database,
        input: DeleteJenkinsParameterTemplateInput,
    ) -> Result<bool, AppError> {
        let template_key = input.template_key.trim();
        if template_key.is_empty() {
            return Err(AppError::InvalidInput("templateKey 不能为空".into()));
        }
        let requester = Self::normalize_requester(input.requester.as_deref());
        let deleted = db.delete_jenkins_parameter_template(template_key, &requester)?;
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: requester,
                source: "jenkins".into(),
                server_alias: String::new(),
                action: "jenkins.parameters.template.delete".into(),
                risk: "L1".into(),
                result: "成功".into(),
                summary: format!("删除 Jenkins 参数模板：{}", template_key),
                detail_json: Some(
                    json!({
                        "templateKey": template_key,
                        "deleted": deleted
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        )?;
        Ok(deleted)
    }

    pub fn inspect_file_parameter(
        input: crate::models::InspectJenkinsFileParameterInput,
    ) -> Result<JenkinsFileParameterMetadata, AppError> {
        let parameter_name = input.parameter_name.trim();
        if parameter_name.is_empty() {
            return Err(AppError::InvalidInput("File Parameter 名称不能为空".into()));
        }
        let local_path = input.local_path.trim();
        if local_path.is_empty() {
            return Err(AppError::InvalidInput("请选择或输入本地文件路径".into()));
        }
        let path = PathBuf::from(local_path);
        if !path.is_absolute() {
            return Err(AppError::InvalidInput(
                "File Parameter 本地路径必须是绝对路径".into(),
            ));
        }
        if !path.is_file() {
            return Err(AppError::InvalidInput(
                "File Parameter 路径必须指向一个本地文件".into(),
            ));
        }
        let metadata = std::fs::metadata(&path)?;
        let size_bytes = i64::try_from(metadata.len())
            .map_err(|_| AppError::InvalidInput("文件大小超过可处理范围".into()))?;
        let modified_at = metadata
            .modified()
            .ok()
            .map(chrono::DateTime::<chrono::Utc>::from)
            .map(|time| time.to_rfc3339());
        Ok(JenkinsFileParameterMetadata {
            parameter_name: parameter_name.to_string(),
            local_path: path.to_string_lossy().to_string(),
            file_name: path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default(),
            size_bytes,
            sha256: sha256_file(&path)?,
            modified_at,
        })
    }

    pub fn create_build_trigger_approval(
        db: &Database,
        input: TriggerJenkinsBuildInput,
    ) -> Result<ApprovalRequest, AppError> {
        let connection_key = input.connection_key.trim();
        let job_full_name = input.job_full_name.trim();
        let parameter_definition_hash = input.parameter_definition_hash.trim();
        let reason = input.reason.trim();
        if connection_key.is_empty() || job_full_name.is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if parameter_definition_hash.is_empty() {
            return Err(AppError::InvalidInput(
                "parameterDefinitionHash 不能为空".into(),
            ));
        }
        if reason.is_empty() {
            return Err(AppError::InvalidInput("构建审批理由不能为空".into()));
        }
        let connection = Self::require_connection(db, connection_key)?;
        Self::ensure_build_trigger_allowed(&connection)?;
        let parameters_json = Self::sanitize_parameter_approval_payload(&input.parameters_json)?;
        let concurrency_blocked = Self::job_has_unfinished_run(db, connection_key, job_full_name)?
            && !Self::allow_concurrent_build_for_job(&connection, job_full_name)?;
        let risk = Self::normalize_build_trigger_risk(
            &connection,
            job_full_name,
            input.risk_level.as_deref(),
            &parameters_json,
            concurrency_blocked,
        )?;
        if risk == "blocked" {
            return Err(AppError::InvalidInput(
                "当前风险规则阻断该 Jenkins 构建触发".into(),
            ));
        }
        let requester = input
            .requester
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("local-user");
        let created_at_bucket = Self::approval_created_at_bucket(chrono::Utc::now());
        let risk_flags = Self::build_risk_flags(
            &connection,
            job_full_name,
            &parameters_json,
            concurrency_blocked,
        );
        let controlled_payload = json!({
            "action": "jenkins_build_trigger",
            "connectionKey": connection.connection_key,
            "connectionConfigVersion": connection.config_version,
            "jobFullName": job_full_name,
            "parameterDefinitionHash": parameter_definition_hash,
            "parameters": parameters_json,
            "requester": requester,
            "reason": reason,
            "riskLevel": risk,
            "riskFlags": risk_flags,
            "createdAtBucket": created_at_bucket,
            "status": "approval_created"
        });
        let request_hash = Self::request_hash(&controlled_payload)?;
        let mut payload = controlled_payload;
        if let Value::Object(map) = &mut payload {
            map.insert("requestHash".into(), Value::String(request_hash.clone()));
        }
        let approval = ApprovalService::create(
            db,
            CreateApprovalRequestInput {
                source: "jenkins".into(),
                requester: requester.into(),
                server_alias: connection.ssh_server_alias.clone(),
                action: "jenkins_build_trigger".into(),
                risk: risk.clone(),
                command: request_hash,
                resource: format!("{}:{}", connection.connection_key, job_full_name),
                reason: reason.into(),
                summary: format!("触发 Jenkins Job '{}'（{}）", job_full_name, risk),
                payload_json: Some(serde_json::to_string(&payload)?),
                expires_at: None,
            },
        )?;
        Self::audit_connection(
            db,
            "jenkins.build.approval.create",
            &connection,
            "创建 Jenkins 构建触发审批",
        )?;
        Ok(approval)
    }

    pub async fn trigger_build_without_approval(
        db: &Database,
        input: TriggerJenkinsBuildInput,
    ) -> Result<JenkinsBuildTriggerResult, AppError> {
        let context = Self::build_direct_trigger_context(db, input)?;
        if let Err(error) = Self::verify_parameter_definition_hash(
            db,
            &context.connection.connection_key,
            &context.job_full_name,
            &context.parameter_definition_hash,
        )
        .await
        {
            let _ = Self::audit_build_trigger_execution(
                db,
                Some(&context.connection),
                context.approval_id,
                &context.request_hash,
                &context.job_full_name,
                "parameter_definition_verify",
                false,
                "失败",
                &error.to_string(),
                None,
            );
            return Err(error);
        }

        let build_parameters = if Self::parameter_payload_has_entries(&context.parameters_json) {
            match Self::collect_build_parameters(db, &context.parameters_json) {
                Ok(parameters) => parameters,
                Err(error) => {
                    let _ = Self::audit_build_trigger_execution(
                        db,
                        Some(&context.connection),
                        context.approval_id,
                        &context.request_hash,
                        &context.job_full_name,
                        "parameter_prepare",
                        false,
                        "失败",
                        &error.to_string(),
                        None,
                    );
                    return Err(error);
                }
            }
        } else {
            Vec::new()
        };

        let trigger_result = if build_parameters.is_empty() {
            Self::trigger_plain_build(db, &context.connection, &context.job_full_name).await
        } else {
            Self::trigger_parameterized_build(
                db,
                &context.connection,
                &context.job_full_name,
                &build_parameters,
            )
            .await
        };

        match trigger_result {
            Ok((queue_id, location)) => {
                let mut result = JenkinsBuildTriggerResult {
                    approval_id: 0,
                    request_hash: context.request_hash.clone(),
                    connection_key: context.connection.connection_key.clone(),
                    job_full_name: context.job_full_name.clone(),
                    queue_id,
                    location: location.clone(),
                    run_key: JenkinsBuildTracker::run_key(&context.request_hash),
                    build_number: None,
                    status: "queued".into(),
                };
                let tracked_run = JenkinsBuildTracker::record_triggered(db, &context, &result)?;
                let synced_run =
                    JenkinsBuildTracker::sync_queue_once(db, &context.connection, &tracked_run)
                        .await
                        .unwrap_or(tracked_run);
                result.run_key = synced_run.run_key.clone();
                result.build_number = synced_run.build_number;
                result.status = synced_run.status.clone();
                Self::record_recent_parameter_values(db, &context, &synced_run)?;
                Self::audit_build_trigger_execution(
                    db,
                    Some(&context.connection),
                    0,
                    &context.request_hash,
                    &context.job_full_name,
                    "jenkins_post",
                    true,
                    "成功",
                    if build_parameters.is_empty() {
                        "Jenkins 普通构建已按无需审批策略直接触发"
                    } else {
                        "Jenkins 参数构建已按无需审批策略直接触发"
                    },
                    Some(json!({
                        "queueId": result.queue_id,
                        "location": location,
                        "status": result.status,
                        "runKey": synced_run.run_key,
                        "buildNumber": synced_run.build_number,
                        "runStatus": synced_run.status,
                        "riskLevel": context.risk_level,
                        "approvalPolicy": "none",
                        "parameterized": !build_parameters.is_empty(),
                        "parameterNames": build_parameters.iter().map(|parameter| match parameter {
                            JenkinsBuildParameter::Scalar { name, .. } => name,
                            JenkinsBuildParameter::File { name, .. } => name,
                        }).collect::<Vec<_>>()
                    })),
                )?;
                Ok(result)
            }
            Err(error) => {
                let _ = Self::audit_build_trigger_execution(
                    db,
                    Some(&context.connection),
                    0,
                    &context.request_hash,
                    &context.job_full_name,
                    "jenkins_post",
                    true,
                    "失败",
                    &error.to_string(),
                    None,
                );
                Err(error)
            }
        }
    }

    pub async fn trigger_build_without_approval_with_event(
        app: &tauri::AppHandle,
        db: &Database,
        input: TriggerJenkinsBuildInput,
    ) -> Result<JenkinsBuildTriggerResult, AppError> {
        let result = Self::trigger_build_without_approval(db, input).await?;
        let _ = db
            .list_jenkins_build_runs(&ListJenkinsBuildsInput {
                connection_key: result.connection_key.clone(),
                job_full_name: Some(result.job_full_name.clone()),
                limit: Some(1),
                offset: Some(0),
                cursor: None,
            })
            .ok()
            .and_then(|runs| {
                runs.into_iter()
                    .find(|run| run.request_id == result.request_hash)
            })
            .map(|run| Self::emit_build_status_event(app, &run));
        Ok(result)
    }

    pub async fn execute_build_trigger_approved(
        db: &Database,
        input: ExecuteJenkinsBuildApprovedInput,
    ) -> Result<JenkinsBuildTriggerResult, AppError> {
        let context = match Self::validate_build_trigger_approval(db, &input) {
            Ok(context) => context,
            Err(error) => {
                let _ = Self::audit_build_trigger_execution(
                    db,
                    None,
                    input.approval_id,
                    input.request_hash.as_deref().unwrap_or_default(),
                    "",
                    "validation",
                    false,
                    "失败",
                    &error.to_string(),
                    None,
                );
                return Err(error);
            }
        };

        if let Err(error) = Self::verify_parameter_definition_hash(
            db,
            &context.connection.connection_key,
            &context.job_full_name,
            &context.parameter_definition_hash,
        )
        .await
        {
            let _ = Self::audit_build_trigger_execution(
                db,
                Some(&context.connection),
                context.approval_id,
                &context.request_hash,
                &context.job_full_name,
                "parameter_definition_verify",
                false,
                "失败",
                &error.to_string(),
                None,
            );
            return Err(error);
        }

        let build_parameters = if Self::parameter_payload_has_entries(&context.parameters_json) {
            match Self::collect_build_parameters(db, &context.parameters_json) {
                Ok(parameters) => parameters,
                Err(error) => {
                    let _ = Self::audit_build_trigger_execution(
                        db,
                        Some(&context.connection),
                        context.approval_id,
                        &context.request_hash,
                        &context.job_full_name,
                        "parameter_prepare",
                        false,
                        "失败",
                        &error.to_string(),
                        None,
                    );
                    return Err(error);
                }
            }
        } else {
            Vec::new()
        };

        let trigger_result = if build_parameters.is_empty() {
            Self::trigger_plain_build(db, &context.connection, &context.job_full_name).await
        } else {
            Self::trigger_parameterized_build(
                db,
                &context.connection,
                &context.job_full_name,
                &build_parameters,
            )
            .await
        };

        match trigger_result {
            Ok((queue_id, location)) => {
                let mut result = JenkinsBuildTriggerResult {
                    approval_id: context.approval_id,
                    request_hash: context.request_hash.clone(),
                    connection_key: context.connection.connection_key.clone(),
                    job_full_name: context.job_full_name.clone(),
                    queue_id,
                    location: location.clone(),
                    run_key: JenkinsBuildTracker::run_key(&context.request_hash),
                    build_number: None,
                    status: "queued".into(),
                };
                let tracked_run = JenkinsBuildTracker::record_triggered(db, &context, &result)?;
                let synced_run =
                    JenkinsBuildTracker::sync_queue_once(db, &context.connection, &tracked_run)
                        .await
                        .unwrap_or(tracked_run);
                result.run_key = synced_run.run_key.clone();
                result.build_number = synced_run.build_number;
                result.status = synced_run.status.clone();
                Self::record_recent_parameter_values(db, &context, &synced_run)?;
                Self::audit_build_trigger_execution(
                    db,
                    Some(&context.connection),
                    context.approval_id,
                    &context.request_hash,
                    &context.job_full_name,
                    "jenkins_post",
                    true,
                    "成功",
                    if build_parameters.is_empty() {
                        "Jenkins 普通构建已触发"
                    } else {
                        "Jenkins 参数构建已触发"
                    },
                    Some(json!({
                        "queueId": result.queue_id,
                        "location": location,
                        "status": result.status,
                        "runKey": synced_run.run_key,
                        "buildNumber": synced_run.build_number,
                        "runStatus": synced_run.status,
                        "riskLevel": context.risk_level,
                        "parameterized": !build_parameters.is_empty(),
                        "parameterNames": build_parameters.iter().map(|parameter| match parameter {
                            JenkinsBuildParameter::Scalar { name, .. } => name,
                            JenkinsBuildParameter::File { name, .. } => name,
                        }).collect::<Vec<_>>()
                    })),
                )?;
                Ok(result)
            }
            Err(error) => {
                let _ = Self::audit_build_trigger_execution(
                    db,
                    Some(&context.connection),
                    context.approval_id,
                    &context.request_hash,
                    &context.job_full_name,
                    "jenkins_post",
                    true,
                    "失败",
                    &error.to_string(),
                    None,
                );
                Err(error)
            }
        }
    }

    pub async fn execute_build_trigger_approved_with_event(
        app: &tauri::AppHandle,
        db: &Database,
        input: ExecuteJenkinsBuildApprovedInput,
    ) -> Result<JenkinsBuildTriggerResult, AppError> {
        let result = Self::execute_build_trigger_approved(db, input).await?;
        let _ = db
            .list_jenkins_build_runs(&ListJenkinsBuildsInput {
                connection_key: result.connection_key.clone(),
                job_full_name: Some(result.job_full_name.clone()),
                limit: Some(1),
                offset: Some(0),
                cursor: None,
            })
            .ok()
            .and_then(|runs| {
                runs.into_iter()
                    .find(|run| run.request_id == result.request_hash)
            })
            .map(|run| Self::emit_build_status_event(app, &run));
        Ok(result)
    }

    pub fn create_build_stop_approval(
        db: &Database,
        input: StopJenkinsBuildInput,
    ) -> Result<ApprovalRequest, AppError> {
        let connection_key = input.connection_key.trim();
        let job_full_name = input.job_full_name.trim();
        let reason = input.reason.trim();
        if connection_key.is_empty() || job_full_name.is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if input.build_number <= 0 {
            return Err(AppError::InvalidInput("buildNumber 必须大于 0".into()));
        }
        if reason.is_empty() {
            return Err(AppError::InvalidInput("停止构建审批理由不能为空".into()));
        }
        let connection = Self::require_connection(db, connection_key)?;
        Self::ensure_build_trigger_allowed(&connection)?;
        let risk = Self::normalize_build_stop_risk(
            &connection,
            job_full_name,
            input.risk_level.as_deref(),
        )?;
        let requester = input
            .requester
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("local-user");
        let created_at_bucket = Self::approval_created_at_bucket(chrono::Utc::now());
        let risk_flags = Self::build_stop_risk_flags(&connection, job_full_name);
        let controlled_payload = json!({
            "action": "jenkins_build_stop",
            "connectionKey": connection.connection_key,
            "connectionConfigVersion": connection.config_version,
            "jobFullName": job_full_name,
            "buildNumber": input.build_number,
            "requester": requester,
            "reason": reason,
            "riskLevel": risk,
            "riskFlags": risk_flags,
            "createdAtBucket": created_at_bucket,
            "status": "approval_created"
        });
        let request_hash = Self::request_hash(&controlled_payload)?;
        let mut payload = controlled_payload;
        if let Value::Object(map) = &mut payload {
            map.insert("requestHash".into(), Value::String(request_hash.clone()));
        }
        let approval = ApprovalService::create(
            db,
            CreateApprovalRequestInput {
                source: "jenkins".into(),
                requester: requester.into(),
                server_alias: connection.ssh_server_alias.clone(),
                action: "jenkins_build_stop".into(),
                risk: risk.clone(),
                command: request_hash,
                resource: format!(
                    "{}:{}#{}",
                    connection.connection_key, job_full_name, input.build_number
                ),
                reason: reason.into(),
                summary: format!(
                    "停止 Jenkins Job '{}' #{}（{}）",
                    job_full_name, input.build_number, risk
                ),
                payload_json: Some(serde_json::to_string(&payload)?),
                expires_at: None,
            },
        )?;
        Self::audit_connection(
            db,
            "jenkins.build.stop.approval.create",
            &connection,
            "创建 Jenkins 停止构建审批",
        )?;
        Ok(approval)
    }

    pub async fn stop_build_without_approval(
        db: &Database,
        input: StopJenkinsBuildInput,
    ) -> Result<JenkinsBuildStopResult, AppError> {
        let context = Self::build_direct_stop_context(db, input)?;
        let stop_result = Self::stop_build(
            db,
            &context.connection,
            &context.job_full_name,
            context.build_number,
        )
        .await;
        match stop_result {
            Ok(()) => {
                Self::mark_build_stop_requested(db, &context)?;
                Self::audit_build_stop_execution(
                    db,
                    Some(&context.connection),
                    0,
                    &context.request_hash,
                    &context.job_full_name,
                    context.build_number,
                    "jenkins_post",
                    "成功",
                    &format!(
                        "Jenkins 停止构建请求已按无需审批策略直接发送，风险等级 {}",
                        context.risk_level
                    ),
                )?;
                Ok(JenkinsBuildStopResult {
                    approval_id: 0,
                    request_hash: context.request_hash,
                    connection_key: context.connection.connection_key,
                    job_full_name: context.job_full_name,
                    build_number: context.build_number,
                    status: "stop_requested".into(),
                })
            }
            Err(error) => {
                let _ = Self::audit_build_stop_execution(
                    db,
                    Some(&context.connection),
                    0,
                    &context.request_hash,
                    &context.job_full_name,
                    context.build_number,
                    "jenkins_post",
                    "失败",
                    &error.to_string(),
                );
                Err(error)
            }
        }
    }

    pub async fn stop_build_without_approval_with_event(
        app: &tauri::AppHandle,
        db: &Database,
        input: StopJenkinsBuildInput,
    ) -> Result<JenkinsBuildStopResult, AppError> {
        let result = Self::stop_build_without_approval(db, input).await?;
        let _ = db
            .get_jenkins_build_run_by_number(
                &result.connection_key,
                &result.job_full_name,
                result.build_number,
            )
            .ok()
            .flatten()
            .map(|run| Self::emit_build_status_event(app, &run));
        Ok(result)
    }

    pub async fn execute_build_stop_approved(
        db: &Database,
        input: ExecuteJenkinsBuildStopApprovedInput,
    ) -> Result<JenkinsBuildStopResult, AppError> {
        let context = match Self::validate_build_stop_approval(db, &input) {
            Ok(context) => context,
            Err(error) => {
                let _ = Self::audit_build_stop_execution(
                    db,
                    None,
                    input.approval_id,
                    input.request_hash.as_deref().unwrap_or_default(),
                    "",
                    0,
                    "validation",
                    "失败",
                    &error.to_string(),
                );
                return Err(error);
            }
        };
        let stop_result = Self::stop_build(
            db,
            &context.connection,
            &context.job_full_name,
            context.build_number,
        )
        .await;
        match stop_result {
            Ok(()) => {
                Self::mark_build_stop_requested(db, &context)?;
                let success_message = format!(
                    "Jenkins 停止构建请求已发送，风险等级 {}",
                    context.risk_level
                );
                Self::audit_build_stop_execution(
                    db,
                    Some(&context.connection),
                    context.approval_id,
                    &context.request_hash,
                    &context.job_full_name,
                    context.build_number,
                    "jenkins_post",
                    "成功",
                    &success_message,
                )?;
                Ok(JenkinsBuildStopResult {
                    approval_id: context.approval_id,
                    request_hash: context.request_hash,
                    connection_key: context.connection.connection_key,
                    job_full_name: context.job_full_name,
                    build_number: context.build_number,
                    status: "stop_requested".into(),
                })
            }
            Err(error) => {
                let _ = Self::audit_build_stop_execution(
                    db,
                    Some(&context.connection),
                    context.approval_id,
                    &context.request_hash,
                    &context.job_full_name,
                    context.build_number,
                    "jenkins_post",
                    "失败",
                    &error.to_string(),
                );
                Err(error)
            }
        }
    }

    pub async fn execute_build_stop_approved_with_event(
        app: &tauri::AppHandle,
        db: &Database,
        input: ExecuteJenkinsBuildStopApprovedInput,
    ) -> Result<JenkinsBuildStopResult, AppError> {
        let result = Self::execute_build_stop_approved(db, input).await?;
        let _ = db
            .get_jenkins_build_run_by_number(
                &result.connection_key,
                &result.job_full_name,
                result.build_number,
            )
            .ok()
            .flatten()
            .map(|run| Self::emit_build_status_event(app, &run));
        Ok(result)
    }

    pub async fn get_build_detail(
        db: &Database,
        input: GetJenkinsBuildInput,
    ) -> Result<JenkinsBuild, AppError> {
        let connection = Self::require_connection(db, &input.connection_key)?;
        let build =
            Self::fetch_build_detail(db, &connection, &input.job_full_name, input.build_number)
                .await?;
        let build = JenkinsBuildTracker::record_observed_build(db, &connection, &build)?;
        Self::audit_read(
            db,
            "jenkins.build.detail",
            &connection,
            "读取 Jenkins 构建详情",
            json!({
                "connectionKey": connection.connection_key,
                "jobFullName": input.job_full_name,
                "buildNumber": input.build_number,
                "status": build.status,
                "result": build.result
            }),
            None,
        )?;
        Ok(build)
    }

    pub async fn get_build_detail_with_event(
        app: &tauri::AppHandle,
        db: &Database,
        input: GetJenkinsBuildInput,
    ) -> Result<JenkinsBuild, AppError> {
        let build = Self::get_build_detail_with_notification(app, db, input).await?;
        Self::emit_build_status_event(app, &build);
        Ok(build)
    }

    pub async fn get_build_detail_with_notification(
        app: &tauri::AppHandle,
        db: &Database,
        input: GetJenkinsBuildInput,
    ) -> Result<JenkinsBuild, AppError> {
        let connection_key = input.connection_key.clone();
        let build = Self::get_build_detail(db, input).await?;
        let connection = Self::require_connection(db, &connection_key)?;
        Self::maybe_notify_build_completed(app, &connection, &build);
        Ok(build)
    }

    pub async fn recover_unfinished_runs_on_startup(
        app: &tauri::AppHandle,
        db: &Database,
    ) -> Result<Vec<JenkinsBuild>, AppError> {
        let runs = db.list_unfinished_jenkins_build_runs(100)?;
        if runs.is_empty() {
            log::info!("Jenkins 启动恢复：未发现未完成构建 run");
            return Ok(Vec::new());
        }

        log::info!("Jenkins 启动恢复：准备同步 {} 条未完成 run", runs.len());
        let mut recovered = Vec::with_capacity(runs.len());
        for run in runs {
            let synced = match Self::recover_unfinished_run_once(db, &run).await {
                Ok(build) => build,
                Err(error) => {
                    log::warn!(
                        "Jenkins 启动恢复失败: runKey={}, connectionKey={}, job={}, error={}",
                        run.run_key,
                        run.connection_key,
                        run.job_full_name,
                        error
                    );
                    JenkinsBuildTracker::mark_sync_failed(db, &run, "startup_recovery", &error)?
                }
            };
            Self::emit_build_status_event(app, &synced);
            recovered.push(synced);
        }
        Ok(recovered)
    }

    pub async fn sync_unfinished_runs_for_connection(
        app: &tauri::AppHandle,
        db: &Database,
        connection_key: &str,
    ) -> Result<Vec<JenkinsBuild>, AppError> {
        let connection_key = connection_key.trim();
        if connection_key.is_empty() {
            return Err(AppError::InvalidInput("connectionKey 不能为空".into()));
        }
        let runs = db.list_unfinished_jenkins_build_runs(100)?;
        let mut synced_runs = Vec::new();
        for run in runs
            .into_iter()
            .filter(|run| run.connection_key == connection_key)
        {
            let synced = match Self::recover_unfinished_run_once(db, &run).await {
                Ok(build) => build,
                Err(error) => {
                    log::warn!(
                        "Jenkins 未完成 run 同步失败: runKey={}, connectionKey={}, job={}, error={}",
                        run.run_key,
                        run.connection_key,
                        run.job_full_name,
                        error
                    );
                    JenkinsBuildTracker::mark_sync_failed(db, &run, "runtime_sync", &error)?
                }
            };
            Self::emit_build_status_event(app, &synced);
            synced_runs.push(synced);
        }
        Ok(synced_runs)
    }

    async fn recover_unfinished_run_once(
        db: &Database,
        run: &JenkinsBuild,
    ) -> Result<JenkinsBuild, AppError> {
        let connection = Self::require_connection(db, &run.connection_key)?;
        if !connection.enabled {
            return Err(AppError::InvalidInput(
                "Jenkins 连接未启用，启动恢复不重试".into(),
            ));
        }
        if run.build_number.is_some() {
            return JenkinsBuildTracker::sync_build_detail_once(db, &connection, run).await;
        }
        if !run.queue_id.trim().is_empty() {
            return JenkinsBuildTracker::sync_queue_once(db, &connection, run).await;
        }
        Err(AppError::InvalidInput(
            "未完成 run 缺少 queueId 和 buildNumber，无法恢复同步".into(),
        ))
    }

    pub async fn read_build_log(
        db: &Database,
        input: JenkinsBuildLogInput,
    ) -> Result<JenkinsBuildLogResult, AppError> {
        if input.connection_key.trim().is_empty() || input.job_full_name.trim().is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if input.build_number <= 0 {
            return Err(AppError::InvalidInput("构建号必须大于 0".into()));
        }
        let connection = Self::require_connection(db, &input.connection_key)?;
        let start = input.start.unwrap_or(0).max(0);
        let url =
            Self::build_progressive_log_url(&connection, &input.job_full_name, input.build_number);
        let (text, headers) =
            Self::jenkins_get_text(db, &connection, &url, &[("start", start.to_string())]).await?;
        let next_start = headers
            .get("X-Text-Size")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(start + text.len() as i64);
        let has_more = headers
            .get("X-More-Data")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let request_id = input
            .request_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(Self::new_request_id);
        let mut returned_text = text;
        let mut returned_start = start;
        if let Some(tail_bytes) = input.tail_bytes {
            let tail_bytes = tail_bytes.clamp(1024, 1024 * 1024) as usize;
            if returned_text.len() > tail_bytes {
                let mut trim_start = returned_text.len().saturating_sub(tail_bytes);
                while trim_start < returned_text.len()
                    && !returned_text.is_char_boundary(trim_start)
                {
                    trim_start += 1;
                }
                returned_text = returned_text[trim_start..].to_string();
                returned_start = next_start.saturating_sub(returned_text.len() as i64);
            }
        }
        Self::audit_log_session(
            db,
            &connection,
            &request_id,
            &input,
            returned_start,
            next_start,
            returned_text.len() as i64,
            has_more,
        )?;
        Ok(JenkinsBuildLogResult {
            request_id,
            text: Self::redact_log_text(&returned_text),
            start: returned_start,
            next_start,
            has_more,
            redacted: true,
            message: "日志读取成功，已按安全规则脱敏".into(),
        })
    }

    pub fn record_log_copy_audit(
        db: &Database,
        input: RecordJenkinsLogCopyAuditInput,
    ) -> Result<(), AppError> {
        if input.connection_key.trim().is_empty() || input.job_full_name.trim().is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if input.build_number <= 0 {
            return Err(AppError::InvalidInput("构建号必须大于 0".into()));
        }
        if input.end_offset < input.start_offset {
            return Err(AppError::InvalidInput(
                "日志复制结束 offset 不能小于开始 offset".into(),
            ));
        }
        let connection = Self::require_connection(db, &input.connection_key)?;
        let request_id = input
            .request_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(Self::new_request_id);
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "jenkins".into(),
                server_alias: connection.ssh_server_alias.clone(),
                action: "jenkins.build.log.copy".into(),
                risk: "readonly".into(),
                result: "成功".into(),
                summary: format!(
                    "复制 Jenkins 构建日志片段：{} #{}",
                    input.job_full_name, input.build_number
                ),
                detail_json: Some(
                    json!({
                        "connectionKey": connection.connection_key,
                        "jobFullName": input.job_full_name,
                        "buildNumber": input.build_number,
                        "startOffset": input.start_offset,
                        "endOffset": input.end_offset,
                        "bytes": input.bytes.max(0),
                        "redacted": input.redacted,
                        "rawLogAccess": input.raw_log_access,
                        "confirmationSource": input.confirmation_source.unwrap_or_else(|| "ui-loaded-log".into()),
                        "contentStored": false
                    })
                    .to_string(),
                ),
                request_id: Some(request_id),
                approval_id: None,
            },
        )?;
        Ok(())
    }

    pub async fn generate_failure_analysis(
        db: &Database,
        input: GenerateJenkinsFailureAnalysisInput,
    ) -> Result<JenkinsBuildAnalysis, AppError> {
        let connection_key = input.connection_key.trim();
        let job_full_name = input.job_full_name.trim();
        let snippet = input.log_snippet.trim();
        if connection_key.is_empty() || job_full_name.is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if input.build_number <= 0 {
            return Err(AppError::InvalidInput("buildNumber 必须是正整数".into()));
        }
        if snippet.is_empty() {
            return Err(AppError::InvalidInput("失败日志片段不能为空".into()));
        }
        if input.snippet_start_line <= 0 || input.snippet_end_line < input.snippet_start_line {
            return Err(AppError::InvalidInput("失败日志片段行号范围无效".into()));
        }
        let connection = Self::require_connection(db, connection_key)?;
        let snippet_sha256 = format!("{:x}", Sha256::digest(snippet.as_bytes()));
        let requester = input
            .requester
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("local-user");
        let prompt = format!(
            "请基于以下 Jenkins 构建失败日志脱敏片段生成排障总结。\
             \n要求：1. 使用中文；2. 不还原或猜测任何凭据、Token、密码；3. 只依据片段内容；\
             \n4. 输出包含：失败现象、最可能原因、关键证据、建议处理步骤。\
             \n\n连接: {connection_key}\nJob: {job_full_name}\nBuild: #{}\n日志行范围: {}-{}\n命中行数: {}\n片段 SHA-256: {}\n\n脱敏日志片段:\n```text\n{}\n```",
            input.build_number,
            input.snippet_start_line,
            input.snippet_end_line,
            input.matched_lines.max(0),
            snippet_sha256,
            snippet
        );
        let answer = AiProviderService::ask(
            db,
            AiProviderAskInput {
                prompt,
                provider_key: input.provider_key.clone(),
                system_prompt: Some(
                    "你是 Jenkins 构建失败排障助手。只能使用用户提供的脱敏日志片段进行分析，不得输出或推断敏感信息。"
                        .into(),
                ),
                skill_scope: None,
                use_skill_trigger: Some(false),
            },
        )
        .await?;
        let hash_prefix: String = snippet_sha256.chars().take(16).collect();
        let analysis = JenkinsBuildAnalysis {
            id: 0,
            analysis_key: format!(
                "jenkins-analysis-{}-{}-{}",
                input.build_number,
                chrono::Utc::now().timestamp_millis(),
                hash_prefix
            ),
            run_key: input.run_key.unwrap_or_default(),
            request_id: input.request_id.unwrap_or_default(),
            connection_key: connection.connection_key.clone(),
            job_full_name: job_full_name.into(),
            build_number: input.build_number,
            provider_key: answer.provider_key,
            provider_name: answer.provider_name,
            model: answer.model,
            summary_markdown: answer.answer,
            snippet_sha256,
            snippet_start_line: input.snippet_start_line,
            snippet_end_line: input.snippet_end_line,
            matched_lines: input.matched_lines.max(0),
            created_by: requester.into(),
            created_at: String::new(),
        };
        let saved = db.create_jenkins_build_analysis(&analysis)?;
        Self::audit_connection(
            db,
            "jenkins.build.failure_analysis.create",
            &connection,
            "生成 Jenkins 构建失败 AI 总结",
        )?;
        Ok(saved)
    }

    pub fn get_latest_build_analysis(
        db: &Database,
        input: GetJenkinsBuildInput,
    ) -> Result<Option<JenkinsBuildAnalysis>, AppError> {
        let connection_key = input.connection_key.trim();
        let job_full_name = input.job_full_name.trim();
        if connection_key.is_empty() || job_full_name.is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if input.build_number <= 0 {
            return Err(AppError::InvalidInput("buildNumber 必须是正整数".into()));
        }
        db.get_latest_jenkins_build_analysis(connection_key, job_full_name, input.build_number)
    }

    pub async fn list_queue(
        db: &Database,
        connection_key: String,
    ) -> Result<Vec<JenkinsQueueItem>, AppError> {
        let connection_key = connection_key.trim();
        if connection_key.is_empty() {
            return Err(AppError::InvalidInput("连接不能为空".into()));
        }
        let connection = Self::require_connection(db, connection_key)?;
        let url = Self::queue_api_url(&connection);
        let value = Self::jenkins_get_json(db, &connection, &url, None).await?;
        let items = value
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| Self::map_queue_item(&connection, item))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Self::audit_read(
            db,
            "jenkins.queue.list",
            &connection,
            "读取 Jenkins 队列",
            json!({
                "connectionKey": connection.connection_key,
                "count": items.len()
            }),
            None,
        )?;
        Ok(items)
    }

    pub async fn poll_queue_item(
        db: &Database,
        input: PollJenkinsQueueItemInput,
    ) -> Result<JenkinsQueueItem, AppError> {
        let connection_key = input.connection_key.trim();
        let queue_id = input.queue_id.trim();
        if connection_key.is_empty() || queue_id.is_empty() {
            return Err(AppError::InvalidInput("连接和 queueId 不能为空".into()));
        }
        let connection = Self::require_connection(db, connection_key)?;
        let url = Self::queue_item_api_url(&connection, queue_id);
        let value = Self::jenkins_get_json(db, &connection, &url, None).await?;
        let item = Self::map_queue_item(&connection, &value).ok_or_else(|| {
            AppError::InvalidInput(format!("Jenkins queue item '{}' 响应格式无效", queue_id))
        })?;
        Self::audit_read(
            db,
            "jenkins.queue.item.poll",
            &connection,
            "轮询 Jenkins queue item",
            json!({
                "connectionKey": connection.connection_key,
                "queueId": item.queue_id,
                "jobFullName": item.job_full_name,
                "status": item.status,
                "buildNumber": item.build_number
            }),
            None,
        )?;
        Ok(item)
    }

    pub async fn list_artifacts(
        db: &Database,
        input: ListJenkinsArtifactsInput,
    ) -> Result<Vec<JenkinsArtifact>, AppError> {
        if input.connection_key.trim().is_empty() || input.job_full_name.trim().is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if input.build_number <= 0 {
            return Err(AppError::InvalidInput("构建号必须大于 0".into()));
        }
        let connection = Self::require_connection(db, &input.connection_key)?;
        let mut artifacts =
            Self::fetch_artifacts(db, &connection, &input.job_full_name, input.build_number)
                .await?;
        let records = db.list_jenkins_artifact_records(
            &input.connection_key,
            &input.job_full_name,
            input.build_number,
        )?;
        for artifact in &mut artifacts {
            if let Some(record) = records
                .iter()
                .find(|record| record.relative_path == artifact.relative_path)
            {
                artifact.id = record.id;
                artifact.artifact_key = record.artifact_key.clone();
                artifact.request_id = record.request_id.clone();
                artifact.local_path = record.local_path.clone();
                artifact.size_bytes = record.size_bytes;
                artifact.sha256 = record.sha256.clone();
                artifact.status = record.status.clone();
                artifact.downloaded_at = record.downloaded_at.clone();
                artifact.cleaned_at = record.cleaned_at.clone();
                artifact.created_at = record.created_at.clone();
                artifact.updated_at = record.updated_at.clone();
            }
        }
        Self::audit_read(
            db,
            "jenkins.artifacts.list",
            &connection,
            "读取 Jenkins artifact 列表",
            json!({
                "connectionKey": connection.connection_key,
                "jobFullName": input.job_full_name,
                "buildNumber": input.build_number,
                "artifactCount": artifacts.len()
            }),
            None,
        )?;
        Ok(artifacts)
    }

    pub async fn download_artifact(
        app: &tauri::AppHandle,
        db: &Database,
        input: DownloadJenkinsArtifactInput,
    ) -> Result<JenkinsArtifact, AppError> {
        if input.connection_key.trim().is_empty()
            || input.job_full_name.trim().is_empty()
            || input.relative_path.trim().is_empty()
        {
            return Err(AppError::InvalidInput(
                "连接、Job 和 artifact 路径不能为空".into(),
            ));
        }
        if input.build_number <= 0 {
            return Err(AppError::InvalidInput("构建号必须大于 0".into()));
        }
        let connection = Self::require_connection(db, &input.connection_key)?;
        let artifacts =
            Self::fetch_artifacts(db, &connection, &input.job_full_name, input.build_number)
                .await?;
        let artifact = artifacts
            .into_iter()
            .find(|item| item.relative_path == input.relative_path)
            .ok_or_else(|| AppError::NotFound("Jenkins artifact 不存在".into()))?;
        let source_url = Self::artifact_download_url(
            &connection,
            &input.job_full_name,
            input.build_number,
            &artifact.relative_path,
        );
        let mut stream = Self::jenkins_get_stream(db, &connection, &source_url).await?;
        if let Some(size) = stream
            .response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok())
        {
            if size > JENKINS_ARTIFACT_MAX_BYTES {
                return Err(AppError::InvalidInput(format!(
                    "artifact 超过 {}MB 下载限制",
                    JENKINS_ARTIFACT_MAX_BYTES / 1024 / 1024
                )));
            }
        }

        let request_id = Self::new_request_id();
        let relative_path = safe_relative_path(&artifact.relative_path)?;
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let local_dir = app_data_dir
            .join("jenkins-artifacts")
            .join(&connection.connection_key)
            .join(sanitize_key_segment(&input.job_full_name))
            .join(input.build_number.to_string());
        let local_path = local_dir.join(&relative_path);
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if local_path.exists() {
            return Err(AppError::InvalidInput(
                "本地托管目录已存在同名 artifact，请先清理后再下载".into(),
            ));
        }

        let mut file = std::fs::File::create(&local_path)?;
        let mut hasher = Sha256::new();
        let mut total: i64 = 0;
        while let Some(chunk) = stream.response.chunk().await.map_err(Self::http_error)? {
            total += chunk.len() as i64;
            if total > JENKINS_ARTIFACT_MAX_BYTES {
                let _ = std::fs::remove_file(&local_path);
                return Err(AppError::InvalidInput(format!(
                    "artifact 超过 {}MB 下载限制",
                    JENKINS_ARTIFACT_MAX_BYTES / 1024 / 1024
                )));
            }
            hasher.update(&chunk);
            std::io::Write::write_all(&mut file, &chunk)?;
        }
        let sha256 = format!("{:x}", hasher.finalize());
        let saved = db.upsert_jenkins_artifact_record(&JenkinsArtifact {
            id: 0,
            artifact_key: Self::artifact_key(
                &connection.connection_key,
                &input.job_full_name,
                input.build_number,
                &artifact.relative_path,
            ),
            request_id: request_id.clone(),
            connection_key: connection.connection_key.clone(),
            job_full_name: input.job_full_name.clone(),
            build_number: input.build_number,
            file_name: artifact.file_name.clone(),
            relative_path: artifact.relative_path.clone(),
            local_path: local_path.to_string_lossy().to_string(),
            size_bytes: Some(total),
            sha256,
            source_url,
            status: "available".into(),
            risk_flags: Self::artifact_risk_flags(&artifact.file_name),
            downloaded_at: None,
            cleaned_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        })?;
        let mut result = saved;
        result.source_url = artifact.source_url;
        result.risk_flags = Self::artifact_risk_flags(&result.file_name);
        Self::audit_read(
            db,
            "jenkins.artifact.download",
            &connection,
            "下载 Jenkins artifact",
            json!({
                "connectionKey": connection.connection_key,
                "jobFullName": input.job_full_name,
                "buildNumber": input.build_number,
                "relativePath": artifact.relative_path,
                "sizeBytes": result.size_bytes,
                "sha256": result.sha256,
                "riskFlags": result.risk_flags,
                "artifactKey": result.artifact_key
            }),
            Some(request_id),
        )?;
        Ok(result)
    }

    pub fn cleanup_artifact_local_file(
        app: &tauri::AppHandle,
        db: &Database,
        input: CleanupJenkinsArtifactInput,
    ) -> Result<JenkinsArtifact, AppError> {
        let artifact_key = input.artifact_key.trim();
        if artifact_key.is_empty() {
            return Err(AppError::InvalidInput("artifactKey 不能为空".into()));
        }
        let record = db
            .get_jenkins_artifact_record(artifact_key)?
            .ok_or_else(|| AppError::NotFound("Jenkins artifact 记录不存在".into()))?;
        if record.local_path.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "该 artifact 没有本地托管文件可清理".into(),
            ));
        }
        let managed_root = Self::managed_artifact_root(app)?;
        let local_path = Self::validate_managed_artifact_path(&managed_root, &record.local_path)?;
        let status = if local_path.exists() {
            std::fs::remove_file(&local_path)?;
            "local_deleted"
        } else {
            "file_missing"
        };
        let mut updated = db.mark_jenkins_artifact_local_cleanup(artifact_key, status)?;
        updated.risk_flags = Self::artifact_risk_flags(&updated.file_name);
        Self::audit_artifact_cleanup(db, &record, status)?;
        Ok(updated)
    }

    pub fn create_artifact_deployment_candidate(
        db: &Database,
        input: CreateJenkinsArtifactDeploymentCandidateInput,
    ) -> Result<DeploymentCandidate, AppError> {
        let artifact_key = input.artifact_key.trim();
        if artifact_key.is_empty() {
            return Err(AppError::InvalidInput("artifactKey 不能为空".into()));
        }
        let mut record = db
            .get_jenkins_artifact_record(artifact_key)?
            .ok_or_else(|| AppError::NotFound("Jenkins artifact 记录不存在".into()))?;
        record.risk_flags = Self::artifact_risk_flags(&record.file_name);
        if record.local_path.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "该 artifact 尚未下载到应用托管目录，不能创建部署候选".into(),
            ));
        }
        if record.status != "available" {
            return Err(AppError::InvalidInput(format!(
                "只有 available 状态的 artifact 可以创建部署候选，当前状态为 {}",
                record.status
            )));
        }
        if !PathBuf::from(record.local_path.trim()).exists() {
            return Err(AppError::InvalidInput(
                "artifact 本地文件不存在，请重新下载后再创建部署候选".into(),
            ));
        }
        let build = db
            .get_jenkins_build_run_by_number(
                &record.connection_key,
                &record.job_full_name,
                record.build_number,
            )?
            .ok_or_else(|| {
                AppError::InvalidInput("未找到该 artifact 对应的 Jenkins 构建记录".into())
            })?;
        if !Self::is_successful_build(&build) {
            return Err(AppError::InvalidInput(format!(
                "只有成功构建可以创建部署候选，当前 result={} status={}",
                build.result, build.status
            )));
        }

        let connection = Self::require_connection(db, &record.connection_key)?;
        let candidate = Self::deployment_candidate_from_artifact(&record);
        Self::audit_read(
            db,
            "jenkins.artifact.deployment_candidate.create",
            &connection,
            "创建 Jenkins artifact 部署候选",
            json!({
                "artifactKey": record.artifact_key,
                "connectionKey": record.connection_key,
                "jobFullName": record.job_full_name,
                "buildNumber": record.build_number,
                "relativePath": record.relative_path,
                "candidateKey": candidate.key
            }),
            Some(record.request_id),
        )?;
        Ok(candidate)
    }

    pub async fn create_build_deployment_dry_run(
        db: &Database,
        input: CreateJenkinsBuildDeploymentDryRunInput,
    ) -> Result<DeploymentPlan, AppError> {
        let artifact_key = input.artifact_key.trim();
        let server_alias = input.server_alias.trim();
        if artifact_key.is_empty() {
            return Err(AppError::InvalidInput("artifactKey 不能为空".into()));
        }
        if server_alias.is_empty() {
            return Err(AppError::InvalidInput("目标服务器不能为空".into()));
        }

        let mut record = db
            .get_jenkins_artifact_record(artifact_key)?
            .ok_or_else(|| AppError::NotFound("Jenkins artifact 记录不存在".into()))?;
        record.risk_flags = Self::artifact_risk_flags(&record.file_name);
        if record.local_path.trim().is_empty() || record.status != "available" {
            return Err(AppError::InvalidInput(
                "只有已下载且 available 的 artifact 可以进入部署 dry-run".into(),
            ));
        }
        if !PathBuf::from(record.local_path.trim()).exists() {
            return Err(AppError::InvalidInput(
                "artifact 本地文件不存在，请重新下载后再生成部署 dry-run".into(),
            ));
        }
        let build = db
            .get_jenkins_build_run_by_number(
                &record.connection_key,
                &record.job_full_name,
                record.build_number,
            )?
            .ok_or_else(|| {
                AppError::InvalidInput("未找到该 artifact 对应的 Jenkins 构建记录".into())
            })?;
        if !Self::is_successful_build(&build) {
            return Err(AppError::InvalidInput(format!(
                "只有成功构建可以进入部署 dry-run，当前 result={} status={}",
                build.result, build.status
            )));
        }

        let connection = Self::require_connection(db, &record.connection_key)?;
        let candidate = Self::deployment_candidate_from_artifact(&record);
        let deploy_root = input
            .deploy_root
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("/opt/tauri-ssh/stacks/{}", candidate.key));
        let mut target = Self::deployment_target_from_candidate(
            &candidate,
            server_alias,
            &deploy_root,
            input.domain.unwrap_or_default(),
            input.https_enabled.unwrap_or(false),
            input.port,
            input.health_check_url.unwrap_or_default(),
        );
        target.config_json = Self::merge_deployment_dry_run_config(
            &candidate.config_json,
            &build,
            &record,
            &connection,
        )?;
        let plan = DeploymentService::create_dry_run_for_target(db, &target, String::new()).await?;
        Self::audit_read(
            db,
            "jenkins.build.deployment_dry_run",
            &connection,
            "从 Jenkins 构建结果生成部署 dry-run",
            json!({
                "planId": plan.plan_id,
                "artifactKey": record.artifact_key,
                "runKey": build.run_key,
                "connectionKey": record.connection_key,
                "jobFullName": record.job_full_name,
                "buildNumber": record.build_number,
                "serverAlias": server_alias,
                "targetKey": target.target_key,
                "recipe": target.recipe,
                "approvalRequired": plan.approval_required,
                "risk": plan.risk
            }),
            Some(build.request_id),
        )?;
        Ok(plan)
    }

    fn require_connection(
        db: &Database,
        connection_key: &str,
    ) -> Result<JenkinsConnection, AppError> {
        let key = connection_key.trim();
        if key.is_empty() {
            return Err(AppError::InvalidInput("Jenkins 连接不能为空".into()));
        }
        db.get_jenkins_connection(key)?
            .ok_or_else(|| AppError::NotFound(format!("Jenkins 连接 '{}' 不存在", key)))
    }

    fn validate_name(name: &str) -> Result<(), AppError> {
        if name.trim().is_empty() {
            return Err(AppError::InvalidInput("连接名称不能为空".into()));
        }
        Ok(())
    }

    fn normalize_approval_policy(input: Option<&str>) -> Result<String, AppError> {
        let policy = input
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("manual");
        match policy {
            "manual" | "risk_based" | "readonly" | "none" => Ok(policy.to_string()),
            _ => Err(AppError::InvalidInput(format!(
                "Jenkins 审批策略 '{}' 不支持",
                policy
            ))),
        }
    }

    fn ensure_build_trigger_allowed(connection: &JenkinsConnection) -> Result<(), AppError> {
        if !connection.enabled {
            return Err(AppError::InvalidInput("Jenkins 连接未启用".into()));
        }
        if [
            "disabled",
            "credential_missing",
            "credential_failed",
            "failed",
        ]
        .contains(&connection.status.as_str())
        {
            return Err(AppError::InvalidInput(format!(
                "Jenkins 连接状态 '{}' 不允许触发构建",
                connection.status
            )));
        }
        if connection.approval_policy == "readonly" {
            return Err(AppError::InvalidInput(
                "该 Jenkins 连接审批策略禁止写入".into(),
            ));
        }
        Ok(())
    }

    fn ensure_no_approval_policy(connection: &JenkinsConnection) -> Result<(), AppError> {
        if connection.approval_policy != "none" {
            return Err(AppError::InvalidInput(
                "该 Jenkins 连接未选择无需审批策略，不能绕过审批队列".into(),
            ));
        }
        Ok(())
    }

    fn normalize_build_trigger_risk(
        connection: &JenkinsConnection,
        job_full_name: &str,
        requested: Option<&str>,
        parameters_json: &Value,
        concurrency_blocked: bool,
    ) -> Result<String, AppError> {
        if let Some(risk) = requested.map(str::trim).filter(|value| !value.is_empty()) {
            Self::validate_write_risk("riskLevel", risk)?;
            return Ok(risk.into());
        }
        let rules = Self::parse_risk_rules(&connection.risk_rules_json)?;
        let mut risk = rules.fallback_risk.clone();
        if Self::parameter_payload_has_flag(parameters_json, "unsupported", true) {
            return Ok("blocked".into());
        }
        if concurrency_blocked {
            return Ok("blocked".into());
        }
        if rules.environment_risk == "auto" {
            if connection.environment == "prod" {
                risk = Self::max_risk(&risk, "L3").into();
            }
        } else {
            risk = Self::max_risk(&risk, &rules.environment_risk).into();
        }
        if Self::parameter_payload_has_file(parameters_json) {
            risk = Self::max_risk(&risk, &rules.file_parameter_risk).into();
        }
        if Self::parameter_payload_has_flag(parameters_json, "dynamicParameter", true) {
            risk = Self::max_risk(&risk, "L3").into();
        }
        for rule in rules.job_rules.iter().filter(|rule| rule.enabled) {
            let regex = Regex::new(rule.pattern.trim()).map_err(|e| {
                AppError::InvalidInput(format!("Job 风险正则无效 '{}': {}", rule.pattern, e))
            })?;
            if regex.is_match(job_full_name) {
                risk = Self::max_risk(&risk, &rule.risk).into();
            }
        }
        for rule in rules.parameter_rules.iter().filter(|rule| rule.enabled) {
            if Self::parameter_payload_matches_rule(parameters_json, rule) {
                risk = Self::max_risk(&risk, &rule.risk).into();
            }
        }
        Ok(risk)
    }

    fn normalize_build_stop_risk(
        connection: &JenkinsConnection,
        job_full_name: &str,
        requested: Option<&str>,
    ) -> Result<String, AppError> {
        if let Some(risk) = requested.map(str::trim).filter(|value| !value.is_empty()) {
            Self::validate_write_risk("riskLevel", risk)?;
            return Ok(risk.into());
        }
        if connection.environment == "prod" || Self::job_name_is_release_or_prod(job_full_name) {
            return Ok("L3".into());
        }
        Ok("L2".into())
    }

    fn build_stop_risk_flags(connection: &JenkinsConnection, job_full_name: &str) -> Vec<String> {
        let mut flags = Vec::new();
        if connection.environment == "prod" {
            flags.push("prod_environment".into());
        }
        if Self::job_name_is_release_or_prod(job_full_name) {
            flags.push("release_or_prod_job".into());
        }
        flags
    }

    fn job_name_is_release_or_prod(job_full_name: &str) -> bool {
        let lowered = job_full_name.to_ascii_lowercase();
        lowered.contains("release") || lowered.contains("prod") || lowered.contains("production")
    }

    fn request_hash(value: &Value) -> Result<String, AppError> {
        let bytes = serde_json::to_vec(value)?;
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }

    fn validate_build_trigger_approval(
        db: &Database,
        input: &ExecuteJenkinsBuildApprovedInput,
    ) -> Result<JenkinsBuildApprovalContext, AppError> {
        if input.approval_id <= 0 {
            return Err(AppError::InvalidInput("approvalId 不能为空".into()));
        }
        let approval = db.get_approval_request(input.approval_id)?.ok_or_else(|| {
            AppError::NotFound(format!("审批请求 '{}' 不存在", input.approval_id))
        })?;
        if approval.source != "jenkins" || approval.action != "jenkins_build_trigger" {
            return Err(AppError::InvalidInput(
                "审批请求不是 Jenkins 构建触发类型".into(),
            ));
        }
        if approval.status != "approved" {
            return Err(AppError::InvalidInput(format!(
                "审批请求状态 '{}' 不允许执行",
                approval.status
            )));
        }
        let payload: Value = serde_json::from_str(&approval.payload_json)?;
        let request_hash = payload
            .get("requestHash")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("审批 payload 缺少 requestHash".into()))?
            .to_string();
        if let Some(expected) = input
            .request_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if expected != request_hash {
                return Err(AppError::InvalidInput("requestHash 不匹配".into()));
            }
        }
        if approval.command != request_hash {
            return Err(AppError::InvalidInput(
                "审批 command 与 requestHash 不匹配".into(),
            ));
        }
        let mut controlled_payload = payload.clone();
        if let Value::Object(map) = &mut controlled_payload {
            map.remove("requestHash");
        }
        let recomputed_hash = Self::request_hash(&controlled_payload)?;
        if recomputed_hash != request_hash {
            return Err(AppError::InvalidInput("requestHash 复验失败".into()));
        }
        if payload.get("action").and_then(Value::as_str) != Some("jenkins_build_trigger") {
            return Err(AppError::InvalidInput("审批 payload action 不正确".into()));
        }

        let connection_key = payload
            .get("connectionKey")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("审批 payload 缺少 connectionKey".into()))?;
        let connection = Self::require_connection(db, connection_key)?;
        Self::ensure_build_trigger_allowed(&connection)?;
        if !connection.allow_mcp_write {
            return Err(AppError::InvalidInput(
                "该 Jenkins 连接未开启 allow_mcp_write，不能执行 approved 构建触发".into(),
            ));
        }
        let approved_config_version = payload
            .get("connectionConfigVersion")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                AppError::InvalidInput("审批 payload 缺少 connectionConfigVersion".into())
            })?;
        if approved_config_version != connection.config_version {
            return Err(AppError::InvalidInput(
                "connection_changed_after_approval".into(),
            ));
        }
        let job_full_name = payload
            .get("jobFullName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("审批 payload 缺少 jobFullName".into()))?
            .to_string();
        let parameter_definition_hash = payload
            .get("parameterDefinitionHash")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AppError::InvalidInput("审批 payload 缺少 parameterDefinitionHash".into())
            })?
            .to_string();
        let parameters_json = payload.get("parameters").cloned().unwrap_or(Value::Null);
        let approved_risk = payload
            .get("riskLevel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&approval.risk)
            .to_string();
        let requester = payload
            .get("requester")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&approval.requester)
            .to_string();
        let concurrency_blocked = Self::job_has_unfinished_run(db, connection_key, &job_full_name)?
            && !Self::allow_concurrent_build_for_job(&connection, &job_full_name)?;
        let current_risk = Self::normalize_build_trigger_risk(
            &connection,
            &job_full_name,
            None,
            &parameters_json,
            concurrency_blocked,
        )?;
        if current_risk == "blocked"
            || Self::risk_rank(&current_risk) > Self::risk_rank(&approved_risk)
        {
            return Err(AppError::InvalidInput(
                "build_risk_escalated_after_approval".into(),
            ));
        }

        Ok(JenkinsBuildApprovalContext {
            approval_id: approval.id,
            request_hash,
            connection,
            job_full_name,
            parameter_definition_hash,
            parameters_json,
            risk_level: approved_risk,
            requester,
        })
    }

    fn build_direct_trigger_context(
        db: &Database,
        input: TriggerJenkinsBuildInput,
    ) -> Result<JenkinsBuildApprovalContext, AppError> {
        let connection_key = input.connection_key.trim();
        let job_full_name = input.job_full_name.trim();
        let parameter_definition_hash = input.parameter_definition_hash.trim();
        if connection_key.is_empty() || job_full_name.is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if parameter_definition_hash.is_empty() {
            return Err(AppError::InvalidInput(
                "parameterDefinitionHash 不能为空".into(),
            ));
        }
        let connection = Self::require_connection(db, connection_key)?;
        Self::ensure_build_trigger_allowed(&connection)?;
        Self::ensure_no_approval_policy(&connection)?;
        let parameters_json = Self::sanitize_parameter_approval_payload(&input.parameters_json)?;
        let concurrency_blocked = Self::job_has_unfinished_run(db, connection_key, job_full_name)?
            && !Self::allow_concurrent_build_for_job(&connection, job_full_name)?;
        let risk = Self::normalize_build_trigger_risk(
            &connection,
            job_full_name,
            input.risk_level.as_deref(),
            &parameters_json,
            concurrency_blocked,
        )?;
        if risk == "blocked" {
            return Err(AppError::InvalidInput(
                "当前风险规则阻断该 Jenkins 构建触发".into(),
            ));
        }
        let requester = input
            .requester
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("local-user");
        let reason = input
            .reason
            .trim()
            .is_empty()
            .then_some("连接策略为无需审批，直接触发 Jenkins 构建")
            .unwrap_or_else(|| input.reason.trim());
        let created_at_bucket = Self::approval_created_at_bucket(chrono::Utc::now());
        let risk_flags = Self::build_risk_flags(
            &connection,
            job_full_name,
            &parameters_json,
            concurrency_blocked,
        );
        let controlled_payload = json!({
            "action": "jenkins_build_trigger",
            "connectionKey": connection.connection_key,
            "connectionConfigVersion": connection.config_version,
            "jobFullName": job_full_name,
            "parameterDefinitionHash": parameter_definition_hash,
            "parameters": parameters_json,
            "requester": requester,
            "reason": reason,
            "riskLevel": risk,
            "riskFlags": risk_flags,
            "approvalPolicy": "none",
            "createdAtBucket": created_at_bucket,
            "status": "no_approval_direct"
        });
        let request_hash = Self::request_hash(&controlled_payload)?;
        Ok(JenkinsBuildApprovalContext {
            approval_id: 0,
            request_hash,
            connection,
            job_full_name: job_full_name.to_string(),
            parameter_definition_hash: parameter_definition_hash.to_string(),
            parameters_json,
            risk_level: risk,
            requester: requester.to_string(),
        })
    }

    fn validate_build_stop_approval(
        db: &Database,
        input: &ExecuteJenkinsBuildStopApprovedInput,
    ) -> Result<JenkinsBuildStopApprovalContext, AppError> {
        if input.approval_id <= 0 {
            return Err(AppError::InvalidInput("approvalId 不能为空".into()));
        }
        let approval = db.get_approval_request(input.approval_id)?.ok_or_else(|| {
            AppError::NotFound(format!("审批请求 '{}' 不存在", input.approval_id))
        })?;
        if approval.source != "jenkins" || approval.action != "jenkins_build_stop" {
            return Err(AppError::InvalidInput(
                "审批请求不是 Jenkins 停止构建类型".into(),
            ));
        }
        if approval.status != "approved" {
            return Err(AppError::InvalidInput(format!(
                "审批请求状态 '{}' 不允许执行",
                approval.status
            )));
        }
        let payload: Value = serde_json::from_str(&approval.payload_json)?;
        let request_hash = payload
            .get("requestHash")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("审批 payload 缺少 requestHash".into()))?
            .to_string();
        if let Some(expected) = input
            .request_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if expected != request_hash {
                return Err(AppError::InvalidInput("requestHash 不匹配".into()));
            }
        }
        if approval.command != request_hash {
            return Err(AppError::InvalidInput(
                "审批 command 与 requestHash 不匹配".into(),
            ));
        }
        let mut controlled_payload = payload.clone();
        if let Value::Object(map) = &mut controlled_payload {
            map.remove("requestHash");
        }
        let recomputed_hash = Self::request_hash(&controlled_payload)?;
        if recomputed_hash != request_hash {
            return Err(AppError::InvalidInput("requestHash 复验失败".into()));
        }
        if payload.get("action").and_then(Value::as_str) != Some("jenkins_build_stop") {
            return Err(AppError::InvalidInput("审批 payload action 不正确".into()));
        }
        let connection_key = payload
            .get("connectionKey")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("审批 payload 缺少 connectionKey".into()))?;
        let connection = Self::require_connection(db, connection_key)?;
        Self::ensure_build_trigger_allowed(&connection)?;
        if !connection.allow_mcp_write {
            return Err(AppError::InvalidInput(
                "该 Jenkins 连接未开启 allow_mcp_write，不能执行 approved 停止构建".into(),
            ));
        }
        let approved_config_version = payload
            .get("connectionConfigVersion")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                AppError::InvalidInput("审批 payload 缺少 connectionConfigVersion".into())
            })?;
        if approved_config_version != connection.config_version {
            return Err(AppError::InvalidInput(
                "connection_changed_after_approval".into(),
            ));
        }
        let job_full_name = payload
            .get("jobFullName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("审批 payload 缺少 jobFullName".into()))?
            .to_string();
        let build_number = payload
            .get("buildNumber")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .ok_or_else(|| AppError::InvalidInput("审批 payload 缺少 buildNumber".into()))?;
        let approved_risk = payload
            .get("riskLevel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&approval.risk)
            .to_string();
        let requester = payload
            .get("requester")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&approval.requester)
            .to_string();
        let current_risk = Self::normalize_build_stop_risk(&connection, &job_full_name, None)?;
        if Self::risk_rank(&current_risk) > Self::risk_rank(&approved_risk) {
            return Err(AppError::InvalidInput(
                "build_stop_risk_escalated_after_approval".into(),
            ));
        }

        Ok(JenkinsBuildStopApprovalContext {
            approval_id: approval.id,
            request_hash,
            connection,
            job_full_name,
            build_number,
            risk_level: approved_risk,
            requester,
        })
    }

    fn build_direct_stop_context(
        db: &Database,
        input: StopJenkinsBuildInput,
    ) -> Result<JenkinsBuildStopApprovalContext, AppError> {
        let connection_key = input.connection_key.trim();
        let job_full_name = input.job_full_name.trim();
        if connection_key.is_empty() || job_full_name.is_empty() {
            return Err(AppError::InvalidInput("连接和 Job 不能为空".into()));
        }
        if input.build_number <= 0 {
            return Err(AppError::InvalidInput("buildNumber 必须大于 0".into()));
        }
        let connection = Self::require_connection(db, connection_key)?;
        Self::ensure_build_trigger_allowed(&connection)?;
        Self::ensure_no_approval_policy(&connection)?;
        let risk = Self::normalize_build_stop_risk(
            &connection,
            job_full_name,
            input.risk_level.as_deref(),
        )?;
        let requester = input
            .requester
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("local-user");
        let reason = input
            .reason
            .trim()
            .is_empty()
            .then_some("连接策略为无需审批，直接停止 Jenkins 构建")
            .unwrap_or_else(|| input.reason.trim());
        let created_at_bucket = Self::approval_created_at_bucket(chrono::Utc::now());
        let risk_flags = Self::build_stop_risk_flags(&connection, job_full_name);
        let controlled_payload = json!({
            "action": "jenkins_build_stop",
            "connectionKey": connection.connection_key,
            "connectionConfigVersion": connection.config_version,
            "jobFullName": job_full_name,
            "buildNumber": input.build_number,
            "requester": requester,
            "reason": reason,
            "riskLevel": risk,
            "riskFlags": risk_flags,
            "approvalPolicy": "none",
            "createdAtBucket": created_at_bucket,
            "status": "no_approval_direct"
        });
        let request_hash = Self::request_hash(&controlled_payload)?;
        Ok(JenkinsBuildStopApprovalContext {
            approval_id: 0,
            request_hash,
            connection,
            job_full_name: job_full_name.to_string(),
            build_number: input.build_number,
            risk_level: risk,
            requester: requester.to_string(),
        })
    }

    fn risk_rank(risk: &str) -> i64 {
        match risk {
            "L1" | "readonly" => 1,
            "L2" => 2,
            "L3" => 3,
            "blocked" => 4,
            _ => 2,
        }
    }

    fn max_risk<'a>(left: &'a str, right: &'a str) -> &'a str {
        if Self::risk_rank(right) > Self::risk_rank(left) {
            right
        } else {
            left
        }
    }

    fn job_has_unfinished_run(
        db: &Database,
        connection_key: &str,
        job_full_name: &str,
    ) -> Result<bool, AppError> {
        let input = ListJenkinsBuildsInput {
            connection_key: connection_key.to_string(),
            job_full_name: Some(job_full_name.to_string()),
            limit: Some(50),
            offset: Some(0),
            cursor: None,
        };
        Ok(db.list_jenkins_build_runs(&input)?.iter().any(|run| {
            matches!(
                run.status.as_str(),
                "queued"
                    | "waiting"
                    | "blocked"
                    | "stuck"
                    | "triggered"
                    | "building"
                    | "tracking_timeout"
            ) || (run.finished_at.is_none()
                && !matches!(
                    run.status.as_str(),
                    "success"
                        | "failure"
                        | "unstable"
                        | "aborted"
                        | "not_built"
                        | "queue_timeout"
                        | "sync_failed"
                ))
        }))
    }

    fn approval_created_at_bucket(now: chrono::DateTime<chrono::Utc>) -> String {
        now.format("%Y-%m-%dT%H:%MZ").to_string()
    }

    fn build_risk_flags(
        connection: &JenkinsConnection,
        job_full_name: &str,
        parameters_json: &Value,
        concurrency_blocked: bool,
    ) -> Vec<String> {
        let mut flags = Vec::new();
        if connection.environment == "prod" {
            flags.push("prod_environment".into());
        }
        if Self::job_name_is_release_or_prod(job_full_name) {
            flags.push("release_or_prod_job".into());
        }
        if Self::parameter_payload_has_file(parameters_json) {
            flags.push("file_parameter".into());
        }
        if Self::parameter_payload_has_flag(parameters_json, "dynamicParameter", true) {
            flags.push("dynamic_parameter".into());
        }
        if Self::parameter_payload_has_flag(parameters_json, "unsupported", true) {
            flags.push("unsupported_parameter".into());
        }
        if concurrency_blocked {
            flags.push("concurrent_build_blocked".into());
        }
        flags
    }

    fn parameter_payload_has_entries(value: &Value) -> bool {
        match value {
            Value::Null => false,
            Value::Array(items) => !items.is_empty(),
            Value::Object(map) => {
                if let Some(parameters) = map.get("parameters") {
                    return Self::parameter_payload_has_entries(parameters);
                }
                !map.is_empty()
            }
            Value::String(value) => !value.trim().is_empty(),
            _ => true,
        }
    }

    fn validate_parameter_template_payload(value: &Value) -> Result<(), AppError> {
        let parameters = value
            .get("parameters")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::InvalidInput("参数模板缺少 parameters 数组".into()))?;
        for parameter in parameters {
            let name = parameter
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::InvalidInput("参数模板包含缺少名称的参数".into()))?;
            let sensitive = parameter
                .get("sensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !sensitive {
                continue;
            }
            let value = parameter.get("value").ok_or_else(|| {
                AppError::InvalidInput(format!("敏感参数 {} 缺少 secretRef", name))
            })?;
            let value_kind = value
                .get("valueKind")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let secret_ref = value
                .get("secretRef")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if value_kind != "secret_ref" || secret_ref.is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "敏感参数 {} 的模板值只能保存 secretRef",
                    name
                )));
            }
        }
        Ok(())
    }

    fn collect_build_parameters(
        db: &Database,
        parameters_json: &Value,
    ) -> Result<Vec<JenkinsBuildParameter>, AppError> {
        let items = parameters_json
            .get("parameters")
            .and_then(Value::as_array)
            .or_else(|| parameters_json.as_array())
            .ok_or_else(|| AppError::InvalidInput("构建参数 payload 格式无效".into()))?;
        let mut parameters = Vec::new();
        for item in items {
            let map = item
                .as_object()
                .ok_or_else(|| AppError::InvalidInput("构建参数项格式无效".into()))?;
            let name = map
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::InvalidInput("构建参数名称不能为空".into()))?;
            if map
                .get("unsupported")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(AppError::InvalidInput(format!(
                    "参数 '{}' 类型不受支持，不能执行构建",
                    name
                )));
            }
            if map
                .get("fileParameter")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                parameters.push(Self::resolve_file_parameter(name, map)?);
                continue;
            }
            let value = map.get("value").unwrap_or(&Value::Null);
            let resolved_value = if map
                .get("sensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                Self::resolve_secret_parameter_value(db, name, value)?
            } else {
                Self::build_parameter_value_to_string(value)?
            };
            parameters.push(JenkinsBuildParameter::Scalar {
                name: name.to_string(),
                value: resolved_value,
            });
        }
        Ok(parameters)
    }

    fn stage_file_parameter_reference(local_path: &str) -> Result<String, AppError> {
        let path = PathBuf::from(local_path.trim());
        if !path.is_absolute() || !path.is_file() {
            return Err(AppError::InvalidInput(
                "File Parameter 本地文件引用无效".into(),
            ));
        }
        let mut hasher = Sha256::new();
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(sha256_file(&path)?.as_bytes());
        let ref_key = format!("{:x}", hasher.finalize());
        let file_name = path
            .file_name()
            .map(|value| sanitize_key_segment(&value.to_string_lossy()))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "upload".into());
        let dir = Self::file_parameter_reference_dir()?;
        std::fs::create_dir_all(&dir)?;
        let staged_path = dir.join(format!("{}-{}", ref_key, file_name));
        std::fs::copy(&path, &staged_path)?;
        Ok(staged_path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default())
    }

    fn file_parameter_reference_dir() -> Result<PathBuf, AppError> {
        Ok(std::env::temp_dir().join("tauri-ssh-jenkins-file-refs"))
    }

    fn resolve_file_parameter(
        parameter_name: &str,
        map: &serde_json::Map<String, Value>,
    ) -> Result<JenkinsBuildParameter, AppError> {
        let value = map.get("value").and_then(Value::as_object).ok_or_else(|| {
            AppError::InvalidInput(format!(
                "File Parameter '{}' 缺少受控元数据",
                parameter_name
            ))
        })?;
        let local_path_ref = value
            .get("localPathRef")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AppError::InvalidInput("file_parameter_reference_missing_after_approval".into())
            })?;
        if local_path_ref.contains('/')
            || local_path_ref.contains('\\')
            || local_path_ref.contains("..")
        {
            return Err(AppError::InvalidInput(
                "file_parameter_reference_invalid_after_approval".into(),
            ));
        }
        let path = Self::file_parameter_reference_dir()?.join(local_path_ref);
        if !path.is_file() {
            return Err(AppError::InvalidInput(
                "file_parameter_reference_missing_after_approval".into(),
            ));
        }
        let expected_sha256 = value
            .get("sha256")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("File Parameter 缺少 sha256".into()))?;
        let actual_sha256 = sha256_file(&path)?;
        if actual_sha256 != expected_sha256 {
            return Err(AppError::InvalidInput(
                "file_parameter_changed_after_approval".into(),
            ));
        }
        let metadata = std::fs::metadata(&path)?;
        let actual_size = i64::try_from(metadata.len())
            .map_err(|_| AppError::InvalidInput("文件大小超过可处理范围".into()))?;
        let expected_size = value
            .get("sizeBytes")
            .and_then(Value::as_i64)
            .unwrap_or(actual_size);
        if actual_size != expected_size {
            return Err(AppError::InvalidInput(
                "file_parameter_changed_after_approval".into(),
            ));
        }
        let file_name = value
            .get("fileName")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(parameter_name)
            .to_string();
        Ok(JenkinsBuildParameter::File {
            name: parameter_name.to_string(),
            file_name,
            path,
        })
    }

    fn record_recent_parameter_values(
        db: &Database,
        context: &JenkinsBuildApprovalContext,
        run: &JenkinsBuild,
    ) -> Result<(), AppError> {
        if !context.connection.parameter_prefill_enabled {
            return Ok(());
        }
        let requester = Self::normalize_requester(Some(&context.requester));
        let items = context
            .parameters_json
            .get("parameters")
            .and_then(Value::as_array)
            .or_else(|| context.parameters_json.as_array());
        let Some(items) = items else {
            return Ok(());
        };
        let mut saved_names = Vec::new();
        for item in items {
            let Some(map) = item.as_object() else {
                continue;
            };
            let Some(name) = map
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if map
                .get("fileParameter")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || map
                    .get("unsupported")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                continue;
            }
            let sensitive = map
                .get("sensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let Some((value_kind, value_json)) =
                Self::recent_parameter_value_payload(sensitive, map.get("value"))
            else {
                continue;
            };
            db.upsert_jenkins_recent_parameter_value(&JenkinsRecentParameterValue {
                id: 0,
                connection_key: context.connection.connection_key.clone(),
                job_full_name: context.job_full_name.clone(),
                parameter_name: name.to_string(),
                requester: requester.clone(),
                value_kind,
                value_json,
                sensitive,
                updated_from_run_key: run.run_key.clone(),
                updated_at: String::new(),
            })?;
            saved_names.push(name.to_string());
        }
        if !saved_names.is_empty() {
            Self::audit_connection(
                db,
                "jenkins.parameters.recent.save",
                &context.connection,
                &format!("保存 Jenkins 最近参数值：{} 个参数", saved_names.len()),
            )?;
        }
        Ok(())
    }

    fn recent_parameter_value_payload(
        sensitive: bool,
        value: Option<&Value>,
    ) -> Option<(String, Value)> {
        let value = value.unwrap_or(&Value::Null);
        if sensitive {
            let secret_ref = value
                .as_object()
                .and_then(|map| {
                    if map.get("valueKind").and_then(Value::as_str) == Some("secret_ref") {
                        map.get("secretRef").and_then(Value::as_str)
                    } else {
                        None
                    }
                })
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            return Some(("secret_ref".into(), json!({ "secretRef": secret_ref })));
        }
        match value {
            Value::Null | Value::String(_) | Value::Bool(_) | Value::Number(_) => {
                Some(("plain".into(), value.clone()))
            }
            _ => None,
        }
    }

    fn resolve_secret_parameter_value(
        db: &Database,
        parameter_name: &str,
        value: &Value,
    ) -> Result<String, AppError> {
        let secret_ref = value
            .as_object()
            .and_then(|map| {
                if map.get("valueKind").and_then(Value::as_str) == Some("secret_ref") {
                    map.get("secretRef").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "敏感参数 '{}' 缺少 secretRef，不能执行构建",
                    parameter_name
                ))
            })?;
        let credential = db
            .get_secure_credential(secret_ref)?
            .ok_or_else(|| AppError::NotFound(format!("安全凭证 '{}' 不存在", secret_ref)))?;
        if !credential.enabled || !credential.has_secret {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 不可用",
                secret_ref
            )));
        }
        SecureCredentialService::get_secret(db, &credential.credential_key)
    }

    fn build_parameter_value_to_string(value: &Value) -> Result<String, AppError> {
        match value {
            Value::Null => Ok(String::new()),
            Value::String(value) => Ok(value.clone()),
            Value::Bool(value) => Ok(value.to_string()),
            Value::Number(value) => Ok(value.to_string()),
            Value::Object(map)
                if map.get("valueKind").and_then(Value::as_str) == Some("secret_ref") =>
            {
                Err(AppError::InvalidInput(
                    "非敏感参数不允许使用 secretRef 值".into(),
                ))
            }
            Value::Object(_) | Value::Array(_) => Err(AppError::InvalidInput(
                "构建参数值只支持字符串、数字或布尔值".into(),
            )),
        }
    }

    fn sanitize_parameter_approval_payload(value: &Value) -> Result<Value, AppError> {
        match value {
            Value::Array(items) => Ok(Value::Array(
                items
                    .iter()
                    .map(Self::sanitize_parameter_approval_payload)
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Value::Object(map) => {
                let mut sanitized = serde_json::Map::new();
                let sensitive = map
                    .get("sensitive")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let is_file_parameter_value = map.contains_key("localPath")
                    && (map.contains_key("fileName")
                        || map.contains_key("sha256")
                        || map.contains_key("sizeBytes"));
                for (key, child) in map {
                    if key == "localPath" {
                        if is_file_parameter_value {
                            let local_path = child.as_str().unwrap_or_default();
                            let local_path_ref = Self::stage_file_parameter_reference(local_path)?;
                            sanitized.insert("localPathRef".into(), Value::String(local_path_ref));
                        }
                        continue;
                    }
                    if sensitive && key == "value" && !Self::value_is_secret_ref(child) {
                        sanitized.insert(key.clone(), json!("[REDACTED]"));
                        continue;
                    }
                    if Self::looks_sensitive_key(key) && !Self::value_is_secret_ref(child) {
                        sanitized.insert(key.clone(), json!("[REDACTED]"));
                        continue;
                    }
                    sanitized.insert(
                        key.clone(),
                        Self::sanitize_parameter_approval_payload(child)?,
                    );
                }
                Ok(Value::Object(sanitized))
            }
            _ => Ok(value.clone()),
        }
    }

    fn parameter_payload_has_file(value: &Value) -> bool {
        match value {
            Value::Array(items) => items.iter().any(Self::parameter_payload_has_file),
            Value::Object(map) => {
                map.get("fileParameter")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || map.contains_key("fileName")
                    || map.values().any(Self::parameter_payload_has_file)
            }
            _ => false,
        }
    }

    fn parameter_payload_has_flag(value: &Value, flag: &str, expected: bool) -> bool {
        match value {
            Value::Array(items) => items
                .iter()
                .any(|item| Self::parameter_payload_has_flag(item, flag, expected)),
            Value::Object(map) => {
                map.get(flag).and_then(Value::as_bool) == Some(expected)
                    || map
                        .values()
                        .any(|item| Self::parameter_payload_has_flag(item, flag, expected))
            }
            _ => false,
        }
    }

    fn parameter_payload_matches_rule(value: &Value, rule: &JenkinsParameterRiskRule) -> bool {
        let expected_name = rule.name.trim();
        if expected_name.is_empty() {
            return false;
        }
        match value {
            Value::Array(items) => items
                .iter()
                .any(|item| Self::parameter_payload_matches_rule(item, rule)),
            Value::Object(map) => {
                if let Some(parameters) = map.get("parameters") {
                    return Self::parameter_payload_matches_rule(parameters, rule);
                }
                let name_matches = map
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .map(|name| name == expected_name)
                    .unwrap_or(false);
                if !name_matches {
                    return map
                        .values()
                        .any(|item| Self::parameter_payload_matches_rule(item, rule));
                }
                let actual = map
                    .get("value")
                    .map(Self::risk_rule_value_to_string)
                    .unwrap_or_default();
                let expected = rule.value.trim();
                expected.is_empty() || actual == expected
            }
            _ => false,
        }
    }

    fn risk_rule_value_to_string(value: &Value) -> String {
        match value {
            Value::Null => String::new(),
            Value::String(value) => value.clone(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            Value::Object(map) => map
                .get("fileName")
                .or_else(|| map.get("secretRef"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            Value::Array(_) => value.to_string(),
        }
    }

    fn value_is_secret_ref(value: &Value) -> bool {
        value
            .as_object()
            .and_then(|map| map.get("valueKind"))
            .and_then(Value::as_str)
            == Some("secret_ref")
    }

    fn looks_sensitive_key(key: &str) -> bool {
        let normalized = key.to_ascii_lowercase();
        [
            "password",
            "passwd",
            "token",
            "secret",
            "credential",
            "cookie",
            "auth",
            "api_key",
        ]
        .iter()
        .any(|part| normalized.contains(part))
    }

    fn parse_risk_rules(raw: &str) -> Result<JenkinsRiskRules, AppError> {
        let normalized = Self::normalize_risk_rules_json(Some(raw))?;
        serde_json::from_str::<JenkinsRiskRules>(&normalized)
            .map_err(|e| AppError::InvalidInput(format!("风险规则格式无效: {}", e)))
    }

    fn validate_risk_rules(rules: &JenkinsRiskRules) -> Result<(), AppError> {
        if rules.version != 1 {
            return Err(AppError::InvalidInput("风险规则版本暂只支持 v1".into()));
        }
        for (field, risk) in [
            ("fallbackRisk", rules.fallback_risk.as_str()),
            ("fileParameterRisk", rules.file_parameter_risk.as_str()),
        ] {
            Self::validate_write_risk(field, risk)?;
        }
        if !["auto", "L2", "L3", "blocked"].contains(&rules.environment_risk.as_str()) {
            return Err(AppError::InvalidInput(
                "environmentRisk 只能是 auto、L2、L3 或 blocked".into(),
            ));
        }
        for pattern in &rules.concurrency.allow_concurrent_patterns {
            Self::validate_regex_pattern("allowConcurrentPatterns", pattern)?;
        }
        for rule in &rules.job_rules {
            Self::validate_regex_pattern("jobRules.pattern", &rule.pattern)?;
            Self::validate_write_risk("jobRules.risk", &rule.risk)?;
        }
        for rule in &rules.parameter_rules {
            if rule.name.trim().is_empty() {
                return Err(AppError::InvalidInput("参数风险规则名称不能为空".into()));
            }
            Self::validate_write_risk("parameterRules.risk", &rule.risk)?;
        }
        Ok(())
    }

    fn validate_write_risk(field: &str, risk: &str) -> Result<(), AppError> {
        if ["L2", "L3", "blocked"].contains(&risk) {
            return Ok(());
        }
        Err(AppError::InvalidInput(format!(
            "{} 只能是 L2、L3 或 blocked",
            field
        )))
    }

    fn validate_regex_pattern(field: &str, pattern: &str) -> Result<(), AppError> {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return Err(AppError::InvalidInput(format!("{} 不能为空", field)));
        }
        Regex::new(pattern).map_err(|e| {
            AppError::InvalidInput(format!("{} 正则无效 '{}': {}", field, pattern, e))
        })?;
        Ok(())
    }

    fn normalize_base_url(raw: &str) -> Result<String, AppError> {
        let value = raw.trim().trim_end_matches('/').to_string();
        if value.is_empty() {
            return Err(AppError::InvalidInput("Jenkins Base URL 不能为空".into()));
        }
        if !(value.starts_with("http://") || value.starts_with("https://")) {
            return Err(AppError::InvalidInput(
                "Jenkins Base URL 只支持 HTTP/HTTPS".into(),
            ));
        }
        if value.contains('?') || value.contains('#') {
            return Err(AppError::InvalidInput(
                "Jenkins Base URL 不能包含 query 或 hash".into(),
            ));
        }
        Ok(value)
    }

    async fn probe_connection(
        db: &Database,
        connection: &JenkinsConnection,
    ) -> Result<JenkinsProbeResult, AppError> {
        if connection.credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "未配置 Jenkins API Token 安全凭证".into(),
            ));
        }
        let credential = db
            .get_secure_credential(&connection.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", connection.credential_key))
            })?;
        Self::validate_jenkins_credential(&credential)?;
        let token = SecureCredentialService::get_secret(db, &credential.credential_key)?;
        let username = credential.account_name.trim();
        if username.is_empty() {
            return Err(AppError::InvalidInput(
                "Jenkins 安全凭证需要填写 accountName 作为用户名".into(),
            ));
        }

        let url = format!("{}/api/json", connection.base_url.trim_end_matches('/'));
        let target = Self::prepare_request_target(db, connection, &url)?;
        let response = Self::http_client(connection.tls_verify)?
            .get(&target.url)
            .header(USER_AGENT, "tauri-ssh")
            .header(ACCEPT, "application/json")
            .basic_auth(username, Some(token))
            .send()
            .await
            .map_err(Self::http_error)?;
        let status = response.status();
        let version = response
            .headers()
            .get("X-Jenkins")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "Jenkins API 返回 HTTP {}",
                status.as_u16()
            )));
        }
        let body = response.text().await.map_err(Self::http_error)?;
        let value: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
        let capabilities = json!({
            "mode": value.get("mode").and_then(Value::as_str).unwrap_or_default(),
            "nodeDescription": value.get("nodeDescription").and_then(Value::as_str).unwrap_or_default(),
            "useSecurity": value.get("useSecurity").and_then(Value::as_bool).unwrap_or(false),
            "hasJobs": value.get("jobs").and_then(Value::as_array).map(|jobs| !jobs.is_empty()).unwrap_or(false),
            "api": "remote-access-json"
        })
        .to_string();
        Ok(JenkinsProbeResult {
            version,
            capabilities,
            credential_display_name: credential.display_name,
            username_masked: Self::mask_username(username),
        })
    }

    async fn fetch_jobs(
        db: &Database,
        connection: &JenkinsConnection,
        input: &ListJenkinsJobsInput,
    ) -> Result<Vec<JenkinsJob>, AppError> {
        let api_url = Self::jobs_api_url(connection, input)?;
        let depth = input.depth.unwrap_or(3).clamp(1, 5) as usize;
        let tree = format!("jobs[{}]", Self::job_tree_spec(depth));
        let value = Self::jenkins_get_json(db, connection, &api_url, Some(&tree)).await?;
        let jobs = value
            .get("jobs")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| Self::map_job_tree(item, 0, depth))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(jobs)
    }

    async fn fetch_builds(
        db: &Database,
        connection: &JenkinsConnection,
        input: &ListJenkinsBuildsInput,
    ) -> Result<Vec<JenkinsBuild>, AppError> {
        let job_full_name = input
            .job_full_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("Job 名称不能为空".into()))?;
        let limit = input.limit.unwrap_or(30).clamp(1, 100);
        let offset = input
            .cursor
            .as_deref()
            .and_then(|value| value.trim().parse::<i64>().ok())
            .or(input.offset)
            .unwrap_or(0)
            .max(0);
        let api_url = Self::job_api_url(connection, job_full_name);
        let tree = format!(
            "builds[number,result,building,timestamp,duration,url,description,actions[causes[shortDescription,userName]]]{{{},{}}}",
            offset, limit
        );
        let value = Self::jenkins_get_json(db, connection, &api_url, Some(&tree)).await?;
        let builds = value
            .get("builds")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| Self::map_build(connection, job_full_name, item))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(builds)
    }

    async fn fetch_build_detail(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
        build_number: i64,
    ) -> Result<JenkinsBuild, AppError> {
        if build_number <= 0 {
            return Err(AppError::InvalidInput("构建号必须大于 0".into()));
        }
        let api_url = Self::build_api_url(connection, job_full_name, build_number);
        let tree = "number,result,building,timestamp,duration,url,description,actions[causes[shortDescription,userName]]";
        let value = Self::jenkins_get_json(db, connection, &api_url, Some(tree)).await?;
        Self::map_build(connection, job_full_name, &value)
            .ok_or_else(|| AppError::NotFound("Jenkins 构建详情不存在".into()))
    }

    async fn fetch_parameters(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
    ) -> Result<Vec<crate::models::JenkinsParameterDefinition>, AppError> {
        let job_full_name = job_full_name.trim();
        if job_full_name.is_empty() {
            return Err(AppError::InvalidInput("Job 名称不能为空".into()));
        }
        let api_url = Self::job_api_url(connection, job_full_name);
        let tree =
            "property[parameterDefinitions[name,type,description,defaultValue,choices,_class]]";
        let value = Self::jenkins_get_json(db, connection, &api_url, Some(tree)).await?;
        Ok(Self::map_parameter_definitions(&value))
    }

    fn parameter_cache_key(connection: &JenkinsConnection, job_full_name: &str) -> String {
        format!(
            "{}:{}:{}",
            connection.connection_key,
            connection.config_version,
            job_full_name.trim()
        )
    }

    fn find_job_in_tree<'a>(jobs: &'a [JenkinsJob], job_full_name: &str) -> Option<&'a JenkinsJob> {
        let target = job_full_name.trim();
        for job in jobs {
            if job.job_full_name == target {
                return Some(job);
            }
            if let Some(found) = Self::find_job_in_tree(&job.children, target) {
                return Some(found);
            }
        }
        None
    }

    fn apply_job_favorites(jobs: &mut [JenkinsJob], favorites: &HashSet<String>) {
        for job in jobs {
            job.favorite = favorites.contains(&job.job_full_name);
            Self::apply_job_favorites(&mut job.children, favorites);
        }
    }

    fn get_cached_parameters(
        cache_key: &str,
        connection: &JenkinsConnection,
        job_full_name: &str,
    ) -> Result<Option<JenkinsParameterDefinitionsResult>, AppError> {
        let cache = JENKINS_PARAMETER_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut cache = cache
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?;
        let Some(entry) = cache.get(cache_key).cloned() else {
            return Ok(None);
        };
        if entry.inserted_at.elapsed()
            > Duration::from_secs(JENKINS_PARAMETER_CACHE_TTL_SECS as u64)
        {
            cache.remove(cache_key);
            return Ok(None);
        }
        Ok(Some(JenkinsParameterDefinitionsResult {
            connection_key: connection.connection_key.clone(),
            job_full_name: job_full_name.trim().to_string(),
            parameter_definition_hash: entry.parameter_definition_hash,
            parameters: entry.parameters,
            from_cache: true,
            ttl_seconds: JENKINS_PARAMETER_CACHE_TTL_SECS,
            cached_at: entry.cached_at,
            expires_at: entry.expires_at,
        }))
    }

    fn cache_parameters(
        cache_key: &str,
        connection: &JenkinsConnection,
        job_full_name: &str,
        parameters: Vec<JenkinsParameterDefinition>,
    ) -> Result<JenkinsParameterDefinitionsResult, AppError> {
        let result = Self::build_parameter_result(connection, job_full_name, parameters, false)?;
        let entry = JenkinsParameterCacheEntry {
            parameters: result.parameters.clone(),
            parameter_definition_hash: result.parameter_definition_hash.clone(),
            inserted_at: Instant::now(),
            cached_at: result.cached_at.clone(),
            expires_at: result.expires_at.clone(),
        };
        let cache = JENKINS_PARAMETER_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        cache
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?
            .insert(cache_key.to_string(), entry);
        Ok(result)
    }

    fn build_parameter_result(
        connection: &JenkinsConnection,
        job_full_name: &str,
        parameters: Vec<JenkinsParameterDefinition>,
        from_cache: bool,
    ) -> Result<JenkinsParameterDefinitionsResult, AppError> {
        let cached_at = chrono::Utc::now();
        let expires_at = cached_at + chrono::Duration::seconds(JENKINS_PARAMETER_CACHE_TTL_SECS);
        Ok(JenkinsParameterDefinitionsResult {
            connection_key: connection.connection_key.clone(),
            job_full_name: job_full_name.trim().to_string(),
            parameter_definition_hash: Self::parameter_definition_hash(&parameters)?,
            parameters,
            from_cache,
            ttl_seconds: JENKINS_PARAMETER_CACHE_TTL_SECS,
            cached_at: cached_at.to_rfc3339(),
            expires_at: expires_at.to_rfc3339(),
        })
    }

    fn parameter_definition_hash(
        parameters: &[JenkinsParameterDefinition],
    ) -> Result<String, AppError> {
        let bytes = serde_json::to_vec(parameters)
            .map_err(|error| AppError::Custom(format!("参数定义序列化失败: {}", error)))?;
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }

    async fn fetch_artifacts(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
        build_number: i64,
    ) -> Result<Vec<JenkinsArtifact>, AppError> {
        if build_number <= 0 {
            return Err(AppError::InvalidInput("构建号必须大于 0".into()));
        }
        let api_url = Self::build_api_url(connection, job_full_name, build_number);
        let value = Self::jenkins_get_json(
            db,
            connection,
            &api_url,
            Some("artifacts[fileName,relativePath]"),
        )
        .await?;
        Ok(value
            .get("artifacts")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let file_name = item
                            .get("fileName")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .trim();
                        let relative_path = item
                            .get("relativePath")
                            .and_then(Value::as_str)
                            .unwrap_or(file_name)
                            .trim();
                        if file_name.is_empty() || relative_path.is_empty() {
                            return None;
                        }
                        Some(JenkinsArtifact {
                            id: 0,
                            artifact_key: Self::artifact_key(
                                &connection.connection_key,
                                job_full_name,
                                build_number,
                                relative_path,
                            ),
                            request_id: String::new(),
                            connection_key: connection.connection_key.clone(),
                            job_full_name: job_full_name.to_string(),
                            build_number,
                            file_name: file_name.to_string(),
                            relative_path: relative_path.to_string(),
                            local_path: String::new(),
                            size_bytes: None,
                            sha256: String::new(),
                            source_url: Self::artifact_download_url(
                                connection,
                                job_full_name,
                                build_number,
                                relative_path,
                            ),
                            status: "remote".into(),
                            risk_flags: Self::artifact_risk_flags(file_name),
                            downloaded_at: None,
                            cleaned_at: None,
                            created_at: String::new(),
                            updated_at: String::new(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default())
    }

    async fn jenkins_get_json(
        db: &Database,
        connection: &JenkinsConnection,
        url: &str,
        tree: Option<&str>,
    ) -> Result<Value, AppError> {
        if connection.credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "未配置 Jenkins API Token 安全凭证".into(),
            ));
        }
        let credential = db
            .get_secure_credential(&connection.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", connection.credential_key))
            })?;
        Self::validate_jenkins_credential(&credential)?;
        let token = SecureCredentialService::get_secret(db, &credential.credential_key)?;
        let username = credential.account_name.trim();
        if username.is_empty() {
            return Err(AppError::InvalidInput(
                "Jenkins 安全凭证需要填写 accountName 作为用户名".into(),
            ));
        }
        let target = Self::prepare_request_target(db, connection, url)?;
        let client = Self::http_client(connection.tls_verify)?;
        let mut request = client
            .get(&target.url)
            .header(USER_AGENT, "tauri-ssh")
            .header(ACCEPT, "application/json")
            .basic_auth(username, Some(token));
        if let Some(tree) = tree {
            request = request.query(&[("tree", tree)]);
        }
        let response = request.send().await.map_err(Self::http_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "Jenkins API 返回 HTTP {}",
                status.as_u16()
            )));
        }
        response.json::<Value>().await.map_err(Self::http_error)
    }

    async fn fetch_crumb(
        db: &Database,
        connection: &JenkinsConnection,
    ) -> Result<JenkinsCrumbCacheEntry, AppError> {
        let (username, token) = Self::resolve_jenkins_auth(db, connection)?;
        let url = Self::crumb_api_url(connection);
        let target = Self::prepare_request_target(db, connection, &url)?;
        let value = Self::http_client(connection.tls_verify)?
            .get(&target.url)
            .header(USER_AGENT, "tauri-ssh")
            .header(ACCEPT, "application/json")
            .basic_auth(username, Some(token))
            .send()
            .await
            .map_err(Self::http_error)?
            .error_for_status()
            .map_err(Self::http_error)?
            .json::<Value>()
            .await
            .map_err(Self::http_error)?;
        let field = value
            .get("crumbRequestField")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::Custom("Jenkins crumb 响应缺少 crumbRequestField".into()))?;
        let crumb = value
            .get("crumb")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::Custom("Jenkins crumb 响应缺少 crumb".into()))?;
        Ok(JenkinsCrumbCacheEntry {
            field: field.to_string(),
            value: crumb.to_string(),
            inserted_at: Instant::now(),
        })
    }

    async fn get_cached_or_fetch_crumb(
        db: &Database,
        connection: &JenkinsConnection,
    ) -> Result<JenkinsCrumbCacheEntry, AppError> {
        let key = Self::crumb_cache_key(connection);
        let cache = JENKINS_CRUMB_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(entry) = cache
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?
            .get(&key)
            .cloned()
            .filter(|entry| {
                entry.inserted_at.elapsed() < Duration::from_secs(JENKINS_CRUMB_CACHE_TTL_SECS)
            })
        {
            return Ok(entry);
        }
        let entry = Self::fetch_crumb(db, connection).await?;
        cache
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?
            .insert(key, entry.clone());
        Ok(entry)
    }

    fn clear_cached_crumb(connection: &JenkinsConnection) -> Result<(), AppError> {
        let cache = JENKINS_CRUMB_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        cache
            .lock()
            .map_err(|error| AppError::Custom(error.to_string()))?
            .remove(&Self::crumb_cache_key(connection));
        Ok(())
    }

    fn apply_crumb(
        request: reqwest::RequestBuilder,
        crumb: Option<&JenkinsCrumbCacheEntry>,
    ) -> reqwest::RequestBuilder {
        let Some(crumb) = crumb else {
            return request;
        };
        let Ok(name) = HeaderName::from_bytes(crumb.field.as_bytes()) else {
            return request;
        };
        let Ok(value) = HeaderValue::from_str(&crumb.value) else {
            return request;
        };
        request.header(name, value)
    }

    async fn optional_crumb(
        db: &Database,
        connection: &JenkinsConnection,
    ) -> Option<JenkinsCrumbCacheEntry> {
        Self::get_cached_or_fetch_crumb(db, connection).await.ok()
    }

    async fn retry_crumb_after_forbidden(
        db: &Database,
        connection: &JenkinsConnection,
    ) -> Result<JenkinsCrumbCacheEntry, AppError> {
        Self::clear_cached_crumb(connection)?;
        Self::get_cached_or_fetch_crumb(db, connection)
            .await
            .map_err(|error| AppError::Custom(format!("Jenkins CSRF crumb 获取失败: {}", error)))
    }

    async fn trigger_plain_build_once(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
        crumb: Option<&JenkinsCrumbCacheEntry>,
    ) -> Result<reqwest::Response, AppError> {
        let (username, token) = Self::resolve_jenkins_auth(db, connection)?;
        let url = Self::build_trigger_url(connection, job_full_name);
        let target = Self::prepare_request_target(db, connection, &url)?;
        let request = Self::http_client(connection.tls_verify)?
            .post(&target.url)
            .header(USER_AGENT, "tauri-ssh")
            .basic_auth(username, Some(token));
        Self::apply_crumb(request, crumb)
            .send()
            .await
            .map_err(Self::http_error)
    }

    async fn trigger_plain_build(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
    ) -> Result<(Option<String>, Option<String>), AppError> {
        let crumb = Self::optional_crumb(db, connection).await;
        let mut response =
            Self::trigger_plain_build_once(db, connection, job_full_name, crumb.as_ref()).await?;
        if response.status().as_u16() == 403 {
            let retry_crumb = Self::retry_crumb_after_forbidden(db, connection).await?;
            response =
                Self::trigger_plain_build_once(db, connection, job_full_name, Some(&retry_crumb))
                    .await?;
        }
        let status = response.status();
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if !(status.is_success() || status.is_redirection()) {
            return Err(AppError::Custom(format!(
                "Jenkins 构建触发返回 HTTP {}",
                status.as_u16()
            )));
        }
        let queue_id = location
            .as_deref()
            .and_then(Self::parse_queue_id_from_location);
        Ok((queue_id, location))
    }

    fn build_parameterized_request(
        client: reqwest::Client,
        target_url: &str,
        username: String,
        token: String,
        parameters: &[JenkinsBuildParameter],
        crumb: Option<&JenkinsCrumbCacheEntry>,
    ) -> Result<reqwest::RequestBuilder, AppError> {
        let has_file = parameters
            .iter()
            .any(|parameter| matches!(parameter, JenkinsBuildParameter::File { .. }));
        let mut request = client
            .post(target_url)
            .header(USER_AGENT, "tauri-ssh")
            .basic_auth(username, Some(token));
        request = Self::apply_crumb(request, crumb);
        if has_file {
            let mut form = multipart::Form::new();
            let mut parameter_specs = Vec::new();
            for parameter in parameters {
                match parameter {
                    JenkinsBuildParameter::Scalar { name, value } => {
                        form = form.text(name.clone(), value.clone());
                        parameter_specs.push(json!({"name": name, "value": value}));
                    }
                    JenkinsBuildParameter::File {
                        name,
                        file_name,
                        path,
                    } => {
                        let bytes = std::fs::read(path)?;
                        let part = multipart::Part::bytes(bytes).file_name(file_name.clone());
                        form = form.part(name.clone(), part);
                        parameter_specs.push(json!({
                            "name": name,
                            "file": name,
                            "filename": file_name
                        }));
                    }
                }
            }
            form = form.text("json", json!({ "parameter": parameter_specs }).to_string());
            Ok(request.multipart(form))
        } else {
            let scalar_parameters = parameters
                .iter()
                .filter_map(|parameter| match parameter {
                    JenkinsBuildParameter::Scalar { name, value } => {
                        Some((name.clone(), value.clone()))
                    }
                    JenkinsBuildParameter::File { .. } => None,
                })
                .collect::<Vec<_>>();
            Ok(request.form(&scalar_parameters))
        }
    }

    async fn trigger_parameterized_build_once(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
        parameters: &[JenkinsBuildParameter],
        crumb: Option<&JenkinsCrumbCacheEntry>,
    ) -> Result<reqwest::Response, AppError> {
        let (username, token) = Self::resolve_jenkins_auth(db, connection)?;
        let url = Self::build_with_parameters_url(connection, job_full_name);
        let target = Self::prepare_request_target(db, connection, &url)?;
        let request = Self::build_parameterized_request(
            Self::http_client(connection.tls_verify)?,
            &target.url,
            username,
            token,
            parameters,
            crumb,
        )?;
        request.send().await.map_err(Self::http_error)
    }

    async fn trigger_parameterized_build(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
        parameters: &[JenkinsBuildParameter],
    ) -> Result<(Option<String>, Option<String>), AppError> {
        if parameters.is_empty() {
            return Self::trigger_plain_build(db, connection, job_full_name).await;
        }
        let crumb = Self::optional_crumb(db, connection).await;
        let mut response = Self::trigger_parameterized_build_once(
            db,
            connection,
            job_full_name,
            parameters,
            crumb.as_ref(),
        )
        .await?;
        if response.status().as_u16() == 403 {
            let retry_crumb = Self::retry_crumb_after_forbidden(db, connection).await?;
            response = Self::trigger_parameterized_build_once(
                db,
                connection,
                job_full_name,
                parameters,
                Some(&retry_crumb),
            )
            .await?;
        }
        let status = response.status();
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if !(status.is_success() || status.is_redirection()) {
            return Err(AppError::Custom(format!(
                "Jenkins 参数构建触发返回 HTTP {}",
                status.as_u16()
            )));
        }
        let queue_id = location
            .as_deref()
            .and_then(Self::parse_queue_id_from_location);
        Ok((queue_id, location))
    }

    async fn stop_build_once(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
        build_number: i64,
        crumb: Option<&JenkinsCrumbCacheEntry>,
    ) -> Result<reqwest::Response, AppError> {
        let (username, token) = Self::resolve_jenkins_auth(db, connection)?;
        let url = Self::build_stop_url(connection, job_full_name, build_number);
        let target = Self::prepare_request_target(db, connection, &url)?;
        let request = Self::http_client(connection.tls_verify)?
            .post(&target.url)
            .header(USER_AGENT, "tauri-ssh")
            .basic_auth(username, Some(token));
        Self::apply_crumb(request, crumb)
            .send()
            .await
            .map_err(Self::http_error)
    }

    async fn stop_build(
        db: &Database,
        connection: &JenkinsConnection,
        job_full_name: &str,
        build_number: i64,
    ) -> Result<(), AppError> {
        let crumb = Self::optional_crumb(db, connection).await;
        let mut response =
            Self::stop_build_once(db, connection, job_full_name, build_number, crumb.as_ref())
                .await?;
        if response.status().as_u16() == 403 {
            let retry_crumb = Self::retry_crumb_after_forbidden(db, connection).await?;
            response = Self::stop_build_once(
                db,
                connection,
                job_full_name,
                build_number,
                Some(&retry_crumb),
            )
            .await?;
        }
        let status = response.status();
        if !(status.is_success() || status.is_redirection()) {
            return Err(AppError::Custom(format!(
                "Jenkins 停止构建返回 HTTP {}",
                status.as_u16()
            )));
        }
        Ok(())
    }

    fn mark_build_stop_requested(
        db: &Database,
        context: &JenkinsBuildStopApprovalContext,
    ) -> Result<(), AppError> {
        if let Some(mut run) = db.get_jenkins_build_run_by_number(
            &context.connection.connection_key,
            &context.job_full_name,
            context.build_number,
        )? {
            run.status = "stop_requested".into();
            run.status_source = "local".into();
            run.result = String::new();
            run.last_error_code.clear();
            run.last_error_message = if context.approval_id > 0 {
                format!("停止构建请求已由 {} 发起", context.requester)
            } else {
                format!("停止构建请求已由 {} 按无需审批策略发起", context.requester)
            };
            db.upsert_jenkins_build_run(
                &run,
                (context.approval_id > 0).then_some(context.approval_id),
                context.connection.config_version,
                &context.request_hash,
                "{}",
            )?;
        }
        Ok(())
    }

    fn resolve_jenkins_auth(
        db: &Database,
        connection: &JenkinsConnection,
    ) -> Result<(String, String), AppError> {
        if connection.credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "未配置 Jenkins API Token 安全凭证".into(),
            ));
        }
        let credential = db
            .get_secure_credential(&connection.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", connection.credential_key))
            })?;
        Self::validate_jenkins_credential(&credential)?;
        let token = SecureCredentialService::get_secret(db, &credential.credential_key)?;
        let username = credential.account_name.trim();
        if username.is_empty() {
            return Err(AppError::InvalidInput(
                "Jenkins 安全凭证需要填写 accountName 作为用户名".into(),
            ));
        }
        Ok((username.to_string(), token))
    }

    async fn jenkins_get_text(
        db: &Database,
        connection: &JenkinsConnection,
        url: &str,
        query: &[(&str, String)],
    ) -> Result<(String, reqwest::header::HeaderMap), AppError> {
        if connection.credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "未配置 Jenkins API Token 安全凭证".into(),
            ));
        }
        let credential = db
            .get_secure_credential(&connection.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", connection.credential_key))
            })?;
        Self::validate_jenkins_credential(&credential)?;
        let token = SecureCredentialService::get_secret(db, &credential.credential_key)?;
        let username = credential.account_name.trim();
        if username.is_empty() {
            return Err(AppError::InvalidInput(
                "Jenkins 安全凭证需要填写 accountName 作为用户名".into(),
            ));
        }
        let target = Self::prepare_request_target(db, connection, url)?;
        let response = Self::http_client(connection.tls_verify)?
            .get(&target.url)
            .query(query)
            .header(USER_AGENT, "tauri-ssh")
            .basic_auth(username, Some(token))
            .send()
            .await
            .map_err(Self::http_error)?;
        let status = response.status();
        let headers = response.headers().clone();
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "Jenkins API 返回 HTTP {}",
                status.as_u16()
            )));
        }
        let mut text = response.text().await.map_err(Self::http_error)?;
        if text.len() > 131_072 {
            text.truncate(131_072);
        }
        Ok((text, headers))
    }

    async fn jenkins_get_stream(
        db: &Database,
        connection: &JenkinsConnection,
        url: &str,
    ) -> Result<JenkinsStreamResponse, AppError> {
        if connection.credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "未配置 Jenkins API Token 安全凭证".into(),
            ));
        }
        let credential = db
            .get_secure_credential(&connection.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", connection.credential_key))
            })?;
        Self::validate_jenkins_credential(&credential)?;
        let token = SecureCredentialService::get_secret(db, &credential.credential_key)?;
        let username = credential.account_name.trim();
        if username.is_empty() {
            return Err(AppError::InvalidInput(
                "Jenkins 安全凭证需要填写 accountName 作为用户名".into(),
            ));
        }
        let target = Self::prepare_request_target(db, connection, url)?;
        let response = Self::http_client(connection.tls_verify)?
            .get(&target.url)
            .header(USER_AGENT, "tauri-ssh")
            .basic_auth(username, Some(token))
            .send()
            .await
            .map_err(Self::http_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "Jenkins API 返回 HTTP {}",
                status.as_u16()
            )));
        }
        Ok(JenkinsStreamResponse {
            response,
            _tunnel: target._tunnel,
        })
    }

    fn prepare_request_target(
        db: &Database,
        connection: &JenkinsConnection,
        url: &str,
    ) -> Result<JenkinsRequestTarget, AppError> {
        let ssh_server_alias = connection.ssh_server_alias.trim();
        if ssh_server_alias.is_empty() {
            return Ok(JenkinsRequestTarget {
                url: url.to_string(),
                _tunnel: None,
            });
        }

        let (parsed, remote_host, remote_port) =
            Self::parse_tunnel_remote(url, connection.tls_verify)?;
        let tunnel = Self::start_ssh_tunnel(db, ssh_server_alias, &remote_host, remote_port)?;
        Ok(JenkinsRequestTarget {
            url: Self::rewrite_tunnel_url(parsed, tunnel.local_port)?,
            _tunnel: Some(tunnel),
        })
    }

    fn parse_tunnel_remote(url: &str, tls_verify: bool) -> Result<(Url, String, u16), AppError> {
        let parsed = Url::parse(url).map_err(|error| {
            AppError::InvalidInput(format!(
                "Jenkins URL 解析失败，无法创建 SSH 隧道: {}",
                error
            ))
        })?;
        if parsed.scheme() == "https" && tls_verify {
            return Err(AppError::InvalidInput(
                "HTTPS Jenkins 通过本机 SSH 隧道访问时会发生证书主机名不匹配；请关闭该 Jenkins 连接的 TLS 校验，或改用 HTTP 内网地址"
                    .into(),
            ));
        }
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(AppError::InvalidInput(
                "Jenkins SSH 隧道只支持 HTTP/HTTPS URL".into(),
            ));
        }
        let remote_host = parsed
            .host_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::InvalidInput("Jenkins URL 缺少主机名".into()))?
            .to_string();
        let remote_port = parsed
            .port_or_known_default()
            .ok_or_else(|| AppError::InvalidInput("Jenkins URL 缺少端口信息".into()))?;
        Ok((parsed, remote_host, remote_port))
    }

    fn rewrite_tunnel_url(mut parsed: Url, local_port: u16) -> Result<String, AppError> {
        parsed
            .set_host(Some("127.0.0.1"))
            .map_err(|_| AppError::InvalidInput("Jenkins 隧道本机地址设置失败".into()))?;
        parsed
            .set_port(Some(local_port))
            .map_err(|_| AppError::InvalidInput("Jenkins 隧道本机端口设置失败".into()))?;
        Ok(parsed.to_string())
    }

    fn start_ssh_tunnel(
        db: &Database,
        ssh_server_alias: &str,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<SshTunnelGuard, AppError> {
        let (_, session) = TerminalService::connect_saved_server(db, ssh_server_alias, 15)?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| {
            AppError::Custom(format!("Jenkins SSH 隧道本机端口创建失败: {}", error))
        })?;
        listener.set_nonblocking(true).map_err(|error| {
            AppError::Custom(format!("Jenkins SSH 隧道监听配置失败: {}", error))
        })?;
        let local_port = listener
            .local_addr()
            .map_err(|error| AppError::Custom(format!("Jenkins SSH 隧道端口读取失败: {}", error)))?
            .port();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let remote_host = remote_host.to_string();
        thread::spawn(move || {
            let started = Instant::now();
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Err(error) = Self::forward_ssh_tunnel_connection(
                            session,
                            stream,
                            remote_host,
                            remote_port,
                        ) {
                            log::warn!("Jenkins SSH 隧道转发失败: {}", error);
                        }
                        break;
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        if started.elapsed() > Duration::from_secs(30) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(error) => {
                        log::warn!("Jenkins SSH 隧道监听失败: {}", error);
                        break;
                    }
                }
            }
        });
        Ok(SshTunnelGuard {
            shutdown_tx,
            local_port,
        })
    }

    fn forward_ssh_tunnel_connection(
        session: Session,
        mut local_stream: TcpStream,
        remote_host: String,
        remote_port: u16,
    ) -> Result<(), AppError> {
        local_stream.set_nonblocking(true).map_err(|error| {
            AppError::Custom(format!("Jenkins SSH 隧道本地连接配置失败: {}", error))
        })?;
        session.set_blocking(false);
        let mut channel = session
            .channel_direct_tcpip(&remote_host, remote_port, None)
            .map_err(|error| {
                AppError::Custom(format!(
                    "Jenkins SSH 隧道连接 {}:{} 失败: {}",
                    remote_host, remote_port, error
                ))
            })?;
        let mut local_buf = [0u8; 16 * 1024];
        let mut remote_buf = [0u8; 16 * 1024];
        let mut local_closed = false;
        let started = Instant::now();

        loop {
            let mut progressed = false;
            if !local_closed {
                match local_stream.read(&mut local_buf) {
                    Ok(0) => {
                        local_closed = true;
                        let _ = channel.send_eof();
                    }
                    Ok(size) => {
                        Self::write_all_ssh_channel(&mut channel, &local_buf[..size])?;
                        progressed = true;
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                    Err(error) => {
                        return Err(AppError::Custom(format!(
                            "Jenkins SSH 隧道读取本地请求失败: {}",
                            error
                        )));
                    }
                }
            }

            match channel.read(&mut remote_buf) {
                Ok(0) => {
                    if channel.eof() {
                        break;
                    }
                }
                Ok(size) => {
                    Self::write_all_tcp_stream(&mut local_stream, &remote_buf[..size])?;
                    progressed = true;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => {
                    return Err(AppError::Custom(format!(
                        "Jenkins SSH 隧道读取远端响应失败: {}",
                        error
                    )));
                }
            }

            if channel.eof() {
                break;
            }
            if !progressed {
                if started.elapsed() > Duration::from_secs(300) {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
        let _ = channel.close();
        let _ = local_stream.shutdown(Shutdown::Both);
        Ok(())
    }

    fn write_all_ssh_channel(
        channel: &mut ssh2::Channel,
        mut buffer: &[u8],
    ) -> Result<(), AppError> {
        while !buffer.is_empty() {
            match channel.write(buffer) {
                Ok(0) => thread::sleep(Duration::from_millis(5)),
                Ok(size) => buffer = &buffer[size..],
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => {
                    return Err(AppError::Custom(format!(
                        "Jenkins SSH 隧道写入远端失败: {}",
                        error
                    )));
                }
            }
        }
        channel
            .flush()
            .map_err(|error| AppError::Custom(format!("Jenkins SSH 隧道刷新远端失败: {}", error)))
    }

    fn write_all_tcp_stream(stream: &mut TcpStream, mut buffer: &[u8]) -> Result<(), AppError> {
        while !buffer.is_empty() {
            match stream.write(buffer) {
                Ok(0) => thread::sleep(Duration::from_millis(5)),
                Ok(size) => buffer = &buffer[size..],
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => {
                    return Err(AppError::Custom(format!(
                        "Jenkins SSH 隧道写入本地响应失败: {}",
                        error
                    )));
                }
            }
        }
        stream.flush().map_err(|error| {
            AppError::Custom(format!("Jenkins SSH 隧道刷新本地响应失败: {}", error))
        })
    }

    fn jobs_api_url(
        connection: &JenkinsConnection,
        input: &ListJenkinsJobsInput,
    ) -> Result<String, AppError> {
        let mut url = connection.base_url.trim_end_matches('/').to_string();
        if let Some(folder) = input
            .folder
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            for part in folder
                .split('/')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                url.push_str("/job/");
                url.push_str(&percent_encode(part));
            }
        } else if let Some(view) = input
            .view_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            url.push_str("/view/");
            url.push_str(&percent_encode(view));
        }
        url.push_str("/api/json");
        Ok(url)
    }

    fn job_api_url(connection: &JenkinsConnection, job_full_name: &str) -> String {
        let mut url = connection.base_url.trim_end_matches('/').to_string();
        for part in job_full_name
            .split('/')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            url.push_str("/job/");
            url.push_str(&percent_encode(part));
        }
        url.push_str("/api/json");
        url
    }

    fn build_api_url(
        connection: &JenkinsConnection,
        job_full_name: &str,
        build_number: i64,
    ) -> String {
        let mut url = Self::job_api_url(connection, job_full_name);
        url.truncate(url.len().saturating_sub("/api/json".len()));
        url.push('/');
        url.push_str(&build_number.to_string());
        url.push_str("/api/json");
        url
    }

    fn build_progressive_log_url(
        connection: &JenkinsConnection,
        job_full_name: &str,
        build_number: i64,
    ) -> String {
        let mut url = Self::job_api_url(connection, job_full_name);
        url.truncate(url.len().saturating_sub("/api/json".len()));
        url.push('/');
        url.push_str(&build_number.to_string());
        url.push_str("/logText/progressiveText");
        url
    }

    fn build_trigger_url(connection: &JenkinsConnection, job_full_name: &str) -> String {
        let mut url = Self::job_api_url(connection, job_full_name);
        url.truncate(url.len().saturating_sub("/api/json".len()));
        url.push_str("/build");
        url
    }

    fn build_with_parameters_url(connection: &JenkinsConnection, job_full_name: &str) -> String {
        let mut url = Self::job_api_url(connection, job_full_name);
        url.truncate(url.len().saturating_sub("/api/json".len()));
        url.push_str("/buildWithParameters");
        url
    }

    fn build_stop_url(
        connection: &JenkinsConnection,
        job_full_name: &str,
        build_number: i64,
    ) -> String {
        let mut url = Self::job_api_url(connection, job_full_name);
        url.truncate(url.len().saturating_sub("/api/json".len()));
        url.push('/');
        url.push_str(&build_number.to_string());
        url.push_str("/stop");
        url
    }

    fn crumb_api_url(connection: &JenkinsConnection) -> String {
        format!(
            "{}/crumbIssuer/api/json",
            connection.base_url.trim_end_matches('/')
        )
    }

    fn crumb_cache_key(connection: &JenkinsConnection) -> String {
        format!(
            "{}:{}:{}:{}",
            connection.connection_key,
            connection.credential_key,
            connection.base_url.trim_end_matches('/'),
            connection.config_version
        )
    }

    fn queue_api_url(connection: &JenkinsConnection) -> String {
        format!(
            "{}/queue/api/json",
            connection.base_url.trim_end_matches('/')
        )
    }

    fn queue_item_api_url(connection: &JenkinsConnection, queue_id: &str) -> String {
        format!(
            "{}/queue/item/{}/api/json",
            connection.base_url.trim_end_matches('/'),
            percent_encode(queue_id.trim())
        )
    }

    fn parse_queue_id_from_location(location: &str) -> Option<String> {
        let trimmed = location.trim().trim_end_matches('/');
        let last = trimmed.rsplit('/').next()?.trim();
        if last.chars().all(|ch| ch.is_ascii_digit()) {
            Some(last.to_string())
        } else {
            None
        }
    }

    fn millis_to_rfc3339(millis: i64) -> String {
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)
            .map(|time| time.to_rfc3339())
            .unwrap_or_default()
    }

    fn artifact_download_url(
        connection: &JenkinsConnection,
        job_full_name: &str,
        build_number: i64,
        relative_path: &str,
    ) -> String {
        let mut url = Self::job_api_url(connection, job_full_name);
        url.truncate(url.len().saturating_sub("/api/json".len()));
        url.push('/');
        url.push_str(&build_number.to_string());
        url.push_str("/artifact");
        for part in relative_path.split('/') {
            if !part.trim().is_empty() {
                url.push('/');
                url.push_str(&percent_encode(part.trim()));
            }
        }
        url
    }

    fn artifact_key(
        connection_key: &str,
        job_full_name: &str,
        build_number: i64,
        relative_path: &str,
    ) -> String {
        let hash = Sha256::digest(relative_path.as_bytes());
        format!(
            "jenkins-artifact-{}-{}-{}-{:x}",
            sanitize_key_segment(connection_key),
            sanitize_key_segment(job_full_name),
            build_number,
            hash
        )
    }

    fn artifact_risk_flags(file_name: &str) -> Vec<String> {
        let lower = file_name.to_ascii_lowercase();
        let high_risk_ext = [
            ".sh", ".bat", ".cmd", ".ps1", ".exe", ".dmg", ".pkg", ".jar",
        ];
        if high_risk_ext.iter().any(|ext| lower.ends_with(ext)) {
            vec!["executable_or_installer".into()]
        } else {
            Vec::new()
        }
    }

    fn deployment_recipe_for_artifact(file_name: &str) -> String {
        let lower = file_name.to_ascii_lowercase();
        if lower.ends_with(".jar") {
            "systemd-binary".into()
        } else if lower.ends_with(".zip")
            || lower.ends_with(".tar")
            || lower.ends_with(".tar.gz")
            || lower.ends_with(".tgz")
        {
            "static-openresty".into()
        } else {
            "custom-script".into()
        }
    }

    fn deployment_candidate_warnings(artifact: &JenkinsArtifact) -> Vec<String> {
        let mut warnings = vec![
            "候选仅引用应用托管目录中的 Jenkins artifact；真实部署仍需创建部署目标并走审批链路。"
                .into(),
        ];
        if !artifact.risk_flags.is_empty() {
            warnings.push("该 artifact 带有高风险类型标记，部署前必须重新确认执行策略。".into());
        }
        warnings
    }

    fn deployment_candidate_from_artifact(artifact: &JenkinsArtifact) -> DeploymentCandidate {
        let recipe = Self::deployment_recipe_for_artifact(&artifact.file_name);
        let local_path = PathBuf::from(artifact.local_path.trim());
        let workdir = local_path
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| artifact.local_path.clone());
        DeploymentCandidate {
            key: format!(
                "jenkins-artifact-{}-{}-{}-{}",
                sanitize_key_segment(&artifact.connection_key),
                sanitize_key_segment(&artifact.job_full_name),
                artifact.build_number,
                sanitize_key_segment(&artifact.file_name)
            ),
            name: format!(
                "Jenkins {} #{} {}",
                artifact.job_full_name, artifact.build_number, artifact.file_name
            ),
            recipe,
            confidence: 70,
            source_type: "local".into(),
            workdir,
            build_command: String::new(),
            start_command: String::new(),
            artifact_dir: artifact.local_path.clone(),
            dockerfile: String::new(),
            compose_file: String::new(),
            exposed_ports: Vec::new(),
            env_files: Vec::new(),
            detected_frameworks: vec!["jenkins-artifact".into()],
            warnings: Self::deployment_candidate_warnings(artifact),
            config_json: json!({
                "source": "jenkins-artifact",
                "artifactKey": artifact.artifact_key,
                "requestId": artifact.request_id,
                "connectionKey": artifact.connection_key,
                "jobFullName": artifact.job_full_name,
                "buildNumber": artifact.build_number,
                "relativePath": artifact.relative_path,
                "fileName": artifact.file_name,
                "localPath": artifact.local_path,
                "sizeBytes": artifact.size_bytes,
                "sha256": artifact.sha256,
                "riskFlags": artifact.risk_flags,
                "status": artifact.status
            })
            .to_string(),
        }
    }

    fn deployment_target_from_candidate(
        candidate: &DeploymentCandidate,
        server_alias: &str,
        deploy_root: &str,
        domain: String,
        https_enabled: bool,
        port: Option<i64>,
        health_check_url: String,
    ) -> DeploymentTarget {
        DeploymentTarget {
            id: 0,
            target_key: candidate.key.clone(),
            name: candidate.name.clone(),
            server_alias: server_alias.into(),
            recipe: candidate.recipe.clone(),
            source_type: candidate.source_type.clone(),
            project_path: candidate.artifact_dir.clone(),
            git_url: String::new(),
            git_ref: String::new(),
            git_credential_key: String::new(),
            docker_build_mode: "remote".into(),
            workdir: candidate.workdir.clone(),
            deploy_root: deploy_root.into(),
            domain,
            https_enabled,
            port,
            health_check_url,
            config_json: candidate.config_json.clone(),
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn merge_deployment_dry_run_config(
        config_json: &str,
        build: &JenkinsBuild,
        artifact: &JenkinsArtifact,
        connection: &JenkinsConnection,
    ) -> Result<String, AppError> {
        let mut value = serde_json::from_str::<Value>(config_json).map_err(|error| {
            AppError::InvalidInput(format!("部署候选配置 JSON 无效: {}", error))
        })?;
        if let Some(object) = value.as_object_mut() {
            object.insert("deploymentProfile".into(), json!("jenkins-artifact"));
            object.insert(
                "jenkinsBuild".into(),
                json!({
                    "runKey": build.run_key,
                    "requestId": build.request_id,
                    "connectionName": connection.name,
                    "connectionKey": artifact.connection_key,
                    "jobFullName": artifact.job_full_name,
                    "buildNumber": artifact.build_number,
                    "result": build.result,
                    "status": build.status
                }),
            );
        }
        Ok(value.to_string())
    }

    fn is_successful_build(build: &JenkinsBuild) -> bool {
        build.result.eq_ignore_ascii_case("SUCCESS")
            || (build.status.eq_ignore_ascii_case("success") && build.result.trim().is_empty())
            || (build.status.eq_ignore_ascii_case("completed")
                && build.result.eq_ignore_ascii_case("SUCCESS"))
    }

    fn managed_artifact_root(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|error| AppError::Custom(error.to_string()))?
            .join("jenkins-artifacts");
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    fn validate_managed_artifact_path(root: &Path, local_path: &str) -> Result<PathBuf, AppError> {
        let path = PathBuf::from(local_path.trim());
        if path.as_os_str().is_empty() || !path.is_absolute() {
            return Err(AppError::InvalidInput(
                "artifact 本地路径必须是应用托管目录下的绝对路径".into(),
            ));
        }
        let normalized_root = normalize_absolute_path(root)?;
        let canonical_root = root.canonicalize()?;
        if path.exists() {
            let canonical_path = path.canonicalize()?;
            if !canonical_path.starts_with(&canonical_root) {
                return Err(AppError::InvalidInput(
                    "只能清理应用托管目录中的 Jenkins artifact".into(),
                ));
            }
            return Ok(canonical_path);
        }

        let normalized_path = normalize_absolute_path(&path)?;
        if !normalized_path.starts_with(&normalized_root)
            && !normalized_path.starts_with(&canonical_root)
        {
            return Err(AppError::InvalidInput(
                "只能清理应用托管目录中的 Jenkins artifact".into(),
            ));
        }
        Ok(normalized_path)
    }

    fn job_tree_spec(depth: usize) -> String {
        let base = "name,fullName,url,color,buildable,_class,lastBuild[number,result]";
        if depth <= 1 {
            return base.into();
        }
        format!("{},jobs[{}]", base, Self::job_tree_spec(depth - 1))
    }

    fn map_job_tree(value: &Value, depth: usize, max_depth: usize) -> Option<JenkinsJob> {
        let mut job = Self::map_job(value)?;
        let has_more = depth + 1 >= max_depth
            && value
                .get("jobs")
                .and_then(Value::as_array)
                .map(|children| !children.is_empty())
                .unwrap_or(false);
        job.has_more = has_more;
        if depth + 1 >= max_depth {
            return Some(job);
        }
        if let Some(children) = value.get("jobs").and_then(Value::as_array) {
            job.children = children
                .iter()
                .filter_map(|child| Self::map_job_tree(child, depth + 1, max_depth))
                .collect();
        }
        Some(job)
    }

    fn map_job(value: &Value) -> Option<JenkinsJob> {
        let full_name = value
            .get("fullName")
            .or_else(|| value.get("name"))
            .and_then(Value::as_str)?
            .to_string();
        let display_name = value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&full_name)
            .to_string();
        let raw_color = value
            .get("color")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let last_build = value.get("lastBuild");
        Some(JenkinsJob {
            job_full_name: full_name,
            display_name,
            url: value
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            job_type: Self::job_type_from_class(
                value
                    .get("_class")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            normalized_status: Self::normalize_job_status(&raw_color),
            raw_color,
            buildable: value
                .get("buildable")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            last_build_number: last_build
                .and_then(|item| item.get("number"))
                .and_then(Value::as_i64),
            last_build_status: last_build
                .and_then(|item| item.get("result"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase(),
            favorite: false,
            has_more: false,
            children: Vec::new(),
        })
    }

    fn map_queue_item(connection: &JenkinsConnection, value: &Value) -> Option<JenkinsQueueItem> {
        let queue_id = value
            .get("id")
            .and_then(Value::as_i64)
            .map(|id| id.to_string())
            .or_else(|| {
                value
                    .get("queueId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                value
                    .get("url")
                    .and_then(Value::as_str)
                    .and_then(Self::parse_queue_id_from_location)
            })?;
        let task = value.get("task").unwrap_or(&Value::Null);
        let executable = value.get("executable").unwrap_or(&Value::Null);
        let job_full_name = task
            .get("fullName")
            .or_else(|| task.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let build_number = executable.get("number").and_then(Value::as_i64);
        let executable_url = executable
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let status = if build_number.is_some() {
            "executable"
        } else if value
            .get("cancelled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "cancelled"
        } else if value
            .get("blocked")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            "blocked"
        } else if value.get("stuck").and_then(Value::as_bool).unwrap_or(false) {
            "stuck"
        } else {
            "waiting"
        }
        .to_string();
        let message = value
            .get("why")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let created_at = value
            .get("inQueueSince")
            .and_then(Value::as_i64)
            .map(Self::millis_to_rfc3339)
            .unwrap_or_default();

        Some(JenkinsQueueItem {
            queue_id,
            connection_key: connection.connection_key.clone(),
            job_full_name,
            build_number,
            executable_url,
            status,
            message,
            created_at,
        })
    }

    fn map_parameter_definitions(value: &Value) -> Vec<crate::models::JenkinsParameterDefinition> {
        value
            .get("property")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|property| {
                property
                    .get("parameterDefinitions")
                    .and_then(Value::as_array)
            })
            .flatten()
            .filter_map(Self::map_parameter_definition)
            .collect()
    }

    fn map_parameter_definition(
        value: &Value,
    ) -> Option<crate::models::JenkinsParameterDefinition> {
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if name.is_empty() {
            return None;
        }
        let raw_class = value
            .get("_class")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let raw_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let parameter_type = Self::normalize_parameter_type(&raw_class, raw_type);
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let description = value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let default_value = value.get("defaultValue").cloned().unwrap_or(Value::Null);
        let dynamic_parameter = Self::is_dynamic_parameter(&raw_class);
        let file_parameter = parameter_type == "file";
        let sensitive = parameter_type == "password" || Self::looks_sensitive_parameter_name(name);
        let unsupported = parameter_type == "unsupported";
        Some(crate::models::JenkinsParameterDefinition {
            name: name.to_string(),
            parameter_type,
            description,
            default_value,
            choices,
            required: false,
            sensitive,
            file_parameter,
            dynamic_parameter,
            unsupported,
            raw_class,
        })
    }

    fn normalize_parameter_type(raw_class: &str, raw_type: &str) -> String {
        let class = raw_class.to_ascii_lowercase();
        let raw_type = raw_type.to_ascii_lowercase();
        if class.contains("password") || raw_type.contains("password") {
            "password".into()
        } else if class.contains("fileparameterdefinition") || raw_type == "file" {
            "file".into()
        } else if class.contains("boolean") || raw_type == "boolean" || raw_type == "bool" {
            "boolean".into()
        } else if class.contains("choice") || raw_type == "choice" {
            "choice".into()
        } else if class.contains("string") || raw_type == "string" || raw_type == "text" {
            "string".into()
        } else if Self::is_dynamic_parameter(raw_class) {
            "string".into()
        } else {
            "unsupported".into()
        }
    }

    fn is_dynamic_parameter(raw_class: &str) -> bool {
        let class = raw_class.to_ascii_lowercase();
        class.contains("activechoice")
            || class.contains("active_choice")
            || class.contains("cascadechoice")
            || class.contains("dynamic")
            || class.contains("extendedchoice")
            || class.contains("gitparameter")
    }

    fn looks_sensitive_parameter_name(name: &str) -> bool {
        let value = name.to_ascii_lowercase();
        [
            "password",
            "passwd",
            "token",
            "secret",
            "credential",
            "api_key",
            "apikey",
            "access_key",
        ]
        .iter()
        .any(|marker| value.contains(marker))
    }

    fn map_build(
        connection: &JenkinsConnection,
        job_full_name: &str,
        value: &Value,
    ) -> Option<JenkinsBuild> {
        let build_number = value.get("number").and_then(Value::as_i64)?;
        let result = value
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let building = value
            .get("building")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let status = if building {
            "building".into()
        } else if result.is_empty() {
            "unknown".into()
        } else {
            Self::normalize_build_result(&result)
        };
        let started_at = value
            .get("timestamp")
            .and_then(Value::as_i64)
            .and_then(|millis| chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis))
            .map(|time| time.to_rfc3339())
            .unwrap_or_default();
        Some(JenkinsBuild {
            run_key: format!(
                "jenkins-{}-{}-{}",
                connection.connection_key,
                sanitize_key_segment(job_full_name),
                build_number
            ),
            request_id: String::new(),
            connection_key: connection.connection_key.clone(),
            job_full_name: job_full_name.to_string(),
            queue_id: String::new(),
            build_number: Some(build_number),
            status,
            status_source: "jenkins".into(),
            result,
            cause: Self::build_cause_summary(value),
            created_by: "jenkins".into(),
            created_at: started_at.clone(),
            updated_at: started_at.clone(),
            started_at: (!started_at.is_empty()).then_some(started_at),
            finished_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
        })
    }

    fn maybe_notify_build_completed(
        app: &tauri::AppHandle,
        connection: &JenkinsConnection,
        build: &JenkinsBuild,
    ) {
        let status = build.status.as_str();
        if !Self::should_notify_build_status(connection, status) {
            return;
        }
        let build_number = build.build_number.unwrap_or_default();
        if build_number <= 0 {
            return;
        }
        let notification_key = format!(
            "{}:{}:{}:{}",
            connection.connection_key, build.job_full_name, build_number, status
        );
        let sent = JENKINS_SENT_NOTIFICATIONS.get_or_init(|| Mutex::new(HashSet::new()));
        let Ok(mut sent) = sent.lock() else {
            return;
        };
        if !sent.insert(notification_key) {
            return;
        }
        drop(sent);

        let title = match status {
            "success" => "Jenkins 构建成功",
            "failure" => "Jenkins 构建失败",
            "unstable" => "Jenkins 构建不稳定",
            "aborted" => "Jenkins 构建已终止",
            _ => "Jenkins 构建已完成",
        };
        let body = format!(
            "{}：{} #{}",
            connection.name, build.job_full_name, build_number
        );
        let result = app.notification().builder().title(title).body(&body).show();
        if let Err(error) = result {
            log::warn!("Jenkins 构建完成通知发送失败: {}", error);
        }
    }

    fn should_notify_build_status(connection: &JenkinsConnection, status: &str) -> bool {
        match status {
            "success" => connection.notify_on_success,
            "failure" => connection.notify_on_failure,
            "unstable" => connection.notify_on_unstable,
            "aborted" => connection.notify_on_aborted,
            _ => false,
        }
    }

    fn normalize_build_result(result: &str) -> String {
        match result.trim().to_ascii_lowercase().as_str() {
            "success" => "success".into(),
            "failure" => "failure".into(),
            "unstable" => "unstable".into(),
            "aborted" => "aborted".into(),
            "not_built" | "notbuilt" => "not_built".into(),
            _ => "unknown".into(),
        }
    }

    fn build_cause_summary(value: &Value) -> String {
        value
            .get("actions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|action| action.get("causes").and_then(Value::as_array))
            .flatten()
            .find_map(|cause| {
                cause
                    .get("shortDescription")
                    .and_then(Value::as_str)
                    .map(Self::redact_error_message)
            })
            .unwrap_or_default()
    }

    fn validate_jenkins_credential(credential: &SecureCredential) -> Result<(), AppError> {
        if !credential.enabled {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 已禁用",
                credential.credential_key
            )));
        }
        if !credential.has_secret {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 未保存 API Token",
                credential.credential_key
            )));
        }
        if !["http_api", "custom"].contains(&credential.provider.as_str()) {
            return Err(AppError::InvalidInput(
                "Jenkins 连接只允许引用 http_api / custom 类型安全凭证".into(),
            ));
        }
        Ok(())
    }

    fn http_client(tls_verify: bool) -> Result<reqwest::Client, AppError> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .danger_accept_invalid_certs(!tls_verify)
            .build()
            .map_err(Self::http_error)
    }

    fn http_error(error: reqwest::Error) -> AppError {
        if error.is_timeout() {
            AppError::Custom("Jenkins 请求超时".into())
        } else if error.is_connect() {
            AppError::Custom(format!("Jenkins 连接失败: {}", error))
        } else {
            AppError::Custom(format!("Jenkins 请求失败: {}", error))
        }
    }

    fn jenkins_error_code(message: &str) -> &'static str {
        if message.contains("401") || message.contains("403") {
            "credential_failed"
        } else if message.contains("超时") {
            "timeout"
        } else if message.contains("连接失败") {
            "connect_failed"
        } else if message.contains("不存在")
            || message.contains("未保存")
            || message.contains("未配置")
        {
            "credential_missing"
        } else {
            "jenkins_test_failed"
        }
    }

    fn redact_error_message(message: &str) -> String {
        let mut value = message.replace('\n', " ");
        for marker in ["token", "Token", "Authorization", "Basic", "Bearer"] {
            value = value.replace(marker, "[REDACTED]");
        }
        if value.chars().count() > 500 {
            value.chars().take(500).collect()
        } else {
            value
        }
    }

    fn redact_log_text(text: &str) -> String {
        text.lines()
            .map(|line| {
                let mut output = line.to_string();
                for key in [
                    "password",
                    "passwd",
                    "token",
                    "api_key",
                    "apikey",
                    "secret",
                    "authorization",
                ] {
                    output = redact_key_value_like(&output, key);
                }
                output
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn mask_username(username: &str) -> String {
        let trimmed = username.trim();
        let chars = trimmed.chars().collect::<Vec<_>>();
        match chars.len() {
            0 => String::new(),
            1 | 2 => "*".repeat(chars.len()),
            _ => format!(
                "{}***{}",
                chars.first().unwrap_or(&'*'),
                chars.last().unwrap_or(&'*')
            ),
        }
    }

    fn normalize_job_status(color: &str) -> String {
        let value = color.trim().to_ascii_lowercase();
        if value.is_empty() || value == "notbuilt" {
            "not_built".into()
        } else if value.contains("anime") {
            "building".into()
        } else if value.starts_with("blue") || value.starts_with("green") {
            "success".into()
        } else if value.starts_with("red") {
            "failure".into()
        } else if value.starts_with("yellow") {
            "unstable".into()
        } else if value.starts_with("aborted") {
            "aborted".into()
        } else if value.starts_with("disabled") {
            "disabled".into()
        } else {
            "unknown".into()
        }
    }

    fn job_type_from_class(class_name: &str) -> String {
        let value = class_name.to_ascii_lowercase();
        if value.contains("folder") {
            "folder".into()
        } else if value.contains("workflowmultibranch") {
            "multibranch".into()
        } else if value.contains("workflowjob") {
            "pipeline".into()
        } else if value.contains("freestyleproject") {
            "freestyle".into()
        } else {
            "job".into()
        }
    }

    fn audit_connection(
        db: &Database,
        action: &str,
        connection: &JenkinsConnection,
        summary: &str,
    ) -> Result<(), AppError> {
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "jenkins".into(),
                server_alias: connection.ssh_server_alias.clone(),
                action: action.into(),
                risk: "L1".into(),
                result: "成功".into(),
                summary: format!("{}：{}", summary, connection.name),
                detail_json: Some(
                    json!({
                        "connectionKey": connection.connection_key,
                        "baseUrl": connection.base_url,
                        "environment": connection.environment,
                        "configVersion": connection.config_version
                    })
                    .to_string(),
                ),
                request_id: None,
                approval_id: None,
            },
        )?;
        Ok(())
    }

    fn audit_read(
        db: &Database,
        action: &str,
        connection: &JenkinsConnection,
        summary: &str,
        detail: Value,
        request_id: Option<String>,
    ) -> Result<(), AppError> {
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "jenkins".into(),
                server_alias: connection.ssh_server_alias.clone(),
                action: action.into(),
                risk: "readonly".into(),
                result: "成功".into(),
                summary: format!("{}：{}", summary, connection.name),
                detail_json: Some(detail.to_string()),
                request_id,
                approval_id: None,
            },
        )?;
        Ok(())
    }

    fn audit_build_trigger_execution(
        db: &Database,
        connection: Option<&JenkinsConnection>,
        approval_id: i64,
        request_hash: &str,
        job_full_name: &str,
        stage: &str,
        touched_jenkins: bool,
        result: &str,
        message: &str,
        extra: Option<Value>,
    ) -> Result<(), AppError> {
        let connection_key = connection
            .map(|value| value.connection_key.clone())
            .unwrap_or_default();
        let server_alias = connection
            .map(|value| value.ssh_server_alias.clone())
            .unwrap_or_default();
        let risk = connection
            .map(|value| {
                if value.environment == "prod" {
                    "L3"
                } else {
                    "L2"
                }
            })
            .unwrap_or("L2");
        let detail = json!({
            "approvalId": approval_id,
            "requestHash": request_hash,
            "connectionKey": connection_key,
            "jobFullName": job_full_name,
            "stage": stage,
            "touchedJenkins": touched_jenkins,
            "message": message,
            "extra": extra.unwrap_or_else(|| json!({}))
        });
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "jenkins".into(),
                server_alias,
                action: "jenkins.build.trigger.execute".into(),
                risk: risk.into(),
                result: result.into(),
                summary: format!("执行 Jenkins 构建触发：{}（{}）", job_full_name, result),
                detail_json: Some(detail.to_string()),
                request_id: Some(request_hash.to_string()),
                approval_id: Some(approval_id),
            },
        )?;
        Ok(())
    }

    fn audit_build_stop_execution(
        db: &Database,
        connection: Option<&JenkinsConnection>,
        approval_id: i64,
        request_hash: &str,
        job_full_name: &str,
        build_number: i64,
        stage: &str,
        result: &str,
        message: &str,
    ) -> Result<(), AppError> {
        let connection_key = connection
            .map(|value| value.connection_key.clone())
            .unwrap_or_default();
        let server_alias = connection
            .map(|value| value.ssh_server_alias.clone())
            .unwrap_or_default();
        let risk = connection
            .map(|value| {
                if value.environment == "prod" || Self::job_name_is_release_or_prod(job_full_name) {
                    "L3"
                } else {
                    "L2"
                }
            })
            .unwrap_or("L2");
        let detail = json!({
            "approvalId": approval_id,
            "requestHash": request_hash,
            "connectionKey": connection_key,
            "jobFullName": job_full_name,
            "buildNumber": build_number,
            "stage": stage,
            "touchedJenkins": stage == "jenkins_post",
            "message": message
        });
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "jenkins".into(),
                server_alias,
                action: "jenkins.build.stop.execute".into(),
                risk: risk.into(),
                result: result.into(),
                summary: format!(
                    "执行 Jenkins 停止构建：{} #{}（{}）",
                    job_full_name, build_number, result
                ),
                detail_json: Some(detail.to_string()),
                request_id: Some(request_hash.to_string()),
                approval_id: Some(approval_id),
            },
        )?;
        Ok(())
    }

    fn emit_build_status_event(app: &tauri::AppHandle, build: &JenkinsBuild) {
        let payload = JenkinsBuildStatusEvent {
            run_key: build.run_key.clone(),
            request_id: build.request_id.clone(),
            connection_key: build.connection_key.clone(),
            job_full_name: build.job_full_name.clone(),
            queue_id: build.queue_id.clone(),
            build_number: build.build_number,
            status: build.status.clone(),
            status_source: build.status_source.clone(),
            result: build.result.clone(),
            updated_at: build.updated_at.clone(),
        };
        if let Err(error) = app.emit("jenkins-build-status", payload) {
            log::warn!("Jenkins 构建状态事件推送失败: {}", error);
        }
    }

    fn audit_log_session(
        db: &Database,
        connection: &JenkinsConnection,
        request_id: &str,
        input: &JenkinsBuildLogInput,
        start: i64,
        next_start: i64,
        returned_bytes: i64,
        has_more: bool,
    ) -> Result<(), AppError> {
        let previous = Self::find_log_session_audit(db, request_id)?;
        let now = chrono::Utc::now().timestamp_millis();
        let (session_start, poll_count, total_bytes, first_seen_at) = previous
            .as_ref()
            .map(|detail| {
                (
                    detail
                        .get("startOffset")
                        .and_then(Value::as_i64)
                        .unwrap_or(start)
                        .min(start),
                    detail.get("pollCount").and_then(Value::as_i64).unwrap_or(0) + 1,
                    detail
                        .get("totalBytes")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        + returned_bytes.max(0),
                    detail
                        .get("firstSeenAt")
                        .and_then(Value::as_i64)
                        .unwrap_or(now),
                )
            })
            .unwrap_or((start, 1, returned_bytes.max(0), now));
        let duration_ms = now.saturating_sub(first_seen_at);
        AuditService::upsert_by_request_action(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "jenkins".into(),
                server_alias: connection.ssh_server_alias.clone(),
                action: "jenkins.build.log.progressive".into(),
                risk: "readonly".into(),
                result: "成功".into(),
                summary: format!(
                    "读取 Jenkins 构建日志会话：{} #{}，{} 次轮询",
                    input.job_full_name, input.build_number, poll_count
                ),
                detail_json: Some(
                    json!({
                        "connectionKey": connection.connection_key,
                        "jobFullName": input.job_full_name,
                        "buildNumber": input.build_number,
                        "viewer": "local-user",
                        "startOffset": session_start,
                        "endOffset": next_start.max(session_start),
                        "lastStartOffset": start,
                        "lastNextOffset": next_start,
                        "returnedBytes": returned_bytes.max(0),
                        "totalBytes": total_bytes,
                        "pollCount": poll_count,
                        "hasMore": has_more,
                        "truncated": false,
                        "redacted": true,
                        "durationMs": duration_ms,
                        "firstSeenAt": first_seen_at,
                        "lastSeenAt": now,
                        "contentStored": false
                    })
                    .to_string(),
                ),
                request_id: Some(request_id.to_string()),
                approval_id: None,
            },
        )?;
        Ok(())
    }

    fn find_log_session_audit(db: &Database, request_id: &str) -> Result<Option<Value>, AppError> {
        let rows = AuditService::list(
            db,
            crate::models::ListAuditLogsInput {
                actor: None,
                source: Some("jenkins".into()),
                server_alias: None,
                action: Some("jenkins.build.log.progressive".into()),
                risk: None,
                result: None,
                keyword: Some(request_id.to_string()),
                limit: Some(1),
            },
        )?;
        Ok(rows
            .into_iter()
            .find(|row| row.request_id == request_id)
            .and_then(|row| serde_json::from_str::<Value>(&row.detail_json).ok()))
    }

    fn audit_artifact_cleanup(
        db: &Database,
        artifact: &JenkinsArtifact,
        status: &str,
    ) -> Result<(), AppError> {
        AuditService::create(
            db,
            CreateAuditLogInput {
                actor: "local-user".into(),
                source: "jenkins".into(),
                server_alias: String::new(),
                action: "jenkins.artifact.cleanup".into(),
                risk: "L1".into(),
                result: "成功".into(),
                summary: format!(
                    "清理 Jenkins artifact 本地文件：{} #{} {}",
                    artifact.job_full_name, artifact.build_number, artifact.relative_path
                ),
                detail_json: Some(
                    json!({
                        "artifactKey": artifact.artifact_key,
                        "connectionKey": artifact.connection_key,
                        "jobFullName": artifact.job_full_name,
                        "buildNumber": artifact.build_number,
                        "relativePath": artifact.relative_path,
                        "status": status
                    })
                    .to_string(),
                ),
                request_id: Some(artifact.request_id.clone()),
                approval_id: None,
            },
        )?;
        Ok(())
    }

    fn new_connection_key() -> String {
        format!("jenkins-{}", chrono::Utc::now().timestamp_millis())
    }

    fn new_request_id() -> String {
        format!("jenkins-req-{}", chrono::Utc::now().timestamp_millis())
    }

    fn normalize_requester(requester: Option<&str>) -> String {
        requester
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("local-user")
            .to_string()
    }
}

impl JenkinsBuildTracker {
    fn record_observed_build(
        db: &Database,
        connection: &JenkinsConnection,
        observed: &JenkinsBuild,
    ) -> Result<JenkinsBuild, AppError> {
        let Some(build_number) = observed.build_number else {
            return Ok(observed.clone());
        };
        let mut build = observed.clone();
        if let Some(existing) = db.get_jenkins_build_run_by_number(
            &connection.connection_key,
            &observed.job_full_name,
            build_number,
        )? {
            build.run_key = existing.run_key;
            build.request_id = existing.request_id;
            build.queue_id = existing.queue_id;
            build.created_by = existing.created_by;
            build.created_at = if build.created_at.trim().is_empty() {
                existing.created_at
            } else {
                build.created_at
            };
        }
        db.upsert_jenkins_build_run(&build, None, connection.config_version, "", "{}")
    }

    fn record_triggered(
        db: &Database,
        context: &JenkinsBuildApprovalContext,
        result: &JenkinsBuildTriggerResult,
    ) -> Result<JenkinsBuild, AppError> {
        let run = JenkinsBuild {
            run_key: Self::run_key(&context.request_hash),
            request_id: context.request_hash.clone(),
            connection_key: context.connection.connection_key.clone(),
            job_full_name: context.job_full_name.clone(),
            queue_id: result.queue_id.clone().unwrap_or_default(),
            build_number: None,
            status: if result.queue_id.is_some() {
                "queued".into()
            } else {
                "triggered".into()
            },
            status_source: "local".into(),
            result: String::new(),
            cause: if context.approval_id > 0 {
                "Tauri SSH approved trigger".into()
            } else {
                "Tauri SSH no-approval trigger".into()
            },
            created_by: context.requester.clone(),
            created_at: String::new(),
            updated_at: String::new(),
            started_at: None,
            finished_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
        };
        db.upsert_jenkins_build_run(
            &run,
            (context.approval_id > 0).then_some(context.approval_id),
            context.connection.config_version,
            &context.request_hash,
            &context.parameters_json.to_string(),
        )
    }

    fn queue_wait_secs(run: &JenkinsBuild) -> Option<i64> {
        let created_at = run.created_at.trim();
        if created_at.is_empty() {
            return None;
        }
        let parsed = chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S").ok()?;
        let now = chrono::Local::now().naive_local();
        Some((now - parsed).num_seconds().max(0))
    }

    fn should_mark_queue_timeout(run: &JenkinsBuild) -> bool {
        run.build_number.is_none()
            && !run.queue_id.trim().is_empty()
            && matches!(
                run.status.as_str(),
                "queued" | "waiting" | "blocked" | "stuck" | "triggered"
            )
            && Self::queue_wait_secs(run)
                .map(|seconds| seconds >= JENKINS_QUEUE_TIMEOUT_SECS)
                .unwrap_or(false)
    }

    fn mark_queue_timeout(db: &Database, run: &JenkinsBuild) -> Result<JenkinsBuild, AppError> {
        db.mark_jenkins_build_run_queue_timeout(
            &run.run_key,
            &format!(
                "Queue 等待超过 {} 分钟，已停止本地盲目轮询",
                JENKINS_QUEUE_TIMEOUT_SECS / 60
            ),
        )
    }

    async fn sync_queue_once(
        db: &Database,
        connection: &JenkinsConnection,
        run: &JenkinsBuild,
    ) -> Result<JenkinsBuild, AppError> {
        if run.queue_id.trim().is_empty() {
            return Ok(run.clone());
        }
        if Self::should_mark_queue_timeout(run) {
            return Self::mark_queue_timeout(db, run);
        }
        let queue_item = JenkinsService::poll_queue_item(
            db,
            PollJenkinsQueueItemInput {
                connection_key: connection.connection_key.clone(),
                queue_id: run.queue_id.clone(),
            },
        )
        .await?;
        Self::sync_from_queue_item(db, connection, run, &queue_item).await
    }

    async fn sync_build_detail_once(
        db: &Database,
        connection: &JenkinsConnection,
        run: &JenkinsBuild,
    ) -> Result<JenkinsBuild, AppError> {
        let build_number = run
            .build_number
            .ok_or_else(|| AppError::InvalidInput("未完成 run 缺少 buildNumber".into()))?;
        let mut build =
            JenkinsService::fetch_build_detail(db, connection, &run.job_full_name, build_number)
                .await?;
        build.run_key = run.run_key.clone();
        build.request_id = run.request_id.clone();
        build.queue_id = run.queue_id.clone();
        build.created_by = run.created_by.clone();
        db.upsert_jenkins_build_run(&build, None, connection.config_version, "", "{}")
    }

    async fn sync_from_queue_item(
        db: &Database,
        connection: &JenkinsConnection,
        run: &JenkinsBuild,
        queue_item: &JenkinsQueueItem,
    ) -> Result<JenkinsBuild, AppError> {
        if let Some(build_number) = queue_item.build_number {
            let mut build = match JenkinsService::fetch_build_detail(
                db,
                connection,
                &run.job_full_name,
                build_number,
            )
            .await
            {
                Ok(build) => build,
                Err(_) => Self::local_building_run(run, queue_item),
            };
            build.run_key = run.run_key.clone();
            build.request_id = run.request_id.clone();
            build.queue_id = run.queue_id.clone();
            build.created_by = run.created_by.clone();
            return db.upsert_jenkins_build_run(&build, None, connection.config_version, "", "{}");
        }

        if Self::should_mark_queue_timeout(run) {
            return Self::mark_queue_timeout(db, run);
        }
        let mut queued = run.clone();
        queued.status = queue_item.status.clone();
        queued.status_source = "local".into();
        queued.last_error_code.clear();
        queued.last_error_message = queue_item.message.clone();
        db.upsert_jenkins_build_run(&queued, None, connection.config_version, "", "{}")
    }

    fn local_building_run(run: &JenkinsBuild, queue_item: &JenkinsQueueItem) -> JenkinsBuild {
        let mut build = run.clone();
        build.build_number = queue_item.build_number;
        build.status = "building".into();
        build.status_source = "local".into();
        build.last_error_code.clear();
        build.last_error_message = "queue item 已产生 buildNumber，构建详情稍后同步".into();
        build
    }

    fn mark_sync_failed(
        db: &Database,
        run: &JenkinsBuild,
        code: &str,
        error: &AppError,
    ) -> Result<JenkinsBuild, AppError> {
        db.mark_jenkins_build_run_sync_failed(&run.run_key, code, &error.to_string())
    }

    fn run_key(request_hash: &str) -> String {
        format!("jenkins-run-{}", request_hash)
    }
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{:02X}", byte).chars().collect::<Vec<_>>(),
        })
        .collect()
}

fn sanitize_key_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn safe_relative_path(value: &str) -> Result<PathBuf, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("artifact 路径不能为空".into()));
    }
    let mut path = PathBuf::new();
    for component in PathBuf::from(trimmed).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            _ => {
                return Err(AppError::InvalidInput(
                    "artifact 路径不能包含绝对路径或上级目录".into(),
                ));
            }
        }
    }
    if path.as_os_str().is_empty() {
        return Err(AppError::InvalidInput("artifact 路径不能为空".into()));
    }
    Ok(path)
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf, AppError> {
    if !path.is_absolute() {
        return Err(AppError::InvalidInput("路径必须是绝对路径".into()));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(AppError::InvalidInput(
                    "artifact 本地路径不能包含上级目录".into(),
                ));
            }
        }
    }
    Ok(normalized)
}

fn redact_key_value_like(line: &str, key: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let Some(start) = lower.find(key) else {
        return line.to_string();
    };
    let after_key = start + key.len();
    let separator_offset = lower[after_key..]
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .count();
    let separator_index = after_key + separator_offset;
    let Some(separator) = lower[separator_index..].chars().next() else {
        return line.to_string();
    };
    if separator != '=' && separator != ':' {
        return line.to_string();
    }
    let value_start = separator_index + separator.len_utf8();
    let value_end = line[value_start..]
        .find(|ch: char| ch.is_whitespace() || ch == '&' || ch == ',' || ch == ';')
        .map(|offset| value_start + offset)
        .unwrap_or_else(|| line.len());
    format!("{}[REDACTED]{}", &line[..value_start], &line[value_end..])
}

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use crate::database::Database;
    use crate::models::{
        CreateJenkinsArtifactDeploymentCandidateInput, CreateJenkinsBuildDeploymentDryRunInput,
        DecideApprovalRequestInput, EnableAiUnrestrictedInput, ExecuteJenkinsBuildApprovedInput,
        ExecuteJenkinsBuildStopApprovedInput, ForgetJenkinsRecentParameterValueInput,
        GetJenkinsBuildInput, JenkinsArtifact, JenkinsBuild, JenkinsBuildAnalysis,
        JenkinsBuildTriggerResult, JenkinsConnection, JenkinsQueueItem, ListJenkinsBuildsInput,
        ListJenkinsParameterTemplatesInput, ListJenkinsRecentParameterValuesInput,
        StopJenkinsBuildInput, TriggerJenkinsBuildInput, UpsertJenkinsConnectionInput,
        UpsertJenkinsParameterTemplateInput,
    };
    use crate::services::system_settings::SystemSettingsService;

    use super::{
        percent_encode, safe_relative_path, sanitize_key_segment, sha256_file,
        JenkinsBuildApprovalContext, JenkinsBuildParameter, JenkinsBuildTracker, JenkinsService,
    };
    use serde_json::json;
    use std::path::PathBuf;

    fn test_connection() -> JenkinsConnection {
        JenkinsConnection {
            id: 1,
            connection_key: "jenkins-test".into(),
            config_version: 7,
            name: "Test Jenkins".into(),
            base_url: "http://jenkins.test".into(),
            credential_key: "cred".into(),
            credential_display_name: "cred".into(),
            username_masked: "u***r".into(),
            ssh_server_alias: String::new(),
            environment: "dev".into(),
            environment_label: "开发".into(),
            tls_verify: true,
            default_view: String::new(),
            default_folder: String::new(),
            allow_mcp_read: true,
            allow_mcp_write: false,
            approval_policy: "L3".into(),
            parameter_prefill_enabled: true,
            risk_rules_json: "{}".into(),
            notify_on_success: false,
            notify_on_failure: true,
            notify_on_unstable: true,
            notify_on_aborted: true,
            status: "ok".into(),
            version: "2.0".into(),
            capabilities_json: "{}".into(),
            last_error_code: String::new(),
            last_error_message: String::new(),
            description: String::new(),
            enabled: true,
            last_tested_at: None,
            created_at: String::new(),
            updated_at: String::new(),
            deleted_at: None,
        }
    }

    fn upsert_enabled_test_connection(db: &Database) -> JenkinsConnection {
        let mut input = UpsertJenkinsConnectionInput {
            connection_key: Some("jenkins-test".into()),
            name: "Test Jenkins".into(),
            base_url: "http://jenkins.test".into(),
            credential_key: Some("cred".into()),
            credential_display_name: Some("cred".into()),
            username_masked: Some("u***r".into()),
            ssh_server_alias: None,
            environment: Some("dev".into()),
            environment_label: Some("开发".into()),
            tls_verify: Some(true),
            default_view: None,
            default_folder: None,
            allow_mcp_read: Some(true),
            allow_mcp_write: Some(true),
            approval_policy: Some("manual".into()),
            parameter_prefill_enabled: Some(true),
            risk_rules_json: Some("{}".into()),
            notify_on_success: Some(false),
            notify_on_failure: Some(true),
            notify_on_unstable: Some(true),
            notify_on_aborted: Some(true),
            description: None,
            enabled: Some(false),
        };
        JenkinsService::upsert_connection(db, input.clone())
            .expect("upsert draft test Jenkins connection");
        db.update_jenkins_connection_test_result(
            "jenkins-test",
            "ok",
            "2.0",
            "{}",
            "cred",
            "u***r",
            "",
            "",
        )
        .expect("mark test Jenkins connection ok");
        input.enabled = Some(true);
        JenkinsService::upsert_connection(db, input).expect("enable test Jenkins connection")
    }

    #[test]
    fn normalize_base_url_trims_trailing_slash() {
        let value = JenkinsService::normalize_base_url(" https://ci.example.com/jenkins/ ")
            .expect("valid Jenkins base URL");
        assert_eq!(value, "https://ci.example.com/jenkins");
    }

    #[test]
    fn normalize_base_url_rejects_query_or_hash() {
        assert!(JenkinsService::normalize_base_url("https://ci.example.com/?a=1").is_err());
        assert!(JenkinsService::normalize_base_url("https://ci.example.com/#jobs").is_err());
    }

    #[test]
    fn new_connection_key_uses_jenkins_prefix() {
        let key = JenkinsService::new_connection_key();
        assert!(key.starts_with("jenkins-"));
    }

    #[test]
    fn normalize_risk_rules_defaults_to_block_concurrent_builds() {
        let normalized = JenkinsService::normalize_risk_rules_json(Some("{}"))
            .expect("default risk rules should normalize");
        let mut connection = test_connection();
        connection.risk_rules_json = normalized;

        assert!(
            !JenkinsService::allow_concurrent_build_for_job(&connection, "dev-build")
                .expect("concurrency rule should parse")
        );
    }

    #[test]
    fn concurrent_builds_require_explicit_matching_pattern() {
        let mut connection = test_connection();
        connection.risk_rules_json = json!({
            "version": 1,
            "fallbackRisk": "L2",
            "fileParameterRisk": "L3",
            "environmentRisk": "auto",
            "concurrency": {
                "allowConcurrentBuilds": true,
                "allowConcurrentPatterns": ["^dev-.*"]
            },
            "jobRules": [],
            "parameterRules": []
        })
        .to_string();

        assert!(
            JenkinsService::allow_concurrent_build_for_job(&connection, "dev-build")
                .expect("matching whitelist should allow concurrency")
        );
        assert!(
            !JenkinsService::allow_concurrent_build_for_job(&connection, "prod-release")
                .expect("non matching job should remain blocked")
        );
    }

    #[test]
    fn normalize_approval_policy_accepts_no_approval_only_as_known_policy() {
        assert_eq!(
            JenkinsService::normalize_approval_policy(Some(" none "))
                .expect("none policy should normalize"),
            "none"
        );
        assert!(JenkinsService::normalize_approval_policy(Some("skip")).is_err());
    }

    #[test]
    fn direct_trigger_context_requires_no_approval_policy() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);

        let error = JenkinsService::build_direct_trigger_context(
            &db,
            TriggerJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/build".into(),
                parameter_definition_hash: "hash-1".into(),
                parameters_json: json!({"parameters": []}),
                requester: Some("tester".into()),
                reason: String::new(),
                risk_level: None,
            },
        )
        .expect_err("manual policy must not bypass approval");

        assert!(error.to_string().contains("无需审批策略"));
    }

    #[test]
    fn direct_trigger_context_accepts_no_approval_policy_without_reason() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);
        JenkinsService::upsert_connection(
            &db,
            UpsertJenkinsConnectionInput {
                connection_key: Some("jenkins-test".into()),
                name: "Test Jenkins".into(),
                base_url: "http://jenkins.test".into(),
                credential_key: Some("cred".into()),
                credential_display_name: Some("cred".into()),
                username_masked: Some("u***r".into()),
                ssh_server_alias: None,
                environment: Some("dev".into()),
                environment_label: Some("开发".into()),
                tls_verify: Some(true),
                default_view: None,
                default_folder: None,
                allow_mcp_read: Some(true),
                allow_mcp_write: Some(true),
                approval_policy: Some("none".into()),
                parameter_prefill_enabled: Some(true),
                risk_rules_json: Some("{}".into()),
                notify_on_success: Some(false),
                notify_on_failure: Some(true),
                notify_on_unstable: Some(true),
                notify_on_aborted: Some(true),
                description: None,
                enabled: Some(true),
            },
        )
        .expect("switch test Jenkins connection to no approval");

        let context = JenkinsService::build_direct_trigger_context(
            &db,
            TriggerJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/build".into(),
                parameter_definition_hash: "hash-1".into(),
                parameters_json: json!({"parameters": []}),
                requester: Some("tester".into()),
                reason: String::new(),
                risk_level: None,
            },
        )
        .expect("none policy should allow direct trigger context");

        assert_eq!(context.approval_id, 0);
        assert_eq!(context.risk_level, "L2");
        assert_eq!(context.requester, "tester");
    }

    #[test]
    fn upsert_connection_rejects_enable_before_successful_test() {
        let db = Database::init(":memory:").expect("init db");
        let error = JenkinsService::upsert_connection(
            &db,
            UpsertJenkinsConnectionInput {
                connection_key: Some("jenkins-untested".into()),
                name: "Untested Jenkins".into(),
                base_url: "http://jenkins.test".into(),
                credential_key: Some("cred".into()),
                credential_display_name: Some("cred".into()),
                username_masked: Some("u***r".into()),
                ssh_server_alias: None,
                environment: Some("dev".into()),
                environment_label: None,
                tls_verify: Some(true),
                default_view: None,
                default_folder: None,
                allow_mcp_read: Some(true),
                allow_mcp_write: Some(false),
                approval_policy: Some("manual".into()),
                parameter_prefill_enabled: Some(true),
                risk_rules_json: Some("{}".into()),
                notify_on_success: Some(false),
                notify_on_failure: Some(true),
                notify_on_unstable: Some(true),
                notify_on_aborted: Some(true),
                description: None,
                enabled: Some(true),
            },
        )
        .expect_err("untested connection should not enable");

        assert!(error.to_string().contains("测试成功"));
    }

    #[test]
    fn build_trigger_risk_uses_job_and_parameter_rules() {
        let mut connection = test_connection();
        connection.risk_rules_json = json!({
            "version": 1,
            "fallbackRisk": "L2",
            "fileParameterRisk": "L3",
            "environmentRisk": "auto",
            "concurrency": {
                "allowConcurrentBuilds": false,
                "allowConcurrentPatterns": []
            },
            "jobRules": [
                {"pattern": ".*deploy.*", "risk": "L3", "enabled": true}
            ],
            "parameterRules": [
                {"name": "ENV", "value": "prod", "risk": "blocked", "enabled": true}
            ]
        })
        .to_string();

        let l3 = JenkinsService::normalize_build_trigger_risk(
            &connection,
            "folder/deploy",
            None,
            &json!({"parameters": [{"name": "ENV", "value": "dev"}]}),
            false,
        )
        .expect("job rule should calculate risk");
        assert_eq!(l3, "L3");

        let blocked = JenkinsService::normalize_build_trigger_risk(
            &connection,
            "folder/build",
            None,
            &json!({"parameters": [{"name": "ENV", "value": "prod"}]}),
            false,
        )
        .expect("parameter rule should calculate risk");
        assert_eq!(blocked, "blocked");
    }

    #[test]
    fn create_build_trigger_approval_blocks_unfinished_same_job_by_default() {
        let db = Database::init(":memory:").expect("init db");
        let connection = upsert_enabled_test_connection(&db);
        db.upsert_jenkins_build_run(
            &JenkinsBuild {
                run_key: "jenkins-run-existing".into(),
                request_id: "request-existing".into(),
                connection_key: connection.connection_key.clone(),
                job_full_name: "folder/deploy".into(),
                queue_id: "100".into(),
                build_number: None,
                status: "queued".into(),
                status_source: "local".into(),
                result: String::new(),
                cause: "test".into(),
                created_by: "tester".into(),
                created_at: String::new(),
                updated_at: String::new(),
                started_at: None,
                finished_at: None,
                last_error_code: String::new(),
                last_error_message: String::new(),
            },
            None,
            connection.config_version,
            "",
            "{}",
        )
        .expect("seed unfinished run");

        let error = JenkinsService::create_build_trigger_approval(
            &db,
            TriggerJenkinsBuildInput {
                connection_key: connection.connection_key,
                job_full_name: "folder/deploy".into(),
                parameter_definition_hash: "hash-1".into(),
                parameters_json: json!({"parameters": []}),
                requester: Some("tester".into()),
                reason: "验证并发阻断".into(),
                risk_level: None,
            },
        )
        .expect_err("unfinished same job should block approval");

        assert!(error.to_string().contains("阻断"));
    }

    #[test]
    fn create_build_trigger_approval_writes_redacted_payload() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);
        let file_path = std::env::temp_dir().join(format!(
            "jenkins-file-param-test-{}.zip",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(&file_path, b"release-package").expect("write temp file parameter");
        let file_sha256 = sha256_file(&file_path).expect("hash temp file parameter");

        let approval = JenkinsService::create_build_trigger_approval(
            &db,
            TriggerJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                parameter_definition_hash: "hash-1".into(),
                parameters_json: json!({
                    "parameters": [
                        {
                            "name": "DEPLOY_PASSWORD",
                            "sensitive": true,
                            "value": "plain-secret"
                        },
                        {
                            "name": "PACKAGE",
                            "fileParameter": true,
                            "value": {
                                "fileName": "release.zip",
                                "localPath": file_path.to_string_lossy(),
                                "sha256": file_sha256.clone(),
                                "sizeBytes": 15
                            }
                        }
                    ]
                }),
                requester: Some("tester".into()),
                reason: "发布测试版本".into(),
                risk_level: None,
            },
        )
        .expect("create Jenkins build approval");

        assert_eq!(approval.source, "jenkins");
        assert_eq!(approval.action, "jenkins_build_trigger");
        assert_eq!(approval.status, "pending");
        assert_eq!(approval.risk, "L3");
        assert_eq!(approval.command.len(), 64);
        let payload: serde_json::Value =
            serde_json::from_str(&approval.payload_json).expect("approval payload json");
        assert_eq!(payload["action"], "jenkins_build_trigger");
        assert_eq!(payload["connectionKey"], "jenkins-test");
        assert_eq!(payload["connectionConfigVersion"], 1);
        assert_eq!(payload["jobFullName"], "folder/deploy");
        assert_eq!(payload["parameterDefinitionHash"], "hash-1");
        assert_eq!(payload["riskLevel"], "L3");
        assert_eq!(payload["riskFlags"], json!(["file_parameter"]));
        assert!(payload["createdAtBucket"]
            .as_str()
            .is_some_and(|value| value.ends_with('Z')));
        assert_eq!(payload["requestHash"], approval.command);
        assert_eq!(
            payload["parameters"]["parameters"][1]["value"]["fileName"],
            "release.zip"
        );
        assert_eq!(
            payload["parameters"]["parameters"][1]["value"]["sha256"],
            file_sha256
        );
        assert!(
            payload["parameters"]["parameters"][1]["value"]["localPathRef"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(payload["parameters"]["parameters"][1]["value"]["localPath"].is_null());
        let payload_text = approval.payload_json;
        assert!(payload_text.contains("[REDACTED]"));
        assert!(!payload_text.contains("plain-secret"));
        assert!(!payload_text.contains(&file_path.to_string_lossy().to_string()));
        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn create_build_trigger_approval_auto_approves_when_ai_unrestricted_is_active() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);
        SystemSettingsService::enable_ai_unrestricted_mode(
            &db,
            EnableAiUnrestrictedInput { minutes: Some(10) },
        )
        .expect("enable ai unrestricted");

        let approval = JenkinsService::create_build_trigger_approval(
            &db,
            TriggerJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                parameter_definition_hash: "hash-1".into(),
                parameters_json: json!({"parameters": []}),
                requester: Some("mcp-client".into()),
                reason: "MCP 请求触发 Jenkins 构建".into(),
                risk_level: Some("L2".into()),
            },
        )
        .expect("create Jenkins build approval");

        assert_eq!(approval.status, "approved");
        assert_eq!(approval.decided_by, "ai-unrestricted");
        assert!(approval
            .decision_note
            .contains("AI 临时放行已开启，系统自动确认"));
    }

    #[test]
    fn build_trigger_approval_validation_rejects_hash_mismatch() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);

        let approval = JenkinsService::create_build_trigger_approval(
            &db,
            TriggerJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                parameter_definition_hash: "hash-1".into(),
                parameters_json: json!({"parameters": []}),
                requester: Some("tester".into()),
                reason: "触发普通构建".into(),
                risk_level: Some("L2".into()),
            },
        )
        .expect("create Jenkins build approval");
        db.decide_approval_request(&DecideApprovalRequestInput {
            id: approval.id,
            decision: "approved".into(),
            note: "ok".into(),
            decided_by: "tester".into(),
        })
        .expect("approve request");

        let error = JenkinsService::validate_build_trigger_approval(
            &db,
            &ExecuteJenkinsBuildApprovedInput {
                approval_id: approval.id,
                request_hash: Some("bad-hash".into()),
            },
        )
        .expect_err("hash mismatch should reject");
        assert!(error.to_string().contains("requestHash 不匹配"));
    }

    #[test]
    fn build_trigger_approval_validation_accepts_approved_plain_build() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);

        let approval = JenkinsService::create_build_trigger_approval(
            &db,
            TriggerJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                parameter_definition_hash: "hash-1".into(),
                parameters_json: json!({"parameters": []}),
                requester: Some("tester".into()),
                reason: "触发普通构建".into(),
                risk_level: Some("L2".into()),
            },
        )
        .expect("create Jenkins build approval");
        db.decide_approval_request(&DecideApprovalRequestInput {
            id: approval.id,
            decision: "approved".into(),
            note: "ok".into(),
            decided_by: "tester".into(),
        })
        .expect("approve request");

        let context = JenkinsService::validate_build_trigger_approval(
            &db,
            &ExecuteJenkinsBuildApprovedInput {
                approval_id: approval.id,
                request_hash: Some(approval.command.clone()),
            },
        )
        .expect("approved plain build should validate");
        assert_eq!(context.request_hash, approval.command);
        assert_eq!(context.job_full_name, "folder/deploy");
        assert!(!JenkinsService::parameter_payload_has_entries(
            &context.parameters_json
        ));
    }

    #[test]
    fn build_trigger_approval_validation_rejects_when_mcp_write_is_disabled() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);

        let approval = JenkinsService::create_build_trigger_approval(
            &db,
            TriggerJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                parameter_definition_hash: "hash-1".into(),
                parameters_json: json!({"parameters": []}),
                requester: Some("tester".into()),
                reason: "触发普通构建".into(),
                risk_level: Some("L2".into()),
            },
        )
        .expect("create Jenkins build approval");
        db.decide_approval_request(&DecideApprovalRequestInput {
            id: approval.id,
            decision: "approved".into(),
            note: "ok".into(),
            decided_by: "tester".into(),
        })
        .expect("approve request");
        JenkinsService::upsert_connection(
            &db,
            UpsertJenkinsConnectionInput {
                connection_key: Some("jenkins-test".into()),
                name: "Test Jenkins".into(),
                base_url: "http://jenkins.test".into(),
                credential_key: Some("cred".into()),
                credential_display_name: Some("cred".into()),
                username_masked: Some("u***r".into()),
                ssh_server_alias: None,
                environment: Some("dev".into()),
                environment_label: Some("开发".into()),
                tls_verify: Some(true),
                default_view: None,
                default_folder: None,
                allow_mcp_read: Some(true),
                allow_mcp_write: Some(false),
                approval_policy: Some("manual".into()),
                parameter_prefill_enabled: Some(true),
                risk_rules_json: Some("{}".into()),
                notify_on_success: Some(false),
                notify_on_failure: Some(true),
                notify_on_unstable: Some(true),
                notify_on_aborted: Some(true),
                description: None,
                enabled: Some(true),
            },
        )
        .expect("disable mcp write");

        let error = JenkinsService::validate_build_trigger_approval(
            &db,
            &ExecuteJenkinsBuildApprovedInput {
                approval_id: approval.id,
                request_hash: Some(approval.command.clone()),
            },
        )
        .expect_err("disabled mcp write should reject approved execution");

        assert!(error.to_string().contains("未开启 allow_mcp_write"));
    }

    #[test]
    fn create_build_stop_approval_escalates_release_job_to_l3() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);

        let approval = JenkinsService::create_build_stop_approval(
            &db,
            StopJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/release-prod".into(),
                build_number: 42,
                requester: Some("tester".into()),
                reason: "发布构建异常，需要停止".into(),
                risk_level: None,
            },
        )
        .expect("create Jenkins stop approval");

        assert_eq!(approval.source, "jenkins");
        assert_eq!(approval.action, "jenkins_build_stop");
        assert_eq!(approval.risk, "L3");
        assert_eq!(approval.command.len(), 64);
        let payload: serde_json::Value =
            serde_json::from_str(&approval.payload_json).expect("approval payload json");
        assert_eq!(payload["action"], "jenkins_build_stop");
        assert_eq!(payload["connectionKey"], "jenkins-test");
        assert_eq!(payload["jobFullName"], "folder/release-prod");
        assert_eq!(payload["buildNumber"], 42);
        assert_eq!(payload["riskLevel"], "L3");
        assert_eq!(payload["riskFlags"], json!(["release_or_prod_job"]));
        assert_eq!(payload["requestHash"], approval.command);
        assert!(approval.resource.ends_with("#42"));
    }

    #[test]
    fn build_stop_approval_validation_accepts_approved_request() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);

        let approval = JenkinsService::create_build_stop_approval(
            &db,
            StopJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                build_number: 9,
                requester: Some("tester".into()),
                reason: "构建卡住，申请停止".into(),
                risk_level: Some("L2".into()),
            },
        )
        .expect("create Jenkins stop approval");
        db.decide_approval_request(&DecideApprovalRequestInput {
            id: approval.id,
            decision: "approved".into(),
            note: "ok".into(),
            decided_by: "tester".into(),
        })
        .expect("approve request");

        let context = JenkinsService::validate_build_stop_approval(
            &db,
            &ExecuteJenkinsBuildStopApprovedInput {
                approval_id: approval.id,
                request_hash: Some(approval.command.clone()),
            },
        )
        .expect("approved stop should validate");

        assert_eq!(context.request_hash, approval.command);
        assert_eq!(context.job_full_name, "folder/deploy");
        assert_eq!(context.build_number, 9);
        assert_eq!(context.risk_level, "L2");
    }

    #[test]
    fn parameter_template_saves_secret_ref_only_payload() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);

        let saved = JenkinsService::upsert_parameter_template(
            &db,
            UpsertJenkinsParameterTemplateInput {
                template_key: None,
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                name: "灰度发布".into(),
                parameters_json: json!({
                    "parameters": [
                        {
                            "name": "BRANCH",
                            "type": "string",
                            "sensitive": false,
                            "value": "main"
                        },
                        {
                            "name": "DEPLOY_PASSWORD",
                            "type": "password",
                            "sensitive": true,
                            "value": {
                                "valueKind": "secret_ref",
                                "secretRef": "jenkins-prod-password"
                            }
                        }
                    ]
                }),
                parameter_definition_hash: Some("hash-1".into()),
                requester: Some("tester".into()),
            },
        )
        .expect("save parameter template");

        assert_eq!(saved.name, "灰度发布");
        assert_eq!(saved.parameter_definition_hash, "hash-1");
        let templates = JenkinsService::list_parameter_templates(
            &db,
            ListJenkinsParameterTemplatesInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                requester: Some("tester".into()),
            },
        )
        .expect("list templates");
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].template_key, saved.template_key);
        assert_eq!(
            templates[0].parameters_json["parameters"][1]["value"]["secretRef"],
            "jenkins-prod-password"
        );
    }

    #[test]
    fn parameter_template_rejects_sensitive_plaintext() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);

        let error = JenkinsService::upsert_parameter_template(
            &db,
            UpsertJenkinsParameterTemplateInput {
                template_key: None,
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                name: "错误模板".into(),
                parameters_json: json!({
                    "parameters": [
                        {
                            "name": "DEPLOY_PASSWORD",
                            "type": "password",
                            "sensitive": true,
                            "value": "plain-secret"
                        }
                    ]
                }),
                parameter_definition_hash: Some("hash-1".into()),
                requester: Some("tester".into()),
            },
        )
        .expect_err("sensitive plaintext should be rejected");

        assert!(error.to_string().contains("只能保存 secretRef"));
    }

    #[test]
    fn parameter_payload_entry_detection_handles_plain_and_parameterized_builds() {
        assert!(!JenkinsService::parameter_payload_has_entries(&json!({})));
        assert!(!JenkinsService::parameter_payload_has_entries(
            &json!({"parameters": []})
        ));
        assert!(JenkinsService::parameter_payload_has_entries(
            &json!({"parameters": [{"name": "BRANCH", "value": "main"}]})
        ));
    }

    #[test]
    fn collect_build_parameters_serializes_standard_values() {
        let db = Database::init(":memory:").expect("init db");
        let parameters = JenkinsService::collect_build_parameters(
            &db,
            &json!({
                "parameters": [
                    {"name": "BRANCH", "value": "main"},
                    {"name": "DRY_RUN", "value": true},
                    {"name": "RETRIES", "value": 3}
                ]
            }),
        )
        .expect("standard parameters should collect");

        assert_eq!(
            parameters,
            vec![
                JenkinsBuildParameter::Scalar {
                    name: "BRANCH".into(),
                    value: "main".into()
                },
                JenkinsBuildParameter::Scalar {
                    name: "DRY_RUN".into(),
                    value: "true".into()
                },
                JenkinsBuildParameter::Scalar {
                    name: "RETRIES".into(),
                    value: "3".into()
                }
            ]
        );
    }

    #[test]
    fn collect_build_parameters_rejects_missing_secret_ref() {
        let db = Database::init(":memory:").expect("init db");
        let error = JenkinsService::collect_build_parameters(
            &db,
            &json!({
                "parameters": [
                    {"name": "PASSWORD", "sensitive": true, "value": {"valueKind": "secret_ref", "missing": true}}
                ]
            }),
        )
        .expect_err("missing secretRef should reject");

        assert!(error.to_string().contains("缺少 secretRef"));
    }

    #[test]
    fn collect_build_parameters_rejects_file_parameter_without_controlled_reference() {
        let db = Database::init(":memory:").expect("init db");
        let error = JenkinsService::collect_build_parameters(
            &db,
            &json!({
                "parameters": [
                    {
                        "name": "PACKAGE",
                        "fileParameter": true,
                        "value": {"fileName": "release.zip", "sha256": "abc", "sizeBytes": 10}
                    }
                ]
            }),
        )
        .expect_err("file parameter without controlled local reference should reject");

        assert!(error
            .to_string()
            .contains("file_parameter_reference_missing_after_approval"));
    }

    #[test]
    fn recent_parameter_payload_keeps_secret_ref_without_plaintext() {
        let plain = JenkinsService::recent_parameter_value_payload(false, Some(&json!("main")))
            .expect("plain value should be saved");
        assert_eq!(plain.0, "plain");
        assert_eq!(plain.1, json!("main"));

        let secret = JenkinsService::recent_parameter_value_payload(
            true,
            Some(&json!({"valueKind": "secret_ref", "secretRef": "cred-prod-token"})),
        )
        .expect("secretRef should be saved as reference");
        assert_eq!(secret.0, "secret_ref");
        assert_eq!(secret.1, json!({"secretRef": "cred-prod-token"}));

        assert!(JenkinsService::recent_parameter_value_payload(
            true,
            Some(&json!({"valueKind": "secret_ref", "missing": true})),
        )
        .is_none());
        assert!(JenkinsService::recent_parameter_value_payload(
            false,
            Some(&json!({"fileName": "release.zip"})),
        )
        .is_none());
    }

    #[test]
    fn recent_parameter_values_can_be_listed_and_forgotten() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);
        let context = JenkinsBuildApprovalContext {
            approval_id: 10,
            request_hash: "recent-values-hash".into(),
            connection: test_connection(),
            job_full_name: "folder/deploy".into(),
            parameter_definition_hash: "hash-1".into(),
            parameters_json: json!({
                "parameters": [
                    {"name": "BRANCH", "value": "main"},
                    {"name": "TOKEN", "sensitive": true, "value": {"valueKind": "secret_ref", "secretRef": "cred-token"}},
                    {"name": "PACKAGE", "fileParameter": true, "value": {"fileName": "release.zip"}}
                ]
            }),
            risk_level: "L2".into(),
            requester: "tester".into(),
        };
        let run = JenkinsBuild {
            run_key: "jenkins-run-recent-values-hash".into(),
            request_id: context.request_hash.clone(),
            connection_key: context.connection.connection_key.clone(),
            job_full_name: context.job_full_name.clone(),
            queue_id: "126".into(),
            build_number: Some(56),
            status: "queued".into(),
            status_source: "local".into(),
            result: String::new(),
            cause: String::new(),
            created_by: "tester".into(),
            created_at: String::new(),
            updated_at: String::new(),
            started_at: None,
            finished_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
        };

        JenkinsService::record_recent_parameter_values(&db, &context, &run)
            .expect("recent values should save");
        let values = JenkinsService::list_recent_parameter_values(
            &db,
            ListJenkinsRecentParameterValuesInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                requester: Some("tester".into()),
            },
        )
        .expect("recent values should list");
        assert_eq!(values.len(), 2);
        assert!(values.iter().any(|item| item.parameter_name == "BRANCH"));
        assert!(values.iter().any(|item| {
            item.parameter_name == "TOKEN"
                && item.value_kind == "secret_ref"
                && item.value_json == json!({"secretRef": "cred-token"})
        }));

        let deleted = JenkinsService::forget_recent_parameter_value(
            &db,
            ForgetJenkinsRecentParameterValueInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                parameter_name: "BRANCH".into(),
                requester: Some("tester".into()),
            },
        )
        .expect("recent value should forget");
        assert!(deleted);

        let values = JenkinsService::list_recent_parameter_values(
            &db,
            ListJenkinsRecentParameterValuesInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                requester: Some("tester".into()),
            },
        )
        .expect("recent values should list after delete");
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].parameter_name, "TOKEN");
    }

    #[test]
    fn build_trigger_url_and_queue_id_are_stable() {
        let connection = test_connection();
        assert_eq!(
            JenkinsService::build_trigger_url(&connection, "folder/release job"),
            "http://jenkins.test/job/folder/job/release%20job/build"
        );
        assert_eq!(
            JenkinsService::build_with_parameters_url(&connection, "folder/release job"),
            "http://jenkins.test/job/folder/job/release%20job/buildWithParameters"
        );
        assert_eq!(
            JenkinsService::parse_queue_id_from_location("http://jenkins.test/queue/item/123/"),
            Some("123".into())
        );
        assert_eq!(
            JenkinsService::parse_queue_id_from_location("http://jenkins.test/queue/"),
            None
        );
    }

    #[test]
    fn map_queue_item_handles_waiting_and_executable_states() {
        let connection = test_connection();
        let waiting = JenkinsService::map_queue_item(
            &connection,
            &json!({
                "id": 123,
                "task": {"fullName": "folder/deploy"},
                "why": "Waiting for next available executor",
                "blocked": false,
                "stuck": false,
                "inQueueSince": 1_735_689_600_000_i64
            }),
        )
        .expect("waiting queue item should map");
        assert_eq!(waiting.queue_id, "123");
        assert_eq!(waiting.job_full_name, "folder/deploy");
        assert_eq!(waiting.status, "waiting");
        assert_eq!(waiting.build_number, None);
        assert_eq!(waiting.created_at, "2025-01-01T00:00:00+00:00");

        let executable = JenkinsService::map_queue_item(
            &connection,
            &json!({
                "id": 124,
                "task": {"name": "deploy"},
                "executable": {"number": 42, "url": "http://jenkins.test/job/deploy/42/"}
            }),
        )
        .expect("executable queue item should map");
        assert_eq!(executable.status, "executable");
        assert_eq!(executable.build_number, Some(42));
        assert_eq!(
            executable.executable_url,
            "http://jenkins.test/job/deploy/42/"
        );
    }

    #[test]
    fn build_tracker_records_triggered_run() {
        let db = Database::init(":memory:").expect("init db");
        let connection = upsert_enabled_test_connection(&db);
        let context = JenkinsBuildApprovalContext {
            approval_id: 7,
            request_hash: "abcdef123456".into(),
            connection,
            job_full_name: "folder/deploy".into(),
            parameter_definition_hash: "hash-1".into(),
            parameters_json: json!({"parameters": []}),
            risk_level: "L2".into(),
            requester: "tester".into(),
        };
        let result = JenkinsBuildTriggerResult {
            approval_id: 7,
            request_hash: context.request_hash.clone(),
            connection_key: context.connection.connection_key.clone(),
            job_full_name: context.job_full_name.clone(),
            queue_id: Some("123".into()),
            location: Some("http://jenkins.test/queue/item/123/".into()),
            run_key: "jenkins-run-abcdef123456".into(),
            build_number: None,
            status: "queued".into(),
        };

        let run = JenkinsBuildTracker::record_triggered(&db, &context, &result)
            .expect("triggered run should persist");

        assert_eq!(run.run_key, "jenkins-run-abcdef123456");
        assert_eq!(run.queue_id, "123");
        assert_eq!(run.status, "queued");
        assert_eq!(run.status_source, "local");
        assert_eq!(run.created_by, "tester");
    }

    #[tokio::test]
    async fn build_tracker_updates_local_queue_status_without_build_number() {
        let db = Database::init(":memory:").expect("init db");
        let connection = upsert_enabled_test_connection(&db);
        let context = JenkinsBuildApprovalContext {
            approval_id: 8,
            request_hash: "fedcba654321".into(),
            connection: connection.clone(),
            job_full_name: "folder/deploy".into(),
            parameter_definition_hash: "hash-1".into(),
            parameters_json: json!({"parameters": []}),
            risk_level: "L2".into(),
            requester: "tester".into(),
        };
        let result = JenkinsBuildTriggerResult {
            approval_id: 8,
            request_hash: context.request_hash.clone(),
            connection_key: context.connection.connection_key.clone(),
            job_full_name: context.job_full_name.clone(),
            queue_id: Some("124".into()),
            location: None,
            run_key: "jenkins-run-fedcba654321".into(),
            build_number: None,
            status: "queued".into(),
        };
        let run = JenkinsBuildTracker::record_triggered(&db, &context, &result)
            .expect("triggered run should persist");
        let queue_item = JenkinsQueueItem {
            queue_id: "124".into(),
            connection_key: connection.connection_key.clone(),
            job_full_name: "folder/deploy".into(),
            build_number: None,
            executable_url: String::new(),
            status: "blocked".into(),
            message: "Waiting for upstream project".into(),
            created_at: String::new(),
        };

        let updated =
            JenkinsBuildTracker::sync_from_queue_item(&db, &connection, &run, &queue_item)
                .await
                .expect("queue status should update local run");

        assert_eq!(updated.run_key, run.run_key);
        assert_eq!(updated.status, "blocked");
        assert_eq!(updated.status_source, "local");
        assert_eq!(updated.last_error_message, "Waiting for upstream project");
    }

    #[test]
    fn build_tracker_records_observed_build_without_duplicate_tracked_run() {
        let db = Database::init(":memory:").expect("init db");
        let connection = upsert_enabled_test_connection(&db);
        let context = JenkinsBuildApprovalContext {
            approval_id: 9,
            request_hash: "tracked-build-hash".into(),
            connection: connection.clone(),
            job_full_name: "folder/deploy".into(),
            parameter_definition_hash: "hash-1".into(),
            parameters_json: json!({"parameters": []}),
            risk_level: "L2".into(),
            requester: "tester".into(),
        };
        let result = JenkinsBuildTriggerResult {
            approval_id: 9,
            request_hash: context.request_hash.clone(),
            connection_key: context.connection.connection_key.clone(),
            job_full_name: context.job_full_name.clone(),
            queue_id: Some("125".into()),
            location: None,
            run_key: "jenkins-run-tracked-build-hash".into(),
            build_number: None,
            status: "queued".into(),
        };
        let mut run = JenkinsBuildTracker::record_triggered(&db, &context, &result)
            .expect("triggered run should persist");
        run.build_number = Some(55);
        db.upsert_jenkins_build_run(&run, None, connection.config_version, "", "{}")
            .expect("build number should persist");

        let observed = JenkinsBuild {
            run_key: "jenkins-jenkins-test-folder-deploy-55".into(),
            request_id: String::new(),
            connection_key: connection.connection_key.clone(),
            job_full_name: "folder/deploy".into(),
            queue_id: String::new(),
            build_number: Some(55),
            status: "success".into(),
            status_source: "jenkins".into(),
            result: "success".into(),
            cause: "Started by user tester".into(),
            created_by: "jenkins".into(),
            created_at: "2026-07-04T10:00:00+00:00".into(),
            updated_at: "2026-07-04T10:00:00+00:00".into(),
            started_at: None,
            finished_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
        };

        let recorded = JenkinsBuildTracker::record_observed_build(&db, &connection, &observed)
            .expect("observed build should merge into tracked run");

        assert_eq!(recorded.run_key, "jenkins-run-tracked-build-hash");
        assert_eq!(recorded.request_id, "tracked-build-hash");
        assert_eq!(recorded.queue_id, "125");
        assert_eq!(recorded.status, "success");
        assert_eq!(recorded.status_source, "jenkins");

        let rows = db
            .list_jenkins_build_runs(&ListJenkinsBuildsInput {
                connection_key: connection.connection_key,
                job_full_name: None,
                limit: Some(10),
                offset: Some(0),
                cursor: None,
            })
            .expect("run list should load");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn normalize_job_status_maps_jenkins_colors() {
        assert_eq!(JenkinsService::normalize_job_status("blue"), "success");
        assert_eq!(
            JenkinsService::normalize_job_status("red_anime"),
            "building"
        );
        assert_eq!(JenkinsService::normalize_job_status("yellow"), "unstable");
        assert_eq!(
            JenkinsService::normalize_job_status("notbuilt"),
            "not_built"
        );
        assert_eq!(JenkinsService::normalize_job_status(""), "not_built");
    }

    #[test]
    fn percent_encode_handles_folder_segments() {
        assert_eq!(
            percent_encode("release job/生产"),
            "release%20job%2F%E7%94%9F%E4%BA%A7"
        );
    }

    #[test]
    fn sanitize_key_segment_removes_path_separators() {
        assert_eq!(sanitize_key_segment("folder/job #1"), "folder-job--1");
    }

    #[test]
    fn map_job_tree_preserves_folder_hierarchy() {
        let value = json!({
            "name": "Provider",
            "fullName": "Provider",
            "_class": "com.cloudbees.hudson.plugins.folder.Folder",
            "jobs": [{
                "name": "fj-adb-provider",
                "fullName": "Provider/fj-adb-provider",
                "_class": "org.jenkinsci.plugins.workflow.job.WorkflowJob",
                "color": "blue",
                "buildable": true,
                "lastBuild": {"number": 29, "result": "SUCCESS"}
            }]
        });

        let root = JenkinsService::map_job_tree(&value, 0, 3).expect("folder job");
        assert_eq!(root.job_full_name, "Provider");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].job_full_name, "Provider/fj-adb-provider");
        assert!(root.children[0].children.is_empty());
    }

    #[test]
    fn safe_relative_path_rejects_escape() {
        assert!(safe_relative_path("../secret.txt").is_err());
        assert!(safe_relative_path("/tmp/secret.txt").is_err());
        assert!(safe_relative_path("dist/app.tar.gz").is_ok());
    }

    #[test]
    fn parse_tunnel_remote_uses_known_default_ports() {
        let (_, host, port) =
            JenkinsService::parse_tunnel_remote("http://jenkins.internal/ci/api/json", true)
                .expect("valid HTTP tunnel target");
        assert_eq!(host, "jenkins.internal");
        assert_eq!(port, 80);

        let (_, host, port) =
            JenkinsService::parse_tunnel_remote("https://jenkins.internal:8443/ci/api/json", false)
                .expect("valid HTTPS tunnel target when TLS verify is disabled");
        assert_eq!(host, "jenkins.internal");
        assert_eq!(port, 8443);
    }

    #[test]
    fn parse_tunnel_remote_rejects_https_with_tls_verify() {
        assert!(
            JenkinsService::parse_tunnel_remote("https://jenkins.internal/api/json", true).is_err()
        );
    }

    #[test]
    fn rewrite_tunnel_url_preserves_path_and_query() {
        let (url, _, _) = JenkinsService::parse_tunnel_remote(
            "http://jenkins.internal/ci/api/json?tree=jobs",
            true,
        )
        .expect("valid tunnel target");
        let rewritten =
            JenkinsService::rewrite_tunnel_url(url, 34567).expect("rewrite to local tunnel");
        assert_eq!(rewritten, "http://127.0.0.1:34567/ci/api/json?tree=jobs");
    }

    #[test]
    fn map_parameter_definitions_handles_standard_and_sensitive_types() {
        let value = json!({
            "property": [{
                "parameterDefinitions": [
                    {
                        "name": "BRANCH",
                        "description": "Git branch",
                        "defaultValue": "main",
                        "_class": "hudson.model.StringParameterDefinition"
                    },
                    {
                        "name": "DEPLOY_ENV",
                        "defaultValue": "dev",
                        "choices": ["dev", "prod"],
                        "_class": "hudson.model.ChoiceParameterDefinition"
                    },
                    {
                        "name": "DRY_RUN",
                        "defaultValue": true,
                        "_class": "hudson.model.BooleanParameterDefinition"
                    },
                    {
                        "name": "API_TOKEN",
                        "_class": "hudson.model.PasswordParameterDefinition"
                    },
                    {
                        "name": "PACKAGE",
                        "_class": "hudson.model.FileParameterDefinition"
                    }
                ]
            }]
        });

        let parameters = JenkinsService::map_parameter_definitions(&value);
        assert_eq!(parameters.len(), 5);
        assert_eq!(parameters[0].parameter_type, "string");
        assert_eq!(parameters[1].parameter_type, "choice");
        assert_eq!(parameters[1].choices, vec!["dev", "prod"]);
        assert_eq!(parameters[2].parameter_type, "boolean");
        assert!(parameters[3].sensitive);
        assert_eq!(parameters[3].parameter_type, "password");
        assert!(parameters[4].file_parameter);
    }

    #[test]
    fn map_parameter_definitions_marks_dynamic_and_unsupported_types() {
        let value = json!({
            "property": [{
                "parameterDefinitions": [
                    {
                        "name": "GIT_BRANCH",
                        "_class": "net.uaznia.lukanus.hudson.plugins.gitparameter.GitParameterDefinition"
                    },
                    {
                        "name": "CUSTOM_THING",
                        "_class": "example.CustomParameterDefinition"
                    }
                ]
            }]
        });

        let parameters = JenkinsService::map_parameter_definitions(&value);
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].parameter_type, "string");
        assert!(parameters[0].dynamic_parameter);
        assert_eq!(parameters[1].parameter_type, "unsupported");
        assert!(parameters[1].unsupported);
    }

    #[test]
    fn parameter_definition_hash_is_stable_and_sensitive_to_changes() {
        let value = json!({
            "property": [{
                "parameterDefinitions": [
                    { "name": "BRANCH", "_class": "hudson.model.StringParameterDefinition" }
                ]
            }]
        });
        let mut parameters = JenkinsService::map_parameter_definitions(&value);
        let first_hash = JenkinsService::parameter_definition_hash(&parameters)
            .expect("hash parameter definitions");
        let second_hash = JenkinsService::parameter_definition_hash(&parameters)
            .expect("hash parameter definitions again");
        assert_eq!(first_hash, second_hash);

        parameters[0].default_value = json!("main");
        let changed_hash = JenkinsService::parameter_definition_hash(&parameters)
            .expect("hash changed parameter definitions");
        assert_ne!(first_hash, changed_hash);
    }

    #[test]
    fn parameter_cache_returns_hash_and_cache_metadata() {
        let connection = test_connection();
        let value = json!({
            "property": [{
                "parameterDefinitions": [
                    { "name": "BRANCH", "_class": "hudson.model.StringParameterDefinition" }
                ]
            }]
        });
        let parameters = JenkinsService::map_parameter_definitions(&value);
        let cache_key = format!(
            "{}:{}:{}",
            JenkinsService::parameter_cache_key(&connection, "folder/job"),
            std::process::id(),
            "cache-test"
        );
        let fresh =
            JenkinsService::cache_parameters(&cache_key, &connection, "folder/job", parameters)
                .expect("cache parameters");
        assert!(!fresh.from_cache);
        assert_eq!(fresh.ttl_seconds, 60);
        assert!(!fresh.parameter_definition_hash.is_empty());

        let cached = JenkinsService::get_cached_parameters(&cache_key, &connection, "folder/job")
            .expect("read cached parameters")
            .expect("cache hit");
        assert!(cached.from_cache);
        assert_eq!(
            cached.parameter_definition_hash,
            fresh.parameter_definition_hash
        );
        assert_eq!(cached.parameters.len(), 1);
    }

    #[test]
    fn inspect_file_parameter_returns_controlled_metadata() {
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-jenkins-file-param-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create test dir");
        let file_path = root.join("payload.txt");
        std::fs::write(&file_path, b"hello file parameter").expect("write test file");

        let metadata = JenkinsService::inspect_file_parameter(
            crate::models::InspectJenkinsFileParameterInput {
                parameter_name: "UPLOAD".into(),
                local_path: file_path.to_string_lossy().to_string(),
            },
        )
        .expect("inspect file parameter");
        assert_eq!(metadata.parameter_name, "UPLOAD");
        assert_eq!(metadata.file_name, "payload.txt");
        assert_eq!(metadata.size_bytes, 20);
        assert_eq!(
            metadata.sha256,
            "68b3d665e508723e10828ffe7e2478b5e971bf8c3f66a73585681ed6c10e360f"
        );
        assert!(metadata.modified_at.is_some());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn validate_managed_artifact_path_rejects_escape() {
        let root = std::env::temp_dir()
            .join(format!("tauri-ssh-jenkins-test-{}", std::process::id()))
            .join("jenkins-artifacts");
        std::fs::create_dir_all(&root).expect("create managed root");
        let inside = root.join("conn/job/1/app.tar.gz");
        assert!(
            JenkinsService::validate_managed_artifact_path(&root, &inside.to_string_lossy())
                .is_ok()
        );
        let outside = PathBuf::from("/tmp/app.tar.gz");
        assert!(
            JenkinsService::validate_managed_artifact_path(&root, &outside.to_string_lossy())
                .is_err()
        );
        std::fs::remove_dir_all(root.parent().expect("test root parent")).ok();
    }

    #[test]
    fn artifact_deployment_candidate_rejects_missing_local_path() {
        let db = Database::init(":memory:").expect("init db");
        upsert_enabled_test_connection(&db);
        db.upsert_jenkins_artifact_record(&JenkinsArtifact {
            id: 0,
            artifact_key: "artifact-missing-local".into(),
            request_id: "req-artifact".into(),
            connection_key: "jenkins-test".into(),
            job_full_name: "folder/deploy".into(),
            build_number: 42,
            file_name: "release.zip".into(),
            relative_path: "dist/release.zip".into(),
            local_path: String::new(),
            size_bytes: Some(10),
            sha256: "sha-test".into(),
            source_url: String::new(),
            status: "available".into(),
            risk_flags: Vec::new(),
            downloaded_at: None,
            cleaned_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .expect("insert artifact");

        let error = JenkinsService::create_artifact_deployment_candidate(
            &db,
            CreateJenkinsArtifactDeploymentCandidateInput {
                artifact_key: "artifact-missing-local".into(),
            },
        )
        .expect_err("missing local path should reject");
        assert!(error.to_string().contains("尚未下载"));
    }

    #[test]
    fn artifact_deployment_candidate_contains_artifact_metadata() {
        let db = Database::init(":memory:").expect("init db");
        let connection = upsert_enabled_test_connection(&db);
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-jenkins-candidate-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create candidate test dir");
        let artifact_path = root.join("release.jar");
        std::fs::write(&artifact_path, b"jar").expect("write artifact");
        db.upsert_jenkins_artifact_record(&JenkinsArtifact {
            id: 0,
            artifact_key: "artifact-available".into(),
            request_id: "req-artifact".into(),
            connection_key: "jenkins-test".into(),
            job_full_name: "folder/deploy".into(),
            build_number: 42,
            file_name: "release.jar".into(),
            relative_path: "build/libs/release.jar".into(),
            local_path: artifact_path.to_string_lossy().to_string(),
            size_bytes: Some(3),
            sha256: "sha-test".into(),
            source_url: String::new(),
            status: "available".into(),
            risk_flags: Vec::new(),
            downloaded_at: None,
            cleaned_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .expect("insert artifact");
        db.upsert_jenkins_build_run(
            &JenkinsBuild {
                run_key: "jenkins-run-success-candidate".into(),
                request_id: "req-build".into(),
                connection_key: connection.connection_key.clone(),
                job_full_name: "folder/deploy".into(),
                queue_id: String::new(),
                build_number: Some(42),
                status: "success".into(),
                status_source: "local".into(),
                result: "SUCCESS".into(),
                cause: String::new(),
                created_by: "tester".into(),
                created_at: String::new(),
                updated_at: String::new(),
                started_at: None,
                finished_at: Some("2026-01-01T00:00:00Z".into()),
                last_error_code: String::new(),
                last_error_message: String::new(),
            },
            None,
            connection.config_version,
            "",
            "{}",
        )
        .expect("insert successful build");

        let candidate = JenkinsService::create_artifact_deployment_candidate(
            &db,
            CreateJenkinsArtifactDeploymentCandidateInput {
                artifact_key: "artifact-available".into(),
            },
        )
        .expect("candidate should be created");
        assert_eq!(candidate.source_type, "local");
        assert_eq!(candidate.recipe, "systemd-binary");
        assert_eq!(candidate.artifact_dir, artifact_path.to_string_lossy());
        let config: serde_json::Value =
            serde_json::from_str(&candidate.config_json).expect("candidate config json");
        assert_eq!(config["source"], "jenkins-artifact");
        assert_eq!(config["artifactKey"], "artifact-available");
        assert_eq!(config["sha256"], "sha-test");
        assert_eq!(config["riskFlags"], json!(["executable_or_installer"]));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn artifact_deployment_candidate_rejects_failed_build() {
        let db = Database::init(":memory:").expect("init db");
        let connection = upsert_enabled_test_connection(&db);
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-jenkins-candidate-failed-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create candidate failed test dir");
        let artifact_path = root.join("release.zip");
        std::fs::write(&artifact_path, b"zip").expect("write artifact");
        db.upsert_jenkins_artifact_record(&JenkinsArtifact {
            id: 0,
            artifact_key: "artifact-failed-candidate".into(),
            request_id: "req-artifact".into(),
            connection_key: connection.connection_key.clone(),
            job_full_name: "folder/deploy".into(),
            build_number: 42,
            file_name: "release.zip".into(),
            relative_path: "dist/release.zip".into(),
            local_path: artifact_path.to_string_lossy().to_string(),
            size_bytes: Some(3),
            sha256: "sha-test".into(),
            source_url: String::new(),
            status: "available".into(),
            risk_flags: Vec::new(),
            downloaded_at: None,
            cleaned_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .expect("insert artifact");
        db.upsert_jenkins_build_run(
            &JenkinsBuild {
                run_key: "jenkins-run-failed-candidate".into(),
                request_id: "req-build".into(),
                connection_key: connection.connection_key.clone(),
                job_full_name: "folder/deploy".into(),
                queue_id: String::new(),
                build_number: Some(42),
                status: "completed".into(),
                status_source: "local".into(),
                result: "FAILURE".into(),
                cause: String::new(),
                created_by: "tester".into(),
                created_at: String::new(),
                updated_at: String::new(),
                started_at: None,
                finished_at: Some("2026-01-01T00:00:00Z".into()),
                last_error_code: String::new(),
                last_error_message: String::new(),
            },
            None,
            connection.config_version,
            "",
            "{}",
        )
        .expect("insert failed build");

        let error = JenkinsService::create_artifact_deployment_candidate(
            &db,
            CreateJenkinsArtifactDeploymentCandidateInput {
                artifact_key: "artifact-failed-candidate".into(),
            },
        )
        .expect_err("failed build should not create deployment candidate");
        assert!(error.to_string().contains("只有成功构建"));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn build_deployment_dry_run_rejects_failed_build_before_probe() {
        let db = Database::init(":memory:").expect("init db");
        let connection = upsert_enabled_test_connection(&db);
        let root = std::env::temp_dir().join(format!(
            "tauri-ssh-jenkins-dry-run-failed-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create dry-run test dir");
        let artifact_path = root.join("release.zip");
        std::fs::write(&artifact_path, b"zip").expect("write artifact");
        db.upsert_jenkins_artifact_record(&JenkinsArtifact {
            id: 0,
            artifact_key: "artifact-failed-build".into(),
            request_id: "req-artifact".into(),
            connection_key: connection.connection_key.clone(),
            job_full_name: "folder/deploy".into(),
            build_number: 42,
            file_name: "release.zip".into(),
            relative_path: "dist/release.zip".into(),
            local_path: artifact_path.to_string_lossy().to_string(),
            size_bytes: Some(3),
            sha256: "sha-test".into(),
            source_url: String::new(),
            status: "available".into(),
            risk_flags: Vec::new(),
            downloaded_at: None,
            cleaned_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .expect("insert artifact");
        db.upsert_jenkins_build_run(
            &JenkinsBuild {
                run_key: "jenkins-run-failed".into(),
                request_id: "req-build".into(),
                connection_key: connection.connection_key.clone(),
                job_full_name: "folder/deploy".into(),
                queue_id: String::new(),
                build_number: Some(42),
                status: "completed".into(),
                status_source: "local".into(),
                result: "FAILURE".into(),
                cause: String::new(),
                created_by: "tester".into(),
                created_at: String::new(),
                updated_at: String::new(),
                started_at: None,
                finished_at: Some("2026-01-01T00:00:00Z".into()),
                last_error_code: String::new(),
                last_error_message: String::new(),
            },
            None,
            connection.config_version,
            "",
            "{}",
        )
        .expect("insert failed build");

        let error = JenkinsService::create_build_deployment_dry_run(
            &db,
            CreateJenkinsBuildDeploymentDryRunInput {
                artifact_key: "artifact-failed-build".into(),
                server_alias: "prod-server".into(),
                deploy_root: None,
                domain: None,
                https_enabled: None,
                port: None,
                health_check_url: None,
            },
        )
        .await
        .expect_err("failed build should not enter deployment dry-run");
        assert!(error.to_string().contains("只有成功构建"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn redact_log_text_masks_key_value_secrets() {
        let text = JenkinsService::redact_log_text("token=abc password:secret ok");
        assert!(text.contains("token=[REDACTED]"));
        assert!(text.contains("password:[REDACTED]"));
    }

    #[test]
    fn build_analysis_record_saves_summary_without_log_body() {
        let db = Database::init(":memory:").expect("init db");
        let analysis = JenkinsBuildAnalysis {
            id: 0,
            analysis_key: "jenkins-analysis-test".into(),
            run_key: "jenkins-run-test".into(),
            request_id: "req-test".into(),
            connection_key: "jenkins-test".into(),
            job_full_name: "folder/deploy".into(),
            build_number: 42,
            provider_key: "provider".into(),
            provider_name: "Provider".into(),
            model: "model".into(),
            summary_markdown: "失败原因：编译错误。".into(),
            snippet_sha256: "abc123".into(),
            snippet_start_line: 10,
            snippet_end_line: 20,
            matched_lines: 2,
            created_by: "tester".into(),
            created_at: String::new(),
        };

        let saved = db
            .create_jenkins_build_analysis(&analysis)
            .expect("analysis should save");

        assert_eq!(saved.analysis_key, analysis.analysis_key);
        assert_eq!(saved.summary_markdown, "失败原因：编译错误。");
        assert_eq!(saved.snippet_sha256, "abc123");
        assert_eq!(saved.snippet_start_line, 10);
        assert_eq!(saved.snippet_end_line, 20);
    }

    #[test]
    fn latest_build_analysis_returns_newest_record_for_build() {
        let db = Database::init(":memory:").expect("init db");
        for (key, summary) in [
            ("jenkins-analysis-old", "旧总结"),
            ("jenkins-analysis-new", "新总结"),
        ] {
            db.create_jenkins_build_analysis(&JenkinsBuildAnalysis {
                id: 0,
                analysis_key: key.into(),
                run_key: "jenkins-run-test".into(),
                request_id: "req-test".into(),
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                build_number: 42,
                provider_key: "provider".into(),
                provider_name: "Provider".into(),
                model: "model".into(),
                summary_markdown: summary.into(),
                snippet_sha256: key.into(),
                snippet_start_line: 10,
                snippet_end_line: 20,
                matched_lines: 2,
                created_by: "tester".into(),
                created_at: String::new(),
            })
            .expect("analysis should save");
        }

        let latest = JenkinsService::get_latest_build_analysis(
            &db,
            GetJenkinsBuildInput {
                connection_key: "jenkins-test".into(),
                job_full_name: "folder/deploy".into(),
                build_number: 42,
            },
        )
        .expect("latest analysis query should succeed")
        .expect("latest analysis should exist");

        assert_eq!(latest.analysis_key, "jenkins-analysis-new");
        assert_eq!(latest.summary_markdown, "新总结");
    }
}
