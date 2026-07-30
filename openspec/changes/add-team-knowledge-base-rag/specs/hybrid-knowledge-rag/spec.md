## Purpose

定义项目与版本感知的混合检索和 RAG 问答契约，使系统能够融合全文、向量及研发关系证据，并输出可核查且不串用历史版本的回答。

## ADDED Requirements

### Requirement: Apply project and version hard filters
系统 SHALL 在全文、向量和关系召回前解析并应用项目、发布版本、代码快照、文档类型和权限过滤；无法唯一识别时 SHALL 要求用户选择或明确返回歧义。

#### Scenario: Ask about a historical release
- **WHEN** 用户明确询问项目 A 的 `v1.6.0`
- **THEN** 主证据仅来自映射到项目 A 和 `v1.6.0` 的文档、禅道快照及代码快照

#### Scenario: Ambiguous project alias
- **WHEN** 用户输入的项目名匹配多个知识项目
- **THEN** 系统返回候选项目并要求选择，不得任意选取一个项目回答

### Requirement: Fuse exact, semantic, and relational retrieval
系统 SHALL 并行执行 FTS5 精确召回、活动 Profile 向量召回和有限深度关系扩展，并 SHALL 对精确项目、版本、需求编号和已确认关系给予更高排序权重。

#### Scenario: Exact requirement and semantic design match
- **WHEN** 问题同时包含需求编号和自然语言实现描述
- **THEN** 系统融合需求编号精确结果与语义相关设计/代码结果，并保留各召回通道信息

#### Scenario: Relation expansion limit
- **WHEN** 某实体拥有大量上下游关系
- **THEN** 系统按允许的关系类型、深度和数量上限扩展，避免无界图遍历

### Requirement: Build evidence-only RAG context
系统 SHALL 仅把通过权限、版本和敏感检查的检索片段组装为大模型上下文，并 SHALL 为每个片段分配稳定引用标识和来源元数据。

#### Scenario: Preview model context
- **WHEN** 用户请求预览一次知识问答上下文
- **THEN** 系统显示将发送的片段、项目、版本、来源、位置和引用标识，但不暴露 Provider 密钥

### Requirement: Produce cited answers
系统 SHALL 要求重要事实引用至少一个知识片段，并 SHALL 在回答中提供可打开的文档章节、禅道实体、Git Commit、代码路径和行号等来源信息。

#### Scenario: Answer requirements and implementation for a release
- **WHEN** 检索到某版本的需求、设计、代码和测试证据
- **THEN** 回答按需求、实现、测试、风险和引用组织，且每项核心结论可追溯到对应证据

### Requirement: Prevent historical evidence leakage
系统 MUST 不得将更晚版本内容表述为历史版本当时的事实；后续版本资料如被使用，MUST 单独标记为“后续演进”。

#### Scenario: Newer design conflicts with historical design
- **WHEN** `v1.7.0` 文档描述的方案与用户询问的 `v1.6.0` 不同
- **THEN** 系统以 `v1.6.0` 证据回答当时方案，并仅在单独的后续演进部分引用 `v1.7.0`

### Requirement: Expose evidence gaps and conflicts
系统 SHALL 在缺少需求、实现或测试证据时明确说明缺口，并 SHALL 在来源冲突时并列展示来源、时间和冲突内容，不得用模型常识补齐内部事实。

#### Scenario: Completed task without code evidence
- **WHEN** 禅道任务显示完成但没有已确认 Commit 或代码符号关系
- **THEN** 回答说明“流程状态已完成，但未找到代码实现证据”

#### Scenario: Test evidence only contains failures
- **WHEN** 关联测试执行只有失败结果
- **THEN** 系统不得声称需求已验证通过，并展示失败证据和当前缺口

### Requirement: Support retrieval evaluation
系统 SHALL 支持使用固定评测集记录 Recall@K、MRR、引用准确率、版本串用率、无证据拒答率和查询延迟。

#### Scenario: Run a regression evaluation
- **WHEN** 活动 Profile、分块策略或排序规则发生变化
- **THEN** 系统能够运行相同评测集并输出可比较指标，供激活或回滚决策使用
