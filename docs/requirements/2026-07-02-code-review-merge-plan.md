# 代码审核与分支合并功能详细设计方案

**状态**: 规划中  
**创建时间**: 2026-07-02  
**目标版本**: v0.3  
**菜单位置**: 安全 -> 代码审核  
**目标模块**: 安全 / 安全凭证 / Git 工作区 / AI Provider / 审批队列 / 审计日志 / MCP Server  
**参考形态**: Git 工作区、受控 Git 操作、AI 代码审查、批量分支合并助手

---

## 1. 背景与目标

当前应用已经具备安全凭证、Git 工作区、Git Provider API、AI Provider、审批队列、审计日志和 MCP 工具能力。Git 工作区已经可以管理本地仓库、绑定凭证、查看分支、Pull、Push、切换分支、合并分支和 AI 提交。

菜单命名上，将现有一级菜单 `安全凭证` 调整为 `安全`。`安全` 作为更上层的能力集合，统一承载凭证库、会话、MCP 接入、审计、策略、Git 工作区和代码审核等子功能。

下一步需要把这些能力组合成一个面向研发协作的“代码审核”模块，解决两个典型场景：

1. 用户选择一个 Git 工作区项目，选择源分支和目标分支，系统先展示两个分支之间的代码修改，再调用 AI 分析变更，生成详细审查报告。用户确认审查结果后，再执行分支合并。
2. 用户直接复制粘贴一段需求文字，例如：

```text
将 fj-evaluate、fj-evaluate-service、fj-db-service、fj-feb 项目的 dev-过错认定v3.7 分支
和 fj-evaluate-app 的 dev-v3.7 分支都合并 dev 分支
```

系统自动解析出多个项目、源分支、目标分支，生成批量合并计划，并对每个项目执行与第一个场景相同的审查流程。

一句话定位：

> 代码审核模块是一个基于本地 Git 工作区和安全凭证的受控合并工作台：先看 diff、再 AI 审查、再用户确认、最后执行合并，并把全过程审计留痕。

---

## 2. 功能边界

### 2.1 首版必须实现

- 将一级菜单 `安全凭证` 调整为 `安全`。
- 在 `安全` 模块下新增 `代码审核` 菜单。
- 支持选择已有 Git 工作区项目。
- 首版仅支持本地 Git 工作区，不直接选择远程 Provider 仓库进行审查或合并。
- 支持选择源分支和目标分支。
- 支持读取分支最后提交信息。
- 支持生成分支比较结果：
  - 提交列表。
  - 文件变更列表。
  - diff 片段。
  - 新增/删除/修改统计。
- 支持 AI 代码审查：
  - 总体风险评级。
  - 重点风险清单。
  - 文件级审查结果。
  - 可能的兼容性/安全/性能/数据库/配置风险。
  - 建议人工重点复核项。
- 支持用户确认后执行合并。
- 支持批量文本解析：
  - 解析项目名。
  - 解析源分支。
  - 解析目标分支。
  - 区分前端项目和后端项目的不同源分支。
  - 生成可编辑的任务列表。
- 支持批量计划逐项审查、逐项确认、逐项合并。
- 全流程写入审计日志。

### 2.2 首版不做

- 不自动解决冲突。
- 不自动 Push 远程，除非后续用户明确要求。
- 不自动创建 Pull Request / Merge Request。
- 不做跨仓库依赖顺序的智能推断，只按用户确认的任务顺序执行。
- 不做批量合并事务。多个仓库的合并任务逐项独立执行，某项失败不自动回滚其他已成功项。
- 不绕过现有 Git 工作区的干净工作区校验。
- 生成审查前也必须要求工作区干净，避免本地未提交改动污染审查 diff。
- 不把 Git Token、SSH Key、密码等凭证明文传给 AI。
- 不自动执行测试、编译、构建命令。

### 2.3 后续增强

- 合并后自动 Push。
- 自动创建 PR/MR。
- 支持 GitHub / GitLab / GitCode / Gitee Provider API 模式，直接选择远程仓库、分支、PR/MR 进行审查。
- 支持冲突文件辅助分析。
- 支持临时预合并分支，在不污染目标分支的情况下提前检测冲突。
- 支持基于规则的审核 Gate，例如严重风险必须阻止合并。
- 支持关联需求号、版本号、发布单。
- 支持 MCP 工具对外提供 `code_review_*` 能力。
- 支持审查报告文件导出。
- 支持 AI 审查历史版本。

---

## 3. 用户流程设计

### 3.1 单项目代码审核合并

1. 用户进入 `安全 -> 代码审核`。
2. 选择 Git 工作区。
3. 点击刷新分支，系统读取本地和远程分支。
4. 选择源分支和目标分支。
5. 系统展示：
   - 当前分支。
   - 源分支最后提交。
   - 目标分支最后提交。
   - 工作区是否干净。
   - ahead/behind 状态。
6. 用户点击 `生成审查`。
7. 后端执行：
   - `git fetch --prune`。
   - 校验源分支和目标分支存在。
   - 校验工作区干净。
   - 生成 merge-base。
   - 读取提交列表。
   - 生成 diff 统计和 diff 片段。
8. 前端展示代码修改。
9. 后端调用 AI Provider 生成审查报告。
10. 用户阅读审查报告。
11. 用户点击 `确认合并`。
12. 系统二次确认，并提示风险摘要。
13. 后端执行合并前重新校验审查快照：
    - 先执行一次 `git fetch --prune`，刷新远程分支状态。
    - 当前工作区必须干净。
    - 当前源分支 HEAD 必须等于任务保存的 `source_head`。
    - 当前目标分支 HEAD 必须等于任务保存的 `target_head`。
    - 当前 `merge-base(source, target)` 必须等于任务保存的 `merge_base`。
    - fetch 后任一快照字段不一致时，任务状态改为 `stale`，禁止合并，要求重新生成 diff 和 AI 审查。
14. 后端执行合并：
    - checkout 目标分支。
    - merge 源分支。
    - 成功后刷新工作区状态。
15. 写审计日志。
16. 合并成功后展示 `推送远程` 按钮，推送必须由用户另行点击触发。

### 3.2 批量文本解析合并

用户粘贴示例文本：

```text
宾哥过错认定 3.7 版本帮忙合个 dev
前端项目：fj-evaluate-app 分支：dev-v3.7
后端项目：fj-evaluate、fj-evaluate-service、fj-db-service、fj-feb 分支：dev-过错认定v3.7
```

解析目标：

| 项目 | 源分支 | 目标分支 |
|------|--------|----------|
| fj-evaluate-app | dev-v3.7 | dev |
| fj-evaluate | dev-过错认定v3.7 | dev |
| fj-evaluate-service | dev-过错认定v3.7 | dev |
| fj-db-service | dev-过错认定v3.7 | dev |
| fj-feb | dev-过错认定v3.7 | dev |

流程：

1. 用户进入 `批量解析` Tab。
2. 粘贴自然语言文本。
3. 点击 `解析合并任务`。
4. 系统先用规则解析：
   - `前端项目` 段落。
   - `后端项目` 段落。
   - `分支：xxx`。
   - `合个 dev` / `合并 dev` / `合到 dev` 作为目标分支。
5. 规则解析置信度不足时，调用 AI 辅助解析，要求输出严格 JSON。
6. 系统根据项目名匹配已有 Git 工作区：
   - 优先匹配 `workspace.name`。
   - 其次匹配仓库目录名。
   - 再匹配 remote URL 中的 repo 名。
7. 展示可编辑任务表。
8. 用户确认任务列表。
9. 系统按任务逐项生成审查：
   - 每个任务独立读取 diff。
   - 每个任务独立生成 AI 审查报告。
   - 支持失败项跳过或重试。
10. 用户逐项或批量确认合并。
11. 系统逐项独立执行合并：
    - 每个项目是一条独立审查任务。
    - 批量按钮只批量触发多个独立任务。
    - 某项失败、冲突或 stale 不自动回滚其他已成功项。
    - 用户需要强一致时，应先确认全部审查结果，再按任务逐项执行。
12. 合并完成后生成批量结果报告，明确展示成功、失败、冲突、stale 和跳过数量。

---

## 4. 页面设计

### 4.1 菜单与路由

- 菜单：`安全 -> 代码审核`
- 路由：`/secure-credentials/code-review`
- 页面文件：`src/pages/secure-credentials/code-review.tsx`
- 类型文件：`src/types/codeReview.ts`
- API 文件：`src/lib/api/codeReview.ts`

说明：首版可以保留现有 `/secure-credentials/*` 路由前缀，先只调整侧边栏一级菜单显示名称为 `安全`，避免一次性迁移路由导致历史链接和页面状态失效。

首版必须使用独立页面文件，避免 `secure-credentials/index.tsx` 继续膨胀。`secure-credentials/index.tsx` 不承载代码审核业务 UI，只保留现有安全凭证模块页面能力。

### 4.2 页面结构

使用 Ant Design `Tabs`：

- `单项目审核`
- `批量解析`
- `审查记录`
- `策略设置`（可后置）

### 4.3 单项目审核 Tab

主要区域：

- 顶部表单：
  - Git 工作区 Select。
  - 源分支 Select。
  - 目标分支 Select。
  - 刷新分支 Button。
  - 生成审查 Button。
- 工作区状态 Card：
  - 当前分支。
  - 工作区状态。
  - 源分支最后提交。
  - 目标分支最后提交。
  - ahead/behind。
- Diff 预览：
  - 文件变更 Table。
  - 文件 diff Drawer。
  - 大 diff 自动截断，并提示可打开文件级查看。
- AI 审查：
  - 总体结论。
  - 风险等级。
  - 问题清单。
  - 文件级建议。
  - 审查 prompt 和模型摘要。
- 操作区：
  - 重新审查。
  - 确认合并。
  - 放弃。

### 4.4 批量解析 Tab

主要区域：

- TextArea：粘贴合并需求。
- 解析 Button。
- 解析结果 Table：
  - 项目名。
  - 匹配工作区。
  - 源分支。
  - 目标分支。
  - 匹配状态。
  - 最后提交。
  - 操作。
- 批量操作：
  - 生成全部审查。
  - 仅生成选中项审查。
  - 确认合并选中项。
- 批量结果：
  - 成功数。
  - 失败数。
  - 冲突数。
  - stale 数。
  - 跳过数。
  - 待确认数。
  - 每项错误原因。

### 4.5 审查记录 Tab

列表字段：

- 审查编号。
- 工作区。
- 源分支。
- 目标分支。
- 状态。
- 风险等级。
- AI Provider / 模型。
- 创建时间。
- 操作：查看详情、重新审查、复制报告。

报告输出：

- 首版支持 `复制报告`，复制 Markdown 格式审查报告。
- Markdown 内容包含项目、源分支、目标分支、风险等级、问题清单、测试建议、AI Provider / 模型和审查时间。
- 首版不做文件导出，导出 Markdown / HTML / PDF 可后续增强。

---

## 5. 后端架构设计

### 5.1 新增模块

```text
src-tauri/src/
├── commands/
│   └── code_review.rs
├── services/
│   └── code_review.rs
├── database/
│   └── mod.rs
├── models/
│   └── mod.rs
```

调用链路：

```text
React 页面
  -> src/lib/api/codeReview.ts
    -> commands::code_review
      -> CodeReviewService
        -> GitWorkspaceService / AiProviderService / AuditService
        -> Database
```

后端拆分原则：

- `GitWorkspaceService` 只保留基础 Git 工作区能力。
- `CodeReviewService` 负责审查任务状态机、diff 快照、AI 审查、合并门禁、推送状态、批量解析和审计上下文。
- 不把代码审核业务继续塞进 `git_workspace.rs`。
- 不直接复用 `GitWorkspaceService::merge_branch` 执行最终合并，除非该方法已经支持防漂移校验、`push_status`、高风险目标分支审计和代码审核任务上下文。
- 首版应在 `CodeReviewService` 内实现受控合并流程，或抽取 Git helper 后由 `CodeReviewService` 编排。
- 数据库方法可以继续放在 `database/mod.rs`，但方法名统一使用 `code_review_*` 前缀并集中分组。

### 5.2 复用现有能力

- `GitWorkspaceService`
  - 工作区列表。
  - 分支列表。
  - Refresh。
  - Switch branch。
  - Merge branch。
  - 凭证注入 Git 命令。
- `AiProviderService`
  - AI 审查 prompt 调用。
  - Skill/经验注入。
- `AuditService`
  - 审查创建、AI 调用、合并执行、失败记录。
- `SecureCredentialService`
  - Git 工作区绑定凭证时复用安全凭证。

首版范围说明：

- 代码审核与分支合并首版只基于本地 Git 工作区。
- 不直接通过 GitHub / GitLab / GitCode / Gitee Provider API 选择远程仓库执行 compare 或 merge。
- 远程 Provider 模式后续主要用于 PR/MR 审查、网页仓库 diff 和平台权限预检，不替代本地 Git 工作区合并流程。

### 5.3 Git 命令策略

只允许后端执行固定 Git 命令模板：

```text
git fetch --prune
git rev-parse --verify <branch>
git merge-base <source> <target>
git log --oneline --decorate <base>..<source>
git diff --stat <base>..<source>
git diff --name-status <base>..<source>
git diff --unified=80 <base>..<source> -- <path>
git status --porcelain
git checkout <target>
git merge --no-edit <source>
```

禁止用户自由输入 Git 命令。

合并策略：

- 首版沿用 Git 默认 merge 行为，允许 fast-forward。
- 不强制使用 `--no-ff` 生成合并提交。
- 审计以 `code_review_tasks` 和审计日志为准，不依赖 Git merge commit 本身记录“审核合并动作”。
- 后续如需强制 merge commit，可在策略设置中增加“强制生成合并提交”开关。

Diff 基准策略：

- 审查主 diff 使用 `merge-base(source, target)..source`。
- 不使用 `target..source` 作为主审查 diff，避免源分支和目标分支互相都有提交时产生方向性误解。
- 页面同时展示目标分支相对 merge-base 的提交数量，提醒用户目标分支是否已经前进。
- 如果确认合并前 merge-base 变化，任务状态改为 `stale`，要求重新生成 diff 和 AI 审查。
- 合并前的二次 `fetch --prune` 如果导致源分支、目标分支或 merge-base 变化，也按 `stale` 处理。

工作区干净策略：

- 创建审查任务和生成 diff 前必须校验工作区干净。
- 合并前必须再次校验工作区干净。
- 如果存在未提交改动，前端展示未提交文件列表并禁止生成审查或合并。
- 系统不自动 `stash`。
- 系统不自动提交本地改动。
- 系统不自动丢弃本地改动。

### 5.4 冲突处理

首版策略：

- 合并前必须确认工作区干净。
- 合并前必须再次执行 `git fetch --prune`，然后再做防漂移校验。
- 合并前必须做防漂移校验，确认 `source_head`、`target_head`、`merge_base` 与生成审查时完全一致。
- 目标分支命中高风险规则时，必须进入单项详情手动确认，禁止批量确认。
- 如果源分支、目标分支或 merge-base 已变化：
  - 任务状态改为 `stale`。
  - 审计记录为 `code_review_merge_stale`。
  - 前端提示“分支已变化，需要重新生成审查”。
  - 不执行 checkout 或 merge。
- 如果 merge 出现冲突：
  - 后端返回冲突文件列表。
  - 审计记录为 `merge_conflict`。
  - 前端提示用户手动处理。
  - 不自动 `git merge --abort`，除非提供明确按钮 `中止本次合并`。
- 首版不创建临时预合并分支。冲突发生后，工作区会停留在 Git 冲突状态，用户必须处理冲突或点击 `中止合并`。
- 后续增强可增加 `code-review/premerge/<task_key>` 临时分支进行预合并检查，并在检查完成后清理临时分支。

建议提供按钮：

- `中止合并`：执行 `git merge --abort`，需要二次确认。

### 5.5 目标分支保护与权限提示

首版不硬编码禁止合并到 `master`、`main`、`release/*`、`prod/*`、`production`，但这些目标分支默认视为高风险目标分支。

命中高风险目标分支时：

- 页面显示高风险提示。
- 禁用批量确认合并。
- 只能进入单项详情手动确认。
- 二次确认文案必须包含工作区名、源分支名和目标分支名。
- 审计 metadata 增加 `highRiskTargetBranch: true`。

权限提示策略：

- 本地分支合并阶段主要校验本地工作区和 Git 命令结果。
- `确认合并` 首版只执行本地 merge，不自动 push。
- 本地 merge 成功后，页面显示 `推送远程` 按钮。
- 点击 `推送远程` 时再检测远程权限并执行 push。
- 如果远程平台或仓库策略不允许对应分支合并、推送或 PR/MR 合并，后端需要识别 Git/Provider 返回的权限错误并转成用户可读提示。
- 常见提示包括：
  - “当前凭证没有合并到目标分支的权限”。
  - “目标分支受保护，请通过 PR/MR 或管理员审批合并”。
  - “当前凭证没有推送目标分支权限，合并已在本地完成但无法推送远程”。
- 后续接入 GitHub/GitLab/GitCode/Gitee Provider API 后，可在审查前读取分支保护规则和当前会话权限，提前显示“可合并/不可合并/需要 PR”的状态。
- 高风险目标分支规则后续允许在策略设置中维护。

---

## 6. 数据模型设计

### 6.1 code_review_tasks

```sql
CREATE TABLE IF NOT EXISTS code_review_tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_key TEXT NOT NULL UNIQUE,
  workspace_key TEXT NOT NULL,
  workspace_name TEXT NOT NULL,
  repo_path TEXT NOT NULL,
  source_branch TEXT NOT NULL,
  target_branch TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft',
  risk_level TEXT NOT NULL DEFAULT 'unknown',
  merge_base TEXT NOT NULL DEFAULT '',
  source_head TEXT NOT NULL DEFAULT '',
  target_head TEXT NOT NULL DEFAULT '',
  push_status TEXT NOT NULL DEFAULT 'not_requested',
  diff_stat_json TEXT NOT NULL DEFAULT '{}',
  changed_files_json TEXT NOT NULL DEFAULT '[]',
  commit_list_json TEXT NOT NULL DEFAULT '[]',
  diff_excerpt_json TEXT NOT NULL DEFAULT '[]',
  ai_provider TEXT NOT NULL DEFAULT '',
  ai_model TEXT NOT NULL DEFAULT '',
  ai_review_markdown TEXT NOT NULL DEFAULT '',
  ai_review_json TEXT NOT NULL DEFAULT '{}',
  batch_key TEXT NOT NULL DEFAULT '',
  created_by TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  merged_at TEXT DEFAULT NULL,
  error_message TEXT NOT NULL DEFAULT ''
);
```

状态枚举：

- `draft`
- `diff_ready`
- `reviewing`
- `review_ready`
- `merge_pending`
- `merged`
- `merge_failed`
- `conflict`
- `stale`
- `cancelled`

风险枚举：

- `unknown`
- `low`
- `medium`
- `high`
- `critical`

推送状态枚举：

- `not_requested`
- `pushing`
- `pushed`
- `push_failed`

说明：

- `status = merged` 只表示本地已合并。
- 远程是否已经更新由 `push_status` 单独表达。
- 列表中使用组合状态展示，例如“本地已合并 / 远程未推送”“本地已合并 / 推送失败”。
- 首版不长期保存完整 diff，只保存 diff stat、文件列表、提交列表、必要的截断 diff 摘要和 AI 审查报告。
- 完整 diff 查看优先根据任务快照从本地仓库实时重新计算；如果本地快照已不可用，则提示“完整 diff 已无法从当前工作区重建”。

### 6.2 code_review_batches

`code_review_batches` 只记录批量解析和批量执行汇总，不承担跨仓库事务语义。每个仓库的实际审查、AI 报告和合并状态仍落在独立 `code_review_tasks`。

```sql
CREATE TABLE IF NOT EXISTS code_review_batches (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_key TEXT NOT NULL UNIQUE,
  raw_text TEXT NOT NULL,
  parsed_json TEXT NOT NULL DEFAULT '{}',
  status TEXT NOT NULL DEFAULT 'parsed',
  total_count INTEGER NOT NULL DEFAULT 0,
  success_count INTEGER NOT NULL DEFAULT 0,
  failed_count INTEGER NOT NULL DEFAULT 0,
  created_by TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
```

---

## 7. Rust 模型与 Command 设计

### 7.1 主要模型

```rust
pub struct CodeReviewTask {
    pub id: i64,
    pub task_key: String,
    pub workspace_key: String,
    pub workspace_name: String,
    pub repo_path: String,
    pub source_branch: String,
    pub target_branch: String,
    pub status: String,
    pub risk_level: String,
    pub merge_base: String,
    pub source_head: String,
    pub target_head: String,
    pub push_status: String,
    pub diff_stat: serde_json::Value,
    pub changed_files: Vec<CodeReviewChangedFile>,
    pub commits: Vec<CodeReviewCommit>,
    pub diff_excerpt: serde_json::Value,
    pub ai_provider: String,
    pub ai_model: String,
    pub ai_review_markdown: String,
    pub ai_review_json: serde_json::Value,
    pub batch_key: String,
    pub error_message: String,
}
```

### 7.2 Command 清单

```rust
#[tauri::command]
pub async fn create_code_review_task(input: CreateCodeReviewTaskInput) -> Result<CodeReviewTask, CommandError>

#[tauri::command]
pub async fn prepare_code_review_diff(task_key: String) -> Result<CodeReviewTask, CommandError>

#[tauri::command]
pub async fn run_code_review_ai(task_key: String, provider_key: Option<String>) -> Result<CodeReviewTask, CommandError>

#[tauri::command]
pub async fn merge_code_review_task(task_key: String) -> Result<CodeReviewTask, CommandError>

#[tauri::command]
pub async fn parse_code_review_batch(input: ParseCodeReviewBatchInput) -> Result<CodeReviewBatchParseResult, CommandError>

#[tauri::command]
pub async fn create_code_review_batch_tasks(input: CreateCodeReviewBatchTasksInput) -> Result<Vec<CodeReviewTask>, CommandError>

#[tauri::command]
pub fn list_code_review_tasks(input: ListCodeReviewTasksInput) -> Result<Vec<CodeReviewTask>, CommandError>

#[tauri::command]
pub fn get_code_review_task(task_key: String) -> Result<CodeReviewTask, CommandError>
```

---

## 8. AI 审查设计

### 8.1 Prompt 输入

后端传给 AI 的上下文必须包含：

- 项目名称。
- 仓库路径摘要，不传本地敏感绝对路径给模型时可只传仓库名。
- 源分支。
- 目标分支。
- merge-base。
- 提交列表。
- 文件变更列表。
- diff 片段。
- 项目类型推断：
  - 前端 / 后端 / Java / Node / Rust / Go / Python / 配置。
- 已启用 Skill 和经验库召回内容。

Skill 和经验库注入策略：

- 只注入与 `git`、`code-review`、当前项目技术栈相关的 Skill。
- 不把所有 Skill 全量注入 prompt，避免上下文污染。
- 经验库召回最多 3-5 条。
- 每条经验只注入标题、适用范围和简短摘要。
- 页面展示本次注入的 Skill 和经验条目，方便用户判断 AI 审查依据。
- 用户可在重新审查前关闭某条经验或 Skill 注入。

AI 输入边界：

- 首版默认只传 diff、提交信息和文件变更元数据，不默认读取完整文件内容。
- 对于小文件或关键文件，可在后续增强中由用户手动选择加入有限上下文后重新审查。
- 对于超大文件，只传 diff 片段、文件路径和变更统计。
- 不向 AI 传递 `.env`、密钥、证书、私钥、Token 文件、凭证配置或疑似敏感字段内容。
- 如果 diff 中出现疑似敏感值，后端应优先脱敏后再传给 AI。

### 8.2 Prompt 输出格式

要求 AI 同时输出 Markdown 和结构化 JSON：

```json
{
  "riskLevel": "medium",
  "summary": "本次变更主要...",
  "blockingIssues": [
    {
      "file": "src/xxx.ts",
      "severity": "high",
      "title": "可能的空指针",
      "reason": "...",
      "suggestion": "..."
    }
  ],
  "warnings": [],
  "testSuggestions": [],
  "mergeRecommendation": "allow_with_attention"
}
```

重新审查策略：

- 首版只保留最新一次 AI 审查结果。
- `run_code_review_ai` 会覆盖当前任务的 `ai_review_markdown` 和 `ai_review_json`。
- 每次重新审查都写审计日志，记录 Provider、模型、耗时和触发来源。
- 不新增 AI 审查历史版本表；后续增强可增加 `code_review_ai_runs` 保存多版本报告。

### 8.3 审查维度

- 编译风险。
- 运行时异常。
- API 兼容性。
- 数据库变更风险。
- 配置变更风险。
- 安全风险。
- 性能风险。
- 并发/事务风险。
- 前后端接口契约不一致。
- 测试覆盖建议。

测试与编译策略：

- 首版 AI 审查不自动执行测试、编译或构建命令。
- AI 可以输出建议运行的检查命令，例如 `npm test`、`mvn test`、`cargo test`。
- 页面展示建议命令，用户可复制或后续通过终端/Runbook 执行。
- 后续可为 Git 工作区配置“检查命令”，由用户手动触发。
- 检查命令属于可执行命令，必须经过危险命令黑名单和审计。

### 8.4 AI Gate 策略

AI 审查只提供风险建议，不直接执行合并。

- AI 结果为 `critical` 或 `high`：
  - 默认禁用批量确认合并。
  - 用户可以进入单项详情查看风险并手动确认。
  - 手动确认必须二次确认，并记录高风险人工确认审计。
- AI 结果为 `medium`、`low` 或 `unknown`：
  - 允许显示确认合并按钮。
  - 仍必须由用户点击确认后才执行合并。
- 不允许“AI 自动通过后自动合并”。
- MCP 场景必须走 `controlled -> approved`，不能由单个工具调用直接完成合并。

### 8.5 大 diff 截断策略

为了避免超出模型上下文：

- 每个文件最多传 400 行 diff。
- 总 diff 最多传 8000 行或 300KB。
- 超限时优先保留：
  - 源码文件。
  - 配置文件。
  - SQL / migration。
  - 接口定义。
- 低优先级忽略：
  - lock 文件。
  - dist/build 产物。
  - 图片/二进制文件。

---

## 9. 批量文本解析设计

### 9.1 规则解析优先

先用规则解析常见表达：

- `将 A、B、C 的 x 分支合并 y 分支`
- `A 项目 x 合 dev`
- `前端项目：A 分支：x`
- `后端项目：A、B、C 分支：x`
- `合个 dev`
- `合并到 dev`
- `目标分支：dev`

### 9.2 AI 解析兜底

当规则解析结果不完整时调用 AI，要求输出严格 JSON：

```json
{
  "targetBranch": "dev",
  "items": [
    {
      "projectName": "fj-evaluate-app",
      "sourceBranch": "dev-v3.7",
      "targetBranch": "dev",
      "group": "frontend"
    }
  ],
  "confidence": 0.92,
  "warnings": []
}
```

AI 解析约束：

- AI 解析结果不能直接执行。
- 所有解析结果必须先展示在可编辑任务表中。
- 用户必须点击 `确认解析结果` 后，系统才允许创建批量审查任务。
- 规则解析已有结果时，AI 可以做校验或补全，但不能静默覆盖用户可见结果。
- AI 输出必须是严格 JSON；解析失败时提示用户手动编辑。
- `confidence < 0.85` 的任务必须标记为待确认。

### 9.3 工作区匹配规则

项目名到 Git 工作区匹配顺序：

1. `workspace.name` 完全匹配。
2. `repo_path` 最后一级目录完全匹配。
3. `remote_url` 仓库名匹配。
4. 忽略大小写匹配。
5. 模糊匹配并要求用户确认。

匹配失败时不允许自动执行，必须由用户手动选择工作区。

多候选处理：

- 只有完全唯一匹配时才允许自动绑定工作区。
- 如果同一个项目名匹配到多个 Git 工作区，不自动选择任何一个，任务状态显示为“待选择”。
- 前端展示候选工作区列表，包括工作区名称、仓库路径摘要、remote URL 摘要、绑定凭证状态和当前分支。
- 用户手动选择后，可在当前 batch 内记忆该项目名到 `workspace_key` 的映射，方便同批次后续任务复用。
- 首版不写入永久偏好；后续可新增“项目名 -> workspace_key”映射表。

---

## 10. 安全与审计设计

### 10.1 安全边界

- AI 只能看到 diff、提交信息、文件路径和脱敏项目元数据。
- AI 不能看到 Git Token、SSH Key、密码。
- Git 远程访问由后端 Git 工作区凭证注入完成。
- 合并操作必须来自固定命令模板。
- 批量合并必须逐项记录。

### 10.2 审计事件

新增审计 action：

- `code_review_task_create`
- `code_review_diff_prepare`
- `code_review_ai_run`
- `code_review_merge_confirm`
- `code_review_merge_success`
- `code_review_merge_failed`
- `code_review_merge_conflict`
- `code_review_merge_stale`
- `code_review_push_success`
- `code_review_push_failed`
- `code_review_batch_parse`
- `code_review_batch_create`

审计摘要包括：

- 工作区 Key。
- 源分支。
- 目标分支。
- 风险等级。
- AI Provider / 模型。
- 是否批量任务。
- 结果。

---

## 11. MCP 工具规划

首版页面功能优先，MCP 后置到第六阶段实现。第 1-5 阶段不开放 MCP 合并能力，避免在 UI 状态机和审批链路成熟前让外部 AI agent 触发高风险操作。

建议 MCP 工具：

- `code_review_workspaces_list`
- `code_review_task_create`
- `code_review_diff_prepare`
- `code_review_ai_run`
- `code_review_task_get`
- `code_review_batch_parse`
- `code_review_batch_tasks_create`
- `code_review_merge_controlled`
- `code_review_merge_approved`

策略：

- 只读工具可直接返回脱敏数据。
- MCP 第一版优先开放只读和审查类工具：工作区列表、创建审查任务、准备 diff、运行 AI 审查、读取审查报告。
- `merge_controlled` 只创建审批请求。
- `merge_approved` 校验审批、payload hash、工作区和分支一致后执行。
- 不允许单个 MCP 工具调用直接完成真实合并。
- MCP 合并能力必须依赖已经成熟的 `code_review_tasks` 状态机和审批队列。

---

## 12. 实施步骤

### 第一阶段：基础数据与后端能力

- [ ] 新增数据库表 `code_review_tasks`。
- [ ] 新增数据库表 `code_review_batches`。
- [ ] 新增 Rust 模型。
- [ ] 新增 `CodeReviewService`。
- [ ] 新增 `commands::code_review`。
- [ ] 新增固定 Git diff/branch/commit 读取能力。
- [ ] 在 `CodeReviewService` 内实现受控合并流程，不直接绕过审查任务状态机调用普通 merge。
- [ ] 新增 Command 注册。
- [ ] 新增前端 API 封装。

验收标准：

- 能创建审查任务。
- 能读取源/目标分支差异。
- 能保存审查任务状态。

### 第二阶段：单项目审核页面

- [ ] 将侧边栏一级菜单 `安全凭证` 调整为 `安全`。
- [ ] 新增 `安全 -> 代码审核` 菜单。
- [ ] 新增 `/secure-credentials/code-review` 路由。
- [ ] 新增独立页面 `src/pages/secure-credentials/code-review.tsx`。
- [ ] 新增 `src/types/codeReview.ts`。
- [ ] 新增 `src/lib/api/codeReview.ts`。
- [ ] 实现 Git 工作区选择。
- [ ] 实现源/目标分支选择。
- [ ] 展示最后提交和工作区状态。
- [ ] 展示文件变更列表和 diff 预览。

验收标准：

- 用户能完成单项目 diff 预览。
- 大 diff 能截断并提示。

### 第三阶段：AI 审查

- [ ] 构建 AI 审查 prompt。
- [ ] 接入 `AiProviderService`。
- [ ] 保存 Markdown 审查结果。
- [ ] 保存结构化风险结果。
- [ ] 页面展示审查报告。
- [ ] 审计 AI 审查调用。

验收标准：

- AI 能基于真实 diff 给出审查报告。
- 报告能落库并重新查看。

### 第四阶段：确认合并

- [ ] 实现合并前二次确认。
- [ ] 合并前重新校验工作区干净。
- [ ] 合并前重新校验源分支 HEAD、目标分支 HEAD 和 merge-base，防止审查后分支漂移。
- [ ] checkout 目标分支。
- [ ] merge 源分支。
- [ ] 处理冲突状态。
- [ ] 合并成功后刷新工作区状态。
- [ ] 合并成功后提供单独的推送远程入口。
- [ ] 写审计日志。

验收标准：

- 用户确认后能完成无冲突合并。
- 冲突时能清晰展示失败原因和冲突文件。

### 第五阶段：批量文本解析

- [ ] 实现规则解析器。
- [ ] 实现 AI 解析兜底。
- [ ] 实现项目名到 Git 工作区匹配。
- [ ] 展示可编辑任务表。
- [ ] 支持批量创建审查任务。
- [ ] 支持批量生成审查。
- [ ] 支持选中项确认合并。

验收标准：

- 能正确解析示例图片文字中的 5 个项目合并任务。
- 匹配失败项必须停留在待用户选择状态。

### 第六阶段：MCP 与策略增强

- [ ] 新增 MCP 只读工具。
- [ ] 新增合并受控工具。
- [ ] 接入审批队列。
- [ ] 接入策略配置。
- [ ] 接入审查记录导出。

验收标准：

- MCP 可创建审查任务和读取审查报告。
- MCP 不能直接绕过用户确认执行合并。

---

## 13. 验收用例

### 13.1 单项目正常合并

前置：

- 工作区存在。
- 源分支和目标分支存在。
- 工作区干净。

步骤：

1. 选择工作区。
2. 选择源分支。
3. 选择目标分支。
4. 生成审查。
5. 查看 diff 和 AI 报告。
6. 确认合并。

预期：

- 审查任务状态变为 `merged`。
- 工作区分支为目标分支。
- 不自动推送远程。
- 页面展示 `推送远程` 操作。
- 审计日志有完整记录。

### 13.2 审查后分支发生变化

前置：

- 已生成审查任务。
- 审查任务保存了 `source_head`、`target_head` 和 `merge_base`。
- 用户确认合并前，源分支或目标分支被新的提交更新。

预期：

- 禁止合并。
- 任务状态变为 `stale`。
- 提示需要重新生成 diff 和 AI 审查。
- 审计记录 `code_review_merge_stale`。

### 13.3 批量解析示例

输入：

```text
前端项目：fj-evaluate-app 分支：dev-v3.7
后端项目：fj-evaluate、fj-evaluate-service、fj-db-service、fj-feb 分支：dev-过错认定v3.7
都合并 dev 分支
```

预期：

- 生成 5 个任务。
- `fj-evaluate-app` 源分支为 `dev-v3.7`。
- 其他 4 个项目源分支为 `dev-过错认定v3.7`。
- 目标分支均为 `dev`。

### 13.4 工作区不干净

### 13.4 批量逐项独立执行

前置：

- 批量任务包含 5 个项目。
- 其中 3 个项目可以无冲突合并，1 个项目分支 stale，1 个项目合并冲突。

预期：

- 3 个项目状态变为 `merged`。
- stale 项状态变为 `stale`。
- 冲突项状态变为 `conflict`。
- 系统不自动回滚已经成功的 3 个项目。
- 批量结果报告展示成功 3、stale 1、冲突 1。

### 13.5 工作区不干净

预期：

- 禁止切换分支或合并。
- 提示未提交文件。
- 不执行破坏性操作。

### 13.6 分支不存在

预期：

- 标记任务为失败。
- 提示源分支或目标分支不存在。
- 不执行合并。

### 13.7 合并冲突

预期：

- 任务状态变为 `conflict`。
- 展示冲突文件列表。
- 审计记录 `code_review_merge_conflict`。

---

## 14. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| AI 审查遗漏问题 | 用户误合并 | 明确 AI 仅辅助，保留人工确认；高风险文件提示重点复核 |
| 大 diff 超上下文 | 审查不完整 | 分片审查、文件优先级、截断提示 |
| 批量解析错误 | 合并错项目或错分支 | 解析结果必须可编辑，低置信度必须用户确认 |
| Git 冲突 | 工作区进入冲突状态 | 明确冲突状态，提供中止合并按钮 |
| 本地工作区脏数据 | 覆盖用户改动 | 合并前强制干净工作区校验 |
| 审查后分支漂移 | AI 审查内容与实际合并内容不一致 | 合并前强制校验 source_head、target_head 和 merge_base，不一致则标记 stale 并要求重新审查 |
| 凭据泄露 | 安全事故 | 凭据只在后端注入，AI 和 MCP 不接触明文 |
| 多项目批量失败 | 执行不一致 | 每项独立状态，失败不阻断已完成项，结果报告汇总 |

---

## 15. 推荐实现顺序

推荐先做页面内闭环，再做 MCP：

1. 单项目 diff + AI 审查。
2. 单项目确认合并。
3. 审查记录持久化。
4. 批量文本解析。
5. 批量审查与合并。
6. MCP 工具与审批增强。

原因：

- 用户主要入口是桌面 UI。
- 单项目闭环是批量能力的基础。
- MCP 合并必须依赖已成熟的审查任务和审批状态。

---

## 16. 已确认设计决策

| 序号 | 决策项 | 已确认方案 |
|------|--------|------------|
| 1 | 审查后防漂移 | 合并前强制校验 `source_head`、`target_head`、`merge_base`，不一致则任务进入 `stale`，禁止合并并要求重新审查。 |
| 2 | 批量执行语义 | 批量合并逐项独立执行，不做跨仓库事务，某项失败不回滚其他已成功项。 |
| 3 | Git 合并策略 | 首版允许 fast-forward，不强制 `--no-ff` 生成合并提交。 |
| 4 | AI Gate | AI 只给审查建议，不能自动合并；所有合并必须用户确认，高风险默认禁用批量确认。 |
| 5 | Diff 基准 | 主审查 diff 使用 `merge-base(source, target)..source`，不使用 `target..source` 作为主审查 diff。 |
| 6 | 多候选工作区匹配 | 项目名匹配到多个 Git 工作区时不自动选择，必须用户手动确认。 |
| 7 | 合并前 fetch | 确认合并前再次执行 `git fetch --prune`，fetch 后快照变化则进入 `stale`。 |
| 8 | 高风险目标分支 | `master`、`main`、`release/*`、`prod/*`、`production` 默认高风险，禁止批量确认，只能单项详情确认。 |
| 9 | 分支权限提示 | 当凭证没有合并、推送或 PR/MR 权限时，必须给出明确中文提示。 |
| 10 | 合并与推送拆分 | `确认合并` 只执行本地 merge；合并成功后另行展示 `推送远程` 按钮。 |
| 11 | 推送状态 | 新增 `push_status`，`status = merged` 只表示本地已合并，远程同步状态单独展示。 |
| 12 | AI 输入边界 | 首版默认只传 diff、提交信息和文件变更元数据，不默认读取完整文件内容。 |
| 13 | 测试与编译 | 首版不自动执行测试、编译、构建命令，只展示 AI 建议检查命令。 |
| 14 | 预合并分支 | 首版不创建临时预合并分支；冲突后提供冲突提示和 `中止合并` 按钮。 |
| 15 | Diff 持久化 | 首版不长期保存完整 diff，只保存摘要、文件列表、提交列表和 AI 报告。 |
| 16 | 菜单命名 | 一级菜单 `安全凭证` 调整为 `安全`，代码审核放在 `安全 -> 代码审核`。 |
| 17 | 首版仓库范围 | 首版仅支持本地 Git 工作区，不直接通过远程 Provider API 选择仓库审查或合并。 |
| 18 | 工作区干净校验 | 创建审查、生成 diff、确认合并前都必须要求工作区干净；不自动 stash、提交或丢弃。 |
| 19 | Skill/经验注入 | 只注入相关 Skill 和最多 3-5 条经验，页面展示本次注入内容。 |
| 20 | 批量 AI 解析 | AI 解析结果不能直接执行，必须展示可编辑结果并由用户确认。 |
| 21 | 前端拆分 | 代码审核使用独立页面、类型和 API 文件，不继续扩张 `secure-credentials/index.tsx`。 |
| 22 | 后端拆分 | 新增 `CodeReviewService` 和 `commands::code_review`，不把代码审核业务塞进 `git_workspace.rs`。 |
| 23 | MCP 节奏 | MCP 后置到第六阶段，第一版优先只读和审查类工具，合并必须走审批。 |
| 24 | 报告输出 | 首版支持复制 Markdown 审查报告，文件导出后续增强。 |
| 25 | 重新审查 | 首版只保留最新 AI 审查结果，重新审查覆盖当前报告并写审计日志。 |
