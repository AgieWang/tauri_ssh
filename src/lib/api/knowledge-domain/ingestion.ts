import { hasTauriRuntime, invoke } from "../client";
import { knowledgeApi } from "../knowledge";
import type {
  KnowledgeDocumentUploadResult,
  KnowledgeDocumentUploadBatchResult,
  PreparedKnowledgeUploadDirectory,
  PreparedKnowledgeUploadFile,
  PrepareKnowledgeUploadDirectoryInput,
  PrepareKnowledgeUploadFileInput,
  UploadKnowledgeAssetBatchInput,
  UploadKnowledgeAssetInput,
} from "@/types/knowledge-domain/ingestion";

export const knowledgeIngestionApi = {
  upsertSource: knowledgeApi.upsertSource,
  upsertSourcesAtomically: knowledgeApi.upsertSourcesAtomically,
  previewParse: knowledgeApi.previewParseAndChunk,
  parseAndIndexVersion: knowledgeApi.parseAndIndexVersion,
  previewSourceScope: knowledgeApi.previewSourceScope,
  startSourceSync: knowledgeApi.startSourceSync,
  prepareUploadFile: (input: PrepareKnowledgeUploadFileInput) => {
    if (!hasTauriRuntime()) {
      return Promise.reject(
        new Error("选择上传文件需要在 Tauri 桌面端运行。"),
      ) as Promise<PreparedKnowledgeUploadFile>;
    }
    return invoke<PreparedKnowledgeUploadFile>(
      "prepare_knowledge_upload_file",
      {
        input,
      },
    );
  },
  prepareUploadDirectory: (input: PrepareKnowledgeUploadDirectoryInput) => {
    if (!hasTauriRuntime()) {
      return Promise.reject(
        new Error("选择上传文件夹需要在 Tauri 桌面端运行。"),
      ) as Promise<PreparedKnowledgeUploadDirectory>;
    }
    return invoke<PreparedKnowledgeUploadDirectory>(
      "prepare_knowledge_upload_directory",
      {
        input,
      },
    );
  },
  createDocumentUpload: (input: UploadKnowledgeAssetInput) => {
    if (!hasTauriRuntime()) {
      return Promise.reject(
        new Error("创建文档导入任务需要在 Tauri 桌面端运行。"),
      ) as Promise<KnowledgeDocumentUploadResult>;
    }
    return invoke<KnowledgeDocumentUploadResult>(
      "create_knowledge_document_upload",
      { input },
    );
  },
  createDocumentUploadBatch: (input: UploadKnowledgeAssetBatchInput) => {
    if (!hasTauriRuntime()) {
      return Promise.reject(
        new Error("创建文档导入任务需要在 Tauri 桌面端运行。"),
      ) as Promise<KnowledgeDocumentUploadBatchResult>;
    }
    return invoke<KnowledgeDocumentUploadBatchResult>(
      "create_knowledge_document_upload_batch",
      { input },
    );
  },
};
