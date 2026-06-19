# CLAUDE.md - Tauri Desktop App Framework

> 🔴 **子项目模式提示**：本项目是基于 [tauri 框架](https://gitcode.com/zhuawashi/tauri) 创建的子项目。
>
> - 本文件（CLAUDE.md）= 框架只读副本，不要直接改
> - 项目专属约束请写到 `.codex/PROJECT.md`
> - 项目专属技能请放到 `.codex/project-skills/`
> - 反哺到框架的候选请写到 `.codex/docs/experience/feedback-to-framework.md`

## 语言设置
**必须使用中文**与用户对话。

## 🔴 文件编码与注释规范（必须遵守）

### 1. 统一编码
- **所有源码与配置文件统一强制使用 UTF-8（无 BOM）**
- 包括但不限于：`.rs`、`.tsx`、`.ts`、`.js`、`.json`、`.toml`、`.html`、`.css`、`.md`
- **绝对禁止**：UTF-8 with BOM、GBK、GB2312、ANSI、ISO-8859-1 等混用
- 创建文件时必须强制使用 UTF-8（无 BOM）

### 2. 中文内容规范
- 中文注释、中文日志、中文文档必须可读，不允许乱码（如"鍥藉"）
- 发现乱码优先检查文件实际编码与 IDE 显示编码是否一致

### 3. 注释规范
- 保留已有业务注释，不随意删除历史说明和关键注释块
- 新增代码必须补充必要注释，说明"为什么这样做"，避免空泛注释

### 4. 变更原则
- 功能改动与注释改动尽量分开，便于回溯
- 修编码问题时不改业务逻辑，只做最小必要修改

---

## 术语约定
| 术语 | 含义 | 对应目录 |
|------|------|---------|
| **后端** | Rust Core（Tauri 后端进程） | `src-tauri/src/` |
| **前端** | React UI（WebView 进程） | `src/` |
| **配置** | Tauri 核心配置 | `src-tauri/tauri.conf.json` |
| **权限** | Capabilities 安全声明 | `src-tauri/capabilities/` |
| **Command** | Rust 侧可被前端调用的函数 | `#[tauri::command]` |
| **IPC** | 进程间通信（前端 ↔ Rust） | `invoke()` / `listen()` |

---

## 核心架构（必须牢记）

| 项目 | 规范 |
|------|------|
| **应用类型** | Tauri 2.x 桌面应用（双进程架构） |
| **后端语言** | Rust 2021 edition |
| **前端框架** | React 19 + TypeScript 5.8 |
| **UI 组件库** | Ant Design (v5+) + Lucide React 图标库 |
| **样式方案** | TailwindCSS 4 + CSS Variables 设计令牌 |
| **状态管理** | Zustand (v5+)（全局状态）+ React Hooks（局部状态） |
| **路由方案** | React Router 7（HashRouter） |
| **构建工具** | Vite 7 (前端) + Cargo (后端) |
| **通信机制** | Tauri IPC（`invoke` 调用 Rust Commands） |
| **序列化** | serde + serde_json（Rust ↔ JSON ↔ TypeScript） |
| **数据库** | SQLite（rusqlite，Rust 直接操作） |
| **错误处理** | thiserror + CommandError 结构化错误（Rust）+ ErrorBoundary（React） |
| **安全模型** | Capabilities 细粒度权限声明 |
| **应用标识** | `com.agilefr.tauri_ssh` |
| **可选移动端** | PWA-first（独立 SPA + axum 远程网关）/ 可加 Tauri Mobile 壳，详见下方"可选：移动端伴侣架构" |

### 双进程架构

```
┌───────────────────────────────────────────────────────┐
│                     Tauri 应用                         │
│                                                       │
│  ┌──────────────────┐  IPC (invoke)  ┌──────────────────┐
│  │   WebView 进程    │ ◄════════════► │   Rust Core 进程  │
│  │                  │                │                  │
│  │  React 19        │  Commands      │  commands/       │
│  │  Ant Design 5    │  Events        │  services/       │
│  │  TailwindCSS 4   │  ────────►     │  database/       │
│  │  Zustand         │                │  models/         │
│  │  React Router    │  ◄────────     │  error.rs        │
│  │                  │  返回值         │  state.rs        │
│  │  UI 渲染         │                │                  │
│  │  用户交互        │                │  系统 API        │
│  │  前端状态        │                │  文件操作        │
│  │                  │                │  SQLite 数据库   │
│  └──────────────────┘                └──────────────────┘
└───────────────────────────────────────────────────────┘
```

### 可选：移动端伴侣架构（PWA-first）

桌面端可作为"远程网关"对外服务移动端伴侣。两条可选路线：

| 路线 | 形态 | 适用场景 |
|------|------|---------|
| **PWA-first** | 移动端是纯浏览器 SPA，访问桌面端起的 axum HTTP/WS 服务 | 推荐默认；零安装、跨平台、上线快 |
| **Tauri Mobile 壳** | 在 PWA 之上再打包一层 Tauri Android/iOS 容器 | 需要原生能力（推送、生物识别、Intent 拦截） |

```
┌──────────────────────────────────────────────────┐
│  桌面端 (PC)                                      │
│  ┌──────────────┐  IPC  ┌──────────────────┐    │
│  │  WebView     │◄─────►│  Rust Core       │    │
│  │  (PC SPA)    │       │  + axum 远程网关 │    │
│  └──────────────┘       │   (端口可配)     │    │
│                         └────────┬─────────┘    │
└────────────────────────────────HTTP/WS──────────┘
                                  ↓
                  ┌───────────────────────────┐
                  │  移动端伴侣（手机/平板）   │
                  │  浏览器加载 dist-mobile   │
                  │  或 Tauri Mobile webview  │
                  └───────────────────────────┘
```

#### 关键目录

| 目录/文件 | 用途 | 仅 PWA | + Tauri Mobile |
|----------|------|:------:|:--------------:|
| `src/mobile/` | 移动端独立 React SPA（独立入口/路由/store） | ✅ | ✅ |
| `vite.mobile.config.ts` | 独立 vite 配置，`base: "./"` 兼容多托管 | ✅ | ✅ |
| `dist-mobile/` | 构建产物（`pnpm mobile:build`） | ✅ | ✅ |
| `src-tauri/src/remote/` | 桌面端 axum 远程网关（auth + handlers） | ✅ | ✅ |
| `mobile-tauri/` | 独立 Tauri Mobile 子项目（极简 webview 容器） | — | ✅ |

#### 设计哲学

- **业务逻辑全在桌面端**：移动端只是 webview / SPA，所有数据通过 HTTP / WebSocket 拉取。Tauri Mobile 壳的 `lib.rs` 只注册 `log/os/opener`，**不写业务 Command**。
- **构建产物复用**：`mobile-tauri/src-tauri/tauri.conf.json` 的 `frontendDist: "../../dist-mobile"`，PWA 和原生壳共用一份前端产物。
- **版本号独立**：移动端有独立 tag（`mobile-v*.*.*`），桌面端推 `v*.*.*`，CI 分流互不阻塞。
- **不内嵌网络隧道**：`frpc` / `easytier` 等二进制在国内多家杀软中被误报为木马。**改为引导用户自配反向代理**，桌面端只负责绑定本地端口。

#### 相关 skill

- `mobile-app-architecture` — 整体方案选型 + 双 vite 构建 + 目录骨架
- `remote-gateway` — axum 远程网关骨架 + Token 鉴权 + 失败追踪
- `tauri-mobile-android` — Android 打包专项（NDK 中文路径、`.cargo/config.toml` 强制 ASCII target-dir、minSdkVersion、versionCode 严格递增）
- `release-publish` 中的"双线发布（桌面 + 移动）"章节

### 后端三层架构

```
Commands 层（IPC 入口）→ Services 层（业务逻辑）→ Database 层（数据访问）
```

### 分层职责

| 层级 | 职责 | 关键技术 |
|------|------|---------|
| **WebView 层** | UI 渲染、用户交互 | React 19 + Ant Design + TailwindCSS |
| **状态管理层** | 全局状态、设置管理 | Zustand（`src/store/`） |
| **API 封装层** | 统一 invoke 调用 | `src/lib/api/index.ts` |
| **IPC 桥接层** | 前后端通信 | `invoke()` / `listen()` |
| **Command 层** | IPC 接口定义 | `#[tauri::command]`（`src-tauri/src/commands/`） |
| **Service 层** | 业务逻辑 | `src-tauri/src/services/` |
| **Database 层** | 数据访问 DAO | `src-tauri/src/database/`（rusqlite） |
| **Plugin 层** | 功能扩展 | `tauri::Builder.plugin()` 注册 |
| **Capabilities 层** | 安全权限控制 | JSON 声明式权限 |

---

## 目录结构

```
tauri/
├── index.html                    # HTML 入口（SPA 挂载点）
├── package.json                  # Node.js 依赖和脚本
├── tsconfig.json                 # TypeScript 配置（含 @/ 路径别名）
├── vite.config.ts                # Vite 构建配置（TailwindCSS + 路径别名）
│
├── src/                          # ★ 前端源码（React + TypeScript）
│   ├── main.tsx                  # 前端入口（ReactDOM.createRoot + 窗口显示兜底）
│   ├── App.tsx                   # 主组件（ConfigProvider + 主题 + ErrorBoundary）
│   ├── Router.tsx                # 路由配置（React Router）
│   ├── vite-env.d.ts             # Vite 类型声明
│   ├── theme/                    # 主题配置
│   │   └── antdTheme.ts          # Ant Design 暗色/亮色主题
│   ├── styles/                   # 样式系统
│   │   ├── variables.css         # CSS 设计令牌（颜色/间距/圆角）
│   │   └── global.css            # TailwindCSS + 全局样式
│   ├── store/                    # Zustand 全局状态（按职责拆分）
│   │   ├── app.ts                # UI 状态（主题/侧边栏）
│   │   ├── settings.ts           # 设置状态（语言/关闭行为）
│   │   └── index.ts              # Re-export Hub
│   ├── types/                    # TypeScript 类型定义（按模块拆分）
│   │   ├── config.ts             # 配置相关类型
│   │   ├── system.ts             # 系统信息类型
│   │   └── index.ts              # Re-export Hub
│   ├── hooks/
│   │   └── useCommand.ts         # useCommand Hook + safeInvoke 工具
│   ├── lib/
│   │   └── api/                  # API 调用封装（按模块拆分）
│   │       ├── client.ts         # 基础层（invoke + 结构化错误解析）
│   │       ├── system.ts         # 系统 API
│   │       ├── config.ts         # 配置 API
│   │       ├── updater.ts        # 更新 API
│   │       └── index.ts          # Re-export Hub
│   ├── components/
│   │   ├── ui/
│   │   │   └── ErrorBoundary.tsx  # 错误边界组件
│   │   └── layout/
│   │       ├── AppLayout.tsx      # 应用布局（Sider + Header + Content）
│   │       └── Sidebar.tsx        # 侧边栏导航
│   └── pages/
│       ├── home/
│       │   └── index.tsx          # 首页
│       ├── settings/
│       │   └── index.tsx          # 设置页（配置管理）
│       └── about/
│           └── index.tsx          # 关于页（系统信息）
│
├── src-tauri/                    # ★ Rust 后端（Tauri Core）
│   ├── Cargo.toml                # Rust 依赖配置
│   ├── build.rs                  # Tauri 构建脚本
│   ├── tauri.conf.json           # ★ Tauri 核心配置
│   ├── capabilities/             # ★ 权限声明
│   │   └── default.json
│   ├── icons/                    # 应用图标
│   └── src/
│       ├── main.rs               # Rust 进程入口
│       ├── lib.rs                # ★ 核心入口（Builder + 插件 + Command 注册）
│       ├── error.rs              # ★ 统一错误类型（AppError + CommandError 结构化错误）
│       ├── state.rs              # ★ 应用状态（AppState + Database）
│       ├── models/
│       │   └── mod.rs            # 数据模型（AppConfig / SystemInfo）
│       ├── database/
│       │   ├── mod.rs            # 数据库操作（Database struct + DAO）
│       │   └── schema.rs         # 表结构迁移（PRAGMA user_version）
│       ├── services/
│       │   ├── mod.rs            # 服务层入口
│       │   └── config.rs         # 配置业务逻辑
│       ├── shared/               # 公共工具层
│       │   ├── mod.rs            # 模块入口
│       │   └── time_utils.rs     # 时间工具函数
│       └── commands/
│           ├── mod.rs            # Command 模块入口
│           ├── system.rs         # 系统 Commands（greet / get_system_info）
│           └── config.rs         # 配置 Commands（CRUD）
│
├── public/                       # 静态资源
└── docs/                         # 项目文档
```

---

## 🔴 Skills 强制评估（必须遵守）

> **每次用户提问时，Hook 会注入技能评估提示。必须严格遵循！**

**流程**：
1. **评估**：根据注入的技能列表，列出匹配的技能及理由
2. **激活**：对每个匹配的技能调用 `Skill(技能名)`
3. **实现**：激活完成后开始实现

---

## 🔴 多会话并发自动避让协议（L1/L2/L3 三层触发）

> 用户可能同时开多个 Claude Code 会话操作同一仓库。本会话必须**自动感知并避让**其他会话的工作，**默认静默执行，不打扰用户**。
> 设计原则：宁可绕路，绝不覆盖；宁可静默放弃，绝不擅自 stash / reset / checkout。

### L1 — 启动时探测（首次响应前，仅执行一次）

```bash
git status -s
git branch --show-current
```

- 把"未提交文件清单"和"当前分支"记入会话上下文，整个会话复用，**不向用户复述**
- 若清单非空且与本次任务无关 → 视为"他者占用区"，本会话**不修改、不 stash、不 checkout、不 reset** 这些文件
- 若清单非空且与本次任务相关（用户接续之前的工作）→ 当作己方未完成工作正常处理

### L2 — 修改文件前（按需触发，单文件粒度）

修改任意已存在文件**之前**，执行：

```bash
git log -1 --format="%ar|%s" <file>
```

判定规则（严格按此执行，不询问）：

| 条件 | 处置 |
|------|------|
| 距今 ≥ 15 分钟 | ✅ 自由修改 |
| 距今 < 15 分钟 + 文件**不在** L1 未提交清单 | ✅ 自由修改（已提交的近期改动不冲突） |
| 距今 < 15 分钟 + 文件**在** L1 未提交清单 + 可绕开（新增功能/换路径） | ⚠️ **静默换路径绕开**，不告知用户 |
| 距今 < 15 分钟 + 文件**在** L1 未提交清单 + 必须改同文件 | 🛑 **此时唯一允许打扰用户一次**："`<file>` 15min 内有未提交改动，疑似其他会话占用，是否继续？" |

### L3 — 提交前（强校验，必做）

`git commit` 前：

```bash
git diff --cached --name-only
```

- 对照本会话明确改过的文件清单（自维护）
- 越界文件 → **静默 `git restore --staged <file>`**，仅提交本会话范围内文件
- 逐个 `git add <具体文件>`，**禁止** `git add -A` / `git add .`
- commit message 末尾可附 `[scope: <模块>]` 标识本次会话范围

### 跨会话操作禁令（不询问、直接禁止）

| 禁令 | 原因 |
|------|------|
| ❌ `git stash` / `git stash pop` | 会污染其他会话的工作区 |
| ❌ `git reset --hard` | 会丢其他会话的未提交改动 |
| ❌ `git checkout <file>`（丢弃改动） | 同上 |
| ❌ `git checkout <branch>`（切分支） | 除非用户明确指示 |
| ❌ `git add -A` / `git add .` | 可能误提交他者文件，必须逐个 add |
| ❌ `git clean -fd` | 会删他者未跟踪文件 |
| ❌ kill 端口 / `taskkill /F` 进程 | 他者 dev server / `tauri dev` 可能在用 |
| ❌ 删除其他会话的任务文档或 WIP 文件 | 同上 |

### 高并发场景升级 → git worktree

若用户明确"并行开发"或预计 30+ 分钟同时改**不同模块**（如同时改前端 + Rust 命令），主动建议：

```bash
claude --worktree feature-x
```

官方原生支持，自动隔离目录与分支。3-5 个并行最佳，5+ 会撞 API 速率限制。
> 注意：worktree **不能**隔离 vite dev server 端口、`tauri dev` 进程、`src-tauri/target/` 编译缓存（会重复编译 Rust）。同时跑 dev 仍需手动错开端口或只在一个 worktree 跑 dev。

---

## ⚠️ 开发强制要求

**开发前必须：先读参考代码 → 了解现有模式 → 按相同风格编写**

### 参考代码位置

| 开发类型 | 参考代码 |
|---------|---------|
| **Rust Command** | `src-tauri/src/commands/*.rs`（三层架构：Command → Service → Database） |
| **Rust 数据模型** | `src-tauri/src/models/mod.rs` |
| **Rust 错误处理** | `src-tauri/src/error.rs`（AppError + CommandError 结构化错误） |
| **Rust 服务层** | `src-tauri/src/services/*.rs` |
| **Rust 数据库层** | `src-tauri/src/database/mod.rs` |
| **前端页面组件** | `src/pages/*/index.tsx`（Ant Design + TailwindCSS） |
| **前端布局** | `src/components/layout/AppLayout.tsx` |
| **前端 API 封装** | `src/lib/api/index.ts`（invoke 调用封装） |
| **前端状态管理** | `src/store/index.ts`（Zustand store） |
| **前端类型定义** | `src/types/index.ts` |
| **主题设计令牌** | `src/styles/variables.css`（CSS 变量，双主题颜色/间距/阴影） |
| **Ant Design 主题** | `src/theme/antdTheme.ts`（darkTheme/lightTheme + getAntdTheme） |
| **Tauri 配置** | `src-tauri/tauri.conf.json` |
| **权限声明** | `src-tauri/capabilities/default.json` |

---

## 🔴 绝对禁止的写法

### Rust 后端

| 错误做法 | 正确做法 | 原因 |
|---------|---------|------|
| `unwrap()` 处理可能失败的操作 | `Result<T, CommandError>` + `?` 运算符 | `unwrap` 会导致 panic 崩溃 |
| Command 中 `panic!()` | 返回 `Err(AppError::...)` | panic 会崩溃整个应用 |
| 不加 `#[tauri::command]` 就期望前端调用 | 必须标记 `#[tauri::command]` 并在 `generate_handler!` 注册 | 否则前端 invoke 找不到 |
| 直接在 Command 中做长时间阻塞操作 | 使用 `async` Command 或 `tokio::spawn` | 阻塞会冻结 IPC 响应 |
| 不声明 Capabilities 就使用插件 API | 在 `capabilities/*.json` 中显式声明权限 | Tauri 2.x 强制权限检查 |
| 使用 `std::thread::sleep` 阻塞主线程 | 使用 `tokio::time::sleep` 异步等待 | 阻塞主线程冻结应用 |
| Command 直接操作数据库 | Command → Service → Database 三层 | 保持架构分层清晰 |

### TypeScript 前端

| 错误做法 | 正确做法 | 原因 |
|---------|---------|------|
| `fetch("http://...")` 直接请求外部 API | 通过 Rust Command 代理请求 | 安全限制 + 跨域问题 |
| 硬编码文件系统路径 `"C:\\Users\\..."` | 使用 Tauri path API（`appDataDir()` 等） | 跨平台路径不同 |
| 使用 `class` 组件 | 使用函数组件 + Hooks（ErrorBoundary 除外） | React 19 推荐模式 |
| `any` 类型 | 定义明确的 TypeScript 接口 | strict 模式要求 |
| `invoke` 不处理错误 | `try-catch` 包裹或使用 `safeInvoke` | Command 可能返回错误 |
| 直接 `import` Node.js 模块 | 使用 `@tauri-apps/api/*` 或 Rust Command | WebView 中无 Node.js |
| 裸写 `invoke()` 调用 | 封装到 `src/lib/api/` 中统一管理 | 便于维护和类型安全 |
| 硬编码颜色值 `#1a1a1c` | 使用 `var(--bg-primary)` 或 `token.colorBgLayout` | 主题切换时不会响应 |
| 使用 TailwindCSS `dark:` 前缀 | 使用 CSS 变量 `var(--xxx)` | 项目用 `data-theme` 而非 `dark` 类 |

---

## Tauri Command 开发规范（三层架构）

### 新增功能的标准流程

```
1. 在 models/ 定义数据结构（derive Serialize/Deserialize）
2. 在 database/ 实现 DAO 方法（SQL 操作）
3. 在 services/ 实现业务逻辑
4. 在 commands/ 实现 Command 入口（调用 Service）
5. 在 lib.rs 的 generate_handler![] 注册
6. 在 src/types/ 定义对应 TypeScript 接口
7. 在 src/lib/api/ 封装 invoke 调用
8. 在 src/pages/ 实现 UI 页面
9. 更新 capabilities（如使用新插件）
```

### Rust 三层架构示例

```rust
// ─── models/mod.rs ───
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub key: String,
    pub value: String,
}

// ─── database/mod.rs ───
impl Database {
    pub fn get_all_config(&self) -> Result<Vec<AppConfig>, AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        // SQL 查询...
    }
}

// ─── services/config.rs ───
pub struct ConfigService;
impl ConfigService {
    pub fn get_all(db: &Database) -> Result<Vec<AppConfig>, AppError> {
        db.get_all_config()
    }
}

// ─── commands/config.rs ───
#[tauri::command]
pub fn get_all_config(state: tauri::State<'_, AppState>) -> Result<Vec<AppConfig>, CommandError> {
    services::config::ConfigService::get_all(&state.db).map_err(|e| e.into())
}
```

### TypeScript 侧调用

```typescript
// ─── src/lib/api/index.ts ───
export const configApi = {
  getAll: () => invoke<AppConfig[]>("get_all_config"),
  get: (key: string) => invoke<string | null>("get_config", { key }),
  set: (key: string, value: string) => invoke<void>("set_config", { key, value }),
};

// ─── src/pages/settings/index.tsx ───
const data = await configApi.getAll();
```

### Command 命名规范

| 维度 | 规范 | 示例 |
|------|------|------|
| Rust 函数名 | snake_case | `fn get_all_config()` |
| invoke 调用名 | 与 Rust 函数名一致 | `invoke("get_all_config")` |
| 参数名 | Rust: snake_case, TS: camelCase | Tauri 自动转换 |
| 返回类型 | `Result<T, CommandError>` | `-> Result<Vec<AppConfig>, CommandError>` |

---

## 前端核心规范 (src/)

### 技术栈

| 技术 | 用途 | 导入方式 |
|------|------|---------|
| **Ant Design 5** | UI 组件库（Button/Table/Card/Form 等） | `import { Button } from "antd"` |
| **Ant Design Icons** | 图标 | `import { SettingOutlined } from "@ant-design/icons"` |
| **Lucide React** | 补充图标 | `import { Home } from "lucide-react"` |
| **TailwindCSS 4** | 原子化样式 | `className="flex items-center gap-2"` |
| **Zustand** | 全局状态管理 | `import { useAppStore } from "@/store"` |
| **React Router** | 路由导航 | `import { useNavigate } from "react-router-dom"` |

### 组件开发模式

```tsx
// 使用 Ant Design + TailwindCSS + invoke 封装
import { Card, Table, message } from "antd";
import { configApi } from "@/lib/api";

export default function SettingsPage() {
  const [data, setData] = useState<AppConfig[]>([]);
  const [loading, setLoading] = useState(false);

  async function loadData() {
    setLoading(true);
    try {
      const configs = await configApi.getAll();
      setData(configs);
    } catch (e) {
      message.error(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { loadData(); }, []);

  return (
    <div className="max-w-2xl mx-auto">
      <Card title="配置列表">
        <Table dataSource={data} loading={loading} rowKey="key" />
      </Card>
    </div>
  );
}
```

### 状态管理

| 场景 | 方案 | 示例 |
|------|------|------|
| 组件内状态 | `useState` | `const [count, setCount] = useState(0)` |
| 全局 UI 状态（主题/侧边栏） | Zustand | `useAppStore((s) => s.theme)` |
| 后端持久数据 | Rust SQLite + Command | 通过 `configApi.getAll()` 获取 |
| 键值持久化（轻量设置） | tauri-plugin-store | `Store.load("settings.json")` |

### 主题系统

项目采用 **CSS 变量 + Ant Design 主题 + data-theme 属性** 三层主题架构：

| 层级 | 文件 | 职责 |
|------|------|------|
| CSS 变量 | `src/styles/variables.css` | 设计令牌（颜色/间距/阴影/圆角/字体），通过 `:root[data-theme]` 切换 |
| Ant Design | `src/theme/antdTheme.ts` | 组件库主题（`darkTheme`/`lightTheme`），与 CSS 变量同步 |
| 状态管理 | `src/store/app.ts` | 三态切换（dark/light/system），Zustand 管理 |
| 应用入口 | `src/App.tsx` | `data-theme` 属性写入 + `ConfigProvider` 主题选择 |

**样式选择规则**：
- Ant Design 组件内部 → 使用 `token.*`（通过 `useToken()`）
- 自定义组件 → 使用 `var(--xxx)` CSS 变量
- 布局/间距 → TailwindCSS 原子类
- 禁止硬编码颜色值，禁止使用 TailwindCSS `dark:` 前缀

### 路径别名

所有前端导入使用 `@/` 别名：
```typescript
import { useAppStore } from "@/store";
import { configApi } from "@/lib/api";
import type { AppConfig } from "@/types";
```

---

## Capabilities 权限配置

### 当前已声明权限

```json
{
  "permissions": [
    "core:default",
    "core:window:default",
    "core:window:allow-minimize",
    "core:window:allow-maximize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-close",
    "core:window:allow-hide",
    "core:window:allow-start-dragging",
    "core:menu:default",
    "core:tray:default",
    "opener:default",
    "store:default",
    "log:default",
    "updater:default",
    "process:default"
  ]
}
```

### 常用权限列表

| 插件 | 权限 | 说明 |
|------|------|------|
| core | `core:default` | 核心默认权限 |
| opener | `opener:default` | 打开 URL/文件 |
| store | `store:default` | 键值存储 |
| log | `log:default` | 日志系统 |
| fs | `fs:default` | 文件系统基础 |
| dialog | `dialog:default` | 文件选择对话框 |
| notification | `notification:default` | 系统通知 |
| sql | `sql:default` | 数据库操作 |

> **重要**: 每个使用的插件 API 都必须在 capabilities 中声明权限，否则运行时会报错。

---

## Rust 编码规范

### 错误处理（结构化 CommandError）

```rust
use crate::error::CommandError;

// ✅ Command 返回 CommandError（结构化错误，前端可按 code 判断）
#[tauri::command]
pub fn read_config(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<String, CommandError> {
    services::config::ConfigService::get(&state.db, &key)
        .map_err(|e| e.into())  // AppError -> CommandError 自动转换
}

// 前端解析结构化错误：
// import { getErrorCode, getErrorMessage } from "@/lib/api";
// try { ... } catch (e) {
//   if (getErrorCode(e) === "NOT_FOUND") { /* 特定处理 */ }
//   message.error(getErrorMessage(e));
// }
```

### 数据库操作（rusqlite）

```rust
// 所有 SQL 操作在 database/ 层
impl Database {
    pub fn get_config(&self, key: &str) -> Result<Option<String>, AppError> {
        let conn = self.conn.lock().map_err(|e| AppError::Custom(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT value FROM app_config WHERE key = ?1")?;
        let result = stmt.query_row(params![key], |row| row.get(0)).optional()?;
        Ok(result)
    }
}
```

### Schema 迁移

使用 `PRAGMA user_version` 管理数据库版本：

```rust
// database/schema.rs
pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 1 {
        // 创建表...
        conn.pragma_update(None, "user_version", 1)?;
    }
    // 后续版本迁移...
}
```

---

## 常见错误速查

### Rust 后端

| 错误写法 | 正确写法 |
|---------|---------|
| `state.lock().unwrap()` | `state.lock().map_err(\|e\| AppError::Custom(e.to_string()))?` |
| Command 直接写 SQL | Command → Service → Database 三层 |
| 忘记在 `generate_handler![]` 注册 | 每个新 Command 必须注册 |
| 返回 `String` 而非 `Result` | 返回 `Result<T, CommandError>` |

### TypeScript 前端

| 错误写法 | 正确写法 |
|---------|---------|
| `invoke("getUser")` | `invoke("get_user")`（snake_case） |
| 裸写 `invoke()` | 封装到 `src/lib/api/` |
| 不用 Ant Design 组件 | 优先使用 antd 组件（Table/Card/Form 等） |
| 不用 `@/` 别名 | `import { X } from "@/types"` |

---

## 构建与运行

### 常用命令

```bash
# 开发模式（前端 HMR + Rust 热编译）
pnpm tauri dev

# 生产构建（生成安装包）
pnpm tauri build

# 仅构建前端
pnpm build

# TypeScript 类型检查
npx tsc --noEmit

# Rust 代码检查
cd src-tauri && cargo clippy

# Rust 编译检查
cd src-tauri && cargo check

# Rust 测试
cd src-tauri && cargo test
```

### 开发服务器

| 项目 | 值 |
|------|-----|
| **前端开发地址** | `http://localhost:1422` |
| **MCP chrome-devtools** | 使用 `http://localhost:1422` 访问应用页面 |

> **注意**：使用 chrome-devtools MCP 工具时，`navigate_page` / `new_page` 等操作的 URL 应指向 `http://localhost:1422`（Tauri 开发模式下的 Vite 前端服务端口）。

### 编译优化配置

```toml
# Cargo.toml 已配置：

# 开发模式 - 依赖包 O2 优化（加速运行不影响本地编译）
[profile.dev.package."*"]
opt-level = 2

# 发布模式 - 最小体积 + LTO + 符号剥离
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

### 数据库优化配置

- **WAL 模式**：提升并发读性能
- **busy_timeout(5s)**：防止并发写入死锁
- **软删除**：`deleted_at` 字段，支持恢复

### 当前已安装的 Rust 依赖

| Crate | 版本 | 用途 |
|-------|------|------|
| `tauri` | 2.x | Tauri 核心 |
| `tauri-plugin-opener` | 2 | 打开 URL/文件 |
| `tauri-plugin-store` | 2 | 键值存储 |
| `tauri-plugin-log` | 2 | 日志系统 |
| `thiserror` | 2 | 错误类型派生 |
| `rusqlite` | 0.31 (bundled) | SQLite 数据库 |
| `serde` / `serde_json` | 1 | JSON 序列化 |
| `log` | 0.4 | 日志门面 |
| `chrono` | 0.4 (serde) | 日期时间 |

### 当前已安装的前端依赖

| 包 | 用途 |
|----|------|
| `antd` | Ant Design UI 组件库 |
| `@ant-design/icons` | Ant Design 图标 |
| `react-router-dom` | 路由 |
| `zustand` | 状态管理 |
| `lucide-react` | 图标补充 |
| `tailwindcss` + `@tailwindcss/vite` | 原子化 CSS |
| `@tauri-apps/plugin-store` | 键值存储（前端 SDK） |
| `@tauri-apps/plugin-log` | 日志（前端 SDK） |

---

## 快速命令

| 命令 | 用途 |
|------|------|
| `/dev` | 开发新功能（三层架构全栈代码生成） |
| `/command` | 快速创建 Tauri Command |
| `/check` | 代码规范检查（Rust + TypeScript） |
| `/start` | 项目快速了解 |
| `/progress` | 项目进度报告 |
| `/next` | 下一步建议 |
| `/release` | 发布新版本（CI 全自动构建 + 推送） |
| `/update-docs` | 文档站点管理（VitePress 初始化 / 增量更新 / 全量重建） |

---

## Tauri 核心类型速查

| 类型 | 用途 | 使用场景 |
|------|------|---------|
| `tauri::Builder` | 应用构建器 | 注册插件、Commands、状态、事件 |
| `tauri::AppHandle` | 应用句柄 | 在 Command 中访问应用实例 |
| `tauri::Window` | 窗口句柄 | 操作窗口（大小/位置/标题） |
| `tauri::State<T>` | 全局状态 | Command 中注入共享状态 |
| `tauri::Manager` | 管理 trait | 获取窗口、发送事件 |
| `tauri::Emitter` | 事件发送 trait | 向前端发送事件 |
| `tauri::Listener` | 事件监听 trait | 监听前端事件 |

---

## 🔴 开发前检查清单

- [ ] **已读参考代码** — `src-tauri/src/commands/*.rs` 和 `src/pages/*/index.tsx`
- [ ] **遵循三层架构** — Command → Service → Database
- [ ] **已了解双进程架构** — 前端（WebView）和后端（Rust）通过 IPC 通信
- [ ] **使用 Ant Design** — UI 组件优先使用 antd
- [ ] **使用 TailwindCSS** — 布局样式使用 Tailwind 类
- [ ] **API 统一封装** — invoke 调用封装到 `src/lib/api/`
- [ ] **类型对齐** — Rust struct 和 TypeScript interface 保持一致
- [ ] **已确认 Capabilities** — 使用的插件 API 都已在 capabilities 中声明
- [ ] **错误处理正确** — Rust 用 `AppError`/`CommandError`/`Result<T, CommandError>`，前端用 `try-catch` + `getErrorMessage()`
- [ ] **不违反禁止项** — 检查上方禁止表格
