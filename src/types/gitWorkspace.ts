export type GitWorkspaceStatus = "clean" | "dirty" | "ahead" | "behind" | "diverged" | "unknown";

export interface GitWorkspace {
  id: number;
  workspaceKey: string;
  name: string;
  repoPath: string;
  credentialKey: string;
  branch: string;
  remoteUrl: string;
  status: GitWorkspaceStatus | string;
  changedFiles: number;
  ahead: number;
  behind: number;
  description: string;
  lastScannedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ListGitWorkspacesInput {
  keyword?: string;
  credentialKey?: string;
}

export interface UpsertGitWorkspaceInput {
  id?: number;
  workspaceKey: string;
  name: string;
  repoPath: string;
  credentialKey?: string;
  description?: string;
}

export interface ScanGitWorkspaceRootInput {
  rootPath: string;
  credentialKey?: string;
}

export interface ScanGitWorkspaceRootResult {
  workspaces: GitWorkspace[];
  discovered: number;
  scannedEntries: number;
  skippedEntries: number;
  limited: boolean;
  message: string;
}

export interface GitWorkspaceScanStartResult {
  jobId: string;
  status: string;
  message: string;
}

export interface GitWorkspaceScanJobStatus {
  jobId: string;
  status: "running" | "completed" | "failed" | string;
  message: string;
  startedAt: string;
  finishedAt: string | null;
  result: ScanGitWorkspaceRootResult | null;
  error: string | null;
}

export interface GitWorkspaceDetail {
  workspace: GitWorkspace;
  statusText: string;
  recentLog: string[];
}

export interface AiCommitGitWorkspaceInput {
  workspaceKey: string;
}

export interface AiCommitGitWorkspaceResult {
  workspace: GitWorkspace;
  commitMessage: string;
  commitHash: string;
  providerName: string;
  model: string;
}

export interface GitWorkspaceStatusResult {
  workspace: GitWorkspace;
  porcelain: string;
  stagedFiles: string[];
  unstagedFiles: string[];
  untrackedFiles: string[];
}

export interface GitWorkspaceDiffInput {
  workspaceKey: string;
  staged?: boolean;
  path?: string;
  maxChars?: number;
}

export interface GitWorkspaceDiffResult {
  workspaceKey: string;
  staged: boolean;
  path?: string | null;
  diff: string;
  truncated: boolean;
}

export interface StageGitWorkspaceFilesInput {
  workspaceKey: string;
  paths: string[];
}

export interface CommitGitWorkspaceInput {
  workspaceKey: string;
  message: string;
  paths?: string[];
}

export interface CommitGitWorkspaceResult {
  workspace: GitWorkspace;
  commitMessage: string;
  commitHash: string;
}

export interface GitWorkspaceBranch {
  name: string;
  displayName: string;
  isCurrent: boolean;
  isRemote: boolean;
  lastCommitHash: string;
  lastCommitMessage: string;
  lastCommitAt: string;
}

export interface SwitchGitWorkspaceBranchInput {
  workspaceKey: string;
  branch: string;
}

export interface MergeGitWorkspaceBranchInput {
  workspaceKey: string;
  sourceBranch: string;
  targetBranch: string;
}
