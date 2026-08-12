# 架构模式按需参考

## 项目层次与职责

| 层 | 典型位置 | 核心职责 | 不应承担 |
|---|---|---|---|
| React 页面/组件 | `src/pages/`、`src/components/` | 展示、交互、局部 UI 状态 | SQL、系统调用、核心业务规则 |
| Zustand/Hook | `src/store/`、`src/hooks/` | 跨组件 UI 状态、可复用交互逻辑 | 业务持久化 |
| API 封装 | `src/lib/api/` | 类型安全的 `invoke` 封装 | 页面渲染、数据库访问 |
| Command | `src-tauri/src/commands/` | IPC 入参校验、调用 Service、结果转换 | SQL 和复杂业务规则 |
| Service | `src-tauri/src/services/` | 业务规则、编排与跨 DAO 协作 | UI 展示逻辑 |
| Database | `src-tauri/src/database/` | SQL、事务、迁移与数据映射 | IPC 与 UI 语义 |
| Model | `src-tauri/src/models/`、`src/types/` | 领域/传输结构及双端契约 | 隐式副作用 |
| State | `src-tauri/src/state.rs` | 受控共享资源与生命周期 | 无边界的全局可变状态 |

## 逻辑放置决策表

| 问题 | 放置位置 |
|---|---|
| 只影响渲染或表单交互 | React 组件/Hook |
| 多页面共享 UI 状态 | Zustand Store |
| 前后端调用契约 | TypeScript API + Rust Command |
| 业务校验、流程编排 | Rust Service |
| SQL、事务、SQLite 映射 | Rust Database |
| OS、文件、进程或凭据能力 | Rust Service/专用基础设施模块 |
| 插件权限范围 | `src-tauri/capabilities/` |

## 标准调用链

```text
用户交互
  -> React 页面/组件
  -> src/lib/api 类型安全封装
  -> #[tauri::command]
  -> Service 业务逻辑
  -> Database/系统能力
  -> Result 与 DTO
  -> React 状态和 UI 反馈
```

## 常见架构异味

- Command 中直接拼接 SQL 或承载长业务流程。
- 页面散落裸 `invoke()`，导致命名和错误处理不一致。
- 同一业务实体在 Rust 与 TypeScript 中字段语义不同。
- 为复用少量代码创建万能工具层，反而隐藏领域归属。
- 把持久数据塞入 Zustand，或把短期 UI 状态写入 SQLite。
- Service 相互循环调用，或 Database 反向依赖 Command。
- 架构图与实际注册、路由、schema 不一致。

## 设计验证建议

- 使用 `rg` 追踪命令注册、`invoke` 调用、模块导出和 SQL 表名。
- 对数据库边界验证迁移与真实数据格式。
- 对跨进程契约执行 Rust/TypeScript 检查和相应测试。
- 对页面链路使用 Codex 内置浏览器或 Control Chrome 验收。
