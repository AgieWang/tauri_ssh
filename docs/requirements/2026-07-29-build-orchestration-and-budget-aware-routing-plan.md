# 多构建后端与 GitHub Actions 预算感知调度详细实施方案

**状态**: 规划稿  
**创建时间**: 2026-07-29  
**目标版本**: v0.3 首版，v0.4 / v0.5 增强  
**目标模块**: 构建中心 / GitHub Actions / Self-hosted Runner / Jenkins / 安全凭证 / 审批队列 / 审计日志 / MCP Server  
**适用项目**: Tauri SSH 及其后续纳管的 Tauri 桌面应用  

**参考资料**:

- GitHub 服务条款（Account Terms）: https://docs.github.com/en/site-policy/github-terms/github-terms-of-service#b-account-terms
- GitHub Actions 计费: https://docs.github.com/en/billing/concepts/product-billing/github-actions
- GitHub Billing Usage REST API: https://docs.github.com/en/rest/billing/usage
- GitHub Workflow Dispatch REST API: https://docs.github.com/en/rest/actions/workflows#create-a-workflow-dispatch-event
- GitHub Self-hosted Runner: https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/about-self-hosted-runners
- Jenkins Remote Access API: https://www.jenkins.io/doc/book/using/remote-access-api/
- Tauri Updater 签名: https://v2.tauri.app/plugin/updater/

---

## 1. 执行结论

本方案不实现“自动注册多个免费 GitHub 账号，并在额度耗尽时轮换账号继续消耗免费额度”。

原因如下：

1. GitHub 要求账号由人创建，禁止机器人自动注册账号。
2. GitHub 对个人、法人和机器账号数量存在明确限制。
3. Actions 免费额度归属于仓库所有者，而不是发起 API 调用时使用的 Token。
4. 单纯把账号 A 的 Token 切换为账号 B，不会让账号 A 名下私有仓库改用账号 B 的 Actions 额度。
5. 若为了切换额度而把同一源码镜像到多个免费账号仓库，会引入账号封禁、源码泄露、签名私钥扩散、Release 产物错配和供应链污染风险。

本方案采用合规替代：

> 在 Tauri SSH 中建设统一“构建中心”，管理用户主动授权的 GitHub 身份和组织，并在 GitHub-hosted、GitHub self-hosted、Jenkins 三类合法构建后端之间进行预算感知、健康感知和平台感知的自动路由。

默认路由顺序：

1. 在线且可信的 GitHub self-hosted Runner。
2. 在线且具备目标平台 Agent 的 Jenkins。
3. 仍有安全余额的 GitHub-hosted Runner。
4. 等待额度重置，或经过 L3 人工审批后允许付费构建。

首版不自动购买额度、不自动公开仓库、不复制源码到其他免费账号、不复制 Tauri updater 私钥。

---

## 2. 背景与现状

### 2.1 当前发布流程

当前 `.github/workflows/release.yml` 在推送版本 Tag 后执行桌面端构建矩阵：

- Windows x64：NSIS 安装包。
- macOS Apple Silicon：APP、DMG 和 updater 产物。
- macOS Intel：APP、DMG 和 updater 产物。
- Linux x64：DEB、AppImage 和 updater 产物。
- 可选 Android：APK、AAB。

构建完成后，由 GitHub Actions 创建 Draft Release。随后从 GitHub Release 下载产物，校验签名，再同步到 Gitee Release 仓库并生成 `update.json`。

### 2.2 当前可以复用的能力

项目已经具备以下基础，不应重复建设：

| 现有能力 | 主要位置 | 本方案复用方式 |
|---|---|---|
| 安全凭证加密存储 | `services/secure_credential.rs`、`secure_credentials` 表 | GitHub/Jenkins Token 只保存为 `credentialKey` 引用 |
| GitHub Provider Adapter | `services/secure_credential.rs` | 复用 GitHub API 请求头、脱敏、审计和策略校验 |
| GitHub workflow dispatch | `trigger_workflow` 写操作 | 作为 GitHub Actions 构建触发底座 |
| Git 工作区 | `services/git_workspace.rs` | 关联本地项目、远端仓库和 GitHub 凭证 |
| Jenkins 连接与构建 | `services/jenkins.rs` | 复用连接、Job、构建触发、队列、日志和 artifact |
| 审批队列 | `services/approval.rs`、`approval_requests` 表 | 构建、取消、付费放行使用现有审批模型 |
| 审计日志 | `audit_logs`、安全凭证审计 | 记录调度决策、API 调用和构建结果 |
| MCP Server | `services/mcp.rs`、`dev_server/mod.rs` | 暴露只读查询和 controlled/approved 两段式写工具 |
| Tauri updater | `tauri.conf.json`、签名 Secrets | 保持现有公钥、签名格式和 `update.json` 合约 |

### 2.3 当前问题

1. GitHub-hosted Runner 的免费分钟有限。
2. macOS 托管 Runner 单位成本明显高于 Windows 和 Linux。
3. 当前工作流构建矩阵固定，不能按预算和平台需求动态选择。
4. GitHub Actions、Jenkins 和未来自托管 Runner 彼此独立，缺少统一调度。
5. 用户无法在发起构建前看到预计耗时、预算影响和回退路径。
6. Workflow Dispatch 成功只代表请求已接收，不代表已取得具体 Run ID。
7. 多平台部分成功后，缺少统一的逻辑构建请求和分平台尝试记录。
8. 额度查询可能因账号类型、API 权限或 GitHub 计费模式变化而不可用，不能把“额度未知”误判为“额度充足”。

---

## 3. 产品定位

### 3.1 一句话定位

> 一个继承 Tauri SSH 安全凭证、审批和审计体系的多构建后端调度中心，按项目、平台、预算、健康状态和策略自动选择 GitHub-hosted、GitHub self-hosted 或 Jenkins，并统一跟踪构建与发布产物。

### 3.2 目标

1. 支持用户主动绑定多个合法 GitHub 账号或组织。
2. 支持 GitHub-hosted、GitHub self-hosted、Jenkins 三类构建后端。
3. 按平台能力、健康状态、预算、安全阈值和优先级自动选路。
4. 构建前提供 Dry-run，明确展示选择理由和拒绝理由。
5. 额度不足时自动回退到 self-hosted 或 Jenkins。
6. 所有远端写操作进入审批和审计。
7. 统一记录逻辑构建请求、分平台尝试、外部 Run、产物和签名校验结果。
8. 保持现有 GitHub Draft Release 和 Gitee updater 发布链路兼容。
9. UI 与 MCP 复用同一套 Service 层策略，MCP 不拥有额外权限。
10. 构建调度具备幂等、并发预算预留、故障恢复和断点续踪能力。

### 3.3 非目标

首版明确不做：

- 自动注册 GitHub 账号。
- 为规避免费额度而建立账号池。
- 自动把同一私有源码镜像到多个免费账号。
- 自动把私有仓库改为公开仓库。
- 自动购买或提高 GitHub Actions 预算。
- 自动创建或编辑完整 GitHub Actions Workflow。
- 替代 Jenkins Controller、插件管理或节点管理。
- 在应用内安装 GitHub Runner 服务。
- 把 Tauri updater 私钥返回前端、MCP 或日志。
- 无审批地发布正式版本或覆盖 `update.json`。
- Windows/macOS/Linux 的非原生交叉打包承诺。

---

## 4. 合规与账号边界

### 4.1 允许的多账号场景

应用可以管理以下合法身份：

- 用户自己的 GitHub 个人账号。
- 用户作为成员或管理员的 GitHub Organization。
- 用户获授权管理的客户 Organization。
- 一个专门用于自动化的机器账号，但必须由人创建并承担责任。
- 企业内多个有真实业务边界的 Organization。

每个身份必须由用户主动授权或手工添加安全凭证。应用只保存脱敏元数据和 `credentialKey`，不能代替用户注册账号。

### 4.2 禁止的额度规避场景

以下行为在产品层面直接禁止：

- 按剩余免费分钟把同一项目轮换到多个免费个人账号。
- 自动创建账号、邮箱或机器账号。
- 为获取新额度自动 Fork 或镜像同一私有项目。
- 把同一签名私钥批量复制到无业务归属的免费账号仓库。
- 将额度查询结果作为“规避计费”的账号选择器。

### 4.3 仓库所有者约束

GitHub Actions 额度按仓库所有者计费。因此：

- `credentialKey` 只决定谁有权调用 API。
- `owner/repo` 决定 Actions 额度归属。
- 调度器不能通过换 Token 改变计费主体。
- 多 GitHub 连接只允许构建其明确授权且业务归属匹配的项目。
- `build_projects.canonical_owner` 与 GitHub-hosted Provider 的 `billing_owner` 必须一致。
- 若不一致，Provider 只能作为外部合法构建后端使用，不能作为免费额度回退目标。

---

## 5. 术语与领域模型

| 术语 | 含义 |
|---|---|
| 构建项目 `BuildProject` | 一个被纳管的代码仓库和发布配置 |
| 构建后端 `BuildProvider` | GitHub-hosted、GitHub self-hosted 或 Jenkins |
| 构建路由 `BuildRoute` | 项目 + 平台到后端的优先级和约束 |
| 预算快照 `BudgetSnapshot` | 某 GitHub 计费主体在某周期的额度和使用量快照 |
| 预算预留 `BudgetReservation` | 防止并发构建同时透支预算的临时分钟占用 |
| 构建请求 `BuildRequest` | 一次逻辑发布或测试构建 |
| 构建尝试 `BuildAttempt` | 某平台在某个后端上的一次实际执行 |
| 外部运行 `ExternalRun` | GitHub Actions Run 或 Jenkins Build |
| 构建产物 `BuildArtifact` | EXE、DMG、AppImage、签名文件等 |
| 路由 Dry-run | 不触发构建，只计算候选、预算和选择理由 |
| 托管预算 | GitHub-hosted Runner 可使用的包含分钟和允许付费金额 |

---

## 6. 总体架构

```text
React 构建中心
  ├─ 项目
  ├─ 构建后端
  ├─ 路由策略
  ├─ 预算
  └─ 构建记录 / 产物
        │
        ▼ invoke / Dev API
Commands 层
  └─ build_orchestration.rs
        │
        ▼
Services 层
  ├─ BuildOrchestrationService    逻辑构建请求与状态机
  ├─ BuildRoutingService          候选过滤、评分与选择
  ├─ BuildBudgetService           快照、估算、预留和结算
  ├─ GitHubActionsService         Workflow、Run、Job、Artifact
  ├─ JenkinsBuildAdapter          复用 JenkinsService
  ├─ BuildArtifactService         下载、哈希、签名和清单
  └─ BuildRecoveryService         启动恢复和后台轮询
        │
        ├───────────────┬──────────────────┐
        ▼               ▼                  ▼
GitHub-hosted      GitHub self-hosted     Jenkins
Actions Workflow   Actions Workflow       Pipeline/Job
        │               │                  │
        └───────────────┴──────────────────┘
                        │
                        ▼
             GitHub Draft Release / 本地托管产物
                        │
                        ▼
                 Gitee update.json
```

### 6.1 分层职责

| 层级 | 职责 |
|---|---|
| React | 展示、表单、Dry-run、轮询状态、用户确认 |
| Commands | IPC 参数校验、调用 Service、错误转换 |
| BuildOrchestrationService | 请求状态机、审批、幂等、平台拆分、聚合 |
| BuildRoutingService | 平台能力、健康、预算、策略和优先级计算 |
| BuildBudgetService | 额度快照、成本估算、预算预留和实际结算 |
| Provider Adapter | 调用 GitHub/Jenkins，不决定业务策略 |
| Database | 持久化项目、路由、快照、请求、尝试和产物 |
| Approval/Audit | 复用现有审批和审计，不建立平行体系 |

---

## 7. 构建后端能力模型

### 7.1 Provider 类型

```text
github_hosted
github_self_hosted
jenkins
```

不将每个 GitHub 账号定义成独立“免费额度池”。Provider 表示合法的执行后端和计费主体。

### 7.2 平台能力

| Provider | Windows | macOS ARM | macOS Intel | Linux | Android |
|---|---:|---:|---:|---:|---:|
| GitHub-hosted | 是 | 是 | 是 | 是 | 是 |
| GitHub self-hosted | 取决于 Runner OS/Arch | 取决于 Mac Runner | 取决于 Mac Runner | 取决于 Runner | 取决于 SDK |
| Jenkins | 取决于 Agent Label | 取决于 Mac Agent | 取决于 Mac Agent | 取决于 Agent | 取决于 SDK |

### 7.3 原生构建约束

- Windows NSIS 默认只路由到 Windows Runner/Agent。
- macOS DMG、APP、签名和 notarization 默认只路由到 macOS Runner/Agent。
- Linux DEB/AppImage 默认只路由到 Linux Runner/Agent。
- Android 默认只路由到已配置 JDK、Android SDK 和 NDK 的 Linux/Windows/macOS 节点。
- 不把“Rust 可添加 target”误认为完整 Tauri 安装包可以跨平台生成。

### 7.4 Provider 健康状态

```text
draft
active
degraded
offline
credential_missing
credential_failed
permission_denied
budget_exhausted
disabled
```

只有 `active` 和满足策略的 `degraded` Provider 可以进入候选集。

---

## 8. 自动路由算法

### 8.1 输入

路由器接受：

- `projectKey`
- `commitSha`
- `version`
- `buildKind`: `test` / `release`
- `platforms`
- `requestedProviderKey`（可选，人工指定）
- `allowPaid`（默认 false）
- `requester`
- `source`: `ui` / `mcp` / `automation`

### 8.2 候选过滤

对每个平台依次执行：

1. Provider 必须启用且未软删除。
2. Provider 必须绑定当前项目的路由。
3. Provider 必须声明支持目标平台、架构和 bundle。
4. Provider 健康快照不得过期；过期则先刷新或降级为不可用。
5. GitHub Provider 的凭证必须存在、启用、状态正常，并拥有目标仓库 Actions 权限。
6. Jenkins Provider 的连接、Job 和凭证必须正常，Job 必须 buildable。
7. self-hosted Provider 必须找到匹配 Runner Label 的在线 Runner。
8. GitHub-hosted Provider 必须满足预算安全阈值。
9. 正式发布必须满足签名配置就绪。
10. Provider 必须满足项目所有者和计费主体约束。
11. MCP 调用还必须满足 `allow_mcp_read/write`。

被排除的 Provider 必须返回结构化理由，不能只返回“无可用节点”。

### 8.3 候选评分

默认评分：

```text
score =
  route_priority_score
  + health_score
  + budget_score
  + warm_cache_score
  + historical_success_score
  - estimated_cost_score
  - queue_delay_score
  - recent_failure_penalty
```

建议权重：

| 维度 | 权重 |
|---|---:|
| 路由优先级 | 35 |
| 健康状态 | 20 |
| 预算充足度 | 15 |
| 历史成功率 | 10 |
| 队列等待 | 10 |
| 热缓存 | 5 |
| 预估成本 | 5 |

首版不允许用户填写任意表达式，只提供表单化权重和固定边界。

### 8.4 默认选择规则

```text
if self_hosted 在线且平台匹配:
    选择 self_hosted
else if Jenkins Agent 在线且 Job 可构建:
    选择 Jenkins
else if GitHub-hosted 剩余额度 >= 预计分钟 + 安全余量:
    选择 GitHub-hosted
else if buildKind == release and allowPaid 已获 L3 审批:
    选择 GitHub-hosted 付费
else:
    返回 BLOCKED_BUDGET
```

### 8.5 人工指定 Provider

人工指定不等于绕过安全策略：

- 指定 Provider 仍需通过平台、凭证、预算和签名校验。
- 指定付费 GitHub-hosted 仍需 L3 审批。
- 指定离线 Provider 返回明确错误，不自动假装成功。
- UI 必须展示“已人工覆盖自动路由”。
- 审计记录自动候选、人工选择和最终选择。

### 8.6 路由 Dry-run 返回

```json
{
  "projectKey": "tauri-ssh",
  "commitSha": "abc123",
  "platformPlans": [
    {
      "platform": "windows-x86_64",
      "selectedProviderKey": "github-selfhosted-win",
      "estimatedMinutes": 14,
      "estimatedCostUsd": 0,
      "requiresApproval": true,
      "candidates": [
        {
          "providerKey": "github-selfhosted-win",
          "eligible": true,
          "score": 92,
          "reasons": ["Runner 在线", "平台匹配", "不消耗 GitHub-hosted 分钟"]
        },
        {
          "providerKey": "github-hosted-main",
          "eligible": false,
          "score": 0,
          "reasons": ["剩余额度低于安全阈值"]
        }
      ]
    }
  ],
  "policyVersion": 3,
  "budgetSnapshotAt": "2026-07-29T10:00:00+08:00"
}
```

---

## 9. 预算模型

### 9.1 预算主体

预算 Key：

```text
github:{ownerType}:{ownerLogin}:{billingCycle}
```

示例：

```text
github:organization:AgieWang:2026-07
```

### 9.2 快照字段

- 包含分钟。
- 已使用分钟。
- 已付费分钟或费用。
- 已预留分钟。
- 可用分钟。
- 计费周期开始与结束。
- 各 OS 使用明细。
- 数据来源：`github_api` / `manual` / `estimated`。
- 刷新时间和过期时间。
- 原始响应哈希，不保存完整敏感响应。

### 9.3 额度未知策略

如果 Billing Usage API 因权限、账号类型或计费模式不可用：

1. 不把未知视为无限额度。
2. 优先 self-hosted 或 Jenkins。
3. 用户可配置手工月度预算。
4. 手工预算必须显示 `manual` 标识。
5. 超过快照 TTL 后，GitHub-hosted 自动路由降级。
6. 用户仍可在 L3 审批后执行一次付费风险构建。

### 9.4 分钟估算

按 `project + platform + providerType` 保存最近成功样本。

```text
estimatedMinutes =
  max(
    最近 10 次成功构建的 P75 分钟数,
    最近一次成功分钟数
  ) * 1.20
```

无历史样本时使用保守默认值：

| 平台 | 默认预估 |
|---|---:|
| Windows x64 | 20 分钟 |
| macOS ARM | 25 分钟 |
| macOS Intel | 25 分钟 |
| Linux x64 | 20 分钟 |
| Android | 30 分钟 |

默认值只能作为预留依据，不能当作实际计费结果。

### 9.5 安全阈值

建议默认：

```text
safetyReserveMinutes = max(100, includedMinutes * 10%)
```

当：

```text
availableMinutes - estimatedMinutes < safetyReserveMinutes
```

则 GitHub-hosted 自动候选被排除。

### 9.6 并发预算预留

调度时必须在同一 SQLite 事务内：

1. 读取当前预算快照。
2. 汇总未释放的预留分钟。
3. 校验可用分钟。
4. 创建预算预留。
5. 创建构建尝试。

完成后：

- 成功或失败均释放预留。
- 有实际分钟时写入实际分钟。
- 外部运行状态未知时保留预留，直到超时或人工结算。
- 应用崩溃后由恢复任务重新核对。

这样可避免两个并发构建都读取到相同余额并同时透支。

---

## 10. 状态机

### 10.1 BuildRequest 状态

```text
draft
  -> planning
  -> awaiting_approval
  -> queued
  -> running
  -> aggregating
  -> verifying
  -> succeeded

任意执行态可进入:
  partial
  failed
  cancelling
  cancelled
  blocked
```

规则：

- 至少一个平台成功、至少一个平台失败时为 `partial`。
- 所有平台成功后才进入 `aggregating`。
- 产物数量、哈希或签名不通过时不能进入 `succeeded`。
- `blocked` 表示策略或预算阻断，不等于外部构建失败。

### 10.2 BuildAttempt 状态

```text
pending
  -> dispatching
  -> dispatched
  -> queued
  -> running
  -> succeeded

异常终态:
  failed
  cancelled
  timed_out
  skipped
  unknown
```

### 10.3 幂等键

```text
requestHash = SHA256(
  projectKey
  + commitSha
  + version
  + buildKind
  + sorted(platforms)
  + policyVersion
)
```

- 同一个 `requestHash` 存在未终结请求时，默认返回原请求。
- 用户明确选择“重新构建”时生成 `retrySequence`。
- Provider attempt 的幂等键为 `requestHash + platform + retrySequence`。
- 审批执行时必须复验 `approvalId + requestHash + policyVersion`。

---

## 11. 数据库设计

当前 Schema 版本为 24，首版建议新增 `v24 -> v25` 迁移。

### 11.1 `build_projects`

```sql
CREATE TABLE IF NOT EXISTS build_projects (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    project_key                 TEXT NOT NULL UNIQUE,
    name                        TEXT NOT NULL,
    git_workspace_key           TEXT NOT NULL DEFAULT '',
    canonical_owner             TEXT NOT NULL,
    canonical_repo              TEXT NOT NULL,
    default_ref                 TEXT NOT NULL DEFAULT 'master',
    workflow_file               TEXT NOT NULL DEFAULT 'release.yml',
    product_name                TEXT NOT NULL DEFAULT '',
    release_strategy            TEXT NOT NULL DEFAULT 'github_draft_then_gitee',
    release_repo_url            TEXT NOT NULL DEFAULT '',
    supported_platforms_json    TEXT NOT NULL DEFAULT '[]',
    artifact_rules_json         TEXT NOT NULL DEFAULT '{}',
    signing_policy_json         TEXT NOT NULL DEFAULT '{}',
    approval_policy             TEXT NOT NULL DEFAULT 'manual',
    allow_mcp_read              INTEGER NOT NULL DEFAULT 1,
    allow_mcp_write             INTEGER NOT NULL DEFAULT 0,
    enabled                     INTEGER NOT NULL DEFAULT 1,
    config_version              INTEGER NOT NULL DEFAULT 1,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at                  TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_build_projects_repo
    ON build_projects(canonical_owner, canonical_repo);
```

### 11.2 `build_providers`

```sql
CREATE TABLE IF NOT EXISTS build_providers (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_key                TEXT NOT NULL UNIQUE,
    name                        TEXT NOT NULL,
    provider_type               TEXT NOT NULL,
    credential_key              TEXT NOT NULL DEFAULT '',
    billing_owner_type          TEXT NOT NULL DEFAULT '',
    billing_owner_login         TEXT NOT NULL DEFAULT '',
    github_owner                TEXT NOT NULL DEFAULT '',
    github_repo                 TEXT NOT NULL DEFAULT '',
    github_workflow_id          TEXT NOT NULL DEFAULT '',
    jenkins_connection_key      TEXT NOT NULL DEFAULT '',
    jenkins_job_full_name       TEXT NOT NULL DEFAULT '',
    runner_labels_json          TEXT NOT NULL DEFAULT '[]',
    supported_platforms_json    TEXT NOT NULL DEFAULT '[]',
    priority                    INTEGER NOT NULL DEFAULT 100,
    safety_reserve_minutes      INTEGER NOT NULL DEFAULT 100,
    manual_monthly_limit        INTEGER DEFAULT NULL,
    allow_paid                  INTEGER NOT NULL DEFAULT 0,
    allow_mcp_read              INTEGER NOT NULL DEFAULT 1,
    allow_mcp_write             INTEGER NOT NULL DEFAULT 0,
    approval_policy             TEXT NOT NULL DEFAULT 'manual',
    health_status               TEXT NOT NULL DEFAULT 'draft',
    health_detail_json          TEXT NOT NULL DEFAULT '{}',
    last_health_checked_at      TEXT DEFAULT NULL,
    enabled                     INTEGER NOT NULL DEFAULT 0,
    config_version              INTEGER NOT NULL DEFAULT 1,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at                  TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_build_providers_type
    ON build_providers(provider_type, enabled);
CREATE INDEX IF NOT EXISTS idx_build_providers_credential
    ON build_providers(credential_key);
CREATE INDEX IF NOT EXISTS idx_build_providers_billing_owner
    ON build_providers(billing_owner_type, billing_owner_login);
```

约束由 Service 层校验：

- `github_hosted` / `github_self_hosted` 必须有 GitHub 仓库和 workflow。
- `jenkins` 必须有 `jenkins_connection_key` 和 `jenkins_job_full_name`。
- `github_self_hosted` 必须有 Runner Labels。
- Provider 不能同时配置 GitHub 和 Jenkins 执行字段。

### 11.3 `build_routes`

```sql
CREATE TABLE IF NOT EXISTS build_routes (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    project_key                 TEXT NOT NULL,
    platform                    TEXT NOT NULL,
    provider_key                TEXT NOT NULL,
    route_priority              INTEGER NOT NULL DEFAULT 100,
    enabled                     INTEGER NOT NULL DEFAULT 1,
    constraints_json            TEXT NOT NULL DEFAULT '{}',
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(project_key, platform, provider_key)
);

CREATE INDEX IF NOT EXISTS idx_build_routes_lookup
    ON build_routes(project_key, platform, enabled, route_priority);
```

### 11.4 `build_budget_snapshots`

```sql
CREATE TABLE IF NOT EXISTS build_budget_snapshots (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    budget_key                  TEXT NOT NULL,
    provider_key                TEXT NOT NULL,
    cycle_start                 TEXT NOT NULL,
    cycle_end                   TEXT NOT NULL,
    included_minutes            INTEGER DEFAULT NULL,
    used_minutes                INTEGER DEFAULT NULL,
    paid_minutes                INTEGER DEFAULT NULL,
    reserved_minutes            INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd          REAL DEFAULT NULL,
    breakdown_json              TEXT NOT NULL DEFAULT '{}',
    source                      TEXT NOT NULL,
    status                      TEXT NOT NULL DEFAULT 'unknown',
    response_hash               TEXT NOT NULL DEFAULT '',
    fetched_at                  TEXT NOT NULL,
    expires_at                  TEXT NOT NULL,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(budget_key, cycle_start)
);

CREATE INDEX IF NOT EXISTS idx_build_budget_provider
    ON build_budget_snapshots(provider_key, cycle_start DESC);
```

### 11.5 `build_requests`

```sql
CREATE TABLE IF NOT EXISTS build_requests (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id                  TEXT NOT NULL UNIQUE,
    request_hash                TEXT NOT NULL,
    retry_sequence              INTEGER NOT NULL DEFAULT 0,
    project_key                 TEXT NOT NULL,
    commit_sha                  TEXT NOT NULL,
    git_ref                     TEXT NOT NULL,
    version                     TEXT NOT NULL DEFAULT '',
    build_kind                  TEXT NOT NULL DEFAULT 'test',
    requested_platforms_json    TEXT NOT NULL DEFAULT '[]',
    status                      TEXT NOT NULL DEFAULT 'draft',
    source                      TEXT NOT NULL DEFAULT 'ui',
    requester                   TEXT NOT NULL DEFAULT '',
    route_plan_json             TEXT NOT NULL DEFAULT '{}',
    policy_version              INTEGER NOT NULL DEFAULT 1,
    approval_id                 INTEGER DEFAULT NULL,
    summary                     TEXT NOT NULL DEFAULT '',
    error_code                  TEXT NOT NULL DEFAULT '',
    error_message               TEXT NOT NULL DEFAULT '',
    started_at                  TEXT DEFAULT NULL,
    finished_at                 TEXT DEFAULT NULL,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(request_hash, retry_sequence)
);

CREATE INDEX IF NOT EXISTS idx_build_requests_project
    ON build_requests(project_key, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_build_requests_status
    ON build_requests(status, updated_at DESC);
```

### 11.6 `build_attempts`

```sql
CREATE TABLE IF NOT EXISTS build_attempts (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    attempt_id                  TEXT NOT NULL UNIQUE,
    request_id                  TEXT NOT NULL,
    platform                    TEXT NOT NULL,
    provider_key                TEXT NOT NULL,
    status                      TEXT NOT NULL DEFAULT 'pending',
    dispatch_key                TEXT NOT NULL,
    external_run_id             TEXT NOT NULL DEFAULT '',
    external_run_number         INTEGER DEFAULT NULL,
    external_run_url            TEXT NOT NULL DEFAULT '',
    external_status             TEXT NOT NULL DEFAULT '',
    external_conclusion         TEXT NOT NULL DEFAULT '',
    estimated_minutes           INTEGER NOT NULL DEFAULT 0,
    actual_minutes              INTEGER DEFAULT NULL,
    queue_seconds               INTEGER DEFAULT NULL,
    reservation_id              TEXT NOT NULL DEFAULT '',
    error_code                  TEXT NOT NULL DEFAULT '',
    error_message               TEXT NOT NULL DEFAULT '',
    dispatched_at               TEXT DEFAULT NULL,
    started_at                  TEXT DEFAULT NULL,
    finished_at                 TEXT DEFAULT NULL,
    last_polled_at              TEXT DEFAULT NULL,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(dispatch_key)
);

CREATE INDEX IF NOT EXISTS idx_build_attempts_request
    ON build_attempts(request_id, platform);
CREATE INDEX IF NOT EXISTS idx_build_attempts_external
    ON build_attempts(provider_key, external_run_id);
CREATE INDEX IF NOT EXISTS idx_build_attempts_status
    ON build_attempts(status, updated_at);
```

### 11.7 `build_budget_reservations`

```sql
CREATE TABLE IF NOT EXISTS build_budget_reservations (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    reservation_id              TEXT NOT NULL UNIQUE,
    budget_key                  TEXT NOT NULL,
    request_id                  TEXT NOT NULL,
    attempt_id                  TEXT NOT NULL,
    reserved_minutes            INTEGER NOT NULL,
    status                      TEXT NOT NULL DEFAULT 'active',
    expires_at                  TEXT NOT NULL,
    released_at                 TEXT DEFAULT NULL,
    actual_minutes              INTEGER DEFAULT NULL,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_build_reservations_budget
    ON build_budget_reservations(budget_key, status, expires_at);
```

### 11.8 `build_artifacts`

```sql
CREATE TABLE IF NOT EXISTS build_artifacts (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    artifact_id                 TEXT NOT NULL UNIQUE,
    request_id                  TEXT NOT NULL,
    attempt_id                  TEXT NOT NULL,
    platform                    TEXT NOT NULL,
    artifact_name               TEXT NOT NULL,
    artifact_kind               TEXT NOT NULL,
    external_url                TEXT NOT NULL DEFAULT '',
    local_path                  TEXT NOT NULL DEFAULT '',
    size_bytes                  INTEGER NOT NULL DEFAULT 0,
    sha256                      TEXT NOT NULL DEFAULT '',
    signature_artifact_id       TEXT NOT NULL DEFAULT '',
    signature_verified          INTEGER NOT NULL DEFAULT 0,
    verification_status         TEXT NOT NULL DEFAULT 'pending',
    published_url               TEXT NOT NULL DEFAULT '',
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_build_artifacts_request
    ON build_artifacts(request_id, platform);
CREATE INDEX IF NOT EXISTS idx_build_artifacts_attempt
    ON build_artifacts(attempt_id);
```

### 11.9 数据保留

- 构建请求、尝试、审批和审计元数据长期保留。
- 控制台日志正文不落库，只保存外部 URL、摘要和错误码。
- 本地 artifact 文件按项目策略保留，默认 30 天。
- GitHub/Gitee Release 的长期保留由发布策略控制。
- Gitee 更新仓库继续建议只保留最近 3 个正式版本目录。

---

## 12. Rust 数据模型

建议新增到 `src-tauri/src/models/mod.rs`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProject {
    pub id: i64,
    pub project_key: String,
    pub name: String,
    pub git_workspace_key: String,
    pub canonical_owner: String,
    pub canonical_repo: String,
    pub default_ref: String,
    pub workflow_file: String,
    pub supported_platforms: Vec<String>,
    pub approval_policy: String,
    pub allow_mcp_read: bool,
    pub allow_mcp_write: bool,
    pub enabled: bool,
    pub config_version: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProvider {
    pub id: i64,
    pub provider_key: String,
    pub name: String,
    pub provider_type: String,
    pub credential_key: String,
    pub billing_owner_type: String,
    pub billing_owner_login: String,
    pub github_owner: String,
    pub github_repo: String,
    pub github_workflow_id: String,
    pub jenkins_connection_key: String,
    pub jenkins_job_full_name: String,
    pub runner_labels: Vec<String>,
    pub supported_platforms: Vec<String>,
    pub priority: i64,
    pub safety_reserve_minutes: i64,
    pub manual_monthly_limit: Option<i64>,
    pub allow_paid: bool,
    pub allow_mcp_read: bool,
    pub allow_mcp_write: bool,
    pub approval_policy: String,
    pub health_status: String,
    pub enabled: bool,
    pub config_version: i64,
    pub updated_at: String,
}
```

还需定义：

- `BuildRoute`
- `BuildBudgetSnapshot`
- `BuildBudgetReservation`
- `BuildRequest`
- `BuildAttempt`
- `BuildArtifact`
- `BuildRoutePreviewInput/Result`
- `CreateBuildRequestInput`
- `ExecuteBuildRequestApprovedInput`
- `CancelBuildRequestInput`
- `RetryBuildAttemptInput`
- `RefreshBuildBudgetInput`
- `BuildRequestDetail`
- `BuildProviderTestResult`

所有枚举首版继续使用受控字符串并在 Service 层统一校验，避免数据库历史值因 Rust enum 严格反序列化导致应用无法启动。

---

## 13. 后端模块设计

### 13.1 文件布局

```text
src-tauri/src/
├── commands/
│   └── build_orchestration.rs
├── services/
│   ├── build_orchestration.rs
│   ├── build_routing.rs
│   ├── build_budget.rs
│   ├── github_actions.rs
│   ├── build_artifact.rs
│   └── build_recovery.rs
├── database/
│   ├── schema.rs
│   └── mod.rs
└── models/
    └── mod.rs
```

首版沿用项目现有 `database/mod.rs` 模式，避免为了本功能同时进行数据库层大重构。后续如果数据库文件继续增长，再独立拆分 `database/build_orchestration.rs`。

### 13.2 `BuildOrchestrationService`

职责：

- 创建逻辑构建请求。
- 调用 Routing Service 生成 Dry-run。
- 计算 `requestHash`。
- 创建审批请求。
- 审批通过后复验策略、配置版本和预算。
- 为每个平台创建 Attempt。
- 并发触发不同平台，但限制同 Provider 并发。
- 聚合 Attempt 状态。
- 进入 artifact 校验和发布阶段。
- 取消、重试和启动恢复。
- 发出 Tauri Event 更新 UI。

### 13.3 `BuildRoutingService`

职责：

- 校验项目、平台和 Provider 配置。
- 获取健康和预算快照。
- 过滤候选。
- 计算评分。
- 返回候选理由和排除理由。
- 保证 billing owner 约束。
- 不发起任何外部写操作。

### 13.4 `BuildBudgetService`

职责：

- 从 GitHub Billing Usage API获取预算信息。
- 处理无 Billing 权限和 API 不可用。
- 管理手工预算。
- 根据历史 Attempt 估算分钟。
- 在 SQLite 事务内创建和释放预算预留。
- 结算实际分钟。
- 清理过期预留。
- 输出预算状态：
  - `healthy`
  - `low`
  - `exhausted`
  - `unknown`
  - `stale`

### 13.5 `GitHubActionsService`

职责：

- 读取 Workflow。
- 触发 `workflow_dispatch`。
- 查询 Workflow Run、Job 和结论。
- 查询 self-hosted Runner 和 Labels。
- 取消 Run。
- 列出和下载 Actions Artifact。
- 查询 Draft Release 和 Assets。
- 所有请求通过安全凭证 Provider Adapter 发出。
- 不直接读取 Token 明文到 Command 或前端。

### 13.6 `JenkinsBuildAdapter`

不复制 Jenkins HTTP 实现，只做领域适配：

```text
BuildAttempt
  -> JenkinsBuildAdapter
  -> JenkinsService::create/execute trigger
  -> Jenkins queue/build tracking
  -> BuildAttempt status mapping
```

状态映射：

| Jenkins | BuildAttempt |
|---|---|
| queued | queued |
| building | running |
| success | succeeded |
| failure | failed |
| unstable | failed 或 partial，按项目策略 |
| aborted | cancelled |
| not_built | skipped |
| unknown | unknown |

### 13.7 `BuildArtifactService`

职责：

- 拉取 GitHub Actions/GitHub Release/Jenkins artifact 元数据。
- 下载到应用托管目录。
- 限制单文件和单请求总大小。
- 计算 SHA-256。
- 按项目 Artifact Rule 校验数量、命名和平台。
- 校验 `.sig` 文件存在且与 updater manifest 一致。
- 生成候选发布清单。
- 首版不自动修改 Gitee `update.json`，只生成受控发布候选。

### 13.8 `BuildRecoveryService`

应用启动后：

1. 查找 `dispatching/dispatched/queued/running/cancelling` Attempt。
2. 按 Provider 查询真实外部状态。
3. 修正本地状态，但不覆盖已确认的终态。
4. 释放已结束 Attempt 的预算预留。
5. 标记超过最大跟踪时间的 Attempt 为 `timed_out` 或 `unknown`。
6. 写入恢复审计。
7. 不自动重跑正式发布。

---

## 14. GitHub 身份与授权设计

### 14.1 首版

复用安全凭证模块，支持用户手工添加 Fine-grained PAT：

- `provider=github`
- `credentialType=token`
- `accountName` 保存账号名。
- `scopeJson` 保存声明权限。
- Secret 加密保存，不回显。

连接测试读取：

- 当前用户。
- 可访问仓库。
- Workflow 列表。
- Actions 权限。
- Billing Usage 权限（可选）。

### 14.2 后续 OAuth Device Flow

后续新增“连接 GitHub”：

1. 用户点击“连接 GitHub”。
2. Rust 后端请求 Device Code。
3. 前端展示验证码并通过系统浏览器打开 GitHub。
4. 后端按规范轮询授权结果。
5. Token 写入安全凭证，不返回前端。
6. 前端只接收账号摘要和 `credentialKey`。
7. 支持撤销和重新授权。

禁止：

- 应用自动创建 GitHub 账号。
- 自动填写注册表单。
- 保存 GitHub 密码。
- 在 WebView 中窃取 Cookie。

### 14.3 权限最小化

根据功能按需申请：

- Repository metadata: read。
- Actions: read/write。
- Contents: read；只有创建 Tag/Release 时才需要 write。
- Releases: write。
- Administration: 默认不申请。
- Organization billing: 仅预算管理员显式授权时使用。

如果 Billing Usage 权限缺失，构建功能仍可用，但预算状态为 `unknown` 或使用手工预算。

---

## 15. GitHub Workflow 改造

### 15.1 增加手动触发

在现有 Tag 触发基础上增加：

```yaml
on:
  push:
    tags:
      - 'v*.*.*'
      - 'mobile-v*.*.*'
  workflow_dispatch:
    inputs:
      requestId:
        description: 'Tauri SSH 构建请求 ID'
        required: true
        type: string
      buildKind:
        description: 'test 或 release'
        required: true
        type: choice
        options:
          - test
          - release
      platforms:
        description: 'JSON 平台数组'
        required: true
        type: string
      runnerMode:
        description: 'hosted 或 self_hosted'
        required: true
        type: choice
        options:
          - hosted
          - self_hosted
      releaseVersion:
        description: '版本号'
        required: false
        type: string
```

### 15.2 固定 Job，避免动态 Runner 注入

不允许用户输入任意 `runs-on`。使用固定 Job：

```yaml
jobs:
  release-hosted:
    if: ${{ inputs.runnerMode == 'hosted' || github.event_name == 'push' }}
    runs-on: ${{ matrix.platform }}

  release-self-hosted-windows:
    if: ${{ inputs.runnerMode == 'self_hosted' && contains(inputs.platforms, 'windows-x86_64') }}
    runs-on: [self-hosted, Windows, X64, tauri-release]

  release-self-hosted-macos:
    if: ${{ inputs.runnerMode == 'self_hosted' && contains(inputs.platforms, 'darwin') }}
    runs-on: [self-hosted, macOS, ARM64, tauri-release]

  release-self-hosted-linux:
    if: ${{ inputs.runnerMode == 'self_hosted' && contains(inputs.platforms, 'linux-x86_64') }}
    runs-on: [self-hosted, Linux, X64, tauri-release]
```

生产实现不要用字符串 `contains` 解析 JSON 作为最终安全判断，应在前置 Job 中把受控平台映射为固定 outputs/matrix，或分别使用布尔输入。

### 15.3 Run 关联

Workflow Dispatch API 返回成功时通常只表示请求已接收，不直接返回 Run ID。解决方式：

1. 设置：

```yaml
run-name: >-
  Build ${{ inputs.requestId || github.ref_name }}
```

2. Workflow 内把 `requestId` 写入构建摘要和产物清单。
3. Dispatch 前记录服务器时间、`ref` 和 `headSha`。
4. Dispatch 后查询该 Workflow 最近运行。
5. 使用 `event=workflow_dispatch`、`headSha`、`created_at` 窗口和 `display_title` 匹配。
6. 匹配到多个时不猜测，保持 `dispatched` 并继续轮询。
7. 超时后标记 `DISPATCH_RUN_NOT_FOUND`，不能重复触发，除非用户明确重试。

### 15.4 缓存

- 保留现有 Rust Cache。
- `setup-node` 增加 pnpm cache。
- 使用 `pnpm install --frozen-lockfile`。
- self-hosted Runner 应定期清理工作目录，但保留受控 Cargo/pnpm 缓存。
- 不在多个不可信仓库间共享可写缓存目录。

### 15.5 并发

建议：

```yaml
concurrency:
  group: release-${{ github.repository }}-${{ inputs.requestId || github.ref }}
  cancel-in-progress: false
```

正式发布不自动取消；测试构建可配置 `cancel-in-progress: true`。

---

## 16. Self-hosted Runner 管理

### 16.1 首版范围

首版只读取和选择已由管理员注册好的 Runner，不负责安装 Runner 服务。

支持：

- 查询在线/离线。
- 查询 OS、Arch、Labels、Busy 状态。
- 配置项目允许的 Labels。
- 路由前健康检查。
- 展示最近使用情况。

不支持：

- 远程下载并执行 Runner 安装脚本。
- 自动生成长期注册凭证。
- 修改系统服务。
- 在普通办公账号下无隔离运行。

### 16.2 安全要求

- Runner 使用专用系统账号。
- 只允许可信私有仓库和受保护 Tag。
- 禁止不可信 Fork PR 使用带 Secrets 的 self-hosted Runner。
- 使用专用 Runner Group 和仓库访问范围。
- 每个平台使用独立工作目录。
- 正式签名 Runner 不运行普通 PR。
- Runner 主机不保存可导出的 updater 私钥文件；优先运行时注入。
- 执行完成后清理临时签名文件。
- 日志不得输出 Secrets。

### 16.3 推荐节点

| 平台 | 推荐 |
|---|---|
| Windows | 专用 Windows 11/Server x64 主机 |
| macOS | Apple Silicon Mac mini，按需构建 ARM 和 Intel |
| Linux | Ubuntu 22.04/24.04 专用 VM |
| Android | 独立 Linux Agent，预装固定 JDK/SDK/NDK |

---

## 17. Jenkins 集成

### 17.1 复用方式

每个 Jenkins Provider 引用：

- `jenkinsConnectionKey`
- `jenkinsJobFullName`
- 受控参数映射
- 平台/Agent Label

触发参数建议：

```text
REQUEST_ID
PROJECT_KEY
GIT_URL
GIT_REF
COMMIT_SHA
BUILD_KIND
RELEASE_VERSION
PLATFORM
ARTIFACT_MANIFEST
```

敏感参数只能使用 Jenkins Credentials Binding 或安全凭证 `secretRef`，不能通过普通字符串参数传 Token/私钥。

### 17.2 Jenkinsfile 要求

- Checkout 必须固定到 `COMMIT_SHA`，不能只依赖浮动分支。
- 输出 `artifact-manifest.json`。
- manifest 包含文件名、平台、大小和 SHA-256。
- Tauri updater 签名通过 Jenkins Credentials 注入。
- 构建结束清理临时密钥。
- Artifact 由 Jenkins 归档，Tauri SSH 通过现有 artifact API 下载。
- Job 参数定义变化继续使用现有 `parameterDefinitionHash` 校验。

### 17.3 状态同步

复用现有 Jenkins queue/build 跟踪：

- Queue ID 写入 Attempt。
- 获得 Build Number 后写入 `external_run_number`。
- Jenkins URL 写入 `external_run_url`。
- 应用重启后通过连接 + Job + Build Number 恢复。

---

## 18. Command / IPC 设计

### 18.1 只读 Commands

| Command | 返回 |
|---|---|
| `list_build_projects` | 项目列表 |
| `get_build_project` | 项目详情 |
| `list_build_providers` | Provider 列表 |
| `get_build_provider` | Provider 详情 |
| `test_build_provider` | 健康和权限测试结果 |
| `preview_build_route` | Dry-run 路由结果 |
| `get_build_budget_overview` | 预算汇总 |
| `list_build_requests` | 构建请求列表 |
| `get_build_request_detail` | 请求 + Attempt + Artifact |
| `list_build_artifacts` | 产物列表 |

### 18.2 配置写 Commands

| Command | 风险 |
|---|---|
| `upsert_build_project` | L2；涉及正式发布时 L3 |
| `set_build_project_enabled` | L2 |
| `upsert_build_provider` | L2 |
| `set_build_provider_enabled` | L2 |
| `upsert_build_routes` | L2 |
| `refresh_build_budget` | readonly 外部调用 + 本地写快照 |
| `set_manual_build_budget` | L2 |

### 18.3 构建写 Commands

沿用 controlled/approved 模式：

| Command | 说明 |
|---|---|
| `create_build_request_approval` | 生成计划并创建审批 |
| `execute_build_request_approved` | 复验后触发构建 |
| `create_build_cancel_approval` | 创建取消审批 |
| `execute_build_cancel_approved` | 取消外部运行 |
| `create_build_retry_approval` | 对失败平台创建重试审批 |
| `execute_build_retry_approved` | 创建新的 retrySequence |
| `sync_build_request` | 主动同步外部状态 |
| `download_build_artifact` | 下载到应用托管目录 |
| `create_release_publish_approval` | 创建正式发布审批 |
| `execute_release_publish_approved` | 校验后发布和更新 manifest |

### 18.4 Command 示例

```rust
#[tauri::command]
pub async fn preview_build_route(
    state: tauri::State<'_, AppState>,
    input: BuildRoutePreviewInput,
) -> Result<BuildRoutePreviewResult, CommandError> {
    BuildRoutingService::preview(&state.db, input)
        .await
        .map_err(|error| error.into())
}

#[tauri::command]
pub async fn execute_build_request_approved(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: ExecuteBuildRequestApprovedInput,
) -> Result<BuildRequestDetail, CommandError> {
    BuildOrchestrationService::execute_approved(&app, &state.db, input)
        .await
        .map_err(|error| error.into())
}
```

所有 Commands 必须在 `lib.rs` 的 `generate_handler![]` 注册，并在 Dev API 提供等价路由用于浏览器开发验证。

---

## 19. MCP 工具设计

### 19.1 只读工具

| 工具 | 策略 |
|---|---|
| `build_projects_list` | 只返回 `allow_mcp_read=true` 项目 |
| `build_providers_list` | 返回脱敏 Provider，不返回 Token |
| `build_budget_overview` | 返回汇总和快照时间 |
| `build_route_preview` | Dry-run，不触发构建 |
| `build_requests_list` | 限制数量，默认 30 |
| `build_request_detail` | 不返回日志正文和敏感参数 |
| `build_artifacts_list` | 只返回元数据 |

### 19.2 写工具

| 工具 | 策略 |
|---|---|
| `build_request_controlled` | 创建审批，不直接触发 |
| `build_request_approved` | 校验 approvalId/requestHash 后执行 |
| `build_cancel_controlled` | 创建取消审批 |
| `build_cancel_approved` | 审批后取消 |
| `build_retry_controlled` | 创建失败平台重试审批 |
| `build_retry_approved` | 审批后重试 |
| `release_publish_controlled` | 创建正式发布审批 |
| `release_publish_approved` | 审批后发布 |

### 19.3 MCP 返回限制

- 列表默认 30，最大 100。
- 日志正文不通过构建工具返回，仍使用 Jenkins/GitHub 受限日志工具。
- 不返回 Token、Authorization、Cookie、签名私钥或 Secrets 名值。
- 预算明细不返回支付信息。
- 产物下载只能写入应用托管目录。
- MCP 不能设置 `allowPaid=true` 绕过 L3 审批。

---

## 20. 前端设计

### 20.1 菜单与路由

建议新增一级页面：

```text
运维
└── 构建中心
```

路由：

```text
/build-center
```

### 20.2 页面结构

```text
构建中心
├── 总览
├── 项目
├── 构建后端
├── 路由策略
├── 预算
└── 构建记录
```

### 20.3 总览

使用：

- `Card`：今日构建、成功率、运行中、预算状态。
- `Progress`：GitHub-hosted 使用比例。
- `Alert`：额度低、额度未知、Runner 离线、签名配置异常。
- `Table`：最近构建。
- `Tag`：Provider 类型和健康状态。

### 20.4 项目页

字段：

- 项目 Key。
- 名称。
- Git 工作区。
- Canonical GitHub 仓库。
- 默认分支和 Workflow。
- 支持平台。
- Artifact 规则。
- 签名策略。
- 审批策略。
- MCP 读写开关。

### 20.5 构建后端页

展示：

- Provider 名称和类型。
- GitHub/Jenkins 连接引用。
- 计费主体。
- 支持平台。
- Runner Labels 或 Jenkins Job。
- 健康状态。
- 预算状态。
- 优先级。
- 是否允许付费。
- 最近测试时间。

操作：

- 新建/编辑。
- 测试连接。
- 刷新健康。
- 启用/禁用。
- 查看路由引用。

### 20.6 路由策略页

按项目和平台展示优先级：

```text
windows-x86_64
  1. github-selfhosted-win
  2. jenkins-win
  3. github-hosted-main
```

支持拖动排序，但保存时转换为明确 `routePriority`，并执行完整校验。

### 20.7 发起构建

使用 `Steps`：

1. 选择项目和 Commit。
2. 选择构建类型和平台。
3. 执行路由 Dry-run。
4. 展示预算、后端、预计时间和风险。
5. 创建审批或直接执行符合策略的测试构建。

正式发布必须展示：

- 版本号。
- Commit SHA。
- 平台列表。
- 每个平台 Provider。
- 是否消耗 GitHub-hosted 额度。
- 是否可能产生付费。
- updater 签名状态。
- 发布目标。

### 20.8 构建详情

使用时间线展示：

```text
已创建
→ 路由完成
→ 审批通过
→ Windows dispatched
→ macOS queued
→ Linux running
→ 产物收集
→ 签名校验
→ 发布完成
```

每个平台单独显示 Attempt，避免一个平台失败遮盖其他成功平台。

### 20.9 前端文件

```text
src/
├── pages/build-center/index.tsx
├── types/buildOrchestration.ts
├── lib/api/buildOrchestration.ts
└── store/buildOrchestration.ts       # 仅保存筛选和页面状态
```

构建真状态来自 Rust/SQLite，不放入 Zustand 作为唯一真源。

---

## 21. Artifact、签名与发布

### 21.1 Artifact Manifest

每个外部构建后端必须输出统一清单：

```json
{
  "requestId": "build_01...",
  "attemptId": "attempt_01...",
  "projectKey": "tauri-ssh",
  "commitSha": "abc123",
  "version": "0.3.0",
  "platform": "windows-x86_64",
  "artifacts": [
    {
      "name": "Tauri.SSH_0.3.0_x64-setup.exe",
      "kind": "installer",
      "sizeBytes": 12345678,
      "sha256": "..."
    },
    {
      "name": "Tauri.SSH_0.3.0_x64-setup.exe.sig",
      "kind": "updater_signature",
      "sizeBytes": 512,
      "sha256": "..."
    }
  ]
}
```

### 21.2 校验门禁

正式发布前必须：

1. Commit SHA 与请求一致。
2. 版本号三处一致。
3. 平台产物数量符合项目规则。
4. 文件名符合 productName 和版本。
5. 安装包和签名文件配对。
6. SHA-256 与 manifest 一致。
7. `.sig` 内容可解析且与 `update.json` 候选一致。
8. 不混入其他请求或其他版本产物。
9. 所有要求平台都成功；允许部分发布时必须单独 L3 审批。

### 21.3 updater 私钥

- GitHub Actions 使用 Repository/Environment Secrets。
- Jenkins 使用 Jenkins Credentials。
- Tauri SSH 首版不集中托管 updater 私钥。
- Provider 配置只记录签名就绪状态和引用摘要。
- 不支持从 GitHub Secret 读回私钥。
- 不在多个无业务归属仓库间复制私钥。

### 21.4 发布分阶段

v0.3：

- 调度、跟踪和 artifact 元数据。
- 仍由现有发布流程人工同步 Gitee。

v0.4：

- 自动下载到应用托管目录。
- 自动校验哈希和签名。
- 生成待发布 `update.json` 候选。

v0.5：

- L3 审批后同步 Gitee/GitHub Release 仓库。
- 发布后回读验证 URL、版本、签名和 README。

---

## 22. 安全设计

### 22.1 凭证

- 只引用 `credentialKey`。
- Secret 只在 Rust Service 单次调用内解密。
- 不返回前端或 MCP。
- 不写普通日志。
- 认证失败只返回脱敏错误。
- Provider 删除采用软删除，不级联删除历史构建。

### 22.2 审批等级

| 操作 | 默认风险 |
|---|---|
| 查询预算、Provider、Runner、构建状态 | readonly |
| 测试 Provider | readonly / L1 |
| 普通测试构建 | L2，可按项目策略免审批 |
| 正式 Release 构建 | L2 |
| 允许 GitHub-hosted 付费超额 | L3 |
| 修改正式路由或签名策略 | L3 |
| 取消生产发布构建 | L3 |
| 发布或覆盖 `update.json` | L3 |
| 部分平台发布 | L3 |

### 22.3 requestHash 复验

审批创建时保存：

- 项目和配置版本。
- Commit SHA。
- 平台。
- Provider。
- 预算快照。
- 预计分钟。
- `allowPaid`。
- 路由计划。

执行时重新计算 Hash。任意关键字段变化则拒绝执行并要求重新审批。

### 22.4 SSRF 与 URL

- GitHub API Base URL 默认固定 `https://api.github.com`。
- GitHub Enterprise Base URL 必须来自已测试安全凭证配置。
- Jenkins Base URL 复用现有标准化和 SSH 隧道策略。
- Artifact URL 必须属于已知 GitHub/Jenkins 主机。
- 禁止用户提交任意下载 URL。
- 重定向后再次校验域名。

### 22.5 Tauri Capabilities

GitHub/Jenkins HTTP 请求由 Rust `reqwest` 发起，不需要向前端开放通用 HTTP Capability。

前端仅需要：

- 现有 Core IPC。
- Opener：打开 GitHub Device Flow、Actions Run、Jenkins Build 页面。
- Notification：构建完成通知。

不应为了构建中心新增：

- 通用 Shell 执行权限。
- 任意文件系统权限。
- 前端 HTTP 全域权限。

Artifact 文件选择和打开必须限制在应用托管目录。

### 22.6 日志脱敏

统一脱敏：

- `Authorization`
- `Cookie`
- `Set-Cookie`
- Token
- PAT 前缀和后缀
- Device Code
- OAuth Access Token
- updater 私钥
- Jenkins Crumb
- 签名 Secret

---

## 23. 错误码

建议新增结构化错误码：

| 错误码 | 含义 |
|---|---|
| `BUILD_PROJECT_NOT_FOUND` | 项目不存在 |
| `BUILD_PROVIDER_NOT_FOUND` | Provider 不存在 |
| `BUILD_PROVIDER_DISABLED` | Provider 已禁用 |
| `BUILD_PROVIDER_UNHEALTHY` | Provider 不健康 |
| `BUILD_PLATFORM_UNSUPPORTED` | 平台不支持 |
| `BUILD_NO_ELIGIBLE_PROVIDER` | 无候选 Provider |
| `BUILD_BUDGET_UNKNOWN` | 额度未知 |
| `BUILD_BUDGET_LOW` | 额度低于安全阈值 |
| `BUILD_BUDGET_EXHAUSTED` | 额度不足 |
| `BUILD_PAID_APPROVAL_REQUIRED` | 需要付费审批 |
| `BUILD_OWNER_MISMATCH` | 项目所有者与计费主体不匹配 |
| `BUILD_CREDENTIAL_MISSING` | 凭证缺失 |
| `BUILD_PERMISSION_DENIED` | Actions/Jenkins 权限不足 |
| `BUILD_RUNNER_OFFLINE` | Runner 离线 |
| `BUILD_DISPATCH_FAILED` | 触发失败 |
| `BUILD_DISPATCH_RUN_NOT_FOUND` | Dispatch 后未关联到 Run |
| `BUILD_REQUEST_CONFLICT` | 幂等请求冲突 |
| `BUILD_POLICY_CHANGED` | 审批后策略变化 |
| `BUILD_ARTIFACT_INCOMPLETE` | 产物不完整 |
| `BUILD_ARTIFACT_HASH_MISMATCH` | 哈希不一致 |
| `BUILD_SIGNATURE_INVALID` | updater 签名校验失败 |
| `BUILD_TRACKING_TIMEOUT` | 外部运行跟踪超时 |

UI 不直接显示原始 API 响应或堆栈，只展示错误码、中文摘要和可操作建议。

---

## 24. 失败恢复与重试

### 24.1 Dispatch 超时

- HTTP 超时不代表 GitHub/Jenkins 没有接收。
- 先按 `requestId`、时间窗、Commit SHA 查询外部运行。
- 确认不存在后才允许重试。
- 重试生成新的 `retrySequence`，不能覆盖原 Attempt。

### 24.2 某平台失败

- 其他平台继续运行。
- 请求状态进入 `partial`。
- 只允许重试失败平台。
- 重试前重新路由，可切换到其他 Provider。
- 正式发布默认要求所有目标平台成功。

### 24.3 Provider 中途离线

- 外部 Run 已创建：继续轮询，不能自动在另一个 Provider 重复构建。
- 确认外部 Run 未启动且已取消：允许创建新 Attempt。
- 无法确认：标记 `unknown`，要求人工处理。

### 24.4 应用重启

- 从 SQLite 恢复未结束请求。
- 查询外部状态。
- 重新启动轮询。
- 不重复 Dispatch。
- 不重复创建 Release。
- 不自动重新批准已过期审批。

### 24.5 预算快照刷新失败

- 使用未过期旧快照。
- 旧快照过期则状态为 `stale`。
- `stale` 时自动路由不选择 GitHub-hosted。
- self-hosted/Jenkins 不受托管分钟快照影响。

---

## 25. 通知与事件

### 25.1 Tauri Events

建议事件：

```text
build-request-updated
build-attempt-updated
build-budget-updated
build-provider-health-updated
build-artifact-updated
```

Payload 只发送 ID、状态和摘要，页面收到事件后再通过 API 拉取完整详情。

### 25.2 系统通知

按项目配置：

- 构建成功。
- 构建失败。
- 部分平台失败。
- 额度低于阈值。
- Runner 离线。
- 产物或签名校验失败。
- 审批等待。

点击通知打开 `/build-center` 对应请求详情。

---

## 26. 测试方案

### 26.1 Rust 单元测试

#### 路由

- self-hosted 在线时优先选择。
- self-hosted 离线时回退 Jenkins。
- Jenkins 离线时回退 GitHub-hosted。
- GitHub-hosted 额度不足时被排除。
- 额度未知时不自动选择 GitHub-hosted。
- 平台不匹配时过滤。
- billing owner 不匹配时过滤。
- 人工指定 Provider 仍不能绕过策略。

#### 预算

- 预算预留和释放。
- 两个并发请求不能超额预留。
- 过期预留清理。
- 实际分钟结算。
- 手工预算和 API 快照优先级。
- 安全阈值计算。

#### 状态机

- 非法状态跳转拒绝。
- 部分平台失败汇总为 `partial`。
- 所有平台成功后进入 artifact 阶段。
- 重复 Dispatch 幂等。
- 审批 Hash 变化拒绝执行。

#### 安全

- Token 不出现在错误和日志。
- Artifact URL 域名校验。
- MCP 不能对 `allow_mcp_write=false` 项目发起构建。
- `allowPaid=true` 必须经过 L3 审批。

### 26.2 Adapter 测试

使用本地 Mock HTTP Server：

- GitHub Workflow Dispatch 204。
- GitHub 401/403/404/422。
- Workflow Dispatch 已接收但 Run 延迟出现。
- GitHub Billing 权限不足。
- GitHub Runner 在线/离线/Busy。
- Actions Artifact 分页和下载。
- Jenkins Queue 到 Build Number。
- Jenkins Crumb 过期。
- Jenkins Artifact 下载失败。

### 26.3 数据库测试

- v24 -> v25 迁移。
- 全新库直接迁移到 v25。
- 唯一索引和软删除行为。
- JSON 字段非法值处理。
- 并发预算事务。
- 未结束请求恢复查询。

### 26.4 前端测试

- Dry-run 候选和拒绝理由。
- 额度未知/低/耗尽状态。
- 发起构建表单平台校验。
- 付费风险确认。
- 构建详情多平台时间线。
- 事件更新和轮询降级。
- 错误码中文提示。

### 26.5 运行时验收

前端页面测试必须使用 Codex 内置浏览器或 Control Chrome：

1. 打开 `http://localhost:1422/#/build-center`。
2. 创建测试项目和 Provider。
3. 执行路由 Dry-run。
4. 验证预算卡片和候选理由。
5. 触发测试 Workflow。
6. 在 GitHub Actions 页面确认 Run。
7. 返回应用确认状态同步。
8. 下载并校验测试 Artifact。
9. 模拟 Runner 离线并验证回退。
10. 验证控制台无错误、Network 无敏感数据。

### 26.6 跨平台验收

- Windows NSIS 实机安装。
- macOS ARM DMG 安装。
- macOS Intel 产物在 Intel 或受控兼容环境验证。
- Linux AppImage/DEB 验证。
- updater 从 Gitee `update.json` 升级验证。

构建成功不等于发布验收完成。

---

## 27. 审计设计

审计动作：

```text
build.project.upsert
build.provider.upsert
build.provider.test
build.provider.enable
build.route.update
build.budget.refresh
build.budget.manual_update
build.route.preview
build.request.create
build.request.approve
build.request.dispatch
build.request.cancel
build.attempt.retry
build.artifact.download
build.artifact.verify
build.release.publish
```

审计详情允许：

- 项目 Key。
- Provider Key。
- 平台。
- Commit SHA。
- Workflow/Job。
- 外部 Run ID。
- 预算快照时间。
- 预计/实际分钟。
- 路由理由摘要。
- 结果和错误码。

审计详情禁止：

- Token。
- Authorization Header。
- Cookie。
- updater 私钥。
- Jenkins Crumb。
- Secret 参数值。
- 完整构建日志正文。

---

## 28. 分阶段实施计划

### 阶段 0：工作流与基础验证，预计 1～2 天

- [ ] 给现有 `release.yml` 增加 `workflow_dispatch`。
- [ ] 增加 `requestId` 和固定 Runner Mode。
- [ ] 增加 `run-name` 和 artifact manifest。
- [ ] 验证 hosted Workflow Dispatch。
- [ ] 验证 self-hosted Runner Labels。
- [ ] 验证 GitHub API 权限。

交付门禁：

- 可以通过 API 触发一次测试构建。
- 可以可靠关联到 Run ID。
- 不影响原 Tag 发布。

### 阶段 1：数据与只读构建中心，预计 3～5 天

- [ ] v24 -> v25 数据库迁移。
- [ ] BuildProject/Provider/Route 模型。
- [ ] Provider 健康测试。
- [ ] GitHub Actions Run/Runner 查询。
- [ ] 预算快照和手工预算。
- [ ] 构建中心总览、项目、Provider、预算页面。
- [ ] 路由 Dry-run。

交付门禁：

- UI 能解释每个平台为什么选择/排除某 Provider。
- Token 不出现在前端、日志和审计。

### 阶段 2：受控构建调度，预计 4～6 天

- [ ] BuildRequest/Attempt 状态机。
- [ ] 幂等和预算预留。
- [ ] 审批创建和执行。
- [ ] GitHub-hosted/self-hosted Dispatch。
- [ ] Jenkins Adapter。
- [ ] 构建状态轮询和 Tauri Event。
- [ ] 失败平台重试和取消。
- [ ] 启动恢复。

交付门禁：

- 额度不足时自动回退到 self-hosted/Jenkins。
- 不重复触发同一请求。
- 应用重启后可以恢复跟踪。

### 阶段 3：Artifact 与签名校验，预计 3～5 天

- [ ] 统一 artifact manifest。
- [ ] GitHub/Jenkins artifact 下载。
- [ ] 应用托管目录和大小限制。
- [ ] SHA-256 校验。
- [ ] updater `.sig` 配对校验。
- [ ] 发布候选清单。

交付门禁：

- 混入其他版本或平台产物时必须阻断。
- 产物和签名不完整时不能发布。

### 阶段 4：发布闭环与 MCP，预计 3～5 天

- [ ] 生成 Gitee/GitHub `update.json` 候选。
- [ ] L3 发布审批。
- [ ] 发布后回读验证。
- [ ] MCP 只读工具。
- [ ] MCP controlled/approved 写工具。
- [ ] 通知和审计完善。

交付门禁：

- 发布完成后验证真实更新端点。
- MCP 与 UI 使用同一策略，无额外权限。

### 总体预计

首个可用版本约 11～18 个开发日；完整发布闭环约 14～23 个开发日。实际时间取决于：

- GitHub Billing API 权限。
- Self-hosted 节点准备情况。
- Jenkins Job 是否已有统一参数。
- macOS 签名/notarization 配置。
- Gitee 发布自动化授权方式。

---

## 29. 预计修改文件

### 29.1 后端

```text
src-tauri/src/database/schema.rs
src-tauri/src/database/mod.rs
src-tauri/src/models/mod.rs
src-tauri/src/services/mod.rs
src-tauri/src/services/build_orchestration.rs
src-tauri/src/services/build_routing.rs
src-tauri/src/services/build_budget.rs
src-tauri/src/services/github_actions.rs
src-tauri/src/services/build_artifact.rs
src-tauri/src/services/build_recovery.rs
src-tauri/src/commands/mod.rs
src-tauri/src/commands/build_orchestration.rs
src-tauri/src/lib.rs
src-tauri/src/dev_server/mod.rs
```

### 29.2 前端

```text
src/types/buildOrchestration.ts
src/types/index.ts
src/lib/api/buildOrchestration.ts
src/lib/api/index.ts
src/pages/build-center/index.tsx
src/Router.tsx
src/components/layout/AppLayout.tsx
```

### 29.3 CI

```text
.github/workflows/release.yml
```

如新增 Jenkins Pipeline，再增加仓库根目录 `Jenkinsfile`；是否创建由实施阶段单独确认。

---

## 30. 验收标准

### 30.1 功能

- [ ] 可以绑定多个用户主动授权的 GitHub 身份/组织。
- [ ] 不提供自动注册 GitHub 账号能力。
- [ ] 可以配置 GitHub-hosted、self-hosted 和 Jenkins。
- [ ] 每个平台可以独立路由。
- [ ] Dry-run 能展示候选、评分、预算和拒绝理由。
- [ ] 额度不足时自动回退到 self-hosted/Jenkins。
- [ ] 无回退节点时明确阻断，不自动付费。
- [ ] 付费构建必须 L3 审批。
- [ ] 多平台状态独立跟踪。
- [ ] 失败平台可以单独重试。
- [ ] 应用重启后恢复跟踪。
- [ ] Artifact 和签名校验通过后才能发布。

### 30.2 安全

- [ ] Token、Cookie、Crumb、私钥不出现在前端、MCP、日志和审计。
- [ ] Billing owner 与项目所有者不匹配时不能作为额度切换路径。
- [ ] self-hosted Runner 不接受不可信 PR。
- [ ] 正式发布和 `update.json` 覆盖经过 L3 审批。
- [ ] MCP 不能绕过 UI 的策略和审批。
- [ ] Artifact 下载限制到应用托管目录。

### 30.3 可靠性

- [ ] 重复点击不会重复 Dispatch。
- [ ] Dispatch 超时不会立即重复触发。
- [ ] 并发请求不会透支预算预留。
- [ ] 额度未知不会被当成额度充足。
- [ ] 外部运行状态与本地跟踪状态明确区分。
- [ ] 部分成功不会被错误标记为全部成功。

### 30.4 发布

- [ ] 原 Tag 发布仍可工作。
- [ ] Workflow Dispatch 可工作。
- [ ] Windows/macOS/Linux 真实安装包验证。
- [ ] updater `.sig` 与 `update.json` 一致。
- [ ] Gitee 更新端点真实回读成功。

---

## 31. 关键风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| GitHub Billing API 权限不足 | 无法精确读取余额 | 支持手工预算，未知时回退 self-hosted/Jenkins |
| Dispatch 后 Run ID 延迟 | 可能重复构建 | requestId + run-name + 时间窗关联，未确认前禁止重试 |
| Self-hosted Runner 被恶意 Workflow 利用 | Secrets 和主机泄露 | 仅可信仓库/Tag、Runner Group、专用账号、签名节点隔离 |
| 多平台部分失败 | Release 不完整 | Request/Attempt 分层，正式发布默认要求全平台成功 |
| updater 私钥扩散 | 更新供应链失陷 | 私钥只留在 GitHub/Jenkins Secret，不由应用回读 |
| 预算并发透支 | 产生意外费用 | SQLite 事务内预算预留 |
| GitHub API/计费规则变化 | 预算逻辑失效 | Adapter 隔离、来源标记、额度未知安全降级 |
| Gitee 仓库容量继续增长 | 无法推送新版本 | 只保留最近 3 个正式版本，长期迁移对象存储/CDN |
| 构建日志包含 Secret | 凭据泄露 | 上游日志掩码 + 本地二次脱敏，不持久化正文 |

---

## 32. 默认决策与待确认项

### 32.1 已确定默认决策

1. 不实现账号自动注册和免费额度轮换。
2. 多账号只表示合法授权的不同项目/组织。
3. Actions 额度以仓库 Owner 为主体。
4. 自动路由优先 self-hosted，其次 Jenkins，最后 GitHub-hosted。
5. 额度未知时不自动使用 GitHub-hosted。
6. 自动付费默认关闭。
7. updater 私钥继续由 GitHub/Jenkins Secrets 托管。
8. 首版不自动安装 Runner。
9. 首版保留现有 Tag 发布兼容。
10. UI 与 MCP 复用同一 Service、审批和审计。

### 32.2 实施前需要确认

- [ ] 是否已有可用 Windows self-hosted 主机。
- [ ] 是否使用当前 Mac 作为 macOS Runner，还是单独采购 Mac mini。
- [ ] 是否保留 Linux 正式发布。
- [ ] Jenkins 是否已有 Windows/macOS/Linux Agent。
- [ ] GitHub 仓库属于个人账号还是 Organization。
- [ ] 当前账号是否有 Billing Usage API 权限。
- [ ] 测试构建是否允许免审批。
- [ ] 正式发布是否允许部分平台发布。
- [ ] Gitee 是否继续作为主 updater 端点。
- [ ] 是否在 v0.4 后自动发布 Gitee。

---

## 33. 推荐实施顺序

推荐先完成最小闭环：

1. 改造 `release.yml` 支持 `workflow_dispatch + requestId`。
2. 用现有安全凭证触发一次 GitHub Workflow。
3. 可靠关联 Run ID 并同步状态。
4. 接入一个 Windows self-hosted Runner。
5. 完成路由 Dry-run 和预算手工阈值。
6. 验证额度不足时从 GitHub-hosted 回退 self-hosted。
7. 再接入 Jenkins 和其他平台。
8. 最后自动化 artifact、签名和 Gitee 发布。

不要一开始同时实现 OAuth、Billing API、三平台 Runner、Jenkins、artifact 发布和 MCP。先证明“一个项目、一个平台、两个后端、一次可靠回退”，再扩展到完整矩阵。

