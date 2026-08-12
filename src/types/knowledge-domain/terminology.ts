/** 当前项目内、经人工确认后才能参与检索扩展的术语映射。 */
export interface KnowledgeProjectTerm {
  id: number;
  projectId: number;
  term: string;
  aliases: string[];
  confirmationNote: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

export interface UpsertKnowledgeProjectTermInput {
  id?: number | null;
  projectId: number;
  term: string;
  aliases?: string[];
  confirmationNote: string;
  createdBy?: string | null;
}

/** 搜索响应中实际触发的术语，便于用户理解扩展原因。 */
export interface KnowledgeProjectTermExpansion {
  term: string;
  aliases: string[];
}
