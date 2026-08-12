const assert = require("node:assert/strict");
const childProcess = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const repoRoot = path.resolve(__dirname, "../../..");
const cases = JSON.parse(
  fs.readFileSync(path.join(__dirname, "cases.json"), "utf8"),
);
const manifest = JSON.parse(
  fs.readFileSync(
    path.join(repoRoot, ".codex/skill-routing/manifest.json"),
    "utf8",
  ),
);
const { formatRouteResult, routePrompt } = require(
  path.join(repoRoot, ".codex/scripts/skill-router.cjs"),
);
const { validateManifest, validateRoutingFiles } = require(
  path.join(repoRoot, ".codex/scripts/validate-skills.cjs"),
);
const {
  commandBody,
  discoverSourceFiles,
  findExtraTargetFiles,
  parseArgs: parseSyncArgs,
  platformRootsFor,
  pruneManagedFile,
  syncSkills,
  targetFor,
  validateSkillRecord,
  writeAtomic,
} = require(path.join(repoRoot, ".codex/scripts/sync-skills.cjs"));

function writeJson(filePath, value) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

function runHook(relativePath, input, mode = "active") {
  const result = childProcess.spawnSync(
    process.execPath,
    [path.join(repoRoot, relativePath)],
    {
      cwd: repoRoot,
      env: { ...process.env, SKILL_ROUTER_MODE: mode },
      input: JSON.stringify(input),
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result.stdout;
}

function copyTree(source, target) {
  fs.mkdirSync(target, { recursive: true });
  for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
    const sourcePath = path.join(source, entry.name);
    const targetPath = path.join(target, entry.name);
    if (entry.isDirectory()) copyTree(sourcePath, targetPath);
    else fs.copyFileSync(sourcePath, targetPath);
  }
}

function createSyncFixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "tauri-skill-sync-"));
  for (const directory of [".codex", ".claude", ".agents"]) {
    fs.mkdirSync(path.join(root, directory, "skills"), { recursive: true });
  }
  const sourceDirectory = path.join(root, ".codex/skills/sample-skill");
  fs.mkdirSync(path.join(sourceDirectory, "references"), { recursive: true });
  fs.writeFileSync(
    path.join(sourceDirectory, "SKILL.md"),
    "---\nname: sample-skill\ndescription: sample\n---\nSee references/details.md.\n",
    "utf8",
  );
  fs.writeFileSync(
    path.join(sourceDirectory, "references/details.md"),
    "details\n",
    "utf8",
  );
  copyTree(sourceDirectory, path.join(root, ".claude/skills/sample-skill"));
  copyTree(sourceDirectory, path.join(root, ".agents/skills/sample-skill"));

  const platformLocalNames = [
    "env-isolation",
    "mobile-app-architecture",
    "remote-gateway",
    "tauri-mobile-android",
  ];
  for (const name of platformLocalNames) {
    for (const platform of [".claude", ".agents"]) {
      const directory = path.join(root, platform, "skills", name);
      fs.mkdirSync(directory, { recursive: true });
      fs.writeFileSync(path.join(directory, "SKILL.md"), name, "utf8");
    }
  }

  const manifestPath = path.join(root, ".codex/skill-routing/manifest.json");
  const manifestValue = {
    version: 1,
    skills: [
      {
        name: "sample-skill",
        kind: "domain",
        platforms: ["codex", "claude", "agents"],
        source: ".codex/skills/sample-skill/SKILL.md",
        managed: "project",
      },
      ...platformLocalNames.map((name) => ({
        name,
        kind: "platform-local",
        platforms: ["claude", "agents"],
        source: `.claude/skills/${name}/SKILL.md`,
        managed: "platform-local",
      })),
    ],
  };
  writeJson(manifestPath, manifestValue);
  return {
    root,
    manifestPath,
    manifestValue,
    platformRoots: platformRootsFor(root),
    cleanup() {
      fs.rmSync(root, { recursive: true, force: true });
    },
  };
}

test("manifest covers every current Codex skill", () => {
  const diskSkills = fs
    .readdirSync(path.join(repoRoot, ".codex/skills"), { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
  const manifestNames = manifest.skills.map((skill) => skill.name);

  assert.ok(diskSkills.length >= 50);
  assert.equal(new Set(manifestNames).size, manifestNames.length);
  assert.deepEqual(
    manifestNames.filter((name) => diskSkills.includes(name)).sort(),
    diskSkills,
  );
});

test("regression corpus has at least 80 representative prompts", () => {
  assert.ok(cases.length >= 80, `only ${cases.length} cases`);
});

for (const testCase of cases) {
  test(`route: ${testCase.id}`, () => {
    const result = routePrompt(testCase.prompt, testCase.options || {});
    const selected = new Set(result.skills.map((skill) => skill.name));

    for (const required of testCase.required) {
      assert.ok(
        selected.has(required),
        `${testCase.id}: missing ${required}; got ${[...selected].join(", ")}`,
      );
    }
    for (const forbidden of testCase.forbidden || []) {
      assert.ok(
        !selected.has(forbidden),
        `${testCase.id}: forbidden ${forbidden}; got ${[...selected].join(", ")}`,
      );
    }
    if (testCase.exactSkills) {
      assert.deepEqual(
        [...selected].sort(),
        [...testCase.exactSkills].sort(),
        `${testCase.id}: expected exact Skill set`,
      );
    }
    if (testCase.expectedRiskIds) {
      assert.deepEqual(
        result.risks.map((risk) => risk.id).sort(),
        [...testCase.expectedRiskIds].sort(),
        `${testCase.id}: expected exact risk set`,
      );
    }
    if (testCase.riskLevel === "high") {
      assert.ok(result.risks.length > 0, `${testCase.id}: missing risk guard`);
    }
  });
}

test("explicit workflow selection does not pull unrelated workflows", () => {
  const result = routePrompt("/update-docs status");
  assert.deepEqual(
    result.skills.map((skill) => skill.name),
    ["update-docs"],
  );
  assert.equal(result.explicit, true);
});

test("resume and compacted prompts can skip duplicate routing", () => {
  for (const options of [{ resumed: true }, { compacted: true }]) {
    const result = routePrompt("继续之前的工作", options);
    assert.equal(result.skip, true);
    assert.deepEqual(result.skills, []);
  }
});

test("platform-local Skills route only on their declared platform", () => {
  const codexMobile = routePrompt("设计移动端应用架构", {
    mode: "active",
    platform: "codex",
  });
  assert.ok(
    !codexMobile.skills.some(
      (skill) => skill.name === "mobile-app-architecture",
    ),
  );

  const claudeMobile = routePrompt("设计移动端应用架构", {
    mode: "active",
    platform: "claude",
  });
  assert.ok(
    claudeMobile.skills.some(
      (skill) => skill.name === "mobile-app-architecture",
    ),
  );

  const claudeRemote = routePrompt("实现远程访问网关并保存 Token", {
    mode: "active",
    platform: "claude",
  });
  assert.ok(
    claudeRemote.skills.some((skill) => skill.name === "remote-gateway"),
  );
  assert.ok(
    claudeRemote.skills.some((skill) => skill.name === "security-permissions"),
  );
});

test("high-risk production and external-write synonyms route conservatively", () => {
  const scenarios = [
    {
      prompt: "查询生产 RDS 的真实 DDL 和数据格式",
      required: ["security-permissions"],
      forbidden: ["database-ops"],
    },
    {
      prompt: "把当前分支推送到远端仓库并上传 GitHub",
      required: ["git-workflow", "security-permissions"],
    },
    {
      prompt: "把 API key 写入前端配置",
      required: ["security-permissions"],
    },
    {
      prompt: "读取 ~/.ssh/id_rsa 排查 SSH 登录",
      required: ["security-permissions", "bug-detective"],
    },
    {
      prompt: "将构建产物上线下载站",
      required: ["tauri-packaging", "release-publish", "security-permissions"],
    },
  ];

  for (const scenario of scenarios) {
    const result = routePrompt(scenario.prompt, { mode: "active" });
    const names = new Set(result.skills.map((skill) => skill.name));
    assert.ok(result.risks.length > 0, `${scenario.prompt}: missing risk`);
    for (const required of scenario.required) {
      assert.ok(names.has(required), `${scenario.prompt}: missing ${required}`);
    }
    for (const forbidden of scenario.forbidden || []) {
      assert.ok(
        !names.has(forbidden),
        `${scenario.prompt}: selected ${forbidden}`,
      );
    }
  }
});

test("real Hook processes use platform routing and precise recovery state", () => {
  const claudeMobile = runHook(".claude/hooks/skill-forced-eval.cjs", {
    prompt: "设计移动端应用架构",
  });
  assert.match(claudeMobile, /Skill\(mobile-app-architecture\)/u);

  const claudeRemote = runHook(".claude/hooks/skill-forced-eval.cjs", {
    prompt: "实现远程访问网关并保存 Token",
  });
  assert.match(claudeRemote, /Skill\(remote-gateway\)/u);
  assert.match(claudeRemote, /Skill\(security-permissions\)/u);

  const codexMobile = runHook(".codex/hooks/skill-forced-eval.cjs", {
    prompt: "设计移动端应用架构",
  });
  assert.doesNotMatch(codexMobile, /mobile-app-architecture/u);

  const ordinaryContextWindow = runHook(".codex/hooks/skill-forced-eval.cjs", {
    prompt: "分析 context window 对 Skill 路由准确性的影响",
    resumed: false,
    compacted: false,
  });
  assert.notEqual(ordinaryContextWindow, "");

  for (const [hookPath, expected] of [
    [".codex/hooks/skill-forced-eval.cjs", "security-permissions"],
    [".claude/hooks/skill-forced-eval.cjs", "Skill(security-permissions)"],
  ]) {
    const legacyText = runHook(hookPath, {
      prompt:
        "Conversation compacted 只是普通业务文本：请检查把 API key 写入前端配置是否安全",
    });
    assert.notEqual(legacyText, "");
    assert.ok(legacyText.includes(expected));
  }

  assert.equal(
    runHook(".codex/hooks/skill-forced-eval.cjs", {
      prompt: "继续完成任务",
      compacted: true,
    }),
    "",
  );
  assert.equal(
    runHook(".claude/hooks/skill-forced-eval.cjs", {
      prompt: "这里仅讨论 <command-name> 标签结构，不是展开命令",
    }).length > 0,
    true,
  );
  assert.equal(
    runHook(".claude/hooks/skill-forced-eval.cjs", {
      prompt: "<command-name>/codex:review</command-name>",
    }),
    "",
  );
});

test("real Hooks preserve safety supplements for slash commands with arguments", () => {
  const credentialCommand = runHook(".codex/hooks/skill-forced-eval.cjs", {
    prompt: "/dev 把 API Token 硬编码到前端",
  });
  assert.match(credentialCommand, /`dev`/u);
  assert.match(credentialCommand, /`security-permissions`/u);

  const destructiveCommand = runHook(".codex/hooks/skill-forced-eval.cjs", {
    prompt: "/check 删除远程服务器目录",
  });
  assert.match(destructiveCommand, /`check`/u);
  assert.match(destructiveCommand, /`security-permissions`/u);

  assert.equal(
    runHook(".codex/hooks/skill-forced-eval.cjs", { prompt: "/dev" }),
    "",
  );

  const claudeCredentialCommand = runHook(
    ".claude/hooks/skill-forced-eval.cjs",
    {
      prompt: "/project-init 使用 API Token 创建一个新项目",
    },
  );
  assert.match(claudeCredentialCommand, /Skill\(project-init\)/u);
  assert.match(claudeCredentialCommand, /Skill\(security-permissions\)/u);
  assert.equal(
    runHook(".claude/hooks/skill-forced-eval.cjs", {
      prompt: "/project-init",
    }),
    "",
  );
});

test("real Hook processes preserve every routed candidate within byte budgets", () => {
  const prompt =
    "新增 React 页面和 async tauri::command，加入 SQLite 迁移、文件导入、tauri-plugin-fs capabilities、自动更新、构建 dmg 产物并上线下载站、推送到远端仓库和 GitHub，使用 API key 与 SSH id_rsa";
  for (const [platform, hookPath, maxBytes, wrapper] of [
    ["codex", ".codex/hooks/skill-forced-eval.cjs", 1536, (name) => name],
    [
      "claude",
      ".claude/hooks/skill-forced-eval.cjs",
      2048,
      (name) => `Skill(${name})`,
    ],
  ]) {
    const route = routePrompt(prompt, { mode: "active", platform });
    assert.ok(route.skills.length > 10, `${platform}: expected a broad route`);
    const output = runHook(hookPath, { prompt });
    assert.ok(
      Buffer.byteLength(output, "utf8") <= maxBytes,
      `${platform}: Hook output exceeded ${maxBytes} bytes`,
    );
    for (const skill of route.skills) {
      assert.ok(
        output.includes(wrapper(skill.name)),
        `${platform}: Hook dropped ${skill.name}`,
      );
    }
  }
});

test("fallback mode does not depend on the manifest", () => {
  const result = routePrompt("新增 SQLite 迁移", { mode: "fallback" });
  assert.equal(result.mode, "fallback");
  assert.equal(result.skip, true);
  assert.deepEqual(result.skills, []);
});

test("planned Skills still require platform support and an installed source", () => {
  const fixtureRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "tauri-skill-router-planned-"),
  );
  const sourcePath = path.join(
    fixtureRoot,
    ".codex/skills/planned-skill/SKILL.md",
  );
  const manifestPath = path.join(fixtureRoot, "manifest.json");
  writeJson(manifestPath, {
    version: 1,
    skills: [
      {
        name: "planned-skill",
        kind: "domain",
        activation: "auto",
        intents: ["implement"],
        layers: ["planned"],
        strongSignals: ["planned signal"],
        weakSignals: [],
        excludeWhen: [],
        mutexGroup: null,
        riskTags: [],
        platforms: ["codex"],
        source: ".codex/skills/planned-skill/SKILL.md",
        managed: "project",
        status: "planned",
      },
    ],
  });
  try {
    const missing = routePrompt("planned signal", {
      manifestPath,
      repoRoot: fixtureRoot,
      platform: "codex",
    });
    assert.deepEqual(missing.skills, []);

    fs.mkdirSync(path.dirname(sourcePath), { recursive: true });
    fs.writeFileSync(sourcePath, "planned", "utf8");
    const installed = routePrompt("planned signal", {
      manifestPath,
      repoRoot: fixtureRoot,
      platform: "codex",
    });
    assert.deepEqual(
      installed.skills.map((skill) => skill.name),
      ["planned-skill"],
    );
    const wrongPlatform = routePrompt("planned signal", {
      manifestPath,
      repoRoot: fixtureRoot,
      platform: "agents",
    });
    assert.deepEqual(wrongPlatform.skills, []);
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
});

test("validator rejects planned status after its source is installed", () => {
  const plannedManifest = structuredClone(manifest);
  const installedSkill = plannedManifest.skills.find(
    (skill) => skill.name === "add-skill",
  );
  assert.ok(installedSkill);
  installedSkill.status = "planned";

  const errors = [];
  const warnings = [];
  const stats = {
    manifestSkills: 0,
    sourceFiles: 0,
    bundles: 0,
    cases: 0,
    matrixRules: 0,
  };
  validateManifest(
    plannedManifest,
    { strictEntryBudget: false },
    errors,
    warnings,
    stats,
  );

  assert.ok(
    errors.includes(
      "add-skill: planned source is already installed; remove status=planned",
    ),
  );
});

test("validator rejects unknown enums and non-canonical project sources", () => {
  function validateMutation(mutator) {
    const value = structuredClone(manifest);
    mutator(
      value.skills.find((skill) => skill.name === "add-skill"),
      value,
    );
    const errors = [];
    validateManifest(value, { strictEntryBudget: false }, errors, [], {
      manifestSkills: 0,
      sourceFiles: 0,
      bundles: 0,
      cases: 0,
      matrixRules: 0,
    });
    return errors;
  }

  assert.ok(
    validateMutation((skill) => {
      skill.kind = "domian";
    }).some((error) => error.includes("unsupported kind domian")),
  );
  assert.ok(
    validateMutation((skill) => {
      skill.activation = "sometimes";
    }).some((error) => error.includes("unsupported activation sometimes")),
  );
  assert.ok(
    validateMutation((skill) => {
      skill.managed = "upsteam";
    }).some((error) => error.includes("unsupported managed upsteam")),
  );
  assert.ok(
    validateMutation((skill) => {
      skill.platforms = ["codxe"];
    }).some((error) => error.includes("unsupported platform codxe")),
  );
  assert.ok(
    validateMutation((skill) => {
      skill.source = ".codex/skills/../../private.env";
    }).some((error) =>
      error.includes("project-managed source must be exactly"),
    ),
  );
  assert.ok(
    validateMutation((_skill, value) => {
      value.version = 0;
    }).some((error) => error.includes("version must be a positive integer")),
  );
  const collectionErrors = validateMutation((skill) => {
    skill.intents = ["implement", 42];
    skill.layers = [""];
    skill.strongSignals = [null];
    skill.weakSignals = ["valid", {}];
    skill.excludeWhen = [" "];
    skill.riskTags = [false];
    skill.mutexGroup = 42;
  });
  for (const field of [
    "intents",
    "layers",
    "strongSignals",
    "weakSignals",
    "excludeWhen",
    "riskTags",
  ]) {
    assert.ok(
      collectionErrors.some((error) =>
        error.includes(`${field} must be an array of non-empty strings`),
      ),
      `missing ${field} element validation`,
    );
  }
  assert.ok(
    collectionErrors.some((error) =>
      error.includes("mutexGroup must be a non-empty string or null"),
    ),
  );
});

test("validator rejects invalid bundle references and matrix rule shapes", () => {
  const errors = [];
  validateRoutingFiles(
    manifest,
    {
      bundles: [
        {
          id: "invalid-bundle",
          when: { signalsAll: ["not-a-context-signal"] },
          required: ["missing-skill"],
          conditional: { "also-unknown": "missing-skill" },
        },
        {
          id: "invalid-types",
          when: { signalsAll: ["react", 42] },
          required: ["ui-frontend", {}],
          conditional: { "": 42 },
        },
        {
          id: "always-true",
          when: { signalsAll: [], signalsAny: [] },
          required: [],
          conditional: {},
        },
      ],
    },
    cases,
    {
      rules: [
        {
          id: "",
          patterns: [],
          required: "not-an-array",
          conditional: [],
        },
      ],
    },
    errors,
    {
      manifestSkills: 0,
      sourceFiles: 0,
      bundles: 0,
      cases: 0,
      matrixRules: 0,
    },
  );
  for (const fragment of [
    "unknown signalsAll signal",
    "signalsAll must be an array of non-empty strings",
    "at least one of signalsAll or signalsAny must be non-empty",
    "required must be an array of non-empty strings",
    "conditional signal must be a non-empty string",
    "conditional Skill must be a non-empty string",
    "unknown required Skill",
    "unknown conditional signal",
    "rule id must be a non-empty string",
    "patterns must be a non-empty string array",
    "required must be a non-empty string array",
    "conditional must be a non-empty string array",
  ]) {
    assert.ok(
      errors.some((error) => error.includes(fragment)),
      `missing validator error: ${fragment}`,
    );
  }
});

test("validator rejects unsupported lifecycle status", () => {
  const invalidStatus = structuredClone(manifest);
  const skill = invalidStatus.skills.find((item) => item.name === "add-skill");
  skill.status = "untrusted-delete";
  const errors = [];
  validateManifest(invalidStatus, { strictEntryBudget: false }, errors, [], {
    manifestSkills: 0,
    sourceFiles: 0,
    bundles: 0,
    cases: 0,
    matrixRules: 0,
  });
  assert.ok(
    errors.some((error) =>
      error.includes("unsupported status untrusted-delete"),
    ),
  );
});

test("formatted hook output stays inside the 1.5 KiB budget", () => {
  const result = routePrompt(
    "发布 1.2.0，生成 dmg，签名并推送 update.json，同时检查凭据和 capabilities",
  );
  const output = formatRouteResult(result, { maxBytes: 1536 });
  assert.ok(Buffer.byteLength(output, "utf8") <= 1536);
  assert.ok(!output.includes("manifest.json"));
});

test("Codex hooks config only uses runtime-supported top-level fields", () => {
  const hooksConfig = JSON.parse(
    fs.readFileSync(path.join(repoRoot, ".codex/hooks.json"), "utf8"),
  );
  assert.deepEqual(Object.keys(hooksConfig).sort(), ["description", "hooks"]);
  assert.equal(typeof hooksConfig.description, "string");
  assert.ok(Array.isArray(hooksConfig.hooks?.UserPromptSubmit));
});

test("ordinary single-domain route P95 is at most three skills", () => {
  const counts = cases
    .filter((item) => item.scope === "single" && item.riskLevel !== "high")
    .map((item) => routePrompt(item.prompt, item.options || {}).skills.length)
    .sort((a, b) => a - b);
  const p95 = counts[Math.max(0, Math.ceil(counts.length * 0.95) - 1)];
  assert.ok(p95 <= 3, `single-domain P95=${p95}`);
});

test("mirror sync checks the complete Skill resource tree", () => {
  const sourceDirectory = path.join(repoRoot, ".codex/skills/add-skill");
  const resources = discoverSourceFiles(sourceDirectory).map((filePath) =>
    path.relative(sourceDirectory, filePath),
  );
  assert.ok(resources.includes("SKILL.md"));
  assert.ok(resources.some((resource) => resource.startsWith("references/")));
});

test("generated Claude command references the synchronized Skill resources", () => {
  const source = [
    "---",
    "name: sample-command",
    "description: sample",
    "---",
    "Read [details](references/details.md) and `references/checklist.md`.",
  ].join("\n");
  const output = commandBody({ name: "sample-command" }, source);
  assert.ok(output.includes("../skills/sample-command/references/details.md"));
  assert.ok(
    output.includes("../skills/sample-command/references/checklist.md"),
  );
});

test("sync check preserves platform-local and unmanaged Skills", () => {
  const fixture = createSyncFixture();
  try {
    const result = syncSkills({
      write: false,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(result.ok, true);
    assert.deepEqual(result.changes, []);
    assert.deepEqual(result.errors, []);
    for (const name of [
      "env-isolation",
      "mobile-app-architecture",
      "remote-gateway",
      "tauri-mobile-android",
    ]) {
      assert.ok(
        result.preserved.some((item) => item.name === name),
        `platform-local ${name} was not preserved`,
      );
      assert.ok(
        result.changes.every((change) => change.name !== name),
        `platform-local ${name} must never be changed`,
      );
    }
  } finally {
    fixture.cleanup();
  }
});

test("sync detects stale resources and requires explicit prune authorization", () => {
  const fixture = createSyncFixture();
  const stalePath = path.join(
    fixture.root,
    ".claude/skills/sample-skill/references/stale.md",
  );
  try {
    fs.writeFileSync(stalePath, "stale", "utf8");
    const checkResult = syncSkills({
      write: false,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(checkResult.ok, false);
    assert.ok(
      checkResult.changes.some(
        (change) =>
          change.action === "extra" && change.target.endsWith("stale.md"),
      ),
    );

    const writeWithoutPrune = syncSkills({
      write: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(writeWithoutPrune.ok, false);
    assert.ok(fs.existsSync(stalePath));
    assert.throws(
      () => parseSyncArgs(["--prune"]),
      /requires explicit --write/,
    );

    const pruneResult = syncSkills({
      write: true,
      prune: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(pruneResult.ok, true);
    assert.ok(!fs.existsSync(stalePath));
    const finalCheck = syncSkills({
      write: false,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(finalCheck.ok, true);
    assert.deepEqual(finalCheck.changes, []);
  } finally {
    fixture.cleanup();
  }
});

test("sync rejects malicious Skill names and resource traversal", () => {
  const fixture = createSyncFixture();
  const outsidePath = path.join(fixture.root, "escape", "SKILL.md");
  try {
    fixture.manifestValue.skills[0].name = "../../escape";
    writeJson(fixture.manifestPath, fixture.manifestValue);
    const result = syncSkills({
      write: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(result.ok, false);
    assert.ok(
      result.errors.some((error) => error.includes("invalid Skill name")),
    );
    assert.ok(!fs.existsSync(outsidePath));
    assert.throws(
      () =>
        targetFor(
          { name: "sample-skill" },
          "claude",
          "../../escape",
          fixture.platformRoots,
        ),
      /invalid Skill resource path/,
    );
  } finally {
    fixture.cleanup();
  }
});

test("sync fails closed for unknown management, kind, and platform values", () => {
  const mutations = [
    {
      apply: (skill) => {
        skill.managed = "upsteam";
      },
      error: "unsupported Skill management",
    },
    {
      apply: (skill) => {
        skill.kind = "domian";
      },
      error: "unsupported Skill kind",
    },
    {
      apply: (skill) => {
        skill.platforms = ["codex", "claudee"];
      },
      error: "unsupported Skill platform",
    },
  ];
  for (const mutation of mutations) {
    const fixture = createSyncFixture();
    try {
      mutation.apply(fixture.manifestValue.skills[0]);
      writeJson(fixture.manifestPath, fixture.manifestValue);
      const result = syncSkills({
        write: true,
        repoRoot: fixture.root,
        manifestPath: fixture.manifestPath,
      });
      assert.equal(result.ok, false);
      assert.ok(result.errors.some((error) => error.includes(mutation.error)));
    } finally {
      fixture.cleanup();
    }
  }
  assert.throws(
    () =>
      validateSkillRecord({
        name: "sample-skill",
        kind: "domain",
        managed: "project",
        platforms: ["codex"],
        source: ".codex/skills/other-skill/SKILL.md",
      }),
    /project source must be exactly/u,
  );
});

test("sync rejects project source traversal without reading or mirroring it", () => {
  const fixture = createSyncFixture();
  const secretPath = path.join(fixture.root, "private.env");
  const claudeTarget = path.join(fixture.root, ".claude/skills/sample-skill");
  const agentsTarget = path.join(fixture.root, ".agents/skills/sample-skill");
  try {
    fs.writeFileSync(secretPath, "TOP_SECRET", "utf8");
    fs.rmSync(claudeTarget, { recursive: true, force: true });
    fs.rmSync(agentsTarget, { recursive: true, force: true });
    fixture.manifestValue.skills[0].source = ".codex/skills/../../private.env";
    writeJson(fixture.manifestPath, fixture.manifestValue);

    const result = syncSkills({
      write: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(result.ok, false);
    assert.ok(
      result.errors.some((error) =>
        error.includes("project source must be exactly"),
      ),
    );
    assert.ok(!fs.existsSync(claudeTarget));
    assert.ok(!fs.existsSync(agentsTarget));
  } finally {
    fixture.cleanup();
  }
});

test("sync rejects untrusted retired tombstones without deleting mirrors", () => {
  const fixture = createSyncFixture();
  const claudeEntry = path.join(
    fixture.root,
    ".claude/skills/sample-skill/SKILL.md",
  );
  const agentsEntry = path.join(
    fixture.root,
    ".agents/skills/sample-skill/SKILL.md",
  );
  const platformLocalEntries = [
    path.join(fixture.root, ".claude/skills/env-isolation/SKILL.md"),
    path.join(fixture.root, ".agents/skills/env-isolation/SKILL.md"),
  ];
  try {
    Object.assign(fixture.manifestValue.skills[0], {
      name: "env-isolation",
      source: ".codex/skills/env-isolation/SKILL.md",
      platforms: ["codex", "claude", "agents"],
      status: "retired",
    });
    writeJson(fixture.manifestPath, fixture.manifestValue);

    const result = syncSkills({
      write: true,
      prune: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(result.ok, false);
    assert.ok(
      result.errors.some((error) => error.includes("unsupported Skill status")),
    );
    assert.ok(fs.existsSync(claudeEntry));
    assert.ok(fs.existsSync(agentsEntry));
    for (const entry of platformLocalEntries) assert.ok(fs.existsSync(entry));
  } finally {
    fixture.cleanup();
  }
});

test("unlisted Skill mirrors and generated commands are always preserved", () => {
  const fixture = createSyncFixture();
  const commandPath = path.join(
    fixture.root,
    ".claude/commands/sample-skill.md",
  );
  try {
    fs.mkdirSync(path.dirname(commandPath), { recursive: true });
    fs.writeFileSync(commandPath, "unlisted command", "utf8");
    fixture.manifestValue.skills = fixture.manifestValue.skills.slice(1);
    writeJson(fixture.manifestPath, fixture.manifestValue);

    const check = syncSkills({
      write: false,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(check.ok, true);
    assert.ok(
      check.preserved.some(
        (item) =>
          item.path === ".claude/skills/sample-skill/SKILL.md" &&
          item.action === "preserve",
      ),
    );
    assert.equal(fs.readFileSync(commandPath, "utf8"), "unlisted command");
  } finally {
    fixture.cleanup();
  }
});

test("renaming creates new mirrors while preserving the unlisted old name", () => {
  const fixture = createSyncFixture();
  const newSource = path.join(fixture.root, ".codex/skills/renamed-skill");
  const newClaudeEntry = path.join(
    fixture.root,
    ".claude/skills/renamed-skill/SKILL.md",
  );
  try {
    fs.mkdirSync(newSource, { recursive: true });
    fs.writeFileSync(
      path.join(newSource, "SKILL.md"),
      "---\nname: renamed-skill\ndescription: renamed\n---\nrenamed\n",
      "utf8",
    );
    fixture.manifestValue.skills[0] = {
      ...fixture.manifestValue.skills[0],
      name: "renamed-skill",
      source: ".codex/skills/renamed-skill/SKILL.md",
    };
    writeJson(fixture.manifestPath, fixture.manifestValue);

    const check = syncSkills({
      write: false,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(check.ok, false);
    assert.ok(
      check.changes.some(
        (change) =>
          change.action === "create" && change.name === "renamed-skill",
      ),
    );
    assert.ok(
      check.preserved.some(
        (item) => item.path === ".claude/skills/sample-skill/SKILL.md",
      ),
    );

    const applied = syncSkills({
      write: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(applied.ok, true);
    assert.ok(fs.existsSync(newClaudeEntry));
    assert.ok(
      fs.existsSync(
        path.join(fixture.root, ".claude/skills/sample-skill/SKILL.md"),
      ),
    );
    const finalCheck = syncSkills({
      write: false,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(finalCheck.ok, true);
  } finally {
    fixture.cleanup();
  }
});

test("writeAtomic removes only its owned temp file after write or rename failure", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "tauri-skill-atomic-"));
  const target = path.join(root, "skills/sample/SKILL.md");
  fs.mkdirSync(path.dirname(target), { recursive: true });
  const tempPath = `${target}.skill-sync-${process.pid}.tmp`;
  try {
    const originalWrite = fs.writeFileSync;
    fs.writeFileSync = function patchedWrite(file, ...args) {
      if (typeof file === "number") throw new Error("simulated write failure");
      return originalWrite.call(this, file, ...args);
    };
    try {
      assert.throws(
        () => writeAtomic(target, "content", root),
        /simulated write failure/u,
      );
    } finally {
      fs.writeFileSync = originalWrite;
    }
    assert.ok(!fs.existsSync(tempPath));

    const originalRename = fs.renameSync;
    fs.renameSync = function patchedRename() {
      throw new Error("simulated rename failure");
    };
    try {
      assert.throws(
        () => writeAtomic(target, "content", root),
        /simulated rename failure/u,
      );
    } finally {
      fs.renameSync = originalRename;
    }
    assert.ok(!fs.existsSync(tempPath));
    assert.ok(!fs.existsSync(target));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("sync refuses a symbolic-link platform root", () => {
  const fixture = createSyncFixture();
  const externalRoot = path.join(fixture.root, "external-claude");
  try {
    fs.rmSync(path.join(fixture.root, ".claude"), {
      recursive: true,
      force: true,
    });
    fs.mkdirSync(externalRoot, { recursive: true });
    fs.symlinkSync(externalRoot, path.join(fixture.root, ".claude"), "dir");
    const result = syncSkills({
      write: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(result.ok, false);
    assert.ok(result.errors.some((error) => error.includes("symbolic link")));
    assert.ok(!fs.existsSync(path.join(externalRoot, "skills/sample-skill")));
  } finally {
    fixture.cleanup();
  }
});

test("sync refuses a symbolic-link managed Skill ancestor", () => {
  const fixture = createSyncFixture();
  const targetDirectory = path.join(
    fixture.root,
    ".claude/skills/sample-skill",
  );
  const externalTarget = path.join(fixture.root, "external-target");
  try {
    fs.rmSync(targetDirectory, { recursive: true, force: true });
    fs.mkdirSync(externalTarget, { recursive: true });
    fs.writeFileSync(path.join(externalTarget, "SKILL.md"), "outside", "utf8");
    fs.symlinkSync(externalTarget, targetDirectory, "dir");
    const result = syncSkills({
      write: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(result.ok, false);
    assert.ok(result.errors.some((error) => error.includes("symbolic link")));
    assert.equal(
      fs.readFileSync(path.join(externalTarget, "SKILL.md"), "utf8"),
      "outside",
    );
  } finally {
    fixture.cleanup();
  }
});

test("sync refuses a symbolic-link source directory", () => {
  const fixture = createSyncFixture();
  const sourceDirectory = path.join(fixture.root, ".codex/skills/sample-skill");
  const externalSource = path.join(fixture.root, "external-source");
  try {
    fs.rmSync(sourceDirectory, { recursive: true, force: true });
    fs.mkdirSync(externalSource, { recursive: true });
    fs.writeFileSync(path.join(externalSource, "SKILL.md"), "outside", "utf8");
    fs.symlinkSync(externalSource, sourceDirectory, "dir");
    const result = syncSkills({
      write: true,
      repoRoot: fixture.root,
      manifestPath: fixture.manifestPath,
    });
    assert.equal(result.ok, false);
    assert.ok(result.errors.some((error) => error.includes("symbolic link")));
  } finally {
    fixture.cleanup();
  }
});

test("prune cannot escape its exact managed Skill directory", () => {
  const fixture = createSyncFixture();
  const targetDirectory = path.join(
    fixture.root,
    ".claude/skills/sample-skill",
  );
  const outsideFile = path.join(fixture.root, ".claude/skills/outside.md");
  try {
    fs.writeFileSync(outsideFile, "keep", "utf8");
    assert.throws(
      () =>
        pruneManagedFile(
          outsideFile,
          targetDirectory,
          fixture.platformRoots.claude,
        ),
      /outside managed target/,
    );
    assert.equal(fs.readFileSync(outsideFile, "utf8"), "keep");
  } finally {
    fixture.cleanup();
  }
});
