import type {
  GenerateKnowledgeCodeDocumentsResult,
  KnowledgeCodeAnalysisResult,
  KnowledgeCodeSnapshot,
  KnowledgeCodeSource,
  UpsertKnowledgeCodeSourceInput,
} from "@/types/knowledge";

export type {
  GenerateKnowledgeCodeDocumentsResult,
  KnowledgeCodeAnalysisResult,
  KnowledgeCodeSnapshot,
  KnowledgeCodeSource,
  UpsertKnowledgeCodeSourceInput,
};

/** 仅查询该项目的代码源；项目范围在 Rust IPC 边界再次校验。 */
export interface ListKnowledgeAnalysisCodeSnapshotsInput {
  projectId: number;
  sourceId?: number | null;
}

/**
 * 新工作台以 projectVersionId 表述版本范围，领域 API 在兼容层映射为旧快照接口的
 * releaseId；不传版本时只能形成未绑定发布版本的快照，不能当作发布事实。
 */
export interface CaptureKnowledgeAnalysisGitSnapshotInput {
  projectId: number;
  sourceId: number;
  gitRef: string;
  projectVersionId?: number | null;
}

export interface AnalyzeKnowledgeAnalysisSnapshotInput {
  projectId: number;
  snapshotId: number;
}

export interface GenerateKnowledgeAnalysisDocumentsInput {
  projectId: number;
  snapshotId: number;
}

export interface CreateKnowledgeAnalysisDraftInput {
  projectId: number;
  projectVersionId: number;
  snapshotIds: number[];
  /** 留空时由后端选择当前已配置的聊天 Provider。 */
  providerKey?: string | null;
  templateKey?: string | null;
}

export interface KnowledgeAnalysisDraft {
  id: number;
  analysisRunId: number;
  projectId: number;
  projectVersionId: number;
  snapshotIds: number[];
  providerKey: string;
  model: string;
  templateKey: string;
  content: string;
  claimRefs: string[];
  status: string;
  confirmedDocumentVersionId?: number | null;
}

export interface ConfirmKnowledgeAnalysisDraftInput {
  draftId: number;
  title: string;
  content: string;
  versionLabel: string;
  authorLabel?: string | null;
}

export interface ConfirmKnowledgeAnalysisDraftResult {
  draft: KnowledgeAnalysisDraft;
  document: {
    documentId: number;
    documentVersionId: number;
    parentVersionId?: number | null;
    contentHash: string;
    indexJobId: number;
    indexJobStatus: string;
  };
}
