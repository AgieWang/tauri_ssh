import { devApiFetch, hasTauriRuntime, invoke } from "../client";
import { knowledgeApi } from "../knowledge";
import type {
  KnowledgeCatalogSearchInput,
  KnowledgeCatalogSearchPage,
} from "@/types/knowledge-domain/search";

/**
 * 新工作台只经此门面调用已注册的检索 Command，避免页面复制旧检索、向量或权限规则。
 * FTS 覆盖标题和正文；向量未就绪时仍可提供确定性的全文结果。
 */
export const knowledgeSearchApi = {
  searchCatalog: (input: KnowledgeCatalogSearchInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeCatalogSearchPage>("search_knowledge_catalog", {
          input,
        })
      : devApiFetch<KnowledgeCatalogSearchPage>(
          `/knowledge/projects/${input.projectId}/search`,
          { method: "POST", body: JSON.stringify(input) },
        ),
  rebuildFts: knowledgeApi.rebuildFts,
  searchFts: knowledgeApi.searchFts,
  previewRagContext: knowledgeApi.previewRagContext,
  searchActiveVectors: knowledgeApi.searchActiveVectors,
};
