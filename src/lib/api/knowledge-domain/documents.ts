import { devApiFetch, hasTauriRuntime, invoke } from "../client";
import { knowledgeApi } from "../knowledge";
import type {
  CommitKnowledgeDocumentDraftInput,
  KnowledgeDocumentCommitResult,
  KnowledgeDocumentDraftInput,
  KnowledgeDocumentDraftSaveResult,
  KnowledgeDocumentImagePreview,
  RestoreKnowledgeDocumentVersionToDraftInput,
  RestoreKnowledgeDocumentVersionToDraftResult,
} from "@/types/knowledge-domain/documents";
import type {
  KnowledgeDocument,
  KnowledgeListInput,
  KnowledgePage,
  RestoreKnowledgeDocumentResult,
} from "@/types";

function deletedDocumentListQuery(input?: KnowledgeListInput) {
  const query = new URLSearchParams();
  if (input?.projectId != null) query.set("projectId", String(input.projectId));
  if (input?.releaseId != null) query.set("releaseId", String(input.releaseId));
  if (input?.sourceId != null) query.set("sourceId", String(input.sourceId));
  if (input?.keyword) query.set("keyword", input.keyword);
  if (input?.status) query.set("status", input.status);
  if (input?.offset != null) query.set("offset", String(input.offset));
  if (input?.limit != null) query.set("limit", String(input.limit));
  const suffix = query.toString();
  return suffix
    ? `/knowledge/documents/deleted?${suffix}`
    : "/knowledge/documents/deleted";
}

export const knowledgeDocumentsApi = {
  list: knowledgeApi.listDocuments,
  listDeleted: (input?: KnowledgeListInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgePage<KnowledgeDocument>>(
          "list_deleted_knowledge_documents",
          { input },
        )
      : devApiFetch<KnowledgePage<KnowledgeDocument>>(
          deletedDocumentListQuery(input),
        ),
  detail: knowledgeApi.getDocumentDetail,
  retryProcessing: (jobKey: string) => knowledgeApi.retryJob(jobKey),
  imagePreview: (documentId: number) => {
    if (!hasTauriRuntime()) {
      return Promise.reject(
        new Error("图片预览需要在 Tauri 桌面端运行。"),
      ) as Promise<KnowledgeDocumentImagePreview>;
    }
    return invoke<KnowledgeDocumentImagePreview>(
      "get_knowledge_document_image_preview",
      { documentId },
    );
  },
  listVersions: knowledgeApi.listVersions,
  listChunks: knowledgeApi.listChunks,
  compareVersions: knowledgeApi.compareVersions,
  citationDetail: knowledgeApi.getCitationDetail,
  previewDeletion: knowledgeApi.previewDocumentDeletion,
  softDelete: knowledgeApi.deleteDocument,
  restore: (documentId: number): Promise<RestoreKnowledgeDocumentResult> =>
    knowledgeApi.restoreDocument(documentId),
  saveDraft: (input: KnowledgeDocumentDraftInput) => {
    if (!hasTauriRuntime()) {
      return Promise.reject(
        new Error("保存草稿需要在 Tauri 桌面端运行。"),
      ) as Promise<KnowledgeDocumentDraftSaveResult>;
    }
    return invoke<KnowledgeDocumentDraftSaveResult>(
      "save_knowledge_document_draft",
      { input },
    );
  },
  commitDraft: (input: CommitKnowledgeDocumentDraftInput) => {
    if (!hasTauriRuntime()) {
      return Promise.reject(
        new Error("提交文档需要在 Tauri 桌面端运行。"),
      ) as Promise<KnowledgeDocumentCommitResult>;
    }
    return invoke<KnowledgeDocumentCommitResult>(
      "commit_knowledge_document_draft",
      { input },
    );
  },
  restoreVersionToDraft: (
    input: RestoreKnowledgeDocumentVersionToDraftInput,
  ) =>
    hasTauriRuntime()
      ? invoke<RestoreKnowledgeDocumentVersionToDraftResult>(
          "restore_knowledge_document_version_to_draft",
          { input },
        )
      : devApiFetch<RestoreKnowledgeDocumentVersionToDraftResult>(
          "/knowledge/document-versions/restore-draft",
          { method: "POST", body: JSON.stringify(input) },
        ),
};
