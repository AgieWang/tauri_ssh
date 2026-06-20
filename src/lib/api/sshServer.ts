import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  SshConfigImportResult,
  SshServer,
  SshServerConnectionTestInput,
  SshServerTestResult,
  UpsertSshServerInput,
} from "@/types";

export const sshServerApi = {
  list: () =>
    hasTauriRuntime()
      ? invoke<SshServer[]>("list_ssh_servers")
      : devApiFetch<SshServer[]>("/ssh-servers"),
  upsert: (input: UpsertSshServerInput) =>
    hasTauriRuntime()
      ? invoke<SshServer>("upsert_ssh_server", { input })
      : devApiFetch<SshServer>("/ssh-servers", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  delete: (alias: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_ssh_server", { alias })
      : devApiFetch<void>(`/ssh-servers/${encodeURIComponent(alias)}`, {
          method: "DELETE",
        }),
  importSshConfig: (path?: string | null) =>
    hasTauriRuntime()
      ? invoke<SshConfigImportResult>("import_ssh_config", { path: path ?? null })
      : devApiFetch<SshConfigImportResult>("/ssh-servers/import", {
          method: "POST",
          body: JSON.stringify(path ?? null),
        }),
  test: (alias: string) =>
    hasTauriRuntime()
      ? invoke<SshServerTestResult>("test_ssh_server", { alias })
      : devApiFetch<SshServerTestResult>(
          `/ssh-servers/${encodeURIComponent(alias)}/test`,
          { method: "POST" },
        ),
  testConnection: (input: SshServerConnectionTestInput) =>
    hasTauriRuntime()
      ? invoke<SshServerTestResult>("test_ssh_server_connection", { input })
      : devApiFetch<SshServerTestResult>("/ssh-servers/test-connection", {
          method: "POST",
          body: JSON.stringify(input),
        }),
};
