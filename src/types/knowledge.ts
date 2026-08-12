export interface KnowledgePage<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

export interface KnowledgeProject {
  id: number;
  projectKey: string;
  name: string;
  aliases: string[];
  description: string;
  gitWorkspaceKeys: string[];
  gitWorkspaceKey: string;
  defaultBranch: string;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface KnowledgeRelease {
  id: number;
  projectId: number;
  version: string;
  tagName: string;
  branch: string;
  commitSha: string;
  description: string;
  releasedAt?: string | null;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface KnowledgeGitRef {
  refType: "branch" | "tag" | "commit";
  name: string;
  commitSha: string;
  subject: string;
  committedAt: string;
  current: boolean;
}

export interface KnowledgeSource {
  id: number;
  sourceKey: string;
  projectId?: number | null;
  sourceType: string;
  displayName: string;
  rootPath: string;
  gitWorkspaceKey: string;
  includeGlobs: string[];
  excludeGlobs: string[];
  versionStrategy: string;
  syncMode: string;
  allowRemoteEmbedding: boolean;
  enabled: boolean;
  lastCommitSha: string;
  lastSyncStatus: string;
  lastSyncedAt?: string | null;
  lastError?: string | null;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface KnowledgeSourceScopeEntry {
  relativePath: string;
  entryType: "file" | "directory" | "symlink" | "other";
  decision: "included" | "skipped";
  reason: string;
  sizeBytes: number;
}

export interface KnowledgeSourceScopePreview {
  sourceType: string;
  canonicalRoot: string;
  includeGlobs: string[];
  excludeGlobs: string[];
  allowRemoteEmbedding: boolean;
  includedFiles: number;
  skippedEntries: number;
  includedBytes: number;
  truncated: boolean;
  warnings: string[];
  entries: KnowledgeSourceScopeEntry[];
}

export interface SyncKnowledgeGitSourceInput {
  sourceId: number;
  releaseId?: number | null;
  gitRef: string;
}

export interface SyncKnowledgeLocalSourceInput {
  sourceId: number;
  releaseId?: number | null;
}

export interface StartKnowledgeSourceSyncInput {
  sourceId: number;
  releaseId?: number | null;
  gitRef?: string | null;
}

export interface KnowledgeSourceSyncResult {
  sourceId: number;
  commitSha: string;
  scannedFiles: number;
  createdVersions: number;
  unchangedFiles: number;
  deletedPaths: number;
  skippedFiles: number;
  warnings: string[];
}

export interface KnowledgeDocument {
  id: number;
  documentKey: string;
  projectId?: number | null;
  sourceId?: number | null;
  docType: string;
  title: string;
  logicalPath: string;
  /** 后端基于上传关联返回的来源文件夹；普通文档保持为空。 */
  sourceFolderName?: string | null;
  status: string;
  sensitivity: string;
  tags: string[];
  latestVersionId?: number | null;
  allowAi: boolean;
  allowMcp: boolean;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface KnowledgeDocumentVersion {
  id: number;
  documentId: number;
  releaseId?: number | null;
  versionLabel: string;
  gitBranch: string;
  commitSha: string;
  sourcePath: string;
  mimeType: string;
  content: string;
  contentHash: string;
  parsedMeta: Record<string, unknown>;
  tokenEstimate: number;
  valid: boolean;
  createdAt: string;
}

export interface KnowledgeDocumentProcessingTaskSummary {
  id: number;
  jobKey: string;
  jobType: string;
  status: string;
  progressCurrent: number;
  progressTotal: number;
  message: string;
  cancelRequested: boolean;
}

export interface KnowledgeDocumentParseSummary {
  parserId: string;
  parserVersion: string;
  qualityLevel: string;
  warnings: string[];
}

export interface KnowledgeDocumentProcessingSummary {
  status: string;
  message: string;
  failureReason?: string | null;
  contentAvailable: boolean;
  availableActions: string[];
  task?: KnowledgeDocumentProcessingTaskSummary | null;
  parser?: KnowledgeDocumentParseSummary | null;
}

/** 删除确认页的影响范围；永久删除不由该契约开放。 */
export interface KnowledgeDocumentDeletionImpactPreview {
  documentId: number;
  title: string;
  versionCount: number;
  chunkCount: number;
  vectorCount: number;
  relationCount: number;
  assetCount: number;
  ftsEntryCount: number;
  permanentDeletionEnabled: false;
  permanentDeletionBlockReason: string;
}

/** 软删除恢复结果只报告当前文档与幂等重建的全文索引条目数量。 */
export interface RestoreKnowledgeDocumentResult {
  document: KnowledgeDocument;
  rebuiltFtsEntries: number;
}

export interface KnowledgeDocumentDetail {
  document: KnowledgeDocument;
  versions: KnowledgeDocumentVersion[];
  processing: KnowledgeDocumentProcessingSummary;
}

export interface CompareKnowledgeDocumentVersionsInput {
  fromVersionId: number;
  toVersionId: number;
}

/** 可用于审阅的解析产物签名；不含本地路径、资产存储键或未清洗的结构化内容。 */
export interface KnowledgeDocumentComparisonArtifact {
  parserId: string;
  parserVersion: string;
  qualityLevel: string;
  normalizedHash: string;
  assetHash: string | null;
}

export interface KnowledgeDocumentComparison {
  fromVersion: KnowledgeDocumentVersion;
  toVersion: KnowledgeDocumentVersion;
  contentChanged: boolean;
  assetChanged: boolean;
  parserChanged: boolean;
  unchanged: boolean;
  commonPrefixLines: number;
  commonSuffixLines: number;
  removedLines: string[];
  addedLines: string[];
  fromAssetHashes: string[];
  toAssetHashes: string[];
  fromParseArtifacts: KnowledgeDocumentComparisonArtifact[];
  toParseArtifacts: KnowledgeDocumentComparisonArtifact[];
}

export interface KnowledgeCitationDetail {
  citation: KnowledgeCitation;
  document: KnowledgeDocument;
  version: KnowledgeDocumentVersion;
  chunk: KnowledgeChunk;
}

export interface KnowledgeChunk {
  id: number;
  documentVersionId: number;
  chunkIndex: number;
  headingPath: string;
  content: string;
  contentHash: string;
  location: Record<string, unknown>;
  tokenEstimate: number;
  embeddingStatus: string;
  createdAt: string;
  updatedAt: string;
}

export interface KnowledgeEmbeddingProfile {
  id: number;
  profileKey: string;
  name: string;
  mode: "local" | "remote";
  providerKey: string;
  model: string;
  modelRevision: string;
  dimension: number;
  normalized: boolean;
  config: Record<string, unknown>;
  fingerprint: string;
  status: string;
  isActive: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface KnowledgeEmbeddingIndexValidation {
  profileId: number;
  profileKey: string;
  expectedChunks: number;
  indexedChunks: number;
  staleChunks: number;
  dimensionMismatchChunks: number;
  invalidVectorChunks: number;
  complete: boolean;
}

export interface KnowledgeEmbeddingLifecycleResult {
  profile: KnowledgeEmbeddingProfile;
  validation: KnowledgeEmbeddingIndexValidation;
}

export interface KnowledgeLocalEmbeddingCacheEntry {
  modelKey: string;
  sizeBytes: number;
  sha256: string;
  importedAt: string;
}

export interface ImportKnowledgeLocalEmbeddingModelInput {
  modelKey: string;
  sourcePath: string;
  expectedSha256: string;
}

export interface KnowledgeLocalEmbeddingModelImportResult {
  modelKey: string;
  sha256: string;
  sizeBytes: number;
  importedAt: string;
}

export interface RemoveKnowledgeLocalEmbeddingModelInput {
  modelKey: string;
}

export interface DownloadKnowledgeLocalEmbeddingModelInput {
  modelKey: string;
}

export interface KnowledgeLocalEmbeddingDownloadProgress {
  stage: string;
  modelKey: string;
  filesCompleted: number;
  filesTotal: number;
  bytesDownloaded: number;
  totalBytes: number;
}

export interface GenerateKnowledgeLocalEmbeddingsInput {
  modelKey: string;
  texts: string[];
  prefix: string;
  batchSize?: number | null;
}

export interface KnowledgeEmbeddingProfileTestResult {
  profile: KnowledgeEmbeddingProfile;
  dimension: number;
  probeText: string;
}

export interface KnowledgeLocalEmbeddingRuntimeStatus {
  runtime: string;
  fastembedFeatureEnabled: boolean;
  runtimeAvailable: boolean;
  automaticDownloadEnabled: boolean;
  cacheDir: string;
  cacheExists: boolean;
  cachedModels: KnowledgeLocalEmbeddingCacheEntry[];
  warnings: string[];
}

export interface EstimateKnowledgeEmbeddingRebuildInput {
  profileId: number;
}

export interface KnowledgeRemoteRebuildSourceEstimate {
  sourceId?: number | null;
  sourceKey: string;
  displayName: string;
  eligibleChunks: number;
  eligibleCharacters: number;
  blockedChunks: number;
}

export interface KnowledgeEmbeddingIndexAvailability {
  profileId: number;
  profileKey: string;
  totalChunks: number;
  indexedChunks: number;
  missingChunks: number;
  available: boolean;
}

export interface KnowledgeEmbeddingRebuildEstimate {
  targetProfileId: number;
  targetProfileKey: string;
  targetMode: "local" | "remote";
  targetDimension: number;
  affectedDocuments: number;
  affectedChunks: number;
  reusableChunks: number;
  chunksToEmbed: number;
  localWorkChunks: number;
  remoteEligibleChunks: number;
  remoteCharacters: number;
  remoteBlockedChunks: number;
  estimatedIndexBytes: number;
  additionalDiskBytes: number;
  requiresRemoteConfirmation: boolean;
  remoteSources: KnowledgeRemoteRebuildSourceEstimate[];
  currentIndex?: KnowledgeEmbeddingIndexAvailability | null;
}

export interface KnowledgeChunkEmbedding {
  chunkId: number;
  profileId: number;
  dimension: number;
  vectorNorm: number;
  contentHash: string;
  createdAt: string;
}

export interface KnowledgeFtsCapability {
  fts5Available: boolean;
  trigramAvailable: boolean;
  activeTokenizer: string;
}

export interface KnowledgeRelation {
  id: number;
  projectId?: number | null;
  releaseId?: number | null;
  documentVersionId?: number | null;
  snapshotId?: number | null;
  sensitivity: string;
  /** needs_rebuild 的历史关系默认不会进入召回。 */
  scopeStatus: string;
  fromType: string;
  fromKey: string;
  relationType: string;
  toType: string;
  toKey: string;
  evidence: Record<string, unknown>;
  confidence: number;
  confirmed: boolean;
  source: string;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface KnowledgeJob {
  id: number;
  jobKey: string;
  jobType: string;
  sourceId?: number | null;
  profileId?: number | null;
  status: string;
  progressCurrent: number;
  progressTotal: number;
  message: string;
  error?: string | null;
  checkpoint: Record<string, unknown>;
  heartbeatAt?: string | null;
  cancelRequested: boolean;
  startedAt: string;
  finishedAt?: string | null;
}

export interface KnowledgeGenerationRun {
  id: number;
  runKey: string;
  projectId: number;
  releaseId?: number | null;
  sourceId?: number | null;
  syncJobId?: number | null;
  templateVersion: string;
  documentTypes: string[];
  inputHash: string;
  status: string;
  generatedCount: number;
  skippedCount: number;
  aiSummaryEnabled: boolean;
  aiProviderKey: string;
  aiModel: string;
  error?: string | null;
  startedAt: string;
  finishedAt?: string | null;
}

export interface ZentaoConnection {
  id: number;
  connectionKey: string;
  name: string;
  baseUrl: string;
  apiVersion: string;
  authMode: string;
  endpointProfile: string;
  credentialConfigured: boolean;
  tlsVerify: boolean;
  allowInsecureHttp: boolean;
  requestTimeoutSeconds: number;
  pageSize: number;
  rateLimitPerSecond: number;
  capabilities: Record<string, unknown>;
  enabled: boolean;
  lastTestStatus: string;
  lastTestedAt?: string | null;
  lastError?: string | null;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface ZentaoProjectMapping {
  id: number;
  connectionId: number;
  knowledgeProjectId: number;
  remoteProductId: string;
  remoteProjectId: string;
  remoteExecutionIds: string[];
  releaseMapping: Record<string, unknown>;
  syncScope: Record<string, unknown>;
  syncSince?: string | null;
  includeComments: boolean;
  includeWorklogs: boolean;
  includeAttachmentMetadata: boolean;
  allowRemoteEmbedding: boolean;
  allowRemoteAi: boolean;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface ZentaoSyncCursor {
  id: number;
  mappingId: number;
  entityType: string;
  lastUpdatedAt: string;
  lastExternalId: string;
  checkpoint: Record<string, unknown>;
  lastSuccessAt?: string | null;
  lastFullSyncAt?: string | null;
  updatedAt: string;
}

export interface ZentaoEntity {
  id: number;
  connectionId: number;
  mappingId: number;
  knowledgeProjectId: number;
  releaseId?: number | null;
  entityType: string;
  externalId: string;
  externalKey: string;
  title: string;
  bodyMarkdown: string;
  originalStatus: string;
  normalizedStatus: string;
  assigneeExternalId: string;
  parentExternalKey: string;
  remoteUrl: string;
  contentHash: string;
  rawJsonHash: string;
  rawSnapshot?: Record<string, unknown> | null;
  sourceCreatedAt?: string | null;
  sourceUpdatedAt?: string | null;
  firstSyncedAt: string;
  lastSyncedAt: string;
  missingCount: number;
  status: string;
  deletedAt?: string | null;
}

export interface ZentaoEntityRelation {
  id: number;
  fromExternalKey: string;
  relationType: string;
  toExternalKey: string;
  evidence: Record<string, unknown>;
  source: string;
  confidence: number;
  confirmed: boolean;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string | null;
}

export interface KnowledgeCodeSnapshot {
  id: number;
  snapshotKey: string;
  sourceId: number;
  projectId?: number | null;
  releaseId?: number | null;
  snapshotType: string;
  refName: string;
  commitSha: string;
  baseCommitSha: string;
  branchName: string;
  worktreeDirty: boolean;
  /** 工作树的状态和文件哈希；只表示本地观察，不是发布事实。 */
  dirtyState: Record<string, unknown>;
  capturedAt: string;
  fileCount: number;
  symbolCount: number;
  relationCount: number;
  analyzerVersion: string;
  status: string;
  error?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface CreateKnowledgeCodeSnapshotInput {
  snapshotKey: string;
  sourceId: number;
  projectId?: number | null;
  releaseId?: number | null;
  snapshotType: string;
  refName: string;
  commitSha: string;
  baseCommitSha: string;
  branchName: string;
  worktreeDirty: boolean;
  dirtyState: Record<string, unknown>;
  capturedAt: string;
  fileCount: number;
  analyzerVersion: string;
  status: string;
}

export interface CaptureKnowledgeGitSnapshotInput {
  sourceId: number;
  gitRef: string;
  releaseId?: number | null;
}

export interface CaptureKnowledgeDirtyWorktreeSnapshotInput {
  sourceId: number;
  releaseId?: number | null;
}

export interface CaptureKnowledgeLocalDirectorySnapshotInput {
  sourceId: number;
  releaseId?: number | null;
}

export interface KnowledgeCodeFile {
  id: number;
  snapshotId: number;
  documentVersionId?: number | null;
  relativePath: string;
  language: string;
  fileSize: number;
  contentHash: string;
  analysisLevel: "ast" | "structured_fallback" | "text_only" | "skipped";
  isGenerated: boolean;
  isTest: boolean;
  sensitivity: string;
  status: string;
  skipReason: string;
  createdAt: string;
}

export interface KnowledgeCodeFileContent {
  file: KnowledgeCodeFile;
  content: string;
}

export interface KnowledgeCodeSymbol {
  id: number;
  snapshotId: number;
  fileId: number;
  symbolKey: string;
  symbolKind: string;
  name: string;
  qualifiedName: string;
  signature: string;
  visibility: string;
  parentSymbolKey: string;
  startLine: number;
  startColumn: number;
  endLine: number;
  endColumn: number;
  docComment: string;
  contentHash: string;
  analysisLevel: string;
  createdAt: string;
}

export interface KnowledgeCodeRelation {
  id: number;
  snapshotId: number;
  fromSymbolKey: string;
  relationType: string;
  toSymbolKey: string;
  toExternalType: string;
  toExternalKey: string;
  evidenceFileId?: number | null;
  evidenceStartLine?: number | null;
  evidenceEndLine?: number | null;
  evidenceText: string;
  resolver: string;
  confidence: number;
  confirmed: boolean;
  createdAt: string;
}

export interface KnowledgeCodeAnalysisResult {
  snapshot: KnowledgeCodeSnapshot;
  analyzedFiles: number;
  skippedFiles: number;
  symbols: number;
  documents: number;
  warnings: string[];
}

/** 在单个已完成分析的源码快照中搜索符号，避免跨历史版本混用同名符号。 */
export interface SearchKnowledgeCodeSymbolsInput {
  snapshotId: number;
  keyword?: string | null;
}

export interface KnowledgeCodeCallGraphInput {
  snapshotId: number;
  symbolKey: string;
  /** 关系图深度由后端限制为 1 至 5。 */
  maxDepth?: number | null;
  /** 候选关系默认不参与图分析，调用方必须显式开启。 */
  includeUnconfirmed?: boolean | null;
}

export interface KnowledgeCodeCallGraph {
  snapshotId: number;
  rootSymbolKey: string;
  nodes: KnowledgeCodeSymbol[];
  edges: KnowledgeCodeRelation[];
  maxDepth: number;
  truncated: boolean;
}

export interface CompareKnowledgeCodeSnapshotsInput {
  fromSnapshotId: number;
  toSnapshotId: number;
}

export interface KnowledgeCodeFileChange {
  changeType: "added" | "modified" | "deleted" | "renamed";
  fromPath: string;
  toPath: string;
  contentHash: string;
}

export interface KnowledgeCodeSnapshotComparison {
  fromSnapshot: KnowledgeCodeSnapshot;
  toSnapshot: KnowledgeCodeSnapshot;
  fileChanges: KnowledgeCodeFileChange[];
  addedSymbolKeys: string[];
  removedSymbolKeys: string[];
  retainedSymbolKeys: string[];
}

export interface AnalyzeKnowledgeCodeImpactInput {
  snapshotId: number;
  symbolKeys: string[];
  /** 影响分析只反向遍历已确认关系，后端限制为 1 至 5。 */
  maxDepth?: number | null;
}

export interface KnowledgeCodeSourceSettings {
  sourceId: number;
  includeUntracked: boolean;
  maxFileSizeBytes: number;
  allowedLanguages: string[];
  allowRemoteProcessing: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface KnowledgeCodeSource {
  source: KnowledgeSource;
  settings: KnowledgeCodeSourceSettings;
}

export interface KnowledgeListInput {
  projectId?: number | null;
  releaseId?: number | null;
  sourceId?: number | null;
  keyword?: string | null;
  status?: string | null;
  offset?: number;
  limit?: number;
}

export interface UpsertKnowledgeProjectInput {
  id?: number | null;
  projectKey: string;
  name: string;
  aliases: string[];
  description: string;
  gitWorkspaceKeys: string[];
  gitWorkspaceKey: string;
  defaultBranch: string;
  enabled: boolean;
}

export interface UpsertKnowledgeReleaseInput {
  id?: number | null;
  projectId: number;
  version: string;
  tagName: string;
  branch: string;
  commitSha: string;
  description: string;
  releasedAt?: string | null;
}

export interface UpsertKnowledgeSourceInput {
  id?: number | null;
  sourceKey: string;
  projectId?: number | null;
  sourceType: string;
  displayName: string;
  rootPath: string;
  gitWorkspaceKey: string;
  includeGlobs: string[];
  excludeGlobs: string[];
  versionStrategy: string;
  syncMode: string;
  allowRemoteEmbedding: boolean;
  enabled: boolean;
}

export interface UpsertKnowledgeCodeSourceInput {
  source: UpsertKnowledgeSourceInput;
  includeUntracked: boolean;
  maxFileSizeBytes: number;
  allowedLanguages: string[];
  allowRemoteProcessing: boolean;
}

export interface UpsertKnowledgeDocumentInput {
  id?: number | null;
  documentKey: string;
  projectId?: number | null;
  sourceId?: number | null;
  docType: string;
  title: string;
  logicalPath: string;
  sensitivity: string;
  tags: string[];
  allowAi: boolean;
  allowMcp: boolean;
}

export interface CreateKnowledgeDocumentVersionInput {
  documentId: number;
  releaseId?: number | null;
  versionLabel: string;
  gitBranch: string;
  commitSha: string;
  sourcePath: string;
  mimeType: string;
  content: string;
  contentHash: string;
  parsedMeta: Record<string, unknown>;
  tokenEstimate: number;
}

export interface KnowledgeChunkWriteInput {
  chunkIndex: number;
  headingPath: string;
  content: string;
  contentHash: string;
  location: Record<string, unknown>;
  tokenEstimate: number;
}

export interface KnowledgeParseInput {
  sourcePath: string;
  mimeType: string;
  content: string;
  /** 二进制容器由受控后端导入路径提供，普通文本预览无需传递。 */
  binaryContent?: number[] | null;
}

export interface KnowledgeParsedBlock {
  blockType: string;
  headingPath: string[];
  content: string;
  startLine: number;
  endLine: number;
  metadata: Record<string, unknown>;
}

export interface KnowledgeParsedDocument {
  parserId: string;
  normalizationVersion: string;
  normalizedContent: string;
  frontMatter: Record<string, unknown>;
  blocks: KnowledgeParsedBlock[];
  warnings: string[];
}

export interface KnowledgeChunkOptions {
  targetChars?: number | null;
  maxChars?: number | null;
  overlapChars?: number | null;
}

export interface KnowledgeParseAndChunkInput {
  document: KnowledgeParseInput;
  options?: KnowledgeChunkOptions | null;
}

export interface KnowledgeParseAndChunkResult {
  parsed: KnowledgeParsedDocument;
  chunkStrategyId: string;
  chunks: KnowledgeChunkWriteInput[];
}

export interface UpsertKnowledgeEmbeddingProfileInput {
  id?: number | null;
  profileKey: string;
  name: string;
  mode: "local" | "remote";
  providerKey: string;
  model: string;
  modelRevision: string;
  dimension: number;
  normalized: boolean;
  config: Record<string, unknown>;
  fingerprint: string;
}

export interface KnowledgeEmbeddingFingerprintInput {
  mode: "local" | "remote";
  providerProtocol: string;
  endpointIdentity: string;
  providerKey: string;
  model: string;
  modelRevision: string;
  dimension: number;
  normalized: boolean;
  queryPrefix: string;
  documentPrefix: string;
  chunkStrategyId: string;
  normalizationVersion: string;
}

export interface UpsertZentaoConnectionInput {
  id?: number | null;
  connectionKey: string;
  name: string;
  baseUrl: string;
  apiVersion: string;
  authMode: string;
  endpointProfile: string;
  credentialKey: string;
  tlsVerify: boolean;
  allowInsecureHttp: boolean;
  requestTimeoutSeconds: number;
  pageSize: number;
  rateLimitPerSecond: number;
  enabled: boolean;
}

export interface UpsertZentaoProjectMappingInput {
  id?: number | null;
  connectionId: number;
  knowledgeProjectId: number;
  remoteProductId: string;
  remoteProjectId: string;
  remoteExecutionIds: string[];
  releaseMapping: Record<string, unknown>;
  syncScope: Record<string, unknown>;
  syncSince?: string | null;
  includeComments: boolean;
  includeWorklogs: boolean;
  includeAttachmentMetadata: boolean;
  allowRemoteEmbedding: boolean;
  allowRemoteAi: boolean;
  enabled: boolean;
}

export interface ZentaoCapabilityProbeResult {
  connectionId: number;
  apiVersion: string;
  authMode: string;
  endpointProfile: string;
  capabilities: Record<string, unknown>;
  status: string;
  message: string;
}

export interface ZentaoRemoteScopeItem {
  entityType: string;
  externalId: string;
  name: string;
  parentExternalId: string;
  status: string;
}

export interface SyncZentaoMappingInput {
  mappingId: number;
  entityTypes: string[];
}

export interface ZentaoSyncResult {
  mappingId: number;
  entityType: string;
  fetchedCount: number;
  changedCount: number;
  unchangedCount: number;
  missingConfirmedCount: number;
  cursor: ZentaoSyncCursor;
}

/** 仅基于已同步实体生成固定事实文档；不触发禅道请求或 AI 摘要。 */
export interface GenerateZentaoKnowledgeDocumentsInput {
  mappingId: number;
}

export interface GenerateZentaoKnowledgeDocumentsResult {
  mappingId: number;
  sourceId: number;
  generatedDocumentVersionIds: number[];
  entityCount: number;
}

export interface GenerateZentaoAiSummaryInput {
  mappingId: number;
  providerKey: string;
  model: string;
  prompt: string;
}

export interface GenerateZentaoAiSummaryResult {
  mappingId: number;
  documentVersionId: number;
  citationCount: number;
  providerKey: string;
  model: string;
}

export interface ImportKnowledgeExperiencesInput {
  projectId?: number | null;
  releaseId?: number | null;
}

export interface ImportKnowledgeExperiencesResult {
  sourceId: number;
  scannedCount: number;
  importedCount: number;
  unchangedCount: number;
  restrictedCount: number;
  generatedDocumentVersionIds: number[];
}

export interface GenerateKnowledgeCodeDocumentsInput {
  snapshotId: number;
}

export interface GenerateKnowledgeCodeDocumentsResult {
  snapshotId: number;
  sourceId: number;
  generatedDocumentVersionIds: number[];
  fileCount: number;
  symbolCount: number;
  relationCount: number;
}

export interface KnowledgeSearchInput {
  query: string;
  projectIds: number[];
  releaseIds: number[];
  sourceIds: number[];
  documentTypes: string[];
  sensitivities: string[];
  snapshotId?: number | null;
  limit?: number;
  includeContext?: boolean;
}

export interface KnowledgeQueryAnalysis {
  query: string;
  projectIds: number[];
  ambiguousProjectIds: number[];
  releases: string[];
  requirementIds: string[];
  commitShas: string[];
  codeSymbols: string[];
  paths: string[];
  apiRoutes: string[];
  tables: string[];
  fields: string[];
}

export interface KnowledgeVectorSearchInput {
  queryVector: number[];
  filters: KnowledgeSearchInput;
}

export interface KnowledgeCitation {
  citationKey: string;
  sourceType: string;
  documentId?: number | null;
  documentVersionId?: number | null;
  chunkId?: number | null;
  projectId?: number | null;
  releaseId?: number | null;
  title: string;
  logicalPath: string;
  headingPath: string;
  commitSha: string;
  externalKey: string;
  snapshotId?: number | null;
  symbolKey: string;
  startLine?: number | null;
  endLine?: number | null;
  excerpt: string;
}

export interface KnowledgeSearchHit {
  score: number;
  channels: string[];
  citation: KnowledgeCitation;
  content: string;
  diagnostics: Record<string, unknown>;
}

export interface KnowledgeAskInput {
  search: KnowledgeSearchInput;
  /** 可选的原始提问；用于在使用优化检索词时保留模型回答所需的业务限定。 */
  originalQuestion?: string;
  /** 内置业务入口选择的回答模式；普通知识问答无需传入。 */
  answerMode?: "releaseRequirementCoverage";
  providerKey: string;
  model: string;
  evidenceOnly?: boolean;
  conversation?: Array<{
    role: "user" | "assistant";
    content: string;
  }>;
}

export interface KnowledgeAskResult {
  answer: string;
  citationValidation: "verified" | "unverified" | "notApplicable";
  citations: KnowledgeCitation[];
  conflicts: string[];
  evidenceGaps: string[];
  retrievalDiagnostics: Record<string, unknown>;
}

export interface KnowledgeRagContextPreview {
  prompt: string;
  context: string;
  citations: KnowledgeCitation[];
  conflicts: string[];
  evidenceGaps: string[];
  retrievalDiagnostics: Record<string, unknown>;
}

export interface RunKnowledgeRetrievalEvaluationInput {
  topK?: number;
}

export interface KnowledgeRetrievalEvaluationCaseResult {
  fixtureId: string;
  hitCount: number;
  recallAtK: number;
  reciprocalRank: number;
  citationAccuracy: number;
  versionLeakage: boolean;
  refusalExpected: boolean;
  refusalCorrect: boolean;
  latencyMs: number;
}

export interface KnowledgeRetrievalEvaluationRun {
  id: number;
  fixtureVersion: string;
  profileId?: number | null;
  topK: number;
  caseCount: number;
  recallAtK: number;
  mrr: number;
  citationAccuracy: number;
  versionLeakageRate: number;
  refusalAccuracy: number;
  p50LatencyMs: number;
  p95LatencyMs: number;
  details: KnowledgeRetrievalEvaluationCaseResult[];
  createdAt: string;
}

export interface BuildKnowledgeEmbeddingBatchInput {
  profileId: number;
  jobKey?: string;
  batchSize?: number;
}

export interface KnowledgeEmbeddingBatchResult {
  profileId: number;
  jobKey: string;
  totalChunks: number;
  processedChunks: number;
  embeddedChunks: number;
  skippedChunks: number;
  blockedChunks: number;
  completed: boolean;
  checkpoint: Record<string, unknown>;
}

export interface KnowledgeJobProgress {
  jobKey: string;
  status: string;
  stage: string;
  current: number;
  total: number;
  message: string;
  canCancel: boolean;
  error?: KnowledgeErrorDetail | null;
}

export interface KnowledgeErrorDetail {
  code: string;
  message: string;
  stage: string;
  sourceKey: string;
  retryable: boolean;
  sanitizedDetails: Record<string, unknown>;
}

export interface ImportKnowledgeCommitRelationsInput {
  commitSha: string;
  commitMessage: string;
  entityPrefixes?: string[];
  confirmed?: boolean;
  /** 提供时，Commit 与禅道实体关系会受此代码快照的项目/版本范围约束。 */
  snapshotId?: number;
}
