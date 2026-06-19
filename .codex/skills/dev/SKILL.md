---
name: dev
description: |
  开发新功能的全栈代码生成器，自动生成 Rust 三层架构后端 + React 前端 UI + Tauri 权限配置的完整功能模块。

  触发场景：
  - 需要开发一个完整的新功能模块（含后端+前端+UI页面）
  - 需要创建带有完整 CRUD 操作的业务模块
  - 需要全栈代码生成（Rust Command + Service + Database + React 页面）

  触发词：开发、dev、新功能、功能开发、全栈、模块开发、页面开发
---

作为新功能开发助手,引导完成 Tauri 桌面应用的全栈功能开发。

## 核心优势

- 全栈自动生成(Rust 三层架构 + React UI + Capabilities 权限声明)
- 遵循 Tauri 双进程架构(WebView 进程 + Rust Core 进程)
- 类型安全(Rust serde 序列化 + TypeScript 类型对齐)
- 自动处理 Capabilities 权限声明(Tauri 2.x 强制权限模型)
- 错误处理规范(Rust `AppError` enum + 前端统一 API 包装)
- 三层分离(Commands -> Services -> Database)

---

## 执行流程

### 第一步:询问需求

使用 AskUserQuestion 工具询问:

**问题1:功能信息**
```
请告诉我要开发的功能:
1. **功能名称**?(如:用户管理、数据导入、系统监控、设置页面)
2. **需要哪些系统能力**?(选择适用项)
   - 文件读写(fs 插件)
   - 网络请求(Rust reqwest)
   - 本地数据库(rusqlite - 已集成)
   - 系统通知(notification 插件)
   - 剪贴板(clipboard 插件)
   - 对话框(dialog 插件 -- 文件选择/保存/确认框)
   - 系统托盘(tray 插件)
   - 全局快捷键(global-shortcut 插件)
   - 窗口操作(多窗口/窗口控制)
   - Shell 命令执行(shell 插件)
   - 自动更新(updater 插件)
   - 无特殊系统能力(纯前端 UI + 基础 Command)
3. **是否需要持久化数据**?(SQLite 数据库 / Rust 侧 State 管理 / 无)
```

**自动推断配置**:
- 文件操作 -> 需要 `fs` 插件 + `dialog` 插件 + 对应 Capabilities
- 网络请求 -> 通过 Rust Command 代理(禁止前端直接 fetch 外部 API)
- 数据库 -> `rusqlite` 已集成,创建 Service + Database 层
- 系统通知 -> `tauri-plugin-notification` + Capabilities 声明
- 持久状态 -> `tauri::State<T>` + `Mutex`/`RwLock` 包裹

---

### 第二步:检查功能是否已存在(强制执行)

```bash
# 检查 Rust Command 是否已有相关功能
Grep pattern: "fn {功能相关关键词}" path: src-tauri/src/commands/ output_mode: files_with_matches

# 检查 Rust Service 是否已有相关功能
Grep pattern: "fn {功能相关关键词}" path: src-tauri/src/services/ output_mode: files_with_matches

# 检查前端 API 是否已有相关功能
Grep pattern: "{功能名相关关键词}" path: src/lib/api/ output_mode: files_with_matches

# 检查前端页面是否已有相关功能
Grep pattern: "{功能名相关关键词}" path: src/pages/ output_mode: files_with_matches
```

**如果功能已存在** -> 停止全栈生成流程,建议增强现有代码(列出现有文件和扩展建议)
**如果功能未实现** -> 继续

---

### 第三步:读取参考代码(强制执行)

```bash
# Rust 后端参考 -- 了解三层架构模式
Read src-tauri/src/commands/user.rs      # Command 层示例
Read src-tauri/src/services/user.rs      # Service 层示例
Read src-tauri/src/database/mod.rs       # Database 层示例
Read src-tauri/src/error.rs              # 统一错误处理

# Rust 主入口 -- 了解 Builder 配置和 Command 注册
Read src-tauri/src/main.rs
Read src-tauri/src/lib.rs

# 前端参考 -- 了解 API 封装和组件结构
Read src/lib/api/index.ts                # API 封装层
Read src/types/index.ts                  # 类型定义
Read src/pages/Users/index.tsx           # 页面组件示例

# 权限声明参考 -- 了解已声明的 Capabilities
Read src-tauri/capabilities/default.json

# Tauri 配置参考 -- 了解应用配置(窗口/安全/构建)
Read src-tauri/tauri.conf.json

# Rust 依赖参考 -- 了解已安装的 crate
Read src-tauri/Cargo.toml
```

**项目已有清晰的模块化结构**:
- `src-tauri/src/commands/` -- Command 层(IPC 入口)
- `src-tauri/src/services/` -- Service 层(业务逻辑)
- `src-tauri/src/database/` -- Database 层(数据持久化)
- `src/pages/` -- 前端页面组件
- `src/lib/api/` -- 前端 API 封装

---

### 第四步:设计数据结构

定义 Rust 结构体和对应的 TypeScript 类型,确保两端类型对齐:

**Rust 侧(serde 自动序列化/反序列化)**:
```rust
use serde::{Deserialize, Serialize};

/// 功能数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XxxData {
    pub id: i64,
    pub name: String,
    pub status: i32,
    pub created_at: String,
}

/// 功能创建请求(如需独立入参类型)
#[derive(Debug, Deserialize)]
pub struct CreateXxxRequest {
    pub name: String,
    pub status: Option<i32>,
}

/// 功能查询请求(如需分页/过滤)
#[derive(Debug, Deserialize)]
pub struct QueryXxxRequest {
    pub keyword: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 功能响应结构(如需分页等封装)
#[derive(Debug, Serialize)]
pub struct XxxListResponse {
    pub items: Vec<XxxData>,
    pub total: usize,
}
```

**TypeScript 侧(与 Rust 类型一一对应)**:
```typescript
// src/types/index.ts 中添加

// 功能数据类型
export interface XxxData {
  id: number;
  name: string;
  status: number;
  createdAt: string;  // Tauri 自动 snake_case -> camelCase
}

// 功能创建请求类型
export interface CreateXxxRequest {
  name: string;
  status?: number;
}

// 功能查询请求类型
export interface QueryXxxRequest {
  keyword?: string;
  page?: number;
  pageSize?: number;
}

// 功能响应类型
export interface XxxListResponse {
  items: XxxData[];
  total: number;
}
```

**类型对齐规则**:
| Rust 类型 | TypeScript 类型 | 说明 |
|-----------|----------------|------|
| `String` / `&str` | `string` | 字符串 |
| `i32` / `i64` / `u32` / `u64` | `number` | 数字 |
| `f64` / `f32` | `number` | 浮点数 |
| `bool` | `boolean` | 布尔 |
| `Vec<T>` | `T[]` | 数组 |
| `Option<T>` | `T \| undefined` 或 `T?` | 可选值 |
| `HashMap<K, V>` | `Record<K, V>` | 映射 |
| `()` | `void` | 空返回 |

---

### 第五步:输出生成方案并确认

```markdown
## 代码生成方案

### 功能概述
- **功能名称**:{功能名}
- **系统能力**:{需要的插件/API 列表}
- **持久化方案**:{SQLite / State / 无}

### 文件清单

**Rust 后端(三层架构)**:
1. `src-tauri/src/commands/xxx.rs` -- Command 层(IPC 入口,参数验证)
2. `src-tauri/src/services/xxx.rs` -- Service 层(业务逻辑)
3. `src-tauri/src/database/xxx.rs` -- Database 层(数据持久化,如需数据库)
4. `src-tauri/src/database/mod.rs` -- 注册新的 Database 模块
5. `src-tauri/src/lib.rs` -- 在 generate_handler![] 中注册新 Command

**React 前端**:
6. `src/types/index.ts` -- 添加 TypeScript 类型定义
7. `src/lib/api/index.ts` -- 添加 API 封装函数
8. `src/pages/Xxx/index.tsx` -- React 页面组件(Ant Design 5 组件)
9. `src/store/xxxStore.ts` -- Zustand 状态管理(如需全局状态)

**权限配置**:
10. `src-tauri/capabilities/default.json` -- 添加所需插件权限

**依赖更新(如需新插件)**:
11. `src-tauri/Cargo.toml` -- 添加 Rust 依赖
12. `package.json` -- 添加 @tauri-apps/plugin-* 前端绑定

确认开始生成?
```

> **注意**:三层架构是强制规范:
> - **Command 层**:仅处理 IPC 调用、参数验证、错误转换
> - **Service 层**:业务逻辑、跨模块调用、事务处理
> - **Database 层**:SQL 执行、数据映射、连接池管理

---

### 第六步:自动生成代码

按三层架构生成完整的 Rust 后端代码（Commands + Services + Database）、React 前端代码（Types + API + Store + Page）、以及 Capabilities 权限配置。

---

### 第七步:完成报告

```markdown
## 代码生成完成

### 已完成
- Rust 三层架构实现(Commands -> Services -> Database)
- Rust 数据结构定义(Serialize/Deserialize)
- Rust 统一错误处理(AppError enum)
- TypeScript 类型定义(与 Rust 对齐)
- 前端 API 封装(统一 invoke 调用)
- Zustand 状态管理(如需全局状态)
- React 页面组件(Ant Design 5 + TailwindCSS 4)
- Capabilities 权限声明更新

### 后续操作
- **重新运行** `pnpm tauri dev` 使 Rust 代码变更生效
- **如添加了新插件**,需确认 `cargo add` 和 `pnpm add` 已执行
- **如添加了新窗口**,需在 `tauri.conf.json` 的 `app.windows` 中配置
- **如需添加路由**,在前端路由配置中添加页面路由
- 推荐运行 `/check` 检查代码规范
- 推荐运行 `cd src-tauri && cargo clippy` 检查 Rust 代码质量
```

---

## AI 强制执行规则

### 流程控制
1. **仅在第五步确认一次,其他步骤自动执行**
2. **第二步必须检查功能是否存在**(Grep 搜索 Commands/Services/API/Pages)
3. **第三步必须读参考代码**(commands/user.rs / services/user.rs / database/mod.rs / lib/api/index.ts / pages/Users/index.tsx)
4. **禁止多次询问用户确认**(确认后直接生成全部代码)

### Rust 后端规范(三层架构)
5. **必须严格遵循三层架构**: Command 层 / Service 层 / Database 层
6. **Command 必须返回 `Result<T, AppError>`**(统一错误处理)
7. **Rust 结构体必须 `#[derive(Debug, Serialize, Deserialize)]`**(serde 序列化必备)
8. **新 Command 必须在 `generate_handler![]` 中注册**(否则前端 invoke 找不到)
9. **禁止在 Command/Service 中直接调用数据库**(必须通过 Database 层)
10. **Database 层函数必须是纯数据操作**(不含业务逻辑)
11. **错误处理必须使用 AppError**(不使用 String,使用 thiserror 定义的枚举)
12. **禁止 `unwrap()` 处理可能失败的操作**(用 `?` 运算符)
13. **禁止在 Command 中 `panic!()`**(会导致应用崩溃)
14. **异步函数使用 `async fn`**(数据库操作、网络请求等)
15. **使用的插件必须在 Builder 中通过 `.plugin()` 注册**

### TypeScript 前端规范
16. **所有 invoke 调用必须封装在 `src/lib/api/index.ts` 中**
17. **所有类型必须定义在 `src/types/index.ts` 中**
18. **全局状态使用 Zustand**(不使用 Context API)
19. **路径导入必须使用 `@/` 别名**
20. **UI 组件使用 Ant Design 5**
21. **样式使用 TailwindCSS 4 类名**
22. **使用函数组件 + Hooks**(React 19 推荐模式,禁止 class 组件)
23. **禁止在前端直接访问文件系统**(通过 Tauri FS API 或 Rust Command)
24. **禁止前端直接 fetch 外部 API**(通过 Rust Command 代理请求)
25. **禁止使用 `any` 类型**
26. **API 函数必须有错误处理**
27. **invoke 命令名使用 snake_case**

### 权限配置规范
28. **使用的插件 API 必须在 Capabilities 中声明权限**
29. **新插件既要 Rust 侧 `cargo add` 也要前端侧 `pnpm add`**
30. **禁止在 Capabilities 中声明未使用的权限**

### 桌面应用特有规范
31. **禁止涉及 REST API 路由注册**(Tauri 是桌面应用,不是 Web 服务器)
32. **禁止涉及复杂的数据库迁移脚本**(建表在 init_database 中)
33. **禁止涉及多租户设计**(桌面应用是单用户本地应用)
34. **禁止涉及菜单 SQL 初始化**(桌面应用无后台管理菜单系统)
35. **禁止涉及 RESTful 路径设计**(通信走 IPC invoke,不是 HTTP)
36. **跨平台路径必须使用 Tauri path API**

### 代码质量规范
37. **SQL 语句必须使用参数化查询**
38. **数据库连接必须通过 `get_connection()` 获取**
39. **新数据表必须在 `init_database()` 中建表**
40. **前端组件必须处理 loading 和 error 状态**
41. **删除操作必须有确认对话框**
42. **表单必须有验证规则**
43. **成功/失败操作必须有提示**
