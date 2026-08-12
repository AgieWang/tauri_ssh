# Tauri SSH Skill 优化完整方案

> 状态：基础设施与 Skill 优化已实施，静态验收通过；真实任务灰度与运行时代表性验收待完成
>
> 适用项目：`/Users/bin/Documents/GitHub/tauri_ssh`
>
> 编写日期：2026-08-01
>
> 实施与静态复测日期：2026-08-04

## 1. 结论

本次已采用“准确性基线 + 确定性路由 + 精简入口 + 按需参考 + 自动校验 + 可回滚模式”的组合方式优化 Tauri SSH Skill。路由基础设施、项目自维护 Skill、按需 References、Claude/Agents 镜像和静态回归门禁已经落地；当前默认路由模式为 `active`，同时保留 `shadow` 与 `fallback`。

优化不以简单删除 Skill、减少测试或降低推理深度为手段。任何 Token 和执行时间收益，都必须建立在以下条件之上：

1. 不降低任务代码实现准确性。
2. 不省略现有代码模式检查、数据库语义确认、安全审查、构建测试和真实页面验证。
3. 不让 Hook 仅凭关键词直接决定实现方式。
4. 不让 Skill 路由结果替代代码、配置、DDL、运行时和真实 UI 证据。
5. 路由不确定时优先扩大必要检查范围，不能为了少加载一个 Skill 而漏掉高风险约束。

目标不是“让每次任务读取尽可能少的内容”，而是“让每次任务只读取完成该任务所需的最小完整信息集”。“最小”用于提高效率，“完整”用于保证准确性，两者缺一不可。

实施结果表明，入口上下文字节和行数已显著下降，225 个路由样例及 258 项测试已通过。多轮准确性与安全复审修复已经落盘，已知阻断均已关闭；保留 1 项 LOW 约束：同一仓库的同步写入器不支持并发，必须串行执行。但是，静态回归不能替代真实代码任务、数据库、发布和页面验收；在这些运行时证据补齐前，本文件不把优化状态表述为“全量生产验收完成”。

## 2. 优化前基线、问题证据与实施后结果

### 2.1 优化前文件规模

2026-08-01 在主工作区只读盘点得到：

| 目录              | Skill 入口数量 |   总字节数 | 总行数 | Frontmatter 字节数 | 正文字节数 |
| ----------------- | -------------: | ---------: | -----: | -----------------: | ---------: |
| `.codex/skills/`  |             56 |    577,761 | 18,356 |             26,796 |    550,965 |
| `.claude/skills/` |             47 |    537,227 | 16,395 |             21,469 |    515,758 |
| `.agents/skills/` |             41 |    483,795 | 15,298 |             19,181 |    464,614 |
| 合计              | 144 份物理文件 | 约 1.60 MB | 50,049 |                  — |          — |

说明：数量统计包含 6 个文件名为小写 `skill.md` 的入口；其余为标准大写 `SKILL.md`。三个目录中存在同名副本，因此 144 份物理文件不等于 144 个独立能力。当前三个目录的 Skill 并集约 60 个，只有 21 个同名 Skill 在三处内容完全一致，至少 23 个同名 Skill 已发生内容差异。

### 2.2 实施后入口规模

2026-08-04 使用 `node .codex/scripts/measure-skill-context.cjs --json` 复测：

| 目录              | Skill 入口数量 | 入口总字节数 | 入口总行数 | Frontmatter 字节数 | 正文字节数 | 入口字节 P50 / P95 | 入口行数 P50 / P95 |
| ----------------- | -------------: | -----------: | ---------: | -----------------: | ---------: | -----------------: | -----------------: |
| `.codex/skills/`  |             50 |      187,687 |      3,636 |             26,159 |    161,528 |      3,525 / 5,601 |           66 / 112 |
| `.claude/skills/` |             41 |      181,557 |      4,000 |             23,179 |    158,378 |      4,419 / 6,380 |           84 / 229 |
| `.agents/skills/` |             41 |      181,557 |      4,000 |             23,179 |    158,378 |      4,419 / 6,380 |           84 / 229 |
| 合计              |            132 |      550,801 |     11,636 |             72,517 |    478,284 |                  — |                  — |

与优化前相比，三端入口总字节数从 1,598,783 降至 550,801，下降约 65.5%；入口总行数从 50,049 降至 11,636，下降约 76.8%。本轮按用户授权移除了不再采用的阶段型能力及其镜像；其余降幅来自入口精简和按需披露，安全、数据库、测试、构建与浏览器验收能力均保留。

References 属于按需加载层，不能与入口自动上下文混算：

| 目录                           | Reference 文件数 |  字节数 |  行数 |
| ------------------------------ | ---------------: | ------: | ----: |
| `.codex/skills/*/references/`  |               73 | 239,302 | 6,768 |
| `.claude/skills/*/references/` |               58 | 198,339 | 5,665 |
| `.agents/skills/*/references/` |               58 | 198,339 | 5,665 |

这些内容没有被删除，而是从默认入口迁到按场景读取的文件。实际 Token 数仍需运行时 usage 才能确认，本文件只记录 UTF-8 字节，不把字节换算成 Token。

### 2.3 优化前已有的正确优化

`.codex/hooks/skill-forced-eval.cjs` 已经是一个“极简版” Hook：

- 不再把全部 Skill 清单重复注入 Codex 上下文。
- 设计为跳过恢复会话和上下文压缩提示。
- 会跳过斜杠命令。
- 只输出评估、读取、实现的流程约束。

这部分方向正确，应当保留。第二轮复审发现旧实现曾从用户正文中的 `context window` 等文字猜测恢复态，可能误跳过合法任务；当前已改为只信任宿主结构化字段，并增加真实 Hook 子进程回归。后续不是退回到“大清单全注入”，而是在此基础上增加可测试的路由结果、冲突消解和准确性保护。

### 2.4 优化前主要问题及当前处置

#### 问题一：Claude Hook 仍然硬编码整份技能清单

`.claude/hooks/skill-forced-eval.cjs` 每次普通 Prompt 都注入完整分类清单、激活规则和示例。它与每个 Skill 的 Frontmatter 重复，且修改 Skill 后需要同步维护 Hook 列表，容易产生 Token 浪费和声明漂移。

#### 问题二：多个 Skill 使用过宽触发词

典型冲突包括：

| 宽泛词                         | 可能同时误触发的 Skill                                           |
| ------------------------------ | ---------------------------------------------------------------- |
| `设计`、`方案`                 | `brainstorm`、`architecture-design`、`tech-decision`             |
| `Command`、`invoke`、`IPC`     | `api-development`、`tauri-commands`、`command`                   |
| `权限`、`Capabilities`、`安全` | `security-permissions`、`tauri-capabilities`                     |
| `发布`、`build`、`release`     | `release`、`release-publish`、`tauri-packaging`、`tauri-updater` |
| `文档`、`更新文档`             | `doc-generation`、`docs-management`、`sync`、`update-docs`       |
| `状态`                         | `store-management`、`progress`、`update-status`、业务状态排查    |
| `文件`、`工具`                 | `project-navigator`、`file-storage`、`utils-toolkit`             |

宽泛触发词不仅增加 Token，还会使多个 Skill 给出相互重叠或不同侧重点的规则，降低执行稳定性。

#### 问题三：部分入口文件过长

`.codex/skills/` 中有 13 个 Skill 超过 400 行，8 个超过 600 行。最大文件包括：

| Skill            |  行数 | 字节数 |
| ---------------- | ----: | -----: |
| `project-init`   | 1,491 | 54,598 |
| `add-skill`      | 1,085 | 33,142 |
| `check`          |   783 | 22,218 |
| `tauri-commands` |   655 | 16,694 |
| `task-tracker`   |   632 | 18,882 |
| `code-patterns`  |   626 | 18,171 |
| `command`        |   605 | 18,348 |
| `exp-sediment`   |   615 | 21,060 |

其中很多内容是模板、长示例、平台分支或低频操作。把这些内容全部放在入口 `SKILL.md`，会导致一次普通触发加载大量本次任务不需要的上下文。

#### 问题四：三套 Skill 副本已经发生漂移

例如 `bug-detective`、`database-ops`、`error-handler`、`release-publish`、`ui-frontend` 等在不同目录大小明显不同；`env-isolation` 在 `.codex/skills/` 缺失；部分 Codex 工作流 Skill 只存在于 `.codex/skills/`。

副本漂移会带来两类准确性风险：

1. 同一个问题在 Codex、Claude 或其他 Agent 中得到不同执行约束。
2. 修改了一个目录后误以为所有运行时已生效。

#### 问题五：工作流命令与领域 Skill 混在同一自动触发池

`dev`、`command`、`check`、`start`、`progress`、`next`、`release`、`sync`、`update-docs`、`update-status` 等本质上更接近显式工作流命令。它们不应因为用户在普通句子中出现“开发”“命令”“检查”“下一步”“状态”等词就自动加载。

#### 问题六：验证规则主要是文字清单，缺少可执行门禁

当前多个 Skill 都写了“检查、测试、构建、格式化”等要求，但没有统一脚本根据实际变更文件生成验证矩阵，也没有自动检查：

- Skill Frontmatter 是否过宽或重复。
- 三套镜像是否一致。
- 路由是否漏掉高风险 Skill。
- 修改 Rust、TypeScript、数据库、Capabilities 或页面后是否选择了对应验证动作。

#### 问题七：存在内容质量问题

例如 `theme-system` 的描述中已经出现乱码字符。编码错误会直接影响 Skill 匹配、文档可读性和模型理解，必须作为准确性问题处理，而不是单纯格式问题。

#### 问题八：阶段型能力增加路由与维护成本

阶段型入口依赖额外互斥、状态推断、镜像和命令映射，但与本项目“真实代码证据 + 领域 Skill + 变更文件验证矩阵”的准确性主链重叠。本轮按用户授权移除阶段型入口，保留通用领域规范、显式工作流、高风险门禁和运行时验收规则。

当前处置状态：

| 优化前问题             | 实施结果                                                                              |
| ---------------------- | ------------------------------------------------------------------------------------- |
| Claude Hook 全量硬编码 | 已删除，改为复用确定性 Router                                                         |
| 宽泛触发和重叠         | 已通过 Manifest 强弱信号、exclude、mutex 和正反向用例收敛                             |
| 长入口                 | 50 个 project 入口全部不超过 117 行，细节迁入 73 个 Codex References                  |
| 三端漂移               | Manifest 管理范围 `sync --check` PASS；upstream/platform-local 明确保留               |
| 工作流误触发           | 18 个 explicit 激活记录，普通自然语言边界已测试                                       |
| 缺少可执行门禁         | Router、validate、sync、measure 和 258 项测试已落地；自动 diff→矩阵执行器仍待补       |
| 编码/乱码              | UTF-8、BOM、替换字符检查通过，`theme-system` 描述已修复                               |
| 阶段型能力维护成本     | 已移除阶段入口、阶段推断、互斥、组合、命令和镜像；代码规范由领域 Skill 与验证矩阵兜底 |

## 3. 优化目标与不可突破的边界

### 3.1 目标优先级

| 优先级 | 目标                 | 解释                                                     |
| ------ | -------------------- | -------------------------------------------------------- |
| P0     | 任务代码准确性不下降 | 任何优化若导致漏读约束、漏测或错误实现，立即回滚         |
| P0     | 高风险任务零漏路由   | 数据库、安全、凭据、发布、远程操作、删除操作必须保守处理 |
| P1     | 减少无关 Skill 加载  | 普通任务默认只加载最小完整技能集                         |
| P1     | 减少重复上下文       | Hook 不重复注入已有 Frontmatter，不重复加载相同规则      |
| P1     | 缩短执行前准备时间   | 将静态匹配、冲突消解和检查交给确定性脚本                 |
| P2     | 降低维护成本         | 一处修改、自动同步、自动发现漂移                         |
| P2     | 可观测与可回滚       | 每次优化有基线、对照、灰度和回滚开关                     |

### 3.2 不允许用来节省 Token 的做法

- 不删除数据库、测试、安全、错误处理或浏览器验收能力。
- 不把“构建通过”当作完整验收。
- 不减少用户明确要求的独立测试和代码审查。
- 不省略修改前阅读现有参考代码。
- 不用样例覆盖字段说明、DDL 或真实接口契约。
- 不用静态代码分析替代真实页面验证。
- 不把所有复杂任务强行限制为最多两个 Skill。
- 不通过降低模型推理强度掩盖路由问题。
- 不把全部项目规则塞进一个超大“万能 Skill”。

### 3.3 准确性不变量

无论路由选中几个 Skill，以下规则都必须一直有效：

1. 修改已有功能前读取真实参考代码和调用链。
2. Rust 后端遵守 Command → Service → Database 分层。
3. TypeScript 的 `invoke` 统一走 `src/lib/api/`，类型与 Rust 对齐。
4. 涉及数据库 DDL 或数据格式时，读取配置并通过 Tauri SSH MCP 查询真实数据库。
5. 涉及页面变更时，必须使用 Codex 内置浏览器或 Control Chrome 验证。
6. 涉及凭据、Git、服务器和数据库操作时，优先使用 Tauri SSH MCP，不能暴露明文凭据。
7. 交付前执行与变更类型相匹配的格式化、静态检查、测试、构建和 `git diff --check`。
8. 保留其他会话未提交工作，不执行 stash、reset、跨分支切换或全量 add。

这些规则放在短基线中，不依赖某个具体 Skill 是否恰好被命中。

## 4. 推荐架构

```text
用户 Prompt
   |
   v
Prompt 规范化与显式命令识别
   |
   +-- 宿主结构化恢复/压缩状态 ----> 跳过重复路由注入
   |
   +-- 显式 /command 或 $skill ----> 只激活指定工作流，按需补安全 Skill
   |
   v
确定性路由器
   |- 识别任务意图：解释 / 诊断 / 方案 / 实现 / 验证 / 发布
   |- 识别技术层：React / IPC / Rust / SQLite / 权限 / 文件 / 更新
   |- 识别风险：凭据 / 数据库 / 远程 / 删除 / 发布 / 外部写入
   |- 强触发、排除词、互斥组、组合规则和文件证据
   v
最小完整 Skill 集
   |- 普通单域任务：通常 1～2 个
   |- 跨层实现任务：通常 2～4 个
   |- 高风险任务：不设硬上限，以准确性为准
   v
精简 SKILL.md 入口
   |- 决策规则
   |- 强制步骤
   |- 何时读取哪个 reference
   |- 验证出口条件
   v
按需 references / scripts
   |- 只读取本任务对应章节
   |- 运行确定性校验脚本
   v
代码实现与证据化验收
```

### 4.1 四层信息模型

| 层            | 内容                                          | 加载时机       | Token 策略               |
| ------------- | --------------------------------------------- | -------------- | ------------------------ |
| L0 项目短基线 | 永久准确性规则、工具优先级、验证底线          | 每个任务       | 必须短、小、稳定         |
| L1 路由元数据 | 强触发、排除、互斥、风险、平台                | Hook 执行时    | 脚本读取，不整体注入模型 |
| L2 Skill 入口 | 本 Skill 决策树、强制步骤、引用索引、完成条件 | Skill 被选中时 | 每个入口尽量 80～180 行  |
| L3 References | 长模板、代码示例、平台差异、详细清单          | 具体场景需要时 | 按需读取，不默认全读     |

### 4.2 路由原则

1. 使用“强触发 + 意图 + 技术层 + 风险”组合，不使用单个宽泛词直接命中。
2. `设计`、`状态`、`文件`、`数据`、`检查`、`开发` 等词只能作为弱信号。
3. 高风险信号拥有更高召回优先级；宁可多加载一个安全 Skill，也不能漏掉安全边界。
4. 工作流命令默认 `explicit-only`，只在显式 `/命令`、`$skill` 或明确请求完整工作流时激活。
5. 领域代码规范由专有强信号、跨层 bundle 和变更文件验证矩阵共同兜底；显式工作流不参与普通自然语言自动路由。
6. 路由只决定读取哪些规则，不直接决定代码修改；实现仍必须以真实仓库证据为准。
7. 当用户请求跨层功能时，允许组合多个 Skill，不设置破坏准确性的固定上限。

## 5. 方案对比与推荐决策

| 方案              | 内容                                                   | 优点                                 | 缺点                                                    | 结论   |
| ----------------- | ------------------------------------------------------ | ------------------------------------ | ------------------------------------------------------- | ------ |
| A：只压缩 Hook    | 缩短 Hook 输出，不改 Skill                             | 风险小、改动少                       | 当前 Codex 已基本完成，无法解决误触发、长入口和副本漂移 | 不足   |
| B：只压缩 Skill   | 所有 `SKILL.md` 变短                                   | 单次加载 Token 下降                  | 路由仍可能一次选中很多 Skill，副本仍会漂移              | 不足   |
| C：只做关键词路由 | 用脚本命中 Skill                                       | 执行快                               | 纯关键词容易漏掉语义和高风险隐含任务                    | 不采用 |
| D：混合方案       | 短基线、结构化路由、精简入口、按需引用、自动测试、灰度 | 同时改善准确性、效率、Token 和维护性 | 初期需要建立清单与测试                                  | 推荐   |

决策：已采用 D。路由与测试先落地，随后使用多代理按互不冲突目录并行拆分 Skill；这偏离了原定“小范围串行切换”，偏差、静态补偿措施和未完成的真实任务灰度记录在第 12～15 节。

## 6. 已落地文件结构

```text
.codex/
├── PROJECT.md
├── hooks.json
├── hooks/
│   └── skill-forced-eval.cjs
├── skill-routing/
│   ├── manifest.json
│   ├── bundles.json
│   └── README.schema.txt
├── scripts/
│   ├── skill-router.cjs
│   ├── validate-skills.cjs
│   ├── sync-skills.cjs
│   └── measure-skill-context.cjs
├── tests/
│   └── skill-routing/
│       ├── cases.json
│       ├── router.test.cjs
│       └── expected-matrix.json
└── skills/
    └── <skill>/
        ├── SKILL.md
        └── references/
            └── <按场景拆分的参考文件>.md

.claude/
├── hooks/skill-forced-eval.cjs
├── commands/<由同步脚本生成或校验的命令正文>.md
└── skills/<共享 Skill 镜像>/

.agents/
└── skills/<共享 Skill 镜像>/
```

上述结构已经落地。实际新增 3 个路由元数据文件、4 个 Node 脚本、3 个测试/矩阵文件，并在 `package.json` 增加 4 个不依赖第三方包的维护入口。

## 7. 已落地路由清单

### 7.1 `manifest.json` 实际字段与规模

```json
{
  "version": 1,
  "sourceOfTruth": ".codex/skills",
  "allowedStrongSignalOverlaps": {
    "channel stream": ["tauri-commands", "tauri-events"]
  },
  "skills": [
    {
      "name": "tauri-commands",
      "kind": "domain",
      "activation": "auto",
      "intents": ["implement", "refactor"],
      "layers": ["ipc", "rust-command"],
      "strongSignals": [
        "#[tauri::command]",
        "generate_handler",
        "State<",
        "AppHandle"
      ],
      "weakSignals": ["Command", "invoke", "IPC"],
      "excludeWhen": ["仅解释 API 概念", "显式 /command 工作流"],
      "mutexGroup": "ipc-command-detail",
      "riskTags": [],
      "platforms": ["codex", "claude", "agents"],
      "source": ".codex/skills/tauri-commands/SKILL.md",
      "managed": "project"
    }
  ]
}
```

当前 Manifest 共 54 条记录：50 个 `managed=project`、4 个 `managed=platform-local`。激活类型为 34 个 `auto`、18 个 `explicit`、2 个 `risk`；平台覆盖为 Codex 50、Claude 41、Agents 41。清单不再包含阶段型 kind、activation、phase 字段或专用互斥配置。

`managed` 决定同步边界：项目自维护内容以 `.codex/skills/` 为规范源；upstream 只校验不覆盖；platform-local 只报告和保留。由于当前没有可信历史 provenance 证明某个整项旧镜像曾由同步器受管，Manifest 明确拒绝 `status=retired` 自动整项删除；未列入 Manifest 的 Skill 目录和生成命令始终保留。删除或重命名时，同步器只创建当前新目标，旧镜像必须在用户明确授权后逐个精确人工处理。`phase`、`status` 只在对应管理场景出现，不能把平台本地或 upstream 文件机械改写成 Codex 正文。

### 7.2 `bundles.json` 实际组合规则

示例：

```json
{
  "bundles": [
    {
      "id": "fullstack-ipc-feature",
      "when": {
        "signalsAll": ["react", "command"]
      },
      "required": ["ui-frontend", "api-development"],
      "conditional": {}
    }
  ]
}
```

当前共 7 个 bundle，使用 `signalsAll` 或 `signalsAny` 组合全栈 IPC、SQLite 迁移、文件系统、插件权限、完整发布、凭据/远程写入和 Capabilities。组合规则只负责避免漏选，不把所有可能相关 Skill 无条件塞入上下文。

## 8. 逐文件实施台账

### 8.1 核心配置与 Hook 实施结果

| 文件                                  | 实施状态 | 为什么                                                           | 实际修改内容                                                                                                                                                                                                                                                         |
| ------------------------------------- | -------- | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AGENTS.md`                           | 不修改   | 这是框架只读副本，且包含完整项目通用规约；直接改会被框架同步覆盖 | 保持现状；需要补充的项目规则写入 `.codex/PROJECT.md`                                                                                                                                                                                                                 |
| `.codex/PROJECT.md`                   | 已修改   | 原文件缺少 Skill 路由边界和不可削弱的验收基线                    | 已增加“准确性不变量”“Skill 最小完整集”“数据库/浏览器/安全/验证门禁”“显式工作流规则”；保持短规则，不复制 AGENTS 全文                                                                                                                                                  |
| `.codex/config.toml`                  | 不修改   | 已启用 Hooks，优化不需要改变模型或运行时参数                     | 保持 `hooks = true`；不通过调整模型推理强度节省 Token                                                                                                                                                                                                                |
| `.codex/hooks.json`                   | 已修改   | 需要让 Hook 调用新的路由脚本，并保留超时与失败降级               | `UserPromptSubmit` 仍调用同名 Hook；更新注释和状态文本；超时保持 10 秒；路由失败不阻断任务                                                                                                                                                                           |
| `.codex/hooks/skill-forced-eval.cjs`  | 已修改   | 原 Hook 只给流程，不提供可测试候选集                             | 已调用 `skill-router.cjs`；只信任宿主结构化字段判断恢复/压缩状态，不再从用户正文猜测；保留斜杠命令跳过；支持 `active/shadow/fallback`；在字节预算内保留完整候选并压缩理由；输出风险和不确定项；限制 1.5 KiB；异常时退回极简评估                                      |
| `.claude/settings.json`               | 不修改   | 现有 Hook 路径不变即可复用新实现                                 | 仅验证仍指向 `.claude/hooks/skill-forced-eval.cjs`，避免无必要配置改动                                                                                                                                                                                               |
| `.claude/hooks/skill-forced-eval.cjs` | 已修改   | 原 Hook 硬编码完整清单，每轮注入量大且会与 Skill 内容漂移        | 已删除硬编码清单并复用 `.codex/scripts/skill-router.cjs`；显式使用 `platform=claude`，可命中 Claude platform-local Skill 且不会注入 Codex-only Skill；只信任宿主结构化恢复/压缩状态；保留真实 expanded command 和斜杠命令跳过；在 2 KiB 预算内保留完整候选并压缩理由 |
| `.codex/hooks/pre-tool-use.cjs`       | 不修改   | 属于安全命令拦截，不是 Skill Token 问题；删除或合并会降低安全性  | 保持独立，避免路由优化影响危险命令拦截                                                                                                                                                                                                                               |
| `.claude/hooks/pre-tool-use.cjs`      | 不修改   | 同上                                                             | 保持现状                                                                                                                                                                                                                                                             |
| `.codex/hooks/session-start.cjs`      | 不修改   | 负责经验摘要加载，与 Skill 路由职责不同                          | 后续单独评估 8 KB 摘要上限，本次不混改                                                                                                                                                                                                                               |
| `.codex/hooks/stop.cjs`               | 不修改   | 与 Skill 路由无直接关系                                          | 本方案不把验证逻辑塞进 Stop Hook，避免自动续轮或结束死循环                                                                                                                                                                                                           |

### 8.2 新增路由、同步、校验和度量文件

| 文件                                              | 为什么新增                                                                                 | 实际职责与结果                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `.codex/skill-routing/manifest.json`              | 消除 Hook、AGENTS、Frontmatter 多处手工维护的路由漂移                                      | 保存 Skill 类型、自动/显式激活、意图、技术层、强弱信号、排除条件、互斥组、风险标签、平台和规范源路径；不保存长文档                                                                                                                                                                                                                                                                                                                                           |
| `.codex/skill-routing/bundles.json`               | 保证跨层任务不会因“最小化”漏掉必要 Skill                                                   | 保存少量确定性组合：全栈 IPC、SQLite 迁移、文件导入导出、自动更新、发布、安全凭据与权限；条件满足才展开                                                                                                                                                                                                                                                                                                                                                      |
| `.codex/skill-routing/README.schema.txt`          | 防止维护者误解 JSON 字段，但不额外生成大篇 Markdown 文档                                   | 用纯文本说明字段类型、优先级、互斥和降级规则；控制在 150 行内                                                                                                                                                                                                                                                                                                                                                                                                |
| `.codex/scripts/skill-router.cjs`                 | 将匹配从不可回归的模型自由判断升级为可测试的确定性预选                                     | 解析 Prompt；识别显式命令、意图、技术层和风险；按目标平台过滤；计算强弱信号；应用排除、互斥、bundle 和安全补充；保守覆盖生产库/RDS/ADB/真实 DDL、远端 Git、API key/PAT/私钥/id_rsa、上传/上线/部署等同义风险；返回排序候选；不读取源码、不写文件、不接触凭据                                                                                                                                                                                                 |
| `.codex/scripts/validate-skills.cjs`              | 把格式、一致性和边界检查变成自动门禁                                                       | 校验 UTF-8 无 BOM、YAML name、描述长度、强弱触发、重复强信号、引用、入口预算、乱码和镜像漂移；对 Manifest version/kind/activation/managed/status/platform、bundle 引用和 matrix 规则执行 fail-closed schema 校验；project source 必须精确位于自身 `.codex/skills/<name>/` 内，拒绝规范化路径逃逸                                                                                                                                                             |
| `.codex/scripts/sync-skills.cjs`                  | 解决三套副本手工复制导致的差异，并在没有可信历史 provenance 时避免误删平台本地或未列管内容 | 独立执行 fail-closed Manifest 安全校验；以精确 `source` 为准按 `platforms` 同步完整资源树；生成 Claude command；拒绝恶意名称、source/resource 路径穿越和符号链接越界；拒绝不受支持的生命周期状态；未列入 Manifest 的 Skill 镜像和生成命令始终保留；重命名只创建新镜像并保留旧名称，旧项由用户明确授权后逐个精确人工处理；`--prune` 只可清理当前仍列管 Skill 目录内的陈旧资源；原子写失败清理自身临时文件；同仓库写模式必须串行，当前不承诺多 writer 并发安全 |
| `.codex/scripts/measure-skill-context.cjs`        | 用数据验证 Token 优化是否有效                                                              | 输出每个目录的文件数、Frontmatter 字节、正文总量、入口 P50/P95、路由样例加载字节；不虚构 Token，Token 只在能获得运行时 usage 时记录                                                                                                                                                                                                                                                                                                                          |
| `.codex/tests/skill-routing/cases.json`           | 防止路由优化后出现漏选或误选                                                               | 已保存 225 个代表性 Prompt、必选、禁选、精确 Skill 集、精确风险集和原因；风险同义语义扩展到生产数据库、远端 Git、真实凭据、模型 Token 计量、否定发布、只读 Updater、破坏性操作、制品库、下载站和外部上线的自然语序                                                                                                                                                                                                                                           |
| `.codex/tests/skill-routing/router.test.cjs`      | 自动执行路由回归                                                                           | 当前执行 258 项测试，覆盖全部用例、平台/platform-local 隔离、高风险同义语义、代码规范跨子任务组合及受控否定子句、真实 Hook 子进程、带参数斜杠命令安全补充、仅结构化恢复态、完整候选预算压缩、严格 schema、非规范 source 拒绝、不支持生命周期状态拒绝、unlisted 镜像/command 永久保留、重命名保留旧名、当前受管目录陈旧资源 prune 授权、原子写失败清理、完整资源树、符号链接与路径越界保护                                                                    |
| `.codex/tests/skill-routing/expected-matrix.json` | 将“修改什么就验证什么”结构化                                                               | 定义路径到验证动作映射，例如 `.rs` → fmt/clippy/test，`.tsx` → format/tsc/vitest/browser，schema → cargo test/迁移测试，capabilities → JSON/权限运行时检查                                                                                                                                                                                                                                                                                                   |
| `package.json`                                    | 给维护者提供统一入口                                                                       | 仅在本方案范围新增 `skill:check`、`skill:test`、`skill:measure`、`skill:sync:check`；不引入 npm 依赖。该文件原有并发业务改动不计入本方案                                                                                                                                                                                                                                                                                                                     |

多轮复审修复后的最终静态结果：`validate-skills` PASS（54/54 源、7 bundles、225 cases、9 matrix rules）；`router.test.cjs` 258/258 PASS；`sync-skills --check` PASS。多轮复审识别的已知阻断均已关闭；并发同步 writer 需要串行执行作为 LOW 约束保留。文档格式、UTF-8 和限定范围 `git diff --check` 在本文件定稿时再次执行。

本次已实际执行一次 `sync-skills --write`，只更新 `add-skill` 与 `exp-sediment` 的 8 个受管镜像文件；没有使用 `--prune`，随后重新执行 `--check` 为 PASS。另按用户授权逐个删除 19 个已废弃阶段资源及对应空目录，没有扫描或删除其他未列管资源。隔离测试确认不受支持的生命周期状态会被拒绝，未列入 Manifest 的其他镜像和生成命令一律保留。

### 8.3 Skill 统一实施规则

50 个项目自维护 Skill 已按以下规则处理：

1. Frontmatter 第一行只描述边界，不宣传“所有相关问题都自动使用”。
2. 强触发词必须是领域专有词或明确意图；宽泛词只放弱信号，不直接触发。
3. 增加“不应触发”示例，解决同类 Skill 冲突。
4. `SKILL.md` 只保留决策树、强制步骤、引用索引和完成条件。
5. 长示例、平台差异、完整模板、速查表拆到 `references/`。
6. 入口目标 80～180 行；复杂 Skill 可到 220 行，但需要说明理由。
7. 任何拆分不得删除规则，只改变加载位置。
8. 涉及安全、数据库、发布的关键禁令保留在入口，不能全部下沉。
9. 所有文件修正为 UTF-8 无 BOM，并检查中文乱码。

### 8.4 逐个 Skill 入口实施清单

下表中的路径默认是 `.codex/skills/<name>/SKILL.md`。表内 50 个项目 Skill 已完成修改。所有 project 入口均不超过 117 行；共享 Skill 通过同步脚本镜像，不再人工维护三份正文。

| Skill 文件                           | 为什么修改                                                               | 实际修改内容                                                                                                                                                                               |
| ------------------------------------ | ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `add-skill/SKILL.md`                 | 1,085 行且仍声明 `.claude/skills` 为唯一源，与当前三目录现实和新方案冲突 | 缩为技能创建/修改入口；改为 Manifest 声明规范；写清 `.codex/skills` 规范源、镜像生成和 upstream 例外；把 YAML 规范、示例、同步、重命名、删除、验证分别移到 `references/`；禁止手工复制三份 |
| `api-development/SKILL.md`           | 与 `tauri-commands`、`command` 重叠                                      | 将边界限定为“端到端 IPC 契约设计与前后端类型对齐”；普通 Rust Command 高级细节交给 `tauri-commands`；显式脚手架交给 `command`；拆出参数命名、返回类型、注册和 TS API 参考                   |
| `architecture-design/SKILL.md`       | `设计`、`模块`、`结构`过宽                                               | 只在模块边界、分层、跨进程职责或重构架构时触发；普通方案探索不触发；保留架构约束，案例下沉                                                                                                 |
| `autonomous-dev/SKILL.md`            | 属于终端式连续开发工作流，不应自动触发                                   | 标记 `explicit-only`；只有用户明确要求自主连续开发、循环推进或“不停直到完成”时激活；保留停止条件、每轮验证和权限边界                                                                       |
| `brainstorm/SKILL.md`                | “方案、建议、怎么做”会命中大量普通问题                                   | 只处理发散探索且用户尚未要求实现的任务；明确实现任务不自动附加；把通用技术栈清单移到项目基线或参考文件                                                                                     |
| `bug-detective/SKILL.md`             | 当前 Codex 版较短，但三端内容不一致                                      | 统一为诊断型 Skill；明确“诊断不等于授权修复”；加入证据顺序：复现、日志、调用链、数据、部署版本、浏览器；排除纯编译语法问题的专用 resolver 场景                                             |
| `check/SKILL.md`                     | 783 行，且“检查/review”过宽                                              | 标记显式工作流；入口只做变更分类和检查编排；语言/模块具体规则移到 references；最终读取 `expected-matrix.json` 决定检查，不重复所有编码规范                                                 |
| `code-patterns/SKILL.md`             | 626 行，和各领域 Skill 重复规范                                          | 只在用户明确问最佳实践、设计模式、规范重构时触发；删除重复的数据库、UI、Command 完整教程，改为引用对应领域 Skill                                                                           |
| `collaborating-with-codex/SKILL.md`  | 当前环境本身就是 Codex，普通“代码审查”不应触发外部协作                   | 标记 `explicit-only`；只有用户明确要求 Codex CLI 桥接、多模型对比或委托时使用；保留脚本安全和 diff 边界                                                                                    |
| `collaborating-with-gemini/SKILL.md` | “UI设计/CSS/样式”会使普通前端任务误触发外部模型                          | 标记 `explicit-only`；只有用户明确要求 Gemini 才激活；普通 UI 使用 `ui-frontend`                                                                                                           |
| `command/SKILL.md`                   | 605 行，与两个 IPC Skill 重叠                                            | 定义为显式 `/command` 脚手架工作流；不因普通“命令”触发；入口只做收集参数、生成文件清单、执行和验证，长模板拆 references                                                                    |
| `database-ops/SKILL.md`              | “数据、表、查询”过宽，且三端内容不同                                     | 明确只覆盖 Tauri 本地 SQLite/rusqlite；数据库 DDL/数据格式必须按项目规则查询真实来源；把迁移、DAO、事务、并发、测试分拆；高风险数据修改规则留入口                                          |
| `dev/SKILL.md`                       | “开发、新功能、页面开发”几乎会命中所有编码任务                           | 标记显式 `/dev` 全栈编排；普通实现由领域 Skill 路由；保留跨层文件清单和验证出口，模板下沉                                                                                                  |
| `doc-generation/SKILL.md`            | 与 `docs-management`、`update-docs` 重叠                                 | 限定为项目内开发者 Markdown/API 参考生成；不处理 VitePress；不因普通“写文档”自动触发站点工作流                                                                                             |
| `docs-management/SKILL.md`           | 371 行且只针对 VitePress，当前请求这类内部方案不应误用其站点流程         | Frontmatter 明确“VitePress 对外站点”；内部 `docs/` 方案文档不触发初始化；长初始化与同步示例拆 references                                                                                   |
| `error-handler/SKILL.md`             | 与 `bug-detective`、Rust 错误和 React 错误边界交叉                       | 只在实现/重构错误传播、AppError、ErrorBoundary 时触发；排查已发生故障优先 `bug-detective`；Rust 所有权编译问题交给 `rust-fundamentals`                                                     |
| `exp/SKILL.md`                       | 与 `exp-sediment` 高度重叠                                               | 作为显式 `/exp` 工作流入口，压缩到执行步骤；详细沉淀方法引用 `exp-sediment`；不因普通“总结”自动触发                                                                                        |
| `exp-sediment/SKILL.md`              | 615 行且触发词包含大量“以前/之前”泛化表达                                | 仅在用户明确要求沉淀经验或查询本项目沉淀时激活；长审计清单、资产类型和模板拆 references；遵守“只有用户明确要求才更新 Memory”                                                               |
| `file-storage/SKILL.md`              | “保存、打开、文件”会误命中代码定位和普通文件编辑                         | 只在应用功能涉及文件系统 API、导入导出、拖放或对话框时触发；普通仓库文件编辑不触发                                                                                                         |
| `git-workflow/SKILL.md`              | “提交、版本发布”与发布 Skill 重叠                                        | 只处理 Git 分支、提交、合并、推拉等操作；发布构建交给 release 系列；保留多会话并发和逐文件暂存禁令                                                                                         |
| `i18n-development/SKILL.md`          | 目前边界相对清晰                                                         | 精简重复技术栈说明；强触发保留 i18n、locale、语言切换；普通中文文案修改不自动触发                                                                                                          |
| `json-serialization/SKILL.md`        | 371 行，“数据传输/类型转换”可能过触发                                    | 只处理 serde/JSON/TS 序列化边界；普通业务字段映射不自动触发；把日期、枚举、Option、camelCase 示例拆 references                                                                             |
| `next/SKILL.md`                      | “建议、优先级”过宽                                                       | 标记显式 `/next` 或明确项目路线建议；普通任务中的“下一步”不触发全仓扫描                                                                                                                    |
| `notification-system/SKILL.md`       | “消息、toast”会与 Ant Design 页面提示混淆                                | 只在原生系统/桌面通知时触发；页面 `message`/`notification` 默认由 `ui-frontend` 处理                                                                                                       |
| `performance-doctor/SKILL.md`        | “优化”会误命中 Skill 自身优化、代码整理等非运行性能任务                  | 只在应用运行性能、内存、CPU、启动、体积或编译性能诊断时激活；提示词“Skill 优化”不得命中此 Skill                                                                                            |
| `progress/SKILL.md`                  | “报告、状态、概览”过宽                                                   | 标记显式 `/progress` 或明确项目进度报告；普通状态字段、请求状态不触发                                                                                                                      |
| `project-init/SKILL.md`              | 1,491 行，是最大入口；低频但单次开销很大                                 | 标记显式新项目工作流；入口只保留阶段、权限和回滚；模板更新、信息收集、目录复制、标识替换、Git、签名、平台步骤拆成多个 references；不得自动执行外部发布                                     |
| `project-navigator/SKILL.md`         | “目录、文件、定位”会命中大量普通代码任务                                 | 只在用户需要理解仓库结构、寻找入口或追踪调用链时触发；已提供精确文件路径的修改任务通常不额外加载                                                                                           |
| `release/SKILL.md`                   | 与 `release-publish`、`tauri-packaging` 重叠且涉及外部写入               | 标记显式 `/release` 编排；只负责完整发布流程顺序；具体打包用 `tauri-packaging`，Updater 用 `tauri-updater`，远程发布用 `release-publish`                                                   |
| `release-publish/SKILL.md`           | 517 行且包含推送、签名、update.json 高风险操作                           | Frontmatter 设为显式终端动作；入口保留审批、版本、签名、凭据、远端验证和回滚；平台命令与产物清单拆 references；禁止仅凭“发布”二字自动外部写入                                              |
| `rust-fundamentals/SKILL.md`         | “Rust、编译错误”范围较宽但仍是必要基础                                   | 只在所有权、借用、生命周期、trait、async、Send/Sync 或 Rust 编译语义问题时激活；普通 Rust 业务修改由具体领域 Skill 处理                                                                    |
| `security-permissions/SKILL.md`      | 与 `tauri-capabilities` 重叠                                             | 定位为威胁边界、凭据、CSP、最小权限和安全审查；不承载 Capabilities JSON 细节；任何凭据/远程/外部输入任务高召回                                                                             |
| `start/SKILL.md`                     | “开始、了解项目”过宽                                                     | 标记显式 `/start` 或新会话项目概览；普通任务开头不触发                                                                                                                                     |
| `store-management/SKILL.md`          | “state、状态、持久化”会命中业务状态和数据库任务                          | 只处理 Zustand、React 全局状态、Rust AppState 和状态持久化设计；业务状态字段查询不触发                                                                                                     |
| `sync/SKILL.md`                      | “同步、数据一致性”可能误命中业务数据同步                                 | 标记显式 `/sync` 文档/框架同步工作流；业务同步任务不得命中                                                                                                                                 |
| `task-tracker/SKILL.md`              | 632 行且“多步骤开发、方案讨论、遇到问题”会让大量任务自动创建文档         | 仅在用户明确要求跟踪、恢复、归档任务，或明确长期多会话工作时激活；普通多步骤任务使用运行时计划，不自动生成任务文档；模板拆 references                                                      |
| `tauri-capabilities/SKILL.md`        | 与 `security-permissions` 重叠                                           | 只处理 Tauri 2 `capabilities/*.json`、permission、scope 和窗口权限差异；安全架构交给 `security-permissions`                                                                                |
| `tauri-commands/SKILL.md`            | 655 行，与 `api-development` 重叠                                        | 定位为 Rust Command 高级实现：State/AppHandle/async/stream/事件；端到端契约由 `api-development`；长代码模式拆 references，关键注册和错误规则保留入口                                       |
| `tauri-events/SKILL.md`              | “通知、推送、实时更新”与系统通知混淆                                     | 只处理 Tauri `emit/listen/Emitter/EventTarget` 事件通信；原生通知交给 `notification-system`，普通 React 状态更新不触发                                                                     |
| `tauri-packaging/SKILL.md`           | “构建、build、发布”过宽                                                  | 只处理安装包、bundle、签名和跨平台产物；普通 `pnpm build`/`cargo check` 不触发；远端发布交给 `release-publish`                                                                             |
| `tauri-plugins/SKILL.md`             | “集成、扩展、第三方”过宽                                                 | 只有引入、配置或开发 `tauri-plugin-*` 时激活；普通 npm/crate 依赖不自动触发；权限联动条件化加载 `tauri-capabilities`                                                                       |
| `tauri-updater/SKILL.md`             | “更新、升级、update”会误命中普通代码更新                                 | 只处理应用自动更新、Updater 插件、签名和 update manifest；普通更新文件或依赖不触发                                                                                                         |
| `tauri-window-management/SKILL.md`   | 当前边界较清楚，但“窗口”可能指浏览器窗口                                 | 强触发限定为 Tauri Window/WebviewWindow/tray/titlebar；浏览器测试窗口不触发                                                                                                                |
| `tech-decision/SKILL.md`             | 与 brainstorm/architecture 重叠                                          | 只在需要对比方案并形成可追踪决策时触发；普通想法探索用 brainstorm，已决定的实现不加载；ADR 模板下沉                                                                                        |
| `test-development/SKILL.md`          | “测试/test”可能在所有任务交付阶段触发并重复加载                          | 只在新增/修改测试、设计测试策略或修复测试失败时激活；普通实现的必跑测试由 L0 基线和验证矩阵保证，不要求每次加载完整测试 Skill                                                              |
| `theme-system/SKILL.md`              | Frontmatter 已出现乱码，且与 UI 样式重叠                                 | 首先修复 UTF-8 中文；只处理主题、暗亮模式、设计令牌、CSS Variables 和 antdTheme；普通页面样式由 `ui-frontend`                                                                              |
| `ui-frontend/SKILL.md`               | 444 行，触发词“页面、样式、React”较宽，但属于高频核心                    | 保留前端页面/组件强触发；长 Table/Form/Modal 示例拆 references；入口保留可访问性、状态、错误提示、API 边界和浏览器强制验收                                                                 |
| `update-docs/SKILL.md`               | 与 `docs-management` 重叠                                                | 标记显式 `/update-docs` 工作流；内部方案文档不触发；正文只编排 docs-management，不复制完整 VitePress 指南                                                                                  |
| `update-status/SKILL.md`             | “更新状态”易与业务状态修改混淆                                           | 标记显式 `/update-status` 或明确项目管理状态更新；业务字段状态不触发                                                                                                                       |
| `utils-toolkit/SKILL.md`             | “工具、通用、文件处理”过宽                                               | 只在设计可复用工具函数、日期/字符串/路径公共库时激活；单次局部辅助函数不自动触发；文件系统功能交给 `file-storage`                                                                          |

### 8.5 新增 References 逐文件实施台账

实际在 45 个 Codex Skill 目录新增 73 个 Reference。每个文件只承载一种详细信息，入口明确说明何时读取；另外 5 个短 Skill（`autonomous-dev`、`exp`、`next`、`start`、`theme-system`）不需要 Reference。

| 新文件                                                                         | 为什么新增                             | 承载内容                                                       |
| ------------------------------------------------------------------------------ | -------------------------------------- | -------------------------------------------------------------- |
| `.codex/skills/add-skill/references/create-update-delete.md`                   | 创建、维护、重命名和删除流程较长且低频 | 各操作步骤、影响面和安全边界                                   |
| `.codex/skills/add-skill/references/frontmatter-and-routing.md`                | 路由元数据规范不应常驻入口             | Frontmatter、Manifest、强弱信号、排除、互斥和风险规则          |
| `.codex/skills/add-skill/references/mirror-sync.md`                            | 三端同步与例外处理易出错               | 规范源、镜像生成、Claude command、upstream/platform-local 规则 |
| `.codex/skills/add-skill/references/validation.md`                             | 验证清单细节多                         | 编码、结构、路由、镜像和激活测试                               |
| `.codex/skills/api-development/references/ipc-contract.md`                     | IPC 类型契约示例较长                   | Rust/TypeScript 参数、返回、错误和命名转换                     |
| `.codex/skills/api-development/references/registration-checklist.md`           | 跨层注册点多，入口只保留原则           | models/mod/lib/generate_handler/types/api/call-site 全链路检查 |
| `.codex/skills/architecture-design/references/architecture-patterns.md`        | 架构案例仅在设计时需要                 | 双进程边界、三层分层、模块拆分和依赖方向                       |
| `.codex/skills/brainstorm/references/option-evaluation.md`                     | 候选比较模板不应随每次触发加载         | 方案生成、约束、取舍、风险和收敛方法                           |
| `.codex/skills/bug-detective/references/known-failure-patterns.md`             | 已知故障模式是诊断时的按需知识         | Tauri、IPC、数据库、权限和构建常见根因                         |
| `.codex/skills/bug-detective/references/symptom-playbook.md`                   | 按症状排查步骤较长                     | 复现、日志、调用链、数据、部署和页面证据顺序                   |
| `.codex/skills/check/references/frontend-checks.md`                            | 前端命令与验收步骤细节多               | formatter、tsc、vitest、build 和浏览器验收                     |
| `.codex/skills/check/references/rust-checks.md`                                | Rust 检查组合按改动而异                | fmt、check、test、clippy 和 Cargo 检查                         |
| `.codex/skills/check/references/tauri-config-checks.md`                        | Tauri 配置检查是条件分支               | tauri.conf、Capabilities、插件注册和 JSON 验证                 |
| `.codex/skills/code-patterns/references/react-patterns.md`                     | React 完整模式会膨胀入口               | 组件、Hook、类型、状态和 API 边界模式                          |
| `.codex/skills/code-patterns/references/rust-patterns.md`                      | Rust 完整模式会膨胀入口                | 所有权、错误、异步、分层和可维护性模式                         |
| `.codex/skills/collaborating-with-codex/references/bridge-usage.md`            | 外部桥接参数只在用户显式委托时需要     | Codex CLI 只读调用、参数、Session、Diff 复核和故障处理         |
| `.codex/skills/collaborating-with-gemini/references/bridge-usage.md`           | 普通 UI 任务不应加载外部模型说明       | Gemini CLI 显式 `--sandbox`、文件引用、Session 和输出复核      |
| `.codex/skills/command/references/command-templates.md`                        | Command 脚手架模板体积大               | Rust/TypeScript 生成模板和占位符                               |
| `.codex/skills/command/references/scaffolding-inputs.md`                       | 输入采集仅在显式脚手架时使用           | 模块名、参数、返回、同步/异步和文件范围校验                    |
| `.codex/skills/command/references/verification.md`                             | 脚手架验证点跨多层                     | 注册、类型、API、测试、构建和差异检查                          |
| `.codex/skills/database-ops/references/dao-and-transactions.md`                | DAO 与事务示例较长                     | rusqlite 查询、事务、锁、映射和错误传播                        |
| `.codex/skills/database-ops/references/database-verification.md`               | 真实数据核验是条件性高风险流程         | 配置读取、Tauri SSH MCP、DDL/数据格式和迁移验证                |
| `.codex/skills/database-ops/references/schema-migrations.md`                   | 迁移细节不应每次加载                   | PRAGMA user_version、升级、初始化、兼容和回滚                  |
| `.codex/skills/dev/references/fullstack-plan.md`                               | 全栈编排只在显式 `/dev` 使用           | React → API → Command → Service → Database 文件计划            |
| `.codex/skills/dev/references/generation-rules.md`                             | 生成规则和模板细节较长                 | 各层代码生成、注释、编码和冲突保护                             |
| `.codex/skills/doc-generation/references/developer-doc-templates.md`           | 内部开发者文档模板不应常驻入口         | Command、模块、数据库和 IPC Markdown 模板                      |
| `.codex/skills/doc-generation/references/source-scanning.md`                   | 代码证据扫描按文档类型变化             | Command 注册、调用链、Schema、事件和证据等级                   |
| `.codex/skills/docs-management/references/incremental-sync.md`                 | VitePress 增量同步流程长且低频         | `.docs-meta.json`、路径映射、生成标记和全量重建保护            |
| `.codex/skills/docs-management/references/site-content-and-verification.md`    | 站点写作和验收只在站点任务需要         | 内容证据、VitePress 构建和真实浏览器验收                       |
| `.codex/skills/docs-management/references/site-initialization.md`              | 首次建站包含大量交互与模板步骤         | 目标位置、占位符、初始化、Git 边界和验收                       |
| `.codex/skills/error-handler/references/error-propagation-patterns.md`         | Rust/IPC/React 错误示例较长            | AppError、Command 转换、safeInvoke 和 ErrorBoundary            |
| `.codex/skills/exp-sediment/references/audit-and-templates.md`                 | 经验审计与模板低频且内容长             | 健康审计、候选检查和输出模板                                   |
| `.codex/skills/exp-sediment/references/candidate-catalog.md`                   | 经验资产分类不应常驻入口               | Skill、PROJECT、Memory、docs 等候选类型与边界                  |
| `.codex/skills/exp-sediment/references/execution-and-storage.md`               | 写入路径和授权规则复杂                 | 显式授权、存储位置、更新流程和验证                             |
| `.codex/skills/file-storage/references/filesystem-patterns.md`                 | 文件 API 代码模式按场景加载            | 目录、导入导出、拖放、dialog、路径和权限                       |
| `.codex/skills/git-workflow/references/git-operations.md`                      | Git 命令细节长且有破坏风险             | 分支、逐文件暂存、提交、合并、推拉和并发避让                   |
| `.codex/skills/i18n-development/references/react-i18next-patterns.md`          | i18n 完整示例低频                      | 资源组织、切换、插值、复数和测试                               |
| `.codex/skills/json-serialization/references/serde-patterns.md`                | serde 属性组合较多                     | rename、enum、Option、日期和自定义序列化                       |
| `.codex/skills/json-serialization/references/type-mapping.md`                  | Rust/JSON/TS 映射表较长                | 类型、可空性、camelCase 和兼容性映射                           |
| `.codex/skills/notification-system/references/native-notification-patterns.md` | 原生通知实现示例仅特定任务需要         | 插件注册、权限、前端调用和平台差异                             |
| `.codex/skills/performance-doctor/references/profiling-and-tuning.md`          | Profiling 工具与调优步骤较长           | 测量、火焰图、内存、启动、体积和编译性能                       |
| `.codex/skills/progress/references/report-template.md`                         | 进度报告是显式低频工作流               | 项目进度、模块、验证、风险和下一步模板                         |
| `.codex/skills/project-init/references/file-generation.md`                     | 新项目文件生成步骤体积大               | 导出、复制、替换、编码、冲突和清理边界                         |
| `.codex/skills/project-init/references/git-and-signing.md`                     | Git、远端和签名是高风险步骤            | 仓库可见性、提交、推送、签名密钥和回滚                         |
| `.codex/skills/project-init/references/project-inputs.md`                      | 初始化信息采集字段多                   | 名称、标识符、包名、端口、发布与确认清单                       |
| `.codex/skills/project-init/references/template-update.md`                     | 模板检测与更新只在初始化时需要         | 版本、fetch、差异、只读模板和失败处理                          |
| `.codex/skills/project-navigator/references/repository-map.md`                 | 完整仓库地图不应每次加载               | React、IPC、Rust、数据库、配置和相似实现入口                   |
| `.codex/skills/release-publish/references/platform-publish.md`                 | 多平台发布命令和产物表较长             | Gitee/GitHub/R2、产物、URL 和远端验证                          |
| `.codex/skills/release-publish/references/release-gates.md`                    | 高风险门禁必须完整但可按发布场景加载   | 版本、签名、凭据、审批、CI、回滚和证据                         |
| `.codex/skills/release/references/release-orchestration.md`                    | `/release` 只负责编排，详细顺序下沉    | 版本、打包、Updater、发布和验证阶段                            |
| `.codex/skills/rust-fundamentals/references/rust-semantics-examples.md`        | Rust 语义案例较长                      | 所有权、借用、生命周期、trait、async 和 Send/Sync              |
| `.codex/skills/security-permissions/references/security-review.md`             | 安全审查清单详细且条件化               | 凭据、输入、CSP、最小权限、外部访问和审计                      |
| `.codex/skills/store-management/references/persistence-and-theme.md`           | 持久化和主题联动不是每次状态任务都需要 | store 持久化、迁移、主题和恢复                                 |
| `.codex/skills/store-management/references/zustand-and-app-state.md`           | 前后端状态完整模式较长                 | Zustand slice、React 状态和 Rust AppState 边界                 |
| `.codex/skills/sync/references/project-doc-sync.md`                            | `/sync` 文档/框架同步流程低频          | 源目标、差异、保护规则和验证                                   |
| `.codex/skills/task-tracker/references/task-templates.md`                      | 持久化任务模板体积大                   | active/archive 结构、字段、恢复和状态迁移                      |
| `.codex/skills/tauri-capabilities/references/permission-and-scope.md`          | permission/scope 示例较长              | Tauri 2 权限、窗口作用域和最小授权                             |
| `.codex/skills/tauri-commands/references/complete-examples.md`                 | 完整 Command 示例会显著膨胀入口        | 分层、注册、API、类型和测试的端到端示例                        |
| `.codex/skills/tauri-commands/references/injection-and-async.md`               | 高级注入与异步仅特定 Command 需要      | State、AppHandle、Window、async 和阻塞处理                     |
| `.codex/skills/tauri-commands/references/progress-and-streaming.md`            | 进度/流式通信是条件分支                | emit/listen、Channel、stream、取消和清理                       |
| `.codex/skills/tauri-events/references/event-patterns.md`                      | 事件完整示例较长                       | emit/listen、EventTarget、Payload、取消监听和测试              |
| `.codex/skills/tauri-packaging/references/platform-bundles.md`                 | 平台产物差异只在打包时需要             | MSI/NSIS、DMG、AppImage、签名和产物检查                        |
| `.codex/skills/tauri-plugins/references/plugin-integration.md`                 | 插件集成跨 Cargo、Builder、前端和权限  | 依赖、注册、Capabilities、API 和升级检查                       |
| `.codex/skills/tauri-updater/references/signing-and-manifest.md`               | Updater 签名与 Manifest 属于高风险细节 | 公私钥边界、update.json、URL、版本和验签                       |
| `.codex/skills/tauri-window-management/references/window-patterns.md`          | 窗口/托盘平台模式较长                  | Window、WebviewWindow、标题栏、托盘和生命周期                  |
| `.codex/skills/tech-decision/references/adr-template.md`                       | ADR 模板仅在形成决策记录时需要         | 背景、候选、权衡、决策、后果和复审                             |
| `.codex/skills/test-development/references/test-patterns.md`                   | Rust/React/IPC 测试模式内容长          | 单元、集成、fixture、mock、回归和覆盖策略                      |
| `.codex/skills/ui-frontend/references/browser-acceptance.md`                   | 页面验收步骤必须完整但只在 UI 任务加载 | 内置浏览器/Chrome、控制台、交互和截图证据                      |
| `.codex/skills/ui-frontend/references/settings-and-layout.md`                  | 设置页与布局模式是特定 UI 场景         | AppLayout、Sidebar、设置表单和响应式布局                       |
| `.codex/skills/ui-frontend/references/table-form-modal.md`                     | Ant Design 完整组件示例较长            | Table、Form、Modal、状态、错误和可访问性                       |
| `.codex/skills/update-docs/references/site-workflows.md`                       | `/update-docs` 仅编排站点工作流        | 初始化、增量、全量和 docs-management 委托                      |
| `.codex/skills/update-status/references/status-update-workflow.md`             | 项目状态更新是显式低频流程             | 状态来源、字段、更新、验证和冲突处理                           |
| `.codex/skills/utils-toolkit/references/reusable-utilities.md`                 | 公共工具示例不应由“工具”一词常驻加载   | 日期、字符串、路径、错误和跨模块复用准则                       |

### 8.6 镜像目录实施结果

| 目录/文件组                                     | 实际结果                                                                                          | 为什么这样处理                                           |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `.claude/skills/<37 个 project Skill>/`         | 已从 Codex 规范源生成 37 个入口和 58 个 References，`sync --check` 通过                           | 消除项目共享 Skill 的第二份人工真相                      |
| `.agents/skills/<37 个 project Skill>/`         | 已生成同一组 37 个入口和 58 个 References，`sync --check` 通过                                    | 只同步 Manifest 明确支持 Agents 的项目 Skill             |
| `.claude/commands/collaborating-with-codex.md`  | 已由对应 Skill 去 Frontmatter 生成，并重写 Reference 相对链接                                     | 显式外部协作需要 Claude command 入口，避免复制正文       |
| `.claude/commands/collaborating-with-gemini.md` | 同上                                                                                              | 保证 Gemini 协作只由显式 command 激活                    |
| `.claude/commands/project-init.md`              | 同上                                                                                              | 让低频高风险初始化保持显式工作流                         |
| `.claude/commands/task-tracker.md`              | 同上                                                                                              | 普通多步骤任务不自动创建持久化任务文档                   |
| 19 个已废弃阶段资源                             | 已按授权逐文件删除 7 个 Codex 入口、6 个 Claude 镜像和 6 个 Claude 命令                           | 消除专用阶段推断、互斥和重复上下文                       |
| 4 个 platform-local Skill                       | `env-isolation`、`mobile-app-architecture`、`remote-gateway`、`tauri-mobile-android` 只报告和保留 | 平台专属语义不能由全局字符串替换生成                     |
| `.agents/skills/env-isolation/SKILL.md`         | 单独把不存在的 `~/.Codex` 修正为 `~/.claude`，当前与 Claude 源哈希一致                            | 修复旧机械替换留下的无效路径，不改变隔离业务规则         |
| 3 个小写 `skill.md`                             | 继续作为 platform-local 兼容入口保留，校验器支持大小写                                            | 尚无跨运行时兼容证据支持强制重命名，避免破坏平台本地发现 |

Manifest 管理范围内共享资源当前零漂移；这不表示三端完整目录必须完全相同。未列入 Manifest 的 Skill 目录和生成命令，以及 upstream、platform-local 内容，始终不会被同步器覆盖或删除。`--write --prune` 只能清理当前仍列管的精确 Skill 目录内的陈旧资源，并经过符号链接、路径越界和显式授权测试；整项旧镜像只能在用户明确授权后逐个精确人工处理。同步写模式没有跨进程锁，同一仓库只能由一个 writer 串行执行；这是最终复审保留的 LOW 运行约束。

37 个共享 project Skill 为：`add-skill`、`api-development`、`architecture-design`、`brainstorm`、`bug-detective`、`code-patterns`、`collaborating-with-codex`、`collaborating-with-gemini`、`database-ops`、`docs-management`、`error-handler`、`exp-sediment`、`file-storage`、`git-workflow`、`i18n-development`、`json-serialization`、`notification-system`、`performance-doctor`、`project-init`、`project-navigator`、`release-publish`、`rust-fundamentals`、`security-permissions`、`store-management`、`task-tracker`、`tauri-capabilities`、`tauri-commands`、`tauri-events`、`tauri-packaging`、`tauri-plugins`、`tauri-updater`、`tauri-window-management`、`tech-decision`、`test-development`、`theme-system`、`ui-frontend`、`utils-toolkit`。每个镜像入口及其 Reference 与第 8.4、8.5 节的 Codex 规范源对应，修改原因和内容不再重复写第二份。

## 9. 准确性保护设计

### 9.1 路由准确性不是代码准确性的唯一来源

路由输出只能说明“建议读取哪些 Skill”，不能宣称任务已满足约束。最终准确性由以下证据闭环保证：

```text
任务字段/需求
  -> 当前代码与参考实现
  -> 前后端/IPC/数据库/权限调用链
  -> 实际修改
  -> 格式与静态检查
  -> 单元/集成测试
  -> 构建
  -> 真实数据库或真实页面验证
  -> 独立审查
```

### 9.2 按变更文件触发验证，而不是按已加载 Skill 触发验证

这条规则非常关键。即使路由器漏选了某个测试 Skill，只要实际改动了相应文件，验证矩阵仍必须要求执行检查。

| 实际变更                            | 必须验证                                                                       |
| ----------------------------------- | ------------------------------------------------------------------------------ |
| `src-tauri/src/**/*.rs`             | `cargo fmt --check`、聚焦 `cargo test`、`cargo check`，高风险时 `cargo clippy` |
| `src/**/*.ts(x)`                    | 项目格式化、`tsc --noEmit`、聚焦 Vitest、`pnpm build`                          |
| `src/pages/**`、组件、样式          | 上述前端验证 + Codex 内置浏览器或 Control Chrome                               |
| `src-tauri/src/database/**`、schema | 迁移测试、旧版本升级、新库初始化、真实数据格式确认                             |
| `src-tauri/capabilities/**`         | JSON 校验、权限最小化、真实运行时能力验证                                      |
| `Cargo.toml` / lock                 | `cargo check`、相关测试、依赖安全和跨平台影响                                  |
| `package.json` / lock               | 安装一致性、类型检查、测试、构建                                               |
| Updater / release 文件              | 版本一致性、签名、产物、update manifest、CI 和回滚检查                         |
| 所有文本修改                        | UTF-8 无 BOM、无乱码、`git diff --check`                                       |

实施状态：9 条路径规则已写入 `.codex/tests/skill-routing/expected-matrix.json`，`check` Skill 会按实际变更文件读取并编排检查，`validate-skills` 会校验矩阵结构。当前尚未提供一个独立脚本自动读取 `git diff` 并输出命中规则，因此“矩阵已结构化”和“验证动作已自动执行”必须区分；后者仍属于运行时执行责任。

### 9.3 高风险保守路由

以下信号不得依赖弱关键词评分，命中后直接加入安全检查：

- SSH 密钥、密码、Token、凭据、Safe Credentials。
- 数据库 DDL、迁移、删除、批量更新、生产数据。
- Git push、tag、release、签名、远程发布。
- 服务器访问、Jenkins、MCP 外部写入。
- 文件删除、覆盖、清理、跨目录移动。
- Capabilities、CSP、scope、外部 URL、命令执行。

### 9.4 路由失败降级

路由脚本出现 JSON 错误、清单缺失、超时或异常时：

1. Hook 不阻断用户任务。
2. 回退到当前 Codex 极简流程，让模型基于已加载 Frontmatter 评估。
3. 输出一条短警告，不输出堆栈和本地敏感路径。
4. 后续 `skill:test` 必须暴露失败，不能长期静默。

Codex 与 Claude Hook 均已实现 `active`、`shadow`、`fallback` 三种模式。广域高风险真实 Hook 子进程实测：Codex active/shadow/fallback 为 1,317/381/381 字节，Claude 为 1,994/385/385 字节；结构化恢复态和斜杠命令均为 0 字节。包含普通 `context window` 文字的合法任务不会再被误判为恢复态，能产生非零输出并正确路由。异常降级不阻断任务，并保持在对应 Hook 的 1.5/2 KiB 上限内。

## 10. 路由回归测试集

### 10.1 用例分类

当前已落地 225 个真实风格用例：

| 类别/ID 前缀                  | 实际数量 | 重点                                                                                       |
| ----------------------------- | -------: | ------------------------------------------------------------------------------------------ |
| 普通问答 `plain`              |        8 | 简单回答不加载无关工作流                                                                   |
| Rust Command/IPC `ipc`        |        8 | 区分 api-development、tauri-commands、command                                              |
| React UI `ui`                 |        8 | UI、主题、状态、通知边界                                                                   |
| SQLite/数据 `db`              |        8 | 本地数据库与普通业务“数据”区分                                                             |
| Bug 诊断 `bug`                |        9 | 诊断、错误传播、Rust 编译问题和 Skill 未触发区分                                           |
| 安全/权限/凭据 `sec`          |        8 | 高风险安全门禁                                                                             |
| 文件/窗口/事件/插件 `feature` |        8 | 相邻领域冲突消解                                                                           |
| 测试/构建/发布/更新 `release` |       10 | 构建、打包、发布和 Updater 区分                                                            |
| 文档/任务/经验 `docs`         |       10 | 显式工作流不被普通措辞触发                                                                 |
| 代码规范组合 `standards`      |       17 | IPC、React、Rust、SQLite、文件权限、插件、Updater、Zustand、Skill 维护、跨子任务和受控否定 |
| 工作流与边界 `workflow`       |       16 | 显式命令、自然语言和相邻工作流边界                                                         |
| 一般边界 `boundary`           |        4 | 内部方案、安全与空候选降级                                                                 |
| 风险标签与同义语义 `risk`     |       16 | 终端动作、安全补充、生产数据库、远端 Git、凭据、制品库、下载站和外部上线的多种自然语序     |
| 精度对抗 `precision`          |       95 | 精确 Skill/风险集合、模型 Token、否定作用域、只读 Updater、凭据和破坏性操作边界            |

### 10.2 代表性断言

| Prompt                                         | 必选                                          | 禁选                                      |
| ---------------------------------------------- | --------------------------------------------- | ----------------------------------------- |
| “给设置页新增 SSH 连接表单并调用 Rust Command” | `ui-frontend`, `api-development`              | `command`（除非显式请求脚手架）           |
| “Command 里注入 AppHandle 并实时回报进度”      | `tauri-commands`, `tauri-events`              | `notification-system`                     |
| “为什么这个请求报错，先定位不要修改”           | `bug-detective`                               | `error-handler`, `dev`                    |
| “新增 SQLite v35 迁移并验证旧库升级”           | `database-ops`, `test-development`            | `store-management`                        |
| “页面显示一条 antd message”                    | `ui-frontend`                                 | `notification-system`                     |
| “发送 macOS 原生系统通知”                      | `notification-system`                         | `tauri-events`                            |
| “修改 capabilities/default.json 的 fs scope”   | `tauri-capabilities`, `security-permissions`  | `file-storage` 仅在实现文件功能时条件加入 |
| “pnpm build 报 TypeScript 类型错误”            | `bug-detective` 或专用构建 resolver           | `tauri-packaging`                         |
| “生成 dmg 并签名”                              | `tauri-packaging`                             | `release-publish`，除非要求发布           |
| “发布 1.2.0 并推送 update.json”                | `release`, `release-publish`, `tauri-updater` | 无；属于高风险完整组合                    |
| “更新一个普通函数”                             | 对应领域 Skill                                | `tauri-updater`, `update-status`          |
| “Rust borrow 错误并补充回归测试”               | `rust-fundamentals`, `test-development`       | 无；语言与测试规范需要组合                |
| “写一份内部优化方案到 docs”                    | `brainstorm` 或 `tech-decision`               | `docs-management`, `update-docs`          |
| “/update-docs status”                          | `update-docs`                                 | `doc-generation`                          |

### 10.3 准确性验收门槛

- 所有 `required` Skill 命中率必须为 100%。
- 高风险类别不得漏选安全门禁。
- 所有 `forbidden` 误触发率必须为 0%。
- 17 个代码规范组合用例必须命中对应领域与安全 Skill。
- 普通单域任务候选集 P95 不超过 3 个；跨层任务不设硬性准确性上限。
- Hook 必须先通过理由压缩在预算内保留完整候选；不得用固定数量硬裁剪高风险或跨域 Skill。完整调试信息写到测试输出，不注入上下文。

静态结果：`node --test .codex/tests/skill-routing/router.test.cjs` 共 258 项，258 项通过。除 225 个逐 Prompt 断言外，还覆盖 Manifest 完整性、平台与 platform-local 隔离、显式工作流隔离、仅结构化恢复/压缩跳过、无结构化字段时不从正文猜测恢复态、fallback、真实 Hook 完整候选预算、带参数斜杠命令安全补充、普通单域 P95、完整资源树同步、Claude command Reference 重写、platform-local/unmanaged/unlisted 永久保留、不支持生命周期状态拒绝、重命名只创建新镜像并保留旧名、当前受管目录陈旧资源显式 prune、原子写失败清理、严格 schema、非规范 source、planned 平台约束、符号链接和路径越界防护。

## 11. Token 与执行效率指标

### 11.1 优化前基线与实施后实测

当前可直接量化的是字节、行数、选中数量和执行时间。中文 Token 与模型分词有关，不能直接把字节数冒充精确 Token。

优化前基线：

- Codex Skill 正文总量：550,965 字节。
- Codex Frontmatter：26,796 字节。
- 13 个入口超过 400 行。
- 三目录只有 21 个同名 Skill 完全一致。
- Claude Hook 仍注入完整硬编码清单。

实施后实测：

- 路由样例：225。
- 每个样例选中 Skill 数 P50/P95：1 / 4。
- 选中入口 UTF-8 字节 P50/P95：4,662 / 13,280。
- 路由延迟 P95：本轮复测为 1.321 ms，低于 50 ms 目标。
- Codex 高风险 active/shadow/fallback Hook 输出：1,317 / 381 / 381 字节。
- Claude 高风险 active/shadow/fallback Hook 输出：1,994 / 385 / 385 字节。
- 结构化恢复态与斜杠命令重复输出：0 字节；普通 `context window` 任务输出非零并正确路由。
- 50 个 project Skill 入口全部不超过 117 行；Codex 全部入口 P50/P95 为 66/112 行。

高收益入口行数变化：

| Skill            | 优化前 | 实施后 |  降幅 |
| ---------------- | -----: | -----: | ----: |
| `project-init`   |  1,491 |    106 | 92.9% |
| `add-skill`      |  1,085 |    109 | 90.0% |
| `check`          |    783 |    101 | 87.1% |
| `tauri-commands` |    655 |     82 | 87.5% |
| `task-tracker`   |    632 |    117 | 81.5% |
| `code-patterns`  |    626 |     99 | 84.2% |
| `command`        |    605 |     81 | 86.6% |
| `exp-sediment`   |    615 |     58 | 90.6% |

### 11.2 目标值

| 指标                             |                          目标 | 实施结果                                  |
| -------------------------------- | ----------------------------: | ----------------------------------------- |
| Codex Hook 常规输出              |                      ≤ 1.5 KB | 广域高风险 active 实测 1,317 B，PASS      |
| Claude Hook 常规输出             |                        ≤ 2 KB | 广域高风险 active 实测 1,994 B，PASS      |
| 高频 Skill 入口                  | 80～180 行，复杂入口 ≤ 220 行 | project 入口 P50 66 行、最大 117 行，PASS |
| 普通单域任务 Skill 数            |         通常 1～2 个，P95 ≤ 3 | 路由测试断言 PASS                         |
| 跨层实现任务 Skill 数            |                  通常 2～4 个 | 全样例 P95 为 4                           |
| 恢复/压缩 Prompt 重复注入        |                             0 | 实测 0 B，PASS                            |
| Manifest 管理范围共享 Skill 漂移 |                             0 | `skill:sync:check` PASS                   |
| 路由测试 required 命中           |                          100% | 258/258 测试 PASS                         |
| 路由脚本 P95 耗时                |                       < 50 ms | 本轮复测 1.321 ms，PASS                   |

### 11.3 已观察收益与 Token 口径

已观察到的上下文收益来自：

1. 普通任务不再同时加载多个重叠 Skill。
2. 长 Skill 只读取入口和本次需要的 reference。
3. Claude Hook 不再重复注入完整清单。
4. 已废弃阶段资源不再进入候选或默认上下文。
5. 显式命令不再被普通自然语言误触发。

入口字节和选中入口字节已实测，但真实 Token 降幅仍必须在灰度期间读取运行时 usage 后计算。当前没有可归因的运行时 Token usage，因此不能把 65.5% 的入口字节下降直接宣称为同等 Token 降幅。

## 12. 实施阶段与顺序

| 阶段           | 状态           | 当前证据 / 未完成项                                                                                                                  |
| -------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 0 基线审计     | 已完成         | 优化前清单、字节、行数和并发工作区已记录                                                                                             |
| 1 路由与测试   | 已完成         | Manifest、7 bundles、225 cases、258 项测试通过                                                                                       |
| 2 Hook 灰度    | 部分完成       | 两端 Hook 和三模式已实现并做确定性样例测试；尚无至少 20 个真实任务观察记录                                                           |
| 3 第一批 Skill | 已完成静态实施 | 高收益入口和 References 已拆分；实际采用并行批量实施，没有逐个真实任务灰度                                                           |
| 4 剩余 Skill   | 已完成静态实施 | 50 个 project 入口完成边界收敛，安全/数据库/发布禁令保留                                                                             |
| 5 镜像与维护   | 已完成         | 单一规范源、同步器、37 个共享镜像及 `add-skill` 新流程落地；非破坏 `--write` 更新 4 个 Reference 镜像后 `--check` PASS，未执行 prune |
| 6 全量验收     | 部分完成       | 静态门禁全部通过；代表性代码任务、数据库、发布和浏览器运行时验收待补                                                                 |

### 阶段 0：冻结基线，只读审计（已完成）

1. 保存当前三目录清单、哈希、字节数和入口行数。
2. 运行现有代表性任务，记录选中 Skill、准备耗时和验证结果。
3. 确认当前其他会话未提交改动范围。
4. 不修改业务代码、Cargo、前端依赖或数据库。

出口条件：基线数据和回归 Prompt 集完成。

### 阶段 1：先建立测试和影子路由（已完成）

1. 新增 Manifest、bundle、router 和测试文件。
2. 新路由只打印到本地测试结果，不改变 Hook 实际输出。
3. 将现有任务样例与新路由比较，人工审查漏选和误选。
4. 高风险用例全部通过后才能进入下一阶段。

出口条件：required 100%，forbidden 0%，高风险零漏选。

### 阶段 2：切换 Hook，但不拆 Skill（部分完成）

1. Codex Hook 使用新路由候选集。
2. Claude Hook 删除全量硬编码清单。
3. 保留旧极简逻辑作为 `SKILL_ROUTER_MODE=fallback` 回退。
4. 观察至少 20 个真实任务。

出口条件：真实任务无准确性回归，Hook 输出和耗时达标。

当前 Hook 输出、模式和耗时已达标，但缺少“至少 20 个真实任务”的独立观察记录，因此本阶段不能标记为完整运行时验收。

### 阶段 3：拆分第一批高收益 Skill（静态实施完成）

优先处理 `project-init`、`add-skill`、`check`、`tauri-commands`、`task-tracker`、`code-patterns`、`command`、`ui-frontend`。

每次只改 1～2 个 Skill：

1. 拆分前运行路由和内容检查。
2. 移动内容到 references，不删除规则。
3. 运行 Skill 自身正反向用例。
4. 运行对应代码任务样例。
5. 通过后再处理下一个。

实际实施使用多代理按互不冲突的 Skill 目录并行拆分，没有严格按“每次只改 1～2 个”串行灰度。为降低批量改动风险，补充了 225 个路由样例、完整引用/编码/镜像校验和独立审计；这仍不能替代真实代码任务回归，偏差必须保留在台账中。

### 阶段 4：处理安全、数据库、发布和剩余 Skill（静态实施完成）

先处理高风险 Skill，再处理低频内容。高风险入口允许比普通 Skill 更长，关键禁令不下沉。

### 阶段 5：统一镜像和维护流程（已完成）

1. `sync-skills.cjs --check` 报告现有漂移。
2. 人工确认每个不同版本应该保留的规则。
3. 设置 `.codex/skills` 为项目 Skill 规范源，upstream Skill 例外写入 Manifest。
4. 生成 Claude/Agents 镜像。
5. 修改 `add-skill`，以后禁止手工三份复制。

### 阶段 6：全量验收（部分完成）

1. 路由回归全部通过。
2. 所有 Skill UTF-8、Frontmatter、引用、镜像检查通过。
3. 对 Rust、React、SQLite、Capabilities、更新、发布各执行至少一个代表性任务。
4. 页面任务使用内置浏览器或 Chrome 验证。
5. `git diff --check` 通过。
6. 独立代码审查确认没有规则丢失。

已完成第 1、2、5、6 项中的静态部分，并完成独立文档/实现审计；第 3、4 项及第 5 项涉及业务代码格式化/构建的部分尚无本轮证据。后续需分别执行 Rust、React、SQLite、Capabilities、Updater、正式发布只读演练，并对页面任务使用内置浏览器或 Control Chrome。

## 13. 灰度、开关与回滚

### 13.1 模式开关

路由器支持三种模式：

| 模式       | 行为                              | 用途         |
| ---------- | --------------------------------- | ------------ |
| `shadow`   | 计算新候选但仍输出旧极简流程      | 第一阶段对照 |
| `active`   | 输出新候选和最小完整集            | 正式灰度     |
| `fallback` | 不读取 Manifest，恢复现有极简流程 | 快速回滚     |

当前默认模式为 `active`，可通过 `SKILL_ROUTER_MODE` 临时切换为 `shadow` 或 `fallback`，非法值回落到 `active`。模式不依赖用户全局配置。三种模式、仅由宿主结构化字段确认的恢复/压缩跳过，以及斜杠命令跳过均已有确定性测试；缺少结构化字段时不会从用户正文猜测恢复态。尚未做一次人为制造生产 Hook 故障的完整回滚演练。

### 13.2 回滚边界

若出现漏选、实现偏差、测试遗漏或 Hook 异常：

1. 先切到 `fallback`，无需回滚业务代码。
2. 恢复对应 `SKILL.md` 入口，但保留测试用例作为回归证据。
3. 不使用 `git reset --hard`、stash 或 checkout 丢弃其他会话改动。
4. 逐文件回退本方案涉及的 Hook、Manifest 或 Skill。
5. 修复用例后重新从 shadow 开始。
6. `sync-skills --write` 或 `--write --prune` 必须等待同仓库其他 writer 结束后串行执行，避免并发写入竞态。

## 14. 验收矩阵与当前状态

| 维度           | 验收标准                                        | 当前状态             | 证据 / 待办                                                                                                                                                     |
| -------------- | ----------------------------------------------- | -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 路由准确性     | required 100%，forbidden 0%，代码规范组合全命中 | 静态通过             | `skill:test` 258/258                                                                                                                                            |
| 代码准确性     | 代表性任务实现结果与优化前一致或更好            | 待运行时验收         | 需补 Rust/React 代表任务的 focused test/build/runtime                                                                                                           |
| 数据库准确性   | DDL/数据格式不靠样例猜测，迁移可升级可初始化    | 待运行时验收         | 需补 MCP 查询、迁移升级和新库初始化                                                                                                                             |
| UI 准确性      | 页面交互、加载、错误态、控制台无回归            | 待运行时验收         | 需使用内置浏览器/Control Chrome 留证                                                                                                                            |
| 安全性         | 高风险路由零漏选，凭据不进入 Prompt/日志        | 静态通过             | 生产数据库、远端 Git、凭据、制品库、下载站和外部上线同义 cases；Manifest/source fail-closed；unlisted 永久保留和同步器路径安全测试；已知阻断均已关闭            |
| Token / 上下文 | Hook 输出达标，选中入口字节下降                 | 字节通过，Token 待测 | Codex/Claude 广域 active Hook 1,317/1,994 B；selected bytes P50/P95 4,662/13,280；无真实 Token usage                                                            |
| 效率           | 路由 P95 < 50 ms，普通任务 Skill 数下降         | 通过                 | 本轮复测 P95 为 1.321 ms；单域 P95 ≤ 3 测试通过                                                                                                                 |
| 一致性         | Manifest 管理范围共享 Skill 零漂移              | 通过                 | `skill:sync:check` PASS；upstream/platform-local 保留                                                                                                           |
| 编码           | UTF-8 无 BOM，无乱码                            | 通过                 | `validate-skills` PASS                                                                                                                                          |
| 工作区安全     | 不覆盖其他会话 WIP                              | 通过，保留 LOW 约束  | 仅 Skill 基础设施、受管镜像、package 维护脚本和本方案文档，不包含业务 WIP；同步器原子写、符号链接和 prune 边界测试通过；同仓库写模式必须串行，不支持并发 writer |

## 15. 完成定义

只有同时满足以下条件，才能宣布 Skill 优化完成。当前只能宣布“静态实施和静态验收完成”：

- [x] 已建立并通过路由回归测试，不是只凭主观判断。
- [x] 高风险静态用例零漏路由。
- [x] Codex/Claude Hook 的恢复、压缩和斜杠命令跳过能力得到保留。
- [x] Claude Hook 不再注入全量硬编码清单。
- [x] 重叠 Skill 已明确边界、排除和互斥关系。
- [x] 高频长入口已拆分为短入口和按需 References。
- [x] 已废弃阶段入口、镜像、命令、路由推断、组合和测试语料已清理。
- [x] Manifest 管理范围有单一规范源和自动一致性检查。
- [x] 所有 Skill 中文内容通过 UTF-8、BOM 和乱码静态校验。
- [x] 验证矩阵已按实际变更路径结构化，`check` Skill 不依赖路由命中来选择验证。
- [ ] 增加自动读取 `git diff` 并输出验证矩阵命中的独立执行器（当前为人工编排）。
- [ ] 代表性 Rust、React、SQLite、Capabilities、Updater、发布任务均已回归。
- [ ] 页面任务已使用 Codex 内置浏览器或 Control Chrome 验证。
- [ ] 至少 20 个真实任务的 active 灰度观察完成且无准确性回归。
- [x] Skill 基础设施格式、测试、同步和 `git diff --check` 通过。
- [x] 已完成多轮独立审查并补齐逐文件台账；已知阻断均已关闭，唯一 LOW 并发 writer 约束已记录；关键安全、数据库、测试和页面规则仍可追溯。
- [x] 已记录上下文字节和执行耗时，且没有把字节冒充 Token。
- [ ] 已取得可归因的真实 Token usage，并评估实际 Token 降幅。

## 16. 本次文档范围说明

本文件现在同时记录原始方案、实际实施文件和验收状态，不再是纯规划文档。本次 Skill 优化涉及：

- `.codex/PROJECT.md`、两端 Skill Hook 和 `.codex/hooks.json`。
- 3 个路由元数据文件、4 个 Node 脚本、3 个测试/矩阵文件。
- 50 个项目自维护 Skill 入口和 73 个 Codex References。
- Manifest 声明范围内的 Claude/Agents 镜像与 4 个 Claude commands。
- 按用户授权精确删除的 19 个阶段入口、镜像和命令文件。
- `package.json` 中 4 个 `skill:*` 维护脚本入口。

没有修改 Tauri SSH 业务源码、数据库、Capabilities、Cargo 依赖或前端实现。工作区原有的大量业务 WIP 属于其他会话，本方案没有把它们归入实施范围，也没有 stash、reset、checkout、clean 或全量暂存。

由于实际采用并行批量拆分，后续真实任务灰度必须继续在主工作区按文件避让；静态验收通过不等于业务运行时验收完成。

## 17. 已参考技能

方案编写与实施维护时参考了：

- `skill-creator`：用于渐进披露、入口精简、Reference 拆分和 Skill 静态验证原则。
- `add-skill`：用于项目规范源、Manifest、镜像、upstream/platform-local 和验证流程。
- `brainstorm`、`tech-decision`：用于原始方案比较、目标约束、灰度和回滚设计。

`docs-management` 优化后只处理 VitePress 对外站点，本文件属于内部实施台账，不应因普通“方案文档”触发该 Skill；`doc-generation` 也仅在根据代码生成开发者 API/IPC 文档时触发。
