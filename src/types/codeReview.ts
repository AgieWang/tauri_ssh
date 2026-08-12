export interface CodeReviewChangedFile {
  path: string;
  status: string;
  additions: number;
  deletions: number;
}

export interface CodeReviewCommit {
  hash: string;
  author: string;
  date: string;
  message: string;
}

export type CodeReviewTaskStatus =
  | "draft"
  | "diff_ready"
  | "reviewing"
  | "review_ready"
  | "merge_pending"
  | "merged"
  | "merge_failed"
  | "conflict"
  | "stale"
  | "cancelled";

export type CodeReviewRiskLevel =
  "unknown" | "low" | "medium" | "high" | "critical";

export type CodeReviewPushStatus =
  "not_requested" | "pushing" | "pushed" | "push_failed";

export interface CodeReviewTask {
  id: number;
  taskKey: string;
  workspaceKey: string;
  workspaceName: string;
  repoPath: string;
  sourceBranch: string;
  targetBranch: string;
  status: CodeReviewTaskStatus;
  riskLevel: CodeReviewRiskLevel;
  mergeBase: string;
  sourceHead: string;
  targetHead: string;
  pushStatus: CodeReviewPushStatus;
  diffStat: Record<string, unknown>;
  changedFiles: CodeReviewChangedFile[];
  commits: CodeReviewCommit[];
  diffExcerpt: unknown;
  aiProvider: string;
  aiModel: string;
  aiReviewMarkdown: string;
  aiReviewJson: Record<string, unknown>;
  batchKey: string;
  errorMessage: string;
  createdAt: string;
  updatedAt: string;
  mergedAt?: string | null;
}

export interface CreateCodeReviewTaskInput {
  workspaceKey: string;
  sourceBranch: string;
  targetBranch: string;
  batchKey?: string;
}

export interface CreateCodeReviewBatchTaskItem {
  workspaceKey: string;
  projectName: string;
  sourceBranch: string;
  targetBranch: string;
}

export interface CreateCodeReviewBatchTasksInput {
  batchKey: string;
  items: CreateCodeReviewBatchTaskItem[];
}

export interface ListCodeReviewTasksInput {
  workspaceKey?: string;
  status?: string;
  keyword?: string;
  limit?: number;
}

export interface RunCodeReviewAiInput {
  taskKey: string;
  providerKey?: string;
}

export interface ParseCodeReviewBatchInput {
  rawText: string;
}

export interface CodeReviewBatchItem {
  projectName: string;
  sourceBranch: string;
  targetBranch: string;
  group: string;
  confidence: number;
  matchedWorkspaceKey?: string | null;
  status: string;
  warnings: string[];
}

export interface CodeReviewBatchParseResult {
  batchKey: string;
  items: CodeReviewBatchItem[];
  warnings: string[];
}
