## Purpose

定义禅道只读接入、版本适配、增量同步、研发关系和项目文档生成行为，使需求、任务与测试过程成为可追溯的知识事实。

## ADDED Requirements

### Requirement: Connect and probe Zentao instances
系统 SHALL 允许用户配置禅道地址和安全凭据引用，并 SHALL 在连接测试时探测版本、认证模式以及产品、项目、执行、需求、任务、Bug 和测试能力。

#### Scenario: Successful capability probe
- **WHEN** 用户使用有效凭据测试一个受支持禅道实例
- **THEN** 系统返回脱敏的版本和能力矩阵，并选择兼容端点配置

#### Scenario: Unsupported or unauthorized endpoint
- **WHEN** 认证失败、权限不足或某类接口不可用
- **THEN** 系统区分错误类型并展示可操作信息，不记录或返回凭据明文

### Requirement: Map Zentao scope to knowledge projects and releases
系统 SHALL 允许用户把禅道产品、项目和执行映射到知识项目，并 SHALL 通过手工映射、发布名称、Tag 或规则把执行/发布绑定到知识发布版本。

#### Scenario: Map an execution to a release
- **WHEN** 用户把禅道执行 Sprint-12 映射到知识版本 `v1.6.0`
- **THEN** 后续同步实体和生成文档均关联 `v1.6.0`

#### Scenario: Unmapped release data
- **WHEN** 禅道实体无法识别发布版本
- **THEN** 系统将其标记为未版本化并提示人工映射，不得自动归入最新版本

### Requirement: Incrementally synchronize Zentao entities
系统 SHALL 按实体类型独立增量同步产品、项目、执行、需求及变更、任务、工时、Bug、测试用例和测试执行，并 MUST 使用连接、类型和外部 ID 保证幂等。

#### Scenario: Resume a paginated sync
- **WHEN** 某实体类型分页同步中途失败
- **THEN** 系统不推进该实体类型的成功游标，并允许从已完成检查点重试

#### Scenario: Repeated unchanged entity
- **WHEN** 同一外部实体的更新时间和规范化内容哈希未变化
- **THEN** 系统跳过新快照、文档生成和重新向量化

### Requirement: Preserve source history and deletion state
系统 SHALL 保存需求变更版本和必要来源时间，并 SHALL 对远程缺失实体先标记缺失而不是立即物理删除。

#### Scenario: Story content changes
- **WHEN** 禅道需求正文或验收标准产生新变更版本
- **THEN** 系统保留旧快照并创建可引用的新快照

#### Scenario: Entity disappears from one sync
- **WHEN** 全量同步暂时找不到之前存在的实体
- **THEN** 系统增加缺失计数并保留历史，只有满足删除确认规则后才标记删除

### Requirement: Build confirmed development relations
系统 SHALL 导入禅道显式的需求—任务—Bug—测试关系，并 SHALL 通过显式链接、Commit 编号约定或人工确认建立禅道与 Git 的关系。

#### Scenario: Commit message contains Zentao identifiers
- **WHEN** Commit 消息包含已配置格式的 Story 和 Task 编号
- **THEN** 系统建立带原始 Commit 消息证据的关系

#### Scenario: AI suggests a relation
- **WHEN** AI 根据语义建议需求与 Commit 或代码符号相关
- **THEN** 系统将关系保存为未确认并记录置信度，在人工确认前不得作为高权重事实

### Requirement: Generate deterministic project documents
系统 SHALL 从规范化禅道事实确定性生成项目概览、版本需求基线、追踪矩阵、任务执行总结、测试质量报告和风险遗留文档。

#### Scenario: Generate release traceability matrix
- **WHEN** 某版本完成禅道同步并触发文档生成
- **THEN** 追踪矩阵列出需求、任务、Commit、测试用例、执行结果及证据缺口，并包含实体 ID 和快照时间

#### Scenario: Identical inputs
- **WHEN** 生成模板版本和所有输入实体哈希与上次一致
- **THEN** 系统不创建重复文档版本

### Requirement: Keep AI summaries separate from facts
系统 SHALL 先生成不依赖大模型的事实文档；可选 AI 摘要 SHALL 只基于事实文档生成，标记 Provider/模型，并保留可校验引用。

#### Scenario: AI summary citation fails
- **WHEN** AI 摘要中的结论无法映射到事实片段
- **THEN** 系统删除该结论或标记为待核实，不得写入事实区域
