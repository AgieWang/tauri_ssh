---
name: store-management
description: |
  用于设计 Zustand 全局状态、React 共享状态、Rust AppState 或 tauri-plugin-store 偏好持久化。

  触发场景：
  - 新增或重构 Zustand store、selector、action 或跨组件共享状态
  - 设计 tauri::State<AppState> 的进程内共享资源
  - 将用户偏好通过 tauri-plugin-store 持久化并恢复
  - 判断组件局部状态、全局状态、Rust 状态和 SQLite 的职责边界

  不应触发：查询业务状态字段、修改流程状态码、数据库记录持久化、普通 useState 局部改动。

  触发词：Zustand、useAppStore、React 全局状态、共享状态架构、tauri::State、AppState、tauri-plugin-store
---

# Tauri 状态管理

## 适用边界

本 Skill 处理应用运行时状态和轻量偏好，不处理业务表中的 `status/state` 字段。仅出现“状态、持久化、store”不足以触发，必须能识别到 Zustand、React 跨组件共享、Rust `AppState` 或 `tauri-plugin-store` 语义。

## 先做状态归属决策

| 数据性质 | 推荐位置 |
|---|---|
| 单组件、短生命周期 UI | `useState` / `useReducer` |
| 跨页面 UI 或会话态 | Zustand，按领域拆分 `src/store/` |
| Rust 进程共享资源 | `tauri::State<AppState>` |
| 少量用户偏好 | `tauri-plugin-store` + Zustand 运行时镜像 |
| 大量结构化业务数据 | Rust Service + SQLite，不放 Zustand/plugin-store |
| 服务端权威状态 | API/后端为真相源，前端只缓存展示态 |

不能仅为减少 props 就创建全局状态；先确认所有者、生命周期、真相源和恢复策略。

## Zustand 强制规则

1. Store 按领域拆分，从 `@/store` 统一导出；不要形成一个巨型 store。
2. 组件使用 selector 订阅最小切片，避免订阅整个 store 导致无关重渲染。
3. state 与 action 使用明确 TypeScript 类型，禁止 `any`。
4. setter 只负责写值或纯状态转换；路由跳转、请求、刷新、通知等副作用放在调用处或专用 action 中并显式命名。
5. 异步 action 要处理竞态、过期响应和错误；不能静默覆盖较新的状态。
6. 详情与代码模式见 [zustand-and-app-state.md](references/zustand-and-app-state.md)。

## Rust AppState 强制规则

- 注册与注入沿用项目已有 `AppState`；Command 只借用状态并调用 Service。
- `Mutex`/`RwLock` 中不执行长耗时或 `.await` 操作；尽快复制所需值后释放锁。
- 锁中毒和并发错误返回 `AppError`/`CommandError`，禁止 `unwrap()` 或 `panic!()`。
- 数据库连接、凭据句柄和运行中任务等共享资源要有清晰所有权与关闭策略。

## 偏好持久化强制规则

- `tauri-plugin-store` 只用于轻量非敏感偏好；结构化业务数据使用 SQLite，凭据使用安全凭据设施。
- 启动恢复必须校验类型、枚举范围和版本；坏数据使用显式默认值并记录可诊断信息。
- 运行时状态更新与磁盘保存失败要区分，避免 UI 显示“已保存”但实际未落盘。
- 插件注册、Capabilities 和主题恢复模式见 [persistence-and-theme.md](references/persistence-and-theme.md)。

## 不应触发示例

- “查询 Jenkins 构建状态为 failed 的记录”——属于业务查询。
- “把工单状态从 1 改成 2”——属于业务逻辑/数据库任务。
- “这个按钮需要一个 loading”——局部 `useState` 足够，通常无需本 Skill。
- “新增 SQLite 表保存历史记录”——使用 `database-ops`。

## 与相关 Skill 的组合

- Zustand 调用 Rust IPC 获取权威数据：本 Skill 负责缓存/展示态，`api-development` 负责契约。
- 偏好使用 plugin-store：本 Skill 负责数据生命周期，`tauri-plugins`/`tauri-capabilities` 负责插件和权限细节。
- 主题状态：本 Skill 负责状态和恢复，`theme-system` 负责设计令牌与组件主题。
- 服务端或 SQLite 数据：后端仍是真相源，不把 Zustand 当离线数据库。

## 最小验证矩阵

- 首次启动、已有有效偏好、损坏偏好和保存失败。
- 两个组件同时订阅/更新时不存在丢失更新。
- 切页、刷新、重启和外部事件不会造成状态循环。

## 完成条件

- 状态所有者、真相源、生命周期和持久化边界明确。
- selector、action 和并发更新不会制造隐藏副作用或状态漂移。
- plugin-store 数据经过类型校验，权限与插件注册完整。
- 前端格式化、类型检查和聚焦测试通过；页面行为变更已用内置浏览器或 Chrome 验证。
- UTF-8 无 BOM，`git diff --check` 通过。
