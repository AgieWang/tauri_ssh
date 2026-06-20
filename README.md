# Tauri SSH

<img src="src-tauri/icons/icon.png" width="96" alt="Tauri SSH icon" />

Tauri SSH 是一款面向 AI 时代的跨平台 SSH 桌面管理工具。它将服务器资产、凭据保险库、真实 SSH 终端、SFTP 文件管理、日志监听、AI Provider、MCP Server、审批队列与审计日志整合到一个本地优先的桌面应用中，适合个人开发者、运维人员和需要让 Agent 安全参与服务器操作的团队使用。

> 当前版本：`v0.1.5`，仍处于活跃开发阶段。请在生产环境使用前完整校验凭据管理、AI 权限策略、审批规则和审计链路。

## 功能概览

### 工作台

- 汇总服务器、凭据、AI Provider、MCP、审批和审计等关键模块状态。
- 提供常用运维入口，便于从首页快速进入终端、SFTP、日志监听等场景。

### 服务器资产管理

- 支持按分组管理 SSH 服务器，维护主机、端口、账号、认证方式、跳板机和启用状态。
- 支持从当前操作系统的 OpenSSH Config 导入服务器配置。
- 支持服务器连接测试，并记录最近连接状态。
- 支持为每台服务器配置 AI 权限级别，用于后续 AI 命令执行、审批和禁止策略判断。

### 凭据保险库

- 支持本地维护密码、密钥、Token 等凭据引用。
- 前端只展示掩码和状态，不直接展示敏感明文。
- 支持凭据新增、更新、授权范围调整、轮换和删除。
- 可与服务器认证方式联动，避免在服务器配置中反复暴露真实凭据。

### 终端 + AI

- 基于 `xterm.js` 实现真实 SSH 终端体验。
- 支持多 Tab 同时打开多个服务器终端，同一服务器也可打开多个独立会话。
- 支持终端窗口高度自适应、最大化和底部输入区域留白优化。
- 支持中文自然语言开头的输入触发 AI 问答或命令建议。
- 根据服务器配置的 AI 权限级别判断命令是否自动执行、进入审批或直接禁止。
- AI 输出支持 Markdown 风格格式化展示，并在等待模型响应时显示思考提示。

### SFTP 文件管理

- 支持真实 SFTP 目录浏览、文件读取、上传、下载、新建、重命名和删除。
- 提供类似桌面文件管理器的目录树与文件列表交互。
- 内置 CodeMirror 文本编辑器，支持多语言语法高亮、搜索和编辑保存。
- 暗色主题下对按钮、表格和文本可读性做了适配。

### 日志监听

- 支持对同一台或不同服务器的多个日志文件进行多标签 `tail` 监听。
- 支持关键词搜索、过滤和手动刷新。
- 日志内容区域尽量扩展到页面最大宽度，适合长日志行排查。
- AI 解释采用按钮触发，避免刷新日志时自动消耗模型调用。

### AI Provider

- 支持通过界面添加和管理主流模型服务商配置。
- 当前定位覆盖 Anthropic、OpenAI、Gemini、DeepSeek、智谱 GLM、Kimi、MiniMax、小米等 Provider。
- 支持模型列表读取、默认模型选择、连接测试和启用状态管理。
- API Key 等敏感配置只在后端处理，前端以掩码状态呈现。

### MCP Server

- 应用可作为本地 MCP Server，为 Agent 工具提供受控的服务器运维能力。
- 当前 MCP 端点默认面向本机访问，支持通过 `mcp-remote` 兼容 stdio 客户端。
- 支持为 Claude Code、Codex、Cursor、Cline、Zed、Windsurf、Roo、Qwen 等 Agent 客户端生成或写入配置。
- 已规划并实现多类 MCP 工具，包括服务器列表、连接资料、只读命令执行、SFTP 只读、日志快照、AI Provider 列表、审批请求和受控执行等。

### 堡垒机会话

- 支持记录和管理堡垒机会话入口、服务器引用和打开方式。
- 该模块用于会话管理和入口整合，不包含绕过第三方鉴权或提取受保护系统凭证的能力。

### 审批队列与审计日志

- 审批队列用于承接 AI 或 MCP 触发的高风险操作请求。
- 审批结果会影响受控命令、SFTP 写入、重命名、删除等动作是否允许继续执行。
- 审计日志记录终端 AI、MCP 工具、审批、SFTP、日志监听等关键操作。
- 支持审计日志查询、筛选和导出，便于回溯 Agent 与用户操作。

### 系统设置

- 支持本地应用设置读取、更新、重置和导出。
- 支持自动更新相关配置，发布产物可结合 Tauri Updater 使用。

## 技术栈

| 层级 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2 |
| 后端 | Rust 2021、rusqlite、ssh2、reqwest、tokio、axum |
| 前端 | React 19、TypeScript 5、Vite 7 |
| UI | Ant Design 6、TailwindCSS 4、Lucide React |
| 状态管理 | Zustand |
| 终端 | xterm.js |
| 编辑器 | CodeMirror |
| 数据存储 | SQLite，本地优先 |
| 打包发布 | Tauri Bundle、GitHub Actions、Tauri Updater |

## 项目结构

```text
.
├── src/                       # React 前端
│   ├── components/            # 布局与通用组件
│   ├── lib/api/               # Tauri IPC API 封装
│   ├── pages/                 # 页面模块
│   ├── store/                 # Zustand 状态
│   └── styles/                # 全局样式与主题变量
├── src-tauri/                 # Rust / Tauri 后端
│   ├── capabilities/          # Tauri 权限声明
│   ├── icons/                 # 应用图标
│   ├── src/
│   │   ├── commands/          # IPC Command 入口
│   │   ├── database/          # SQLite 数据访问与迁移
│   │   ├── models/            # Rust 数据模型
│   │   ├── services/          # 业务逻辑
│   │   └── remote/            # 本地远程访问网关相关能力
│   └── tauri.conf.json        # Tauri 应用配置
├── .github/workflows/         # GitHub Actions 发布流程
├── package.json
└── README.md
```

## 本地开发

### 环境要求

- Node.js 22 或更高版本
- pnpm 11.8.0
- Rust stable
- Tauri 2 所需系统依赖

macOS / Windows / Linux 的 Tauri 依赖安装可参考官方文档：[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)。

### 安装依赖

```bash
pnpm install
```

### 启动桌面开发模式

```bash
pnpm tauri:dev
```

开发模式下前端地址默认为：

```text
http://localhost:1422
```

注意：浏览器预览只能调试前端界面和本地 HTTP Dev API 代理能力；涉及真实 Tauri IPC、SSH、SFTP、系统路径、Updater、原生窗口等能力时，应以桌面运行结果为准。

### 前端构建

```bash
pnpm build
```

### 桌面构建

```bash
pnpm tauri:build
```

## GitHub Actions 发布

项目已包含 `.github/workflows/release.yml`，推送符合格式的 Git Tag 后会触发跨平台构建：

```bash
git tag v0.1.5
git push origin v0.1.5
```

桌面端发布 Tag 格式：

```text
v*.*.*
```

当前工作流包含：

- Windows：NSIS 安装包
- macOS：`app` / `dmg`，包含 `aarch64-apple-darwin` 与 `x86_64-apple-darwin`
- Linux：`deb` / `AppImage`

如需生成带更新签名的产物，请在 GitHub Repository Secrets 中配置：

| Secret | 说明 |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Tauri Updater 签名私钥内容 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 签名私钥密码 |

发布工作流会创建 GitHub Draft Release，构建产物会作为 Release Assets 上传。

## 安全说明

- 不要提交 `src-tauri/keys/` 下的私钥文件。
- 不要在 README、Issue、日志或截图中暴露服务器密码、私钥、API Key、Token。
- AI 命令执行必须结合服务器 AI 权限级别、审批队列和审计日志使用。
- MCP 工具面向 Agent 开放能力前，应先确认只读、需审批、禁止三类策略符合预期。
- JumpServer / 堡垒机场景应遵守组织内部访问规范，本项目不提供绕过鉴权或隐蔽代理能力。

## 开发状态

Tauri SSH 当前处于 `v0.1` 阶段，核心模块已具备真实功能雏形，但仍建议按以下顺序继续完善：

- 补齐关键模块的自动化测试和端到端验证。
- 强化凭据加密、密钥派生和跨平台安全存储策略。
- 完善 MCP Server 的工具权限矩阵、调用限额和审计字段。
- 完善跨平台打包、签名、公证和自动更新发布流程。

## License

暂未声明开源许可证。如需公开开源，请先补充 LICENSE 文件并明确授权范围。
