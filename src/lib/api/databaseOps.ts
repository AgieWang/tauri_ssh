import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  DatabaseCellUpdateInput,
  DatabaseCellUpdateResult,
  DatabaseConnection,
  DatabaseConnectionTestResult,
  DatabaseExportInput,
  DatabaseExportResult,
  DatabaseNameListInput,
  DatabaseNameListResult,
  DatabaseQueryInput,
  DatabaseQueryResult,
  DatabaseSchemaInput,
  DatabaseSchemaResult,
  DatabaseTableDetail,
  DatabaseTableDetailInput,
  RedisDatabaseListInput,
  RedisDatabaseListResult,
  RedisDescribeKeysInput,
  RedisKeyTreeInput,
  RedisKeyTreeResult,
  RedisScanInput,
  RedisScanResult,
  RedisValuePreview,
  RedisValuePreviewInput,
  UpsertDatabaseConnectionInput,
} from "@/types";

export const databaseOpsApi = {
  listConnections: () =>
    hasTauriRuntime()
      ? invoke<DatabaseConnection[]>("list_database_connections")
      : devApiFetch<DatabaseConnection[]>("/database/connections"),
  upsertConnection: (input: UpsertDatabaseConnectionInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseConnection>("upsert_database_connection", { input })
      : devApiFetch<DatabaseConnection>("/database/connections", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  deleteConnection: (key: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_database_connection", { key })
      : devApiFetch<void>(`/database/connections/${encodeURIComponent(key)}`, {
          method: "DELETE",
        }),
  testConnection: (key: string) =>
    hasTauriRuntime()
      ? invoke<DatabaseConnectionTestResult>("test_database_connection", {
          key,
        })
      : devApiFetch<DatabaseConnectionTestResult>(
          `/database/connections/${encodeURIComponent(key)}/test`,
          { method: "POST" },
        ),
  executeReadonlyQuery: (input: DatabaseQueryInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseQueryResult>("execute_database_readonly_query", {
          input,
        })
      : devApiFetch<DatabaseQueryResult>("/database/query", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listDatabaseNames: (input: DatabaseNameListInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseNameListResult>("list_database_names", { input })
      : devApiFetch<DatabaseNameListResult>("/database/names", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listDatabaseSchema: (input: DatabaseSchemaInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseSchemaResult>("list_database_schema", { input })
      : devApiFetch<DatabaseSchemaResult>("/database/schema", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  getDatabaseTableDetail: (input: DatabaseTableDetailInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseTableDetail>("get_database_table_detail", { input })
      : devApiFetch<DatabaseTableDetail>("/database/table-detail", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  executeSql: (input: DatabaseQueryInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseQueryResult>("execute_database_sql", { input })
      : devApiFetch<DatabaseQueryResult>("/database/sql", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  executeSqlBatch: (input: DatabaseQueryInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseQueryResult[]>("execute_database_sql_batch", { input })
      : devApiFetch<DatabaseQueryResult[]>("/database/sql/batch", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  updateQueryResultCell: (input: DatabaseCellUpdateInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseCellUpdateResult>("update_database_query_result_cell", {
          input,
        })
      : devApiFetch<DatabaseCellUpdateResult>("/database/query/cell", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  exportDatabase: (input: DatabaseExportInput) =>
    hasTauriRuntime()
      ? invoke<DatabaseExportResult>("export_database", { input })
      : devApiFetch<DatabaseExportResult>("/database/export", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  scanRedisKeys: (input: RedisScanInput) =>
    hasTauriRuntime()
      ? invoke<RedisScanResult>("scan_redis_keys", { input })
      : devApiFetch<RedisScanResult>("/database/redis/scan", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  describeRedisKeys: (input: RedisDescribeKeysInput) =>
    hasTauriRuntime()
      ? invoke<RedisScanResult>("describe_redis_keys", { input })
      : devApiFetch<RedisScanResult>("/database/redis/describe", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listRedisDatabases: (input: RedisDatabaseListInput) =>
    hasTauriRuntime()
      ? invoke<RedisDatabaseListResult>("list_redis_databases", { input })
      : devApiFetch<RedisDatabaseListResult>("/database/redis/databases", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  listRedisKeyTree: (input: RedisKeyTreeInput) =>
    hasTauriRuntime()
      ? invoke<RedisKeyTreeResult>("list_redis_key_tree", { input })
      : devApiFetch<RedisKeyTreeResult>("/database/redis/key-tree", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  getRedisValuePreview: (input: RedisValuePreviewInput) =>
    hasTauriRuntime()
      ? invoke<RedisValuePreview>("get_redis_value_preview", { input })
      : devApiFetch<RedisValuePreview>("/database/redis/value", {
          method: "POST",
          body: JSON.stringify(input),
        }),
};
