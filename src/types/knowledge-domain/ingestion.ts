export interface UploadKnowledgeAssetInput {
  projectId: number;
  projectVersionId?: number | null;
  crossVersionScope?: "project_all_versions" | null;
  fileHandle: string;
  displayName?: string | null;
  /** 文件夹上传时用于在文档列表恢复来源文件夹展示；普通文件上传省略。 */
  sourceFolderName?: string | null;
  allowRemoteOcr?: boolean;
  ocrProviderKey?: string | null;
}

export interface UploadKnowledgeAssetFileInput {
  fileHandle: string;
  displayName?: string | null;
  allowRemoteOcr?: boolean;
  ocrProviderKey?: string | null;
}

export interface UploadKnowledgeAssetBatchInput {
  projectId: number;
  projectVersionId?: number | null;
  crossVersionScope?: "project_all_versions" | null;
  /** 一次文件夹选择对应一个来源文件夹，文件仍按独立文档入库。 */
  sourceFolderName?: string | null;
  files: UploadKnowledgeAssetFileInput[];
}

export interface PrepareKnowledgeUploadFileInput {
  selectedPath: string;
}

export interface PrepareKnowledgeUploadDirectoryInput {
  selectedPath: string;
}

export interface PreparedKnowledgeUploadFile {
  fileHandle: string;
  displayName: string;
  sizeBytes: number;
}

export interface PreparedKnowledgeUploadDirectory {
  directoryName: string;
  files: PreparedKnowledgeUploadFile[];
  skippedCount: number;
  totalSizeBytes: number;
}

export interface KnowledgeDocumentUploadResult {
  documentId: number;
  assetId: number;
  importJobId: number;
  importJobKey: string;
  status: "queued";
}

export interface KnowledgeDocumentUploadBatchItemResult {
  displayName: string;
  result: KnowledgeDocumentUploadResult | null;
  errorMessage: string | null;
}

export interface KnowledgeDocumentUploadBatchResult {
  items: KnowledgeDocumentUploadBatchItemResult[];
}
