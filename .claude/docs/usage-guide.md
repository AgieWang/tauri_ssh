# Tauri 桌面应用 AI 辅助体系使用指南

## 1. 概述

本项目为 **Tauri 2.x 桌面应用**量身定制了一套完整的 AI 辅助开发体系，基于 Claude Code + Codex 双系统架构。

| 维度 | 数据 |
|------|------|
| **技术栈** | Rust 2021 + Tauri 2.x + React 19 + TypeScript 5.8 + Vite 7 |
| **Claude 技能数** | 33 个（L1:8 + L3:15 + L4:10） |
| **Codex 技能数** | 43 个（33 镜像 + 7 命令 + 3 管理） |
| **快速命令** | 7 个（/dev /command /check /start /progress /next /release） |
| **核心架构** | 双进程（WebView + Rust Core）+ 三层架构（Command → Service → Database） |

本体系覆盖了从 Rust 后端 Command 开发到 React 前端页面构建的全栈场景，所有技能均围绕 Tauri 双进程架构和 IPC 通信设计。

---

## 2. 快速启动

### 2.1 环境准备

确保已安装以下工具：

- **Node.js** 18+ 和 pnpm
- **Rust** 工具链（rustup + cargo）
- **Claude Code CLI**（`npm install -g @anthropic-ai/claude-code`）
- **Codex CLI**（可选，用于 Codex 体系）

### 2.2 部署 AI 配置

将以下文件复制到你的 Tauri 项目根目录：

```bash
# 必须部署的文件
CLAUDE.md              # Claude Code 核心指令文件
.claude/               # Claude Code 完整配置目录
  ├── settings.json    # Hook 注册
  ├── hooks/           # 技能评估 + 安全检查
  ├── commands/        # 7 个快速命令
  ├── skills/          # 33 个专业技能
  └── docs/            # 开发文档

# Codex 体系（可选）
AGENTS.md              # Codex 入口文件
.codex/                # Codex 完整配置目录
  └── skills/          # 43 个 Codex 技能
```

### 2.3 验证安装

```bash
# 启动 Claude Code
claude

# 测试技能评估是否生效
# 输入任意问题，观察是否有技能匹配提示
> 我想添加一个新的 Tauri Command
# 应当自动匹配 crud-development 或 api-development 技能
```

验证要点：
- 每次提问后，AI 会先列出匹配的技能及理由
- 输入 `/dev` 等命令应获得对应的开发向导
- Hook 不报错（检查 `.claude/settings.json` 格式正确）

---

## 3. 技能指令使用说明

### 3.1 快速命令一览

| 命令 | 用途 | 典型场景 |
|------|------|---------|
| `/dev` | 开发新功能 | 三层架构全栈代码生成（Rust + TypeScript） |
| `/command` | 快速创建 Tauri Command | 新增一个 IPC 接口（Command + Service + DAO） |
| `/check` | 代码规范检查 | Rust + TypeScript 双语言规范审查 |
| `/start` | 项目快速了解 | 首次接触项目时的全面导航 |
| `/progress` | 项目进度报告 | 当前功能完成度、待办事项 |
| `/next` | 下一步建议 | AI 分析项目状态并推荐后续任务 |
| `/release` | 发布新版本 | CI 全自动构建 + 推送 |

### 3.2 技能自动触发

技能通过 `UserPromptSubmit` Hook 自动评估触发，无需手动指定。常见触发词示例：

| 你说的话 | 自动激活的技能 |
|---------|--------------|
| "帮我写一个文件管理的 Command" | `crud-development` + `api-development` |
| "Rust 这个 unwrap 报错了" | `bug-detective` + `error-handler` |
| "前端页面用 Ant Design 写一个表格" | `ui-frontend` |
| "项目结构是怎样的" | `project-navigator` |
| "怎么做数据库迁移" | `database-ops` |
| "需要加一个 Capability 权限" | `security-auth` |
| "性能有点慢" | `performance-doctor` |

### 3.3 场景示例

**场景 1：新增一个用户管理 Command**
```
> /command
AI 会引导你完成：
1. models/ 定义 User 数据结构
2. database/ 实现 DAO（SQL 操作）
3. services/ 实现业务逻辑
4. commands/ 实现 Command 入口
5. lib.rs 注册到 generate_handler![]
6. src/types/ 定义 TypeScript 接口
7. src/lib/api/ 封装 invoke 调用
8. 更新 capabilities（如需要）
```

**场景 2：排查 IPC 通信错误**
```
> invoke 调用返回了 "command not found" 错误
AI 自动激活 bug-detective + error-handler 技能，检查：
- Command 是否添加了 #[tauri::command] 宏
- 是否在 generate_handler![] 中注册
- invoke 调用名是否与 Rust 函数名一致（snake_case）
```

**场景 3：全栈功能开发**
```
> /dev
AI 会询问功能需求，然后按三层架构生成：
- Rust 后端（Model → Database → Service → Command）
- TypeScript 前端（Types → API → Page）
- 自动更新 Capabilities
```

**场景 4：发布新版本**
```
> /release
AI 引导完成：版本号更新 → 构建检查 → 打包 → 推送
```

**场景 5：性能优化**
```
> 应用启动变慢了，怎么优化
AI 激活 performance-doctor 技能，从 Rust 编译优化、前端 bundle 体积、
SQLite WAL 模式、async Command 等多维度给出建议
```

---

## 4. 开发工作流

### 4.1 标准开发流程

```
需求分析 → /dev 或 /command → 三层架构生成 → /check 规范检查 → 测试 → /release 发布
```

详细步骤：

1. **需求分析**：使用 `brainstorm` 或 `tech-decision` 技能讨论方案
2. **代码生成**：通过 `/dev` 命令进行全栈开发，AI 遵循三层架构（Command → Service → Database）
3. **规范检查**：使用 `/check` 命令检查 Rust 和 TypeScript 代码规范
4. **Git 提交**：`git-workflow` 技能辅助规范的 commit message
5. **发布部署**：`/release` 命令完成打包和版本发布

### 4.2 多 AI 协作

本项目支持 Claude Code + Codex 双系统协作：

- **Claude Code**：主力开发系统，33 个技能 + 7 个命令 + Hook 自动评估
- **Codex**：辅助系统，43 个技能，适用于批量代码生成和文档任务
- **协作指南**：激活 `collaborating-with-codex` 技能查看详细协作方法

两个系统共享 33 个核心技能，内容完全一致。Codex 额外包含 7 个命令技能（由 Claude commands 转换）和 3 个管理技能。

---

## 5. 打包与导出说明

### 5.1 文件清单

完整的 AI 配置包包含以下文件：

```
交付文件:
├── CLAUDE.md                      # 核心指令（~620 行）
├── .claude/
│   ├── settings.json              # Hook 配置
│   ├── hooks/
│   │   ├── skill-forced-eval.cjs  # 技能评估 Hook
│   │   └── pre-tool-use.cjs       # 安全检查 Hook
│   ├── commands/                   # 7 个命令文件
│   ├── skills/                     # 33 个技能目录
│   └── docs/                       # 文档
├── AGENTS.md                       # Codex 入口
└── .codex/
    └── skills/                     # 43 个 Codex 技能
```

### 5.2 tar.gz 解压部署

如果通过导出包（tar.gz）安装：

```bash
# 解压到项目根目录
cd /path/to/your-tauri-project
tar -xzf tauri-claude-config.tar.gz

# 验证文件结构
ls -la CLAUDE.md .claude/ AGENTS.md .codex/
```

### 5.3 自定义扩展

- **添加新技能**：使用 `add-skill` 技能（输入"添加技能"触发），按向导在 `.claude/skills/` 下创建
- **修改现有技能**：直接编辑 `.claude/skills/{skill-name}/SKILL.md`
- **添加新命令**：在 `.claude/commands/` 下创建 `.md` 文件
- **调整 Hook**：编辑 `.claude/hooks/` 下的 `.cjs` 文件

扩展后需同步到 Codex 体系：将新技能复制到 `.codex/skills/` 并添加 YAML 头部。

---

## 6. 注意事项

1. **settings.json 格式**：必须使用 `matcher` + `hooks` 嵌套格式，旧版扁平格式会导致 Claude Code 报 Settings Error
2. **Hook 依赖 Node.js**：确保系统 PATH 中有 `node` 命令，Hook 脚本为 `.cjs`（CommonJS）格式
3. **双进程架构**：开发时始终牢记前端（WebView）和后端（Rust Core）通过 IPC 通信，不能直接共享内存
4. **Capabilities 权限**：使用 Tauri 插件 API 前必须在 `capabilities/*.json` 中声明权限
5. **Rust 错误处理**：所有 Command 返回 `Result<T, CommandError>`，禁止使用 `unwrap()` 或 `panic!()`
6. **API 封装规范**：前端 `invoke()` 调用统一封装在 `src/lib/api/`，不在组件中裸写
7. **CLAUDE.md 不要随意修改**：它是 AI 理解项目架构的核心文件，修改前请充分了解其结构
8. **Codex 体系同步**：新增或修改 Claude 技能后，记得同步更新 `.codex/skills/` 中的对应文件
