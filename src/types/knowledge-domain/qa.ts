export interface KnowledgeConversationMessage {
  role: "user" | "assistant";
  content: string;
}

export interface KnowledgeScopedQuestionInput {
  projectId: number;
  projectVersionId: number;
  question: string;
  repositoryBindingIds?: number[];
  conversation?: KnowledgeConversationMessage[];
  /** 仅查看本地可追溯证据时可省略服务商与模型。 */
  evidenceOnly?: boolean;
  providerKey?: string;
  model?: string;
}

export interface KnowledgeQaSession {
  id: number;
  projectId: number;
  projectVersionId: number;
  releaseCommitSha: string;
  providerKey: string;
  model: string;
  title: string;
  messageCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface KnowledgeQaMessage {
  id: number;
  sessionId: number;
  role: "user" | "assistant";
  content: string;
  evidenceOnly: boolean;
  answer?: import("@/types").KnowledgeAskResult | null;
  createdAt: string;
}

export interface KnowledgeQaSessionDetail {
  session: KnowledgeQaSession;
  messages: KnowledgeQaMessage[];
}

export interface PersistKnowledgeQaRoundInput {
  sessionId?: number;
  projectId: number;
  projectVersionId: number;
  providerKey: string;
  model: string;
  question: string;
  answer: import("@/types").KnowledgeAskResult;
  evidenceOnly: boolean;
}
