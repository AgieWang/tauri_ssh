#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");

const REPO_ROOT = path.resolve(__dirname, "../..");

function platformRootsFor(repoRoot) {
  return {
    codex: path.join(repoRoot, ".codex"),
    claude: path.join(repoRoot, ".claude"),
    agents: path.join(repoRoot, ".agents"),
  };
}

const PLATFORM_ROOTS = platformRootsFor(REPO_ROOT);
const MANIFEST_PATH = path.join(
  REPO_ROOT,
  ".codex/skill-routing/manifest.json",
);
const ALLOWED_KINDS = new Set([
  "domain",
  "safety",
  "workflow-command",
  "terminal-action",
  "platform-local",
]);
const ALLOWED_MANAGEMENT = new Set(["project", "upstream", "platform-local"]);
const ALLOWED_PLATFORMS = new Set(["codex", "claude", "agents"]);
const ALLOWED_STATUSES = new Set(["planned"]);

function parseArgs(argv) {
  const write = argv.includes("--write");
  const prune = argv.includes("--prune");
  if (write && argv.includes("--check"))
    throw new Error("choose either --check or --write");
  if (prune && !write)
    throw new Error("--prune requires explicit --write authorization");
  return { write, prune, json: argv.includes("--json") };
}

function isPathInside(root, target, allowRoot = false) {
  const relative = path.relative(path.resolve(root), path.resolve(target));
  if (relative === "") return allowRoot;
  return !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative);
}

function validateSkillName(name) {
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(String(name || ""))) {
    throw new Error(`invalid Skill name: ${String(name)}`);
  }
}

function isExactProjectSource(skill) {
  return [
    `.codex/skills/${skill.name}/SKILL.md`,
    `.codex/skills/${skill.name}/skill.md`,
  ].includes(skill.source);
}

function validateSkillRecord(skill) {
  validateSkillName(skill?.name);
  if (!ALLOWED_KINDS.has(skill.kind)) {
    throw new Error(`unsupported Skill kind: ${String(skill.kind)}`);
  }
  if (!ALLOWED_MANAGEMENT.has(skill.managed)) {
    throw new Error(`unsupported Skill management: ${String(skill.managed)}`);
  }
  if (!Array.isArray(skill.platforms) || skill.platforms.length === 0) {
    throw new Error("Skill platforms must be a non-empty array");
  }
  for (const platform of skill.platforms) {
    if (!ALLOWED_PLATFORMS.has(platform)) {
      throw new Error(`unsupported Skill platform: ${String(platform)}`);
    }
  }
  if (skill.status !== undefined && !ALLOWED_STATUSES.has(skill.status)) {
    throw new Error(`unsupported Skill status: ${String(skill.status)}`);
  }
  if (skill.managed === "project" && !isExactProjectSource(skill)) {
    throw new Error(
      `project source must be exactly .codex/skills/${skill.name}/SKILL.md or skill.md`,
    );
  }
}

function validateRelativeResource(relativePath) {
  const value = String(relativePath || "");
  const normalized = path.normalize(value);
  if (
    !value ||
    path.isAbsolute(value) ||
    normalized === ".." ||
    normalized.startsWith(`..${path.sep}`)
  ) {
    throw new Error(`invalid Skill resource path: ${value}`);
  }
  return normalized;
}

function assertSafePath(root, target, options = {}) {
  const resolvedRoot = path.resolve(root);
  const resolvedTarget = path.resolve(target);
  if (!isPathInside(resolvedRoot, resolvedTarget, options.allowRoot === true)) {
    throw new Error(`path escapes managed root: ${resolvedTarget}`);
  }
  if (!fs.existsSync(resolvedRoot)) {
    throw new Error(`managed root does not exist: ${resolvedRoot}`);
  }
  const rootStat = fs.lstatSync(resolvedRoot);
  if (rootStat.isSymbolicLink() || !rootStat.isDirectory()) {
    throw new Error(`managed root must be a real directory: ${resolvedRoot}`);
  }
  const realRoot = fs.realpathSync(resolvedRoot);
  let current = resolvedRoot;
  const relative = path.relative(resolvedRoot, resolvedTarget);
  for (const segment of relative.split(path.sep).filter(Boolean)) {
    current = path.join(current, segment);
    if (!fs.existsSync(current)) break;
    const currentStat = fs.lstatSync(current);
    if (currentStat.isSymbolicLink()) {
      throw new Error(`symbolic link is forbidden in managed path: ${current}`);
    }
    const realCurrent = fs.realpathSync(current);
    if (!isPathInside(realRoot, realCurrent, true)) {
      throw new Error(`real path escapes managed root: ${current}`);
    }
  }
  return resolvedTarget;
}

function resolveRepoPath(relativePath, repoRoot = REPO_ROOT) {
  const resolved = path.resolve(repoRoot, relativePath);
  if (!isPathInside(repoRoot, resolved))
    throw new Error(`unsafe path: ${relativePath}`);
  return resolved;
}

function stripFrontmatter(content) {
  return content.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/u, "");
}

function targetFor(
  skill,
  platform,
  relativePath = "SKILL.md",
  platformRoots = PLATFORM_ROOTS,
) {
  validateSkillName(skill.name);
  const resourcePath = validateRelativeResource(relativePath);
  const root = platformRoots[platform];
  if (!root) throw new Error(`unsupported platform ${platform}`);
  const target = path.join(root, "skills", skill.name, resourcePath);
  if (!isPathInside(root, target))
    throw new Error(`unsafe target path: ${target}`);
  return target;
}

function commandTargetFor(skill, platformRoots = PLATFORM_ROOTS) {
  validateSkillName(skill.name);
  return path.join(platformRoots.claude, "commands", `${skill.name}.md`);
}

function discoverSourceFiles(sourceDirectory, safetyRoot = sourceDirectory) {
  assertSafePath(safetyRoot, sourceDirectory, { allowRoot: true });
  const results = [];
  const visit = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const sourcePath = path.join(directory, entry.name);
      if (entry.isSymbolicLink()) {
        throw new Error(
          `symlink is not supported in Skill resources: ${path.relative(safetyRoot, sourcePath)}`,
        );
      }
      if (entry.isDirectory()) visit(sourcePath);
      else if (entry.isFile()) results.push(sourcePath);
    }
  };
  visit(sourceDirectory);
  return results.sort();
}

function findExtraTargetFiles(
  targetDirectory,
  expectedRelativePaths,
  safetyRoot = targetDirectory,
) {
  if (!fs.existsSync(targetDirectory)) return [];
  const expected = new Set(
    [...expectedRelativePaths].map((relativePath) =>
      path.normalize(relativePath),
    ),
  );
  return discoverSourceFiles(targetDirectory, safetyRoot).filter((filePath) => {
    const relativePath = path.normalize(
      path.relative(targetDirectory, filePath),
    );
    return !expected.has(relativePath);
  });
}

function pruneManagedFile(
  filePath,
  targetDirectory,
  platformRoot = targetDirectory,
) {
  const resolvedFile = path.resolve(filePath);
  const resolvedTarget = path.resolve(targetDirectory);
  assertSafePath(platformRoot, resolvedTarget);
  if (!isPathInside(resolvedTarget, resolvedFile)) {
    throw new Error(`refusing to prune outside managed target: ${filePath}`);
  }
  assertSafePath(resolvedTarget, resolvedFile);
  const fileStat = fs.lstatSync(resolvedFile);
  if (!fileStat.isFile() || fileStat.isSymbolicLink()) {
    throw new Error(`refusing to prune non-regular managed file: ${filePath}`);
  }
  assertSafePath(platformRoot, resolvedTarget);
  assertSafePath(resolvedTarget, resolvedFile);
  fs.unlinkSync(resolvedFile);
}

function commandBody(skill, sourceContent) {
  const body = stripFrontmatter(sourceContent);
  return body.replaceAll("references/", `../skills/${skill.name}/references/`);
}

function writeAtomic(filePath, content, managedRoot) {
  assertSafePath(managedRoot, filePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  assertSafePath(managedRoot, path.dirname(filePath), { allowRoot: true });
  const tempPath = `${filePath}.skill-sync-${process.pid}.tmp`;
  assertSafePath(managedRoot, tempPath);
  let descriptor = null;
  let tempCreated = false;
  try {
    descriptor = fs.openSync(tempPath, "wx", 0o600);
    tempCreated = true;
    fs.writeFileSync(descriptor, content);
    fs.fsyncSync(descriptor);
    fs.closeSync(descriptor);
    descriptor = null;
    assertSafePath(managedRoot, tempPath);
    assertSafePath(managedRoot, filePath);
    fs.renameSync(tempPath, filePath);
    tempCreated = false;
    assertSafePath(managedRoot, filePath);
  } catch (error) {
    if (descriptor !== null) {
      try {
        fs.closeSync(descriptor);
      } catch {
        // 保留原始写入错误；精确临时文件仍会在下方尝试清理。
      }
    }
    if (tempCreated && fs.existsSync(tempPath)) {
      try {
        assertSafePath(managedRoot, tempPath);
        const tempStat = fs.lstatSync(tempPath);
        if (!tempStat.isFile() || tempStat.isSymbolicLink()) {
          throw new Error(`temporary path is not a regular file: ${tempPath}`);
        }
        assertSafePath(managedRoot, tempPath);
        fs.unlinkSync(tempPath);
      } catch (cleanupError) {
        throw new Error(
          `${error.message}; failed to clean owned temp file: ${cleanupError.message}`,
        );
      }
    }
    throw error;
  }
}

function discoverUnmanaged(
  manifest,
  repoRoot = REPO_ROOT,
  platformRoots = PLATFORM_ROOTS,
) {
  const managedKeys = new Set();
  for (const skill of manifest.skills) {
    validateSkillName(skill.name);
    for (const platform of skill.platforms || []) {
      if (!platformRoots[platform]) continue;
      const target = targetFor(skill, platform, "SKILL.md", platformRoots);
      managedKeys.add(path.normalize(target));
      managedKeys.add(path.join(path.dirname(target), "skill.md"));
    }
  }
  const unmanaged = [];
  for (const [platform, root] of Object.entries(platformRoots)) {
    assertSafePath(repoRoot, root);
    const skillsRoot = path.join(root, "skills");
    if (!fs.existsSync(skillsRoot)) continue;
    assertSafePath(root, skillsRoot);
    for (const directory of fs.readdirSync(skillsRoot, {
      withFileTypes: true,
    })) {
      if (!directory.isDirectory()) continue;
      for (const fileName of fs.readdirSync(
        path.join(skillsRoot, directory.name),
      )) {
        if (fileName.toLocaleLowerCase("en-US") !== "skill.md") continue;
        const filePath = path.join(skillsRoot, directory.name, fileName);
        if (!managedKeys.has(path.normalize(filePath))) {
          unmanaged.push({
            platform,
            path: path.relative(repoRoot, filePath),
            action: "preserve",
          });
        }
      }
    }
  }
  return unmanaged.sort((a, b) => a.path.localeCompare(b.path));
}

function syncSkills(options = {}) {
  const repoRoot = path.resolve(options.repoRoot || REPO_ROOT);
  const platformRoots = options.platformRoots || platformRootsFor(repoRoot);
  const manifestPath = path.resolve(options.manifestPath || MANIFEST_PATH);
  const changes = [];
  const errors = [];
  const preserved = [];

  try {
    assertSafePath(repoRoot, repoRoot, { allowRoot: true });
    assertSafePath(repoRoot, manifestPath);
  } catch (error) {
    return {
      ok: false,
      mode: options.write ? (options.prune ? "write-prune" : "write") : "check",
      changes,
      errors: [error.message],
      preserved,
    };
  }
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));

  if (!Number.isInteger(manifest.version) || manifest.version <= 0) {
    errors.push("manifest version must be a positive integer");
  }
  if (!Array.isArray(manifest.skills)) {
    errors.push("manifest skills must be an array");
  } else {
    for (const skill of manifest.skills) {
      try {
        validateSkillRecord(skill);
      } catch (error) {
        errors.push(`${skill?.name || "<unnamed>"}: ${error.message}`);
      }
    }
  }
  if (errors.length > 0) {
    return {
      ok: false,
      mode: options.write ? (options.prune ? "write-prune" : "write") : "check",
      changes,
      errors,
      preserved,
    };
  }

  for (const skill of manifest.skills) {
    let sourcePath;
    try {
      sourcePath = resolveRepoPath(skill.source, repoRoot);
      assertSafePath(repoRoot, sourcePath);
    } catch (error) {
      errors.push(`${skill.name}: ${error.message}`);
      continue;
    }

    if (skill.managed === "platform-local") {
      preserved.push({
        name: skill.name,
        source: skill.source,
        reason: "platform-local",
      });
      continue;
    }
    if (!fs.existsSync(sourcePath)) {
      if (skill.status !== "planned")
        errors.push(`${skill.name}: missing source ${skill.source}`);
      continue;
    }
    if (skill.managed === "upstream") {
      preserved.push({
        name: skill.name,
        source: skill.source,
        reason: "upstream",
      });
      continue;
    }

    const sourceDirectory = path.dirname(sourcePath);
    let sourceFiles;
    try {
      sourceFiles = discoverSourceFiles(sourceDirectory, repoRoot);
    } catch (error) {
      errors.push(`${skill.name}: ${error.message}`);
      continue;
    }
    for (const platform of skill.platforms || []) {
      if (platform === "codex") continue;
      const platformRoot = platformRoots[platform];
      if (!platformRoot) {
        errors.push(`${skill.name}: unsupported platform ${platform}`);
        continue;
      }
      try {
        assertSafePath(repoRoot, platformRoot);
      } catch (error) {
        errors.push(`${skill.name}: ${error.message}`);
        continue;
      }
      const expectedResources = new Set();
      for (const resourcePath of sourceFiles) {
        const relativeResource = path.relative(sourceDirectory, resourcePath);
        let target;
        try {
          const safeResource = validateRelativeResource(relativeResource);
          expectedResources.add(safeResource);
          target = targetFor(skill, platform, safeResource, platformRoots);
          assertSafePath(platformRoot, target);
        } catch (error) {
          errors.push(`${skill.name}: ${error.message}`);
          continue;
        }
        const expected = fs.readFileSync(resourcePath);
        const actual = fs.existsSync(target) ? fs.readFileSync(target) : null;
        if (actual !== null && actual.equals(expected)) continue;
        const change = {
          name: skill.name,
          platform,
          target: path.relative(repoRoot, target),
          action: actual === null ? "create" : "update",
          resource: relativeResource,
        };
        changes.push(change);
        if (options.write) {
          try {
            writeAtomic(target, expected, platformRoot);
          } catch (error) {
            errors.push(`${skill.name}: ${error.message}`);
          }
        }
      }
      if (platform === "claude" && skill.kind === "workflow-command") {
        const target = commandTargetFor(skill, platformRoots);
        const expected = commandBody(
          skill,
          fs.readFileSync(sourcePath, "utf8"),
        );
        let actual = null;
        try {
          assertSafePath(platformRoot, target);
          actual = fs.existsSync(target)
            ? fs.readFileSync(target, "utf8")
            : null;
        } catch (error) {
          errors.push(`${skill.name}: ${error.message}`);
          continue;
        }
        if (actual !== expected) {
          changes.push({
            name: skill.name,
            platform,
            target: path.relative(repoRoot, target),
            action: actual === null ? "create" : "update",
            resource: "<generated-command>",
          });
          if (options.write) {
            try {
              writeAtomic(target, expected, platformRoot);
            } catch (error) {
              errors.push(`${skill.name}: ${error.message}`);
            }
          }
        }
      }
      const targetDirectory = path.join(platformRoot, "skills", skill.name);
      let extraFiles = [];
      try {
        extraFiles = findExtraTargetFiles(
          targetDirectory,
          expectedResources,
          platformRoot,
        );
      } catch (error) {
        errors.push(`${skill.name}: ${error.message}`);
      }
      for (const extraFile of extraFiles) {
        const relativeResource = path.relative(targetDirectory, extraFile);
        const change = {
          name: skill.name,
          platform,
          target: path.relative(repoRoot, extraFile),
          action: options.write && options.prune ? "prune" : "extra",
          resource: relativeResource,
        };
        changes.push(change);
        if (options.write && options.prune) {
          try {
            pruneManagedFile(extraFile, targetDirectory, platformRoot);
          } catch (error) {
            errors.push(`${skill.name}: ${error.message}`);
          }
        }
      }
    }
  }

  try {
    preserved.push(...discoverUnmanaged(manifest, repoRoot, platformRoots));
  } catch (error) {
    errors.push(error.message);
  }
  const unresolvedExtras = changes.filter(
    (change) => change.action === "extra",
  ).length;
  return {
    ok:
      errors.length === 0 &&
      (options.write ? unresolvedExtras === 0 : changes.length === 0),
    mode: options.write ? (options.prune ? "write-prune" : "write") : "check",
    changes,
    errors,
    preserved,
  };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const result = syncSkills(options);
  if (options.json)
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  else {
    process.stdout.write(
      `Skill mirror sync (${result.mode}): ${result.ok ? "PASS" : "DRIFT"}\n`,
    );
    for (const change of result.changes) {
      process.stdout.write(
        `${result.mode.startsWith("write") ? "WRITE" : "DRIFT"} ${change.action} ${change.target}\n`,
      );
    }
    for (const error of result.errors) process.stdout.write(`ERROR ${error}\n`);
    for (const item of result.preserved) {
      process.stdout.write(
        `KEEP  ${item.source || item.path} (${item.reason || "unmanaged"})\n`,
      );
    }
  }
  process.exitCode = result.ok ? 0 : 1;
}

if (require.main === module) main();

module.exports = {
  assertSafePath,
  commandBody,
  commandTargetFor,
  discoverSourceFiles,
  discoverUnmanaged,
  findExtraTargetFiles,
  isPathInside,
  parseArgs,
  platformRootsFor,
  pruneManagedFile,
  resolveRepoPath,
  stripFrontmatter,
  syncSkills,
  targetFor,
  validateRelativeResource,
  validateSkillRecord,
  validateSkillName,
  writeAtomic,
};
