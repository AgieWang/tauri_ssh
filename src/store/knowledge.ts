import { create } from "zustand";

interface KnowledgeStore {
  /** 问答与目录页面共享的范围，避免切换视图后误丢失版本隔离条件。 */
  projectIds: number[];
  releaseIds: number[];
  setProjectIds: (projectIds: number[]) => void;
  setReleaseIds: (releaseIds: number[]) => void;
}

export const useKnowledgeStore = create<KnowledgeStore>((set) => ({
  projectIds: [],
  releaseIds: [],
  setProjectIds: (projectIds) => set({ projectIds, releaseIds: [] }),
  setReleaseIds: (releaseIds) => set({ releaseIds }),
}));
