#!/usr/bin/env node
// Codex UserPromptSubmit Hook - 输出最小完整 Skill 候选集。
// 路由异常时退回 Frontmatter 评估，不阻断用户任务。

const fs = require("fs");

const MAX_OUTPUT_BYTES = 1536;
const VALID_MODES = new Set(["shadow", "active", "fallback"]);
const FALLBACK_INSTRUCTIONS = `## Skill 评估（降级模式）

路由暂不可用，请基于 Codex 已加载的 \`.codex/skills/\` Frontmatter 完成最小完整集评估：
1. 列出匹配技能及简短理由；无匹配则明确说明
2. 完整读取所有匹配的 \`SKILL.md\` 后再执行任务
3. 根据实际文件与风险补充数据库、安全、测试和浏览器验收，不能因降级省略验证`;

let raw = "";
try {
  raw = fs.readFileSync(0, "utf8");
} catch {
  process.exit(0);
}

let input;
try {
  input = JSON.parse(raw);
} catch {
  process.exit(0);
}

const prompt = typeof input.prompt === "string" ? input.prompt.trim() : "";

if (!prompt) {
  process.exit(0);
}

// 只使用 Hook 的结构化恢复态；普通 Prompt 文本永不作为静默跳过依据。
const recoveryBooleanFields = [
  "resumed",
  "compacted",
  "isResumed",
  "isCompacted",
];
const structuredRecovery = recoveryBooleanFields.some(
  (field) => input[field] === true,
);
const structuredState = [
  input.sessionState,
  input.session_state,
  input.contextState,
  input.context_state,
].find((value) => typeof value === "string");
const structuredStateRecovery = /^(?:resumed|compacted)$/iu.test(
  structuredState || "",
);
if (structuredRecovery || structuredStateRecovery) {
  process.exit(0);
}

// 纯斜杠命令由 Codex 直接运行 Skill；带参数的命令仍需补充安全路由。
const isBareSlashCommand = /^\/[^\/\s]+$/u.test(prompt);
if (isBareSlashCommand) {
  process.exit(0);
}

function writeBounded(text) {
  const content =
    Buffer.byteLength(text, "utf8") <= MAX_OUTPUT_BYTES
      ? text
      : `${Buffer.from(text, "utf8")
          .subarray(0, MAX_OUTPUT_BYTES - 32)
          .toString("utf8")
          .replace(/\uFFFD?$/, "")}\n…（输出已截断）`;
  process.stdout.write(content);
}

function shortText(value, maxLength = 120) {
  const text = String(value || "")
    .replace(/\s+/g, " ")
    .trim();
  return text.length <= maxLength ? text : `${text.slice(0, maxLength - 1)}…`;
}

function renderRoutingOutput(candidates, risks, uncertainties) {
  const verbose = ["## Skill 路由结果", ""];
  if (candidates.length > 0) {
    verbose.push("候选技能（按顺序）：");
    for (const candidate of candidates) {
      verbose.push(
        `- \`${shortText(candidate.name, 48)}\`: ${shortText(candidate.reason, 88) || "命中确定性路由规则"}`,
      );
    }
  } else {
    verbose.push(
      "候选技能：无确定命中；执行中如发现新的技术层或风险，必须重新评估。",
    );
  }
  if (risks.length > 0) {
    verbose.push("", "风险补充：");
    for (const item of risks) verbose.push(`- ${shortText(item, 80)}`);
  }
  if (uncertainties.length > 0) {
    verbose.push("", "不确定项：");
    for (const item of uncertainties) verbose.push(`- ${shortText(item, 80)}`);
  }
  verbose.push(
    "",
    "执行要求：完整读取以上候选的 `.codex/skills/<技能名>/SKILL.md` 后再行动；路由只缩小规则范围，真实代码、配置、数据库、风险与变更文件仍决定实现和验收。",
  );
  const verboseText = verbose.join("\n");
  if (Buffer.byteLength(verboseText, "utf8") <= MAX_OUTPUT_BYTES) {
    return verboseText;
  }

  const compact = ["## Skill 路由结果"];
  compact.push(
    candidates.length > 0
      ? `候选：${candidates.map((candidate) => `\`${candidate.name}\``).join("、")}`
      : "候选：无确定命中",
  );
  if (risks.length > 0) {
    compact.push(
      `风险：${risks.map((item) => shortText(item, 36)).join("；")}`,
    );
  }
  if (uncertainties.length > 0) {
    compact.push(
      `不确定：${uncertainties.map((item) => shortText(item, 32)).join("；")}`,
    );
  }
  compact.push("要求：完整读取全部候选；实际文件与风险决定实现和验收。");
  const compactText = compact.join("\n");
  if (Buffer.byteLength(compactText, "utf8") <= MAX_OUTPUT_BYTES) {
    return compactText;
  }

  const namesOnly = `技能:${candidates
    .map((candidate) => candidate.name)
    .join(",")}`;
  if (Buffer.byteLength(namesOnly, "utf8") > MAX_OUTPUT_BYTES) {
    throw new RangeError("all candidate names exceed the Hook output budget");
  }
  return namesOnly;
}

function fallback(withWarning = false) {
  writeBounded(
    `${withWarning ? "⚠️ Skill 路由异常，已无阻断降级。\n\n" : ""}${FALLBACK_INSTRUCTIONS}`,
  );
}

const requestedMode = String(
  process.env.SKILL_ROUTER_MODE || "active",
).toLowerCase();
const mode = VALID_MODES.has(requestedMode) ? requestedMode : "active";

if (mode === "fallback") {
  fallback();
  process.exit(0);
}

let routing;
try {
  const { routePrompt } = require("../scripts/skill-router.cjs");
  if (typeof routePrompt !== "function") {
    throw new TypeError("routePrompt export is unavailable");
  }
  routing = routePrompt(prompt, { mode, platform: "codex" });
} catch {
  fallback(true);
  process.exit(0);
}

if (!routing || typeof routing !== "object") {
  fallback(true);
  process.exit(0);
}

const isSkipped = routing.skipped === true || routing.skip === true;
if (isSkipped) {
  if (routing.mode === "fallback" || routing.skipReason === "fallback") {
    fallback();
  }
  process.exit(0);
}

// shadow 只计算候选，保持旧的极简 Frontmatter 输出，便于灰度对照。
if (routing.mode === "shadow" || mode === "shadow") {
  fallback();
  process.exit(0);
}

const routedCandidates = Array.isArray(routing.candidates)
  ? routing.candidates
  : routing.skills;
const routedRisks = Array.isArray(routing.riskSupplements)
  ? routing.riskSupplements
  : routing.risks;
const candidates = Array.isArray(routedCandidates) ? routedCandidates : [];
const risks = Array.isArray(routedRisks) ? routedRisks : [];
const uncertainties = Array.isArray(routing.uncertainties)
  ? routing.uncertainties
  : [];

writeBounded(renderRoutingOutput(candidates, risks, uncertainties));
process.exit(0);
