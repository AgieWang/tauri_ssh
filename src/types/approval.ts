export type ApprovalStatus =
  "pending" | "approved" | "rejected" | "cancelled" | "expired";

export type ApprovalRisk =
  "readonly" | "L1" | "L2" | "L3" | "review" | "high" | "blocked";

export interface ApprovalRequest {
  id: number;
  source: string;
  requester: string;
  serverAlias: string;
  action: string;
  risk: ApprovalRisk | string;
  status: ApprovalStatus | string;
  command: string;
  resource: string;
  reason: string;
  summary: string;
  payloadJson: string;
  decisionNote: string;
  decidedBy: string;
  decidedAt: string | null;
  expiresAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface CreateApprovalRequestInput {
  source: string;
  requester: string;
  serverAlias: string;
  action: string;
  risk: ApprovalRisk | string;
  command: string;
  resource: string;
  reason: string;
  summary: string;
  payloadJson?: string | null;
  expiresAt?: string | null;
}

export interface DecideApprovalRequestInput {
  id: number;
  decision: "approved" | "rejected" | "cancelled";
  note: string;
  decidedBy: string;
}

export interface ListApprovalRequestsInput {
  status?: ApprovalStatus | "all";
  limit?: number;
}
