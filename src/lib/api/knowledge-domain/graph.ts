import type {
  KnowledgeGraphBuildInput,
  KnowledgeGraphBuildResult,
  KnowledgeGraphProjection,
  KnowledgeGraphQueryInput,
} from "@/types/knowledge-domain/graph";
import { devApiFetch, hasTauriRuntime, invoke } from "../client";

/** 图谱构建和查询都经由本地 Rust 服务；浏览器开发回退路径复用同一领域 Service。 */
export const knowledgeGraphApi = {
  buildProjectGraph: (input: KnowledgeGraphBuildInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeGraphBuildResult>("build_knowledge_project_graph", {
          input,
        })
      : devApiFetch<KnowledgeGraphBuildResult>(
          `/knowledge/projects/${input.projectId}/graph/build`,
          { method: "POST", body: JSON.stringify(input) },
        ),
  queryProjectGraph: (input: KnowledgeGraphQueryInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeGraphProjection>("query_knowledge_project_graph", {
          input,
        })
      : devApiFetch<KnowledgeGraphProjection>(
          `/knowledge/projects/${input.projectId}/graph`,
          { method: "POST", body: JSON.stringify(input) },
        ),
};
