export type DatabaseType = "mysql" | "postgresql" | "redis";
export type DatabaseConnectionMode = "direct" | "ssh_tunnel";
export type DatabaseAuthType = "direct_password" | "credential_ref";
export type DatabaseSecurityMode = "approval_all" | "confirm_execute";
export type DatabaseConnectionStatus =
  "unknown" | "online" | "offline" | "degraded";

export interface DatabaseConnection {
  key: string;
  name: string;
  groupName: string;
  dbType: DatabaseType;
  connectionMode: DatabaseConnectionMode;
  host: string;
  port: number;
  databaseName: string;
  username: string;
  authType: DatabaseAuthType;
  credentialRef: string;
  passwordMasked?: string | null;
  hasPassword: boolean;
  sshServerAlias: string;
  securityMode: DatabaseSecurityMode;
  aiPolicy: string;
  pageSize: number;
  status: DatabaseConnectionStatus;
  enabled: boolean;
  lastConnectedAt?: string | null;
  notes: string;
  updatedAt: string;
}

export interface UpsertDatabaseConnectionInput {
  key: string;
  name: string;
  groupName: string;
  dbType: DatabaseType;
  connectionMode: DatabaseConnectionMode;
  host: string;
  port: number;
  databaseName: string;
  username: string;
  authType: DatabaseAuthType;
  credentialRef: string;
  password?: string | null;
  clearPassword?: boolean | null;
  sshServerAlias: string;
  securityMode: DatabaseSecurityMode;
  aiPolicy: string;
  pageSize: number;
  status?: DatabaseConnectionStatus | null;
  enabled: boolean;
  notes: string;
}

export interface DatabaseConnectionTestResult {
  ok: boolean;
  connectionKey: string;
  endpoint: string;
  latencyMs: number;
  message: string;
}

export interface DatabaseQueryInput {
  connectionKey: string;
  databaseName?: string | null;
  sql: string;
  page?: number;
  pageSize?: number;
}

export interface DatabaseQueryResult {
  columns: string[];
  columnTypes: string[];
  editable?: DatabaseEditableQueryMeta | null;
  rows: Array<Record<string, unknown>>;
  rowCount: number;
  rowsAffected: number;
  page: number;
  pageSize: number;
  durationMs: number;
  truncated: boolean;
  statementType: string;
  status: string;
  message: string;
}

export interface DatabaseEditableQueryMeta {
  enabled: boolean;
  reason: string;
  tableName?: string | null;
  tableSchema?: string | null;
  primaryKeyColumns: string[];
  editableColumns: string[];
  readonlyColumns: string[];
}

export interface DatabaseCellUpdateInput {
  connectionKey: string;
  databaseName?: string | null;
  tableName: string;
  tableSchema?: string | null;
  primaryKey: Record<string, unknown>;
  columnName: string;
  oldValue: unknown;
  newValue: unknown;
}

export interface DatabaseCellUpdateResult {
  updated: boolean;
  rowsAffected: number;
  message: string;
  value: unknown;
}

export interface DatabaseNameListInput {
  connectionKey: string;
}

export interface DatabaseNameListResult {
  connectionKey: string;
  databases: string[];
  current?: string | null;
}

export interface DatabaseSchemaInput {
  connectionKey: string;
  databaseName?: string | null;
}

export interface DatabaseTableDetailInput {
  connectionKey: string;
  databaseName?: string | null;
  tableName: string;
  schemaName?: string | null;
}

export interface DatabaseTableSchema {
  name: string;
  schemaName?: string | null;
  objectType: string;
  rowCount?: string | null;
  dataLength?: string | null;
  engine?: string | null;
  createdAt?: string | null;
  updatedAt?: string | null;
  collation?: string | null;
  comment?: string | null;
  columns: string[];
  columnDetails: DatabaseColumnSchema[];
  indexes: DatabaseIndexSchema[];
}

export interface DatabaseTableDetail {
  table: DatabaseTableSchema;
  createSql: string;
  ddlSource: "raw" | "simplified";
}

export interface DatabaseColumnSchema {
  name: string;
  dataType: string;
  columnType: string;
  nullable: boolean;
  defaultValue?: string | null;
  comment?: string | null;
  extra: string;
  ordinalPosition: number;
}

export interface DatabaseIndexSchema {
  name: string;
  columns: string[];
  unique: boolean;
}

export interface DatabaseSchemaResult {
  connectionKey: string;
  databaseName?: string | null;
  tables: DatabaseTableSchema[];
}

export type DatabaseExportMode = "table_csv" | "query_csv" | "sql_backup";

export interface DatabaseExportInput {
  connectionKey: string;
  databaseName?: string | null;
  mode: DatabaseExportMode;
  tableName?: string | null;
  sql?: string | null;
  includeData?: boolean | null;
  maxRows?: number | null;
}

export interface DatabaseExportResult {
  fileName: string;
  filePath: string;
  rowCount: number;
  tableCount: number;
  mode: DatabaseExportMode;
  message: string;
}

export interface RedisScanInput {
  connectionKey: string;
  databaseName?: string | null;
  pattern?: string;
  cursor?: number;
  count?: number;
}

export interface RedisDescribeKeysInput {
  connectionKey: string;
  databaseName?: string | null;
  keys: string[];
}

export interface RedisKeyEntry {
  key: string;
  keyType: string;
  ttl: number;
}

export interface RedisScanResult {
  cursor: number;
  keys: RedisKeyEntry[];
}

export interface RedisDatabaseListInput {
  connectionKey: string;
}

export interface RedisDatabaseInfo {
  name: string;
  index: number;
  keyCount: number;
}

export interface RedisDatabaseListResult {
  connectionKey: string;
  current: string;
  databases: RedisDatabaseInfo[];
}

export interface RedisKeyTreeInput {
  connectionKey: string;
  databaseName?: string | null;
  pattern?: string | null;
  limit?: number | null;
}

export interface RedisKeyTreeResult {
  connectionKey: string;
  databaseName?: string | null;
  pattern: string;
  keys: string[];
  totalScanned: number;
  truncated: boolean;
}

export interface RedisValuePreviewInput {
  connectionKey: string;
  databaseName?: string | null;
  key: string;
}

export interface RedisValuePreview {
  key: string;
  keyType: string;
  ttl: number;
  preview: unknown;
}
