#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");
const { performance } = require("node:perf_hooks");

const REPO_ROOT = path.resolve(__dirname, "../..");
const DEFAULT_MANIFEST = path.join(
  REPO_ROOT,
  ".codex/skill-routing/manifest.json",
);
const DEFAULT_BUNDLES = path.join(
  REPO_ROOT,
  ".codex/skill-routing/bundles.json",
);
const DEFAULT_MODE = "shadow";
const VALID_MODES = new Set(["shadow", "active", "fallback"]);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function normalizePrompt(prompt) {
  return String(prompt || "")
    .normalize("NFKC")
    .replace(/\s+/g, " ")
    .trim();
}

function contains(text, signal) {
  return text
    .toLocaleLowerCase("zh-CN")
    .includes(String(signal).toLocaleLowerCase("zh-CN"));
}

function splitPromptClauses(prompt) {
  return prompt
    .split(/[，,。；;\n]|(?:同时|另外|并且|附带|但|改为|而是)/u)
    .map((clause) => clause.trim())
    .filter(Boolean);
}

function isNegatedAt(clause, matchIndex) {
  const prefix = clause.slice(Math.max(0, matchIndex - 32), matchIndex);
  if (/(?:不但|不仅)\s*$/u.test(prefix)) return false;
  return /(?:请勿|禁止|不要|无需|无须|不能|不可|别|不)(?:想|打算|需要|考虑)?(?:再|继续)?(?:进行|执行|做|使用|用|发送|发|实现|新增|添加|接入|集成|配置|启用|采用|显示|调用|触发|开启)?\s*$/u.test(
    prefix,
  );
}

function affirmativeStringMatches(prompt, signals) {
  const clauses = splitPromptClauses(prompt);
  return signals.filter((signal) =>
    clauses.some((clause) => {
      const normalizedClause = clause.toLocaleLowerCase("zh-CN");
      const normalizedSignal = String(signal).toLocaleLowerCase("zh-CN");
      let offset = 0;
      while (offset <= normalizedClause.length) {
        const index = normalizedClause.indexOf(normalizedSignal, offset);
        if (index < 0) return false;
        if (!isNegatedAt(clause, index)) return true;
        offset = index + Math.max(normalizedSignal.length, 1);
      }
      return false;
    }),
  );
}

function hasAffirmativePattern(prompt, pattern) {
  return splitPromptClauses(prompt).some((clause) => {
    const flags = pattern.flags.includes("g")
      ? pattern.flags
      : `${pattern.flags}g`;
    const localPattern = new RegExp(pattern.source, flags);
    let match;
    while ((match = localPattern.exec(clause)) !== null) {
      if (!isNegatedAt(clause, match.index)) return true;
      if (match[0].length === 0) localPattern.lastIndex += 1;
    }
    return false;
  });
}

function stripNegatedExternalActions(prompt) {
  const negatedExternalAction =
    /(?:请)?(?:也)?(?:请勿|禁止|不要|无需|无须|不能|不可|别|不)(?:再|进行|执行|做)?(?:(?:把|将)[^。；,，\n]{0,32}?)?(?:发布|推送|上传|部署|上线|创建|新建|同步|提交|发版|投产)/iu;
  return splitPromptClauses(prompt)
    .filter((clause) => !negatedExternalAction.test(clause))
    .join(" ");
}

function hasCredentialSignal(prompt) {
  const nonTokenCredential =
    /SSH 密钥|密码|凭据|Safe Credentials|密钥链|API[-_\s]?key|\b(?:GitHub\s+)?PAT\b|personal access token|client[-_\s]?secret|客户端密钥|AWS_SECRET_ACCESS_KEY|访问密钥|~\/\.ssh\/id_(?:rsa|ed25519)|\bid_(?:rsa|ed25519)\b|私钥/iu;
  const tokenTerm = /Token|令牌/iu;
  const strongCredentialContext =
    /Bearer|refresh[-_\s]?token|access[-_\s]?token|刷新令牌|登录|认证|访问|OAuth|PAT|泄露|明文/iu;
  const modelTokenContext =
    /(?:大模型|模型|LLM|context window|上下文(?:窗口)?).{0,20}token.{0,30}(?:预算|上限|消耗|计数|使用量|费用|输入|输出|数量|硬编码(?:为)?\s*\d+)|token.{0,12}(?:预算|上限|消耗|计数|使用量|费用|输入|输出|数量)/iu;
  const dangerousCredentialUse =
    /(?:API[-_\s]?Token|Token|令牌).{0,20}(?:硬编码|明文|写入|输出(?:到|至|进))|(?:硬编码|明文|写入|输出(?:到|至|进)).{0,20}(?:API[-_\s]?Token|Token|令牌)/iu;
  const dangerousApiTokenUse =
    /(?:输出|打印|记录|写入|硬编码).{0,32}(?:大模型|LLM)?.{0,8}API[-_\s]?Token/iu;
  const apiTokenTerm = /API[-_\s]?Token/iu;

  return splitPromptClauses(prompt).some((clause) => {
    if (nonTokenCredential.test(clause)) return true;
    if (!tokenTerm.test(clause)) return false;
    if (strongCredentialContext.test(clause)) return true;
    if (
      (dangerousCredentialUse.test(clause) ||
        dangerousApiTokenUse.test(clause)) &&
      (apiTokenTerm.test(clause) || !modelTokenContext.test(clause))
    ) {
      return true;
    }
    if (modelTokenContext.test(clause)) return false;
    return apiTokenTerm.test(clause);
  });
}

function hasDestructiveSignal(prompt) {
  const destructiveOperation =
    /删除|覆盖|覆写|清理|批量更新|批量删除|delete\s+from|drop\s+(?:table|database|schema|view|index)|truncate|\brm\s+(?:-[a-z]*r[a-z]*(?:\s+-[a-z]+)*|--recursive(?:\s+--force)?)|\bunlink\s+/iu;
  const scopedClear =
    /清空.{0,16}(?:SQLite|数据库|数据表|目录|文件|缓存)|(?:SQLite|数据库|数据表|目录|文件|缓存).{0,16}清空/iu;
  const negatedDestructiveOperation =
    /(?:请)?(?:也)?(?:请勿|禁止|不要|无需|无须|不能|不可|别|不)(?:再|进行|执行|做)?(?:(?:把|将)[^。；,，\n]{0,32}?)?(?:删除|覆盖|覆写|清理|批量更新|批量删除|清空|delete\s+from|drop\s+(?:table|database|schema|view|index)|truncate|\brm\s+|\bunlink\s+)/iu;

  return splitPromptClauses(prompt).some((clause) => {
    const destructive =
      destructiveOperation.test(clause) || scopedClear.test(clause);
    return destructive && !negatedDestructiveOperation.test(clause);
  });
}

function hasUpdaterMutationSignal(prompt) {
  const updaterSignal =
    /tauri-plugin-updater|updater|update\.json|自动更新|更新签名|OTA/iu;
  const mutationBeforeUpdater =
    /(?:新增|添加|实现|创建|修改|修复|接入|集成|配置|生成|写入|更新)(?:(?!README|报告|文档|代码|页面|组件|检查|读取|查看|解析|验证|之后|然后|随后|直接|再).){0,20}(?:tauri-plugin-updater|updater|update\.json|自动更新|更新签名|OTA)/iu;
  const mutationAfterUpdater =
    /(?:tauri-plugin-updater|updater|update\.json|自动更新|更新签名|OTA)(?:(?!README|报告|文档|校验).){0,20}(?:新增|添加|修改|写入|重写|替换|删除|接入|集成|配置|更新)/iu;
  const negatedUpdaterMutation =
    /(?:(?:请)?(?:也)?(?:请勿|禁止|不要|无需|无须|不能|不可|别|不)(?:再|进行|执行|做)?(?:(?:把|将)[^。；,，\n]{0,24}?)?(?:(?:新增|添加|实现|创建|修改|修复|接入|集成|配置|生成|写入|更新).{0,12}(?:tauri-plugin-updater|updater|update\.json|自动更新|更新签名|OTA)|(?:tauri-plugin-updater|updater|update\.json|自动更新|更新签名|OTA).{0,12}(?:新增|添加|修改|写入|重写|替换|删除|接入|集成|配置|更新))|(?:tauri-plugin-updater|updater|update\.json|自动更新|更新签名|OTA).{0,12}(?:请勿|禁止|不要|无需|无须|不能|不可|别|不)(?:再|进行|执行|做)?(?:新增|添加|修改|写入|重写|替换|删除|接入|集成|配置|更新))/iu;
  const nonMutatingUpdaterArtifact =
    /(?:生成|创建).{0,20}(?:tauri-plugin-updater|updater|update\.json|自动更新|OTA).{0,16}(?:校验|验证|检查|解析|审计)?(?:报告|摘要|文档|结果)/iu;

  return splitPromptClauses(prompt).some(
    (clause) =>
      updaterSignal.test(clause) &&
      !negatedUpdaterMutation.test(clause) &&
      !nonMutatingUpdaterArtifact.test(clause) &&
      (mutationBeforeUpdater.test(clause) || mutationAfterUpdater.test(clause)),
  );
}

function inferContext(prompt) {
  const signals = new Set();
  // 否定的发布动作不参与发布/远程写入推断；同句中的其他明确动作仍会保留。
  const operationPrompt = stripNegatedExternalActions(prompt);
  const add = (name, pattern) => {
    if (pattern.test(prompt)) signals.add(name);
  };
  const addOperation = (name, pattern) => {
    if (pattern.test(operationPrompt)) signals.add(name);
  };

  add("react", /React|页面|组件|Ant Design|antd|Zustand|前端/iu);
  add(
    "command",
    /Command|invoke|IPC|tauri::command|generate_handler|(?:新增|添加|实现|修改|开发).{0,12}(?:API|接口|端点)|(?:API|接口|端点).{0,12}(?:新增|添加|实现|修改|开发)/iu,
  );
  add(
    "serialization",
    /serde|反序列化|序列化|camelCase|JSON (?:载荷|日期|字段|传输)/iu,
  );
  add("sqlite", /SQLite|rusqlite|PRAGMA|本地数据库|数据库 DDL|DAO/iu);
  add("migration", /迁移|user_version|旧库升级|初始化数据库/iu);
  add(
    "filesystem",
    /文件导入|文件导出|拖放|fs API|文件对话框|应用读取本地配置文件/iu,
  );
  if (hasAffirmativePattern(prompt, /tauri-plugin-|Tauri 插件/iu)) {
    signals.add("tauri-plugin");
  }
  add(
    "capabilities",
    /capabilit|permission|scope|权限声明|Tauri 权限|WebView.{0,12}权限|shell 权限|default\.json.{0,24}(?:shell:|core:)|(?:shell:|core:).{0,24}权限|开放.{0,24}权限/iu,
  );
  add("updater", /updater|update\.json|自动更新|更新签名|OTA/iu);
  add(
    "packaging",
    /dmg|msi|appimage|安装包|bundle|签名|构建产物|制品库|构建制品/iu,
  );
  add(
    "git",
    /\bgit\b|commit|branch|分支|打 tag|push|merge|远端仓库|代码仓库|GitHub|GitLab|Gitee|Gitea|Bitbucket|\borigin\b|\bupstream\b|提交(?:当前|本次)?(?:代码|改动|变更|修改)|提交信息/iu,
  );
  addOperation(
    "release",
    /发布|release|打 tag|发版.{0,16}(?:生产|线上)|投产|(?:上线|部署|上传)[^。；\n]{0,24}(?:下载站|制品库|artifact repository)|(?:部署|上线)[^。；\n]{0,16}(?:生产|线上)(?:环境)?/iu,
  );
  addOperation(
    "publish",
    /发布|推送|push|release 产物|发版.{0,16}(?:生产|线上)|投产|(?:(?:创建|新建|发布)[^。；\n]{0,16}(?:GitHub|GitLab|Gitee|Gitea)|(?:GitHub|GitLab|Gitee|Gitea)[^。；\n]{0,16}(?:创建|新建|发布))\s*Release|(?:推送|上传)[^。；\n]{0,24}(?:(?:远端|远程)(?:代码)?仓库|GitHub|GitLab|Gitee|Gitea|Bitbucket)|(?:上线|部署|上传)[^。；\n]{0,24}(?:下载站|制品库|artifact repository)|(?:部署|上线)[^。；\n]{0,16}(?:生产|线上)(?:环境)?/iu,
  );
  if (hasCredentialSignal(prompt)) signals.add("credentials");
  addOperation(
    "remote-write",
    /远程执行|远程发布|服务器|Jenkins|git\s+push|发版.{0,16}(?:生产|线上)|投产|(?:同步|提交|推送|上传)[^。；\n]{0,24}(?:(?:远端|远程)(?:代码)?仓库|GitHub|GitLab|Gitee|Gitea|Bitbucket)|(?:git\s+push|推送)[^。；\n]{0,12}(?:origin|upstream)\b|(?:(?:创建|新建|发布)[^。；\n]{0,16}(?:GitHub|GitLab|Gitee|Gitea)|(?:GitHub|GitLab|Gitee|Gitea)[^。；\n]{0,16}(?:创建|新建|发布))\s*Release|(?:上线|部署|上传)[^。；\n]{0,24}(?:下载站|制品库|artifact repository)|(?:部署|上线)[^。；\n]{0,16}(?:生产|线上)(?:环境)?/iu,
  );
  add(
    "remote-database",
    /远程\s*(?:MySQL|PostgreSQL|Postgres|数据库)|Nacos[^。]*(?:MySQL|PostgreSQL|Postgres|数据库)|(?:生产|线上|RDS|外部数据库|远程数据库).{0,24}(?:DDL|Schema|数据格式|表结构|MySQL|PostgreSQL|Postgres|数据库)|(?:MySQL|PostgreSQL|Postgres|数据库).{0,24}(?:生产|线上|RDS|真实\s*DDL|数据格式)/iu,
  );
  if (hasDestructiveSignal(prompt)) signals.add("destructive");
  add("implement", /新增|添加|实现|创建|修改|修复|接入|集成|配置|生成|写入/iu);
  add(
    "diagnose",
    /为什么|定位|诊断|排查|报错|编译报|失败|不生效|没有收到|没触发|未触发|没有触发/iu,
  );
  add("test", /测试|test|验证|用例|mock/iu);
  add("design", /设计|架构|方案|ADR|拆分/iu);
  add("document", /文档|VitePress|用户手册|Markdown/iu);

  return signals;
}

function inferRisk(prompt, signals) {
  const risks = [];
  const add = (id, reason) => {
    if (!risks.some((risk) => risk.id === id)) risks.push({ id, reason });
  };

  if (signals.has("credentials"))
    add("credentials", "涉及凭据或密钥，必须避免明文暴露");
  if (signals.has("remote-write"))
    add("remote-write", "涉及远程或外部写入，需确认目标与回滚路径");
  if (signals.has("remote-database"))
    add(
      "remote-database",
      "涉及外部数据库，应按项目基线读取配置并通过 Tauri SSH MCP 核对真实 DDL 与数据格式",
    );
  if (signals.has("destructive"))
    add("destructive", "涉及删除或覆盖，需先解析精确目标并保留可恢复性");
  if (signals.has("capabilities"))
    add("capabilities", "涉及权限范围，需执行最小权限与运行时验证");
  if (signals.has("release") && signals.has("publish"))
    add("release", "涉及发布或推送，需验证版本、签名、产物与回滚");
  if (signals.has("sqlite") && /迁移|DDL|删除|批量|生产数据/iu.test(prompt)) {
    add(
      "database",
      "涉及数据库结构或高影响数据操作，需验证真实 DDL 和迁移路径",
    );
  }
  if (signals.has("sqlite") && !risks.some((risk) => risk.id === "database")) {
    add(
      "database",
      "涉及本地数据库，需要核对真实 schema、事务边界和数据兼容性",
    );
  }
  if (signals.has("filesystem"))
    add("filesystem", "涉及应用文件系统能力，需要限制路径范围并验证权限");
  if (signals.has("tauri-plugin"))
    add("plugin-permission", "Tauri 插件需要核对注册、Capabilities 和最小权限");
  if (/\bCSP\b|安全审查|最小权限/iu.test(prompt))
    add("security", "涉及安全策略，需要执行威胁边界和最小权限审查");
  if (/签名/iu.test(prompt))
    add("signing", "涉及签名材料，不能输出私钥或明文凭据");
  if (/全新项目|新建一个桌面项目|创建新 Tauri 项目/iu.test(prompt))
    add("filesystem", "项目初始化会创建大量文件，需确认目标目录和冲突");

  return risks;
}

function explicitSkillName(prompt, knownNames) {
  const match = prompt.match(/^\s*[/$]([a-z0-9][a-z0-9-]*)\b/iu);
  return match && knownNames.has(match[1]) ? match[1] : null;
}

function compatibleWeakSignal(skill, context) {
  const layers = new Set(skill.layers || []);
  const map = {
    react: ["react", "theme", "state"],
    command: ["ipc", "rust-command"],
    sqlite: ["sqlite", "database"],
    filesystem: ["filesystem"],
    "tauri-plugin": ["plugin"],
    capabilities: ["capabilities", "security"],
    updater: ["updater"],
    packaging: ["packaging"],
    git: ["git"],
    diagnose: ["diagnostics", "errors", "performance"],
    test: ["tests"],
    design: ["architecture", "planning", "decision", "patterns"],
    document: ["developer-docs", "vitepress", "experience"],
    serialization: ["serialization"],
  };
  for (const signal of context) {
    if ((map[signal] || []).some((layer) => layers.has(layer))) return true;
  }
  return false;
}

function addCandidate(selected, byName, name, reason, score, source) {
  const skill = byName.get(name);
  if (!skill) return;
  const current = selected.get(name);
  if (!current || score > current.score) {
    selected.set(name, { name, reason, score, source });
  }
}

function deleteUnlessStrongSignal(selected, byName, name, prompt) {
  const skill = byName.get(name);
  const hasStrongSignal =
    affirmativeStringMatches(prompt, skill?.strongSignals || []).length > 0;
  if (!hasStrongSignal) selected.delete(name);
}

function addRisk(risks, id, reason) {
  if (!risks.some((risk) => risk.id === id)) risks.push({ id, reason });
}

function applySelectedRiskTags(selected, byName, risks, prompt) {
  const tagReasons = {
    credentials: "所选 Skill 涉及凭据或签名材料，必须避免明文暴露",
    remote: "所选 Skill 涉及远程访问，需要核对授权、目标和审计路径",
    "external-write": "所选 Skill 可能产生外部写入，需要明确授权和回滚路径",
    release: "所选 Skill 涉及发布链路，需要验证版本、签名、产物和回滚",
    capabilities: "所选 Skill 涉及权限范围，需要最小权限与运行时验证",
    filesystem: "所选 Skill 涉及文件系统范围，需要限制目标并防止路径越界",
    database: "所选 Skill 涉及数据库，需要核对真实 schema、事务和数据兼容性",
  };
  const selectedBeforeSafety = [...selected.values()];
  const consumedTags = new Set();
  for (const candidate of selectedBeforeSafety) {
    if (candidate.name === "security-permissions") continue;
    // 只读检查 update.json 不会访问签名凭据或执行发布；实现/修改更新链路时才启用其固有风险标签。
    if (
      candidate.name === "tauri-updater" &&
      !hasUpdaterMutationSignal(prompt)
    ) {
      continue;
    }
    const skill = byName.get(candidate.name);
    for (const tag of skill?.riskTags || []) {
      if (!(tag in tagReasons)) continue;
      consumedTags.add(tag);
      const equivalentRiskId =
        tag === "external-write"
          ? "remote-write"
          : tag === "remote"
            ? "remote-write"
            : tag;
      if (risks.some((risk) => risk.id === equivalentRiskId)) continue;
      addRisk(risks, `skill:${tag}`, tagReasons[tag]);
    }
  }
  if (consumedTags.size > 0) {
    addCandidate(
      selected,
      byName,
      "security-permissions",
      `所选 Skill 风险标签：${[...consumedTags].sort().join("、")}`,
      125,
      "risk-tag",
    );
  }
}

function applyRiskSafetyCandidate(selected, byName, risks) {
  const safetyRisk = risks.some((risk) =>
    [
      "credentials",
      "remote-write",
      "remote-database",
      "destructive",
      "capabilities",
      "release",
      "signing",
    ].includes(risk.id),
  );
  if (safetyRisk) {
    addCandidate(
      selected,
      byName,
      "security-permissions",
      "高风险信号需要安全边界",
      120,
      "risk",
    );
  }
}

function applyBundles(selected, byName, bundleConfig, context) {
  for (const bundle of bundleConfig.bundles || []) {
    const all = bundle.when?.signalsAll || [];
    const any = bundle.when?.signalsAny || [];
    const matchesAll =
      all.length === 0 || all.every((signal) => context.has(signal));
    const matchesAny =
      any.length === 0 || any.some((signal) => context.has(signal));
    if (!matchesAll || !matchesAny) continue;

    for (const name of bundle.required || []) {
      addCandidate(
        selected,
        byName,
        name,
        `组合规则 ${bundle.id}`,
        80,
        "bundle",
      );
    }
    for (const [signal, name] of Object.entries(bundle.conditional || {})) {
      if (context.has(signal)) {
        addCandidate(
          selected,
          byName,
          name,
          `组合规则 ${bundle.id}: ${signal}`,
          75,
          "bundle",
        );
      }
    }
  }
}

function applyConservativeRules(selected, byName, prompt, context, risks) {
  applyRiskSafetyCandidate(selected, byName, risks);
  if (
    context.has("diagnose") &&
    !/实现 AppError|统一错误传播|ErrorBoundary/iu.test(prompt)
  ) {
    addCandidate(
      selected,
      byName,
      "bug-detective",
      "诊断意图需要证据化定位",
      105,
      "intent",
    );
  }
  if (context.has("tauri-plugin")) {
    addCandidate(
      selected,
      byName,
      "tauri-capabilities",
      "Tauri 插件需核对权限声明",
      85,
      "bundle",
    );
    addCandidate(
      selected,
      byName,
      "security-permissions",
      "插件能力需最小权限审查",
      85,
      "bundle",
    );
  }
  if (context.has("filesystem") && context.has("implement")) {
    addCandidate(
      selected,
      byName,
      "file-storage",
      "应用文件系统功能",
      95,
      "rule",
    );
    addCandidate(
      selected,
      byName,
      "tauri-capabilities",
      "文件功能需核对 Capabilities",
      85,
      "bundle",
    );
    addCandidate(
      selected,
      byName,
      "security-permissions",
      "文件范围需最小权限审查",
      85,
      "bundle",
    );
  }
  if (context.has("sqlite") && context.has("migration")) {
    addCandidate(selected, byName, "database-ops", "SQLite 迁移", 110, "rule");
    addCandidate(
      selected,
      byName,
      "test-development",
      "迁移必须验证旧库升级与新库初始化",
      90,
      "bundle",
    );
  }
  if (context.has("git") && context.has("remote-write")) {
    addCandidate(
      selected,
      byName,
      "git-workflow",
      "远端 Git 写入需要受控工作流",
      110,
      "risk",
    );
  }
  if (context.has("packaging") && context.has("publish")) {
    addCandidate(
      selected,
      byName,
      "tauri-packaging",
      "构建产物需先完成打包与产物验证",
      105,
      "rule",
    );
    addCandidate(
      selected,
      byName,
      "release-publish",
      "构建产物上线属于外部发布",
      110,
      "risk",
    );
  }
  if (/业务数据状态/iu.test(prompt))
    deleteUnlessStrongSignal(selected, byName, "database-ops", prompt);
  if (/不需要 SQLite/iu.test(prompt)) selected.delete("database-ops");
  if (/antd message/iu.test(prompt)) {
    deleteUnlessStrongSignal(selected, byName, "notification-system", prompt);
    addCandidate(
      selected,
      byName,
      "ui-frontend",
      "Ant Design 页面消息",
      110,
      "rule",
    );
  }
  if (/原生系统通知|tauri-plugin-notification/iu.test(prompt))
    deleteUnlessStrongSignal(selected, byName, "tauri-events", prompt);
  if (/先定位|先诊断|不要修改/iu.test(prompt)) {
    selected.delete("error-handler");
    selected.delete("dev");
  }
  if (
    /实现 AppError|统一错误传播|ErrorBoundary/iu.test(prompt) &&
    !/先定位|先诊断/iu.test(prompt)
  ) {
    selected.delete("bug-detective");
  }
  if (/普通 npm/iu.test(prompt))
    deleteUnlessStrongSignal(selected, byName, "tauri-plugins", prompt);
  if (/内部优化方案/iu.test(prompt)) {
    deleteUnlessStrongSignal(selected, byName, "docs-management", prompt);
    deleteUnlessStrongSignal(selected, byName, "update-docs", prompt);
  }
  if (/沉淀.*Skill|Skill.*沉淀/iu.test(prompt)) {
    addCandidate(
      selected,
      byName,
      "add-skill",
      "沉淀结果包含 Skill 创建或维护",
      95,
      "rule",
    );
  }
  if (
    /生成 dmg|安装包|AppImage|MSI/iu.test(prompt) &&
    !/发布|推送|上线|部署|上传/iu.test(prompt)
  ) {
    selected.delete("release-publish");
    selected.delete("release");
  }
  if (/pnpm build/iu.test(prompt) && !/安装包|bundle/iu.test(prompt)) {
    selected.delete("tauri-packaging");
    selected.delete("release");
  }
  if (
    /(?:不要|无需|不)(?:再|进行)?发布/iu.test(prompt) &&
    !(context.has("release") && context.has("publish"))
  ) {
    selected.delete("release");
    selected.delete("release-publish");
  }
  if (/普通业务状态字段/iu.test(prompt)) {
    deleteUnlessStrongSignal(selected, byName, "update-status", prompt);
    deleteUnlessStrongSignal(selected, byName, "store-management", prompt);
    deleteUnlessStrongSignal(selected, byName, "progress", prompt);
  }
}

function resolveMutex(selected, manifest) {
  const groups = new Map();
  for (const candidate of selected.values()) {
    const skill = manifest.skills.find((item) => item.name === candidate.name);
    if (!skill?.mutexGroup) continue;
    if (!groups.has(skill.mutexGroup)) groups.set(skill.mutexGroup, []);
    groups.get(skill.mutexGroup).push(candidate);
  }
  for (const [group, candidates] of groups) {
    if (group === "notification-kind") continue;
    if (group === "workflow-command" && candidates.length > 1) {
      candidates.sort(
        (a, b) => b.score - a.score || a.name.localeCompare(b.name),
      );
      for (const candidate of candidates.slice(1))
        selected.delete(candidate.name);
      continue;
    }
  }
}

function routePrompt(prompt, options = {}) {
  const startedAt = performance.now();
  const normalized = normalizePrompt(prompt);
  const requestedMode =
    options.mode || process.env.SKILL_ROUTER_MODE || DEFAULT_MODE;
  const mode = VALID_MODES.has(requestedMode) ? requestedMode : DEFAULT_MODE;
  const empty = normalized.length === 0;
  const skipReason = empty
    ? "empty-prompt"
    : options.resumed
      ? "resumed-session"
      : options.compacted
        ? "compacted-context"
        : mode === "fallback"
          ? "fallback-mode"
          : null;
  if (skipReason) {
    const result = {
      mode,
      skip: true,
      skipped: true,
      skipReason,
      explicit: false,
      skills: [],
      candidates: [],
      risks: [],
      riskSupplements: [],
      uncertainties: [],
      elapsedMs: Number((performance.now() - startedAt).toFixed(3)),
    };
    return result;
  }

  const manifest = readJson(options.manifestPath || DEFAULT_MANIFEST);
  const bundles = readJson(options.bundlesPath || DEFAULT_BUNDLES);
  const platform = options.platform || "codex";
  const routeRepoRoot = path.resolve(options.repoRoot || REPO_ROOT);
  const availableSkills = manifest.skills.filter((skill) => {
    if (!(skill.platforms || []).includes(platform)) return false;
    const sourcePath = path.resolve(routeRepoRoot, skill.source || "");
    const relativeSource = path.relative(routeRepoRoot, sourcePath);
    if (
      relativeSource === "" ||
      relativeSource.startsWith(`..${path.sep}`) ||
      path.isAbsolute(relativeSource)
    ) {
      return false;
    }
    return fs.existsSync(sourcePath);
  });
  const byName = new Map(availableSkills.map((skill) => [skill.name, skill]));
  const context = inferContext(normalized);
  const risks = inferRisk(normalized, context);
  const selected = new Map();
  const explicitName = explicitSkillName(normalized, new Set(byName.keys()));

  if (explicitName) {
    addCandidate(
      selected,
      byName,
      explicitName,
      `显式选择 ${explicitName}`,
      1000,
      "explicit",
    );
    if (explicitName === "release" && risks.length === 0) {
      risks.push({
        id: "release",
        reason: "显式发布工作流需要版本、签名、产物与回滚验证",
      });
    }
    applyRiskSafetyCandidate(selected, byName, risks);
    applySelectedRiskTags(selected, byName, risks, normalized);
  } else {
    const externalActionPrompt = stripNegatedExternalActions(normalized);
    for (const skill of availableSkills) {
      const matchingPrompt = ["release", "release-publish"].includes(skill.name)
        ? externalActionPrompt
        : normalized;
      let strongMatches = affirmativeStringMatches(
        matchingPrompt,
        skill.strongSignals || [],
      );
      if (
        skill.name === "security-permissions" &&
        !hasCredentialSignal(normalized)
      ) {
        strongMatches = strongMatches.filter(
          (signal) => !/token|令牌/iu.test(signal),
        );
      }
      const excluded =
        affirmativeStringMatches(matchingPrompt, skill.excludeWhen || [])
          .length > 0;
      // 子句内的否定信号不参与匹配；其他独立肯定子任务的专有强信号仍可保留对应规范。
      if (excluded && strongMatches.length === 0) continue;
      const weakMatches = (skill.weakSignals || []).filter((signal) =>
        contains(matchingPrompt, signal),
      );
      const isExplicitLike = ["workflow-command", "terminal-action"].includes(
        skill.kind,
      );
      if (isExplicitLike && strongMatches.length === 0) continue;
      if (strongMatches.length > 0) {
        const score = 100 + strongMatches.length * 15;
        addCandidate(
          selected,
          byName,
          skill.name,
          `强信号：${strongMatches.slice(0, 2).join("、")}`,
          score,
          "strong-signal",
        );
      } else if (
        skill.activation === "auto" &&
        weakMatches.length > 0 &&
        compatibleWeakSignal(skill, context)
      ) {
        addCandidate(
          selected,
          byName,
          skill.name,
          `上下文弱信号：${weakMatches.slice(0, 2).join("、")}`,
          35 + weakMatches.length * 5,
          "weak-signal",
        );
      }
    }

    applyBundles(selected, byName, bundles, context);
    applyConservativeRules(selected, byName, normalized, context, risks);
    applySelectedRiskTags(selected, byName, risks, normalized);
    resolveMutex(selected, manifest);
  }

  const skills = [...selected.values()].sort(
    (a, b) => b.score - a.score || a.name.localeCompare(b.name),
  );
  const riskSupplements = risks.map((risk) => risk.reason);
  const uncertainties = [];
  if (
    !explicitName &&
    skills.length === 0 &&
    /修改|实现|修复|新增/iu.test(normalized)
  ) {
    uncertainties.push(
      "未识别明确技术层；实现前需从真实文件与调用链补充领域判断",
    );
  }

  return {
    mode,
    skip: false,
    skipped: false,
    skipReason: null,
    explicit: Boolean(explicitName),
    skills,
    candidates: skills,
    risks,
    riskSupplements,
    uncertainties,
    elapsedMs: Number((performance.now() - startedAt).toFixed(3)),
  };
}

function truncateUtf8(text, maxBytes) {
  if (Buffer.byteLength(text, "utf8") <= maxBytes) return text;
  let output = "";
  for (const character of text) {
    const next = output + character;
    if (Buffer.byteLength(`${next}\n…`, "utf8") > maxBytes) break;
    output = next;
  }
  return `${output.trimEnd()}\n…`;
}

function formatRouteResult(result, options = {}) {
  const maxBytes = Number.isFinite(options.maxBytes) ? options.maxBytes : 1536;
  if (result.skip) return "";

  const lines = ["## Skill 路由建议"];
  if (result.skills.length === 0) {
    lines.push(
      "- 未识别明确领域 Skill；按项目短基线执行并以真实代码证据补充判断。",
    );
  } else {
    for (const skill of result.skills) {
      lines.push(`- ${skill.name}: ${skill.reason}`);
    }
  }
  if (result.risks.length > 0) {
    lines.push("风险补充：");
    for (const risk of result.risks) lines.push(`- ${risk.reason}`);
  }
  if (result.uncertainties.length > 0) {
    lines.push("不确定项：");
    for (const uncertainty of result.uncertainties)
      lines.push(`- ${uncertainty}`);
  }
  lines.push(
    "路由只决定读取规则；实现与验收仍以真实代码、配置、数据和运行结果为准。",
  );
  return truncateUtf8(lines.join("\n"), maxBytes);
}

function parseCliInput(argv) {
  const options = {};
  const promptParts = [];
  let jsonOutput = false;
  for (const arg of argv) {
    if (arg === "--json") jsonOutput = true;
    else if (arg.startsWith("--mode="))
      options.mode = arg.slice("--mode=".length);
    else if (arg.startsWith("--platform="))
      options.platform = arg.slice("--platform=".length);
    else promptParts.push(arg);
  }

  if (promptParts.length > 0) {
    return { prompt: promptParts.join(" "), options, jsonOutput };
  }

  const stdin = fs.readFileSync(0, "utf8").trim();
  if (!stdin) return { prompt: "", options, jsonOutput };
  try {
    const parsed = JSON.parse(stdin);
    if (typeof parsed === "string")
      return { prompt: parsed, options, jsonOutput };
    return {
      prompt: parsed.prompt || parsed.userPrompt || "",
      options: { ...options, ...(parsed.options || {}) },
      jsonOutput: parsed.json === true || jsonOutput,
    };
  } catch {
    return { prompt: stdin, options, jsonOutput };
  }
}

if (require.main === module) {
  const input = parseCliInput(process.argv.slice(2));
  const result = routePrompt(input.prompt, input.options);
  process.stdout.write(
    input.jsonOutput
      ? `${JSON.stringify(result, null, 2)}\n`
      : `${formatRouteResult(result)}\n`,
  );
}

module.exports = {
  formatRouteResult,
  inferContext,
  normalizePrompt,
  routePrompt,
};
