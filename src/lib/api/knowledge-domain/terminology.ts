import { devApiFetch, hasTauriRuntime, invoke } from "../client";
import type {
  KnowledgeProjectTerm,
  UpsertKnowledgeProjectTermInput,
} from "@/types/knowledge-domain/terminology";

/** 项目术语只经受控 IPC 或同一 Service 的开发回退路径读写，页面不直接处理本地数据库。 */
export const knowledgeTerminologyApi = {
  listProjectTerms: (projectId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeProjectTerm[]>("list_knowledge_project_terms", {
          projectId,
        })
      : devApiFetch<KnowledgeProjectTerm[]>(
          `/knowledge/projects/${projectId}/terms`,
        ),
  upsertProjectTerm: (input: UpsertKnowledgeProjectTermInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeProjectTerm>("upsert_knowledge_project_term", { input })
      : devApiFetch<KnowledgeProjectTerm>(
          `/knowledge/projects/${input.projectId}/terms`,
          { method: "POST", body: JSON.stringify(input) },
        ),
  deleteProjectTerm: (projectId: number, termId: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_knowledge_project_term", { projectId, termId })
      : devApiFetch<void>(`/knowledge/projects/${projectId}/terms/${termId}`, {
          method: "DELETE",
        }),
};
