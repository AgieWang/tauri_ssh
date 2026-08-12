import { devApiFetch, hasTauriRuntime, invoke } from "../client";
import { knowledgeApi } from "../knowledge";
import type {
  AnalyzeKnowledgeAnalysisSnapshotInput,
  CaptureKnowledgeAnalysisGitSnapshotInput,
  GenerateKnowledgeAnalysisDocumentsInput,
  CreateKnowledgeAnalysisDraftInput,
  ConfirmKnowledgeAnalysisDraftInput,
  ConfirmKnowledgeAnalysisDraftResult,
  GenerateKnowledgeCodeDocumentsResult,
  KnowledgeAnalysisDraft,
  KnowledgeCodeAnalysisResult,
  KnowledgeCodeSnapshot,
  KnowledgeCodeSource,
  ListKnowledgeAnalysisCodeSnapshotsInput,
  UpsertKnowledgeCodeSourceInput,
} from "@/types/knowledge-domain/analysis";

/**
 * 源码分析领域的真实 IPC 出口。桌面端使用新领域 Command；浏览器开发验收复用旧 Dev API
 * 的等价只读/固定报告路径；AI 草稿也通过同一项目版本、快照与来源授权边界。
 */
export const knowledgeAnalysisApi = {
  // 目录工作台首次登记的 Git 来源，同时登记为本地静态分析来源；调用仍复用旧服务，
  // 以保留路径、符号链接和远程处理授权的统一校验。
  upsertCodeSource: (input: UpsertKnowledgeCodeSourceInput) =>
    knowledgeApi.upsertCodeSource(input),
  listCodeSources: (projectId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSource[]>("list_knowledge_analysis_code_sources", {
          projectId,
        })
      : devApiFetch<KnowledgeCodeSource[]>(
          `/knowledge/projects/${projectId}/analysis/code-sources`,
        ),
  listCodeSnapshots: ({
    projectId,
    sourceId,
  }: ListKnowledgeAnalysisCodeSnapshotsInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSnapshot[]>(
          "list_knowledge_analysis_code_snapshots",
          {
            projectId,
            sourceId,
          },
        )
      : devApiFetch<KnowledgeCodeSnapshot[]>(
          sourceId == null
            ? `/knowledge/projects/${projectId}/analysis/code-snapshots`
            : `/knowledge/projects/${projectId}/analysis/code-snapshots?sourceId=${sourceId}`,
        ),
  captureGitSnapshot: (input: CaptureKnowledgeAnalysisGitSnapshotInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeSnapshot>(
          "capture_knowledge_analysis_git_snapshot",
          {
            projectId: input.projectId,
            input: {
              sourceId: input.sourceId,
              gitRef: input.gitRef,
              releaseId: input.projectVersionId ?? null,
            },
          },
        )
      : devApiFetch<KnowledgeCodeSnapshot>(
          `/knowledge/projects/${input.projectId}/analysis/code-snapshots/git`,
          {
            method: "POST",
            body: JSON.stringify({
              sourceId: input.sourceId,
              gitRef: input.gitRef,
              releaseId: input.projectVersionId ?? null,
            }),
          },
        ),
  analyzeSnapshot: ({
    projectId,
    snapshotId,
  }: AnalyzeKnowledgeAnalysisSnapshotInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCodeAnalysisResult>(
          "analyze_knowledge_analysis_snapshot",
          {
            projectId,
            snapshotId,
          },
        )
      : devApiFetch<KnowledgeCodeAnalysisResult>(
          `/knowledge/projects/${projectId}/analysis/code-snapshots/${snapshotId}/analyze`,
          { method: "POST" },
        ),
  generateDocuments: ({
    projectId,
    snapshotId,
  }: GenerateKnowledgeAnalysisDocumentsInput) =>
    hasTauriRuntime()
      ? invoke<GenerateKnowledgeCodeDocumentsResult>(
          "generate_knowledge_analysis_documents",
          { projectId, input: { snapshotId } },
        )
      : devApiFetch<GenerateKnowledgeCodeDocumentsResult>(
          `/knowledge/projects/${projectId}/analysis/code-snapshots/documents/generate`,
          { method: "POST", body: JSON.stringify({ snapshotId }) },
        ),
  createAiDraft: (input: CreateKnowledgeAnalysisDraftInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeAnalysisDraft>("create_knowledge_analysis_ai_draft", {
          input,
        })
      : devApiFetch<KnowledgeAnalysisDraft>(
          `/knowledge/projects/${input.projectId}/analysis/ai-drafts`,
          { method: "POST", body: JSON.stringify(input) },
        ),
  confirmAiDraft: (input: ConfirmKnowledgeAnalysisDraftInput) =>
    hasTauriRuntime()
      ? invoke<ConfirmKnowledgeAnalysisDraftResult>(
          "confirm_knowledge_analysis_ai_draft",
          { input },
        )
      : devApiFetch<ConfirmKnowledgeAnalysisDraftResult>(
          `/knowledge/analysis/ai-drafts/${input.draftId}/confirm`,
          { method: "POST", body: JSON.stringify(input) },
        ),
};
