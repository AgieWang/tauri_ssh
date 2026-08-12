#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");

const REPO_ROOT = path.resolve(__dirname, "../..");
const MANIFEST_PATH = path.join(
  REPO_ROOT,
  ".codex/skill-routing/manifest.json",
);
const BUNDLES_PATH = path.join(REPO_ROOT, ".codex/skill-routing/bundles.json");
const CASES_PATH = path.join(
  REPO_ROOT,
  ".codex/tests/skill-routing/cases.json",
);
const MATRIX_PATH = path.join(
  REPO_ROOT,
  ".codex/tests/skill-routing/expected-matrix.json",
);
const REQUIRED_FIELDS = [
  "name",
  "kind",
  "activation",
  "intents",
  "layers",
  "strongSignals",
  "weakSignals",
  "excludeWhen",
  "mutexGroup",
  "riskTags",
  "platforms",
  "source",
  "managed",
];
const ARRAY_FIELDS = [
  "intents",
  "layers",
  "strongSignals",
  "weakSignals",
  "excludeWhen",
  "riskTags",
  "platforms",
];
const ALLOWED_KINDS = new Set([
  "domain",
  "safety",
  "workflow-command",
  "terminal-action",
  "platform-local",
]);
const ALLOWED_ACTIVATIONS = new Set(["auto", "explicit", "risk"]);
const ALLOWED_MANAGEMENT = new Set(["project", "upstream", "platform-local"]);
const ALLOWED_PLATFORMS = new Set(["codex", "claude", "agents"]);
const ALLOWED_STATUSES = new Set(["planned"]);
const KNOWN_ROUTING_SIGNALS = new Set([
  "react",
  "command",
  "serialization",
  "sqlite",
  "migration",
  "filesystem",
  "tauri-plugin",
  "capabilities",
  "updater",
  "packaging",
  "git",
  "release",
  "publish",
  "credentials",
  "remote-write",
  "remote-database",
  "destructive",
  "implement",
  "diagnose",
  "test",
  "design",
  "document",
]);

function parseArgs(argv) {
  return {
    json: argv.includes("--json"),
    strictEntryBudget: argv.includes("--strict-entry-budget"),
  };
}

function readJson(filePath, errors) {
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (error) {
    errors.push(
      `${path.relative(REPO_ROOT, filePath)}: invalid JSON (${error.message})`,
    );
    return null;
  }
}

function resolveRepoPath(relativePath) {
  const resolved = path.resolve(REPO_ROOT, relativePath);
  const prefix = `${REPO_ROOT}${path.sep}`;
  if (resolved !== REPO_ROOT && !resolved.startsWith(prefix)) {
    throw new Error(`path escapes repository: ${relativePath}`);
  }
  return resolved;
}

function isExactProjectSource(skill) {
  if (typeof skill?.name !== "string" || typeof skill?.source !== "string") {
    return false;
  }
  return [
    `.codex/skills/${skill.name}/SKILL.md`,
    `.codex/skills/${skill.name}/skill.md`,
  ].includes(skill.source);
}

function isNonEmptyStringArray(value) {
  return (
    Array.isArray(value) &&
    value.length > 0 &&
    value.every((item) => typeof item === "string" && item.trim().length > 0)
  );
}

function isStringArray(value) {
  return (
    Array.isArray(value) &&
    value.every((item) => typeof item === "string" && item.trim().length > 0)
  );
}

function decodeUtf8(filePath, errors) {
  const bytes = fs.readFileSync(filePath);
  if (
    bytes.length >= 3 &&
    bytes[0] === 0xef &&
    bytes[1] === 0xbb &&
    bytes[2] === 0xbf
  ) {
    errors.push(
      `${path.relative(REPO_ROOT, filePath)}: UTF-8 BOM is forbidden`,
    );
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch (error) {
    errors.push(
      `${path.relative(REPO_ROOT, filePath)}: invalid UTF-8 (${error.message})`,
    );
    return bytes.toString("utf8");
  }
}

function parseFrontmatter(content) {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/u);
  if (!match) return null;
  const name = match[1].match(/^name:\s*([^\r\n]+)$/mu)?.[1]?.trim();
  const descriptionMarker = match[1].match(/^description:\s*(?:\|\s*|\S.*)$/mu);
  return {
    name,
    hasDescription: Boolean(descriptionMarker),
    bytes: Buffer.byteLength(match[0]),
  };
}

function discoverSkillEntrypoints(root) {
  if (!fs.existsSync(root)) return [];
  const results = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    for (const fileName of fs.readdirSync(path.join(root, entry.name))) {
      if (fileName.toLocaleLowerCase("en-US") === "skill.md") {
        results.push(path.join(root, entry.name, fileName));
      }
    }
  }
  return results.sort();
}

function discoverMarkdownFiles(root) {
  if (!fs.existsSync(root)) return [];
  const results = [];
  const visit = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const target = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(target);
      else if (
        entry.isFile() &&
        entry.name.toLocaleLowerCase("en-US").endsWith(".md")
      ) {
        results.push(target);
      }
    }
  };
  visit(root);
  return results.sort();
}

function validateDangerousExamples(errors) {
  const rules = [
    { id: "broad-git-add", pattern: /\bgit\s+add\s+(?:-A\b|\.(?:\s|$))/iu },
    { id: "credential-extraction", pattern: /\bgit\s+credential\s+fill\b/iu },
    { id: "eval-echo", pattern: /\beval\s+echo\b/iu },
  ];
  const safeContext =
    /禁止|不得|不要|不用|严禁|反例|错误(?:做法|示例)?|不执行|不使用|受.*安全边界覆盖|❌|legacy.*(?:禁止|风险)|仅用于识别/iu;
  for (const filePath of discoverMarkdownFiles(
    path.join(REPO_ROOT, ".codex/skills"),
  )) {
    const lines = decodeUtf8(filePath, errors).split(/\r?\n/u);
    lines.forEach((line, index) => {
      for (const rule of rules) {
        if (!rule.pattern.test(line)) continue;
        const context = lines
          .slice(Math.max(0, index - 3), Math.min(lines.length, index + 2))
          .join(" ");
        if (!safeContext.test(context)) {
          errors.push(
            `${path.relative(REPO_ROOT, filePath)}:${index + 1}: unsafe executable example (${rule.id}) lacks an explicit prohibition`,
          );
        }
      }
    });
    lines.forEach((line, index) => {
      if (!/query_row/iu.test(line)) return;
      const end = Math.min(lines.length, index + 12);
      const window = lines.slice(index, end).join("\n");
      const okOffset = window.search(/\.ok\s*\(\s*\)/u);
      if (okOffset < 0) return;
      const context = lines
        .slice(Math.max(0, index - 3), Math.min(lines.length, end + 1))
        .join(" ");
      if (!safeContext.test(context)) {
        const relativeOkLine = window.slice(0, okOffset).split("\n").length - 1;
        errors.push(
          `${path.relative(REPO_ROOT, filePath)}:${index + relativeOkLine + 1}: unsafe executable example (swallowed-query-error) lacks an explicit prohibition`,
        );
      }
    });
  }
}

function validateManifest(manifest, options, errors, warnings, stats) {
  if (!manifest || !Array.isArray(manifest.skills)) {
    errors.push("manifest.json: skills must be an array");
    return;
  }
  if (!Number.isInteger(manifest.version) || manifest.version <= 0) {
    errors.push("manifest.json: version must be a positive integer");
  }
  const names = new Set();
  const strongOwners = new Map();

  for (const skill of manifest.skills) {
    stats.manifestSkills += 1;
    for (const field of REQUIRED_FIELDS) {
      if (!(field in skill))
        errors.push(`${skill.name || "<unnamed>"}: missing ${field}`);
    }
    for (const field of ARRAY_FIELDS) {
      if (!isStringArray(skill[field])) {
        errors.push(
          `${skill.name || "<unnamed>"}: ${field} must be an array of non-empty strings`,
        );
      }
    }
    if (
      skill.mutexGroup !== null &&
      (typeof skill.mutexGroup !== "string" ||
        skill.mutexGroup.trim().length === 0)
    ) {
      errors.push(
        `${skill.name || "<unnamed>"}: mutexGroup must be a non-empty string or null`,
      );
    }
    if (!ALLOWED_KINDS.has(skill.kind)) {
      errors.push(
        `${skill.name || "<unnamed>"}: unsupported kind ${skill.kind}`,
      );
    }
    if (!ALLOWED_ACTIVATIONS.has(skill.activation)) {
      errors.push(
        `${skill.name || "<unnamed>"}: unsupported activation ${skill.activation}`,
      );
    }
    if (!ALLOWED_MANAGEMENT.has(skill.managed)) {
      errors.push(
        `${skill.name || "<unnamed>"}: unsupported managed ${skill.managed}`,
      );
    }
    if (!isNonEmptyStringArray(skill.platforms)) {
      errors.push(`${skill.name || "<unnamed>"}: platforms must not be empty`);
    } else {
      for (const platform of skill.platforms) {
        if (!ALLOWED_PLATFORMS.has(platform)) {
          errors.push(
            `${skill.name || "<unnamed>"}: unsupported platform ${platform}`,
          );
        }
      }
    }
    if (skill.status !== undefined && !ALLOWED_STATUSES.has(skill.status)) {
      errors.push(
        `${skill.name || "<unnamed>"}: unsupported status ${skill.status}`,
      );
    }
    if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(skill.name || "")) {
      errors.push(`${skill.name || "<unnamed>"}: name must be kebab-case`);
    }
    if (names.has(skill.name))
      errors.push(`${skill.name}: duplicate manifest entry`);
    names.add(skill.name);
    if (
      ["workflow-command", "terminal-action"].includes(skill.kind) &&
      skill.activation === "auto"
    ) {
      errors.push(`${skill.name}: ${skill.kind} cannot use auto activation`);
    }
    if (skill.managed === "project" && !isExactProjectSource(skill)) {
      errors.push(
        `${skill.name}: project-managed source must be exactly .codex/skills/${skill.name}/SKILL.md or skill.md`,
      );
    }

    let sourcePath;
    try {
      sourcePath = resolveRepoPath(skill.source);
    } catch (error) {
      errors.push(`${skill.name}: ${error.message}`);
      continue;
    }
    if (!fs.existsSync(sourcePath)) {
      if (skill.status === "planned")
        warnings.push(`${skill.name}: planned source is not installed yet`);
      else errors.push(`${skill.name}: missing source ${skill.source}`);
      continue;
    }
    if (skill.status === "planned") {
      errors.push(
        `${skill.name}: planned source is already installed; remove status=planned`,
      );
    }

    stats.sourceFiles += 1;
    const content = decodeUtf8(sourcePath, errors);
    if (content.includes("\uFFFD"))
      errors.push(`${skill.source}: contains Unicode replacement character`);
    const frontmatter = parseFrontmatter(content);
    if (!frontmatter) errors.push(`${skill.source}: missing YAML frontmatter`);
    else {
      if (frontmatter.name !== skill.name) {
        errors.push(
          `${skill.source}: frontmatter name ${frontmatter.name || "<missing>"} != ${skill.name}`,
        );
      }
      if (!frontmatter.hasDescription)
        errors.push(`${skill.source}: description must use a multiline block`);
    }
    const lines = content.split(/\r?\n/u).length;
    if (lines > 220 && skill.managed === "project") {
      const message = `${skill.source}: ${lines} lines exceeds the 220-line entry budget`;
      if (options.strictEntryBudget) errors.push(message);
      else warnings.push(message);
    }
    for (const reference of content.matchAll(
      /\((references\/[^)#\s]+\.md)(?:#[^)]+)?\)/gu,
    )) {
      const target = path.resolve(path.dirname(sourcePath), reference[1]);
      if (!fs.existsSync(target))
        errors.push(`${skill.source}: missing reference ${reference[1]}`);
    }
    for (const signal of skill.strongSignals || []) {
      if (typeof signal !== "string" || signal.trim().length === 0) continue;
      const normalized = signal.normalize("NFKC").toLocaleLowerCase("zh-CN");
      if (!strongOwners.has(normalized)) strongOwners.set(normalized, []);
      strongOwners.get(normalized).push(skill.name);
    }
  }

  const codexRoot = path.join(REPO_ROOT, ".codex/skills");
  for (const entrypoint of discoverSkillEntrypoints(codexRoot)) {
    const name = path.basename(path.dirname(entrypoint));
    if (!names.has(name))
      errors.push(
        `${path.relative(REPO_ROOT, entrypoint)}: missing manifest entry`,
      );
  }
  for (const [signal, owners] of strongOwners) {
    const uniqueOwners = [...new Set(owners)].sort();
    const allowedOwners = [
      ...(manifest.allowedStrongSignalOverlaps?.[signal] || []),
    ].sort();
    if (
      uniqueOwners.length > 1 &&
      JSON.stringify(uniqueOwners) !== JSON.stringify(allowedOwners)
    )
      warnings.push(
        `strong signal "${signal}" is shared by ${uniqueOwners.join(", ")}`,
      );
  }
}

function validateRoutingFiles(
  manifest,
  bundleConfig,
  cases,
  matrix,
  errors,
  stats,
) {
  const activeSkillNames = new Set(
    (manifest?.skills || []).map((skill) => skill.name),
  );
  if (!bundleConfig || !Array.isArray(bundleConfig.bundles)) {
    errors.push("bundles.json: bundles must be an array");
  } else {
    const ids = new Set();
    for (const bundle of bundleConfig.bundles) {
      if (typeof bundle.id !== "string" || bundle.id.trim().length === 0) {
        errors.push("bundles.json: bundle missing id");
        continue;
      }
      if (ids.has(bundle.id))
        errors.push(`bundles.json: duplicate id ${bundle.id}`);
      ids.add(bundle.id);
      const all = bundle.when?.signalsAll;
      const any = bundle.when?.signalsAny;
      if (!Array.isArray(all) && !Array.isArray(any)) {
        errors.push(`${bundle.id}: when must declare signalsAll or signalsAny`);
      }
      for (const [field, signals] of [
        ["signalsAll", all],
        ["signalsAny", any],
      ]) {
        if (signals === undefined) continue;
        if (!isStringArray(signals)) {
          errors.push(
            `${bundle.id}: ${field} must be an array of non-empty strings`,
          );
          continue;
        }
        for (const signal of signals) {
          if (!KNOWN_ROUTING_SIGNALS.has(signal)) {
            errors.push(
              `${bundle.id}: unknown ${field} signal ${String(signal)}`,
            );
          }
        }
      }
      const validAll = isStringArray(all) ? all : [];
      const validAny = isStringArray(any) ? any : [];
      if (validAll.length === 0 && validAny.length === 0) {
        errors.push(
          `${bundle.id}: at least one of signalsAll or signalsAny must be non-empty`,
        );
      }
      if (!isStringArray(bundle.required)) {
        errors.push(
          `${bundle.id}: required must be an array of non-empty strings`,
        );
      } else {
        for (const name of bundle.required) {
          if (!activeSkillNames.has(name)) {
            errors.push(`${bundle.id}: unknown required Skill ${String(name)}`);
          }
        }
      }
      if (
        !bundle.conditional ||
        typeof bundle.conditional !== "object" ||
        Array.isArray(bundle.conditional)
      ) {
        errors.push(`${bundle.id}: conditional must be an object`);
      } else {
        for (const [signal, name] of Object.entries(bundle.conditional)) {
          if (signal.trim().length === 0) {
            errors.push(
              `${bundle.id}: conditional signal must be a non-empty string`,
            );
          } else if (!KNOWN_ROUTING_SIGNALS.has(signal)) {
            errors.push(`${bundle.id}: unknown conditional signal ${signal}`);
          }
          if (typeof name !== "string" || name.trim().length === 0) {
            errors.push(
              `${bundle.id}: conditional Skill must be a non-empty string`,
            );
          } else if (!activeSkillNames.has(name)) {
            errors.push(
              `${bundle.id}: unknown conditional Skill ${String(name)}`,
            );
          }
        }
      }
    }
    stats.bundles = bundleConfig.bundles.length;
  }
  if (!Array.isArray(cases) || cases.length < 80)
    errors.push("cases.json: at least 80 cases are required");
  else stats.cases = cases.length;
  if (!matrix || !Array.isArray(matrix.rules) || matrix.rules.length === 0) {
    errors.push("expected-matrix.json: rules must be a non-empty array");
  } else {
    const matrixIds = new Set();
    for (const rule of matrix.rules) {
      if (typeof rule.id !== "string" || rule.id.trim().length === 0) {
        errors.push("expected-matrix.json: rule id must be a non-empty string");
      } else if (matrixIds.has(rule.id)) {
        errors.push(`expected-matrix.json: duplicate rule id ${rule.id}`);
      } else {
        matrixIds.add(rule.id);
      }
      for (const field of ["patterns", "required"]) {
        if (!isNonEmptyStringArray(rule[field])) {
          errors.push(
            `expected-matrix.json: ${rule.id || "<unnamed>"} ${field} must be a non-empty string array`,
          );
        }
      }
      if (
        rule.conditional !== undefined &&
        !isNonEmptyStringArray(rule.conditional)
      ) {
        errors.push(
          `expected-matrix.json: ${rule.id || "<unnamed>"} conditional must be a non-empty string array when present`,
        );
      }
    }
    stats.matrixRules = matrix.rules.length;
  }
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const errors = [];
  const warnings = [];
  const stats = {
    manifestSkills: 0,
    sourceFiles: 0,
    bundles: 0,
    cases: 0,
    matrixRules: 0,
  };
  const manifest = readJson(MANIFEST_PATH, errors);
  const bundles = readJson(BUNDLES_PATH, errors);
  const cases = readJson(CASES_PATH, errors);
  const matrix = readJson(MATRIX_PATH, errors);
  validateManifest(manifest, options, errors, warnings, stats);
  validateRoutingFiles(manifest, bundles, cases, matrix, errors, stats);
  validateDangerousExamples(errors);

  const result = { ok: errors.length === 0, errors, warnings, stats };
  if (options.json)
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  else {
    process.stdout.write(`Skill validation: ${result.ok ? "PASS" : "FAIL"}\n`);
    process.stdout.write(
      `Sources ${stats.sourceFiles}/${stats.manifestSkills}, bundles ${stats.bundles}, cases ${stats.cases}, matrix rules ${stats.matrixRules}\n`,
    );
    for (const error of errors) process.stdout.write(`ERROR ${error}\n`);
    for (const warning of warnings) process.stdout.write(`WARN  ${warning}\n`);
  }
  process.exitCode = result.ok ? 0 : 1;
}

if (require.main === module) main();

module.exports = {
  discoverMarkdownFiles,
  discoverSkillEntrypoints,
  isExactProjectSource,
  parseFrontmatter,
  validateDangerousExamples,
  validateManifest,
  validateRoutingFiles,
};
