## Purpose

定义知识读取、凭据、敏感内容、远程模型、MCP 和审计的统一治理行为，保证内部文档与源码默认留在授权边界内并可追查所有高风险操作。

## ADDED Requirements

### Requirement: Keep credentials out of knowledge storage and UI
系统 MUST 使用现有安全凭据能力保存 Git、禅道和 AI Provider 秘密，知识库数据库仅保存凭据引用，查询接口和日志不得返回密码、Token、Cookie、Session 或私钥明文。

#### Scenario: Save a Zentao connection
- **WHEN** 用户提交禅道密码或 Token
- **THEN** 秘密进入安全凭据存储，禅道连接表只保存 `credential_key`，前端后续仅看到掩码状态

### Requirement: Enforce source-scoped local access
系统 SHALL 要求本地文件和源码来源具有明确授权根目录，并 MUST 对路径规范化、路径穿越、符号链接和来源边界进行后端校验。

#### Scenario: Command requests arbitrary path
- **WHEN** 前端或 MCP 请求读取未登记来源中的任意绝对路径
- **THEN** 系统拒绝请求并写入不含文件正文的安全审计

### Requirement: Require layered remote processing authorization
系统 MUST 分别控制远程 Embedding 和远程聊天模型，并 SHALL 仅在系统总开关、来源开关、文档敏感级别和内容检查均允许时发送最小必要片段。

#### Scenario: Source disallows remote embedding
- **WHEN** 活动 Profile 为远程模式但来源的 `allow_remote_embedding` 为 false
- **THEN** 系统不得发送该来源片段，并将其标记为受策略阻断

#### Scenario: Local embedding fails
- **WHEN** 本地模型生成失败
- **THEN** 系统返回失败或允许用户重试，不得未经授权自动回退到远程模型

### Requirement: Detect and block sensitive content
系统 SHALL 在持久化、远程处理和 MCP 返回前检测私钥、证书、密码、Token、连接串、凭据文件和标记为 restricted 的内容。

#### Scenario: Source file contains a private key
- **WHEN** 源码扫描命中私钥规则
- **THEN** 系统默认只保存路径、哈希和阻断原因，不保存或发送秘密正文

#### Scenario: Sensitive detection is audited
- **WHEN** 内容被敏感规则阻断
- **THEN** 审计仅记录规则 ID、来源和处置结果，不记录命中的秘密

### Requirement: Use read-only and controlled MCP boundaries
系统 SHALL 默认仅通过 MCP 暴露项目、版本、搜索、文档详情、引用详情和问答等只读能力；任何写入、关系确认、远程处理或 Git 内容变更 MUST 使用受控操作和审计。

#### Scenario: MCP reads an allowed citation
- **WHEN** 已授权 MCP 客户端请求允许公开的知识引用
- **THEN** 系统返回经过权限和敏感过滤的内容及来源，不返回底层凭据

#### Scenario: MCP attempts direct Git write
- **WHEN** MCP 客户端试图通过只读知识工具修改 Git 来源
- **THEN** 系统拒绝操作，不把只读工具升级为隐式写入

### Requirement: Audit security-sensitive knowledge actions
系统 SHALL 审计项目/来源配置、同步、Profile 测试/激活/回滚、远程发送、禅道连接、代码快照、文档生成、关系确认、MCP 访问和 RAG 引用。

#### Scenario: Remote embedding batch completes
- **WHEN** 系统完成一批远程向量化
- **THEN** 审计记录 Provider、模型、来源、批次数、字符量、延迟和结果，但不记录原始正文或 API Key

### Requirement: Avoid executing analyzed source code
系统 MUST 将源码知识化作为只读静态分析；除非用户在独立受控流程明确发起，知识库分析不得执行仓库脚本、构建、测试、包管理器、Git Hook 或二进制。

#### Scenario: Repository contains build scripts
- **WHEN** 代码源包含安装脚本、构建脚本或 Git Hook
- **THEN** 系统仅将允许的文本作为静态内容分析，不执行这些文件
