use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    SshConfigImportResult, SshServer, SshServerConnectionTestInput, SshServerTestResult,
    UpsertSshServerInput,
};

pub struct SshServerService;
const PASSWORD_SECRET_SEED_KEY: &str = "ssh_server_password_secret_seed";

impl SshServerService {
    pub fn list(db: &Database) -> Result<Vec<SshServer>, AppError> {
        db.list_ssh_servers()
    }

    pub fn upsert(db: &Database, input: UpsertSshServerInput) -> Result<SshServer, AppError> {
        Self::validate_server(&input)?;
        let encrypted_password = match input
            .password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(password) => {
                let (nonce, ciphertext) = Self::encrypt_password(db, password)?;
                Some((nonce, ciphertext))
            }
            None => None,
        };
        let encrypted_ref = encrypted_password
            .as_ref()
            .map(|(nonce, ciphertext)| (nonce.as_str(), ciphertext.as_str()));
        db.upsert_ssh_server(&input, encrypted_ref, input.clear_password.unwrap_or(false))
    }

    pub fn delete(db: &Database, alias: &str) -> Result<(), AppError> {
        if alias.trim().is_empty() {
            return Err(AppError::InvalidInput("服务器别名不能为空".into()));
        }
        if !db.delete_ssh_server(alias)? {
            return Err(AppError::NotFound(format!("SSH 服务器 '{}' 不存在", alias)));
        }
        Ok(())
    }

    pub async fn test(db: &Database, alias: &str) -> Result<SshServerTestResult, AppError> {
        let server = db
            .get_ssh_server(alias)?
            .ok_or_else(|| AppError::NotFound(format!("SSH 服务器 '{}' 不存在", alias)))?;
        if !server.enabled {
            return Err(AppError::InvalidInput("服务器已禁用".into()));
        }
        if server.source == "jumpserver" || server.status == "web" {
            return Ok(SshServerTestResult {
                ok: false,
                alias: server.alias,
                endpoint: server.host,
                latency_ms: 0,
                message: "Web / JumpServer 会话不执行本地 TCP 测试".into(),
            });
        }

        let result = Self::test_tcp_endpoint(&server.alias, &server.host, server.port).await?;
        if result.ok {
            db.update_ssh_server_status(&server.alias, "online", true)?;
        } else if result.message == "TCP 连接超时" {
            db.update_ssh_server_status(&server.alias, "degraded", false)?;
        } else {
            db.update_ssh_server_status(&server.alias, "offline", false)?;
        }
        Ok(result)
    }

    pub async fn test_connection(
        input: SshServerConnectionTestInput,
    ) -> Result<SshServerTestResult, AppError> {
        let alias = input
            .alias
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("未保存服务器");
        Self::test_tcp_endpoint(alias, &input.host, input.port).await
    }

    pub fn import_ssh_config(
        db: &Database,
        path: Option<String>,
    ) -> Result<SshConfigImportResult, AppError> {
        let (path, attempted_paths, is_default_path) =
            match path.filter(|item| !item.trim().is_empty()) {
                Some(path) => {
                    let path = PathBuf::from(path);
                    (path.clone(), vec![path], false)
                }
                None => {
                    let (path, attempted_paths) = Self::default_ssh_config_path();
                    (path, attempted_paths, true)
                }
            };
        let content = std::fs::read_to_string(&path).map_err(|error| {
            let attempted = attempted_paths
                .iter()
                .map(|item| item.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            if error.kind() == std::io::ErrorKind::NotFound && is_default_path {
                return AppError::InvalidInput(format!(
                    "未发现本机 SSH Config。当前系统默认查找路径为 {}，你可以先手工新增服务器，或创建 ~/.ssh/config 后再导入。",
                    attempted
                ));
            }
            if error.kind() == std::io::ErrorKind::NotFound {
                return AppError::InvalidInput(format!(
                    "选择的 SSH Config 文件不存在：{}",
                    path.display()
                ));
            }
            AppError::Custom(format!(
                "读取 SSH Config 失败（系统: {}，尝试路径: {}，选中: {}）: {}",
                std::env::consts::OS,
                attempted,
                path.display(),
                error
            ))
        })?;
        let (inputs, skipped) = Self::parse_ssh_config(&content);
        let mut imported = 0;
        for input in inputs {
            db.upsert_ssh_server(&input, None, false)?;
            imported += 1;
        }
        Ok(SshConfigImportResult {
            imported,
            skipped,
            servers: db.list_ssh_servers()?,
        })
    }

    fn validate_server(input: &UpsertSshServerInput) -> Result<(), AppError> {
        if input.alias.trim().is_empty() {
            return Err(AppError::InvalidInput("服务器别名不能为空".into()));
        }
        if input.host.trim().is_empty() {
            return Err(AppError::InvalidInput("主机地址不能为空".into()));
        }
        if !(1..=65535).contains(&input.port) {
            return Err(AppError::InvalidInput("端口必须在 1-65535 之间".into()));
        }
        if !["manual", "ssh_config", "jumpserver"].contains(&input.source.as_str()) {
            return Err(AppError::InvalidInput("来源无效".into()));
        }
        if ![
            "key",
            "password_ref",
            "direct_password",
            "session_reference",
        ]
        .contains(&input.auth_type.as_str())
        {
            return Err(AppError::InvalidInput("认证方式无效".into()));
        }
        if input.auth_type == "direct_password"
            && input.password.as_deref().unwrap_or("").trim().is_empty()
            && input.auth_ref.trim().is_empty()
        {
            return Err(AppError::InvalidInput("请填写直接密码".into()));
        }
        if !["readonly", "L1", "L2", "L3", "blocked"].contains(&input.ai_policy.as_str()) {
            return Err(AppError::InvalidInput("AI 权限无效".into()));
        }
        Ok(())
    }

    async fn test_tcp_endpoint(
        alias: &str,
        host: &str,
        port: i64,
    ) -> Result<SshServerTestResult, AppError> {
        let host = host.trim();
        if host.is_empty() {
            return Err(AppError::InvalidInput("主机地址不能为空".into()));
        }
        if !(1..=65535).contains(&port) {
            return Err(AppError::InvalidInput("端口必须在 1-65535 之间".into()));
        }

        let endpoint = format!("{}:{}", host, port);
        let started = Instant::now();
        let result = timeout(Duration::from_secs(3), TcpStream::connect(&endpoint)).await;
        let latency_ms = started.elapsed().as_millis() as i64;

        match result {
            Ok(Ok(_stream)) => Ok(SshServerTestResult {
                ok: true,
                alias: alias.to_string(),
                endpoint,
                latency_ms,
                message: "TCP 连接成功".into(),
            }),
            Ok(Err(error)) => Ok(SshServerTestResult {
                ok: false,
                alias: alias.to_string(),
                endpoint,
                latency_ms,
                message: format!("TCP 连接失败: {}", error),
            }),
            Err(_) => Ok(SshServerTestResult {
                ok: false,
                alias: alias.to_string(),
                endpoint,
                latency_ms,
                message: "TCP 连接超时".into(),
            }),
        }
    }

    fn default_ssh_config_path() -> (PathBuf, Vec<PathBuf>) {
        let candidates = Self::default_ssh_config_candidates();
        let selected = candidates
            .iter()
            .find(|path| path.is_file())
            .cloned()
            .unwrap_or_else(|| {
                candidates
                    .first()
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from(".").join(".ssh").join("config"))
            });
        (selected, candidates)
    }

    fn default_ssh_config_candidates() -> Vec<PathBuf> {
        let mut candidates = Vec::new();

        if cfg!(windows) {
            if let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
                candidates.push(profile.join(".ssh").join("config"));
            }
            if let (Some(drive), Some(path)) =
                (std::env::var_os("HOMEDRIVE"), std::env::var_os("HOMEPATH"))
            {
                candidates.push(
                    PathBuf::from(format!(
                        "{}{}",
                        drive.to_string_lossy(),
                        path.to_string_lossy()
                    ))
                    .join(".ssh")
                    .join("config"),
                );
            }
        }

        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            candidates.push(home.join(".ssh").join("config"));
        }

        candidates.dedup();
        candidates
    }

    fn parse_ssh_config(content: &str) -> (Vec<UpsertSshServerInput>, i64) {
        let mut blocks: Vec<(Vec<String>, HashMap<String, String>)> = Vec::new();
        let mut hosts: Vec<String> = Vec::new();
        let mut values: HashMap<String, String> = HashMap::new();

        for raw_line in content.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(2, char::is_whitespace);
            let key = parts.next().unwrap_or("").to_ascii_lowercase();
            let value = parts.next().unwrap_or("").trim().to_string();
            if key == "host" {
                if !hosts.is_empty() {
                    blocks.push((hosts, values));
                }
                hosts = value.split_whitespace().map(ToString::to_string).collect();
                values = HashMap::new();
            } else if !hosts.is_empty() {
                values.insert(key, value);
            }
        }
        if !hosts.is_empty() {
            blocks.push((hosts, values));
        }

        let mut skipped = 0;
        let mut inputs = Vec::new();
        for (hosts, values) in blocks {
            for alias in hosts {
                if alias.contains('*') || alias.contains('?') || alias.starts_with('!') {
                    skipped += 1;
                    continue;
                }
                let host = values
                    .get("hostname")
                    .cloned()
                    .unwrap_or_else(|| alias.clone());
                let port = values
                    .get("port")
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(22);
                let username = values.get("user").cloned().unwrap_or_default();
                let identity_file = values.get("identityfile").cloned().unwrap_or_default();
                let proxy_jump = values.get("proxyjump").cloned().unwrap_or_default();
                let auth_ref = if identity_file.is_empty() {
                    "ssh_config".into()
                } else {
                    format!("key:{}", identity_file)
                };
                inputs.push(UpsertSshServerInput {
                    group_name: Self::infer_group_name(&alias),
                    alias,
                    host,
                    port,
                    username,
                    source: "ssh_config".into(),
                    auth_type: "key".into(),
                    auth_ref,
                    identity_file,
                    password: None,
                    clear_password: Some(false),
                    proxy_jump,
                    ai_policy: "L1".into(),
                    status: Some("unknown".into()),
                    enabled: true,
                });
            }
        }
        (inputs, skipped)
    }

    fn infer_group_name(alias: &str) -> String {
        let lower = alias.to_ascii_lowercase();
        if lower.starts_with("prod") {
            "生产".into()
        } else if lower.starts_with("stage") || lower.starts_with("stg") {
            "预发".into()
        } else if lower.contains("jump") || lower.contains("bastion") {
            "堡垒机".into()
        } else {
            "默认".into()
        }
    }

    fn encrypt_password(db: &Database, password: &str) -> Result<(String, String), AppError> {
        let key = Self::password_key(db)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| AppError::Custom("密码密钥初始化失败".into()))?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, password.as_bytes())
            .map_err(|_| AppError::Custom("服务器密码加密失败".into()))?;
        Ok((
            general_purpose::STANDARD.encode(nonce_bytes),
            general_purpose::STANDARD.encode(ciphertext),
        ))
    }

    fn password_key(db: &Database) -> Result<[u8; 32], AppError> {
        let seed = match db.get_config(PASSWORD_SECRET_SEED_KEY)? {
            Some(value) => value,
            None => {
                let mut bytes = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut bytes);
                let value = general_purpose::STANDARD.encode(bytes);
                db.set_config(PASSWORD_SECRET_SEED_KEY, &value)?;
                value
            }
        };
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        hasher.update(b"tauri-ssh-server-password");
        let digest = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest);
        Ok(key)
    }
}
