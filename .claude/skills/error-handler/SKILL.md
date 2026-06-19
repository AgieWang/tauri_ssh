---
name: error-handler
description: |
  Tauri 异常处理技能，覆盖 Rust 错误处理和 React 错误边界。

  触发场景：
  - 需要设计错误处理策略
  - 需要处理 Rust Command 中的错误
  - 需要处理前端 invoke 调用失败
  - 需要实现全局错误处理

  触发词：异常、错误处理、Error、Result、try-catch、panic、崩溃、错误边界
---

# Tauri 异常处理

## 分层错误处理策略

```
前端 (React)                         后端 (Rust)
┌──────────────────────┐          ┌──────────────────────┐
│ getErrorMessage()    │          │ AppError 枚举        │
│ getErrorCode()       │          │ CommandError struct   │
│ ErrorBoundary        │          │ thiserror            │
│ try-catch            │ ◄─IPC─► │ Result<T, CommandError>│
│ Ant Design Result    │          │ 三层错误传播          │
└──────────────────────┘          └──────────────────────┘
```

---

## Rust 错误处理

### 1. AppError 枚举（src-tauri/src/error.rs）

```rust
use thiserror::Error;

/// 应用统一错误类型
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

    #[error("{0}")]
    Custom(String),
}

/// 让 AppError 转换为 String（向后兼容）
impl From<AppError> for String {
    fn from(err: AppError) -> String {
        err.to_string()
    }
}

/// Command 层返回的结构化错误（序列化为 JSON 传给前端）
#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    pub code: String,
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
            AppError::Custom(_) => "INTERNAL",
        };
        CommandError {
            code: code.to_string(),
            message: err.to_string(),
        }
    }
}
```

#### 错误码映射表

| AppError 变体 | 错误码 | 含义 |
|--------------|--------|------|
| `Io(...)` | `IO_ERROR` | 文件/IO 操作错误 |
| `Database(...)` | `DATABASE_ERROR` | 数据库操作错误 |
| `Json(...)` | `JSON_ERROR` | JSON 解析/序列化错误 |
| `NotFound(...)` | `NOT_FOUND` | 资源未找到 |
| `InvalidInput(...)` | `INVALID_INPUT` | 输入参数无效 |
| `Custom(...)` | `INTERNAL` | 内部/自定义错误 |

### 2. 三层错误传播

#### Database 层（返回 AppError）

```rust
// database/mod.rs
impl Database {
    pub fn get_config(&self, key: &str) -> Result<Option<String>, AppError> {
        let conn = self.conn.lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;

        let mut stmt = conn.prepare("SELECT value FROM app_config WHERE key = ?1")?;

        let result = stmt
            .query_row([key], |row| row.get::<_, String>(0))
            .ok();

        Ok(result)
    }
}
```

#### Service 层（转换业务错误）

```rust
// services/config_service.rs
impl ConfigService {
    pub fn get_required(&self, db: &Database, key: &str) -> Result<String, AppError> {
        db.get_config(key)?
            .ok_or_else(|| AppError::NotFound(format!("配置 {} 不存在", key)))
    }
}
```

#### Command 层（转换为 CommandError 给前端）

```rust
// commands/config.rs
use tauri::State;
use crate::error::CommandError;

#[tauri::command]
pub fn get_config(db: State<'_, Database>, key: String) -> Result<String, CommandError> {
    db.get_config(&key)
        .map_err(CommandError::from)?  // AppError -> CommandError
        .ok_or_else(|| CommandError::from(AppError::NotFound(format!("配置 {} 不存在", key))))
}
```

### 3. Mutex 安全处理

```rust
// ✅ 正确：使用 map_err 转换 Mutex 错误
let conn = self.conn.lock()
    .map_err(|e| AppError::Custom(format!("锁定失败: {}", e)))?;

// ❌ 错误：使用 unwrap（会 panic）
let conn = self.conn.lock().unwrap();
```

### 4. 错误传播模式

```rust
// ✅ 推荐：使用 ? 自动传播
#[tauri::command]
fn read_config(path: String) -> Result<String, AppError> {
    let content = std::fs::read_to_string(&path)?;  // IoError 自动转换
    if content.is_empty() {
        return Err(AppError::InvalidInput("配置文件为空".into()));
    }
    Ok(content)
}

// ❌ 错误：使用 unwrap/expect
#[tauri::command]
fn bad_read(path: String) -> String {
    std::fs::read_to_string(&path).unwrap()  // panic! 崩溃整个应用
}
```

---

## React 错误处理

### 1. invoke 错误处理（使用 getErrorMessage）

```tsx
import { message } from "antd";
import { invoke } from "@tauri-apps/api/core";
import { getErrorMessage, getErrorCode } from "@/lib/api/client";

// ✅ 标准模式：使用 try-catch + getErrorMessage
async function loadData() {
  try {
    const result = await invoke<DataType>("get_data");
    setData(result);
    message.success("加载成功");
  } catch (error) {
    message.error(getErrorMessage(error));  // 解析 CommandError 中的 message
    console.error("加载失败:", error);
  }
}

// ✅ 条件错误处理：根据错误码执行不同逻辑
async function loadUser(id: number) {
  try {
    const user = await invoke<User>("get_user", { id });
    setUser(user);
  } catch (error) {
    if (getErrorCode(error) === "NOT_FOUND") {
      message.warning("用户不存在，即将跳转...");
      navigate("/users");
    } else {
      message.error(getErrorMessage(error));
    }
  }
}
```

### 2. 前端错误解析工具（src/lib/api/client.ts）

```typescript
// src/lib/api/client.ts

/** CommandError 结构（与 Rust CommandError 对齐） */
interface CommandError {
  code: string;
  message: string;
}

/** 解析 invoke 抛出的错误为 CommandError */
export function parseCommandError(error: unknown): CommandError | null {
  if (typeof error === "string") {
    try {
      const parsed = JSON.parse(error);
      if (parsed.code && parsed.message) return parsed;
    } catch {
      // 非 JSON 字符串，返回 null
    }
  }
  return null;
}

/** 从错误中提取用户可读的消息 */
export function getErrorMessage(error: unknown): string {
  const cmdErr = parseCommandError(error);
  if (cmdErr) return cmdErr.message;
  return String(error);
}

/** 从错误中提取错误码（用于条件判断） */
export function getErrorCode(error: unknown): string | null {
  const cmdErr = parseCommandError(error);
  return cmdErr?.code ?? null;
}
```

### 3. 封装 API 调用（src/lib/api/index.ts）

```typescript
import { invoke } from "@tauri-apps/api/core";
import { getErrorMessage } from "@/lib/api/client";
import type { AppConfig } from "@/types";

/** 配置管理 API */
export const configApi = {
  getAll: () => invoke<AppConfig[]>("get_all_config"),
  get: (key: string) => invoke<string>("get_config", { key }),
  set: (key: string, value: string) =>
    invoke<void>("set_config", { key, value }),
  delete: (key: string) => invoke<void>("delete_config", { key }),
};

// 使用时统一处理错误
try {
  const configs = await configApi.getAll();
} catch (error) {
  message.error(getErrorMessage(error));  // 使用 getErrorMessage 而非 String(error)
}
```

### 4. ErrorBoundary 组件（Ant Design Result）

```tsx
import { Component, ReactNode } from "react";
import { Result, Button } from "antd";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: any) {
    console.error("ErrorBoundary 捕获错误:", error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback || (
        <Result
          status="error"
          title="应用出现错误"
          subTitle={this.state.error?.message}
          extra={
            <Button type="primary" onClick={() => window.location.reload()}>
              刷新页面
            </Button>
          }
        />
      );
    }
    return this.props.children;
  }
}
```

### 5. 全局错误处理 Hook

```tsx
import { useState } from "react";
import { message } from "antd";
import { invoke } from "@tauri-apps/api/core";
import { getErrorMessage, getErrorCode } from "@/lib/api/client";

export function useErrorHandler() {
  const [error, setError] = useState<string | null>(null);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function safeInvoke<T>(
    cmd: string,
    args?: Record<string, unknown>,
    showSuccessMsg?: string
  ): Promise<T | null> {
    setLoading(true);
    setError(null);
    setErrorCode(null);

    try {
      const result = await invoke<T>(cmd, args);
      if (showSuccessMsg) {
        message.success(showSuccessMsg);
      }
      return result;
    } catch (e) {
      const msg = getErrorMessage(e);
      const code = getErrorCode(e);
      setError(msg);
      setErrorCode(code);
      message.error(msg);
      console.error(`Command "${cmd}" 失败 [${code}]:`, msg);
      return null;
    } finally {
      setLoading(false);
    }
  }

  return {
    error,
    errorCode,
    loading,
    safeInvoke,
    clearError: () => { setError(null); setErrorCode(null); },
  };
}

// 使用示例
const { loading, safeInvoke } = useErrorHandler();

async function handleSave() {
  const result = await safeInvoke<string>(
    "set_config",
    { key: "theme", value: "dark" },
    "保存成功"
  );
  if (result) {
    // 成功后的逻辑
  }
}
```

---

## 错误流程图

```
┌─────────────────────────────────────────────────────────────┐
│                       Rust 后端                              │
├─────────────────────────────────────────────────────────────┤
│  Database::get_config()                                     │
│    ↓ 返回 Result<Option<String>, AppError>                 │
│  Service::get_required()                                    │
│    ↓ 业务校验，转换 None 为 AppError::NotFound             │
│  Command::get_config()                                      │
│    ↓ CommandError::from(AppError) → { code, message }      │
│    ↓ 返回 Result<T, CommandError>（序列化为 JSON）          │
└─────────────────────────────────────────────────────────────┘
                             ↓ IPC (invoke) → JSON 错误字符串
┌─────────────────────────────────────────────────────────────┐
│                      React 前端                              │
├─────────────────────────────────────────────────────────────┤
│  try { await configApi.get("theme") }                      │
│  catch (error) {                                            │
│    getErrorMessage(error)  → 用户可读的错误消息             │
│    getErrorCode(error)     → "NOT_FOUND" 等错误码          │
│    message.error(getErrorMessage(error))                    │
│  }                                                          │
│    ↓ 用户看到 Ant Design 错误提示                           │
└─────────────────────────────────────────────────────────────┘
```

---

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| Rust 中 `unwrap()` 处理可能失败的操作 | 使用 `?` 运算符 + `Result<T, AppError>` |
| 不定义统一错误类型 | 使用 `thiserror` 定义 `AppError` 枚举 |
| Command 返回 `Result<T, String>` | 返回 `Result<T, CommandError>`（结构化错误） |
| 前端不 catch invoke 错误 | 所有 `invoke` 调用都用 `try-catch` |
| 前端用 `String(error)` 显示错误 | 使用 `getErrorMessage(error)` 解析错误消息 |
| 前端不区分错误类型 | 使用 `getErrorCode(error)` 进行条件处理 |
| 错误信息不可读 | 提供用户友好的中文错误提示 |
| Mutex 使用 `unwrap()` | 使用 `map_err` 转换为 `AppError::Custom` |
| 前端用 `alert()` 显示错误 | 使用 Ant Design `message.error()` |
| 不处理 ErrorBoundary | 在根组件添加 `<ErrorBoundary>` |

---

## 完整示例（三层 + 前端）

### Rust 后端

```rust
// error.rs
#[derive(Debug, Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("未找到: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<AppError> for CommandError {
    fn from(err: AppError) -> Self {
        let code = match &err {
            AppError::Database(_) => "DATABASE_ERROR",
            AppError::NotFound(_) => "NOT_FOUND",
            // ... 其他变体
        };
        CommandError { code: code.to_string(), message: err.to_string() }
    }
}

// database/mod.rs
impl Database {
    pub fn get_user(&self, id: i64) -> Result<Option<User>, AppError> {
        let conn = self.conn.lock()
            .map_err(|e| AppError::Custom(e.to_string()))?;
        // ... SQL 查询
    }
}

// services/user_service.rs
impl UserService {
    pub fn get_required(&self, db: &Database, id: i64) -> Result<User, AppError> {
        db.get_user(id)?
            .ok_or_else(|| AppError::NotFound(format!("用户 {} 不存在", id)))
    }
}

// commands/user.rs
#[tauri::command]
pub fn get_user(db: State<'_, Database>, id: i64) -> Result<User, CommandError> {
    let service = UserService::new();
    service.get_required(&db, id)
        .map_err(CommandError::from)
}
```

### React 前端

```tsx
import { message } from "antd";
import { invoke } from "@tauri-apps/api/core";
import { getErrorMessage, getErrorCode } from "@/lib/api/client";

async function loadUser(id: number) {
  try {
    const user = await invoke<User>("get_user", { id });
    setUser(user);
  } catch (error) {
    if (getErrorCode(error) === "NOT_FOUND") {
      message.warning("用户不存在");
    } else {
      message.error(getErrorMessage(error));
    }
  }
}
```
