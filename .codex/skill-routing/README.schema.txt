Tauri SSH Skill routing schema, version 1

Purpose
- Keep routing metadata deterministic and small.
- Never replace repository inspection, tests, database checks, security review,
  builds, or browser acceptance.
- Select the smallest complete Skill set; high-risk routes have no hard cap.

manifest.json
- version: positive integer schema version.
- sourceOfTruth: project-owned Skill root. It does not imply that every platform
  must have the same complete inventory.
- allowedStrongSignalOverlaps: normalized strong signal to the exact Skill set
  that intentionally composes for that signal. Unlisted overlap remains a warning.
- skills: unique Skill records.

Skill record
- name: kebab-case Skill name.
- kind: domain, safety, workflow-command, terminal-action, or platform-local.
- activation: auto, explicit, or risk.
- intents: normalized task intents used as secondary evidence.
- layers: technical layers used as secondary evidence.
- strongSignals: domain-specific prompt fragments; one match can select a Skill.
- weakSignals: broad fragments; never sufficient without compatible context.
- excludeWhen: prompt fragments that suppress an otherwise ambiguous match.
- mutexGroup: only the highest scoring member survives unless the router declares
  a documented composition exception.
- riskTags: risk categories that require conservative routing.
- platforms: codex, claude, and/or agents.
- source: repository-relative canonical entry path. SKILL.md and skill.md are
  both supported during compatibility migration. Project-managed sources must
  exactly match .codex/skills/<same-name>/SKILL.md or skill.md.
- managed: project, upstream, or platform-local.
- status: planned is the only supported status. It permits a deliberately
  reserved source path to be absent and must be removed once installed.

Management rules
- managed=project: .codex source is canonical; sync may check or write declared
  mirrors only.
- managed=upstream: sync checks existence but never overwrites official content.
- managed=platform-local: report and preserve. Sync never overwrites or deletes it.
- Files not listed in the manifest are always preserved.
- --check is the default for sync-skills.cjs; --write creates or updates only
  declared managed mirrors and never deletes files.
- Extra files under an exact managed mirror directory are DRIFT. They remain in
  place in --check and --write modes.
- Deleting those exact extra managed files requires explicit --write --prune.
  Prune never touches platform-local, upstream, or unlisted Skill directories.
- Sync never infers whole-Skill deletion from a new or removed Manifest record.
  Old mirrors and generated commands become unlisted and remain preserved.
- Deleting or renaming a whole Skill requires separate user authorization and a
  reviewed list of exact old source, mirror, and command files. Remove those
  targets one by one with safe file operations; never use an unlisted-directory scan.
- Concurrent --write/--prune sync processes are unsupported. Run one sync writer
  at a time; path and symlink checks do not replace process-level coordination.

Routing priority
1. Skip resumed or compacted prompts when the caller provides that state.
2. fallback mode returns no candidates and lets the Hook use its compact fallback.
3. Explicit /command or $skill selects that workflow only, then adds mandatory
   safety guards if the same prompt contains a high-risk operation.
4. Strong signals select domain Skills.
5. Weak signals require matching inferred intent or technical layer.
6. Bundles fill required cross-layer or safety coverage.
7. Exclusions and mutex groups remove known false positives.

bundles.json
- id: unique bundle identifier.
- when.signalsAll: every inferred signal must be present.
- when.signalsAny: at least one inferred signal must be present.
- required: unconditional additions when the condition matches.
- conditional: inferred signal to Skill mapping.

Failure behavior
- Router errors are surfaced to tests.
- Hooks catch errors and fall back to the compact evaluation process.
- Hook output contains candidates and short reasons, never the full manifest.
