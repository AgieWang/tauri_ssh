import { lazy, Suspense, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  Alert,
  AutoComplete,
  Badge,
  Button,
  Card,
  Drawer,
  Form,
  Input,
  InputNumber,
  Modal,
  Pagination,
  Popconfirm,
  Select,
  Space,
  Switch,
  Table,
  Tabs,
  Tag,
  Tree,
  Typography,
  message,
} from "antd";
import type { TableProps } from "antd";
import { Bot, Edit3, PlugZap, Plus, RefreshCw, Sparkles, Trash2, Wand2 } from "lucide-react";
import {
  aiSkillApi,
  aiProviderApi,
  credentialVaultApi,
  databaseOpsApi,
  getErrorMessage,
  sshServerApi,
} from "@/lib/api";
import { useAppStore } from "@/store";
import type {
  CredentialVaultItem,
  DatabaseColumnSchema,
  DatabaseConnection,
  DatabaseConnectionStatus,
  DatabaseExportMode,
  DatabaseExportResult,
  DatabaseIndexSchema,
  DatabaseNameListResult,
  DatabaseQueryResult,
  DatabaseSchemaResult,
  DatabaseSecurityMode,
  DatabaseTableSchema,
  DatabaseType,
  RedisDatabaseInfo,
  RedisKeyEntry,
  RedisValuePreview,
  SshServer,
  UpsertDatabaseConnectionInput,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

type SqlDialectKey = "mysql" | "postgresql" | "standard";
type SqlExecutionStatus = "idle" | "loading_databases" | "running" | "success" | "error";
type SqlAiMode = "ask" | "generate" | "fix" | "optimize";

interface RedisTreeNode {
  key: string;
  title: ReactNode;
  titleText: string;
  pattern: string;
  count: number;
  leafKeys: string[];
  children?: RedisTreeNode[];
}

interface DatabaseObjectTreeNode {
  key: string;
  title: ReactNode;
  object?: DatabaseTableSchema;
  children?: DatabaseObjectTreeNode[];
}

interface AddColumnFormValues {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string;
}

interface ModifyColumnFormValues {
  oldName: string;
  newName?: string;
  dataType?: string;
  nullable?: boolean;
  defaultValue?: string;
}

interface AddIndexFormValues {
  name: string;
  columns: string[];
  unique: boolean;
}

interface CreateTableColumnFormValues {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue?: string;
  primaryKey?: boolean;
}

interface CreateTableFormValues {
  tableName: string;
  columns: CreateTableColumnFormValues[];
}

function objectTypeMeta(objectType?: string) {
  const normalized = (objectType ?? "").toUpperCase();
  if (normalized.includes("VIEW")) {
    return { text: "视图", color: "purple" };
  }
  return { text: "表", color: "blue" };
}

function quoteIdentifier(value: string, dbType?: DatabaseType) {
  const quote = dbType === "mysql" ? "`" : "\"";
  const escaped = value.split(quote).join(`${quote}${quote}`);
  return dbType === "mysql" ? `\`${escaped}\`` : `"${escaped}"`;
}

function sqlLiteral(value: string) {
  return `'${value.replace(/'/g, "''")}'`;
}

function normalizeDefaultValue(value?: string) {
  const trimmed = value?.trim();
  if (!trimmed) return "";
  if (/^(null|current_timestamp\(\)?|now\(\)|true|false)$/i.test(trimmed)) {
    return trimmed;
  }
  if (/^-?\d+(\.\d+)?$/.test(trimmed)) {
    return trimmed;
  }
  return sqlLiteral(trimmed);
}

function tableSqlName(object: DatabaseTableSchema, dbType?: DatabaseType) {
  if (object.schemaName && dbType === "postgresql") {
    return `${quoteIdentifier(object.schemaName, dbType)}.${quoteIdentifier(object.name, dbType)}`;
  }
  return quoteIdentifier(object.name, dbType);
}

function createTableSqlName(tableName: string, dbType?: DatabaseType) {
  return quoteIdentifier(tableName.trim(), dbType);
}

function columnDefinitionSql(values: AddColumnFormValues | ModifyColumnFormValues) {
  const dataType = values.dataType?.trim();
  if (!dataType) {
    throw new Error("字段类型不能为空");
  }
  const nullable = values.nullable ?? true;
  const defaultSql = normalizeDefaultValue(values.defaultValue);
  const chunks = [dataType, nullable ? "NULL" : "NOT NULL"];
  if (defaultSql) {
    chunks.push(`DEFAULT ${defaultSql}`);
  }
  return chunks.join(" ");
}

function createTableColumnDefinitionSql(column: CreateTableColumnFormValues, dbType?: DatabaseType) {
  const name = column.name?.trim();
  if (!name) {
    throw new Error("字段名不能为空");
  }
  return `${quoteIdentifier(name, dbType)} ${columnDefinitionSql(column)}`;
}

function extractSqlFromAiAnswer(answer: string) {
  const fenced = answer.match(/```(?:sql)?\s*([\s\S]*?)```/i);
  const candidate = fenced?.[1] ?? answer;
  return candidate.trim();
}

function formatSqlAiProbeRows(rows: Array<Record<string, unknown>>) {
  if (rows.length === 0) return "无返回行";
  return rows
    .slice(0, 3)
    .map((row) =>
      Object.entries(row)
        .map(([key, value]) => `${key}=${value === null || value === undefined ? "NULL" : String(value)}`)
        .join(", "),
    )
    .join("\n");
}

function normalizeMarkdownForPanel(markdown: string) {
  return markdown
    .replace(/\s+(#{1,6}\s+)/g, "\n\n$1")
    .replace(/\s+(```[a-zA-Z0-9_-]*)/g, "\n\n$1")
    .replace(/```([a-zA-Z0-9_-]+)\s+/g, "```$1\n")
    .replace(/```\s+/g, "```\n")
    .trim();
}

function renderInlineMarkdown(value: string) {
  const segments = value.split(/(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|\*[^*\n]+\*)/g).filter(Boolean);
  return segments.map((segment, index) => {
    if (segment.startsWith("`") && segment.endsWith("`")) {
      return <code key={`${segment}-${index}`}>{segment.slice(1, -1)}</code>;
    }
    if ((segment.startsWith("**") && segment.endsWith("**")) || (segment.startsWith("__") && segment.endsWith("__"))) {
      return <strong key={`${segment}-${index}`}>{segment.slice(2, -2)}</strong>;
    }
    if (segment.startsWith("*") && segment.endsWith("*")) {
      return <em key={`${segment}-${index}`}>{segment.slice(1, -1)}</em>;
    }
    return <span key={`${segment}-${index}`}>{segment}</span>;
  });
}

function renderMarkdownHeading(level: number, content: string) {
  const HeadingTag = (level <= 2 ? "h4" : "h5") as "h4" | "h5";
  return <HeadingTag>{renderInlineMarkdown(content)}</HeadingTag>;
}

function renderMarkdownTextBlock(block: string, blockIndex: number) {
  const lines = block.split("\n").filter((line) => line.trim().length > 0);
  const firstLine = lines[0]?.trim() ?? "";
  const heading = firstLine.match(/^(#{1,6})\s+(.+)$/);
  if (heading) {
    const rest = lines.slice(1).join("\n");
    return (
      <section key={`${blockIndex}-${firstLine}`}>
        {renderMarkdownHeading(heading[1].length, heading[2])}
        {rest ? <SqlMarkdownAnswer content={rest} /> : null}
      </section>
    );
  }
  if (lines.every((line) => /^[-*]\s+/.test(line.trim()))) {
    return (
      <ul key={`${blockIndex}-${firstLine}`}>
        {lines.map((line, index) => (
          <li key={`${line}-${index}`}>{renderInlineMarkdown(line.trim().replace(/^[-*]\s+/, ""))}</li>
        ))}
      </ul>
    );
  }
  if (lines.every((line) => /^\d+[.)]\s+/.test(line.trim()))) {
    return (
      <ol key={`${blockIndex}-${firstLine}`}>
        {lines.map((line, index) => (
          <li key={`${line}-${index}`}>{renderInlineMarkdown(line.trim().replace(/^\d+[.)]\s+/, ""))}</li>
        ))}
      </ol>
    );
  }
  return (
    <p key={`${blockIndex}-${firstLine}`}>
      {renderInlineMarkdown(lines.join(" "))}
    </p>
  );
}

function SqlMarkdownAnswer({ content }: { content: string }) {
  const normalized = normalizeMarkdownForPanel(content);
  const blocks: Array<{ type: "text" | "code"; value: string; lang?: string }> = [];
  let textBuffer: string[] = [];
  let codeBuffer: string[] = [];
  let codeLang = "";

  normalized.split("\n").forEach((line) => {
    const fence = line.match(/^```([a-zA-Z0-9_-]*)\s*$/);
    if (fence) {
      if (codeBuffer.length > 0 || codeLang) {
        blocks.push({ type: "code", value: codeBuffer.join("\n").trimEnd(), lang: codeLang || "text" });
        codeBuffer = [];
        codeLang = "";
      } else {
        if (textBuffer.some((item) => item.trim())) {
          blocks.push({ type: "text", value: textBuffer.join("\n").trim() });
          textBuffer = [];
        }
        codeLang = fence[1] || "text";
      }
      return;
    }
    if (codeLang) {
      codeBuffer.push(line);
    } else {
      textBuffer.push(line);
    }
  });
  if (codeBuffer.length > 0 || codeLang) {
    blocks.push({ type: "code", value: codeBuffer.join("\n").trimEnd(), lang: codeLang || "text" });
  }
  if (textBuffer.some((item) => item.trim())) {
    blocks.push({ type: "text", value: textBuffer.join("\n").trim() });
  }

  return (
    <div className="database-sql-ai-markdown">
      {blocks.map((block, index) => {
        if (block.type === "code") {
          return (
            <div className="database-sql-ai-code-block" key={`code-${index}`}>
              <div className="database-sql-ai-code-lang">{block.lang}</div>
              <pre><code>{block.value}</code></pre>
            </div>
          );
        }
        return block.value
          .split(/\n{2,}/)
          .filter(Boolean)
          .map((textBlock, textIndex) => renderMarkdownTextBlock(textBlock, index * 100 + textIndex));
      })}
    </div>
  );
}

function renderRedisTreeTitle(label: string, count: number) {
  const text = `${label}（${count}）`;
  return (
    <span
      title={`${label}（${count} 个 Key）`}
      style={{
        display: "inline-block",
        width: "max-content",
        verticalAlign: "bottom",
        whiteSpace: "nowrap",
      }}
    >
      {text}
    </span>
  );
}

function buildRedisTree(keys: string[]): RedisTreeNode[] {
  const root: RedisTreeNode = {
    key: "__all__",
    title: renderRedisTreeTitle("全部 Key", keys.length),
    titleText: "全部 Key",
    pattern: "*",
    count: keys.length,
    leafKeys: [...keys],
    children: [],
  };

  for (const redisKey of keys) {
    const parts = redisKey.split(":").filter(Boolean);
    let current = root;
    let prefix = "";
    const visibleParts = parts.length > 0 ? parts : [redisKey];
    for (const part of visibleParts) {
      prefix = prefix ? `${prefix}:${part}` : part;
      current.children ??= [];
      let child = current.children.find((item) => item.key === prefix);
      if (!child) {
        child = {
          key: prefix,
          title: part,
          titleText: part,
          pattern: prefix === redisKey ? redisKey : `${prefix}:*`,
          count: 0,
          leafKeys: [],
          children: [],
        };
        current.children.push(child);
      }
      child.count += 1;
      child.leafKeys.push(redisKey);
      child.pattern = child.children && child.children.length > 0 ? `${prefix}:*` : child.pattern;
      current = child;
    }
  }

  function finalize(nodes?: RedisTreeNode[]) {
    if (!nodes) return;
    nodes.sort((a, b) => a.key.localeCompare(b.key));
    for (const node of nodes) {
      node.title = renderRedisTreeTitle(node.titleText, node.count);
      if (node.children?.length === 0) {
        delete node.children;
      } else {
        node.pattern = `${node.key}:*`;
        finalize(node.children);
      }
    }
  }

  finalize(root.children);
  return [root];
}

const SqlCodeEditor = lazy(async () => {
  const [
    codeMirrorModule,
    githubThemeModule,
    commandsModule,
    sqlModule,
    languageModule,
    viewModule,
  ] = await Promise.all([
    import("@uiw/react-codemirror"),
    import("@uiw/codemirror-theme-github"),
    import("@codemirror/commands"),
    import("@codemirror/lang-sql"),
    import("@codemirror/language"),
    import("@codemirror/view"),
  ]);
  const CodeMirror = codeMirrorModule.default;
  const dialectMap = {
    mysql: sqlModule.MySQL,
    postgresql: sqlModule.PostgreSQL,
    standard: sqlModule.StandardSQL,
  };
  return {
    default: function LazySqlCodeEditor(props: {
      value: string;
      dialect: SqlDialectKey;
      dark: boolean;
      schema: Record<string, readonly string[]>;
      onChange: (value: string) => void;
      onRun: () => void;
    }) {
      return (
        <CodeMirror
          value={props.value}
          height="190px"
          theme={props.dark ? githubThemeModule.githubDark : githubThemeModule.githubLight}
          extensions={[
            languageModule.indentUnit.of("  "),
            viewModule.keymap.of([
              {
                key: "Shift-Enter",
                run: () => {
                  props.onRun();
                  return true;
                },
              },
              commandsModule.indentWithTab,
            ]),
            viewModule.EditorView.lineWrapping,
            sqlModule.sql({
              dialect: dialectMap[props.dialect] ?? sqlModule.StandardSQL,
              schema: props.schema,
              upperCaseKeywords: true,
            }),
          ]}
          basicSetup={{
            lineNumbers: true,
            highlightActiveLineGutter: true,
            highlightSpecialChars: true,
            foldGutter: true,
            drawSelection: true,
            dropCursor: true,
            allowMultipleSelections: true,
            indentOnInput: true,
            bracketMatching: true,
            closeBrackets: true,
            autocompletion: true,
            rectangularSelection: true,
            crosshairCursor: true,
            highlightActiveLine: true,
            highlightSelectionMatches: true,
            closeBracketsKeymap: true,
            searchKeymap: true,
            foldKeymap: true,
            completionKeymap: true,
            lintKeymap: true,
          }}
          onChange={(value) => props.onChange(value)}
        />
      );
    },
  };
});

const dbTypeOptions: Array<{ value: DatabaseType; label: string; defaultPort: number }> = [
  { value: "mysql", label: "MySQL / MariaDB", defaultPort: 3306 },
  { value: "postgresql", label: "PostgreSQL", defaultPort: 5432 },
  { value: "redis", label: "Redis", defaultPort: 6379 },
];

const dbTypeLabel = Object.fromEntries(
  dbTypeOptions.map((item) => [item.value, item.label]),
) as Record<DatabaseType, string>;

const statusMeta: Record<DatabaseConnectionStatus, { text: string; color: string; badge: "success" | "processing" | "default" | "warning" | "error" }> = {
  unknown: { text: "未检测", color: "default", badge: "default" },
  online: { text: "在线", color: "green", badge: "success" },
  offline: { text: "离线", color: "red", badge: "error" },
  degraded: { text: "异常", color: "orange", badge: "warning" },
};

const securityModeLabel: Record<DatabaseSecurityMode, string> = {
  approval_all: "全部审批",
  confirm_execute: "二次确认执行",
};

const aiPolicyOptions = [
  { value: "readonly", label: "只读 - 仅允许查看" },
  { value: "L1", label: "低风险 - 只读与安全检查" },
  { value: "L2", label: "中风险 - 常规运维需审批" },
  { value: "L3", label: "高风险 - 变更/重启强审批" },
  { value: "blocked", label: "禁用 - AI 不可操作" },
];

const commonMysqlColumnTypes = [
  "char(32)",
  "varchar(32)",
  "varchar(64)",
  "varchar(100)",
  "varchar(255)",
  "varchar(500)",
  "tinytext",
  "text",
  "mediumtext",
  "longtext",
  "tinyint",
  "tinyint(1)",
  "smallint",
  "mediumint",
  "int",
  "int unsigned",
  "bigint",
  "bigint unsigned",
  "decimal(18,2)",
  "decimal(20,6)",
  "float",
  "double",
  "bit(1)",
  "boolean",
  "date",
  "datetime",
  "timestamp",
  "time",
  "year",
  "json",
  "binary(16)",
  "varbinary(255)",
  "tinyblob",
  "blob",
  "mediumblob",
  "longblob",
  "enum('a','b')",
];

const commonPostgresColumnTypes = [
  "varchar(255)",
  "varchar(64)",
  "char(32)",
  "text",
  "smallint",
  "integer",
  "bigint",
  "serial",
  "bigserial",
  "numeric(18,2)",
  "real",
  "double precision",
  "money",
  "boolean",
  "date",
  "time",
  "timestamp",
  "timestamptz",
  "interval",
  "uuid",
  "json",
  "jsonb",
  "bytea",
  "inet",
  "cidr",
  "macaddr",
  "xml",
  "text[]",
  "integer[]",
];

const defaultFormValues: UpsertDatabaseConnectionInput = {
  key: "",
  name: "",
  groupName: "默认分组",
  dbType: "mysql",
  connectionMode: "direct",
  host: "127.0.0.1",
  port: 3306,
  databaseName: "",
  username: "",
  authType: "direct_password",
  credentialRef: "",
  password: "",
  clearPassword: false,
  sshServerAlias: "",
  securityMode: "approval_all",
  aiPolicy: "L2",
  pageSize: 500,
  status: "unknown",
  enabled: true,
  notes: "",
};

function statusTag(status: DatabaseConnectionStatus) {
  const meta = statusMeta[status] ?? statusMeta.unknown;
  return (
    <Badge status={meta.badge} text={<Tag color={meta.color}>{meta.text}</Tag>} />
  );
}

export default function DatabasePage() {
  const theme = useAppStore((state) => state.theme);
  const [activeTab, setActiveTab] = useState("connections");
  const [connections, setConnections] = useState<DatabaseConnection[]>([]);
  const [credentials, setCredentials] = useState<CredentialVaultItem[]>([]);
  const [servers, setServers] = useState<SshServer[]>([]);
  const [loading, setLoading] = useState(false);
  const [testingKey, setTestingKey] = useState<string | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editing, setEditing] = useState<DatabaseConnection | null>(null);
  const [queryConnectionKey, setQueryConnectionKey] = useState<string>();
  const [queryDatabaseName, setQueryDatabaseName] = useState<string>();
  const [databaseNames, setDatabaseNames] = useState<string[]>([]);
  const [databaseSchema, setDatabaseSchema] = useState<DatabaseSchemaResult | null>(null);
  const [schemaLoading, setSchemaLoading] = useState(false);
  const [objectConnectionKey, setObjectConnectionKey] = useState<string>();
  const [objectDatabaseName, setObjectDatabaseName] = useState<string>();
  const [objectDatabaseNames, setObjectDatabaseNames] = useState<string[]>([]);
  const [objectSchema, setObjectSchema] = useState<DatabaseSchemaResult | null>(null);
  const [objectLoading, setObjectLoading] = useState(false);
  const [selectedObjectKey, setSelectedObjectKey] = useState<string>();
  const [createTableDrawerOpen, setCreateTableDrawerOpen] = useState(false);
  const [structureDrawerOpen, setStructureDrawerOpen] = useState(false);
  const [structureSubmitting, setStructureSubmitting] = useState(false);
  const [querySql, setQuerySql] = useState("select 1");
  const [queryResults, setQueryResults] = useState<DatabaseQueryResult[]>([]);
  const [activeQueryResultKey, setActiveQueryResultKey] = useState("0");
  const [queryLoading, setQueryLoading] = useState(false);
  const [databaseLoading, setDatabaseLoading] = useState(false);
  const [sqlExecutionStatus, setSqlExecutionStatus] = useState<SqlExecutionStatus>("idle");
  const [sqlExecutionMessage, setSqlExecutionMessage] = useState("等待执行 SQL");
  const [sqlAiMode, setSqlAiMode] = useState<SqlAiMode>("generate");
  const [sqlAiPrompt, setSqlAiPrompt] = useState("");
  const [sqlAiAnswer, setSqlAiAnswer] = useState("");
  const [sqlAiMeta, setSqlAiMeta] = useState("");
  const [sqlAiLoading, setSqlAiLoading] = useState(false);
  const [sqlExperienceSaving, setSqlExperienceSaving] = useState(false);
  const [redisConnectionKey, setRedisConnectionKey] = useState<string>();
  const [redisDatabaseName, setRedisDatabaseName] = useState<string>();
  const [redisDatabases, setRedisDatabases] = useState<RedisDatabaseInfo[]>([]);
  const [redisPattern, setRedisPattern] = useState("*");
  const [redisTreeKeys, setRedisTreeKeys] = useState<string[]>([]);
  const [redisTreeLoading, setRedisTreeLoading] = useState(false);
  const [redisSelectedTreeKey, setRedisSelectedTreeKey] = useState("__all__");
  const [redisSelectedKeyCount, setRedisSelectedKeyCount] = useState(0);
  const [redisSelectedLeafKeys, setRedisSelectedLeafKeys] = useState<string[]>([]);
  const [redisCursor, setRedisCursor] = useState(0);
  const [redisKeys, setRedisKeys] = useState<RedisKeyEntry[]>([]);
  const [redisPage, setRedisPage] = useState(1);
  const [redisPageSize, setRedisPageSize] = useState(12);
  const [redisPageCursors, setRedisPageCursors] = useState<Record<number, number>>({ 1: 0 });
  const [redisPreview, setRedisPreview] = useState<RedisValuePreview | null>(null);
  const [redisLoading, setRedisLoading] = useState(false);
  const [redisDatabaseLoading, setRedisDatabaseLoading] = useState(false);
  const [exportConnectionKey, setExportConnectionKey] = useState<string>();
  const [exportDatabaseName, setExportDatabaseName] = useState<string>();
  const [exportDatabaseNames, setExportDatabaseNames] = useState<string[]>([]);
  const [exportSchema, setExportSchema] = useState<DatabaseSchemaResult | null>(null);
  const [exportMode, setExportMode] = useState<DatabaseExportMode>("table_csv");
  const [exportTableName, setExportTableName] = useState<string>();
  const [exportSql, setExportSql] = useState("SELECT * FROM your_table LIMIT 1000;");
  const [exportIncludeData, setExportIncludeData] = useState(true);
  const [exportMaxRows, setExportMaxRows] = useState(100000);
  const [exportLoading, setExportLoading] = useState(false);
  const [exportResult, setExportResult] = useState<DatabaseExportResult | null>(null);
  const [form] = Form.useForm<UpsertDatabaseConnectionInput>();
  const [createTableForm] = Form.useForm<CreateTableFormValues>();
  const [addColumnForm] = Form.useForm<AddColumnFormValues>();
  const [modifyColumnForm] = Form.useForm<ModifyColumnFormValues>();
  const [addIndexForm] = Form.useForm<AddIndexFormValues>();
  const connectionMode = Form.useWatch("connectionMode", form);
  const authType = Form.useWatch("authType", form);

  const credentialOptions = useMemo(
    () =>
      credentials
        .filter((item) => item.enabled)
        .map((item) => ({
          value: item.key,
          label: `${item.key}（${item.credentialType}）`,
        })),
    [credentials],
  );

  const serverOptions = useMemo(
    () =>
      servers
        .filter((item) => item.enabled)
        .map((item) => ({
          value: item.alias,
          label: `${item.alias}（${item.host}:${item.port}）`,
        })),
    [servers],
  );

  const sqlConnectionOptions = useMemo(
    () =>
      connections
        .filter((item) => item.enabled && item.dbType !== "redis")
        .map((item) => ({
          value: item.key,
          label: `${item.name}（${dbTypeLabel[item.dbType]}）`,
        })),
    [connections],
  );

  const redisConnectionOptions = useMemo(
    () =>
      connections
        .filter((item) => item.enabled && item.dbType === "redis")
        .map((item) => ({
          value: item.key,
          label: `${item.name}（${item.host}:${item.port}）`,
        })),
    [connections],
  );

  const databaseNameOptions = useMemo(
    () => databaseNames.map((name) => ({ value: name, label: name })),
    [databaseNames],
  );

  const objectDatabaseNameOptions = useMemo(
    () => objectDatabaseNames.map((name) => ({ value: name, label: name })),
    [objectDatabaseNames],
  );

  const exportDatabaseNameOptions = useMemo(
    () => exportDatabaseNames.map((name) => ({ value: name, label: name })),
    [exportDatabaseNames],
  );

  const exportTableOptions = useMemo(
    () =>
      (exportSchema?.tables ?? [])
        .filter((table) => !table.objectType.toUpperCase().includes("VIEW"))
        .map((table) => ({ value: table.name, label: table.name })),
    [exportSchema],
  );

  const redisDatabaseOptions = useMemo(
    () =>
      redisDatabases.map((item) => ({
        value: item.name,
        label: `DB ${item.index}（${item.keyCount} keys）`,
      })),
    [redisDatabases],
  );

  const currentRedisDatabase = useMemo(
    () => redisDatabases.find((item) => item.name === redisDatabaseName),
    [redisDatabaseName, redisDatabases],
  );

  const redisTreeData = useMemo(() => buildRedisTree(redisTreeKeys), [redisTreeKeys]);

  const sqlCompletionSchema = useMemo(() => {
    const schema: Record<string, readonly string[]> = {};
    for (const table of databaseSchema?.tables ?? []) {
      const columns = table.columns;
      schema[table.name] = columns;
      if (table.schemaName) {
        schema[`${table.schemaName}.${table.name}`] = columns;
        if (table.schemaName === "public") {
          schema[table.name] = columns;
        }
      }
    }
    return schema;
  }, [databaseSchema]);

  const sqlAiSchemaSummary = useMemo(
    () =>
      (databaseSchema?.tables ?? [])
        .slice(0, 80)
        .map((table) => {
          const columns = table.columnDetails
            .slice(0, 24)
            .map((column) => `${column.name}:${column.columnType || column.dataType}`)
            .join(", ");
          return `${table.schemaName ? `${table.schemaName}.` : ""}${table.name}(${columns})`;
        })
        .join("\n"),
    [databaseSchema],
  );

  const selectedSqlConnection = useMemo(
    () => connections.find((item) => item.key === queryConnectionKey),
    [connections, queryConnectionKey],
  );

  const selectedObjectConnection = useMemo(
    () => connections.find((item) => item.key === objectConnectionKey),
    [connections, objectConnectionKey],
  );

  const selectedObject = useMemo(
    () =>
      objectSchema?.tables.find((item) => {
        const key = `${item.schemaName ?? objectSchema.databaseName ?? "default"}.${item.name}`;
        return key === selectedObjectKey;
      }),
    [objectSchema, selectedObjectKey],
  );

  const objectTreeData = useMemo<DatabaseObjectTreeNode[]>(() => {
    const tables = objectSchema?.tables ?? [];
    const grouped = new Map<string, DatabaseTableSchema[]>();
    for (const table of tables) {
      const group = table.schemaName ?? objectSchema?.databaseName ?? "当前数据库";
      grouped.set(group, [...(grouped.get(group) ?? []), table]);
    }
    return [...grouped.entries()].map(([group, items]) => ({
      key: `group:${group}`,
      title: `${group}（${items.length}）`,
      children: items.map((item) => {
        const meta = objectTypeMeta(item.objectType);
        const key = `${item.schemaName ?? objectSchema?.databaseName ?? "default"}.${item.name}`;
        return {
          key,
          object: item,
          title: (
            <Space size={6}>
              <span>{item.name}</span>
              <Tag color={meta.color}>{meta.text}</Tag>
            </Space>
          ),
        };
      }),
    }));
  }, [objectSchema]);

  const selectedObjectColumnOptions = useMemo(
    () =>
      (selectedObject?.columnDetails ?? []).map((column) => ({
        value: column.name,
        label: column.columnType || column.dataType
          ? `${column.name}（${column.columnType || column.dataType}）`
          : column.name,
      })),
    [selectedObject],
  );

  const primaryKeyColumns = useMemo(
    () => selectedObject?.indexes.find((index) => index.name === "PRIMARY")?.columns ?? [],
    [selectedObject],
  );

  const columnTypeOptions = useMemo(
    () =>
      (selectedObjectConnection?.dbType === "postgresql"
        ? commonPostgresColumnTypes
        : commonMysqlColumnTypes
      ).map((value) => ({ value })),
    [selectedObjectConnection?.dbType],
  );

  const showAllColumnTypes = () => true;

  const sqlDialect = useMemo<SqlDialectKey>(() => {
    if (selectedSqlConnection?.dbType === "mysql") return "mysql";
    if (selectedSqlConnection?.dbType === "postgresql") return "postgresql";
    return "standard";
  }, [selectedSqlConnection?.dbType]);

  async function loadDatabaseNames(connectionKey: string) {
    setDatabaseLoading(true);
    setSqlExecutionStatus("loading_databases");
    setSqlExecutionMessage("正在读取数据库列表...");
    try {
      const result: DatabaseNameListResult = await databaseOpsApi.listDatabaseNames({
        connectionKey,
      });
      setDatabaseNames(result.databases);
      const preferred = result.current && result.databases.includes(result.current)
        ? result.current
        : result.databases[0];
      setQueryDatabaseName(preferred);
      setSqlExecutionStatus("idle");
      setSqlExecutionMessage(
        result.databases.length > 0
          ? `已读取 ${result.databases.length} 个数据库`
          : "未读取到可选数据库",
      );
    } catch (error) {
      setDatabaseNames([]);
      setQueryDatabaseName(undefined);
      setSqlExecutionStatus("error");
      setSqlExecutionMessage(getErrorMessage(error));
    } finally {
      setDatabaseLoading(false);
    }
  }

  async function loadDatabaseSchema(connectionKey: string, databaseName?: string) {
    setSchemaLoading(true);
    try {
      const result = await databaseOpsApi.listDatabaseSchema({
        connectionKey,
        databaseName,
      });
      setDatabaseSchema(result);
    } catch {
      // 结构补全是增强能力，失败时保留 SQL 关键字补全，避免打断查询流程。
      setDatabaseSchema(null);
    } finally {
      setSchemaLoading(false);
    }
  }

  async function loadObjectDatabaseNames(connectionKey: string) {
    setObjectLoading(true);
    setObjectSchema(null);
    setSelectedObjectKey(undefined);
    try {
      const result = await databaseOpsApi.listDatabaseNames({ connectionKey });
      setObjectDatabaseNames(result.databases);
      const preferred = result.current && result.databases.includes(result.current)
        ? result.current
        : result.databases[0];
      setObjectDatabaseName(preferred);
    } catch (error) {
      setObjectDatabaseNames([]);
      setObjectDatabaseName(undefined);
      message.error(getErrorMessage(error));
    } finally {
      setObjectLoading(false);
    }
  }

  async function loadObjectSchema(connectionKey: string, databaseName?: string) {
    setObjectLoading(true);
    setSelectedObjectKey(undefined);
    try {
      const result = await databaseOpsApi.listDatabaseSchema({
        connectionKey,
        databaseName,
      });
      setObjectSchema(result);
    } catch (error) {
      setObjectSchema(null);
      message.error(getErrorMessage(error));
    } finally {
      setObjectLoading(false);
    }
  }

  async function loadExportDatabaseNames(connectionKey: string) {
    setExportLoading(true);
    setExportDatabaseNames([]);
    setExportDatabaseName(undefined);
    setExportSchema(null);
    setExportTableName(undefined);
    setExportResult(null);
    try {
      const result = await databaseOpsApi.listDatabaseNames({ connectionKey });
      setExportDatabaseNames(result.databases);
      const preferred = result.current && result.databases.includes(result.current)
        ? result.current
        : result.databases[0];
      setExportDatabaseName(preferred);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setExportLoading(false);
    }
  }

  async function loadExportSchema(connectionKey: string, databaseName: string) {
    setExportLoading(true);
    setExportSchema(null);
    setExportTableName(undefined);
    try {
      const result = await databaseOpsApi.listDatabaseSchema({
        connectionKey,
        databaseName,
      });
      setExportSchema(result);
      const firstTable = result.tables.find((table) => !table.objectType.toUpperCase().includes("VIEW"));
      setExportTableName(firstTable?.name);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setExportLoading(false);
    }
  }

  async function loadRedisDatabases(connectionKey: string) {
    setRedisDatabaseLoading(true);
    setRedisDatabases([]);
    setRedisKeys([]);
    setRedisTreeKeys([]);
    setRedisSelectedTreeKey("__all__");
    setRedisPattern("*");
    setRedisPage(1);
    setRedisCursor(0);
    setRedisPageCursors({ 1: 0 });
    setRedisPreview(null);
    try {
      const result = await databaseOpsApi.listRedisDatabases({ connectionKey });
      setRedisDatabases(result.databases);
      const selected = result.databases.some((item) => item.name === result.current)
        ? result.current
        : result.databases[0]?.name;
      setRedisDatabaseName(
        selected,
      );
      setRedisSelectedKeyCount(
        result.databases.find((item) => item.name === selected)?.keyCount ?? 0,
      );
    } catch (error) {
      setRedisDatabaseName(undefined);
      message.error(getErrorMessage(error));
    } finally {
      setRedisDatabaseLoading(false);
    }
  }

  async function loadRedisKeyTree(connectionKey: string, databaseName: string) {
    setRedisTreeLoading(true);
    setRedisTreeKeys([]);
    setRedisSelectedTreeKey("__all__");
    setRedisPattern("*");
    setRedisPage(1);
    setRedisCursor(0);
    setRedisPageCursors({ 1: 0 });
    setRedisSelectedLeafKeys([]);
    try {
      const result = await databaseOpsApi.listRedisKeyTree({
        connectionKey,
        databaseName,
        pattern: "*",
        limit: 20000,
      });
      setRedisTreeKeys(result.keys);
      setRedisSelectedLeafKeys(result.keys);
      setRedisSelectedKeyCount(result.keys.length);
      if (result.truncated) {
        message.warning("Key 树仅加载前 20000 个 Key，请通过层级或搜索缩小范围");
      }
      void loadRedisTreePage(1, redisPageSize, result.keys, connectionKey, databaseName);
    } catch (error) {
      setRedisSelectedLeafKeys([]);
      setRedisSelectedKeyCount(0);
      message.error(getErrorMessage(error));
    } finally {
      setRedisTreeLoading(false);
    }
  }

  async function loadData() {
    setLoading(true);
    try {
      const [connectionRows, credentialRows, serverRows] = await Promise.all([
        databaseOpsApi.listConnections(),
        credentialVaultApi.list(),
        sshServerApi.list(),
      ]);
      setConnections(connectionRows);
      setCredentials(credentialRows);
      setServers(serverRows);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    loadData();
  }, []);

  useEffect(() => {
    if (!queryConnectionKey) {
      setDatabaseNames([]);
      setQueryDatabaseName(undefined);
      setDatabaseSchema(null);
      setSqlExecutionStatus("idle");
      setSqlExecutionMessage("请选择数据库连接");
      return;
    }
    void loadDatabaseNames(queryConnectionKey);
  }, [queryConnectionKey]);

  useEffect(() => {
    if (!queryConnectionKey || !queryDatabaseName) {
      setDatabaseSchema(null);
      return;
    }
    void loadDatabaseSchema(queryConnectionKey, queryDatabaseName);
  }, [queryConnectionKey, queryDatabaseName]);

  useEffect(() => {
    if (!objectConnectionKey) {
      setObjectDatabaseNames([]);
      setObjectDatabaseName(undefined);
      setObjectSchema(null);
      setSelectedObjectKey(undefined);
      return;
    }
    void loadObjectDatabaseNames(objectConnectionKey);
  }, [objectConnectionKey]);

  useEffect(() => {
    if (!objectConnectionKey || !objectDatabaseName) {
      setObjectSchema(null);
      setSelectedObjectKey(undefined);
      return;
    }
    void loadObjectSchema(objectConnectionKey, objectDatabaseName);
  }, [objectConnectionKey, objectDatabaseName]);

  useEffect(() => {
    if (!redisConnectionKey) {
      setRedisDatabases([]);
      setRedisDatabaseName(undefined);
      setRedisTreeKeys([]);
      setRedisSelectedTreeKey("__all__");
      setRedisSelectedKeyCount(0);
      setRedisSelectedLeafKeys([]);
      setRedisKeys([]);
      setRedisPreview(null);
      return;
    }
    void loadRedisDatabases(redisConnectionKey);
  }, [redisConnectionKey]);

  useEffect(() => {
    if (!redisConnectionKey || !redisDatabaseName) {
      setRedisTreeKeys([]);
      setRedisSelectedTreeKey("__all__");
      setRedisSelectedKeyCount(0);
      setRedisSelectedLeafKeys([]);
      return;
    }
    void loadRedisKeyTree(redisConnectionKey, redisDatabaseName);
  }, [redisConnectionKey, redisDatabaseName]);

  useEffect(() => {
    if (!exportConnectionKey) {
      setExportDatabaseNames([]);
      setExportDatabaseName(undefined);
      setExportSchema(null);
      setExportTableName(undefined);
      return;
    }
    void loadExportDatabaseNames(exportConnectionKey);
  }, [exportConnectionKey]);

  useEffect(() => {
    if (!exportConnectionKey || !exportDatabaseName) {
      setExportSchema(null);
      setExportTableName(undefined);
      return;
    }
    void loadExportSchema(exportConnectionKey, exportDatabaseName);
  }, [exportConnectionKey, exportDatabaseName]);

  function openObjectInSql(object: DatabaseTableSchema) {
    if (!objectConnectionKey) return;
    const dbType = selectedObjectConnection?.dbType;
    const tableName = tableSqlName(object, dbType);
    setQueryConnectionKey(objectConnectionKey);
    setQueryDatabaseName(objectDatabaseName);
    setQuerySql(`SELECT * FROM ${tableName} LIMIT 500;`);
    setActiveTab("sql");
  }

  function openStructureDrawer() {
    addColumnForm.resetFields();
    modifyColumnForm.resetFields();
    addIndexForm.resetFields();
    addColumnForm.setFieldsValue({ dataType: "varchar(255)", nullable: true });
    addIndexForm.setFieldsValue({ unique: false });
    if (selectedObject?.columnDetails[0]) {
      const column = selectedObject.columnDetails[0];
      modifyColumnForm.setFieldsValue({
        oldName: column.name,
        newName: column.name,
        dataType: column.columnType || column.dataType,
        nullable: column.nullable,
        defaultValue: column.defaultValue ?? undefined,
      });
    }
    setStructureDrawerOpen(true);
  }

  function openCreateTableDrawer() {
    createTableForm.resetFields();
    createTableForm.setFieldsValue({
      tableName: "",
      columns: [
        { name: "id", dataType: "bigint", nullable: false, primaryKey: true },
        { name: "name", dataType: "varchar(255)", nullable: true, primaryKey: false },
      ],
    });
    setCreateTableDrawerOpen(true);
  }

  async function executeStructureSql(sql: string, successMessage: string) {
    if (!objectConnectionKey || !objectDatabaseName) return;
    await new Promise<void>((resolve, reject) => {
      Modal.confirm({
        title: "确认执行表结构变更",
        okText: "执行",
        cancelText: "取消",
        width: 720,
        content: (
          <Space direction="vertical" style={{ width: "100%" }}>
            <Alert
              showIcon
              type="warning"
              message="该操作会直接修改数据库结构，请确认已了解影响。"
            />
            <pre style={{ maxHeight: 260, overflow: "auto", whiteSpace: "pre-wrap" }}>{sql}</pre>
          </Space>
        ),
        onOk: async () => {
          setStructureSubmitting(true);
          try {
            const results = await databaseOpsApi.executeSqlBatch({
              connectionKey: objectConnectionKey,
              databaseName: objectDatabaseName,
              sql,
              page: 1,
              pageSize: 500,
            });
            const failed = results.find((item) => item.status === "error");
            if (failed) {
              throw new Error(failed.message);
            }
            message.success(successMessage);
            await loadObjectSchema(objectConnectionKey, objectDatabaseName);
            resolve();
          } catch (error) {
            message.error(getErrorMessage(error));
            reject(error);
          } finally {
            setStructureSubmitting(false);
          }
        },
        onCancel: () => resolve(),
      });
    });
  }

  async function createTable() {
    const values = await createTableForm.validateFields();
    const tableName = values.tableName.trim();
    if (!tableName) {
      message.warning("请输入表名");
      return;
    }
    const columns = (values.columns ?? []).filter((column) => column.name?.trim());
    if (columns.length === 0) {
      message.warning("至少需要添加一个字段");
      return;
    }
    const dbType = selectedObjectConnection?.dbType;
    const primaryColumns = columns
      .filter((column) => column.primaryKey)
      .map((column) => quoteIdentifier(column.name.trim(), dbType));
    const definitions = columns.map((column) => createTableColumnDefinitionSql(column, dbType));
    if (primaryColumns.length > 0) {
      definitions.push(`PRIMARY KEY (${primaryColumns.join(", ")})`);
    }
    const sql = `CREATE TABLE ${createTableSqlName(tableName, dbType)} (\n  ${definitions.join(",\n  ")}\n);`;
    await executeStructureSql(sql, "数据表已新增");
    setCreateTableDrawerOpen(false);
  }

  async function dropTable() {
    if (!selectedObject) return;
    const dbType = selectedObjectConnection?.dbType;
    const isView = objectTypeMeta(selectedObject.objectType).text === "视图";
    const sql = `DROP ${isView ? "VIEW" : "TABLE"} ${tableSqlName(selectedObject, dbType)};`;
    await executeStructureSql(sql, isView ? "视图已删除" : "数据表已删除");
    setSelectedObjectKey(undefined);
  }

  async function addColumn() {
    if (!selectedObject) return;
    const values = await addColumnForm.validateFields();
    const dbType = selectedObjectConnection?.dbType;
    const sql = `ALTER TABLE ${tableSqlName(selectedObject, dbType)} ADD COLUMN ${quoteIdentifier(values.name, dbType)} ${columnDefinitionSql(values)};`;
    await executeStructureSql(sql, "字段已新增");
    addColumnForm.resetFields();
    addColumnForm.setFieldsValue({ dataType: "varchar(255)", nullable: true });
  }

  async function modifyColumn() {
    if (!selectedObject) return;
    const values = await modifyColumnForm.validateFields();
    const dbType = selectedObjectConnection?.dbType;
    const oldName = values.oldName;
    const newName = values.newName?.trim() || oldName;
    const tableName = tableSqlName(selectedObject, dbType);
    let sql = "";
    if (dbType === "mysql") {
      sql = `ALTER TABLE ${tableName} CHANGE COLUMN ${quoteIdentifier(oldName, dbType)} ${quoteIdentifier(newName, dbType)} ${columnDefinitionSql(values)};`;
    } else {
      const statements = [];
      if (newName !== oldName) {
        statements.push(`ALTER TABLE ${tableName} RENAME COLUMN ${quoteIdentifier(oldName, dbType)} TO ${quoteIdentifier(newName, dbType)};`);
      }
      const targetName = quoteIdentifier(newName, dbType);
      if (values.dataType?.trim()) {
        statements.push(`ALTER TABLE ${tableName} ALTER COLUMN ${targetName} TYPE ${values.dataType.trim()};`);
      }
      statements.push(`ALTER TABLE ${tableName} ALTER COLUMN ${targetName} ${values.nullable ? "DROP" : "SET"} NOT NULL;`);
      const defaultSql = normalizeDefaultValue(values.defaultValue);
      statements.push(
        defaultSql
          ? `ALTER TABLE ${tableName} ALTER COLUMN ${targetName} SET DEFAULT ${defaultSql};`
          : `ALTER TABLE ${tableName} ALTER COLUMN ${targetName} DROP DEFAULT;`,
      );
      sql = statements.join("\n");
    }
    await executeStructureSql(sql, "字段已修改");
  }

  async function dropColumn(column: DatabaseColumnSchema) {
    if (!selectedObject) return;
    const dbType = selectedObjectConnection?.dbType;
    const sql = `ALTER TABLE ${tableSqlName(selectedObject, dbType)} DROP COLUMN ${quoteIdentifier(column.name, dbType)};`;
    await executeStructureSql(sql, "字段已删除");
  }

  async function setPrimaryKey(column: DatabaseColumnSchema) {
    if (!selectedObject) return;
    const dbType = selectedObjectConnection?.dbType;
    const sql = `ALTER TABLE ${tableSqlName(selectedObject, dbType)} ADD PRIMARY KEY (${quoteIdentifier(column.name, dbType)});`;
    await executeStructureSql(sql, "主键已设置");
  }

  async function addIndex() {
    if (!selectedObject) return;
    const values = await addIndexForm.validateFields();
    const dbType = selectedObjectConnection?.dbType;
    const columns = values.columns.map((name) => quoteIdentifier(name, dbType)).join(", ");
    const sql = `CREATE ${values.unique ? "UNIQUE " : ""}INDEX ${quoteIdentifier(values.name, dbType)} ON ${tableSqlName(selectedObject, dbType)} (${columns});`;
    await executeStructureSql(sql, "索引已新增");
    addIndexForm.resetFields();
    addIndexForm.setFieldsValue({ unique: false });
  }

  async function dropIndex(index: DatabaseIndexSchema) {
    if (!selectedObject) return;
    const dbType = selectedObjectConnection?.dbType;
    const sql = dbType === "mysql"
      ? index.name === "PRIMARY"
        ? `ALTER TABLE ${tableSqlName(selectedObject, dbType)} DROP PRIMARY KEY;`
        : `DROP INDEX ${quoteIdentifier(index.name, dbType)} ON ${tableSqlName(selectedObject, dbType)};`
      : `DROP INDEX ${selectedObject.schemaName ? `${quoteIdentifier(selectedObject.schemaName, dbType)}.` : ""}${quoteIdentifier(index.name, dbType)};`;
    await executeStructureSql(sql, "索引已删除");
  }

  function openCreateDrawer() {
    setEditing(null);
    form.setFieldsValue(defaultFormValues);
    setDrawerOpen(true);
  }

  function openEditDrawer(record: DatabaseConnection) {
    setEditing(record);
    form.setFieldsValue({
      ...record,
      password: "",
      clearPassword: false,
      status: record.status,
    });
    setDrawerOpen(true);
  }

  async function submitForm() {
    try {
      const values = await form.validateFields();
      const hasNewPassword = Boolean(values.password?.trim());
      const keepsExistingPassword = Boolean(editing?.hasPassword) && !values.clearPassword;
      if (values.authType === "direct_password" && !hasNewPassword && !keepsExistingPassword) {
        message.warning("直接密码认证需要填写密码");
        return;
      }
      const input: UpsertDatabaseConnectionInput = {
        ...defaultFormValues,
        ...values,
        // 未挂载的条件字段不会出现在 validateFields 结果中，提交前必须补齐后端必填字段。
        credentialRef: values.authType === "credential_ref" ? values.credentialRef : "",
        password: values.authType === "direct_password" && hasNewPassword
          ? values.password
          : null,
        clearPassword: values.clearPassword ?? false,
        sshServerAlias: values.connectionMode === "ssh_tunnel" ? values.sshServerAlias : "",
        status: "unknown",
      };
      await databaseOpsApi.upsertConnection(input);
      message.success("数据库连接已保存");
      setDrawerOpen(false);
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function deleteConnection(key: string) {
    try {
      await databaseOpsApi.deleteConnection(key);
      message.success("数据库连接已删除");
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function testConnection(key: string) {
    setTestingKey(key);
    try {
      const result = await databaseOpsApi.testConnection(key);
      if (result.ok) {
        message.success(`${result.message}，耗时 ${result.latencyMs}ms`);
      } else {
        message.warning(result.message);
      }
      await loadData();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setTestingKey(null);
    }
  }

  async function executeQuery() {
    if (!queryConnectionKey) {
      message.warning("请先选择数据库连接");
      return;
    }
    setQueryResults([]);
    setActiveQueryResultKey("0");
    setQueryLoading(true);
    setSqlExecutionStatus("running");
    setSqlExecutionMessage("SQL 执行中...");
    try {
      const results = await databaseOpsApi.executeSqlBatch({
        connectionKey: queryConnectionKey,
        databaseName: queryDatabaseName,
        sql: querySql,
        page: 1,
        pageSize: 500,
      });
      const errorCount = results.filter((item) => item.status === "error").length;
      const successCount = results.length - errorCount;
      setQueryResults(results);
      setActiveQueryResultKey(String(Math.max(0, results.findIndex((item) => item.status === "error"))));
      setSqlExecutionStatus(errorCount > 0 ? "error" : "success");
      setSqlExecutionMessage(
        errorCount > 0
          ? `已执行 ${successCount}/${results.length} 条，${errorCount} 条失败`
          : `已成功执行 ${results.length} 条 SQL`,
      );
      if (errorCount > 0) {
        message.error("存在 SQL 执行失败，请查看结果 Tab");
      } else {
        message.success(`已成功执行 ${results.length} 条 SQL`);
      }
    } catch (error) {
      const messageText = getErrorMessage(error);
      setSqlExecutionStatus("error");
      setSqlExecutionMessage(messageText);
      message.error(messageText);
    } finally {
      setQueryLoading(false);
    }
  }

  async function runDatabaseExport() {
    if (!exportConnectionKey || !exportDatabaseName) {
      message.warning("请先选择连接和数据库");
      return;
    }
    if (exportMode === "table_csv" && !exportTableName) {
      message.warning("请选择要导出的数据表");
      return;
    }
    if (exportMode === "query_csv" && !exportSql.trim()) {
      message.warning("请输入要导出的 SELECT SQL");
      return;
    }
    setExportLoading(true);
    setExportResult(null);
    try {
      const result = await databaseOpsApi.exportDatabase({
        connectionKey: exportConnectionKey,
        databaseName: exportDatabaseName,
        mode: exportMode,
        tableName: exportMode === "query_csv" ? null : exportTableName ?? null,
        sql: exportMode === "query_csv" ? exportSql : null,
        includeData: exportIncludeData,
        maxRows: exportMode === "sql_backup" ? null : exportMaxRows,
      });
      setExportResult(result);
      message.success(result.message);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setExportLoading(false);
    }
  }

  async function runSqlAi(mode = sqlAiMode) {
    const freePrompt = sqlAiPrompt.trim();
    if (!queryConnectionKey || !selectedSqlConnection) {
      message.warning("请先选择 MySQL / PostgreSQL 数据库连接");
      return;
    }
    if (mode === "ask" && !freePrompt) {
      message.warning("请输入要问 AI 的问题");
      return;
    }
    if ((mode === "fix" || mode === "optimize") && !querySql.trim()) {
      message.warning("请先输入 SQL");
      return;
    }
    setSqlAiLoading(true);
    setSqlAiAnswer("AI 思考中...");
    setSqlAiMeta("");
    const failedResult = queryResults.find((item) => item.status === "error");
    const latestResult = queryResults[queryResults.length - 1];
    const dialectLabel = dbTypeLabel[selectedSqlConnection.dbType];
    const dialectRule = selectedSqlConnection.dbType === "mysql"
      ? "只能生成 MySQL 语法，不要输出 PostgreSQL 的 pg_stat_activity、current_setting、:: 类型转换、RETURNING 等语法。"
      : "只能生成 PostgreSQL 语法，不要输出 MySQL 的 information_schema.processlist、SHOW VARIABLES、反引号等语法。";
    const statusText = statusMeta[selectedSqlConnection.status]?.text ?? selectedSqlConnection.status;
    const versionProbeSql = selectedSqlConnection.dbType === "mysql"
      ? "SELECT VERSION() AS version, @@version_comment AS version_comment, @@version_compile_os AS version_compile_os, @@max_connections AS max_connections;"
      : "SELECT version() AS version, current_setting('server_version') AS server_version, current_database() AS current_database, current_schema() AS current_schema;";
    let runtimeContext = "";
    try {
      const probeResults = await databaseOpsApi.executeSqlBatch({
        connectionKey: selectedSqlConnection.key,
        databaseName: queryDatabaseName,
        sql: versionProbeSql,
        page: 1,
        pageSize: 5,
      });
      const probe = probeResults[0];
      runtimeContext = probe && probe.status !== "error"
        ? [
          "数据库运行时探测：成功",
          `版本/状态查询 SQL：${versionProbeSql}`,
          `版本/状态查询结果：\n${formatSqlAiProbeRows(probe.rows)}`,
          `探测耗时：${probe.durationMs}ms`,
        ].join("\n")
        : [
          "数据库运行时探测：失败",
          `版本/状态查询 SQL：${versionProbeSql}`,
          `失败原因：${probe?.message ?? "未返回探测结果"}`,
        ].join("\n");
    } catch (error) {
      runtimeContext = [
        "数据库运行时探测：失败",
        `版本/状态查询 SQL：${versionProbeSql}`,
        `失败原因：${getErrorMessage(error)}`,
      ].join("\n");
    }
    const taskText: Record<SqlAiMode, string> = {
      ask: "回答用户关于当前数据库、SQL、表结构或查询结果的问题。",
      generate: "根据用户需求生成可直接执行的 SQL。只输出必要解释和 SQL 代码块。",
      fix: "纠正当前 SQL 的语法、表名字段名、分页或方言问题。优先给出修正后的 SQL。",
      optimize: "分析当前 SQL 的性能问题，给出索引建议、改写建议和优化后的 SQL。",
    };
    const promptParts = [
      `任务：${taskText[mode]}`,
      `当前数据库类型：${dialectLabel}`,
      `数据库方言硬约束：${dialectRule}`,
      `连接名称：${selectedSqlConnection.name}`,
      `连接状态：${statusText}（${selectedSqlConnection.status}）`,
      `连接模式：${selectedSqlConnection.connectionMode === "ssh_tunnel" ? `SSH 隧道（${selectedSqlConnection.sshServerAlias || "未配置服务器"}）` : "直连"}`,
      `连接端点：${selectedSqlConnection.host}:${selectedSqlConnection.port}`,
      `连接用户：${selectedSqlConnection.username || "未填写"}`,
      `默认库配置：${selectedSqlConnection.databaseName || "未配置"}`,
      `当前数据库：${queryDatabaseName ?? "未选择"}`,
      runtimeContext,
      sqlAiSchemaSummary ? `当前库结构摘要：\n${sqlAiSchemaSummary}` : "当前库结构摘要：未加载",
      querySql.trim() ? `当前 SQL：\n${querySql.trim()}` : "当前 SQL：未输入",
      failedResult ? `最近错误：${failedResult.message}` : "",
      latestResult && latestResult.status !== "error"
        ? `最近执行结果：${latestResult.message}，列：${latestResult.columns.join(", ")}`
        : "",
      freePrompt ? `用户问题/需求：${freePrompt}` : "",
      `要求：回答使用中文；SQL 使用 \`\`\`sql 代码块；所有 SQL 必须严格使用 ${dialectLabel} 方言；如果问题涉及其他数据库类型，必须先指出当前连接是 ${dialectLabel}，不能直接给其他数据库方言；如果会修改数据，必须明确提示风险；不要编造不存在的表字段。`,
    ].filter(Boolean);
    try {
      const result = await aiProviderApi.ask({
        prompt: promptParts.join("\n\n"),
        skillScope: "sql",
        useSkillTrigger: true,
        systemPrompt: [
          "你是 Tauri SSH 数据库 SQL 助手，擅长 SQL 诊断、查询生成和性能调优。",
          `当前连接数据库类型已经确定为 ${dialectLabel}。`,
          dialectRule,
          "不要同时给多个数据库类型的答案，除非用户明确要求对比；默认只回答当前连接数据库类型。",
          "所有 SQL 示例必须放在 ```sql 代码块中。",
        ].join("\n"),
      });
      setSqlAiAnswer(result.answer);
      setSqlAiMeta(`${result.providerName} / ${result.model} / ${result.latencyMs}ms`);
    } catch (error) {
      const errorMessage = getErrorMessage(error);
      setSqlAiAnswer(`AI 调用失败：${errorMessage}`);
      message.error(errorMessage);
    } finally {
      setSqlAiLoading(false);
    }
  }

  async function saveSqlAiExperience() {
    const answer = sqlAiAnswer.trim();
    if (!answer || answer === "AI 思考中...") {
      message.warning("当前没有可沉淀的 SQL AI 输出");
      return;
    }
    const failedResult = queryResults.find((item) => item.status === "error");
    const latestResult = queryResults[queryResults.length - 1];
    const modeLabel: Record<SqlAiMode, string> = {
      ask: "自主提问",
      generate: "生成 SQL",
      fix: "SQL 纠错",
      optimize: "SQL 调优",
    };
    setSqlExperienceSaving(true);
    try {
      const experience = await aiSkillApi.upsertExperience({
        title: `SQL经验：${modeLabel[sqlAiMode]}`,
        symptom: [
          `连接：${selectedSqlConnection?.name ?? "未选择"}`,
          `数据库类型：${selectedSqlConnection ? dbTypeLabel[selectedSqlConnection.dbType] : "未选择"}`,
          `当前数据库：${queryDatabaseName ?? "未选择"}`,
          sqlAiPrompt.trim() ? `用户问题/需求：\n${sqlAiPrompt.trim()}` : "",
          querySql.trim() ? `当前 SQL：\n${querySql.trim()}` : "",
          failedResult ? `最近错误：${failedResult.message}` : "",
          latestResult && latestResult.status !== "error" ? `最近执行结果：${latestResult.message}` : "",
        ].filter(Boolean).join("\n\n"),
        cause: "",
        solution: answer,
        scenario: "sql",
        source: "ai",
        tags: ["sql", "database", "ai", sqlAiMode, selectedSqlConnection?.dbType ?? ""].filter(
          (tag): tag is string => Boolean(tag),
        ),
        enabled: true,
      });
      message.success(`已沉淀经验：${experience.title}`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSqlExperienceSaving(false);
    }
  }

  async function loadRedisTreePage(
    page = 1,
    pageSize = redisPageSize,
    leafKeys = redisSelectedLeafKeys,
    connectionKey = redisConnectionKey,
    databaseName = redisDatabaseName,
  ) {
    if (!connectionKey) {
      message.warning("请先选择 Redis 连接");
      return;
    }
    if (!databaseName) {
      message.warning("请先选择 Redis DB");
      return;
    }
    const normalizedKeys = [...leafKeys].sort((a, b) => a.localeCompare(b));
    const start = (page - 1) * pageSize;
    const pageKeys = normalizedKeys.slice(start, start + pageSize);
    setRedisLoading(true);
    try {
      if (pageKeys.length === 0) {
        setRedisKeys([]);
      } else {
        const result = await databaseOpsApi.describeRedisKeys({
          connectionKey,
          databaseName,
          keys: pageKeys,
        });
        setRedisKeys(result.keys);
      }
      setRedisPage(page);
      setRedisCursor(0);
      setRedisSelectedKeyCount(normalizedKeys.length);
      setRedisPreview(null);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setRedisLoading(false);
    }
  }

  async function scanRedis(page = 1, count = redisPageSize, pattern = redisPattern) {
    if (!redisConnectionKey) {
      message.warning("请先选择 Redis 连接");
      return;
    }
    if (!redisDatabaseName) {
      message.warning("请先选择 Redis DB");
      return;
    }
    setRedisSelectedLeafKeys([]);
    setRedisSelectedKeyCount(0);
    setRedisLoading(true);
    try {
      const cursorSnapshot = { ...redisPageCursors };
      const knownPages = Object.keys(cursorSnapshot)
        .map(Number)
        .filter((item) => item <= page && Number.isFinite(cursorSnapshot[item]))
        .sort((a, b) => b - a);
      let scanPage = knownPages[0] ?? 1;
      let cursor = cursorSnapshot[scanPage] ?? 0;
      let pageResult: Awaited<ReturnType<typeof databaseOpsApi.scanRedisKeys>> | null = null;
      let loadedPage = scanPage;

      while (scanPage <= page) {
        pageResult = await databaseOpsApi.scanRedisKeys({
          connectionKey: redisConnectionKey,
          databaseName: redisDatabaseName,
          pattern: pattern || "*",
          cursor,
          count,
        });
        loadedPage = scanPage;
        cursor = pageResult.cursor;
        cursorSnapshot[scanPage + 1] = cursor;
        if (cursor === 0 && scanPage < page) {
          break;
        }
        scanPage += 1;
      }

      setRedisPage(loadedPage);
      setRedisCursor(pageResult?.cursor ?? 0);
      setRedisKeys(pageResult?.keys ?? []);
      setRedisPageCursors(cursorSnapshot);
      setRedisPreview(null);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setRedisLoading(false);
    }
  }

  async function previewRedisValue(key: string) {
    if (!redisConnectionKey) return;
    setRedisLoading(true);
    try {
      setRedisPreview(await databaseOpsApi.getRedisValuePreview({
        connectionKey: redisConnectionKey,
        databaseName: redisDatabaseName,
        key,
      }));
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setRedisLoading(false);
    }
  }

  function renderQueryResult(result: DatabaseQueryResult, index: number) {
    const rows = result.rows.map((row, rowIndex) => ({
      __rowId: `${index}-${result.page}-${rowIndex}`,
      ...row,
    }));
    return (
      <Space direction="vertical" style={{ width: "100%" }} size="small">
        {result.status === "error" ? (
          <Alert showIcon type="error" message={result.message} />
        ) : (
          <Table
            rowKey="__rowId"
            size="small"
            scroll={{ x: true }}
            loading={queryLoading}
            columns={result.columns.map((column) => ({
              title: column,
              dataIndex: column,
              ellipsis: true,
              render: (value: unknown) =>
                value == null
                  ? "null"
                  : typeof value === "object"
                    ? JSON.stringify(value)
                    : String(value ?? ""),
            }))}
            dataSource={rows}
            pagination={{ pageSize: 20 }}
          />
        )}
        <Text type="secondary">
          状态：{result.status}，语句：{result.statementType.toUpperCase()}，
          {result.columns.length > 0
            ? `返回 ${result.rowCount} 行，第 ${result.page} 页，单页 ${result.pageSize} 行`
            : result.status === "error"
              ? "未返回结果"
              : `影响 ${result.rowsAffected} 行`}
          ，耗时 {result.durationMs}ms
          {result.truncated ? "，还有更多结果，请继续分页查询" : ""}
        </Text>
      </Space>
    );
  }

  const columns: TableProps<DatabaseConnection>["columns"] = [
    {
      title: "连接",
      dataIndex: "name",
      width: 240,
      render: (_, record) => (
        <Space direction="vertical" size={0} style={{ minWidth: 180 }}>
          <Text strong ellipsis style={{ maxWidth: 220 }}>
            {record.name}
          </Text>
          <Text type="secondary" ellipsis style={{ maxWidth: 220 }}>
            {record.key}
          </Text>
        </Space>
      ),
    },
    {
      title: "类型",
      dataIndex: "dbType",
      width: 140,
      render: (value: DatabaseType) => <Tag color="blue">{dbTypeLabel[value]}</Tag>,
    },
    {
      title: "地址",
      width: 220,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text>{record.host}:{record.port}</Text>
          <Text type="secondary">{record.databaseName || "未指定库名"}</Text>
        </Space>
      ),
    },
    {
      title: "连接方式",
      width: 170,
      render: (_, record) =>
        record.connectionMode === "ssh_tunnel"
          ? `SSH 隧道：${record.sshServerAlias}`
          : "直连",
    },
    {
      title: "安全级别",
      dataIndex: "securityMode",
      width: 140,
      render: (value: DatabaseSecurityMode) => securityModeLabel[value],
    },
    {
      title: "状态",
      dataIndex: "status",
      width: 110,
      render: (value: DatabaseConnectionStatus) => statusTag(value),
    },
    {
      title: "操作",
      width: 280,
      render: (_, record) => (
        <Space>
          <Button
            size="small"
            icon={<PlugZap size={14} />}
            loading={testingKey === record.key}
            onClick={() => testConnection(record.key)}
          >
            测试
          </Button>
          <Button size="small" icon={<Edit3 size={14} />} onClick={() => openEditDrawer(record)}>
            编辑
          </Button>
          <Popconfirm
            title="确认删除该数据库连接？"
            okText="删除"
            cancelText="取消"
            onConfirm={() => deleteConnection(record.key)}
          >
            <Button size="small" danger icon={<Trash2 size={14} />}>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div className="prototype-page">
      <div className="prototype-page-header">
        <div>
          <Title level={3} style={{ margin: 0, fontSize: 24, lineHeight: "32px" }}>数据库管理</Title>
          <Paragraph type="secondary" style={{ marginBottom: 0 }}>
            管理 MySQL、PostgreSQL 和 Redis 连接，后续查询、导出、审批与 MCP 工具会复用这里的连接配置。
          </Paragraph>
        </div>
        <Space>
          <Button icon={<RefreshCw size={16} />} onClick={loadData} loading={loading}>
            刷新
          </Button>
          <Button type="primary" icon={<Plus size={16} />} onClick={openCreateDrawer}>
            新建连接
          </Button>
        </Space>
      </div>

      <Tabs
        activeKey={activeTab}
        onChange={setActiveTab}
        items={[
          {
            key: "connections",
            label: "连接",
            children: (
              <Card>
                <Table
                  rowKey="key"
                  columns={columns}
                  dataSource={connections}
                  loading={loading}
                  pagination={{ pageSize: 10 }}
                  scroll={{ x: 1300 }}
                />
              </Card>
            ),
          },
          {
            key: "objects",
            label: "对象浏览",
            children: (
              <Card>
                <Space direction="vertical" style={{ width: "100%" }} size="middle">
                  <Space.Compact style={{ width: "100%" }}>
                    <Select
                      style={{ width: 320 }}
                      placeholder="选择 MySQL / PostgreSQL 连接"
                      options={sqlConnectionOptions}
                      value={objectConnectionKey}
                      loading={loading}
                      onChange={setObjectConnectionKey}
                    />
                    <Select
                      style={{ width: 260 }}
                      placeholder="选择数据库"
                      options={objectDatabaseNameOptions}
                      value={objectDatabaseName}
                      loading={objectLoading}
                      disabled={!objectConnectionKey || objectLoading}
                      onChange={setObjectDatabaseName}
                    />
                    <Button
                      icon={<RefreshCw size={14} />}
                      loading={objectLoading}
                      disabled={!objectConnectionKey || !objectDatabaseName}
                      onClick={() => {
                        if (objectConnectionKey && objectDatabaseName) {
                          void loadObjectSchema(objectConnectionKey, objectDatabaseName);
                        }
                      }}
                    >
                      刷新对象
                    </Button>
                    <Button
                      type="primary"
                      icon={<Plus size={14} />}
                      disabled={!objectConnectionKey || !objectDatabaseName}
                      onClick={openCreateTableDrawer}
                    >
                      新增数据表
                    </Button>
                  </Space.Compact>
                  <div
                    className="prototype-database-object-layout"
                    style={{
                      display: "grid",
                      gap: 16,
                      gridTemplateColumns: "320px minmax(0, 1fr)",
                      alignItems: "stretch",
                    }}
                  >
                    <Card
                      className="prototype-database-object-tree-card"
                      size="small"
                      title={`对象树${objectSchema ? `（${objectSchema.tables.length}）` : ""}`}
                      loading={objectLoading}
                    >
                      {objectTreeData.length > 0 ? (
                        <Tree<DatabaseObjectTreeNode>
                          blockNode
                          showLine
                          defaultExpandAll
                          treeData={objectTreeData}
                          selectedKeys={selectedObjectKey ? [selectedObjectKey] : []}
                          onSelect={(_, info) => {
                            const node = info.node as DatabaseObjectTreeNode;
                            if (node.object) {
                              setSelectedObjectKey(node.key);
                            }
                          }}
                        />
                      ) : (
                        <Text type="secondary">选择连接和数据库后加载对象。</Text>
                      )}
                    </Card>
                    <Space direction="vertical" style={{ width: "100%" }} size="middle">
                      <Card
                        size="small"
                        title={
                          selectedObject
                            ? `${selectedObject.schemaName ? `${selectedObject.schemaName}.` : ""}${selectedObject.name}`
                            : "对象详情"
                        }
                        extra={
                          selectedObject ? (
                            <Space>
                              <Button size="small" onClick={() => openObjectInSql(selectedObject)}>
                                查询数据
                              </Button>
                              <Button size="small" type="primary" onClick={openStructureDrawer}>
                                编辑结构
                              </Button>
                              <Popconfirm
                                title={`确认删除 ${selectedObject.name}？`}
                                description="该操作会直接删除表或视图，请确认已备份。"
                                okText="删除"
                                cancelText="取消"
                                onConfirm={dropTable}
                              >
                                <Button size="small" danger disabled={structureSubmitting}>
                                  {objectTypeMeta(selectedObject.objectType).text === "视图" ? "删除视图" : "删除表"}
                                </Button>
                              </Popconfirm>
                            </Space>
                          ) : null
                        }
                      >
                        {selectedObject ? (
                          <Space direction="vertical" style={{ width: "100%" }} size="middle">
                            <Space>
                              <Tag color={objectTypeMeta(selectedObject.objectType).color}>
                                {objectTypeMeta(selectedObject.objectType).text}
                              </Tag>
                              <Text type="secondary">{selectedObject.columns.length} 个字段</Text>
                              <Text type="secondary">{selectedObject.indexes.length} 个索引</Text>
                            </Space>
                            <Table<DatabaseColumnSchema>
                              rowKey="name"
                              size="small"
                              pagination={false}
                              columns={[
                                { title: "字段名", dataIndex: "name", ellipsis: true },
                                {
                                  title: "类型",
                                  ellipsis: true,
                                  render: (_, record) => record.columnType || record.dataType || "-",
                                },
                                {
                                  title: "可空",
                                  dataIndex: "nullable",
                                  width: 80,
                                  render: (nullable: boolean) => (
                                    <Tag color={nullable ? "default" : "orange"}>
                                      {nullable ? "是" : "否"}
                                    </Tag>
                                  ),
                                },
                                {
                                  title: "默认值",
                                  dataIndex: "defaultValue",
                                  ellipsis: true,
                                  render: (value?: string | null) => value ?? "NULL",
                                },
                                {
                                  title: "额外",
                                  ellipsis: true,
                                  render: (_, record) => {
                                    const tags = [];
                                    if (primaryKeyColumns.includes(record.name)) {
                                      tags.push(<Tag key="pk" color="gold">主键</Tag>);
                                    }
                                    if (record.extra) {
                                      tags.push(<Tag key="extra">{record.extra}</Tag>);
                                    }
                                    return tags.length > 0 ? <Space size={4}>{tags}</Space> : "-";
                                  },
                                },
                                {
                                  title: "操作",
                                  width: 150,
                                  render: (_, record) => (
                                    <Space size={4}>
                                      {!primaryKeyColumns.includes(record.name) && (
                                        <Popconfirm
                                          title={`确认将 ${record.name} 设置为主键？`}
                                          okText="设置"
                                          cancelText="取消"
                                          onConfirm={() => setPrimaryKey(record)}
                                        >
                                          <Button size="small" disabled={structureSubmitting}>
                                            设主键
                                          </Button>
                                        </Popconfirm>
                                      )}
                                      <Popconfirm
                                        title={`确认删除字段 ${record.name}？`}
                                        okText="删除"
                                        cancelText="取消"
                                        onConfirm={() => dropColumn(record)}
                                      >
                                        <Button size="small" danger disabled={structureSubmitting}>
                                          删除
                                        </Button>
                                      </Popconfirm>
                                    </Space>
                                  ),
                                },
                              ]}
                              dataSource={selectedObject.columnDetails}
                            />
                          </Space>
                        ) : (
                          <Text type="secondary">从左侧对象树选择表或视图后查看字段和索引。</Text>
                        )}
                      </Card>
                      <Card size="small" title="索引">
                        {selectedObject?.indexes.length ? (
                          <Table<DatabaseIndexSchema>
                            rowKey="name"
                            size="small"
                            pagination={false}
                            columns={[
                              { title: "索引名", dataIndex: "name", ellipsis: true },
                              {
                                title: "字段",
                                dataIndex: "columns",
                                ellipsis: true,
                                render: (columns: string[]) => columns.join(", ") || "-",
                              },
                              {
                                title: "唯一",
                                dataIndex: "unique",
                                width: 90,
                                render: (unique: boolean) => (
                                  <Tag color={unique ? "green" : "default"}>
                                    {unique ? "是" : "否"}
                                  </Tag>
                                ),
                              },
                              {
                                title: "操作",
                                width: 90,
                                render: (_, record) => (
                                  <Popconfirm
                                    title={`确认删除索引 ${record.name}？`}
                                    okText="删除"
                                    cancelText="取消"
                                    onConfirm={() => dropIndex(record)}
                                  >
                                    <Button size="small" danger disabled={structureSubmitting}>
                                      删除
                                    </Button>
                                  </Popconfirm>
                                ),
                              },
                            ]}
                            dataSource={selectedObject.indexes}
                          />
                        ) : (
                          <Text type="secondary">当前对象暂无索引信息。</Text>
                        )}
                      </Card>
                    </Space>
                  </div>
                </Space>
              </Card>
            ),
          },
          {
            key: "sql",
            label: "SQL 控制台",
            children: (
              <Card>
                <Space direction="vertical" style={{ width: "100%" }} size="middle">
                  <Space.Compact style={{ width: "100%" }}>
                    <Select
                      style={{ width: 280 }}
                      placeholder="选择 MySQL / PostgreSQL 连接"
                      options={sqlConnectionOptions}
                      value={queryConnectionKey}
                      onChange={setQueryConnectionKey}
                    />
                    <Select
                      style={{ width: 260 }}
                      placeholder="选择数据库"
                      options={databaseNameOptions}
                      value={queryDatabaseName}
                      loading={databaseLoading}
                      disabled={!queryConnectionKey || databaseLoading}
                      onChange={setQueryDatabaseName}
                    />
                    <Button type="primary" loading={queryLoading} onClick={executeQuery}>
                      执行 SQL
                    </Button>
                  </Space.Compact>
                  <Alert
                    showIcon
                    type={
                      sqlExecutionStatus === "success"
                        ? "success"
                        : sqlExecutionStatus === "error"
                          ? "error"
                          : sqlExecutionStatus === "running" || sqlExecutionStatus === "loading_databases"
                            ? "info"
                            : "info"
                    }
                    message={sqlExecutionMessage}
                  />
                  <div style={{ border: "1px solid var(--border)", borderRadius: 6, overflow: "hidden" }}>
                    <Suspense fallback={<Input.TextArea rows={8} value={querySql} readOnly />}>
                      <SqlCodeEditor
                        value={querySql}
                        dialect={sqlDialect}
                        dark={theme === "dark"}
                        schema={sqlCompletionSchema}
                        onChange={setQuerySql}
                        onRun={executeQuery}
                      />
                    </Suspense>
                  </div>
                  <div
                    style={{
                      border: "1px solid var(--border)",
                      borderRadius: 6,
                      padding: 12,
                      background: "var(--bg-secondary)",
                    }}
                  >
                    <Space direction="vertical" style={{ width: "100%" }} size="middle">
                      <div className="flex items-center justify-between gap-3">
                        <Space size={8}>
                          <Bot size={16} />
                          <Text strong>SQL AI 助手</Text>
                          <Tag color="blue">真实模型调用</Tag>
                        </Space>
                        {sqlAiMeta && <Text type="secondary">{sqlAiMeta}</Text>}
                      </div>
                      <Space wrap>
                        <Select<SqlAiMode>
                          style={{ width: 150 }}
                          value={sqlAiMode}
                          onChange={setSqlAiMode}
                          options={[
                            { value: "ask", label: "自主提问" },
                            { value: "generate", label: "生成 SQL" },
                            { value: "fix", label: "SQL 纠错" },
                            { value: "optimize", label: "SQL 调优" },
                          ]}
                        />
                        <Button
                          type="primary"
                          icon={<Sparkles size={14} />}
                          loading={sqlAiLoading}
                          disabled={!selectedSqlConnection}
                          onClick={() => runSqlAi()}
                        >
                          {sqlAiMode === "ask"
                            ? "问 AI"
                            : sqlAiMode === "generate"
                              ? "生成 SQL"
                              : sqlAiMode === "fix"
                                ? "纠错 SQL"
                                : "调优 SQL"}
                        </Button>
                        <Button
                          icon={<Wand2 size={14} />}
                          disabled={!sqlAiAnswer || sqlAiAnswer === "AI 思考中..." || sqlAiLoading}
                          onClick={() => {
                            const sql = extractSqlFromAiAnswer(sqlAiAnswer);
                            if (!sql) {
                              message.warning("AI 输出中没有可应用的 SQL");
                              return;
                            }
                            setQuerySql(sql);
                            message.success("已应用到 SQL 编辑器");
                          }}
                        >
                          应用 SQL
                        </Button>
                        <Button disabled={!selectedSqlConnection || !querySql.trim()} loading={sqlAiLoading} onClick={() => runSqlAi("fix")}>
                          纠错当前 SQL
                        </Button>
                        <Button disabled={!selectedSqlConnection || !querySql.trim()} loading={sqlAiLoading} onClick={() => runSqlAi("optimize")}>
                          调优当前 SQL
                        </Button>
                        <Button
                          disabled={!sqlAiAnswer || sqlAiAnswer === "AI 思考中..." || sqlAiLoading}
                          loading={sqlExperienceSaving}
                          onClick={() => void saveSqlAiExperience()}
                        >
                          沉淀经验
                        </Button>
                      </Space>
                      <Input.TextArea
                        rows={3}
                        value={sqlAiPrompt}
                        onChange={(event) => setSqlAiPrompt(event.target.value)}
                        placeholder="描述你的需求或问题，例如：按状态统计订单数量并按数量倒序；解释这条 SQL 为什么慢；修复当前 SQL 报错。"
                      />
                      {sqlAiAnswer && (
                        <div
                          className="database-sql-ai-answer"
                          style={{
                            maxHeight: 300,
                            overflow: "auto",
                            border: "1px solid var(--border)",
                            borderRadius: 6,
                            padding: 12,
                            background: "var(--bg-primary)",
                          }}
                        >
                          <SqlMarkdownAnswer content={sqlAiAnswer} />
                        </div>
                      )}
                    </Space>
                  </div>
                  {queryResults.length > 0 && (
                    <Tabs
                      type="card"
                      size="small"
                      activeKey={activeQueryResultKey}
                      onChange={setActiveQueryResultKey}
                      items={queryResults.map((result, index) => ({
                        key: String(index),
                        label: (
                          <Space size={6}>
                            <span>结果 {index + 1}</span>
                            <Tag color={result.status === "error" ? "red" : "green"}>
                              {result.statementType.toUpperCase()}
                            </Tag>
                          </Space>
                        ),
                        children: renderQueryResult(result, index),
                      }))}
                    />
                  )}
                  {databaseSchema && (
                    <Text type="secondary">
                      已加载 {databaseSchema.tables.length} 张表的代码提示
                      {schemaLoading ? "，正在刷新..." : ""}
                    </Text>
                  )}
                </Space>
              </Card>
            ),
          },
          {
            key: "redis",
            label: "Redis 浏览",
            children: (
              <Card>
                <Space direction="vertical" style={{ width: "100%" }} size="middle">
                  <Space.Compact style={{ width: "100%" }}>
                    <Select
                      style={{ width: 280 }}
                      placeholder="选择 Redis 连接"
                      options={redisConnectionOptions}
                      value={redisConnectionKey}
                      onChange={setRedisConnectionKey}
                    />
                    <Select
                      style={{ width: 190 }}
                      placeholder="选择 DB"
                      options={redisDatabaseOptions}
                      value={redisDatabaseName}
                      loading={redisDatabaseLoading}
                      disabled={!redisConnectionKey || redisDatabaseLoading}
                      onChange={(value) => {
                        setRedisDatabaseName(value);
                        setRedisPage(1);
                        setRedisCursor(0);
                        setRedisPageCursors({ 1: 0 });
                        setRedisKeys([]);
                        setRedisPreview(null);
                      }}
                    />
                    <Button
                      icon={<RefreshCw size={14} />}
                      loading={redisTreeLoading}
                      disabled={!redisConnectionKey || !redisDatabaseName}
                      onClick={() => {
                        if (redisConnectionKey && redisDatabaseName) {
                          void loadRedisKeyTree(redisConnectionKey, redisDatabaseName);
                        }
                      }}
                    >
                      刷新 Key 树
                    </Button>
                    <Button
                      loading={redisLoading}
                      disabled={!redisConnectionKey || !redisDatabaseName}
                      onClick={() => scanRedis(1, redisPageSize, redisPattern)}
                    >
                      扫描
                    </Button>
                    <Button disabled={redisCursor === 0} onClick={() => scanRedis(redisPage + 1)}>
                      下一批
                    </Button>
                  </Space.Compact>
                  {redisDatabaseName && (
                    <Text type="secondary">
                      当前 DB {redisDatabaseName}：
                      {currentRedisDatabase?.keyCount ?? 0}
                      {" "}个 Key，当前层级 {redisSelectedKeyCount} 个 Key
                    </Text>
                  )}
                  <div
                    style={{
                      display: "grid",
                      gap: 16,
                      gridTemplateColumns: "280px minmax(0, 1fr) minmax(360px, 1fr)",
                    }}
                  >
                    <Card
                      size="small"
                      title="Key 树"
                      loading={redisTreeLoading}
                      styles={{ body: { overflowX: "auto", overflowY: "hidden", paddingBottom: 8 } }}
                    >
                      <div style={{ minWidth: 640, width: "max-content" }}>
                        <Tree<RedisTreeNode>
                          blockNode
                          showLine
                          height={520}
                          style={{ minWidth: 640, width: "max-content" }}
                          treeData={redisTreeData}
                          selectedKeys={[redisSelectedTreeKey]}
                          defaultExpandAll={redisTreeKeys.length <= 200}
                          onSelect={(_, info) => {
                            const node = info.node as RedisTreeNode;
                            const pattern = node.pattern || "*";
                            const leafKeys = node.leafKeys ?? [];
                            setRedisSelectedTreeKey(node.key);
                            setRedisSelectedLeafKeys(leafKeys);
                            setRedisSelectedKeyCount(leafKeys.length);
                            setRedisPattern(pattern);
                            setRedisPage(1);
                            setRedisCursor(0);
                            setRedisPageCursors({ 1: 0 });
                            setRedisKeys([]);
                            setRedisPreview(null);
                            void loadRedisTreePage(1, redisPageSize, leafKeys);
                          }}
                        />
                      </div>
                    </Card>
                    <Card size="small" title="Key 列表">
                      <Space direction="vertical" style={{ width: "100%" }}>
                        <Table<RedisKeyEntry>
                          rowKey="key"
                          size="small"
                          loading={redisLoading}
                          columns={[
                            { title: "Key", dataIndex: "key", ellipsis: true },
                            { title: "类型", dataIndex: "keyType", width: 90 },
                            { title: "TTL", dataIndex: "ttl", width: 90 },
                            {
                              title: "操作",
                              width: 90,
                              render: (_, record) => (
                                <Button size="small" onClick={() => previewRedisValue(record.key)}>
                                  预览
                                </Button>
                              ),
                            },
                          ]}
                          dataSource={redisKeys}
                          pagination={false}
                        />
                        <Pagination
                          align="end"
                          current={redisPage}
                          pageSize={redisPageSize}
                          total={redisSelectedKeyCount || redisKeys.length}
                          showSizeChanger
                          pageSizeOptions={[12, 20, 50, 100]}
                          showTotal={(total) => `共 ${total} 个 Key`}
                          onChange={(page, size) => {
                            if (size !== redisPageSize) {
                              setRedisPageSize(size);
                              setRedisPage(1);
                              setRedisCursor(0);
                              setRedisPageCursors({ 1: 0 });
                              if (redisSelectedLeafKeys.length > 0) {
                                void loadRedisTreePage(1, size, redisSelectedLeafKeys);
                              } else {
                                void scanRedis(1, size, redisPattern);
                              }
                              return;
                            }
                            if (redisSelectedLeafKeys.length > 0) {
                              void loadRedisTreePage(page, size, redisSelectedLeafKeys);
                            } else {
                              void scanRedis(page, size, redisPattern);
                            }
                          }}
                          onShowSizeChange={(_, size) => {
                            setRedisPageSize(size);
                            setRedisPage(1);
                            setRedisCursor(0);
                            setRedisPageCursors({ 1: 0 });
                            if (redisSelectedLeafKeys.length > 0) {
                              void loadRedisTreePage(1, size, redisSelectedLeafKeys);
                            } else {
                              void scanRedis(1, size, redisPattern);
                            }
                          }}
                        />
                      </Space>
                    </Card>
                    <Card size="small" title="Value 预览">
                      {redisPreview ? (
                        <pre style={{ whiteSpace: "pre-wrap", wordBreak: "break-word", margin: 0 }}>
                          {JSON.stringify(redisPreview, null, 2)}
                        </pre>
                      ) : (
                        <Text type="secondary">选择一个 Key 后查看只读预览。</Text>
                      )}
                    </Card>
                  </div>
                </Space>
              </Card>
            ),
          },
          {
            key: "exports",
            label: "备份与导出",
            children: (
              <Card>
                <Space direction="vertical" style={{ width: "100%" }} size="middle">
                  <Alert
                    showIcon
                    type="info"
                    message="导出文件会写入系统设置中的数据库导出目录，默认是当前系统下载目录。"
                  />
                  <Space.Compact style={{ width: "100%" }}>
                    <Select
                      style={{ width: 300 }}
                      placeholder="选择 MySQL / PostgreSQL 连接"
                      options={sqlConnectionOptions}
                      value={exportConnectionKey}
                      loading={loading}
                      onChange={setExportConnectionKey}
                    />
                    <Select
                      style={{ width: 240 }}
                      placeholder="选择数据库"
                      options={exportDatabaseNameOptions}
                      value={exportDatabaseName}
                      loading={exportLoading}
                      disabled={!exportConnectionKey || exportLoading}
                      onChange={setExportDatabaseName}
                    />
                    <Button
                      icon={<RefreshCw size={14} />}
                      loading={exportLoading}
                      disabled={!exportConnectionKey || !exportDatabaseName}
                      onClick={() => {
                        if (exportConnectionKey && exportDatabaseName) {
                          void loadExportSchema(exportConnectionKey, exportDatabaseName);
                        }
                      }}
                    >
                      刷新表
                    </Button>
                  </Space.Compact>
                  <Card size="small" title="导出任务">
                    <Space direction="vertical" style={{ width: "100%" }} size="middle">
                      <Space wrap>
                        <Select<DatabaseExportMode>
                          style={{ width: 180 }}
                          value={exportMode}
                          options={[
                            { value: "table_csv", label: "表数据 CSV" },
                            { value: "query_csv", label: "SQL 查询 CSV" },
                            { value: "sql_backup", label: "SQL 备份" },
                          ]}
                          onChange={setExportMode}
                        />
                        {exportMode !== "query_csv" && (
                          <Select
                            allowClear={exportMode === "sql_backup"}
                            style={{ width: 320 }}
                            placeholder={exportMode === "sql_backup" ? "选择表；留空表示备份整个库" : "选择数据表"}
                            options={exportTableOptions}
                            value={exportTableName}
                            loading={exportLoading}
                            disabled={!exportSchema}
                            onChange={setExportTableName}
                          />
                        )}
                        {exportMode !== "sql_backup" && (
                          <InputNumber
                            min={1}
                            max={1000000}
                            value={exportMaxRows}
                            addonBefore="最多行数"
                            onChange={(value) => setExportMaxRows(value ?? 100000)}
                          />
                        )}
                        {exportMode === "sql_backup" && (
                          <Switch
                            checked={exportIncludeData}
                            checkedChildren="含数据"
                            unCheckedChildren="仅结构"
                            onChange={setExportIncludeData}
                          />
                        )}
                        <Button
                          type="primary"
                          loading={exportLoading}
                          disabled={!exportConnectionKey || !exportDatabaseName}
                          onClick={runDatabaseExport}
                        >
                          开始导出
                        </Button>
                      </Space>
                      {exportMode === "query_csv" && (
                        <Input.TextArea
                          value={exportSql}
                          rows={6}
                          placeholder="输入 SELECT / SHOW / DESCRIBE 查询，结果将导出为 CSV"
                          onChange={(event) => setExportSql(event.target.value)}
                        />
                      )}
                    </Space>
                  </Card>
                  {exportResult && (
                    <Card size="small" title="最近一次导出结果">
                      <Space direction="vertical" style={{ width: "100%" }}>
                        <Text strong>{exportResult.message}</Text>
                        <Text>文件名：{exportResult.fileName}</Text>
                        <Paragraph copyable={{ text: exportResult.filePath }} style={{ marginBottom: 0 }}>
                          文件路径：{exportResult.filePath}
                        </Paragraph>
                        <Text type="secondary">
                          模式：{exportResult.mode}，表数：{exportResult.tableCount}，行数：{exportResult.rowCount}
                        </Text>
                      </Space>
                    </Card>
                  )}
                </Space>
              </Card>
            ),
          },
        ]}
      />

      <Drawer
        title="新增数据表"
        open={createTableDrawerOpen}
        size="large"
        onClose={() => setCreateTableDrawerOpen(false)}
        extra={
          <Space>
            <Button onClick={() => setCreateTableDrawerOpen(false)}>取消</Button>
            <Button type="primary" loading={structureSubmitting} onClick={createTable}>
              创建数据表
            </Button>
          </Space>
        }
      >
        <Space direction="vertical" style={{ width: "100%" }} size="middle">
          <Alert
            showIcon
            type="warning"
            message="创建数据表会直接执行 DDL，请确认当前连接和数据库选择正确。"
          />
          <Form form={createTableForm} layout="vertical">
            <Form.Item name="tableName" label="表名" rules={[{ required: true, message: "请输入表名" }]}>
              <Input placeholder="new_table" />
            </Form.Item>
            <Form.List name="columns">
              {(fields, { add, remove }) => (
                <Space direction="vertical" style={{ width: "100%" }} size="middle">
                  {fields.map((field, index) => (
                    <Card
                      key={field.key}
                      size="small"
                      title={`字段 ${index + 1}`}
                      extra={
                        fields.length > 1 ? (
                          <Button
                            size="small"
                            danger
                            icon={<Trash2 size={14} />}
                            onClick={() => remove(field.name)}
                          >
                            删除字段
                          </Button>
                        ) : null
                      }
                    >
                      <Space direction="vertical" style={{ width: "100%" }}>
                        <Space.Compact style={{ width: "100%" }}>
                          <Form.Item
                            name={[field.name, "name"]}
                            rules={[{ required: true, message: "请输入字段名" }]}
                            style={{ width: "30%", marginBottom: 12 }}
                          >
                            <Input placeholder="字段名" />
                          </Form.Item>
                          <Form.Item
                            name={[field.name, "dataType"]}
                            rules={[{ required: true, message: "请输入字段类型" }]}
                            style={{ width: "32%", marginBottom: 12 }}
                          >
                            <AutoComplete
                              options={columnTypeOptions}
                              placeholder="字段类型"
                              filterOption={showAllColumnTypes}
                            />
                          </Form.Item>
                          <Form.Item name={[field.name, "defaultValue"]} style={{ width: "38%", marginBottom: 12 }}>
                            <Input placeholder="默认值，留空表示无默认值" />
                          </Form.Item>
                        </Space.Compact>
                        <Space size="large">
                          <Form.Item
                            name={[field.name, "nullable"]}
                            valuePropName="checked"
                            initialValue
                            style={{ marginBottom: 0 }}
                          >
                            <Switch checkedChildren="允许 NULL" unCheckedChildren="禁止 NULL" />
                          </Form.Item>
                          <Form.Item
                            name={[field.name, "primaryKey"]}
                            valuePropName="checked"
                            initialValue={false}
                            style={{ marginBottom: 0 }}
                          >
                            <Switch checkedChildren="主键" unCheckedChildren="非主键" />
                          </Form.Item>
                        </Space>
                      </Space>
                    </Card>
                  ))}
                  <Button
                    block
                    icon={<Plus size={14} />}
                    onClick={() => add({ name: "", dataType: "varchar(255)", nullable: true, primaryKey: false })}
                  >
                    添加字段
                  </Button>
                </Space>
              )}
            </Form.List>
          </Form>
        </Space>
      </Drawer>

      <Drawer
        title={
          selectedObject
            ? `编辑表结构：${selectedObject.schemaName ? `${selectedObject.schemaName}.` : ""}${selectedObject.name}`
            : "编辑表结构"
        }
        open={structureDrawerOpen}
        size="large"
        onClose={() => setStructureDrawerOpen(false)}
      >
        <Space direction="vertical" style={{ width: "100%" }} size="middle">
          <Alert
            showIcon
            type="warning"
            message="结构变更会直接执行 DDL，请确认已备份并了解对业务的影响。"
          />
          <Tabs
            items={[
              {
                key: "add-column",
                label: "新增字段",
                children: (
                  <Form form={addColumnForm} layout="vertical">
                    <Form.Item name="name" label="字段名" rules={[{ required: true, message: "请输入字段名" }]}>
                      <Input placeholder="new_column" />
                    </Form.Item>
                    <Form.Item name="dataType" label="字段类型" rules={[{ required: true, message: "请输入字段类型" }]}>
                      <AutoComplete
                        options={columnTypeOptions}
                        placeholder="选择或输入字段类型"
                        filterOption={showAllColumnTypes}
                      />
                    </Form.Item>
                    <Form.Item name="nullable" label="允许 NULL" valuePropName="checked" initialValue>
                      <Switch checkedChildren="允许" unCheckedChildren="禁止" />
                    </Form.Item>
                    <Form.Item name="defaultValue" label="默认值">
                      <Input placeholder="留空表示无默认值；字符串会自动加引号" />
                    </Form.Item>
                    <Button type="primary" loading={structureSubmitting} onClick={addColumn}>
                      执行新增字段
                    </Button>
                  </Form>
                ),
              },
              {
                key: "modify-column",
                label: "修改字段",
                children: (
                  <Form form={modifyColumnForm} layout="vertical">
                    <Form.Item name="oldName" label="原字段" rules={[{ required: true, message: "请选择字段" }]}>
                      <Select
                        options={selectedObjectColumnOptions}
                        placeholder="选择字段"
                        onChange={(value) => {
                          const column = selectedObject?.columnDetails.find((item) => item.name === value);
                          if (column) {
                            modifyColumnForm.setFieldsValue({
                              newName: column.name,
                              dataType: column.columnType || column.dataType,
                              nullable: column.nullable,
                              defaultValue: column.defaultValue ?? undefined,
                            });
                          }
                        }}
                      />
                    </Form.Item>
                    <Form.Item name="newName" label="新字段名">
                      <Input placeholder="留空或不变表示不改名" />
                    </Form.Item>
                    <Form.Item name="dataType" label="字段类型" rules={[{ required: true, message: "请输入字段类型" }]}>
                      <AutoComplete
                        options={columnTypeOptions}
                        placeholder="选择或输入字段类型"
                        filterOption={showAllColumnTypes}
                      />
                    </Form.Item>
                    <Form.Item name="nullable" label="允许 NULL" valuePropName="checked">
                      <Switch checkedChildren="允许" unCheckedChildren="禁止" />
                    </Form.Item>
                    <Form.Item name="defaultValue" label="默认值">
                      <Input placeholder="留空表示移除默认值；字符串会自动加引号" />
                    </Form.Item>
                    <Button type="primary" loading={structureSubmitting} onClick={modifyColumn}>
                      执行修改字段
                    </Button>
                  </Form>
                ),
              },
              {
                key: "add-index",
                label: "新增索引",
                children: (
                  <Form form={addIndexForm} layout="vertical">
                    <Form.Item name="name" label="索引名" rules={[{ required: true, message: "请输入索引名" }]}>
                      <Input placeholder="idx_column_name" />
                    </Form.Item>
                    <Form.Item name="columns" label="索引字段" rules={[{ required: true, message: "请选择字段" }]}>
                      <Select mode="multiple" options={selectedObjectColumnOptions} placeholder="选择一个或多个字段" />
                    </Form.Item>
                    <Form.Item name="unique" label="唯一索引" valuePropName="checked" initialValue={false}>
                      <Switch checkedChildren="唯一" unCheckedChildren="普通" />
                    </Form.Item>
                    <Button type="primary" loading={structureSubmitting} onClick={addIndex}>
                      执行新增索引
                    </Button>
                  </Form>
                ),
              },
            ]}
          />
        </Space>
      </Drawer>

      <Drawer
        title={editing ? "编辑数据库连接" : "新建数据库连接"}
        open={drawerOpen}
        size="large"
        onClose={() => setDrawerOpen(false)}
        extra={
          <Space>
            <Button onClick={() => setDrawerOpen(false)}>取消</Button>
            <Button type="primary" onClick={submitForm}>保存</Button>
          </Space>
        }
      >
        <Form form={form} layout="vertical" initialValues={defaultFormValues}>
          <Form.Item name="key" label="连接 Key" rules={[{ required: true, message: "请输入连接 Key" }]}>
            <Input disabled={Boolean(editing)} placeholder="prod-mysql" />
          </Form.Item>
          <Form.Item name="name" label="连接名称" rules={[{ required: true, message: "请输入连接名称" }]}>
            <Input placeholder="生产 MySQL" />
          </Form.Item>
          <Form.Item name="groupName" label="分组" rules={[{ required: true, message: "请输入分组" }]}>
            <Input placeholder="生产环境" />
          </Form.Item>
          <Form.Item name="dbType" label="数据库类型" rules={[{ required: true }]}>
            <Select
              options={dbTypeOptions}
              onChange={(value: DatabaseType) => {
                const option = dbTypeOptions.find((item) => item.value === value);
                form.setFieldValue("port", option?.defaultPort ?? 3306);
              }}
            />
          </Form.Item>
          <Form.Item name="connectionMode" label="连接方式" rules={[{ required: true }]}>
            <Select
              options={[
                { value: "direct", label: "直连" },
                { value: "ssh_tunnel", label: "SSH 隧道" },
              ]}
            />
          </Form.Item>
          {connectionMode === "ssh_tunnel" && (
            <Form.Item name="sshServerAlias" label="跳板服务器" rules={[{ required: true, message: "请选择跳板服务器" }]}>
              <Select options={serverOptions} placeholder="选择已配置服务器" />
            </Form.Item>
          )}
          <Space.Compact style={{ width: "100%" }}>
            <Form.Item name="host" label="主机地址" style={{ width: "70%" }} rules={[{ required: true, message: "请输入主机地址" }]}>
              <Input placeholder="127.0.0.1" />
            </Form.Item>
            <Form.Item name="port" label="端口" style={{ width: "30%" }} rules={[{ required: true, message: "请输入端口" }]}>
              <InputNumber min={1} max={65535} style={{ width: "100%" }} />
            </Form.Item>
          </Space.Compact>
          <Form.Item name="databaseName" label="数据库 / DB">
            <Input placeholder="库名；Redis 可填写 0" />
          </Form.Item>
          <Form.Item name="username" label="用户名">
            <Input placeholder="root / postgres / default" />
          </Form.Item>
          <Form.Item name="authType" label="认证方式" rules={[{ required: true }]}>
            <Select
              options={[
                { value: "direct_password", label: "直接密码" },
                { value: "credential_ref", label: "凭据引用" },
              ]}
            />
          </Form.Item>
          {authType === "credential_ref" ? (
            <Form.Item name="credentialRef" label="凭据引用" rules={[{ required: true, message: "请选择凭据" }]}>
              <Select options={credentialOptions} placeholder="选择凭据保险库条目" />
            </Form.Item>
          ) : (
            <>
              <Form.Item name="password" label={editing ? "新密码（留空则不修改）" : "密码"}>
                <Input.Password placeholder={editing ? "留空保留原密码" : "输入数据库密码"} />
              </Form.Item>
              {editing?.hasPassword && (
                <Form.Item name="clearPassword" valuePropName="checked">
                  <Switch checkedChildren="清除密码" unCheckedChildren="保留密码" />
                </Form.Item>
              )}
            </>
          )}
          <Form.Item name="securityMode" label="数据库安全级别" rules={[{ required: true }]}>
            <Select
              options={[
                { value: "approval_all", label: "全部审批" },
                { value: "confirm_execute", label: "二次确认执行" },
              ]}
            />
          </Form.Item>
          <Form.Item name="aiPolicy" label="复用服务器 AI 权限级别" rules={[{ required: true }]}>
            <Select options={aiPolicyOptions} />
          </Form.Item>
          <Form.Item name="pageSize" label="单页行数" rules={[{ required: true }]}>
            <InputNumber min={1} max={500} style={{ width: "100%" }} />
          </Form.Item>
          <Form.Item name="enabled" label="启用" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="notes" label="备注">
            <Input.TextArea rows={3} />
          </Form.Item>
        </Form>
      </Drawer>
    </div>
  );
}
