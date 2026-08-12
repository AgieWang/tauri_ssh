# IPC 契约参考

## 目录

1. 契约表
2. 命名与序列化
3. 返回值与错误
4. 示例
5. 常见错误

## 1. 契约表

实现前为每个接口记录以下项目，评审时逐格核对：

| 项目 | 示例 | 核对重点 |
|---|---|---|
| Command 名 | `get_config` | Rust 函数、注册项、invoke 字符串完全一致 |
| Rust 入参 | `key: String` | 类型、是否可空、验证位置 |
| JS payload | `{ key }` | 参数键转换规则和嵌套层级 |
| Rust 返回 | `Result<Option<AppConfig>, CommandError>` | `None` 是否是合法结果 |
| TS 返回 | `Promise<AppConfig | null>` | 与 JSON 实际值一致 |
| 错误 | `{ code, message }` | UI 能否稳定解析和分支处理 |

## 2. 命名与序列化

### Command 与方法名

- Rust Command 和 `invoke()` 字符串使用同一 `snake_case` 名称。
- TypeScript 业务方法使用 `camelCase`，但内部 invoke 字符串不得随之变化。
- 不用 REST URL、HTTP 动词或 Controller 概念代替 IPC Command。

```rust
#[tauri::command]
pub fn create_user(user_name: String) -> Result<User, CommandError> {
    // 参数校验后调用 Service。
}
```

```typescript
export const userApi = {
  createUser: (userName: string) =>
    invoke<User>("create_user", { userName }),
};
```

Tauri Command 参数默认可按 camelCase 传入，但若使用 `#[tauri::command(rename_all = "snake_case")]` 等覆盖，必须按实际配置调用。

### 结构体字段

serde 不会因为前端是 TypeScript 就自动改字段名。需要 camelCase JSON 时显式声明，并让 TS 与序列化结果一致：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub user_id: i64,
    pub display_name: String,
}
```

```typescript
export interface User {
  userId: number;
  displayName: string;
}
```

若现有模型没有 `rename_all`，TypeScript 必须保留实际 JSON 字段，不能根据习惯改名。

### 常用类型映射

| Rust | JSON / TypeScript | 注意 |
|---|---|---|
| `String` | `string` | 编码统一 UTF-8 |
| `bool` | `boolean` | 不用 0/1 猜布尔 |
| `i32/u32/f64` | `number` | 核对范围和精度 |
| `i64/u64` | `number` 或字符串 | 超过 JS 安全整数时必须定协议 |
| `Vec<T>` | `T[]` | 核对元素字段 |
| `Option<T>` | `T | null` | serde 默认序列化为 null；不要默认写成 undefined |
| `HashMap<String, T>` | `Record<string, T>` | key 必须可序列化为字符串 |
| `()` | `void` | HTTP/Dev API 降级路径也要兼容空响应 |

## 3. 返回值与错误

常见返回形态：

```rust
pub fn do_action() -> Result<(), CommandError>;
pub fn get_item(id: i64) -> Result<Item, CommandError>;
pub fn list_items() -> Result<Vec<Item>, CommandError>;
pub fn find_item(id: i64) -> Result<Option<Item>, CommandError>;
```

优先沿用 `src-tauri/src/error.rs` 的结构化 Command 错误；不要为单个接口另造字符串协议。前端通过 `parseCommandError` / `getErrorMessage` 处理 `unknown`：

```typescript
try {
  return await userApi.getUser(id);
} catch (error: unknown) {
  message.error(getErrorMessage(error));
  throw error;
}
```

不要用 ``message.error(`失败: ${error}`)``，对象错误会显示成 `[object Object]`，也会丢失错误码。

## 4. 示例

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub fn get_config(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Option<AppConfig>, CommandError> {
    ConfigService::get(&state.db, &key).map_err(Into::into)
}
```

```typescript
export interface AppConfig {
  key: string;
  value: string;
}

export const configApi = {
  get: (key: string) =>
    invoke<AppConfig | null>("get_config", { key }),
};
```

异步 Command 的前端签名不需要特殊形式，仍返回 `Promise<T>`。State/AppHandle/Window 是框架注入参数，不出现在 JS payload 中；详细实现读取 `tauri-commands/references/injection-and-async.md`。

## 5. 常见错误

| 错误 | 修正 |
|---|---|
| 组件直接 `invoke()` | 封装到 `src/lib/api/<domain>.ts` |
| Rust/TS 各自手写不同字段 | 用契约表逐字段对齐并验证 JSON |
| 将 `Option<T>` 写成必填 T | TS 使用 `T | null` 并处理空态 |
| i64 ID 默认当普通 number | 先核对最大值，必要时传字符串 |
| Command 中写 SQL | SQL 下沉 Database 层 |
| 新建一套字符串错误 | 沿用结构化 `CommandError` |
| 网络 IO 放同步 Command | 使用 async；高级模式交给 `tauri-commands` |
