---
name: git-workflow
description: |
  Git 工作流与版本管理技能，规范分支策略、提交信息和发布流程。

  触发场景：
  - 用户需要创建分支或合并代码
  - 用户需要规范提交信息格式
  - 用户需要管理版本发布流程

  触发词：Git、分支、提交、合并、版本发布
---

# Git 工作流与版本管理

## 概述

Tauri Desktop App 的 Git 工作流与版本管理技能，规范分支命名、提交信息格式和发布流程。

---

## 分支策略

### 分支命名规范

| 分支类型 | 命名格式 | 示例 |
|---------|---------|------|
| 主分支 | `master` / `main` | `master` |
| 开发分支 | `dev` | `dev` |
| 功能分支 | `feature/{功能名}` | `feature/file-manager` |
| 修复分支 | `fix/{问题描述}` | `fix/window-resize-crash` |
| 发布分支 | `release/v{版本}` | `release/v0.2.0` |

---

## 提交信息规范

### Conventional Commits

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Type 定义

| Type | 说明 | 示例 |
|------|------|------|
| `feat` | 新功能 | `feat(rust): 添加文件读写 Command` |
| `fix` | 修复 Bug | `fix(react): 修复状态更新不生效` |
| `refactor` | 重构 | `refactor(rust): 重构错误处理为 thiserror` |
| `docs` | 文档 | `docs: 更新 README` |
| `style` | 格式 | `style(rust): cargo fmt 格式化` |
| `test` | 测试 | `test(rust): 添加 Command 单元测试` |
| `chore` | 杂务 | `chore: 更新 Cargo.toml 依赖` |
| `build` | 构建 | `build: 配置 Tauri 打包参数` |

### Scope 建议

| Scope | 说明 |
|-------|------|
| `rust` | Rust 后端代码 |
| `react` | React 前端代码 |
| `tauri` | Tauri 配置 (tauri.conf.json) |
| `caps` | Capabilities 权限配置 |
| `deps` | 依赖更新 |

---

## 发布流程（CI 全自动模式）

> 项目已配置 GitHub Actions CI，**本地不需要执行 `pnpm tauri build`**。
> 使用 `/release` 命令可自动完成全部发布流程。

```
1. 更新版本号（三处同步）
   - package.json: version
   - src-tauri/Cargo.toml: version
   - src-tauri/tauri.conf.json: version
2. 更新 release 仓库 README.md（下载链接 + 版本历史）
3. 提交并推送 release 仓库 README 变更
4. 提交源码仓库 + 推送到 GitHub
5. 打 Git Tag（v*.*.* 格式）并推送
   → 自动触发 GitHub Actions CI
   → CI 构建 Windows/macOS/Linux 三平台安装包
   → CI 自动推送产物 + update.json 到 release 仓库
```

### 快速发布

```bash
# 使用 /release 命令一键发布
/release
```

### 手动发布（备用）

```bash
# 1. 更新版本号后提交
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml package.json
git commit -m "release: vX.Y.Z"

# 2. 推送到 GitHub
git push <github_remote> <主分支>

# 3. 打 Tag 触发 CI
git tag vX.Y.Z
git push <github_remote> vX.Y.Z
```

---

## 🔴 远程 Git 操作优先用 Tauri SSH 安全凭证

> 需要远程认证的 git 操作（`pull` / `push` / `clone` / 建仓 / 分支操作 / 推送后远端核对），优先使用 Tauri SSH「安全凭证」和「Git 工作区」能力。凭据由 Tauri SSH 后端解密并通过受控会话或 Git AskPass 注入，AI 不接触 token / 密码明文，也不把明文写入 `.git/config`。

### 🔴 三条铁律

1. **零明文**：任何平台的 git 推送 / 建仓 / API 一律走 Tauri SSH 安全凭证、Git 工作区或安全凭证 MCP 会话，**永不**手拼 `https://user:TOKEN@host` 或明文传 token。
2. **优先绑定工作区**：本地仓库先登记到「安全凭证 → Git 工作区」，自动匹配 GitHub / GitLab / GitCode / Gitee 凭证；之后用 `pull_git_workspace` / `push_git_workspace` / `switch_git_workspace_branch` / `merge_git_workspace_branch` 等受控能力。
3. **失败即停**：安全凭证不存在、未启用、未允许 MCP、权限不足、会话审批失败时，停下提示用户去 Tauri SSH 修正凭证或授权，**绝不**回退明文 token。

### 用法

- **列凭据**：通过 Tauri SSH 安全凭证列表筛选 `provider ∈ {github, gitlab, gitcode, gitee}`，且 `enabled=true`、`hasSecret=true`、`allowMcp=true`、`status=active`。
- **绑定仓库**：使用 Git 工作区登记本地仓库路径和 `credentialKey`；扫描工作区时让 Tauri SSH 根据 remote host 自动匹配凭证。
- **拉取 / 推送**：优先用 `pull_git_workspace(workspaceKey)` / `push_git_workspace(workspaceKey)`，由后端通过 AskPass 注入凭证。
- **分支操作**：切换分支前确认工作区干净；合并分支前查看源分支和目标分支最后提交，再用 `merge_git_workspace_branch`。
- **平台 API**：需要建仓、查仓库、读写文件、合并请求等平台 API 时，通过 Tauri SSH 安全凭证 MCP 会话或 `secure_credential_git_*` 受控能力完成；只返回会话能力或操作结果，不返回 token 明文。
- **失败**：按 Tauri SSH 错误处理（凭证未启用 / 权限范围不足 / MCP 未允许 / 审批未通过 / 工作区不干净），绝不落明文 token。

## 常见错误

| 错误做法 | 正确做法 |
|---------|---------|
| 直接在 master 上开发 | 创建功能分支开发 |
| 提交信息写"修改代码" | 按 Conventional Commits 规范编写 |
| 版本号只改 package.json | 同步修改 Cargo.toml 和 tauri.conf.json |
| 提交 target/ 编译产物 | 确保 .gitignore 正确配置 |
