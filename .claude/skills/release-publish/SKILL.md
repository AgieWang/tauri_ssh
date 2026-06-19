---
name: release-publish
description: |
  发布版本/发布更新/release/推送Gitee/签名构建/update.json/版本发布/R2 CDN/rclone

  触发场景：
  - 需要发布新版本
  - 需要执行发布流程
  - 需要更新版本号并推送

  触发词：发布、release、版本发布、推送、打Tag、update.json、签名构建
---

# 发布更新

## 概述

Tauri 桌面应用采用 **CI 构建 + 本地推送** 模式（可选 R2 CDN 加速）：

```
本地：更新版本号 → 提交 → 打 Tag → 推送（触发 CI）
  ↓ CI 构建中（不推送任何内容到 release 仓库）
CI：构建安装包（按配置的平台） → 上传到 GitHub Release（草稿）
  ↓ CI 完成后，用户下载产物
本地：[可选] 上传产物到 R2 CDN + 更新 README + 复制产物 + 生成 update.json → 推送到 Gitee/GitHub release 仓库
```

> **关键原则**：CI 构建完成、用户提供下载文件之前，**不要推送任何内容到 release 仓库**。
> README 更新、产物复制、update.json 生成在获得产物后一次性完成并推送。

> **本地不需要执行 `pnpm tauri build`**。CI 负责构建和签名。
> 构建完成后，用户手动从 GitHub Release 下载产物，Claude 负责本地处理和推送。

### 平台配置

发布流程支持按需选择构建平台，通过 `.claude/release-config.json` 的 `platforms` 字段配置：

| platforms 值 | CI 构建矩阵 | 产物数量 |
|-------------|------------|---------|
| `["windows", "macos"]` | Windows + macOS ARM + macOS Intel | 8 个 |
| `["windows", "macos", "linux"]` | 全平台 | 11 个 |
| `["windows"]` | 仅 Windows | 2 个 |
| `["macos"]` | 仅 macOS ARM + Intel | 6 个 |

> **首次发布时通过 `/release` 命令询问用户选择平台，记录后不再重复询问。**
> 去掉 Linux 可节省 CI 时间、减少产物体积（Linux AppImage 约 80MB）。

### 三级分发策略

支持可选的 R2 CDN 作为主下载源，通过 `.claude/release-config.json` 的 `r2.enabled` 字段控制：

**R2 CDN 启用时（r2.enabled = true）：**

| 用途 | 平台 | 角色 | 原因 |
|------|------|------|------|
| **源码托管** | GitHub（私有） | — | 代码管理 + CI 构建 |
| **CI 构建** | GitHub Actions | — | 跨平台构建 + 签名 |
| **安装包下载 + 自动更新** | Cloudflare R2 CDN | **主源** | 全球 CDN，零流量费，上传秒级 |
| **自动更新兜底** | Gitee（公开） | **备源** | R2 不通时兜底，中国大陆可访问 |
| **备份存档** | GitHub（公开） | **存档** | 海外用户 + 历史备份 |

> **R2 CDN 优势**：上传 24MB 产物仅需 ~8 秒，零流量费，全球 CDN 加速。
> 自定义域名（如 `dl.example.com`）为可选配置，需将域名 DNS 迁移到 Cloudflare。
> 当前使用 R2.dev 公开 URL，功能完全等价。

**R2 CDN 未启用时（默认模式）：**

| 用途 | 平台 | 原因 |
|------|------|------|
| **源码托管** | GitHub（私有） | 代码管理 + CI 构建 |
| **CI 构建** | GitHub Actions | 跨平台构建 + 签名 |
| **自动更新端点** | Gitee（公开） | 中国大陆可访问 |
| **安装包下载** | Gitee（公开） | 中国大陆可下载 |
| **备份存档** | GitHub（公开） | 海外用户 + 备份 |

### 为什么不让 CI 推送到 release 仓库？

GitHub Actions 在美国服务器运行，推送二进制产物到 Gitee（中国）经常超时（50 分钟+）。
因此改为用户本地下载产物后，由 Claude 在本地完成推送，速度更快且更可控。

---

## 首次发布前的准备工作

> **首次使用发布功能时，必须先完成以下配置。后续发布跳过此节。**

### 1. 创建 Release 仓库

需要创建两个 **公开** 仓库用于存放安装包和 update.json：

```bash
# Gitee（主更新端点，中国大陆可访问）
https://gitee.com/<用户名>/<项目名>-release

# GitHub（备份）
https://github.com/<用户名>/<项目名>-release
```

每个仓库需要一个 `README.md` 和 `update.json`（本地推送时自动生成 update.json）。

### 2. 生成签名密钥

```bash
# 在项目根目录生成更新签名密钥对
pnpm tauri signer generate -w src-tauri/keys/tauri-updater.key
# 密码提示时直接按两次回车（空密码）
```

生成后：
- 将 `.key.pub` 文件内容复制到 `tauri.conf.json` → `plugins.updater.pubkey`
- 将 `.key` 文件内容添加到 GitHub Secrets → `TAURI_SIGNING_PRIVATE_KEY`
- 确保 `src-tauri/keys/` 已加入 `.gitignore`

### 3. 配置 GitHub Secrets

在 **源码仓库**（私有）的 Settings → Secrets and variables → Actions 中添加：

| Secret 名称 | 值 | 说明 |
|-------------|-----|------|
| `TAURI_SIGNING_PRIVATE_KEY` | `src-tauri/keys/tauri-updater.key` 文件的完整内容 | 更新签名私钥 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 空字符串（留空即可） | 私钥密码（无密码） |

> **注意**：不再需要 `RELEASE_REPO_TOKEN`、`GITEE_USERNAME`、`GITEE_TOKEN`，
> 因为 CI 不再推送到 release 仓库，推送由本地完成。

### 4. 配置 tauri.conf.json

**如果 R2 CDN 启用（r2.enabled = true）：**

```json
{
  "plugins": {
    "updater": {
      "endpoints": [
        "<r2.publicUrl>/<r2.pathPrefix>/update.json",
        "https://gitee.com/<用户名>/<项目名>-release/raw/master/update.json"
      ],
      "pubkey": "<公钥内容>"
    }
  },
  "bundle": {
    "targets": ["nsis"],
    "createUpdaterArtifacts": "v1Compatible"
  }
}
```

> R2 作为第一端点（主源），Gitee 作为第二端点（备源）。Tauri updater 按顺序尝试，第一个失败自动 fallback。

**如果 R2 未启用（默认）：**

```json
{
  "plugins": {
    "updater": {
      "endpoints": ["https://gitee.com/<用户名>/<项目名>-release/raw/master/update.json"],
      "pubkey": "<公钥内容>"
    }
  },
  "bundle": {
    "targets": ["nsis"],
    "createUpdaterArtifacts": "v1Compatible"
  }
}
```

### 5. 添加 GitHub remote（如仅有 Gitee remote）

```bash
git remote add github https://github.com/<用户名>/<项目名>.git
```

#### 5.1 多 CI 仓库 fallback（应对私有仓 Actions 配额耗尽）

> GitHub Actions 对私有仓有每月免费分钟数（macOS×10 倍消耗最快）。月底容易耗尽，CI 直接 3-5 秒失败。
> **建议**：源码同时配置主/备两个 GitHub remote（不同账号），主仓配额用尽时切到备仓继续发版。

```bash
# 主仓（默认）
git remote add github   https://github.com/<account-a>/<repo>.git
# 备仓（额度耗尽时切换，可用 SSH 避开 HTTPS credential helper 多账号冲突）
git remote add github2  git@github.com:<account-b>/<repo>.git
```

**关键约束**：

- 两个仓库需各自配置完整的 GitHub Secrets（签名私钥、可选 Android keystore 等），secret **不能跨仓共享**
- 发版时 `git push <remote> <tag>`，**Tag 只推到选定的 remote**，避免两边同时触发 CI 浪费额度
- 不同账号的 PAT 分别保存为 `~/.gh_token_<account>`，发版前根据 remote 选择对应 token
- workflow 文件需同步到两个 remote（任何一边修改后用 `git push <other>` 同步代码）
- 步骤 3 推 tag 前必须**询问用户**用哪个 CI remote，并把选择记录下来供步骤 4 的 `CI_OWNER_REPO` 变量使用

### 6. 克隆 Release 仓库到本地

```bash
# 建议放在源码仓库的同级目录
git clone https://gitee.com/<用户名>/<项目名>-release.git   # Gitee
git clone https://github.com/<用户名>/<项目名>-release.git  # GitHub（另一个目录名）
```

### 7. 根据平台配置修改 CI workflow

根据 `.claude/release-config.json` 中的 `platforms` 配置，修改 `.github/workflows/release.yml` 的构建矩阵：

**Windows + macOS（推荐，不含 Linux）：**
```yaml
matrix:
  include:
    - platform: windows-latest
      args: '--bundles nsis'
    - platform: macos-latest
      args: '--bundles app,dmg'
      target: aarch64-apple-darwin
    - platform: macos-latest
      args: '--bundles app,dmg'
      target: x86_64-apple-darwin
```

**全平台（含 Linux）：**
```yaml
matrix:
  include:
    - platform: windows-latest
      args: '--bundles nsis'
    - platform: macos-latest
      args: '--bundles app,dmg'
      target: aarch64-apple-darwin
    - platform: macos-latest
      args: '--bundles app,dmg'
      target: x86_64-apple-darwin
    - platform: ubuntu-22.04
      args: '--bundles deb,appimage'
```

---

## 关键配置（用户须在首次发布时提供）

> **以下信息在首次发布时通过 `/release` 命令询问用户获取，后续自动记忆。**

| 配置项 | 说明 | 示例 |
|--------|------|------|
| **应用名称** | CI 产物前缀（productName） | `MyApp` |
| **支持平台** | 构建哪些平台 | `["windows", "macos"]` |
| **源码仓库 GitHub remote 名** | 推送源码用 | `github` 或 `origin` |
| **源码仓库 GitHub URL** | CI 所在仓库 | `https://github.com/user/my-app` |
| **Release 仓库（Gitee）URL** | 更新端点（主/备取决于 R2） | `https://gitee.com/user/my-app-release` |
| **Release 仓库（GitHub）URL** | 备份存档 | `https://github.com/user/my-app-release` |
| **本地 Release 仓库（Gitee）路径** | 本地 clone 目录 | `../my-app-release-gitee` |
| **本地 Release 仓库（GitHub）路径** | 本地 clone 目录 | `../my-app-release` |
| **主分支名** | master 或 main | `master` |
| **R2 CDN 启用**（可选） | 是否使用 R2 作为主源 | `true` / `false` |
| **R2 公开地址**（可选） | R2.dev 或自定义域名 | `https://pub-xxx.r2.dev` |
| **R2 rclone remote**（可选） | rclone 配置名 | `r2` |
| **R2 bucket**（可选） | R2 存储桶名 | `downloads` |
| **R2 路径前缀**（可选） | 多项目隔离路径 | `myapp` |

---

## 版本号位置（三处必须同步）

| 文件 | 字段 |
|------|------|
| `src-tauri/tauri.conf.json` | `"version": "x.y.z"` |
| `src-tauri/Cargo.toml` | `version = "x.y.z"` |
| `package.json` | `"version": "x.y.z"` |

---

## 完整发布流程

### 步骤 1：询问版本号和更新说明

```
使用 AskUserQuestion 询问：
1. 新版本号？（当前版本读取自 tauri.conf.json）
2. 更新说明？（将写入 release 仓库 README.md 版本历史）
```

### 步骤 2：更新三处版本号

```bash
Edit src-tauri/tauri.conf.json   # "version": "新版本号"
Edit src-tauri/Cargo.toml        # version = "新版本号"
Edit package.json                # "version": "新版本号"
```

### 步骤 3：提交源码仓库并打 Tag 触发 CI

> **注意**：此阶段只操作源码仓库，**不推送任何内容到 release 仓库**。
> release 仓库的 README、产物、update.json 全部在步骤 5（CI 完成后）一次性处理。

```bash
cd "<源码仓库路径>"

# 提交版本号更新 + 其他变更
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml package.json
git commit -m "release: v$VERSION

<更新说明摘要>

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"

# 推送到 GitHub（remote 名根据用户配置）
git push <github_remote> <主分支名>

# 打 Tag 并推送（触发 GitHub Actions CI）
git tag "v$VERSION"
git push <github_remote> "v$VERSION"
```

### 步骤 4：自动监听 CI + 自动下载产物（Claude 全自动，无需用户介入）

> **演化背景**：早期流程让用户在浏览器手动下载所有 CI 产物，再 `AskUserQuestion` 询问目录。
> 实操中用户经常下漏文件、下到默认目录被多项目混在一起、下到一半浏览器中断。
> 现升级为 **Claude 自己用 git credential helper 拿 token → API 轮询 CI → 用 asset id 下载到独立子目录**。
> 用户全程不操作浏览器；仅在自动流程崩掉时（步骤 4.5）才回退到手动下载。

**各平台对应的 CI 产物**：

| 平台 | 产物数量 | 文件列表 |
|------|---------|---------|
| Windows | 2 个 | `.exe` + `.exe.sig` |
| macOS ARM | 3 个 | `_aarch64.dmg` + `_aarch64.app.tar.gz` + `_aarch64.app.tar.gz.sig` |
| macOS Intel | 3 个 | `_x64.dmg` + `_x64.app.tar.gz` + `_x64.app.tar.gz.sig` |
| Linux | 3 个 | `.AppImage` + `.AppImage.sig` + `.deb` |

#### 4.1 拿 GitHub Token（git credential helper）

```bash
TOKEN=$(printf "protocol=https\nhost=github.com\n" | git credential fill 2>/dev/null \
        | grep "^password=" | cut -d= -f2)
if [ -z "$TOKEN" ] || [ ${#TOKEN} -lt 20 ]; then
  echo "❌ 拿不到 token，请确认 git config 里 credential.helper = manager 并已存过 GitHub 密码"
  exit 1
fi
echo "✅ token 长度 ${#TOKEN}（已隐藏内容）"
```

> 不要用 `gh auth login` —— 项目环境通常已经用 OS Credential Manager 存好了，`git credential fill` 一句话拿到，零配置。
> **fine-grained PAT 必须有 `Contents: Read and write` + `Actions: Read`**（CI 创建的是 draft release，仅 `Contents: Read` 看不到也下载不了）。

#### 4.2 监听 CI 进度（每 30s 轮询）

```bash
TAG="v$VERSION"
CI_OWNER_REPO="<owner>/<repo>"   # 按用户选择的 CI remote 替换

while true; do
  STATUS_LINE=$(curl -s -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/$CI_OWNER_REPO/actions/runs?per_page=10" \
    | node -e "
      const d=JSON.parse(require('fs').readFileSync(0,'utf8'));
      const r=(d.workflow_runs||[]).find(x=>x.head_branch==='$TAG');
      if(!r){console.log('not_found null'); process.exit(0);}
      console.log(r.status, r.conclusion);
    ")
  STATUS="${STATUS_LINE%% *}"
  CONCLUSION="${STATUS_LINE##* }"
  echo "[$(date +%H:%M:%S)] status=$STATUS conclusion=$CONCLUSION"
  if [ "$STATUS" = "completed" ]; then
    [ "$CONCLUSION" = "success" ] && { echo "✅ CI 成功"; break; }
    echo "❌ CI 失败 conclusion=$CONCLUSION"; exit 1
  fi
  sleep 30
done
```

#### 4.3 列出 release 的所有 asset

```bash
ASSETS_JSON=$(curl -s -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/$CI_OWNER_REPO/releases?per_page=10")

echo "$ASSETS_JSON" | node -e "
const d = JSON.parse(require('fs').readFileSync(0, 'utf8'));
const r = d.find(x => x.tag_name === 'v$VERSION');
if (!r) { console.error('没找到 v$VERSION release'); process.exit(1); }
console.log('id:', r.id, 'draft:', r.draft, 'name:', r.name);
r.assets.forEach(a => console.log(a.id, a.size.toString().padStart(10), a.name));
"
```

> CI 创建的是 **draft release**（`draft: true`），普通浏览器 URL 看不到，必须用 asset id + token + `Accept: application/octet-stream` 调 API 下载。

#### 4.4 自动下载到独立子目录（避免与其他项目混）

```bash
# 一个版本一个目录，旧版本残留 .sig 不会污染本次（v3.0.7 踩过坑：下载目录混入其他项目 .sig，cat 通配符拼接出非法 base64）
DOWNLOAD_DIR="D:/download/<app-slug>-v$VERSION"
mkdir -p "$DOWNLOAD_DIR" && cd "$DOWNLOAD_DIR"

echo "$ASSETS_JSON" | node -e "
const d = JSON.parse(require('fs').readFileSync(0, 'utf8'));
const r = d.find(x => x.tag_name === 'v$VERSION');
r.assets
  .filter(a => a.name.startsWith('<AppName>_'))   // 用产物前缀过滤，避免误下其他 release
  .forEach(a => console.log(a.id + '|' + a.name));
" | while IFS='|' read -r ID NAME; do
  echo "→ $NAME"
  curl -sL -H "Authorization: Bearer $TOKEN" -H "Accept: application/octet-stream" \
    "https://api.github.com/repos/$CI_OWNER_REPO/releases/assets/$ID" \
    -o "$NAME"
done

ls -lh "$DOWNLOAD_DIR/"
```

> **产物前缀过滤**（`<AppName>_`）是防御性的：万一同 tag 误推了其他文件（如桌面 + 移动同 tag），这里也只下载本流程需要的产物。

#### 4.5 备用：自动下载失败时回退手动

如果 token 过期 / API 限流 / 网络问题导致自动下载失败，**才**询问用户：

> 「自动下载失败，你能手动从浏览器下载到哪个目录？」

用户提供目录后跳到步骤 4.6 复用同一套预检逻辑。

#### 4.6 强制预检（自动 / 手动下载都跑）

> 自动下载用独立子目录已经隔离，理论上不会污染。但仍要跑预检——以防 GitHub API 偶发返回不全 / curl 中途网断只下了一半。

```bash
VERSION="x.y.z"
EXPECTED_COUNT=12   # 按 platforms 配置调整：Windows+macOS+Linux = 12; 仅 Windows+macOS = 8；移动 APK+AAB = 2

COUNT=$(ls "$DOWNLOAD_DIR"/<AppName>_* 2>/dev/null | wc -l)
if [ "$COUNT" -ne "$EXPECTED_COUNT" ]; then
  echo "❌ 产物数量异常: 实际 $COUNT 预期 $EXPECTED_COUNT"
  echo "   缺失 = CI 还没构建完 / 部分文件下载失败"
  echo "   过多 = 上一版本 .sig 没清掉(签名读取会被污染)"
  exit 1
fi

# 进一步：每种 .sig 必须正好 1 份（防止跨版本污染导致 cat 拼接非法 base64）
for pattern in "<AppName>_*x64-setup.exe.sig" "<AppName>_*aarch64.app.tar.gz.sig" \
               "<AppName>_*x64.app.tar.gz.sig" "<AppName>_*amd64.AppImage.sig"; do
  c=$(ls "$DOWNLOAD_DIR"/$pattern 2>/dev/null | wc -l)
  if [ "$c" -ne 1 ]; then
    echo "❌ 模式 $pattern 匹配到 $c 个文件(应正好 1 个)"
    ls "$DOWNLOAD_DIR"/$pattern 2>/dev/null
    exit 1
  fi
done
echo "✅ 预检通过"
```

**预检失败处置**：缺失 → 等 CI 完成后重新下载缺的；过多 → 删上一版本残留 / 删其他项目文件；强烈建议**用独立子目录**（如 `D:/download/<app-slug>-v$VERSION/`）天然隔离。

### 步骤 5：处理下载的产物 + 更新 README（Claude 自动执行）

用户提供下载目录后，Claude 自动执行以下操作：

```bash
VERSION="x.y.z"
DOWNLOAD_DIR="<用户提供的下载目录>"
GITEE_DIR="<本地 Gitee Release 仓库路径>"
GITHUB_DIR="<本地 GitHub Release 仓库路径>"

# === R2 配置（从 release-config.json 读取，r2.enabled 时才有值） ===
R2_ENABLED=<true/false>
R2_PUBLIC_URL="<r2.publicUrl>"          # 如 https://pub-xxx.r2.dev
RCLONE_REMOTE="<r2.rcloneRemote>"      # 如 r2
R2_BUCKET="<r2.bucket>"                # 如 downloads
R2_PREFIX="<r2.pathPrefix>"            # 如 myapp
RCLONE="$HOME/bin/rclone.exe"          # rclone 程序路径

# ========== 1. [可选] 上传产物到 R2 CDN（如果 r2.enabled） ==========
if [ "$R2_ENABLED" = "true" ]; then
  $RCLONE copy "$DOWNLOAD_DIR"/ ${RCLONE_REMOTE}:${R2_BUCKET}/${R2_PREFIX}/releases/v${VERSION}/ --progress \
    --include "*.exe" --include "*.exe.sig" --include "*.dmg" \
    --include "*.app.tar.gz" --include "*.app.tar.gz.sig" \
    --include "*.AppImage" --include "*.AppImage.sig" --include "*.deb"
fi

# ========== 2. 复制所有产物到两个 release 仓库 ==========
for DIR in "$GITEE_DIR" "$GITHUB_DIR"; do
  mkdir -p "$DIR/releases/v$VERSION"
  # 按 platforms 配置复制对应文件
  cp "$DOWNLOAD_DIR"/*.exe "$DIR/releases/v$VERSION/" 2>/dev/null         # windows
  cp "$DOWNLOAD_DIR"/*.exe.sig "$DIR/releases/v$VERSION/" 2>/dev/null     # windows
  cp "$DOWNLOAD_DIR"/*.dmg "$DIR/releases/v$VERSION/" 2>/dev/null         # macos
  cp "$DOWNLOAD_DIR"/*.app.tar.gz "$DIR/releases/v$VERSION/" 2>/dev/null  # macos
  cp "$DOWNLOAD_DIR"/*.app.tar.gz.sig "$DIR/releases/v$VERSION/" 2>/dev/null # macos
  cp "$DOWNLOAD_DIR"/*.AppImage "$DIR/releases/v$VERSION/" 2>/dev/null    # linux
  cp "$DOWNLOAD_DIR"/*.AppImage.sig "$DIR/releases/v$VERSION/" 2>/dev/null # linux
  cp "$DOWNLOAD_DIR"/*.deb "$DIR/releases/v$VERSION/" 2>/dev/null         # linux
done

# ========== 3. 读取签名文件，生成 update.json（仅包含已配置平台） ==========
#
# 🔴🔴🔴 这是整个发布最易出错、出错后果最严重的一步 🔴🔴🔴
# 签名（.sig 内容）错一个字符 → 所有用户自动更新报 "signature verification failed"。
# 必须严格遵守下面的 4 条注入规则，禁止任何形式的手动粘贴 base64 签名。
#
# 签名注入规则（违反会导致自动更新签名验证失败）：
#   1. 必须先用 shell 变量读取 .sig 文件内容（tr -d '\r\n' 去掉 Windows 换行）
#   2. 所有版本（R2/Gitee/GitHub）的 update.json 都必须用**双引号 heredoc**（<< JSONEOF，不带单引号），
#      通过 $VAR 注入签名 —— 这样 shell 自动替换变量，无需人工接触签名内容
#   3. 🚫 绝对禁止：单引号 heredoc（<< 'JSONEOF'）后手动粘贴签名（400+ 字符，一个字符差异即失败）
#   4. 生成后必须验证：对比 update.json 内签名与原始 .sig 文件完全一致

# --- 3a. 读取各平台签名（仅读 platforms 配置含有的平台）---
# 🔴 必须用 <AppName>_ 前缀过滤,不能用 *x64-setup.exe.sig 这种纯后缀通配符!
# 踩坑实录: cat *x64-setup.exe.sig 会把同目录下其他项目的 .sig 一起 cat 出来,
# 拼接成 800+ 字符的非法 base64,Tauri updater 报 "Invalid symbol 61, offset 426"。
# 强烈建议下载用独立子目录(步骤 4.4 的 D:/download/<app-slug>-v$VERSION/)天然隔离。
WIN_SIG=$(cat "$DOWNLOAD_DIR"/<AppName>_*x64-setup.exe.sig 2>/dev/null | tr -d '\r\n')      # windows
MAC_ARM_SIG=$(cat "$DOWNLOAD_DIR"/<AppName>_*aarch64.app.tar.gz.sig 2>/dev/null | tr -d '\r\n') # macos arm
MAC_X64_SIG=$(cat "$DOWNLOAD_DIR"/<AppName>_*x64.app.tar.gz.sig 2>/dev/null | tr -d '\r\n')  # macos intel
LINUX_SIG=$(cat "$DOWNLOAD_DIR"/<AppName>_*amd64.AppImage.sig 2>/dev/null | tr -d '\r\n')    # linux

# --- 3b. 防御性校验①: 每个签名 = 字符数 ≤ 2（只有末尾的 base64 padding）---
# 超过 2 个 = 说明 cat 拼接了多个文件或签名格式异常,立即中止。
# 只校验 platforms 配置含有的平台(未配置平台的变量为空,跳过)。
for var_name in WIN_SIG MAC_ARM_SIG MAC_X64_SIG LINUX_SIG; do
  sig_value=$(eval echo "\${$var_name}")
  [ -z "$sig_value" ] && continue
  eq_count=$(echo -n "$sig_value" | tr -cd '=' | wc -c)
  if [ "$eq_count" -gt 2 ]; then
    echo "❌ $var_name 含 $eq_count 个 = 字符(应 ≤ 2),可能 cat 了多个 sig 文件"
    echo "   下载目录是否混入其他项目的 .sig? 自查: ls $DOWNLOAD_DIR/*.sig"
    exit 1
  fi
done

# --- 3c. 防御性校验②: node 真实 base64 解码（终极兜底，唯一可靠验证）---
# 即使 = 字符通过,也可能含其他非法字符(空格/中文/控制字符)。base64 真解码 + 重编码比对才可靠。
for var_name in WIN_SIG MAC_ARM_SIG MAC_X64_SIG LINUX_SIG; do
  sig_value=$(eval echo "\${$var_name}")
  [ -z "$sig_value" ] && continue
  SIG="$sig_value" node -e "
    try {
      const buf = Buffer.from(process.env.SIG, 'base64');
      if (buf.toString('base64') !== process.env.SIG.trim()) {
        console.error('❌ 签名含非 base64 字符或格式异常 (re-encode mismatch)'); process.exit(1);
      }
      // Tauri ed25519 签名 + metadata,总长一般 200-800 字节
      if (buf.length < 200 || buf.length > 800) {
        console.error('❌ 签名解码后字节数异常:', buf.length, '(预期 200-800)'); process.exit(1);
      }
    } catch (e) { console.error('❌ base64 解码失败:', e.message); process.exit(1); }
  " || { echo "签名变量 $var_name 验证失败"; exit 1; }
done
echo "✅ 所有签名 base64 解码验证通过"

# --- 3d. 用 node 安全转义"更新说明"作为 notes（多行用 \n,禁止直接字符串拼接）---
RELEASE_NOTES="<本次发布说明，与步骤 1 询问用户时一致，可多行用 \n 分隔>"
NOTES_JSON=$(NOTES="$RELEASE_NOTES" node -e "process.stdout.write(JSON.stringify(process.env.NOTES))")

# --- 3e. 统一生成函数：3 个版本只传不同 BASE_URL，确保签名/结构完全一致 ---
# 🔴 platforms 配置不含某平台时,删除函数内对应的 "<target>": {...} 块。
generate_update_json() {
  local BASE_URL="$1"
  local OUTPUT="$2"
  cat > "$OUTPUT" << JSONEOF
{
  "version": "$VERSION",
  "notes": $NOTES_JSON,
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {
    "windows-x86_64": {
      "url": "$BASE_URL/<AppName>_${VERSION}_x64-setup.exe",
      "signature": "$WIN_SIG"
    },
    "darwin-aarch64": {
      "url": "$BASE_URL/<AppName>_aarch64.app.tar.gz",
      "signature": "$MAC_ARM_SIG"
    },
    "darwin-x86_64": {
      "url": "$BASE_URL/<AppName>_x64.app.tar.gz",
      "signature": "$MAC_X64_SIG"
    },
    "linux-x86_64": {
      "url": "$BASE_URL/<AppName>_${VERSION}_amd64.AppImage",
      "signature": "$LINUX_SIG"
    }
  }
}
JSONEOF
}

# URL 基准（仅生成 platforms 含有的；R2 仅 r2.enabled 时）
GITEE_BASE="https://gitee.com/<用户名>/<项目名>-release/raw/master/releases/v${VERSION}"
GITHUB_BASE="https://github.com/<用户名>/<项目名>-release/raw/master/releases/v${VERSION}"
generate_update_json "$GITEE_BASE"  "$GITEE_DIR/update.json"
generate_update_json "$GITHUB_BASE" "$GITHUB_DIR/update.json"
if [ "$R2_ENABLED" = "true" ]; then
  R2_UPDATE_BASE="${R2_PUBLIC_URL}/${R2_PREFIX}/releases/v${VERSION}"
  generate_update_json "$R2_UPDATE_BASE" "/tmp/update-r2.json"
fi

# --- 3f. 🔴 生成后强制验证签名一致性（防止写入错误，必须执行）---
TARGETS="$GITEE_DIR/update.json $GITHUB_DIR/update.json"
[ "$R2_ENABLED" = "true" ] && TARGETS="$TARGETS /tmp/update-r2.json"
for f in $TARGETS; do
  SIG_IN_JSON=$(node -e "const d=JSON.parse(require('fs').readFileSync('$f','utf8')); console.log(d.platforms['windows-x86_64'].signature)" | tr -d '\r\n')
  if [ "$SIG_IN_JSON" != "$WIN_SIG" ]; then
    echo "❌ 签名不匹配: $f"; exit 1
  fi
done
echo "✅ 所有 update.json 签名一致性验证通过"

# ========== 4. [可选] 上传 R2 版 update.json + versions.json（如果 r2.enabled） ==========
if [ "$R2_ENABLED" = "true" ]; then
  $RCLONE copyto /tmp/update-r2.json ${RCLONE_REMOTE}:${R2_BUCKET}/${R2_PREFIX}/update.json --progress

  # ========== 4b. [可选] 更新 R2 版本列表（文档站下载页依赖此文件） ==========
  # versions.json 格式: {"versions": ["v2.8.2", "v2.8.1", ...]}（新版本插数组头部）
  curl -s "${R2_PUBLIC_URL}/${R2_PREFIX}/versions.json" -o /tmp/versions.json 2>/dev/null || echo '{"versions":[]}' > /tmp/versions.json
  node -e "
    const fs=require('fs'); const p='/tmp/versions.json';
    let d; try{ d=JSON.parse(fs.readFileSync(p,'utf8')); }catch(e){ d={versions:[]}; }
    d.versions = d.versions || [];
    if(!d.versions.includes('v$VERSION')) d.versions.unshift('v$VERSION');
    fs.writeFileSync(p, JSON.stringify(d,null,2));
  "
  $RCLONE copyto /tmp/versions.json ${RCLONE_REMOTE}:${R2_BUCKET}/${R2_PREFIX}/versions.json --progress
fi

# ========== 5. 更新两个 release 仓库的 README.md（三处更新） ==========
#    - 最新版本下载表格（版本号 + 多平台链接）
#    - 版本历史（添加新版本条目）
#    - 项目结构树（添加新版本目录）
#    两个仓库的 README.md 内容一致，同步更新
Edit "$GITEE_DIR/README.md"
Edit "$GITHUB_DIR/README.md"
```

**下载表格模板**（根据 platforms 配置选择包含哪些平台）：

```markdown
### 最新版本: vx.y.z

| 平台 | 下载链接 |
|------|---------|
| Windows x64 | [AppName_x.y.z_x64-setup.exe](releases/vx.y.z/AppName_x.y.z_x64-setup.exe) |
| macOS Apple Silicon | [AppName_x.y.z_aarch64.dmg](releases/vx.y.z/AppName_x.y.z_aarch64.dmg) |
| macOS Intel | [AppName_x.y.z_x64.dmg](releases/vx.y.z/AppName_x.y.z_x64.dmg) |
| Linux | [AppName_x.y.z_amd64.AppImage](releases/vx.y.z/AppName_x.y.z_amd64.AppImage) |
```

**项目结构树模板**（根据 platforms 配置选择包含哪些文件）：

```
    └── vx.y.z/         # vx.y.z 版本
        ├── AppName_x.y.z_x64-setup.exe           # Windows 安装包
        ├── AppName_x.y.z_x64-setup.exe.sig       # Windows updater 签名
        ├── AppName_x.y.z_aarch64.dmg             # macOS Apple Silicon
        ├── AppName_aarch64.app.tar.gz            # macOS ARM updater 产物
        ├── AppName_aarch64.app.tar.gz.sig        # macOS ARM updater 签名
        ├── AppName_x.y.z_x64.dmg                 # macOS Intel
        ├── AppName_x64.app.tar.gz               # macOS Intel updater 产物
        └── AppName_x64.app.tar.gz.sig           # macOS Intel updater 签名
```

**update.json 模板**（根据 platforms 配置选择包含哪些平台）：

```json
{
  "version": "x.y.z",
  "notes": "Release vx.y.z",
  "pub_date": "2026-03-10T12:00:00Z",
  "platforms": {
    "windows-x86_64": { ... },      // ← platforms 含 windows
    "darwin-aarch64": { ... },       // ← platforms 含 macos
    "darwin-x86_64": { ... },        // ← platforms 含 macos
    "linux-x86_64": { ... }          // ← platforms 含 linux
  }
}
```

> **注意**：各版本 update.json 只有 URL 中的 `<BASE>` 不同。
> - R2（如果启用）: `<r2.publicUrl>/<r2.pathPrefix>/releases/vx.y.z`
> - Gitee: `https://gitee.com/<用户名>/<项目名>-release/raw/master/releases/vx.y.z`
> - GitHub: `https://github.com/<用户名>/<项目名>-release/raw/master/releases/vx.y.z`
>
> R2 版 update.json 上传到 R2 CDN 作为主更新端点，Gitee 版写入 Gitee release 仓库作为备用。

### 步骤 6：推送 release 仓库（README + 产物 + update.json）

> **推送前必须先拉取**：上一版本可能已推送产物到远程，本地可能落后。
> **推送超时处理**：release 仓库包含大量二进制文件，`git push` 可能超时。如果推送失败（SSL_ERROR_SYSCALL / RPC failed / hung up），**不要重试**，直接提示用户手动执行推送命令，然后继续后续步骤。

```bash
# === Gitee release 仓库 ===
cd "$GITEE_DIR"
git add -A
git commit -m "release: v$VERSION

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
git pull --rebase origin master
git push origin master

# === GitHub release 仓库 ===
cd "$GITHUB_DIR"
git add -A
git commit -m "release: v$VERSION

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
git pull --rebase origin master
git push origin master
```

### 步骤 6.5：触发文档站重建（可选，仅文档站消费 R2 versions.json 时）

> **何时需要**：项目有 VitePress 文档站，且下载页（`DownloadSection.vue` 之类）在**构建时**
> 从 R2 拉 `versions.json` 嵌入静态快照。此时仅更新 R2 的 versions.json 还不够——
> 文档站是上次构建的旧快照，必须重新构建才能拿到新版本列表。
> **纯桌面 + 无文档站 / 文档站运行时直接 fetch R2 的项目可跳过本步。**

```bash
# 写一个版本标记文件制造真实 diff（比空 commit 更稳，能确实触发 Pages 重建）
# 文件放文档站源码仓库的 docs/public/ 下，构建时一并发布但不影响页面
cat > docs/public/.last-release.json << JSONEOF
{
  "version": "v$VERSION",
  "released_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
JSONEOF

git add docs/public/.last-release.json
git commit -m "chore: 同步 v$VERSION 发布（触发下载页快照重建）

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
git push origin master   # 推到文档站托管分支（Gitee Pages / GitHub Pages / Cloudflare Pages）
```

> 推送后托管平台自动重新构建文档站，下载页在构建期重新拉取 R2 `versions.json`，
> 新版本作为快照嵌入。**移动端**对应 `docs/public/.last-release-mobile.json`（与桌面平级，
> 写移动端版本号，消费 `mobile-versions.json`），见下方"双线发布"章节。

### 步骤 7：完成报告

```markdown
## 发布完成

| 项目 | 值 |
|------|-----|
| 版本 | vx.y.z |
| 支持平台 | <从 platforms 配置读取> |
| 源码仓库 | 已推送到 <GitHub URL> |
| CI 构建 | 已完成，产物已上传到 GitHub Release |
| R2 CDN | 产物 + update.json 已上传（如果 r2.enabled，否则显示"未启用"） |
| Release 仓库（Gitee） | 产物 + update.json 已推送 |
| Release 仓库（GitHub） | 产物 + update.json 已推送 |
| 文档站下载页 | 已推送 .last-release.json 触发重建（如有文档站，否则显示"无"） |
| 应用内自动更新 | R2 主 + Gitee 备，双端点已生效（如果 r2.enabled）/ Gitee 端点已生效（如果 r2 未启用） |
```

---

## CI 构建流程

### 概述

通过 GitHub Actions 在云端自动构建安装包并签名，无需本地构建。
CI **只负责构建和上传到 GitHub Release**，不负责推送到 release 仓库。
构建矩阵由 `platforms` 配置决定。

### 工作流文件

`.github/workflows/release.yml`

### 触发方式

推送 `v*.*.*` 格式的 Git Tag 时自动触发：

```bash
git tag v0.2.0
git push <github_remote> v0.2.0
```

### 构建矩阵（按 platforms 配置）

| 平台 | Runner | Bundle 参数 | Updater 产物 | 安装包产物 | platforms 值 |
|------|--------|-------------|-------------|-----------|-------------|
| Windows | `windows-latest` | `--bundles nsis` | `.exe` + `.exe.sig` | `.exe` (NSIS) | `windows` |
| macOS (ARM) | `macos-latest` | `--bundles app,dmg` | `.app.tar.gz` + `.sig` | `.dmg` (aarch64) | `macos` |
| macOS (Intel) | `macos-latest` | `--bundles app,dmg` | `.app.tar.gz` + `.sig` | `.dmg` (x86_64) | `macos` |
| Linux | `ubuntu-22.04` | `--bundles deb,appimage` | `.AppImage` + `.sig` | `.deb` + `.AppImage` | `linux` |

> **macOS 必须包含 `app` bundle**
> - `dmg` 只生成安装用的 DMG 镜像，**不生成 updater 产物**
> - `app` 生成 `.app` 应用包，Tauri 会自动打包为 `.app.tar.gz` 并签名
> - 正确写法：`--bundles app,dmg`（先 app 再 dmg）

### 签名说明

- CI 构建时自动使用 `TAURI_SIGNING_PRIVATE_KEY` 进行签名
- **签名文件（`.sig`）已包含在 CI 产物中**，用户只需下载即可
- 用户不需要在本地做任何签名操作
- Claude 读取 `.sig` 文件内容来生成 `update.json`

---

## 密钥管理

### 重新生成密钥（需手动执行）

```bash
pnpm tauri signer generate -w src-tauri/keys/tauri-updater.key
# 密码提示时直接按两次回车（空密码）
```

**重新生成后必须：**
1. 更新 `tauri.conf.json` 中的 `pubkey`（读取 `.key.pub` 文件内容）
2. 更新 GitHub Secrets 中的 `TAURI_SIGNING_PRIVATE_KEY`（读取 `.key` 文件内容）
3. 重新构建并发布（旧版本的签名将不可用，但不影响已安装用户）

### 安全提醒

- **私钥 (`tauri-updater.key`) 绝不能提交到公开仓库**
- `src-tauri/keys/` 应加入主项目的 `.gitignore`

---

## Cloudflare R2 CDN 配置（可选）

> 以下内容仅在 `release-config.json` 中 `r2.enabled = true` 时适用。

### R2 目录规划（支持多项目）

```
<bucket>/                              ← Bucket 根目录（如 downloads）
├── <pathPrefix>/                      ← 项目隔离目录（如 myapp）
│   ├── releases/vX.Y.Z/              ← 版本产物
│   └── update.json                   ← Tauri 自动更新端点
├── other-project/                     ← 其他项目
│   └── releases/
└── shared/                            ← 共享资源
```

### 自定义域名（可选）

> 当前使用 R2.dev 公开 URL（如 `https://pub-xxx.r2.dev`），功能完全等价。
> 如需绑定自定义域名（如 `dl.example.com`），需要：
> 1. 将域名 DNS 迁移到 Cloudflare（NS 变更）
> 2. R2 Settings → Custom Domains → 添加子域名
> 3. 更新 `tauri.conf.json` 和 release-config.json 中的 URL
>
> **注意**：DNS 迁移会影响现有域名解析，需提前导入所有 DNS 记录。

### R2 成本（永久免费额度内）

| 项目 | 免费额度 | 实际用量（估） |
|------|---------|--------------|
| 存储 | 10 GB/月 | ~500MB（20 版本） |
| 上传操作 | 100万次/月 | ~50次/月 |
| 下载操作 | 1000万次/月 | ~2000次/月 |
| 出站流量 | **无限免费** | ~50GB/月 |

### rclone 配置方法

```bash
# 1. 安装 rclone（下载到 ~/bin/rclone.exe 或其他位置）
# 2. 配置 R2 remote
rclone config
# 选 "New remote" → 名称填 r2 → 类型选 "Cloudflare R2" → 填入 Access Key ID + Secret
# 配置完成后验证：
rclone ls r2:<bucket>/

# 3. 测试上传
rclone copy ./test.txt r2:<bucket>/<pathPrefix>/test/ --progress
```

> rclone 配置文件位于 `~/.config/rclone/rclone.conf`。

### Tauri updater 端点配置（R2 启用时）

```json
{
  "plugins": {
    "updater": {
      "endpoints": [
        "<r2.publicUrl>/<r2.pathPrefix>/update.json",
        "https://gitee.com/<用户名>/<项目名>-release/raw/master/update.json"
      ]
    }
  }
}
```

> R2 作为第一端点（主源），Gitee 作为第二端点（备源）。Tauri updater 会按顺序尝试，第一个失败自动 fallback。

---

## 移动端 Android 发布要点

> 以下针对 `mobile-tauri` 子项目（Tauri 2.x Android target）。Android 与桌面端发布完全解耦，但有几个 Android 特有的坑必须提前防住。

### 1. Release keystore 必须在首次发版前生成并配置

**症状**：用户每次升级 APK 都被系统拦截「与已安装应用签名不同」，必须先卸载旧版才能装新版。

**根因**：CI workflow 没配 `ANDROID_KEYSTORE_BASE64` secret 时，gradle 会用 GitHub runner 临时生成的 `debug.keystore` 签名 → **每次构建签名都不同** → 每个版本对系统而言都是「另一个 app」。

**正确做法**（首次发版前一次性配置）：

```bash
# 1. 本地生成稳定 release keystore（验证期 100 年，密码随机生成 22 字符）
KEYSTORE_PWD=$(openssl rand -base64 24 | tr -d '+/=' | head -c 22)
keytool -genkeypair -v \
  -keystore <app>-release.keystore \
  -alias <app> -keyalg RSA -keysize 4096 -validity 36500 \
  -storepass "$KEYSTORE_PWD" -keypass "$KEYSTORE_PWD" \
  -dname "CN=<App Name>, O=<Org>, C=CN"

# 2. 转 base64（注入 GitHub Secret）
base64 -w0 <app>-release.keystore > keystore.b64

# 3. 在每个 CI 仓库 Settings → Secrets 配置 4 个 secret：
#    ANDROID_KEYSTORE_BASE64    = <keystore.b64 内容>
#    ANDROID_KEYSTORE_PASSWORD  = $KEYSTORE_PWD
#    ANDROID_KEY_ALIAS          = <app>
#    ANDROID_KEY_PASSWORD       = $KEYSTORE_PWD
```

**workflow 配置**（在 release-android job 里）：

```yaml
- name: Setup release keystore
  if: env.HAS_KEYSTORE == 'true'
  env:
    HAS_KEYSTORE: ${{ secrets.ANDROID_KEYSTORE_BASE64 != '' }}
  run: |
    echo "${{ secrets.ANDROID_KEYSTORE_BASE64 }}" | base64 -d > release.keystore
    # 把密钥注入 gradle.properties 或 signing config
```

> **多 CI 仓库 fallback 注意**：每个 CI 仓库都要独立配置 4 个 secret。secret 不跨仓共享，否则备仓 build 时 `if: env.HAS_KEYSTORE == 'true'` 判断为 false，step 被 skip，又退回 debug 签名（CI 显示绿色但产物用错签名，最容易被忽略的失败模式）。

> **keystore 备份**：keystore 文件 + 密码必须异地备份（云盘 + 离线 U 盘）。**丢了无法找回**，会迫使所有用户卸载重装。

### 2. 发布前用 apksigner 验证签名（防止 build skipped 静默退化）

**症状**：CI 显示绿色但产物用的是临时 debug 签名（step 13 被 skip 了不抛错）。

**做法**：发布前对每个 APK 做一次签名指纹比对（与 keystore 期望指纹一致才推 R2）：

```bash
# 期望指纹（一次性记录到 keystore 配置文件）
EXPECTED=$(keytool -list -v -keystore release.keystore -storepass "$PWD" \
  -alias "$ALIAS" | grep "SHA256:" | awk -F'SHA256:' '{print $2}' | tr -d ' ')

# 验证下载的 APK
ACTUAL=$("$ANDROID_SDK/build-tools/<ver>/apksigner" verify --print-certs <apk> \
  | grep "Signer #1 certificate SHA-256 digest:" | awk '{print $NF}')

[ "$ACTUAL" = "$EXPECTED" ] || { echo "❌ 签名不匹配，禁止发布"; exit 1; }
```

### 3. mobile- 前缀拼接陷阱（双重 mobile 404）

Android 发版常用 tag 格式 `mobile-vX.Y.Z`（与桌面 `vX.Y.Z` 区分）。模板/脚本里如果直接把整个 tag 拼到文件名后面，会得到 `<App>-mobile-mobile-v0.3.4.apk` 这种双 mobile 路径 → R2/Gitee 404。

**正确做法**：先剥前缀再拼：

```bash
# bash
VER="mobile-v0.3.4"
FILE_VER="${VER#mobile-}"   # → v0.3.4
APK="<App>-mobile-${FILE_VER}.apk"
```

```typescript
// TS / JS
const fileVer = ver.replace(/^mobile-/, "");
const apk = `<App>-mobile-${fileVer}.apk`;
```

**同源陷阱**：移动端 `parseSemver` 也常忘记剥 `mobile-` 前缀，导致 `mobile-v0.3.2` 经过 `replace(/^v/, "")` 不变 → 正则不匹配 → `compareSemver` 视为同版本 → 检查更新永远报「已是最新」。

```typescript
// ❌ 错的
function parseSemver(s: string) {
  const m = s.replace(/^v/, "").match(/^(\d+)\.(\d+)\.(\d+)/);
  // mobile-v0.3.2 不会被匹配
}

// ✅ 对的：先剥 mobile- 再剥 v
function parseSemver(s: string) {
  const cleaned = s.replace(/^mobile-/, "").replace(/^v/, "");
  return cleaned.match(/^(\d+)\.(\d+)\.(\d+)/);
}
```

### 4. Android versionCode 必须严格递增

`versionName` 是字符串可重复，但 `versionCode` 是整数，**比已安装版本大才允许覆盖安装**。常用约定：

```kotlin
// gen/android/app/build.gradle.kts
versionCode = X * 10000 + Y * 100 + Z   // v0.3.6 → 306
versionName = "0.3.6"
```

回退版本号（如 0.3.6 → 0.3.5）时 versionCode 必须仍递增（如 305 → 307），否则用户装不上。

---

## 常见问题排查

### 应用内更新问题

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| 应用检查不到更新 | release 仓库是私有的 | 将仓库设为公开，否则 raw 地址需认证 |
| 应用检查不到更新 | update.json 中版本号 <= 当前版本 | 确保 update.json 的 version 大于已安装版本 |
| 签名验证失败 | 公钥不匹配 | 确保 `tauri.conf.json` 中的 pubkey 与签名使用的私钥配对 |
| 签名验证失败 | update.json 中签名内容与 `.sig` 文件不一致 | **禁止手动粘贴签名**，必须用 shell 变量注入（步骤 5「3a~3f」的 `generate_update_json()`），生成后比对一致性 |
| 签名报 "Invalid symbol 61, offset 4xx" | `cat *.sig` 通配符把多个项目的 .sig 拼成非法 base64 | 用 `<AppName>_` 前缀过滤 .sig；下载用独立子目录隔离（步骤 4.4） |

### R2 CDN 问题（r2.enabled 时）

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| R2 下载失败 | R2.dev 域名偶尔被墙 | Tauri updater 自动 fallback 到 Gitee 备源端点 |
| rclone 上传失败 | Access Key 过期或 bucket 名错误 | 检查 `~/.config/rclone/rclone.conf` 中的 R2 配置 |
| R2 update.json 未更新 | rclone copyto 命令未执行 | 检查 `release-config.json` 中 `r2.enabled` 是否为 `true` |
| R2 产物 URL 404 | pathPrefix 或 bucket 名不匹配 | 确认 `r2.publicUrl` + `r2.pathPrefix` 与 rclone 上传路径一致 |
| 自定义域名不生效 | DNS 未迁移到 Cloudflare | 使用 R2.dev 公开 URL 作为替代 |

### Git 推送问题

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| Release 仓库 push rejected | 上一版本已推送产物到远程，本地落后 | **先 `git pull --rebase origin master` 再 push** |

### CI 构建问题（踩坑总结）

| 问题 | 根因 | 解决方案 |
|------|------|---------|
| macOS updater 产物缺失 | `--bundles dmg` 不生成 updater 产物 | **必须用 `--bundles app,dmg`** |
| Linux 编译 unused import 警告 | `#[cfg(target_os = "windows")]` 下的 import 在 Linux 不使用 | 将 import 也放在 `#[cfg()]` 块内 |
| CI 推送 Gitee 超时 | GitHub Actions（美国）推送到 Gitee（中国）太慢 | **已改为本地推送**，不再由 CI 推送 |
| GitHub API 用 PAT 拿 release 返回 404 / 看不到刚 build 的 release | CI 创建的是 `draft: true` release，fine-grained PAT 仅有 `Contents: Read` 看不到 draft | PAT 权限升级为 `Contents: Read and write` + `Actions: Read`；下载 asset 时也必须用 `Authorization: Bearer <token>` + `Accept: application/octet-stream` 调 API（普通浏览器 URL 无法下载 draft asset） |
| 私有仓 CI 推 tag 后 3-5 秒就失败，无任何 step 输出 | GitHub Actions 私有仓每月免费分钟数耗尽（macOS 10 倍倍率最容易超） | 切到备用 CI 仓库（见步骤 5.1 多 CI 仓库 fallback）；下月 1 号自动重置 |

---

## 附录：本地构建（仅在 CI 不可用时使用）

> 正常发布流程使用 CI，以下仅作为 CI 不可用时的备用方案。

### Windows 本地签名构建

> **Windows 环境变量设置注意事项**
>
> Claude Code 的 Bash 工具运行在 Git Bash (MSYS2) 环境中。
> - **正确**：`export VAR=value && command`（bash export 语法）
> - **失败**：`set VAR=value && command`（CMD 语法在 bash 中无效）
> - **失败**：`$env:VAR='value'; command`（PowerShell 语法）

```bash
# 读取私钥并构建（单条 Bash 调用）
export TAURI_SIGNING_PRIVATE_KEY="<src-tauri/keys/tauri-updater.key 文件完整内容>" && \
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" && \
pnpm tauri build 2>&1

# 构建超时设置：600000ms（10分钟）
# 建议后台运行：run_in_background: true
# 构建成功标志：输出末尾出现 `Finished 1 updater signature at:`
```

---

## 双线发布（桌面 + 移动）

> 适用项目：基于本框架开发了移动端伴侣（见 `mobile-app-architecture` skill）。
> 桌面单端项目可忽略本章。

### 总览：CI 分流（方案 B）

```
桌面端（v*.*.*）             移动端（mobile-v*.*.*）
   ↓ 推 v3.x.x tag                ↓ 推 mobile-v0.x.x tag
release.yml: release job       release.yml: release-android job
   ↓                                   ↓
仓库 releases/vX.Y.Z/            仓库 releases-mobile/mobile-vX.Y.Z/
+ 同步 versions.json            + 同步 mobile-versions.json
```

> **桌面 / 移动版本号独立**，可以分别迭代。桌面端不需要 APK 包就能发版；移动端不需要等桌面端构建完成。
> 文档站下载页同时展示两条版本线，分别是"桌面应用"和"移动伴侣"两个区块。

### 版本号同步清单

桌面端（保持原有 N 处）+ 移动端 4 处：

| 文件 | 字段 |
|------|------|
| `mobile-tauri/src-tauri/tauri.conf.json` | `"version": "x.y.z"` |
| `mobile-tauri/src-tauri/Cargo.toml` | `version = "x.y.z"` |
| `mobile-tauri/package.json` | `"version": "x.y.z"` |
| `mobile-tauri/src-tauri/gen/android/app/build.gradle.kts` | `versionName = "x.y.z"`、**`versionCode` 必须严格递增** |

> **🔴 关键**：移动端版本号与桌面端**独立**。桌面 v3.5.0 时移动端可能还在 v0.2.1，互不影响。
> versionCode 推荐公式：`major*10000 + minor*100 + patch`（0.3.6 → 306）。

### CI 分流写法

```yaml
on:
  push:
    tags:
      - 'v*.*.*'
      - 'mobile-v*.*.*'

jobs:
  release:
    if: ${{ !startsWith(github.ref_name, 'mobile-') }}   # ← 桌面 job 跳过 mobile-* tag
    # 桌面构建 (windows-latest + macos-latest)...

  release-android:
    if: ${{ startsWith(github.ref_name, 'mobile-') }}    # ← 移动 job 只在 mobile-* tag 上跑
    # Android 构建...
```

桌面端推 `v3.x.x` tag 只触发 `release` job，移动端推 `mobile-v0.x.x` tag 只触发 `release-android` job，互不干扰。

### 移动端发布特殊点

| 项 | 桌面 | 移动 |
|----|------|------|
| update.json | 生成 | **不生成**（Android 侧载分发不支持 Tauri updater 静默更新） |
| 产物 | NSIS / MSI / DMG / AppImage | APK / AAB |
| 用户安装 | 自动更新 | 手动下载侧载 |
| versionCode | — | **必须严格递增**（同号或更小 Android 拒绝覆盖安装） |

### 完整流程（移动端 mobile-v*.*.*）

```
本地：bump 4 处版本号 → commit → 打 mobile-v$VER tag → push tag
   ↓
CI 自动：tauri android build → 上传 APK / AAB 产物
   ↓
本地：下载产物 → 上传到对应 CDN/存储桶（与桌面端隔离的子目录）
       + 复制到 release 仓库 releases-mobile/
       + 更新 mobile-versions.json
       + 更新 .last-release-mobile.json（触发文档站重建）
```

### `<APP_NAME>_` 前缀过滤

发布产物文件名加项目自定义前缀（如 `<APP_NAME>_`），CI 复制脚本按前缀过滤，
避免桌面 release 不小心混入移动产物（同 tag 下载错文件）。

### 桌面发版前必须看 mobile-tauri/ 状态

mobile-tauri/ 有独立 `.gitignore` 但与主仓共用 `src/mobile/` 和 `src-tauri/`。桌面发版前
必须 `cd mobile-tauri && git status -s` 检查，避免遗漏未提交改动。

### 相关 skill

- `mobile-app-architecture` — 移动端架构选型与目录骨架
- `tauri-mobile-android` — Android 打包专项（NDK / versionCode / 签名）
