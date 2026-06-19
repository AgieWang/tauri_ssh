// require_token 中间件 —— 校验 Authorization: Bearer <token>
//
// 无 token 时（开发态）放行；锁定窗口期内统一返 429；其余统一 401 + record_failure。

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
