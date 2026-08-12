export type KnowledgeDomainJobType =
  "sync" | "upload" | "parse" | "analysis" | "embedding" | "graph" | "backfill";

export interface KnowledgeDomainJobRequest {
  jobType: KnowledgeDomainJobType;
  idempotencyKey: string;
  projectId?: number | null;
  projectVersionId?: number | null;
  payloadRef?: string | null;
}
