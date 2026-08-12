# 任务：团队知识库 RAG OpenSpec 实施

**状态**: 🟢 进行中  
**创建时间**: 2026-07-31  
**更新时间**: 2026-07-31  
**Git 分支**: master  
**OpenSpec Change**: `add-team-knowledge-base-rag`

## 目标

实现本地优先、项目与版本隔离的团队知识库：统一接入 Git、本地源码、禅道和既有经验，提供 FTS、可选本地/远程 Embedding、证据引用与 RAG 问答。

## 当前进度

- OpenSpec：103/117 项已完成。
- 已完成：核心目录/版本/来源/文档/FTS/向量模型、Git 与本地快照、P0 语言结构化分析、混合检索、证据预览、固定 FTS 评测、本地 Embedding 生命周期、禅道映射与事实文档、源码快照/符号/关系/工程文档、知识 MCP 与安全审计。
- 本次修复：源码关系只在明确快照且经确认时进入 RAG 关系通道；失败快照及被跳过文件不会参与 FTS/向量召回；禅道远程项目映射唯一、父子实体关系包含实体类型；本地模型运行状态、离线导入、受控镜像下载、缓存清理与 Profile 实测已补齐浏览器 Dev API 对等入口。
- 已验证：`cargo fmt --check`、`cargo test --lib --no-fail-fast`（141 passed）、`cargo check`、`pnpm exec tsc --noEmit`、`pnpm build`、浏览器知识库/Embedding 表单验收、`git diff --check` 与严格 OpenSpec 校验。
- 本轮补齐：远程 Profile 固定短文本探测、逐片段远程策略校验后的 Provider 批量构建、异常维度阻断和脱敏批次审计；浏览器 Dev API 与 Tauri Command 均调用同一 Service。重新验证 `cargo test --lib --no-fail-fast`（142 passed）、`pnpm test`（5 passed）、`pnpm build`、TypeScript 检查和浏览器远程模式表单。
- 本次续办：核验并回填解析/分块/FTS/版本历史、Embedding 生命周期、远程策略/蓝绿、混合检索回归、三个功能页与 Command/Dev API 对等任务；完成浏览器页签验收与完整质量检查（Rust 143 项、Vitest 6 项）。真实模型、禅道和桌面运行时仍需外部环境证据。
- 本轮前端补齐：Embedding Profile 远程模式强制要求 Provider 并显示来源级授权警告；问答引用通过后端详情校验后才打开原文。新增组件/API 测试覆盖表单默认值、模式依赖字段、授权提示、任务进度、重试分派、引用打开和脱敏错误（13 项通过）。
- 本轮迁移回归：从带 `ai_experiences` 数据和关闭知识库入口设置的磁盘复制库升级，验证 Schema 升级不会删除历史经验，也不会通过删除知识表实现回退。
- 安全复审修复：禅道连接地址限制为 HTTPS，避免安全凭据服务在 HTTP 明文链路上发送认证信息；定向回归、`cargo check` 与最终代码审查通过，无遗留 P0/P1 阻断。
- 分阶段发布：实现持久化的 `disabled → catalog → local_embedding → hybrid_rag → zentao → code_analysis → mcp` 设置。目录、Embedding、混合检索、禅道、源码入口在共用 Service 边界校验阶段；知识库页面按阶段延迟加载并隐藏尚未开放的集成功能，桌面 Command 与浏览器 Dev API 提供同一状态读写入口。
- 解析器 Spike：macOS 上 P0 Rust/TS/JS/TSX/Vue/Java/SQL 的结构化降级 fixture 3/3 通过；已在 ADR 记录质量级别、现有依赖边界和缺失的 Windows/Linux/真实语料证据。开发桌面二进制可编译、初始化迁移至 v34，但原生窗口仍缺少可控交互证据，不能作为关键流程验收通过。若正式版占用默认本地 API 端口，开发实例可通过仅回环的 `TAURI_SSH_LOCAL_API_ADDR` 与 `VITE_TAURI_SSH_DEV_API_BASE_URL` 覆盖端口，避免 Dev API 验收路径冲突。
- 蓝绿索引验收：已覆盖 100,000 片段下的完整性校验、原子激活、回滚与退休索引清理（1.49 秒）。真实 `embedding_build` 任务的失败后恢复仍需通过实际批处理路径验证，因此 12.5 保持待验收；不以固定向量代替真实 Embedding 模型质量验证。

## 外部依赖与待办

- 本地 Embedding 模型的中英文/代码语料基准和 Windows/macOS/Linux 实测。
- 目标禅道只读实例的地址、授权凭据引用及接口能力探测；未获得前不伪造真实同步验收。
- 后续实施：禅道故事变更、工时、测试运行、构建/发布等端点的真实实例兼容性；本地模型真实推理；完整组件/API 测试、桌面运行时及跨平台验收。
- 全库 `cargo clippy --lib -- -D warnings` 当前被 53 条既有告警阻断，需在单独的代码质量 change 中处理；本次知识库模块未新增 Clippy 告警。
