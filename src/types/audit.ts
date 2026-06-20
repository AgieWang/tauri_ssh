export type AuditRisk = "L0" | "L1" | "L2" | "L3" | "readonly" | "blocked" | "ai";

export interface AuditLog {
  id: number;
  occurredAt: string;
  actor: string;
  source: string;
  serverAlias: string;
  action: string;
  risk: AuditRisk;
  result: string;
  summary: string;
  detailJson: string;
  requestId: string;
  approvalId: number | null;
  createdAt: string;
}

export interface ListAuditLogsInput {
  actor?: string | null;
  source?: string | null;
  serverAlias?: string | null;
  action?: string | null;
  risk?: AuditRisk | null;
  result?: string | null;
  keyword?: string | null;
  limit?: number | null;
}

export interface CreateAuditLogInput {
  actor: string;
  source: string;
  serverAlias: string;
  action: string;
  risk: AuditRisk;
  result: string;
  summary: string;
  detailJson?: string | null;
  requestId?: string | null;
  approvalId?: number | null;
}

export interface AuditLogExportResult {
  fileName: string;
  content: string;
  count: number;
}
