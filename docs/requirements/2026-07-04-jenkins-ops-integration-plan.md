# Jenkins 运维集成功能详细规划

**状态**: 已压测规划稿
**创建时间**: 2026-07-04
**目标版本**: v0.1 首版，v0.2 / v0.3 增强
**目标模块**: 运维 / Jenkins 构建运维工作台 / MCP Server / 审批队列 / 审计日志 / 安全凭证
**参考资料**:
- Jenkins Remote Access API: https://www.jenkins.io/doc/book/using/remote-access-api/
- Jenkins CSRF Protection: https://www.jenkins.io/doc/book/security/csrf-protection/
- Jenkins Script Console 安全说明: https://www.jenkins.io/doc/book/managing/script-console/

---

## 1. 背景与目标

Tauri SSH 当前已经具备服务器资产、SSH 终端、SFTP、数据库运维、资源监控、自动部署、安全凭证、审批队列、审计日志、AI Skill、Runbook 和 MCP Server。下一步如果要覆盖更完整的研发运维闭环，Jenkins 是一个很自然的外部系统入口。

Jenkins 模块首版定位为“构建运维工作台”，不是轻量 API 连接器。目标不是在应用内重做 Jenkins 管理后台，而是把 Jenkins 的常用构建和排障能力纳入 Tauri SSH 的受控运维体系：

- 在应用中统一管理 Jenkins 连接，不暴露 Jenkins Token 明文。
- 读取 Jenkins Job、构建列表、构建详情、队列和控制台日志。
- 发起普通构建、参数化构建、停止构建等写操作，并进入审批、审计和 AI 放行策略。
- 让 MCP Agent 可以只读查询 Jenkins 状态，也可以在审批后触发构建。
- 将 Jenkins 构建结果与自动部署、Git 工作区、代码审核模块形成后续联动。

一句话定位：

> 一个继承安全凭证、审批队列、审计日志和 MCP 策略的 Jenkins 构建运维工作台，让用户在 Tauri SSH 内完成连接、Job 浏览、参数构建、审批触发、队列跟踪、日志排障、artifact 下载和构建通知。

---

## 2. 产品原则

1. **不替代 Jenkins**
   - 不做完整 Job 配置编辑器。
   - 不做 Jenkins 插件管理、节点管理、权限管理。
   - 不做 Pipeline 可视化编辑器。
   - 首版覆盖构建运维闭环：连接、Job 浏览、参数构建、审批、队列跟踪、日志查看、artifact 下载、通知和审计。

2. **凭据后端托管**
   - 首版只支持 Jenkins username + API Token，不支持密码登录和 Cookie 会话。
   - Jenkins 连接表只保存 `credentialKey`。
   - 前端、MCP 响应、审计详情都不能返回 Token 明文。

3. **默认只读安全**
   - Job 列表、构建列表、构建详情、日志读取属于只读能力。
   - 触发构建、停止构建、重放构建、删除构建等都属于写操作，默认进入审批。

4. **MCP 与 UI 同权**
   - UI 点击和 MCP 调用必须复用同一套 Rust Service 层策略。
   - MCP 不能拥有比 UI 更高的 Jenkins 权限。
   - MCP 写操作必须使用 controlled/approved 两段式工具。

5. **日志受限返回**
   - 构建日志可能很大，必须支持分页、增量、最大字符数限制。
   - MCP 返回默认截断，提供 `start` / `tailBytes` / `progressive` 参数。

6. **高危 Jenkins 能力不开放**
   - 首版不开放 Script Console。
   - 首版不开放 Job XML 配置修改。
   - 首版不开放插件安装、系统重启、节点脚本执行。
   - 后续若开放，必须作为 L3 高危操作并默认禁用。

---

## 3. Jenkins API 可行性

Jenkins 官方 Remote Access API 支持以 REST-like 方式读取信息并触发构建，API 数据格式包括 JSON、XML 等。常见接口形态如下：

| 能力 | Jenkins API 形态 | 首版采用 |
|------|------------------|----------|
| 顶层信息 / Job 列表 | `/api/json` | 是 |
| Job 详情 | `/job/{jobPath}/api/json` | 是 |
| 构建详情 | `/job/{jobPath}/{buildNumber}/api/json` | 是 |
| 普通构建 | `/job/{jobPath}/build` POST | 是 |
| 参数构建 | `/job/{jobPath}/buildWithParameters` POST | 是 |
| 构建日志 | `/job/{jobPath}/{buildNumber}/consoleText` | 是 |
| 增量日志 | `/job/{jobPath}/{buildNumber}/logText/progressiveText` | 是 |
| 队列摘要 | `/queue/api/json` | 受限 |
| 队列项详情 | `/queue/item/{id}/api/json` | 是 |
| 停止构建 | `/job/{jobPath}/{buildNumber}/stop` POST | v0.2 |
| Job 配置 XML | `/job/{jobPath}/config.xml` | 暂不做 |
| Script Console | `/scriptText` | 禁止 |

CSRF 方面，Jenkins 对 POST 修改类请求通常要求 crumb；使用用户名和 API Token 的请求在 Jenkins 文档中说明可免 crumb，但实际部署可能受代理、插件和安全配置影响。实现上建议统一支持 crumb 自动获取：

1. 首次写请求前请求 `/crumbIssuer/api/json`。
2. 按 `connectionKey + credentialKey + baseUrl` 将 crumb 字段名和值保存到内存缓存。
3. crumb 默认 TTL 30 分钟，应用退出即清空，不落库。
4. 后续 POST 带上 crumb header。
5. 任意 403 crumb 相关错误立即清除缓存，并重试获取一次 crumb。
6. 如果 API Token 免 crumb，crumb 获取失败不一定阻断；写请求失败后再按错误提示降级处理。
7. 审计只记录“使用/刷新 crumb”，不记录 crumb 字段值。

---

## 4. 功能范围

### 4.1 v0.1 必做

#### Jenkins 连接管理

- 新增 Jenkins 连接列表。
- 新建 / 编辑 / 删除 / 恢复 Jenkins 连接。
- 支持复制连接配置，但不复制 `connectionKey`、名称、`credentialKey`、构建记录、artifact、最近 Job 和最近成功参数值。
- 最近成功参数值可能包含环境、分支、发布参数或 `secretRef` 引用，跨连接复制容易误触发或引用错误凭证。
- 复制后的连接默认 `enabled=false`，必须测试连接成功后才能启用。
- 复制操作写入审计日志。
- 启用/禁用连接不进入审批队列，但必须写审计。
- 禁用连接立即生效，阻断 UI 构建触发和 MCP 使用。
- 启用连接前必须先测试连接成功；`environment=prod` 或 `allow_mcp_write=true` 时，启用前弹出人工确认。
- 删除连接采用软删除：设置 `enabled=false` 和 `deleted_at`，不级联删除构建记录、artifact 记录和审计日志。
- 删除后的连接默认隐藏，可切换“显示已删除”。
- 恢复连接设置 `deleted_at=null` 和 `enabled=true`，并强制执行一次连接测试。
- 恢复测试失败也允许恢复，但状态标记为 `failed`，不允许触发构建和 MCP 使用。
- 恢复连接写入审计日志，不自动补齐 Jenkins 历史构建。
- 字段：
  - 连接 Key（创建后只读，不允许编辑）
  - 名称
  - Jenkins Base URL
  - 凭证引用 `credentialKey`
  - 可选 SSH 隧道服务器 `sshServerAlias`
  - 环境分类 `environment`
  - 自定义环境名称 `environmentLabel`
  - TLS 证书校验开关 `tlsVerify`
  - 默认 View
  - 默认文件夹
  - 是否允许 MCP 只读访问
  - 是否允许 MCP 写操作审批
  - 审批策略
  - 是否启用最近参数回填
  - 连接级风险规则（表单化编辑，底层保存为 JSON）
  - 构建完成通知策略
  - 备注
  - 启用状态
- 连接测试：
  - 分为网络连通性测试和凭证测试。
  - 连接测试失败也允许保存连接，便于后续编辑 URL、SSH 隧道或凭证后重新测试。
  - 校验 Base URL 可访问。
  - 读取 `X-Jenkins` 响应头或 `/api/json` 判断 Jenkins 实例。
  - 校验凭证可读取当前用户具备权限的 Job 列表。
  - 凭证不可用时仍允许完成网络连通性测试。
  - 凭证不可用时连接状态为 `credential_missing` 或 `credential_failed`，不能标记 `active`。
  - 网络不可达时连接状态为 `failed`。
  - UI 可显示“网络可达，凭证不可用”。
  - `failed`、`credential_missing`、`credential_failed` 连接不能读取 Job、触发构建或暴露给 MCP。
  - 返回脱敏账号摘要和 Jenkins 版本。
  - 保存 `credentialDisplayName` 或 `usernameMasked`，不保存 Token 明文或片段。
  - 返回能力探测结果：可读取 Job、可读取构建、可触发构建、可读取 artifact。
  - 保存最近一次脱敏测试结果，包含失败代码、失败摘要和能力探测 JSON。
  - 首版不镜像 Jenkins RBAC 权限模型，具体写操作仍以 Jenkins API 返回和审批链路为准。
  - v0.1 不做 Jenkins 凭证自动轮换检测，不后台主动探测 Token。
  - 只有用户手动测试连接，或读取 Job、触发构建等实际调用失败时，才更新连接状态为 `credential_failed` 并写审计。

Base URL 标准化：

- 只允许 `http://` 和 `https://`。
- 保存时去掉末尾 `/`。
- 保留 Jenkins 子路径，例如 `https://ci.example.com/jenkins`。
- 禁止保存带 query 或 hash 的 URL。
- 默认推荐使用 `https://`。
- `http://` 或 `tlsVerify=false` 时显示风险提示，写操作风险至少 L2；生产环境升 L3。

#### Job 浏览

- 展示 Jenkins Job 树：
  - Freestyle
  - Pipeline
  - Multibranch Pipeline
  - Folder / Organization Folder
- 支持读取和选择 Jenkins View。
- 连接配置可保存默认 View，Job 列表可按 View 读取。
- 不创建、修改、删除 Jenkins View。
- View 不存在时返回明确错误，不自动回退到 All。
- 支持关键字搜索，但 v0.1 只在当前 View / Folder 已加载 Job 树中本地过滤。
- 未加载的深层 Folder 不因搜索自动全站扫描。
- UI 需要提示“搜索当前已加载范围”。
- 支持按颜色 / 状态过滤：
  - success
  - failed
  - unstable
  - disabled
  - building
  - not built
- 展示 Job 基础信息：
  - 名称
  - URL
  - 类型
  - 最近构建号
  - 最近构建结果
  - 最近构建时间
  - 是否正在构建
  - 是否可构建
- Job 树使用短 TTL 内存缓存，默认 30-60 秒。
- 手动刷新必须绕过缓存重新请求 Jenkins。
- Folder / Job 树默认递归读取 3 层，最大递归深度 5 层。
- UI 展开更深 Folder 时按需加载，不一次性扫全站。
- 返回结果需要包含 `hasMore`，提示更深层级可继续加载。
- Multibranch Pipeline 只展示 Jenkins 已索引出的分支 Job，例如 `repo/branch`。
- v0.1 不做 SCM 分支发现，不读取 Git 分支列表，不触发 Jenkins scan / branch indexing。
- 不负责创建新分支构建；需要构建的目标必须已经是 Jenkins 可见 Job 或明确参数。
- 首版以 `jobFullName` 作为 Job 主标识，不自动识别 Jenkins Job 改名或移动 Folder。
- 当旧 `jobFullName` 返回 404 时，UI 提示“Job 可能已改名、移动或删除”。
- v0.1 支持轻量收藏 Job。
- 收藏只保存 `connectionKey + jobFullName + displayName + url + lastKnownStatus`。
- 收藏用于快速入口和构建跟踪范围。
- 收藏不触发自动全量轮询，只在用户打开 Jenkins 页面或手动刷新时更新状态。
- 收藏/取消收藏不走审批，但写审计。
- MCP v0.1 不管理收藏，仅供 UI 使用。

#### 构建列表

- 查看某个 Job 的构建列表。
- 展示：
  - 构建号
  - 状态
  - 触发原因
  - 触发人
  - 分支 / SCM 信息
  - 开始时间
  - 持续时间
  - 构建参数摘要
  - 结果链接
- 构建列表默认实时读取，不落库镜像 Jenkins 全量历史。
- 构建列表按 Job 分页读取，默认最新 30 条，单次最多 100 条。
- UI 提供“加载更多”，不默认扫描全部历史。
- 只有本应用触发、用户手动跟踪或收藏的构建进入本地持久化记录。
- v0.1 不提供独立 Jenkins 全局队列页。
- 构建触发后展示本次 queue item。
- 当前 Job 详情可显示该 Job 的排队状态。

#### 构建详情

- 展示构建详情：
  - Result
  - Building
  - Duration
  - Timestamp
  - Queue ID
  - Parameters
  - Causes
  - Change sets
  - Artifacts
  - Upstream / downstream 信息
- Causes 展示触发类型：手动 / SCM / 定时 / 上游构建 / 远程触发。
- 触发人只显示 displayName，不显示邮箱、ID token 或远程地址。
- 远程触发原因中的 URL、token、IP 需要脱敏或隐藏。
- MCP 返回 Causes 时同样脱敏。
- 不把 Jenkins 用户身份映射到 Tauri SSH 本地用户。
- Change Sets 只展示 commit id、作者显示名和提交摘要。
- 作者邮箱默认不展示。
- 提交摘要需要执行通用脱敏规则，防止 commit message 携带 token。
- MCP 返回 Change Sets 时同样脱敏。
- v0.1 不展示代码 diff。
- 如果 Jenkins build API 返回简单 test summary，可作为只读字段展示。
- v0.1 不解析 JUnit XML，不做测试报告表和测试趋势图。

#### 构建日志

- 读取 `consoleText`。
- 支持 progressive log：
  - `start`
  - `X-Text-Size`
  - `X-More-Data`
- UI 支持：
  - 自动滚动到底部
  - 暂停自动滚动
  - 关键字搜索
  - 错误 / 警告高亮
- 复制选中日志
- 只加载尾部 N KB
- v0.1 不提供日志文件下载，只支持查看和复制选中范围。
- MCP 日志读取默认返回尾部 200KB，`tailBytes` 最大 1MB。
- MCP progressive 日志读取支持 `start` 参数，并返回 `truncated`、`nextStart`、`textSize`。
- MCP/AI 日志内容永远使用脱敏结果。
- v0.1 不持久化 Jenkins 控制台日志正文。
- 本地只保存日志读取审计、读取范围、字节数、脱敏状态和必要的分析摘要元数据。
- progressive 日志读取按会话合并审计，会话 key 为 `requestId + connectionKey + jobFullName + buildNumber + viewer`。
- progressive 日志会话结束条件：用户关闭日志面板、构建完成后停止跟随、超过 60 秒无继续读取、切换到其他构建。
- 复制选中日志必须写轻量审计，只记录复制范围和字节数，不记录复制内容。
- 如 v0.2 支持日志导出，也只能导出脱敏日志，并作为单独导出操作写审计；原始日志永不提供下载。
- v0.2 如支持 AI 失败总结，只保存基于脱敏日志片段生成的本地分析记录，不保存原始日志。

#### 触发构建

- 支持普通构建。
- 支持参数化构建。
- 构建参数来源：
  - Job parameter definition
  - 用户手动输入
  - 最近构建参数回填
- Job 参数定义使用短 TTL 内存缓存，默认 60 秒，不持久化。
- 参数定义缓存 key 使用 `connectionKey + jobFullName`，如 Jenkins API 可获得 Job 配置更新时间或等价版本字段，则纳入缓存校验。
- 手动刷新必须绕过参数定义缓存。
- 创建构建审批前必须重新读取参数定义，或至少校验参数定义未变化。
- v0.1 不保存命名构建参数模板。
- v0.2 再支持每个 Job 保存多个命名参数模板。
- 参数模板中的敏感参数只能保存 `secretRef`，不能保存明文。
- 使用参数模板触发构建仍必须走审批，审批摘要展示模板名和参数摘要。
- 首版标准渲染 Jenkins 常见参数：string、boolean、choice、password、file。
- Active Choices、Extended Choice、Git Parameter 等动态插件参数不做深度适配；如果 API 无法返回可选值，UI 降级为手动输入或提示“需在 Jenkins 页面选择”。
- 动态参数默认标记 `dynamicParameter=true`。
- 如果无法获得稳定参数定义摘要，则该 Job 不允许 MCP 触发。
- UI 人工输入后触发必须升级到 L3 审批；如果 approved 执行前仍无法确认参数定义一致，则拒绝自动执行。
- 参数表单可识别 `BRANCH`、`GIT_BRANCH`、`COMMIT`、`GIT_COMMIT` 等字段并显示提示。
- v0.1 不自动从 Git 工作区读取分支或 commit 注入构建参数。
- 最近构建参数回填默认只允许复用同一 `requester` 在同一 `connectionKey + jobFullName + parameterName` 下授权使用过的值。
- 管理员可在连接级开启共享最近成功值；开启后相关构建触发风险至少 L3。
- 连接级可关闭最近参数回填；关闭后历史构建仍保留脱敏参数摘要，但不再用于 UI 或 MCP 回填。
- v0.1 只保留最近成功值，不保存无限历史值列表。
- 构建运行记录和审批摘要只保存脱敏后的参数摘要，不保存原始敏感参数值。
- 非敏感参数可保存原值。
- 敏感参数保存为 `***`、`secretRef` 或 `useLastSuccessfulValue` 标记。
- 触发前展示确认 Modal：
  - Jenkins 连接
  - Job 路径
  - 参数
  - 触发人
  - 风险等级
- 写操作必须创建审批请求。
- 审批通过后执行构建触发。
- v0.1 构建触发只能来自 UI 用户动作或 MCP controlled/approved 请求。
- v0.1 不支持批量触发多个 Job。
- v0.1 不提供定时触发、后台自动触发、失败后自动重试、构建完成后自动触发下游 Jenkins Job。
- 返回 queue item URL / queue ID，并轮询解析实际 build number。
- 触发成功后先返回 queueId / queueUrl，不阻塞等待完整构建详情。
- 后端 tracker 拿到 buildNumber 后，再读取一次构建详情并更新本地 run。
- UI 在 queue 阶段显示“排队中”，拿到 buildNumber 后切到构建详情。
- MCP approved 可先返回 queueId，不等待完整构建详情。
- Queue 等待 build number 默认超时 10 分钟；超时只代表 Tauri SSH 跟踪超时，不代表 Jenkins 构建失败。
- 支持 File Parameter，但只允许通过本地文件路径引用上传。
- File Parameter 在 controlled 阶段计算并固化文件名、大小、sha256 和修改时间。
- approved 执行时重新计算 sha256；如果文件变化则拒绝执行。
- File Parameter 持久化只保存文件名、大小、sha256、mtime；MCP 场景必要时只保存 basename，避免保存原始路径中的敏感目录信息。
- MCP 不允许传 base64 文件内容，不允许把文件内容写入审批 payload。

#### Artifact 下载

- 支持查看构建 artifacts 列表。
- 支持下载单个 artifact。
- v0.1 不支持批量下载多个 artifact。
- v0.1 不支持批量删除或批量清理记录。
- 下载记录写入审计日志。
- 大文件下载使用流式写入本地文件，避免一次性进入内存。
- 单个 artifact 默认最大 500MB。
- Jenkins 连接级可配置 artifact 下载上限，但最高不超过 2GB。
- 下载前优先读取 artifact 元数据；如果拿不到大小，下载过程中按已接收字节计数，超过上限立即中止并写审计。
- UI 下载可由用户选择保存路径。
- MCP 下载只能写入应用管理目录，例如 `appData/jenkins-artifacts/{connectionKey}/{job}/{buildNumber}/...`。
- MCP 不接受任意 `destinationPath`，不覆盖已有文件。
- UI 和 MCP 下载都受同一套大小限制。
- 首版不按扩展名黑名单阻断下载。
- `.sh`、`.bat`、`.cmd`、`.ps1`、`.exe`、`.dmg`、`.pkg`、`.jar` 等可执行或安装类文件标记为高风险 artifact。
- UI 下载高风险 artifact 前提示风险；MCP 返回包含 `riskFlags` 并写审计。
- 不自动执行、不自动打开高风险 artifact。
- 下载完成后创建 `jenkins_artifacts` 记录，返回 artifact record id、大小和 sha256。
- v0.1 只支持单个 artifact 的“清理本地文件”操作，不支持批量清理。
- 清理前必须弹人工确认。
- 清理只删除应用托管目录中的本地文件，不删除 `jenkins_artifacts` 记录、审批记录或审计记录。
- 清理后 artifact record 状态更新为 `local_deleted`；如果巡检或打开时发现文件不存在，状态更新为 `file_missing`。
- MCP 不提供 artifact 清理工具，避免 AI 删除本地制品。

#### 桌面通知

- 构建进入队列、开始、成功、失败、终止时可发送桌面通知。
- 通知开关按 Jenkins 连接配置。
- 通知文案统一中文。
- 通知标题示例：`Jenkins 构建失败` / `Jenkins 构建不稳定` / `Jenkins 构建已终止`。
- 通知内容只包含连接名称、Job 名称和构建号。
- 通知内容不包含参数值、日志片段和 artifact 路径。
- 默认通知失败、终止、不稳定。
- 成功通知默认关闭，用户可在连接配置中开启。
- 通知只针对本应用触发或用户手动跟踪的构建，不默认监控所有 Jenkins Job。
- 点击通知打开 Tauri SSH 内的构建详情，不直接打开 Jenkins。
- 通知点击只作为本地 UI 导航，不写审计；查看日志和下载 artifact 仍按对应动作写审计。

#### 审计日志

- 连接测试。
- Job 列表读取。
- 构建详情读取。
- 日志读取。
- 创建审批。
- 触发构建执行。
- 构建停止执行。
- MCP 调用。

### 4.2 v0.2 应做

- 停止正在运行的构建。
- v0.2 构建参数模板。
- v0.2 构建失败原因 AI 总结。
- v0.2 从构建日志中提取错误段落。
- Jenkins scan / branch indexing 不进入 v0.2。
- 与 Git 工作区联动：
  - 当前分支 / commit 作为构建参数。
  - 构建完成后回写本地运行记录。
- 与自动部署联动：
  - Jenkins 构建成功后创建部署 dry-run。
  - Jenkins artifact 进入部署 artifact。
- MCP 增量日志订阅式读取。

### 4.3 v0.3 可选

- Jenkins view 管理，只读优先。
- 构建趋势图。
- Test Report 浏览。
- 失败用例列表。
- Pipeline replay。
- 重试参数构建。
- Jenkins scan / branch indexing；如支持，必须作为 L3 写操作并进入审批审计。
- Blue Ocean URL 深链，需要先通过能力探测判断插件是否可用。
- 多 Jenkins 聚合看板，仅限只读。
- 定时刷新策略增强。
- 构建队列阻塞分析。
- Agent / executor 只读状态，通过 `/computer/api/json` 等接口实现。

### 4.4 明确暂不做

- 不做 Jenkins 用户权限管理。
- 不做插件安装 / 升级。
- 不做 Job XML 编辑。
- 不做 Script Console。
- 不做 Jenkins controller restart。
- 不做节点脚本执行。
- 不做 Jenkins 凭据读取或复制。
- v0.1 不做 Jenkins 连接导入/导出。
- 后续如需导入/导出，必须脱敏，不能包含安全凭证明文，并需要处理跨机器凭证缺失。

---

## 5. 信息架构与菜单规划

建议新增入口：

```text
运维
├── 自动部署
├── Jenkins
├── 资源监控
└── 数据库管理
```

Jenkins 页面内部结构：

```text
Jenkins 页面
├── 连接栏
│   ├── 连接选择
│   ├── 测试连接
│   ├── 新建连接
│   └── 刷新
├── Job 浏览
│   ├── Job 树
│   ├── 搜索/过滤
│   ├── Job 状态卡片
│   └── 在 Jenkins 打开
├── 构建列表
│   ├── 构建 Table
│   ├── 触发构建
│   ├── 队列状态
│   └── 在 Jenkins 打开
└── 构建详情 Drawer
    ├── 基本信息
    ├── 参数
    ├── Change Sets
    ├── Artifacts
    ├── 日志
    └── 在 Jenkins 打开
```

v0.1 页面一次只选择一个 Jenkins 连接作为当前上下文，不跨连接聚合 Job、构建、队列或失败统计。后续 v0.3 如做多 Jenkins 聚合看板，只能作为只读看板；写操作必须回到单连接上下文。

---

## 6. UI 设计建议

### 6.1 页面布局

首版建议使用三栏工作台：

1. 左侧 280px：连接和 Job 树。
2. 中间：构建列表和队列列表。
3. 右侧 Drawer：构建详情和日志。

不要做成大段说明页，首屏应直接是可操作工作台。

### 6.2 主要组件

| 区域 | Ant Design 组件 | 说明 |
|------|------------------|------|
| 连接选择 | Select / Button / Dropdown | 切换 Jenkins 实例 |
| Job 树 | Tree / Input.Search | Folder 和 Job 层级 |
| 构建列表 | Table / Tag / Tooltip | 构建号、状态、耗时、触发人 |
| 参数构建 | Modal / Form / Input / Select / Checkbox | 动态渲染参数 |
| 构建详情 | Drawer / Descriptions / Tabs | 详情、参数、变更、日志 |
| 日志 | 虚拟滚动容器 / Typography.Text | 大文本分段加载 |
| 队列 | Table / Badge | queue item 状态 |
| 风险提示 | Alert / Modal.confirm | 写操作审批提示 |
| 外部跳转 | Button / Tooltip | 使用系统浏览器打开 Jenkins 原始页面 |

### 6.3 构建状态映射

| Jenkins 状态 | UI 状态 | 颜色 |
|--------------|---------|------|
| SUCCESS | 成功 | green |
| FAILURE | 失败 | red |
| UNSTABLE | 不稳定 | orange |
| ABORTED | 已终止 | default |
| BUILDING | 构建中 | blue |
| NOT_BUILT | 未构建 | default |
| UNKNOWN | 未知 | default |

UI 最终状态按归一后的 result/status 显示，Jenkins `color` 字段只作为原始字段保留，不直接依赖 `blue_anime`、`red_anime` 等值驱动 UI。MCP 返回需要同时包含归一状态和 Jenkins 原始字段。

### 6.4 日志体验

- 默认读取尾部 200 KB。
- 点击“加载更多”向前拉取。
- 正在构建时每 2 秒 progressive 读取。
- 日志搜索只在已加载内容中搜索。
- 错误高亮规则：
  - `ERROR`
  - `FAILURE`
  - `Exception`
  - `Traceback`
  - `BUILD FAILED`
  - `npm ERR!`
  - `MavenCompilationFailureException`

---

## 7. 后端架构设计

### 7.1 文件拆分

```text
src-tauri/src/
├── commands/
│   └── jenkins.rs
├── services/
│   └── jenkins.rs
├── database/
│   └── mod.rs
├── models/
│   └── mod.rs
└── dev_server/
    └── mod.rs

src/
├── pages/
│   └── jenkins/
│       └── index.tsx
├── lib/api/
│   └── jenkins.ts
└── types/
    └── jenkins.ts
```

### 7.2 调用链路

```text
React Jenkins 页面
  -> jenkinsApi
    -> Tauri invoke
      -> commands/jenkins.rs
        -> services/jenkins.rs
          -> secure_credential.rs 获取短期凭据 / 密钥
          -> reqwest 调用 Jenkins API
          -> database/mod.rs 持久化连接、审计、最近记录
          -> approval.rs 创建/校验审批
          -> audit.rs 记录审计
```

MCP 调用：

```text
MCP tool call
  -> dev_server/mod.rs
    -> JenkinsService
      -> ApprovalService / AuditService / SecureCredentialService
```

### 7.3 Rust Service 职责

`JenkinsService` 应承担：

- 连接 CRUD 业务校验。
- Jenkins URL 标准化。
- 安全凭证解析。
- HTTP Basic / Bearer / API Token 认证封装。
- Crumb 自动获取和内存短期缓存。
- Jenkins API 请求封装。
- Jenkins JSON 解析与字段兼容。
- Job path 编码。
- 日志分页 / 截断。
- 写操作审批 requestHash 生成。
- 审计日志写入。
- 连接级请求限流和本地等待队列。
- v0.1 不在 Jenkins 连接中新增 HTTP proxy 配置或 proxy credential 类型。

### 7.4 不建议前端直接请求 Jenkins

原因：

- Jenkins 常见跨域配置不一定允许 WebView 直接请求。
- 前端不能接触 Token。
- CSRF crumb、Cookie、认证失败重试需要后端统一处理。
- 审计和审批必须在 Service 层强制执行。
- MCP 与 UI 需要同权复用。

---

## 8. 数据库设计

### 8.1 表：jenkins_connections

```sql
CREATE TABLE IF NOT EXISTS jenkins_connections (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_key      TEXT NOT NULL UNIQUE,
    name                TEXT NOT NULL,
    config_version      INTEGER NOT NULL DEFAULT 1,
    base_url            TEXT NOT NULL,
    credential_key      TEXT NOT NULL DEFAULT '',
    credential_display_name TEXT NOT NULL DEFAULT '',
    username_masked     TEXT NOT NULL DEFAULT '',
    ssh_server_alias    TEXT NOT NULL DEFAULT '',
    environment         TEXT NOT NULL DEFAULT 'custom',
    environment_label   TEXT NOT NULL DEFAULT '',
    tls_verify          INTEGER NOT NULL DEFAULT 1,
    default_view        TEXT NOT NULL DEFAULT '',
    default_folder      TEXT NOT NULL DEFAULT '',
    allow_mcp_read      INTEGER NOT NULL DEFAULT 0,
    allow_mcp_write     INTEGER NOT NULL DEFAULT 0,
    approval_policy     TEXT NOT NULL DEFAULT 'write_requires_approval',
    parameter_prefill_enabled INTEGER NOT NULL DEFAULT 1,
    risk_rules_json     TEXT NOT NULL DEFAULT '[]',
    notification_on_success INTEGER NOT NULL DEFAULT 0,
    notification_on_failure INTEGER NOT NULL DEFAULT 1,
    notification_on_unstable INTEGER NOT NULL DEFAULT 1,
    notification_on_aborted INTEGER NOT NULL DEFAULT 1,
    status              TEXT NOT NULL DEFAULT 'unknown',
    version             TEXT NOT NULL DEFAULT '',
    capabilities_json   TEXT NOT NULL DEFAULT '{}',
    last_error_code     TEXT NOT NULL DEFAULT '',
    last_error_message  TEXT NOT NULL DEFAULT '',
    description         TEXT NOT NULL DEFAULT '',
    enabled             INTEGER NOT NULL DEFAULT 1,
    last_tested_at      TEXT DEFAULT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    deleted_at          TEXT DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_jenkins_connections_key
ON jenkins_connections(connection_key);

CREATE INDEX IF NOT EXISTS idx_jenkins_connections_enabled
ON jenkins_connections(enabled, deleted_at);
```

字段说明：

| 字段 | 说明 |
|------|------|
| `connection_key` | 本机唯一连接标识，创建后只读；如需修改 Key，应新建连接并软删除旧连接 |
| `config_version` | 连接关键配置版本，默认 1；关键字段变化时自增，用于审批后变更检测 |
| `base_url` | Jenkins 根地址，保存前按 Base URL 标准化规则处理 |
| `credential_key` | 安全凭证引用 |
| `credential_display_name` | 最近一次连接测试得到的凭证显示名，必须脱敏 |
| `username_masked` | 最近一次连接测试得到的账号摘要，必须脱敏 |
| `ssh_server_alias` | 可选 SSH 隧道服务器，用于访问内网 Jenkins |
| `environment` | 内置枚举：`dev` / `test` / `staging` / `prod` / `custom`，UI 显示为开发 / 测试 / 预发 / 生产 / 自定义 |
| `environment_label` | 自定义环境名称，仅在 `environment=custom` 或需要补充展示时使用 |
| `tls_verify` | 是否校验 TLS 证书，默认 true |
| `default_view` | 默认 Jenkins View，可为空 |
| `default_folder` | 默认 Folder 路径，可为空 |
| `allow_mcp_read` | 是否允许 MCP 读取 Job、构建、日志 |
| `allow_mcp_write` | 是否允许 MCP 创建构建/停止构建审批 |
| `approval_policy` | `write_requires_approval` / `all_requires_approval` |
| `parameter_prefill_enabled` | 是否启用最近参数回填，默认 true |
| `risk_rules_json` | 连接级 Job/参数风险规则，v0.1 由表单化编辑器生成，底层保存 JSON |
| `notification_on_success` | 构建成功是否通知，默认 false |
| `notification_on_failure` | 构建失败是否通知，默认 true |
| `notification_on_unstable` | 构建不稳定是否通知，默认 true |
| `notification_on_aborted` | 构建终止是否通知，默认 true |
| `status` | `unknown` / `active` / `failed` / `disabled` / `credential_missing` / `credential_failed` |
| `version` | 从 `X-Jenkins` 或 API 获取 |
| `capabilities_json` | 最近一次连接测试得到的能力探测结果 |
| `last_error_code` | 最近一次连接测试失败代码，必须脱敏 |
| `last_error_message` | 最近一次连接测试失败摘要，必须脱敏 |
| `deleted_at` | 软删除时间；删除连接不清理历史构建、artifact 和审计记录 |

连接测试结果保存规则：

- 保存 `last_tested_at`、`status`、`version`、`capabilities_json`、`last_error_code` 和 `last_error_message`。
- 保存 `credential_display_name` 或 `username_masked`，用于连接列表展示凭证摘要。
- `last_error_message` 只保存排障摘要，不保存 Token、Cookie、crumb、Authorization header、完整请求头或敏感参数。
- UI 连接列表展示简短失败原因，连接详情 Drawer 展示脱敏排障建议。
- UI 连接列表可显示“凭证：jenkins-user / api-token”等脱敏摘要，不显示 Token 片段。
- MCP `jenkins_connections_list` 只返回 `credentialKey` 和脱敏凭证摘要。
- 如凭证被删除或不可用，连接显示“凭证不可用”，并禁止构建触发和 MCP 写。
- `credential_missing` / `credential_failed` 连接不允许读取 Job、读取日志、触发构建，也不作为 MCP 可用连接返回。
- 凭证摘要只在连接测试时更新，不在列表页直接读取密钥。
- Jenkins 模块不做自动凭证轮换检测；凭证状态只在手动连接测试或实际 Jenkins 调用失败时更新。
- 连接测试审计记录可保存更多上下文，但必须执行同样的脱敏规则。

关键字段变更失效规则：

- `base_url`、`credential_key`、`ssh_server_alias`、`tls_verify` 任一变化后，连接状态回到 `unknown`。
- `config_version` 自增。
- `name`、`description`、通知开关、默认 View/Folder、风险规则、MCP 开关不触发 `config_version` 自增。
- 风险规则和 MCP 开关在 approved 执行时实时复验，不依赖 `config_version` 判定。
- 立即清空该连接相关的 crumb 内存缓存和 Job 树缓存。
- 清空 `capabilities_json`、`version`、`credential_display_name`、`username_masked`、`last_error_code`、`last_error_message`。
- 变更后必须重新测试连接，测试成功前不允许读取 Job、触发构建或作为 MCP 可用连接返回。
- 该规则避免旧地址、旧凭证、旧隧道或旧 TLS 策略的能力标签误导 UI、审批和 MCP。
- `allow_mcp_read` / `allow_mcp_write` 变更立即生效，不等待已有审批完成。

删除语义：

- `delete_jenkins_connection` 默认只软删除连接。
- 软删除后的连接不再允许触发构建、刷新状态或通过 MCP 使用。
- 已触发或跟踪的构建记录仍可查看本地快照，但不能继续同步 Jenkins，除非恢复连接。
- 本地 artifact record 和 artifact 文件默认保留。
- 审计日志永久保留，不随连接删除。
- 如用户需要清理本地 artifact 文件，单独提供“清理本地制品”操作，并写入审计日志。

### 8.2 表：jenkins_recent_jobs

```sql
CREATE TABLE IF NOT EXISTS jenkins_recent_jobs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_key      TEXT NOT NULL,
    job_full_name       TEXT NOT NULL,
    job_url             TEXT NOT NULL DEFAULT '',
    job_type            TEXT NOT NULL DEFAULT '',
    display_name        TEXT NOT NULL DEFAULT '',
    color               TEXT NOT NULL DEFAULT '',
    last_build_number   INTEGER DEFAULT NULL,
    last_result         TEXT NOT NULL DEFAULT '',
    last_built_at       TEXT DEFAULT NULL,
    favorite            INTEGER NOT NULL DEFAULT 0,
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(connection_key, job_full_name)
);

CREATE INDEX IF NOT EXISTS idx_jenkins_recent_jobs_connection
ON jenkins_recent_jobs(connection_key, updated_at DESC);
```

用途：

- 缓存最近访问 Job。
- 支持收藏。
- 支持连接切换后快速显示历史列表。

### 8.3 表：jenkins_build_runs

```sql
CREATE TABLE IF NOT EXISTS jenkins_build_runs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    run_key             TEXT NOT NULL UNIQUE,
    request_id          TEXT NOT NULL DEFAULT '',
    connection_key      TEXT NOT NULL,
    job_full_name       TEXT NOT NULL,
    queue_id            INTEGER DEFAULT NULL,
    build_number        INTEGER DEFAULT NULL,
    trigger_type        TEXT NOT NULL DEFAULT 'manual',
    parameters_json     TEXT NOT NULL DEFAULT '{}',
    status              TEXT NOT NULL DEFAULT 'queued',
    status_source       TEXT NOT NULL DEFAULT 'local',
    result              TEXT NOT NULL DEFAULT '',
    approval_id         INTEGER DEFAULT NULL,
    triggered_by        TEXT NOT NULL DEFAULT 'local-user',
    started_at          TEXT DEFAULT NULL,
    finished_at         TEXT DEFAULT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_jenkins_build_runs_connection
ON jenkins_build_runs(connection_key, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_jenkins_build_runs_job
ON jenkins_build_runs(connection_key, job_full_name, created_at DESC);
```

用途：

- 记录由 Tauri SSH 触发或跟踪的构建。
- 记录用户手动点击“跟踪此构建”的构建。
- `request_id` 串联审批创建、approved 执行、Jenkins HTTP 请求、queue 跟踪和审计日志。
- 关联审批 ID。
- `parameters_json` 保存脱敏参数摘要；非敏感参数可保存原值，敏感参数只能保存 `***`、`secretRef` 或 `useLastSuccessfulValue` 标记。
- 非敏感参数原值仅用于审批摘要、构建记录和最近成功参数回填；不提供无限历史参数值列表。
- `status` 固定枚举：`queued`、`building`、`success`、`failure`、`unstable`、`aborted`、`not_built`、`queue_timeout`、`tracking_timeout`、`sync_failed`、`unknown`。
- `status_source` 固定枚举：`jenkins` / `local`；`queue_timeout`、`tracking_timeout`、`sync_failed`、`unknown` 默认属于本地状态。
- UI 和 MCP 必须区分 `status_source=local` 的跟踪状态与 `status_source=jenkins` 的 Jenkins 真实结果。
- approved 执行时从安全凭证或受控文件路径解析真实值，不从 `parameters_json` 还原敏感值。
- 后续与自动部署、通知、AI 分析联动。
- 不是 Jenkins build history 镜像表，不全量同步 Jenkins 历史。
- 不保存 Jenkins 控制台日志正文，只保存构建元数据和脱敏参数摘要。
- `job_full_name` 保留触发或跟踪时的原始 Job 路径；Job 改名或移动后不自动迁移历史记录。
- 用户可手动将历史记录关联到新 Job；首版不做自动匹配和批量迁移。
- `jenkins_build_runs` 持久保留，除非后续引入全局数据清理策略。

### 8.4 表：jenkins_recent_parameter_values

```sql
CREATE TABLE IF NOT EXISTS jenkins_recent_parameter_values (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_key      TEXT NOT NULL,
    job_full_name       TEXT NOT NULL,
    parameter_name      TEXT NOT NULL,
    requester           TEXT NOT NULL DEFAULT 'local-user',
    value_kind          TEXT NOT NULL DEFAULT 'plain',
    value_json          TEXT NOT NULL DEFAULT '{}',
    sensitive           INTEGER NOT NULL DEFAULT 0,
    updated_from_run_key TEXT NOT NULL DEFAULT '',
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE(connection_key, job_full_name, parameter_name, requester)
);

CREATE INDEX IF NOT EXISTS idx_jenkins_recent_parameter_values_job
ON jenkins_recent_parameter_values(connection_key, job_full_name, requester);
```

用途：

- 保存最近成功构建可回填的参数值，不保存无限历史值列表。
- 回填查询不扫描 `jenkins_build_runs` 历史。
- `requester` 默认按本地请求方隔离；共享模式使用 `requester='__shared__'`。
- `value_kind` 取值：`plain`、`secret_ref`、`use_last_successful_value`、`file_meta`。
- 非敏感参数可保存原值；敏感参数不得保存明文，只能保存 `secretRef` 或授权复用标记。
- `updated_from_run_key` 指向最近一次成功构建记录，便于审计来源。
- 连接关闭 `parameter_prefill_enabled` 后，不读取该表作为回填候选，但不立即删除已有记录。
- UI 在 Job 参数表单旁提供“忘记此参数最近值”，仅删除当前 `connectionKey + jobFullName + parameterName + requester` 记录。
- 共享值 `requester='__shared__'` 只能由管理员删除。
- 忘记最近参数值不走审批，但必须写审计。
- MCP 不提供删除最近参数值工具。

### 8.5 表：jenkins_artifacts

```sql
CREATE TABLE IF NOT EXISTS jenkins_artifacts (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    artifact_key        TEXT NOT NULL UNIQUE,
    request_id          TEXT NOT NULL DEFAULT '',
    connection_key      TEXT NOT NULL,
    job_full_name       TEXT NOT NULL,
    build_number        INTEGER NOT NULL,
    artifact_path       TEXT NOT NULL,
    file_name           TEXT NOT NULL,
    local_path          TEXT NOT NULL,
    size_bytes          INTEGER NOT NULL DEFAULT 0,
    sha256              TEXT NOT NULL DEFAULT '',
    source_url          TEXT NOT NULL DEFAULT '',
    status              TEXT NOT NULL DEFAULT 'available',
    downloaded_by       TEXT NOT NULL DEFAULT 'local-user',
    downloaded_at       TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_jenkins_artifacts_build
ON jenkins_artifacts(connection_key, job_full_name, build_number);
```

用途：

- Artifact 下载后成为应用内一等记录。
- `request_id` 串联 artifact 下载请求、Jenkins HTTP 请求和审计日志。
- 自动部署后续引用 artifact record，而不是裸本地路径。
- MCP 下载 artifact 只返回 artifact record id、本地托管路径、大小、sha256 和 `riskFlags`。
- Artifact 清理、打开、部署联动都通过该表管理。
- `status` 取值：`available`、`local_deleted`、`file_missing`。
- 清理本地文件不删除 artifact record；后续部署引用时必须校验 `status=available` 且文件存在。
- 自动部署引用 artifact 时，需要按部署策略重新校验 artifact 类型和风险。

---

## 9. 数据模型草案

### 9.1 JenkinsConnection

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsConnection {
    pub id: i64,
    pub connection_key: String,
    pub config_version: i64,
    pub name: String,
    pub base_url: String,
    pub credential_key: String,
    pub credential_display_name: String,
    pub username_masked: String,
    pub ssh_server_alias: String,
    pub environment: String,
    pub environment_label: String,
    pub tls_verify: bool,
    pub default_view: String,
    pub default_folder: String,
    pub allow_mcp_read: bool,
    pub allow_mcp_write: bool,
    pub approval_policy: String,
    pub parameter_prefill_enabled: bool,
    pub risk_rules_json: String,
    pub notification_on_success: bool,
    pub notification_on_failure: bool,
    pub notification_on_unstable: bool,
    pub notification_on_aborted: bool,
    pub status: String,
    pub version: String,
    pub capabilities_json: String,
    pub last_error_code: String,
    pub last_error_message: String,
    pub description: String,
    pub enabled: bool,
    pub last_tested_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

### 9.2 JenkinsJob

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsJob {
    pub name: String,
    pub full_name: String,
    pub display_name: String,
    pub url: String,
    pub class_name: String,
    pub job_type: String,
    pub color: String,
    pub buildable: bool,
    pub in_queue: bool,
    pub last_build_number: Option<i64>,
    pub last_build_result: String,
    pub last_build_at: Option<String>,
    pub children: Vec<JenkinsJob>,
}
```

### 9.3 JenkinsBuild

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsBuild {
    pub number: i64,
    pub url: String,
    pub status: String,
    pub status_source: String,
    pub result: String,
    pub building: bool,
    pub duration_ms: i64,
    pub timestamp: i64,
    pub display_name: String,
    pub full_display_name: String,
    pub queue_id: Option<i64>,
    pub causes: Vec<JenkinsBuildCause>,
    pub parameters: serde_json::Value,
}
```

### 9.4 TriggerJenkinsBuildInput

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerJenkinsBuildInput {
    pub connection_key: String,
    pub job_full_name: String,
    pub parameters: serde_json::Value,
    pub requester: Option<String>,
    pub reason: Option<String>,
}
```

### 9.5 JenkinsBuildRun

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsBuildRun {
    pub id: i64,
    pub run_key: String,
    pub request_id: String,
    pub connection_key: String,
    pub job_full_name: String,
    pub queue_id: Option<i64>,
    pub build_number: Option<i64>,
    pub trigger_type: String,
    pub parameters_json: serde_json::Value,
    pub status: String,
    pub status_source: String,
    pub result: String,
    pub approval_id: Option<i64>,
    pub triggered_by: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

### 9.6 JenkinsRecentParameterValue

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsRecentParameterValue {
    pub id: i64,
    pub connection_key: String,
    pub job_full_name: String,
    pub parameter_name: String,
    pub requester: String,
    pub value_kind: String,
    pub value_json: serde_json::Value,
    pub sensitive: bool,
    pub updated_from_run_key: String,
    pub updated_at: String,
}
```

### 9.7 JenkinsArtifact

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JenkinsArtifact {
    pub id: i64,
    pub artifact_key: String,
    pub request_id: String,
    pub connection_key: String,
    pub job_full_name: String,
    pub build_number: i64,
    pub artifact_path: String,
    pub file_name: String,
    pub local_path: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub source_url: String,
    pub status: String,
    pub downloaded_by: String,
    pub downloaded_at: String,
    pub created_at: String,
}
```

---

## 10. Command 设计

### 10.1 连接管理

| Command | 说明 |
|---------|------|
| `list_jenkins_connections` | 列出 Jenkins 连接 |
| `upsert_jenkins_connection` | 新建/编辑 Jenkins 连接 |
| `duplicate_jenkins_connection` | 复制连接配置，不复制 Key、名称、凭证、历史数据、最近 Job 和最近成功参数值 |
| `delete_jenkins_connection` | 删除 Jenkins 连接 |
| `restore_jenkins_connection` | 恢复软删除的 Jenkins 连接并触发连接测试 |
| `test_jenkins_connection` | 测试连接并刷新版本/状态 |
| `forget_jenkins_recent_parameter_value` | 忘记单个参数的最近值；不走审批但写审计，不能由 MCP 调用 |

### 10.2 只读查询

| Command | 说明 |
|---------|------|
| `list_jenkins_jobs` | 读取 Job 树或当前 Folder 下 Job，支持 `forceRefresh` |
| `get_jenkins_job_detail` | 读取 Job 详情 |
| `list_jenkins_builds` | 读取构建列表，默认实时读取，支持 `limit` 和 `cursor/offset`，默认 30、最大 100 |
| `get_jenkins_build_detail` | 读取构建详情 |
| `read_jenkins_build_log` | 读取构建日志 |
| `read_jenkins_build_log_progressive` | 增量读取构建日志 |
| `record_jenkins_log_copy_audit` | 记录日志复制轻量审计，不保存复制内容 |
| `list_jenkins_queue` | 读取队列 |
| `get_jenkins_queue_item` | 读取队列项 |

### 10.3 写操作

| Command | 说明 |
|---------|------|
| `create_jenkins_build_trigger_approval` | 创建触发构建审批 |
| `execute_jenkins_build_trigger_approved` | 审批后触发构建 |
| `create_jenkins_build_stop_approval` | v0.2 创建停止构建审批 |
| `execute_jenkins_build_stop_approved` | v0.2 审批后停止构建 |
| `cleanup_jenkins_artifact_local_file` | 清理单个 artifact 本地文件；不走审批但人工确认并写审计 |

---

## 11. MCP 工具设计

### 11.1 只读工具

| MCP Tool | 风险 | 说明 |
|----------|------|------|
| `jenkins_connections_list` | readonly | 列出允许 MCP 使用的 Jenkins 连接 |
| `jenkins_jobs_list` | readonly | 读取 Job 树、指定 Folder 或指定 View，允许 `viewName`、`depth` 和 `forceRefresh=true` |
| `jenkins_job_detail` | readonly | 读取 Job 详情和参数定义 |
| `jenkins_builds_list` | readonly | 读取构建列表，默认实时读取，`limit` 默认 30、最大 100，支持 `cursor/offset` |
| `jenkins_build_detail` | readonly | 读取构建详情 |
| `jenkins_build_log_read` | readonly | 读取受限长度日志 |
| `jenkins_build_log_progressive` | readonly | 增量读取日志 |
| `jenkins_queue_list` | readonly | 读取受限队列摘要，优先按 Job 查询并限制数量 |
| `jenkins_queue_item` | readonly | 读取队列项 |

MCP 工具通用返回：

- 每次调用返回 `requestId`。
- `requestId` 用于串联 MCP 调用、审批、Jenkins HTTP 请求、queue 跟踪、build run、artifact 下载和审计日志。
- MCP 返回不暴露内部错误堆栈；错误响应只包含 `requestId`、错误码、脱敏错误摘要和必要排障提示。

MCP 只读轻量审计：

- `jenkins_jobs_list`、`jenkins_job_detail`、`jenkins_builds_list`、`jenkins_build_detail`、`jenkins_build_log_read`、`jenkins_build_log_progressive`、`jenkins_queue_list`、`jenkins_queue_item` 都写轻量审计。
- 审计字段包括 `requestId`、工具名、`connectionKey`、`jobFullName`、`buildNumber`、分页参数、日志范围、结果状态、耗时和脱敏错误摘要。
- 不记录完整响应、Job 树完整内容、构建列表完整内容、日志正文、构建参数明文或 Jenkins 原始错误堆栈。
- `jenkins_connections_list` 只记录汇总审计，例如返回数量、过滤条件、结果状态和耗时，不记录每个连接详情。

日志工具参数和返回：

- `jenkins_build_log_read` 支持 `tailBytes`，默认 204800，最大 1048576。
- `jenkins_build_log_progressive` 支持 `start`，用于 Jenkins progressive log 增量读取。
- 日志工具返回 `truncated`、`nextStart`、`textSize` 和脱敏后的 `content`。
- MCP/AI 不提供原始日志模式。

构建列表工具参数和审计：

- `jenkins_builds_list` 必须指定 `connectionKey` 和 `jobFullName`。
- `limit` 默认 30，最大 100。
- 支持 `cursor` 或 `offset` 继续读取更早构建。
- 不扫描 Jenkins 全量历史。
- 审计只记录请求元数据、分页参数和目标 Job，不记录完整响应内容。

Job 列表参数和返回：

- `jenkins_jobs_list` 支持 `depth`，默认 3，最大 5。
- 超过最大深度的 Folder 返回 `hasMore=true`，由 UI 或调用方按需继续读取。
- 不提供一次性扫描全站 Job 的工具参数。
- MCP 不提供全站模糊搜索工具，也不允许通过搜索结果直接触发构建。

队列工具参数和返回：

- `jenkins_queue_list` 默认只返回当前连接下调用方可见的队列摘要，并限制数量。
- 优先通过 `jobFullName` 查询指定 Job 相关队列。
- v0.1 不提供全局队列监控视图；v0.2 可作为队列监控增强。

### 11.2 写操作工具

| MCP Tool | 风险 | 说明 |
|----------|------|------|
| `jenkins_build_trigger_controlled` | L2/L3 | 创建构建审批 |
| `jenkins_build_trigger_approved` | L2/L3 | 审批后触发构建 |
| `jenkins_build_stop_controlled` | L2/L3 | v0.2 创建停止构建审批 |
| `jenkins_build_stop_approved` | L2/L3 | v0.2 审批后停止构建 |
| `jenkins_artifact_download` | readonly | 下载 artifact 到应用托管目录并创建 artifact record |

MCP 不提供 artifact 清理工具；本地 artifact 文件清理只能由 UI 用户人工确认后执行。

### 11.3 requestHash 固化字段

触发构建审批 payload 必须包含：

```json
{
  "tool": "jenkins_build_trigger_controlled",
  "connectionKey": "prod-jenkins",
  "connectionConfigVersion": 3,
  "jobFullName": "folder/service-deploy",
  "parameterDefinitionHash": "sha256-of-parameter-definition-summary",
  "parameters": {
    "BRANCH": "master",
    "ENV": "prod"
  },
  "sensitiveParameterRefs": {
    "DEPLOY_PASSWORD": {
      "secretRef": "jenkins-prod-deploy-password"
    }
  },
  "fileParameters": [
    {
      "name": "PACKAGE",
      "fileName": "release.zip",
      "sizeBytes": 10485760,
      "sha256": "controlled-stage-sha256",
      "mtime": "2026-07-04T10:30:00+08:00"
    }
  ],
  "requester": "local-user",
  "reason": "发布生产版本",
  "riskLevel": "L3",
  "riskFlags": ["prod_environment", "file_parameter"],
  "createdAtBucket": "2026-07-04T10:30+08:00"
}
```

approved 执行时必须重新计算 hash，并校验：

- `approvalId`
- `action`
- `connectionKey`
- `connectionConfigVersion`
- `jobFullName`
- `parameterDefinitionHash`
- 脱敏后的普通参数摘要
- 敏感参数引用：`secretRef` / `useLastSuccessfulValue`
- File Parameter 受控元数据：参数名、文件名、大小、sha256、mtime
- `requester`
- `reason`
- `riskLevel`
- `riskFlags`
- `createdAtBucket`
- `requestHash`

任何字段变化都必须拒绝执行。File Parameter 在 approved 阶段必须重新计算 sha256；文件内容、大小或修改时间变化时拒绝执行。

`parameterDefinitionHash` 由 controlled 阶段读取到的参数定义摘要计算，摘要包括参数名、类型、是否敏感、choice 可选项摘要、是否 File Parameter 等字段。approved 执行前必须重新读取参数定义并计算 hash；不一致时拒绝执行，错误码 `parameter_definition_changed_after_approval`。

如果动态参数插件导致参数定义无法稳定生成 hash：

- 参数定义摘要中标记 `dynamicParameter=true`。
- MCP 不允许触发该 Job。
- UI 可提示跳转 Jenkins 原页面，或允许人工输入后进入 L3 审批。
- approved 执行前仍必须重新读取定义；无法确认一致时拒绝自动执行。

`requestHash` payload 不允许包含明文敏感参数值、Token、Cookie、crumb、Authorization header 或文件内容。

### 11.4 MCP 触发构建约束

- `jenkins_build_trigger_controlled` 必须显式传入 `connectionKey`、`jobFullName` 和 `parameters`。
- 不提供 `trigger_by_keyword`、`trigger_latest_matched_job` 等模糊触发工具。
- `jenkins_jobs_list` 可以返回候选 Job，但触发构建必须由用户或上层流程明确选择。
- 如果搜索结果不唯一，MCP 必须返回候选列表，不得自行选择。
- MCP 不允许猜测动态插件参数值；无法确定的参数必须返回参数缺失或需要人工确认。
- 敏感参数不允许直接传明文，只允许使用 `secretRef` 或连接允许的 `useLastSuccessfulValue`。
- `useLastSuccessfulValue` 默认只能复用同一 requester 的授权历史值，不允许跨用户复用。
- 连接级显式开启共享最近成功值后才允许跨用户复用，且相关触发审批风险至少 L3。

敏感参数示例：

```json
{
  "parameters": {
    "ENV": "prod",
    "BRANCH": "master",
    "DEPLOY_PASSWORD": {
      "secretRef": "jenkins-prod-deploy-password"
    }
  }
}
```

approved 执行时：

- 校验 `secretRef` 是否允许被 Jenkins 使用。
- 从后端安全凭证读取真实值。
- 注入 Jenkins `buildWithParameters`。
- 审计只记录 `secretRef`，不记录真实值。

---

## 12. 安全策略

### 12.1 凭证类型

建议复用安全凭证模块，新增或复用 provider：

| provider | 说明 |
|----------|------|
| `jenkins` | Jenkins username + API Token |
| `http_api` | 仅作为迁移兼容或兜底，不作为首选 |

已确认新增 `jenkins` provider，原因是需要 Jenkins 专属字段、测试连接、crumb 处理、File Parameter、artifact 下载和审计分类。

凭证字段：

- `accountName`: Jenkins username
- `secret`: Jenkins API Token
- `baseUrl`: Jenkins Base URL
- `approvalPolicy`: 写操作审批策略

首版不支持：

- Jenkins 密码登录。
- Jenkins Cookie 会话。
- SSO 表单登录。
- 2FA 交互式登录。
- iframe 或 WebView 内嵌 Jenkins 页面。

SSH 隧道只解决网络访问，不解决 Jenkins 登录方式。

HTTP proxy：

- v0.1 不提供 Jenkins 专属 HTTP proxy 配置。
- 内网访问优先使用已登记 SSH 隧道能力。
- 如 HTTP client 默认遵循系统代理，可沿用系统能力，但 Jenkins 连接不保存代理账号密码。
- 后续如需 HTTP proxy，作为 v0.2 连接增强，并通过安全凭证引用保存代理凭据，不保存明文。
- v0.1 不读取 `/computer/api/json`，不做 Agent / executor 看板。
- v0.1 只展示 queue item 中已返回的简短排队原因。

外部页面跳转：

- Job、Build、Console、Artifact 位置可提供“在 Jenkins 打开”按钮。
- 使用系统浏览器打开 Jenkins 原始 URL，不在 Tauri WebView 内嵌。
- v0.1 不识别 Blue Ocean 插件是否安装，不生成 Blue Ocean URL。
- 不向 Jenkins 注入 Cookie、session 或 Token。
- 打开行为写审计日志，但不作为写操作审批。

### 12.2 风险等级

| 操作 | 风险等级 | 处理 |
|------|----------|------|
| 连接测试 | readonly | 直接执行并审计 |
| 启用连接 | readonly/L2 | 测试成功后执行并审计；prod 或 MCP 写开启时弹确认 |
| 禁用连接 | readonly | 直接执行并审计 |
| Job 列表 | readonly | 直接执行并审计 |
| 构建列表 | readonly | 直接执行并审计 |
| 日志读取 | readonly | 直接执行，限制大小 |
| 触发 dev/test 构建 | L2 | 审批 |
| 触发 prod 构建 | L3 | 审批 |
| 停止构建 | L2 | 审批 |
| 删除构建 | L3 | 首版不做 |
| 修改 Job 配置 | L3 | 首版不做 |
| Script Console | blocked | 禁止 |

风险等级顺序固定为：

```text
readonly < L2 < L3 < blocked
```

比较规则：

- `blocked` 永远拒绝执行，不进入审批。
- approved 复验时，当前风险高于审批时风险则拒绝并要求重新创建审批。
- approved 复验时，当前风险等于或低于审批时风险，仍按审批时风险和审批结果执行，不自动降级。
- 实现必须使用统一风险排序函数，不允许各处自行按字符串比较。

### 12.3 Job 风险识别

默认风险为 L2；按连接环境、Job 名称、构建参数、File Parameter、并发状态自动升级。风险计算顺序：

1. blocked rule。
2. 连接 `environment`。
3. Job path / 参数规则。
4. File Parameter / 并发构建风险升级。
5. 未匹配 fallback。

基础规则：

- Job 名称包含 `prod` / `production` / `release` -> L3
- 参数中 `ENV=prod` -> L3
- 参数中包含 `DEPLOY=true` -> L3
- Job 名称包含 `dev` / `test` -> L2
- 连接 `environment=prod` -> 默认 L3
- File Parameter -> L3
- 同 Job 正在构建时再次触发 -> 默认 blocked
- 只读查询 -> readonly

风险规则按 Jenkins 连接配置，不使用全局规则作为首版主配置。连接级规则示例：

```json
[
  { "pattern": ".*prod.*", "risk": "L3" },
  { "parameter": "ENV", "value": "prod", "risk": "L3" },
  { "pattern": ".*test.*", "risk": "L2" },
  { "treatUnmatchedAsHighRisk": false },
  { "allowConcurrentBuilds": false },
  { "allowConcurrentPatterns": ["^dev-.*"] }
]
```

v0.1 风险规则 UI 使用表单化编辑，不允许用户直接自由编辑 JSON。支持的规则类型：

- Job 正则匹配：按 `jobFullName` 或显示名匹配并升级到 L2/L3 或 blocked。
- 参数匹配：按参数名、参数值或参数正则匹配并升级风险。
- 环境匹配：按连接 `environment` 或 `environmentLabel` 升级风险。
- File Parameter：命中文件参数时升级到 L3 或 blocked。
- 并发构建：配置是否允许同 Job 并发，以及允许并发的 Job 正则白名单。
- fallback：未匹配规则默认 L2，可配置为 L3 或 blocked。

底层仍保存为 `risk_rules_json`，便于后续导入导出和后端统一计算。UI 可以提供“查看 JSON”只读调试视图，但不提供自由 JSON 编辑入口，避免格式错误或字段拼写错误导致审批策略绕过。

同一个 Job 正在构建时，默认禁止再次触发，不进入审批。只有连接级规则显式允许并发时才允许继续创建审批；prod/release Job 即使允许并发，也必须 L3。

### 12.4 禁止能力

首版必须在 Service 层直接拒绝：

- `/script`
- `/scriptText`
- `/config.xml` POST
- `/pluginManager/*`
- `/restart`
- `/safeRestart`
- `/computer/*/scriptText`

### 12.5 SSH 隧道访问 Jenkins

首版需要支持通过已登记 SSH 服务器访问内网 Jenkins。

实现建议：

- Jenkins 连接可选 `sshServerAlias`。
- 选择 SSH 隧道时，后端通过本机临时端口转发访问 Jenkins。
- 隧道生命周期采用连接级短期复用：以 `connectionKey + sshServerAlias` 作为 key，空闲 5 分钟自动关闭。
- 隧道凭据复用现有服务器资产和安全凭证，不在 Jenkins 模块重复保存。
- 应用退出时关闭全部 Jenkins 隧道。
- 隧道创建失败时返回明确错误。
- 审计日志记录使用了哪个 SSH 服务器别名，但不记录隧道密钥。

### 12.6 MCP 构建触发策略

MCP 需要允许触发 Jenkins 构建，但必须同时满足：

1. Jenkins 连接启用 `allow_mcp_write`。
2. 工具调用使用 `jenkins_build_trigger_controlled` 创建审批。
3. `jenkins_build_trigger_approved` 执行时校验 requestHash。
4. 连接级风险规则判定风险等级。
5. AI 放行仅自动确认审批，不跳过审计，也不允许被 blocked 的能力执行。

只读 MCP 工具要求 `allow_mcp_read=true`。`allow_mcp_read` / `allow_mcp_write` 开关变更立即影响后续 MCP 调用。

approved 执行阶段必须重新读取连接状态和策略，并再次校验：

- 连接存在且未软删除。
- 连接 `enabled=true`。
- 连接状态不是 `credential_missing` / `credential_failed` / `disabled`。
- 相关安全凭证仍可用。
- `allow_mcp_write=true`。
- 使用当前 `risk_rules_json` 重新计算风险。
- 如果当前风险高于审批时风险，拒绝执行并要求重新创建审批。
- 如果当前风险低于审批时风险，不自动降级，仍按原审批风险执行。
- requestHash 仍匹配 controlled 阶段 payload。

如果连接被禁用、凭证失效或 `allow_mcp_write=false`，则拒绝执行，审批记录标记为执行失败或策略拒绝，并写审计。审批创建成功不代表未来一定可执行。

### 12.7 TLS 校验策略

- `tls_verify` 默认 true。
- 如内网 Jenkins 使用自签名证书，用户可在连接配置中显式关闭。
- 关闭 TLS 校验时：
  - 连接列表显示“TLS 未校验”风险标记。
  - 审计日志记录 `tlsVerify=false`。
  - 所有写操作风险至少 L2。
  - `environment=prod` 且 `tls_verify=false` 时，写操作风险提升到 L3。

### 12.8 敏感参数与日志脱敏

构建参数命中以下关键词时视为敏感：

- `password`
- `passwd`
- `token`
- `secret`
- `key`
- `credential`
- `cookie`
- `auth`

日志脱敏还需要识别通用凭证模式：

- `Authorization: Bearer ...`
- `Authorization: Basic ...`
- AWS Access Key 形态。
- GitHub / GitLab Token 常见前缀。
- URL query 中的 `token=...`、`password=...`、`secret=...`、`auth=...`。

处理规则：

- 审批摘要、审计日志、MCP 响应、桌面通知中敏感参数值显示为 `***`。
- UI 表单中敏感参数使用 Password 输入。
- MCP 不允许传明文敏感参数，只允许 `secretRef` 或已授权的 `useLastSuccessfulValue`。
- `useLastSuccessfulValue` 需要绑定 `connectionKey + jobFullName + parameterName + requester` 校验。
- 管理员开启共享最近成功值后，可跨 requester 复用，但必须记录审计并将构建触发风险提升到 L3。
- 连接关闭 `parameter_prefill_enabled` 后，不再返回最近参数回填候选；历史构建的脱敏参数摘要仍保留。
- 日志默认以 safe 模式展示，尽力脱敏。
- UI 可提供“显示原始日志”，但必须人工确认，且不持久化。
- 原始日志确认只对当前构建详情 UI 会话有效，默认 10 分钟。
- 切换构建、关闭构建详情 Drawer、连接关键字段变化或确认超时后，原始日志确认立即失效。
- 原始日志确认状态不写数据库，不影响 MCP/AI。
- 用户复制原始日志内容时必须记录 `rawLogAccess=true` 和确认来源。
- MCP/AI 永远只返回脱敏日志。
- MCP 不提供原始日志，也不提供“复制日志”动作。
- MCP 日志读取默认最多返回尾部 200KB，最大 1MB；UI 可以分块加载更多内容，但仍需要避免一次性读取超大日志。
- 脱敏统一替换为 `***`，保留必要上下文用于排障。
- 脱敏规则以避免漏脱敏为优先，允许少量误伤。
- 审计日志只记录日志读取范围、大小和构建标识，不保存日志正文。
- 不新增日志正文持久化表；Jenkins 仍是控制台日志真源。
- AI 分析记录只能引用脱敏日志片段的摘要和来源范围，不能保存原始日志内容。

---

## 13. 审批与审计

Jenkins 模块不单独定义审批和审计保留期：

- Jenkins 审批请求进入统一审批队列，复用现有审批保留策略。
- Jenkins 审计进入统一审计日志，复用现有审计保留策略。
- Artifact 文件清理独立处理，不影响审批记录和审计记录。
- Artifact 本地文件清理不进入写审批，但必须人工确认并写审计；只删除应用托管目录内文件，不删除 artifact record。
- 后续如需 Jenkins 专属清理，需要作为全局数据保留策略的一部分设计。

审批展示和持久化规则：

- requestHash 计算使用 controlled 阶段固化的受控 payload。
- requestHash 固化 `connectionKey`、`connectionConfigVersion`、`jobFullName`、`parameterDefinitionHash`、参数摘要、敏感参数引用、File Parameter 受控元数据、`requester`、`reason`、`riskLevel`、`riskFlags` 和 `createdAtBucket`。
- approved 执行时必须复验 requestHash；Job、参数定义、参数、风险、请求人、理由、文件 sha256 或敏感参数引用变化时拒绝执行。
- approved 执行前必须重新读取参数定义并比对 `parameterDefinitionHash`；不一致时拒绝执行，错误码为 `parameter_definition_changed_after_approval`。
- approved 执行时还必须复验当前连接状态、凭证状态和 MCP 写权限；连接禁用、凭证失效或 `allow_mcp_write=false` 时拒绝执行并写审计。
- approved 执行时必须使用当前 `risk_rules_json` 重新计算风险；当前风险高于审批时风险则拒绝执行并要求重新创建审批，当前风险降低也不自动降级执行。
- `baseUrl`、`credentialKey`、`sshServerAlias`、`tlsVerify` 在审批创建后发生变化时，`config_version` 自增；pending 审批不主动批量改状态，但 approved 执行必须对比 `connectionConfigVersion` 并拒绝不一致的请求，错误码为 `connection_changed_after_approval`。
- 审批详情应提示“连接配置已变更，请重新创建审批”。
- 审批摘要、审计详情和本地构建记录只展示或保存脱敏参数摘要。
- approved 执行时再从安全凭证或受控文件路径解析真实值。
- 只要进入 approved 执行阶段，无论成功或失败，都必须写一条执行审计。
- 执行审计需要区分失败阶段：`policy_check`、`request_hash_check`、`credential_resolve`、`file_recheck`、`crumb_fetch`、`jenkins_http`、`queue_track`、`build_track`。
- 执行审计需要记录是否已触达 Jenkins，便于区分本地策略拒绝和远端 API 失败。

### 13.1 审批动作

建议 action 命名：

| action | 说明 |
|--------|------|
| `jenkins_build_trigger` | 触发构建 |
| `jenkins_build_stop` | 停止构建 |
| `jenkins_replay_build` | 重放构建，v0.3 可选 |

### 13.2 审批摘要

触发构建审批摘要建议：

```text
触发 Jenkins 构建：prod-jenkins / folder/service-deploy
参数：BRANCH=master, ENV=prod
```

### 13.3 审计字段

通用审计日志：

| 字段 | 示例 |
|------|------|
| request_id | jenkins-20260704-abcdef |
| actor | local-user / mcp-client / codex |
| source | jenkins |
| server_alias | prod-jenkins |
| action | jenkins_build_trigger |
| risk | L2 / L3 |
| result | success / failed / approval_required / blocked |
| summary | 触发 Jenkins 构建 |
| approval_id | 审批 ID |
| execution_stage | policy_check / jenkins_http / queue_track |
| error_code | request_hash_mismatch / jenkins_403 / crumb_failed |
| touched_jenkins | true / false |
| detail_json | 脱敏详情 |

`detail_json` 不允许包含 Token、Cookie、crumb。

approved 执行审计规则：

- 审批记录表达“是否允许执行”，执行审计表达“实际执行发生了什么”。
- 只要审批通过后进入 approved 执行阶段，都必须写执行审计。
- 成功时记录 `requestId`、`approvalId`、`runKey`、queueId/buildNumber、Jenkins HTTP 结果摘要。
- 失败时记录 `requestId`、`approvalId`、失败阶段、错误码、脱敏错误摘要和 `touchedJenkins`。
- requestHash 不匹配、连接关键字段审批后变更、连接禁用、凭证失效、`allow_mcp_write=false`、File Parameter 复验失败属于 `touchedJenkins=false`。
- crumb 获取失败、Jenkins 返回 403/404/500、网络超时属于按实际情况记录 `touchedJenkins`。
- 执行审计不保存 Token、Cookie、crumb、Authorization header、明文敏感参数、文件内容或内部错误堆栈。

UI 只读轻量审计规则：

- UI 发起真实 Jenkins 请求时写轻量审计，包括连接测试、Job 列表刷新、构建列表、构建详情、日志读取、队列读取和 artifact 下载。
- 普通页面切换、展开已缓存节点、不触发 Jenkins 请求的本地筛选搜索不写审计。
- 日志 progressive 读取按一次日志会话合并审计，记录起止 offset、读取总字节、是否截断、是否脱敏、结果状态和耗时，不按每次轮询写一条审计。
- progressive 日志会话按 `requestId + connectionKey + jobFullName + buildNumber + viewer` 合并。
- 会话审计字段包括 `startOffset`、`endOffset`、`totalBytes`、`truncated`、`redacted`、`durationMs`、`pollCount` 和结束原因。
- 复制选中日志写轻量审计，字段包括 `connectionKey`、`jobFullName`、`buildNumber`、`startOffset`、`endOffset`、`bytes`、`redacted`、`rawLogAccess` 和确认来源。
- 日志复制审计不保存复制内容。
- UI 只读审计不保存完整响应、Job 树完整内容、构建列表完整内容、日志正文、参数明文、artifact 文件内容或 Jenkins 原始错误堆栈。

`request_id` / `correlationId` 规则：

- 每次 Jenkins UI 或 MCP 调用都生成 `requestId`。
- `requestId` 贯穿审批创建、approved 执行、Jenkins HTTP 请求、queue 跟踪、build run、artifact 下载和审计日志。
- `approvalId`、`runKey`、`artifactKey` 是业务对象关联；`requestId` 是一次调用链路排障标识。
- 用户可在审批详情、构建运行详情、artifact 详情和审计详情中看到 `requestId`。
- 错误响应返回 `requestId`，但不返回内部错误堆栈或敏感请求上下文。

---

## 14. Jenkins HTTP 封装设计

### 14.1 URL 编码

Jenkins Folder Job URL 不是简单拼接：

```text
folder/service
=> /job/folder/job/service/
```

需要实现：

```rust
fn job_path_to_url(job_full_name: &str) -> String {
    job_full_name
        .split('/')
        .map(urlencode)
        .map(|part| format!("job/{}", part))
        .collect::<Vec<_>>()
        .join("/")
}
```

### 14.2 请求封装

建议 `JenkinsClient` 内部封装：

- `get_json<T>(path, query)`
- `post_empty(path)`
- `post_form(path, form)`
- `get_text(path, query)`
- `get_crumb_if_needed()`
- `build_auth_headers()`
- `sanitize_error()`

### 14.3 超时策略

| 请求类型 | 超时 |
|----------|------|
| 连接测试 | 10s |
| Job 列表 | 20s |
| 构建详情 | 10s |
| 日志读取 | 30s |
| 触发构建 | 20s |
| 队列轮询 | 单次 5s |

### 14.4 错误归一化

| HTTP 状态 | 用户提示 |
|-----------|----------|
| 401 | Jenkins 凭证无效或已过期 |
| 403 | Jenkins 权限不足或 CSRF crumb 缺失 |
| 404 | Jenkins 连接、Job 或构建不存在；Job 可能已改名、移动或删除 |
|  crumb error | Jenkins CSRF 校验失败，请检查 API Token 或 crumb 配置 |
| timeout | Jenkins 请求超时 |

---

## 15. 构建跟踪器设计

触发构建后的队列轮询和构建状态跟踪由 Rust 后端负责，前端只展示状态或订阅更新。

### 15.0 连接级并发限制

- 每个 Jenkins 连接最多同时执行 4 个只读请求。
- 每个 Jenkins 连接同时最多执行 1 个 approved 写操作。
- 日志 progressive 轮询按 `connectionKey + jobFullName + buildNumber` 保持单通道，避免多个 Tab 重复请求同一日志接口。
- 超出并发限制时进入本地等待队列。
- 等待超过 30 秒返回忙碌提示。
- UI 和 MCP 共享同一限流器，MCP 不能绕过 UI 限制。

### 15.1 JenkinsBuildTracker

职责：

- `execute_jenkins_build_trigger_approved` 触发成功后创建 `jenkins_build_runs`。
- 后台轮询 queue item，直到拿到 `executable.number`。
- 拿到 build number 后低频轮询构建状态。
- 构建完成后更新本地 run 记录。
- 按连接通知策略发送桌面通知。
- 状态变化时通过 Tauri event 推送本地 run 摘要给前端。
- 页面关闭不影响跟踪；应用退出则停止跟踪，下次打开可从 Jenkins 重新同步。
- 应用启动时只恢复本应用触发或用户手动跟踪的未完成 run，不扫描 Jenkins 全量历史。

建议策略：

| 阶段 | 轮询间隔 | 超时 |
|------|----------|------|
| queue -> build number | 2s | 10 分钟 |
| building -> completed | 5s / 10s | 默认 2 小时，可按连接配置上限 |

超时语义：

- Queue 超时后，本地 run 记录标记为 `queue_timeout`，停止盲目轮询。
- Queue 超时返回本地 `queue_timeout` 状态。
- Build 跟踪超时后，本地 run 记录标记为 `tracking_timeout`，但不停止 Jenkins 构建。
- 启动恢复同步失败时，本地 run 记录标记为 `sync_failed`。
- `queue_timeout`、`tracking_timeout` 和 `sync_failed` 都不是 Jenkins 实际构建结果。
- 用户打开构建详情时，可以重新同步 Jenkins 真实状态。
- MCP 返回必须区分本地跟踪超时和 Jenkins 的 `FAILURE` / `ABORTED` / `UNSTABLE` 等真实结果。
- MCP 返回 run/build 状态时必须包含 `statusSource: "jenkins" | "local"`。

跟踪范围：

- 默认只跟踪 Tauri SSH 触发的构建。
- 用户可手动点击“跟踪此构建”加入跟踪。
- 不默认全量监控所有 Jenkins Job。
- 收藏 Job 定时刷新作为后续增强，不进入首版默认行为。

启动恢复策略：

- 应用启动时只恢复 `jenkins_build_runs` 中状态为 `queued`、`building`、`tracking_timeout` 的 run。
- 恢复范围仅限本应用触发或用户手动跟踪的 run。
- 按 `connectionKey + jobFullName + queueId/buildNumber` 尝试同步一次 Jenkins 真实状态。
- 如果连接已禁用、软删除、凭证缺失或凭证失效，不恢复跟踪，只显示本地最后状态和手动刷新入口。
- 恢复失败不做无限后台重试，状态标记为 `sync_failed`，并记录脱敏错误摘要。
- 用户手动刷新构建详情时，可以再次触发单次同步。

前端更新机制：

- 后端 tracker 负责轮询 Jenkins，前端不直接轮询 Jenkins queue/build 接口。
- 前端订阅 Tauri event：`jenkins://build-run-updated`。
- 事件 payload 只包含本地状态摘要：
  - `requestId`
  - `runKey`
  - `connectionKey`
  - `jobFullName`
  - `queueId`
  - `buildNumber`
  - `status`
  - `statusSource`
  - `result`
  - `updatedAt`
- 事件不推送日志正文、构建参数明文、artifact 本地路径、Token、crumb 或 Jenkins 原始错误堆栈。
- 页面保留手动刷新按钮，用于重新同步 Jenkins 真实状态。
- 多个页面打开同一构建时复用后端 tracker 和本地事件，不重复打 Jenkins。

---

## 16. 前端 API 设计

`src/lib/api/jenkins.ts`：

```typescript
export const jenkinsApi = {
  listConnections: () => invoke<JenkinsConnection[]>("list_jenkins_connections"),
  upsertConnection: (input: UpsertJenkinsConnectionInput) =>
    invoke<JenkinsConnection>("upsert_jenkins_connection", { input }),
  deleteConnection: (connectionKey: string) =>
    invoke<void>("delete_jenkins_connection", { connectionKey }),
  testConnection: (connectionKey: string) =>
    invoke<JenkinsConnectionTestResult>("test_jenkins_connection", { connectionKey }),
  listJobs: (input: ListJenkinsJobsInput) =>
    invoke<JenkinsJob[]>("list_jenkins_jobs", { input }),
  listBuilds: (input: ListJenkinsBuildsInput) =>
    invoke<JenkinsBuild[]>("list_jenkins_builds", { input }),
  getBuildDetail: (input: GetJenkinsBuildInput) =>
    invoke<JenkinsBuildDetail>("get_jenkins_build_detail", { input }),
  readBuildLog: (input: ReadJenkinsBuildLogInput) =>
    invoke<JenkinsBuildLogChunk>("read_jenkins_build_log", { input }),
  createTriggerApproval: (input: TriggerJenkinsBuildInput) =>
    invoke<ApprovalRequest>("create_jenkins_build_trigger_approval", { input }),
  executeTriggerApproved: (input: ExecuteJenkinsBuildApprovedInput) =>
    invoke<JenkinsBuildTriggerResult>("execute_jenkins_build_trigger_approved", { input }),
};
```

---

## 17. 页面交互流程

### 17.1 新增连接

```text
点击新增连接
  -> 填写名称 / URL / 凭证引用 / MCP 权限
  -> 保存连接
  -> 测试连接
  -> 成功后显示 Jenkins 版本和当前用户摘要
```

### 17.2 查看构建日志

```text
选择连接
  -> 选择 Job
    -> 点击构建记录
      -> 打开 Drawer
        -> Tab: 日志
          -> 首次读取尾部日志
          -> 如果 building=true，启动 progressive 轮询
          -> 构建完成后停止轮询
```

### 17.3 触发参数构建

```text
选择 Job
  -> 点击触发构建
    -> 读取参数定义
    -> 动态生成 Form
    -> 用户填写参数
    -> 点击提交
    -> 创建审批请求
    -> 审批通过
    -> 执行 buildWithParameters
    -> 返回 queueId
    -> 轮询 queue item
    -> 获取 buildNumber
    -> 打开构建详情
```

---

## 18. 与现有模块关系

### 18.1 安全凭证

Jenkins 连接不保存密钥，统一引用安全凭证。

### 18.2 审批队列

所有 Jenkins 写操作复用 `approval_requests`。

### 18.3 审计日志

Jenkins 所有调用写入通用 `audit_logs`，安全凭证访问写入 `secure_credential_audit_logs`。
日志读取审计只保存目标构建、读取范围、返回大小、是否脱敏、是否截断和请求来源，不保存日志正文。

### 18.4 MCP Server

Jenkins MCP 工具加入 MCP 工具清单，并显示策略说明。

### 18.5 自动部署

后续可将 Jenkins 构建产物作为部署输入：

```text
Jenkins Build Success
  -> 获取 artifact
  -> 创建 Deployment Dry Run
  -> 审批
  -> 自动部署
```

v0.1 只提示下一步操作，不自动触发新的 Jenkins 写操作。后续如需定时构建、失败重试或构建后联动触发，需要进入自动化 / Runbook 模块并单独建模审批策略。

### 18.6 Git 工作区

后续可从本地 Git 工作区传入：

- branch
- commit hash
- tag
- release version

作为 Jenkins 构建参数。

v0.1 不自动读取当前 Git 工作区分支、commit 或 tag 写入 Jenkins 参数。v0.2 如果从 Git 工作区页面发起 Jenkins 构建，必须显式带入 branch/commit，并在审批摘要中展示。MCP 必须显式传入 branch/commit 参数，不允许从上下文猜测。

---

## 19. 实施计划

### 阶段 1：只读 MVP

- [x] 定义 Jenkins models。
- [x] 新增 `jenkins_connections` 表。
- [x] 新增 `jenkins_recent_jobs`、`jenkins_build_runs`、`jenkins_recent_parameter_values`、`jenkins_artifacts` 表。
- [x] 实现连接 CRUD。
- [x] 实现 `config_version` 和连接关键字段变更失效逻辑。
- [x] 实现软删除连接恢复。
- [x] 实现连接配置复制。
- [x] 实现 Jenkins HTTP client。
- [x] 实现 Jenkins username + API Token 认证。
- [x] 实现连接测试。
- [x] 实现 Jenkins 连接通过 SSH 隧道访问。
- [x] 实现 TLS 校验开关和风险提示。
- [x] 实现 Job 列表读取。
- [x] 实现构建列表读取。
- [x] 实现构建详情读取。
- [x] 实现日志读取。
- [x] 实现 progressive log、日志会话合并审计和日志复制审计。
- [x] 实现 artifacts 列表和下载。
- [x] 实现 artifact 本地文件清理。
- [x] 实现 UI/MCP 只读轻量审计。
- [x] 实现构建完成桌面通知。
- [x] 实现 `allow_mcp_read` 只读 MCP 开关。
- [x] 新增 Jenkins 页面和路由。
- [x] 新增 MCP 只读工具。
- [x] 添加 Rust 单元测试。

验收标准：

- 能添加 Jenkins 连接。
- 能测试连接并显示 Jenkins 版本。
- 能读取 Job 列表。
- 能查看构建列表。
- 能打开构建详情和日志。
- 正在构建的日志可增量刷新，progressive 审计按会话合并。
- 能通过 SSH 隧道访问内网 Jenkins。
- 能下载 artifact。
- 能清理单个 artifact 本地文件，记录保留且写审计。
- 构建状态变化可发送桌面通知。
- MCP 可只读查询，不返回凭证明文。
- UI/MCP 只读操作写轻量审计，不记录完整响应或日志正文。

实现说明：

- 当前 SSH 隧道采用 Jenkins 请求级临时本机端口转发：每次请求通过已登记 SSH 服务器建立 `127.0.0.1` 临时端口，请求结束后自动关闭。
- 隧道凭据复用服务器资产，不在 Jenkins 模块保存或返回 SSH 密钥、服务器密码、Jenkins API Token 明文。
- HTTPS Jenkins 通过本机隧道访问时，如连接开启 TLS 校验会因证书主机名与 `127.0.0.1` 不匹配而拒绝；首版要求关闭该 Jenkins 连接的 TLS 校验，或使用 HTTP 内网地址。
- 规划中的连接级 5 分钟空闲复用池保留为后续性能优化项，不影响阶段 1 只读能力验收。

### 阶段 2：受控构建

- [x] 实现参数定义解析。
- [x] 实现参数定义短 TTL 缓存和 `parameterDefinitionHash` 复验。
- [x] 实现标准参数 Form。
- [x] 实现动态插件参数降级提示。
- [x] 实现 File Parameter 上传。
- [x] 实现敏感参数识别和 `secretRef` 注入。
- [x] 实现连接级风险规则、表单化规则编辑器和默认并发阻断。
- [x] 实现构建审批创建。
- [x] 实现 requestHash 固化 `connectionConfigVersion`、`parameterDefinitionHash`、风险和文件元数据。
- [x] 实现审批后触发普通构建。
- [x] 实现审批后触发参数构建。
- [x] 实现 queue item 轮询。
- [x] 实现后端 JenkinsBuildTracker。
- [x] 实现构建状态 Tauri event 推送。
- [x] 实现应用启动后的未完成 run 恢复同步。
- [x] 实现构建运行记录。
- [x] 实现最近成功参数值记录和“忘记最近值”。
- [x] 新增 MCP controlled/approved 工具。
- [x] 允许 MCP 在连接级 `allow_mcp_write` 开启后触发构建。
- [x] 接入 AI 放行自动确认逻辑。
- [x] 添加构建触发测试。

验收标准：

- 触发构建必须先进入审批。
- AI 放行打开时可自动确认，但保留审批记录。
- approved 执行时 requestHash 不匹配会拒绝。
- approved 执行时如果参数定义在审批后变化，必须以 `parameter_definition_changed_after_approval` 拒绝执行并写审计。
- approved 执行时如果当前风险规则计算出的风险高于审批时风险，必须拒绝并要求重新创建审批；风险降低不自动降级执行。
- approved 执行时如果连接关键字段在审批后变更，必须以 `connection_changed_after_approval` 拒绝执行并写审计。
- approved 执行时如果连接已禁用、凭证失效或 `allow_mcp_write=false`，必须拒绝执行并写审计。
- approved 执行无论成功或失败都必须产生执行审计，并标明失败阶段和是否触达 Jenkins。
- 构建触发成功后可看到 queueId 和 buildNumber。
- 前端通过 Tauri event 收到 queue/build 状态摘要更新，且事件不包含日志正文或敏感参数。
- 应用重启后只恢复本应用触发或手动跟踪的未完成 run；连接禁用或凭证失效时不后台重试，只保留本地最后状态。
- MCP 可触发构建，但必须经过 controlled/approved 链路。
- 同 Job 正在构建时默认阻断再次触发。
- 风险规则只能通过表单编辑，JSON 视图只读，保存后后端按 `risk_rules_json` 计算风险。
- 敏感参数不进入审批摘要、审计日志和 MCP 响应。

实现说明：

- 已新增 Jenkins 参数定义只读解析接口，支持从 Job API `property.parameterDefinitions` 解析 string、boolean、choice、password、file 参数。
- Active Choices、Extended Choice、Git Parameter 等动态参数会标记 `dynamicParameter=true` 并按字符串输入降级，未知类型标记 `unsupported=true`。
- 参数名命中 password、token、secret、credential、api_key 等敏感特征时标记 `sensitive=true`，后续审批摘要和审计可直接复用该标记做脱敏。
- 参数定义短 TTL 缓存和 `parameterDefinitionHash` 复验已作为独立步骤完成。
- 参数定义接口已返回 `parameterDefinitionHash`、`fromCache`、`cachedAt`、`expiresAt` 和 `ttlSeconds`，默认 TTL 为 60 秒；`refresh=true` 会绕过缓存重新读取 Jenkins。
- 新增 `verify_jenkins_parameter_definition_hash` / `/dev-api/jenkins/parameters/verify-hash` 复验入口，Hash 不一致时返回 `parameter_definition_changed_after_approval`，供后续 approved 执行链路复用。
- 缓存 key 包含 `connectionKey + configVersion + jobFullName`，连接关键字段变更导致 `configVersion` 变化后不会复用旧参数定义。
- Jenkins 页面 Job 表新增“参数”入口，可读取参数定义并打开构建参数抽屉；标准参数 Form 已支持 string、boolean、choice、password/sensitive 参数录入。
- 新增 `execute_jenkins_build_trigger_approved` / `/dev-api/jenkins/builds/trigger-approved`，只执行已 approved 且 requestHash 复验通过的普通 `/build` 触发。
- approved 执行会复验 `connectionConfigVersion`、`parameterDefinitionHash`、当前风险等级和 `allow_mcp_write`；连接变更、参数定义变更、风险升高或连接未允许写入时拒绝执行并写执行审计。
- 普通构建和参数化构建触发成功后返回 Jenkins queue `Location`、可解析的 `queueId`、本地 `runKey`，并在单次 queue 同步拿到 Jenkins executable 时直接带出 `buildNumber` 和当前 run 状态。
- approved 执行已支持标准参数和敏感参数触发 `buildWithParameters`；标准参数按字符串/数字/布尔值提交，敏感参数只接受 `secretRef` 并在 Rust 后端通过安全凭证服务短暂解析，不进入审批 payload、审计 detail 或 MCP 响应。
- File Parameter 已接入受控文件引用：创建审批时把本地文件复制到应用托管临时引用，只在 payload 保存 `localPathRef`、文件名、大小和 sha256，不保存原始绝对路径；approved 执行前复验 sha256/size，成功后通过 Jenkins multipart `buildWithParameters` 上传。
- queue item 轮询已接入 Jenkins `/queue/api/json` 和 `/queue/item/{queueId}/api/json`，返回 `waiting/blocked/stuck/cancelled/executable` 状态、说明、queueId、Job 名称以及可用时的 `buildNumber` / executable URL。
- 新增 `poll_jenkins_queue_item` Command、`/dev-api/jenkins/queue/item` 和 MCP 工具 `jenkins_queue_item_poll`；现有队列 tab 的 `list_jenkins_queue` 已从本地占位改为真实 Jenkins 队列读取，并写轻量只读审计。
- 后端 `JenkinsBuildTracker` 已接入 approved 触发成功链路：触发成功后写入 `jenkins_build_runs` queued run，并可基于 queue item 单次推进本地 run 状态；当 queue item 出现 `executable.number` 时会尝试同步 Jenkins build detail，失败时保留本地 `building` 状态等待后续同步。
- 当前 tracker 完成的是后端持久化和单次同步核心；Tauri event 推送、启动恢复后台调度、长轮询间隔/超时控制仍按后续阶段 2 清单继续实现。
- MCP 构建触发采用 `jenkins_build_trigger_controlled` / `jenkins_build_trigger_approved` 双工具：controlled 仅创建审批且要求连接开启 `allow_mcp_write`，approved 执行前复验 requestHash、连接状态、凭证状态、`allow_mcp_write`、参数定义和风险；AI 放行通过统一审批服务自动确认，仍保留审批与审计记录。
- 构建状态 Tauri event 已接入，事件名为 `jenkins-build-status`；approved 触发成功写入/同步 run 后会推送状态摘要，读取构建详情时也会推送 Jenkins 状态摘要。
- `jenkins-build-status` payload 仅包含 `runKey`、`requestId`、`connectionKey`、`jobFullName`、`queueId`、`buildNumber`、`status`、`statusSource`、`result`、`updatedAt`，不包含日志正文、参数值、Token、crumb 或文件内容；Jenkins 页面在 Tauri 运行时监听该事件并刷新当前连接的构建列表。浏览器 dev-api 预览环境不会接收 Tauri event。
- 应用启动后会异步执行一次 Jenkins 未完成 run 恢复同步：从 `jenkins_build_runs` 查询本应用记录的 `queued`、`triggered`、`building`、`tracking_timeout` 等未完成状态，已有 `buildNumber` 时读取构建详情，没有 `buildNumber` 但有 `queueId` 时轮询 queue item。
- 启动恢复不会触发新的 Jenkins 构建，不读取控制台日志正文，不返回或写入敏感参数值；同步成功后复用 `jenkins-build-status` 事件推送状态摘要。
- 如果连接不存在、连接禁用、凭证不可用或 Jenkins 读取失败，恢复任务只把该 run 标记为 `sync_failed` 并记录脱敏错误摘要，不做后台反复重试；用户后续可在恢复连接后通过详情刷新或后续受控同步路径继续推进。
- 构建运行记录已接入本地 `jenkins_build_runs`：Job 行新增“构建”操作，用户手动同步该 Job 最近构建时会读取 Jenkins 远端构建摘要并写入本地运行记录表，随后切换到“构建”Tab 展示。
- 构建详情读取也会回写本地运行记录；如果本地已有同连接、同 Job、同 buildNumber 的受控触发 run，则复用原 `runKey`、`requestId`、`queueId` 和创建人，只更新 Jenkins 状态/结果，避免同一次构建生成重复记录。
- 构建运行记录同步仍受“不镜像 Jenkins 全量历史”约束：只在用户手动读取最近构建或打开详情时写入有限记录，不后台全量抓取，不保存日志正文和敏感参数。
- 最近参数值已接入 `jenkins_recent_parameter_values`：approved 构建触发被 Jenkins 接受并写入 run 后，会保存本次提交的非敏感标量参数和敏感参数的 `secretRef` 引用；File Parameter、unsupported 参数、缺失 `secretRef` 的敏感参数和复杂对象不会进入最近值表。
- 参数抽屉读取参数定义时会按 `connectionKey + jobFullName + requester` 读取最近值并用于回填；连接关闭 `parameter_prefill_enabled` 时服务端返回空候选但不删除历史记录。
- 参数标签旁会显示“最近值”和“忘记”入口；忘记操作只删除当前 requester 的单个参数最近值，不走审批但写 `jenkins.parameters.recent.forget` 审计，不暴露为 MCP 工具。
- 当前“成功”边界按 v0.1 受控触发链路定义为 Jenkins 接受 approved 触发请求；后续如果引入持续完成态跟踪，可再细化为仅 Jenkins 构建结果 `success` 后回写。
- 参数抽屉展示 `parameterDefinitionHash`、缓存来源和过期时间，并可复制脱敏参数摘要；当前不触发构建、不创建审批。
- 动态插件参数会在参数抽屉顶部显示降级提示，列出参数名，并改为手动输入；摘要保留 `dynamicParameter=true`，不执行 Jenkins 页面脚本联动。
- File Parameter 已完成受控构建前置采集：Tauri 运行时支持通过系统文件选择器选择本地文件，浏览器预览环境支持手动填写绝对路径。
- 后端新增 File Parameter 元数据检查入口，只读取并返回 basename、size、sha256 和 modified_at；参数摘要只保存受控元数据，不包含完整本地路径。
- File Parameter 已完成审批前元数据固化、approved 阶段文件复验和 multipart 触发执行；文件参数仍不会进入最近参数值复用表。
- 敏感参数识别和 `secretRef` 注入已完成表单侧前置能力：参数名和 password 类型命中敏感规则后，构建参数抽屉不再录入明文密码值，而是录入安全凭证引用。
- 脱敏参数摘要对敏感参数输出 `{ valueKind: "secret_ref", secretRef }`；缺失引用时输出 missing 标记，避免后续审批摘要、审计或 MCP 响应携带敏感明文。
- `secretRef` 已完成受控构建参数摘要注入和 approved 执行解析；执行时只在 Rust 后端短暂解密并注入 Jenkins `buildWithParameters`，不写入审批摘要、审计日志或 MCP 响应。
- 连接级风险规则已改为表单化编辑，支持未匹配默认风险、环境风险、File Parameter 风险、同 Job 并发开关、并发白名单、Job 正则风险规则和参数风险规则；底层仍保存规范化 `risk_rules_json`。
- 风险规则 JSON 仅作为只读预览展示，不再提供自由 JSON 编辑入口；保存连接时后端会校验规则版本、风险等级和正则格式。
- 默认并发阻断已落到规则模型：`allowConcurrentBuilds=false` 时同 Job 再次触发应直接 blocked；只有显式开启并发且 Job 命中白名单正则时才允许后续创建审批。
- 风险规则已完整接入构建审批创建和 approved 复验：后端按 `risk_rules_json` 综合 fallback、环境、Job 正则、参数规则、File Parameter、动态/不支持参数和同 Job 未完成 run 计算风险；同 Job 未完成 run 默认 blocked，只有显式开启并发且命中白名单正则才允许继续创建审批。
- 构建审批创建已接入通用 `approval_requests` 队列，新增 `create_jenkins_build_trigger_approval` Command 和 `/dev-api/jenkins/builds/trigger-approval`，审批来源为 `jenkins`，动作为 `jenkins_build_trigger`。
- 参数抽屉新增“创建审批”入口和审批理由输入；创建审批只写本地审批队列，不触发 Jenkins。
- 审批创建会校验连接启用状态、连接审批策略、`parameterDefinitionHash` 和审批理由；审批 payload 保存连接配置版本、Job、参数定义 Hash、脱敏参数摘要、请求方、理由和风险等级。
- 审批 payload 会移除 File Parameter 的 `localPath`，敏感参数明文会被 `[REDACTED]` 替换；`secretRef` 引用保留给后续 approved 执行解析。
- 本轮同时修复 `jenkins_connections` 插入 SQL 占位符数量错误，避免新建 Jenkins 连接时 SQLite 报 `26 values for 27 columns`。
- `requestHash` 固化已接入构建审批创建：controlled payload 先固化 `action`、`connectionKey`、`connectionConfigVersion`、`jobFullName`、`parameterDefinitionHash`、脱敏参数摘要、请求方、理由、`riskLevel`、`riskFlags` 和 `createdAtBucket`，再计算 SHA-256。
- 审批记录的 `command` 字段保存 `requestHash`，`payloadJson.requestHash` 保存同一值；approved 执行阶段可用该 hash 复验 Job、参数定义、风险、文件元数据和敏感参数引用是否被篡改。
- File Parameter 的受控元数据参与 hash，但本地绝对路径和文件内容不进入 hash payload；敏感明文会先脱敏再参与 hash，避免审批队列和审计链路保存秘密。
- 2026-07-05 审查修正：补齐 File Parameter approved multipart 上传、风险规则/同 Job 并发阻断审批接入、Job 收藏、MCP `depth/offset/cursor/tailBytes` schema 透传，以及连接启用/恢复硬约束；新增 `jenkins_recent_jobs.favorite` 迁移和 Job 收藏审计。
- 2026-07-05 二次审查修正：补齐 Jenkins crumb 内存缓存和 403 重试、Queue 10 分钟 `queue_timeout`、`jenkins_job_detail` Command/MCP、MCP `forceRefresh` 别名、MCP 日志默认 200KB 尾部限制、连接默认 View/Folder 应用，以及 UI/MCP 过期文案。

### 阶段 3：v0.2 排障增强

- [x] 实现日志搜索和高亮增强。
- [x] v0.2 实现失败日志片段提取。
- [x] v0.2 实现 AI 构建失败总结。
- [x] v0.2 实现停止构建审批。
- [x] v0.2 实现构建参数模板。

验收标准：

- 构建失败时可一键提取错误摘要。
- AI 总结只能使用脱敏日志片段。
- 总结结果只保存为本地 build analysis record，不写回 Jenkins。
- v0.2 停止构建走审批；生产环境或 release/prod Job 升 L3。
- 参数模板敏感值只能保存 `secretRef`，触发仍走审批。

实现说明：

- 构建日志抽屉已支持在已加载日志片段内搜索，显示搜索命中数；日志面板会高亮搜索词和 `ERROR`、`FAILURE`、`Exception`、`Traceback`、`BUILD FAILED`、`npm ERR!`、`MavenCompilationFailureException` 等错误关键字。搜索和高亮只作用于前端已加载的脱敏日志文本，不新增后端日志读取范围，也不保存日志正文。
- 构建日志抽屉已支持“一键提取失败片段”：仅从当前已加载的脱敏日志文本中查找错误关键字命中行，并提取前后少量上下文生成本地临时摘要；不触发新的 Jenkins 日志读取，不落库，不写回 Jenkins，也不把原始日志正文传给 AI。
- AI 构建失败总结已接入本地 `jenkins_build_analyses` 记录：前端只把当前已加载且已脱敏的失败片段发送给后端 AI 分析；数据库仅保存 AI 总结、provider/model、片段 SHA-256、来源行号范围和命中行数，不保存日志正文，不写回 Jenkins。
- 停止构建已接入受控审批链路：新增 `create_jenkins_build_stop_approval` / `execute_jenkins_build_stop_approved` Command、dev-api 和 MCP `jenkins_build_stop_controlled` / `jenkins_build_stop_approved`；controlled 只创建审批并固化 `requestHash`，approved 执行前复验 hash、连接配置版本、连接状态、`allow_mcp_write` 和风险，成功后调用 Jenkins `/job/{jobPath}/{buildNumber}/stop`，并把本地 run 标记为 `stop_requested`。生产环境或 `release` / `prod` / `production` Job 默认升 L3。
- 构建参数模板已接入本地 `jenkins_parameter_templates` 表和参数抽屉 UI：每个 `connectionKey + jobFullName + requester` 可保存多个命名模板，模板只保存脱敏 `parametersJson` 和 `parameterDefinitionHash`；敏感参数保存时必须是 `{ valueKind: "secret_ref", secretRef }`，明文会被后端拒绝。套用模板只回填参数表单，触发构建仍必须点击“创建审批”并进入既有审批链路。

### 阶段 4：v0.2 / v0.3 联动自动部署

- [x] Jenkins artifact record 接入自动部署 artifact。
- [x] Jenkins 构建结果进入部署 dry-run。
- [x] Git 工作区 commit/branch 作为构建参数。
- [x] 构建成功自动提示部署。
- [x] 构建失败阻断部署。

实现记录：

- 已新增 Jenkins artifact 部署候选生成能力：只允许 `status=available` 且本地文件存在的 artifact record 生成 `DeploymentCandidate`，候选 `configJson` 保留 `artifactKey`、`requestId`、`connectionKey`、`jobFullName`、`buildNumber`、`relativePath`、`sha256` 和风险标记；该步骤不执行部署、不创建部署目标，只为后续 dry-run 和审批部署链路提供受控输入。
- 已新增 Jenkins 构建结果进入部署 dry-run 能力：只允许成功构建对应的 `available` artifact 生成 dry-run；后端构造临时 `DeploymentTarget` 复用自动部署环境探测、阶段生成和风险判断，不落地部署目标、不执行部署命令。Jenkins artifact 来源在阶段计划中显示为“使用 Jenkins artifact”，静态站 artifact 不再生成本地前端构建阶段。
- 已新增 Jenkins 参数抽屉 Git 工作区显式注入能力：选择 Git 工作区后读取当前 branch 和 HEAD commit，只填充 branch/ref/commit/revision/sha 类参数，不填充敏感、文件或不支持参数；注入值会进入现有参数摘要、审批 payload 和 requestHash。
- 已新增构建成功部署提示：监听 Jenkins 构建状态事件，成功构建发现 artifact 时给出部署准备提示；构建详情中成功状态会展示部署准备 Alert，对已下载且 `available` 的 artifact 提供“生成部署候选”入口，仍只进入候选和 Dry-run，不自动执行部署。
- 已新增构建失败部署阻断：后端在创建 Jenkins artifact 部署候选前复验对应构建记录，非成功构建直接拒绝，Dry-run 原有成功构建校验继续保留；前端在失败/不稳定/中止/未构建状态下展示阻断提示，禁用部署候选入口，并在状态事件中提示该构建已阻断部署。

验收标准：

- 能从 Jenkins 构建记录创建部署候选。
- 能通过审批链路完成构建后部署。

---

## 20. 测试计划

### 20.1 Rust 单元测试

- URL 标准化。
- Job path 编码。
- Jenkins JSON 解析。
- Crumb header 解析。
- requestHash 生成和校验。
- 日志截断。
- 日志脱敏。
- 敏感参数识别。
- File Parameter sha256 固化和复验。
- Artifact 托管路径生成。
- 连接级风险规则。
- 并发构建阻断。
- 参数表单定义转换。

### 20.2 集成测试

需要准备一个测试 Jenkins：

- 一个 Freestyle Job。
- 一个 Pipeline Job。
- 一个参数化 Job。
- 一个 File Parameter Job。
- 一个 Folder 下的 Job。
- 一个会失败的 Job。
- 一个长日志 Job。
- 一个可生成 artifact 的 Job。

测试项：

- 连接测试成功 / 失败。
- 401 / 403 / 404 错误提示。
- Job 列表分页 / 深度。
- 构建列表。
- 构建详情。
- consoleText 日志。
- progressiveText 日志。
- 普通构建触发。
- 参数构建触发。
- 队列轮询。
- 停止构建。
- artifact 下载。
- 桌面通知。
- SSH 隧道访问。

### 20.3 浏览器 / 桌面验证

- Jenkins 页面首屏布局。
- 新建连接 Drawer。
- Job 树展开。
- 构建列表筛选。
- 构建详情 Drawer。
- 日志大文本滚动性能。
- 触发构建 Modal。
- 审批队列跳转。

---

## 21. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| Jenkins API 响应因插件不同字段不一致 | 解析失败 | 结构体使用可选字段，保留 raw JSON |
| Folder / Multibranch URL 编码复杂 | Job 找不到 | 统一 job path 转换函数并测试 |
| 日志过大 | UI 卡顿 / MCP 响应过大 | 分块、截断、虚拟滚动 |
| CSRF crumb 兼容问题 | POST 失败 | 支持 crumb 获取、API Token 免 crumb、错误提示 |
| 用户凭证权限过大 | 可误触生产构建 | 连接和 Job 风险规则 + 审批 |
| AI 误触构建 | 生产事故 | controlled/approved + requestHash + 危险规则 |
| Jenkins 内网访问 | 连接失败 | 首版支持通过已登记 SSH 服务器建立隧道访问 |
| Script Console 高危 | 安全风险 | 首版直接禁止 |

---

## 22. 方案对比

| 方案 | 描述 | 优点 | 缺点 | 结论 |
|------|------|------|------|------|
| 方案 A：直接复用通用 HTTP API 凭证 | Jenkins 当作普通 HTTP API | 实现最快 | 缺少 Jenkins 专属模型、UI 和审计语义 | 不推荐作为最终形态 |
| 方案 B：新增 Jenkins 专属模块 | 独立连接、模型、页面、MCP 工具 | 产品体验完整，安全边界清晰 | 开发量中等 | 推荐 |
| 方案 C：只做 MCP 工具不做 UI | 给 AI 用，不做用户页面 | 开发较快 | 用户无法直观看构建和日志 | 可作为过渡，不推荐长期 |
| 方案 D：深度 CI/CD 平台 | Pipeline、环境、发布全托管 | 长期能力强 | 超出桌面工具首版范围 | 远期考虑 |

推荐：**方案 B：新增 Jenkins 专属模块**。

---

## 23. 推荐首版最小闭环

最小可交付闭环：

1. 新增 Jenkins 连接。
2. 测试连接。
3. 读取 Job 列表。
4. 查看构建列表。
5. 查看构建日志。
6. 创建触发构建审批。
7. 审批后触发构建。
8. 跳转查看新构建日志。
9. 记录审计日志。
10. 暴露 MCP 只读和受控触发工具。

这个闭环可以证明：

- Jenkins 连接可用。
- 凭据链路安全。
- UI 有价值。
- MCP 有价值。
- 审批链路能覆盖外部 CI 写操作。

---

## 24. 已确认产品决策

1. **首版定位**: Jenkins 模块首版是构建运维工作台，不是轻量连接器。
2. **认证方式**: 首版只支持 Jenkins username + API Token，不支持密码登录和 Cookie。
3. **MCP 触发目标**: MCP 触发构建必须显式指定 `connectionKey + jobFullName + parameters`，不允许按关键字猜测 Job。
4. **File Parameter**: 只允许本地文件路径引用，controlled 阶段固化 sha256，approved 阶段复验，不允许 MCP 传 base64 文件内容。
5. **Artifact 下载路径**: MCP 下载 artifact 只能写入应用托管目录，不能写任意本地路径。
6. **SSH 隧道生命周期**: Jenkins SSH 隧道按连接级短期复用，空闲 5 分钟自动关闭。
7. **桌面通知默认值**: 默认通知失败、终止、不稳定；成功通知默认关闭。
8. **风险默认值**: 未匹配规则默认 L2，生产、File Parameter、并发等条件自动升级到 L3 或 blocked。
9. **并发构建**: 同 Job 正在构建时默认禁止再次触发，只有连接级规则显式放开才允许。
10. **敏感参数**: 构建参数必须做敏感字段识别和脱敏。
11. **MCP 敏感参数**: MCP 不允许传明文敏感值，只允许 `secretRef` 或授权的 `useLastSuccessfulValue`。
12. **日志敏感信息**: 日志脱敏同时使用敏感关键词和通用凭证模式；UI 默认脱敏展示，人工确认后可看原始日志；MCP/AI 永远只返回脱敏日志，脱敏优先避免漏报。
13. **构建记录持久化**: Jenkins 原始历史实时读取为主，本地只持久化本应用触发、跟踪或关注的构建。
14. **构建跟踪职责**: 队列轮询和构建状态跟踪由 Rust 后端负责，前端只展示/订阅状态。
15. **MCP 权限拆分**: `allow_mcp` 拆成 `allow_mcp_read` 和 `allow_mcp_write`。
16. **环境分类**: Jenkins 连接需要 `environment` 字段，作为风险计算输入。
17. **通知/轮询范围**: 首版不全量监控 Jenkins，只跟踪本应用触发或用户手动关注的构建。
18. **Artifact 一等记录**: Jenkins artifact 下载后成为应用内 artifact record，后续自动部署引用 record。
19. **TLS 校验**: 支持连接级 `tls_verify=false`，默认 true；关闭时显示风险并提升写操作风险。
20. **反向代理路径**: 支持 Jenkins 部署在 `/jenkins` 等子路径下。
21. **权限预检**: 首版只做能力探测，不复制 Jenkins RBAC 权限模型；连接页显示可读、可触发、可下载等能力标签，实际操作失败进入审计日志。
22. **查询缓存**: Job 树只做 30-60 秒短 TTL 内存缓存，构建列表默认实时读取；MCP 可用 `forceRefresh=true` 绕过缓存，不持久化全量 Jenkins 历史。
23. **最近成功值复用**: `useLastSuccessfulValue` 默认按 requester 隔离；连接级开启共享复用后允许跨用户使用，但相关触发必须 L3 并写审计。
24. **构建跟踪超时**: Queue 等待 build number 默认 10 分钟，Build 跟踪默认 2 小时；超时只影响本地跟踪状态，不代表 Jenkins 构建失败。
25. **停止构建边界**: 停止构建不进入 v0.1，放到 v0.2；默认走审批，生产环境或 release/prod Job 升 L3。
26. **连接删除策略**: Jenkins 连接默认软删除，不级联删除构建记录、artifact 记录和审计日志；本地制品清理作为独立审计操作。
27. **连接恢复策略**: 连接恢复进入 v0.1；恢复后强制连接测试，失败则状态为 `failed` 且禁止构建触发和 MCP 使用，不自动补齐历史构建。
28. **连接 Key 不可变**: `connectionKey` 创建后不允许修改；如需调整 Key，应新建连接并软删除旧连接，避免历史构建、artifact、审批和审计断链。
29. **Job 标识策略**: 首版以 `jobFullName` 作为 Job 主标识；Job 改名或移动后不自动迁移历史，MCP 遇到旧路径 404 只返回明确错误和候选结果，不自行替换目标。
30. **动态参数插件**: v0.1 只标准支持 string、boolean、choice、password、file 参数；动态插件参数降级为手动输入或跳转 Jenkins，MCP 不猜参数值。
31. **Jenkins 原始页面**: 允许通过系统浏览器打开 Jenkins 原始页面辅助排障；不内嵌 Jenkins，不注入 Cookie/session/Token，跳转行为只写审计不走写审批。
32. **连接测试结果**: Jenkins 连接保存最近一次脱敏测试结果和能力探测 JSON；失败详情只保留排障摘要，不保存 Token、Cookie、crumb 或 Authorization header。
33. **MCP 日志限制**: MCP 默认返回脱敏日志尾部 200KB，`tailBytes` 最大 1MB；progressive 读取返回 `truncated`、`nextStart` 和 `textSize`，不提供原始日志模式。
34. **Artifact 大小限制**: 单个 artifact 默认最大 500MB，连接级可配置但最高 2GB；UI 和 MCP 下载同限，超过上限立即中止并写审计。
35. **Artifact 类型风险**: v0.1 不按扩展名黑名单阻断 artifact 下载；可执行或安装类文件只标记 `riskFlags`、提示风险并写审计，不自动执行或打开。
36. **无后台触发**: v0.1 构建触发只来自 UI 用户动作或 MCP controlled/approved 请求；不做定时构建、后台自动触发、失败自动重试或构建后自动触发下游 Job。
37. **Git 参数联动**: v0.1 只提示 branch/commit 类参数，不自动从 Git 工作区注入；v0.2 显式联动时必须进入审批摘要，MCP 必须显式传参。
38. **连接配置复制**: v0.1 允许复制 Jenkins 连接配置，但不复制 `connectionKey`、名称、`credentialKey`、构建记录、artifact、最近 Job 和最近成功参数值；复制后默认禁用，测试成功后才能启用。
39. **连接启停策略**: v0.1 启用/禁用 Jenkins 连接不走审批但写审计；禁用立即阻断 UI 构建和 MCP 使用，启用必须先测试成功，prod 或 MCP 写开启时弹人工确认。
40. **Jenkins View**: v0.1 支持读取和选择 Jenkins View、保存默认 View，并允许 MCP `jenkins_jobs_list` 传 `viewName`；不创建、修改或删除 View，View 不存在时不自动回退 All。
41. **Job 树深度**: v0.1 Folder / Job 树默认递归深度 3 层、最大 5 层；更深层级按需加载并返回 `hasMore=true`，不一次性扫描全站。
42. **队列展示范围**: v0.1 不做独立全局队列页，只展示本次触发 queue item 和当前 Job 排队状态；MCP 队列读取默认返回受限摘要，优先按 Job 查询。
43. **HTTP Proxy**: v0.1 不做 Jenkins 专属 HTTP proxy 配置，不新增 proxy credential；内网访问优先 SSH 隧道，后续如需代理必须复用安全凭证引用。
44. **Crumb 缓存**: Jenkins crumb 只按 `connectionKey + credentialKey + baseUrl` 做内存缓存，默认 TTL 30 分钟；403 crumb 错误清缓存并重试一次，不落库也不记录 crumb 值。
45. **连接级限流**: v0.1 每个 Jenkins 连接最多 4 个并发只读请求、1 个并发 approved 写操作；日志 progressive 单 Job 单通道，等待队列 30 秒超时，UI 和 MCP 共用限流器。
46. **审批审计保留**: Jenkins 审批和审计复用现有统一保留策略，不在 Jenkins 模块单独设置保留期；`jenkins_build_runs` 持久保留，artifact 文件清理不影响审批/审计。
47. **AI 失败总结**: Jenkins 构建失败 AI 总结不进入 v0.1，放到 v0.2；只能使用脱敏日志片段，结果保存为本地分析记录，不写回 Jenkins。
48. **测试报告边界**: JUnit/Test Report 不进入 v0.1；v0.1 仅展示 Jenkins API 返回的简单 test summary，不解析 XML、不建测试表，v0.3 再做测试报告和趋势图。
49. **Blue Ocean**: Blue Ocean URL 深链不进入 v0.1；v0.1 只提供 Jenkins 原始 URL，v0.3 如需支持需先能力探测插件可用性。
50. **Agent/Executor**: Agent / executor 状态读取不进入 v0.1；v0.1 不读取 `/computer/api/json`，只展示 queue item 已返回的简短排队原因，v0.3 再做只读看板和队列阻塞分析。
51. **环境枚举**: v0.1 环境使用固定枚举 `dev/test/staging/prod/custom`，UI 中文显示为开发/测试/预发/生产/自定义；允许保存 `environmentLabel`，默认只有 `prod` 自动升级生产风险，`custom` 不自动按生产处理。
52. **通知文案**: Jenkins 桌面通知统一中文，只包含连接名称、Job 名称和构建号；不包含参数、日志或 artifact 路径，点击只打开本地构建详情且不写审计。
53. **无批量操作**: v0.1 不支持批量触发 Job、批量下载 artifact、批量删除或批量清理记录；所有写操作和下载都按单个目标处理。
54. **凭证摘要**: 连接列表和 MCP 连接列表只返回 `credentialKey` 与脱敏凭证摘要；不显示 Token 片段，凭证不可用时禁止构建触发和 MCP 写，摘要只由连接测试更新。
55. **连接测试分层**: 凭证不可用时仍可测试 Jenkins 网络连通性，但连接状态只能是 `credential_missing` / `credential_failed`，不允许读取 Job/日志、触发构建或作为 MCP 可用连接。
56. **Base URL 标准化**: Jenkins Base URL 只允许 HTTP/HTTPS，保存时去掉末尾 `/`、保留子路径、禁止 query/hash；默认推荐 HTTPS，HTTP 或关闭 TLS 校验时写操作至少 L2，生产环境升 L3。
57. **Job 搜索范围**: v0.1 Job 搜索只在当前 View / Folder 已加载数据中本地过滤，UI 明示搜索范围；MCP 不提供全站模糊搜索或按搜索结果直接触发构建能力。
58. **构建参数模板**: 构建参数模板不进入 v0.1；v0.1 只做最近参数回填，v0.2 再支持每个 Job 的命名模板，敏感参数只能保存 `secretRef`，触发仍走审批。
59. **Replay/重试边界**: Pipeline replay 和重试参数构建不承诺 v0.2，放到 v0.3 可选；replay 必须 L3，且不允许 AI 修改 Jenkinsfile 或脚本内容。
60. **收藏 Job**: v0.1 支持 UI 轻量收藏 Job，只保存连接、Job 标识、显示名、URL 和最后状态；不自动全量轮询，收藏/取消收藏不走审批但写审计，MCP v0.1 不管理收藏。
61. **连接导入导出**: Jenkins 连接导入/导出不进入 v0.1；复制连接覆盖本机复用场景，后续如需导入/导出必须脱敏且处理跨机器凭证缺失。
62. **多连接聚合**: v0.1 一次只选择一个 Jenkins 连接作为当前上下文，不跨连接聚合；v0.3 如做多 Jenkins 聚合看板，仅限只读，写操作必须回到单连接上下文。
63. **Change Sets 脱敏**: v0.1 Change Sets 只展示 commit id、作者显示名和脱敏后的提交摘要；默认不展示邮箱，MCP 同样脱敏，不展示代码 diff。
64. **Causes 脱敏**: v0.1 Causes 只展示触发类型和脱敏触发人显示名；不展示邮箱、ID token、远程地址，远程 URL/token/IP 脱敏或隐藏，不映射到本地用户。
65. **参数摘要持久化**: 构建运行记录、审批摘要和审计详情只保存脱敏参数摘要；敏感参数保存 `***`、`secretRef` 或 `useLastSuccessfulValue` 标记，真实值仅在 approved 执行时解析。
66. **触发返回节奏**: 构建触发成功后先返回 queueId/queueUrl；tracker 拿到 buildNumber 后再同步构建详情，MCP approved 不阻塞等待完整详情，queue 超时返回 `queue_timeout`。
67. **状态归一**: UI 状态颜色按归一 result/status 显示，不直接依赖 Jenkins `color`；MCP 返回归一状态和 Jenkins 原始字段。
68. **构建列表分页**: v0.1 按 Job 读取构建列表，默认最新 30 条、单次最多 100 条；UI 用“加载更多”，MCP `jenkins_builds_list` 支持 `limit` 和 `cursor/offset`，不扫描全量历史，审计只记录请求元数据。
69. **日志正文不落库**: v0.1 不持久化 Jenkins 控制台日志正文；本地只保存读取审计、范围元数据和必要的脱敏分析摘要，Jenkins 仍是日志真源；v0.2 AI 失败总结也只能保存基于脱敏日志片段的分析记录。
70. **Artifact 本地清理**: v0.1 只支持单个 artifact 的本地文件清理，清理前人工确认；只删除应用托管目录内文件，不删除 artifact record、审批和审计记录；状态改为 `local_deleted` 或 `file_missing`；MCP 不提供清理工具。
71. **requestHash 固化**: 构建审批 requestHash 固化连接、连接配置版本、Job、参数定义摘要 hash、参数摘要、敏感参数引用、File Parameter 文件名/大小/sha256/mtime、requester、reason、riskLevel、riskFlags 和 createdAtBucket；approved 执行时复验，任何目标、参数定义、参数、文件或引用变化都拒绝执行，hash payload 不包含明文敏感值或文件内容。
72. **MCP 权限即时生效**: `allow_mcp_read` / `allow_mcp_write` 变更立即生效；approved 执行时重新读取连接、凭证和 MCP 写权限，连接禁用、凭证失效或 `allow_mcp_write=false` 时拒绝执行，审批记录标记执行失败或策略拒绝并写审计。
73. **风险规则编辑**: v0.1 连接级风险规则使用表单化编辑器，支持 Job 正则、参数、环境、File Parameter、并发构建和 fallback 规则；底层保存 `risk_rules_json`，UI 只提供 JSON 只读调试视图，不允许自由编辑 JSON。
74. **requestId 链路追踪**: 每次 Jenkins UI/MCP 调用生成 `requestId`，贯穿审批创建、approved 执行、Jenkins HTTP 请求、queue 跟踪、build run、artifact 下载和审计日志；`approvalId`、`runKey`、`artifactKey` 负责业务关联，`requestId` 负责排障串链路；MCP 返回带 `requestId` 但不暴露内部错误堆栈。
75. **approved 执行审计**: 只要审批通过后进入 approved 执行阶段，无论 Jenkins API 成功、连接禁用、凭证失效、requestHash 不匹配、crumb 失败或 Jenkins 返回 403/404/500，都必须写执行审计，包含 `requestId`、`approvalId`、失败阶段、错误码、脱敏错误摘要和是否已触达 Jenkins。
76. **构建状态推送**: v0.1 由 Rust 后端 tracker 轮询 Jenkins，前端通过 Tauri event 订阅本地 run 状态变化并保留手动刷新；事件只推送 `requestId`、`runKey`、`queueId`、`buildNumber`、`status`、`statusSource`、`result`、`updatedAt` 等摘要，不推日志正文、参数明文或敏感路径。
77. **跟踪恢复策略**: 应用启动时只恢复本应用触发或用户手动跟踪、且状态为 `queued` / `building` / `tracking_timeout` 的 run；按 `connectionKey + jobFullName + queueId/buildNumber` 单次同步 Jenkins 真实状态，失败标记 `sync_failed`，禁用连接或凭证失效的 run 不后台恢复，只显示本地最后状态和手动刷新入口。
78. **Run 状态枚举**: `jenkins_build_runs.status` 固定为 `queued`、`building`、`success`、`failure`、`unstable`、`aborted`、`not_built`、`queue_timeout`、`tracking_timeout`、`sync_failed`、`unknown`；前 7 个可来自 Jenkins 归一结果，后 4 个是 Tauri SSH 本地跟踪状态；UI 和 MCP 必须返回 `statusSource: "jenkins" | "local"`。
79. **参数回填保存**: v0.1 非敏感参数可保存原值，用于审批摘要、构建记录和最近成功参数回填；只保留最近成功值，不保存无限历史值列表；敏感参数永不保存明文；连接级 `parameter_prefill_enabled=false` 后不再用于 UI/MCP 回填，但历史构建脱敏摘要仍保留。
80. **最近成功参数表**: v0.1 单独建立 `jenkins_recent_parameter_values`，按 `connection_key + job_full_name + parameter_name + requester` 保存最近成功值；共享模式使用 `requester='__shared__'`；字段包含 `value_kind`、`value_json`、`sensitive`、`updated_from_run_key` 和 `updated_at`，避免为回填扫描构建历史。
81. **复制连接不复制参数值**: `duplicate_jenkins_connection` 不复制 `jenkins_recent_parameter_values`；最近成功参数值可能包含环境、分支、发布参数或 `secretRef` 引用，跨连接复制容易误触发或引用错误凭证。
82. **忘记最近参数值**: v0.1 支持 UI 按单个 `connectionKey + jobFullName + parameterName + requester` 忘记最近参数值；共享值 `requester='__shared__'` 只有管理员可删；操作不走审批但写审计，MCP 不提供删除工具。
83. **Multibranch 边界**: v0.1 只按 Jenkins Job/Folder 树展示 multibranch pipeline 下已索引出的分支 Job，不单独做 SCM 分支发现、不读取 Git 分支列表、不触发 Jenkins scan，也不创建新分支构建。
84. **Scan/索引排期**: Jenkins scan / branch indexing 不进入 v0.2，至少放到 v0.3 可选；该能力会触发 Jenkins 对 SCM 的扫描，属于有外部副作用的写操作，如支持必须按 L3 进入审批和审计。
85. **凭证轮换检测**: v0.1 不做 Jenkins 凭证自动轮换检测，不后台主动探测 Token；只在手动连接测试或读取 Job、触发构建等实际调用失败时更新连接状态为 `credential_failed` 并写审计，凭证本身仍由安全凭证模块管理。
86. **失败连接保存**: Jenkins 连接测试失败也允许保存；网络不可达保存为 `failed`，缺少凭证保存为 `credential_missing`，网络可达但凭证失败保存为 `credential_failed`；这些连接不能读取 Job、触发构建或暴露给 MCP，只允许后续编辑后重新测试。
87. **连接关键字段失效**: `baseUrl`、`credentialKey`、`sshServerAlias`、`tlsVerify` 任一变化后，`config_version` 自增，清空 crumb/Job 树缓存、能力探测、版本、凭证摘要和最近错误，连接状态回到 `unknown`，重新测试成功前不允许读取 Job、触发构建或暴露给 MCP。
88. **审批后连接变更**: 连接关键字段变更后不主动批量修改已有 pending 审批状态，但该连接下 Jenkins 写审批在 approved 执行时必须对比 controlled 阶段的 `connectionConfigVersion`；不一致则拒绝，错误码 `connection_changed_after_approval`，审批详情提示连接配置已变更并要求重新创建审批。
89. **连接配置版本**: `jenkins_connections` 增加 `config_version INTEGER DEFAULT 1`；仅 `baseUrl`、`credentialKey`、`sshServerAlias`、`tlsVerify` 变化时自增，controlled 阶段写入 requestHash payload 为 `connectionConfigVersion`，approved 执行时对比当前版本；名称、备注、通知、默认 View/Folder、风险规则和 MCP 开关不自增，风险规则和 MCP 开关靠 approved 实时复验。
90. **风险规则实时复验**: 风险规则变更不触发 `config_version`，但 approved 执行时必须使用当前 `risk_rules_json` 重新计算风险；按 `readonly < L2 < L3 < blocked` 比较，当前风险高于审批时风险则拒绝并要求重新创建审批，当前风险降低也不自动降级执行，避免先按低风险建审批再改规则绕过 L3。
91. **风险等级顺序**: 风险等级固定为 `readonly < L2 < L3 < blocked`；`blocked` 永远拒绝，approved 复验必须使用统一风险排序函数，不能按字符串临时比较。
92. **MCP 只读审计**: MCP 只读工具写轻量审计，只记录 `requestId`、工具名、目标连接/Job/构建、分页或日志范围、结果状态和耗时；不记录完整响应、日志正文、参数明文或连接详情，`jenkins_connections_list` 只记录汇总审计。
93. **UI 只读审计**: UI 只读操作也写轻量审计，但只在真实请求 Jenkins 时记录；页面切换、展开已缓存节点和本地筛选搜索不审计；日志 progressive 按会话合并审计，避免每次轮询写入审计表。
94. **Progressive 日志会话**: progressive 日志审计会话按 `requestId + connectionKey + jobFullName + buildNumber + viewer` 定义；用户关闭日志面板、构建完成后停止跟随、超过 60 秒无继续读取或切换构建时结束，会话审计合并更新 `startOffset/endOffset/totalBytes/truncated/redacted/durationMs/pollCount`。
95. **日志复制审计**: UI 复制选中日志必须写轻量审计，只记录连接、Job、构建号、复制 offset 范围、字节数和是否脱敏，不记录复制内容；原始日志模式复制额外记录 `rawLogAccess=true` 和确认来源；MCP 不提供原始日志或复制动作。
96. **原始日志确认**: UI 原始日志模式每个构建详情会话确认一次，默认 10 分钟有效；切换构建、关闭详情 Drawer、连接关键字段变化或超时后失效；确认状态只在当前 UI 会话内存中存在，不持久化，不影响 MCP/AI。
97. **日志下载边界**: v0.1 不提供日志文件下载，只支持查看和复制选中范围；如 v0.2 支持日志导出，也只能导出脱敏日志并写独立审计；原始日志永不提供下载。
98. **参数定义缓存**: v0.1 对 Jenkins Job 参数定义做短 TTL 内存缓存，默认 60 秒，按 `connectionKey + jobFullName` 缓存；手动刷新绕过缓存，创建构建审批前必须重新读取参数定义或校验未变化；不持久化参数定义，避免 Jenkins Job 配置变更后本地表单过期。
99. **参数定义复验**: controlled 阶段将参数定义摘要 hash 写入 requestHash payload，摘要包含参数名、类型、是否敏感、choice 可选项摘要和是否 File Parameter；approved 执行前重新读取参数定义并比对 hash，不一致则以 `parameter_definition_changed_after_approval` 拒绝执行并要求重新创建审批。
100. **动态参数安全降级**: Active Choices、Extended Choice、Git Parameter 等动态参数默认标记 `dynamicParameter=true`；无法获得稳定参数定义 hash 时，该 Job 不允许 MCP 触发，UI 只能跳转 Jenkins 原页面或人工输入后进入 L3 审批，approved 前仍无法确认一致则拒绝自动执行。

这些决策已纳入前文功能范围、数据库字段、安全策略和实施计划。
