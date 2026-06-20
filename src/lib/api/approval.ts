import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  ApprovalRequest,
  CreateApprovalRequestInput,
  DecideApprovalRequestInput,
  ListApprovalRequestsInput,
} from "@/types/approval";

export const approvalApi = {
  list(input: ListApprovalRequestsInput = {}): Promise<ApprovalRequest[]> {
    if (hasTauriRuntime()) {
      return invoke("list_approval_requests", { input });
    }
    const params = new URLSearchParams();
    if (input.status) params.set("status", input.status);
    if (input.limit) params.set("limit", String(input.limit));
    const query = params.toString();
    return devApiFetch(`/approvals${query ? `?${query}` : ""}`);
  },

  create(input: CreateApprovalRequestInput): Promise<ApprovalRequest> {
    if (hasTauriRuntime()) {
      return invoke("create_approval_request", { input });
    }
    return devApiFetch("/approvals", {
      method: "POST",
      body: JSON.stringify(input),
    });
  },

  decide(input: DecideApprovalRequestInput): Promise<ApprovalRequest> {
    if (hasTauriRuntime()) {
      return invoke("decide_approval_request", { input });
    }
    return devApiFetch("/approvals/decide", {
      method: "POST",
      body: JSON.stringify(input),
    });
  },
};
