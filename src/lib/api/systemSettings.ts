import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  AiUnrestrictedState,
  EnableAiUnrestrictedInput,
  SystemSettings,
  SystemSettingsExportResult,
  UpdateSystemSettingsInput,
} from "@/types";

export const systemSettingsApi = {
  get: () =>
    hasTauriRuntime()
      ? invoke<SystemSettings>("get_system_settings")
      : devApiFetch<SystemSettings>("/system-settings"),
  update: (input: UpdateSystemSettingsInput) =>
    hasTauriRuntime()
      ? invoke<SystemSettings>("update_system_settings", { input })
      : devApiFetch<SystemSettings>("/system-settings", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  reset: () =>
    hasTauriRuntime()
      ? invoke<SystemSettings>("reset_system_settings")
      : devApiFetch<SystemSettings>("/system-settings/reset", {
          method: "POST",
        }),
  export: () =>
    hasTauriRuntime()
      ? invoke<SystemSettingsExportResult>("export_system_settings")
      : devApiFetch<SystemSettingsExportResult>("/system-settings/export", {
          method: "POST",
        }),
  getAiUnrestrictedState: () =>
    hasTauriRuntime()
      ? invoke<AiUnrestrictedState>("get_ai_unrestricted_state")
      : devApiFetch<AiUnrestrictedState>("/system-settings/ai-unrestricted"),
  enableAiUnrestrictedMode: (
    input: EnableAiUnrestrictedInput = { minutes: 30 },
  ) =>
    hasTauriRuntime()
      ? invoke<AiUnrestrictedState>("enable_ai_unrestricted_mode", { input })
      : devApiFetch<AiUnrestrictedState>(
          "/system-settings/ai-unrestricted/enable",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  disableAiUnrestrictedMode: () =>
    hasTauriRuntime()
      ? invoke<AiUnrestrictedState>("disable_ai_unrestricted_mode")
      : devApiFetch<AiUnrestrictedState>(
          "/system-settings/ai-unrestricted/disable",
          {
            method: "POST",
          },
        ),
};
