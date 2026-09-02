/** 新工作台的仓库版本策略；内部值与 Rust serde 枚举保持一致。 */
export type KnowledgeVersionStrategy =
  "manual" | "tag_or_branch" | "branch" | "tag";

export type KnowledgeGitRefType = "branch" | "tag" | "commit";

export interface RepositoryBindingInput {
  workspaceKey: string;
  alias?: string | null;
  role?: string | null;
  defaultBranch?: string | null;
  /** 省略时由后端采用 manual，避免前端猜测默认分支。 */
  versionStrategy?: KnowledgeVersionStrategy;
}

export interface KnowledgeRepositoryBindingInput {
  projectId: number;
  repositories?: RepositoryBindingInput[];
}

export interface KnowledgeRepositoryBinding {
  id: number;
  projectId: number;
  workspaceKey: string;
  alias: string;
  repositoryRole: string;
  defaultBranch: string;
  versionStrategy: KnowledgeVersionStrategy;
  enabled: boolean;
  deletedAt: string | null;
}

export interface KnowledgeRepositoryAvailability {
  repositoryBindingId: number;
  workspaceKey: string;
  available: boolean;
  branch: string;
  headCommit: string;
  dirty: boolean;
  changedFileCount: number;
  message: string;
}

export interface ProjectVersionRepositoryRefInput {
  repositoryBindingId: number;
  refType: KnowledgeGitRefType;
  refName: string;
  excluded?: boolean;
}

export interface KnowledgeProjectVersionManifestInput {
  projectId: number;
  version: string;
  repositories?: ProjectVersionRepositoryRefInput[];
}

export interface KnowledgeReleaseRepositoryManifest {
  id: number;
  releaseId: number;
  repositoryBindingId: number;
  requestedRefType: KnowledgeGitRefType;
  requestedRefName: string;
  resolvedCommitSha: string;
  captureKind: string;
  inclusionStatus: string;
  exclusionReason: string;
  worktreeDirty: boolean;
  capturedAt: string | null;
}

export interface KnowledgeProjectVersionManifestResult {
  releaseId: number;
  projectId: number;
  version: string;
  status: string;
  repositories: KnowledgeReleaseRepositoryManifest[];
}

export interface KnowledgeProjectVersionStageCompleteness {
  stage: string;
  label: string;
  status: "ready" | "partial" | "pending" | "not_started";
  completedCount: number;
  totalCount: number;
  summary: string;
}

export interface KnowledgeProjectVersionCompleteness {
  releaseId: number;
  projectId: number;
  version: string;
  status: "ready" | "partial";
  stages: KnowledgeProjectVersionStageCompleteness[];
}

export interface KnowledgeProjectVersionBackfillInput {
  releaseId: number;
}
