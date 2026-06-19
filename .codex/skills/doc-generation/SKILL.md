---
name: doc-generation
description: |
  项目文档自动生成。基于代码扫描自动生成 Command 文档、模块文档和数据库文档。

  触发场景：
  - 需要生成项目文档
  - 需要更新 Command/API 文档
  - 需要生成数据库文档

  触发词：文档生成、doc-generation、生成文档、Command文档、API文档、数据库文档
---

# doc-generation - 项目文档自动生成

基于 Tauri 项目代码自动生成各类开发文档。

---

## 文档类型

| 类型 | 输出路径 | 数据来源 |
|------|---------|---------|
| Command 文档 | `docs/commands/` | Rust `#[tauri::command]` 函数扫描 |
| 模块文档 | `docs/modules/` | Rust 后端 + React 前端代码结构 |
| 数据库文档 | `docs/database/` | rusqlite Schema 定义 / 迁移文件 |

---

## 1. Command 文档生成

### 1.1 扫描 Tauri Commands

Tauri 项目通过 `#[tauri::command]` 宏定义前后端通信接口：

```bash
# 扫描所有 Command 定义
Glob pattern: "src-tauri/src/commands/*.rs"
```

对每个 Command 文件，提取：
- `#[tauri::command]` 标记的函数名
- 函数签名（参数类型 + 返回类型）
- 函数注释（`///` doc comment）
- State 依赖（`tauri::State<T>` 参数）
- 错误类型（`Result<T, E>` 中的 E）

### 1.2 扫描前端 invoke 调用

```bash
# 扫描所有 invoke 调用点
Grep pattern: "invoke\(" in src/**/*.ts src/**/*.tsx
```

对每个 `invoke` 调用，提取：
- 调用的 command 名称（`invoke("command_name", ...)`）
- 传递的参数
- 调用所在的页面/组件

### 1.3 输出 Command 文档

```markdown
## {模块名} Commands

### {command_name}

- **描述**: {doc comment}
- **Rust 函数**: `{fn_name}()`
- **文件**: `src-tauri/src/commands/{module}.rs`
- **参数**:
  | 参数名 | 类型 | 说明 |
  |--------|------|------|
  | name | String | 名称 |
  | config | AppConfig | 配置对象 |
- **返回值**: `Result<Vec<Item>, AppError>`
- **State 依赖**: `DbConnection`, `AppState`
- **前端调用点**:
  - `src/pages/settings.tsx:45` — `invoke("get_settings")`
  - `src/lib/api/index.ts:12` — API 封装

### Capabilities 权限要求

| Command | 需要的权限 | 声明文件 |
|---------|-----------|---------|
| {command} | `core:default` | `src-tauri/capabilities/default.json` |
```

---

## 2. 模块文档生成

### 2.1 扫描 Rust 后端模块

```bash
# 列出后端模块
Glob pattern: "src-tauri/src/commands/*.rs"
Glob pattern: "src-tauri/src/services/*.rs"
Glob pattern: "src-tauri/src/database/*.rs"
Glob pattern: "src-tauri/src/models/*.rs"
```

### 2.2 扫描 React 前端模块

```bash
# 列出前端页面
Glob pattern: "src/pages/**/*.tsx"
# 列出 Store
Glob pattern: "src/store/*.ts"
# 列出 API 封装
Glob pattern: "src/lib/api/*.ts"
```

### 2.3 对每个功能模块生成文档

```markdown
# {模块名} 模块

## 概述
- **功能**: {功能描述}
- **涉及层级**: Rust Command + Service + Database + React Page

## Rust 后端

### Command 层
- **文件**: `src-tauri/src/commands/{module}.rs`
- **Command 数**: X 个
- **函数列表**:
  | 函数 | 描述 | 参数 | 返回值 |
  |------|------|------|--------|
  | get_items | 获取列表 | page: u32, size: u32 | Vec<Item> |

### Service 层
- **文件**: `src-tauri/src/services/{module}.rs`
- **职责**: 业务逻辑处理

### Database 层
- **文件**: `src-tauri/src/database/{module}.rs`
- **表名**: {table_name}
- **操作**: CRUD + 自定义查询

### Model
- **文件**: `src-tauri/src/models/{module}.rs`
- **结构体**: {StructName}
- **字段数**: X

## React 前端

### 页面
- **文件**: `src/pages/{module}.tsx`
- **路由**: `/{module}`
- **组件**: Ant Design Table + Form

### Store（Zustand）
- **文件**: `src/store/{module}.ts`
- **状态字段**: X 个
- **Action**: X 个

### API 封装
- **文件**: `src/lib/api/{module}.ts`
- **invoke 调用**: X 个
```

---

## 3. 数据库文档生成

### 3.1 扫描数据库 Schema

Tauri 项目使用 rusqlite 直接操作 SQLite：

```bash
# 扫描建表语句
Grep pattern: "CREATE TABLE" in src-tauri/src/database/*.rs
Grep pattern: "CREATE TABLE" in src-tauri/migrations/*.sql (如有)

# 扫描 Model 结构体定义
Grep pattern: "pub struct" in src-tauri/src/models/*.rs
```

### 3.2 提取表结构信息

对每个表/Model，提取：
- 表名（`CREATE TABLE` 或 struct 注释）
- 字段名、类型、约束
- 索引定义
- 外键关系

### 3.3 输出数据库文档

```markdown
# 数据库文档

## 概述
- **数据库**: SQLite（rusqlite）
- **存储位置**: 应用数据目录（`app_data_dir()`）
- **迁移方式**: 代码内嵌 SQL / 迁移文件

## 表清单

| 表名 | 模块 | 说明 | 字段数 |
|------|------|------|--------|
| settings | core | 应用设置 | X |
| items | business | 业务数据 | X |

---

## settings（应用设置表）

| 字段名 | 类型 | 可空 | 默认值 | 索引 | 注释 |
|--------|------|------|--------|------|------|
| id | INTEGER | NO | 自增 | PK | 主键 |
| key | TEXT | NO | - | UNIQUE | 设置键 |
| value | TEXT | YES | NULL | - | 设置值 |
| created_at | TEXT | NO | CURRENT_TIMESTAMP | - | 创建时间 |
| updated_at | TEXT | NO | CURRENT_TIMESTAMP | - | 更新时间 |

### Rust Model 对应

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub id: i64,
    pub key: String,
    pub value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

### TypeScript 类型对应

```typescript
interface Setting {
  id: number;
  key: string;
  value?: string;
  createdAt: string;
  updatedAt: string;
}
```
```

---

## 4. IPC 通信文档（Tauri 专属）

### 4.1 生成前后端通信映射

扫描所有 `invoke` 调用和 `#[tauri::command]` 定义，生成完整的 IPC 映射：

```markdown
## IPC 通信映射

| 前端调用 | Rust Command | 参数 | 返回值 | 页面 |
|---------|-------------|------|--------|------|
| invoke("get_items") | get_items() | { page, size } | Vec<Item> | ItemList.tsx |
| invoke("create_item") | create_item() | { name, desc } | Item | ItemForm.tsx |
| listen("item-updated") | emit("item-updated") | Item | - | ItemList.tsx |
```

### 4.2 事件通信文档

```bash
# 扫描 Rust 端 emit
Grep pattern: "emit\(" in src-tauri/src/**/*.rs
# 扫描前端 listen
Grep pattern: "listen\(" in src/**/*.ts src/**/*.tsx
```

---

## 执行命令

```
用户: 生成 Command 文档
-> 执行步骤 1，输出 Command 接口文档

用户: 生成数据库文档
-> 执行步骤 3，输出数据库表结构

用户: 生成全部文档
-> 依次执行步骤 1、2、3、4

用户: 生成 IPC 文档
-> 执行步骤 4，输出前后端通信映射

用户: 生成某模块文档
-> 只扫描该模块，执行步骤 2
```

---

## 注意事项

- Tauri Command 是最核心的接口文档来源（等同于传统 Web 项目的 API 文档）
- 数据库文档基于 Rust 代码中的 SQL 和 Model 定义生成，不直接连接 SQLite 数据库
- 生成的文档写入 `docs/` 目录，不覆盖用户手动编写的内容
- IPC 通信映射是 Tauri 项目独有的文档类型，帮助理解前后端数据流
- serde 的 `#[serde(rename_all = "camelCase")]` 等属性会影响前后端字段命名映射，文档中需标注
