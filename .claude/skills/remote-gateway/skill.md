---
name: remote-gateway
description: Tauri 桌面端 axum 远程访问网关骨架。Token 鉴权 + 失败追踪 + handler 分模块 + WebSocket。供移动端伴侣（mobile-app-architecture）通过 HTTP/WS 远程访问桌面能力。
effort: high
---

# Remote Gateway

## 触发场景

- 桌面应用要对外暴露 HTTP API 给手机/其他客户端
- 实现移动端伴侣方案（见 `mobile-app-architecture`）
- 需要受控暴露文件、AI 会话、终端等桌面能力到局域网/反向代理后

## 触发词

axum、远程访问、远程网关、Token 鉴权、移动端 API、reverse proxy、内网穿透、WebSocket

---

## 架构

```
┌──────────────────────────────────────────────┐
│  Tauri 桌面 Rust 进程                         │
│                                              │
│  ┌────────────────────────────────────┐    │
│  │  axum Router                       │    │
│  │  ├─ /v1/ping            (公开)     │    │
│  │  ├─ middleware: require_token      │    │
│  │  └─ /v1/* protected routes         │    │
│  │     ├─ /sessions  (handlers/)      │    │
│  │     ├─ /pty       (WebSocket)      │    │
│  │     └─ /upload    (multipart)      │    │
│  └──────┬─────────────────────────────┘    │
│         │ AppHandle 注入                    │
│  ┌──────▼─────┐                             │
│  │ AppState   │ DB / 配置 / Provider 等     │
│  └────────────┘                             │
└──────────────────────────────────────────────┘
        ↑ HTTP/WS（用户自配反向代理暴露公网）
        │
   ┌────┴────┐
   │ 移动端   │
   └─────────┘
```

---

## 模块骨架

```
src-tauri/src/remote/
├── mod.rs                  # 仅 pub mod 重导
├── config.rs               # RemoteConfig（port/host/base_path/token policy）
├── context.rs              # RemoteContext（封装 AppHandle + token，给 handler 注入）
├── auth.rs                 # require_token 中间件 + 失败响应
├── failure_tracker.rs      # 鉴权失败计数 + 限流（防暴力）
├── server.rs               # start() / shutdown / RemoteHandle / RemoteStatus
└── handlers/
    ├── mod.rs              # 公共 trait / 错误转换
    └── *.rs                # 各业务 handler（ping/sessions/upload/...）
```

---

## 关键依赖（Cargo.toml）

```toml
[dependencies]
axum = { version = "0.7", features = ["ws", "multipart"] }
tower-http = { version = "0.5", features = ["cors"] }
tokio = { version = "1", features = ["full"] }
```

---

## 范本：mod.rs

```rust
// 远程访问网关（移动端伴侣的桌面侧入口）
pub mod auth;
pub mod config;
pub mod context;
pub mod failure_tracker;
pub mod handlers;
pub mod server;

pub use config::RemoteConfig;
pub use server::{RemoteServerState, RemoteStatus};
```

## 范本：config.rs

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub host: String,        // 默认 "127.0.0.1"，公网由用户自配反向代理
    pub port: u16,           // 默认 7800
    pub base_path: String,   // 默认 "/v1"
    pub require_token: bool, // 默认 true
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 7800,
            base_path: "/v1".into(),
            require_token: true,
        }
    }
}
```

## 范本：context.rs

```rust
use std::sync::Arc;
use crate::remote::failure_tracker::FailureTracker;

#[derive(Clone)]
pub struct RemoteContext {
    pub app_handle: tauri::AppHandle,
    pub token: Option<String>,
    pub failure_tracker: Arc<FailureTracker>,
}

impl RemoteContext {
    pub fn new(app_handle: tauri::AppHandle, token: Option<String>) -> Self {
        Self {
            app_handle,
            token,
            failure_tracker: Arc::new(FailureTracker::default()),
        }
    }
}
```

## 范本：failure_tracker.rs（防暴力枚举）

```rust
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct FailureTracker {
    failures: Mutex<VecDeque<Instant>>,
    threshold: usize,
    window: Duration,
}

impl Default for FailureTracker {
    fn default() -> Self {
        Self {
            failures: Mutex::new(VecDeque::new()),
            threshold: 10,
            window: Duration::from_secs(60),
        }
    }
}

impl FailureTracker {
    pub fn record_failure(&self) {
        if let Ok(mut q) = self.failures.lock() {
            let now = Instant::now();
            // 清理窗口外的旧记录
            while let Some(&front) = q.front() {
                if now.duration_since(front) > self.window {
                    q.pop_front();
                } else {
                    break;
                }
            }
            q.push_back(now);
        }
    }

    pub fn is_locked(&self) -> bool {
        if let Ok(q) = self.failures.lock() {
            q.len() >= self.threshold
        } else {
            false
        }
    }
}
```

## 范本：auth.rs（中间件）

```rust
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use crate::remote::context::RemoteContext;

pub async fn require_token(
    State(ctx): State<RemoteContext>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if ctx.failure_tracker.is_locked() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    if let Some(expected) = ctx.token.as_deref() {
        let provided = headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "));

        match provided {
            Some(t) if t == expected => Ok(next.run(req).await),
            _ => {
                ctx.failure_tracker.record_failure();
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    } else {
        // 无 token → 仅本地调试模式
        Ok(next.run(req).await)
    }
}
```

## 范本：server.rs（核心结构）

```rust
use std::net::SocketAddr;
use std::sync::Mutex;

use axum::{middleware, routing::get, Json, Router};
use serde::Serialize;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};

use super::auth::require_token;
use super::config::RemoteConfig;
use super::context::RemoteContext;
use super::handlers;

pub struct RemoteHandle {
    pub config: RemoteConfig,
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

pub struct RemoteServerState {
    pub handle: Mutex<Option<RemoteHandle>>,
}

impl Default for RemoteServerState {
    fn default() -> Self {
        Self { handle: Mutex::new(None) }
    }
}

#[derive(Debug, Serialize)]
pub struct RemoteStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub host: Option<String>,
    pub base_path: Option<String>,
    pub token: Option<String>, // 给前端展示给用户配到手机端
}

#[derive(Serialize)]
struct PingResponse {
    ok: bool,
    version: &'static str,
}

async fn ping() -> Json<PingResponse> {
    Json(PingResponse { ok: true, version: env!("CARGO_PKG_VERSION") })
}

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

    // 公开路由
    let public = Router::new().route("/ping", get(ping));

    // 受保护路由（按业务追加 .route(...)）
    let protected = Router::new()
        // .route("/sessions", get(handlers::sessions::list))
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

    Ok(RemoteHandle {
        config,
        token,
        shutdown_tx: Some(shutdown_tx),
        join_handle,
    })
}
```

---

## 与 Tauri 集成（lib.rs）

```rust
.manage(remote::RemoteServerState::default())
.invoke_handler(tauri::generate_handler![
    remote_start, remote_stop, remote_status, remote_set_token,
])
```

Command 层职责：读 RemoteConfig → 调 `server::start()` → 把 handle 存进 RemoteServerState。

---

## 安全要点

| 项 | 做法 |
|----|------|
| **不暴露公网** | 默认绑定 127.0.0.1 / 局域网 IP；公网由用户自配反向代理 |
| **Token 强随机** | 首次启动生成 32+ 字符 random token，写入 secure store |
| **失败限流** | failure_tracker 阻止暴力枚举 |
| **HTTPS 由反代负责** | 应用自身只跑 HTTP，简化代码 |
| **CORS Any（开发期）→ 生产收紧** | 上线时按实际伴侣 origin 收紧 |
| **不内嵌隧道** | frpc/easytier 等会被国内杀软误报，改让用户自配 |

---

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| 把业务逻辑写在 handler 里 | handler 调 service 层，与 Tauri Command 共用 service |
| 忘记中间件 → 路由全公开 | `route_layer(middleware::from_fn(...))` 必须套在 protected router 上 |
| Token 明文写配置文件 | 写到 secure store 或加密 setting key |
| WebSocket 不做心跳 | 加 ping/pong，移动端切后台/弱网会断流 |
| Server shutdown 不优雅 | 用 oneshot::Sender 发停止信号 + await join_handle |
| `host` 直接绑 `0.0.0.0` 暴露公网 | 默认 `127.0.0.1`；公网走用户配置的反向代理 |

---

## 相关 skill

- `mobile-app-architecture` — 上层架构选型
- `tauri-commands` — Command 与 service/handler 共享逻辑
- `error-handler` — handler 中 Result/IntoResponse 的转换
- `bug-detective` — frpc 误报、Token 泄漏排查
