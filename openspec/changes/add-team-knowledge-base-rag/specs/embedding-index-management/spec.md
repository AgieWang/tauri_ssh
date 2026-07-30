## Purpose

定义本地和远程 Embedding、全文索引、向量存储及索引生命周期的行为，确保模型切换安全、索引可回滚且不同向量空间绝不混用。

## ADDED Requirements

### Requirement: Support local and remote embedding profiles
系统 SHALL 提供本地模型和远程 Embedding API 两种模式，并 SHALL 将模式、Provider、模型、模型修订、维度、归一化和分块策略保存为独立 Profile。

#### Scenario: Configure local embedding
- **WHEN** 用户选择本地模式、模型和缓存目录并通过测试
- **THEN** 系统创建本地 Profile，并在后台使用本地模型生成文档和查询向量

#### Scenario: Configure remote embedding
- **WHEN** 用户选择远程 Provider 和独立 Embedding 模型并通过真实短文本测试
- **THEN** 系统依据实际响应保存维度和协议能力，且不修改聊天模型配置

### Requirement: Enforce embedding compatibility
系统 MUST 使用完整 Profile 指纹隔离向量空间，并 MUST 拒绝跨模式、跨模型、跨修订、跨维度、跨归一化或跨分块策略比较向量。

#### Scenario: Query uses a different profile
- **WHEN** 查询向量 Profile 与候选文档向量 Profile 不一致
- **THEN** 系统拒绝该向量比较并返回可诊断的 Profile 不兼容错误

#### Scenario: Provider returns unexpected dimension
- **WHEN** 远程 Provider 返回的向量维度与已测试 Profile 不一致
- **THEN** 系统立即停止当前批次，不保存错误维度向量，也不激活该索引

### Requirement: Maintain full-text and vector indexes
系统 SHALL 为知识片段维护 FTS5 全文索引和活动 Profile 的向量，并 SHALL 将向量、维度、范数、Profile 及生成时间保存在本地。

#### Scenario: Exact and semantic indexes are built
- **WHEN** 新知识片段完成解析且允许索引
- **THEN** 系统更新全文索引，并使用当前构建 Profile 生成对应向量

#### Scenario: Content hash is unchanged
- **WHEN** 片段内容哈希和 Profile 指纹均未变化
- **THEN** 系统复用已有索引结果，不重复请求模型

### Requirement: Use blue-green profile rebuilds
系统 SHALL 在新 Profile 独立完成全量构建和完整性检查后原子激活，并 MUST 在构建期间保持旧活动索引可用。

#### Scenario: Successful profile switch
- **WHEN** 新 Profile 的所有必要片段均生成兼容向量且完整性检查通过
- **THEN** 系统原子切换活动 Profile，并暂时保留旧 Profile 以供回滚

#### Scenario: Rebuild fails
- **WHEN** 新 Profile 构建、远程请求或完整性检查失败
- **THEN** 系统将新 Profile 标记失败，旧活动索引继续提供检索服务

### Requirement: Estimate and control rebuild work
系统 SHALL 在启动重建前展示受影响文档、片段、预计本地工作量和远程发送字符量，并 SHALL 由用户确认涉及远程处理的重建。

#### Scenario: Remote rebuild confirmation
- **WHEN** Profile 变化需要把知识片段发送到远程 Embedding
- **THEN** 系统展示来源范围和预计字符量，并仅在用户确认且治理规则允许后启动

### Requirement: Operate without a server vector database
首期系统 SHALL 使用本地 SQLite 作为向量与元数据事实存储，并 SHALL 在未部署集中式向量数据库时完成索引、检索、重建和恢复。

#### Scenario: Fully local deployment
- **WHEN** 用户仅配置本地模型且没有任何向量服务器
- **THEN** 系统仍可完成分块、向量生成、混合检索和 RAG 上下文构建
