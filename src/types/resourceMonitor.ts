export type ResourceTargetType = "server" | "mysql" | "postgresql" | "redis";
export type ResourceStatus = "unknown" | "healthy" | "warning" | "failed";

export interface ResourceMetricSnapshot {
  id: number;
  targetType: ResourceTargetType | string;
  targetKey: string;
  status: ResourceStatus | string;
  collectedAt: string;
  durationMs: number;
  summary: Record<string, unknown>;
  metrics: Record<string, unknown>;
  error?: string | null;
}

export interface ResourceMonitorTarget {
  id?: number | null;
  targetType: ResourceTargetType | string;
  targetKey: string;
  displayName: string;
  groupName: string;
  enabled: boolean;
  collectIntervalSec: number;
  lastStatus: ResourceStatus | string;
  lastCollectedAt?: string | null;
  lastError?: string | null;
  latestSnapshot?: ResourceMetricSnapshot | null;
  updatedAt: string;
}

export interface UpsertResourceMonitorTargetInput {
  targetType: ResourceTargetType | string;
  targetKey: string;
  displayName?: string | null;
  enabled?: boolean | null;
  collectIntervalSec?: number | null;
}

export interface ResourceSnapshotListInput {
  targetType?: ResourceTargetType | string | null;
  targetKey?: string | null;
  limit?: number | null;
}

export interface CollectResourceBatchInput {
  targetType?: ResourceTargetType | string | null;
  onlyEnabled?: boolean | null;
}

export interface CollectResourceBatchResult {
  total: number;
  success: number;
  failed: number;
  snapshots: ResourceMetricSnapshot[];
}

export interface ResourceMonitorOverview {
  totalTargets: number;
  enabledTargets: number;
  healthyTargets: number;
  warningTargets: number;
  failedTargets: number;
  openAlerts: number;
  latestCollectedAt?: string | null;
}

export type ResourceAlertSeverity = "info" | "warning" | "critical";
export type ResourceAlertStatus = "open" | "resolved";
export type ResourceAlertOperator = ">" | ">=" | "<" | "<=" | "==";

export interface ResourceAlertRule {
  id: number;
  targetType: ResourceTargetType | string;
  targetKey: string;
  metricKey: string;
  operator: ResourceAlertOperator | string;
  thresholdValue: number;
  severity: ResourceAlertSeverity | string;
  enabled: boolean;
  updatedAt: string;
}

export interface UpsertResourceAlertRuleInput {
  id?: number | null;
  targetType: ResourceTargetType | string;
  targetKey?: string | null;
  metricKey: string;
  operator: ResourceAlertOperator | string;
  thresholdValue: number;
  severity: ResourceAlertSeverity | string;
  enabled?: boolean | null;
}

export interface ListResourceAlertRulesInput {
  targetType?: ResourceTargetType | string | null;
  targetKey?: string | null;
  enabled?: boolean | null;
}

export interface ResourceAlertEvent {
  id: number;
  ruleId: number;
  targetType: ResourceTargetType | string;
  targetKey: string;
  severity: ResourceAlertSeverity | string;
  status: ResourceAlertStatus | string;
  metricKey: string;
  metricValue: number;
  thresholdValue: number;
  message: string;
  firstSeenAt: string;
  lastSeenAt: string;
  resolvedAt?: string | null;
  snapshotId?: number | null;
}

export interface ListResourceAlertEventsInput {
  status?: ResourceAlertStatus | string | null;
  targetType?: ResourceTargetType | string | null;
  targetKey?: string | null;
  limit?: number | null;
}
