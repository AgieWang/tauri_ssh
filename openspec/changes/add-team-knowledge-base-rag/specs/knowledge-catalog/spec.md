## Purpose

定义团队知识项目、发布版本、知识源、文档版本、片段和后台同步任务的可观察行为，使不同来源的研发事实能够统一管理并保持完整版本追溯。

## ADDED Requirements

### Requirement: Manage knowledge projects and releases
系统 SHALL 允许用户创建、更新、查询和软删除知识项目，并为项目维护别名、默认分支及多个发布版本；每个发布版本 SHALL 能关联 Git Tag、分支和基线 Commit。

#### Scenario: Create a versioned knowledge project
- **WHEN** 用户创建知识项目并登记版本 `v1.6.0`、Tag 和基线 Commit
- **THEN** 系统保存项目与版本映射，并允许后续文档、禅道实体和代码快照关联该版本

#### Scenario: Unrecognized version is not treated as latest
- **WHEN** 同步内容无法通过声明、映射或路径规则识别版本
- **THEN** 系统将其标记为 `unversioned`，且不得自动归入最新发布版本

### Requirement: Register scoped knowledge sources
系统 SHALL 支持 Git 工作区、本地文档目录、单文件、手工 Markdown、现有经验库、禅道和本地源码目录等来源，并 SHALL 为每个来源保存项目归属、同步模式、包含/排除规则和远程处理授权。

#### Scenario: Register local directory source
- **WHEN** 用户通过目录选择器授权一个本地目录并设置包含和排除规则
- **THEN** 系统仅登记该授权根目录及其规则，不允许来源读取根目录之外的文件

#### Scenario: Disable a source
- **WHEN** 用户禁用某个知识源
- **THEN** 后续同步和索引任务跳过该来源，历史文档版本和引用仍可查询

### Requirement: Preserve logical documents and immutable versions
系统 SHALL 以稳定文档标识管理逻辑文档，并 SHALL 为内容、版本、分支或 Commit 变化创建可追溯的文档版本，而不是覆盖历史正文。

#### Scenario: Document changes in a new release
- **WHEN** 同一逻辑文档在新发布版本中的内容哈希发生变化
- **THEN** 系统创建新的文档版本，并保留旧版本的来源、内容哈希和引用

#### Scenario: Deleted source document
- **WHEN** 同步确认来源文件已删除
- **THEN** 系统将对应当前文档版本标记为失效，但不物理删除历史版本

### Requirement: Incrementally synchronize content
系统 SHALL 使用来源游标、Git Diff、更新时间和内容哈希识别新增、修改、重命名及删除内容，并 MUST 只重新解析和索引发生变化的文档或片段。

#### Scenario: Unchanged content is skipped
- **WHEN** 再次同步得到的规范化内容哈希与已索引版本一致
- **THEN** 系统跳过文档版本创建、分块和向量化，并记录为未变化

#### Scenario: Partial synchronization failure
- **WHEN** 同步任务在部分批次完成后失败
- **THEN** 系统保留已提交的幂等结果，不推进未完整完成实体类型的成功游标，并允许从安全检查点重试

### Requirement: Parse and chunk supported documents
系统 SHALL 对 Markdown、TXT、SQL、JSON 和 YAML 提供确定性解析，并 SHALL 按标题、语句或结构边界生成包含项目、版本、来源、位置和内容哈希的知识片段。

#### Scenario: Markdown structure is preserved
- **WHEN** Markdown 包含标题、表格和代码块
- **THEN** 系统按标题层级分块，并尽量保持表格与代码块完整且保留标题路径

#### Scenario: Unsupported or failed parser
- **WHEN** 文件格式不受支持或解析失败
- **THEN** 系统标记明确状态和原因，不得静默产生错误正文

### Requirement: Track long-running knowledge jobs
系统 SHALL 为同步、解析、分块、全文索引、向量化和生成文档等长耗时操作提供持久化任务状态、阶段进度、错误、取消和中断恢复能力。

#### Scenario: Application restarts during indexing
- **WHEN** 应用启动时发现无心跳的运行中知识任务
- **THEN** 系统将任务标记为 `interrupted`，并允许从已完成批次恢复

#### Scenario: Safe cancellation
- **WHEN** 用户取消仍可安全中断的任务
- **THEN** 系统停止后续批次、保留已完成幂等结果，并将任务标记为 `cancelled`
