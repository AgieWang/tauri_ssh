// /v1/ping — 公开健康探测端点
//
// 移动端首次配对前用此端点验证 baseUrl 可达。

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct PingResponse {
    pub ok: bool,
    pub version: &'static str,
    pub service: &'static str,
}

pub async fn ping() -> Json<PingResponse> {
    Json(PingResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION"),
        service: "tauri-remote-gateway",
    })
}
