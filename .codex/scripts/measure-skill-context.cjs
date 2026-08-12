#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");
const { performance } = require("node:perf_hooks");
const { routePrompt } = require("./skill-router.cjs");

const REPO_ROOT = path.resolve(__dirname, "../..");
const PLATFORM_DIRS = [".codex/skills", ".claude/skills", ".agents/skills"];

function percentile(values, ratio) {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(sorted.length * ratio) - 1)];
}

function splitFrontmatter(content) {
  const match = content.match(/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/u);
  const frontmatter = match ? match[0] : "";
  return {
    frontmatterBytes: Buffer.byteLength(frontmatter, "utf8"),
    bodyBytes:
      Buffer.byteLength(content, "utf8") -
      Buffer.byteLength(frontmatter, "utf8"),
  };
}

function discoverEntries(relativeRoot) {
  const root = path.join(REPO_ROOT, relativeRoot);
  if (!fs.existsSync(root)) return [];
  const entries = [];
  for (const directory of fs.readdirSync(root, { withFileTypes: true })) {
    if (!directory.isDirectory()) continue;
    const fileName = fs
      .readdirSync(path.join(root, directory.name))
      .find((name) => name.toLocaleLowerCase("en-US") === "skill.md");
    if (!fileName) continue;
    const filePath = path.join(root, directory.name, fileName);
    const content = fs.readFileSync(filePath, "utf8");
    const parts = splitFrontmatter(content);
    entries.push({
      name: directory.name,
      path: path.relative(REPO_ROOT, filePath),
      bytes: Buffer.byteLength(content, "utf8"),
      lines: content.split(/\r?\n/u).length,
      ...parts,
    });
  }
  return entries;
}

function measure(options = {}) {
  const directories = {};
  for (const relativeRoot of PLATFORM_DIRS) {
    const entries = discoverEntries(relativeRoot);
    directories[relativeRoot] = {
      files: entries.length,
      bytes: entries.reduce((sum, entry) => sum + entry.bytes, 0),
      lines: entries.reduce((sum, entry) => sum + entry.lines, 0),
      frontmatterBytes: entries.reduce(
        (sum, entry) => sum + entry.frontmatterBytes,
        0,
      ),
      bodyBytes: entries.reduce((sum, entry) => sum + entry.bodyBytes, 0),
      entryBytesP50: percentile(
        entries.map((entry) => entry.bytes),
        0.5,
      ),
      entryBytesP95: percentile(
        entries.map((entry) => entry.bytes),
        0.95,
      ),
      entryLinesP50: percentile(
        entries.map((entry) => entry.lines),
        0.5,
      ),
      entryLinesP95: percentile(
        entries.map((entry) => entry.lines),
        0.95,
      ),
    };
  }

  const manifest = JSON.parse(
    fs.readFileSync(
      path.join(REPO_ROOT, ".codex/skill-routing/manifest.json"),
      "utf8",
    ),
  );
  const byName = new Map(manifest.skills.map((skill) => [skill.name, skill]));
  const cases = JSON.parse(
    fs.readFileSync(
      path.join(REPO_ROOT, ".codex/tests/skill-routing/cases.json"),
      "utf8",
    ),
  );
  const routeMeasurements = [];
  const repeats = options.repeats || 10;
  for (const testCase of cases) {
    let result;
    const durations = [];
    for (let index = 0; index < repeats; index += 1) {
      const started = performance.now();
      result = routePrompt(testCase.prompt, testCase.options || {});
      durations.push(performance.now() - started);
    }
    let selectedBytes = 0;
    for (const selected of result.skills) {
      const skill = byName.get(selected.name);
      if (!skill) continue;
      const sourcePath = path.join(REPO_ROOT, skill.source);
      if (fs.existsSync(sourcePath))
        selectedBytes += fs.statSync(sourcePath).size;
    }
    routeMeasurements.push({
      id: testCase.id,
      skillCount: result.skills.length,
      selectedBytes,
      p95Ms: Number(percentile(durations, 0.95).toFixed(3)),
    });
  }

  return {
    measuredAt: new Date().toISOString(),
    units: { context: "UTF-8 bytes (not tokens)", time: "milliseconds" },
    directories,
    routing: {
      cases: routeMeasurements.length,
      skillCountP50: percentile(
        routeMeasurements.map((item) => item.skillCount),
        0.5,
      ),
      skillCountP95: percentile(
        routeMeasurements.map((item) => item.skillCount),
        0.95,
      ),
      selectedBytesP50: percentile(
        routeMeasurements.map((item) => item.selectedBytes),
        0.5,
      ),
      selectedBytesP95: percentile(
        routeMeasurements.map((item) => item.selectedBytes),
        0.95,
      ),
      latencyP95Ms: percentile(
        routeMeasurements.map((item) => item.p95Ms),
        0.95,
      ),
      samples: routeMeasurements,
    },
  };
}

function main() {
  const json = process.argv.includes("--json");
  const result = measure();
  if (json) {
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    return;
  }
  for (const [directory, stats] of Object.entries(result.directories)) {
    process.stdout.write(
      `${directory}: ${stats.files} files, ${stats.bytes} bytes, ${stats.lines} lines, entry P50/P95 ${stats.entryBytesP50}/${stats.entryBytesP95} bytes\n`,
    );
  }
  process.stdout.write(
    `routing: ${result.routing.cases} cases, skills P50/P95 ${result.routing.skillCountP50}/${result.routing.skillCountP95}, selected bytes P50/P95 ${result.routing.selectedBytesP50}/${result.routing.selectedBytesP95}, latency P95 ${result.routing.latencyP95Ms} ms\n`,
  );
  process.stdout.write(
    "Token counts are intentionally not estimated from bytes.\n",
  );
}

if (require.main === module) main();

module.exports = { discoverEntries, measure, percentile, splitFrontmatter };
