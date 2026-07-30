# 团队级多项目知识库与混合 RAG 实施方案

> 文档状态：提议中
>
> 创建日期：2026-07-29
>
> 适用项目：Tauri SSH
>
> 目标版本：V0.1～V0.4
>
> 核心决策：Git、禅道与本地源码共同提供研发事实、客户端本地索引、必须向量化、本地与远程 Embedding 可切换

---

## 1. 背景

Tauri SSH 已具备以下与知识库相关的基础能力：

- AI Provider 配置、加密凭据、模型列表、场景路由和问答接口。
- Skill 管理、经验库、Runbook、Prompt 注入和 MCP 暴露能力。
- Git 工作区扫描、分支、提交、拉取和安全凭据复用能力。
- SQLite 本地数据库、增量迁移和软删除模式。
- 终端、日志、数据库、Jenkins、部署、代码审核等知识产生入口。
- Tauri 文件选择、应用数据目录和 Rust 文件处理能力。

现有经验库主要面向“问题现象—根因—解决方案”的结构化经验，无法完整承载团队需求说明书、项目文档、历史版本、技术设计、代码实现证据和跨项目经验分析。

本方案将知识库定位为团队级研发知识中枢：通过 Git 共享需求文档、技术方案、历史版本和代码证据，通过禅道读取产品、项目、执行、需求、任务、Bug 和测试事实，并可读取分析用户授权的本地源码目录；每个 Tauri SSH 客户端维护本地结构化元数据、代码符号关系、全文索引和向量索引，最终形成可引用、可追溯、版本感知的混合 RAG 问答能力。

---

## 2. 建设目标

### 2.1 核心目标

1. 汇总团队需求说明书、技术方案、接口文档、数据库设计、测试资料、发布说明和开发经验。
2. 支持多个项目、多仓库、多分支、多版本统一管理。
3. 支持查询某项目在指定版本下的需求、设计和实际实现方案。
4. 支持关键词检索、向量语义检索、元数据过滤和研发关系扩展。
5. 支持大模型基于检索证据回答，并返回可核查的文档、章节、版本和代码来源。
6. 支持本地 Embedding 与远程 Embedding API 通过设置切换。
7. 默认不要求部署服务器向量数据库。
8. 与现有 AI Provider、Skill、经验库、MCP、审计和安全凭据体系集成。
9. 支持从禅道增量同步需求、任务执行、Bug、测试用例和测试执行结果。
10. 基于禅道与 Git 事实自动生成可追溯的项目文档，并进入统一检索和 RAG 流程。
11. 支持读取 Git 已提交版本和用户授权的本地工作区源码，分析模块、符号、调用链、接口、SQL、配置和测试。
12. 将代码快照、代码关系和自动生成的代码文档纳入统一 FTS、向量检索及 RAG。

### 2.2 典型问题

- 某项目在 `v1.6.0` 版本增加了哪些需求？
- 某需求当时为什么采用当前方案？
- 某需求对应哪些接口、数据库变更、Commit 和测试？
- 同一需求在不同版本中的变化是什么？
- 多个项目如何实现相似功能，有哪些共性经验？
- 某次生产问题是否在其他项目出现过？
- 当前实现与原需求是否存在偏差？
- 禅道中某版本有哪些需求尚未完成、尚未测试或存在未关闭 Bug？
- 某需求从提出、任务执行、代码提交到测试通过的完整证据链是什么？
- 某个接口从前端页面到后端 Command、Service 和 Database 的完整调用链是什么？
- 某次 Git Commit 修改了哪些模块，可能影响哪些接口、数据表和测试？
- 本地尚未提交的代码与指定发布版本相比有哪些实现差异？

### 2.3 非目标

首期不建设以下能力：

- 不共享或同步每个客户端的 SQLite 数据库文件。
- 不首期部署 PostgreSQL、Qdrant、Milvus 等集中式向量设施。
- 不首期建设多人实时协同编辑器。
- 不允许 AI 自动修改 Git 知识源并直接推送。
- 不保证从完全缺失的文档中还原历史需求。
- 不允许用当前版本文档替代指定历史版本的证据。
- 不首期实现图片 OCR、扫描件识别和复杂表格语义还原。

---

## 3. 核心术语

| 术语 | 定义 |
| --- | --- |
| 知识源 | Git 工作区、禅道、本地源码目录、普通文档目录、单个文件、手工文档或现有经验库 |
| 知识项目 | 被知识库管理的业务项目，可以关联一个或多个 Git 工作区 |
| 发布版本 | 项目的业务版本，可关联 Git Tag、分支和基线 Commit |
| 逻辑文档 | 跨版本保持稳定身份的文档，例如“统一车辆调度需求说明书” |
| 文档版本 | 逻辑文档在某个项目版本、分支或 Commit 下的内容快照 |
| 知识片段 | 文档按标题和长度切分后用于检索与 RAG 的最小单元 |
| Embedding Profile | 一套完整的向量配置，包括模式、模型、维度和分块策略 |
| 混合检索 | 元数据过滤 + FTS5 全文检索 + 向量检索 + 关系扩展 |
| RAG | 检索知识片段后交给大模型生成带引用回答的流程 |
| 活动索引 | 当前问答和检索正式使用的 Embedding Profile 对应索引 |
| 禅道实体快照 | 某次同步获得的需求、任务、Bug 或测试实体的规范化内容及原始证据摘要 |
| 追踪矩阵 | 需求与任务、Commit、测试、版本之间的可核查关系集合 |
| 代码快照 | Git Commit/Tag、当前工作树或本地目录在某个时间点的可重复分析基线 |
| 代码符号 | 类、函数、方法、结构体、接口、组件、路由、Command、SQL 对象等可定位实体 |

---

## 4. 已确认的产品决策

### 4.1 知识共享

- Git 仓库是团队共享知识的首选事实来源。
- 禅道是需求流转、任务执行和测试过程的业务事实来源；Git 是文档、代码与 Commit 的工程事实来源。
- 非 Git 本地源码属于本机补充证据；除非显式发布到团队 Git，否则不能作为团队共享的历史事实。
- 两类事实按项目、版本、需求/任务编号进行关联，冲突时并列展示来源和时间，不静默覆盖。
- Tauri SSH 本地 SQLite 只保存本机索引、配置、任务状态和使用记录。
- 每个客户端从有权限的 Git 工作区和禅道实例同步数据并独立构建索引。
- Git 权限、分支、Tag、Commit 和历史记录用于团队共享与版本追溯。

### 4.2 向量化

- 知识片段必须支持向量化。
- 支持两种 Embedding 模式：
  - 方案 A：客户端本地模型生成向量。
  - 方案 B：远程 Embedding API 生成向量，向量仍保存在本地。
- 用户在系统设置中选择模式和模型。
- 不同模型、维度、归一化方式和分块策略的向量禁止混用。
- 切换 Embedding Profile 必须重建索引。

### 4.3 检索

- 精确项目和版本过滤优先于全文或向量相似度。
- FTS5 负责需求编号、类名、字段名、接口和版本号等精确检索。
- 向量检索负责自然语言、同义表达和跨项目语义匹配。
- 关系查询负责从需求扩展到设计、代码、测试和发布证据。
- 大模型只能基于检索证据作答，不允许把推测表述为事实。

### 4.4 索引切换

- 采用蓝绿索引切换。
- 新 Profile 建索引期间，旧活动索引继续提供服务。
- 新索引全部成功后，原子切换为活动索引。
- 新索引失败时保留旧索引，并显示失败原因。
- 不允许保存设置后立即删除当前可用索引。

---

## 5. 用户角色与用户故事

### 5.1 开发人员

- 我可以按项目和版本搜索需求、技术方案和代码实现。
- 我可以查看某需求关联的 Commit、接口、SQL 和测试。
- 我可以将开发过程中验证过的经验提交到团队知识仓库。
- 我可以比较多个项目处理相似问题的不同方案。

### 5.2 产品与需求人员

- 我可以查看某个版本最终纳入了哪些需求。
- 我可以查看需求在后续版本中的调整和替代关系。
- 我可以从实现证据判断需求是否实际落地。

### 5.3 架构师与技术负责人

- 我可以检索历史架构决策及其背景。
- 我可以对比不同项目中的技术选型和演进路线。
- 我可以识别重复建设、技术债和可复用模式。

### 5.4 测试人员

- 我可以从需求定位对应测试用例和验证记录。
- 我可以查询某版本有哪些需求缺少测试证据。

### 5.5 知识管理员

- 我可以配置知识项目、来源、解析规则和版本识别策略。
- 我可以查看同步、解析、分块和向量化任务状态。
- 我可以切换本地或远程 Embedding，并安全重建索引。
- 我可以控制哪些来源允许发送到远程 Embedding API。

---

## 6. 功能范围

### 6.1 V0.1：多项目版本化知识库

- 知识项目管理。
- Git 工作区、本地目录和单文件知识源。
- Markdown、TXT、SQL、JSON、YAML 文本解析。
- DOCX、PDF 文本提取能力预留，完成技术验证后纳入。
- 逻辑文档与文档版本管理。
- 项目版本、Tag、分支和 Commit 关联。
- 文档分块。
- FTS5 全文检索。
- 本地 Embedding。
- 远程 Embedding API。
- Embedding 设置切换和蓝绿重建。
- 混合检索。
- 文档、片段和引用查看。
- 基础 RAG 问答。

### 6.2 V0.2：需求与实现追踪

- 禅道连接、版本能力探测和项目映射。
- 禅道需求、需求变更、任务、工时、Bug、测试用例和测试执行增量同步。
- 基于模板生成项目概览、版本需求基线、追踪矩阵、任务总结和测试质量报告。
- Git Commit、Tag、分支和工作树源码分析。
- 用户授权的非 Git 本地源码目录分析。
- 文件、符号、导入、调用、继承、IPC、API、SQL、数据表和测试关系。
- 自动生成模块说明、接口文档、数据库文档、调用链和版本实现摘要。
- 需求编号和需求实体识别。
- 需求、设计、接口、数据库、代码、测试、版本关系。
- Git Commit 和文件路径证据。
- 人工建立关系。
- AI 建议关系，人工确认。
- 某项目某版本的需求与实现总结。
- 版本间差异分析。

### 6.3 V0.3：跨项目经验分析

- 跨项目相似需求召回。
- 实现方案对比。
- 故障、性能和数据问题经验聚合。
- 从文档和分析结果提炼结构化经验。
- 与现有 `ai_experiences` 双向兼容。
- 经验质量、验证状态和过期状态。

### 6.4 V0.4：团队集中化可选演进

- 可选的集中知识服务。
- 可选的服务器向量数据库。
- 统一索引、权限、反馈和审计。
- Web 与移动端访问。
- 本地模式与集中模式兼容。

---

## 7. 总体架构

```text
团队 Git 仓库 / 禅道 / 知识仓库 / 本地文档
                  │
                  ▼
       KnowledgeSourceService
       来源发现、同步、版本识别
                  │
                  ▼
       KnowledgeParserService
       解析、规范化、脱敏、分块
                  │
         ┌────────┴────────┐
         ▼                 ▼
   SQLite + FTS5      EmbeddingService
   元数据/全文索引     Local / Remote
         │                 │
         └────────┬────────┘
                  ▼
       KnowledgeRetrievalService
       过滤、全文、向量、关系、融合
                  │
        ┌─────────┴──────────┐
        ▼                    ▼
  知识库搜索页面        KnowledgeRagService
                              │
                              ▼
                    现有 AiProviderService
                              │
                              ▼
                     带引用的大模型回答
```

### 7.1 部署边界

首期新增能力全部运行在 Tauri SSH 客户端：

- 文档同步和解析：Rust。
- 本地模型推理：Rust。
- 远程 Embedding 请求：Rust。
- SQLite、FTS5 和向量存储：本地。
- RAG 检索与 Prompt 组装：Rust。
- 页面和设置：React。

远程依赖只有：

- 团队已有 Git 平台。
- 用户配置的禅道实例。
- 用户选择方案 B 时的 Embedding API。
- 大模型问答使用的现有 AI Provider。

---

## 8. 知识来源与团队共享

### 8.1 来源类型

| 来源类型 | 说明 | 首期 |
| --- | --- | --- |
| `git_workspace` | 复用现有 Git 工作区 | 必做 |
| `local_directory` | 监听或手工同步本地目录 | 必做 |
| `local_code_directory` | 分析用户授权的非 Git 本地源码目录 | V0.2 |
| `single_file` | 导入单个文件 | 必做 |
| `manual` | 在知识库页面创建 Markdown | 必做 |
| `ai_experience` | 兼容现有经验库 | 必做 |
| `zentao` | 禅道需求、任务、Bug 与测试数据 | V0.2 |
| `remote_url` | 远程文档链接 | 暂缓 |
| `central_service` | 集中知识服务 | V0.4 |

### 8.2 Git 文档约定

建议各项目按以下结构管理文档：

```text
docs/
├── requirements/
├── designs/
├── api/
├── database/
├── adr/
├── testing/
├── releases/
└── experiences/
```

推荐 Markdown Front Matter：

```yaml
---
project: unified-vehicle
version: v1.6.0
doc_type: requirement
document_key: unified-vehicle-dispatch-requirement
requirement_ids:
  - REQ-102
tags:
  - dispatch
  - vehicle
relations:
  - type: implemented_by
    target: commit:abc123
---
```

### 8.3 版本识别优先级

1. 文档 Front Matter 明确声明。
2. 用户绑定的发布版本和 Git Tag。
3. 文档所在分支和当前 Commit。
4. 路径规则，例如 `docs/releases/v1.6.0/`。
5. 手工选择。

无法识别版本时标记为 `unversioned`，不能自动归入最新版本。

### 8.4 同步策略

- Git 来源记录最近索引的 Commit。
- 使用 Git Diff 识别新增、修改、重命名和删除。
- 内容哈希未变化时跳过解析和向量化。
- 删除文件对应文档版本标记为失效，不物理删除历史。
- 文档变更后只重建受影响的片段。
- Git 操作继续复用现有安全凭据和 Git 工作区服务。

---

## 9. 文档解析与分块

### 9.1 格式支持

| 格式 | 处理策略 |
| --- | --- |
| Markdown | 保留标题层级、代码块、表格和 Front Matter |
| TXT / LOG | 按段落、时间块和长度切分 |
| SQL | 按语句、注释块和对象定义切分 |
| JSON / YAML | 规范化后按顶层路径切分 |
| DOCX | 提取标题、段落、表格文本，保留原文件引用 |
| PDF | 提取文本和页码；扫描版标记为需要 OCR |
| 源码 | 首期只保存路径和摘要，不默认索引所有源码正文 |

### 9.2 分块规则

默认分块策略：

- 优先按 Markdown 标题和文档结构切分。
- 单片段目标长度为 500～800 个近似 Token。
- 最大长度为 1,000 个近似 Token。
- 相邻片段重叠 80～120 个近似 Token。
- 代码块、SQL 语句和表格尽量保持完整。
- 每个片段保留：
  - 标题路径。
  - 页码或行号范围。
  - 文档版本。
  - 项目、版本、分支和 Commit。
  - 内容哈希。

分块策略本身纳入 Embedding Profile 指纹。调整分块规则后必须重建索引。

### 9.3 内容规范化

- 换行统一为 `\n`。
- 去除 UTF-8 BOM。
- 保留中文、英文、代码标识符和路径。
- 避免无意义空白和重复导航文本。
- 不改变需求编号、接口、字段、SQL 和 Commit。
- 对远程 Embedding 请求执行敏感信息检查和脱敏。

---

## 10. Embedding 双模式设计

### 10.1 统一抽象

后端定义统一 Embedding Provider 边界：

```text
EmbeddingProvider
├── health_check()
├── profile()
├── embed_documents(texts)
└── embed_query(text)

实现：
├── LocalFastEmbedProvider
└── RemoteEmbeddingProvider
```

业务层只依赖统一接口，不感知本地或远程实现。

### 10.2 方案 A：本地 Embedding

推荐候选：

- Rust crate：`fastembed`。
- 调研时 crates.io 当前版本：`5.17.3`。
- 默认候选模型：`intfloat/multilingual-e5-small`。
- 中文优先备选：`BAAI/bge-small-zh-v1.5`。
- 模型最终选择需通过中文需求、代码标识符和跨项目样本评测确认。

本地模式特性：

- 文档内容不离开本机。
- 模型文件下载到应用数据目录。
- 支持离线导入模型文件。
- 支持下载进度、校验和、暂停后重试。
- CPU 为首期默认执行设备。
- GPU、Metal、CUDA、DirectML 作为后续优化，不作为首期验收条件。
- 推理在后台任务执行，不阻塞 Tauri IPC 和 UI。

依赖配置候选：

```toml
fastembed = { version = "5.17", default-features = false, features = ["hf-hub-rustls-tls", "ort-download-binaries-rustls-tls"] }
```

以上配置必须经过三平台技术 Spike 后再锁定。若 ONNX Runtime 下载、签名或离线构建不满足发布要求，则改用动态加载或应用资源携带的固定 Runtime，不允许在正式构建阶段依赖不可控的在线下载。

模型目录建议：

```text
$APPDATA/models/embedding/{model-key}/{revision}/
```

默认不将模型打入安装包，原因：

- 安装包体积增长明显。
- 模型许可、更新和不同平台二进制需要独立治理。
- 团队可能需要使用内部镜像或离线文件。

### 10.3 方案 B：远程 Embedding API

远程模式复用现有 `ai_providers`：

- API Key 继续由 Rust 加密保存。
- 前端只看到掩码，不读取明文。
- Embedding Profile 引用 `provider_key`。
- Embedding 模型与聊天 `default_model` 分开配置。
- Provider 能力增加 `embedding` 标识。

首批协议：

- OpenAI-compatible：`POST /embeddings`。
- Ollama-compatible：`POST /api/embed`。
- 其他专有协议通过适配器逐步增加。

OpenAI-compatible 请求形状：

```json
{
  "model": "configured-embedding-model",
  "input": ["document chunk 1", "document chunk 2"]
}
```

Ollama-compatible 请求形状：

```json
{
  "model": "configured-embedding-model",
  "input": ["document chunk 1", "document chunk 2"]
}
```

协议适配层不得假设所有 Provider 都支持 `dimensions` 参数；只有 Provider 明确支持时才发送。首次测试必须使用短文本生成真实向量，并以实际返回值确定维度。

远程请求约束：

- 支持批量输入。
- 支持超时、并发数、重试和退避。
- 请求前检查来源是否允许远程向量化。
- 请求日志不记录原始文档正文。
- 记录 Provider、模型、批次数、字符数、延迟和失败原因。
- Provider 返回维度与 Profile 声明不一致时立即停止任务。

### 10.4 设置项

系统设置增加“知识库与向量化”：

| 设置 | 说明 |
| --- | --- |
| Embedding 模式 | `local` 或 `remote` |
| 本地模型 | 本地模式使用的模型 |
| 本地模型目录 | 自动下载目录或离线模型目录 |
| 本地批大小 | 默认根据设备能力确定 |
| 远程 Provider | 引用现有 AI Provider |
| 远程模型 | 独立的 Embedding 模型 |
| 远程批大小 | 避免超过 Provider 限额 |
| 并发数 | 默认保守值 |
| 超时时间 | 单次远程请求超时 |
| 允许远程发送 | 总开关，来源还需单独允许 |
| 自动重试 | 是否重试临时错误 |
| 活动 Profile | 当前正式使用的向量配置 |

### 10.5 模式切换

用户保存影响向量兼容性的设置时：

1. 后端计算新的 Profile 指纹。
2. 如果指纹未变化，只保存非索引设置。
3. 如果指纹变化，创建新的非活动 Profile。
4. UI 显示预计需要重建的文档和片段数量。
5. 用户确认后创建重建任务。
6. 新 Profile 独立生成全部向量。
7. 完整性检查通过后切换活动 Profile。
8. 旧 Profile 暂时保留，支持回滚。
9. 后台清理超过保留期的旧索引。

禁止行为：

- 不允许使用远程模型生成查询向量，却与本地模型的文档向量比较。
- 不允许仅凭维度相同就认为不同模型兼容。
- 不允许本地模式失败后未经授权自动把内容发送到远程。

### 10.6 Profile 指纹

指纹至少包含：

```text
mode
provider protocol
provider key（远程）
endpoint identity（远程）
model
model revision
dimension
normalization
query prefix
document prefix
chunk strategy version
content normalization version
```

---

## 11. 向量存储与检索

### 11.1 首期存储

- 向量以 `f32` 小端 BLOB 保存在 SQLite。
- 保存维度、模型 Profile、向量范数和生成时间。
- 同一个片段允许同时保留多个 Profile 的向量。
- 查询只读取活动 Profile。
- Profile 切换完成前不删除旧向量。

### 11.2 首期向量搜索

首期采用：

1. 项目、版本、文档类型和权限硬过滤。
2. 加载过滤后候选片段的活动向量。
3. Rust 并行计算余弦相似度。
4. 返回向量 Top K。

该方式避免服务器向量数据库，适用于首期文档规模。

### 11.3 大规模升级

满足任一条件时评估本地 HNSW：

- 活动片段超过约 20 万。
- 跨项目向量查询 P95 超过 500ms。
- 单次候选向量内存占用影响前台操作。

本地 HNSW 候选可采用纯 Rust 或可持久化实现。调研时 `hnsw_rs` crates.io 当前版本为 `0.3.4`，但在正式引入前必须验证：

- Windows、macOS、Linux 构建。
- 索引落盘和崩溃恢复。
- Profile 切换和删除。
- 大小端、维度和数据一致性。

无论使用何种 ANN 实现，SQLite 仍是元数据事实来源，ANN 索引可以从 SQLite 重建。

---

## 12. 混合检索设计

### 12.1 查询流程

```text
用户问题
  ↓
QueryAnalyzer
识别项目、版本、需求编号、文档类型
  ↓
MetadataFilter
先限定项目和版本
  ↓
并行召回
├── FTS5 精确检索
├── Vector 语义检索
└── Relation 关系扩展
  ↓
RankFusion
  ↓
可选 Rerank
  ↓
Top Context + Citations
```

### 12.2 查询分析

优先使用确定性规则：

- 项目名称和别名匹配。
- `v1.6.0`、`1.6` 等版本识别。
- `REQ-102` 等需求编号识别。
- 类名、接口路径、表名和字段名识别。

只有无法确定时才调用大模型做 Query Rewrite。大模型解析结果必须映射到已存在的项目和版本，不能凭空创建过滤条件。

### 12.3 融合排序

建议采用 RRF 或归一化加权：

```text
final_score =
  fts_score
  + vector_score
  + exact_project_bonus
  + exact_version_bonus
  + requirement_id_bonus
  + confirmed_relation_bonus
  + verified_document_bonus
  - stale_document_penalty
```

原则：

- 精确版本匹配必须高于语义相似的其他版本。
- 明确需求编号命中必须高于普通文本相似。
- 已确认关系高于 AI 建议但未确认的关系。
- 被标记过期或失效的文档默认不进入主结果。

### 12.4 FTS5

- 运行时检查 FTS5 能力。
- 优先评估 `trigram` tokenizer 对中文和子串检索的效果。
- 如果运行环境不支持，回退到 `unicode61` 并增加应用层查询拆分。
- FTS 表只保存检索字段，正文事实仍保存在普通表。

---

## 13. RAG 问答设计

### 13.1 问答输入

```text
question
projectKeys[]
releaseIds[]
documentTypes[]
includeExperiences
includeCodeEvidence
maxCitations
providerKey
```

### 13.2 上下文构建

顺序：

1. 系统安全规则。
2. 项目和版本限定。
3. 回答格式要求。
4. 检索到的知识片段。
5. 每个片段的引用标识。
6. 用户问题。

每个片段使用稳定引用编号：

```text
[K1] 项目、版本、文档标题、章节、路径、Commit
[K2] 项目、版本、文档标题、页码、路径、Commit
```

### 13.3 回答格式

“某项目某版本需求和实现”默认回答：

1. 版本背景。
2. 核心需求。
3. 技术设计。
4. 接口和数据库变更。
5. 代码实现证据。
6. 测试和验收证据。
7. 与前后版本差异。
8. 已知问题和经验。
9. 引用来源。
10. 证据缺口。

### 13.4 证据规则

- 重要结论必须引用至少一个片段。
- 历史版本问题禁止引用更晚版本作为当时事实。
- 后续版本资料只能放在“后续演进”部分。
- 没有实现证据时明确写“未找到实现证据”。
- 文档互相冲突时并列展示冲突和各自来源。
- 不得用模型常识补充项目内部事实。

---

## 14. 需求—设计—实现关系

### 14.1 实体

- Project
- Release
- Requirement
- Document
- DocumentVersion
- Chunk
- Api
- DatabaseChange
- CodeFile
- CodeSnapshot
- CodeSymbol
- GitCommit
- TestCase
- Experience

### 14.2 关系类型

| 关系 | 示例 |
| --- | --- |
| `belongs_to` | 文档属于项目 |
| `introduced_in` | 需求首次进入某版本 |
| `changed_in` | 需求在某版本变更，或代码文件/符号在 Commit 中变化 |
| `designed_by` | 需求对应设计方案 |
| `implemented_by` | 需求由 Commit 或代码文件实现 |
| `declares` | 代码文件声明符号 |
| `calls` / `imports` | 代码符号调用或依赖其他符号 |
| `affects_api` | 需求影响接口 |
| `affects_database` | 需求影响数据库 |
| `verified_by` | 需求由测试用例验证 |
| `replaced_by` | 方案被后续方案替代 |
| `derived_from` | 经验来源于文档或故障 |
| `related_to` | 一般关联 |

### 14.3 关系来源

- Front Matter 显式声明。
- 用户手工创建。
- Git Commit 消息或 MR 描述解析。
- AI 建议。

AI 建议关系默认：

- `confirmed = false`
- 保存置信度和依据。
- 不参与高权重事实回答，直到人工确认。

---

## 15. SQLite 数据模型

当前 Schema 版本为 v24，建议知识库从 v25 开始分阶段迁移。

### 15.1 `knowledge_projects`

```sql
CREATE TABLE IF NOT EXISTS knowledge_projects (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    project_key         TEXT NOT NULL UNIQUE,
    name                TEXT NOT NULL,
    aliases_json        TEXT NOT NULL DEFAULT '[]',
    description         TEXT NOT NULL DEFAULT '',
    git_workspace_key   TEXT NOT NULL DEFAULT '',
    default_branch      TEXT NOT NULL DEFAULT '',
    enabled             INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at          TEXT DEFAULT NULL
);
```

### 15.2 `knowledge_releases`

```sql
CREATE TABLE IF NOT EXISTS knowledge_releases (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id    INTEGER NOT NULL,
    version       TEXT NOT NULL,
    tag_name      TEXT NOT NULL DEFAULT '',
    branch        TEXT NOT NULL DEFAULT '',
    commit_sha    TEXT NOT NULL DEFAULT '',
    description   TEXT NOT NULL DEFAULT '',
    released_at   TEXT DEFAULT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at    TEXT DEFAULT NULL,
    UNIQUE(project_id, version)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_releases_project
ON knowledge_releases(project_id, released_at);
```

### 15.3 `knowledge_sources`

```sql
CREATE TABLE IF NOT EXISTS knowledge_sources (
    id                        INTEGER PRIMARY KEY AUTOINCREMENT,
    source_key                TEXT NOT NULL UNIQUE,
    project_id                INTEGER DEFAULT NULL,
    source_type               TEXT NOT NULL,
    display_name              TEXT NOT NULL,
    root_path                 TEXT NOT NULL DEFAULT '',
    git_workspace_key         TEXT NOT NULL DEFAULT '',
    include_globs_json        TEXT NOT NULL DEFAULT '[]',
    exclude_globs_json        TEXT NOT NULL DEFAULT '[]',
    version_strategy          TEXT NOT NULL DEFAULT 'manual',
    sync_mode                 TEXT NOT NULL DEFAULT 'manual',
    allow_remote_embedding    INTEGER NOT NULL DEFAULT 0,
    enabled                   INTEGER NOT NULL DEFAULT 1,
    last_commit_sha           TEXT NOT NULL DEFAULT '',
    last_sync_status          TEXT NOT NULL DEFAULT 'never',
    last_synced_at            TEXT DEFAULT NULL,
    last_error                TEXT DEFAULT NULL,
    created_at                TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at                TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_knowledge_sources_project
ON knowledge_sources(project_id, enabled);
```

### 15.4 `knowledge_documents`

```sql
CREATE TABLE IF NOT EXISTS knowledge_documents (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    document_key        TEXT NOT NULL UNIQUE,
    project_id          INTEGER DEFAULT NULL,
    source_id           INTEGER DEFAULT NULL,
    doc_type            TEXT NOT NULL,
    title               TEXT NOT NULL,
    logical_path        TEXT NOT NULL DEFAULT '',
    status              TEXT NOT NULL DEFAULT 'active',
    sensitivity         TEXT NOT NULL DEFAULT 'internal',
    tags_json           TEXT NOT NULL DEFAULT '[]',
    latest_version_id   INTEGER DEFAULT NULL,
    allow_ai            INTEGER NOT NULL DEFAULT 1,
    allow_mcp           INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at          TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_knowledge_documents_project_type
ON knowledge_documents(project_id, doc_type, status);
```

### 15.5 `knowledge_document_versions`

```sql
CREATE TABLE IF NOT EXISTS knowledge_document_versions (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id         INTEGER NOT NULL,
    release_id          INTEGER DEFAULT NULL,
    version_label       TEXT NOT NULL DEFAULT '',
    git_branch          TEXT NOT NULL DEFAULT '',
    commit_sha          TEXT NOT NULL DEFAULT '',
    source_path         TEXT NOT NULL DEFAULT '',
    mime_type           TEXT NOT NULL DEFAULT 'text/markdown',
    content             TEXT NOT NULL,
    content_hash        TEXT NOT NULL,
    parsed_meta_json    TEXT NOT NULL DEFAULT '{}',
    token_estimate      INTEGER NOT NULL DEFAULT 0,
    valid               INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(document_id, version_label, content_hash)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_doc_versions_release
ON knowledge_document_versions(release_id, document_id);
```

### 15.6 `knowledge_chunks`

```sql
CREATE TABLE IF NOT EXISTS knowledge_chunks (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    document_version_id   INTEGER NOT NULL,
    chunk_index           INTEGER NOT NULL,
    heading_path          TEXT NOT NULL DEFAULT '',
    content               TEXT NOT NULL,
    content_hash          TEXT NOT NULL,
    location_json         TEXT NOT NULL DEFAULT '{}',
    token_estimate        INTEGER NOT NULL DEFAULT 0,
    embedding_status      TEXT NOT NULL DEFAULT 'pending',
    created_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(document_version_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_version
ON knowledge_chunks(document_version_id, chunk_index);
```

FTS 表的 tokenizer 需要在运行时能力探测后确定：

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_chunks_fts USING fts5(
    chunk_id UNINDEXED,
    title,
    heading_path,
    content,
    tokenize = 'unicode61'
);
```

### 15.7 `knowledge_embedding_profiles`

```sql
CREATE TABLE IF NOT EXISTS knowledge_embedding_profiles (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_key         TEXT NOT NULL UNIQUE,
    name                TEXT NOT NULL,
    mode                TEXT NOT NULL,
    provider_key        TEXT NOT NULL DEFAULT '',
    model               TEXT NOT NULL,
    model_revision      TEXT NOT NULL DEFAULT '',
    dimension           INTEGER NOT NULL DEFAULT 0,
    normalized          INTEGER NOT NULL DEFAULT 1,
    config_json         TEXT NOT NULL DEFAULT '{}',
    fingerprint         TEXT NOT NULL UNIQUE,
    status              TEXT NOT NULL DEFAULT 'draft',
    is_active           INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_knowledge_embedding_profiles_active
ON knowledge_embedding_profiles(is_active, status);

CREATE UNIQUE INDEX IF NOT EXISTS ux_knowledge_embedding_profiles_one_active
ON knowledge_embedding_profiles(is_active)
WHERE is_active = 1;
```

### 15.8 `knowledge_chunk_embeddings`

```sql
CREATE TABLE IF NOT EXISTS knowledge_chunk_embeddings (
    chunk_id       INTEGER NOT NULL,
    profile_id     INTEGER NOT NULL,
    dimension      INTEGER NOT NULL,
    vector_blob    BLOB NOT NULL,
    vector_norm    REAL NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    PRIMARY KEY(chunk_id, profile_id)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_embeddings_profile
ON knowledge_chunk_embeddings(profile_id, chunk_id);
```

### 15.9 `knowledge_relations`

```sql
CREATE TABLE IF NOT EXISTS knowledge_relations (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    from_type           TEXT NOT NULL,
    from_key            TEXT NOT NULL,
    relation_type       TEXT NOT NULL,
    to_type             TEXT NOT NULL,
    to_key              TEXT NOT NULL,
    evidence_json       TEXT NOT NULL DEFAULT '{}',
    confidence          REAL NOT NULL DEFAULT 1.0,
    confirmed           INTEGER NOT NULL DEFAULT 1,
    source              TEXT NOT NULL DEFAULT 'user',
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at          TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_knowledge_relations_from
ON knowledge_relations(from_type, from_key, relation_type);

CREATE INDEX IF NOT EXISTS idx_knowledge_relations_to
ON knowledge_relations(to_type, to_key, relation_type);
```

### 15.10 `knowledge_jobs`

```sql
CREATE TABLE IF NOT EXISTS knowledge_jobs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    job_key             TEXT NOT NULL UNIQUE,
    job_type            TEXT NOT NULL,
    source_id           INTEGER DEFAULT NULL,
    profile_id          INTEGER DEFAULT NULL,
    status              TEXT NOT NULL,
    progress_current    INTEGER NOT NULL DEFAULT 0,
    progress_total      INTEGER NOT NULL DEFAULT 0,
    message             TEXT NOT NULL DEFAULT '',
    error               TEXT DEFAULT NULL,
    started_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    finished_at         TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_knowledge_jobs_status
ON knowledge_jobs(status, started_at);
```

---

## 16. Rust 后端模块

建议新增：

```text
src-tauri/src/
├── commands/
│   └── knowledge.rs
├── services/
│   ├── knowledge.rs
│   ├── knowledge_parser.rs
│   ├── knowledge_retrieval.rs
│   ├── knowledge_rag.rs
│   ├── embedding.rs
│   ├── embedding_local.rs
│   └── embedding_remote.rs
├── database/
│   └── knowledge.rs
└── models/
    └── knowledge.rs
```

如果当前项目暂不拆分 `models/mod.rs` 和 `database/mod.rs`，首期可以继续在原文件增加实现，但知识库规模增长后应拆分，避免单文件继续膨胀。

### 16.1 Command 层

- IPC 参数校验。
- 启动异步任务。
- 查询任务状态。
- 调用 Service。
- `AppError` 转换为 Command 错误。
- 不直接解析文件、不直接写 SQL、不直接调用 Embedding API。

### 16.2 Service 层

- 项目和版本业务校验。
- 文件和 Git 来源同步。
- 文档解析、分块和内容哈希。
- Embedding Provider 选择。
- 蓝绿索引重建。
- 混合检索和排序。
- RAG Prompt 与引用组装。
- 远程发送安全策略。

### 16.3 Database 层

- 表 CRUD。
- 事务。
- FTS 同步。
- 向量 BLOB 编解码。
- Profile 激活事务。
- 关系查询。
- 任务状态落库。

### 16.4 后台任务

长耗时操作必须在 Tauri 异步运行时执行：

- Git 来源扫描。
- 文档解析。
- 模型下载。
- 本地批量向量生成。
- 远程批量向量生成。
- 全量重建。

任务进度通过：

- `get_knowledge_job_status` 轮询。
- 或 Tauri Event 推送 `knowledge-job-progress`。

---

## 17. Tauri Commands

### 17.1 项目与版本

| Command | 说明 |
| --- | --- |
| `list_knowledge_projects` | 查询知识项目 |
| `upsert_knowledge_project` | 新增或更新项目 |
| `delete_knowledge_project` | 软删除项目 |
| `list_knowledge_releases` | 查询项目版本 |
| `upsert_knowledge_release` | 新增或更新版本 |
| `discover_knowledge_releases` | 从 Git Tag 和分支建议版本 |

### 17.2 来源与同步

| Command | 说明 |
| --- | --- |
| `list_knowledge_sources` | 查询知识源 |
| `upsert_knowledge_source` | 新增或更新知识源 |
| `delete_knowledge_source` | 软删除知识源 |
| `start_knowledge_source_sync` | 启动来源同步 |
| `get_knowledge_job_status` | 查询任务状态 |
| `cancel_knowledge_job` | 取消尚可安全中断的任务 |

### 17.3 文档

| Command | 说明 |
| --- | --- |
| `list_knowledge_documents` | 分页查询文档 |
| `get_knowledge_document` | 获取逻辑文档及版本 |
| `upsert_knowledge_document` | 手工保存 Markdown 文档 |
| `delete_knowledge_document` | 软删除文档 |
| `list_knowledge_document_versions` | 查询历史版本 |
| `compare_knowledge_document_versions` | 比较两个文档版本 |

### 17.4 Embedding

| Command | 说明 |
| --- | --- |
| `get_embedding_settings` | 获取当前设置和活动 Profile |
| `test_embedding_profile` | 测试模型并读取实际维度 |
| `save_embedding_profile` | 保存草稿 Profile |
| `estimate_embedding_rebuild` | 估算片段数、时间和远程字符量 |
| `start_embedding_rebuild` | 启动蓝绿重建 |
| `activate_embedding_profile` | 仅允许完整性检查通过后激活 |
| `rollback_embedding_profile` | 回滚上一个可用 Profile |
| `list_embedding_profiles` | 查询 Profile 和索引状态 |

### 17.5 检索与 RAG

| Command | 说明 |
| --- | --- |
| `search_knowledge` | 混合检索 |
| `preview_knowledge_context` | 预览将注入大模型的片段 |
| `ask_knowledge` | 执行 RAG 问答 |
| `list_knowledge_relations` | 查询实体关系 |
| `upsert_knowledge_relation` | 保存人工确认关系 |
| `suggest_knowledge_relations` | AI 建议关系 |

所有 Command 同步提供浏览器 Dev API，以保持浏览器验收与桌面运行一致。

---

## 18. TypeScript 类型

建议新增：

```text
src/types/knowledge.ts
src/lib/api/knowledge.ts
```

核心类型：

```ts
export type EmbeddingMode = "local" | "remote";

export interface EmbeddingProfile {
  id: number;
  profileKey: string;
  name: string;
  mode: EmbeddingMode;
  providerKey: string;
  model: string;
  modelRevision: string;
  dimension: number;
  normalized: boolean;
  fingerprint: string;
  status: "draft" | "building" | "ready" | "failed" | "retired";
  isActive: boolean;
}

export interface KnowledgeSearchInput {
  query: string;
  projectKeys?: string[];
  releaseIds?: number[];
  documentTypes?: string[];
  tags?: string[];
  includeExpired?: boolean;
  limit?: number;
}

export interface KnowledgeCitation {
  citationKey: string;
  projectKey: string;
  projectName: string;
  releaseVersion: string;
  documentTitle: string;
  documentType: string;
  headingPath: string;
  sourcePath: string;
  commitSha: string;
  location: Record<string, unknown>;
  excerpt: string;
}

export interface KnowledgeAnswer {
  answer: string;
  citations: KnowledgeCitation[];
  evidenceGaps: string[];
  providerName: string;
  model: string;
  embeddingProfileKey: string;
}
```

所有 Rust Model 使用：

```rust
#[serde(rename_all = "camelCase")]
```

---

## 19. 前端页面设计

### 19.1 菜单

在 `AI / MCP` 下增加：

- `知识库`
- 路由：`/knowledge`
- 图标：`BookOpen`

现有 `Skill 管理` 保留，后续可重命名为 `Skill / Runbook`。经验库 UI 逐步迁移到知识库的“经验”类型，不破坏旧接口。

### 19.2 页面结构

```text
知识库
├── 问答
├── 文档
├── 项目与版本
├── 知识源
├── 关系
└── 索引任务
```

### 19.3 问答页

- 顶部项目、版本、文档类型筛选。
- 中间对话区。
- 右侧证据引用面板。
- 每条引用可打开原文、对应版本、Git 路径和 Commit。
- 显示当前 Embedding 模式、Profile 和问答模型。
- 支持“查看检索过程”：
  - 查询解析。
  - FTS 命中。
  - 向量命中。
  - 关系扩展。
  - 最终上下文。

### 19.4 文档页

三栏布局：

```text
项目/来源树 | 搜索与文档列表 | 文档预览/版本/引用
```

使用 Ant Design：

- `Tree`
- `Input.Search`
- `Select`
- `Table` 或 `List`
- `Tabs`
- `Drawer`
- `Tag`
- `Progress`
- `Empty`
- `Alert`

### 19.5 项目与版本页

- 项目列表。
- 关联 Git 工作区。
- 项目别名。
- 默认分支。
- 版本列表。
- 从 Git Tag 自动发现版本。
- 手工绑定版本、分支和 Commit。

### 19.6 知识源页

- 来源类型。
- 项目。
- 路径或 Git 工作区。
- Include/Exclude 规则。
- 版本识别策略。
- 是否允许远程 Embedding。
- 上次同步 Commit。
- 同步状态和错误。
- 手工同步。

### 19.7 索引任务页

- 同步、解析、分块、Embedding、重建任务。
- 当前进度。
- 失败批次。
- 重试。
- 可安全取消。
- 活动 Profile。
- 旧 Profile 回滚。

### 19.8 Embedding 设置

按照现有设置 Drawer/Tabs 规范增加“知识库与向量化”Tab。

切换模式时显示不同表单：

本地：

- 模型。
- 下载状态。
- 模型目录。
- CPU 线程和批大小。
- 测试按钮。

远程：

- AI Provider。
- Embedding 模型。
- 批大小、并发和超时。
- 远程发送总开关。
- 数据安全提示。
- 测试按钮。

保存后如需重建，弹出确认 Modal：

- 旧 Profile。
- 新 Profile。
- 片段数。
- 预计远程发送字符数。
- 旧索引是否继续服务。
- “保存并重建”按钮。

---

## 20. 与现有模块集成

### 20.1 AI Provider

- 复用 Provider 加密凭据。
- 新增 Embedding 能力与协议适配。
- Embedding 模型不覆盖聊天 `default_model`。
- `ask_ai_provider` 继续负责生成回答。
- `KnowledgeRagService` 负责检索和 Prompt 组装。

### 20.2 Skill

知识库问答调用 AI 时设置新作用域：

- `knowledge`

需要扩展 `AiSkillScope`，允许创建：

- 知识问答安全规则。
- 引用格式规则。
- 无证据拒答规则。
- 版本隔离规则。

### 20.3 经验库

- 现有 `ai_experiences` 暂不删除。
- 新增经验时同步创建 `doc_type=experience` 的知识文档。
- 旧经验可通过迁移任务导入知识库。
- 原 `recall_ai_experiences` 接口保持兼容。
- 后续由统一 `KnowledgeRetrievalService` 提供召回。

### 20.4 MCP

首批只读工具：

- `knowledge_projects_list`
- `knowledge_releases_list`
- `knowledge_search`
- `knowledge_document_detail`
- `knowledge_ask`
- `knowledge_citations_detail`

受控写工具：

- `knowledge_experience_upsert_controlled`
- `knowledge_relation_suggest`

写入 Git 源、修改项目设置、切换 Embedding Profile 和启动远程向量化必须受控并写审计。

### 20.5 Git 工作区

- 复用现有工作区登记、凭据、分支和 Commit 信息。
- 知识同步默认只读，不自动切换用户当前分支。
- 历史版本读取优先使用 `git show <commit>:<path>`，避免 checkout 干扰工作区。
- 不对用户未提交文件执行 stash、reset 或 checkout。

---

## 21. 安全与隐私

### 21.1 本地模式

- 文档内容不发送到 Embedding API。
- 模型下载地址和哈希写审计，不记录文档正文。
- 模型文件必须校验来源和完整性。
- 加载用户自定义模型前显示信任提示。

### 21.2 远程模式

远程向量化必须同时满足：

1. 系统设置允许远程 Embedding。
2. 知识源允许远程 Embedding。
3. 文档敏感级别允许发送。
4. 内容通过敏感信息检查。
5. Provider 已配置并通过测试。

默认禁止远程发送：

- 私钥。
- 密码和 Token。
- 数据库连接串。
- 凭据文件。
- 完整环境变量。
- 含密配置文件。
- 明确标记 `secret` 或 `restricted` 的文档。

### 21.3 权限

- 文件选择使用现有 `dialog:default`。
- 文件正文由 Rust Command 读取，避免给 WebView 广泛文件系统权限。
- 不因知识库导入而增加 `fs:default` 全局权限。
- 路径必须 canonicalize 后验证位于已授权来源根目录。
- 禁止 `..` 路径穿越。

### 21.4 审计

记录：

- 项目、版本和来源变更。
- 手工文档变更。
- Embedding Profile 新建、测试、激活和回滚。
- 远程向量化 Provider、模型、批次数和字符量。
- MCP 知识读取和受控写入。
- RAG 使用的项目、版本和引用 ID。

默认不记录：

- API Key 明文。
- 完整远程请求正文。
- 完整用户问题中的敏感内容。

---

## 22. 任务状态机

### 22.1 同步任务

```text
pending
  → scanning
  → parsing
  → chunking
  → indexing_fts
  → embedding
  → completed

任意阶段
  → failed
  → retrying
  → cancelled
```

### 22.2 Profile 状态

```text
draft
  → testing
  → building
  → ready
  → active
  → retired

testing / building
  → failed
```

### 22.3 崩溃恢复

- 任务状态落 SQLite。
- 应用启动时将无心跳的运行中任务标记为 `interrupted`。
- 用户可以从失败批次重试。
- 内容哈希已完成的片段不重复向量化。
- Profile 未通过完整性检查不得激活。

---

## 23. 测试方案

### 23.1 Database 层

- v24 → v25+ 迁移。
- 所有表和索引创建。
- 代码快照、文件、符号和关系唯一性。
- 同一 Commit 重复分析幂等。
- 文件重命名和失效快照处理。
- 软删除。
- 文档版本唯一性。
- FTS 插入、更新、删除同步。
- 向量 BLOB 编解码和维度检查。
- Profile 激活事务只有一个活动记录。
- 旧 Profile 回滚。

### 23.2 Service 层

- Git Commit、Tag 和工作树快照读取。
- 非 Git 本地目录边界和符号链接校验。
- 代码文件类型识别、大小限制和二进制跳过。
- AST/语法分析失败时的降级策略。
- 符号、导入、调用、IPC、API、SQL 和测试关系提取。
- Git Diff 增量分析和依赖失效传播。
- 代码片段按符号边界分块。
- 禅道版本和接口能力探测。
- 禅道分页、限流、超时、重试和增量游标。
- 禅道实体规范化、软删除和重复同步幂等。
- 需求—任务—Bug—测试关系导入。
- 模板文档确定性生成和内容哈希去重。
- 项目别名和版本解析。
- Git Tag、分支、Commit 版本识别。
- Markdown 标题分块。
- 代码块、SQL 和表格不被错误截断。
- 内容哈希增量同步。
- 本地 Embedding 批处理。
- 远程 Embedding 请求和错误解析。
- 远程发送权限矩阵。
- 不同 Profile 向量禁止混用。
- 蓝绿重建失败保留旧索引。
- FTS 与向量融合排序。
- 精确版本优先级。
- 证据不足拒答。

### 23.3 Command 层

- 参数为空和非法 ID。
- 未配置 Profile。
- Profile 测试失败。
- 未授权来源请求远程 Embedding。
- 任务创建和状态查询。
- 取消不可中断阶段的行为。

### 23.4 前端

- 模式切换表单。
- 本地和远程设置字段显隐。
- 测试连接。
- 重建确认。
- 任务进度。
- 搜索筛选。
- 引用打开。
- 错误统一使用 `getErrorMessage(error)`。

### 23.5 检索评测集

建立固定评测问题：

- 精确需求编号。
- 项目别名。
- 精确历史版本。
- 中文同义表达。
- 中英文混合类名和接口。
- 跨项目相似方案。
- 文档冲突。
- 缺少实现证据。
- 不同版本同名需求。

指标：

- Recall@K。
- MRR。
- 引用准确率。
- 版本串用率。
- 无证据拒答率。
- 查询 P50/P95。

### 23.6 运行验证

- `pnpm build`
- `cd src-tauri && cargo check`
- `cd src-tauri && cargo test`
- 前端页面强制使用 Codex 内置浏览器或 Chrome 验证。
- 桌面端验证本地模型下载、索引、模式切换和回滚。
- 远程模式使用受控测试文档验证，不发送真实敏感资料。

---

## 24. 实施步骤

### M1：数据模型与知识源

- [ ] 新增知识库 Rust/TypeScript Model。
- [ ] 新增 v25 迁移和知识项目、版本、来源、文档表。
- [ ] 新增知识项目和版本 Commands。
- [ ] 复用 Git 工作区读取 Commit、Tag 和文件内容。
- [ ] 实现目录和单文件来源。
- [ ] 实现内容哈希和增量同步。

### M2：解析、分块与 FTS

- [ ] 实现 Markdown/TXT/SQL/JSON/YAML 解析。
- [ ] 实现标题感知分块。
- [ ] 新增知识片段和 FTS 表。
- [ ] 实现 FTS 同步。
- [ ] 实现文档列表、详情和版本页。
- [ ] 建立检索基础评测集。

### M3：本地 Embedding

- [ ] 完成本地推理依赖技术 Spike。
- [ ] 引入 `fastembed` 或通过 Spike 确认的等价实现。
- [ ] 实现模型下载、离线导入和缓存。
- [ ] 实现 `LocalFastEmbedProvider`。
- [ ] 新增 Profile 和向量表。
- [ ] 实现批量向量化和进度。
- [ ] 实现本地余弦检索。

### M4：远程 Embedding

- [ ] 扩展 AI Provider Embedding 能力。
- [ ] 实现 OpenAI-compatible Embedding。
- [ ] 实现 Ollama-compatible Embedding。
- [ ] 实现来源级远程发送开关。
- [ ] 实现脱敏和安全检查。
- [ ] 实现超时、批量、并发、重试和审计。

### M5：设置切换与蓝绿索引

- [ ] 新增 Embedding 设置 UI。
- [ ] 实现 Profile 指纹。
- [ ] 实现重建估算。
- [ ] 实现蓝绿索引构建。
- [ ] 实现原子激活。
- [ ] 实现旧 Profile 回滚和清理。

### M6：混合检索与 RAG

- [ ] 实现查询分析。
- [ ] 实现项目和版本硬过滤。
- [ ] 实现 FTS 与向量并行召回。
- [ ] 实现融合排序。
- [ ] 实现引用生成。
- [ ] 实现上下文预览。
- [ ] 接入现有 AI Provider。
- [ ] 实现知识问答页面。

### M7：需求与实现关系

- [ ] 实现禅道连接和接口适配器。
- [ ] 实现产品、项目、执行与知识项目映射。
- [ ] 增量同步需求、任务、工时、Bug、测试用例和测试执行。
- [ ] 生成版本需求基线、追踪矩阵、任务总结和测试质量报告。
- [ ] 将生成文档接入分块、FTS、向量化和引用流程。
- [ ] 新增关系表和 CRUD。
- [ ] 支持 Front Matter 关系导入。
- [ ] 支持人工确认。
- [ ] 支持 Git Commit 证据。
- [ ] 实现需求—设计—实现—测试展示。
- [ ] 实现版本实现总结。

### M8：Git 与本地源码知识化

- [ ] 新增代码快照、文件、符号和关系数据模型。
- [ ] 复用 Git 工作区读取 Commit、Tag、树对象和 Diff，不切换工作区。
- [ ] 实现用户授权的非 Git 本地代码目录扫描。
- [ ] 实现默认排除、密钥检测、二进制和大文件保护。
- [ ] 实现首批语言解析器和通用降级解析器。
- [ ] 提取模块、符号、调用、IPC、API、SQL、数据表和测试关系。
- [ ] 实现按符号边界的 FTS 与向量分块。
- [ ] 生成模块、API、数据库、调用链和版本实现文档。
- [ ] 将代码证据与禅道需求、任务、测试及 Git Commit 关联。

### M9：经验库与 MCP

- [ ] 迁移现有经验。
- [ ] 保持旧 Command 和 MCP 兼容。
- [ ] 新增知识库 MCP 只读工具。
- [ ] 新增受控经验写入。
- [ ] 审计覆盖。

---

## 25. 文件规划

### 25.1 Rust

```text
src-tauri/src/models/knowledge.rs
src-tauri/src/database/knowledge.rs
src-tauri/src/services/knowledge.rs
src-tauri/src/services/knowledge_parser.rs
src-tauri/src/services/knowledge_retrieval.rs
src-tauri/src/services/knowledge_rag.rs
src-tauri/src/services/embedding.rs
src-tauri/src/services/embedding_local.rs
src-tauri/src/services/embedding_remote.rs
src-tauri/src/commands/knowledge.rs
```

需要修改：

```text
src-tauri/src/lib.rs
src-tauri/src/models/mod.rs
src-tauri/src/database/mod.rs
src-tauri/src/database/schema.rs
src-tauri/src/services/mod.rs
src-tauri/src/commands/mod.rs
src-tauri/src/services/ai_provider.rs
src-tauri/src/services/ai_skill.rs
src-tauri/src/dev_server/mod.rs
src-tauri/Cargo.toml
```

### 25.2 React

```text
src/pages/knowledge/index.tsx
src/pages/knowledge/components/KnowledgeAsk.tsx
src/pages/knowledge/components/KnowledgeDocuments.tsx
src/pages/knowledge/components/KnowledgeProjects.tsx
src/pages/knowledge/components/KnowledgeSources.tsx
src/pages/knowledge/components/KnowledgeRelations.tsx
src/pages/knowledge/components/KnowledgeJobs.tsx
src/pages/knowledge/components/EmbeddingSettings.tsx
src/types/knowledge.ts
src/lib/api/knowledge.ts
src/store/knowledge.ts
```

需要修改：

```text
src/Router.tsx
src/components/layout/Sidebar.tsx
src/pages/prototype/index.tsx
src/types/index.ts
src/lib/api/index.ts
src/store/index.ts
```

---

## 26. 验收标准

### 26.1 知识管理

- 可以创建多个知识项目并关联 Git 工作区。
- 可以配置禅道连接，将禅道产品、项目和执行映射到知识项目及发布版本。
- 可以增量同步禅道需求、任务执行、Bug、测试用例和测试结果。
- 可以预览并生成带禅道实体引用的项目文档。
- 可以配置项目别名、默认分支和发布版本。
- 可以导入 Markdown、TXT、SQL、JSON、YAML。
- 可以查看文档历史版本、来源路径和 Commit。
- Git 文档变化后只更新受影响文档和片段。
- 可以选择 Git Commit、Tag、分支或当前工作树建立代码快照。
- 可以授权非 Git 本地代码目录并配置包含/排除规则。
- 可以查看代码文件、符号、调用关系及自动生成的代码文档。

### 26.2 向量化

- 本地模式能下载或导入模型并生成向量。
- 远程模式能复用 AI Provider 密钥并生成向量。
- 设置页面可以切换本地和远程模式。
- 切换 Profile 会提示并启动重建。
- 重建失败时旧索引仍可检索。
- 不同模型的向量不会混用。
- 未允许远程发送的来源不会调用远程 Embedding。

### 26.3 检索

- 可以按项目、版本、文档类型和标签过滤。
- 可以按 Commit、Tag、工作树快照、语言、模块、文件和符号类型过滤。
- 精确需求编号、接口、字段和 Commit 可通过 FTS 命中。
- 类名、函数名、Command、路由、接口路径、表名和 SQL 字段可精确命中。
- 语义相近但用词不同的问题可通过向量命中。
- 指定历史版本时不会混入后续版本作为当时事实。
- 搜索结果显示文档、章节、版本、路径和 Commit。

### 26.4 RAG 问答

对问题：

> 我想了解某某项目当初在某个版本的需求和具体实现方案。

系统必须：

- 正确识别项目和版本，或要求用户选择。
- 召回该版本的需求、设计、接口、数据库和代码证据。
- 按结构生成回答。
- 为重要结论提供引用。
- 显示证据缺口。
- 不使用最新版本内容冒充历史事实。
- 可以回答需求在禅道中的状态、任务执行情况和测试结论，并引用实体 ID 与同步时间。
- 无法确认需求与 Commit 关系时明确显示“未建立代码证据”，不得自动宣称已实现。
- 代码结论引用具体仓库、Commit/快照、文件路径和行号范围。
- 工作树未提交代码必须明确标记“本地未提交快照”，不得冒充发布版本实现。

### 26.5 安全

- 前端无法读取 AI Provider 密钥明文。
- 远程 Embedding 请求不记录原始文档正文。
- 私钥、密码、Token 和受限文档默认不能发送远程。
- 文件读取限制在用户明确配置的知识源。
- MCP 写操作受控并写审计。

### 26.6 质量

- Rust 核心逻辑具备单元测试。
- 数据库迁移和 Profile 切换具备集成测试。
- 前端模式切换和任务状态具备组件测试。
- 浏览器和桌面端关键流程验收通过。
- 所有新增或修改源码为 UTF-8 无 BOM。

---

## 27. 风险与对策

| 风险 | 对策 |
| --- | --- |
| 本地模型导致包体或下载过大 | 模型独立下载、离线导入、按需安装 |
| ONNX Runtime 跨平台构建复杂 | 在 M3 前完成 Windows/macOS/Linux Spike |
| 远程 Embedding 泄露内部文档 | 总开关 + 来源开关 + 敏感级别 + 脱敏 |
| 模型切换造成索引不可用 | 蓝绿构建，成功后原子切换 |
| 不同模型维度相同但语义空间不同 | 使用完整 Profile 指纹，禁止跨 Profile 比较 |
| SQLite 向量扫描变慢 | 先硬过滤，达到阈值后引入本地 HNSW |
| 中文 FTS 效果不足 | 运行时测试 trigram，FTS 与向量互补 |
| 文档版本标注不完整 | 多级版本识别，无法识别时标记 unversioned |
| AI 混淆历史和当前事实 | 项目/版本硬过滤，Prompt 约束和引用校验 |
| 需求与代码没有显式关系 | Front Matter、人工关联、AI 建议后确认 |
| 每个客户端重复远程向量化产生费用 | 增量哈希、批量请求、后续可选集中服务 |
| Git 工作区有未提交修改 | 历史读取使用 `git show`，不 checkout、不 stash |
| 禅道版本和 API 路径差异 | 连接时探测版本和能力，通过适配器隔离，不写死单一路径 |
| 禅道数据与 Git 版本口径不一致 | 显式项目/版本映射，保留来源时间并提示冲突 |
| 需求与 Commit 无编号关联 | 解析 Commit 约定、支持人工确认，AI 关系默认低权重 |
| 禅道评论和附件包含敏感信息 | 默认不拉附件正文，来源级范围控制和远程发送审查 |
| 源码包含密钥、证书或生产配置 | 默认排除 + 内容检测 + 阻断远程发送 + 审计 |
| 依赖目录和生成代码导致索引膨胀 | 默认排除 vendor/node_modules/target/dist/build 和生成文件 |
| 代码语法解析器覆盖不全 | AST 解析器分语言启用，失败时降级为符号/文本分块并标记质量 |
| 本地未提交代码被当作版本事实 | 独立工作树快照，强制显示 dirty 状态和基线 Commit |
| 调用关系静态分析不完整 | 保存关系类型、解析方法和置信度，动态调用明确标记未知 |

---

## 28. 技术 Spike 清单

正式编码前必须完成：

1. `fastembed` 在 Windows、macOS、Linux 的编译和模型加载。
2. `multilingual-e5-small` 与 `bge-small-zh-v1.5` 的中文、代码标识符评测。
3. 模型下载、内部镜像和离线导入。
4. 10 万、20 万片段本地余弦检索性能。
5. FTS5 `trigram` tokenizer 的运行环境兼容性。
6. DOCX 和 PDF 文本提取质量。
7. OpenAI-compatible 与 Ollama Embedding 响应适配。
8. 远程 Provider 批大小和限流策略。
9. 蓝绿重建磁盘占用和回滚。
10. 不同模型、维度和 Profile 的一致性保护。
11. 目标禅道实例的版本、认证方式、分页、限流和实体字段核验。
12. 禅道需求变更、任务工时、测试单和测试结果的真实返回结构。
13. 禅道实体与 Git Commit 编号关联规则的历史数据命中率。
14. Java、TypeScript/JavaScript、Vue、Rust、SQL 等首批语言的语法解析质量。
15. 10 万文件和百万符号级增量索引的时间、内存和 SQLite 查询性能。
16. Git 重命名、合并 Commit、子模块、LFS、稀疏检出和 dirty 工作树读取。
17. 源码敏感信息检测的误报、漏报和远程发送阻断。

---

## 29. 推荐实施顺序

推荐顺序：

```text
项目/版本/文档事实模型
  → 文档解析和 FTS
  → 本地 Embedding
  → 远程 Embedding
  → Profile 设置与蓝绿重建
  → 混合检索
  → RAG 问答与引用
  → 需求—实现关系
  → 跨项目经验分析
```

不能先做一个简单聊天页面再补数据模型。项目、版本、文档版本和引用如果设计错误，后续向量和问答都需要重建。

---

## 30. 最终推荐

采用以下总体方案：

- Git 作为文档、历史源码与 Commit 的工程事实来源，禅道作为需求、任务、Bug 和测试的过程事实来源；用户授权的本地源码作为本机补充分析来源。
- Tauri SSH 本地 SQLite 保存元数据、全文索引和向量。
- FTS5 与向量检索并行，元数据和版本过滤优先。
- 本地 Embedding 与远程 Embedding 通过统一 Provider 和系统设置切换。
- 本地默认模型优先评估 `multilingual-e5-small`。
- 远程模式复用现有 AI Provider 密钥和网络能力，但使用独立 Embedding 模型配置。
- Profile 变化使用蓝绿全量重建，成功后再激活。
- 首期不部署服务器向量数据库。
- 数据规模或团队治理需求达到阈值后，再演进集中式知识服务。

该方案能够实现：

> 通过知识库大模型问答，查询某项目在指定历史版本下的需求、任务执行、设计、接口、数据库、代码实现和测试结果，并返回可核查的禅道实体、文档章节、Git 路径和 Commit 证据。

前提是团队允许读取相应禅道项目，并将设计、实现关系或相应 Git 证据纳入知识源。系统可以辅助建立关系，但不能从不存在的历史资料中生成可信事实。

---

## 31. 禅道数据接入与项目文档生成详细方案

### 31.1 建设目标与边界

禅道集成的目标不是把禅道页面全文复制到知识库，而是把研发过程中的结构化事实转换为可追溯、可版本化、可检索的项目知识。

完整链路：

```text
禅道连接
  → 版本与接口能力探测
  → 产品/项目/执行映射
  → 需求/任务/Bug/测试增量同步
  → 实体规范化与关系构建
  → 关联 Git Commit、代码和发布版本
  → 确定性模板生成项目 Markdown
  → 文档解析、分块、FTS5 与向量化
  → 混合检索、关系扩展与 RAG 问答
```

首期边界：

- 只读禅道，不在知识库页面反向修改需求、任务、Bug 或测试结果。
- 默认同步实体正文和必要的关系字段，不下载附件正文。
- 评论、操作日志和附件元数据由连接配置决定是否同步。
- 不直接连接禅道数据库，优先通过官方 REST API 读取。
- 不把某个固定 `/api.php/v1/...` 路径视为所有禅道版本的通用标准。
- 不把 AI 推测出的需求—Commit 关系当成已确认事实。

### 31.2 禅道版本与 API 兼容策略

不同禅道版本、部署模式和授权版本可能存在以下差异：

- REST API 是否启用。
- 登录、Token 或 Session 认证方式。
- API 根路径和版本前缀。
- “项目”“执行”“迭代”等实体命名和层级。
- 需求类型、测试任务、测试单、构建和发布接口是否可用。
- 分页参数、时间字段、删除标记和响应包装结构。

因此连接配置必须包含：

| 字段 | 说明 |
| --- | --- |
| `base_url` | 禅道实例根地址 |
| `api_version` | 自动探测或人工选择的 API 版本 |
| `auth_mode` | Token、Session 或目标实例支持的认证模式 |
| `endpoint_profile` | 与禅道版本匹配的接口适配器 |
| `credential_key` | 安全凭据库中的凭据引用 |
| `tls_verify` | 是否校验证书，生产环境默认开启 |
| `request_timeout_seconds` | 请求超时 |
| `page_size` | 单页数量 |
| `rate_limit_per_second` | 客户端限速 |

连接测试按以下顺序执行：

1. 校验 URL 协议和主机范围。
2. 从安全凭据库取出凭据，仅在 Rust 内存中使用。
3. 请求版本或用户信息端点。
4. 探测产品、项目、执行、需求、任务、Bug 和测试相关能力。
5. 记录字段样例的结构摘要，不记录敏感正文。
6. 选择兼容的 `ZentaoApiAdapter`。
7. 返回能力矩阵，不向前端返回 Token 或 Session。

适配器接口建议：

```rust
#[async_trait]
pub trait ZentaoApiAdapter: Send + Sync {
    async fn probe_capabilities(&self) -> Result<ZentaoCapabilities, AppError>;
    async fn list_products(&self, page: PageRequest) -> Result<Page<ZentaoProduct>, AppError>;
    async fn list_projects(&self, page: PageRequest) -> Result<Page<ZentaoProject>, AppError>;
    async fn list_executions(&self, scope: &RemoteScope) -> Result<Vec<ZentaoExecution>, AppError>;
    async fn list_stories(&self, query: &ZentaoSyncQuery) -> Result<Page<ZentaoStory>, AppError>;
    async fn list_tasks(&self, query: &ZentaoSyncQuery) -> Result<Page<ZentaoTask>, AppError>;
    async fn list_bugs(&self, query: &ZentaoSyncQuery) -> Result<Page<ZentaoBug>, AppError>;
    async fn list_test_cases(&self, query: &ZentaoSyncQuery) -> Result<Page<ZentaoCase>, AppError>;
    async fn list_test_runs(&self, query: &ZentaoSyncQuery) -> Result<Page<ZentaoTestRun>, AppError>;
}
```

具体端点和返回字段必须在编码前对目标禅道实例做技术 Spike，并以真实字段说明为准。

### 31.3 同步实体范围

| 实体 | 主要内容 | 首期 | 用途 |
| --- | --- | --- | --- |
| 产品 | 名称、状态、负责人 | 必做 | 需求归属和映射 |
| 项目 | 名称、状态、周期、负责人 | 必做 | 知识项目映射 |
| 执行/迭代 | 名称、周期、状态、所属项目 | 必做 | 版本和迭代范围 |
| 需求 | 标题、正文、验收标准、状态、优先级、版本 | 必做 | 需求事实 |
| 需求变更 | 变更内容、版本号、操作者、时间 | 必做 | 历史版本追溯 |
| 任务 | 标题、描述、负责人、状态、预计/消耗/剩余工时 | 必做 | 实现过程 |
| 任务工时 | 日期、执行人、消耗、备注 | 可配置 | 执行分析 |
| Bug | 标题、步骤、严重级别、状态、解决方案、关联需求 | 必做 | 质量和遗留问题 |
| 测试用例 | 前置条件、步骤、预期结果、关联需求 | 必做 | 验收设计 |
| 测试任务/测试单 | 测试范围、负责人、构建、状态 | 必做 | 测试批次 |
| 测试执行 | 用例、结果、执行人、时间、备注 | 必做 | 测试证据 |
| 构建 | 名称、关联需求/Bug、时间 | 可配置 | 发布候选关联 |
| 发布 | 名称、版本、发布日期、范围 | 可配置 | 版本事实 |
| 评论 | 评论内容、作者、时间 | 默认关闭 | 决策背景补充 |
| 附件 | 文件名、类型、大小、下载地址 | 仅元数据 | 证据指引 |

实体进入知识库前统一转换为规范模型：

```text
source_system = zentao
entity_type   = story | task | bug | case | test_run | ...
external_id   = 禅道实体 ID
external_key  = zentao:{connection_key}:{entity_type}:{external_id}
project_id    = 映射后的知识项目
release_id    = 映射后的发布版本，可为空
title         = 规范化标题
body_markdown = 从结构化字段确定性生成的 Markdown
status        = 原始状态和统一状态
updated_at    = 禅道更新时间
content_hash  = 规范化内容哈希
```

### 31.4 项目、执行与版本映射

禅道层级不能直接等同于知识库层级，必须显式配置映射。

```text
禅道产品 ─┐
禅道项目 ─┼─→ knowledge_project
禅道执行 ─┘

禅道执行 / 计划 / 发布 / 构建
            └─→ knowledge_release
```

每条项目映射至少包含：

- 禅道连接。
- 远程产品 ID，可为空。
- 远程项目 ID。
- 远程执行 ID 列表。
- 本地 `knowledge_project_id`。
- 版本映射策略。
- 是否同步需求、任务、Bug 和测试。
- 同步起始时间。
- 评论、工时和附件元数据开关。
- 是否允许进入远程 Embedding。
- 是否允许进入远程大模型分析。

版本映射策略：

1. 手工把禅道执行或发布绑定到 `knowledge_release`。
2. 按禅道发布名称与 Git Tag 精确匹配。
3. 按配置的正则规则提取版本号。
4. 无法识别时归入 `unversioned`，等待人工确认。

严禁把所有未映射数据默认归入“最新版本”。

### 31.5 增量同步和一致性

每类实体独立维护同步游标：

```text
connection + mapping + entity_type
  → last_updated_at
  → last_external_id
  → last_page
  → last_success_at
  → last_full_sync_at
```

同步流程：

1. 创建 `zentao_sync` 后台任务。
2. 获取映射和实体范围。
3. 按实体类型、更新时间和 ID 分页读取。
4. 将远程响应转换为规范实体。
5. 在事务中 Upsert 实体和显式关系。
6. 内容哈希变化时才生成新实体快照。
7. 更新对应的逻辑文档和文档版本。
8. 只对变化文档重新分块、更新 FTS 和生成向量。
9. 所有分页成功后提交该实体类型的新游标。
10. 生成或更新项目汇总文档。

一致性规则：

- 远程实体使用 `connection_id + entity_type + external_id` 唯一约束。
- 同一更新时间重复同步必须幂等。
- 一页失败不推进整个实体类型的成功游标。
- 远程删除或不可见不能立即物理删除，先标记 `missing`。
- 连续两次全量校验仍不存在时标记 `deleted`，保留历史快照。
- 禅道更新时间回退时以内容哈希识别变化。
- 需求变更版本必须保留，不能只保存需求最新正文。
- 同步中断后可从最后完成的实体类型和分页断点恢复。

同步状态机：

```text
pending
  → probing
  → fetching_products
  → fetching_projects
  → fetching_stories
  → fetching_tasks
  → fetching_bugs
  → fetching_test_cases
  → fetching_test_runs
  → building_relations
  → generating_documents
  → chunking
  → indexing_fts
  → embedding
  → completed

任意阶段 → failed | interrupted | cancelled
```

### 31.6 研发关系构建

禅道同步后应建立以下高可信关系：

| 来源实体 | 关系 | 目标实体 |
| --- | --- | --- |
| 需求 | `belongs_to` | 项目/产品 |
| 需求 | `introduced_in` / `changed_in` | 发布版本 |
| 任务 | `implements` | 需求 |
| Bug | `affects` | 需求/任务/版本 |
| 测试用例 | `verifies` | 需求 |
| 测试执行 | `executes` | 测试用例 |
| 测试执行 | `verified_in` | 构建/版本 |
| 构建 | `contains` | 需求/Bug |
| 发布 | `releases` | 构建/版本 |

禅道与 Git 的关系按可信度分级：

1. 禅道实体显式填写 Commit、MR 或代码链接：自动确认。
2. Git Commit 消息包含约定编号，例如 `story#123`、`task#456`：自动建立，保留匹配证据。
3. 文档 Front Matter 显式声明：自动确认。
4. 用户手工确认：确认事实。
5. AI 根据语义建议：`confirmed=false`，不得作为高权重实现证据。

推荐 Commit 约定：

```text
feat(dispatch): 支持跨区域派车

Zentao-Story: 1234
Zentao-Task: 5678
Release: v1.6.0
```

当系统无法建立需求—代码关系时，生成文档必须写明：

> 禅道需求已同步，但未发现经过确认的 Commit 或代码实现证据。

### 31.7 自动生成的项目文档

系统为每个项目和版本生成以下逻辑文档：

| 文档类型 | 建议路径 | 内容 |
| --- | --- | --- |
| 项目概览 | `generated/zentao/project-overview.md` | 项目范围、周期、成员、总体状态 |
| 版本需求基线 | `generated/zentao/releases/{version}/requirements.md` | 版本需求列表、优先级、状态和变更 |
| 需求追踪矩阵 | `generated/zentao/releases/{version}/traceability.md` | 需求—任务—Commit—测试关系 |
| 任务执行总结 | `generated/zentao/releases/{version}/task-execution.md` | 任务完成度、工时和阻塞 |
| 测试质量报告 | `generated/zentao/releases/{version}/test-quality.md` | 用例覆盖、执行结果和 Bug |
| 版本变更记录 | `generated/zentao/releases/{version}/change-log.md` | 需求变更、解决 Bug 和发布范围 |
| 风险与遗留问题 | `generated/zentao/releases/{version}/open-risks.md` | 未完成任务、未关闭 Bug、缺失测试 |
| 单需求证据报告 | `generated/zentao/stories/{id}.md` | 需求详情和完整证据链 |

文档生成采用“两阶段”：

#### 阶段一：确定性事实文档

- 使用固定模板和结构化字段。
- 排序规则固定，保证相同输入产生相同内容。
- 每条事实包含禅道实体 ID、实体 URL、更新时间和同步时间。
- 指标由代码计算，不由大模型计算。
- 生成结果计算 `content_hash`，无变化不创建新版本。
- 该阶段即使未配置大模型也能运行。

#### 阶段二：可选 AI 摘要

- 只基于阶段一事实文档生成摘要、风险说明和跨项目对比。
- Prompt 中明确禁止补充未出现的项目事实。
- AI 输出标记 `generated_by_ai=true` 和所用 Provider/模型。
- AI 摘要与事实正文分区保存。
- 每项结论必须引用阶段一实体或文档片段。
- 引用校验失败的句子删除或标记为“待核实”。

需求追踪矩阵示例：

```markdown
# v1.6.0 需求实现追踪矩阵

| 需求 | 状态 | 实现任务 | Commit | 测试用例 | 最近结果 | 证据状态 |
| --- | --- | --- | --- | --- | --- | --- |
| Story #1234 跨区域派车 | 已关闭 | Task #5678 | abc123 | Case #901 | 通过 | 完整 |
| Story #1240 调度备注 | 已激活 | Task #5682 | 未关联 | Case #910 | 未执行 | 缺少代码与测试证据 |

数据快照：2026-07-29 10:30:00
来源：禅道连接 `zentao-prod`，项目 #20，执行 #35
```

### 31.8 生成文档进入知识库的规则

生成文档与普通 Git 文档使用同一数据模型：

- `source_type = zentao`
- `doc_type = project_overview | release_requirement | traceability | task_report | test_report | risk_report`
- `document_key` 使用稳定业务键。
- `version_label` 使用禅道版本、发布版本或快照时间。
- `parsed_meta_json` 保存实体 ID 列表、生成模板版本和同步批次。
- `content_hash` 用于增量去重。
- `release_id` 必须来自明确映射。

生成后执行：

```text
Markdown
  → 标题感知分块
  → FTS5 更新
  → 活动 Embedding Profile 向量化
  → 建立 chunk → zentao entity 证据关系
  → 可供 search_knowledge / ask_knowledge 使用
```

如果配置方案 A：

- 在本机使用活动本地模型生成向量。
- 禅道正文不离开客户端。

如果配置方案 B：

- 先检查禅道来源的 `allow_remote_embedding`。
- 再进行密钥、个人信息和敏感字段检查。
- 仅发送分块后的必要文本，不发送完整原始 JSON。
- 向量仍保存到本地 SQLite。

切换 A/B Profile 时，禅道生成文档和其他知识文档一起参与蓝绿索引重建。

### 31.9 SQLite 数据模型

#### 31.9.1 `zentao_connections`

```sql
CREATE TABLE IF NOT EXISTS zentao_connections (
    id                        INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_key            TEXT NOT NULL UNIQUE,
    name                      TEXT NOT NULL,
    base_url                  TEXT NOT NULL,
    api_version               TEXT NOT NULL DEFAULT 'auto',
    auth_mode                 TEXT NOT NULL DEFAULT 'auto',
    endpoint_profile          TEXT NOT NULL DEFAULT '',
    credential_key            TEXT NOT NULL,
    tls_verify                INTEGER NOT NULL DEFAULT 1,
    request_timeout_seconds   INTEGER NOT NULL DEFAULT 30,
    page_size                 INTEGER NOT NULL DEFAULT 100,
    rate_limit_per_second     REAL NOT NULL DEFAULT 5,
    capabilities_json         TEXT NOT NULL DEFAULT '{}',
    enabled                   INTEGER NOT NULL DEFAULT 1,
    last_test_status          TEXT NOT NULL DEFAULT 'never',
    last_tested_at            TEXT DEFAULT NULL,
    last_error                TEXT DEFAULT NULL,
    created_at                TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at                TEXT DEFAULT NULL
);
```

`credential_key` 只引用安全凭据库，不保存用户名密码、Token 或 Session 明文。

#### 31.9.2 `zentao_project_mappings`

```sql
CREATE TABLE IF NOT EXISTS zentao_project_mappings (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id               INTEGER NOT NULL,
    knowledge_project_id        INTEGER NOT NULL,
    remote_product_id           TEXT NOT NULL DEFAULT '',
    remote_project_id           TEXT NOT NULL,
    remote_execution_ids_json   TEXT NOT NULL DEFAULT '[]',
    release_mapping_json        TEXT NOT NULL DEFAULT '{}',
    sync_scope_json              TEXT NOT NULL DEFAULT '{}',
    sync_since                  TEXT DEFAULT NULL,
    include_comments            INTEGER NOT NULL DEFAULT 0,
    include_worklogs            INTEGER NOT NULL DEFAULT 1,
    include_attachment_metadata INTEGER NOT NULL DEFAULT 1,
    allow_remote_embedding      INTEGER NOT NULL DEFAULT 0,
    allow_remote_ai             INTEGER NOT NULL DEFAULT 0,
    enabled                     INTEGER NOT NULL DEFAULT 1,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at                  TEXT DEFAULT NULL,
    UNIQUE(connection_id, knowledge_project_id, remote_project_id)
);

CREATE INDEX IF NOT EXISTS idx_zentao_mappings_project
ON zentao_project_mappings(knowledge_project_id, enabled);
```

#### 31.9.3 `zentao_sync_cursors`

```sql
CREATE TABLE IF NOT EXISTS zentao_sync_cursors (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    mapping_id          INTEGER NOT NULL,
    entity_type         TEXT NOT NULL,
    last_updated_at     TEXT NOT NULL DEFAULT '',
    last_external_id    TEXT NOT NULL DEFAULT '',
    checkpoint_json     TEXT NOT NULL DEFAULT '{}',
    last_success_at     TEXT DEFAULT NULL,
    last_full_sync_at   TEXT DEFAULT NULL,
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(mapping_id, entity_type)
);
```

#### 31.9.4 `zentao_entities`

```sql
CREATE TABLE IF NOT EXISTS zentao_entities (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id       INTEGER NOT NULL,
    mapping_id          INTEGER NOT NULL,
    knowledge_project_id INTEGER NOT NULL,
    release_id          INTEGER DEFAULT NULL,
    entity_type         TEXT NOT NULL,
    external_id         TEXT NOT NULL,
    external_key        TEXT NOT NULL UNIQUE,
    title               TEXT NOT NULL DEFAULT '',
    body_markdown       TEXT NOT NULL DEFAULT '',
    original_status     TEXT NOT NULL DEFAULT '',
    normalized_status   TEXT NOT NULL DEFAULT '',
    assignee_external_id TEXT NOT NULL DEFAULT '',
    parent_external_key TEXT NOT NULL DEFAULT '',
    remote_url          TEXT NOT NULL DEFAULT '',
    content_hash        TEXT NOT NULL,
    raw_json_hash       TEXT NOT NULL DEFAULT '',
    raw_snapshot_json   TEXT DEFAULT NULL,
    source_created_at   TEXT DEFAULT NULL,
    source_updated_at   TEXT DEFAULT NULL,
    first_synced_at     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    last_synced_at      TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    missing_count       INTEGER NOT NULL DEFAULT 0,
    status              TEXT NOT NULL DEFAULT 'active',
    deleted_at          TEXT DEFAULT NULL,
    UNIQUE(connection_id, entity_type, external_id)
);

CREATE INDEX IF NOT EXISTS idx_zentao_entities_project_type
ON zentao_entities(knowledge_project_id, release_id, entity_type, normalized_status);

CREATE INDEX IF NOT EXISTS idx_zentao_entities_updated
ON zentao_entities(mapping_id, entity_type, source_updated_at);
```

`raw_snapshot_json` 是否保存由数据治理设置决定。保存时应过滤凭据、Cookie 和不需要的个人信息；也可只保存 `raw_json_hash` 与必要字段，降低敏感数据落盘范围。

#### 31.9.5 `zentao_entity_relations`

```sql
CREATE TABLE IF NOT EXISTS zentao_entity_relations (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    from_external_key   TEXT NOT NULL,
    relation_type       TEXT NOT NULL,
    to_external_key     TEXT NOT NULL,
    evidence_json       TEXT NOT NULL DEFAULT '{}',
    source              TEXT NOT NULL DEFAULT 'zentao',
    confidence          REAL NOT NULL DEFAULT 1.0,
    confirmed           INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at          TEXT DEFAULT NULL,
    UNIQUE(from_external_key, relation_type, to_external_key, source)
);

CREATE INDEX IF NOT EXISTS idx_zentao_relations_from
ON zentao_entity_relations(from_external_key, relation_type);

CREATE INDEX IF NOT EXISTS idx_zentao_relations_to
ON zentao_entity_relations(to_external_key, relation_type);
```

确认后的关系同步写入通用 `knowledge_relations`，供统一检索服务使用。

#### 31.9.6 `knowledge_generation_runs`

```sql
CREATE TABLE IF NOT EXISTS knowledge_generation_runs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    run_key             TEXT NOT NULL UNIQUE,
    project_id          INTEGER NOT NULL,
    release_id          INTEGER DEFAULT NULL,
    source_id           INTEGER DEFAULT NULL,
    sync_job_id         INTEGER DEFAULT NULL,
    template_version    TEXT NOT NULL,
    document_types_json TEXT NOT NULL DEFAULT '[]',
    input_hash          TEXT NOT NULL,
    status              TEXT NOT NULL,
    generated_count     INTEGER NOT NULL DEFAULT 0,
    skipped_count       INTEGER NOT NULL DEFAULT 0,
    ai_summary_enabled  INTEGER NOT NULL DEFAULT 0,
    ai_provider_key     TEXT NOT NULL DEFAULT '',
    ai_model            TEXT NOT NULL DEFAULT '',
    error               TEXT DEFAULT NULL,
    started_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    finished_at         TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_knowledge_generation_runs_project
ON knowledge_generation_runs(project_id, release_id, started_at);
```

### 31.10 Rust 模块与职责

建议新增：

```text
src-tauri/src/
├── commands/
│   └── zentao_knowledge.rs
├── services/
│   ├── zentao/
│   │   ├── mod.rs
│   │   ├── client.rs
│   │   ├── adapter.rs
│   │   ├── adapter_profiles/
│   │   ├── normalizer.rs
│   │   ├── sync.rs
│   │   ├── relation_builder.rs
│   │   └── document_generator.rs
│   └── knowledge_generation.rs
├── database/
│   └── zentao_knowledge.rs
└── models/
    └── zentao_knowledge.rs
```

职责：

| 层 | 职责 |
| --- | --- |
| Command | 参数校验、启动任务、查询状态、返回脱敏 DTO |
| Service | API 适配、分页同步、规范化、关系构建、模板文档生成 |
| Database | 连接配置、映射、游标、实体、关系和生成记录 CRUD |
| React | 连接配置、映射操作、同步状态、文档预览和差异展示 |

所有禅道 HTTP 请求使用 Rust `reqwest` 发起。前端不得直接请求禅道，也不得接触凭据明文。

### 31.11 Tauri Commands

| Command | 说明 | 关键输入 | 关键输出 |
| --- | --- | --- | --- |
| `list_zentao_connections` | 查询脱敏连接列表 | 无 | 连接状态列表 |
| `upsert_zentao_connection` | 保存连接与凭据引用 | 连接配置 | 连接 ID |
| `test_zentao_connection` | 探测版本、认证和能力 | 连接 ID/草稿配置 | 能力矩阵 |
| `delete_zentao_connection` | 软删除连接 | 连接 ID | 是否成功 |
| `list_zentao_remote_projects` | 读取产品、项目和执行 | 连接 ID | 远程树 |
| `list_zentao_project_mappings` | 查询项目映射 | 项目 ID | 映射列表 |
| `upsert_zentao_project_mapping` | 保存范围与权限 | 映射 DTO | 映射 ID |
| `delete_zentao_project_mapping` | 软删除映射 | 映射 ID | 是否成功 |
| `estimate_zentao_sync` | 估算实体范围和远程 AI 风险 | 映射 ID | 估算结果 |
| `start_zentao_sync` | 启动增量或全量同步 | 映射 ID、模式 | Job ID |
| `get_zentao_sync_status` | 查询各实体同步进度 | Job ID | 阶段与计数 |
| `list_zentao_entities` | 查询规范化实体 | 筛选条件 | 分页实体 |
| `get_zentao_entity` | 查询实体、快照和关系 | 实体 Key | 详情 |
| `preview_zentao_generated_documents` | 预览确定性文档 | 项目/版本/类型 | Markdown 列表 |
| `generate_zentao_project_documents` | 生成并入库项目文档 | 生成配置 | Generation Run ID |

Command 安全规则：

- `upsert_zentao_connection` 只接收一次性凭据输入或已有 `credential_key`。
- 查询 Command 永不返回密码、Token、Cookie 或 Session。
- URL、ID、分页大小和同步时间范围由 Rust 二次校验。
- 同一映射只允许一个同步任务运行。
- 全量同步、远程 Embedding 和远程 AI 摘要需要独立确认。

### 31.12 前端页面设计

知识库的“知识源”页面新增“禅道”来源类型，并增加以下界面。

#### 禅道连接

- 连接名称。
- 禅道地址。
- 认证方式。
- 用户名及密码/Token 的一次性输入。
- TLS 校验。
- “测试并探测能力”按钮。
- 能力矩阵：需求、任务、Bug、测试用例、测试执行、构建、发布。
- 最近测试时间和错误摘要。

#### 项目映射

- 左侧显示禅道产品/项目/执行树。
- 右侧选择知识项目和发布版本。
- 配置需求、任务、工时、Bug、测试、评论和附件元数据范围。
- 配置同步起始时间。
- 配置远程 Embedding 和远程 AI 开关。
- 未映射执行和未识别版本显示警告。

#### 同步中心

- 当前阶段。
- 每类实体已读取、更新、跳过和失败数量。
- 最近成功游标。
- 当前限流和重试状态。
- 生成文档数量。
- 待向量化片段数量。
- 支持安全取消和失败批次重试。

#### 文档生成

- 选择项目、版本和文档类型。
- 预览确定性 Markdown。
- 展示输入实体数量和数据快照时间。
- 可选启用 AI 摘要。
- 展示与上一个文档版本的差异。
- 生成后可直接打开引用和来源实体。

### 31.13 RAG 检索和回答规则

当用户提问：

> 我想了解某某项目当初在某个版本的需求和具体实现方案。

检索流程应为：

1. 识别知识项目和发布版本。
2. 硬过滤该项目与版本。
3. FTS 精确召回版本需求基线、需求 ID、接口和 Commit。
4. 向量召回语义相关的需求、设计和测试片段。
5. 从需求扩展到禅道任务、Bug、测试用例和测试执行。
6. 从已确认关系扩展到 Git Commit、代码文件和技术方案。
7. 对证据按来源、版本、更新时间和确认状态排序。
8. 组装带引用上下文并调用大模型。

回答结构：

1. 项目和版本范围。
2. 需求背景与目标。
3. 禅道需求及变更记录。
4. 任务拆分和实际执行情况。
5. 技术设计与代码实现证据。
6. 测试用例、执行结果和 Bug。
7. 发布状态、风险和遗留问题。
8. 证据清单与缺口。

引用示例：

```text
[禅道 Story #1234，版本 3，更新时间 2026-06-10]
[禅道 Task #5678，状态 done，更新时间 2026-06-18]
[Git Commit abc123，分支 release/v1.6.0]
[禅道 Case #901 / TestRun #1020，结果 pass]
```

证据冲突规则：

- 禅道显示完成但无 Commit 和测试证据：回答“流程状态已完成，但代码/测试证据不足”。
- Git 有实现但禅道任务未完成：并列呈现，不擅自修改状态结论。
- 需求在后续版本变更：历史版本只使用当时快照，后续变化单列提示。
- 测试结果只有失败记录：不得回答“已经验证通过”。

### 31.14 安全、隐私与审计

安全要求：

- 禅道密码、Token 和 Session 保存在现有安全凭据系统。
- SQLite 连接表只保存 `credential_key`。
- 日志隐藏 `Authorization`、Cookie、Token、密码和请求正文。
- 禅道 URL 必须限制为用户配置主机，重定向后重新校验主机。
- 默认禁止访问环回、链路本地和非预期内网地址；企业内网实例由用户明确登记。
- HTTP 客户端限制响应大小、分页大小、超时、并发和重试次数。
- HTML 正文先清洗危险标签，再转换为 Markdown。
- 附件默认只存元数据，不自动下载和解析。
- 用户姓名、评论和工时备注按来源级配置同步。
- 原始 JSON 快照可关闭，或按字段白名单存储。
- 默认不把完整原始 JSON、评论和个人信息发送到远程模型。

远程处理授权分为两个独立开关：

| 开关 | 作用 |
| --- | --- |
| `allow_remote_embedding` | 允许文档分块发送到远程 Embedding |
| `allow_remote_ai` | 允许检索片段发送到远程聊天模型生成分析 |

需要审计：

- 连接创建、测试、修改和删除。
- 项目映射和同步范围变化。
- 手工全量同步和远程发送确认。
- 每类实体同步数量、游标和失败原因。
- 文档生成模板版本、输入哈希和输出文档。
- AI 摘要所用 Provider、模型和引用。
- RAG 回答使用的禅道实体 ID、Git Commit 和文档版本。

### 31.15 测试与验收

#### 适配器契约测试

- 使用脱敏后的真实响应 Fixture 覆盖目标禅道版本。
- 版本探测失败能给出可操作错误。
- 认证过期、权限不足和接口不存在能区分。
- 分页、空页、重复页和最后一页处理正确。
- 缺失可选字段不会导致整个同步失败。
- HTML、富文本和特殊字符正确转换。

#### Database 测试

- 六张新增表和索引迁移成功。
- 连接不保存凭据明文。
- 实体 Upsert 幂等。
- 同一外部 ID 不重复。
- 游标只在实体类型完整成功后推进。
- 软删除和 `missing_count` 正确。
- 关系唯一约束和确认状态正确。
- 生成记录输入哈希去重正确。

#### Service 测试

- 产品、项目、执行和版本映射。
- 增量同步只处理变化实体。
- 需求变更保留多个快照。
- 任务工时、Bug、用例和执行关系正确。
- 确定性模板相同输入生成相同 Markdown。
- 缺少 Commit 或测试时生成明确证据缺口。
- AI 摘要不改变事实区域。
- 远程开关关闭时不调用 Embedding 或聊天 Provider。

#### 集成测试

- 从禅道 Fixture 同步到 SQLite。
- 自动生成 Markdown 文档版本。
- 文档进入分块和 FTS。
- 使用活动 A/B Profile 生成向量。
- 按项目和版本检索到禅道与 Git 联合证据。
- 重复同步无变化时不重新向量化。
- 同步中断恢复不丢失已完成数据。

#### 前端和运行验收

- 连接、能力探测、映射、同步和预览流程可用。
- 错误信息不包含凭据。
- 同步进度和重试状态准确。
- 生成文档可查看来源实体。
- 使用浏览器 Dev API 验收页面交互。
- 使用目标禅道测试实例完成一次真实只读同步。
- 使用一个真实版本回答“需求和具体实现方案”，人工核验所有引用。

验收示例：

```gherkin
Given 已配置禅道项目 A 与知识项目 A 的映射
And 执行 Sprint-12 映射为发布版本 v1.6.0
And 禅道包含需求、任务、测试用例和测试结果
And Git Commit 消息包含对应 Story 和 Task 编号
When 用户执行增量同步并生成版本项目文档
Then 系统生成版本需求基线和追踪矩阵
And 仅对新增或变化片段生成向量
And 问答能够引用禅道实体与 Git Commit
And 未建立的关系明确显示为证据缺口
```

### 31.16 推荐实施顺序

```text
目标禅道 API Spike
  → 连接与能力探测
  → 产品/项目/执行映射
  → 需求和任务增量同步
  → Bug、测试用例和测试执行同步
  → 通用关系构建
  → 确定性 Markdown 生成
  → 分块、FTS 和 A/B 向量化
  → Git Commit 关联
  → RAG 联合问答
  → 可选 AI 摘要和跨项目分析
```

首个可交付闭环建议只选择一个真实项目和一个发布版本，完成：

1. 禅道只读连接。
2. 需求、任务和测试同步。
3. 版本需求基线与追踪矩阵生成。
4. 本地 Embedding 索引。
5. 带禅道和 Git 引用的问答。

闭环验证通过后，再扩展评论、附件、构建、发布和多禅道实例，避免在没有真实接口和字段证据时一次性铺开所有实体。

---

## 32. Git 与本地源码知识化详细方案

### 32.1 目标与原则

源码知识化的目标不是简单把代码文件全文塞入向量库，而是建立可定位、可追溯、版本感知的代码知识模型。

完整链路：

```text
Git 仓库 / 本地源码目录
  → 授权范围和安全过滤
  → Commit/Tag/工作树/目录快照
  → 文件类型识别
  → 语法与符号分析
  → 调用、依赖、IPC、API、SQL、测试关系
  → 代码文档确定性生成
  → 符号级分块、FTS5 与向量化
  → 与禅道需求/任务/测试及 Git Commit 关联
  → 混合检索、影响分析和 RAG 问答
```

核心原则：

- 版本优先：任何代码结论必须绑定 Commit、Tag 或明确的本地快照。
- 结构优先：优先提取模块、符号和调用关系，再进行向量化。
- 最小读取：只读取用户授权根目录及符合规则的文件。
- 默认只读：分析过程不修改、不格式化、不构建、不执行被分析代码。
- 增量更新：内容哈希未变化时不重复解析和向量化。
- 证据可定位：引用至少包含仓库/来源、快照、相对路径和行号。
- 降级可见：解析失败时允许文本分块，但必须标记分析质量。

### 32.2 代码来源与快照类型

| 来源模式 | 快照类型 | 读取方式 | 事实级别 |
| --- | --- | --- | --- |
| Git Commit | `git_commit` | 读取指定 Commit 的树对象 | 可复现历史事实 |
| Git Tag | `git_tag` | 解析 Tag 对应 Commit 后读取 | 可复现发布事实 |
| Git 分支头 | `git_branch_head` | 记录分支当前 Commit | 可复现，但分支会移动 |
| Git 当前工作树 | `git_worktree` | 读取已跟踪及可选未跟踪文件 | 本机临时事实 |
| 非 Git 本地目录 | `local_directory` | 读取授权目录当前内容 | 本机快照事实 |

Git 历史快照读取不得执行 checkout：

- 文件清单使用现有 Git 工作区服务读取树对象。
- 历史文件内容使用等价于 `git show <commit>:<path>` 的只读能力。
- 版本差异使用等价于 `git diff --name-status <old> <new>` 的只读能力。
- Tag、分支和 Commit 解析后记录不可变 Commit SHA。
- 不执行 stash、reset、checkout 或自动切换用户当前分支。

当前工作树快照需要额外记录：

- 基线 Commit。
- 当前分支。
- 工作树是否 dirty。
- 已修改、已暂存和未跟踪文件清单。
- 快照采集时间。
- 每个文件的内容哈希。

工作树内容不得自动归入发布版本。只有用户明确绑定且确认后，才能作为某版本的补充证据。

非 Git 本地目录没有历史能力。每次扫描生成独立快照，可按内容哈希比较变化，但不能伪造 Commit 或发布时间。

### 32.3 授权范围与文件筛选

每个代码源配置：

| 配置 | 说明 |
| --- | --- |
| `root_path` / `git_workspace_key` | 授权根目录或 Git 工作区 |
| `snapshot_strategy` | Commit、Tag、分支头、工作树或目录快照 |
| `include_globs` | 允许的源码路径 |
| `exclude_globs` | 排除路径 |
| `include_untracked` | 工作树是否包含未跟踪文件，默认关闭 |
| `follow_symlinks` | 是否跟随符号链接，默认关闭 |
| `max_file_bytes` | 单文件上限 |
| `allowed_languages` | 允许分析的语言 |
| `allow_remote_embedding` | 是否允许远程生成代码向量 |
| `allow_remote_ai` | 是否允许代码片段进入远程大模型 |
| `retain_source_content` | 是否在 SQLite 保存源码正文 |

默认排除：

```text
.git/
.svn/
.hg/
node_modules/
vendor/
target/
dist/
build/
coverage/
.next/
.nuxt/
out/
tmp/
temp/
*.min.js
*.min.css
*.map
*.class
*.jar
*.war
*.exe
*.dll
*.so
*.dylib
*.png
*.jpg
*.pdf
*.zip
*.tar
*.gz
```

凭据和敏感配置默认阻断：

```text
.env
.env.*
*.pem
*.key
*.p12
*.pfx
id_rsa*
credentials*
secrets*
```

Git 来源默认以已跟踪文件为准并遵守仓库属性；工作树未跟踪文件必须显式开启。非 Git 目录使用默认排除规则与用户规则的并集。

### 32.4 分层分析流水线

```text
SourceDiscovery
  → SnapshotBuilder
  → FileClassifier
  → SecretGuard
  → LanguageAnalyzer
  → SymbolNormalizer
  → RelationResolver
  → CodeDocumentGenerator
  → ChunkIndexer
```

#### 第一层：仓库和目录结构

提取：

- 根模块和子模块。
- 包管理及构建文件。
- 源码、测试、配置、迁移和文档目录。
- Git 子模块和工作区信息。
- 语言、框架和依赖清单。

#### 第二层：语法和符号

优先使用语言语法解析器提取 AST 或等价结构；解析器不可用或语法错误时降级。

分析级别：

| 级别 | 方式 | 结果 |
| --- | --- | --- |
| `ast` | 语言语法解析器 | 精确符号、范围和结构关系 |
| `structured_fallback` | 语言感知的模式解析 | 主要声明、导入和接口 |
| `text_only` | 纯文本与行级分块 | 仅全文和向量检索 |
| `skipped` | 二进制、过大、敏感或不支持 | 只保留文件元数据 |

#### 第三层：跨文件关系

在单文件分析完成后解析：

- 导入和依赖。
- 类型继承和接口实现。
- 函数/方法调用。
- 前端路由与页面。
- Tauri `invoke` 与 Rust `#[tauri::command]`。
- HTTP 路由、客户端调用和 DTO。
- Java Controller/Consumer/Feign/Provider/Mapper 链路。
- Vue/React 页面、API 封装与后端接口。
- SQL 查询、表、字段与迁移。
- 测试用例与被测符号。
- 配置键的定义和读取位置。

动态反射、宏展开、字符串拼接路由和运行时依赖可能无法静态确认，必须标记置信度和限制。

### 32.5 首批语言与分析能力

| 语言/文件 | 首期提取能力 | 建议优先级 |
| --- | --- | --- |
| Rust | 模块、struct、enum、trait、impl、函数、Tauri Command、SQL 字符串 | P0 |
| TypeScript/JavaScript | import/export、函数、类、React 组件、API 调用、invoke | P0 |
| TSX/JSX | 组件、Hooks、路由、事件和 API 调用 | P0 |
| Vue SFC | template/script、组件、路由、Store 和 API 调用 | P0 |
| Java | package、class、interface、方法、注解、Controller、Feign、Mapper | P0 |
| SQL | DDL、DML、表、字段、索引和查询关系 | P0 |
| JSON/YAML/TOML | 配置路径、键值、依赖和环境配置 | P1 |
| Python | 模块、类、函数、路由和导入 | P1 |
| Shell | 函数、命令和脚本步骤 | P1 |
| XML | Maven、Mapper 和配置结构 | P1 |
| 其他文本语言 | 文件级/块级文本分析 | 降级 |

解析器选择需要单独 Spike。首期可评估 Tree-sitter 生态或语言专用解析器，但数据库模型和 `LanguageAnalyzer` 接口不能与某个解析库绑定。

统一接口建议：

```rust
pub trait LanguageAnalyzer: Send + Sync {
    fn language_id(&self) -> &'static str;
    fn supports(&self, path: &Path, content: &[u8]) -> bool;
    fn analyze(&self, input: CodeAnalysisInput) -> Result<CodeAnalysisOutput, AppError>;
}
```

### 32.6 代码符号和稳定标识

代码符号类型包括：

- module/package
- class/interface/trait
- struct/enum
- function/method
- component/page
- route/endpoint
- command/event
- model/dto
- table/column/index
- config_key
- test_case

符号稳定键：

```text
code:{source_key}:{relative_path}:{symbol_kind}:{qualified_name}:{signature_hash}
```

符号信息：

- 名称和全限定名。
- 类型和可见性。
- 签名。
- 文档注释。
- 起止行列。
- 所属符号。
- 语言。
- 内容哈希。
- 解析级别。
- 快照和文件版本。

文件重命名时：

- Git Diff 明确识别 rename 时建立 `renamed_from`。
- 内容相同但路径变化时可以建议重命名关系。
- 不直接复用旧符号 ID；通过稳定逻辑键和关系保持历史可追溯。

### 32.7 代码关系模型

| 关系 | 示例 |
| --- | --- |
| `contains` | 模块包含类或函数 |
| `declares` | 文件声明符号 |
| `imports` | 文件或模块导入另一模块 |
| `calls` | 方法调用另一方法 |
| `implements` | 类实现接口 |
| `extends` | 类型继承父类型 |
| `invokes_command` | React API 调用 Tauri Command |
| `emits_event` / `listens_event` | Tauri 事件发送和监听 |
| `exposes_api` | Controller/路由暴露接口 |
| `calls_api` | 前端或服务调用接口 |
| `calls_feign` | Consumer 调用 Feign |
| `delegates_to` | Command/Controller 调用 Service |
| `queries_table` | Mapper/DAO/SQL 查询数据表 |
| `writes_table` | 代码写入数据表 |
| `reads_config` | 代码读取配置键 |
| `tested_by` | 符号由测试用例覆盖 |
| `changed_in` | 文件或符号在 Commit 中变化 |
| `implements_requirement` | Commit/符号实现禅道需求 |

关系证据保存：

- 来源文件和行号。
- 目标解析方式。
- 原始调用或引用文本。
- 分析器版本。
- 置信度。
- 是否人工确认。

关系扩展必须设置深度和数量限制，避免大型依赖图导致检索爆炸。

### 32.8 代码分块与向量化

代码不能只按固定字符数切分。分块优先级：

1. 类、接口、trait、struct 或组件。
2. 独立函数和方法。
3. 路由、Command、SQL 语句和测试用例。
4. 超大符号按内部语句块和行数二次切分。
5. 无解析器时按注释、空行和长度切分。

每个代码块增加检索前缀：

```text
项目: tauri_ssh
快照: commit abc123
语言: rust
路径: src-tauri/src/commands/knowledge.rs
符号: commands::knowledge::search_knowledge
类型: function
签名: fn search_knowledge(...)
```

向量正文包含：

- 路径和符号限定名。
- 签名。
- 文档注释和关键注释。
- 规范化代码正文。
- 与代码直接相关的结构标签。

不将以下内容直接向量化：

- 二进制。
- 压缩或压成单行的生成文件。
- Lock 文件全文。
- 重复 vendor 代码。
- 检测到密钥的片段。
- 超过限制且无法安全切分的文件。

代码向量仍遵守 A/B Profile：

- 方案 A：本地生成，适合作为源码默认模式。
- 方案 B：必须同时满足系统、代码源、文件敏感级别和内容检测授权。
- 代码源默认 `allow_remote_embedding=false`。
- 查询向量和代码块向量必须使用同一 Profile。

### 32.9 Git 增量与历史分析

首次分析：

1. 解析目标 Commit 或工作树基线。
2. 生成文件清单。
3. 应用排除和敏感规则。
4. 计算内容哈希。
5. 解析文件和符号。
6. 构建跨文件关系。
7. 生成代码文档、FTS 和向量。

后续 Commit：

1. 比较旧、新 Commit 的文件状态。
2. 新增文件：完整分析。
3. 修改文件：重新分析文件和受影响关系。
4. 删除文件：将新快照中的文件标记缺失，保留历史。
5. 重命名文件：建立重命名关系并更新路径。
6. 导出符号发生变化时，使依赖它的关系进入待重算队列。
7. 只重建变化代码块的 FTS 和向量。

工作树重新扫描：

- 比较文件内容哈希而不是仅依赖 mtime。
- 未提交变化创建新工作树快照。
- 工作树恢复干净后允许关闭或归档临时快照。
- 不覆盖对应 Commit 的历史快照。

代码快照状态：

```text
pending
  → discovering
  → filtering
  → parsing
  → resolving_relations
  → generating_documents
  → indexing_fts
  → embedding
  → ready

任意阶段 → partial | failed | interrupted | cancelled
```

`partial` 表示部分语言、文件或关系分析失败，但其余证据仍可检索。页面必须显示跳过和失败原因。

### 32.10 自动生成的代码文档

系统使用确定性模板生成：

| 文档 | 内容 |
| --- | --- |
| 仓库概览 | 技术栈、目录、模块、构建和运行入口 |
| 模块说明 | 模块职责、公开符号、依赖和入口 |
| API/IPC 文档 | 路由或 Command、参数、返回值和调用方 |
| 数据库文档 | DDL、表、字段、索引、Mapper/DAO 使用位置 |
| 调用链文档 | 页面/API → Command/Controller → Service → Database |
| 配置文档 | 配置键、默认值、读取位置和环境差异 |
| 测试映射 | 测试文件、测试用例和被测符号 |
| Commit 变更摘要 | 文件、符号、接口和数据模型变化 |
| 版本实现报告 | 需求—任务—Commit—符号—测试证据 |
| 影响分析报告 | 指定符号变更的上游调用方和下游依赖 |

生成原则与禅道文档一致：

- 第一阶段由代码确定性生成事实。
- 第二阶段可选用大模型补充解释和方案对比。
- AI 不得改变符号签名、行号、Commit、接口或数据库事实。
- 文档保存生成器版本、快照 ID 和输入哈希。
- 相同输入不重复生成新文档版本。

### 32.11 SQLite 数据模型

源码正文继续复用：

- `knowledge_documents`
- `knowledge_document_versions`
- `knowledge_chunks`
- `knowledge_chunk_embeddings`

其中代码文件映射为：

```text
doc_type = source_code
document_key = code:{source_key}:{relative_path}
version_label = commit SHA / worktree snapshot key / local snapshot key
source_path = 相对路径
content = 源码正文，受 retain_source_content 控制
```

新增结构表。

#### 32.11.1 `knowledge_code_snapshots`

```sql
CREATE TABLE IF NOT EXISTS knowledge_code_snapshots (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_key        TEXT NOT NULL UNIQUE,
    source_id           INTEGER NOT NULL,
    project_id          INTEGER DEFAULT NULL,
    release_id          INTEGER DEFAULT NULL,
    snapshot_type       TEXT NOT NULL,
    ref_name            TEXT NOT NULL DEFAULT '',
    commit_sha          TEXT NOT NULL DEFAULT '',
    base_commit_sha     TEXT NOT NULL DEFAULT '',
    branch_name         TEXT NOT NULL DEFAULT '',
    worktree_dirty      INTEGER NOT NULL DEFAULT 0,
    captured_at         TEXT NOT NULL,
    file_count          INTEGER NOT NULL DEFAULT 0,
    symbol_count        INTEGER NOT NULL DEFAULT 0,
    relation_count      INTEGER NOT NULL DEFAULT 0,
    analyzer_version    TEXT NOT NULL,
    status              TEXT NOT NULL,
    error               TEXT DEFAULT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_code_snapshots_source
ON knowledge_code_snapshots(source_id, captured_at);

CREATE INDEX IF NOT EXISTS idx_code_snapshots_commit
ON knowledge_code_snapshots(project_id, commit_sha);
```

#### 32.11.2 `knowledge_code_files`

```sql
CREATE TABLE IF NOT EXISTS knowledge_code_files (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_id           INTEGER NOT NULL,
    document_version_id   INTEGER DEFAULT NULL,
    relative_path         TEXT NOT NULL,
    language              TEXT NOT NULL DEFAULT 'unknown',
    file_size             INTEGER NOT NULL DEFAULT 0,
    content_hash          TEXT NOT NULL,
    analysis_level        TEXT NOT NULL,
    is_generated          INTEGER NOT NULL DEFAULT 0,
    is_test               INTEGER NOT NULL DEFAULT 0,
    sensitivity           TEXT NOT NULL DEFAULT 'internal',
    status                TEXT NOT NULL DEFAULT 'active',
    skip_reason           TEXT NOT NULL DEFAULT '',
    created_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(snapshot_id, relative_path)
);

CREATE INDEX IF NOT EXISTS idx_code_files_snapshot_language
ON knowledge_code_files(snapshot_id, language, status);
```

#### 32.11.3 `knowledge_code_symbols`

```sql
CREATE TABLE IF NOT EXISTS knowledge_code_symbols (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_id         INTEGER NOT NULL,
    file_id             INTEGER NOT NULL,
    symbol_key          TEXT NOT NULL,
    symbol_kind         TEXT NOT NULL,
    name                TEXT NOT NULL,
    qualified_name      TEXT NOT NULL DEFAULT '',
    signature           TEXT NOT NULL DEFAULT '',
    visibility          TEXT NOT NULL DEFAULT '',
    parent_symbol_key   TEXT NOT NULL DEFAULT '',
    start_line          INTEGER NOT NULL,
    start_column        INTEGER NOT NULL DEFAULT 0,
    end_line            INTEGER NOT NULL,
    end_column          INTEGER NOT NULL DEFAULT 0,
    doc_comment         TEXT NOT NULL DEFAULT '',
    content_hash        TEXT NOT NULL,
    analysis_level      TEXT NOT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(snapshot_id, symbol_key)
);

CREATE INDEX IF NOT EXISTS idx_code_symbols_name
ON knowledge_code_symbols(snapshot_id, name, symbol_kind);

CREATE INDEX IF NOT EXISTS idx_code_symbols_qualified
ON knowledge_code_symbols(snapshot_id, qualified_name);
```

#### 32.11.4 `knowledge_code_relations`

```sql
CREATE TABLE IF NOT EXISTS knowledge_code_relations (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_id           INTEGER NOT NULL,
    from_symbol_key       TEXT NOT NULL,
    relation_type         TEXT NOT NULL,
    to_symbol_key         TEXT NOT NULL DEFAULT '',
    to_external_type      TEXT NOT NULL DEFAULT '',
    to_external_key       TEXT NOT NULL DEFAULT '',
    evidence_file_id      INTEGER DEFAULT NULL,
    evidence_start_line   INTEGER DEFAULT NULL,
    evidence_end_line     INTEGER DEFAULT NULL,
    evidence_text         TEXT NOT NULL DEFAULT '',
    resolver              TEXT NOT NULL,
    confidence            REAL NOT NULL DEFAULT 1.0,
    confirmed             INTEGER NOT NULL DEFAULT 1,
    created_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(
        snapshot_id,
        from_symbol_key,
        relation_type,
        to_symbol_key,
        to_external_type,
        to_external_key,
        evidence_start_line
    )
);

CREATE INDEX IF NOT EXISTS idx_code_relations_from
ON knowledge_code_relations(snapshot_id, from_symbol_key, relation_type);

CREATE INDEX IF NOT EXISTS idx_code_relations_to
ON knowledge_code_relations(snapshot_id, to_symbol_key, relation_type);
```

结构化代码关系中需要进入跨知识源检索的部分，同步写入通用 `knowledge_relations`。

### 32.12 Rust 模块与后台任务

建议新增：

```text
src-tauri/src/
├── commands/
│   └── code_knowledge.rs
├── services/
│   ├── code_knowledge/
│   │   ├── mod.rs
│   │   ├── source_discovery.rs
│   │   ├── snapshot.rs
│   │   ├── file_classifier.rs
│   │   ├── secret_guard.rs
│   │   ├── analyzer.rs
│   │   ├── analyzers/
│   │   ├── relation_resolver.rs
│   │   ├── chunker.rs
│   │   └── document_generator.rs
│   └── code_retrieval.rs
├── database/
│   └── code_knowledge.rs
└── models/
    └── code_knowledge.rs
```

分层职责：

- Command：校验来源、快照参数，启动分析并查询结果。
- Service：只读源码、构建快照、解析符号、构建关系和生成文档。
- Database：保存快照、文件、符号、关系、任务和索引。
- React：来源配置、分析进度、代码浏览、关系图和引用展示。

解析和向量化必须使用后台任务，避免阻塞 IPC。大型仓库按文件批次提交事务，并持续上报：

- 已发现文件。
- 已跳过文件。
- 已解析文件。
- 符号和关系数量。
- 生成文档数量。
- FTS 和向量化进度。
- 错误及降级数量。

### 32.13 Tauri Commands

| Command | 说明 |
| --- | --- |
| `list_code_sources` | 查询 Git 与本地代码源 |
| `upsert_code_source` | 保存授权范围和分析配置 |
| `delete_code_source` | 软删除代码源 |
| `preview_code_source_scope` | 预览将读取、排除和阻断的文件 |
| `list_git_code_refs` | 查询可分析的分支、Tag 和 Commit |
| `estimate_code_analysis` | 估算文件、语言、大小和远程处理范围 |
| `start_code_analysis` | 启动 Commit、Tag、工作树或目录快照分析 |
| `get_code_analysis_status` | 查询后台任务进度 |
| `list_code_snapshots` | 查询代码快照 |
| `get_code_snapshot` | 获取快照统计和错误 |
| `list_code_files` | 按快照、语言和模块查询文件 |
| `get_code_file` | 获取允许展示的文件内容和符号 |
| `search_code_symbols` | 精确查询符号 |
| `get_code_symbol` | 获取符号、引用和关系 |
| `get_code_call_graph` | 查询限定深度的调用/依赖图 |
| `compare_code_snapshots` | 比较文件、符号和关系变化 |
| `preview_code_generated_documents` | 预览确定性代码文档 |
| `generate_code_documents` | 生成并纳入知识库 |
| `analyze_code_impact` | 分析指定符号或 Commit 的影响范围 |

所有文件路径必须来自已登记代码源，Command 不接受任意路径直接读取。

### 32.14 前端页面

知识源页面新增“代码源”配置：

- 选择现有 Git 工作区或本地目录。
- 选择 Commit、Tag、分支头、当前工作树或目录快照。
- 配置语言、包含/排除规则、文件大小和未跟踪文件。
- 预览读取范围、敏感阻断和预计文件数量。
- 独立设置远程 Embedding 与远程 AI 权限。

代码知识页面包括：

1. 快照列表：Commit、Tag、dirty 状态、分析时间和质量。
2. 仓库树：文件、语言、状态和跳过原因。
3. 符号搜索：名称、类型、模块和签名。
4. 代码详情：只读源码、行号、符号和引用。
5. 关系视图：调用链、依赖、IPC、API 和数据库关系。
6. 版本比较：新增、修改、删除和重命名的文件与符号。
7. 自动文档：模块、API、数据库、调用链和版本报告。
8. 影响分析：上游调用者、下游依赖、关联需求和测试。

关系图只用于查看有限深度的局部图；大型全仓依赖默认使用表格和分组树，避免页面不可用。

### 32.15 检索与 RAG

代码查询流程：

1. 识别项目和目标版本/快照。
2. 对 Commit、Tag 或快照执行硬过滤。
3. 使用 FTS 精确召回符号、接口、字段、路径、配置键和 SQL。
4. 使用向量召回语义相似实现。
5. 使用代码关系扩展上下游符号。
6. 使用通用关系扩展到禅道需求、任务、测试和 Git Commit。
7. 对结果执行版本、可信度和证据完整性排序。
8. 生成带代码行号引用的回答。

典型问答：

- “这个页面的数据最终查了哪张表？”
- “`warningTime` 从数据库到前端是怎么传递的？”
- “某需求在 `v1.6.0` 具体修改了哪些类和 SQL？”
- “这个 Tauri Command 被哪些页面调用？”
- “修改这个 Service 可能影响哪些接口和测试？”
- “本地工作树与 `v1.6.0` Tag 的实现差异是什么？”

回答引用：

```text
[仓库 tauri_ssh，Commit abc123，src/lib/api/index.ts:120]
[仓库 tauri_ssh，Commit abc123，src-tauri/src/commands/knowledge.rs:45]
[本地工作树快照 worktree-20260729-103000，dirty，src/pages/knowledge/index.tsx:88]
```

当静态分析无法确认调用目标时，回答使用“可能调用”或“未能静态解析”，不得把低置信度关系表述为确定事实。

### 32.16 安全和隐私

代码分析安全要求：

- 用户通过目录对话框或已登记 Git 工作区明确授权。
- Rust 对根目录和每个文件 canonicalize，禁止越过授权根目录。
- 默认不跟随符号链接；开启后仍需校验目标位于授权范围。
- 不给 WebView 开放任意文件系统读取权限。
- 不执行仓库脚本、构建、测试、包管理器或钩子。
- 不解析 `.git` 对象之外的敏感 Git 配置和凭据。
- 不记录完整源码到普通运行日志。
- 源码展示 Command 按授权和敏感级别返回。
- 删除代码源时支持只删索引或连同本地缓存删除。

敏感检测至少覆盖：

- 私钥和证书。
- 常见云平台密钥。
- Token、密码和连接串。
- `.env` 和凭据配置。
- Git remote 中可能出现的明文凭据。

检测到敏感内容时：

1. 文件或片段标记 `restricted`。
2. 默认不保存正文，仅保存路径、哈希和阻断原因。
3. 不进行远程 Embedding。
4. 不发送到远程大模型。
5. 不通过 MCP 返回正文。
6. 审计只记录规则 ID，不记录命中的秘密。

### 32.17 测试与验收

#### 文件和 Git 测试

- Commit、Tag、分支头和工作树快照可正确读取。
- 历史读取不会切换当前分支或修改工作树。
- 非 Git 目录只能读取授权范围。
- 路径穿越和越界符号链接被阻断。
- `.gitignore`、默认排除和用户规则组合正确。
- 二进制、超大文件、依赖目录和生成文件正确跳过。
- dirty 工作树、已暂存和未跟踪文件状态正确。

#### 解析器测试

- 每种 P0 语言使用固定 Fixture 验证符号和行号。
- 语法错误文件可降级且不影响整个快照。
- 同名符号通过限定名和路径区分。
- Tauri invoke/Command、Java Feign/Provider/Mapper、API/SQL 关系正确。
- 动态调用和无法解析关系标记低置信度。
- 注释中的伪代码不会误判为高置信关系。

#### 增量和数据库测试

- 同一 Commit 重复分析幂等。
- 单文件变化只重建相关文件、关系、FTS 和向量。
- 删除和重命名保留历史证据。
- 工作树快照不覆盖 Commit 快照。
- 不同快照的同名符号不会混用。
- 中断恢复后已完成文件不重复处理。

#### 检索测试

- 精确类名、函数、接口、表和字段可命中。
- 自然语言可以召回语义相关代码。
- 指定 Commit 时不会混入工作树或后续版本代码。
- 调用链按限定深度扩展。
- 代码引用的路径和行号与快照内容一致。
- 需求—Commit—符号—测试可以形成联合证据链。

#### 安全测试

- 私钥、Token、密码和连接串 Fixture 被阻断。
- 远程开关关闭时不发起远程请求。
- 敏感命中日志不包含秘密正文。
- MCP 和前端不能绕过来源授权读取任意路径。
- 分析过程不执行仓库内任何代码。

验收场景：

```gherkin
Given 已登记一个 Git 工作区和一个非 Git 本地源码目录
And Git Tag v1.6.0 映射到知识版本 v1.6.0
When 用户分析 Tag、当前工作树和本地目录
Then 系统生成三个相互隔离的代码快照
And 提取模块、符号、调用、API、SQL 和测试关系
And 生成模块文档与版本实现报告
And 仅对变化的代码符号生成新向量
And 问答引用正确快照、文件路径和行号
And dirty 工作树不会被表述为 v1.6.0 的发布事实
```

### 32.18 推荐实施顺序

```text
代码源授权与范围预览
  → Git Commit/Tag/工作树快照
  → 文件分类、安全过滤和内容哈希
  → Rust/TypeScript/Java/Vue/SQL P0 解析器
  → 符号级分块、FTS 和本地向量
  → 导入、调用、IPC、API 和 SQL 关系
  → 增量 Diff 和关系失效传播
  → 确定性代码文档
  → 禅道需求/任务/测试联合关系
  → 版本问答和影响分析
  → 远程 Embedding/AI 的可选授权
```

首个闭环建议选择一个真实 Git 项目：

1. 分析一个发布 Tag 和当前工作树。
2. 提取一条真实的页面/API → 后端 → 数据库调用链。
3. 生成模块与接口文档。
4. 使用本地 Embedding 建索引。
5. 回答一个指定版本的实现问题并核验全部代码引用。

完成 Git 闭环后，再接入非 Git 本地目录和更多语言，避免在符号模型、版本隔离和安全边界尚未验证前扩大扫描范围。
