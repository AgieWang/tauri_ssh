import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  AuditLog,
  AuditLogExportResult,
  CreateAuditLogInput,
  ListAuditLogsInput,
} from "@/types";

function toQuery(input: ListAuditLogsInput) {
  const params = new URLSearchParams();
  Object.entries(input).forEach(([key, value]) => {
    if (value !== undefined && value !== null && String(value).trim() !== "") {
      params.set(key, String(value));
    }
  });
  const query = params.toString();
  return query ? `?${query}` : "";
}

export const auditApi = {
  list: (input: ListAuditLogsInput = {}) =>
    hasTauriRuntime()
      ? invoke<AuditLog[]>("list_audit_logs", { input })
      : devApiFetch<AuditLog[]>(`/audit-logs${toQuery(input)}`),
  create: (input: CreateAuditLogInput) =>
    hasTauriRuntime()
      ? invoke<AuditLog>("create_audit_log", { input })
      : devApiFetch<AuditLog>("/audit-logs", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  export: (input: ListAuditLogsInput = {}) =>
    hasTauriRuntime()
      ? invoke<AuditLogExportResult>("export_audit_logs", { input })
      : devApiFetch<AuditLogExportResult>("/audit-logs/export", {
          method: "POST",
          body: JSON.stringify(input),
        }),
};
