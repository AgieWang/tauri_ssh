import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  CreateSecureCredentialSessionInput,
  ListSecureCredentialAuditLogsInput,
  ListSecureCredentialSessionsInput,
  ListSecureCredentialsInput,
  RotateSecureCredentialInput,
  SecureCredential,
  SecureCredentialAuditLog,
  SecureCredentialGitReadInput,
  SecureCredentialGitWriteInput,
  SecureCredentialGitWriteResult,
  SecureCredentialHttpRequestInput,
  SecureCredentialHttpRequestResult,
  SecureCredentialHttpWriteInput,
  SecureCredentialSession,
  SecureCredentialSessionStatus,
  SecureCredentialOverview,
  SecureCredentialPolicySettings,
  SecureCredentialProviderReadResult,
  SecureCredentialProviderTestInput,
  SecureCredentialProviderTestResult,
  SecureCredentialRepository,
  SecureCredentialRepositoryListInput,
  SetSecureCredentialEnabledInput,
  UpdateSecureCredentialPolicySettingsInput,
  UpsertSecureCredentialInput,
} from "@/types";

export const secureCredentialApi = {
  overview: () =>
    hasTauriRuntime()
      ? invoke<SecureCredentialOverview>("get_secure_credential_overview")
      : devApiFetch<SecureCredentialOverview>("/secure-credentials/overview"),
  policySettings: () =>
    hasTauriRuntime()
      ? invoke<SecureCredentialPolicySettings>("get_secure_credential_policy_settings")
      : devApiFetch<SecureCredentialPolicySettings>("/secure-credentials/policies"),
  updatePolicySettings: (input: UpdateSecureCredentialPolicySettingsInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialPolicySettings>("update_secure_credential_policy_settings", {
          input,
        })
      : devApiFetch<SecureCredentialPolicySettings>("/secure-credentials/policies", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  list: (input?: ListSecureCredentialsInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredential[]>("list_secure_credentials", { input })
      : devApiFetch<SecureCredential[]>("/secure-credentials/list", {
          method: "POST",
          body: JSON.stringify(input ?? null),
        }),
  listAuditLogs: (input?: ListSecureCredentialAuditLogsInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialAuditLog[]>("list_secure_credential_audit_logs", { input })
      : devApiFetch<SecureCredentialAuditLog[]>("/secure-credentials/audit-logs", {
          method: "POST",
          body: JSON.stringify(input ?? null),
        }),
  upsert: (input: UpsertSecureCredentialInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredential>("upsert_secure_credential", { input })
      : devApiFetch<SecureCredential>("/secure-credentials", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  rotate: (input: RotateSecureCredentialInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredential>("rotate_secure_credential", { input })
      : devApiFetch<SecureCredential>("/secure-credentials/rotate", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  setEnabled: (input: SetSecureCredentialEnabledInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredential>("set_secure_credential_enabled", { input })
      : devApiFetch<SecureCredential>("/secure-credentials/enabled", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  delete: (credentialKey: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_secure_credential", { credentialKey })
      : devApiFetch<void>(`/secure-credentials/${encodeURIComponent(credentialKey)}`, {
          method: "DELETE",
        }),
  listSessions: (input?: ListSecureCredentialSessionsInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialSession[]>("list_secure_credential_sessions", { input })
      : devApiFetch<SecureCredentialSession[]>("/secure-credentials/sessions/list", {
          method: "POST",
          body: JSON.stringify(input ?? null),
        }),
  createSession: (input: CreateSecureCredentialSessionInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialSession>("create_secure_credential_session", { input })
      : devApiFetch<SecureCredentialSession>("/secure-credentials/sessions", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  sessionStatus: (sessionId: string) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialSessionStatus>("get_secure_credential_session_status", { sessionId })
      : devApiFetch<SecureCredentialSessionStatus>(
          `/secure-credentials/sessions/${encodeURIComponent(sessionId)}/status`,
        ),
  revokeSession: (sessionId: string) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialSession>("revoke_secure_credential_session", { sessionId })
      : devApiFetch<SecureCredentialSession>(
          `/secure-credentials/sessions/${encodeURIComponent(sessionId)}/revoke`,
          { method: "POST" },
        ),
  testProvider: (input: SecureCredentialProviderTestInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialProviderTestResult>("test_secure_credential_provider", { input })
      : devApiFetch<SecureCredentialProviderTestResult>("/secure-credentials/provider/test", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listRepositories: (input: SecureCredentialRepositoryListInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialRepository[]>("list_secure_credential_repositories", { input })
      : devApiFetch<SecureCredentialRepository[]>("/secure-credentials/provider/repositories", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  gitReadonlyRequest: (input: SecureCredentialGitReadInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialProviderReadResult>("secure_credential_git_readonly_request", {
          input,
        })
      : devApiFetch<SecureCredentialProviderReadResult>(
          "/secure-credentials/provider/git-readonly",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  httpReadonlyRequest: (input: SecureCredentialHttpRequestInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialHttpRequestResult>("secure_credential_http_readonly_request", {
          input,
        })
      : devApiFetch<SecureCredentialHttpRequestResult>(
          "/secure-credentials/provider/http-readonly",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  httpWriteRequest: (input: SecureCredentialHttpWriteInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialHttpRequestResult>("secure_credential_http_write_request", {
          input,
        })
      : devApiFetch<SecureCredentialHttpRequestResult>("/secure-credentials/provider/http-write", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  executeGitWrite: (input: SecureCredentialGitWriteInput) =>
    hasTauriRuntime()
      ? invoke<SecureCredentialGitWriteResult>("execute_secure_credential_git_write", {
          input,
        })
      : devApiFetch<SecureCredentialGitWriteResult>("/secure-credentials/provider/git-write", {
          method: "POST",
          body: JSON.stringify(input),
        }),
};
