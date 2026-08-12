use serde::Serialize;
use thiserror::Error;

/// 应用统一错误类型（内部使用）
#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("未找到: {0}")]
    NotFound(String),

    #[error("参数无效: {0}")]
    InvalidInput(String),

    /// 全文索引是可重建的派生数据。索引存在但历史内容失配时，搜索请求不能在持有
    /// 数据库锁的情况下同步全量回填；调用方应改走显式重建流程后重试。
    #[error("全文索引尚未准备完成，请在知识库中重建全文索引后重试")]
    KnowledgeFtsRebuildRequired,

    /// Provider 网络、超时或响应正文中断等临时故障。该类别允许前端提供明确重试，
    /// 同时与参数错误、权限错误和不可恢复的响应格式错误区分开。
    #[error("{0}")]
    ProviderTransient(String),

    #[error("{0}")]
    Custom(String),
}

/// 结构化错误响应（传递给前端）
///
/// 前端可通过 `code` 字段做精细错误处理：
/// ```typescript
/// try { await invoke("get_config", { key }); }
/// catch (e) {
///   const err = JSON.parse(e as string);
///   if (err.code === "NOT_FOUND") { /* 特定处理 */ }
/// }
/// ```
#[derive(Debug, Serialize)]
pub struct CommandError {
    /// 错误码（大写蛇形：IO_ERROR, DATABASE_ERROR, NOT_FOUND, INVALID_INPUT, INTERNAL）
    pub code: String,
    /// 用户友好的错误信息
    pub message: String,
}

impl From<AppError> for CommandError {
    fn from(err: AppError) -> Self {
        let code = match &err {
            AppError::Io(_) => "IO_ERROR",
            AppError::Database(_) => "DATABASE_ERROR",
            AppError::Json(_) => "JSON_ERROR",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::InvalidInput(_) => "INVALID_INPUT",
            AppError::KnowledgeFtsRebuildRequired => "KNOWLEDGE_FTS_REBUILD_REQUIRED",
            AppError::ProviderTransient(_) => "PROVIDER_TRANSIENT",
            AppError::Custom(_) => "INTERNAL",
        };
        let message = match &err {
            AppError::InvalidInput(message) => message.clone(),
            _ => err.to_string(),
        };
        CommandError {
            code: code.to_string(),
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, CommandError};

    #[test]
    fn exposes_a_stable_rebuild_code_for_an_incomplete_fts_index() {
        let error = CommandError::from(AppError::KnowledgeFtsRebuildRequired);

        assert_eq!(error.code, "KNOWLEDGE_FTS_REBUILD_REQUIRED");
        assert!(error.message.contains("重建全文索引"));
    }

    #[test]
    fn exposes_a_stable_code_for_retryable_provider_failures() {
        let error = CommandError::from(AppError::ProviderTransient(
            "Provider 回答超时，请重试".to_string(),
        ));

        assert_eq!(error.code, "PROVIDER_TRANSIENT");
        assert_eq!(error.message, "Provider 回答超时，请重试");
    }
}

/// 让 Tauri Command 能直接使用 CommandError 作为错误类型
/// Tauri 要求错误类型实现 Into<InvokeError>，序列化为 JSON 字符串传递给前端
impl From<CommandError> for String {
    fn from(err: CommandError) -> String {
        serde_json::to_string(&err).unwrap_or_else(|_| err.message)
    }
}

/// 保留 AppError -> String 的转换（向后兼容）
impl From<AppError> for String {
    fn from(err: AppError) -> String {
        let cmd_err: CommandError = err.into();
        cmd_err.into()
    }
}
