export interface KnowledgeDocumentDraftInput {
  draftId?: number | null;
  documentId?: number | null;
  projectId: number;
  title: string;
  content: string;
  docType?: "markdown" | "rich_text";
  baseVersionId?: number | null;
  revision?: number | null;
  editorLabel?: string | null;
}

export interface KnowledgeDocumentDraft {
  id: number;
  documentId: number | null;
  projectId: number;
  title: string;
  content: string;
  docType: "markdown" | "rich_text";
  baseVersionId: number | null;
  revision: number;
  editorLabel: string;
}

export interface KnowledgeDocumentDraftSaveResult {
  draft: KnowledgeDocumentDraft;
  conflict: boolean;
}

export interface RestoreKnowledgeDocumentVersionToDraftInput {
  sourceVersionId: number;
  draftId?: number | null;
  revision?: number | null;
  editorLabel?: string | null;
}

/** 并发冲突时 draft 为当前服务端草稿，可与 sourceVersion 的历史正文比较。 */
export interface RestoreKnowledgeDocumentVersionToDraftResult {
  sourceVersion: import("@/types").KnowledgeDocumentVersion;
  draft: KnowledgeDocumentDraft;
  conflict: boolean;
}

export interface CommitKnowledgeDocumentDraftInput {
  draftId: number;
  revision: number;
  versionLabel: string;
  projectVersionId?: number | null;
  /** 仅支持明确的 project_all_versions，不能省略范围。 */
  crossVersionScope?: "project_all_versions" | null;
  commitMessage?: string | null;
  authorLabel?: string | null;
}

export interface KnowledgeDocumentCommitResult {
  documentId: number;
  documentVersionId: number;
  parentVersionId: number | null;
  contentHash: string;
  indexJobId: number;
  indexJobStatus: "queued";
}

/** 仅用于页面内展示的受控图片副本；绝不包含应用数据目录或原始文件路径。 */
export interface KnowledgeDocumentImagePreview {
  documentId: number;
  mimeType: string;
  sizeBytes: number;
  width: number | null;
  height: number | null;
  dataUrl: string;
}

export interface KnowledgeDocumentVersionBindingInput {
  documentVersionId: number;
  projectVersionId?: number | null;
  repositoryBindingId?: number | null;
  crossVersionScope?: string | null;
}
