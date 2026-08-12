import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  CollectResourceBatchInput,
  CollectResourceBatchResult,
  KillMysqlQueryInput,
  KillMysqlQueryResult,
  ListResourceAlertEventsInput,
  ListResourceAlertRulesInput,
  MysqlSlowQuery,
  MysqlSlowQueryListInput,
  ResourceAlertEvent,
  ResourceAlertRule,
  ResourceMetricSnapshot,
  ResourceMonitorOverview,
  ResourceMonitorTarget,
  ResourceSnapshotListInput,
  UpsertResourceAlertRuleInput,
  UpsertResourceMonitorTargetInput,
} from "@/types";

export const resourceMonitorApi = {
  listTargets: () =>
    hasTauriRuntime()
      ? invoke<ResourceMonitorTarget[]>("list_resource_monitor_targets")
      : devApiFetch<ResourceMonitorTarget[]>("/resource-monitor/targets"),
  upsertTarget: (input: UpsertResourceMonitorTargetInput) =>
    hasTauriRuntime()
      ? invoke<ResourceMonitorTarget>("upsert_resource_monitor_target", {
          input,
        })
      : devApiFetch<ResourceMonitorTarget>("/resource-monitor/targets", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteTarget: (targetType: string, targetKey: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_resource_monitor_target", {
          targetType,
          targetKey,
        })
      : devApiFetch<void>(
          `/resource-monitor/targets/${encodeURIComponent(targetType)}/${encodeURIComponent(targetKey)}`,
          { method: "DELETE" },
        ),
  overview: () =>
    hasTauriRuntime()
      ? invoke<ResourceMonitorOverview>("get_resource_monitor_overview")
      : devApiFetch<ResourceMonitorOverview>("/resource-monitor/overview"),
  listSnapshots: (input: ResourceSnapshotListInput) =>
    hasTauriRuntime()
      ? invoke<ResourceMetricSnapshot[]>("list_resource_metric_snapshots", {
          input,
        })
      : devApiFetch<ResourceMetricSnapshot[]>("/resource-monitor/snapshots", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  collectServer: (alias: string) =>
    hasTauriRuntime()
      ? invoke<ResourceMetricSnapshot>("collect_server_resource_snapshot", {
          alias,
        })
      : devApiFetch<ResourceMetricSnapshot>(
          `/resource-monitor/server/${encodeURIComponent(alias)}/collect`,
          { method: "POST" },
        ),
  collectDatabase: (connectionKey: string) =>
    hasTauriRuntime()
      ? invoke<ResourceMetricSnapshot>("collect_database_resource_snapshot", {
          connectionKey,
        })
      : devApiFetch<ResourceMetricSnapshot>(
          `/resource-monitor/database/${encodeURIComponent(connectionKey)}/collect`,
          { method: "POST" },
        ),
  collectRedis: (connectionKey: string) =>
    hasTauriRuntime()
      ? invoke<ResourceMetricSnapshot>("collect_redis_resource_snapshot", {
          connectionKey,
        })
      : devApiFetch<ResourceMetricSnapshot>(
          `/resource-monitor/redis/${encodeURIComponent(connectionKey)}/collect`,
          { method: "POST" },
        ),
  collectBatch: (input: CollectResourceBatchInput) =>
    hasTauriRuntime()
      ? invoke<CollectResourceBatchResult>("collect_resource_snapshots_batch", {
          input,
        })
      : devApiFetch<CollectResourceBatchResult>(
          "/resource-monitor/collect-batch",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  listMysqlSlowQueries: (input: MysqlSlowQueryListInput) =>
    hasTauriRuntime()
      ? invoke<MysqlSlowQuery[]>("list_mysql_slow_queries", { input })
      : devApiFetch<MysqlSlowQuery[]>("/resource-monitor/mysql/slow-queries", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  killMysqlQuery: (input: KillMysqlQueryInput) =>
    hasTauriRuntime()
      ? invoke<KillMysqlQueryResult>("kill_mysql_query", { input })
      : devApiFetch<KillMysqlQueryResult>(
          "/resource-monitor/mysql/kill-query",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  listAlertRules: (input: ListResourceAlertRulesInput) =>
    hasTauriRuntime()
      ? invoke<ResourceAlertRule[]>("list_resource_alert_rules", { input })
      : devApiFetch<ResourceAlertRule[]>("/resource-monitor/alert-rules/list", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  upsertAlertRule: (input: UpsertResourceAlertRuleInput) =>
    hasTauriRuntime()
      ? invoke<ResourceAlertRule>("upsert_resource_alert_rule", { input })
      : devApiFetch<ResourceAlertRule>("/resource-monitor/alert-rules", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteAlertRule: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_resource_alert_rule", { id })
      : devApiFetch<void>(
          `/resource-monitor/alert-rules/${encodeURIComponent(id)}`,
          {
            method: "DELETE",
          },
        ),
  listAlertEvents: (input: ListResourceAlertEventsInput) =>
    hasTauriRuntime()
      ? invoke<ResourceAlertEvent[]>("list_resource_alert_events", { input })
      : devApiFetch<ResourceAlertEvent[]>(
          "/resource-monitor/alert-events/list",
          {
            method: "POST",
            body: JSON.stringify(input),
          },
        ),
  resolveAlertEvent: (id: number) =>
    hasTauriRuntime()
      ? invoke<void>("resolve_resource_alert_event", { id })
      : devApiFetch<void>(
          `/resource-monitor/alert-events/${encodeURIComponent(id)}/resolve`,
          {
            method: "POST",
          },
        ),
};
