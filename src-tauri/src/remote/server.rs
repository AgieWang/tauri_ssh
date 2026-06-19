// axum server — 启动 / 优雅关停 / 状态结构
//
// 用法：
//   let handle = remote::server::start(config, app_handle.clone(), Some(token)).await?;
//   *state.handle.lock().unwrap() = Some(handle);
//   // ... 关停：handle.shutdown().await;

use std::net::SocketAddr;
use std::sync::Mutex;

use axum::{middleware, routing::get, Router};
use serde::Serialize;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};

use super::auth::require_token;
use super::config::RemoteConfig;
use super::context::RemoteContext;
use super::handlers;

/// 启动后返回的运行句柄。
pub struct RemoteHandle {
    pub config: RemoteConfig,
    /// Token 副本（用于 status 接口展示，方便用户配到手机端）
    pub token: Option<String>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: tokio::task::JoinHandle<()>,
}

impl RemoteHandle {
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = (&mut self.join_handle).await;
    }
}

/// Tauri State：持有当前运行的 handle（None 表示未启动）
pub struct RemoteServerState {
    pub handle: Mutex<Option<RemoteHandle>>,
}

impl Default for RemoteServerState {
    fn default() -> Self {
        Self {
            handle: Mutex::new(None),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RemoteStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub host: Option<String>,
    pub base_path: Option<String>,
    /// 当前生效的 Token —— 前端首次启动后展示给用户配到手机端
    pub token: Option<String>,
}

/// 启动 axum 服务器。
///
/// - `app_handle`：用于 handler 访问 AppState（DB / 配置目录 / 事件总线）
/// - `token`：可选静态 Token；None 时关闭鉴权（仅本地调试）
pub async fn start(
    config: RemoteConfig,
    app_handle: tauri::AppHandle,
    token: Option<String>,
) -> Result<RemoteHandle, String> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let ctx = RemoteContext::new(app_handle, token.clone());

    // 公开路由（无需 Token）
    let public = Router::new().route("/ping", get(handlers::ping::ping));

    // 受保护路由 —— 业务 handler 在此追加 .route(...)
    let protected = Router::new()
        // .route("/sessions", get(handlers::sessions::list))
        // .route("/upload", axum::routing::post(handlers::upload::upload))
        .route_layer(middleware::from_fn_with_state(ctx.clone(), require_token));

    let app = Router::new()
        .nest(&config.base_path, public.merge(protected))
        .layer(cors)
        .with_state(ctx);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| e.to_string())?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let join_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    log::info!(
        "remote gateway started at {}:{}{}",
        config.host,
        config.port,
        config.base_path
    );

    Ok(RemoteHandle {
        config,
        token,
        shutdown_tx: Some(shutdown_tx),
        join_handle,
    })
}
