import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  JumpServerOpenResult,
  JumpServerSession,
  UpsertJumpServerSessionInput,
} from "@/types";

async function openExternalUrl(url: string) {
  if (hasTauriRuntime()) {
    try {
      const opener = await import("@tauri-apps/plugin-opener");
      await opener.openUrl(url);
      return;
    } catch {
      window.open(url, "_blank", "noopener,noreferrer");
      return;
    }
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

export const jumpserverApi = {
  list: () =>
    hasTauriRuntime()
      ? invoke<JumpServerSession[]>("list_jumpserver_sessions")
      : devApiFetch<JumpServerSession[]>("/jumpserver-sessions"),
  upsert: (input: UpsertJumpServerSessionInput) =>
    hasTauriRuntime()
      ? invoke<JumpServerSession>("upsert_jumpserver_session", { input })
      : devApiFetch<JumpServerSession>("/jumpserver-sessions", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  delete: (key: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_jumpserver_session", { key })
      : devApiFetch<void>(`/jumpserver-sessions/${encodeURIComponent(key)}`, {
          method: "DELETE",
        }),
  open: async (key: string) => {
    const result = hasTauriRuntime()
      ? await invoke<JumpServerOpenResult>("open_jumpserver_session", { key })
      : await devApiFetch<JumpServerOpenResult>(
          `/jumpserver-sessions/${encodeURIComponent(key)}/open`,
          { method: "POST" },
        );
    await openExternalUrl(result.webUrl);
    return result;
  },
};
