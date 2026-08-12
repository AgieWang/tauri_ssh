import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  CaptureKnowledgeDirtyWorktreeSnapshotInput,
  CaptureKnowledgeGitSnapshotInput,
  BuildKnowledgeEmbeddingBatchInput,
  CaptureKnowledgeLocalDirectorySnapshotInput,
  CompareKnowledgeDocumentVersionsInput,
  KnowledgeChunk,
  KnowledgeCodeAnalysisResult,
  KnowledgeCodeFile,
  KnowledgeCodeFileContent,
  KnowledgeCodeSource,
  KnowledgeCodeCallGraph,
  KnowledgeCodeCallGraphInput,
  KnowledgeCodeSnapshot,
  KnowledgeCodeSnapshotComparison,
  KnowledgeCodeSymbol,
  KnowledgeChunkOptions,
  KnowledgeCitationDetail,
  KnowledgeDocumentComparison,
  KnowledgeDocumentDeletionImpactPreview,
  KnowledgeDocumentDetail,
  KnowledgeDocumentVersion,
  KnowledgeListInput,
  KnowledgePage,
  KnowledgeDocument,
  KnowledgeProject,
  KnowledgeRelease,
  KnowledgeEmbeddingFingerprintInput,
  KnowledgeEmbeddingProfile,
  EstimateKnowledgeEmbeddingRebuildInput,
  GenerateZentaoKnowledgeDocumentsInput,
  GenerateZentaoKnowledgeDocumentsResult,
  GenerateZentaoAiSummaryInput,
  GenerateZentaoAiSummaryResult,
  ImportKnowledgeExperiencesInput,
  ImportKnowledgeExperiencesResult,
  GenerateKnowledgeCodeDocumentsInput,
  GenerateKnowledgeCodeDocumentsResult,
  KnowledgeEmbeddingRebuildEstimate,
  KnowledgeEmbeddingBatchResult,
  KnowledgeAskInput,
  KnowledgeAskResult,
  KnowledgeEmbeddingIndexValidation,
  KnowledgeEmbeddingLifecycleResult,
  KnowledgeEmbeddingProfileTestResult,
  KnowledgeLocalEmbeddingModelImportResult,
  KnowledgeLocalEmbeddingRuntimeStatus,
  ImportKnowledgeLocalEmbeddingModelInput,
  DownloadKnowledgeLocalEmbeddingModelInput,
  KnowledgeParseAndChunkInput,
  KnowledgeParseAndChunkResult,
  KnowledgeSearchHit,
  KnowledgeSearchInput,
  KnowledgeRagContextPreview,
  SearchKnowledgeCodeSymbolsInput,
  CompareKnowledgeCodeSnapshotsInput,
  AnalyzeKnowledgeCodeImpactInput,
  KnowledgeRetrievalEvaluationRun,
  RunKnowledgeRetrievalEvaluationInput,
  KnowledgeVectorSearchInput,
  KnowledgeJob,
  KnowledgeSource,
  KnowledgeSourceScopePreview,
  StartKnowledgeSourceSyncInput,
  UpsertKnowledgeEmbeddingProfileInput,
  UpsertKnowledgeCodeSourceInput,
  UpsertZentaoConnectionInput,
  UpsertKnowledgeProjectInput,
  UpsertKnowledgeReleaseInput,
  UpsertKnowledgeSourceInput,
  UpsertZentaoProjectMappingInput,
  SyncZentaoMappingInput,
  ZentaoCapabilityProbeResult,
  ZentaoConnection,
  ZentaoProjectMapping,
  ZentaoRemoteScopeItem,
  ZentaoSyncResult,
  RestoreKnowledgeDocumentResult,
} from "@/types";

function documentListQuery(input?: KnowledgeListInput) {
  const query = new URLSearchParams();
  if (input?.projectId != null) query.set("projectId", String(input.projectId));
  if (input?.releaseId != null) query.set("releaseId", String(input.releaseId));
  if (input?.sourceId != null) query.set("sourceId", String(input.sourceId));
  if (input?.keyword) query.set("keyword", input.keyword);
  if (input?.status) query.set("status", input.status);
  if (input?.offset != null) query.set("offset", String(input.offset));
  if (input?.limit != null) query.set("limit", String(input.limit));
  const suffix = query.toString();
  return suffix ? `/knowledge/documents?${suffix}` : "/knowledge/documents";
}

function knowledgeListQuery(path: string, input?: KnowledgeListInput) {
  const query = new URLSearchParams();
  if (input?.projectId != null) query.set("projectId", String(input.projectId));
  if (input?.releaseId != null) query.set("releaseId", String(input.releaseId));
  if (input?.sourceId != null) query.set("sourceId", String(input.sourceId));
  if (input?.keyword) query.set("keyword", input.keyword);
  if (input?.status) query.set("status", input.status);
  if (input?.offset != null) query.set("offset", String(input.offset));
  if (input?.limit != null) query.set("limit", String(input.limit));
  const suffix = query.toString();
  return suffix ? `${path}?${suffix}` : path;
}

export const knowledgeApi = {
  getRemoteEmbeddingEnabled: () =>
    hasTauriRuntime()
      ? invoke<boolean>("get_knowledge_remote_embedding_enabled")
      : devApiFetch<boolean>("/knowledge/embedding/remote-enabled"),
  listProjects: (input?: KnowledgeListInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgePage<KnowledgeProject>>("list_knowledge_projects", {
          input,
        })
      : devApiFetch<KnowledgePage<KnowledgeProject>>(
          knowledgeListQuery("/knowledge/projects", input),
        ),
  listReleases: (projectId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeRelease[]>("list_knowledge_releases", { projectId })
      : devApiFetch<KnowledgeRelease[]>(
          `/knowledge/projects/${projectId}/releases`,
        ),
  upsertProject: (input: UpsertKnowledgeProjectInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeProject>("upsert_knowledge_project", { input })
      : devApiFetch<KnowledgeProject>("/knowledge/projects", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteProject: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_knowledge_project", { id })
      : devApiFetch<void>(`/knowledge/projects/${id}`, { method: "DELETE" }),
  upsertRelease: (input: UpsertKnowledgeReleaseInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeRelease>("upsert_knowledge_release", { input })
      : devApiFetch<KnowledgeRelease>("/knowledge/releases", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteRelease: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_knowledge_release", { id })
      : devApiFetch<void>(`/knowledge/releases/${id}`, { method: "DELETE" }),
  listSources: (projectId?: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeSource[]>("list_knowledge_sources", { projectId })
      : devApiFetch<KnowledgeSource[]>(
          projectId == null
            ? "/knowledge/sources"
            : `/knowledge/sources?projectId=${projectId}`,
        ),
  upsertSource: (input: UpsertKnowledgeSourceInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeSource>("upsert_knowledge_source", { input })
      : devApiFetch<KnowledgeSource>("/knowledge/sources", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  upsertSourcesAtomically: (inputs: UpsertKnowledgeSourceInput[]) =>
    hasTauriRuntime()
      ? invoke<KnowledgeSource[]>("upsert_knowledge_sources_atomically", {
          inputs,
        })
      : devApiFetch<KnowledgeSource[]>("/knowledge/sources/batch", {
          method: "POST",
          body: JSON.stringify(inputs),
        }),
  deleteSource: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_knowledge_source", { id })
      : devApiFetch<void>(`/knowledge/sources/${id}`, { method: "DELETE" }),
  previewSourceScope: (input: UpsertKnowledgeSourceInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeSourceScopePreview>("preview_knowledge_source_scope", {
          input,
        })
      : devApiFetch<KnowledgeSourceScopePreview>(
          "/knowledge/sources/scope-preview",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  startSourceSync: (input: StartKnowledgeSourceSyncInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeJob>("start_knowledge_source_sync", { input })
      : devApiFetch<KnowledgeJob>("/knowledge/sources/sync", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listJobs: (limit = 30) =>
    hasTauriRuntime()
      ? invoke<KnowledgeJob[]>("list_knowledge_jobs", { limit })
      : devApiFetch<KnowledgeJob[]>(`/knowledge/jobs?limit=${limit}`),
  cancelJob: (jobKey: string) =>
    hasTauriRuntime()
      ? invoke<KnowledgeJob>("cancel_knowledge_job", { jobKey })
      : devApiFetch<KnowledgeJob>(
          `/knowledge/jobs/${encodeURIComponent(jobKey)}/cancel`,
          {
            method: "POST",
          },
        ),
  retryJob: (jobKey: string) =>
    hasTauriRuntime()
      ? invoke<KnowledgeJob>("retry_knowledge_job", { jobKey })
      : devApiFetch<KnowledgeJob>(
          `/knowledge/jobs/${encodeURIComponent(jobKey)}/retry`,
          {
            method: "POST",
          },
        ),
  listEmbeddingProfiles: () =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingProfile[]>("list_knowledge_embedding_profiles")
      : devApiFetch<KnowledgeEmbeddingProfile[]>(
          "/knowledge/embedding/profiles",
        ),
  upsertEmbeddingProfile: (input: UpsertKnowledgeEmbeddingProfileInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingProfile>(
          "upsert_knowledge_embedding_profile",
          { input },
        )
      : devApiFetch<KnowledgeEmbeddingProfile>(
          "/knowledge/embedding/profiles",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  getLocalEmbeddingRuntimeStatus: () =>
    hasTauriRuntime()
      ? invoke<KnowledgeLocalEmbeddingRuntimeStatus>(
          "get_knowledge_local_embedding_runtime_status",
        )
      : devApiFetch<KnowledgeLocalEmbeddingRuntimeStatus>(
          "/knowledge/embedding/local/runtime",
        ),
  importLocalEmbeddingModel: (
    input: ImportKnowledgeLocalEmbeddingModelInput,
  ) =>
    hasTauriRuntime()
      ? invoke<KnowledgeLocalEmbeddingModelImportResult>(
          "import_knowledge_local_embedding_model",
          { input },
        )
      : devApiFetch<KnowledgeLocalEmbeddingModelImportResult>(
          "/knowledge/embedding/local/import",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  downloadLocalEmbeddingModel: (
    input: DownloadKnowledgeLocalEmbeddingModelInput,
  ) =>
    hasTauriRuntime()
      ? invoke<KnowledgeLocalEmbeddingModelImportResult>(
          "download_knowledge_local_embedding_model",
          { input },
        )
      : devApiFetch<KnowledgeLocalEmbeddingModelImportResult>(
          "/knowledge/embedding/local/download",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  removeLocalEmbeddingModel: (modelKey: string) =>
    hasTauriRuntime()
      ? invoke<void>("remove_knowledge_local_embedding_model", {
          input: { modelKey },
        })
      : devApiFetch<void>(
          `/knowledge/embedding/local/cache/${encodeURIComponent(modelKey)}`,
          {
            method: "DELETE",
          },
        ),
  testLocalEmbeddingProfile: (profileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingProfileTestResult>(
          "test_knowledge_local_embedding_profile",
          {
            profileId,
          },
        )
      : devApiFetch<KnowledgeEmbeddingProfileTestResult>(
          `/knowledge/embedding/profiles/${profileId}/test-local`,
          { method: "POST" },
        ),
  testRemoteEmbeddingProfile: (profileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingProfileTestResult>(
          "test_knowledge_remote_embedding_profile",
          {
            profileId,
          },
        )
      : devApiFetch<KnowledgeEmbeddingProfileTestResult>(
          `/knowledge/embedding/profiles/${profileId}/test-remote`,
          { method: "POST" },
        ),
  listZentaoConnections: () =>
    hasTauriRuntime()
      ? invoke<ZentaoConnection[]>("list_zentao_connections")
      : devApiFetch<ZentaoConnection[]>("/knowledge/zentao/connections"),
  upsertZentaoConnection: (input: UpsertZentaoConnectionInput) =>
    hasTauriRuntime()
      ? invoke<ZentaoConnection>("upsert_zentao_connection", { input })
      : devApiFetch<ZentaoConnection>("/knowledge/zentao/connections", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteZentaoConnection: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_zentao_connection", { id })
      : devApiFetch<void>(`/knowledge/zentao/connections/${id}`, {
          method: "DELETE",
        }),
  probeZentaoConnection: (id: number) =>
    hasTauriRuntime()
      ? invoke<ZentaoCapabilityProbeResult>("probe_zentao_connection", { id })
      : devApiFetch<ZentaoCapabilityProbeResult>(
          `/knowledge/zentao/connections/${id}/probe`,
          {
            method: "POST",
          },
        ),
  discoverZentaoRemoteScopes: (connectionId: number) =>
    hasTauriRuntime()
      ? invoke<ZentaoRemoteScopeItem[]>("discover_zentao_remote_scopes", {
          connectionId,
        })
      : devApiFetch<ZentaoRemoteScopeItem[]>(
          `/knowledge/zentao/connections/${connectionId}/scopes`,
        ),
  listZentaoProjectMappings: (connectionId?: number) =>
    hasTauriRuntime()
      ? invoke<ZentaoProjectMapping[]>("list_zentao_project_mappings", {
          connectionId,
        })
      : devApiFetch<ZentaoProjectMapping[]>(
          connectionId == null
            ? "/knowledge/zentao/mappings"
            : `/knowledge/zentao/mappings?connectionId=${connectionId}`,
        ),
  upsertZentaoProjectMapping: (input: UpsertZentaoProjectMappingInput) =>
    hasTauriRuntime()
      ? invoke<ZentaoProjectMapping>("upsert_zentao_project_mapping", { input })
      : devApiFetch<ZentaoProjectMapping>("/knowledge/zentao/mappings", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  syncZentaoMapping: (input: SyncZentaoMappingInput) =>
    hasTauriRuntime()
      ? invoke<ZentaoSyncResult[]>("sync_zentao_mapping", { input })
      : devApiFetch<ZentaoSyncResult[]>("/knowledge/zentao/sync", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  generateZentaoFactDocuments: (
    input: GenerateZentaoKnowledgeDocumentsInput,
  ) =>
    hasTauriRuntime()
      ? invoke<GenerateZentaoKnowledgeDocumentsResult>(
          "generate_zentao_fact_documents",
          {
            input,
          },
        )
      : devApiFetch<GenerateZentaoKnowledgeDocumentsResult>(
          "/knowledge/zentao/documents/generate",
          { method: "POST", body: JSON.stringify(input) },
        ),
  generateZentaoAiSummary: (input: GenerateZentaoAiSummaryInput) =>
    hasTauriRuntime()
      ? invoke<GenerateZentaoAiSummaryResult>("generate_zentao_ai_summary", {
          input,
        })
      : devApiFetch<GenerateZentaoAiSummaryResult>(
          "/knowledge/zentao/ai-summary/generate",
          { method: "POST", body: JSON.stringify(input) },
        ),
  importAiExperiences: (input: ImportKnowledgeExperiencesInput) =>
    hasTauriRuntime()
      ? invoke<ImportKnowledgeExperiencesResult>(
          "import_knowledge_ai_experiences",
          { input },
        )
      : devApiFetch<ImportKnowledgeExperiencesResult>(
          "/knowledge/experiences/import",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  listCodeSources: () =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSource[]>("list_knowledge_code_sources")
      : devApiFetch<KnowledgeCodeSource[]>("/knowledge/code-sources"),
  upsertCodeSource: (input: UpsertKnowledgeCodeSourceInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSource>("upsert_knowledge_code_source", { input })
      : devApiFetch<KnowledgeCodeSource>("/knowledge/code-sources", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  previewCodeSourceScope: (sourceId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeSourceScopePreview>(
          "preview_knowledge_code_source_scope",
          { sourceId },
        )
      : devApiFetch<KnowledgeSourceScopePreview>(
          `/knowledge/code-sources/${sourceId}/scope-preview`,
        ),
  listCodeSnapshots: (sourceId?: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSnapshot[]>("list_knowledge_code_snapshots", {
          sourceId,
        })
      : devApiFetch<KnowledgeCodeSnapshot[]>(
          sourceId == null
            ? "/knowledge/code-snapshots"
            : `/knowledge/code-snapshots?sourceId=${sourceId}`,
        ),
  captureGitSnapshot: (input: CaptureKnowledgeGitSnapshotInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSnapshot>("capture_knowledge_git_snapshot", {
          input,
        })
      : devApiFetch<KnowledgeCodeSnapshot>("/knowledge/code-snapshots/git", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  captureDirtyWorktreeSnapshot: (
    input: CaptureKnowledgeDirtyWorktreeSnapshotInput,
  ) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSnapshot>(
          "capture_knowledge_dirty_worktree_snapshot",
          { input },
        )
      : devApiFetch<KnowledgeCodeSnapshot>(
          "/knowledge/code-snapshots/dirty-worktree",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  captureLocalDirectorySnapshot: (
    input: CaptureKnowledgeLocalDirectorySnapshotInput,
  ) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSnapshot>(
          "capture_knowledge_local_directory_snapshot",
          { input },
        )
      : devApiFetch<KnowledgeCodeSnapshot>(
          "/knowledge/code-snapshots/local-directory",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  analyzeCodeSnapshot: (snapshotId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeAnalysisResult>("analyze_knowledge_code_snapshot", {
          snapshotId,
        })
      : devApiFetch<KnowledgeCodeAnalysisResult>(
          `/knowledge/code-snapshots/${snapshotId}/analyze`,
          { method: "POST" },
        ),
  generateCodeDocuments: (input: GenerateKnowledgeCodeDocumentsInput) =>
    hasTauriRuntime()
      ? invoke<GenerateKnowledgeCodeDocumentsResult>(
          "generate_knowledge_code_documents",
          {
            input,
          },
        )
      : devApiFetch<GenerateKnowledgeCodeDocumentsResult>(
          "/knowledge/code-snapshots/documents/generate",
          { method: "POST", body: JSON.stringify(input) },
        ),
  searchCodeSymbols: (input: SearchKnowledgeCodeSymbolsInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSymbol[]>("search_knowledge_code_symbols", {
          input,
        })
      : devApiFetch<KnowledgeCodeSymbol[]>(
          "/knowledge/code-snapshots/symbols/search",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  listCodeFiles: (snapshotId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeFile[]>("list_knowledge_code_files", { snapshotId })
      : devApiFetch<KnowledgeCodeFile[]>(
          `/knowledge/code-snapshots/${snapshotId}/files`,
        ),
  getCodeFileContent: (snapshotId: number, fileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeFileContent>("get_knowledge_code_file_content", {
          snapshotId,
          fileId,
        })
      : devApiFetch<KnowledgeCodeFileContent>(
          `/knowledge/code-snapshots/${snapshotId}/files/${fileId}/content`,
        ),
  getCodeCallGraph: (input: KnowledgeCodeCallGraphInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeCallGraph>("get_knowledge_code_call_graph", {
          input,
        })
      : devApiFetch<KnowledgeCodeCallGraph>(
          "/knowledge/code-snapshots/call-graph",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  compareCodeSnapshots: (input: CompareKnowledgeCodeSnapshotsInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSnapshotComparison>(
          "compare_knowledge_code_snapshots",
          { input },
        )
      : devApiFetch<KnowledgeCodeSnapshotComparison>(
          "/knowledge/code-snapshots/compare",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  analyzeCodeImpact: (input: AnalyzeKnowledgeCodeImpactInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeCallGraph>("analyze_knowledge_code_impact", {
          input,
        })
      : devApiFetch<KnowledgeCodeCallGraph>(
          "/knowledge/code-snapshots/impact",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  beginEmbeddingProfileRebuild: (profileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingLifecycleResult>(
          "begin_knowledge_embedding_profile_rebuild",
          { profileId },
        )
      : devApiFetch<KnowledgeEmbeddingLifecycleResult>(
          `/knowledge/embedding/profiles/${profileId}/rebuild/begin`,
          { method: "POST" },
        ),
  buildLocalEmbeddingBatch: (input: BuildKnowledgeEmbeddingBatchInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingBatchResult>(
          "build_knowledge_local_embedding_batch",
          { input },
        )
      : devApiFetch<KnowledgeEmbeddingBatchResult>(
          "/knowledge/embedding/local-batch",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  buildRemoteEmbeddingBatch: (input: BuildKnowledgeEmbeddingBatchInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingBatchResult>(
          "build_knowledge_remote_embedding_batch",
          { input },
        )
      : devApiFetch<KnowledgeEmbeddingBatchResult>(
          "/knowledge/embedding/remote-batch",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  validateEmbeddingProfileRebuild: (profileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingIndexValidation>(
          "validate_knowledge_embedding_profile_rebuild",
          { profileId },
        )
      : devApiFetch<KnowledgeEmbeddingIndexValidation>(
          `/knowledge/embedding/profiles/${profileId}/rebuild/validate`,
        ),
  completeEmbeddingProfileRebuild: (profileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingLifecycleResult>(
          "complete_knowledge_embedding_profile_rebuild",
          { profileId },
        )
      : devApiFetch<KnowledgeEmbeddingLifecycleResult>(
          `/knowledge/embedding/profiles/${profileId}/rebuild/complete`,
          { method: "POST" },
        ),
  activateEmbeddingProfileRebuild: (profileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingLifecycleResult>(
          "activate_knowledge_embedding_profile_rebuild",
          { profileId },
        )
      : devApiFetch<KnowledgeEmbeddingLifecycleResult>(
          `/knowledge/embedding/profiles/${profileId}/activate`,
          { method: "POST" },
        ),
  rollbackEmbeddingProfileRebuild: (previousProfileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingLifecycleResult>(
          "rollback_knowledge_embedding_profile_rebuild",
          { previousProfileId },
        )
      : devApiFetch<KnowledgeEmbeddingLifecycleResult>(
          `/knowledge/embedding/profiles/${previousProfileId}/rollback`,
          { method: "POST" },
        ),
  retireEmbeddingProfileRebuild: (profileId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingLifecycleResult>(
          "retire_knowledge_embedding_profile_rebuild",
          { profileId },
        )
      : devApiFetch<KnowledgeEmbeddingLifecycleResult>(
          `/knowledge/embedding/profiles/${profileId}/retire`,
          { method: "POST" },
        ),
  estimateEmbeddingRebuild: (input: EstimateKnowledgeEmbeddingRebuildInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeEmbeddingRebuildEstimate>(
          "estimate_knowledge_embedding_rebuild",
          { input },
        )
      : devApiFetch<KnowledgeEmbeddingRebuildEstimate>(
          "/knowledge/embedding/rebuild-estimate",
          { method: "POST", body: JSON.stringify(input) },
        ),
  calculateEmbeddingFingerprint: (input: KnowledgeEmbeddingFingerprintInput) =>
    hasTauriRuntime()
      ? invoke<string>("calculate_knowledge_embedding_fingerprint", { input })
      : devApiFetch<string>("/knowledge/embedding/fingerprint", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  searchFts: (input: KnowledgeSearchInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeSearchHit[]>("search_knowledge_fts", { input })
      : devApiFetch<KnowledgeSearchHit[]>("/knowledge/search/fts", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  rebuildFts: () =>
    hasTauriRuntime()
      ? invoke<number>("rebuild_knowledge_fts")
      : devApiFetch<number>("/knowledge/search/fts/rebuild", {
          method: "POST",
        }),
  searchActiveVectors: (input: KnowledgeVectorSearchInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeSearchHit[]>("search_active_knowledge_vectors", {
          input,
        })
      : devApiFetch<KnowledgeSearchHit[]>("/knowledge/embedding/search", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  previewRagContext: (search: KnowledgeSearchInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeRagContextPreview>("preview_knowledge_rag_context", {
          search,
        })
      : devApiFetch<KnowledgeRagContextPreview>(
          "/knowledge/rag/context-preview",
          {
            method: "POST",
            body: JSON.stringify(search),
          },
        ),
  ask: (input: KnowledgeAskInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeAskResult>("ask_knowledge", { input })
      : devApiFetch<KnowledgeAskResult>("/knowledge/ask", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  runFixedRetrievalEvaluation: (
    input?: RunKnowledgeRetrievalEvaluationInput,
  ) =>
    hasTauriRuntime()
      ? invoke<KnowledgeRetrievalEvaluationRun>(
          "run_fixed_knowledge_retrieval_evaluation",
          { input },
        )
      : devApiFetch<KnowledgeRetrievalEvaluationRun>(
          "/knowledge/evaluation/run",
          {
            method: "POST",
            body: JSON.stringify(input ?? {}),
          },
        ),
  listDocuments: (input?: KnowledgeListInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgePage<KnowledgeDocument>>("list_knowledge_documents", {
          input,
        })
      : devApiFetch<KnowledgePage<KnowledgeDocument>>(documentListQuery(input)),
  getDocumentDetail: (documentId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeDocumentDetail>("get_knowledge_document_detail", {
          documentId,
        })
      : devApiFetch<KnowledgeDocumentDetail>(
          `/knowledge/documents/${documentId}`,
        ),
  listVersions: (documentId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeDocumentVersion[]>("list_knowledge_document_versions", {
          documentId,
        })
      : devApiFetch<KnowledgeDocumentVersion[]>(
          `/knowledge/documents/${documentId}/versions`,
        ),
  previewDocumentDeletion: (documentId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeDocumentDeletionImpactPreview>(
          "preview_knowledge_document_deletion",
          { documentId },
        )
      : devApiFetch<KnowledgeDocumentDeletionImpactPreview>(
          `/knowledge/documents/${documentId}/deletion-preview`,
        ),
  deleteDocument: (documentId: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_knowledge_document", { id: documentId })
      : devApiFetch<void>(`/knowledge/documents/${documentId}`, {
          method: "DELETE",
        }),
  restoreDocument: (documentId: number) =>
    hasTauriRuntime()
      ? invoke<RestoreKnowledgeDocumentResult>("restore_knowledge_document", {
          documentId,
        })
      : devApiFetch<RestoreKnowledgeDocumentResult>(
          `/knowledge/documents/${documentId}/restore`,
          { method: "POST" },
        ),
  listChunks: (documentVersionId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeChunk[]>("list_knowledge_document_chunks", {
          documentVersionId,
        })
      : devApiFetch<KnowledgeChunk[]>(
          `/knowledge/document-versions/${documentVersionId}/chunks`,
        ),
  compareVersions: (input: CompareKnowledgeDocumentVersionsInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeDocumentComparison>(
          "compare_knowledge_document_versions",
          { input },
        )
      : devApiFetch<KnowledgeDocumentComparison>(
          "/knowledge/document-versions/compare",
          { method: "POST", body: JSON.stringify(input) },
        ),
  getCitationDetail: (chunkId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCitationDetail>("get_knowledge_citation_detail", {
          chunkId,
        })
      : devApiFetch<KnowledgeCitationDetail>(`/knowledge/citations/${chunkId}`),
  previewParseAndChunk: (input: KnowledgeParseAndChunkInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeParseAndChunkResult>(
          "preview_knowledge_parse_and_chunk",
          { input },
        )
      : devApiFetch<KnowledgeParseAndChunkResult>("/knowledge/parse-preview", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  parseAndIndexVersion: (
    documentVersionId: number,
    options?: KnowledgeChunkOptions,
  ) =>
    hasTauriRuntime()
      ? invoke<KnowledgeParseAndChunkResult>(
          "parse_and_index_knowledge_document_version",
          { documentVersionId, options },
        )
      : devApiFetch<KnowledgeParseAndChunkResult>(
          `/knowledge/document-versions/${documentVersionId}/parse`,
          { method: "POST", body: JSON.stringify(options ?? null) },
        ),
};
