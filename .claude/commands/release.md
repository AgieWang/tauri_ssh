# /release - 发布新版本

作为版本发布助手，执行 Tauri 桌面应用的发布流程：更新版本号 → 更新 README → 推送 → 打 Tag 触发 CI → 等待 CI → 下载产物 → 本地推送到 release 仓库。

> **本地不需要执行 `pnpm tauri build`**。CI 负责构建和签名。用户只需从 GitHub Release 下载产物。

## 执行流程

### 第一步：读取当前版本和发布配置

```bash
Read src-tauri/tauri.conf.json  # 读取当前 version 和 productName
```

检查是否存在发布配置文件 `.claude/release-config.json`：
- **存在**：读取配置，跳到第三步
- **不存在**：进入第二步（首次配置）

### 第二步：首次配置（仅首次执行）

使用 AskUserQuestion 询问以下信息：

**问题1**：源码仓库的 GitHub remote 名称是什么？
- 选项：`github`、`origin`、自定义

**问题2**：需要支持哪些平台？
- 选项（多选）：`Windows`、`macOS`、`Linux`
- 默认推荐：Windows + macOS（Linux AppImage 体积大约 80MB）

**问题3**：请提供以下信息（自由文本）：
- 源码仓库 GitHub URL（如 `https://github.com/user/my-app`）
- Release 仓库 Gitee URL（如 `https://gitee.com/user/my-app-release`）
- Release 仓库 GitHub URL（如 `https://github.com/user/my-app-release`）
- 本地 Release 仓库（Gitee）路径
- 本地 Release 仓库（GitHub）路径
- 主分支名（master/main）

**问题4**（可选）：是否使用 Cloudflare R2 CDN 作为主下载源？
- 选项：`是`、`否`（默认否）
- 如果选"是"，继续询问：
  - R2 公开地址（如 `https://pub-xxx.r2.dev`）
  - rclone remote 名（默认 `r2`）
  - R2 bucket 名（默认 `downloads`）
  - R2 路径前缀（如 `myapp`，用于多项目隔离）

将信息保存到 `.claude/release-config.json`：

```json
{
  "appName": "<从 tauri.conf.json 的 productName 读取>",
  "githubRemote": "github",
  "platforms": ["windows", "macos"],
  "sourceRepoUrl": "https://github.com/user/my-app",
  "releaseRepoGiteeUrl": "https://gitee.com/user/my-app-release",
  "releaseRepoGithubUrl": "https://github.com/user/my-app-release",
  "localReleaseGiteePath": "<绝对路径>",
  "localReleaseGithubPath": "<绝对路径>",
  "mainBranch": "master",
  "r2": {
    "enabled": false,
    "publicUrl": "",
    "rcloneRemote": "r2",
    "bucket": "downloads",
    "pathPrefix": ""
  }
}
```

> **R2 CDN 说明**：`r2` 字段为可选配置。`enabled` 为 `false` 时，所有 R2 相关步骤跳过，回退到 Gitee 主源模式。
> 启用后，R2 作为主下载源和自动更新端点，Gitee 降级为备源。

> **平台配置说明**：`platforms` 数组决定 CI 构建矩阵、README 下载表格、产物清单和 update.json 内容。
> 修改平台配置后，需同步更新 `.github/workflows/release.yml` 的构建矩阵。

### 第三步：询问发布信息

使用 AskUserQuestion 询问：

**问题1**：新版本号是什么？（当前: {当前版本}）
- 选项：patch（x.y.Z+1）、minor（x.Y+1.0）、major（X+1.0.0）、自定义

**问题2**：更新说明（将写入 README.md 版本历史）

### 第四步：激活 release-publish 技能

```
Skill(release-publish)
```

### 第五步：按技能中的步骤执行发布前半段

> **注意**：此阶段只操作源码仓库，**不推送任何内容到 release 仓库**。
> release 仓库的 README、产物、update.json 全部在第七步（CI 完成后）一次性处理。

1. 更新三处版本号（tauri.conf.json / Cargo.toml / package.json）
2. 提交源码仓库（包含所有未提交的改动）
3. 推送到 GitHub
4. 打 Tag + 推送（触发 CI）

### 第六步：输出等待提示和文件清单

CI 触发后，**根据 platforms 配置**输出对应平台的文件清单：

```
CI 已触发，请等待构建完成。

构建进度：<源码仓库 GitHub URL>/actions
下载地址：<源码仓库 GitHub URL>/releases

需要下载的文件：
  [仅列出已配置平台的文件]

下载完成后请告诉我文件所在目录。
```

**各平台对应的文件**：
- Windows (2 个): `*.exe` + `*.exe.sig`
- macOS ARM (3 个): `*aarch64.dmg` + `*aarch64.app.tar.gz` + `*aarch64.app.tar.gz.sig`
- macOS Intel (3 个): `*x64.dmg` + `*x64.app.tar.gz` + `*x64.app.tar.gz.sig`
- Linux (3 个): `*.AppImage` + `*.AppImage.sig` + `*.deb`

使用 AskUserQuestion 询问：**文件下载到了哪个目录？**

### 第七步：执行发布后半段（本地处理）

用户提供下载目录后：

1. **如果 r2.enabled**：使用 rclone 上传产物到 R2 CDN（`rclone copy` → `<rcloneRemote>:<bucket>/<pathPrefix>/releases/vX.Y.Z/`）
2. 复制所有产物到两个 release 仓库的 `releases/vX.Y.Z/` 目录
3. 读取 `.sig` 文件生成 `update.json`（**仅包含已配置平台**）
   - **如果 r2.enabled**：生成 R2 版 + Gitee 版 + GitHub 版（3 个版本）
   - **如果 r2 未启用**：生成 Gitee 版 + GitHub 版（2 个版本）
   - **🔴 必须用 shell 变量注入签名**（见技能 release-publish 步骤 5「3a~3f」的 `generate_update_json()` 函数）
   - **🚫 禁止手动粘贴 base64 签名**（400+ 字符极易出错，一个字符差异即导致签名验证失败，所有用户更新报 `signature verification failed`）
   - **生成后必须验证**：对比 update.json 中的签名与原始 `.sig` 文件是否完全一致（步骤 3f 自动比对）
4. **如果 r2.enabled**：上传 R2 版 update.json 到 R2（`rclone copyto` → `<rcloneRemote>:<bucket>/<pathPrefix>/update.json`）
5. **如果 r2.enabled**：更新 R2 上的 `versions.json`（下载当前版本列表 → 在数组头部插入新版本 → 上传回 R2）。文档站下载页依赖此文件获取版本列表。
6. 更新两个 release 仓库的 README.md（下载链接 + 版本历史 + 项目结构树，**仅包含已配置的平台**）
7. 提交 + pull rebase + 推送 release 仓库（Gitee 先推，GitHub 后推）
8. 输出完成报告

---

## AI 执行规则

### 配置管理
1. **首次自动配置**：首次执行时询问仓库信息和平台偏好，保存到 `.claude/release-config.json`
2. **后续自动读取**：后续发布直接读取配置，不再重复询问
3. **平台偏好持久化**：`platforms` 字段记录支持的平台，影响 CI 矩阵、产物清单、README 和 update.json

### 版本号
4. **全自动执行**：除询问版本号、更新说明和下载目录外，不再中途询问确认
5. **三处同步**：tauri.conf.json / Cargo.toml / package.json 版本号必须一致

### README 更新
6. **三处更新**：下载链接表格 + 版本历史条目 + 项目结构树
7. **两个仓库同步**：Gitee 和 GitHub release 仓库的 README.md 内容一致
8. **CI 产物文件名**：使用 `<productName>_` 作为前缀（从 tauri.conf.json 读取）
9. **按平台过滤**：下载表格和项目结构树只包含 `platforms` 配置中的平台

### 推送相关
10. **推送前先拉取**：release 仓库 push 前必须 `git pull --rebase origin master`
11. **Gitee 优先推送**：release 仓库先推 Gitee（主更新端点），后推 GitHub（备份）
12. **Git remote 名**：从 release-config.json 读取
13. **打 Tag 触发 CI**：`git tag vX.Y.Z && git push <remote> vX.Y.Z`

### CI 与产物处理
14. **不需要本地构建**：`pnpm tauri build` 由 CI 执行
15. **签名由 CI 完成**：`.sig` 文件已包含在 CI 产物中，用户只需下载
16. **Claude 生成 update.json**：读取 `.sig` 文件内容写入 update.json（仅包含已配置平台）。如果 r2.enabled，生成 3 个版本（R2 版 + Gitee 版 + GitHub 版）；否则生成 2 个版本（Gitee 版 + GitHub 版）
17. **Claude 推送 release 仓库**：复制产物 + update.json 后本地推送到 Gitee/GitHub

### 🔴 签名注入（防错规则，整个发布最易炸的一步）
18. **必须用 shell 变量注入签名**：先 `WIN_SIG=$(cat <AppName>_*x64-setup.exe.sig | tr -d '\r\n')`，再用**双引号 heredoc** `<< JSONEOF`（无单引号）通过 `$WIN_SIG` 注入
19. **禁止手动粘贴 base64 签名**：400+ 字符极易出错，一个字符差异即导致 `signature verification failed`，所有用户自动更新失效
20. **必须用 `<AppName>_` 前缀过滤 .sig**：禁止 `cat *x64-setup.exe.sig` 纯后缀通配符（会把同目录其他项目的 sig 拼进来 → 非法 base64 "Invalid symbol 61"）；强烈建议下载用独立子目录隔离
21. **生成后必须验证**：① `=` 字符数 ≤ 2 ② node 真 base64 解码 ③ 比对 update.json 内签名与 `.sig` 一致，任一不过立即中止
22. **统一生成函数**：所有版本（R2/Gitee/GitHub）必须用同一个 `generate_update_json()` 函数生成，只传不同的 BASE_URL，确保签名/结构完全一致

### R2 CDN（可选）
23. **R2 为可选功能**：通过 `release-config.json` 的 `r2.enabled` 字段控制，未配置时回退到 Gitee 主源模式
24. **R2 上传使用 rclone**：`rclone copy` 上传产物，`rclone copyto` 上传 update.json
25. **R2 启用后分发策略**：R2 CDN 为主源，Gitee 为备源，GitHub 为存档
