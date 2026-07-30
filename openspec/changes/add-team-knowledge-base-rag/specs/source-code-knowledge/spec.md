## Purpose

定义 Git 历史版本、当前工作树和本地授权源码的只读分析行为，使模块、符号、调用关系和代码文档能够作为版本化知识证据参与检索。

## ADDED Requirements

### Requirement: Create isolated code snapshots
系统 SHALL 支持为 Git Commit、Tag、分支头、当前工作树和非 Git 本地目录创建相互隔离的代码快照，并 SHALL 记录 Commit、基线、分支、dirty 状态、采集时间和文件哈希。

#### Scenario: Analyze a Git tag
- **WHEN** 用户选择 Tag `v1.6.0` 进行分析
- **THEN** 系统解析其不可变 Commit SHA，并在不切换当前工作区的情况下建立历史快照

#### Scenario: Analyze a dirty worktree
- **WHEN** 当前工作树存在已修改、已暂存或允许包含的未跟踪文件
- **THEN** 系统建立独立 dirty 工作树快照，且不得把它表述为基线 Commit 或发布版本事实

### Requirement: Restrict source discovery
系统 SHALL 只读取已登记 Git 工作区或用户授权本地根目录内的文件，并 MUST 应用默认排除、用户规则、文件大小、二进制和符号链接边界检查。

#### Scenario: Symlink escapes the source root
- **WHEN** 源码目录中的符号链接目标位于授权根目录之外
- **THEN** 系统跳过并记录越界原因，不读取目标内容

#### Scenario: Dependency and generated directories
- **WHEN** 扫描遇到 `node_modules`、`vendor`、`target`、`dist` 或配置的生成目录
- **THEN** 系统默认跳过这些目录，除非安全策略明确允许特定路径

### Requirement: Extract code symbols with visible quality
系统 SHALL 为支持语言提取模块、类、接口、结构体、函数、方法、组件、路由、Command、数据模型、SQL 对象和测试符号，并 SHALL 记录文件位置、签名、限定名和分析级别。

#### Scenario: Supported language parses successfully
- **WHEN** P0 语言文件语法有效且解析器可用
- **THEN** 系统保存 AST 级符号、精确行号和稳定符号键

#### Scenario: Parser fails
- **WHEN** 文件存在语法错误或缺少对应解析器
- **THEN** 系统降级为结构化或纯文本分析，标记分析质量且不使整个快照失败

### Requirement: Resolve code relationships
系统 SHALL 构建包含、声明、导入、调用、继承、接口实现、Tauri IPC、HTTP API、Feign、Service、Mapper、SQL 表、配置和测试关系，并 SHALL 为关系记录证据、解析方法和置信度。

#### Scenario: Resolve Tauri IPC chain
- **WHEN** 前端封装调用某个 `invoke` 且 Rust 存在同名已注册 Command
- **THEN** 系统建立前端调用方到 Command 的关系，并继续关联已解析的 Service 和 Database 关系

#### Scenario: Dynamic call cannot be resolved
- **WHEN** 调用目标由反射、宏或动态字符串决定且无法唯一解析
- **THEN** 系统不创建确定关系，或将候选关系标记低置信度和未确认

### Requirement: Incrementally update code knowledge
系统 SHALL 使用 Git Diff 和内容哈希识别新增、修改、删除及重命名文件，并 SHALL 仅重建受影响文件、符号、关系、全文索引和向量。

#### Scenario: One exported symbol changes
- **WHEN** 新 Commit 仅修改一个文件中的公开符号
- **THEN** 系统重新分析该文件并使依赖该符号的关系进入待重算队列，其他未受影响向量保持不变

#### Scenario: File is renamed
- **WHEN** Git Diff 确认文件重命名
- **THEN** 系统保留旧快照路径、记录重命名关系，并在新快照使用新路径

### Requirement: Chunk and index by symbol boundaries
系统 SHALL 优先以类、接口、函数、方法、组件、路由、Command、SQL 和测试用例边界生成代码片段，并 SHALL 为片段附加项目、快照、语言、路径、符号和签名元数据。

#### Scenario: Large source file
- **WHEN** 单个源码文件包含多个可识别符号
- **THEN** 系统按符号创建独立片段，而不是仅按固定字符数切割整个文件

### Requirement: Generate deterministic code documents
系统 SHALL 确定性生成仓库概览、模块说明、API/IPC、数据库、调用链、配置、测试映射、Commit 变更、版本实现和影响分析文档。

#### Scenario: Generate a version implementation report
- **WHEN** 发布版本关联禅道需求、Git Commit、代码符号和测试
- **THEN** 生成报告展示完整证据链，并对缺失关系明确标注

### Requirement: Cite exact code evidence
系统 SHALL 在代码检索和回答中引用来源、Commit/快照、相对路径和行号范围，并 MUST 隔离不同快照的同名符号。

#### Scenario: Compare release and local worktree
- **WHEN** 用户询问工作树与 `v1.6.0` Tag 的差异
- **THEN** 系统分别引用两个快照的文件和符号变化，并明确工作树是否 dirty
