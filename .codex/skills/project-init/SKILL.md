---
name: project-init
description: |
  基于当前 Tauri SSH 框架模板初始化一个全新的独立桌面项目。仅在用户明确要求创建、新建或初始化项目时使用；普通功能开发、仓库克隆、目录创建或项目配置修改不触发。
---

# 新项目初始化

## 目标

从当前模板创建独立项目，完成输入校验、文件生成、标识替换、本地验证和可选 Git 配置。模板仓库保持可追溯，新项目源码默认私有；任何远端创建、推送、发布或签名操作必须单独获得授权。

## 不可突破的边界

- 这是显式工作流；用户未明确要求“创建/初始化新项目”时不要运行。
- 始终先探测模板仓库分支、未提交文件和目标目录；不 stash、不 reset、不切换模板分支、不清理其他会话文件。
- 默认在主工作区执行探测；新项目目录是用户确认后的明确目标，不得覆盖现有目录。
- 模板更新只先 `fetch` 和展示差异；`pull`、合并或切换版本需用户确认。
- 源码主仓库默认且必须为私有。Updater 匿名读取端点必须公开可访问；签名私钥绝不进入仓库、Prompt、日志或配置正文。
- Git、服务器和外部仓库操作优先使用 Tauri SSH MCP；外部写入、创建仓库、push 和发布都需要明确授权。
- 不自动发布、不创建公开源码仓库、不自动移动用户资产、不生成或覆盖签名密钥。
- 所有生成文本保持 UTF-8 无 BOM；后端保持 Command → Service → Database，前端 IPC 通过 `src/lib/api/`。

## 工作流

### 1. 只读准备

1. 记录模板路径、当前分支和 `git status -s`。
2. 检查目标父目录、磁盘空间、工具版本和名称冲突。
3. 读取 [template-update.md](references/template-update.md)，只读比较模板远端；未授权不更新。
4. 读取当前 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、端口配置和项目专属约束，使用真实值生成替换计划。

### 2. 收集并确认输入

读取 [project-inputs.md](references/project-inputs.md)，收集：

- 项目业务说明、产品名、缩写、目录/包名、应用标识符、作者。
- 目标目录、开发与 HMR 端口。
- 是否需要 updater、签名、远端仓库和哪些平台。
- 是否迁移需求、原型、品牌资产；移动操作必须逐项确认。

展示一次完整配置汇总，并在创建目录或生成密钥前获得确认。输入不足时可以推荐值，但不能把推荐当用户确认。

### 3. 创建本地项目

读取 [file-generation.md](references/file-generation.md)：

1. 使用提交快照导出到新的明确目录，不携带 `.git`、依赖、构建产物和个人数据。
2. 初始化新仓库并将模板设为 `upstream`；不改模板仓库历史。
3. 只在新项目目录执行产品名、标识符、包名、作者、端口、更新配置替换。
4. 包名按“长标识先替换、短名称精确替换”执行，绝不全局替换 `tauri` 依赖名。
5. 保留专有 LICENSE，除非第三方归属已明确要求独立许可。
6. 需求、原型和品牌资产默认不移动；用户确认后检查冲突并执行可恢复迁移。

### 4. 配置 Git 与签名

需要本地 Git、远端仓库、Updater 或签名时读取 [git-and-signing.md](references/git-and-signing.md)。

- 本地初始化、远端创建、push、release 视为不同权限级别，逐级确认。
- 逐文件暂存本流程生成的文件，不使用 `git add -A` 或 `git add .`。
- 源码仓库 `private=true`；公开源码必须风险说明和二次明确确认。
- Updater 地址必须匿名可访问；公开端点仅存安装包、manifest 和公钥。
- 私钥只存安全凭据或用户明确的安全路径，不在输出中回显。

### 5. 本地验证

根据实际变更执行：

```bash
pnpm install
pnpm build
npx tsc --noEmit
(cd src-tauri && cargo fmt --check)
(cd src-tauri && cargo check)
git diff --check
```

还需验证：

- 旧产品名、旧 identifier、自定义旧 lib 名在不允许位置零残留。
- `tauri` crate、`@tauri-apps/*` 和技能示例没有被误替换。
- `package.json`、Cargo、Tauri config、Vite 端口和 updater 配置一致。
- 新库首次启动可初始化 SQLite；页面通过 Codex 内置浏览器或 Control Chrome 验收。
- 源码仓库私有、updater 端点匿名可读、密钥未进入 Git。

运行桌面开发服务或占用端口前先确认当前会话没有复用他人进程，禁止直接 kill 端口。

### 6. 交付或可选外部动作

先报告本地项目路径、替换清单、验证结果、保留占位符和待用户动作。只有用户已明确授权时才创建远端、推送或配置发布；不得把本地初始化完成等同于线上发布完成。

## 引用索引

- [template-update.md](references/template-update.md)：模板版本检测、只读比较和更新选择。
- [project-inputs.md](references/project-inputs.md)：必填输入、格式校验、端口和配置确认。
- [file-generation.md](references/file-generation.md)：导出、清理、替换、资产迁移、图标和启动引导。
- [git-and-signing.md](references/git-and-signing.md)：Git、仓库可见性、Updater、Token 和签名安全。

## 完成条件

- 用户确认的目标目录已创建，模板工作区未被意外修改。
- 所有标识替换有精确清单和残留检查，未破坏框架依赖名。
- 构建、类型、Rust、编码和差异检查按范围通过。
- 页面或桌面运行行为已按项目规则验证。
- 外部仓库、发布和签名操作均有单独授权与证据；未授权项明确保留为待办。
