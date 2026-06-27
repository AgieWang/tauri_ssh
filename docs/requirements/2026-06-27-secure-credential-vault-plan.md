# 安全凭证功能模块详细实施方案

**状态**: 实施中
**创建时间**: 2026-06-27
**目标版本**: v0.1.x / v0.2
**菜单位置**: 一级菜单模块 -> 安全凭证
**目标模块**: 安全凭证 / MCP Server / 审计日志 / 审批队列 / 工作区
**参考形态**: 凭证保险库、MCP 接入、凭证审计、Git 工作区

---

## 1. 背景与目标

当前应用已经具备 AI Provider、MCP Server、Skill 管理、审批队列、审计日志、服务器凭据保险库、工作区和 GitHub 推送等能力。下一步如果要让 AI Agent 更深入地操作 GitHub、GitLab、GitCode、HTTP API 和用户自定义服务，必须先解决一个核心问题：

> AI 需要调用服务能力，但不能拿到密码、Token、Cookie、私钥等凭证明文。

因此本方案新增 `安全凭证` 模块，将它作为与 `资产`、`运维`、`AI / MCP`、`治理` 平级的一级菜单模块。它不是简单的密码本，而是一个受控凭证代理系统：

- 本地保存多类型凭证。
- 后端负责加密、解密、轮换、会话签发和真实服务调用。
- MCP 工具只暴露受控能力，不暴露密钥明文。
- AI 只能拿到短期会话句柄或脱敏结果。
- 所有调用进入审批与审计体系。

一句话定位：

> 一个面向 AI Agent 和 MCP 工具的本地安全凭证代理，负责保存凭证、签发短期受控会话、代调用第三方服务，并保证明文永不返回给 AI。

---

## 2. 核心安全边界

### 2.1 必须坚持的边界

1. AI / MCP 永远不能获得真实凭证明文。
2. 前端页面不能从后端读取真实凭证明文。
3. 后端可以在受控服务层临时解密凭据，但只能用于执行指定 Provider Adapter 的请求。
4. MCP 返回值中不得包含 `password`、`token`、`secret`、`private_key`、`authorization`、`cookie` 等敏感字段原值。
5. 任何可能写入远端、创建资源、删除资源、修改配置的操作都必须进入策略判断。
6. 高风险操作必须进入审批队列。
7. 所有 MCP 凭据相关调用都必须写审计日志。

### 2.2 AI 可见对象

AI 可见的不是凭证明文，而是短期会话句柄：

```json
{
  "sessionId": "sess_01JZ...",
  "provider": "github",
  "credentialKey": "github-main",
  "scopes": ["repo:read", "issue:write"],
  "expiresAt": "2026-06-27T18:30:00+08:00",
  "disclosure": "session_only"
}
```

这个 `sessionId` 只在本机应用后端有效：

- 不能作为 GitHub / GitLab / GitCode Token 使用。
- 不能从会话反查明文。
- 过期后自动失效。
- 可被用户手动吊销。
- 只能调用授权范围内的 MCP 工具。

### 2.3 后端内部对象

后端内部可以在一次调用生命周期内短暂持有解密后的凭据：

```text
MCP tool call
  -> SecureCredentialService 校验
  -> SessionBroker 校验会话
  -> ProviderAdapter 内部解密并发起请求
  -> SecretRedactor 脱敏响应
  -> AuditService 记录摘要
  -> MCP 返回脱敏结果
```

解密后的凭据不得写入日志、审计、前端状态、MCP 返回值或错误信息。

---

## 3. 方案对比

### 3.1 方案 A：普通凭据保险库

只保存凭据，并在需要时让用户复制或让 AI 读取凭据。

优点：

- 实现简单。
- 与普通密码管理器类似。

缺点：

- AI 可能拿到明文。
- 无法控制 AI 后续如何使用凭据。
- 无法按服务能力做审计。
- 与 MCP 工具安全边界冲突。

结论：不采用。

### 3.2 方案 B：凭据保险库 + 会话代理

本地保存凭据，后端签发短期会话，AI/MCP 只拿会话句柄，真实请求由后端 Provider Adapter 代执行。

优点：

- 凭证明文不出 Rust 后端。
- MCP 工具可以按会话、范围、风险级别控制。
- 可以统一接入审批、审计、脱敏、限流。
- 适合 GitHub / GitLab / GitCode / HTTP API / 自定义服务扩展。

缺点：

- 需要实现 SessionBroker 和 Provider Adapter。
- 每种服务都要维护受控工具。

结论：首版推荐方案。

### 3.3 方案 C：系统 Keychain + 外部浏览器 OAuth

使用系统钥匙串保存凭据，OAuth 登录走浏览器授权，应用只保存 Refresh Token 或会话引用。

优点：

- 安全性最高。
- 更符合长期产品化方向。

缺点：

- 每个平台适配成本更高。
- OAuth 回调、刷新、撤销逻辑复杂。
- 首版开发量较大。

结论：作为 v0.2+ 增强方向，首版预留字段，不阻塞落地。

### 3.4 推荐结论

首版采用 **方案 B：凭据保险库 + 会话代理**，并预留系统 Keychain/OAuth 升级能力。

实现策略：

1. 基于现有 `credential_vault` 能力升级为 `secure_credentials` V2 模块。
2. 保留现有资产下 `凭据保险库` 的兼容入口，新增一级菜单模块 `安全凭证` 作为主入口。
3. 后端新增 `SecureCredentialService`、`SessionBroker`、`ProviderAdapter`、`SecretRedactor`。
4. MCP 工具不提供任何读取明文的方法，只提供会话创建和受控服务调用。

---

## 4. 功能范围

### 4.1 v0.1 必做

#### 凭证类型

- GitHub Token
- GitLab Token
- GitCode Token
- HTTP API Key
- HTTP Bearer Token
- Basic Auth
- 自定义凭证

#### 凭证管理

- 新建、编辑、删除、禁用、启用凭证。
- 凭证 Key 唯一。
- Provider 分类筛选。
- 标签、文件夹、备注。
- 过期时间和轮换提醒。
- 测试连接。
- 授权范围配置。
- 凭据状态：正常、即将过期、需轮换、禁用、测试失败。

#### 会话管理

- 手动创建短期会话。
- MCP 工具按需创建短期会话。
- 查看会话列表。
- 会话过期时间。
- 手动吊销会话。
- 会话绑定调用方、Provider、凭证 Key、权限范围。

#### MCP 工具

- 列出可用凭证元数据，不含明文。
- 创建安全会话。
- 查询会话状态。
- 吊销会话。
- GitHub / GitLab / GitCode 只读工具。
- HTTP API 受控请求工具。
- 工具调用写审计。

#### 审计

- 凭证新增、编辑、删除、禁用、启用。
- 凭证测试连接。
- 会话创建、过期、吊销。
- MCP 工具调用。
- Provider API 调用结果。
- 失败、拒绝、审批创建。
- 导出 CSV / JSON。

#### 页面

- 新增一级菜单模块 `安全凭证`。
- `安全凭证 -> 概览` 独立页面。
- `安全凭证 -> 凭证库` 独立页面。
- `安全凭证 -> 会话` 独立页面。
- `安全凭证 -> MCP 接入` 独立页面。
- `安全凭证 -> 审计` 独立页面。
- `安全凭证 -> 策略` 独立页面。
- 凭证新建/编辑/详情使用 Drawer，不作为独立菜单。

### 4.2 v0.2 应做

- OAuth Device Flow：GitHub / GitLab。
- 系统 Keychain 优先保存密钥种子或密文。
- 凭证自动轮换提醒。
- Provider 额度刷新。
- GitHub Issue / PR / Release 写操作审批流。
- GitHub 分支、文件提交、PR 创建、PR 合并、Tag、Release、Workflow 写操作审批流。
- GitLab Issue / MR 写操作审批流。
- GitLab 分支、文件提交、MR 创建、MR 合并、Tag、Release、Pipeline 写操作审批流。
- GitCode 仓库写操作审批流。
- GitCode 分支、文件提交、合并请求、Tag、Release 等仓库写操作审批流。
- HTTP API 响应字段级脱敏模板。
- 凭证使用统计趋势。
- MCP 客户端自动配置增强。

### 4.3 暂不做

- 不让 AI 获取真实 Token、Cookie、密码或私钥。
- 不做云同步。
- 不做团队多用户共享。
- 不做浏览器插件。
- 不做绕过第三方平台权限体系的调用。
- 不做无限制 HTTP 代理。
- 不允许 MCP 工具直接传入任意 URL 并携带任意凭证。

---

## 5. 菜单与页面设计

### 5.1 菜单结构

新增与 `工作台`、`资产`、`运维`、`AI / MCP`、`治理` 平级的一级菜单模块：

```text
安全凭证
├── 概览
├── 凭证库
├── 会话
├── MCP 接入
├── 审计
└── 策略
```

路由规划：

```text
/secure-credentials/overview
/secure-credentials/vault
/secure-credentials/sessions
/secure-credentials/mcp
/secure-credentials/audit
/secure-credentials/policies
```

图标建议：

- 一级菜单：`ShieldCheck` 或 `LockKeyhole`
- 概览：`LayoutDashboard`
- 凭证库：`KeyRound`
- 会话：`Clock3` 或 `Radio`
- MCP 接入：`PlugZap`
- 审计：`ScrollText`
- 策略：`SlidersHorizontal`

### 5.2 页面整体布局

采用一级菜单组 + 多独立页面。每个页面使用统一页头，不再在单页面内使用 Tabs 承载全部功能。

```text
安全凭证
为 AI/MCP 提供受控凭证会话，凭证明文只在本机后端内部使用。
```

页面路由之间通过左侧主菜单切换，不使用页面内二级 Tab。这样可以避免单页过重，也便于后续为不同页面独立做权限、加载状态、查询参数和浏览器刷新恢复。

按钮尺寸与现有应用保持一致：

- 普通按钮：90px x 30px。
- 主按钮：`新增凭证`，只出现在凭证库页和需要创建动作的页面。
- 表格操作按钮用 icon + tooltip。

### 5.3 概览页

参考截图中的概览页面，展示：

- 凭证总数。
- 14 天内过期。
- 本周调用次数。
- 授权成功率。
- 当前保险库状态：已解锁 / 已锁定。
- AI 启用状态。
- 最近凭证。
- 近 14 天调用趋势。
- 到期提醒。
- MCP 客户端接入状态。

卡片建议：

| 卡片 | 数据来源 |
| --- | --- |
| 凭证总数 | `secure_credentials` |
| 即将过期 | `expires_at <= now + 14 days` |
| 本周调用 | `secure_credential_audit_logs` |
| 授权成功率 | 成功调用 / 总调用 |
| MCP 接入 | `McpService::clients()` |

路由：

```text
/secure-credentials/overview
```

### 5.4 凭证库页

参考截图中的列表页：

左侧分类：

- 全部凭证。
- GitHub Token。
- GitLab Token。
- GitCode Token。
- HTTP API。
- 自定义。
- 禁用。
- 即将过期。

顶部工具栏：

- 搜索：凭据 Key / Provider / 账号 / 标签 / 备注。
- 最近使用排序。
- 刷新额度。
- 新建凭证。

表格列：

| 列 | 说明 |
| --- | --- |
| Key | 凭证唯一标识 |
| Provider | GitHub / GitLab / GitCode / HTTP API / Custom |
| 关联账号 | 用户名、组织或服务名 |
| 类型 | token / api_key / basic_auth / custom |
| 授权范围 | scope 摘要 |
| 最后使用 | last_used_at |
| 调用次数 | usage_count |
| 过期 | 永久 / 日期 / 即将过期 |
| 状态 | 正常 / 禁用 / 需轮换 |
| 操作 | 复制引用、测试、编辑、轮换、禁用、删除 |

说明：

- `复制引用` 只复制 `vault:<credential_key>` 或 `secure:<credential_key>`，不复制明文。
- `测试` 只返回连接状态和账号摘要。
- `轮换` 提交新密钥，不显示旧密钥。

路由：

```text
/secure-credentials/vault
```

### 5.5 新建/编辑凭证 Drawer

字段：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| Provider | Select | `github` / `gitlab` / `gitcode` / `http_api` / `custom` |
| 凭证 Key | Input | 全局唯一 |
| 显示名称 | Input | 页面展示 |
| 关联账号 | Input | 用户名、组织或服务账号 |
| 凭证类型 | Select | token / api_key / bearer / basic / custom |
| Secret | Password/TextArea | 只写，不回显 |
| API Base URL | Input | GitLab / GitCode / HTTP API 可配置 |
| 授权范围 | Checkbox/Select | repo/read/write/issue/release/custom |
| 允许 MCP 使用 | Switch | 默认关闭或按策略 |
| 需审批操作 | Select | 写操作、删除操作、所有操作 |
| 过期时间 | DatePicker | 可空 |
| 标签 | Select tags | 便于筛选 |
| 文件夹 | Select/Input | 分类 |
| 备注 | TextArea | 不允许填写密钥明文 |

编辑时：

- Secret 字段为空表示保留原密文。
- 如果填入新 Secret，则创建新版本并更新 `rotated_at`。
- 不允许显示旧 Secret。

Drawer 来源：

- 凭证库页点击 `新增凭证` 打开新建 Drawer。
- 凭证库页点击 `编辑` 打开编辑 Drawer。
- 凭证库页点击行详情打开详情 Drawer。

不新增 `/secure-credentials/form` 路由，避免表单页被刷新后残留半编辑状态。

### 5.6 会话页

展示短期会话：

| 列 | 说明 |
| --- | --- |
| Session ID | 脱敏展示 |
| Provider | 服务类型 |
| 凭证 Key | 关联凭证 |
| 调用方 | Claude Code / Codex / MCP Client / local-user |
| Scope | 会话范围 |
| 创建时间 | created_at |
| 过期时间 | expires_at |
| 状态 | active / expired / revoked |
| 操作 | 吊销、查看审计 |

会话详情展示：

- 调用来源。
- 已授权工具。
- 已调用次数。
- 最近调用摘要。
- 不展示明文。

路由：

```text
/secure-credentials/sessions
```

### 5.7 MCP 接入页

复用现有 MCP Server 页面逻辑：

- MCP Server 状态。
- Claude Code 接入状态。
- Codex 接入状态。
- Cursor / Continue 等预留。
- 一键写入配置。
- 测试连接。
- 展示可用安全凭证 MCP 工具。

说明：

- 该页面只管理安全凭证相关 MCP 工具的启用状态。
- 客户端配置仍由现有 `McpService` 统一生成。

路由：

```text
/secure-credentials/mcp
```

### 5.8 审计页

参考截图中的审计页：

概览：

- 总调用。
- 授权成功率。
- 失败次数。
- 被拒次数。

筛选：

- 关键词。
- 时间范围。
- 事件类型。
- Provider。
- 凭证 Key。
- 调用方。
- 结果。

列表维度：

- 时间线。
- 按调用方。
- 按凭证。
- 按能力。

审计项：

```json
{
  "actor": "mcp-client",
  "source": "secure_credential",
  "provider": "github",
  "credentialKey": "github-main",
  "action": "github_repos_list",
  "risk": "readonly",
  "result": "success",
  "durationMs": 381,
  "requestId": "req_...",
  "approvalId": null
}
```

路由：

```text
/secure-credentials/audit
```

### 5.9 策略页

用于配置 MCP 和 AI 使用凭证的默认策略：

- 默认会话 TTL。
- 最大返回行数。
- 是否允许只读自动执行。
- 写操作是否全部审批。
- 是否允许 HTTP API 自定义 Header。
- HTTP API 允许域名白名单。
- 响应脱敏规则。
- 单分钟调用限制。
- 单凭证并发会话限制。

路由：

```text
/secure-credentials/policies
```

---

## 6. 数据模型设计

### 6.1 表：secure_credentials

保存凭证元数据，不保存密文明文。

```sql
CREATE TABLE IF NOT EXISTS secure_credentials (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    credential_key      TEXT NOT NULL UNIQUE,
    display_name        TEXT NOT NULL,
    provider            TEXT NOT NULL,
    credential_type     TEXT NOT NULL,
    account_name        TEXT NOT NULL DEFAULT '',
    base_url            TEXT NOT NULL DEFAULT '',
    scope_json          TEXT NOT NULL DEFAULT '[]',
    tags_json           TEXT NOT NULL DEFAULT '[]',
    folder              TEXT NOT NULL DEFAULT '',
    description         TEXT NOT NULL DEFAULT '',
    status              TEXT NOT NULL DEFAULT 'active',
    enabled             INTEGER NOT NULL DEFAULT 1,
    allow_mcp           INTEGER NOT NULL DEFAULT 0,
    approval_policy     TEXT NOT NULL DEFAULT 'write_requires_approval',
    expires_at          TEXT DEFAULT NULL,
    last_used_at        TEXT DEFAULT NULL,
    usage_count         INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at          TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_secure_credentials_provider
ON secure_credentials(provider);

CREATE INDEX IF NOT EXISTS idx_secure_credentials_status
ON secure_credentials(status);

CREATE INDEX IF NOT EXISTS idx_secure_credentials_allow_mcp
ON secure_credentials(allow_mcp, enabled);
```

Provider 取值：

```text
github | gitlab | gitcode | http_api | custom
```

Credential Type 取值：

```text
token | api_key | bearer_token | basic_auth | custom_secret | session_reference
```

Status 取值：

```text
active | disabled | rotation_due | expired | test_failed
```

Approval Policy 取值：

```text
readonly_auto
write_requires_approval
all_requires_approval
blocked_for_mcp
```

### 6.2 表：secure_credential_secrets

保存加密密文和版本。

```sql
CREATE TABLE IF NOT EXISTS secure_credential_secrets (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    credential_key      TEXT NOT NULL,
    secret_version      INTEGER NOT NULL DEFAULT 1,
    secret_nonce        TEXT NOT NULL,
    secret_ciphertext   TEXT NOT NULL,
    secret_hint         TEXT NOT NULL DEFAULT '',
    secret_hash         TEXT NOT NULL DEFAULT '',
    active              INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    rotated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    FOREIGN KEY (credential_key) REFERENCES secure_credentials(credential_key)
);

CREATE INDEX IF NOT EXISTS idx_secure_credential_secrets_key_active
ON secure_credential_secrets(credential_key, active);
```

说明：

- `secret_hint` 只保存后四位或类似 `ghp_****abcd` 的脱敏提示。
- `secret_hash` 用于判断用户是否重复提交同一密钥，不用于反推明文。
- 同一凭证可以保留历史版本，但只有一个 active。

### 6.3 表：secure_credential_sessions

保存短期会话。

```sql
CREATE TABLE IF NOT EXISTS secure_credential_sessions (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id          TEXT NOT NULL UNIQUE,
    credential_key      TEXT NOT NULL,
    provider            TEXT NOT NULL,
    requester           TEXT NOT NULL,
    client_name         TEXT NOT NULL DEFAULT '',
    scopes_json         TEXT NOT NULL DEFAULT '[]',
    allowed_tools_json  TEXT NOT NULL DEFAULT '[]',
    status              TEXT NOT NULL DEFAULT 'active',
    expires_at          TEXT NOT NULL,
    revoked_at          TEXT DEFAULT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    last_used_at        TEXT DEFAULT NULL,
    usage_count         INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (credential_key) REFERENCES secure_credentials(credential_key)
);

CREATE INDEX IF NOT EXISTS idx_secure_credential_sessions_status
ON secure_credential_sessions(status, expires_at);

CREATE INDEX IF NOT EXISTS idx_secure_credential_sessions_key
ON secure_credential_sessions(credential_key);
```

Session Status：

```text
active | expired | revoked
```

### 6.4 表：secure_credential_policies

保存凭证级策略。

```sql
CREATE TABLE IF NOT EXISTS secure_credential_policies (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    credential_key        TEXT NOT NULL,
    allow_readonly        INTEGER NOT NULL DEFAULT 1,
    allow_write           INTEGER NOT NULL DEFAULT 0,
    allow_delete          INTEGER NOT NULL DEFAULT 0,
    require_approval_json TEXT NOT NULL DEFAULT '["write","delete"]',
    allowed_tools_json    TEXT NOT NULL DEFAULT '[]',
    allowed_domains_json  TEXT NOT NULL DEFAULT '[]',
    allowed_methods_json  TEXT NOT NULL DEFAULT '["GET"]',
    response_limit_bytes  INTEGER NOT NULL DEFAULT 65536,
    rate_limit_per_min    INTEGER NOT NULL DEFAULT 30,
    session_ttl_minutes   INTEGER NOT NULL DEFAULT 30,
    updated_at            TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    FOREIGN KEY (credential_key) REFERENCES secure_credentials(credential_key)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_secure_credential_policies_key
ON secure_credential_policies(credential_key);
```

### 6.5 表：secure_credential_audit_logs

可以独立建表，也可以复用现有 `audit_logs`。首版建议复用 `audit_logs`，但新增独立详情表便于凭证页面高效查询。

```sql
CREATE TABLE IF NOT EXISTS secure_credential_audit_logs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id          TEXT NOT NULL,
    actor               TEXT NOT NULL,
    source              TEXT NOT NULL,
    provider            TEXT NOT NULL,
    credential_key      TEXT NOT NULL,
    session_id_masked   TEXT NOT NULL DEFAULT '',
    tool_name           TEXT NOT NULL DEFAULT '',
    action              TEXT NOT NULL,
    risk                TEXT NOT NULL,
    result              TEXT NOT NULL,
    summary             TEXT NOT NULL,
    detail_json         TEXT NOT NULL DEFAULT '{}',
    duration_ms         INTEGER NOT NULL DEFAULT 0,
    approval_id         INTEGER DEFAULT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_secure_credential_audit_created
ON secure_credential_audit_logs(created_at);

CREATE INDEX IF NOT EXISTS idx_secure_credential_audit_key
ON secure_credential_audit_logs(credential_key);

CREATE INDEX IF NOT EXISTS idx_secure_credential_audit_provider
ON secure_credential_audit_logs(provider);
```

`detail_json` 必须经过脱敏处理。

---

## 7. 后端模块设计

### 7.1 Rust 文件规划

```text
src-tauri/src/commands/secure_credential.rs
src-tauri/src/services/secure_credential.rs
src-tauri/src/services/secure_session.rs
src-tauri/src/services/secure_provider/mod.rs
src-tauri/src/services/secure_provider/github.rs
src-tauri/src/services/secure_provider/gitlab.rs
src-tauri/src/services/secure_provider/gitcode.rs
src-tauri/src/services/secure_provider/http_api.rs
src-tauri/src/services/secret_redactor.rs
src-tauri/src/models/secure_credential.rs
```

如果现有 `models/mod.rs` 继续集中维护模型，可先把类型放入 `models/mod.rs`，后续再拆。

### 7.2 Command 层

新增 Tauri Commands：

```text
list_secure_credentials
get_secure_credential
upsert_secure_credential
delete_secure_credential
set_secure_credential_enabled
test_secure_credential
rotate_secure_credential
list_secure_credential_sessions
create_secure_credential_session
revoke_secure_credential_session
list_secure_credential_audit_logs
export_secure_credential_audit_logs
get_secure_credential_overview
```

Command 只负责：

- 接收前端参数。
- 调用 Service。
- 返回脱敏结果。

不得在 Command 中直接解密或请求第三方 API。

### 7.3 Service 层

#### SecureCredentialService

职责：

- 参数校验。
- 凭证 CRUD。
- Secret 加密保存。
- Secret 轮换。
- 测试连接调度。
- 统计概览。
- 写审计。

#### SecureSessionService

职责：

- 创建 session。
- 校验 session。
- 判断过期。
- 吊销 session。
- 限制工具范围。
- 更新使用统计。

#### SecureProviderService

职责：

- 根据 provider 分发到 adapter。
- 标准化请求与响应。
- 统一错误处理。
- 统一脱敏。

#### SecretRedactor

职责：

- 脱敏 MCP 参数。
- 脱敏 Provider 响应。
- 脱敏错误信息。
- 审计详情脱敏。

敏感 key 规则：

```text
password
passwd
pwd
token
secret
api_key
apikey
authorization
cookie
private_key
access_token
refresh_token
client_secret
```

### 7.4 Database 层

新增方法：

```text
list_secure_credentials(filter)
get_secure_credential(key)
upsert_secure_credential(input)
delete_secure_credential(key)
insert_secure_credential_secret(key, encrypted)
get_active_secure_credential_secret(key)
create_secure_session(input)
get_secure_session(session_id)
revoke_secure_session(session_id)
list_secure_sessions(filter)
create_secure_credential_audit(input)
list_secure_credential_audits(filter)
secure_credential_overview()
```

所有 SQL 使用参数绑定，不允许字符串拼接用户输入。

---

## 8. 加密与密钥管理

### 8.1 首版加密方案

复用现有 AES-256-GCM 思路：

- 每条 Secret 使用随机 nonce。
- 密文保存到 SQLite。
- 密钥种子保存在本地配置。
- Service 层统一封装加密/解密。

首版需要补强：

- `secret_hash` 只保存 SHA-256 摘要。
- 错误信息不包含密文、明文、Token 前缀完整值。
- 轮换时保留旧版本但默认只启用最新版本。

### 8.2 v0.2 Keychain 增强

后续优先使用系统安全存储：

- macOS Keychain。
- Windows Credential Manager。
- Linux Secret Service。

策略：

- 系统 Keychain 可用时，密钥种子放系统安全存储。
- SQLite 只保存密文。
- Keychain 不可用时 fallback 到当前本地加密方案，并在设置页提示安全等级。

### 8.3 解密生命周期

解密只允许发生在：

- 测试连接。
- Provider Adapter 发起请求。
- 轮换校验。

解密结果只存在于函数局部变量，不写日志，不返回前端。

---

## 9. Provider Adapter 设计

### 9.1 通用接口

Provider Adapter 抽象：

```text
test_connection(credential) -> TestResult
list_resources(session, input) -> ProviderResult
execute_action(session, action, input) -> ProviderResult
```

返回值必须经过：

```text
Provider raw response
  -> normalize
  -> redact
  -> limit size
  -> audit summary
  -> return
```

### 9.2 GitHub Adapter

首版能力：

- 测试 Token：读取当前用户。
- 列出仓库。
- 获取仓库详情、默认分支、权限摘要。
- 列出分支。
- 获取分支详情和最新 commit。
- 获取文件内容。
- 获取 commit 列表。
- 获取 Pull Request 列表。
- 列出 issue。
- 获取 release 列表。
- 获取 tag 列表。
- 查询 Actions 状态。

受控写操作能力：

- 创建 issue。
- 新建分支。
- 创建或更新文件并提交 commit。
- 批量提交多个文件。
- 创建 Pull Request。
- 更新 Pull Request 标题、描述、目标分支。
- 合并 Pull Request。
- 关闭 Pull Request。
- 创建 tag。
- 创建 release。
- 触发 workflow_dispatch。
- 取消 workflow run。

高风险写操作能力：

- 删除分支。
- 删除 tag。
- 删除 release。
- 向默认分支直接提交。
- 强制更新引用。
- 修改仓库设置或保护分支规则。

写操作策略：

- 默认需审批。
- 审批通过后执行。
- 对 `main`、`master`、`develop` 等保护分支直接提交默认禁止或强审批。
- 删除分支、删除 tag、删除 release 默认强审批。
- 强制更新引用默认禁止，除非用户在策略页显式开启。
- 审计记录请求摘要和结果 URL。

### 9.3 GitLab Adapter

首版能力：

- 测试 Token：读取当前用户。
- 列出项目。
- 获取项目详情、默认分支、权限摘要。
- 列出分支。
- 获取分支详情和最新 commit。
- 获取文件内容。
- 获取 commit 列表。
- 列出 issue。
- 列出 merge request。
- 列出 tag。
- 列出 release。
- 查询 pipeline 状态。

受控写操作能力：

- 创建 issue。
- 新建分支。
- 创建或更新文件并提交 commit。
- 批量提交多个文件。
- 创建 merge request。
- 更新 merge request 标题、描述、目标分支。
- 合并 merge request。
- 关闭 merge request。
- 创建 tag。
- 创建 release。
- 触发 pipeline。
- 取消 pipeline。

高风险写操作能力：

- 删除分支。
- 删除 tag。
- 删除 release。
- 向默认分支直接提交。
- 修改项目设置或保护分支规则。

需要支持自定义 GitLab base URL：

```text
https://gitlab.com
https://gitlab.company.com
```

### 9.4 GitCode Adapter

首版能力：

- 测试 Token。
- 列出仓库。
- 查询仓库详情。
- 获取默认分支和权限摘要。
- 列出分支。
- 获取分支详情和最新 commit。
- 获取文件内容。
- 获取 commit 列表。
- 列出 issue 或等价工单能力。
- 列出合并请求或等价 PR/MR 能力。
- 列出 tag。
- 列出 release。

受控写操作能力：

- 新建分支。
- 创建或更新文件并提交 commit。
- 批量提交多个文件。
- 创建 issue 或等价工单。
- 创建合并请求或等价 PR/MR。
- 合并合并请求。
- 创建 tag。
- 创建 release。

高风险写操作能力：

- 删除分支。
- 删除 tag。
- 删除 release。
- 向默认分支直接提交。
- 修改仓库设置或保护分支规则。

由于 GitCode API 可能存在企业实例差异，首版 base URL 必须可配置。

### 9.5 Git 仓库写操作通用约束

GitHub、GitLab、GitCode 的仓库写操作必须统一遵守以下约束：

1. AI/MCP 只提交结构化意图，不提交真实 Token。
2. 后端 Provider Adapter 根据 `sessionId` 代调用平台 API。
3. 每个写操作创建审批请求时必须固化参数摘要和 `request_hash`。
4. approved 后执行时必须重新计算 `request_hash`，不一致直接拒绝。
5. 文件提交必须记录文件路径、目标分支、base commit SHA、content SHA-256 和 commit message。
6. 合并 PR/MR 必须记录源分支、目标分支、PR/MR 编号、合并方式和当前 head SHA。
7. 创建分支必须记录来源分支或来源 commit SHA。
8. 删除分支、删除 tag、删除 release 属于高风险操作。
9. 对默认分支直接 commit 默认禁止，推荐走新分支 + PR/MR。
10. 保护分支规则由平台侧和本应用策略双重约束，本应用不得绕过平台保护规则。

写操作输入必须支持 dry-run 预览：

```json
{
  "sessionId": "sess_...",
  "provider": "github",
  "owner": "AgieWang",
  "repo": "tauri_ssh",
  "operation": "file_commit",
  "dryRun": true,
  "targetBranch": "feature/secure-credentials",
  "baseBranch": "master",
  "files": [
    {
      "path": "README.md",
      "contentSha256": "..."
    }
  ]
}
```

dry-run 只返回计划和风险，不写远端。

### 9.6 HTTP API Adapter

HTTP API 是最高风险入口，必须限制：

- 只能请求凭证配置中允许的域名。
- 默认只允许 GET。
- POST / PUT / PATCH / DELETE 默认需审批。
- Header 只能来自模板，不允许 AI 传入任意 Authorization。
- 返回体大小默认限制 64KB。
- 响应 JSON 做敏感字段脱敏。

HTTP API 配置字段：

```json
{
  "baseUrl": "https://api.example.com",
  "allowedDomains": ["api.example.com"],
  "allowedMethods": ["GET"],
  "defaultHeaders": {
    "Accept": "application/json"
  },
  "authPlacement": "header",
  "authHeaderName": "Authorization",
  "authHeaderTemplate": "Bearer {{secret}}"
}
```

AI/MCP 调用时只能传：

- path。
- query。
- body。
- method。

不能传真实认证 Header。

### 9.7 Custom Adapter

自定义凭证不直接开放任意调用。

首版只支持：

- 保存。
- 分类。
- 测试脚本预留。
- 会话引用。

v0.2 再支持用户定义工具模板。

---

## 10. MCP 工具设计

### 10.1 工具分批

#### 第一批：安全凭证元数据与会话

```text
secure_credentials_list
secure_credential_detail
secure_session_create
secure_session_status
secure_session_revoke
secure_credential_audit_list
```

#### 第二批：Provider 只读工具

```text
github_repos_list
github_repo_detail
github_branches_list
github_file_read
github_commits_list
github_pull_requests_list
github_issues_list
github_releases_list
github_tags_list
gitlab_projects_list
gitlab_project_detail
gitlab_branches_list
gitlab_file_read
gitlab_commits_list
gitlab_issues_list
gitlab_merge_requests_list
gitlab_releases_list
gitlab_tags_list
gitcode_repos_list
gitcode_repo_detail
gitcode_branches_list
gitcode_file_read
gitcode_commits_list
gitcode_merge_requests_list
http_api_request_readonly
```

#### 第三批：受控写操作

```text
github_issue_create_controlled
github_branch_create_controlled
github_file_commit_controlled
github_pull_request_create_controlled
github_pull_request_update_controlled
github_pull_request_merge_controlled
github_tag_create_controlled
github_release_create_controlled
github_workflow_dispatch_controlled
gitlab_issue_create_controlled
gitlab_branch_create_controlled
gitlab_file_commit_controlled
gitlab_merge_request_create_controlled
gitlab_merge_request_update_controlled
gitlab_merge_request_merge_controlled
gitlab_tag_create_controlled
gitlab_release_create_controlled
gitlab_pipeline_trigger_controlled
gitcode_issue_create_controlled
gitcode_branch_create_controlled
gitcode_file_commit_controlled
gitcode_merge_request_create_controlled
gitcode_merge_request_merge_controlled
gitcode_tag_create_controlled
gitcode_release_create_controlled
http_api_request_controlled
secure_credential_rotate_request
```

#### 第四批：高风险仓库操作

```text
github_branch_delete_controlled
github_tag_delete_controlled
github_release_delete_controlled
github_ref_update_controlled
github_repository_settings_update_controlled
gitlab_branch_delete_controlled
gitlab_tag_delete_controlled
gitlab_release_delete_controlled
gitlab_project_settings_update_controlled
gitcode_branch_delete_controlled
gitcode_tag_delete_controlled
gitcode_release_delete_controlled
gitcode_repository_settings_update_controlled
```

第四批工具默认不自动执行，只能创建审批请求。是否允许 approved 后执行由策略页开关控制；未开启时即使审批通过也应拒绝执行，并提示“当前策略禁止此类高风险仓库操作”。

### 10.2 secure_session_create

输入：

```json
{
  "credentialKey": "github-main",
  "requester": "codex",
  "scopes": ["repo:read"],
  "ttlMinutes": 30
}
```

输出：

```json
{
  "sessionId": "sess_01JZ...",
  "provider": "github",
  "credentialKey": "github-main",
  "scopes": ["repo:read"],
  "expiresAt": "2026-06-27T18:30:00+08:00",
  "credentialDisclosure": "not_returned"
}
```

拒绝条件：

- 凭证不存在。
- 凭证禁用。
- `allow_mcp=false`。
- scope 不在允许范围内。
- 凭证已过期。
- 策略要求审批但未审批。

### 10.3 Provider 工具调用模式

所有 Provider 工具必须要求 `sessionId`：

```json
{
  "sessionId": "sess_01JZ...",
  "owner": "AgieWang",
  "repo": "tauri_ssh",
  "state": "open"
}
```

内部流程：

1. 校验 session。
2. 校验 session scope。
3. 校验工具权限。
4. 内部解密凭证。
5. Provider Adapter 发起请求。
6. 响应脱敏。
7. 写审计。
8. 返回脱敏业务结果。

### 10.4 禁止提供的 MCP 工具

以下工具永远不实现：

```text
get_credential_secret
decrypt_credential
read_token
read_password
read_cookie
export_credentials_plaintext
```

如果用户需要迁移凭据，应提供加密备份/恢复，而不是明文导出。

---

## 11. 策略、审批与审计

### 11.1 风险分级

| 风险 | 示例 | 默认动作 |
| --- | --- | --- |
| readonly | list repos, list issues, GET HTTP | 自动执行 |
| low | 测试连接、刷新额度 | 自动执行并审计 |
| medium | 创建 issue、创建分支、创建 PR/MR、触发 pipeline | 创建审批 |
| high | 合并 PR/MR、提交文件、创建 release、删除 release、DELETE HTTP | 创建审批或拒绝 |
| blocked | 导出明文、读取 token | 直接拒绝 |

### 11.2 审批规则

需审批操作：

- 写 GitHub issue / release。
- GitHub 新建分支、提交文件、创建 PR、合并 PR、创建 tag、触发 Actions。
- GitHub 删除分支、删除 tag、删除 release、修改仓库设置。
- 触发 GitHub Actions。
- 写 GitLab issue / MR。
- GitLab 新建分支、提交文件、创建 MR、合并 MR、创建 tag、创建 release。
- GitLab 删除分支、删除 tag、删除 release、修改项目设置。
- 触发 GitLab pipeline。
- GitCode 新建分支、提交文件、创建合并请求、合并请求、创建 tag、创建 release。
- GitCode 删除分支、删除 tag、删除 release、修改仓库设置。
- HTTP API 非 GET 方法。
- 用户策略设置为 `all_requires_approval` 的所有操作。
- 凭证轮换请求。

审批请求内容必须包含：

- Provider。
- 凭证 Key。
- 工具名。
- 请求摘要。
- 风险级别。
- 参数脱敏快照。
- 预计影响。
- `request_hash`。
- 对 Git 仓库写操作，还必须包含 owner/project、repo、源分支、目标分支、base/head commit SHA、文件路径列表、文件内容 SHA-256、PR/MR 编号或 release/tag 名称。

### 11.3 审计规则

所有安全凭证动作必须写审计：

| 动作 | source | risk |
| --- | --- | --- |
| 新增凭证 | secure_credential | L2 |
| 编辑凭证 | secure_credential | L2 |
| 删除凭证 | secure_credential | L3 |
| 测试连接 | secure_credential | readonly |
| 创建会话 | mcp / secure_credential | L1 |
| 吊销会话 | secure_credential | L1 |
| Provider 只读请求 | mcp | readonly |
| Provider 写请求审批 | mcp | L2/L3 |
| 明文读取尝试 | mcp | blocked |

审计字段中禁止出现明文。

---

## 12. 前端 API 与类型

### 12.1 TypeScript 类型

新增文件：

```text
src/types/secureCredential.ts
src/lib/api/secureCredential.ts
```

核心类型：

```ts
export type SecureCredentialProvider =
  | "github"
  | "gitlab"
  | "gitcode"
  | "http_api"
  | "custom";

export type SecureCredentialType =
  | "token"
  | "api_key"
  | "bearer_token"
  | "basic_auth"
  | "custom_secret"
  | "session_reference";

export type SecureCredentialStatus =
  | "active"
  | "disabled"
  | "rotation_due"
  | "expired"
  | "test_failed";
```

API 封装：

```text
secureCredentialApi.list()
secureCredentialApi.get(key)
secureCredentialApi.upsert(input)
secureCredentialApi.delete(key)
secureCredentialApi.setEnabled(key, enabled)
secureCredentialApi.test(key)
secureCredentialApi.rotate(input)
secureCredentialApi.overview()
secureCredentialApi.listSessions(filter)
secureCredentialApi.createSession(input)
secureCredentialApi.revokeSession(sessionId)
secureCredentialApi.listAuditLogs(filter)
secureCredentialApi.exportAuditLogs(filter)
```

### 12.2 Dev HTTP API

为浏览器调试增加 Dev API：

```text
GET    /dev-api/secure-credentials/overview
GET    /dev-api/secure-credentials
POST   /dev-api/secure-credentials
GET    /dev-api/secure-credentials/:key
DELETE /dev-api/secure-credentials/:key
POST   /dev-api/secure-credentials/:key/enabled
POST   /dev-api/secure-credentials/:key/test
POST   /dev-api/secure-credentials/:key/rotate
GET    /dev-api/secure-credentials/sessions
POST   /dev-api/secure-credentials/sessions
POST   /dev-api/secure-credentials/sessions/:session_id/revoke
POST   /dev-api/secure-credentials/provider/repositories
POST   /dev-api/secure-credentials/provider/http-readonly
POST   /dev-api/secure-credentials/provider/http-write
POST   /dev-api/secure-credentials/provider/git-write
GET    /dev-api/secure-credentials/audit-logs
POST   /dev-api/secure-credentials/audit-logs/export
```

Dev API 与 Tauri Command 必须调用同一套 Service，不允许实现两套逻辑。

---

## 13. 与现有模块的关系

### 13.1 现有凭据保险库

当前 `资产 -> 凭据保险库` 已有基础凭据加密保存能力。新模块不应直接删除旧模块，建议分阶段处理：

阶段一：

- 保留旧页面。
- 新增一级菜单模块 `安全凭证`。
- 新模块可以读取旧凭据并提示迁移。

阶段二：

- 旧页面顶部提示：`安全凭证已提供更完整的 AI/MCP 凭证治理能力`。
- 支持一键迁移。

阶段三：

- 旧页面变成兼容入口或跳转。

### 13.2 MCP Server

现有 MCP 工具列表需要追加安全凭证工具权限说明：

- 工具名。
- 策略。
- 审计。
- 是否需要 session。

MCP 配置仍由现有 `McpService` 管理。

### 13.3 审批队列

安全凭证写操作复用现有 `ApprovalService`：

- 创建审批。
- 用户确认。
- approved 后执行。
- 拒绝后返回明确原因。

### 13.4 审计日志

安全凭证审计同时写：

- 通用 `audit_logs`。
- 凭证专用 `secure_credential_audit_logs`。

通用审计页可看到所有事件；`安全凭证 -> 审计` 页面只筛选本模块事件。

### 13.5 Skill 管理

后续可新增内置 Skill：

- `secure-credential-guard`
- `github-agent-helper`
- `http-api-safe-caller`

这些 Skill 用于约束 AI 不要索要明文凭据，并优先调用安全凭证 MCP 工具。

---

## 14. 实施步骤

### 阶段 1：文档与骨架

- ☑️ 新增本实施方案文档。
- ☑️ 新增一级菜单模块 `安全凭证`。
- ☑️ 新增独立子菜单：`概览`、`凭证库`、`会话`、`MCP 接入`、`审计`、`策略`。
- ☑️ 新增空页面 `/secure-credentials/overview`。
- ☑️ 新增空页面 `/secure-credentials/vault`。
- ☑️ 新增空页面 `/secure-credentials/sessions`。
- ☑️ 新增空页面 `/secure-credentials/mcp`。
- ☑️ 新增空页面 `/secure-credentials/audit`。
- ☑️ 新增空页面 `/secure-credentials/policies`。
- ☑️ 定义 TypeScript 类型。
- ☑️ 新增 Rust model 类型。

验收：

- 页面可进入。
- 标题和空状态正确。
- `pnpm exec tsc --noEmit` 通过。

### 阶段 2：数据库与后端 CRUD

- ☑️ 新增 SQLite 表迁移。
- ☑️ 新增 Database DAO。
- ☑️ 新增 `SecureCredentialService`。
- ☑️ 新增 Commands。
- ☑️ 新增 Dev API。
- ☑️ 实现凭证列表、新增、编辑、删除、启用、禁用。
- ☑️ 实现 Secret 加密保存和轮换。

验收：

- Secret 不回显。
- 编辑不填 Secret 时保留原密文。
- 删除为软删除。
- `cargo check` 通过。
- `cargo test` 覆盖加密/轮换逻辑。

### 阶段 3：前端安全凭证页面组

- ☑️ `安全凭证 -> 概览` 页面。
- ☑️ `安全凭证 -> 凭证库` 页面。
- ☑️ 新建/编辑 Drawer。
- ☑️ 测试连接按钮。
- ☑️ 轮换 Modal。
- ☑️ `安全凭证 -> 策略` 页面。
- ☑️ `安全凭证 -> 审计` 页面。
- ☑️ 审计页支持概览统计、关键词/Provider/结果筛选和明细表格。
- ☑️ MCP 接入页展示安全凭证 MCP 工具权限、策略和审计说明。
- ☑️ 策略页支持默认会话 TTL、最大返回条数、只读自动执行、全部审批、HTTP 白名单、限流、并发会话和默认分支直提策略。
- ☑️ 暗色主题适配。

验收：

- GitHub / GitLab / GitCode / HTTP API / Custom 类型均可创建。
- 表格筛选、搜索、状态显示正常。
- Secret 不在页面回显。
- 浏览器页面截图无重叠、无文字不可读。

### 阶段 4：SessionBroker

- ☑️ 新增 `secure_credential_sessions`。
- ☑️ 创建 session。
- ☑️ 校验 session。
- ☑️ 吊销 session。
- ☑️ session 过期清理。
- ☑️ `安全凭证 -> 会话` 页面。

验收：

- session 到期不可继续调用。
- revoked session 不可继续调用。
- session 返回值不含明文。

### 阶段 5：Provider Adapter

- ☑️ GitHub 测试连接。
- ☑️ GitHub 只读工具。
- ☑️ GitLab 测试连接。
- ☑️ GitLab 只读工具。
- ☑️ GitCode 测试连接。
- ☑️ GitCode 只读工具。
- ☑️ 通用 Git 只读 Provider 工具 `secure_git_readonly_request`，覆盖仓库详情、分支、文件、commit、PR/MR、issue、tag、release。
- ☑️ HTTP API 只读请求。
- ☑️ HTTP API 域名白名单校验。
- ☑️ 单凭证单分钟调用限流。
- ☑️ 响应脱敏。

验收：

- 真实 Token 可测试连接。
- 返回账号摘要，不返回 Token。
- HTTP API 不允许越过域名白名单。

### 阶段 6：MCP 工具

- ☑️ MCP tools/list 增加安全凭证工具 schema。
- ☑️ `secure_credentials_list`。
- ☑️ `secure_credential_detail`。
- ☑️ `secure_session_create`。
- ☑️ `secure_session_status`。
- ☑️ `secure_session_revoke`。
- ☑️ `secure_credential_audit_list`。
- ☑️ GitHub/GitLab/GitCode 语义化只读工具：仓库、详情、分支、文件、commit、PR/MR、issue、tag、release。
- ☑️ `secure_git_readonly_request`。
- ☑️ HTTP API 只读工具。
- ☑️ `http_api_request_readonly`。
- ☑️ MCP 接入页工具权限列表展示实施方案中的显式工具名，并分页显示。
- ☑️ MCP 调用审计。
- ☑️ 安全凭证专用审计 `secure_credential_audit_logs` 与通用 `audit_logs` 双写。

验收：

- MCP 客户端可发现工具。
- MCP 工具可被调用。
- MCP 返回不含明文。
- 审计日志可查到调用。

### 阶段 7：受控写操作与审批

- ☑️ GitHub issue 创建审批流。
- ☑️ GitHub 分支创建审批流。
- ☑️ GitHub 文件提交审批流。
- ☑️ GitHub Pull Request 创建、更新、合并审批流。
- ☑️ GitHub tag / release 创建审批流。
- ☑️ GitHub workflow_dispatch 审批流。
- ☑️ GitLab issue 创建审批流。
- ☑️ GitLab 分支创建审批流。
- ☑️ GitLab 文件提交审批流。
- ☑️ GitLab Merge Request 创建、更新、合并审批流。
- ☑️ GitLab tag / release 创建审批流。
- ☑️ HTTP API 非 GET 审批流。
- ☑️ GitCode 分支创建审批流。
- ☑️ GitCode 文件提交审批流。
- ☑️ GitCode 合并请求创建、合并审批流。
- ☑️ GitCode tag / release 创建审批流。
- ☑️ 高风险仓库操作策略开关：删除分支、删除 tag、删除 release、更新 Git ref、修改仓库设置。
- ☑️ `github_ref_update_controlled` 创建审批请求，approved 后通过 `update_ref` 执行并受策略开关控制。
- ☑️ 默认/保护分支直接提交策略开关，默认拒绝。
- ☑️ approved 后执行。
- ☑️ rejected 返回明确拒绝原因。

验收：

- 未审批写操作不会执行。
- 审批通过后只执行审批绑定的请求。
- 请求参数 hash 或摘要必须匹配，防止审批后替换参数。
- 文件提交必须校验目标分支、base commit SHA、文件路径和内容 SHA-256。
- PR/MR 合并必须校验 PR/MR 编号、源分支、目标分支和 head SHA。
- 默认分支直接提交默认拒绝，除非策略页显式允许且审批通过。
- 删除分支、删除 tag、删除 release 默认拒绝或强审批。

### 阶段 8：验证与发布准备

- ☑️ `cargo check`。
- ☑️ `cargo test`。
- ☑️ `pnpm exec tsc --noEmit`。
- ☑️ `pnpm build`。
- ☑️ 浏览器验证 `/secure-credentials/overview`。
- ☑️ 浏览器验证 `/secure-credentials/vault`。
- ☑️ 浏览器验证 `/secure-credentials/sessions`。
- ☑️ 浏览器验证 `/secure-credentials/mcp`。
- ☑️ 浏览器验证 `/secure-credentials/audit`。
- ☑️ 浏览器验证 `/secure-credentials/policies`。
- ☑️ MCP `tools/list` 验证。
- ☑️ MCP 工具调用验证。
- ☑️ 审计日志验证。
- ☑️ 暗色主题验证。

---

## 15. 验收标准

### 15.1 功能验收

- 可以新增 GitHub / GitLab / GitCode / HTTP API / Custom 凭证。
- 可以测试 GitHub / GitLab / GitCode 连接。
- 可以通过 MCP 读取 GitHub / GitLab / GitCode 仓库、分支、文件、commit、PR/MR、issue、tag、release。
- 可以通过 MCP 创建 GitHub / GitLab / GitCode 分支。
- 可以通过 MCP 提交 GitHub / GitLab / GitCode 文件变更。
- 可以通过 MCP 创建和合并 GitHub Pull Request、GitLab Merge Request、GitCode 合并请求。
- 可以通过 MCP 创建 GitHub / GitLab / GitCode tag 和 release。
- 可以创建短期会话。
- 可以在会话列表看到 active / expired / revoked。
- 可以通过 MCP 工具创建会话。
- 可以通过 MCP 工具调用只读 Provider 能力。
- 可以查看安全凭证审计。

### 15.2 安全验收

- 前端任何接口不返回明文。
- MCP 任何接口不返回明文。
- 审计日志不保存明文。
- 错误信息不包含明文。
- 明文读取类工具不存在。
- HTTP API 不能访问白名单外域名。
- 写操作未审批不能执行。
- Git 仓库写操作不能绕过 session、scope、审批和 request_hash 校验。
- AI Agent 不能直接获得 GitHub / GitLab / GitCode Token，也不能获得可复用的 Authorization Header。

### 15.3 UI 验收

- 页面标题统一 24px。
- 按钮高度统一 30px。
- 表格列不严重挤压。
- 暗色主题可读。
- Drawer 表单字段联动清晰。
- 空状态说明明确。

### 15.4 MCP 验收

- `tools/list` 包含安全凭证工具。
- 每个工具 schema 参数完整。
- 工具调用失败返回结构化错误。
- 工具调用成功写审计。
- 工具返回值经过脱敏。

---

## 16. 风险与规避

### 16.1 凭据泄露风险

风险：

- 日志、错误、审计、MCP 返回中误带明文。

规避：

- 所有外发结果统一走 `SecretRedactor`。
- 审计写入前统一脱敏。
- Provider Adapter 不允许直接返回原始响应。
- 单元测试覆盖敏感字段脱敏。

### 16.2 HTTP API 被滥用

风险：

- AI 利用 HTTP API 凭据请求任意地址。

规避：

- 域名白名单。
- 方法白名单。
- Header 模板固定。
- 响应大小限制。
- 非 GET 默认审批。

### 16.3 会话被长期使用

风险：

- AI 保留 sessionId 长期调用。

规避：

- 默认 TTL 30 分钟。
- 最大 TTL 限制。
- 用户可吊销。
- 过期自动拒绝。

### 16.4 审批后参数被替换

风险：

- 创建审批时是安全参数，执行时换成危险参数。

规避：

- 审批记录保存参数 hash。
- approved 执行时重新计算 hash。
- 不匹配直接拒绝并写审计。

### 16.5 Git 仓库写操作误操作

风险：

- AI 创建错误分支、提交错误文件、合并错误 PR/MR，或向默认分支直接提交。

规避：

- 写操作默认 dry-run 预览。
- 审批内容必须展示 repo、源分支、目标分支、文件路径、commit message、PR/MR 编号和影响摘要。
- 执行前校验 base/head commit SHA，发现远端已变化则拒绝执行并要求重新审批。
- 默认分支直接提交默认禁止。
- 合并 PR/MR 默认要求平台侧可合并状态为 clean / mergeable。

### 16.5 旧凭据迁移混乱

风险：

- 现有服务器凭据和新安全凭证语义混淆。

规避：

- 新模块使用 `secure:<key>` 引用。
- 旧模块继续使用 `vault:<key>`。
- 迁移时显式选择用途：SSH 凭据 / AI-MCP 服务凭据。

---

## 17. 后续扩展方向

- 系统 Keychain。
- OAuth Device Flow。
- Gitee / Bitbucket / Jira / Notion / Slack Provider。
- TOTP 一次性验证码托管。
- 团队共享与组织策略。
- 凭证健康检查。
- 凭证过期系统通知。
- AI 根据凭证能力自动推荐 MCP 工具。
- 工作区 Git 操作自动选择对应凭证会话。
- GitHub / GitLab / GitCode 仓库级 MCP 工具与本地工作区页面联动，支持从工作区 diff 生成 PR/MR。
- 分支保护规则可视化和策略同步。

---

## 18. 推荐开发顺序

推荐按以下最小可交付单元推进：

1. 页面空壳 + 菜单。
2. 数据库 schema + CRUD。
3. 加密保存 + 不回显。
4. GitHub 测试连接。
5. 会话签发。
6. MCP `secure_session_create`。
7. MCP GitHub 只读工具。
8. 审计补齐。
9. GitLab / GitCode。
10. HTTP API 白名单请求。
11. GitHub 分支、文件提交、PR、合并、tag、release 受控写操作。
12. GitLab 分支、文件提交、MR、合并、tag、release 受控写操作。
13. GitCode 分支、文件提交、合并请求、tag、release 受控写操作。
14. 高风险仓库操作策略开关和强审批。

这样每一阶段都能被真实验证，不会一次性堆出大模块后难以定位问题。
