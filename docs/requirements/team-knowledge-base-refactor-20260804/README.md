# 团队知识库重构需求确认包

- 日期：2026-08-04
- 状态：Proposed（待用户逐项确认）
- 受众：产品、研发、测试、安全与后续实施人员
- 范围：团队知识库现有功能重构及 11 项新增需求

## 结论先行

推荐把知识库重构为一套以“项目 → 多仓库 → 项目版本 → 文档版本 → 可追溯证据”为主线的本地优先平台。标题与全文搜索作为始终可用的基础能力，向量、图谱和 AI 作为可关闭、可降级的增强能力；AI 生成内容必须先成为草稿，经用户编辑确认后才进入正式知识库。

当前仓库已有较多可复用实现，但不是从零开始，也不是全部完成：代码中的 SQLite `SCHEMA_VERSION` 已到 39；项目目录、多仓库绑定、版本清单、受管文档、文件上传和多类解析已有分域代码；搜索增强、版本绑定收口、新分析审核链、新图谱、新向量/问答闭环及真实端到端验收仍需继续实施。

## 如何确认

建议按编号阅读。每份文档末尾都有“请确认”章节：

- 回复“全部按推荐方案确认”，代表接受所有标为“推荐”的默认方案。
- 回复“确认 02、03；修改 06 的 C2 为……”可以分批审批。
- 未明确确认的决策继续保持 Proposed，不作为编码授权或最终验收结论。

## 文档清单

如需一次性评审完整方案，优先阅读：

- [团队知识库重构详细需求实施方案](./19-detailed-requirement-implementation-plan.md)

| # | 文档 | 主要确认内容 |
|---|---|---|
| 00 | [需求基线与范围](./00-requirement-baseline-and-scope.md) | 产品边界、角色、非目标和成功标准 |
| 01 | [信息架构与用户旅程](./01-product-information-architecture-and-user-journeys.md) | 工作台导航与六条线性主流程 |
| 02 | [项目与多仓库目录](./02-project-multi-repository-catalog.md) | 项目、多 Git 仓库、解除关联与只读 Git |
| 03 | [文档接入与格式支持](./03-document-ingestion-and-format-support.md) | Markdown、Office、图片、HTML、PDF 解析范围 |
| 04 | [受管文档生命周期与上传](./04-managed-document-lifecycle-and-upload.md) | CRUD、上传、软删除、恢复、并发 |
| 05 | [标题与全文搜索](./05-title-and-full-text-search.md) | 标题/FTS、筛选、排序、分页和降级 |
| 06 | [项目版本治理](./06-project-version-governance.md) | Branch/Tag/Commit 与多仓库不可变清单 |
| 07 | [自定义文档版本](./07-custom-document-versioning.md) | 草稿、提交、比较、恢复与版本绑定 |
| 08 | [AI 代码分析与人工审核](./08-ai-code-analysis-and-review.md) | 只读分析、草稿、引用、编辑确认 |
| 09 | [项目知识图谱](./09-knowledge-graph.md) | 节点/边、版本隔离、关系确认和展示 |
| 10 | [向量化与本地向量存储](./10-embedding-and-local-vector-store.md) | 本地/远程 Embedding、Profile、蓝绿切换 |
| 11 | [基于证据的 AI 问答](./11-evidence-grounded-ai-qa.md) | 混合检索、引用、拒答与冲突处理 |
| 12 | [数据模型与迁移](./12-data-model-and-migration.md) | schema v39 基线、核心实体、迁移与回滚 |
| 13 | [IPC、API 与任务编排](./13-ipc-api-and-job-orchestration.md) | React→Command 契约、长任务、错误码 |
| 14 | [安全、权限与审计](./14-security-permission-and-audit.md) | 路径、凭据、远程发送、敏感数据与 MCP |
| 15 | [线性 UX 与中文文案](./15-ux-linear-configuration-and-copy.md) | 普通用户体验、渐进展示与反馈规则 |
| 16 | [可观测、测试与验收](./16-observability-test-acceptance.md) | 质量指标、测试矩阵和真实验收边界 |
| 17 | [迭代路线与交付门禁](./17-iteration-plan-and-delivery-gates.md) | 分阶段实施、依赖、里程碑和回滚 |
| 18 | [总确认清单](./18-master-confirmation-checklist.md) | 所有决策编号与推荐项汇总 |
| 19 | [详细需求实施方案](./19-detailed-requirement-implementation-plan.md) | 完整架构、领域实施、迁移、阶段计划和验收 |

## 证据边界

本确认包基于当前仓库的 OpenSpec、Rust/React 分域代码、Command 注册、前端路由、SQLite 迁移和测试清单静态核对生成。它不是生产验收报告；未在本次规划中执行真实文件导入、真实 Tauri IPC、浏览器交互、向量推理或远程 Provider 调用。
