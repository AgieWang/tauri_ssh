import type { KnowledgeSearchHit } from "@/types/knowledge";
import type { KnowledgeProjectTermExpansion } from "./terminology";

export interface KnowledgeCatalogSearchInput {
  projectId: number;
  projectVersionId?: number | null;
  query: string;
  repositoryBindingIds?: number[];
  documentTypes?: string[];
  cursor?: string | null;
  limit?: number | null;
}

export interface KnowledgeCatalogSearchPage {
  items: KnowledgeSearchHit[];
  nextCursor: string | null;
  resultSnapshot: string;
  snapshotChanged: boolean;
  appliedTerms?: KnowledgeProjectTermExpansion[];
}
