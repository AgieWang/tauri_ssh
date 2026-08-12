# `/command` 实现模板

## 目录

1. 数据模型
2. Command/Service/Database
3. 注册
4. TypeScript API
5. 高级类型去向

以下代码是结构示意。错误类型、模块拆分和数据库 API 必须以当前相似实现为准。

## 1. 数据模型

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyData {
    pub id: String,
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMyDataInput {
    pub id: String,
}
```

结构体字段是否 camelCase 取决于 serde 属性；前端必须匹配实际 JSON。

## 2. Command / Service / Database

### 纯计算同步 Command

```rust
#[tauri::command]
pub fn format_text(text: String) -> Result<String, CommandError> {
    let value = text.trim();
    if value.is_empty() {
        return Err(AppError::InvalidInput("文本不能为空".into()).into());
    }
    Ok(value.to_uppercase())
}
```

### 三层 Command

```rust
#[tauri::command]
pub fn get_my_data(
    state: tauri::State<'_, AppState>,
    input: GetMyDataInput,
) -> Result<Option<MyData>, CommandError> {
    MyService::get(&state.db, &input.id).map_err(Into::into)
}
```

```rust
pub struct MyService;

impl MyService {
    pub fn get(db: &Database, id: &str) -> Result<Option<MyData>, AppError> {
        if id.trim().is_empty() {
            return Err(AppError::InvalidInput("ID 不能为空".into()));
        }
        db.get_my_data(id)
    }
}
```

```rust
impl Database {
    pub fn get_my_data(&self, id: &str) -> Result<Option<MyData>, AppError> {
        // 沿用当前连接锁、OptionalExtension 和 row mapping 模式；使用参数化 SQL。
    }
}
```

禁止用 `lock().unwrap()`；锁错误和 SQL 错误向上传播。

### async 文件 IO

```rust
#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String, CommandError> {
    let safe_path = FileService::validate_read_path(&path)?;
    tokio::fs::read_to_string(safe_path)
        .await
        .map_err(AppError::from)
        .map_err(Into::into)
}
```

路径验证、文件大小和编码必须处理。若使用 Tauri fs/dialog 插件，同时更新 Builder 和 Capabilities。

## 3. 注册

```rust
// commands/mod.rs
pub mod my_domain;

// services/mod.rs（仅新增 Service 时）
pub mod my_domain;

// lib.rs
.invoke_handler(tauri::generate_handler![
    commands::my_domain::get_my_data,
])
```

若 handler 由宏或子 builder 集中组装，遵循现有方式，不创建第二套注册点。

## 4. TypeScript API

```typescript
export interface MyData {
  id: string;
  name: string;
}

export interface GetMyDataInput {
  id: string;
}
```

```typescript
import type { GetMyDataInput, MyData } from "@/types";
import { invoke } from "./client";

export const myDataApi = {
  get: (input: GetMyDataInput) =>
    invoke<MyData | null>("get_my_data", { input }),
};
```

```typescript
// lib/api/index.ts
export { myDataApi } from "./myData";
```

调用侧：

```typescript
try {
  const data = await myDataApi.get({ id });
  setData(data);
} catch (error: unknown) {
  message.error(getErrorMessage(error));
}
```

不要在组件内重新定义接口或直接 import `invoke`。

## 5. 高级类型去向

- AppHandle/Window/State 组合、锁跨 await：读取 `tauri-commands/references/injection-and-async.md`。
- 进度/事件/Channel：读取 `tauri-commands/references/progress-and-streaming.md` 和 `tauri-events`。
- 子进程：读取 `tauri-commands/references/complete-examples.md`，必须处理 allowlist、超时、退出码、脱敏和 Windows `CREATE_NO_WINDOW`。
- 文件功能：继续加载 `file-storage` 与 `security-permissions`。
- SQLite：继续加载 `database-ops`，遵循真实 schema/迁移机制。
