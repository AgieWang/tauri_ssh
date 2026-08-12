# Tauri SSH 仓库导航参考

本页是 2026-08-01 当前树的速查，不是永久事实源。使用前必须通过 `rg --files`、模块导出和符号搜索验证目录及文件仍存在；不要把本页反向当成项目必须保持的结构。

## 前端

| 位置 | 常见职责 |
|---|---|
| `src/main.tsx` | React 挂载与全局初始化 |
| `src/App.tsx` | 根组件、Provider、主题和错误边界 |
| `src/Router.tsx` | HashRouter 路由配置 |
| `src/pages/` | 页面级功能入口 |
| `src/components/` | 布局和可复用 UI 组件 |
| `src/hooks/` | 可复用交互和 Command Hook |
| `src/store/` | 按领域拆分的 Zustand Store（当前含 `app.ts`、`settings.ts`、`knowledge.ts`）及 `index.ts` 聚合出口 |
| `src/lib/api/client.ts` | `invoke`/Dev API 基础客户端、`CommandError` 解析与 `getErrorCode/getErrorMessage` |
| `src/lib/api/<domain>.ts` | 按领域拆分的 API 封装；`index.ts` 只作为聚合出口之一 |
| `src/types/<domain>.ts` | 按领域拆分的 TypeScript DTO；`index.ts` 聚合公共导出 |
| `src/styles/`、`src/theme/` | CSS 令牌、全局样式和 Ant Design 主题 |

## Rust/Tauri 后端

| 位置 | 常见职责 |
|---|---|
| `src-tauri/src/lib.rs` | Builder、插件、State 和 Command 注册 |
| `src-tauri/src/commands/` | IPC 入口 |
| `src-tauri/src/services/` | 业务逻辑与系统能力编排 |
| `src-tauri/src/database/` | SQLite DAO 与迁移 |
| `src-tauri/src/models/` | Rust DTO 和领域模型 |
| `src-tauri/src/shared/` | 跨领域共享基础能力（当前含本地 API、时间工具等），通过 `shared/mod.rs` 导出 |
| `src-tauri/src/state.rs` | 应用共享状态 |
| `src-tauri/src/error.rs` | 内部 `AppError` 与 IPC `CommandError { code, message }` |
| `src-tauri/capabilities/` | Tauri 2 权限声明 |
| `src-tauri/tauri.conf.json` | 窗口、构建、bundle 和安全配置 |
| `src-tauri/Cargo.toml` | Rust 依赖与 feature |

## 功能链路检查表

```text
Router/Sidebar
  -> Page/Component
  -> Hook/Store
  -> src/lib/api/<domain>.ts
  -> src/lib/api/client.ts 基础客户端/错误解析
  -> invoke command name
  -> commands/mod.rs + lib.rs registration
  -> Command
  -> Service
  -> Database/System capability
  -> shared 基础设施（适用时）
  -> Model/DTO
  -> response and UI state
```

## 新功能常见影响面

- Model：Rust Model 与按域拆分的 TypeScript 类型，连同聚合出口。
- Database：DAO、schema 和 `PRAGMA user_version` 迁移。
- Service：业务规则。
- Command：IPC 入口、模块导出、handler 注册。
- API：按域 `invoke` 封装、`client.ts` 基础契约和聚合出口。
- UI：页面、路由、导航、状态与错误反馈。
- Security：插件注册和 Capabilities。
- Tests：Rust、Vitest 和真实浏览器路径。

## 追踪技巧

- 已知页面文字：搜索文案或 route path。
- 已知 API 方法：搜索方法名和 `invoke` 字符串。
- 已知 Rust Command：搜索函数名、`generate_handler!` 和 TS 调用。
- 已知表：搜索 DDL、DAO SQL、Model 字段和迁移。
- 已知配置键：搜索读取、默认值、设置页面和持久化位置。
- 已知日志：从日志文本反查发出位置，再向上下游扩展。
- 已知错误码：搜索 `CommandError` 映射、`getErrorCode` 分支与 UI 提示，不能只搜索错误文案。

## 防止结构图陈旧

每次导航至少执行与任务相关的动态检查：

```bash
rg --files src/lib/api src/store src/types \
  src-tauri/src/commands src-tauri/src/services \
  src-tauri/src/database src-tauri/src/models src-tauri/src/shared

rg -n 'pub mod|export \*|export \{|generate_handler!|CommandError' \
  src-tauri/src src/lib/api src/store src/types
```

若某领域已经拆出独立文件，优先以领域文件为定义源，再核对 `index.ts`/`mod.rs` 聚合导出；不要因为旧文档仍指向单文件就修改错误位置。
