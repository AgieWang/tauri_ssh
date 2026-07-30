## Why

Tauri SSH 现有经验库只能沉淀零散的问题与解决方案，无法统一管理团队需求说明书、历史版本、禅道执行事实、源码实现证据和跨项目经验，也无法可靠回答“某项目在某版本的需求及具体实现是什么”。需要建设一个版本感知、证据可追溯的团队知识库，以 Git、禅道和本地授权源码为事实来源，通过本地索引与混合 RAG 为研发、产品和测试提供统一检索与分析能力。

## What Changes

- 新增知识项目、发布版本、知识源、逻辑文档、文档版本、知识片段和关系管理。
- 新增 Git 工作区、本地目录、单文件、手工 Markdown 和现有经验库的增量同步。
- 新增本地与远程两种 Embedding 模式、Profile 指纹、蓝绿索引重建、回滚及本地 SQLite 向量存储。
- 新增元数据过滤、FTS5、向量召回和关系扩展融合的混合检索，以及带版本约束和引用的 RAG 问答。
- 新增禅道连接、能力探测、项目/执行/版本映射，以及需求、任务、工时、Bug、测试用例和测试执行的增量同步。
- 新增基于禅道事实的项目概览、版本需求基线、追踪矩阵、任务总结、测试质量和风险文档生成。
- 新增 Git Commit/Tag/分支头/工作树及非 Git 本地源码快照分析，提取文件、符号、调用、IPC、API、SQL、数据表和测试关系。
- 新增模块、API、数据库、调用链、Commit 变更、版本实现和影响分析文档生成。
- 新增需求—任务—Commit—代码符号—测试的统一证据链、人工关系确认和低置信度 AI 关系建议。
- 新增来源级远程处理授权、敏感信息阻断、凭据隔离、审计、后台任务恢复和 MCP 只读能力。
- 首期全部索引运行在 Tauri SSH 客户端，不要求部署服务器向量数据库。

## Capabilities

### New Capabilities

- `knowledge-catalog`: 管理多项目、多版本知识源、逻辑文档、文档版本、片段、同步任务和来源追溯。
- `embedding-index-management`: 提供本地/远程 Embedding Profile、文档分块、FTS5、向量存储、蓝绿重建、激活和回滚。
- `hybrid-knowledge-rag`: 提供版本硬过滤、全文/向量/关系融合检索、证据上下文组装、带引用问答和证据不足拒答。
- `zentao-knowledge-ingestion`: 连接禅道并增量同步需求、任务、Bug 和测试事实，建立项目/版本映射并生成项目文档。
- `source-code-knowledge`: 对 Git 历史版本、工作树和本地授权源码建立快照，提取代码符号及关系并生成可检索代码文档。
- `knowledge-security-governance`: 对知识源读取、远程向量化、远程大模型、敏感内容、凭据、MCP 和审计实施统一治理。

### Modified Capabilities

无。当前仓库尚无已发布的 OpenSpec capability，本 change 只引入新能力。

## Impact

- Rust 后端新增知识库 Commands、Services、Database DAO、后台任务、解析器、Embedding Provider、检索/RAG、禅道适配器和源码分析器。
- SQLite 从当前 schema 版本分阶段增加知识项目、版本、来源、文档、片段、向量、关系、禅道和源码分析相关表及 FTS5 索引。
- React 新增知识问答、文档、项目版本、知识源、索引任务、Embedding 设置、禅道同步和代码知识页面。
- 扩展现有 AI Provider、Skill scope、经验库、Git 工作区、安全凭据、审计、浏览器 Dev API 和 MCP 集成。
- 可能新增 `fastembed`/ONNX Runtime、HTTP 客户端、文档解析和多语言源码解析依赖；依赖锁定前需完成三平台技术 Spike。
- 远程 Embedding 和远程 AI 可能处理内部文档或代码，必须保持默认关闭并由系统、来源和敏感级别多重授权。
