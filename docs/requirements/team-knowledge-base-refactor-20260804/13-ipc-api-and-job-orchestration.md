# 13 IPC、API 与持久化任务编排确认

- 状态：Proposed
- 对应用户需求：跨层实现与长任务

## 1. 固定调用链

```text
React 页面
  → src/lib/api/knowledge-domain/* 类型安全封装
  → Tauri invoke / 本地开发对等接口
  → commands/knowledge_domain/*
  → services/knowledge_domain/*
  → database/knowledge_domain/* 或受控系统适配器
```

页面禁止裸写 `invoke()`；Command 只做 IPC 校验、Service 调用和结构化错误转换；SQL 只在 Database 层。

## 2. 领域 API 组

| API 组 | 主要能力 |
|---|---|
| catalog | 项目、仓库绑定、版本清单 |
| documents | 草稿、提交、详情、历史、删除/恢复 |
| ingestion | 上传准备、批量上传、解析任务 |
| search | 基础搜索、解释、引用详情 |
| analysis | 运行、草稿、保存、确认入库 |
| graph | 构建、子图、关系确认、证据 |
| qa | 上下文预览、提问、引用打开 |
| jobs | 列表、详情、取消、重试 |
| governance | 开关、授权和评测门禁 |

## 3. DTO 契约

每个请求/响应明确：字段名、类型、必填、默认、可空、枚举、时间格式、分页和错误。Rust 使用显式 serde 命名/默认；TypeScript 镜像字段。重点覆盖过去出现过的 `versionStrategy`、`allowRemoteProcessing` 等默认字段漂移。

## 4. 错误模型

错误至少分：输入校验、未找到、冲突、未授权、能力不可用、超限、处理中、外部 Provider、解析失败、数据库和内部错误。前端显示可操作中文信息；内部堆栈、路径和秘密不返回。

## 5. 任务状态机

```text
queued → running → completed
               ├→ failed → queued（重试）
               ├→ cancelling → cancelled
               └→ interrupted → queued（显式恢复）
```

解析、同步、分析、向量、图谱和大型回填统一进入持久化任务系统。payload 只保存可重放引用和配置快照，不保存 Token 或大段正文。

每个任务定义：幂等键、检查点、心跳、取消边界、重试分类、临时清理、结果完整性和审计关联 ID。文件/网络异步等待期间不持有 SQLite 锁。

## 6. 当前状态

Catalog、Documents、Ingestion 已有较完整的新 Command/API；Search、Analysis、Graph、QA、Jobs、Governance 新分域中仍有占位/兼容出口。旧 Command 数量很多，兼容 Facade 需要逐项契约测试后才能收口。

## 7. 验收标准

- 每个生产 Command 都有定义、模块导出、`generate_handler!` 注册、TS API 和真实调用点。
- Tauri 与开发接口对相同 payload 返回等价结构化结果。
- 缺失默认字段、空值、未知枚举、超长值和错误类型有往返测试。
- 进程重启后 running 任务可识别为 interrupted，并安全重试。
- 同一任务重复点击不会产生重复正式文档或重复激活索引。

## 8. 请确认

- **C13-1（推荐）**：按九个领域组拆 IPC/API，旧接口只做兼容委托。
- **C13-2（推荐）**：所有新 DTO 使用显式 serde/default 并提供 Rust/TS 契约测试。
- **C13-3（推荐）**：所有长任务统一持久化、幂等、可取消/重试/恢复。
- **C13-4（推荐）**：任务重试只对可重试错误自动建议，不自动无限重试。
- **C13-5（待选择）**：兼容旧 Command 的保留周期；推荐新 UI 全量验收后再保留 2 个发布版本。
