# IPC 注册链检查清单

## 目录

1. 八段链路
2. 最小实现骨架
3. 注册与导出
4. 验证

## 1. 八段链路

按真实需求选择层，禁止为了“完整”空造文件：

1. `src-tauri/src/models/`：共享请求、响应和领域模型。
2. `src-tauri/src/database/`：仅当需要 SQLite 数据访问时新增 DAO。
3. `src-tauri/src/services/`：业务校验、组合和事务编排。
4. `src-tauri/src/commands/`：薄 IPC 入口。
5. `src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`：模块导出与 handler 注册。
6. `src/types/`：与 JSON 结果一致的 TypeScript 类型。
7. `src/lib/api/<domain>.ts`、`index.ts`：类型安全的 invoke 封装与统一导出。
8. `src/pages/` 或调用方：loading、error、empty、success 状态。

## 2. 最小实现骨架

```rust
// models/config.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    pub key: String,
    pub value: String,
}
```

```rust
// database/config.rs
impl Database {
    pub fn get_config(&self, key: &str) -> Result<Option<AppConfig>, AppError> {
        // 使用参数化 SQL；锁失败与查询错误都向上传播。
    }
}
```

```rust
// services/config.rs
pub struct ConfigService;

impl ConfigService {
    pub fn get(db: &Database, key: &str) -> Result<Option<AppConfig>, AppError> {
        if key.trim().is_empty() {
            return Err(AppError::InvalidInput("配置键不能为空".into()));
        }
        db.get_config(key)
    }
}
```

```rust
// commands/config.rs
#[tauri::command]
pub fn get_config(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Option<AppConfig>, CommandError> {
    ConfigService::get(&state.db, &key).map_err(Into::into)
}
```

以当前仓库的 `AppError -> CommandError` 转换为准；若相似模块返回其他类型，复用现有模式，不照抄示例。

## 3. 注册与导出

### Rust

```rust
// commands/mod.rs
pub mod config;

// lib.rs
.invoke_handler(tauri::generate_handler![
    commands::config::get_config,
])
```

检查同名 Command 是否已经注册。新增插件能力时还要检查 `.plugin(...)` 和 `src-tauri/capabilities/*.json`；普通自定义 Command 不需要虚构插件 permission。

### TypeScript

```typescript
// src/types/config.ts
export interface AppConfig {
  key: string;
  value: string;
}
```

```typescript
// src/lib/api/config.ts
import type { AppConfig } from "@/types";
import { invoke } from "./client";

export const configApi = {
  get: (key: string) =>
    invoke<AppConfig | null>("get_config", { key }),
};
```

```typescript
// src/lib/api/index.ts
export { configApi } from "./config";
```

调用组件只导入 `configApi`，不从 `@tauri-apps/api/core` 直接导入 invoke。

## 4. 验证

### 静态检查

- 搜索 Command 名，确认仅有预期定义、注册和调用。
- 核对 `commands/mod.rs`、`services/mod.rs`、API/类型 barrel 导出。
- 核对 serde 派生、字段 rename、Option/null、数字范围。
- 核对输入校验与错误码，没有 `unwrap()`、`panic!()` 或吞错 fallback。
- 数据库操作使用参数化 SQL，Command 不直接执行 SQL。

### 命令检查

按实际改动选择：

```bash
cd src-tauri && cargo fmt --check
cd src-tauri && cargo check
pnpm exec tsc --noEmit
pnpm build
git diff --check
```

有聚焦测试时先运行聚焦测试，再决定是否扩大。新增页面或交互调用时，用 Codex 内置浏览器优先、Chrome 次之，确认：

- 页面发出了正确 payload。
- Rust Command 被找到并返回预期 JSON。
- 成功、空数据和错误状态都可见。
- 控制台无未处理 Promise、类型或权限错误。
