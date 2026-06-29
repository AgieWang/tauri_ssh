export type SecureCredentialProvider =
  | "github"
  | "gitlab"
  | "gitcode"
  | "gitee"
  | "http_api"
  | "custom";

export type SecureCredentialType =
  | "token"
  | "api_key"
  | "bearer_token"
  | "basic_auth"
  | "custom_secret"
  | "session_reference";

export type SecureCredentialStatus =
  | "active"
  | "disabled"
  | "rotation_due"
  | "expired"
  | "test_failed";

export type SecureCredentialApprovalPolicy =
  | "readonly_auto"
  | "write_requires_approval"
  | "all_requires_approval"
  | "blocked_for_mcp";

export interface SecureCredential {
  id: number;
  credentialKey: string;
  displayName: string;
  provider: SecureCredentialProvider;
  credentialType: SecureCredentialType;
  accountName: string;
  baseUrl: string;
  scopes: string[];
  tags: string[];
  folder: string;
  description: string;
  status: SecureCredentialStatus;
  enabled: boolean;
  allowMcp: boolean;
  approvalPolicy: SecureCredentialApprovalPolicy;
  expiresAt: string | null;
  lastUsedAt: string | null;
  usageCount: number;
  hasSecret: boolean;
  secretMasked: string | null;
  rotatedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ListSecureCredentialsInput {
  keyword?: string;
  provider?: SecureCredentialProvider | "";
  status?: SecureCredentialStatus | "";
  allowMcp?: boolean;
}

export interface UpsertSecureCredentialInput {
  id?: number;
  credentialKey: string;
  displayName: string;
  provider: SecureCredentialProvider;
  credentialType: SecureCredentialType;
  accountName?: string;
  baseUrl?: string;
  scopes: string[];
  tags: string[];
  folder?: string;
  description?: string;
  status?: SecureCredentialStatus;
  enabled?: boolean;
  allowMcp?: boolean;
  approvalPolicy?: SecureCredentialApprovalPolicy;
  expiresAt?: string | null;
  secret?: string | null;
}

export interface RotateSecureCredentialInput {
  credentialKey: string;
  secret: string;
}

export interface SetSecureCredentialEnabledInput {
  credentialKey: string;
  enabled: boolean;
}

export interface SecureCredentialPolicySettings {
  defaultSessionTtlMinutes: number;
  maxResponseItems: number;
  allowReadonlyAuto: boolean;
  requireApprovalForAll: boolean;
  allowHttpCustomHeaders: boolean;
  httpAllowedDomains: string[];
  rateLimitPerMinute: number;
  maxConcurrentSessions: number;
  allowDefaultBranchCommits: boolean;
  allowHighRiskRepoOps: boolean;
  allowDeleteBranch: boolean;
  allowDeleteTag: boolean;
  allowDeleteRelease: boolean;
  allowUpdateRef: boolean;
  allowUpdateRepoSettings: boolean;
  updatedAt: string | null;
}

export interface UpdateSecureCredentialPolicySettingsInput {
  defaultSessionTtlMinutes: number;
  maxResponseItems: number;
  allowReadonlyAuto: boolean;
  requireApprovalForAll: boolean;
  allowHttpCustomHeaders: boolean;
  httpAllowedDomains: string[];
  rateLimitPerMinute: number;
  maxConcurrentSessions: number;
  allowDefaultBranchCommits: boolean;
  allowHighRiskRepoOps: boolean;
  allowDeleteBranch: boolean;
  allowDeleteTag: boolean;
  allowDeleteRelease: boolean;
  allowUpdateRef: boolean;
  allowUpdateRepoSettings: boolean;
}

export interface SecureCredentialOverview {
  total: number;
  active: number;
  disabled: number;
  mcpEnabled: number;
  expiringSoon: number;
  weeklyCalls: number;
  successRate: number;
}

export interface SecureCredentialAuditLog {
  id: number;
  actor: string;
  source: string;
  provider: SecureCredentialProvider | "";
  credentialKey: string;
  action: string;
  risk: string;
  result: string;
  durationMs: number;
  requestId: string;
  approvalId: number | null;
  detailJson: string;
  createdAt: string;
}

export interface ListSecureCredentialAuditLogsInput {
  keyword?: string;
  source?: string;
  provider?: SecureCredentialProvider | "";
  credentialKey?: string;
  actor?: string;
  action?: string;
  risk?: string;
  result?: string;
  limit?: number;
}

export type SecureCredentialSessionStatusValue = "active" | "expired" | "revoked";

export interface SecureCredentialSession {
  id: number;
  sessionId: string;
  credentialKey: string;
  provider: SecureCredentialProvider;
  caller: string;
  scopes: string[];
  status: SecureCredentialSessionStatusValue;
  expiresAt: string;
  createdAt: string;
  revokedAt: string | null;
  lastUsedAt: string | null;
  callCount: number;
}

export interface ListSecureCredentialSessionsInput {
  credentialKey?: string;
  status?: SecureCredentialSessionStatusValue | "";
  caller?: string;
}

export interface CreateSecureCredentialSessionInput {
  credentialKey: string;
  caller?: string;
  scopes: string[];
  ttlMinutes?: number;
}

export interface SecureCredentialSessionStatus {
  session: SecureCredentialSession;
  valid: boolean;
  reason: string;
}

export interface SecureCredentialProviderTestInput {
  credentialKey: string;
}

export interface SecureCredentialProviderTestResult {
  ok: boolean;
  credentialKey: string;
  provider: SecureCredentialProvider;
  account: string;
  statusCode: number | null;
  latencyMs: number;
  message: string;
  detail: unknown;
}

export interface SecureCredentialRepositoryListInput {
  sessionId: string;
  page?: number;
  perPage?: number;
}

export interface SecureCredentialRepository {
  id: string;
  name: string;
  fullName: string;
  webUrl: string;
  visibility: string;
  defaultBranch: string;
  permissions: unknown;
}

export interface SecureCredentialGitReadInput {
  sessionId: string;
  resource:
    | "repos"
    | "repo_detail"
    | "branches"
    | "file"
    | "commits"
    | "pull_requests"
    | "issues"
    | "releases"
    | "tags";
  repo?: string;
  path?: string;
  reference?: string;
  state?: string;
  page?: number;
  perPage?: number;
}

export interface SecureCredentialProviderReadResult {
  provider: SecureCredentialProvider;
  resource: string;
  statusCode: number;
  url: string;
  body: unknown;
  truncated: boolean;
}

export interface SecureCredentialHttpRequestInput {
  sessionId: string;
  path: string;
  queryJson?: Record<string, string>;
}

export interface SecureCredentialHttpWriteInput {
  sessionId: string;
  method: "POST" | "PUT" | "PATCH" | "DELETE";
  path: string;
  queryJson?: Record<string, string>;
  bodyJson?: unknown;
}

export interface SecureCredentialHttpRequestResult {
  statusCode: number;
  url: string;
  body: unknown;
  truncated: boolean;
}

export type SecureCredentialGitWriteOperation =
  | "create_issue"
  | "create_branch"
  | "commit_file"
  | "create_pr"
  | "update_pr"
  | "merge_pr"
  | "create_tag"
  | "create_release"
  | "trigger_workflow"
  | "delete_branch"
  | "delete_tag"
  | "delete_release"
  | "update_repo_settings";

export interface SecureCredentialGitWriteInput {
  sessionId: string;
  operation: SecureCredentialGitWriteOperation;
  repo: string;
  payload: Record<string, unknown>;
}

export interface SecureCredentialGitWriteResult {
  provider: SecureCredentialProvider;
  operation: SecureCredentialGitWriteOperation;
  repo: string;
  statusCode: number;
  body: unknown;
}
