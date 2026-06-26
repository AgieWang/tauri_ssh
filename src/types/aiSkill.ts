export type AiSkillScope =
  | "global"
  | "terminal"
  | "sql"
  | "logs"
  | "sftp"
  | "mcp"
  | "jumpserver";

export type AiSkillSource = "builtin" | "user";

export interface AiSkill {
  id: number;
  skillKey: string;
  name: string;
  description: string;
  content: string;
  scopes: AiSkillScope[];
  triggerWords: string[];
  tags: string[];
  priority: number;
  enabled: boolean;
  builtin: boolean;
  source: AiSkillSource;
  sourcePath: string;
  contentHash: string;
  missing: boolean;
  builtinVersion: number;
  userOverridden: boolean;
  allowMcp: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface UpsertAiSkillInput {
  id?: number | null;
  skillKey?: string | null;
  name: string;
  description?: string | null;
  content: string;
  scopes: AiSkillScope[];
  triggerWords?: string[] | null;
  tags?: string[] | null;
  priority?: number | null;
  enabled?: boolean | null;
  allowMcp?: boolean | null;
}

export interface ListAiSkillsInput {
  keyword?: string | null;
  source?: AiSkillSource | "all" | null;
  showBuiltin?: boolean | null;
  scope?: AiSkillScope | null;
}

export interface AiSkillStats {
  total: number;
  user: number;
  builtin: number;
  enabled: number;
}

export interface ListAiSkillsResult {
  items: AiSkill[];
  stats: AiSkillStats;
}

export interface SyncBuiltinAiSkillsResult {
  scanned: number;
  inserted: number;
  updated: number;
  missing: number;
}

export interface AiSkillTriggerInput {
  prompt: string;
  scope?: AiSkillScope | null;
  includeGlobal?: boolean | null;
}

export interface AiSkillMatch {
  skill: AiSkill;
  matchedWords: string[];
  score: number;
}

export interface AiExperienceMatch {
  experience: AiExperience;
  matchedWords: string[];
  score: number;
  summary: string;
}

export interface AiSkillTriggerResult {
  prompt: string;
  scope: AiSkillScope;
  matches: AiSkillMatch[];
  experiences: AiExperienceMatch[];
}

export interface AiSkillPromptPreviewInput {
  prompt?: string | null;
  scope: AiSkillScope;
  includeGlobal?: boolean | null;
}

export interface AiSkillPromptPreviewResult {
  scope: AiSkillScope;
  skills: AiSkill[];
  experiences: AiExperienceMatch[];
  promptFragment: string;
}

export interface AiExperienceRecallInput {
  prompt: string;
  scope?: AiSkillScope | "all" | null;
  limit?: number | null;
}

export interface AiExperience {
  id: number;
  experienceKey: string;
  title: string;
  symptom: string;
  cause: string;
  solution: string;
  scenario: string;
  source: "user" | "ai" | "mcp";
  tags: string[];
  referencesJson: string;
  markdownPath: string;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface UpsertAiExperienceInput {
  id?: number | null;
  experienceKey?: string | null;
  title: string;
  symptom?: string | null;
  cause?: string | null;
  solution?: string | null;
  scenario?: string | null;
  source?: "user" | "ai" | "mcp" | null;
  tags?: string[] | null;
  referencesJson?: string | null;
  markdownPath?: string | null;
  enabled?: boolean | null;
}

export interface AiRunbookStep {
  id: string;
  title: string;
  stepType: "note" | "readonly_command" | "approval_command" | "file" | "sql" | "redis";
  content: string;
  riskLevel: "low" | "medium" | "high" | "blocked";
}

export interface AiRunbook {
  id: number;
  runbookKey: string;
  name: string;
  description: string;
  scenario: string;
  tags: string[];
  steps: AiRunbookStep[];
  enabled: boolean;
  allowMcp: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface UpsertAiRunbookInput {
  id?: number | null;
  runbookKey?: string | null;
  name: string;
  description?: string | null;
  scenario?: string | null;
  tags?: string[] | null;
  steps?: AiRunbookStep[] | null;
  enabled?: boolean | null;
  allowMcp?: boolean | null;
}

export interface RunAiRunbookInput {
  id?: number | null;
  runbookKey?: string | null;
  serverAlias?: string | null;
  databaseConnectionKey?: string | null;
  databaseName?: string | null;
  requester?: string | null;
  dryRun?: boolean | null;
}

export interface AiRunbookStepResult {
  stepId: string;
  title: string;
  stepType: string;
  riskLevel: string;
  status: string;
  message: string;
  output: unknown;
  approvalId?: number | null;
  durationMs: number;
}

export interface AiRunbookRunResult {
  runbook: AiRunbook;
  status: string;
  message: string;
  steps: AiRunbookStepResult[];
}
