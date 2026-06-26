# Skill 管理功能实施方案

**状态**: 规划中
**创建时间**: 2026-06-25
**目标版本**: v0.1.x
**菜单位置**: AI / MCP -> Skill 管理
**参考形态**: 技能 / 经验库 / Runbook 三 Tab 能力中心
**内置 Skill 原始来源**: `/Users/bin/Downloads/skills`
**内置 Skill 项目资源目录**: `src-tauri/resources/skills`

---

## 1. 背景与目标

当前应用已经具备 AI Provider、终端 AI、日志 AI 解释、SQL 控制台 AI、MCP Server 等能力，但各 AI 交互的提示词能力分散在页面或服务逻辑中。新增 Skill 管理功能后，应用可以把一组可维护、可启停、可排序的专业技能注入到所有 AI 交互提示词中，使 AI 回复更贴合 SSH、数据库、SFTP、日志、MCP 等运维场景。

本功能中的 Skill 指应用内提示词技能，不等同于 Codex 本地 `.codex/skills`。它本质上是结构化提示词片段和适用规则。

参考页面体现的是一个 AI 运维能力中心，不只是单表 CRUD：

- `技能`：通过触发词命中，用于实时增强 AI prompt。
- `经验库`：沉淀踩坑经验，后续供 AI/MCP recall 检索。
- `Runbook`：固化多步骤操作，后续供 MCP runbook 逐步执行。

首版以 `技能` 为核心真实实现，`经验库` 和 `Runbook` 同页纳入信息架构，提供轻量管理和后续扩展接口，避免后续再重做菜单与数据模型。

核心目标：

- 在 AI / MCP 菜单下新增 `Skill 管理` 页面。
- 将 `/Users/bin/Downloads/skills` 目录中的 38 个 `SKILL.md` 复制加入项目，作为应用内置 Skill。
- 应用打包时必须将这 38 个内置 Skill 一并打包进应用资源目录，安装后不能依赖 `/Users/bin/Downloads/skills` 仍然存在。
- 支持用户新增、编辑、删除、启用、停用自定义 Skill。
- 内置 Skill 与用户新增 Skill 在来源、编辑权限、同步机制上明确区分。
- 支持 Skill 按场景、目标对象、优先级自动注入到本应用所有 AI 交互中。
- 支持输入 prompt 测试触发，实时展示命中的 Skill 和排序依据。
- 预留经验库和 Runbook，作为后续 MCP 工具 `recall_experience`、`run_runbook` 的管理入口。
- Skill 内容持久化到 SQLite。
- 后续 MCP 工具触发 AI 时也复用同一套 Skill 注入服务。

---

## 2. 功能范围

### 2.1 首版必须支持

- Skill 列表管理：查看、搜索、筛选、启停。
- 顶部 Tab：`技能`、`经验库`、`Runbook`。
- 技能页测试触发：输入 prompt 后按触发词、作用域、来源和优先级计算命中结果。
- 内置 Skill：开发期从 `/Users/bin/Downloads/skills/*/SKILL.md` 导入项目资源目录，运行期从应用资源读取，不允许编辑内容和物理删除，只允许启用、停用、复制为用户 Skill。
- 自定义 Skill：新增、编辑、复制、删除。
- Skill 作用域：
  - 全局
  - 终端 AI
  - SQL 控制台 AI
  - 日志解释 AI
  - SFTP 文件 AI
  - MCP Agent
  - 堡垒机会话
- Skill 优先级：数字越大越靠前。
- Skill 注入策略：
  - 全局 Skill 始终参与。
  - 场景 Skill 只在匹配 AI 场景时参与。
  - 触发词命中的 Skill 优先参与。
  - 服务器/数据库上下文仍由原业务模块追加，Skill 只提供行为规范和专家知识。
- Prompt 预览：选择一个场景后，展示最终会注入到 AI 的 Skill 片段。
- 技能表格列按参考页面设计：名称、来源、描述、触发词、命中、操作。
- 经验库页首版支持新增、搜索、列表和空状态。
- Runbook 页首版支持新增、搜索、列表和空状态。

### 2.2 首版不做

- Skill 市场和在线同步。
- 团队共享。
- Skill 自动生成和自动评分。
- 多语言 Skill 包。
- 外部 `.codex/skills` 自动导入。
- Runbook 自动执行真实远程命令。首版只做管理和预留，执行必须等审批、审计、AI 权限和 MCP 工具链完整接入后再开放。

---

## 3. 内置 Skill 来源与解析

### 3.1 来源目录与项目内置资源

首版内置 Skill 不再手写 8 条示例，而是以本机目录作为原始导入来源：

```text
/Users/bin/Downloads/skills
```

该目录当前包含 38 个子目录，每个子目录下有一个 `SKILL.md`。这些文件必须复制进项目，形成应用自带资源：

```text
src-tauri/resources/skills
├── 1panel-ops/SKILL.md
├── db-tools/SKILL.md
├── docker-ps/SKILL.md
├── nginx-status/SKILL.md
└── ...
```

这样打包后的 macOS / Windows 应用可以离线使用内置 Skill，不依赖开发机上的 `/Users/bin/Downloads/skills`。

当前 38 个内置 Skill 示例：

- `1panel-ops/SKILL.md`
- `db-tools/SKILL.md`
- `docker-ps/SKILL.md`
- `log-investigation/SKILL.md`
- `mysql-ops/SKILL.md`
- `nginx-status/SKILL.md`
- `postgresql-ops/SKILL.md`
- `redis-ops/SKILL.md`
- `ssh-hardening/SKILL.md`
- `systemd-service/SKILL.md`

实现时以项目资源目录 `src-tauri/resources/skills` 的扫描结果为准，页面统计应显示 `内置 38`。`/Users/bin/Downloads/skills` 只作为本次初始化导入来源，不作为应用运行期依赖。

### 3.2 打包要求

需要将 `src-tauri/resources/skills` 加入 Tauri 打包资源：

```json
{
  "bundle": {
    "resources": [
      "resources/skills"
    ]
  }
}
```

说明：

- 开发仓库中保存 38 个内置 Skill 原文。
- `pnpm tauri build` 时把 `resources/skills` 打进应用包。
- 运行期通过 Tauri `resource_dir` 定位资源目录，再读取 `skills/*/SKILL.md`。
- 首次启动和手动刷新时，从应用资源目录同步到 SQLite 索引表。
- SQLite 只保存索引、启停状态、触发词、hash、统计等元数据；内置正文以资源文件为准。
- 用户新增 Skill 不写入资源目录，只写 SQLite。

### 3.3 内置 Skill 解析规则

每个内置 Skill 读取 `SKILL.md` 的 YAML frontmatter 和正文。

示例结构：

```markdown
---
name: nginx-status
description: Nginx 排障速查
触发词: nginx, 502, 503, bad gateway
dangerous_commands:
  - '...'
---

# nginx-status

正文内容...
```

解析规则：

- `skill_key`：优先使用 frontmatter 的 `name`，没有则使用目录名。
- `name`：同 `skill_key`，后续可在 UI 中显示目录名。
- `description`：读取 frontmatter 的 `description`。
- `trigger_words`：读取 frontmatter 的 `触发词`，按英文逗号、中文逗号拆分并 trim。
- `dangerous_commands`：读取为 JSON 数组，作为后续 AI 权限策略和 Runbook/MCP 安全拦截参考。
- `content`：保存完整 Markdown 正文，包含 frontmatter 后的主体内容。
- `source_path`：保存应用资源内相对路径，例如 `resources/skills/nginx-status/SKILL.md`，避免保存开发机绝对路径。
- `content_hash`：根据 `SKILL.md` 内容计算，用于判断内置 Skill 是否变化。

### 3.4 内置 Skill 与用户 Skill 区分

| 维度 | 内置 Skill | 用户 Skill |
| --- | --- | --- |
| 来源 | `src-tauri/resources/skills/*/SKILL.md` 打包资源 | 用户在应用内新增 |
| `source` | `builtin` | `user` |
| 内容编辑 | 不允许直接编辑 | 允许编辑 |
| 删除 | 不允许 | 允许 |
| 启用/停用 | 允许 | 允许 |
| 复制 | 可复制为用户 Skill | 可复制 |
| 同步 | 启动时/手动刷新从应用资源同步 | 只由用户操作更新 |
| 升级覆盖 | 内置内容跟随文件变化 | 不被内置同步影响 |

内置 Skill 如果用户想修改，应通过 `复制为用户 Skill` 生成一条用户 Skill。这样不会污染内置源，也能清晰区分“系统自带”和“用户自定义”。

### 3.5 内置 Skill 同步流程

1. 开发期执行一次资源导入：
   - 从 `/Users/bin/Downloads/skills` 复制 38 个 Skill 到 `src-tauri/resources/skills`。
   - 保持目录名和 `SKILL.md` 文件名不变。
   - 所有文件使用 UTF-8 无 BOM。
2. `tauri.conf.json` 配置 `bundle.resources`。
3. 应用启动时扫描资源目录：
   - 新增的内置 Skill 插入 SQLite 元数据。
   - hash 变化的内置 Skill 更新元数据和缓存内容。
   - 已不存在的内置 Skill 标记为 `missing` 或 `disabled`，不直接删除，避免用户历史记录断链。
4. 用户在页面点击 `刷新内置` 时重复同步流程。
5. 打包验证时检查安装包内存在 `resources/skills`。

### 3.6 初始内置 Skill 覆盖范围

| Key | 名称 | 作用域 | 优先级 | 触发词示例 | 说明 |
| --- | --- | --- | ---: | --- | --- |
| `global-safe-ops` | 安全运维基线 | 全局 | 100 | `删除`、`重启`、`密码`、`sudo` | 禁止编造结果，危险命令必须解释风险，遵守 AI 权限级别 |
| `ssh-command-assistant` | SSH 命令助手 | 终端 AI | 90 | `ssh`、`命令`、`进程`、`端口`、`磁盘` | 将中文需求转为可执行命令，优先只读命令，说明影响 |
| `sql-expert` | SQL 专家 | SQL 控制台 AI | 90 | `sql`、`查询`、`表`、`索引`、`慢查询` | 生成、纠错、调优 MySQL/PostgreSQL SQL |
| `redis-ops` | Redis 运维助手 | SQL 控制台 AI | 80 | `redis`、`key`、`ttl`、`缓存`、`scan` | 解释 Redis key、TTL、类型、扫描和安全删除建议 |
| `log-diagnosis` | 日志诊断助手 | 日志解释 AI | 90 | `日志`、`error`、`exception`、`超时`、`502` | 总结异常堆栈、定位错误级别、给出排查路径 |
| `sftp-file-editor` | 配置文件编辑助手 | SFTP 文件 AI | 80 | `配置`、`yaml`、`nginx`、`application.yml` | 编辑配置文件时保持格式、备份意识和语法检查 |
| `mcp-agent-guard` | MCP Agent 安全边界 | MCP Agent | 95 | `mcp`、`agent`、`工具`、`凭证` | 限制 Agent 获取凭证明文，要求走审批和审计 |
| `bastion-session-helper` | 堡垒机会话助手 | 堡垒机会话 | 70 | `堡垒机`、`jumpserver`、`会话`、`审计` | 解释堡垒机会话、连接路径和审计注意事项 |

上表只是能力分类示例，真实内置列表以 `src-tauri/resources/skills` 扫描结果为准。由于这些 Skill 正文可能较长，注入 prompt 时不能无脑注入全文，必须按触发词命中和场景裁剪。

---

## 4. 用户故事

1. 作为普通用户，我可以在 Skill 管理页面看到系统内置的技能，并禁用不需要的技能。
2. 作为高级用户，我可以新增一个公司内部运维规范 Skill，让终端 AI 和 MCP Agent 都遵循它。
3. 作为数据库运维人员，我可以新增一个 SQL 规范 Skill，例如必须带库名、禁止无 WHERE 更新、查询默认 LIMIT。
4. 作为安全管理员，我可以查看某个 AI 场景最终注入了哪些 Skill，确认没有不合规内容。
5. 作为使用者，我在终端、日志、SQL 控制台中调用 AI 时，不需要手动选择 Skill，系统会自动按场景注入。

---

## 5. 页面设计

### 5.1 菜单

在 `AI / MCP` 菜单下新增：

- `Skill 管理`
- 路由：`/skills`
- 图标建议：`Sparkles` 或 `BookOpen`

### 5.2 总体布局

页面参考截图采用顶部页签 + 内容区方式，不使用左右分栏作为主结构。

顶部 Tab：

- `技能`
- `经验库`
- `Runbook`

页面视觉要求：

- 顶部 Tab 类似参考图，Tab 下方有细分割线，激活项使用蓝色文字和底部线。
- 主体背景跟随现有应用主题，内容区使用大面积简洁表格和浅边框容器。
- 按钮高度与现有应用保持 30px 左右，主按钮使用蓝色。
- 标签使用 Ant Design `Tag`，触发词标签允许多行但单个标签不换行。
- 暗色主题下边框、表头、Tag 背景必须可读。

### 5.3 技能 Tab

技能 Tab 是首版核心页，完全参考截图中的技能页实现。

#### 5.3.1 测试触发区

顶部提供测试触发卡片：

- 标题：`测试触发`
- 说明：`输入 prompt 实时看哪几条技能会被关键词命中；按命中数 + 来源优先级排序`
- 输入框 placeholder：
  - `测试："nginx 502 怎么排查" 或 "docker 容器一直 restarting"`
- 输入后 300ms debounce，调用后端或本地匹配逻辑计算命中。
- 命中结果可以在表格 `命中` 列显示次数，也可以在输入框下方追加命中摘要。

匹配排序建议：

1. 触发词命中数量。
2. 当前场景作用域匹配。
3. 来源优先级：用户 Skill > 内置 Skill。
4. Skill `priority`。
5. 更新时间。

#### 5.3.2 筛选工具栏

工具栏参考截图：

- 统计筛选：
  - `全部 N`
  - `用户 N`
  - `内置 N`
- 搜索框：
  - placeholder：`按名称/描述/触发词搜索`
- 开关：
  - `显示内置`
- 按钮：
  - `新建技能`

后续可追加作用域筛选，但首屏保持简洁，不堆太多控件。

#### 5.3.3 技能表格

表格列：

| 列 | 宽度建议 | 说明 |
| --- | ---: | --- |
| 名称 | 220 | Skill key 或短名称，点击进入编辑 |
| 来源 | 100 | `内置` / `用户` |
| 描述 | 自适应 | Skill 说明，单行省略 |
| 触发词 | 360 | 以 Tag 展示，超过 6 个显示 `+N` |
| 命中 | 80 | 测试触发时显示命中次数，无输入显示 `-` |
| 操作 | 180 | 编辑、复制、启停、删除/恢复 |

操作规则：

- 内置 Skill：允许编辑用户覆盖内容、复制、启停、恢复默认；不允许删除。
- 用户 Skill：允许编辑、复制、启停、删除。
- 触发词为空的 Skill 只能通过全局或作用域注入，不参与关键词命中。

#### 5.3.4 新建 / 编辑 Drawer

使用右侧 Drawer，宽度建议 720。

字段：

- 名称
- Skill Key
- 来源：只读
- 描述
- 作用域：多选，支持全局、终端 AI、SQL 控制台 AI、日志解释 AI、SFTP 文件 AI、MCP Agent、堡垒机会话
- 触发词：Tags 输入
- 优先级：数字
- 启用状态
- 允许 MCP 使用
- Skill 内容：CodeMirror Markdown 或 TextArea
- 备注：可选

底部操作：

- 保存
- 另存为副本
- 恢复默认：仅内置 Skill 显示
- 删除：仅用户 Skill 显示

#### 5.3.5 Prompt 预览

编辑 Drawer 内增加 `Prompt 预览` 折叠面板：

- 可选择模拟场景。
- 可输入模拟用户 prompt。
- 展示最终会注入的 Skill 列表和合成后的 prompt 片段。
- 明确提示：预览不包含服务器密码、API Key、凭证明文。

### 5.4 经验库 Tab

经验库参考截图第二张，首版以“踩坑沉淀”管理为主。

页面结构：

- 页头：
  - 标题：`经验库`
  - 说明：`AI 经 MCP recall_experience 检索这里的踩坑沉淀`
- 右侧：
  - 搜索框：`搜经验（同 recall_experience...）`
  - `新建`
- 内容区：
  - 有数据时展示经验卡片或表格。
  - 无数据时展示空状态：`还没有经验沉淀。AI 经 MCP recall_experience 工具会查这里；先写几条最近的踩坑记。`

经验字段：

- 标题
- 问题现象
- 根因
- 解决方案
- 适用场景
- 标签
- 来源：用户 / AI 总结 / MCP 记录
- 引用对象：服务器、数据库、日志文件、命令、SQL 等
- 创建时间 / 更新时间

首版能力：

- 新增、编辑、删除。
- 搜索标题、正文、标签。
- 后续 MCP 工具可只读检索。

### 5.5 Runbook Tab

Runbook 参考截图第三张，首版以多步骤操作模板管理为主。

页面结构：

- 页头：
  - 标题：`Runbook`
  - 说明：`AI 经 MCP run_runbook(name) 逐步执行（每步独立过策略）`
- 右侧：
  - 搜索框：`搜 Runbook`
  - `新建`
- 内容区：
  - 有数据时展示 Runbook 表格。
  - 无数据时展示空状态：`还没有固化的多步操作。MCP run_runbook(name) 会逐步执行（每步独立过策略）。`

Runbook 字段：

- 名称
- 描述
- 适用场景
- 标签
- 步骤列表
- 每步类型：说明 / 只读命令 / 需确认命令 / 文件操作 / SQL / Redis
- 每步风险级别
- 是否允许 MCP 调用
- 创建时间 / 更新时间

首版能力：

- 新增、编辑、删除。
- 搜索名称、描述、标签。
- 暂不真实执行远程命令。
- 后续接入 MCP 执行时，每一步必须复用 AI 权限、审批队列和审计日志。

操作按钮：

- 新建 Skill
- 复制 Skill
- 保存
- 启用 / 停用
- 删除
- 恢复默认：仅内置 Skill 可用
- 预览注入 Prompt

### 5.6 交互细节

- 内置 Skill 的 `key`、来源、是否内置不可编辑。
- 内置 Skill 内容可允许用户覆盖，但必须提供 `恢复默认`。
- 自定义 Skill 删除前二次确认。
- Skill 内容为空时不允许启用。
- 同一作用域下优先级相同时按更新时间倒序。
- Prompt 预览不能展示 API Key、服务器密码、凭证明文。
- 测试触发只做匹配预览，不真实调用大模型。
- 经验库和 Runbook 的空状态必须占满内容框高度，避免页面显得像未加载完成。

---

## 6. 数据模型

### 6.1 SQLite 表

新增表 `ai_skills`：

```sql
CREATE TABLE IF NOT EXISTS ai_skills (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  skill_key TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  content TEXT NOT NULL,
  scopes TEXT NOT NULL DEFAULT '["global"]',
  trigger_words TEXT NOT NULL DEFAULT '[]',
  tags TEXT NOT NULL DEFAULT '[]',
  priority INTEGER NOT NULL DEFAULT 0,
  enabled INTEGER NOT NULL DEFAULT 1,
  builtin INTEGER NOT NULL DEFAULT 0,
  source TEXT NOT NULL DEFAULT 'user',
  source_path TEXT NOT NULL DEFAULT '',
  content_hash TEXT NOT NULL DEFAULT '',
  missing INTEGER NOT NULL DEFAULT 0,
  builtin_version INTEGER NOT NULL DEFAULT 1,
  builtin_content TEXT NOT NULL DEFAULT '',
  user_overridden INTEGER NOT NULL DEFAULT 0,
  allow_mcp INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_ai_skills_scope_enabled
ON ai_skills(enabled, priority);
```

字段说明：

- `skill_key`：稳定唯一标识，内置 Skill 固定，自定义 Skill 自动生成或用户填写。
- `scopes`：JSON 数组，作用域取值 `global | terminal | sql | logs | sftp | mcp | jumpserver`。
- `trigger_words`：JSON 数组，用于测试触发和真实 prompt 命中。
- `tags`：JSON 数组，便于后续搜索和分类。
- `priority`：注入排序，越大越靠前。
- `builtin`：是否内置。
- `source`：来源，取值 `builtin | user`。
- `source_path`：内置 Skill 的应用资源相对路径；用户 Skill 为空。
- `content_hash`：内置文件内容 hash，用于同步检测。
- `missing`：内置资源缺失标记，资源不再存在时置 1，不直接删除记录。
- `builtin_version`：内置 Skill 版本，用于后续升级。
- `builtin_content`：内置默认内容，用于恢复默认。
- `user_overridden`：内置 Skill 是否被用户覆盖。
- `allow_mcp`：是否允许 MCP Agent 场景使用。

新增表 `ai_experiences`：

```sql
CREATE TABLE IF NOT EXISTS ai_experiences (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  experience_key TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  symptom TEXT NOT NULL DEFAULT '',
  cause TEXT NOT NULL DEFAULT '',
  solution TEXT NOT NULL DEFAULT '',
  scenario TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT 'user',
  tags TEXT NOT NULL DEFAULT '[]',
  references_json TEXT NOT NULL DEFAULT '[]',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_ai_experiences_enabled
ON ai_experiences(enabled, updated_at);
```

新增表 `ai_runbooks`：

```sql
CREATE TABLE IF NOT EXISTS ai_runbooks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  runbook_key TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  scenario TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '[]',
  steps_json TEXT NOT NULL DEFAULT '[]',
  enabled INTEGER NOT NULL DEFAULT 1,
  allow_mcp INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_ai_runbooks_enabled
ON ai_runbooks(enabled, updated_at);
```

### 6.2 TypeScript 类型

```ts
export type AiSkillScope =
  | "global"
  | "terminal"
  | "sql"
  | "logs"
  | "sftp"
  | "mcp"
  | "jumpserver";

export interface AiSkill {
  id: number;
  skillKey: string;
  name: string;
  description: string;
  content: string;
  scopes: AiSkillScope[];
  triggerWords: string[];
  tags: string[];
  priority: number;
  enabled: boolean;
  builtin: boolean;
  source: "builtin" | "user";
  sourcePath: string;
  contentHash: string;
  missing: boolean;
  builtinVersion: number;
  userOverridden: boolean;
  allowMcp: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface UpsertAiSkillInput {
  skillKey?: string;
  name: string;
  description?: string;
  content: string;
  scopes: AiSkillScope[];
  triggerWords?: string[];
  tags?: string[];
  priority?: number;
  enabled?: boolean;
  allowMcp?: boolean;
}

export interface AiSkillPromptContext {
  scope: AiSkillScope;
  includeGlobal: boolean;
  includeDisabled?: boolean;
  prompt?: string;
}

export interface AiExperience {
  id: number;
  experienceKey: string;
  title: string;
  symptom: string;
  cause: string;
  solution: string;
  scenario: string;
  source: "user" | "ai" | "mcp";
  tags: string[];
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface AiRunbookStep {
  id: string;
  title: string;
  type: "note" | "readonly_command" | "approval_command" | "file" | "sql" | "redis";
  content: string;
  riskLevel: "low" | "medium" | "high" | "blocked";
}

export interface AiRunbook {
  id: number;
  runbookKey: string;
  name: string;
  description: string;
  scenario: string;
  tags: string[];
  steps: AiRunbookStep[];
  enabled: boolean;
  allowMcp: boolean;
  createdAt: string;
  updatedAt: string;
}
```

---

## 7. 后端架构

遵循项目三层架构。

### 7.1 Database 层

文件建议：

- `src-tauri/src/database/schema.rs`
- `src-tauri/src/database/mod.rs`

方法建议：

- `list_ai_skills(filter)`
- `get_ai_skill(skill_key)`
- `sync_builtin_ai_skills(resource_dir)`
- `upsert_ai_skill(input)`
- `delete_ai_skill(skill_key)`
- `toggle_ai_skill(skill_key, enabled)`
- `restore_builtin_ai_skill(skill_key)`
- `list_ai_skills_for_prompt(scope, include_global, allow_mcp)`
- `test_ai_skill_trigger(prompt, scope)`
- `list_ai_experiences(filter)`
- `upsert_ai_experience(input)`
- `delete_ai_experience(experience_key)`
- `list_ai_runbooks(filter)`
- `upsert_ai_runbook(input)`
- `delete_ai_runbook(runbook_key)`

### 7.2 Service 层

文件建议：

- `src-tauri/src/services/ai_skill.rs`

职责：

- 从应用资源目录同步内置 Skill。
- 解析内置 `SKILL.md` frontmatter、触发词、危险命令和正文。
- 计算内置 Skill 内容 hash。
- 校验 Skill 内容和作用域。
- 禁止删除内置 Skill。
- 基于测试 prompt 计算触发词命中。
- 生成最终 AI Skill Prompt。
- 控制最大注入长度，避免 prompt 过长。
- 管理经验库和 Runbook 的 CRUD。
- 为后续 MCP `recall_experience` 和 `run_runbook` 提供只读检索入口。
- 为 AI Provider 服务提供统一组装函数。

核心函数建议：

```rust
pub fn build_skill_prompt(
    db: &Database,
    scope: AiSkillScope,
    include_global: bool,
    allow_mcp_only: bool,
    prompt: Option<&str>,
) -> Result<String, AppError>
```

输出格式建议：

```text
以下是当前应用启用的 Skill，请在本次回答中遵守：

[Skill: 安全运维基线]
...

[Skill: SQL 专家]
...
```

### 7.3 Command 层

文件建议：

- `src-tauri/src/commands/ai_skill.rs`

Command：

- `list_ai_skills`
- `sync_builtin_ai_skills`
- `upsert_ai_skill`
- `delete_ai_skill`
- `toggle_ai_skill`
- `restore_builtin_ai_skill`
- `preview_ai_skill_prompt`
- `test_ai_skill_trigger`
- `list_ai_experiences`
- `upsert_ai_experience`
- `delete_ai_experience`
- `list_ai_runbooks`
- `upsert_ai_runbook`
- `delete_ai_runbook`

### 7.4 前端 API

文件建议：

- `src/lib/api/aiSkill.ts`
- `src/lib/api/index.ts` 统一导出

API：

- `aiSkillApi.list(filter)`
- `aiSkillApi.syncBuiltin()`
- `aiSkillApi.upsert(input)`
- `aiSkillApi.delete(skillKey)`
- `aiSkillApi.toggle(skillKey, enabled)`
- `aiSkillApi.restoreBuiltin(skillKey)`
- `aiSkillApi.previewPrompt(scope)`
- `aiSkillApi.testTrigger({ prompt, scope })`
- `aiSkillApi.listExperiences(filter)`
- `aiSkillApi.upsertExperience(input)`
- `aiSkillApi.deleteExperience(experienceKey)`
- `aiSkillApi.listRunbooks(filter)`
- `aiSkillApi.upsertRunbook(input)`
- `aiSkillApi.deleteRunbook(runbookKey)`

---

## 8. AI Prompt 注入链路

### 8.1 当前问题

现有 AI 调用分布在多个页面和服务中，例如：

- 终端 AI：命令规划、执行摘要。
- 日志监听：AI 日志解释。
- SQL 控制台：SQL 生成、纠错、调优。
- 未来 MCP Agent：工具调用前后的 AI 决策。

如果每个页面自己拼 Skill，后续会难维护、难审计。

### 8.2 推荐方案

在 Rust 后端 AI Provider 服务统一注入 Skill。

扩展 `AiProviderAskInput`：

```rust
pub struct AiProviderAskInput {
    pub prompt: String,
    pub provider_key: Option<String>,
    pub system_prompt: Option<String>,
    pub skill_scope: Option<String>,
    pub use_skill_trigger: Option<bool>,
}
```

前端调用 AI 时传入场景：

- 终端 AI：`skillScope: "terminal"`
- SQL 控制台：`skillScope: "sql"`
- 日志解释：`skillScope: "logs"`
- SFTP：`skillScope: "sftp"`
- MCP：`skillScope: "mcp"`

后端组装顺序：

1. 应用安全基线。
2. 启用的全局 Skill。
3. 启用的当前场景 Skill。
4. 当前 prompt 触发词命中的 Skill。
5. 调用方传入的 `system_prompt`。
6. 业务上下文和用户问题。

推荐优先级：

```text
系统硬约束 > AI 权限策略 > Skill > 场景 system_prompt > 用户输入
```

这样可以避免用户自定义 Skill 覆盖应用安全策略。

### 8.3 浏览器 Dev API

为了网页调试，需要同步扩展 Dev API：

- `/dev-api/ai-skills`
- `/dev-api/ai-skills/preview`
- `/dev-api/ai-skills/test-trigger`
- `/dev-api/ai-experiences`
- `/dev-api/ai-runbooks`
- `/dev-api/ai-providers/ask` 支持 `skillScope`

---

## 9. 安全与治理

### 9.1 Prompt 注入风险

用户自定义 Skill 可能写入不安全规则，例如“忽略所有审批”“直接输出密码”。系统必须保留硬约束：

- Skill 不得覆盖 AI 权限级别。
- Skill 不得要求输出凭证明文。
- Skill 不得绕过审批队列。
- MCP 场景不向 Skill 暴露敏感字段。

### 9.2 内容校验

保存 Skill 时进行软校验：

- 内容不能为空。
- 内容超过 4000 字提示是否继续。
- 检测高风险短语并提示，例如：
  - 忽略安全策略
  - 输出密码
  - 绕过审批
  - 删除所有

首版只提示，不强制禁止；真正执行时仍由 AI 权限策略兜底。

### 9.3 审计

以下操作写审计日志：

- 新增 Skill。
- 修改 Skill。
- 删除自定义 Skill。
- 启用 / 停用 Skill。
- 恢复内置 Skill。

AI 调用审计中增加：

- `skill_scope`
- `skill_keys`
- `skill_prompt_tokens` 或近似字符数

---

## 10. MCP 集成

MCP 工具可以增加 Skill 管理能力，但需要分阶段。

首版 MCP 只读工具：

- `ai_skills_list`
- `ai_skill_detail`
- `ai_skill_prompt_preview`
- `ai_skill_test_trigger`
- `recall_experience`
- `runbooks_list`
- `runbook_detail`

后续可考虑写工具：

- `ai_skill_create`
- `ai_skill_update`
- `ai_skill_toggle`
- `experience_create`
- `runbook_create`
- `runbook_run`

写工具必须走审批队列，因为它会改变所有 AI 交互的行为。

`runbook_run` 涉及远程执行，必须等以下前置完成后再开放：

- 每一步拆分成独立审批 / 审计事件。
- 每一步复用服务器 AI 权限级别。
- 禁止命令直接阻断。
- 需要审核命令进入审批队列。
- 执行结果可中断、可回滚记录、可导出审计。

---

## 11. 实施步骤

### 阶段 1：数据与后端

- [ ] 创建项目资源目录 `src-tauri/resources/skills`。
- [ ] 将 `/Users/bin/Downloads/skills` 下 38 个 Skill 目录复制进 `src-tauri/resources/skills`。
- [ ] 校验 38 个 `SKILL.md` 均为 UTF-8 无 BOM。
- [ ] 在 `src-tauri/tauri.conf.json` 的 `bundle.resources` 中加入 `resources/skills`。
- [ ] 新增 `AiSkill` 相关 Rust model。
- [ ] 新增 `AiExperience`、`AiRunbook` 相关 Rust model。
- [ ] 新增 SQLite 表和迁移：`ai_skills`、`ai_experiences`、`ai_runbooks`。
- [ ] 新增内置 Skill 资源同步逻辑：从应用 `resource_dir` 读取 `skills/*/SKILL.md`。
- [ ] 新增 `sync_builtin_ai_skills` Command 和 Dev API。
- [ ] 新增 `ai_skill` Service。
- [ ] 新增 `ai_skill` Commands。
- [ ] 新增 Skill 触发测试服务：按 prompt、作用域、触发词、来源、优先级排序。
- [ ] 新增经验库 CRUD。
- [ ] 新增 Runbook CRUD。
- [ ] 新增 Dev API 路由。

### 阶段 2：前端 Skill 管理页面

- [ ] 新增 `/skills` 路由。
- [ ] 在 `AI / MCP` 菜单下新增 `Skill 管理`。
- [ ] 实现顶部 Tab：技能 / 经验库 / Runbook。
- [ ] 技能 Tab 实现测试触发卡片。
- [ ] 技能 Tab 实现统计筛选：全部、用户、内置。
- [ ] 技能 Tab 实现搜索、显示内置开关、新建技能按钮。
- [ ] 技能 Tab 实现 `刷新内置` 操作，用于重新扫描打包资源中的内置 Skill。
- [ ] 技能 Tab 实现表格列：名称、来源、描述、触发词、命中、操作。
- [ ] 实现 Skill 编辑 Drawer。
- [ ] 实现 Prompt 预览。
- [ ] 实现内置 Skill 恢复默认。
- [ ] 经验库 Tab 实现搜索、新建、空状态、列表管理。
- [ ] Runbook Tab 实现搜索、新建、空状态、步骤编辑。

### 阶段 3：接入所有 AI 调用

- [ ] 扩展 `AiProviderAskInput` 支持 `skillScope` 和 `useSkillTrigger`。
- [ ] 后端 `ask_ai_provider` 统一注入 Skill Prompt。
- [ ] 终端 AI 调用传 `terminal`。
- [ ] SQL 控制台 AI 调用传 `sql`。
- [ ] 日志 AI 调用传 `logs`。
- [ ] SFTP AI 调用传 `sftp`。
- [ ] MCP Agent 调用传 `mcp`。

### 阶段 4：审计与治理

- [ ] Skill 管理操作写审计。
- [ ] 经验库和 Runbook 管理操作写审计。
- [ ] AI 调用审计记录使用的 Skill。
- [ ] MCP 只读 Skill 工具。
- [ ] MCP 经验库只读检索工具。
- [ ] MCP Runbook 只读详情工具。
- [ ] 写操作审批规则预留。

### 阶段 5：验证

- [ ] `pnpm build`
- [ ] `cd src-tauri && cargo check`
- [ ] `pnpm tauri build` 前检查 `src-tauri/resources/skills` 存在 38 个 `SKILL.md`。
- [ ] 打包后检查应用资源中包含 `resources/skills`。
- [ ] 浏览器验证 `/skills`
- [ ] 桌面运行验证 `/skills`
- [ ] 验证测试触发输入后命中列正确变化。
- [ ] 验证 SQL 控制台 AI 能注入 SQL Skill。
- [ ] 验证终端 AI 能注入终端 Skill。
- [ ] 验证禁用 Skill 后不再注入。
- [ ] 验证经验库空状态和新建流程。
- [ ] 验证 Runbook 空状态和新建流程。

---

## 12. 验收标准

- AI / MCP 菜单下可进入 `Skill 管理`。
- 页面顶部包含 `技能`、`经验库`、`Runbook` 三个 Tab。
- 项目中存在 `src-tauri/resources/skills`，包含从 `/Users/bin/Downloads/skills` 导入的 38 个内置 Skill。
- 打包后的应用内置资源中包含这 38 个 Skill，安装后不依赖 `/Users/bin/Downloads/skills`。
- 默认能看到 `内置 38`。
- 技能 Tab 视觉结构与参考图一致：测试触发区、统计筛选、搜索、显示内置开关、新建按钮、技能表格。
- 输入测试 prompt 后，命中的 Skill 在 `命中` 列显示次数，并按命中排序。
- 用户能新增自定义 Skill，并在对应场景 Prompt 预览中看到它。
- 禁用 Skill 后，Prompt 预览和真实 AI 调用都不再包含它。
- 经验库 Tab 具备搜索、新建和空状态。
- Runbook Tab 具备搜索、新建和空状态。
- SQL 控制台 AI 调用自动带 `sql` 作用域 Skill。
- 终端 AI 调用自动带 `terminal` 作用域 Skill。
- 日志 AI 调用自动带 `logs` 作用域 Skill。
- 内置 Skill 不能删除，只能禁用或恢复默认。
- 内置 Skill 内容来自应用资源文件，不允许在 UI 中直接编辑；用户需要修改时只能复制为用户 Skill。
- Skill 管理操作写入审计日志。
- 经验库和 Runbook 管理操作写入审计日志。
- 构建检查通过。

---

## 13. 风险与注意事项

- 运行期不能读取 `/Users/bin/Downloads/skills`，该路径只用于开发期导入；打包应用必须只读取自身资源目录。
- macOS `.app`、Windows 安装目录、开发模式下的资源路径不同，必须通过 Tauri `resource_dir` 获取，不能硬编码路径。
- `bundle.resources` 配置遗漏会导致打包后内置 Skill 为空，需要加入构建前检查。
- Prompt 过长会增加成本和延迟，需要限制启用 Skill 数量和总字符数。
- 用户自定义 Skill 可能与内置 Skill 冲突，需要在预览中明确排序。
- 触发词太泛会造成过度命中，需要在 UI 中显示命中原因，便于用户调整。
- 不能让 Skill 覆盖 AI 权限策略和审批队列。
- MCP Agent 场景尤其要避免输出凭证明文。
- Runbook 后续一旦接入真实执行，风险级别高于普通 Skill，必须逐步过策略，不允许整本一次性放行。
- Dev API 和 Tauri IPC 需要保持一致，否则网页调试和桌面运行会出现差异。

---

## 14. 推荐结论

建议采用“SQLite 持久化 + Rust 后端统一注入 + React 三 Tab 管理页面”的方案。

原因：

- 与现有 AI Provider、审计、MCP、系统设置架构一致。
- 能保证所有 AI 交互统一生效，而不是页面各自拼接。
- 可以通过 SQLite 做内置 Skill 版本升级和用户覆盖。
- 参考页中的经验库和 Runbook 可以自然沉淀为后续 MCP 工具，不需要未来重做信息架构。
- 安全策略可放在后端统一兜底，避免前端绕过。
