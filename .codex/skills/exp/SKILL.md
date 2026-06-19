---
name: exp
description: |
  /exp - 经验沉淀助手。把本次会话和最近 Git 提交中产生的隐性知识（踩坑心得、反复纠正、重复模式）转化为显性资产——沉淀到 Skills、CLAUDE.md、Memory、docs 中。

  触发场景：
  - 用户运行 /exp 命令
  - 用户说"沉淀经验 / 总结本次会话 / 把这个记下来"
  - 会话末尾需要把可复用经验写回技能/文档
  - 需要从 git log 抽取可复用模式
  - 需要审计经验资产健康度（/exp review）

  触发词：/exp、沉淀经验、经验沉淀、总结会话、记下来、self-evolution、自我进化、反哺框架、经验审计、exp review、avoid-pitfall
user_invocable: true
---

# /exp - 经验沉淀助手

把本次会话和最近 Git 提交中产生的隐性知识（踩坑心得、反复纠正、重复模式）转化为显性资产——沉淀到 Skills、CLAUDE.md、Memory、docs 中，让下次换个项目、换个人都能复用。

> **核心理念**：每次会话结束前花 2 分钟沉淀，比之后花 2 小时重新踩坑划算。

---

## 使用方式

```bash
/exp                # 默认：分析会话 + 最近 7 天 commits
/exp session        # 仅分析当前会话
/exp commits [N]    # 仅分析最近 N 天 commits（默认 7）
/exp review         # 盘点现有经验资产（过时/冗余检查）
```

---

## 第零步：识别仓库模式（框架 vs 子项目）

本命令在**框架仓库**和**子项目**下行为不同，必须先识别。

**识别顺序**：

1. **优先读取** `.claude/exp.config.json`：
   ```json
   { "mode": "framework" | "subproject", "frameworkRepo": "..." }
   ```
2. **若不存在**，根据启发式判断：
   - `.framework-sync.json` 存在 → **子项目**
   - 否则 → **框架**
3. **向用户确认**检测结果，写入 `.claude/exp.config.json`（仅首次运行）。
4. **首次运行初始化目录**（存在则跳过）：
   ```bash
   mkdir -p .claude/docs/experience/$(TZ=Asia/Shanghai date +%Y-%m)
   mkdir -p .claude/docs/experience/review-reports
   # 子项目模式额外创建：
   [ -f .framework-sync.json ] && touch .claude/docs/experience/feedback-to-framework.md
   [ -f .framework-sync.json ] && [ ! -f .claude/PROJECT.md ] && touch .claude/PROJECT.md
   ```
   > **Why**：技能要求第六步必须写摘要到 `.claude/docs/experience/YYYY-MM/`，但该目录默认不存在，首次写入会失败。

**模式差异**：

| 维度 | 框架模式 | 子项目模式 |
|------|---------|-----------|
| 沉淀去向 | `.claude/skills/` / `CLAUDE.md` | `.claude/project-skills/` / `.claude/PROJECT.md` |
| 写入 Codex 镜像 | ✅ 同步到 `.codex/skills/` | ❌ 子项目不维护 Codex |
| 反哺机制 | ❌ | ✅ 通用经验追加到 `.claude/docs/experience/feedback-to-framework.md` |
| 禁止修改 | - | ⛔ 不改框架 `CLAUDE.md`、`.claude/skills/` |

---

## 第一步：收集输入源

按优先级扫描以下来源，输出给用户一个"我看了什么"的简报：

```bash
# 1. 最近提交
git log --since="{N} days ago" --pretty="%h %s" --no-merges

# 2. 未提交改动
git status --short
git diff --stat

# 3. 活跃任务
ls docs/tasks/active/ 2>/dev/null

# 4. 最近一次 /exp 摘要（避免重复沉淀）
ls -t .claude/docs/experience/*/*.md 2>/dev/null | head -1
```

会话 transcript 分析（依赖模型自主抽取）：
- 识别"纠正信号"：用户说"不要 / 禁止 / 错了 / 以后 / 应该 / 正确做法是"
- 识别"反复信号"：同类问题在会话中出现 ≥ 2 次
- 识别"惊讶信号"：用户说"原来 / 没想到 / 居然"

**简报格式**：
```markdown
📊 本次扫描
- Commits（最近 7 天）：12 条
- 未提交改动：5 个文件
- 活跃任务：2 个
- 会话长度：约 80 轮
- 上次 /exp：2026-04-10（已排除重复候选 3 条）
```

---

## 第二步：识别沉淀候选（按 6 类分类）

| # | 类型 | 识别信号 | 去向（框架模式） | 去向（子项目模式） |
|---|------|---------|----------------|------------------|
| ① | **新技能候选** | 同类问题反复出现且现有 52 个技能不覆盖 | `.claude/skills/xxx/` | `.claude/project-skills/xxx/` |
| ② | **现有技能漏洞** | 激活了某技能但仍答错/被用户补充规则 | 更新 `.claude/skills/yyy/SKILL.md` | 写入 feedback-to-framework.md |
| ③ | **新禁令/规范** | 用户说"以后不要…" / 纠正代码风格 | 追加到 `CLAUDE.md` 禁止表 | 追加到 `.claude/PROJECT.md` |
| ④ | **用户偏好/项目事实** | 用户描述自己的习惯 / 项目约束 | `memory/feedback_*.md` / `memory/project_*.md` | 同左 |
| ⑤ | **最佳实践文档** | 产出了完整的"怎么做"流程但不通用到做技能 | `.claude/docs/experience/YYYY-MM/xxx.md` | 同左 |
| ⑥ | **高频命令候选** | 某组操作一周内出现 ≥ 3 次 | `.claude/commands/xxx.md` + `.codex/skills/xxx/` | 很少触发，建议先观察 |

**⚠️ 子项目模式额外规则**：
- 每条候选必须标注 **归属**：[本项目专属] / [框架通用-反哺]
- [框架通用] 追加到 `.claude/docs/experience/feedback-to-framework.md`，不直接写框架文件

---

## 第三步：展示候选表（主交互界面）

```markdown
## 发现 X 条沉淀候选

| # | 摘要 | 类型 | 置信度 | 建议去向 | 归属（子项目） |
|---|------|------|-------|---------|--------------|
| 1 | Windows bash 禁用 `> nul` | ③ 新禁令 | 高 | CLAUDE.md 禁止表 | 框架通用 |
| 2 | log-audit 漏了"异步日志开关" | ② 技能漏洞 | 高 | 更新 log-audit | 框架通用 |
| 3 | 本项目用内部 IAM 单点登录 | ④ 项目事实 | 中 | memory/project_iam.md | 本项目专属 |
| 4 | "定时任务监控告警"反复被问 | ① 新技能 | 中 | scheduled-jobs 补强 或 新建 | 框架通用 |
| 5 | 本次解决的导出 Excel OOM 流程 | ⑤ 最佳实践 | 低 | .claude/docs/experience/ | 本项目专属 |

请选择处理方式：
[A] 逐项确认（推荐）
[B] 全部沉淀（高置信自动写，中低置信逐项问）
[C] 只沉淀高置信
[D] 全部跳过
```

---

## 第四步：逐项确认（以候选 1 为例）

```markdown
## 候选 #1：Windows bash 禁用 `> nul`

**原始信号**：会话中用户说"不要用 > nul 了，那会在 Windows 创建 nul 文件"

**建议写入**：CLAUDE.md 的"Bash/Shell 禁止项"表格

**预览**：
| ❌ 禁止 | ✅ 正确 |
| `command > nul` | `command > /dev/null 2>&1` |

**Why（从信号中抽取）**：Windows 的 nul 设备在某些 Shell 环境不被识别，会被当作普通文件名创建。

[Y] 写入   [E] 编辑后写入   [S] 跳过   [M] 换个去向
```

**关键约束**：每条沉淀必须带 **Why**（为什么）——这是 memory 规则的核心约定，避免下次读到这条规则时不知道为何存在。

---

## 第五步：分发执行

按用户确认的结果，分类调用：

| 候选类型 | 执行动作 |
|---------|---------|
| ① 新技能 | **委托给 add-skill 技能**（读 `.claude/skills/add-skill/SKILL.md` 完整流程） |
| ② 现有技能漏洞 | Edit 对应 `SKILL.md` + 同步 `.codex/skills/xxx/SKILL.md` |
| ③ 新禁令 | Edit `CLAUDE.md` / `AGENTS.md`（若需要）/ 子项目模式下 Edit `.claude/PROJECT.md` |
| ④ Memory | Write `memory/feedback_*.md` + 更新 `memory/MEMORY.md` 索引 |
| ⑤ 文档 | Write `.claude/docs/experience/YYYY-MM/YYYY-MM-DD-{摘要}.md` |
| ⑥ 新命令 | Write `.claude/commands/xxx.md` + `.codex/skills/xxx/SKILL.md`（加 YAML 头） |

---

## 第六步：归档摘要（每次必写）

写入 `.claude/docs/experience/YYYY-MM/YYYY-MM-DD-exp-summary.md`：

```markdown
# /exp 沉淀摘要 - {日期}

**仓库模式**：framework | subproject
**扫描范围**：会话 + 最近 N 天 commits
**识别候选**：X 条
**已沉淀**：Y 条
**已跳过**：Z 条

## 已沉淀
| # | 摘要 | 类型 | 去向 |
|---|------|------|------|
| 1 | ... | ... | ... |

## 已跳过（附原因）
| # | 摘要 | 跳过原因 |
|---|------|---------|

## 反哺候选（仅子项目）
已追加 N 条到 `.claude/docs/experience/feedback-to-framework.md`
```

**去重作用**：下次 /exp 读取最近摘要，避免重复沉淀同一条。

---

## 第七步：反哺提示（仅子项目模式）

若本次有"框架通用"归属的候选，末尾提示：

```markdown
🔁 反哺提示

本次识别出 N 条适合反哺框架的经验，已追加到：
  .claude/docs/experience/feedback-to-framework.md

下次在框架仓库打开会话时，运行 /exp review 会看到这些建议，
可决定是否合并进框架的 .claude/skills/ 或 CLAUDE.md。
```

---

## /exp review 子命令（盘点模式）

用于审计经验资产健康度，输出报告：

```bash
/exp review
```

**检查维度**：
1. **过时技能**：统计每个 Skill 最近 N 天是否被激活（依赖 hook 日志或人工标注）
2. **冗余重叠**：语义相近的 Skill 对（如 log-audit vs security-guard）
3. **孤岛经验**：`.claude/docs/experience/` 中超过 3 个月未被引用的文档
4. **反哺候选**（仅框架模式）：读所有下游子项目的 `feedback-to-framework.md`（如果约定了路径），列出可合并项
5. **Memory 腐化**：`memory/` 中引用已不存在的文件/方法的条目

**输出**：`.claude/docs/experience/review-reports/YYYY-QN-audit.md`

---

## 实现要点

### 标识"反复出现"的启发式

会话级：
- 用户 2 次以上要求做同一件事
- 模型 2 次以上给出同类代码
- 用户 2 次以上纠正同一个错误

Git 级：
- 同一目录 1 周内 ≥ 3 次修改
- 同类 commit message（"fix: xxx"）出现 ≥ 3 次

### 置信度评分（简化版）

| 证据 | 分值 |
|------|-----|
| 用户明确说"记住/以后/不要" | +3 |
| 同类信号出现 2 次 | +2 |
| 仅 1 次出现 | +1 |
| 有 Why 的完整描述 | +1 |
| 能映射到现有技能 | +1 |

总分 ≥ 5 为**高置信**，3-4 为**中**，1-2 为**低**。

### 防止过度沉淀

- 每次 /exp 最多推荐 **10 条候选**（超出部分压缩摘要到一条"其他观察"）
- 连续 2 次 /exp 都出现同一条但未被采纳的 → 自动降级跳过
- 摘要文档超过 50 条时提示执行 `/exp review` 清理

---

## 与其他命令/技能联动

| 联动对象 | 方式 |
|---------|------|
| `add-skill` | ① 新技能候选直接委托 |
| `task-tracker` | 识别出"任务已完成但未归档"时提示归档 |
| `/start` | 启动时读最近摘要展示"上次沉淀了 X" |
| `/progress` | 进度报告包含"本周沉淀 N 条" |
| `framework-sync` | 子项目做同步前先跑一次 /exp，避免沉淀被覆盖 |
| `MEMORY.md` | 每次沉淀后自动更新索引 |

---

## 注意事项

1. **优先更新，不优先新建** — ② 类（现有技能漏洞）应比 ① 类（新技能）优先，避免技能爆炸（52 → 100）。
2. **Why 是硬性要求** — 没有"为什么"的沉淀是僵尸经验，会随时间失效且无法判断。
3. **子项目不改框架** — 子项目模式下对 `.claude/skills/*` 和根目录 `CLAUDE.md` 只读。
4. **一次会话最多一次 /exp** — 避免同一批信号被重复捕获。
5. **敏感信息过滤** — 包含 token、密码、密钥、用户数据的内容自动跳过，不沉淀。

---

## 快速示例

**场景**：开发完一个支付模块，会话中用户纠正了 3 次"回调幂等必须用 Redis 分布式锁"、反复问"支付超时处理"。

```bash
/exp

📊 扫描完成：12 条 commits + 未提交改动 + 本次会话
发现 4 条沉淀候选：

| # | 摘要 | 类型 | 置信度 | 建议去向 |
|---|------|------|-------|---------|
| 1 | 支付回调幂等须加 Redis 分布式锁 | ③ 新禁令 | 高 | payment-integration 补强 |
| 2 | 支付超时处理流程（3 方案对比） | ⑤ 文档 | 中 | .claude/docs/experience/ |
| 3 | 本项目用第三方 xx 支付网关 | ④ 项目事实 | 高 | memory/project_payment.md |
| 4 | 可考虑新建 scheduled-jobs 结合 payment 技能 | ① 新技能 | 低 | 跳过（置信度不足） |

[A] 逐项确认  →  用户选 A  →  逐条处理  →  摘要写入 .claude/docs/experience/2026-04/2026-04-18-exp-summary.md
```
