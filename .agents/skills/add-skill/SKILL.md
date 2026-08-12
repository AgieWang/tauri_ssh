---
name: add-skill
description: |
  创建、修改、重命名、删除、同步、评估或审计 Tauri SSH 项目 Skill，并维护路由 Manifest 与多端镜像。仅在用户明确提出 Skill 开发、维护或状态评估时使用；普通代码修改、文档编辑或依赖更新不触发。
---

# Skill 创建与维护

## 目标

以 `.codex/skills/` 为项目自维护 Skill 的规范源，通过 `.codex/skill-routing/manifest.json` 声明路由和平台，通过同步脚本生成或校验 `.claude/skills/`、`.agents/skills/` 镜像。保持入口精简，同时保留安全、实现和验证规则。

## 强制边界

- 先读取 system `skill-creator` 的最新 `SKILL.md`，再修改项目 Skill。
- 只在 `.codex/skills/<name>/` 维护项目自有正文；禁止手工复制三套 Skill。
- `managed=upstream` 的 Skill 使用其官方生成或同步流程，不手写覆盖。
- 不直接修改只读的 `AGENTS.md`；项目专属约束写入 `.codex/PROJECT.md`。
- Frontmatter 只保留 `name` 和 `description`。名称与目录一致，使用小写 kebab-case。
- `description` 同时写清能力、强触发和排除边界；避免“文件、开发、检查、更新”等宽泛词单独触发。
- `SKILL.md` 只保留决策、强制步骤、引用索引和完成条件，通常 80～180 行，复杂入口不超过 220 行。
- 不通过删除安全、数据库、测试、构建、浏览器验收规则节约 Token。
- 所有源码与配置保持 UTF-8 无 BOM；保留已有业务注释和其他会话改动。

## 工作流

### 1. 明确操作与边界

确定是创建、修改、重命名、删除还是同步；列出具体 Skill、平台、触发意图、技术层、风险标签和互斥关系。修改前检查现有同名 Skill、相关领域 Skill、Manifest 条目及代码真实模式。

创建、修改、重命名或删除时，读取 [create-update-delete.md](references/create-update-delete.md)。

### 2. 规划渐进披露

将内容分为：

1. Frontmatter：发现和路由所需的最小元数据。
2. `SKILL.md`：执行本 Skill 必须立即知道的流程和禁令。
3. `references/`：长模板、完整代码示例、平台差异和详细清单。
4. `scripts/`：适合确定性执行且会重复使用的操作。
5. `assets/`：输出时复用、无需进入上下文的资源。

不要在入口和 reference 重复保存同一份长内容。超过 100 行的 reference 在顶部提供目录；入口直接链接每个可能需要的 reference，避免深层引用。

### 3. 设计 Frontmatter 与路由

读取 [frontmatter-and-routing.md](references/frontmatter-and-routing.md)，完成：

- `name`、目录名和 Manifest `name` 对齐。
- 描述包含专有强信号、明确意图和“不应触发”边界。
- Manifest 声明 `kind`、`activation`、`intents`、`layers`、强弱信号、排除项、互斥组、风险、平台、规范源和管理方式。
- 工作流命令在 Manifest 使用 `activation=explicit`，行为上仅显式激活；互斥工作流一次只保留一个；高风险信号保守召回。

路由只负责建议读取规则，不替代真实代码、配置、数据库或运行时证据。

### 4. 编写或迁移内容

- 先读 2～3 个相邻 Skill 和项目真实参考代码，复用现有风格。
- 使用祈使式指令，说明“做什么、何时做、何时停止”。
- 将脆弱或危险流程写成低自由度步骤；允许多种正确方案的领域保留判断空间。
- 修改长 Skill 时先迁移原规则到同目录 references，再缩短入口；不得静默丢弃禁令、异常分支、验证条件或必要模板。
- 不创建 README、CHANGELOG 等与 Skill 执行无关的辅助文件。
- 脚本必须实际运行代表性用例；引用路径必须可解析。

### 5. 声明规范源并同步镜像

读取 [mirror-sync.md](references/mirror-sync.md)。先更新 Manifest，再运行：

```bash
node .codex/scripts/sync-skills.cjs --check
```

只有用户已授权写入镜像且检查结果符合预期时，才执行：

```bash
node .codex/scripts/sync-skills.cjs --write
```

同步后再次运行 `--check`。不要用 `cp` 手工覆盖 `.claude/skills/` 或 `.agents/skills/`，也不要覆盖 upstream 管理正文。

### 6. 验证

读取 [validation.md](references/validation.md)，至少完成：

```bash
node .codex/scripts/validate-skills.cjs
node .codex/tests/skill-routing/router.test.cjs
node .codex/scripts/sync-skills.cjs --check
git diff --check
```

再检查 UTF-8 无 BOM、引用存在、入口行数、正向命中、负向不命中、互斥与高风险组合。脚本缺失或失败时不得宣称完成，需说明降级验证结果。

## 引用索引

- [frontmatter-and-routing.md](references/frontmatter-and-routing.md)：YAML、Manifest、强弱信号、排除、互斥与高风险路由。
- [create-update-delete.md](references/create-update-delete.md)：创建、修改、重命名、删除及内容拆分流程。
- [mirror-sync.md](references/mirror-sync.md)：`.codex` 规范源、多端生成、命令映射与 upstream 例外。
- [validation.md](references/validation.md)：结构、编码、路由、镜像和激活验证。

## 完成条件

- Frontmatter、Manifest、目录名和平台声明一致。
- 入口满足行数预算，长细节均可从直接引用恢复。
- 不存在宽泛自动触发、重复强信号或无效互斥。
- `.codex/skills/` 是唯一项目自维护规范源，镜像检查无漂移。
- 路由正反向用例、UTF-8、引用和 `git diff --check` 均通过。
- 未修改用户未授权的 upstream Skill、外部系统或其他会话文件。
