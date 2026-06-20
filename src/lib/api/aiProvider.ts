import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  AiProvider,
  AiProviderAskInput,
  AiProviderAskResult,
  AiProviderModelListInput,
  AiProviderModelListResult,
  AiProviderRoute,
  AiProviderTestResult,
  UpsertAiProviderInput,
  UpsertAiProviderRouteInput,
} from "@/types";

export const aiProviderApi = {
  list: () =>
    hasTauriRuntime()
      ? invoke<AiProvider[]>("list_ai_providers")
      : devApiFetch<AiProvider[]>("/ai-providers"),
  upsert: (input: UpsertAiProviderInput) =>
    hasTauriRuntime()
      ? invoke<AiProvider>("upsert_ai_provider", { input })
      : devApiFetch<AiProvider>("/ai-providers", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  delete: (key: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_ai_provider", { key })
      : devApiFetch<void>(`/ai-providers/${encodeURIComponent(key)}`, {
          method: "DELETE",
        }),
  listRoutes: () =>
    hasTauriRuntime()
      ? invoke<AiProviderRoute[]>("list_ai_provider_routes")
      : devApiFetch<AiProviderRoute[]>("/ai-providers/routes"),
  upsertRoute: (input: UpsertAiProviderRouteInput) =>
    hasTauriRuntime()
      ? invoke<AiProviderRoute>("upsert_ai_provider_route", { input })
      : devApiFetch<AiProviderRoute>("/ai-providers/routes", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  test: (key: string) =>
    hasTauriRuntime()
      ? invoke<AiProviderTestResult>("test_ai_provider", { key })
      : devApiFetch<AiProviderTestResult>(
          `/ai-providers/${encodeURIComponent(key)}/test`,
          { method: "POST" },
        ),
  listModels: (input: AiProviderModelListInput) =>
    hasTauriRuntime()
      ? invoke<AiProviderModelListResult>("list_ai_provider_models", { input })
      : devApiFetch<AiProviderModelListResult>("/ai-providers/models", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  ask: (input: AiProviderAskInput) =>
    hasTauriRuntime()
      ? invoke<AiProviderAskResult>("ask_ai_provider", { input })
      : devApiFetch<AiProviderAskResult>("/ai-providers/ask", {
          method: "POST",
          body: JSON.stringify(input),
        }),
};
