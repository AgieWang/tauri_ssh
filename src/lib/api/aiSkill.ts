import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  AiExperience,
  AiExperienceMatch,
  AiExperienceRecallInput,
  AiRunbook,
  AiRunbookRunResult,
  AiSkill,
  AiSkillPromptPreviewInput,
  AiSkillPromptPreviewResult,
  AiSkillTriggerInput,
  AiSkillTriggerResult,
  ListAiSkillsInput,
  ListAiSkillsResult,
  RunAiRunbookInput,
  SyncBuiltinAiSkillsResult,
  UpsertAiExperienceInput,
  UpsertAiRunbookInput,
  UpsertAiSkillInput,
} from "@/types";

export const aiSkillApi = {
  syncBuiltin: () =>
    hasTauriRuntime()
      ? invoke<SyncBuiltinAiSkillsResult>("sync_builtin_ai_skills")
      : devApiFetch<SyncBuiltinAiSkillsResult>("/ai-skills/sync", {
          method: "POST",
        }),
  list: (input: ListAiSkillsInput) =>
    hasTauriRuntime()
      ? invoke<ListAiSkillsResult>("list_ai_skills", { input })
      : devApiFetch<ListAiSkillsResult>("/ai-skills", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  upsert: (input: UpsertAiSkillInput) =>
    hasTauriRuntime()
      ? invoke<AiSkill>("upsert_ai_skill", { input })
      : devApiFetch<AiSkill>("/ai-skills/upsert", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  setEnabled: (id: number, enabled: boolean) =>
    hasTauriRuntime()
      ? invoke<AiSkill>("set_ai_skill_enabled", { id, enabled })
      : devApiFetch<AiSkill>(`/ai-skills/${id}/enabled`, {
          method: "POST",
          body: JSON.stringify({ enabled }),
        }),
  copy: (id: number) =>
    hasTauriRuntime()
      ? invoke<AiSkill>("copy_ai_skill", { id })
      : devApiFetch<AiSkill>(`/ai-skills/${id}/copy`, { method: "POST" }),
  delete: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_ai_skill", { id })
      : devApiFetch<void>(`/ai-skills/${id}`, { method: "DELETE" }),
  restoreBuiltin: (id: number) =>
    hasTauriRuntime()
      ? invoke<AiSkill>("restore_builtin_ai_skill", { id })
      : devApiFetch<AiSkill>(`/ai-skills/${id}/restore`, { method: "POST" }),
  testTrigger: (input: AiSkillTriggerInput) =>
    hasTauriRuntime()
      ? invoke<AiSkillTriggerResult>("test_ai_skill_trigger", { input })
      : devApiFetch<AiSkillTriggerResult>("/ai-skills/trigger", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  previewPrompt: (input: AiSkillPromptPreviewInput) =>
    hasTauriRuntime()
      ? invoke<AiSkillPromptPreviewResult>("preview_ai_skill_prompt", { input })
      : devApiFetch<AiSkillPromptPreviewResult>("/ai-skills/preview", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listExperiences: (keyword?: string) =>
    hasTauriRuntime()
      ? invoke<AiExperience[]>("list_ai_experiences", { keyword: keyword ?? null })
      : devApiFetch<AiExperience[]>(
          `/ai-experiences${keyword ? `?keyword=${encodeURIComponent(keyword)}` : ""}`,
        ),
  recallExperiences: (input: AiExperienceRecallInput) =>
    hasTauriRuntime()
      ? invoke<AiExperienceMatch[]>("recall_ai_experiences", { input })
      : devApiFetch<AiExperienceMatch[]>("/ai-experiences/recall", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  upsertExperience: (input: UpsertAiExperienceInput) =>
    hasTauriRuntime()
      ? invoke<AiExperience>("upsert_ai_experience", { input })
      : devApiFetch<AiExperience>("/ai-experiences", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteExperience: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_ai_experience", { id })
      : devApiFetch<void>(`/ai-experiences/${id}`, { method: "DELETE" }),
  listRunbooks: (keyword?: string) =>
    hasTauriRuntime()
      ? invoke<AiRunbook[]>("list_ai_runbooks", { keyword: keyword ?? null })
      : devApiFetch<AiRunbook[]>(
          `/ai-runbooks${keyword ? `?keyword=${encodeURIComponent(keyword)}` : ""}`,
        ),
  upsertRunbook: (input: UpsertAiRunbookInput) =>
    hasTauriRuntime()
      ? invoke<AiRunbook>("upsert_ai_runbook", { input })
      : devApiFetch<AiRunbook>("/ai-runbooks", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteRunbook: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_ai_runbook", { id })
      : devApiFetch<void>(`/ai-runbooks/${id}`, { method: "DELETE" }),
  runRunbook: (input: RunAiRunbookInput) =>
    hasTauriRuntime()
      ? invoke<AiRunbookRunResult>("run_ai_runbook", { input })
      : devApiFetch<AiRunbookRunResult>("/ai-runbooks/run", {
          method: "POST",
          body: JSON.stringify(input),
        }),
};
