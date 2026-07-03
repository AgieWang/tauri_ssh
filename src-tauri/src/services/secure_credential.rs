use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Instant;

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    CreateAuditLogInput, CreateSecureCredentialAuditLogInput, CreateSecureCredentialSessionInput,
    ListSecureCredentialAuditLogsInput, ListSecureCredentialSessionsInput,
    ListSecureCredentialsInput, RotateSecureCredentialInput, SecureCredential,
    SecureCredentialAuditLog, SecureCredentialGitReadInput, SecureCredentialGitWriteInput,
    SecureCredentialGitWriteResult, SecureCredentialHttpRequestInput,
    SecureCredentialHttpRequestResult, SecureCredentialHttpWriteInput, SecureCredentialOverview,
    SecureCredentialPolicySettings, SecureCredentialProviderReadResult,
    SecureCredentialProviderTestInput, SecureCredentialProviderTestResult,
    SecureCredentialRepository, SecureCredentialRepositoryListInput, SecureCredentialSession,
    SecureCredentialSessionStatus, SetSecureCredentialEnabledInput,
    UpdateSecureCredentialPolicySettingsInput, UpsertSecureCredentialInput,
};

const SECURE_CREDENTIAL_SECRET_SEED_KEY: &str = "secure_credential_secret_seed";

pub struct SecureCredentialService;

impl SecureCredentialService {
    pub fn overview(db: &Database) -> Result<SecureCredentialOverview, AppError> {
        db.get_secure_credential_overview()
    }

    pub fn policy_settings(db: &Database) -> Result<SecureCredentialPolicySettings, AppError> {
        db.get_secure_credential_policy_settings()
    }

    pub fn update_policy_settings(
        db: &Database,
        input: UpdateSecureCredentialPolicySettingsInput,
    ) -> Result<SecureCredentialPolicySettings, AppError> {
        db.update_secure_credential_policy_settings(&input)
    }

    pub fn list(
        db: &Database,
        input: Option<ListSecureCredentialsInput>,
    ) -> Result<Vec<SecureCredential>, AppError> {
        let filter = input.unwrap_or(ListSecureCredentialsInput {
            keyword: None,
            provider: None,
            status: None,
            allow_mcp: None,
        });
        let keyword = filter.keyword.unwrap_or_default().trim().to_lowercase();
        let provider = filter.provider.unwrap_or_default();
        let status = filter.status.unwrap_or_default();
        let rows = db.list_secure_credentials()?;
        Ok(rows
            .into_iter()
            .filter(|item| provider.is_empty() || item.provider == provider)
            .filter(|item| status.is_empty() || item.status == status)
            .filter(|item| {
                filter
                    .allow_mcp
                    .map_or(true, |value| item.allow_mcp == value)
            })
            .filter(|item| {
                if keyword.is_empty() {
                    return true;
                }
                [
                    item.credential_key.as_str(),
                    item.display_name.as_str(),
                    item.provider.as_str(),
                    item.account_name.as_str(),
                    item.folder.as_str(),
                    item.description.as_str(),
                ]
                .iter()
                .any(|value| value.to_lowercase().contains(&keyword))
                    || item
                        .tags
                        .iter()
                        .any(|value| value.to_lowercase().contains(&keyword))
            })
            .collect())
    }

    pub fn list_audit_logs(
        db: &Database,
        input: Option<ListSecureCredentialAuditLogsInput>,
    ) -> Result<Vec<SecureCredentialAuditLog>, AppError> {
        db.list_secure_credential_audit_logs(&input.unwrap_or(ListSecureCredentialAuditLogsInput {
            keyword: None,
            source: None,
            provider: None,
            credential_key: None,
            actor: None,
            action: None,
            risk: None,
            result: None,
            limit: Some(200),
        }))
    }

    pub fn upsert(
        db: &Database,
        input: UpsertSecureCredentialInput,
    ) -> Result<SecureCredential, AppError> {
        Self::validate_upsert(&input)?;
        let action = if input.id.is_some() {
            "credential_update"
        } else {
            "credential_create"
        };
        let encrypted_secret = match input
            .secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(secret) => {
                let encrypted = Self::encrypt_secret(db, secret)?;
                Some(encrypted)
            }
            None => None,
        };
        let encrypted_ref = encrypted_secret
            .as_ref()
            .map(|(nonce, ciphertext)| (nonce.as_str(), ciphertext.as_str()));
        let credential = db.upsert_secure_credential(&input, encrypted_ref)?;
        Self::record_audit(
            db,
            &credential,
            action,
            "L2",
            "success",
            0,
            json!({
                "displayName": credential.display_name,
                "provider": credential.provider,
                "allowMcp": credential.allow_mcp,
                "secretUpdated": encrypted_ref.is_some()
            }),
            None,
        );
        Ok(credential)
    }

    pub fn rotate(
        db: &Database,
        input: RotateSecureCredentialInput,
    ) -> Result<SecureCredential, AppError> {
        if input.credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput("凭证 Key 不能为空".into()));
        }
        if input.secret.trim().is_empty() {
            return Err(AppError::InvalidInput("新密钥不能为空".into()));
        }
        let encrypted_secret = Self::encrypt_secret(db, input.secret.trim())?;
        let credential = db.rotate_secure_credential(
            input.credential_key.trim(),
            (encrypted_secret.0.as_str(), encrypted_secret.1.as_str()),
        )?;
        Self::record_audit(
            db,
            &credential,
            "credential_rotate",
            "L2",
            "success",
            0,
            json!({"secretUpdated": true}),
            None,
        );
        Ok(credential)
    }

    pub fn set_enabled(
        db: &Database,
        input: SetSecureCredentialEnabledInput,
    ) -> Result<SecureCredential, AppError> {
        if input.credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput("凭证 Key 不能为空".into()));
        }
        let credential = db.set_secure_credential_enabled(&input)?;
        Self::record_audit(
            db,
            &credential,
            if input.enabled {
                "credential_enable"
            } else {
                "credential_disable"
            },
            "L2",
            "success",
            0,
            json!({"enabled": input.enabled}),
            None,
        );
        Ok(credential)
    }

    pub fn delete(db: &Database, credential_key: &str) -> Result<(), AppError> {
        if credential_key.trim().is_empty() {
            return Err(AppError::InvalidInput("凭证 Key 不能为空".into()));
        }
        let credential = db.get_secure_credential(credential_key.trim())?;
        if !db.delete_secure_credential(credential_key.trim())? {
            return Err(AppError::NotFound(format!(
                "安全凭证 '{}' 不存在",
                credential_key
            )));
        }
        if let Some(credential) = credential {
            Self::record_audit(
                db,
                &credential,
                "credential_delete",
                "L3",
                "success",
                0,
                json!({}),
                None,
            );
        }
        Ok(())
    }

    pub fn list_sessions(
        db: &Database,
        input: Option<ListSecureCredentialSessionsInput>,
    ) -> Result<Vec<SecureCredentialSession>, AppError> {
        let filter = input.unwrap_or(ListSecureCredentialSessionsInput {
            credential_key: None,
            status: None,
            caller: None,
        });
        let credential_key = filter.credential_key.unwrap_or_default();
        let status = filter.status.unwrap_or_default();
        let caller = filter.caller.unwrap_or_default();
        let rows = db.list_secure_credential_sessions()?;
        Ok(rows
            .into_iter()
            .filter(|item| credential_key.is_empty() || item.credential_key == credential_key)
            .filter(|item| status.is_empty() || item.status == status)
            .filter(|item| caller.is_empty() || item.caller == caller)
            .collect())
    }

    pub fn create_session(
        db: &Database,
        input: CreateSecureCredentialSessionInput,
    ) -> Result<SecureCredentialSession, AppError> {
        let credential_key = input.credential_key.trim();
        if credential_key.is_empty() {
            return Err(AppError::InvalidInput("凭证 Key 不能为空".into()));
        }
        let credential = db
            .get_secure_credential(credential_key)?
            .ok_or_else(|| AppError::NotFound(format!("安全凭证 '{}' 不存在", credential_key)))?;
        if !credential.enabled || credential.status != "active" {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 当前不可用",
                credential_key
            )));
        }
        if !credential.allow_mcp || credential.approval_policy == "blocked_for_mcp" {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 未允许 MCP 使用",
                credential_key
            )));
        }
        if !credential.has_secret {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 未保存密钥内容",
                credential_key
            )));
        }
        if credential.approval_policy == "all_requires_approval" {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 的策略要求所有 MCP 操作先审批",
                credential_key
            )));
        }
        let settings = db.get_secure_credential_policy_settings()?;
        let active_sessions = db
            .list_secure_credential_sessions()?
            .into_iter()
            .filter(|session| {
                session.credential_key == credential.credential_key && session.status == "active"
            })
            .count() as i64;
        if active_sessions >= settings.max_concurrent_sessions {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 已达到并发会话上限 {}",
                credential_key, settings.max_concurrent_sessions
            )));
        }

        let requested_scopes = if input.scopes.is_empty() {
            credential.scopes.clone()
        } else {
            input.scopes
        };
        for scope in &requested_scopes {
            if !credential.scopes.contains(scope) {
                return Err(AppError::InvalidInput(format!(
                    "请求范围 '{}' 未包含在凭证授权范围内",
                    scope
                )));
            }
        }
        let ttl_minutes = input
            .ttl_minutes
            .unwrap_or(settings.default_session_ttl_minutes)
            .clamp(1, 240);
        let caller = input
            .caller
            .unwrap_or_else(|| "local-user".into())
            .trim()
            .to_string();
        let caller = if caller.is_empty() {
            "local-user".to_string()
        } else {
            caller
        };
        let session_id = Self::new_session_id();
        let session = db.create_secure_credential_session(
            &session_id,
            &credential,
            &caller,
            &requested_scopes,
            ttl_minutes,
        )?;
        Self::record_audit(
            db,
            &credential,
            "session_create",
            "L1",
            "success",
            0,
            json!({
                "sessionId": session.session_id,
                "caller": session.caller,
                "scopes": session.scopes,
                "ttlMinutes": ttl_minutes,
                "expiresAt": session.expires_at
            }),
            None,
        );
        Ok(session)
    }

    pub fn session_status(
        db: &Database,
        session_id: &str,
    ) -> Result<SecureCredentialSessionStatus, AppError> {
        if session_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Session ID 不能为空".into()));
        }
        let session = db
            .get_secure_credential_session(session_id.trim())?
            .ok_or_else(|| AppError::NotFound(format!("会话 '{}' 不存在", session_id)))?;
        let (valid, reason) = match session.status.as_str() {
            "active" => (true, "active".to_string()),
            "expired" => (false, "会话已过期".to_string()),
            "revoked" => (false, "会话已吊销".to_string()),
            other => (false, format!("会话状态不可用: {}", other)),
        };
        if valid {
            db.touch_secure_credential_session(&session.session_id)?;
        }
        Ok(SecureCredentialSessionStatus {
            session,
            valid,
            reason,
        })
    }

    pub fn revoke_session(
        db: &Database,
        session_id: &str,
    ) -> Result<SecureCredentialSession, AppError> {
        if session_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Session ID 不能为空".into()));
        }
        let session = db.revoke_secure_credential_session(session_id.trim())?;
        if let Some(credential) = db.get_secure_credential(&session.credential_key)? {
            Self::record_audit(
                db,
                &credential,
                "session_revoke",
                "L1",
                "success",
                0,
                json!({"sessionId": session.session_id, "caller": session.caller}),
                None,
            );
        }
        Ok(session)
    }

    pub async fn test_provider(
        db: &Database,
        input: SecureCredentialProviderTestInput,
    ) -> Result<SecureCredentialProviderTestResult, AppError> {
        let credential = Self::get_usable_credential(db, &input.credential_key, false)?;
        Self::ensure_base_url_allowed(db, &credential)?;
        Self::ensure_rate_limit(db, &credential)?;
        let secret = Self::get_secret(db, &credential.credential_key)?;
        let started = Instant::now();
        let client = Self::http_client()?;
        let (url, request) = match credential.provider.as_str() {
            "github" => {
                let url = format!("{}/user", Self::api_base_url(&credential)?);
                let req = client.get(&url).headers(Self::github_headers(&secret)?);
                (url, req)
            }
            "gitlab" => {
                let url = format!("{}/user", Self::api_base_url(&credential)?);
                let req = client.get(&url).headers(Self::gitlab_headers(&secret)?);
                (url, req)
            }
            "gitcode" | "gitee" => {
                let url = format!("{}/user", Self::api_base_url(&credential)?);
                let req = client.get(&url).headers(Self::bearer_headers(&secret)?);
                (url, req)
            }
            "http_api" | "custom" => {
                let url = Self::api_base_url(&credential)?;
                let req = client.get(&url).headers(Self::bearer_headers(&secret)?);
                (url, req)
            }
            _ => {
                return Err(AppError::InvalidInput(format!(
                    "暂不支持 Provider '{}'",
                    credential.provider
                )))
            }
        };
        let response = request.send().await.map_err(Self::http_error)?;
        let status_code = response.status().as_u16();
        let text = response.text().await.map_err(Self::http_error)?;
        let detail = Self::parse_and_redact_response(&text);
        let account = Self::account_from_detail(&credential.provider, &detail)
            .unwrap_or_else(|| credential.account_name.clone());
        let ok = (200..300).contains(&status_code);
        let result = SecureCredentialProviderTestResult {
            ok,
            credential_key: credential.credential_key.clone(),
            provider: credential.provider.clone(),
            account,
            status_code: Some(status_code),
            latency_ms: started.elapsed().as_millis() as i64,
            message: if ok {
                "连接测试成功".into()
            } else {
                format!("连接测试失败: HTTP {}", status_code)
            },
            detail: json!({
                "url": url,
                "response": detail
            }),
        };
        Self::record_audit(
            db,
            &credential,
            "provider_test",
            "readonly",
            if result.ok { "success" } else { "failure" },
            result.latency_ms,
            json!({
                "statusCode": result.status_code,
                "account": result.account,
                "url": url
            }),
            None,
        );
        Ok(result)
    }

    pub async fn test_provider_by_session(
        db: &Database,
        session_id: &str,
    ) -> Result<SecureCredentialProviderTestResult, AppError> {
        let session = Self::require_valid_session(db, session_id)?;
        Self::test_provider(
            db,
            SecureCredentialProviderTestInput {
                credential_key: session.credential_key,
            },
        )
        .await
    }

    pub async fn list_repositories(
        db: &Database,
        input: SecureCredentialRepositoryListInput,
    ) -> Result<Vec<SecureCredentialRepository>, AppError> {
        let session = Self::require_valid_session(db, &input.session_id)?;
        let credential = db
            .get_secure_credential(&session.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", session.credential_key))
            })?;
        Self::ensure_read_allowed(db, &credential)?;
        Self::ensure_base_url_allowed(db, &credential)?;
        Self::ensure_rate_limit(db, &credential)?;
        let secret = Self::get_secret(db, &credential.credential_key)?;
        let page = input.page.unwrap_or(1).clamp(1, 1000);
        let settings = db.get_secure_credential_policy_settings()?;
        let per_page = input
            .per_page
            .unwrap_or(50)
            .clamp(1, settings.max_response_items.min(100));
        let client = Self::http_client()?;
        let (url, request) = match credential.provider.as_str() {
            "github" => {
                let url = format!(
                    "{}/user/repos?page={}&per_page={}&sort=updated",
                    Self::api_base_url(&credential)?,
                    page,
                    per_page
                );
                let req = client.get(&url).headers(Self::github_headers(&secret)?);
                (url, req)
            }
            "gitlab" => {
                let url = format!(
                    "{}/projects?membership=true&simple=true&page={}&per_page={}",
                    Self::api_base_url(&credential)?,
                    page,
                    per_page
                );
                let req = client.get(&url).headers(Self::gitlab_headers(&secret)?);
                (url, req)
            }
            "gitcode" | "gitee" => {
                let url = format!(
                    "{}/user/repos?page={}&per_page={}",
                    Self::api_base_url(&credential)?,
                    page,
                    per_page
                );
                let req = client.get(&url).headers(Self::bearer_headers(&secret)?);
                (url, req)
            }
            _ => {
                return Err(AppError::InvalidInput(format!(
                    "Provider '{}' 不支持仓库列表",
                    credential.provider
                )))
            }
        };
        let response = request.send().await.map_err(Self::http_error)?;
        let status = response.status();
        let text = response.text().await.map_err(Self::http_error)?;
        if !status.is_success() {
            return Err(AppError::Custom(format!(
                "读取仓库列表失败: HTTP {} {}",
                status.as_u16(),
                Self::redact_text(&text)
            )));
        }
        let value: Value = serde_json::from_str(&text)?;
        let repositories = Self::map_repositories(&credential.provider, &value, &url);
        Self::record_audit(
            db,
            &credential,
            "git_repositories_list",
            "readonly",
            "success",
            0,
            json!({
                "sessionId": session.session_id,
                "page": page,
                "perPage": per_page,
                "count": repositories.len()
            }),
            None,
        );
        Ok(repositories)
    }

    pub async fn git_readonly_request(
        db: &Database,
        input: SecureCredentialGitReadInput,
    ) -> Result<SecureCredentialProviderReadResult, AppError> {
        let session = Self::require_valid_session(db, &input.session_id)?;
        let credential = db
            .get_secure_credential(&session.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", session.credential_key))
            })?;
        if !["github", "gitlab", "gitcode", "gitee"].contains(&credential.provider.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "Provider '{}' 不支持 Git 只读工具",
                credential.provider
            )));
        }
        Self::ensure_read_allowed(db, &credential)?;
        Self::ensure_base_url_allowed(db, &credential)?;
        Self::ensure_rate_limit(db, &credential)?;
        let secret = Self::get_secret(db, &credential.credential_key)?;
        let settings = db.get_secure_credential_policy_settings()?;
        let page = input.page.unwrap_or(1).clamp(1, 1000);
        let per_page = input
            .per_page
            .unwrap_or(50)
            .clamp(1, settings.max_response_items.min(100));
        let url = Self::git_readonly_url(&credential, &input, page, per_page)?;
        let request = match credential.provider.as_str() {
            "github" => Self::http_client()?
                .get(&url)
                .headers(Self::github_headers(&secret)?),
            "gitlab" => Self::http_client()?
                .get(&url)
                .headers(Self::gitlab_headers(&secret)?),
            "gitcode" | "gitee" => Self::http_client()?
                .get(&url)
                .headers(Self::bearer_headers(&secret)?),
            _ => unreachable!(),
        };
        let response = request.send().await.map_err(Self::http_error)?;
        let status_code = response.status().as_u16();
        let mut text = response.text().await.map_err(Self::http_error)?;
        let truncated = text.len() > 65_536;
        if truncated {
            text.truncate(65_536);
        }
        let result = SecureCredentialProviderReadResult {
            provider: credential.provider.clone(),
            resource: input.resource.clone(),
            status_code,
            url,
            body: Self::parse_and_redact_response(&text),
            truncated,
        };
        Self::record_audit(
            db,
            &credential,
            &format!("git_{}_read", input.resource),
            "readonly",
            if (200..300).contains(&status_code) {
                "success"
            } else {
                "failure"
            },
            0,
            json!({
                "sessionId": session.session_id,
                "resource": input.resource,
                "repo": input.repo,
                "path": input.path,
                "statusCode": status_code,
                "truncated": truncated
            }),
            None,
        );
        Ok(result)
    }

    pub async fn http_readonly_request(
        db: &Database,
        input: SecureCredentialHttpRequestInput,
    ) -> Result<SecureCredentialHttpRequestResult, AppError> {
        let session = Self::require_valid_session(db, &input.session_id)?;
        let credential = db
            .get_secure_credential(&session.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", session.credential_key))
            })?;
        if !["http_api", "custom"].contains(&credential.provider.as_str()) {
            return Err(AppError::InvalidInput(
                "HTTP API 只读请求只允许 http_api / custom Provider".into(),
            ));
        }
        Self::ensure_read_allowed(db, &credential)?;
        Self::ensure_base_url_allowed(db, &credential)?;
        Self::ensure_rate_limit(db, &credential)?;
        let secret = Self::get_secret(db, &credential.credential_key)?;
        let url = Self::build_http_api_url(&credential, &input.path, input.query_json.as_ref())?;
        let response = Self::http_client()?
            .get(&url)
            .headers(Self::bearer_headers(&secret)?)
            .send()
            .await
            .map_err(Self::http_error)?;
        let status_code = response.status().as_u16();
        let mut text = response.text().await.map_err(Self::http_error)?;
        let truncated = text.len() > 65_536;
        if truncated {
            text.truncate(65_536);
        }
        let result = SecureCredentialHttpRequestResult {
            status_code,
            url,
            body: Self::parse_and_redact_response(&text),
            truncated,
        };
        Self::record_audit(
            db,
            &credential,
            "http_readonly_request",
            "readonly",
            if (200..300).contains(&result.status_code) {
                "success"
            } else {
                "failure"
            },
            0,
            json!({
                "sessionId": session.session_id,
                "path": input.path,
                "statusCode": result.status_code,
                "truncated": result.truncated
            }),
            None,
        );
        Ok(result)
    }

    pub async fn http_write_request(
        db: &Database,
        input: SecureCredentialHttpWriteInput,
    ) -> Result<SecureCredentialHttpRequestResult, AppError> {
        let method = input.method.trim().to_ascii_uppercase();
        if !["POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
            return Err(AppError::InvalidInput(
                "HTTP 写请求只支持 POST / PUT / PATCH / DELETE".into(),
            ));
        }
        let session = Self::require_valid_session(db, &input.session_id)?;
        let credential = db
            .get_secure_credential(&session.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", session.credential_key))
            })?;
        if !["http_api", "custom"].contains(&credential.provider.as_str()) {
            return Err(AppError::InvalidInput(
                "HTTP API 写请求只允许 http_api / custom Provider".into(),
            ));
        }
        Self::ensure_base_url_allowed(db, &credential)?;
        Self::ensure_rate_limit(db, &credential)?;
        let secret = Self::get_secret(db, &credential.credential_key)?;
        let url = Self::build_http_api_url(&credential, &input.path, input.query_json.as_ref())?;
        let client = Self::http_client()?;
        let mut request = match method.as_str() {
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            "DELETE" => client.delete(&url),
            _ => unreachable!(),
        };
        request = request.headers(Self::bearer_headers(&secret)?);
        if let Some(body) = input.body_json.as_ref() {
            request = request.json(body);
        }
        let response = request.send().await.map_err(Self::http_error)?;
        let status_code = response.status().as_u16();
        let mut text = response.text().await.map_err(Self::http_error)?;
        let truncated = text.len() > 65_536;
        if truncated {
            text.truncate(65_536);
        }
        let body = Self::parse_and_redact_response(&text);
        if !(200..300).contains(&status_code) {
            return Err(AppError::Custom(format!(
                "HTTP API 写请求失败: HTTP {} {}",
                status_code, body
            )));
        }
        let result = SecureCredentialHttpRequestResult {
            status_code,
            url,
            body,
            truncated,
        };
        Self::record_audit(
            db,
            &credential,
            "http_write_request",
            if method == "DELETE" { "high" } else { "medium" },
            "success",
            0,
            json!({
                "sessionId": session.session_id,
                "method": method,
                "path": input.path,
                "statusCode": result.status_code,
                "truncated": result.truncated
            }),
            None,
        );
        Ok(result)
    }

    pub async fn execute_git_write(
        db: &Database,
        input: SecureCredentialGitWriteInput,
    ) -> Result<SecureCredentialGitWriteResult, AppError> {
        Self::validate_git_write_operation(&input)?;
        let session = Self::require_valid_session(db, &input.session_id)?;
        let credential = db
            .get_secure_credential(&session.credential_key)?
            .ok_or_else(|| {
                AppError::NotFound(format!("安全凭证 '{}' 不存在", session.credential_key))
            })?;
        if !["github", "gitlab", "gitcode", "gitee"].contains(&credential.provider.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "Provider '{}' 不支持 Git 写操作",
                credential.provider
            )));
        }
        Self::ensure_base_url_allowed(db, &credential)?;
        Self::ensure_git_operation_allowed_by_policy(db, &input)?;
        Self::ensure_rate_limit(db, &credential)?;
        let secret = Self::get_secret(db, &credential.credential_key)?;
        let (method, url, body) = Self::git_write_request(&credential, &input)?;
        let client = Self::http_client()?;
        let mut request = match method.as_str() {
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            "DELETE" => client.delete(&url),
            _ => return Err(AppError::InvalidInput("不支持的 Git 写入 HTTP 方法".into())),
        };
        request = match credential.provider.as_str() {
            "github" => request.headers(Self::github_headers(&secret)?),
            "gitlab" => request.headers(Self::gitlab_headers(&secret)?),
            "gitcode" | "gitee" => request.headers(Self::bearer_headers(&secret)?),
            _ => request,
        };
        let response = request.json(&body).send().await.map_err(Self::http_error)?;
        let status_code = response.status().as_u16();
        let text = response.text().await.map_err(Self::http_error)?;
        let body = Self::parse_and_redact_response(&text);
        if !(200..300).contains(&status_code) {
            return Err(AppError::Custom(format!(
                "Git 写操作失败: HTTP {} {}",
                status_code, body
            )));
        }
        let result = SecureCredentialGitWriteResult {
            provider: credential.provider.clone(),
            operation: input.operation.clone(),
            repo: input.repo.clone(),
            status_code,
            body,
        };
        Self::record_audit(
            db,
            &credential,
            "git_write_execute",
            if Self::is_high_risk_git_operation(&result.operation) {
                "high"
            } else {
                "medium"
            },
            "success",
            0,
            json!({
                "sessionId": session.session_id,
                "operation": result.operation.clone(),
                "repo": result.repo.clone(),
                "statusCode": result.status_code
            }),
            None,
        );
        Ok(result)
    }

    #[allow(dead_code)]
    pub fn get_secret(db: &Database, credential_key: &str) -> Result<String, AppError> {
        let row = db
            .get_secure_credential_secret_row(credential_key.trim())?
            .ok_or_else(|| AppError::NotFound(format!("安全凭证 '{}' 不存在", credential_key)))?;
        match (row.secret_nonce, row.secret_ciphertext) {
            (Some(nonce), Some(ciphertext)) => Self::decrypt_secret(db, &nonce, &ciphertext),
            _ => Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 未保存可用密钥内容",
                credential_key
            ))),
        }
    }

    /// 解析 Git 工作区保存的凭证引用。
    ///
    /// 历史数据里 credential_key 可能存的是真实凭证 Key，也可能是 "GitLab" 这类
    /// Provider/显示名。Git 操作前统一解析成真实安全凭证，避免把显示名当主键查询。
    pub fn resolve_git_credential(
        db: &Database,
        credential_ref: &str,
        remote_url: &str,
    ) -> Result<SecureCredential, AppError> {
        let reference = credential_ref.trim();
        if reference.is_empty() {
            return Err(AppError::InvalidInput("Git 凭证引用不能为空".into()));
        }

        if let Some(credential) = db.get_secure_credential(reference)? {
            return Ok(credential);
        }

        let normalized_ref = normalize_credential_reference(reference);
        let remote_host = normalize_remote_host(remote_url).unwrap_or_default();
        let remote_provider = provider_from_remote(remote_url);
        let candidates = db
            .list_secure_credentials()?
            .into_iter()
            .filter(|credential| {
                ["github", "gitlab", "gitcode", "gitee"].contains(&credential.provider.as_str())
            })
            .filter(|credential| {
                credential.enabled && credential.status == "active" && credential.has_secret
            })
            .filter(|credential| {
                let same_provider =
                    normalize_credential_reference(&credential.provider) == normalized_ref;
                let same_display =
                    normalize_credential_reference(&credential.display_name) == normalized_ref;
                let same_key =
                    normalize_credential_reference(&credential.credential_key) == normalized_ref;
                same_provider || same_display || same_key
            })
            .filter(|credential| {
                if let Some(provider) = remote_provider {
                    credential.provider == provider
                } else {
                    true
                }
            })
            .filter(|credential| {
                let base_host = normalize_remote_host(&credential.base_url).unwrap_or_default();
                base_host.is_empty() || remote_host.is_empty() || base_host == remote_host
            })
            .collect::<Vec<_>>();

        match candidates.as_slice() {
            [credential] => Ok(credential.clone()),
            [] => Err(AppError::NotFound(format!(
                "未找到可用于远程仓库 '{}' 的 Git 凭证引用 '{}'，请在 Git 工作区绑定具体凭证 Key",
                sanitize_remote_for_message(remote_url),
                reference
            ))),
            _ => Err(AppError::InvalidInput(format!(
                "Git 凭证引用 '{}' 匹配到多个凭证，请在 Git 工作区绑定具体凭证 Key",
                reference
            ))),
        }
    }

    fn validate_upsert(input: &UpsertSecureCredentialInput) -> Result<(), AppError> {
        let key = input.credential_key.trim();
        if key.is_empty() {
            return Err(AppError::InvalidInput("凭证 Key 不能为空".into()));
        }
        if key.contains(char::is_whitespace) {
            return Err(AppError::InvalidInput("凭证 Key 不能包含空白字符".into()));
        }
        if input.display_name.trim().is_empty() {
            return Err(AppError::InvalidInput("显示名称不能为空".into()));
        }
        if !["github", "gitlab", "gitcode", "gitee", "http_api", "custom"]
            .contains(&input.provider.as_str())
        {
            return Err(AppError::InvalidInput("Provider 类型无效".into()));
        }
        if ![
            "token",
            "api_key",
            "bearer_token",
            "basic_auth",
            "custom_secret",
            "session_reference",
        ]
        .contains(&input.credential_type.as_str())
        {
            return Err(AppError::InvalidInput("凭证类型无效".into()));
        }
        if ![
            "active",
            "disabled",
            "rotation_due",
            "expired",
            "test_failed",
        ]
        .contains(&input.status.as_deref().unwrap_or("active"))
        {
            return Err(AppError::InvalidInput("凭证状态无效".into()));
        }
        if ![
            "readonly_auto",
            "write_requires_approval",
            "all_requires_approval",
            "blocked_for_mcp",
        ]
        .contains(
            &input
                .approval_policy
                .as_deref()
                .unwrap_or("write_requires_approval"),
        ) {
            return Err(AppError::InvalidInput("审批策略无效".into()));
        }
        Ok(())
    }

    fn new_session_id() -> String {
        let mut bytes = [0u8; 18];
        rand::thread_rng().fill_bytes(&mut bytes);
        format!("sess_{}", general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }

    fn get_usable_credential(
        db: &Database,
        credential_key: &str,
        require_mcp: bool,
    ) -> Result<SecureCredential, AppError> {
        let credential = db
            .get_secure_credential(credential_key.trim())?
            .ok_or_else(|| AppError::NotFound(format!("安全凭证 '{}' 不存在", credential_key)))?;
        if !credential.enabled || credential.status != "active" {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 当前不可用",
                credential.credential_key
            )));
        }
        if require_mcp && (!credential.allow_mcp || credential.approval_policy == "blocked_for_mcp")
        {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 未允许 MCP 使用",
                credential.credential_key
            )));
        }
        if !credential.has_secret {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 未保存密钥内容",
                credential.credential_key
            )));
        }
        Ok(credential)
    }

    fn require_valid_session(
        db: &Database,
        session_id: &str,
    ) -> Result<SecureCredentialSession, AppError> {
        let status = Self::session_status(db, session_id)?;
        if !status.valid {
            return Err(AppError::InvalidInput(status.reason));
        }
        Ok(status.session)
    }

    fn http_client() -> Result<reqwest::Client, AppError> {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(Self::http_error)
    }

    fn api_base_url(credential: &SecureCredential) -> Result<String, AppError> {
        let configured = credential.base_url.trim().trim_end_matches('/').to_string();
        let url = match credential.provider.as_str() {
            "github" => {
                if configured.is_empty() {
                    "https://api.github.com".into()
                } else {
                    configured
                }
            }
            "gitlab" => {
                if configured.is_empty() {
                    "https://gitlab.com/api/v4".into()
                } else if configured.ends_with("/api/v4") {
                    configured
                } else {
                    format!("{}/api/v4", configured)
                }
            }
            "gitcode" => {
                if configured.is_empty() {
                    "https://api.gitcode.com/api/v5".into()
                } else if configured.ends_with("/api/v5") {
                    configured
                } else {
                    format!("{}/api/v5", configured)
                }
            }
            "gitee" => {
                if configured.is_empty() {
                    "https://gitee.com/api/v5".into()
                } else if configured.ends_with("/api/v5") {
                    configured
                } else {
                    format!("{}/api/v5", configured)
                }
            }
            "http_api" | "custom" => configured,
            _ => configured,
        };
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err(AppError::InvalidInput(
                "API Base URL 必须以 http:// 或 https:// 开头".into(),
            ));
        }
        Ok(url)
    }

    fn github_headers(secret: &str) -> Result<HeaderMap, AppError> {
        let mut headers = Self::bearer_headers(secret)?;
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        Ok(headers)
    }

    fn gitlab_headers(secret: &str) -> Result<HeaderMap, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("tauri-ssh"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            "PRIVATE-TOKEN",
            HeaderValue::from_str(secret)
                .map_err(|_| AppError::InvalidInput("Token 不能作为 HTTP Header 使用".into()))?,
        );
        Ok(headers)
    }

    fn bearer_headers(secret: &str) -> Result<HeaderMap, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("tauri-ssh"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", secret))
                .map_err(|_| AppError::InvalidInput("Token 不能作为 HTTP Header 使用".into()))?,
        );
        Ok(headers)
    }

    fn account_from_detail(provider: &str, detail: &Value) -> Option<String> {
        let object = detail.as_object()?;
        match provider {
            "github" | "gitcode" | "gitee" => object
                .get("login")
                .or_else(|| object.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string),
            "gitlab" => object
                .get("username")
                .or_else(|| object.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => object
                .get("name")
                .or_else(|| object.get("account"))
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }

    fn parse_and_redact_response(text: &str) -> Value {
        match serde_json::from_str::<Value>(text) {
            Ok(value) => Self::redact_json(value),
            Err(_) => Value::String(Self::redact_text(text)),
        }
    }

    fn redact_json(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut redacted = serde_json::Map::new();
                for (key, value) in map {
                    if Self::is_sensitive_key(&key) {
                        redacted.insert(key, Value::String("[REDACTED]".into()));
                    } else {
                        redacted.insert(key, Self::redact_json(value));
                    }
                }
                Value::Object(redacted)
            }
            Value::Array(values) => {
                Value::Array(values.into_iter().map(Self::redact_json).collect())
            }
            Value::String(value) => Value::String(Self::redact_text(&value)),
            other => other,
        }
    }

    fn is_sensitive_key(key: &str) -> bool {
        let key = key.to_lowercase();
        [
            "token",
            "password",
            "secret",
            "private_key",
            "authorization",
            "cookie",
            "access_token",
            "refresh_token",
        ]
        .iter()
        .any(|needle| key.contains(needle))
    }

    fn redact_text(text: &str) -> String {
        let mut value = text.to_string();
        for marker in ["ghp_", "glpat-", "Bearer ", "PRIVATE-TOKEN"] {
            if let Some(index) = value.find(marker) {
                let end = (index + marker.len() + 12).min(value.len());
                value.replace_range(index..end, "[REDACTED]");
            }
        }
        value
    }

    fn map_repositories(
        provider: &str,
        value: &Value,
        source_url: &str,
    ) -> Vec<SecureCredentialRepository> {
        let rows = value.as_array().cloned().unwrap_or_default();
        rows.into_iter()
            .filter_map(|item| {
                let object = item.as_object()?;
                let id = object
                    .get("id")
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| value.to_string())
                    })
                    .unwrap_or_default();
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let full_name = object
                    .get("full_name")
                    .or_else(|| object.get("path_with_namespace"))
                    .or_else(|| object.get("human_name"))
                    .and_then(Value::as_str)
                    .unwrap_or(&name)
                    .to_string();
                let web_url = object
                    .get("html_url")
                    .or_else(|| object.get("web_url"))
                    .and_then(Value::as_str)
                    .unwrap_or(source_url)
                    .to_string();
                let visibility = object
                    .get("visibility")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let default_branch = object
                    .get("default_branch")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let permissions = match provider {
                    "github" => object
                        .get("permissions")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                    "gitlab" => object
                        .get("permissions")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                    _ => json!({}),
                };
                Some(SecureCredentialRepository {
                    id,
                    name,
                    full_name,
                    web_url,
                    visibility,
                    default_branch,
                    permissions: Self::redact_json(permissions),
                })
            })
            .collect()
    }

    fn validate_git_write_operation(input: &SecureCredentialGitWriteInput) -> Result<(), AppError> {
        if input.session_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Session ID 不能为空".into()));
        }
        if input.repo.trim().is_empty() {
            return Err(AppError::InvalidInput("仓库不能为空".into()));
        }
        if ![
            "create_issue",
            "create_branch",
            "commit_file",
            "create_pr",
            "update_pr",
            "merge_pr",
            "create_tag",
            "create_release",
            "trigger_workflow",
            "delete_branch",
            "delete_tag",
            "delete_release",
            "update_ref",
            "update_repo_settings",
        ]
        .contains(&input.operation.as_str())
        {
            return Err(AppError::InvalidInput("不支持的 Git 写操作".into()));
        }
        if input.operation == "commit_file" {
            Self::payload_string(&input.payload, "branch")?;
            Self::payload_string(&input.payload, "baseSha")?;
            let path = Self::payload_string(&input.payload, "path")?;
            let content = Self::payload_string(&input.payload, "content")?;
            let expected_hash = Self::payload_string(&input.payload, "contentSha256")?;
            let actual_hash = Self::sha256_hex(content.as_bytes());
            if !expected_hash.eq_ignore_ascii_case(&actual_hash) {
                return Err(AppError::InvalidInput(format!(
                    "文件 '{}' 内容 SHA-256 不匹配，请重新生成审批请求",
                    path
                )));
            }
        }
        if input.operation == "merge_pr" {
            Self::payload_i64(&input.payload, "number")?;
            Self::payload_string(&input.payload, "head")?;
            Self::payload_string(&input.payload, "base")?;
            Self::payload_string(&input.payload, "headSha")?;
        }
        Ok(())
    }

    fn ensure_git_operation_allowed_by_policy(
        db: &Database,
        input: &SecureCredentialGitWriteInput,
    ) -> Result<(), AppError> {
        if !Self::is_high_risk_git_operation(&input.operation) {
            if input.operation == "commit_file" {
                let branch = Self::payload_string(&input.payload, "branch")?;
                if ["main", "master", "develop"].contains(&branch.as_str()) {
                    let settings = db.get_secure_credential_policy_settings()?;
                    if !settings.allow_default_branch_commits {
                        return Err(AppError::InvalidInput(
                            "默认/保护分支直接提交默认拒绝，请通过分支 + PR/MR 流程或在策略页显式允许"
                                .into(),
                        ));
                    }
                }
            }
            return Ok(());
        }
        let settings = db.get_secure_credential_policy_settings()?;
        if !settings.allow_high_risk_repo_ops {
            return Err(AppError::InvalidInput(
                "当前策略禁止此类高风险仓库操作".into(),
            ));
        }
        let allowed = match input.operation.as_str() {
            "delete_branch" => settings.allow_delete_branch,
            "delete_tag" => settings.allow_delete_tag,
            "delete_release" => settings.allow_delete_release,
            "update_ref" => settings.allow_update_ref,
            "update_repo_settings" => settings.allow_update_repo_settings,
            _ => false,
        };
        if !allowed {
            return Err(AppError::InvalidInput(
                "当前策略禁止此类高风险仓库操作".into(),
            ));
        }
        Ok(())
    }

    fn is_high_risk_git_operation(operation: &str) -> bool {
        matches!(
            operation,
            "delete_branch"
                | "delete_tag"
                | "delete_release"
                | "update_ref"
                | "update_repo_settings"
        )
    }

    fn ensure_read_allowed(db: &Database, credential: &SecureCredential) -> Result<(), AppError> {
        let settings = db.get_secure_credential_policy_settings()?;
        if !settings.allow_readonly_auto || settings.require_approval_for_all {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 当前策略要求审批后才能执行只读 Provider 调用",
                credential.credential_key
            )));
        }
        if credential.approval_policy == "all_requires_approval" {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 的审批策略要求所有操作审批",
                credential.credential_key
            )));
        }
        Ok(())
    }

    fn ensure_base_url_allowed(
        db: &Database,
        credential: &SecureCredential,
    ) -> Result<(), AppError> {
        let settings = db.get_secure_credential_policy_settings()?;
        if settings.http_allowed_domains.is_empty() {
            return Ok(());
        }
        let url = Self::api_base_url(credential)?;
        let parsed = reqwest::Url::parse(&url)
            .map_err(|_| AppError::InvalidInput("API Base URL 格式无效".into()))?;
        let host = parsed.host_str().unwrap_or_default();
        let allowed = settings.http_allowed_domains.iter().any(|domain| {
            let normalized = domain.trim().trim_start_matches("*.").to_ascii_lowercase();
            let host = host.to_ascii_lowercase();
            host == normalized || host.ends_with(&format!(".{}", normalized))
        });
        if !allowed {
            return Err(AppError::InvalidInput(format!(
                "HTTP API 域名 '{}' 不在安全凭证白名单内",
                host
            )));
        }
        Ok(())
    }

    fn ensure_rate_limit(db: &Database, credential: &SecureCredential) -> Result<(), AppError> {
        let settings = db.get_secure_credential_policy_settings()?;
        let recent = db.count_secure_credential_recent_calls(&credential.credential_key, 60)?;
        if recent >= settings.rate_limit_per_minute {
            return Err(AppError::InvalidInput(format!(
                "安全凭证 '{}' 已达到单分钟调用限制 {}",
                credential.credential_key, settings.rate_limit_per_minute
            )));
        }
        Ok(())
    }

    fn record_audit(
        db: &Database,
        credential: &SecureCredential,
        action: &str,
        risk: &str,
        result: &str,
        duration_ms: i64,
        detail: Value,
        approval_id: Option<i64>,
    ) {
        let detail_json = Self::redact_text(&detail.to_string());
        let _ = db.create_secure_credential_audit_log(&CreateSecureCredentialAuditLogInput {
            actor: "local-user".into(),
            source: "secure_credential".into(),
            provider: credential.provider.clone(),
            credential_key: credential.credential_key.clone(),
            action: action.into(),
            risk: risk.into(),
            result: result.into(),
            duration_ms,
            request_id: None,
            approval_id,
            detail_json: Some(detail_json.clone()),
        });
        let _ = db.create_audit_log(&CreateAuditLogInput {
            actor: "local-user".into(),
            source: "secure_credential".into(),
            server_alias: String::new(),
            action: action.into(),
            risk: risk.into(),
            result: if result == "success" {
                "成功".into()
            } else {
                "失败".into()
            },
            summary: format!("安全凭证 {} {}", credential.credential_key, action),
            detail_json: Some(detail_json),
            request_id: None,
            approval_id,
        });
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    fn git_write_request(
        credential: &SecureCredential,
        input: &SecureCredentialGitWriteInput,
    ) -> Result<(String, String, Value), AppError> {
        let base = Self::api_base_url(credential)?;
        match credential.provider.as_str() {
            "github" => Self::github_write_request(&base, input),
            "gitlab" => Self::gitlab_write_request(&base, input),
            "gitcode" | "gitee" => Self::gitcode_write_request(&base, input),
            _ => Err(AppError::InvalidInput("不支持的 Git Provider".into())),
        }
    }

    fn git_readonly_url(
        credential: &SecureCredential,
        input: &SecureCredentialGitReadInput,
        page: i64,
        per_page: i64,
    ) -> Result<String, AppError> {
        let base = Self::api_base_url(credential)?;
        let resource = input.resource.as_str();
        let repo = input.repo.as_deref().unwrap_or("").trim();
        let state = input.state.as_deref().unwrap_or("open");
        match credential.provider.as_str() {
            "github" | "gitcode" | "gitee" => {
                let repo_required = || {
                    if repo.is_empty() {
                        Err(AppError::InvalidInput("repo 不能为空".into()))
                    } else {
                        Ok(repo)
                    }
                };
                match resource {
                    "repos" => Ok(format!(
                        "{}/user/repos?page={}&per_page={}",
                        base, page, per_page
                    )),
                    "repo_detail" => Ok(format!("{}/repos/{}", base, repo_required()?)),
                    "branches" => Ok(format!(
                        "{}/repos/{}/branches?page={}&per_page={}",
                        base,
                        repo_required()?,
                        page,
                        per_page
                    )),
                    "file" => Ok(format!(
                        "{}/repos/{}/contents/{}?ref={}",
                        base,
                        repo_required()?,
                        Self::payload_like_path(input.path.as_deref(), "path")?,
                        input.reference.as_deref().unwrap_or("HEAD")
                    )),
                    "commits" => Ok(format!(
                        "{}/repos/{}/commits?page={}&per_page={}",
                        base,
                        repo_required()?,
                        page,
                        per_page
                    )),
                    "pull_requests" => Ok(format!(
                        "{}/repos/{}/pulls?state={}&page={}&per_page={}",
                        base,
                        repo_required()?,
                        state,
                        page,
                        per_page
                    )),
                    "issues" => Ok(format!(
                        "{}/repos/{}/issues?state={}&page={}&per_page={}",
                        base,
                        repo_required()?,
                        state,
                        page,
                        per_page
                    )),
                    "releases" => Ok(format!(
                        "{}/repos/{}/releases?page={}&per_page={}",
                        base,
                        repo_required()?,
                        page,
                        per_page
                    )),
                    "tags" => Ok(format!(
                        "{}/repos/{}/tags?page={}&per_page={}",
                        base,
                        repo_required()?,
                        page,
                        per_page
                    )),
                    _ => Err(AppError::InvalidInput("不支持的 Git 只读资源".into())),
                }
            }
            "gitlab" => {
                let repo_required = || {
                    if repo.is_empty() {
                        Err(AppError::InvalidInput("repo 不能为空".into()))
                    } else {
                        Ok(Self::encode_path(repo))
                    }
                };
                match resource {
                    "repos" => Ok(format!(
                        "{}/projects?membership=true&simple=true&page={}&per_page={}",
                        base, page, per_page
                    )),
                    "repo_detail" => Ok(format!("{}/projects/{}", base, repo_required()?)),
                    "branches" => Ok(format!(
                        "{}/projects/{}/repository/branches?page={}&per_page={}",
                        base,
                        repo_required()?,
                        page,
                        per_page
                    )),
                    "file" => Ok(format!(
                        "{}/projects/{}/repository/files/{}?ref={}",
                        base,
                        repo_required()?,
                        Self::encode_path(&Self::payload_like_path(input.path.as_deref(), "path")?),
                        input.reference.as_deref().unwrap_or("HEAD")
                    )),
                    "commits" => Ok(format!(
                        "{}/projects/{}/repository/commits?page={}&per_page={}",
                        base,
                        repo_required()?,
                        page,
                        per_page
                    )),
                    "pull_requests" => Ok(format!(
                        "{}/projects/{}/merge_requests?state={}&page={}&per_page={}",
                        base,
                        repo_required()?,
                        state,
                        page,
                        per_page
                    )),
                    "issues" => Ok(format!(
                        "{}/projects/{}/issues?state={}&page={}&per_page={}",
                        base,
                        repo_required()?,
                        state,
                        page,
                        per_page
                    )),
                    "releases" => Ok(format!(
                        "{}/projects/{}/releases?page={}&per_page={}",
                        base,
                        repo_required()?,
                        page,
                        per_page
                    )),
                    "tags" => Ok(format!(
                        "{}/projects/{}/repository/tags?page={}&per_page={}",
                        base,
                        repo_required()?,
                        page,
                        per_page
                    )),
                    _ => Err(AppError::InvalidInput("不支持的 GitLab 只读资源".into())),
                }
            }
            _ => Err(AppError::InvalidInput("不支持的 Git Provider".into())),
        }
    }

    fn github_write_request(
        base: &str,
        input: &SecureCredentialGitWriteInput,
    ) -> Result<(String, String, Value), AppError> {
        let repo = input.repo.trim();
        match input.operation.as_str() {
            "create_issue" => Ok((
                "POST".into(),
                format!("{}/repos/{}/issues", base, repo),
                json!({
                    "title": Self::payload_string(&input.payload, "title")?,
                    "body": Self::payload_optional_string(&input.payload, "body")
                }),
            )),
            "create_branch" => Ok((
                "POST".into(),
                format!("{}/repos/{}/git/refs", base, repo),
                json!({
                    "ref": format!("refs/heads/{}", Self::payload_string(&input.payload, "branch")?),
                    "sha": Self::payload_string(&input.payload, "sha")?
                }),
            )),
            "commit_file" => Ok((
                "PUT".into(),
                format!(
                    "{}/repos/{}/contents/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "path")?
                ),
                json!({
                    "message": Self::payload_string(&input.payload, "message")?,
                    "content": general_purpose::STANDARD.encode(Self::payload_string(&input.payload, "content")?),
                    "branch": Self::payload_string(&input.payload, "branch")?,
                    "sha": Self::payload_optional_string(&input.payload, "sha")
                }),
            )),
            "create_pr" => Ok((
                "POST".into(),
                format!("{}/repos/{}/pulls", base, repo),
                json!({
                    "title": Self::payload_string(&input.payload, "title")?,
                    "head": Self::payload_string(&input.payload, "head")?,
                    "base": Self::payload_string(&input.payload, "base")?,
                    "body": Self::payload_optional_string(&input.payload, "body")
                }),
            )),
            "update_pr" => Ok((
                "PATCH".into(),
                format!(
                    "{}/repos/{}/pulls/{}",
                    base,
                    repo,
                    Self::payload_i64(&input.payload, "number")?
                ),
                json!({
                    "title": Self::payload_optional_string(&input.payload, "title"),
                    "body": Self::payload_optional_string(&input.payload, "body"),
                    "base": Self::payload_optional_string(&input.payload, "base"),
                    "state": Self::payload_optional_string(&input.payload, "state")
                }),
            )),
            "merge_pr" => Ok((
                "PUT".into(),
                format!(
                    "{}/repos/{}/pulls/{}/merge",
                    base,
                    repo,
                    Self::payload_i64(&input.payload, "number")?
                ),
                json!({
                    "commit_title": Self::payload_optional_string(&input.payload, "commitTitle"),
                    "sha": Self::payload_optional_string(&input.payload, "sha")
                }),
            )),
            "create_tag" => Ok((
                "POST".into(),
                format!("{}/repos/{}/git/refs", base, repo),
                json!({
                    "ref": format!("refs/tags/{}", Self::payload_string(&input.payload, "tag")?),
                    "sha": Self::payload_string(&input.payload, "sha")?
                }),
            )),
            "create_release" => Ok((
                "POST".into(),
                format!("{}/repos/{}/releases", base, repo),
                json!({
                    "tag_name": Self::payload_string(&input.payload, "tag")?,
                    "name": Self::payload_optional_string(&input.payload, "name"),
                    "body": Self::payload_optional_string(&input.payload, "body"),
                    "draft": input.payload.get("draft").and_then(Value::as_bool).unwrap_or(false),
                    "prerelease": input.payload.get("prerelease").and_then(Value::as_bool).unwrap_or(false)
                }),
            )),
            "trigger_workflow" => Ok((
                "POST".into(),
                format!(
                    "{}/repos/{}/actions/workflows/{}/dispatches",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "workflowId")?
                ),
                json!({
                    "ref": Self::payload_string(&input.payload, "ref")?,
                    "inputs": input.payload.get("inputs").cloned().unwrap_or_else(|| json!({}))
                }),
            )),
            "delete_branch" => Ok((
                "DELETE".into(),
                format!(
                    "{}/repos/{}/git/refs/heads/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "branch")?
                ),
                json!({}),
            )),
            "delete_tag" => Ok((
                "DELETE".into(),
                format!(
                    "{}/repos/{}/git/refs/tags/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "tag")?
                ),
                json!({}),
            )),
            "delete_release" => Ok((
                "DELETE".into(),
                format!(
                    "{}/repos/{}/releases/{}",
                    base,
                    repo,
                    Self::payload_i64(&input.payload, "releaseId")?
                ),
                json!({}),
            )),
            "update_ref" => Ok((
                "PATCH".into(),
                format!(
                    "{}/repos/{}/git/refs/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "ref")?
                ),
                json!({
                    "sha": Self::payload_string(&input.payload, "sha")?,
                    "force": input.payload.get("force").and_then(Value::as_bool).unwrap_or(false)
                }),
            )),
            "update_repo_settings" => Ok((
                "PATCH".into(),
                format!("{}/repos/{}", base, repo),
                input
                    .payload
                    .get("settings")
                    .cloned()
                    .unwrap_or_else(|| input.payload.clone()),
            )),
            _ => Err(AppError::InvalidInput("不支持的 GitHub 写操作".into())),
        }
    }

    fn gitlab_write_request(
        base: &str,
        input: &SecureCredentialGitWriteInput,
    ) -> Result<(String, String, Value), AppError> {
        let repo = Self::encode_path(input.repo.trim());
        match input.operation.as_str() {
            "create_issue" => Ok((
                "POST".into(),
                format!("{}/projects/{}/issues", base, repo),
                json!({
                    "title": Self::payload_string(&input.payload, "title")?,
                    "description": Self::payload_optional_string(&input.payload, "body")
                }),
            )),
            "create_branch" => Ok((
                "POST".into(),
                format!("{}/projects/{}/repository/branches", base, repo),
                json!({
                    "branch": Self::payload_string(&input.payload, "branch")?,
                    "ref": Self::payload_string(&input.payload, "ref")?
                }),
            )),
            "commit_file" => Ok((
                "POST".into(),
                format!("{}/projects/{}/repository/commits", base, repo),
                json!({
                    "branch": Self::payload_string(&input.payload, "branch")?,
                    "commit_message": Self::payload_string(&input.payload, "message")?,
                    "actions": [{
                        "action": input.payload.get("action").and_then(Value::as_str).unwrap_or("update"),
                        "file_path": Self::payload_string(&input.payload, "path")?,
                        "content": Self::payload_string(&input.payload, "content")?
                    }]
                }),
            )),
            "create_pr" => Ok((
                "POST".into(),
                format!("{}/projects/{}/merge_requests", base, repo),
                json!({
                    "title": Self::payload_string(&input.payload, "title")?,
                    "source_branch": Self::payload_string(&input.payload, "head")?,
                    "target_branch": Self::payload_string(&input.payload, "base")?,
                    "description": Self::payload_optional_string(&input.payload, "body")
                }),
            )),
            "update_pr" => Ok((
                "PUT".into(),
                format!(
                    "{}/projects/{}/merge_requests/{}",
                    base,
                    repo,
                    Self::payload_i64(&input.payload, "number")?
                ),
                json!({
                    "title": Self::payload_optional_string(&input.payload, "title"),
                    "description": Self::payload_optional_string(&input.payload, "body"),
                    "target_branch": Self::payload_optional_string(&input.payload, "base"),
                    "state_event": Self::payload_optional_string(&input.payload, "stateEvent")
                }),
            )),
            "merge_pr" => Ok((
                "PUT".into(),
                format!(
                    "{}/projects/{}/merge_requests/{}/merge",
                    base,
                    repo,
                    Self::payload_i64(&input.payload, "number")?
                ),
                json!({
                    "sha": Self::payload_optional_string(&input.payload, "sha")
                }),
            )),
            "create_tag" => Ok((
                "POST".into(),
                format!("{}/projects/{}/repository/tags", base, repo),
                json!({
                    "tag_name": Self::payload_string(&input.payload, "tag")?,
                    "ref": Self::payload_string(&input.payload, "ref")?
                }),
            )),
            "create_release" => Ok((
                "POST".into(),
                format!("{}/projects/{}/releases", base, repo),
                json!({
                    "tag_name": Self::payload_string(&input.payload, "tag")?,
                    "name": Self::payload_optional_string(&input.payload, "name").unwrap_or_else(|| Self::payload_string(&input.payload, "tag").unwrap_or_default()),
                    "description": Self::payload_optional_string(&input.payload, "body")
                }),
            )),
            "trigger_workflow" => Ok((
                "POST".into(),
                format!("{}/projects/{}/pipeline", base, repo),
                json!({
                    "ref": Self::payload_string(&input.payload, "ref")?,
                    "variables": input.payload.get("variables").cloned().unwrap_or_else(|| json!([]))
                }),
            )),
            "delete_branch" => Ok((
                "DELETE".into(),
                format!(
                    "{}/projects/{}/repository/branches/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "branch")?
                ),
                json!({}),
            )),
            "delete_tag" => Ok((
                "DELETE".into(),
                format!(
                    "{}/projects/{}/repository/tags/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "tag")?
                ),
                json!({}),
            )),
            "delete_release" => Ok((
                "DELETE".into(),
                format!(
                    "{}/projects/{}/releases/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "tag")?
                ),
                json!({}),
            )),
            "update_ref" => Ok((
                "PUT".into(),
                format!(
                    "{}/projects/{}/repository/branches/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "branch")?
                ),
                json!({
                    "branch": Self::payload_string(&input.payload, "branch")?,
                    "ref": Self::payload_string(&input.payload, "ref")?
                }),
            )),
            "update_repo_settings" => Ok((
                "PUT".into(),
                format!("{}/projects/{}", base, repo),
                input
                    .payload
                    .get("settings")
                    .cloned()
                    .unwrap_or_else(|| input.payload.clone()),
            )),
            _ => Err(AppError::InvalidInput("不支持的 GitLab 写操作".into())),
        }
    }

    fn gitcode_write_request(
        base: &str,
        input: &SecureCredentialGitWriteInput,
    ) -> Result<(String, String, Value), AppError> {
        let repo = input.repo.trim();
        match input.operation.as_str() {
            "create_issue" => Ok((
                "POST".into(),
                format!("{}/repos/{}/issues", base, repo),
                json!({
                    "title": Self::payload_string(&input.payload, "title")?,
                    "body": Self::payload_optional_string(&input.payload, "body")
                }),
            )),
            "create_branch" => Ok((
                "POST".into(),
                format!("{}/repos/{}/branches", base, repo),
                json!({
                    "refs": Self::payload_string(&input.payload, "ref")?,
                    "branch_name": Self::payload_string(&input.payload, "branch")?
                }),
            )),
            "commit_file" => Ok((
                "POST".into(),
                format!(
                    "{}/repos/{}/contents/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "path")?
                ),
                json!({
                    "message": Self::payload_string(&input.payload, "message")?,
                    "content": Self::payload_string(&input.payload, "content")?,
                    "branch": Self::payload_string(&input.payload, "branch")?
                }),
            )),
            "create_pr" => Ok((
                "POST".into(),
                format!("{}/repos/{}/pulls", base, repo),
                json!({
                    "title": Self::payload_string(&input.payload, "title")?,
                    "head": Self::payload_string(&input.payload, "head")?,
                    "base": Self::payload_string(&input.payload, "base")?,
                    "body": Self::payload_optional_string(&input.payload, "body")
                }),
            )),
            "update_pr" => Ok((
                "PATCH".into(),
                format!(
                    "{}/repos/{}/pulls/{}",
                    base,
                    repo,
                    Self::payload_i64(&input.payload, "number")?
                ),
                json!({
                    "title": Self::payload_optional_string(&input.payload, "title"),
                    "body": Self::payload_optional_string(&input.payload, "body"),
                    "base": Self::payload_optional_string(&input.payload, "base"),
                    "state": Self::payload_optional_string(&input.payload, "state")
                }),
            )),
            "merge_pr" => Ok((
                "PUT".into(),
                format!(
                    "{}/repos/{}/pulls/{}/merge",
                    base,
                    repo,
                    Self::payload_i64(&input.payload, "number")?
                ),
                json!({}),
            )),
            "create_tag" => Ok((
                "POST".into(),
                format!("{}/repos/{}/git/refs", base, repo),
                json!({
                    "ref": format!("refs/tags/{}", Self::payload_string(&input.payload, "tag")?),
                    "sha": Self::payload_string(&input.payload, "sha")?
                }),
            )),
            "create_release" => Ok((
                "POST".into(),
                format!("{}/repos/{}/releases", base, repo),
                json!({
                    "tag_name": Self::payload_string(&input.payload, "tag")?,
                    "name": Self::payload_optional_string(&input.payload, "name"),
                    "body": Self::payload_optional_string(&input.payload, "body")
                }),
            )),
            "trigger_workflow" => Err(AppError::InvalidInput(
                "GitCode 首版暂未支持 workflow/pipeline 触发".into(),
            )),
            "delete_branch" => Ok((
                "DELETE".into(),
                format!(
                    "{}/repos/{}/branches/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "branch")?
                ),
                json!({}),
            )),
            "delete_tag" => Ok((
                "DELETE".into(),
                format!(
                    "{}/repos/{}/git/refs/tags/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "tag")?
                ),
                json!({}),
            )),
            "delete_release" => Ok((
                "DELETE".into(),
                format!(
                    "{}/repos/{}/releases/{}",
                    base,
                    repo,
                    Self::payload_i64(&input.payload, "releaseId")?
                ),
                json!({}),
            )),
            "update_ref" => Ok((
                "PATCH".into(),
                format!(
                    "{}/repos/{}/git/refs/{}",
                    base,
                    repo,
                    Self::payload_string(&input.payload, "ref")?
                ),
                json!({
                    "sha": Self::payload_string(&input.payload, "sha")?,
                    "force": input.payload.get("force").and_then(Value::as_bool).unwrap_or(false)
                }),
            )),
            "update_repo_settings" => Ok((
                "PATCH".into(),
                format!("{}/repos/{}", base, repo),
                input
                    .payload
                    .get("settings")
                    .cloned()
                    .unwrap_or_else(|| input.payload.clone()),
            )),
            _ => Err(AppError::InvalidInput("不支持的 GitCode 写操作".into())),
        }
    }

    fn payload_string(payload: &Value, key: &str) -> Result<String, AppError> {
        payload
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| AppError::InvalidInput(format!("payload.{} 不能为空", key)))
    }

    fn payload_optional_string(payload: &Value, key: &str) -> Option<String> {
        payload
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    fn payload_i64(payload: &Value, key: &str) -> Result<i64, AppError> {
        payload
            .get(key)
            .and_then(Value::as_i64)
            .ok_or_else(|| AppError::InvalidInput(format!("payload.{} 必须是数字", key)))
    }

    fn payload_like_path(value: Option<&str>, key: &str) -> Result<String, AppError> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| AppError::InvalidInput(format!("{} 不能为空", key)))
    }

    fn encode_path(value: &str) -> String {
        value.replace('/', "%2F")
    }

    fn build_http_api_url(
        credential: &SecureCredential,
        path: &str,
        query_json: Option<&Value>,
    ) -> Result<String, AppError> {
        if !path.starts_with('/') || path.contains("://") {
            return Err(AppError::InvalidInput(
                "HTTP API path 必须是以 / 开头的相对路径".into(),
            ));
        }
        let base_url = Self::api_base_url(credential)?;
        let mut url = format!("{}{}", base_url.trim_end_matches('/'), path);
        if let Some(query) = query_json.and_then(Value::as_object) {
            let pairs = query
                .iter()
                .filter_map(|(key, value)| value.as_str().map(|text| (key, text)))
                .map(|(key, value)| format!("{}={}", key, value))
                .collect::<Vec<_>>();
            if !pairs.is_empty() {
                url.push('?');
                url.push_str(&pairs.join("&"));
            }
        }
        Ok(url)
    }

    fn http_error(error: reqwest::Error) -> AppError {
        if error.is_timeout() {
            AppError::Custom("Provider 请求超时".into())
        } else if error.is_connect() {
            AppError::Custom(format!("Provider 连接失败: {}", error))
        } else {
            AppError::Custom(format!("Provider 请求失败: {}", error))
        }
    }

    fn encrypt_secret(db: &Database, secret: &str) -> Result<(String, String), AppError> {
        let key = Self::secret_key(db)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| AppError::Custom("安全凭证密钥初始化失败".into()))?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, secret.as_bytes())
            .map_err(|_| AppError::Custom("安全凭证加密失败".into()))?;
        Ok((
            general_purpose::STANDARD.encode(nonce_bytes),
            general_purpose::STANDARD.encode(ciphertext),
        ))
    }

    fn decrypt_secret(db: &Database, nonce: &str, ciphertext: &str) -> Result<String, AppError> {
        let key = Self::secret_key(db)?;
        let nonce_bytes = general_purpose::STANDARD
            .decode(nonce)
            .map_err(|_| AppError::Custom("安全凭证 nonce 解码失败".into()))?;
        let ciphertext_bytes = general_purpose::STANDARD
            .decode(ciphertext)
            .map_err(|_| AppError::Custom("安全凭证密文解码失败".into()))?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| AppError::Custom("安全凭证密钥初始化失败".into()))?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext_bytes.as_ref())
            .map_err(|_| AppError::Custom("安全凭证解密失败".into()))?;
        String::from_utf8(plaintext).map_err(|_| AppError::Custom("安全凭证不是有效 UTF-8".into()))
    }

    fn secret_key(db: &Database) -> Result<[u8; 32], AppError> {
        let seed = match db.get_config(SECURE_CREDENTIAL_SECRET_SEED_KEY)? {
            Some(value) => value,
            None => {
                let mut bytes = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut bytes);
                let value = general_purpose::STANDARD.encode(bytes);
                db.set_config(SECURE_CREDENTIAL_SECRET_SEED_KEY, &value)?;
                value
            }
        };
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        let digest = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest[..32]);
        Ok(key)
    }
}

fn normalize_credential_reference(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_' && *ch != '-')
        .collect()
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

fn sanitize_remote_for_message(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "-".into();
    }
    if let Some((scheme, rest)) = trimmed.split_once("://") {
        let sanitized = rest.split_once('@').map(|(_, right)| right).unwrap_or(rest);
        return format!("{}://{}", scheme, sanitized);
    }
    trimmed
        .split_once('@')
        .map(|(_, right)| right.to_string())
        .unwrap_or_else(|| trimmed.to_string())
}
