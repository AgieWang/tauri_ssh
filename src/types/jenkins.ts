export interface JenkinsConnection {
  id: number;
  connectionKey: string;
  configVersion: number;
  name: string;
  baseUrl: string;
  credentialKey: string;
  credentialDisplayName: string;
  usernameMasked: string;
  sshServerAlias: string;
  environment: string;
  environmentLabel: string;
  tlsVerify: boolean;
  defaultView: string;
  defaultFolder: string;
  allowMcpRead: boolean;
  allowMcpWrite: boolean;
  approvalPolicy: string;
  parameterPrefillEnabled: boolean;
  riskRulesJson: string;
  notifyOnSuccess: boolean;
  notifyOnFailure: boolean;
  notifyOnUnstable: boolean;
  notifyOnAborted: boolean;
  status: string;
  version: string;
  capabilitiesJson: string;
  lastErrorCode: string;
  lastErrorMessage: string;
  description: string;
  enabled: boolean;
  lastTestedAt?: string | null;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface UpsertJenkinsConnectionInput {
  connectionKey?: string;
  name: string;
  baseUrl: string;
  credentialKey?: string;
  credentialDisplayName?: string;
  usernameMasked?: string;
  sshServerAlias?: string;
  environment?: string;
  environmentLabel?: string;
  tlsVerify?: boolean;
  defaultView?: string;
  defaultFolder?: string;
  allowMcpRead?: boolean;
  allowMcpWrite?: boolean;
  approvalPolicy?: string;
  parameterPrefillEnabled?: boolean;
  riskRulesJson?: string;
  notifyOnSuccess?: boolean;
  notifyOnFailure?: boolean;
  notifyOnUnstable?: boolean;
  notifyOnAborted?: boolean;
  description?: string;
  enabled?: boolean;
}

export interface ListJenkinsConnectionsInput {
  includeDeleted?: boolean;
  keyword?: string;
}

export interface JenkinsConnectionTestResult {
  ok: boolean;
  connectionKey: string;
  status: string;
  version: string;
  message: string;
  latencyMs: number;
}

export interface JenkinsJob {
  jobFullName: string;
  displayName: string;
  url: string;
  jobType: string;
  normalizedStatus: string;
  rawColor: string;
  buildable: boolean;
  lastBuildNumber?: number | null;
  lastBuildStatus: string;
  favorite: boolean;
  hasMore: boolean;
  children?: JenkinsJob[];
}

export interface ListJenkinsJobsInput {
  connectionKey: string;
  viewName?: string;
  folder?: string;
  refresh?: boolean;
  forceRefresh?: boolean;
  depth?: number;
}

export interface GetJenkinsJobDetailInput {
  connectionKey: string;
  jobFullName: string;
  refresh?: boolean;
}

export interface JenkinsJobDetail {
  connectionKey: string;
  jobFullName: string;
  job?: JenkinsJob | null;
  parameters: JenkinsParameterDefinitionsResult;
}

export interface SetJenkinsJobFavoriteInput {
  connectionKey: string;
  jobFullName: string;
  favorite: boolean;
  requester?: string;
}

export interface ListJenkinsParametersInput {
  connectionKey: string;
  jobFullName: string;
  refresh?: boolean;
}

export interface JenkinsParameterDefinition {
  name: string;
  parameterType: string;
  description: string;
  defaultValue: unknown;
  choices: string[];
  required: boolean;
  sensitive: boolean;
  fileParameter: boolean;
  dynamicParameter: boolean;
  unsupported: boolean;
  rawClass: string;
}

export interface JenkinsParameterDefinitionsResult {
  connectionKey: string;
  jobFullName: string;
  parameterDefinitionHash: string;
  parameters: JenkinsParameterDefinition[];
  fromCache: boolean;
  ttlSeconds: number;
  cachedAt: string;
  expiresAt: string;
}

export interface JenkinsRecentParameterValue {
  id: number;
  connectionKey: string;
  jobFullName: string;
  parameterName: string;
  requester: string;
  valueKind: string;
  valueJson: unknown;
  sensitive: boolean;
  updatedFromRunKey: string;
  updatedAt: string;
}

export interface JenkinsParameterTemplate {
  id: number;
  templateKey: string;
  connectionKey: string;
  jobFullName: string;
  name: string;
  parametersJson: unknown;
  parameterDefinitionHash: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

export interface ListJenkinsRecentParameterValuesInput {
  connectionKey: string;
  jobFullName: string;
  requester?: string;
}

export interface ListJenkinsParameterTemplatesInput {
  connectionKey: string;
  jobFullName: string;
  requester?: string;
}

export interface UpsertJenkinsParameterTemplateInput {
  templateKey?: string | null;
  connectionKey: string;
  jobFullName: string;
  name: string;
  parametersJson: unknown;
  parameterDefinitionHash?: string | null;
  requester?: string | null;
}

export interface DeleteJenkinsParameterTemplateInput {
  templateKey: string;
  requester?: string | null;
}

export interface ForgetJenkinsRecentParameterValueInput {
  connectionKey: string;
  jobFullName: string;
  parameterName: string;
  requester?: string;
}

export interface VerifyJenkinsParameterDefinitionHashInput {
  connectionKey: string;
  jobFullName: string;
  parameterDefinitionHash: string;
}

export interface InspectJenkinsFileParameterInput {
  parameterName: string;
  localPath: string;
}

export interface JenkinsFileParameterMetadata {
  parameterName: string;
  localPath: string;
  fileName: string;
  sizeBytes: number;
  sha256: string;
  modifiedAt?: string | null;
}

export interface JenkinsSensitiveParameterReference {
  valueKind: "secret_ref";
  secretRef: string;
}

export interface TriggerJenkinsBuildInput {
  connectionKey: string;
  jobFullName: string;
  parameterDefinitionHash: string;
  parametersJson: unknown;
  requester?: string | null;
  reason: string;
  riskLevel?: "L2" | "L3" | "blocked" | string | null;
}

export interface ExecuteJenkinsBuildApprovedInput {
  approvalId: number;
  requestHash?: string | null;
}

export interface JenkinsBuildTriggerResult {
  approvalId: number;
  requestHash: string;
  connectionKey: string;
  jobFullName: string;
  queueId?: string | null;
  location?: string | null;
  runKey: string;
  buildNumber?: number | null;
  status: string;
}

export interface StopJenkinsBuildInput {
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
  requester?: string | null;
  reason: string;
  riskLevel?: "L2" | "L3" | string | null;
}

export interface ExecuteJenkinsBuildStopApprovedInput {
  approvalId: number;
  requestHash?: string | null;
}

export interface JenkinsBuildStopResult {
  approvalId: number;
  requestHash: string;
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
  status: string;
}

export interface JenkinsBuild {
  runKey: string;
  requestId: string;
  connectionKey: string;
  jobFullName: string;
  queueId: string;
  buildNumber?: number | null;
  status: string;
  statusSource: string;
  result: string;
  cause: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  startedAt?: string | null;
  finishedAt?: string | null;
  lastErrorCode: string;
  lastErrorMessage: string;
}

export interface JenkinsBuildStatusEvent {
  runKey: string;
  requestId: string;
  connectionKey: string;
  jobFullName: string;
  queueId: string;
  buildNumber?: number | null;
  status: string;
  statusSource: string;
  result: string;
  updatedAt: string;
}

export interface ListJenkinsBuildsInput {
  connectionKey: string;
  jobFullName?: string;
  limit?: number;
  offset?: number;
  cursor?: string;
}

export interface GetJenkinsBuildInput {
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
}

export interface JenkinsBuildLogInput {
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
  start?: number;
  tailBytes?: number;
  requestId?: string;
}

export interface JenkinsBuildLogResult {
  requestId: string;
  text: string;
  start: number;
  nextStart: number;
  hasMore: boolean;
  redacted: boolean;
  message: string;
}

export interface RecordJenkinsLogCopyAuditInput {
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
  requestId?: string;
  startOffset: number;
  endOffset: number;
  bytes: number;
  redacted: boolean;
  rawLogAccess: boolean;
  confirmationSource?: string;
}

export interface GenerateJenkinsFailureAnalysisInput {
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
  runKey?: string | null;
  requestId?: string | null;
  logSnippet: string;
  snippetStartLine: number;
  snippetEndLine: number;
  matchedLines: number;
  requester?: string | null;
  providerKey?: string | null;
}

export interface JenkinsBuildAnalysis {
  id: number;
  analysisKey: string;
  runKey: string;
  requestId: string;
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
  providerKey: string;
  providerName: string;
  model: string;
  summaryMarkdown: string;
  snippetSha256: string;
  snippetStartLine: number;
  snippetEndLine: number;
  matchedLines: number;
  createdBy: string;
  createdAt: string;
}

export interface ListJenkinsArtifactsInput {
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
}

export interface DownloadJenkinsArtifactInput {
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
  relativePath: string;
}

export interface CleanupJenkinsArtifactInput {
  artifactKey: string;
}

export interface CreateJenkinsArtifactDeploymentCandidateInput {
  artifactKey: string;
}

export interface CreateJenkinsBuildDeploymentDryRunInput {
  artifactKey: string;
  serverAlias: string;
  deployRoot?: string | null;
  domain?: string | null;
  httpsEnabled?: boolean | null;
  port?: number | null;
  healthCheckUrl?: string | null;
}

export interface JenkinsArtifact {
  id: number;
  artifactKey: string;
  requestId: string;
  connectionKey: string;
  jobFullName: string;
  buildNumber: number;
  fileName: string;
  relativePath: string;
  localPath: string;
  sizeBytes?: number | null;
  sha256: string;
  sourceUrl: string;
  status: string;
  riskFlags: string[];
  downloadedAt?: string | null;
  cleanedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface JenkinsQueueItem {
  queueId: string;
  connectionKey: string;
  jobFullName: string;
  buildNumber?: number | null;
  executableUrl: string;
  status: string;
  message: string;
  createdAt: string;
}

export interface PollJenkinsQueueItemInput {
  connectionKey: string;
  queueId: string;
}
