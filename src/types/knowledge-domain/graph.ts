export interface KnowledgeGraphBuildInput {
  projectId: number;
  projectVersionId: number;
  /** 默认只投影已确认关系；开启后会额外显示待确认候选。 */
  includeUnconfirmed?: boolean;
}

export interface KnowledgeGraphBuildResult {
  buildId: number;
  buildKey: string;
  projectId: number;
  projectVersionId: number;
  nodeCount: number;
  edgeCount: number;
  reused: boolean;
}

export interface KnowledgeGraphQueryInput {
  projectId: number;
  projectVersionId: number;
  /** 留空时返回当前范围的有界总览。 */
  rootEntityKey?: string | null;
  /** 与实体标识共同确定根节点，避免同名异类型实体被合并。 */
  rootEntityType?: string | null;
  /** 省略时由后端限制为一层。 */
  depth?: number;
  /** 省略时由后端限制为 100 个节点。 */
  nodeLimit?: number;
  includeUnconfirmed?: boolean;
}

export interface KnowledgeGraphNode {
  id: number;
  entityType: string;
  entityKey: string;
  label: string;
}

export interface KnowledgeGraphEdge {
  id: number;
  fromNodeId: number;
  relationType: string;
  toNodeId: number;
  evidence: Record<string, unknown>;
  confidence: number;
  confirmed: boolean;
  sourceRelationRef: string;
}

export interface KnowledgeGraphProjection {
  buildId: number;
  buildKey: string;
  projectId: number;
  projectVersionId: number;
  nodes: KnowledgeGraphNode[];
  edges: KnowledgeGraphEdge[];
  truncated: boolean;
}

/** 旧关系 Command 的兼容载荷；图谱投影完成前仍需保留来源关系的事实字段。 */
export interface UpsertKnowledgeGraphRelationInput {
  id?: number | null;
  projectId?: number | null;
  releaseId?: number | null;
  documentVersionId?: number | null;
  snapshotId?: number | null;
  sensitivity: string;
  fromType: string;
  fromKey: string;
  relationType: string;
  toType: string;
  toKey: string;
  evidence: Record<string, unknown>;
  confidence: number;
  confirmed: boolean;
  source: string;
}

export interface ListKnowledgeGraphRelationsInput {
  entityType?: string | null;
  entityKey?: string | null;
  projectIds?: number[];
  releaseIds?: number[];
  sensitivities?: string[];
  confirmedOnly?: boolean | null;
  limit?: number | null;
}
