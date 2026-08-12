# 错误传播代码模式

以下示例以当前 `src-tauri/src/error.rs` 与 `src/lib/api/client.ts` 为契约证据。实际修改前仍需读取相邻模块，确认是否存在需要兼容的旧 Command。

## Rust 错误类型

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("未找到: {0}")]
    NotFound(String),
    #[error("参数无效: {0}")]
    InvalidInput(String),
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, serde::Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        let code = match &error {
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::InvalidInput(_) => "INVALID_INPUT",
            AppError::Database(_) => "DATABASE_ERROR",
            AppError::Io(_) => "IO_ERROR",
        };
        Self {
            code: code.to_string(),
            message: error.to_string(),
        }
    }
}
```

当前仓库还保留 `From<AppError> for String` 作为向后兼容。新增 Command 不得利用该兼容转换退化为字符串错误。

## 三层传播

```rust
// Database：保留底层类型
pub fn find_name(&self, id: i64) -> Result<Option<String>, AppError> {
    // 查询细节省略
    todo!()
}

// Service：增加业务语义
pub fn require_name(db: &Database, id: i64) -> Result<String, AppError> {
    db.find_name(id)?
        .ok_or_else(|| AppError::NotFound(format!("记录 {id}")))
}

// Command：只处理 IPC 边界，并保留结构化错误
#[tauri::command]
pub fn get_name(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<String, CommandError> {
    NameService::require_name(&state.db, id).map_err(Into::into)
}
```

新 Command 的硬约束是 `Result<T, CommandError>`。只有维护既有兼容接口时才允许保留 `Result<T, String>`，且不得把它复制到新实现。

## TypeScript API 封装

```typescript
export const nameApi = {
  get: (id: number): Promise<string> => invoke<string>("get_name", { id }),
};
```

`src/lib/api/client.ts` 已提供统一解析：

```typescript
export interface CommandError {
  code: string;
  message: string;
}

export function getErrorMessage(error: unknown): string {
  return parseCommandError(error).message;
}

export function getErrorCode(error: unknown): string {
  return parseCommandError(error).code;
}
```

页面和领域 API 不应各自 `JSON.parse` 错误，也不应依赖中文错误文案判断业务分支。

## 页面恢复

```tsx
async function loadName() {
  setLoading(true);
  try {
    setName(await nameApi.get(id));
  } catch (error: unknown) {
    if (getErrorCode(error) === "NOT_FOUND") {
      setName(null);
      message.warning(getErrorMessage(error));
      return;
    }
    message.error(getErrorMessage(error));
  } finally {
    setLoading(false);
  }
}
```

## ErrorBoundary 使用边界

ErrorBoundary 处理渲染、生命周期和构造过程中的未捕获异常。事件处理器、Promise、定时器和 Tauri `invoke` 失败仍需显式捕获。

## 禁止静默吞错

- Rust 不得用 `.ok()`、`unwrap_or_default()` 或伪造成功值丢弃真实失败；若业务确实允许缺失，必须在类型和注释中明确区分“无数据”与“查询失败”。
- TypeScript 不得使用空 `catch {}`，也不得捕获后返回 `[]`、`null`、`false` 等默认值冒充成功。
- 允许降级时，必须记录脱敏诊断信息、返回可识别的降级状态，并让 UI 表达数据可能不完整。
- `parseCommandError` 内对“非 JSON 错误”的捕获是契约兼容解析，不代表调用方可以吞掉该错误；它仍返回 `UNKNOWN` 并保留原始消息。

## 测试建议

- Database：底层失败能转换为预期 `AppError`。
- Service：未找到、冲突和输入错误有稳定类别。
- Command/API：新 Command 返回结构化 `CommandError`，前端 `getErrorCode/getErrorMessage` 能稳定识别。
- React：失败提示出现、loading 恢复、重试有效。
- 安全：日志和 UI 不包含 token、密码、完整连接串或隐私数据。
- 静默失败：对 `.ok()`、空 `catch` 和默认值回退路径有审查或测试，确保错误未丢失。
