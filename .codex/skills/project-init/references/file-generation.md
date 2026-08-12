# 文件生成与替换

## 目录

- 导出与初始化
- 清理与治理文件
- 精确替换
- 资产和图标
- 验证与启动

## 导出与初始化

优先从已确认提交导出，不携带模板 `.git` 和未跟踪个人数据：

```bash
git archive HEAD | tar -x -C "/absolute/confirmed/new-project"
```

执行前确认目标不存在并创建精确目录。导出后在新目录初始化独立 Git，并将模板远端记录为 `upstream`。所有后续修改必须在新目录执行。

不要覆盖已有目标，不要把 `node_modules/`、`target/`、`dist/`、个人会话数据或凭据带入新项目。

## 清理与治理文件

- 删除需要按新包名重新生成的 `src-tauri/Cargo.lock`，之后用 Cargo 重新生成。
- 框架自身开发指南仅在确认不适合子项目时删除。
- 保留专有 LICENSE 原文；第三方项目需要独立许可时应完整重新评估，不只替换版权人。
- 保留 `.codex/` 作为项目 Skill 规范源；是否支持 Claude/Agents 由 Manifest 和同步脚本决定。
- 子项目专属规则写入 `.codex/PROJECT.md` 或项目约定位置，不直接修改框架只读副本。
- 项目专属 Skill 与框架 Skill 分层，反哺候选先脱敏再提交上游。

## 精确替换

先从当前文件读取旧值，再建立“文件、旧值、新值、允许残留”清单。典型目标：

| 属性 | 精确目标 |
|---|---|
| 产品名 | `tauri.conf.json` productName/title、HTML 标题、导航和首页文案 |
| identifier | `tauri.conf.json` 与明确的项目规范引用 |
| Cargo 包/lib | `[package].name`、`[lib].name`、`main.rs` lib 调用 |
| npm 包名 | `package.json` 的 `name` |
| 作者/描述 | `Cargo.toml` 对应字段 |
| 端口 | Vite server/HMR、Tauri devUrl、相关启动脚本 |
| Updater | endpoints 和公钥占位符 |

替换顺序：自定义长标识（如旧 lib 名）→ 完整产品名 → identifier → 精确包名字段 → 短缩写。绝对禁止全局替换 `tauri`，以下内容必须保留：

- `tauri = { version = "2" }`
- `use tauri::...`
- `@tauri-apps/*`
- Skill 示例中的通用 Tauri 术语。

README 应改为新项目介绍，不保留把子项目描述成框架本体的大段文本。

## 资产和图标

迁移需求、原型或品牌资产前列出源、目标和冲突，按用户确认选择复制或移动。不得静默覆盖同名文件，也不得移动 `.codex/skills/`、Hooks、凭据或项目规范。

若已确认的品牌目录存在 1024x1024 PNG，可执行：

```bash
pnpm tauri icon /absolute/path/to/logo-1024.png
```

否则保留默认图标并列为后续事项。生成后检查 Windows、macOS 和通用尺寸产物。

## 验证与启动

对旧名称、identifier、lib 名执行限定文件类型的残留搜索；逐条审查允许残留。重新生成 lockfile 后运行安装、TypeScript、前端构建、Rust fmt/check 和 `git diff --check`。

首次启动还要确认：

- SQLite 可由迁移自动初始化。
- Vite/Tauri 端口一致且不杀其他会话进程。
- 页面通过内置浏览器或 Control Chrome；桌面专属行为在 Tauri 窗口验证。
- 控制台无新增错误，加载、空态和错误态可用。

启动引导只输出当前项目真实命令。不要主动生成额外 `.md` 提示文档，除非用户明确要求。
