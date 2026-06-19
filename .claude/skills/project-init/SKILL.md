---
name: project-init
description: |
  当用户需要基于本 Tauri 框架创建新项目时自动使用此 Skill。提供完整的交互式项目初始化流程：模板更新检测、项目信息收集、目录创建、标识符替换、Git 仓库创建、签名密钥生成、启动引导。

  触发场景：
  - 用户说"我要开发一个新项目"或"创建一个新项目"
  - 需要基于 Tauri 框架模板初始化新的桌面应用
  - 需要修改项目标识符、名称和配置
  - 需要为新项目创建 Git 仓库并推送代码

  触发词：新项目、创建项目、初始化项目、开新项目、项目初始化、new project、init project、新建项目、开发新项目
---

# 新项目初始化指南

## 🔴 全局强制规则（最高优先级）

> **仓库可见性：源码私有，更新端点仓库公开（唯一例外）**
>
> **① 源码 / 主仓库**：无论平台（Gitee / GitHub / GitCode），都**必须**创建为**私有仓库**
> （`private: true`），**绝对禁止**公开。
> - 即使用户未指定可见性，默认也必须私有
> - API 调用中 `private: true` 必须硬编码，不可省略、不可通过变量覆盖
> - 该规则继承自全局 CLAUDE.md 的"仓库创建规范"章节
>
> **② release / updater 端点仓库**：**必须公开**（`private: false`）。
> - **技术硬约束**：Tauri updater 是用**明文 HTTP GET**（无 token、无认证）去拉 `update.json` 的 raw 地址。
>   端点仓库若是私有，Gitee / GitHub raw 地址需要认证 → updater 拿不到 → 应用永远"检查不到更新"。
> - **为什么这是安全的例外**：release 仓库**只含安装包 + update.json + 更新公钥**，
>   不含任何源码、不含签名私钥（公钥本就设计为可公开）。公开它不泄露任何敏感资产。
> - 这是全局"仓库一律私有"铁律的**唯一已知例外**，因技术必要性而存在，与 release-publish 技能保持一致。
> - 替代方案：若启用 Cloudflare R2（`r2.enabled`），主更新端点走 R2 公开桶，
>   则 Gitee/GitHub release 仓库仅作备源/存档，可视情况私有（但备源 raw 仍会 404，不推荐）。

## 概述

本技能用于基于 Tauri 桌面应用框架（模板仓库）创建全新的独立项目。**模板仓库始终保持不变**，所有操作在新目录中进行，支持反复创建新项目。

**核心理念**：模板仓库 = 只读源 → 复制到新目录 → 在新目录中初始化

**完整流程**：

```
阶段零：环境准备（交互式）
├── 0.1 检测模板仓库更新（git fetch）
├── 0.2 收集项目基本信息（名称/标识符/包名）
├── 0.3 收集发布配置（Git 仓库/更新地址）
└── 0.4 配置确认汇总

阶段一：创建新项目目录
├── 1.1 git archive 导出到新目录（包含所有 git 跟踪文件）
├── 1.2 在新目录初始化 Git（并关联 upstream）
└── 1.3 清理并整理模板文件（Cargo.lock / docs / LICENSE / 可选项）

阶段二：代码初始化（全局替换 + 框架文档重写）
├── 2.1 替换产品名称（Agile Tauri → 新名称）+ 重写 README.md
├── 2.2 替换应用标识符（com.agilefr.tauri → 新标识符）
├── 2.3 替换包名（tauri/tauri_lib → 新包名）
├── 2.4 替换作者和描述
├── 2.5 配置更新地址和签名
├── 2.6 更新框架文档中的引用（CLAUDE.md / AGENTS.md / commands/）
├── 2.7 配置唯一开发端口号
└── 2.8 验证替换结果

阶段三：Git 提交 & 推送
├── 3.1 初始提交
├── 3.2 创建远程仓库（自动通过 Gitee API / 手动）
└── 3.3 关联远程仓库并推送

阶段四：应用图标（可选）
├── 4.1 提示用户准备图标
└── 4.2 生成多尺寸图标

阶段五：启动引导
├── 5.1 安装依赖
├── 5.2 启动开发模式
└── 5.3 验证运行
```

---

## 阶段零：环境准备（交互式）

### Step 0.1：检测模板仓库更新

在模板仓库目录中执行：

```bash
# 拉取远程最新信息（仅元数据，不修改本地文件）
git fetch origin

# 检查与远程的差异
BEHIND=$(git rev-list HEAD..origin/master --count 2>/dev/null)
if [ "$BEHIND" -gt 0 ]; then
  echo "模板仓库有 $BEHIND 个新提交："
  git log HEAD..origin/master --oneline --no-merges
else
  echo "模板仓库已是最新"
fi
```

**展示更新摘要**：

```
模板仓库更新检测：

  master — 有 3 个新提交（最近：feat(rust): 添加文件管理模块...）

是否拉取最新代码？（推荐：是）
```

如果用户确认拉取：

```bash
git pull origin master
```

### Step 0.2：收集项目基本信息

**必须一次性询问用户以下信息**：

```
请提供新项目的基本信息：

1. 项目描述：开发什么应用？（简要描述业务场景）
2. 产品名称：应用窗口标题（英文，如 "Mall Admin"、"IoT Monitor"）
3. 产品名缩写：侧边栏折叠时显示（2-3 字母，如 "MA"、"IM"）
4. 应用标识符：反向域名格式（如 "com.mycompany.mall"）
5. 包名：用于 Cargo/npm（snake_case，如 "mall_admin"）
6. 作者：（如 "Zhang San"）

或者只告诉我项目名称，我来推荐配置。
```

**根据项目描述自动推荐**：

| 项目描述 | 推荐产品名 | 缩写 | 标识符 | 包名 |
|---------|-----------|------|--------|------|
| 电商管理系统 | Mall Admin | MA | com.company.mall | mall_admin |
| 物联网监控 | IoT Monitor | IM | com.company.iot | iot_monitor |
| CRM 客户管理 | CRM System | CS | com.company.crm | crm_system |
| 内部办公 | OA Office | OA | com.company.oa | oa_office |
| 博客管理 | Blog Admin | BA | com.company.blog | blog_admin |

**推荐话术示例**：

```
根据您的项目「电商管理系统」，建议配置：

- 产品名称：Mall Admin
- 产品名缩写：MA
- 应用标识符：com.company.mall
- 包名：mall_admin
- 作者：you

请确认或自定义。
```

**标识符/包名规则**：
- 应用标识符：反向域名格式，仅英文字母和点，全小写
- 包名：仅英文字母和下划线，全小写，不超过 20 字符
- 包名同时也是新项目的**目录名**

### Step 0.3：收集发布配置

> 🔴 **仓库可见性强制规则**：
> - **源码 / 主仓库**：**必须私有**（`private: true`），绝对禁止公开。继承自全局 CLAUDE.md。
> - **release / updater 端点仓库**：**必须公开**（`private: false`）。Tauri updater 用明文 HTTP 拉 raw 地址，
>   私有则应用永远检查不到更新；该仓库只含安装包 + update.json + 公钥，公开不泄露敏感资产（详见顶部"全局强制规则"）。

**必须询问用户**：

```
请选择 Git 仓库方式：
1. 自动创建 Gitee 仓库（源码仓库默认私有，需要 Gitee Token）
2. 提供已有的仓库地址（Gitee/GitHub）
3. 稍后手动创建

⚠️ 源码 / 主仓库强制「私有」，不支持公开可见。

更新服务配置（用于应用自动更新）：
1. 提供 release 仓库地址（如 https://gitee.com/user/myapp-release）
   ⚠️ release 仓库必须「公开」，否则应用自动更新拉不到 update.json
2. 稍后配置（更新功能暂不可用）
```

> **说明**：本框架使用 Gitee/GitHub 静态文件托管 update.json 作为更新端点。
> release 仓库是独立的仓库，CI 构建完成后自动推送安装包和 update.json 到该仓库。

**自动创建 Gitee 仓库的前提**：

需要 Gitee Private Token，检查 `~/.gitee_token` 文件是否存在：
- 如果存在 → 直接读取，无需用户提供
- 如果不存在 → 提示用户前往 `https://gitee.com/profile/personal_access_tokens/new` 生成（勾选 `projects` 权限），然后保存到 `~/.gitee_token`

**自动检测 Gitee 用户名**：从模板仓库的 `git remote get-url origin` 中提取，或通过 Gitee API `GET /api/v5/user` 获取。

### Step 0.4：配置确认汇总

在开始执行前，向用户展示完整的配置汇总：

```
━━━━━━━━━━ 项目初始化配置确认 ━━━━━━━━━━

  产品名称：Mall Admin
  产品名缩写：MA
  应用标识符：com.company.mall
  包名：mall_admin
  Cargo lib 名：mall_admin_lib
  作者：Zhang San

  新目录：{模板仓库同级}/mall_admin
  开发端口：{dev_port}（HMR: {hmr_port}）
  Git 仓库：https://gitee.com/user/mall_admin.git  [私有 🔒]
  Release 仓库：https://gitee.com/user/mall_admin-release.git  [公开 🌐 更新端点需匿名可读]

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

确认无误后开始初始化？(Y/n)
```

---

## 阶段一：创建新项目目录

### Step 1.1：使用 git archive 导出到新目录

**核心命令**：`git archive` 从当前分支导出文件，**无需切换分支**，自动排除 `.git/` 和未跟踪文件。

```bash
TEMPLATE_DIR="$(pwd)"
PARENT_DIR="$(dirname "$TEMPLATE_DIR")"
NEW_DIR="$PARENT_DIR/{包名}"

# 检查新目录是否已存在
if [ -d "$NEW_DIR" ]; then
  echo "目录 $NEW_DIR 已存在！请确认是否覆盖。"
  exit 1
fi

# 创建新目录并导出
mkdir -p "$NEW_DIR"
cd "$TEMPLATE_DIR"
git archive HEAD | tar -x -C "$NEW_DIR"
```

**git archive 的优势**：

| 对比项 | git archive | 手动复制 |
|--------|-------------|---------|
| 是否需要切换分支 | 不需要 | 不需要 |
| 是否排除 .git/ | 自动排除 | 需手动排除 |
| 是否排除 node_modules/ | 自动排除（未跟踪） | 需手动排除 |
| 是否排除 .claude/projects/ | 自动排除（未跟踪） | 需手动排除 |
| Windows 兼容性 | Git Bash 自带 | 需要 robocopy |
| 模板仓库是否受影响 | 完全不受影响 | 完全不受影响 |

**自动排除的内容**（未被 git 跟踪）：

| 排除项 | 原因 |
|--------|------|
| `.git/` | git archive 不导出 .git 目录 |
| `node_modules/` | 在 .gitignore 中 |
| `src-tauri/target/` | 在 .gitignore 中 |
| `.claude/projects/` | 个人 memory 数据，未跟踪 |
| `dist/` | 构建产物，未跟踪 |

**自动包含的内容**：所有**已被 git 跟踪**的文件（`git ls-files` 能列出的）都会被完整导出，不需要逐项列举。以下是几个值得**额外关注**的项，说明其在后续初始化步骤中的处理方式：

| 保留项 | 后续处理 |
|--------|---------|
| `.claude/` （除 `projects/` 未跟踪外） | 技能、命令、hook、配置全部保留；Step 2.2 会更新命令模板里的标识符引用 |
| `.codex/` | Codex 技能镜像，仅用 Claude Code 的项目可在 Step 1.3 中删除 |
| `CLAUDE.md` / `AGENTS.md` | 项目规范文档，Step 2.2 需替换其中的 `com.agilefr.tauri` |
| `README.md` | 当前是**框架介绍**，Step 2.1 中需**整体重写**为新项目简介 |
| `LICENSE` | 模板的**专有商业许可证**（湛江市麻章区湖光镇若依科技工作室版权），统一覆盖框架和所有衍生项目，**保留不动** |
| `docs/development-guide.md` | 框架开发指南，**不适合**作为新项目文档，Step 1.3 中删除 |
| `templates/docs-template/` | VitePress 文档站模板，仅在新项目需要文档站时保留 |
| `.github/workflows/release.yml` | CI 发布配置，保留（release 仓库通过 Step 2.5 的 endpoints 关联） |

### Step 1.2：在新目录初始化 Git 并关联模板仓库

```bash
cd "$NEW_DIR"
git init
git checkout -b master

# 将模板仓库设置为 upstream remote，方便后续对比框架更新
# 获取模板仓库的 remote URL（自动检测）
TEMPLATE_REMOTE=$(cd "$TEMPLATE_DIR" && git remote get-url origin 2>/dev/null)
if [ -n "$TEMPLATE_REMOTE" ]; then
  git remote add upstream "$TEMPLATE_REMOTE"
  echo "已设置 upstream: $TEMPLATE_REMOTE"
fi
```

> **默认使用全新 Git 历史**：新项目不需要框架的开发历史，一个干净的初始提交更合理。
>
> **upstream 的作用**：新项目通过 `upstream` remote 关联模板仓库，日后可以方便地对比框架更新：
> ```bash
> # 查看模板仓库有哪些新提交
> git fetch upstream
> git log master..upstream/master --oneline
>
> # 对比具体差异
> git diff master...upstream/master
>
> # 选择性合并框架更新（谨慎操作）
> git cherry-pick <commit-hash>
> ```

### Step 1.3：清理并整理模板文件

**必做清理**：

```bash
cd "$NEW_DIR"

# 1. 删除 Cargo.lock（新包名后需要重新生成）
rm -f src-tauri/Cargo.lock

# 2. 删除框架开发指南（是框架自身文档，不适合作为新项目文档）
rm -f docs/development-guide.md
```

**LICENSE 处理**：

模板 `LICENSE` 是湛江市麻章区湖光镇若依科技工作室的**专有商业许可证**，该许可证的授权主体已经过通用化处理，可统一覆盖框架本身及所有基于框架构建的衍生桌面应用。

| 行为 | 操作 | 说明 |
|------|------|------|
| **默认** | **原样保留**，**版权人保持为若依科技工作室**，**不要替换** | 框架与全部衍生项目共用同一份授权条款；即使项目归属于同一体系内的其他作者，也应保持版权声明一致 |
| 例外 | 若新项目归属第三方（非若依体系），需重新起草独立的 LICENSE | 此时须完全替换 LICENSE 文件内容，而不是简单修改版权人 |

> ⚠️ **重要**：不要把 LICENSE 误认作 "MIT"。它是一份"授权用户 / 最终客户链式授权"的专有商业许可证，详见文件内容。

**可选清理**（按新项目实际需要）：

```bash
# 3. 如果只使用 Claude Code，不使用 Codex，可删除 Codex 镜像
# rm -rf .codex

# 4. 如果新项目不需要 VitePress 文档站，可删除模板
# rm -rf templates/docs-template

# 5. 删除 prototype/ 等模板仓库的原型/临时目录（如果存在）
# 注：git archive 已自动排除未跟踪文件，通常无需处理
```

**.github/workflows/release.yml 说明**：
- **保留**，新项目的 CI 发布流程与模板一致
- Release 仓库地址通过 Step 2.5 的 `updater.endpoints` 字段关联，**不需要**修改 `release.yml` 本身
- 如果 `release.yml` 内硬编码了 release 仓库名，则在 Step 2.5 中一并替换

### Step 1.4：剪切需求文档和原型图到新项目 ★

> **关键步骤**：用户在模板仓库做完头脑风暴 + 用 AI 工作站设计原型图后，这些资产应当**剪切**（move，不是复制）到新项目，让模板仓库回到干净基线状态，便于下次启动新项目。

#### 1.4.1 扫描模板仓库的潜在资产目录

按优先级扫描以下目录，**任何非空目录都需处理**：

```bash
TEMPLATE_DIR="$(pwd)"   # 上一步已 cd 到 NEW_DIR，需要回到模板仓库扫描
# 注意：以下操作的对象是【模板仓库】，不是新项目目录

ASSET_DIRS=(
  "prototype"               # AI 工作站默认输出原型图目录
  "docs/原型"               # 中文路径变体
  "docs/prototype"          # 英文路径变体
  "docs/requirements"       # 需求文档（英文）
  "docs/需求"               # 需求文档（中文）
  "docs/设计"               # 设计稿（中文）
  "docs/design"             # 设计稿（英文）
  "docs/brand"              # 品牌资产（logo，由工作站 logo-studio 出，兼作 app 图标源）
  ".claude/docs/brainstorm" # 头脑风暴会话沉淀
)

# 扫描非空目录
for d in "${ASSET_DIRS[@]}"; do
  if [ -d "$TEMPLATE_DIR/$d" ] && [ -n "$(ls -A "$TEMPLATE_DIR/$d" 2>/dev/null)" ]; then
    echo "发现非空资产目录：$d"
  fi
done
```

#### 1.4.2 询问用户确认剪切

```
检测到模板仓库存在以下设计资产：

  ✅ prototype/                  3 个文件 (商品列表.html / 订单详情.html / 个人中心.html)
  ✅ docs/requirements/          2 个文件 (PRD-v1.0.md / 用户故事.md)
  ⚠️  docs/原型/                1 个文件 (架构图.png)

是否将这些资产**剪切**（move）到新项目 mall_admin？

  操作后：
  - 资产将出现在新项目对应位置（docs/requirements/ 与 prototype/）
  - 模板仓库恢复为空目录（保留目录结构，便于下次复用）
  - 新项目首次提交会包含这些资产

  推荐：是（让新项目带着完整设计文档启动）

  选项：
    Y - 全部剪切（默认）
    N - 不剪切，资产保留在模板仓库
    P - 仅剪切原型（prototype/ docs/原型/ docs/design/）
    R - 仅剪切需求（docs/requirements/ docs/需求/）

请选择：
```

#### 1.4.3 执行剪切（默认 Y）

```bash
# 在新项目目录创建对应目录
NEW_DIR="..."    # Step 1.1 创建的新项目目录

# 通用剪切函数
move_assets() {
  local src="$TEMPLATE_DIR/$1"
  local dst="$NEW_DIR/$1"

  if [ ! -d "$src" ] || [ -z "$(ls -A "$src" 2>/dev/null)" ]; then
    return  # 源为空，跳过
  fi

  mkdir -p "$dst"
  # 把 src 下所有可见 + 隐藏文件移到 dst
  (shopt -s dotglob nullglob; mv "$src"/* "$dst"/ 2>/dev/null)
  echo "✓ 剪切 $1（$src → $dst）"

  # 源目录保留（空目录，便于下次复用）
}

# 默认全部剪切（用户选 Y）
for d in "${ASSET_DIRS[@]}"; do
  move_assets "$d"
done
```

#### 1.4.4 在新项目中标记资产来源（可选）

```bash
# 在新项目 docs/requirements/ 创建简短 README，说明这些资产从哪来
cat > "$NEW_DIR/docs/requirements/README.md" << 'EOF'
# 需求与设计资产

本目录的需求文档 / 原型图来自项目初始化时从模板仓库剪切。

## 资产清单
（init skill 自动维护此清单 - 可手动整理）

- `PRD-v1.0.md` - 产品需求文档 v1.0
- `用户故事.md` - 用户故事拆解

## 后续维护
- 新需求 → 本目录新建 PRD-v{n}.md
- 设计稿 / 原型 → ../../prototype/ 或 ../design/
- 不要重新引用模板仓库（它已被清空，且会随上游同步更新）
EOF
```

#### 1.4.5 git 状态检查

```bash
# 模板仓库（注意此时本地有文件移动但未提交）：
cd "$TEMPLATE_DIR"
git status --short
# 应该显示这些资产目录的删除 D，但因为这些资产**通常未被 git 跟踪**
#（它们是用户在模板仓库里临时创建的设计稿），所以多半 git status 是干净的

# 如果发现资产已被 git 跟踪（小概率），用户需要决定：
#   a. 在模板仓库 git rm + commit（彻底删除）
#   b. 用 .gitignore 忽略未来同名文件
# 提醒用户但不自动操作（避免误删用户的真实跟踪文件）
```

#### 1.4.6 验证剪切结果

```bash
echo ""
echo "剪切完成。结果："
echo ""
for d in "${ASSET_DIRS[@]}"; do
  src_count=$(ls -1 "$TEMPLATE_DIR/$d" 2>/dev/null | wc -l)
  dst_count=$(ls -1 "$NEW_DIR/$d" 2>/dev/null | wc -l)
  echo "  $d: 模板剩余 $src_count 个，新项目获得 $dst_count 个"
done
```

#### 反模式

| ❌ | ✅ |
|---|----|
| 用 `cp` 而非 `mv` | 必须**剪切**（让模板回归干净基线） |
| 默默执行不询问用户 | 必须列出资产清单 + 询问 Y/N/P/R |
| 删除模板的空目录结构 | 保留空目录，便于下次复用 |
| 强行覆盖新项目已有同名资产 | 检查 + 询问（如果新项目意外已有同名文件） |
| 把 `.claude/skills/` 当资产剪切 | 严禁——技能必须留在模板 |

---

## 阶段 1.5：AI 协作体系增强（subproject 模式，v1.2.1）

> **新增于 2026-05-20**：从 agile-qt 框架反向同步 4 个 subproject 治理特性，五端（Tauri / Android / iOS / HarmonyOS / agile-qt）统一。
>
> **目的**：明确"框架经验 vs 项目经验"边界，让新项目既能享用框架本体的只读体系，又能写自己的可变约束 / 专属技能。

---

### Step 1.5.A：写入 `.claude/exp.config.json`（subproject 模式标记）

在新项目根目录的 `.claude/` 下创建：

```bash
cat > "$NEW_DIR/.claude/exp.config.json" <<EOF
{
  "mode": "subproject",
  "frameworkRepo": "https://gitcode.com/zhuawashi/tauri",
  "frameworkName": "tauri",
  "subprojectName": "{产品名}",
  "createdAt": "$(date +%Y-%m-%d)",
  "experienceLoop": {
    "frameworkReadOnly": [
      ".claude/skills/",
      ".claude/commands/",
      ".claude/hooks/",
      ".codex/",
      "CLAUDE.md",
      "AGENTS.md"
    ],
    "subprojectWritable": [
      ".claude/PROJECT.md",
      ".claude/project-skills/",
      ".claude/docs/experience/"
    ],
    "feedbackChannel": ".claude/docs/experience/feedback-to-framework.md"
  }
}
EOF
```

**字段说明**：

| 字段 | 含义 |
|------|------|
| `mode` | 永远是 `subproject`（区别于框架本体的 `framework`） |
| `frameworkRepo` | 上游框架仓库（用于 `cmd-framework-sync` skill 拉取更新） |
| `subprojectName` | 子项目名（= 产品名） |
| `frameworkReadOnly` | 这些路径跟随框架同步，**禁止子项目本地修改** |
| `subprojectWritable` | 这些路径子项目可自由读写，**不会被 framework-sync 覆盖** |

---

### Step 1.5.B：写入 `.claude/PROJECT.md`（项目专属约束）

新项目的 `.claude/PROJECT.md` 是与 `CLAUDE.md` 互补的"可写副本"：

```bash
cat > "$NEW_DIR/.claude/PROJECT.md" <<EOF
# {产品名} - 项目专属约束（PROJECT.md）

> 本文件与框架本体的 \`CLAUDE.md\`（只读副本）互补。
>
> - \`CLAUDE.md\` = 框架通用规约（跟随 framework-sync 更新，禁止本地修改）
> - \`PROJECT.md\` = 项目专属约束（可写，与框架规则冲突时**优先生效**）

---

## 1. 项目背景

（在这里写本项目的业务背景、核心场景、用户画像。）

## 2. 项目专属技术栈差异

如果本项目相比框架默认栈做了调整，在此声明：

| 维度 | 框架默认 | 本项目 | 原因 |
|------|---------|--------|------|
| 例：UI 库 | shadcn/ui | Radix UI 原生 | （写原因） |

## 3. 项目专属禁令

| # | 禁止 | 原因 / 替代方案 |
|---|------|---------------|
| 1 | 例：禁止 invoke 同步 Rust 命令 > 50ms | 改 spawn async + event |

## 4. 项目专属命名 / 文件结构

（如有偏离框架默认的约定，在此声明。）

## 5. 项目专属经验

会话中沉淀的"只对本项目有用，不值得反哺给框架"的经验写在这里。

> 真正通用、可反哺到框架的经验请写到 \`.claude/docs/experience/feedback-to-framework.md\`。

EOF
```

---

### Step 1.5.C：创建 `.claude/project-skills/` 目录

项目专属技能存放点（与框架本体 `.claude/skills/` 区分）：

```bash
mkdir -p "$NEW_DIR/.claude/project-skills"
cat > "$NEW_DIR/.claude/project-skills/README.md" <<EOF
# 项目专属技能目录（project-skills）

> 本目录与 \`.claude/skills/\` 互补：
>
> - \`.claude/skills/\` = 框架本体技能（只读，跟随 framework-sync 更新）
> - \`.claude/project-skills/\` = 项目专属技能（可写，不参与框架同步）

## 何时往这里加技能？

- 这个技能**只对本项目有用**（如：与某个特定后端 API 强耦合的领域模型生成器）
- 这个技能**包含敏感信息或商业逻辑**，不应反哺给框架

## 何时往 \`.claude/skills/\` 加技能？

- ❌ 不允许直接改框架本体 skills 目录
- ✅ 想给框架加技能 → 先在 \`project-skills/\` 试用，验证通用后通过 \`add-skill\` skill + PR 反哺到框架

## 技能格式

与 \`.claude/skills/\` 完全一致：每个技能一个目录，含 \`SKILL.md\`（YAML frontmatter + 内容）。

Claude Code 会**同时**扫描两个目录，project-skills 中的技能会自动可用。
EOF
```

---

### Step 1.5.D：创建 `.claude/docs/experience/feedback-to-framework.md`

反哺候选汇集池：

```bash
mkdir -p "$NEW_DIR/.claude/docs/experience"
cat > "$NEW_DIR/.claude/docs/experience/feedback-to-framework.md" <<EOF
# 反哺到 tauri 框架的经验候选

> 本文件用于**收集**那些"在本项目踩到的坑 / 验证过的优化方案"，**评估通用性后**反哺到上游框架（\`https://gitcode.com/zhuawashi/tauri\`）。

---

## 写入规则

每条候选包含 6 个字段：

1. **场景** — 一句话描述
2. **现象** — 触发条件 / 错误日志 / 坏味道
3. **根因** — 为什么
4. **解决** — 怎么改（贴代码片段，**先脱敏**）
5. **通用性评分** — ⭐⭐⭐⭐⭐（5 星 = 所有 Tauri 项目都会遇到，1 星 = 仅本项目）
6. **建议反哺位置** — 框架里哪个 skill / CLAUDE.md 章节

---

## 反哺工作流

1. 子项目里随时往本文档追加候选
2. 累积到 5+ 条 → 用 \`cmd-framework-sync\` skill 的反哺子命令
3. 自动生成框架仓库的 PR（脱敏后）
4. 框架 owner review → merge → 下次 \`framework-sync\` 拉回所有子项目

---

## 候选清单

### 候选 #1：（场景）

- **场景**：
- **现象**：
- **根因**：
- **解决**：
- **通用性评分**：
- **建议反哺位置**：

EOF
```

---

### Step 1.5.E：在子项目 CLAUDE.md 顶部加 PROJECT.md 加载提示

子项目的 `CLAUDE.md`（从框架复制来的副本）顶部插入一段，提示读 PROJECT.md：

```bash
# 检查是否已加载提示（避免重复插入）
if ! grep -q "PROJECT.md（项目专属约束）" "$NEW_DIR/CLAUDE.md"; then
  # 用 Python 在 "# CLAUDE.md" 第一行后插入
  python -c "
import io
p = r'$NEW_DIR/CLAUDE.md'
with open(p, 'r', encoding='utf-8') as f:
    lines = f.readlines()
insert_at = next((i+1 for i, l in enumerate(lines) if l.startswith('# CLAUDE.md')), 1)
snippet = '''
> 🔴 **子项目模式提示**：本项目是基于 [tauri 框架](https://gitcode.com/zhuawashi/tauri) 创建的子项目。
>
> - 本文件（CLAUDE.md）= 框架只读副本，不要直接改
> - 项目专属约束请写到 \`\`.claude/PROJECT.md\`\`（项目专属约束）
> - 项目专属技能请放到 \`\`.claude/project-skills/\`\`
> - 反哺到框架的候选请写到 \`\`.claude/docs/experience/feedback-to-framework.md\`\`

'''
lines.insert(insert_at, snippet)
with open(p, 'w', encoding='utf-8') as f:
    f.writelines(lines)
"
fi
```

---

### Step 1.5 完整执行清单

| # | 操作 | 产出 |
|---|------|------|
| 1.5.A | 写 exp.config.json | `.claude/exp.config.json` |
| 1.5.B | 写 PROJECT.md | `.claude/PROJECT.md` |
| 1.5.C | 建 project-skills/ + README | `.claude/project-skills/README.md` |
| 1.5.D | 建 feedback-to-framework.md | `.claude/docs/experience/feedback-to-framework.md` |
| 1.5.E | CLAUDE.md 顶部加提示 | 子项目 CLAUDE.md 已注入 subproject 提示 |

> **关键边界**：完成 Step 1.5 之后，子项目就有了完整的"框架经验 / 项目经验 / 反哺候选"三层边界，会话中沉淀的任何经验都有明确去处。

---

## 阶段二：代码初始化（在新目录中执行）

> **以下所有操作都在新目录 `{NEW_DIR}` 中执行，不要在模板目录中操作！**

### 旧值映射表

| 属性 | 旧值 | 说明 |
|------|------|------|
| **产品名称** | `Agile Tauri` | 窗口标题、托盘提示、页面标题 |
| **产品名缩写** | `AT` | 侧边栏折叠时显示 |
| **应用标识符** | `com.agilefr.tauri` | Tauri identifier |
| **Cargo 包名** | `tauri` | Cargo.toml [package].name |
| **Cargo lib 名** | `tauri_lib` | Cargo.toml [lib].name |
| **npm 包名** | `tauri` | package.json name |
| **作者** | `you` | Cargo.toml authors |
| **描述** | `A Tauri App` | Cargo.toml description |
| **更新地址** | `https://gitee.com/<用户名>/<项目名>-release/raw/master/update.json` | 更新端点占位符 |
| **签名公钥** | `YOUR_UPDATER_PUBKEY_HERE` | 更新签名占位符 |

### Step 2.1：替换产品名称

将 `Agile Tauri` → `{新产品名}`

**精确文件列表**：

| 文件 | 替换内容 | 说明 |
|------|---------|------|
| `src-tauri/tauri.conf.json` | `"productName": "Agile Tauri"` → `"productName": "{新产品名}"` | 安装包名称 |
| `src-tauri/tauri.conf.json` | `"title": "Agile Tauri"` → `"title": "{新产品名}"` | 窗口标题 |
| `src-tauri/src/tray.rs` | `.tooltip("Agile Tauri")` → `.tooltip("{新产品名}")` | 托盘提示文字 |
| `index.html` | `<title>Agile Tauri</title>` → `<title>{新产品名}</title>` | 页面标题 |
| `src/components/layout/Sidebar.tsx` | `"Agile Tauri"` → `"{新产品名}"` | 侧边栏展开时名称 |
| `src/pages/home/index.tsx` | `Agile Tauri` 相关描述文字 | 首页欢迎语 |

**替换产品名缩写**：

| 文件 | 替换内容 | 说明 |
|------|---------|------|
| `src/components/layout/Sidebar.tsx` | `"AT"` → `"{新缩写}"` | 侧边栏折叠时显示 |

**重写 README.md**（整体替换，而非局部修改）：

模板的 `README.md` 描述的是 Tauri 框架本身，内容不适合作为新项目文档。需**整体重写**为新项目介绍，推荐最小模板：

```markdown
# {新产品名}

{新描述}

基于 Tauri 2.x 构建的桌面应用。

## 开发

\`\`\`bash
pnpm install
pnpm tauri dev
\`\`\`

## 构建

\`\`\`bash
pnpm tauri build
\`\`\`
```

> **常见错误**：只替换 `README.md` 中的 `Agile Tauri` / `Tauri Desktop Framework` 字样，保留了整段框架介绍。这会让新项目看起来像是框架本身的一个分支。务必**整体重写**。

### Step 2.2：替换应用标识符

将 `com.agilefr.tauri` → `{新标识符}`

**精确文件列表**：

| 文件 | 替换内容 | 说明 |
|------|---------|------|
| `src-tauri/tauri.conf.json` | `"identifier": "com.agilefr.tauri"` → `"identifier": "{新标识符}"` | 应用唯一标识 |
| `CLAUDE.md` | `com.agilefr.tauri` → `{新标识符}` | 文档中的引用 |
| `AGENTS.md` | `com.agilefr.tauri` → `{新标识符}` | Codex 项目规范（与 CLAUDE.md 对应） |
| `.claude/commands/progress.md` | `com.agilefr.tauri` → `{新标识符}` | 进度报告模板 |
| `.claude/commands/start.md` | `com.agilefr.tauri` → `{新标识符}` | 项目介绍模板 |

> **注意**：`.claude/skills/` 中的技能文档如果包含 `com.agilefr.tauri` 作为示例引用，**不需要替换**。

### Step 2.3：替换包名

将 `tauri` / `tauri_lib` → `{新包名}` / `{新包名}_lib`

**精确文件列表**：

| 文件 | 旧值 | 新值 | 说明 |
|------|------|------|------|
| `src-tauri/Cargo.toml` | `name = "tauri"` | `name = "{新包名}"` | Cargo 包名 |
| `src-tauri/Cargo.toml` | `name = "tauri_lib"` | `name = "{新包名}_lib"` | Cargo lib 名 |
| `src-tauri/src/main.rs` | `tauri_lib::run()` | `{新包名}_lib::run()` | lib 调用 |
| `package.json` | `"name": "tauri"` | `"name": "{新包名}"` | npm 包名 |

> **替换顺序**：**先替换 `tauri_lib`（长）再替换包名级别的 `tauri`（短）**，避免 `tauri_lib` 被部分匹配为 `{新包名}_lib`。
>
> **特别注意**：`Cargo.toml` 中的 `tauri = { version = "2" }` 是依赖声明，**绝对不能替换**！只替换 `[package]` 下的 `name` 和 `[lib]` 下的 `name`。使用精确匹配（如 `name = "tauri"` 而非全局替换 `tauri`）。

### Step 2.4：替换作者和描述

| 文件 | 旧值 | 新值 | 说明 |
|------|------|------|------|
| `src-tauri/Cargo.toml` | `authors = ["you"]` | `authors = ["{新作者}"]` | 开发者 |
| `src-tauri/Cargo.toml` | `description = "A Tauri App"` | `description = "{新描述}"` | 项目描述 |

### Step 2.5：配置更新地址和签名

#### 更新地址

| 文件 | 替换内容 | 说明 |
|------|---------|------|
| `src-tauri/tauri.conf.json` | `updater.endpoints` 数组 | 替换为实际 release 仓库的 raw 地址 |

**根据用户提供的 release 仓库生成地址**：

| 平台 | URL 格式 |
|------|---------|
| Gitee | `https://gitee.com/{user}/{repo}/raw/master/update.json` |
| GitHub | `https://raw.githubusercontent.com/{user}/{repo}/master/update.json` |

**如果用户选择"稍后配置"**，保留占位符不替换。

#### 签名密钥

**询问用户是否现在生成签名密钥对**：

```
是否现在生成更新签名密钥对？
1. 是（推荐，自动生成并配置）
2. 稍后手动生成

注意：签名密钥用于验证应用更新包的安全性。
私钥需妥善保管，不可提交到代码仓库。
```

**如果用户选择生成**：

```bash
cd "$NEW_DIR"

# 生成密钥对（保存到用户目录）
pnpm tauri signer generate -w ~/.tauri/{新包名}.key

# 输出的公钥需要填入 tauri.conf.json 的 updater.pubkey
```

将生成的公钥写入 `tauri.conf.json` 的 `updater.pubkey`。

**如果用户选择稍后**，保留 `YOUR_UPDATER_PUBKEY_HERE` 占位符。

### Step 2.6：更新框架文档中的引用

以下文件需要更新项目名称和标识符引用：

| 文件 | 需要更新的内容 |
|------|--------------|
| `CLAUDE.md` | 应用标识 `com.agilefr.tauri` → 新标识符 |
| `AGENTS.md` | 应用标识 `com.agilefr.tauri` → 新标识符（Codex 侧项目规范） |
| `.claude/commands/progress.md` | 应用标识引用 |
| `.claude/commands/start.md` | 应用标识引用 |

> **注意**：CLAUDE.md / AGENTS.md 中大量内容是通用的架构文档，只需要替换具体的标识符值，不要改动架构说明。
>
> **已在 Step 1.3 删除**：`docs/development-guide.md`（框架自身的开发指南，不适合作为新项目文档），因此不需要替换其中的引用。

### Step 2.7：配置唯一开发端口号

每个项目使用独立端口号，避免多项目同时开发时端口冲突。

**端口分配规则**：

| 项目 | 开发端口 (dev) | HMR 端口 |
|------|---------------|----------|
| 模板仓库 (tauri) | 1420 | 1421 |
| knowledge_base | 1421 | 1431 |
| pix_snap | 1422 | 1432 |
| clip_master | 1423 | 1433 |
| media_grab | 1424 | 1434 |
| **新项目** | **下一个可用**（扫描同级目录） | **dev_port + 10** |

**自动分配端口**：扫描模板仓库同级目录中所有已有项目的端口，取最大值 +1 作为新项目端口。

```bash
# 扫描同级目录中已使用的端口号
PARENT_DIR="$(dirname "$TEMPLATE_DIR")"
MAX_PORT=1420
for proj_dir in "$PARENT_DIR"/*/; do
  if [ -f "$proj_dir/vite.config.ts" ]; then
    PORT=$(grep -oP 'port:\s*\K\d+' "$proj_dir/vite.config.ts" | head -1)
    if [ -n "$PORT" ] && [ "$PORT" -gt "$MAX_PORT" ]; then
      MAX_PORT=$PORT
    fi
  fi
done
NEW_DEV_PORT=$((MAX_PORT + 1))
NEW_HMR_PORT=$((NEW_DEV_PORT + 10))
echo "分配端口: dev=$NEW_DEV_PORT, hmr=$NEW_HMR_PORT"
```

**需要修改的 3 个文件**：

| 文件 | 旧值 | 新值 | 说明 |
|------|------|------|------|
| `vite.config.ts` | `port: 1420` | `port: {dev_port}` | Vite 开发服务器端口 |
| `vite.config.ts` | `port: 1421`（hmr 内） | `port: {hmr_port}` | HMR WebSocket 端口 |
| `src-tauri/tauri.conf.json` | `"devUrl": "http://localhost:1420"` | `"devUrl": "http://localhost:{dev_port}"` | Tauri 开发 URL |
| `package.json` | `"kill-port 1420 & vite"` | `"kill-port {dev_port} & vite"` | 启动前清理端口 |

### Step 2.8：验证替换结果

```bash
cd "$NEW_DIR"

# 验证旧产品名已全部替换
grep -rn "Agile Tauri" \
  --include="*.json" --include="*.ts" --include="*.tsx" --include="*.rs" \
  --include="*.html" --include="*.css" --include="*.md" \
  --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=target .

# 验证旧标识符已全部替换
grep -rn "com\.agilefr\.tauri" \
  --include="*.json" --include="*.ts" --include="*.tsx" --include="*.rs" \
  --include="*.html" --include="*.md" \
  --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=target .

# 验证包名替换（精确匹配，排除依赖声明）
# 检查 main.rs 中的 lib 调用
grep -n "tauri_lib" src-tauri/src/main.rs

# 检查 Cargo.toml 的 [package] name
head -5 src-tauri/Cargo.toml
```

**允许残留的位置**（不需要替换）：
- `.claude/skills/*/SKILL.md` — 技能文档中的示例引用
- `Cargo.toml` 的 `[dependencies]` — `tauri = { version = "2" }` 是依赖名
- `Cargo.lock` — 已在 Step 1.3 删除，会自动重新生成

**如果还有残留**，需要补充替换。

---

## 阶段三：Git 提交 & 推送

### Step 3.1：初始提交

```bash
cd "$NEW_DIR"

git add -A
git commit -m "init: 基于 Tauri 桌面应用框架初始化 {产品名称}"
```

### Step 3.2：创建远程仓库（如果选择自动创建）

**如果用户选择"自动创建 Gitee 仓库"**：

#### 3.2.1 读取 Token 并验证

```bash
# 读取 Token
TOKEN=$(cat ~/.gitee_token)

# 验证 Token 有效性，同时获取用户名
node -e "
const https = require('https');
https.get('https://gitee.com/api/v5/user?access_token=$TOKEN', res => {
  let body = '';
  res.on('data', c => body += c);
  res.on('end', () => {
    const r = JSON.parse(body);
    if (r.login) {
      console.log('用户名:' + r.login);
      console.log('昵称:' + r.name);
    } else {
      console.log('Token 无效');
    }
  });
});
"
```

#### 3.2.2 通过 Gitee API 创建仓库

> **重要**：必须使用 Node.js 发送请求，不要用 curl！
> Git Bash 的 curl 处理中文编码有问题，会导致仓库描述变成乱码。
>
> 🔴 **强制私有仓库（仅限源码 / 主仓库）**：本段创建的是**源码主仓库**，`private` 字段必须硬编码为 `true`，
> **绝对禁止**创建为 `false`（公开仓库）。即使用户未明确要求，默认也必须是私有。该规则遵循全局 CLAUDE.md 的"仓库创建规范"。
> 如果用户明确要求公开源码仓库，需要先二次确认风险，并在对话中得到明确"是，确认公开"的回复后才能改为 `false`。
>
> ⚠️ **例外：若用此段去创建 release / updater 端点仓库（`<项目名>-release`），必须改 `private: false`（公开）**——
> 否则 Tauri updater 拉不到 update.json，自动更新失效。详见顶部"全局强制规则 ②"。

```javascript
// 使用 Node.js 调用 Gitee API 创建仓库
node -e "
const https = require('https');
const data = JSON.stringify({
  access_token: '{TOKEN}',
  name: '{包名}',
  description: '{项目描述}',
  private: true,           // 🔴 强制私有，绝对禁止改为 false
  auto_init: false
});
const options = {
  hostname: 'gitee.com',
  path: '/api/v5/user/repos',
  method: 'POST',
  headers: {
    'Content-Type': 'application/json;charset=utf-8',
    'Content-Length': Buffer.byteLength(data)
  }
};
const req = https.request(options, res => {
  let body = '';
  res.on('data', c => body += c);
  res.on('end', () => {
    const r = JSON.parse(body);
    if (r.html_url) {
      console.log('仓库创建成功:', r.html_url);
    } else {
      console.log('创建失败:', r.message || JSON.stringify(r));
    }
  });
});
req.write(data);
req.end();
"
```

**API 错误处理**：
- 仓库已存在 → 提示用户确认是否使用已有仓库
- Token 权限不足 → 提示用户检查 Token 权限（需要 `projects` 权限）
- 网络错误 → 提示用户检查网络，或改为手动创建

#### 3.2.3 更新仓库描述（如果中文乱码）

如果通过 curl 创建导致描述乱码，用 Node.js 修复：

```javascript
// 使用 PATCH 更新仓库描述
node -e "
const https = require('https');
const data = JSON.stringify({
  access_token: '{TOKEN}',
  name: '{包名}',
  description: '{项目描述}'
});
const options = {
  hostname: 'gitee.com',
  path: '/api/v5/repos/{用户名}/{包名}',
  method: 'PATCH',
  headers: {
    'Content-Type': 'application/json;charset=utf-8',
    'Content-Length': Buffer.byteLength(data)
  }
};
const req = https.request(options, res => {
  let body = '';
  res.on('data', c => body += c);
  res.on('end', () => {
    const r = JSON.parse(body);
    console.log('描述已更新:', r.description);
  });
});
req.write(data);
req.end();
"
```

### Step 3.3：关联远程仓库并推送

```bash
# 添加用户自己的远程仓库（origin）
git remote add origin https://gitee.com/{用户名}/{包名}.git

# 推送到远程
git push -u origin master
```

> **Remote 命名约定**：
> - `origin` — 用户自己的项目仓库（推送代码用）
> - `upstream` — 模板框架仓库（对比更新用，已在 Step 1.2 自动设置）
>
> 可通过 `git remote -v` 确认两个 remote 都已正确配置。

> **如果用户选择"稍后手动创建"**，跳过推送步骤，提示：
> ```
> Git 仓库已本地初始化。创建远程仓库后，执行：
> cd {NEW_DIR}
> git remote add origin {仓库地址}
> git push -u origin master
>
> 模板仓库已关联为 upstream，可随时对比框架更新：
> git fetch upstream && git log master..upstream/master --oneline
> ```

---

## 阶段四：应用图标（可选）

### Step 4.0：自动应用品牌 Logo 为图标（若 /kickoff 已出 Logo）★

如果 `docs/brand/` 下有 `/kickoff` 阶段二剪切进来的 Logo 主图，**直接拿它生成应用图标**，无需用户再准备：

```bash
cd "$NEW_DIR"
LOGO=$(ls docs/brand/logo-1024.png docs/brand/logo*.png 2>/dev/null | head -1)
if [ -n "$LOGO" ]; then
  pnpm tauri icon "$LOGO"   # 自动生成 src-tauri/icons/ 全尺寸（ico/icns/png）
  echo "✓ 已用品牌 Logo 生成应用图标：$LOGO"
else
  echo "未发现 docs/brand/ 下的 Logo，转 Step 4.1 手动准备"
fi
```

仅当 `docs/brand/` 无 Logo 时，才走下面的 Step 4.1 手动流程。

### Step 4.1：提示用户准备图标（仅在无品牌 Logo 时）

```
应用图标配置（可稍后处理）：

当前使用默认 Tauri 图标。如需自定义：

1. 准备一张 1024x1024 的 PNG 图片（方形，透明背景推荐）
2. 执行以下命令自动生成所有尺寸：

   cd {NEW_DIR}
   pnpm tauri icon path/to/icon-1024x1024.png

3. 图标会自动生成到 src-tauri/icons/ 目录

生成的图标：
  - icon.ico      (Windows)
  - icon.icns     (macOS)
  - 32x32.png     (通用)
  - 128x128.png   (通用)
  - 128x128@2x.png (HiDPI)
```

---

## 阶段五：启动引导

### Step 5.1：提示启动步骤

```
项目初始化完成！按以下步骤启动：

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  项目目录：{NEW_DIR}

  1. 安装依赖
     cd {NEW_DIR}
     pnpm install

  2. 启动开发模式（前端 HMR + Rust 热编译）
     pnpm tauri dev

  3. 访问应用
     应用窗口会自动打开
     前端开发地址：http://localhost:{dev_port}

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  后续常用命令：
  - pnpm tauri dev      — 开发模式
  - pnpm tauri build    — 构建安装包
  - npx tsc --noEmit    — TypeScript 类型检查
  - cd src-tauri && cargo clippy  — Rust 代码检查

  环境要求：
  - Node.js 18+
  - pnpm 8+
  - Rust (rustup)
  - 系统 WebView2 (Windows 自带)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### Step 5.2：输出"复制即用"的自主循环起飞指令 ★

> **目的**：用户切换到新项目后，一句话即可让 Claude Code 进入自主连续开发。本步骤输出 `/loop /dev-loop` 起飞指令——配合已剪切进来的需求+原型，新项目无需再手写开发提示词。

#### 5.2.1 检测新项目中已剪切的资产

```bash
HAS_PROTOTYPE=$([ -n "$(ls -A "$NEW_DIR/prototype" 2>/dev/null)" ] && echo "yes" || echo "no")
HAS_REQUIREMENTS=$([ -n "$(ls -A "$NEW_DIR/docs/requirements" 2>/dev/null)" ] && echo "yes" || echo "no")
HAS_DESIGN=$([ -n "$(ls -A "$NEW_DIR/docs/design" 2>/dev/null)" ] && echo "yes" || echo "no")
```

#### 5.2.2 生成提示词模板

向用户展示（边框 + 提示一键复制）：

````
━━━━━━━━━━ 复制以下提示词到新项目 Claude Code ━━━━━━━━━━

/loop /dev-loop

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
````

#### 5.2.3 把提示词同时保存到新项目

```bash
# 保存到 .claude/docs/init-prompt.md，便于用户随时再用
mkdir -p "$NEW_DIR/.claude/docs"
cat > "$NEW_DIR/.claude/docs/init-prompt.md" << 'EOF'
# 初始化提示词（项目首次开发用）

{上面的提示词内容}

## 使用方式
- 首次在新项目打开 Claude Code 时复制粘贴
- 后续可作为"上下文重置"使用（让 AI 重新理解项目）
EOF

echo ""
echo "已保存到：$NEW_DIR/.claude/docs/init-prompt.md"
echo "下次再用直接：cat .claude/docs/init-prompt.md | xclip"
```

#### 反模式

| ❌ | ✅ |
|---|----|
| 让用户切过去还得手写一长串开发提示词 | 直接给 `/loop /dev-loop` 一句起飞 |
| 吐手动 step 1-6 让 AI"先了解一下" | 交给 `dev-loop` 标准每轮 SOP 自主推进 |
| 新项目没继承 `dev-loop` 命令就让它跑 | 确认 `.claude/commands/dev-loop.md` 已随骨架继承（缺则补） |
| 不保存起飞指令 | 写入 `.claude/docs/init-prompt.md` 备用 |

> **生成后向用户简要解释**（无需复制）：新项目已带 `/dev-loop`（随框架骨架继承）+ 已就位的 `docs/requirements/`（需求）和 `prototype/`（原型）；`/loop /dev-loop` = 自主连续开发，每轮 读真相源 → 选下一个未勾 `[ ]` 任务 → 实现 → 验证门（`npx tsc --noEmit` / `cargo check`，UI 任务额外走「原型保真截图闭环」）→ 打勾 → 最小提交，直到任务台账全做完才停。**第 0 轮**会先按需求+原型自动拆出有序任务台账（`docs/tasks/active/BUILD-PLAN.md`）。

---

## 完整替换清单（按文件 + 匹配模式）

> **说明**：不再标注行号——代码演进时行号会漂移。请用**内容匹配**而不是行号定位。

### 产品名称替换（Agile Tauri → {新产品名}）

```
src-tauri/tauri.conf.json       → "productName": "{新产品名}" 和 "title": "{新产品名}"
src-tauri/src/tray.rs           → .tooltip("{新产品名}")
index.html                      → <title>{新产品名}</title>
src/components/layout/Sidebar.tsx → "{新产品名}"（展开时）
src/pages/home/index.tsx        → 首页欢迎语中的产品名
README.md                       → 整体重写（见 Step 2.1）
```

### 产品名缩写替换（AT → {新缩写}）

```
src/components/layout/Sidebar.tsx → "{新缩写}"（折叠时）
```

### 应用标识符替换（com.agilefr.tauri → {新标识符}）

```
src-tauri/tauri.conf.json       → "identifier": "{新标识符}"
CLAUDE.md                       → 应用标识表格
AGENTS.md                       → 应用标识引用（Codex 侧项目规范）
.claude/commands/progress.md    → 应用标识引用
.claude/commands/start.md       → 应用标识引用
```

### 包名替换

> **必须精确匹配，不可全局替换 `tauri` 一词！**

```
# 先替换长的（tauri_lib → {包名}_lib）
src-tauri/Cargo.toml            → name = "{包名}_lib"（[lib] 段）
src-tauri/src/main.rs           → {包名}_lib::run()

# 再替换短的（仅 [package].name 和 package.json name）
src-tauri/Cargo.toml            → name = "{包名}"（[package] 段）
package.json                    → "name": "{包名}"
```

### 作者和描述替换

```
src-tauri/Cargo.toml            → description = "{新描述}"、authors = ["{新作者}"]
```

### 更新配置替换

```
src-tauri/tauri.conf.json       → "endpoints": ["{新更新地址}"]、"pubkey": "{新公钥}"
```

### 端口配置替换（1420 → {dev_port}）

```
vite.config.ts                  → port: {dev_port}（开发服务器）、port: {hmr_port}（HMR）
src-tauri/tauri.conf.json       → "devUrl": "http://localhost:{dev_port}"
package.json                    → "kill-port {dev_port} & vite"
```

### 删除的文件（在 Step 1.3 中完成）

```
src-tauri/Cargo.lock            — 新包名后自动重新生成
docs/development-guide.md       — 框架自身的开发指南，不适合作为新项目文档
```

### 不需要替换的文件

以下文件包含旧值但**不应替换**：

```
.claude/skills/*/SKILL.md              — 技能文档中的示例引用
.codex/skills/*/SKILL.md               — 同上（Codex 镜像）
src-tauri/Cargo.toml [dependencies]    — tauri = { version = "2" } 是依赖名
```

---

## 注意事项

### 1. 模板仓库保持不变 & upstream 关联

所有修改操作都在新目录中进行，模板仓库仅作为只读源。好处：
- 可反复创建新项目，无需重新克隆
- 模板仓库可随时拉取上游更新
- 多个新项目可共用同一个模板

**新项目的 remote 布局**：
```
origin   → 用户自己的项目仓库（日常推送）
upstream → 模板框架仓库（对比框架更新）
```

**框架更新对比工作流**：
```bash
# 1. 拉取模板仓库最新变更
git fetch upstream

# 2. 查看框架有哪些新提交
git log master..upstream/master --oneline

# 3. 查看具体文件差异
git diff master...upstream/master -- src-tauri/src/

# 4. 选择性合并（推荐 cherry-pick 而非 merge，避免冲突）
git cherry-pick <commit-hash>
```

> **注意**：由于新项目做了标识符替换，直接 `git merge upstream/master` 会产生大量冲突。推荐用 `cherry-pick` 或手动对比后逐个应用。

### 2. 包名替换的陷阱

`tauri` 这个词在项目中有两种含义：
- **包名**（`Cargo.toml [package].name`、`package.json name`）→ 需要替换
- **框架依赖名**（`tauri = { version = "2" }`、`use tauri::`、`@tauri-apps/`）→ **绝对不能替换**

因此**禁止全局替换 `tauri` 一词**，必须使用精确匹配：
- `name = "tauri"` → `name = "{包名}"`（只匹配 Cargo.toml 的 name 字段）
- `"name": "tauri"` → `"name": "{包名}"`（只匹配 package.json 的 name 字段）
- `tauri_lib` → `{包名}_lib`（这个可以全局替换，因为是自定义的 lib 名）

### 3. Cargo.lock 处理

删除旧的 `Cargo.lock`，首次 `cargo build` 或 `pnpm tauri dev` 时会自动重新生成，包含正确的新包名。

### 4. 替换顺序（先长后短）

```
1. tauri_lib    → {包名}_lib      （最长，先替换）
2. Agile Tauri  → {新产品名}       （含空格的完整名称）
3. com.agilefr.tauri → {新标识符}  （应用标识符）
4. name = "tauri" → name = "{包名}" （精确匹配包名）
5. AT           → {新缩写}         （最短，最后替换）
6. 端口 1420    → {dev_port}       （开发端口，3 个文件 4 处）
```

### 5. 签名密钥安全

- **私钥（.key 文件）**：保存在 `~/.tauri/` 目录，**绝对不可提交到 Git**
- **公钥**：写入 `tauri.conf.json`，可以公开
- CI 构建时通过 `TAURI_SIGNING_PRIVATE_KEY` 环境变量传入私钥

### 6. Windows 兼容性

- 使用 `git archive` 导出文件，Git Bash 自带 tar
- 路径使用正斜杠 `/`（Git Bash 环境）
- 不使用 `> nul`，使用 `> /dev/null 2>&1`

### 7. SQLite 数据库无需手动初始化

与 ruoyi-plus-uniapp 不同，本框架的 SQLite 数据库由 `database/schema.rs` 中的迁移逻辑在首次启动时**自动创建**，无需手动导入 SQL 文件。

### 8. 🔴 仓库可见性（源码私有，更新端点仓库公开）

- **源码 / 主仓库**：一律创建为**私有**，无论 Gitee / GitHub / GitCode
  - Gitee API 请求体中 `private: true` 必须硬编码，禁止省略、禁止通过变量传入可能为 `false` 的值
  - 即使用户未指定可见性，默认也按私有处理
  - 如果用户明确要求把源码仓库公开，执行前必须：① 说明风险（源码、配置、业务逻辑暴露）② 得到明确二次确认（如"我确认要创建公开仓库"）后才能改 `private: false`
- **release / updater 端点仓库**：**必须公开**（`private: false`），这是全局"仓库一律私有"铁律的**唯一例外**
  - **原因**：Tauri updater 用明文 HTTP GET（无认证）拉 `update.json` 的 raw 地址，私有仓库 raw 需认证 → 应用永远"检查不到更新"
  - **安全性**：release 仓库只含安装包 + update.json + 更新**公钥**，**不含源码、不含签名私钥**，公开不泄露任何敏感资产
  - ⚠️ 早期版本曾写"release 仓库强烈建议保持私有"，**那是错的**——会直接导致自动更新失效，已纠正
  - 若启用 R2（`r2.enabled`），主端点走 R2 公开桶，Gitee/GitHub release 仓库作备源，仍建议公开以保证 fallback 可用

### 9. Gitee Token 管理

- **存储位置**：`~/.gitee_token`（纯文本，仅包含 token 字符串）
- **获取方式**：https://gitee.com/profile/personal_access_tokens/new（勾选 `projects` 权限）
- **安全**：该文件仅保存在用户本地，不会被提交到任何 Git 仓库
- **复用**：一次配置后所有新项目自动复用，无需再次提供
- **API 调用注意**：必须使用 Node.js（`https` 模块）发送请求，**不要用 curl**！Git Bash 的 curl 在 Windows 上处理中文编码有 bug，会导致仓库描述变成乱码（`��������`）

---

## 常见问题

### Q1: 标识符可以包含横线吗？

**A:** 应用标识符（`com.company.app`）只能用点分隔。包名建议用下划线 `_`，横线在 Rust crate 名中会自动转为下划线。

### Q2: 模板仓库有本地修改怎么办？

**A:** `git archive` 从 Git 仓库中导出已提交的文件，不受工作区修改影响，模板仓库完全不会被改动。

### Q3: 替换后 Cargo 编译报错？

**A:** 最常见原因：
1. 包名替换不完整 — 检查 `main.rs` 中的 lib 调用是否已更新
2. 全局替换了依赖名 — `tauri = { version = "2" }` 中的 `tauri` 不能改
3. Cargo.lock 未删除 — 删除后重新编译

### Q4: 新目录已存在怎么办？

**A:** Step 1.1 会检测目标目录是否存在。如果已存在，提示用户确认是否覆盖或使用其他包名。

### Q5: 可以同时创建多个项目吗？

**A:** 可以。每次运行初始化流程都会创建一个新的同级目录，互不影响。模板仓库始终不变。

### Q6: 更新功能可以后续再配置吗？

**A:** 可以。保留 `YOUR_UPDATER_PUBKEY_HERE` 占位符，应用仍可正常运行，只是自动更新功能暂不可用。后续生成密钥并配置即可。

### Q7: 端口号是怎么分配的？

**A:** 每个项目分配唯一的开发端口，避免同时运行多个项目时端口冲突。模板仓库固定使用 1420，新项目自动扫描同级目录取最大端口 +1。HMR 端口 = 开发端口 + 10。
