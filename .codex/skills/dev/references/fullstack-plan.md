# `/dev` 全栈计划与文件矩阵

## 目录

1. 需求输入
2. 数据与能力决策
3. 文件矩阵
4. 实现顺序
5. 范围控制

## 1. 需求输入

从用户描述与现有代码获取：

- 核心用户流程和可观察完成标准；
- 页面入口、表单/表格/详情/弹窗和状态流；
- Command 列表及每个入参、返回、错误；
- 数据来源：SQLite、Rust State、插件 Store、文件、网络或组合；
- 系统能力：文件、网络、通知、对话框、托盘、快捷键、窗口、shell、updater；
- 安全边界：凭据、外部 URL、路径、命令、远程服务和删除；
- 兼容性：旧数据库升级、现有数据、桌面/移动或 dev API 降级路径。

无需逐题询问；从仓库可验证的内容直接读取。只有多个选择会产生不同数据模型、权限或不可逆行为时请求方向。

## 2. 数据与能力决策

| 需求 | 默认归属 | 条件 |
|---|---|---|
| 局部表单/loading | React `useState` | 不跨页面共享 |
| 跨页面 UI 状态 | Zustand | 有明确共享消费者 |
| 轻量设置 | 现有 store 插件模式 | 读取现有实现和权限 |
| 关系/可查询数据 | Rust SQLite | 使用迁移、Service、Database |
| 缓存/运行时句柄 | Rust AppState | 线程安全且生命周期明确 |
| 外部 HTTP | Rust Service/Command | allowlist、超时、错误映射 |
| 文件导入导出 | Rust/Tauri 插件 | dialog/fs scope 与路径安全 |
| 原生通知 | notification 插件 | 页面 message 不算系统通知 |
| 实时进度 | event/Channel | task id、清理、取消、并发 |

## 3. 文件矩阵

仅在条件满足时修改：

### Rust

| 文件 | 作用 | 条件 |
|---|---|---|
| `models/<domain>.rs` | 请求、响应、领域模型 | 有结构化契约 |
| `database/schema.rs` | 版本迁移 | 新表/列/索引 |
| `database/<domain>.rs` | 参数化 SQL/DAO | 使用 SQLite |
| `services/<domain>.rs` | 业务规则、事务、外部能力 | 有业务逻辑 |
| `commands/<domain>.rs` | IPC 入口 | 前端需调用 Rust |
| 各层 `mod.rs` | 模块导出 | 新模块 |
| `lib.rs` | Command/plugin/state 注册 | 新 Command/插件/State |
| `Cargo.toml` | Rust 依赖 | 现有依赖确实不足 |
| `capabilities/*.json` | permission/scope | 使用受控插件能力 |

### React

| 文件 | 作用 | 条件 |
|---|---|---|
| `types/<domain>.ts` | 与 JSON 对齐的类型 | 有结构化契约 |
| `lib/api/<domain>.ts` | Command/dev API 封装 | 有后端调用 |
| `lib/api/index.ts` | 统一导出 | 新 API 模块 |
| `store/<domain>.ts` | 跨组件状态 | 确有共享需求 |
| `pages/<domain>/index.tsx` | 页面入口 | 新页面 |
| `components/<domain>/` | 可复用子组件 | 页面过大或多处复用 |
| `Router.tsx`/Sidebar | 路由/导航 | 用户需要入口 |
| `package.json` | 前端依赖 | 插件/库确实需要 |

## 4. 实现顺序

1. 固定契约和迁移设计；如需了解 DDL/数据格式，按项目规则读取配置并通过 Tauri SSH MCP 查询真实数据库。
2. Models 与 schema migration。
3. Database DAO 与迁移/查询测试。
4. Service 业务规则、事务和安全校验。
5. Command 薄入口、模块导出、handler/plugin/state 注册。
6. TypeScript types、API 模块与错误解析。
7. Store（按需）、页面、子组件、路由/导航。
8. Capabilities 和依赖最小化核对。
9. 格式化、聚焦测试、编译/构建、真实运行时和浏览器验收。

## 5. 计划输出

实现前用短表记录：

```markdown
| 文件 | 操作 | 原因 | 核心改动 | 验证 |
|---|---|---|---|---|
| ... | 修改/新增 | ... | ... | ... |
```

这不是额外文档要求；普通任务可在运行时计划中维护，不主动创建 Markdown 文件。

## 6. 范围控制

- 已有功能：优先最小增强，不生成平行模块。
- 无数据库：不创建空 Database。
- 无共享状态：不创建 Zustand Store。
- 无插件 API：不修改 Capabilities。
- 无页面入口：不擅自加入侧边栏。
- 请求只要方案：不得进入实现。
- 遇到需要发布、远程写入或生产数据变更：停在授权边界。
