use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use ssh2::Session;
use tauri::Emitter;

use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    SshServer, TerminalCommandInput, TerminalCommandResult, TerminalSessionEvent,
    TerminalSessionStartInput, TerminalSessionStartResult,
};

const SSH_PASSWORD_SECRET_SEED_KEY: &str = "ssh_server_password_secret_seed";
const CREDENTIAL_SECRET_SEED_KEY: &str = "credential_vault_secret_seed";

pub struct TerminalService;

enum AuthMaterial {
    Password(String),
    PrivateKey(PathBuf),
}

pub enum TerminalPtyCommand {
    Data(String),
    Resize(u32, u32),
    Close,
}

#[derive(Clone)]
pub struct TerminalSessionHandle {
    tx: Sender<TerminalPtyCommand>,
}

impl TerminalSessionHandle {
    fn new(tx: Sender<TerminalPtyCommand>) -> Self {
        Self { tx }
    }

    pub fn send(&self, command: TerminalPtyCommand) -> Result<(), AppError> {
        self.tx
            .send(command)
            .map_err(|_| AppError::Custom("终端会话已关闭".into()))
    }
}

#[derive(Clone, Default)]
pub struct TerminalSessionRegistry {
    inner: Arc<Mutex<HashMap<String, TerminalSessionHandle>>>,
}

impl TerminalSessionRegistry {
    pub fn insert(
        &self,
        session_id: String,
        handle: TerminalSessionHandle,
    ) -> Result<(), AppError> {
        let mut sessions = self
            .inner
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        sessions.insert(session_id, handle);
        Ok(())
    }

    pub fn write(&self, session_id: &str, data: String) -> Result<(), AppError> {
        let handle = self.get(session_id)?;
        handle.send(TerminalPtyCommand::Data(data))
    }

    pub fn resize(&self, session_id: &str, cols: u32, rows: u32) -> Result<(), AppError> {
        let handle = self.get(session_id)?;
        handle.send(TerminalPtyCommand::Resize(cols, rows))
    }

    pub fn close(&self, session_id: &str) -> Result<(), AppError> {
        let handle = {
            let mut sessions = self
                .inner
                .lock()
                .map_err(|e| AppError::Custom(e.to_string()))?;
            sessions.remove(session_id)
        };
        if let Some(handle) = handle {
            let _ = handle.send(TerminalPtyCommand::Close);
        }
        Ok(())
    }

    pub fn remove(&self, session_id: &str) {
        if let Ok(mut sessions) = self.inner.lock() {
            sessions.remove(session_id);
        }
    }

    fn get(&self, session_id: &str) -> Result<TerminalSessionHandle, AppError> {
        let sessions = self
            .inner
            .lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("终端会话 '{}' 不存在", session_id)))
    }
}

impl TerminalService {
    pub(crate) fn connect_saved_server(
        db: &Database,
        server_alias: &str,
        timeout_secs: u64,
    ) -> Result<(SshServer, Session), AppError> {
        if server_alias.trim().is_empty() {
            return Err(AppError::InvalidInput("请选择服务器".into()));
        }
        let row = db
            .get_ssh_server_secret_row(server_alias)?
            .ok_or_else(|| AppError::NotFound(format!("服务器 '{}' 不存在", server_alias)))?;
        if !row.server.enabled {
            return Err(AppError::InvalidInput("服务器已禁用".into()));
        }
        if row.server.source == "jumpserver" || row.server.auth_type == "session_reference" {
            return Err(AppError::InvalidInput(
                "JumpServer / 会话引用服务器不能通过本地 SSH 直连".into(),
            ));
        }

        let auth =
            Self::resolve_auth(db, &row.server, row.password_nonce, row.password_ciphertext)?;
        let session = Self::connect_authenticated_session(&row.server, auth, timeout_secs)?;
        Ok((row.server, session))
    }

    pub async fn execute(
        db: &Database,
        input: TerminalCommandInput,
    ) -> Result<TerminalCommandResult, AppError> {
        Self::validate_command_input(&input)?;
        if let Some(message) = Self::blocked_command_message(&input.command) {
            return Ok(TerminalCommandResult {
                server_alias: input.server_alias,
                command: input.command,
                exit_status: -1,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 0,
                blocked: true,
                message,
            });
        }

        let timeout_secs = input.timeout_secs.unwrap_or(30).clamp(3, 120);
        let started = Instant::now();
        let (_, session) = Self::connect_saved_server(&db, &input.server_alias, timeout_secs)?;
        let result = Self::execute_blocking_with_session(session, &input.command).await?;
        Ok(TerminalCommandResult {
            server_alias: input.server_alias,
            command: input.command,
            duration_ms: started.elapsed().as_millis() as i64,
            blocked: false,
            message: if result.0 == 0 {
                "SSH 命令执行完成".into()
            } else {
                "SSH 命令执行完成，远端返回非零退出码".into()
            },
            exit_status: result.0,
            stdout: result.1,
            stderr: result.2,
        })
    }

    pub fn start_session(
        db: &Database,
        registry: &TerminalSessionRegistry,
        app_handle: tauri::AppHandle,
        input: TerminalSessionStartInput,
    ) -> Result<TerminalSessionStartResult, AppError> {
        let registry_for_cleanup = registry.clone();
        let (session_id, handle) = Self::start_raw_session(
            db,
            input,
            move |event| {
                let _ = app_handle.emit("terminal-session-event", event);
            },
            Some(Box::new(move |session_id| {
                registry_for_cleanup.remove(&session_id);
            })),
        )?;
        registry.insert(session_id.clone(), handle)?;
        Ok(TerminalSessionStartResult { session_id })
    }

    pub fn start_raw_session<F>(
        db: &Database,
        input: TerminalSessionStartInput,
        on_event: F,
        on_finish: Option<Box<dyn Fn(String) + Send + 'static>>,
    ) -> Result<(String, TerminalSessionHandle), AppError>
    where
        F: Fn(TerminalSessionEvent) + Send + 'static,
    {
        if input.server_alias.trim().is_empty() {
            return Err(AppError::InvalidInput("请选择服务器".into()));
        }
        let (server, session) = Self::connect_saved_server(db, &input.server_alias, 30)?;
        let cols = input.cols.unwrap_or(100).clamp(40, 240);
        let rows = input.rows.unwrap_or(30).clamp(10, 80);
        let session_id = Self::new_session_id(&input.server_alias);
        let (tx, rx) = mpsc::channel();
        let handle = TerminalSessionHandle::new(tx);
        Self::spawn_pty_thread(
            session_id.clone(),
            server,
            session,
            cols,
            rows,
            rx,
            on_event,
            on_finish,
        );
        Ok((session_id, handle))
    }

    fn validate_command_input(input: &TerminalCommandInput) -> Result<(), AppError> {
        if input.server_alias.trim().is_empty() {
            return Err(AppError::InvalidInput("请选择服务器".into()));
        }
        if input.command.trim().is_empty() {
            return Err(AppError::InvalidInput("命令不能为空".into()));
        }
        Ok(())
    }

    fn blocked_command_message(command: &str) -> Option<String> {
        let normalized = command.trim().to_lowercase();
        let blocked = [
            "rm -rf /",
            "mkfs.",
            ":(){:|:&};:",
            "shutdown",
            "poweroff",
            "reboot",
        ];
        if blocked.iter().any(|pattern| normalized.contains(pattern))
            || normalized.contains("dd if=") && normalized.contains(" of=")
        {
            Some("命中高风险命令黑名单，已在本地阻止执行".into())
        } else {
            None
        }
    }

    fn resolve_auth(
        db: &Database,
        server: &SshServer,
        password_nonce: Option<String>,
        password_ciphertext: Option<String>,
    ) -> Result<AuthMaterial, AppError> {
        match server.auth_type.as_str() {
            "direct_password" => match (password_nonce, password_ciphertext) {
                (Some(nonce), Some(ciphertext)) => Ok(AuthMaterial::Password(
                    Self::decrypt_secret(db, SSH_PASSWORD_SECRET_SEED_KEY, &nonce, &ciphertext)?,
                )),
                _ => Err(AppError::InvalidInput("服务器未保存直接密码".into())),
            },
            "password_ref" => {
                let key = server
                    .auth_ref
                    .strip_prefix("vault:")
                    .unwrap_or(server.auth_ref.as_str())
                    .trim();
                if key.is_empty() {
                    return Err(AppError::InvalidInput("密码引用为空".into()));
                }
                let row = db
                    .get_credential_secret_row(key)?
                    .ok_or_else(|| AppError::NotFound(format!("凭据 '{}' 不存在", key)))?;
                match (row.secret_nonce, row.secret_ciphertext) {
                    (Some(nonce), Some(ciphertext)) => Ok(AuthMaterial::Password(
                        Self::decrypt_secret(db, CREDENTIAL_SECRET_SEED_KEY, &nonce, &ciphertext)?,
                    )),
                    _ => Err(AppError::InvalidInput("引用凭据未保存密文".into())),
                }
            }
            "key" => {
                let path = if server.identity_file.trim().is_empty() {
                    server
                        .auth_ref
                        .strip_prefix("key:")
                        .unwrap_or(server.auth_ref.as_str())
                        .trim()
                        .to_string()
                } else {
                    server.identity_file.trim().to_string()
                };
                if path.is_empty() {
                    return Err(AppError::InvalidInput("私钥文件路径为空".into()));
                }
                Ok(AuthMaterial::PrivateKey(Self::expand_home_path(&path)))
            }
            _ => Err(AppError::InvalidInput(
                "当前认证方式不支持 SSH 直连执行".into(),
            )),
        }
    }

    fn spawn_pty_thread<F>(
        session_id: String,
        server: SshServer,
        session: Session,
        cols: u32,
        rows: u32,
        rx: Receiver<TerminalPtyCommand>,
        on_event: F,
        on_finish: Option<Box<dyn Fn(String) + Send + 'static>>,
    ) where
        F: Fn(TerminalSessionEvent) + Send + 'static,
    {
        thread::spawn(move || {
            let emit = |kind: &str, data: Option<String>, message: Option<String>| {
                on_event(TerminalSessionEvent {
                    session_id: session_id.clone(),
                    kind: kind.to_string(),
                    data,
                    message,
                });
            };

            match Self::run_pty_loop(session, &server, cols, rows, rx, &emit) {
                Ok(()) => emit("exit", None, Some("SSH 终端会话已结束".into())),
                Err(error) => emit("error", None, Some(error.to_string())),
            }
            if let Some(on_finish) = on_finish {
                on_finish(session_id);
            }
        });
    }

    fn run_pty_loop<F>(
        session: Session,
        server: &SshServer,
        cols: u32,
        rows: u32,
        rx: Receiver<TerminalPtyCommand>,
        emit: &F,
    ) -> Result<(), AppError>
    where
        F: Fn(&str, Option<String>, Option<String>),
    {
        let mut channel = session
            .channel_session()
            .map_err(|e| AppError::Custom(format!("SSH Channel 创建失败: {}", e)))?;
        channel
            .request_pty("xterm-256color", None, Some((cols, rows, 0, 0)))
            .map_err(|e| AppError::Custom(format!("SSH PTY 创建失败: {}", e)))?;
        channel
            .shell()
            .map_err(|e| AppError::Custom(format!("SSH Shell 启动失败: {}", e)))?;
        session.set_blocking(false);
        emit(
            "status",
            None,
            Some(format!(
                "已连接 {}@{}:{}",
                server.username, server.host, server.port
            )),
        );

        let mut buffer = [0u8; 8192];
        loop {
            while let Ok(command) = rx.try_recv() {
                match command {
                    TerminalPtyCommand::Data(data) => {
                        channel
                            .write_all(data.as_bytes())
                            .map_err(|e| AppError::Custom(format!("SSH 输入写入失败: {}", e)))?;
                        let _ = channel.flush();
                    }
                    TerminalPtyCommand::Resize(next_cols, next_rows) => {
                        channel
                            .request_pty_size(
                                next_cols.clamp(40, 240),
                                next_rows.clamp(10, 80),
                                None,
                                None,
                            )
                            .map_err(|e| AppError::Custom(format!("SSH PTY 调整失败: {}", e)))?;
                    }
                    TerminalPtyCommand::Close => {
                        let _ = channel.close();
                        return Ok(());
                    }
                }
            }

            match channel.read(&mut buffer) {
                Ok(size) if size > 0 => {
                    let data = String::from_utf8_lossy(&buffer[..size]).to_string();
                    emit("data", Some(data), None);
                }
                Ok(_) => {
                    if channel.eof() {
                        return Ok(());
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => {
                    return Err(AppError::Custom(format!("SSH 输出读取失败: {}", error)));
                }
            }

            if channel.eof() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    async fn execute_blocking_with_session(
        session: Session,
        command: &str,
    ) -> Result<(i32, String, String), AppError> {
        let command = command.to_string();
        tokio::task::spawn_blocking(move || {
            let mut channel = session
                .channel_session()
                .map_err(|e| AppError::Custom(format!("SSH Channel 创建失败: {}", e)))?;
            channel
                .exec(&command)
                .map_err(|e| AppError::Custom(format!("SSH 命令发送失败: {}", e)))?;
            let mut stdout = String::new();
            channel.read_to_string(&mut stdout)?;
            let mut stderr = String::new();
            channel.stderr().read_to_string(&mut stderr)?;
            channel
                .wait_close()
                .map_err(|e| AppError::Custom(format!("SSH Channel 关闭失败: {}", e)))?;
            let exit_status = channel.exit_status().unwrap_or(-1);
            Ok((exit_status, stdout, stderr))
        })
        .await
        .map_err(|e| AppError::Custom(format!("SSH 任务执行失败: {}", e)))?
    }

    fn connect_authenticated_session(
        server: &SshServer,
        auth: AuthMaterial,
        timeout_secs: u64,
    ) -> Result<Session, AppError> {
        let endpoint = format!("{}:{}", server.host, server.port);
        let addr = endpoint
            .to_socket_addrs()
            .map_err(|e| AppError::Custom(format!("解析 SSH 地址失败: {}", e)))?
            .next()
            .ok_or_else(|| AppError::Custom("解析 SSH 地址失败：无可用地址".into()))?;
        let timeout = Duration::from_secs(timeout_secs);
        let tcp = TcpStream::connect_timeout(&addr, timeout)
            .map_err(|e| AppError::Custom(format!("SSH 连接失败: {}", e)))?;
        tcp.set_read_timeout(Some(timeout))?;
        tcp.set_write_timeout(Some(timeout))?;

        let mut session =
            Session::new().map_err(|e| AppError::Custom(format!("SSH 会话初始化失败: {}", e)))?;
        session.set_tcp_stream(tcp);
        session
            .handshake()
            .map_err(|e| AppError::Custom(format!("SSH 握手失败: {}", e)))?;

        let username = server.username.trim();
        if username.is_empty() {
            return Err(AppError::InvalidInput("SSH 用户名不能为空".into()));
        }
        match auth {
            AuthMaterial::Password(password) => session
                .userauth_password(username, &password)
                .map_err(|e| AppError::Custom(format!("SSH 密码认证失败: {}", e)))?,
            AuthMaterial::PrivateKey(path) => {
                if !path.is_file() {
                    return Err(AppError::InvalidInput(format!(
                        "私钥文件不存在: {}",
                        path.display()
                    )));
                }
                session
                    .userauth_pubkey_file(username, None, Path::new(&path), None)
                    .map_err(|e| AppError::Custom(format!("SSH 私钥认证失败: {}", e)))?;
            }
        }
        if !session.authenticated() {
            return Err(AppError::Custom("SSH 认证未通过".into()));
        }
        Ok(session)
    }

    fn decrypt_secret(
        db: &Database,
        seed_key: &str,
        nonce: &str,
        ciphertext: &str,
    ) -> Result<String, AppError> {
        let seed = db
            .get_config(seed_key)?
            .ok_or_else(|| AppError::Custom("本地加密种子不存在".into()))?;
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        let digest = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest[..32]);
        let nonce_bytes = general_purpose::STANDARD
            .decode(nonce)
            .map_err(|_| AppError::Custom("凭据 nonce 解码失败".into()))?;
        let ciphertext_bytes = general_purpose::STANDARD
            .decode(ciphertext)
            .map_err(|_| AppError::Custom("凭据密文解码失败".into()))?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| AppError::Custom("凭据密钥初始化失败".into()))?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext_bytes.as_ref())
            .map_err(|_| AppError::Custom("凭据解密失败".into()))?;
        String::from_utf8(plaintext).map_err(|_| AppError::Custom("凭据不是有效 UTF-8".into()))
    }

    fn expand_home_path(path: &str) -> PathBuf {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home).join(rest);
            }
        }
        PathBuf::from(path)
    }

    fn new_session_id(server_alias: &str) -> String {
        let mut random_bytes = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut random_bytes);
        format!(
            "{}-{}",
            server_alias.replace(|c: char| !c.is_ascii_alphanumeric(), "_"),
            general_purpose::URL_SAFE_NO_PAD.encode(random_bytes)
        )
    }
}
