import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Alert,
  Button,
  Card,
  Collapse,
  Descriptions,
  Empty,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Select,
  Space,
  Spin,
  Steps,
  Switch,
  Table,
  Tabs,
  Tag,
  Tree,
  Typography,
  message,
} from "antd";
import type { TableColumnsType, TreeDataNode } from "antd";
import {
  BookOpenCheck,
  Eye,
  FileText,
  FolderSync,
  GitBranch,
  Layers3,
  Plus,
  RefreshCw,
  Send,
  Settings2,
  Sparkles,
} from "lucide-react";
import { MarkdownPreview } from "@/components/ui/MarkdownPreview";
import {
  aiProviderApi,
  getErrorMessage,
  gitWorkspaceApi,
  knowledgeApi,
} from "@/lib/api";
import { useKnowledgeStore } from "@/store";
import type {
  KnowledgeAskResult,
  KnowledgeCitation,
  KnowledgeCodeCallGraph,
  KnowledgeCodeAnalysisResult,
  KnowledgeCodeFile,
  KnowledgeCodeFileContent,
  KnowledgeCodeSnapshot,
  KnowledgeCodeSnapshotComparison,
  KnowledgeCodeSource,
  KnowledgeCodeSymbol,
  KnowledgeDocument,
  KnowledgeDocumentDetail,
  KnowledgeDocumentVersion,
  KnowledgeJob,
  KnowledgePage,
  KnowledgeProject,
  KnowledgeRagContextPreview,
  KnowledgeRelease,
  KnowledgeSearchInput,
  KnowledgeSource,
  KnowledgeSourceScopePreview,
  KnowledgeEmbeddingProfile,
  KnowledgeEmbeddingBatchResult,
  KnowledgeEmbeddingIndexValidation,
  KnowledgeEmbeddingRebuildEstimate,
  KnowledgeLocalEmbeddingRuntimeStatus,
  AiProvider,
  GitWorkspace,
  ZentaoCapabilityProbeResult,
  ZentaoConnection,
  ZentaoProjectMapping,
  ZentaoRemoteScopeItem,
  UpsertKnowledgeCodeSourceInput,
  UpsertKnowledgeEmbeddingProfileInput,
  UpsertKnowledgeProjectInput,
  UpsertKnowledgeReleaseInput,
  UpsertKnowledgeSourceInput,
  UpsertZentaoConnectionInput,
  UpsertZentaoProjectMappingInput,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

const KnowledgeCodePreview = lazy(async () => {
  const [
    codeMirrorModule,
    githubThemeModule,
    cppModule,
    cssModule,
    goModule,
    htmlModule,
    javaModule,
    javascriptModule,
    jsonModule,
    phpModule,
    pythonModule,
    rustModule,
    sqlModule,
    xmlModule,
    yamlModule,
  ] = await Promise.all([
    import("@uiw/react-codemirror"),
    import("@uiw/codemirror-theme-github"),
    import("@codemirror/lang-cpp"),
    import("@codemirror/lang-css"),
    import("@codemirror/lang-go"),
    import("@codemirror/lang-html"),
    import("@codemirror/lang-java"),
    import("@codemirror/lang-javascript"),
    import("@codemirror/lang-json"),
    import("@codemirror/lang-php"),
    import("@codemirror/lang-python"),
    import("@codemirror/lang-rust"),
    import("@codemirror/lang-sql"),
    import("@codemirror/lang-xml"),
    import("@codemirror/lang-yaml"),
  ]);
  const CodeMirror = codeMirrorModule.default;
  const languageExtensions = (language: string) => {
    switch (language) {
      case "java":
        return [javaModule.java()];
      case "typescript":
        return [javascriptModule.javascript({ jsx: true, typescript: true })];
      case "javascript":
        return [javascriptModule.javascript({ jsx: true })];
      case "json":
        return [jsonModule.json()];
      case "yaml":
        return [yamlModule.yaml()];
      case "html":
        return [htmlModule.html()];
      case "css":
        return [cssModule.css()];
      case "xml":
        return [xmlModule.xml()];
      case "sql":
        return [sqlModule.sql()];
      case "python":
        return [pythonModule.python()];
      case "rust":
        return [rustModule.rust()];
      case "go":
        return [goModule.go()];
      case "php":
        return [phpModule.php()];
      case "cpp":
        return [cppModule.cpp()];
      default:
        return [];
    }
  };

  return {
    default: function LazyKnowledgeCodePreview(props: {
      content: string;
      language: string;
    }) {
      return (
        <CodeMirror
          value={props.content}
          height="420px"
          theme={githubThemeModule.githubLight}
          extensions={languageExtensions(props.language)}
          editable={false}
          basicSetup={{
            lineNumbers: true,
            foldGutter: true,
            highlightActiveLineGutter: true,
            highlightSpecialChars: true,
            bracketMatching: true,
            highlightActiveLine: false,
          }}
        />
      );
    },
  };
});

const MARKDOWN_EXTENSIONS = new Set(["md", "mdx", "markdown", "mdown", "mkdn"]);

export function isMarkdownPath(path: string) {
  const extension = path.split(".").pop()?.toLowerCase();
  return extension != null && MARKDOWN_EXTENSIONS.has(extension);
}

export function isMarkdownDocument(
  document: Pick<KnowledgeDocument, "docType" | "logicalPath" | "title">,
  version: Pick<KnowledgeDocumentVersion, "sourcePath" | "mimeType">,
) {
  const path = `${version.sourcePath || document.logicalPath || document.title}`;
  return (
    document.docType.toLowerCase() === "markdown" ||
    version.mimeType.toLowerCase().includes("markdown") ||
    isMarkdownPath(path)
  );
}

export function documentCodeLanguage(path: string) {
  const extension = path.split(".").pop()?.toLowerCase();
  const languageByExtension: Record<string, string> = {
    c: "cpp",
    cc: "cpp",
    cpp: "cpp",
    cs: "cpp",
    css: "css",
    go: "go",
    h: "cpp",
    hpp: "cpp",
    htm: "html",
    html: "html",
    java: "java",
    js: "javascript",
    jsx: "javascript",
    json: "json",
    php: "php",
    py: "python",
    rs: "rust",
    sql: "sql",
    ts: "typescript",
    tsx: "typescript",
    vue: "html",
    xml: "xml",
    yml: "yaml",
    yaml: "yaml",
  };
  return extension == null
    ? "plain"
    : (languageByExtension[extension] ?? "plain");
}

const CODE_LANGUAGE_LABELS: Record<string, string> = {
  cpp: "C/C++",
  css: "CSS",
  go: "Go",
  html: "HTML",
  java: "Java",
  javascript: "JavaScript",
  json: "JSON",
  php: "PHP",
  plain: "纯文本",
  python: "Python",
  rust: "Rust",
  sql: "SQL",
  typescript: "TypeScript",
  xml: "XML",
  yaml: "YAML",
};

export function DocumentContentPreview(props: {
  document: Pick<KnowledgeDocument, "docType" | "logicalPath" | "title">;
  version: Pick<
    KnowledgeDocumentVersion,
    "sourcePath" | "mimeType" | "content"
  >;
}) {
  if (isMarkdownDocument(props.document, props.version)) {
    return (
      <MarkdownPreview
        content={props.version.content}
        testId="knowledge-markdown-preview"
      />
    );
  }

  const sourcePath =
    props.version.sourcePath ||
    props.document.logicalPath ||
    props.document.title;
  const language = documentCodeLanguage(sourcePath);
  return (
    <div data-testid="knowledge-code-preview" data-language={language}>
      <div className="mb-2 flex justify-end">
        <Tag>
          {CODE_LANGUAGE_LABELS[language] ?? CODE_LANGUAGE_LABELS.plain}
        </Tag>
      </div>
      <Suspense fallback={<Spin tip="正在加载代码高亮" />}>
        <KnowledgeCodePreview
          content={props.version.content}
          language={language}
        />
      </Suspense>
    </div>
  );
}

/**
 * 源码快照的文件记录会保留解析级别，但展示语言必须以实际路径为准，
 * 否则解析器降级为 text_only 时会错误地失去 Java、SQL 等的语法高亮。
 */
export function KnowledgeCodeFilePreview(props: KnowledgeCodeFileContent) {
  const { file, content } = props;
  if (isMarkdownPath(file.relativePath)) {
    return (
      <MarkdownPreview
        content={content}
        testId="knowledge-code-file-markdown-preview"
      />
    );
  }

  const language = documentCodeLanguage(file.relativePath);
  return (
    <div data-testid="knowledge-code-file-preview" data-language={language}>
      <div className="mb-2 flex justify-end">
        <Tag>
          {CODE_LANGUAGE_LABELS[language] ?? CODE_LANGUAGE_LABELS.plain}
        </Tag>
      </div>
      <Suspense fallback={<Spin tip="正在加载代码高亮" />}>
        <KnowledgeCodePreview content={content} language={language} />
      </Suspense>
    </div>
  );
}

const INITIAL_SEARCH: KnowledgeSearchInput = {
  query: "",
  projectIds: [],
  releaseIds: [],
  sourceIds: [],
  documentTypes: [],
  sensitivities: [],
  limit: 12,
  includeContext: true,
};

const KNOWLEDGE_SOURCE_TYPE_OPTIONS = [
  { value: "git_workspace", label: "Git 工作区" },
  { value: "local_directory", label: "本地目录" },
  { value: "single_file", label: "单个文件" },
  { value: "manual_markdown", label: "手工 Markdown" },
  { value: "experience", label: "已有 AI 经验" },
  { value: "zentao", label: "禅道事实" },
  { value: "code_directory", label: "本地源码目录" },
];

// 与后端分析器使用的稳定标识保持一致；这里只转换展示文案，避免配置值随文案变化。
const CODE_SOURCE_LANGUAGE_OPTIONS = [
  { value: "rust", label: "Rust" },
  { value: "typescript", label: "TypeScript" },
  { value: "javascript", label: "JavaScript" },
  { value: "vue", label: "Vue" },
  { value: "java", label: "Java" },
  { value: "sql", label: "SQL" },
  { value: "markdown", label: "Markdown（含 md/mdx/markdown/mdown/mkdn）" },
];

const KNOWLEDGE_VERSION_STRATEGY_OPTIONS = [
  { value: "unversioned", label: "未版本化" },
  { value: "git_ref", label: "按 Git 引用" },
  { value: "release_mapping", label: "按发布版本映射" },
];

const KNOWLEDGE_SYNC_MODE_OPTIONS = [
  { value: "incremental", label: "增量同步" },
  { value: "manual", label: "手动同步" },
];

const KNOWLEDGE_SYNC_STATUS_OPTIONS = [
  { value: "never", label: "未同步", color: "default" },
  { value: "running", label: "同步中", color: "processing" },
  { value: "success", label: "同步成功", color: "green" },
  { value: "failed", label: "同步失败", color: "red" },
  { value: "cancelled", label: "已取消", color: "default" },
  { value: "interrupted", label: "已中断", color: "orange" },
] as const;

const KNOWLEDGE_JOB_STATUS_OPTIONS = [
  { value: "queued", label: "等待执行", color: "default" },
  { value: "running", label: "执行中", color: "processing" },
  { value: "completed", label: "已完成", color: "green" },
  { value: "failed", label: "执行失败", color: "red" },
  { value: "cancelled", label: "已取消", color: "default" },
  { value: "interrupted", label: "已中断", color: "orange" },
] as const;

const KNOWLEDGE_JOB_REFRESH_INTERVAL_MS = 1_000;
const KNOWLEDGE_DOCUMENT_PAGE_SIZE = 500;

const LOCAL_EMBEDDING_MODEL_OPTIONS = [
  "multilingual-e5-small-int8",
  "bge-small-zh-v1.5",
];

function uniqueEmbeddingModelNames(...modelGroups: string[][]) {
  return Array.from(
    new Set(
      modelGroups
        .flat()
        .map((model) => model.trim())
        .filter((model) => model.length > 0),
    ),
  );
}

function providerEmbeddingModels(provider: AiProvider | undefined) {
  // Provider 的 models 是通用模型列表，可能含聊天模型。当前契约没有模型级能力标记，
  // 只能使用管理员显式配置的 embeddingModel，避免将聊天模型保存为向量化方案。
  return uniqueEmbeddingModelNames(
    provider?.embeddingModel ? [provider.embeddingModel] : [],
  );
}

export function normalizedEmbeddingProfileProviderKey(
  mode: "local" | "remote",
  providerKey: string,
) {
  return mode === "remote" ? providerKey.trim() : "";
}

export function hasAvailableRemoteEmbeddingProvider(
  providerKey: string,
  providers: AiProvider[],
) {
  return providers.some(
    (provider) =>
      provider.key === providerKey &&
      provider.enabled &&
      provider.status === "configured" &&
      providerEmbeddingModels(provider).length > 0,
  );
}

/**
 * 与后端 AiProviderService::supports_chat 保持相同的能力判断。
 * 显式声明 chat / embedding 的新服务商必须包含 chat；历史服务商仍允许通过默认模型
 * 判定为对话服务，避免“流式/推理”等旧能力标签使原本可用的服务商在页面中消失。
 */
export function isAvailableKnowledgeChatProvider(provider: AiProvider) {
  const hasExplicitCapability = provider.capabilities.some((capability) => {
    const normalizedCapability = capability.toLowerCase();
    return (
      normalizedCapability === "chat" || normalizedCapability === "embedding"
    );
  });
  const supportsChat = hasExplicitCapability
    ? provider.capabilities.some(
        (capability) => capability.toLowerCase() === "chat",
      )
    : provider.defaultModel.trim().length > 0;

  return provider.enabled && provider.status === "configured" && supportsChat;
}

const KNOWLEDGE_DOCUMENT_STATUS_OPTIONS = [
  { value: "active", label: "已生效", color: "green" },
  { value: "inactive", label: "已停用", color: "default" },
  { value: "archived", label: "已归档", color: "gold" },
  { value: "deleted", label: "已删除", color: "red" },
  { value: "processing", label: "处理中", color: "processing" },
  { value: "failed", label: "处理失败", color: "red" },
] as const;

function knowledgeSourceTypeLabel(value: string) {
  return (
    KNOWLEDGE_SOURCE_TYPE_OPTIONS.find((option) => option.value === value)
      ?.label ?? value
  );
}

function knowledgeSyncStatus(value: string | null | undefined) {
  return (
    KNOWLEDGE_SYNC_STATUS_OPTIONS.find((option) => option.value === value) ??
    KNOWLEDGE_SYNC_STATUS_OPTIONS[0]
  );
}

function knowledgeJobStatus(value: string) {
  return (
    KNOWLEDGE_JOB_STATUS_OPTIONS.find((option) => option.value === value) ?? {
      label: "未知状态",
      color: "default" as const,
    }
  );
}

function knowledgeJobProgressLabel(job: KnowledgeJob) {
  if (job.progressTotal > 0) {
    return `${job.progressCurrent} / ${job.progressTotal}`;
  }
  // 增量同步命中相同 Commit 时没有待处理文件，0 是有效结果而非未知总数。
  if (job.status === "completed" && job.jobType === "source_sync") {
    return "无变更";
  }
  return job.status === "completed" ? "无待处理项" : "待统计";
}

function knowledgeDocumentStatus(value: string) {
  return (
    KNOWLEDGE_DOCUMENT_STATUS_OPTIONS.find(
      (option) => option.value === value,
    ) ?? {
      label: "未知状态",
      color: "default" as const,
    }
  );
}

export function knowledgeCodeSnapshotStatus(value: string) {
  const statuses: Record<string, { label: string; color: string }> = {
    captured: { label: "已捕获", color: "blue" },
    analyzing: { label: "分析中", color: "processing" },
    analyzed: { label: "已分析", color: "green" },
    failed: { label: "分析失败", color: "red" },
  };
  return statuses[value] ?? { label: "未知状态", color: "default" };
}

/**
 * 快照 ID 只在本地唯一；同一知识项目接入多个仓库时，必须把代码来源放进展示标签，
 * 避免用户把相同 HEAD 或相同提交前缀误认为同一份代码证据。
 */
export function codeSnapshotSourceLabel(
  snapshot: Pick<KnowledgeCodeSnapshot, "sourceId">,
  codeSources: ReadonlyArray<{
    source: Pick<KnowledgeSource, "id" | "displayName" | "sourceKey">;
  }>,
) {
  const source = codeSources.find(
    (candidate) => candidate.source.id === snapshot.sourceId,
  )?.source;
  if (!source) return `未知代码来源（来源 #${snapshot.sourceId}）`;

  const displayName = source.displayName.trim();
  const sourceKey = source.sourceKey.trim();
  if (!displayName)
    return sourceKey || `未知代码来源（来源 #${snapshot.sourceId}）`;
  return sourceKey && sourceKey !== displayName
    ? `${displayName}（${sourceKey}）`
    : displayName;
}

export function codeSnapshotOptionLabel(
  snapshot: Pick<
    KnowledgeCodeSnapshot,
    "sourceId" | "refName" | "snapshotKey" | "commitSha" | "capturedAt"
  >,
  codeSources: ReadonlyArray<{
    source: Pick<KnowledgeSource, "id" | "displayName" | "sourceKey">;
  }>,
) {
  const reference = snapshot.refName.trim() || snapshot.snapshotKey;
  const revision = snapshot.commitSha
    ? snapshot.commitSha.slice(0, 12)
    : snapshot.capturedAt;
  return `${codeSnapshotSourceLabel(snapshot, codeSources)} · ${reference} · ${revision}`;
}

function knowledgeCodeAnalysisLevel(value: string) {
  const labels: Record<string, string> = {
    ast: "语法树解析",
    structured_fallback: "结构化解析",
    text_only: "文本读取",
    skipped: "已跳过",
  };
  return labels[value] ?? "未知解析级别";
}

export function isKnowledgeCodeFileReadable(file: KnowledgeCodeFile) {
  return (
    file.status === "active" &&
    file.sensitivity === "internal" &&
    file.documentVersionId != null
  );
}

export function knowledgeCodeFileReasonLabel(reason: string) {
  const labels: Record<string, string> = {
    binary_content: "二进制内容",
    unsupported_language: "暂不支持的文件类型",
    language_not_allowed: "不在语言白名单中",
    file_too_large: "超过文件大小上限",
    sensitive_file_name: "文件名触发敏感策略",
    non_utf8_content: "不是 UTF-8 文本",
    "sensitive_content:private_key": "包含私钥，已安全阻断",
    "sensitive_content:certificate": "包含证书，已安全阻断",
    "sensitive_content:credential_or_connection_string":
      "包含凭据或连接信息，已安全阻断",
    "sensitive_content:cloud_or_service_token": "包含服务 Token，已安全阻断",
    "redacted_sensitive_content:credential_or_connection_string":
      "凭据或连接信息已脱敏后索引",
    "redacted_sensitive_content:cloud_or_service_token":
      "服务 Token 已脱敏后索引",
  };
  return labels[reason] ?? (reason || "未提供跳过原因");
}

export function summarizeKnowledgeCodeFiles(files: KnowledgeCodeFile[]) {
  const readableFiles = files.filter(isKnowledgeCodeFileReadable).length;
  const redactedFiles = files.filter((file) =>
    file.skipReason.startsWith("redacted_sensitive_content:"),
  ).length;
  return {
    totalFiles: files.length,
    readableFiles,
    redactedFiles,
    skippedFiles: files.length - readableFiles,
  };
}

type DocumentDirectoryNode = TreeDataNode & {
  documentId?: number;
  children?: DocumentDirectoryNode[];
};

function normalizeDocumentPath(
  logicalPath: string,
  title: string,
  sourceFolderName?: string | null,
) {
  const parts = logicalPath
    .split(/[\\/]+/)
    .map((part) => part.trim())
    .filter(Boolean);
  const folderName = sourceFolderName?.trim();
  if (folderName && parts[0] === "upload-folder" && parts[1] === folderName) {
    return [folderName, ...parts.slice(2)];
  }
  return parts.length > 0 ? parts : [title || "未命名文档"];
}

/**
 * 将扁平文档列表转换为可展开目录树，目录仅来自逻辑路径，不会改变文档的历史身份。
 */
export function buildDocumentDirectoryTree(
  documents: KnowledgeDocument[],
  projects: KnowledgeProject[],
): DocumentDirectoryNode[] {
  const projectNames = new Map(
    projects.map((project) => [project.id, project.name]),
  );
  const roots = new Map<number | "unassigned", DocumentDirectoryNode>();

  for (const document of documents) {
    const projectKey = document.projectId ?? "unassigned";
    let projectNode = roots.get(projectKey);
    if (!projectNode) {
      projectNode = {
        key: `project-${projectKey}`,
        title: projectNames.get(document.projectId ?? -1) ?? "未归属项目",
        icon: <FolderSync size={15} aria-hidden />,
        children: [],
      };
      roots.set(projectKey, projectNode);
    }

    const pathParts = normalizeDocumentPath(
      document.logicalPath,
      document.title,
      document.sourceFolderName,
    );
    let siblings = projectNode.children ?? (projectNode.children = []);
    let parentKey = String(projectNode.key);
    pathParts.slice(0, -1).forEach((directory) => {
      const directoryKey = `${parentKey}/${directory}`;
      let directoryNode = siblings.find((node) => node.key === directoryKey) as
        DocumentDirectoryNode | undefined;
      if (!directoryNode) {
        directoryNode = {
          key: directoryKey,
          title: directory,
          icon: <FolderSync size={15} aria-hidden />,
          children: [],
        };
        siblings.push(directoryNode);
      }
      siblings = directoryNode.children ?? (directoryNode.children = []);
      parentKey = directoryKey;
    });

    const status = knowledgeDocumentStatus(document.status);
    const filename = pathParts[pathParts.length - 1] ?? document.title;
    siblings.push({
      key: `document-${document.id}`,
      isLeaf: true,
      documentId: document.id,
      icon: <FileText size={15} aria-hidden />,
      title: (
        <Space size="small">
          <span>{filename}</span>
          {filename !== document.title && (
            <Text type="secondary">{document.title}</Text>
          )}
          <Tag color={status.color}>{status.label}</Tag>
        </Space>
      ),
    });
  }

  return [...roots.values()];
}

type KnowledgeSourceFormValues = UpsertKnowledgeSourceInput & {
  includeLines?: string;
  excludeLines?: string;
  gitWorkspaceKeys?: string[];
};

type EmbeddingWorkflowPhase =
  "estimate" | "building" | "activate" | "completed";

interface EmbeddingWorkflowState {
  profile: KnowledgeEmbeddingProfile;
  phase: EmbeddingWorkflowPhase;
  testDimension: number;
  estimate: KnowledgeEmbeddingRebuildEstimate;
  batch?: KnowledgeEmbeddingBatchResult;
  validation?: KnowledgeEmbeddingIndexValidation;
}

type CodeAnalysisStage = "capturing" | "analyzing" | "completed" | "failed";

interface CodeAnalysisState {
  sourceId: number;
  sourceName: string;
  stage: CodeAnalysisStage;
  snapshotId?: number;
  result?: KnowledgeCodeAnalysisResult;
  error?: string;
}

function citationLabel(citation: KnowledgeCitation) {
  const location = [citation.logicalPath, citation.headingPath]
    .filter(Boolean)
    .join(" · ");
  const lineRange = citation.startLine
    ? ` L${citation.startLine}-${citation.endLine ?? citation.startLine}`
    : "";
  return `${citation.title || citation.externalKey || citation.citationKey}${location ? `（${location}）` : ""}${lineRange}`;
}

function newlineValues(value?: string) {
  return (value ?? "")
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function valuesToLines(values: string[]) {
  return values.join("\n");
}

function CitationList({
  citations,
  onOpen,
}: {
  citations: KnowledgeCitation[];
  onOpen: (citation: KnowledgeCitation) => void;
}) {
  if (citations.length === 0)
    return (
      <Empty
        image={Empty.PRESENTED_IMAGE_SIMPLE}
        description="本次未使用可引用证据"
      />
    );
  return (
    <Collapse
      size="small"
      items={citations.map((citation) => ({
        key: citation.citationKey,
        label: citationLabel(citation),
        extra: <Tag color="blue">{citation.sourceType}</Tag>,
        children: (
          <Space direction="vertical" size={6} className="w-full">
            {citation.commitSha && (
              <Text type="secondary">Commit：{citation.commitSha}</Text>
            )}
            {citation.symbolKey && (
              <Text type="secondary">关联代码元素：{citation.symbolKey}</Text>
            )}
            <Paragraph
              className="mb-0 whitespace-pre-wrap"
              ellipsis={{ rows: 5, expandable: "collapsible" }}
            >
              {citation.excerpt || "该引用未提供摘录。"}
            </Paragraph>
            {citation.documentId && (
              <Button
                size="small"
                type="link"
                className="self-start px-0"
                onClick={() => onOpen(citation)}
              >
                打开原文
              </Button>
            )}
          </Space>
        ),
      }))}
    />
  );
}

export type KnowledgeInitialCatalogTab = "documents" | "embedding";

export interface KnowledgePageProps {
  /** 项目工作台可直接进入全局向量索引配置；其他入口仍默认文档页。 */
  initialCatalogTab?: KnowledgeInitialCatalogTab;
}

export default function KnowledgePage({
  initialCatalogTab = "documents",
}: KnowledgePageProps) {
  const storedProjectIds = useKnowledgeStore((state) => state.projectIds);
  const storedReleaseIds = useKnowledgeStore((state) => state.releaseIds);
  const setStoredProjectIds = useKnowledgeStore((state) => state.setProjectIds);
  const setStoredReleaseIds = useKnowledgeStore((state) => state.setReleaseIds);
  const [projects, setProjects] = useState<KnowledgeProject[]>([]);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [documents, setDocuments] = useState<KnowledgePage<KnowledgeDocument>>({
    items: [],
    total: 0,
    offset: 0,
    limit: 20,
  });
  const [search, setSearch] = useState<KnowledgeSearchInput>(() => ({
    ...INITIAL_SEARCH,
    projectIds: storedProjectIds,
    releaseIds: storedReleaseIds,
  }));
  const [providerKey, setProviderKey] = useState("");
  const [model, setModel] = useState("");
  const [loading, setLoading] = useState(true);
  const [asking, setAsking] = useState(false);
  const [preview, setPreview] = useState<KnowledgeRagContextPreview | null>(
    null,
  );
  const [answer, setAnswer] = useState<KnowledgeAskResult | null>(null);
  const [selectedDocument, setSelectedDocument] =
    useState<KnowledgeDocumentDetail | null>(null);
  // 初始入口只限制为文档或向量索引；工作台内还可切换到已选文档等其他标签。
  const [activeCatalogTab, setActiveCatalogTab] =
    useState<string>(initialCatalogTab);
  const embeddingWorkspaceAvailable = true;
  const [catalogProjectId, setCatalogProjectId] = useState<
    number | undefined
  >();
  // 来源同步必须显式绑定当前项目的发布版本，避免把不同版本的正文混入同一
  // 检索范围；切换项目时同步版本也必须一起清空。
  const [catalogReleaseId, setCatalogReleaseId] = useState<
    number | undefined
  >();
  // 目录请求可能在切换项目后才返回；只允许最新请求更新当前项目的目录状态。
  const catalogRequestIdRef = useRef(0);
  const [catalogReleases, setCatalogReleases] = useState<KnowledgeRelease[]>(
    [],
  );
  const [sources, setSources] = useState<KnowledgeSource[]>([]);
  const [projectGitWorkspaces, setProjectGitWorkspaces] = useState<
    GitWorkspace[]
  >([]);
  const [projectGitWorkspacesLoading, setProjectGitWorkspacesLoading] =
    useState(false);
  const [jobs, setJobs] = useState<KnowledgeJob[]>([]);
  const [scopePreview, setScopePreview] =
    useState<KnowledgeSourceScopePreview | null>(null);
  const [profiles, setProfiles] = useState<KnowledgeEmbeddingProfile[]>([]);
  const [aiProviders, setAiProviders] = useState<AiProvider[]>([]);
  const [aiProvidersLoading, setAiProvidersLoading] = useState(false);
  const [embeddingEstimate, setEmbeddingEstimate] =
    useState<KnowledgeEmbeddingRebuildEstimate | null>(null);
  const [embeddingWorkflow, setEmbeddingWorkflow] =
    useState<EmbeddingWorkflowState | null>(null);
  const [embeddingWorkflowBusy, setEmbeddingWorkflowBusy] = useState(false);
  const [localEmbeddingRuntime, setLocalEmbeddingRuntime] =
    useState<KnowledgeLocalEmbeddingRuntimeStatus | null>(null);
  const [zentaoConnections, setZentaoConnections] = useState<
    ZentaoConnection[]
  >([]);
  const [zentaoMappings, setZentaoMappings] = useState<ZentaoProjectMapping[]>(
    [],
  );
  const [codeSources, setCodeSources] = useState<KnowledgeCodeSource[]>([]);
  const [codeSnapshots, setCodeSnapshots] = useState<KnowledgeCodeSnapshot[]>(
    [],
  );
  // 后端当前只返回捕获和分析两个完整阶段的结果，不能虚构逐文件百分比；
  // 因此前端明确展示已确认的阶段，并在完成后保留本次统计供用户核对。
  const [codeAnalysisState, setCodeAnalysisState] =
    useState<CodeAnalysisState | null>(null);
  // 同一时间仅允许一个源码捕获或分析任务。State 更新不是同步互斥锁，
  // 因此还需 ref 在同一事件循环内阻止重复点击造成的重叠写入。
  const codeAnalysisRunningRef = useRef(false);
  const [selectedZentaoProbe, setSelectedZentaoProbe] =
    useState<ZentaoCapabilityProbeResult | null>(null);
  const [zentaoScopes, setZentaoScopes] = useState<ZentaoRemoteScopeItem[]>([]);
  const [selectedCodeSnapshotId, setSelectedCodeSnapshotId] =
    useState<number>();
  // 源码快照、文件和关系图均由异步请求读取；这些 ref 用于丢弃快速切换后迟到的旧响应，
  // 防止旧快照内容被误展示为当前选择的证据。
  const selectedCodeSnapshotIdRef = useRef<number | undefined>(undefined);
  const selectedCodeSymbolKeyRef = useRef<string | undefined>(undefined);
  const codeSnapshotSelectionRequestIdRef = useRef(0);
  const codeFileRequestIdRef = useRef(0);
  const codeGraphRequestIdRef = useRef(0);
  const [codeSymbols, setCodeSymbols] = useState<KnowledgeCodeSymbol[]>([]);
  const [codeFiles, setCodeFiles] = useState<KnowledgeCodeFile[]>([]);
  const codeFileSummary = useMemo(
    () => summarizeKnowledgeCodeFiles(codeFiles),
    [codeFiles],
  );
  const [selectedCodeFile, setSelectedCodeFile] =
    useState<KnowledgeCodeFileContent | null>(null);
  const [selectedCodeSymbolKey, setSelectedCodeSymbolKey] = useState<string>();
  const [codeGraph, setCodeGraph] = useState<KnowledgeCodeCallGraph | null>(
    null,
  );
  const [codeImpact, setCodeImpact] = useState<KnowledgeCodeCallGraph | null>(
    null,
  );
  const [codeComparison, setCodeComparison] =
    useState<KnowledgeCodeSnapshotComparison | null>(null);
  const [comparisonFromSnapshotId, setComparisonFromSnapshotId] =
    useState<number>();
  const [comparisonToSnapshotId, setComparisonToSnapshotId] =
    useState<number>();
  const [profileModalOpen, setProfileModalOpen] = useState(false);
  const [remoteEmbeddingConfirmationOpen, setRemoteEmbeddingConfirmationOpen] =
    useState(false);
  const [zentaoModalOpen, setZentaoModalOpen] = useState(false);
  const [mappingModalOpen, setMappingModalOpen] = useState(false);
  const [aiSummaryModalOpen, setAiSummaryModalOpen] = useState(false);
  const [aiSummaryMappingId, setAiSummaryMappingId] = useState<number>();
  const [codeSourceModalOpen, setCodeSourceModalOpen] = useState(false);
  const [projectModalOpen, setProjectModalOpen] = useState(false);
  const [releaseModalOpen, setReleaseModalOpen] = useState(false);
  const [sourceModalOpen, setSourceModalOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [projectForm] = Form.useForm<UpsertKnowledgeProjectInput>();
  const [releaseForm] = Form.useForm<UpsertKnowledgeReleaseInput>();
  const [sourceForm] = Form.useForm<KnowledgeSourceFormValues>();
  const [profileForm] = Form.useForm<
    UpsertKnowledgeEmbeddingProfileInput & { configText?: string }
  >();
  // 弹窗首次挂载前会先 setFieldsValue；使用独立状态确保远程模式的下拉不会在首帧
  // 仍按默认本地模式禁用，其他联动字段也由同一组状态保持首帧一致。
  const [profileMode, setProfileMode] = useState<"local" | "remote">("local");
  const [profileProviderKey, setProfileProviderKey] = useState("");
  const [profileModel, setProfileModel] = useState("");
  const [localEmbeddingForm] = Form.useForm<{
    modelKey: string;
    sourcePath?: string;
    expectedSha256?: string;
  }>();
  const [zentaoForm] = Form.useForm<UpsertZentaoConnectionInput>();
  const zentaoBaseUrl = Form.useWatch("baseUrl", zentaoForm) ?? "";
  const zentaoUsesInsecureHttp = zentaoBaseUrl
    .trim()
    .toLowerCase()
    .startsWith("http://");
  const [mappingForm] = Form.useForm<
    UpsertZentaoProjectMappingInput & {
      executionLines?: string;
      releaseMappingText?: string;
      syncScopeText?: string;
    }
  >();
  const [aiSummaryForm] = Form.useForm<{
    providerKey: string;
    model: string;
    prompt: string;
  }>();
  const [codeSourceForm] = Form.useForm<
    UpsertKnowledgeCodeSourceInput & {
      includeLines?: string;
      excludeLines?: string;
    }
  >();

  const projectOptions = useMemo(
    () =>
      projects.map((project) => ({
        value: project.id,
        label: `${project.name}（${project.projectKey}）`,
      })),
    [projects],
  );
  const releaseOptions = useMemo(
    () =>
      releases.map((release) => ({
        value: release.id,
        label: release.version || "未版本化",
      })),
    [releases],
  );
  const projectGitWorkspaceOptions = useMemo(
    () =>
      projectGitWorkspaces.map((workspace) => ({
        value: workspace.workspaceKey,
        label: `${workspace.name}（${workspace.workspaceKey} · ${workspace.repoPath}）`,
      })),
    [projectGitWorkspaces],
  );
  const remoteEmbeddingProviders = useMemo(
    () =>
      aiProviders.filter(
        (provider) =>
          provider.enabled &&
          provider.status === "configured" &&
          providerEmbeddingModels(provider).length > 0,
      ),
    [aiProviders],
  );
  const knowledgeChatProviderOptions = useMemo(
    () =>
      aiProviders.filter(isAvailableKnowledgeChatProvider).map((provider) => ({
        value: provider.key,
        label: `${provider.name}（${provider.key}）`,
      })),
    [aiProviders],
  );
  const selectedEmbeddingProvider = useMemo(
    () =>
      remoteEmbeddingProviders.find(
        (provider) => provider.key === profileProviderKey,
      ),
    [profileProviderKey, remoteEmbeddingProviders],
  );
  const remoteEmbeddingProviderOptions = useMemo(() => {
    const options: Array<{
      value: string;
      label: string;
      disabled?: boolean;
    }> = remoteEmbeddingProviders.map((provider) => ({
      value: provider.key,
      label: `${provider.name}（${provider.key}）`,
    }));
    // 保留历史方案中已失效的服务商标识，帮助用户定位并更换；该项不可重新选择，
    // 保存前也会强制阻止，不能让历史配置被无感保存为不可用方案。
    if (
      profileProviderKey &&
      !options.some((option) => option.value === profileProviderKey)
    ) {
      options.unshift({
        value: profileProviderKey,
        label: `当前服务商不可用，请重新选择（${profileProviderKey}）`,
        disabled: true,
      });
    }
    return options;
  }, [profileProviderKey, remoteEmbeddingProviders]);
  const profileModelOptions = useMemo(() => {
    const candidateModels =
      profileMode === "remote"
        ? providerEmbeddingModels(selectedEmbeddingProvider)
        : uniqueEmbeddingModelNames(
            LOCAL_EMBEDDING_MODEL_OPTIONS,
            localEmbeddingRuntime?.cachedModels.map(
              (entry) => entry.modelKey,
            ) ?? [],
          );
    return uniqueEmbeddingModelNames(
      candidateModels,
      profileModel ? [profileModel] : [],
    ).map((model) => ({ label: model, value: model }));
  }, [
    localEmbeddingRuntime?.cachedModels,
    profileMode,
    profileModel,
    selectedEmbeddingProvider,
  ]);
  const documentDirectoryTree = useMemo(
    () => buildDocumentDirectoryTree(documents.items, projects),
    [documents.items, projects],
  );

  const loadProjects = useCallback(async () => {
    const result = await knowledgeApi.listProjects({ limit: 100, offset: 0 });
    setProjects(result.items);
  }, []);

  const loadProjectGitWorkspaces = useCallback(async () => {
    setProjectGitWorkspacesLoading(true);
    try {
      setProjectGitWorkspaces(await gitWorkspaceApi.list({}));
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setProjectGitWorkspacesLoading(false);
    }
  }, []);

  const loadAiProviders = useCallback(async () => {
    setAiProvidersLoading(true);
    try {
      setAiProviders(await aiProviderApi.list());
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setAiProvidersLoading(false);
    }
  }, []);

  const loadDocuments = useCallback(async (current: KnowledgeSearchInput) => {
    const documentItems: KnowledgeDocument[] = [];
    let offset = 0;
    let total = 0;

    do {
      const result = await knowledgeApi.listDocuments({
        projectId:
          current.projectIds.length === 1 ? current.projectIds[0] : undefined,
        releaseId:
          current.releaseIds.length === 1 ? current.releaseIds[0] : undefined,
        keyword: current.query || undefined,
        limit: KNOWLEDGE_DOCUMENT_PAGE_SIZE,
        offset,
      });
      documentItems.push(...result.items);
      total = result.total;
      offset += result.items.length;
      // 服务端可能因权限或并发删除返回空页，防止目录加载陷入循环。
      if (result.items.length === 0) break;
    } while (documentItems.length < total);

    setDocuments({
      items: documentItems,
      total,
      offset: 0,
      limit: KNOWLEDGE_DOCUMENT_PAGE_SIZE,
    });
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      await Promise.all([loadProjects(), loadDocuments(search)]);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [loadDocuments, loadProjects, search]);

  const loadCatalog = useCallback(
    async (projectId = catalogProjectId) => {
      const requestId = ++catalogRequestIdRef.current;
      try {
        const [nextSources, nextJobs] = await Promise.all([
          knowledgeApi.listSources(projectId),
          knowledgeApi.listJobs(),
        ]);
        const nextCatalogReleases =
          projectId != null ? await knowledgeApi.listReleases(projectId) : [];
        if (requestId !== catalogRequestIdRef.current) return;

        setSources(nextSources);
        setJobs(nextJobs);
        setCatalogReleases(nextCatalogReleases);
      } catch (error) {
        if (requestId === catalogRequestIdRef.current)
          message.error(getErrorMessage(error));
      }
    },
    [catalogProjectId],
  );

  const loadIntegrationViews = useCallback(async () => {
    try {
      const [
        nextProfiles,
        nextLocalRuntime,
        nextConnections,
        nextMappings,
        nextCodeSources,
        nextSnapshots,
      ] = await Promise.all([
        knowledgeApi.listEmbeddingProfiles(),
        knowledgeApi.getLocalEmbeddingRuntimeStatus(),
        knowledgeApi.listZentaoConnections(),
        knowledgeApi.listZentaoProjectMappings(),
        knowledgeApi.listCodeSources(),
        knowledgeApi.listCodeSnapshots(),
      ]);
      setProfiles(nextProfiles);
      setLocalEmbeddingRuntime(nextLocalRuntime);
      setZentaoConnections(nextConnections);
      setZentaoMappings(nextMappings);
      setCodeSources(nextCodeSources);
      setCodeSnapshots(nextSnapshots);
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }, []);

  useEffect(() => {
    // 知识问答本身需要选择对话服务商，不能依赖用户先打开远程向量化配置弹窗。
    void loadAiProviders();
  }, [loadAiProviders]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!profileModalOpen || profileMode !== "remote") return;
    void loadAiProviders();
  }, [loadAiProviders, profileModalOpen, profileMode]);

  useEffect(() => {
    void loadCatalog();
  }, [loadCatalog]);

  useEffect(() => {
    const hasActiveJobs = jobs.some((job) =>
      ["queued", "running"].includes(job.status),
    );
    if (!hasActiveJobs) return undefined;

    let refreshing = false;
    const refreshActiveJobs = () => {
      if (refreshing) return;
      refreshing = true;
      // 同步任务在后台执行，轮询同时更新任务进度和来源的最近同步状态。
      void loadCatalog().finally(() => {
        refreshing = false;
      });
    };
    const timer = window.setInterval(
      refreshActiveJobs,
      KNOWLEDGE_JOB_REFRESH_INTERVAL_MS,
    );
    return () => window.clearInterval(timer);
  }, [jobs, loadCatalog]);

  useEffect(() => {
    void loadIntegrationViews();
  }, [loadIntegrationViews]);

  useEffect(() => {
    if (search.projectIds.length !== 1) return;
    knowledgeApi
      .listReleases(search.projectIds[0])
      .then(setReleases)
      .catch((error) => {
        message.error(getErrorMessage(error));
      });
  }, [search.projectIds]);

  const changeProjects = async (projectIds: number[]) => {
    const next = { ...search, projectIds, releaseIds: [] };
    setSearch(next);
    setStoredProjectIds(projectIds);
    setReleases([]);
    if (projectIds.length === 1) {
      try {
        setReleases(await knowledgeApi.listReleases(projectIds[0]));
      } catch (error) {
        message.error(getErrorMessage(error));
      }
    }
  };

  const previewContext = async () => {
    if (!search.query.trim()) {
      message.warning("请输入要检索的问题或关键词");
      return;
    }
    setAsking(true);
    try {
      setPreview(await knowledgeApi.previewRagContext(search));
      setAnswer(null);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setAsking(false);
    }
  };

  const ask = async () => {
    if (!search.query.trim()) {
      message.warning("请输入要提问的内容");
      return;
    }
    if (!providerKey.trim() || !model.trim()) {
      message.warning("请选择已配置的 AI 服务商并填写模型名称");
      return;
    }
    setAsking(true);
    try {
      setAnswer(
        await knowledgeApi.ask({
          search,
          providerKey,
          model,
          evidenceOnly: true,
        }),
      );
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setAsking(false);
    }
  };

  const changeKnowledgeChatProvider = (nextProviderKey?: string) => {
    const nextKey = nextProviderKey ?? "";
    setProviderKey(nextKey);
    // 仅在模型尚未填写时带入默认模型，用户已填写的模型不应被切换服务商意外覆盖。
    if (!model.trim()) {
      const provider = aiProviders.find((item) => item.key === nextKey);
      if (provider?.defaultModel.trim()) setModel(provider.defaultModel);
    }
  };

  const openDocument = async (documentId: number) => {
    try {
      setSelectedDocument(await knowledgeApi.getDocumentDetail(documentId));
      setActiveCatalogTab("selected");
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  // 引用先经后端详情接口校验，再打开对应逻辑文档，避免前端按路径直接读取本地文件。
  const openCitation = async (citation: KnowledgeCitation) => {
    if (!citation.chunkId || !citation.documentId) return;
    try {
      const detail = await knowledgeApi.getCitationDetail(citation.chunkId);
      if (detail.document.id !== citation.documentId) {
        throw new Error("引用与知识文档不一致");
      }
      await openDocument(detail.document.id);
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const openProjectModal = (project?: KnowledgeProject) => {
    projectForm.setFieldsValue({
      id: project?.id,
      projectKey: project?.projectKey ?? "",
      name: project?.name ?? "",
      aliases: project?.aliases ?? [],
      description: project?.description ?? "",
      gitWorkspaceKeys:
        project?.gitWorkspaceKeys ??
        (project?.gitWorkspaceKey ? [project.gitWorkspaceKey] : []),
      gitWorkspaceKey: project?.gitWorkspaceKey ?? "",
      defaultBranch: project?.defaultBranch ?? "main",
      enabled: project?.enabled ?? true,
    });
    setProjectModalOpen(true);
    void loadProjectGitWorkspaces();
  };

  const saveProject = async () => {
    try {
      setSaving(true);
      const values = await projectForm.validateFields();
      const gitWorkspaceKeys = values.gitWorkspaceKeys ?? [];
      await knowledgeApi.upsertProject({
        ...values,
        gitWorkspaceKeys,
        // 保留旧字段，供升级中的旧桌面端与只支持单工作区的读取路径兼容。
        gitWorkspaceKey: gitWorkspaceKeys[0] ?? values.gitWorkspaceKey ?? "",
      });
      message.success("知识项目已保存");
      setProjectModalOpen(false);
      await Promise.all([loadProjects(), loadCatalog()]);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const selectCatalogProject = (projectId?: number) => {
    setCatalogProjectId(projectId);
    setCatalogReleaseId(undefined);
  };

  const openReleaseModal = (release?: KnowledgeRelease) => {
    releaseForm.setFieldsValue({
      id: release?.id,
      projectId: release?.projectId ?? catalogProjectId,
      version: release?.version ?? "",
      tagName: release?.tagName ?? "",
      branch: release?.branch ?? "",
      commitSha: release?.commitSha ?? "",
      description: release?.description ?? "",
      releasedAt: release?.releasedAt ?? null,
    });
    setReleaseModalOpen(true);
  };

  const saveRelease = async () => {
    try {
      setSaving(true);
      await knowledgeApi.upsertRelease(await releaseForm.validateFields());
      message.success("知识版本已保存");
      setReleaseModalOpen(false);
      await loadCatalog();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const openSourceModal = (source?: KnowledgeSource) => {
    sourceForm.setFieldsValue({
      id: source?.id,
      sourceKey: source?.sourceKey ?? "",
      projectId: source?.projectId ?? catalogProjectId ?? null,
      sourceType: source?.sourceType ?? "local_directory",
      displayName: source?.displayName ?? "",
      rootPath: source?.rootPath ?? "",
      gitWorkspaceKeys: source?.gitWorkspaceKey ? [source.gitWorkspaceKey] : [],
      gitWorkspaceKey: source?.gitWorkspaceKey ?? "",
      includeGlobs: source?.includeGlobs ?? [],
      excludeGlobs: source?.excludeGlobs ?? [],
      includeLines: valuesToLines(source?.includeGlobs ?? []),
      excludeLines: valuesToLines(source?.excludeGlobs ?? []),
      versionStrategy: source?.versionStrategy ?? "unversioned",
      syncMode: source?.syncMode ?? "incremental",
      allowRemoteEmbedding: source?.allowRemoteEmbedding ?? false,
      enabled: source?.enabled ?? true,
    });
    setScopePreview(null);
    setSourceModalOpen(true);
    void loadProjectGitWorkspaces();
  };

  const sourceInput = async () => {
    const values = await sourceForm.validateFields();
    const gitWorkspaceKeys = Array.from(
      new Set(
        (values.gitWorkspaceKeys ?? [])
          .map((workspaceKey) => workspaceKey.trim())
          .filter(Boolean),
      ),
    );
    return {
      ...values,
      gitWorkspaceKeys,
      gitWorkspaceKey: gitWorkspaceKeys[0] ?? values.gitWorkspaceKey ?? "",
      includeGlobs: newlineValues(values.includeLines),
      excludeGlobs: newlineValues(values.excludeLines),
    };
  };

  const previewScope = async () => {
    try {
      const input = await sourceInput();
      if (input.gitWorkspaceKeys.length > 1) {
        message.info(
          "已选择多个 Git 工作区；请保存后分别在来源列表中预览读取范围",
        );
        return;
      }
      setScopePreview(await knowledgeApi.previewSourceScope(input));
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const saveSource = async () => {
    try {
      setSaving(true);
      const input = await sourceInput();
      const gitWorkspaceKeys =
        input.sourceType === "git_workspace" ? input.gitWorkspaceKeys : [];
      if (gitWorkspaceKeys.length <= 1) {
        await knowledgeApi.upsertSource(input);
        message.success("知识来源已保存");
      } else {
        if (
          input.id != null &&
          input.gitWorkspaceKey &&
          !gitWorkspaceKeys.includes(input.gitWorkspaceKey)
        ) {
          throw new Error(
            "编辑已有 Git 来源时必须保留原工作区；请先单独创建新来源，再删除旧来源",
          );
        }
        await knowledgeApi.upsertSourcesAtomically(
          gitWorkspaceKeys.map((workspaceKey, index) => {
            const keepsExistingSource =
              input.id != null && input.gitWorkspaceKey === workspaceKey;
            const usesPrimaryKey = keepsExistingSource || index === 0;
            return {
              ...input,
              // 每个仓库有独立的游标与 Commit 基线，不能拼成一个知识源。
              // 编辑时按原工作区保留来源 ID，不能因多选顺序变化而迁移历史证据。
              id: keepsExistingSource ? input.id : undefined,
              sourceKey: usesPrimaryKey
                ? input.sourceKey
                : `${input.sourceKey}-${workspaceKey}`,
              displayName: usesPrimaryKey
                ? input.displayName
                : `${input.displayName} · ${workspaceKey}`,
              gitWorkspaceKey: workspaceKey,
            };
          }),
        );
        message.success(
          `已分别保存 ${gitWorkspaceKeys.length} 个 Git 工作区来源`,
        );
      }
      setSourceModalOpen(false);
      await loadCatalog();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const startSourceSync = async (source: KnowledgeSource) => {
    try {
      if (
        ["git_workspace", "local_directory", "single_file"].includes(
          source.sourceType,
        ) &&
        catalogReleaseId == null
      ) {
        message.warning("请先选择要绑定的项目发布版本，再开始同步");
        return;
      }
      const job = await knowledgeApi.startSourceSync({
        sourceId: source.id,
        releaseId: catalogReleaseId,
      });
      message.success(`已启动同步任务：${job.jobKey}`);
      await loadCatalog();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const openProfileModal = (profile?: KnowledgeEmbeddingProfile) => {
    const defaultRemoteProvider = remoteEmbeddingProviders[0];
    const initialMode: "local" | "remote" = profile
      ? profile.mode === "remote"
        ? "remote"
        : "local"
      : defaultRemoteProvider
        ? "remote"
        : "local";
    const initialProviderKey =
      profile?.providerKey ??
      (initialMode === "remote" ? (defaultRemoteProvider?.key ?? "") : "");
    const initialProvider = remoteEmbeddingProviders.find(
      (provider) => provider.key === initialProviderKey,
    );
    const initialModel =
      profile?.model ??
      (initialMode === "remote"
        ? (providerEmbeddingModels(initialProvider)[0] ?? "")
        : "multilingual-e5-small-int8");
    profileForm.setFieldsValue({
      id: profile?.id,
      profileKey: profile?.profileKey ?? "",
      name: profile?.name ?? "",
      mode: initialMode,
      providerKey: initialProviderKey,
      // ADR-001 的真实混合语料基准选定 int8 E5；保留用户手动切换其他兼容模型的能力。
      model: initialModel,
      modelRevision: profile?.modelRevision ?? "",
      dimension: profile?.dimension ?? 384,
      normalized: profile?.normalized ?? true,
      fingerprint: profile?.fingerprint ?? "",
      configText: JSON.stringify(profile?.config ?? {}, null, 2),
    });
    setProfileMode(initialMode);
    setProfileProviderKey(initialProviderKey);
    setProfileModel(initialModel);
    setEmbeddingEstimate(null);
    setProfileModalOpen(true);
  };

  const handleRemoteEmbeddingProviderChange = (providerKey?: string) => {
    const normalizedProviderKey = providerKey ?? "";
    const provider = remoteEmbeddingProviders.find(
      (item) => item.key === normalizedProviderKey,
    );
    const candidateModels = providerEmbeddingModels(provider);
    const currentModel = String(
      profileForm.getFieldValue("model") ?? "",
    ).trim();
    const model = candidateModels.includes(currentModel)
      ? currentModel
      : (candidateModels[0] ?? "");
    profileForm.setFieldsValue({ model });
    setProfileProviderKey(normalizedProviderKey);
    setProfileModel(model);
  };

  const handleEmbeddingModeChange = (mode: "local" | "remote") => {
    setProfileMode(mode);
    const currentModel = String(
      profileForm.getFieldValue("model") ?? "",
    ).trim();
    if (mode === "local") {
      // 本地方案不应携带远程 Provider，否则会污染 Profile 指纹并造成无意义的重建。
      profileForm.setFieldsValue({
        providerKey: "",
        model: LOCAL_EMBEDDING_MODEL_OPTIONS.includes(currentModel)
          ? currentModel
          : LOCAL_EMBEDDING_MODEL_OPTIONS[0],
      });
      setProfileProviderKey("");
      setProfileModel(
        LOCAL_EMBEDDING_MODEL_OPTIONS.includes(currentModel)
          ? currentModel
          : LOCAL_EMBEDDING_MODEL_OPTIONS[0],
      );
      return;
    }
    // 远程模型必须属于新选服务商，不能沿用本地模型标识。
    profileForm.setFieldsValue({ providerKey: "", model: "" });
    setProfileProviderKey("");
    setProfileModel("");
  };

  const saveProfile = async () => {
    try {
      setSaving(true);
      const values = await profileForm.validateFields();
      const providerKey = normalizedEmbeddingProfileProviderKey(
        values.mode,
        values.providerKey,
      );
      if (
        values.mode === "remote" &&
        (!providerKey ||
          !hasAvailableRemoteEmbeddingProvider(providerKey, aiProviders))
      ) {
        throw new Error("请选择已启用且已配置向量模型的服务商");
      }
      const config = JSON.parse(values.configText || "{}") as Record<
        string,
        unknown
      >;
      const fingerprint = await knowledgeApi.calculateEmbeddingFingerprint({
        mode: values.mode,
        providerProtocol: String(
          config.providerProtocol ??
            (values.mode === "local" ? "local" : "openai_compatible"),
        ),
        endpointIdentity: String(config.endpointIdentity ?? ""),
        providerKey,
        model: values.model,
        modelRevision: values.modelRevision,
        dimension: values.dimension,
        normalized: values.normalized,
        queryPrefix: String(config.queryPrefix ?? "query: "),
        documentPrefix: String(config.documentPrefix ?? "passage: "),
        chunkStrategyId: String(
          config.chunkStrategyId ?? "knowledge-structure-v1",
        ),
        normalizationVersion: String(config.normalizationVersion ?? "v1"),
      });
      const {
        configText: _configText,
        config: _ignoredConfig,
        ...profileInput
      } = values;
      const profile = await knowledgeApi.upsertEmbeddingProfile({
        ...profileInput,
        providerKey,
        config,
        fingerprint,
      });
      setEmbeddingEstimate(
        await knowledgeApi.estimateEmbeddingRebuild({ profileId: profile.id }),
      );
      message.success("向量化方案已保存；请确认估算后执行蓝绿重建");
      await loadIntegrationViews();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const startEmbeddingWorkflow = async (profile: KnowledgeEmbeddingProfile) => {
    if (embeddingWorkflow || embeddingWorkflowBusy) {
      message.info("请先完成或关闭当前向量索引构建流程");
      return;
    }
    setEmbeddingWorkflowBusy(true);
    try {
      // 草稿需要先做真实短文本探测；ready/failed 方案已经保存过维度，直接进入
      // 估算/构建，避免后端拒绝对非 draft Profile 重复探测。
      const test =
        profile.status === "draft"
          ? profile.mode === "remote"
            ? await knowledgeApi.testRemoteEmbeddingProfile(profile.id)
            : await knowledgeApi.testLocalEmbeddingProfile(profile.id)
          : { profile, dimension: profile.dimension };
      if (test.dimension <= 0) {
        throw new Error("该向量化方案尚未保存有效维度，请重新配置后再构建");
      }
      const estimate = await knowledgeApi.estimateEmbeddingRebuild({
        profileId: profile.id,
      });
      setEmbeddingEstimate(estimate);
      setEmbeddingWorkflow({
        profile: test.profile,
        phase: "estimate",
        testDimension: test.dimension,
        estimate,
      });
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setEmbeddingWorkflowBusy(false);
    }
  };

  const buildEmbeddingWorkflow = async () => {
    if (!embeddingWorkflow) return;
    const { profile, estimate, testDimension } = embeddingWorkflow;
    setEmbeddingWorkflowBusy(true);
    try {
      await knowledgeApi.beginEmbeddingProfileRebuild(profile.id);
      const build =
        profile.mode === "remote"
          ? knowledgeApi.buildRemoteEmbeddingBatch
          : knowledgeApi.buildLocalEmbeddingBatch;
      let batch: KnowledgeEmbeddingBatchResult | undefined;
      let previousProcessedChunks: number | undefined;
      do {
        const nextBatch = await build({
          profileId: profile.id,
          jobKey: batch?.jobKey,
        });
        if (
          !nextBatch.completed &&
          previousProcessedChunks !== undefined &&
          nextBatch.processedChunks <= previousProcessedChunks
        ) {
          throw new Error("向量构建未推进，请检查同步任务或稍后重试");
        }
        batch = nextBatch;
        previousProcessedChunks = batch.processedChunks;
        setEmbeddingWorkflow({
          profile,
          phase: "building",
          testDimension,
          estimate,
          batch,
        });
      } while (!batch.completed);

      const validation = await knowledgeApi.validateEmbeddingProfileRebuild(
        profile.id,
      );
      if (!validation.complete) {
        // 后端在返回“不完整”时会先把 Profile 标记为 failed，再返回校验错误。
        // 只有真正的数据库/IPC 故障才表示收尾失败；不能把这两类错误静默吞掉，
        // 否则用户会看到一个永久停留在 building 的方案而无法重试。
        let cleanupFailure: string | null = null;
        try {
          await knowledgeApi.completeEmbeddingProfileRebuild(profile.id);
        } catch (cleanupError) {
          const cleanupMessage = getErrorMessage(cleanupError);
          if (!cleanupMessage.includes("Profile 构建未完成")) {
            cleanupFailure = cleanupMessage;
          }
        }
        if (cleanupFailure) {
          throw new Error(
            `向量索引校验未通过，且失败收尾未完成：${cleanupFailure}。请刷新状态后重试。`,
          );
        }
        throw new Error("向量索引校验未通过，方案已标记为失败，可重新构建");
      }
      const completed = await knowledgeApi.completeEmbeddingProfileRebuild(
        profile.id,
      );
      setEmbeddingWorkflow({
        profile: completed.profile,
        phase: "activate",
        testDimension,
        estimate,
        batch,
        validation: completed.validation,
      });
      message.success("向量索引已构建并校验完成，可以激活");
      await loadIntegrationViews();
    } catch (error) {
      message.error(getErrorMessage(error));
      void loadIntegrationViews();
    } finally {
      setEmbeddingWorkflowBusy(false);
    }
  };

  const requestEmbeddingBuild = () => {
    if (!embeddingWorkflow) return;
    if (embeddingWorkflow.estimate.requiresRemoteConfirmation) {
      setRemoteEmbeddingConfirmationOpen(true);
      return;
    }
    void buildEmbeddingWorkflow();
  };

  const confirmEmbeddingBuild = async () => {
    setRemoteEmbeddingConfirmationOpen(false);
    await buildEmbeddingWorkflow();
  };

  const activateEmbeddingWorkflow = async () => {
    if (!embeddingWorkflow) return;
    setEmbeddingWorkflowBusy(true);
    try {
      const activated = await knowledgeApi.activateEmbeddingProfileRebuild(
        embeddingWorkflow.profile.id,
      );
      if (activated?.profile?.isActive !== true) {
        throw new Error(
          "向量索引尚未激活，请刷新索引状态后重试；如仍未激活，请重新完成构建和校验。",
        );
      }
      setEmbeddingWorkflow((current) =>
        current
          ? {
              ...current,
              profile: activated.profile,
              validation: activated.validation,
              phase: "completed",
            }
          : current,
      );
      message.success("远程向量索引已激活");
      await loadIntegrationViews();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setEmbeddingWorkflowBusy(false);
    }
  };

  const probeZentaoConnection = async (connectionId: number) => {
    try {
      const probe = await knowledgeApi.probeZentaoConnection(connectionId);
      setSelectedZentaoProbe(probe);
      setZentaoScopes([]);
      await loadIntegrationViews();
      message.success("禅道能力探测完成");
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const discoverZentaoScopes = async (connectionId: number) => {
    try {
      const scopes =
        await knowledgeApi.discoverZentaoRemoteScopes(connectionId);
      setZentaoScopes(scopes);
      message.success(`已发现 ${scopes.length} 条可映射远程范围`);
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const selectCodeSnapshot = async (snapshotId?: number) => {
    const requestId = ++codeSnapshotSelectionRequestIdRef.current;
    selectedCodeSnapshotIdRef.current = snapshotId;
    selectedCodeSymbolKeyRef.current = undefined;
    // 使正在读取的文件和关系图请求立即失效，避免旧结果覆盖新快照。
    codeFileRequestIdRef.current += 1;
    codeGraphRequestIdRef.current += 1;
    setSelectedCodeSnapshotId(snapshotId);
    setSelectedCodeSymbolKey(undefined);
    setSelectedCodeFile(null);
    setCodeGraph(null);
    setCodeImpact(null);
    if (snapshotId == null) {
      setCodeSymbols([]);
      setCodeFiles([]);
      return;
    }
    try {
      const [nextSymbols, nextFiles] = await Promise.all([
        knowledgeApi.searchCodeSymbols({ snapshotId, keyword: "" }),
        knowledgeApi.listCodeFiles(snapshotId),
      ]);
      if (requestId !== codeSnapshotSelectionRequestIdRef.current) return;
      setCodeSymbols(nextSymbols);
      setCodeFiles(nextFiles);
    } catch (error) {
      if (requestId === codeSnapshotSelectionRequestIdRef.current)
        message.error(getErrorMessage(error));
    }
  };

  const openCodeFile = async (fileId?: number) => {
    if (selectedCodeSnapshotId == null || fileId == null) {
      message.warning("请先选择一个已分析快照和代码文件");
      return;
    }
    const snapshotId = selectedCodeSnapshotId;
    const requestId = ++codeFileRequestIdRef.current;
    try {
      const fileContent = await knowledgeApi.getCodeFileContent(
        snapshotId,
        fileId,
      );
      if (
        requestId !== codeFileRequestIdRef.current ||
        selectedCodeSnapshotIdRef.current !== snapshotId
      )
        return;
      setSelectedCodeFile(fileContent);
    } catch (error) {
      if (
        requestId === codeFileRequestIdRef.current &&
        selectedCodeSnapshotIdRef.current === snapshotId
      )
        message.error(getErrorMessage(error));
    }
  };

  const selectCodeSymbol = (symbolKey?: string) => {
    selectedCodeSymbolKeyRef.current = symbolKey;
    codeGraphRequestIdRef.current += 1;
    setSelectedCodeSymbolKey(symbolKey);
    // 关系图属于特定根符号；切换后清空旧图，避免把上一个符号的结果误认为当前结果。
    setCodeGraph(null);
    setCodeImpact(null);
  };

  const showCodeGraph = async (impact: boolean) => {
    if (selectedCodeSnapshotId == null || !selectedCodeSymbolKey) {
      message.warning("请先选择一个已分析快照和代码元素");
      return;
    }
    const snapshotId = selectedCodeSnapshotId;
    const symbolKey = selectedCodeSymbolKey;
    const requestId = ++codeGraphRequestIdRef.current;
    try {
      if (impact) {
        setCodeGraph(null);
        const graph = await knowledgeApi.analyzeCodeImpact({
          snapshotId,
          symbolKeys: [symbolKey],
          maxDepth: 2,
        });
        if (
          requestId !== codeGraphRequestIdRef.current ||
          selectedCodeSnapshotIdRef.current !== snapshotId ||
          selectedCodeSymbolKeyRef.current !== symbolKey
        )
          return;
        setCodeImpact(graph);
      } else {
        setCodeImpact(null);
        const graph = await knowledgeApi.getCodeCallGraph({
          snapshotId,
          symbolKey,
          maxDepth: 2,
        });
        if (
          requestId !== codeGraphRequestIdRef.current ||
          selectedCodeSnapshotIdRef.current !== snapshotId ||
          selectedCodeSymbolKeyRef.current !== symbolKey
        )
          return;
        setCodeGraph(graph);
      }
    } catch (error) {
      if (
        requestId === codeGraphRequestIdRef.current &&
        selectedCodeSnapshotIdRef.current === snapshotId &&
        selectedCodeSymbolKeyRef.current === symbolKey
      )
        message.error(getErrorMessage(error));
    }
  };

  const compareSelectedCodeSnapshots = async (
    fromSnapshotId?: number,
    toSnapshotId?: number,
  ) => {
    if (
      fromSnapshotId == null ||
      toSnapshotId == null ||
      fromSnapshotId === toSnapshotId
    ) {
      message.warning("请选择两个不同的代码快照进行比较");
      return;
    }
    try {
      setCodeComparison(
        await knowledgeApi.compareCodeSnapshots({
          fromSnapshotId,
          toSnapshotId,
        }),
      );
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const importLocalEmbeddingModel = async () => {
    try {
      setSaving(true);
      const values = await localEmbeddingForm.validateFields();
      await knowledgeApi.importLocalEmbeddingModel({
        modelKey: values.modelKey,
        sourcePath: values.sourcePath ?? "",
        expectedSha256: values.expectedSha256 ?? "",
      });
      message.success("离线模型已校验并导入本地缓存");
      await loadIntegrationViews();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const downloadLocalEmbeddingModel = async () => {
    try {
      setSaving(true);
      const { modelKey } = await localEmbeddingForm.validateFields([
        "modelKey",
      ]);
      await knowledgeApi.downloadLocalEmbeddingModel({ modelKey });
      message.success("模型已从受控内部镜像下载并校验");
      await loadIntegrationViews();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const openZentaoModal = (connection?: ZentaoConnection) => {
    zentaoForm.setFieldsValue({
      id: connection?.id,
      connectionKey: connection?.connectionKey ?? "",
      name: connection?.name ?? "",
      baseUrl: connection?.baseUrl ?? "",
      apiVersion: connection?.apiVersion ?? "auto",
      authMode: connection?.authMode ?? "bearer",
      endpointProfile: connection?.endpointProfile ?? "",
      credentialKey: "",
      tlsVerify: connection?.tlsVerify ?? true,
      allowInsecureHttp: connection?.allowInsecureHttp ?? false,
      requestTimeoutSeconds: connection?.requestTimeoutSeconds ?? 30,
      pageSize: connection?.pageSize ?? 100,
      rateLimitPerSecond: connection?.rateLimitPerSecond ?? 5,
      enabled: connection?.enabled ?? true,
    });
    setZentaoModalOpen(true);
  };

  const persistZentaoConnection = async (
    input: UpsertZentaoConnectionInput,
  ) => {
    setSaving(true);
    try {
      await knowledgeApi.upsertZentaoConnection(input);
      setZentaoModalOpen(false);
      message.success("禅道连接已保存；请先执行只读能力探测");
      await loadIntegrationViews();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const saveZentaoConnection = async () => {
    try {
      const input: UpsertZentaoConnectionInput =
        await zentaoForm.validateFields();
      const usesInsecureHttp = input.baseUrl
        .trim()
        .toLowerCase()
        .startsWith("http://");
      if (!usesInsecureHttp) {
        await persistZentaoConnection(input);
        return;
      }
      if (!input.allowInsecureHttp || input.tlsVerify) {
        message.error("HTTP 连接必须显式允许内网 HTTP，并关闭证书校验");
        return;
      }
      Modal.confirm({
        title: "确认保存明文 HTTP 禅道连接？",
        content:
          "HTTP 会以明文传输 Token/Cookie，仅可用于已受控的内网。连接仍仅允许同源只读 GET，且不会跟随重定向。",
        okText: "确认保存 HTTP 连接",
        cancelText: "取消",
        okButtonProps: { danger: true },
        onOk: () => persistZentaoConnection(input),
      });
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  };

  const openMappingModal = (mapping?: ZentaoProjectMapping) => {
    mappingForm.setFieldsValue({
      id: mapping?.id,
      connectionId: mapping?.connectionId,
      knowledgeProjectId: mapping?.knowledgeProjectId ?? catalogProjectId,
      remoteProductId: mapping?.remoteProductId ?? "",
      remoteProjectId: mapping?.remoteProjectId ?? "",
      remoteExecutionIds: mapping?.remoteExecutionIds ?? [],
      executionLines: valuesToLines(mapping?.remoteExecutionIds ?? []),
      releaseMappingText: JSON.stringify(
        mapping?.releaseMapping ?? {},
        null,
        2,
      ),
      syncScopeText: JSON.stringify(mapping?.syncScope ?? {}, null, 2),
      syncSince: mapping?.syncSince ?? null,
      includeComments: mapping?.includeComments ?? false,
      includeWorklogs: mapping?.includeWorklogs ?? true,
      includeAttachmentMetadata: mapping?.includeAttachmentMetadata ?? true,
      allowRemoteEmbedding: mapping?.allowRemoteEmbedding ?? false,
      allowRemoteAi: mapping?.allowRemoteAi ?? false,
      enabled: mapping?.enabled ?? true,
    });
    setMappingModalOpen(true);
  };

  const saveMapping = async () => {
    try {
      setSaving(true);
      const values = await mappingForm.validateFields();
      const {
        executionLines: _executionLines,
        releaseMappingText: _releaseMappingText,
        syncScopeText: _syncScopeText,
        releaseMapping: _ignoredReleaseMapping,
        syncScope: _ignoredSyncScope,
        ...mappingInput
      } = values;
      await knowledgeApi.upsertZentaoProjectMapping({
        ...mappingInput,
        remoteExecutionIds: newlineValues(values.executionLines),
        releaseMapping: JSON.parse(values.releaseMappingText || "{}") as Record<
          string,
          unknown
        >,
        syncScope: JSON.parse(values.syncScopeText || "{}") as Record<
          string,
          unknown
        >,
      });
      setMappingModalOpen(false);
      message.success("禅道项目映射已保存");
      await loadIntegrationViews();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const openZentaoAiSummaryModal = (mapping: ZentaoProjectMapping) => {
    setAiSummaryMappingId(mapping.id);
    aiSummaryForm.setFieldsValue({
      providerKey: "",
      model: "",
      prompt:
        "请基于已同步的需求、任务、测试与风险事实，概括当前版本的进展、实现证据和待补证据。",
    });
    setAiSummaryModalOpen(true);
  };

  const generateZentaoAiSummary = async () => {
    try {
      if (aiSummaryMappingId == null) return;
      setSaving(true);
      const values = await aiSummaryForm.validateFields();
      const result = await knowledgeApi.generateZentaoAiSummary({
        mappingId: aiSummaryMappingId,
        ...values,
      });
      message.success(
        `AI 摘要已生成，并保留 ${result.citationCount} 条可核验引用`,
      );
      setAiSummaryModalOpen(false);
      await Promise.all([loadCatalog(), loadIntegrationViews()]);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const openCodeSourceModal = (codeSource?: KnowledgeCodeSource) => {
    const source = codeSource?.source;
    codeSourceForm.setFieldsValue({
      source: {
        id: source?.id,
        sourceKey: source?.sourceKey ?? "",
        projectId: source?.projectId ?? catalogProjectId ?? null,
        sourceType: source?.sourceType ?? "local_directory",
        displayName: source?.displayName ?? "",
        rootPath: source?.rootPath ?? "",
        gitWorkspaceKey: source?.gitWorkspaceKey ?? "",
        includeGlobs: source?.includeGlobs ?? ["**/*"],
        excludeGlobs: source?.excludeGlobs ?? [],
        versionStrategy: source?.versionStrategy ?? "unversioned",
        syncMode: source?.syncMode ?? "manual",
        allowRemoteEmbedding: false,
        enabled: source?.enabled ?? true,
      },
      includeUntracked: codeSource?.settings.includeUntracked ?? false,
      maxFileSizeBytes: codeSource?.settings.maxFileSizeBytes ?? 1_048_576,
      allowedLanguages: codeSource?.settings.allowedLanguages ?? [
        "rust",
        "typescript",
        "javascript",
        "vue",
        "java",
        "sql",
        "markdown",
      ],
      allowRemoteProcessing: true,
      includeLines: valuesToLines(source?.includeGlobs ?? ["**/*"]),
      excludeLines: valuesToLines(source?.excludeGlobs ?? []),
    });
    setCodeSourceModalOpen(true);
    // 源码分析和常规知识来源共享已登记的 Git 工作区清单，避免手工输入不存在或
    // 已移除的标识而导致后端读取失败。
    void loadProjectGitWorkspaces();
  };

  const saveCodeSource = async () => {
    try {
      setSaving(true);
      const values = await codeSourceForm.validateFields();
      await knowledgeApi.upsertCodeSource({
        ...values,
        source: {
          ...values.source,
          includeGlobs: newlineValues(values.includeLines),
          excludeGlobs: newlineValues(values.excludeLines),
          // 这两个字段没有在源码来源表单单独展示，Ant Design 不会把未挂载字段
          // 写入 validateFields 结果；此处固定默认值以满足 Rust IPC 必填契约。
          versionStrategy: values.source.versionStrategy ?? "unversioned",
          syncMode: values.source.syncMode ?? "manual",
          allowRemoteEmbedding: false,
        },
        allowedLanguages: values.allowedLanguages,
        // 源码远程 AI 分析默认可用；远程 Embedding 保持独立配置，不能由此开关放宽。
        allowRemoteProcessing: true,
      });
      setCodeSourceModalOpen(false);
      message.success("源码知识源已保存；远程 AI 分析默认可用");
      await loadIntegrationViews();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const captureAndAnalyzeCodeSource = async (
    codeSource: KnowledgeCodeSource,
  ) => {
    if (codeAnalysisRunningRef.current) {
      message.info("已有源码捕获或分析任务正在执行，请等待完成后再试");
      return;
    }
    codeAnalysisRunningRef.current = true;
    const { source } = codeSource;
    const notificationKey = `knowledge-code-analysis-${source.id}`;
    let capturedSnapshot: KnowledgeCodeSnapshot | undefined;
    setCodeAnalysisState({
      sourceId: source.id,
      sourceName: source.displayName,
      stage: "capturing",
    });
    message.loading({
      key: notificationKey,
      content: `正在捕获“${source.displayName}”的代码快照…`,
      duration: 0,
    });

    try {
      capturedSnapshot =
        source.sourceType === "git_workspace"
          ? await knowledgeApi.captureGitSnapshot({
              sourceId: source.id,
              gitRef: "HEAD",
            })
          : await knowledgeApi.captureLocalDirectorySnapshot({
              sourceId: source.id,
            });
      setCodeAnalysisState({
        sourceId: source.id,
        sourceName: source.displayName,
        stage: "analyzing",
        snapshotId: capturedSnapshot.id,
      });
      message.loading({
        key: notificationKey,
        content: `快照已捕获，正在分析“${source.displayName}”…`,
        duration: 0,
      });

      const result = await knowledgeApi.analyzeCodeSnapshot(
        capturedSnapshot.id,
      );
      setCodeAnalysisState({
        sourceId: source.id,
        sourceName: source.displayName,
        stage: "completed",
        snapshotId: capturedSnapshot.id,
        result,
      });
      message.success({
        key: notificationKey,
        content: `分析完成：${result.analyzedFiles} 个文件、${result.symbols} 个代码元素、${result.documents} 份文档`,
      });
    } catch (error) {
      const errorMessage = getErrorMessage(error);
      setCodeAnalysisState({
        sourceId: source.id,
        sourceName: source.displayName,
        stage: "failed",
        snapshotId: capturedSnapshot?.id,
        error: errorMessage,
      });
      message.error({
        key: notificationKey,
        content: `${capturedSnapshot ? "源码分析" : "源码捕获"}失败：${errorMessage}`,
      });
    } finally {
      // 捕获成功后，即使分析失败也要刷新，才能在快照表格中即时显示 failed 和错误信息。
      if (capturedSnapshot) await loadIntegrationViews();
      codeAnalysisRunningRef.current = false;
    }
  };

  const analyzeExistingCodeSnapshot = async (
    snapshot: KnowledgeCodeSnapshot,
  ) => {
    if (codeAnalysisRunningRef.current) {
      message.info("已有源码捕获或分析任务正在执行，请等待完成后再试");
      return;
    }
    codeAnalysisRunningRef.current = true;
    const sourceName =
      codeSources.find((item) => item.source.id === snapshot.sourceId)?.source
        .displayName ?? `快照 #${snapshot.id}`;
    const notificationKey = `knowledge-code-analysis-${snapshot.id}`;
    setCodeAnalysisState({
      sourceId: snapshot.sourceId,
      sourceName,
      stage: "analyzing",
      snapshotId: snapshot.id,
    });
    message.loading({
      key: notificationKey,
      content: `正在分析“${sourceName}”的代码快照…`,
      duration: 0,
    });

    try {
      const result = await knowledgeApi.analyzeCodeSnapshot(snapshot.id);
      setCodeAnalysisState({
        sourceId: snapshot.sourceId,
        sourceName,
        stage: "completed",
        snapshotId: snapshot.id,
        result,
      });
      message.success({
        key: notificationKey,
        content: `分析完成：${result.analyzedFiles} 个文件、${result.symbols} 个代码元素、${result.documents} 份文档`,
      });
    } catch (error) {
      const errorMessage = getErrorMessage(error);
      setCodeAnalysisState({
        sourceId: snapshot.sourceId,
        sourceName,
        stage: "failed",
        snapshotId: snapshot.id,
        error: errorMessage,
      });
      message.error({
        key: notificationKey,
        content: `源码分析失败：${errorMessage}`,
      });
    } finally {
      // 分析失败由后端持久化为 failed；刷新后保留失败快照与重试入口。
      await loadIntegrationViews();
      codeAnalysisRunningRef.current = false;
    }
  };

  const projectColumns: TableColumnsType<KnowledgeProject> = [
    {
      title: "项目",
      dataIndex: "name",
      render: (value, row) => (
        <Button type="link" onClick={() => void selectCatalogProject(row.id)}>
          {value}
        </Button>
      ),
    },
    { title: "标识", dataIndex: "projectKey", width: 150 },
    {
      title: "别名",
      dataIndex: "aliases",
      render: (aliases: string[]) =>
        aliases.map((alias) => <Tag key={alias}>{alias}</Tag>),
    },
    {
      title: "Git 工作区",
      dataIndex: "gitWorkspaceKeys",
      render: (workspaceKeys: string[]) =>
        workspaceKeys.length > 0
          ? workspaceKeys.map((workspaceKey) => (
              <Tag key={workspaceKey} color="blue">
                {workspaceKey}
              </Tag>
            ))
          : "-",
    },
    {
      title: "状态",
      dataIndex: "enabled",
      width: 90,
      render: (enabled) => (
        <Tag color={enabled ? "green" : "default"}>
          {enabled ? "启用" : "停用"}
        </Tag>
      ),
    },
    {
      title: "操作",
      width: 160,
      render: (_, row) => (
        <Space size="small">
          <Button type="link" onClick={() => openProjectModal(row)}>
            编辑
          </Button>
          <Popconfirm
            title="将软删除该知识项目及其可见目录。继续吗？"
            onConfirm={() =>
              void knowledgeApi
                .deleteProject(row.id)
                .then(() => loadCatalog())
                .then(loadProjects)
                .catch((error) => message.error(getErrorMessage(error)))
            }
          >
            <Button type="link" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const releaseColumns: TableColumnsType<KnowledgeRelease> = [
    { title: "版本", dataIndex: "version" },
    {
      title: "Tag / 分支",
      render: (_, row) => row.tagName || row.branch || "未声明",
    },
    { title: "基线 Commit", dataIndex: "commitSha", ellipsis: true },
    {
      title: "操作",
      width: 160,
      render: (_, row) => (
        <Space size="small">
          <Button type="link" onClick={() => openReleaseModal(row)}>
            编辑
          </Button>
          <Popconfirm
            title="将软删除该发布版本。继续吗？"
            onConfirm={() =>
              void knowledgeApi
                .deleteRelease(row.id)
                .then(() => loadCatalog())
                .catch((error) => message.error(getErrorMessage(error)))
            }
          >
            <Button type="link" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const sourceColumns: TableColumnsType<KnowledgeSource> = [
    {
      title: "来源",
      dataIndex: "displayName",
      render: (value, row) => (
        <Space direction="vertical" size={0}>
          <Text>{value}</Text>
          <Text type="secondary" className="text-xs">
            {row.sourceKey}
          </Text>
        </Space>
      ),
    },
    {
      title: "类型",
      dataIndex: "sourceType",
      width: 130,
      render: (value) => <Tag>{knowledgeSourceTypeLabel(value)}</Tag>,
    },
    {
      title: "路径 / 工作区",
      render: (_, row) => row.rootPath || row.gitWorkspaceKey || "系统来源",
    },
    {
      title: "最近同步",
      render: (_, row) => {
        const syncStatus = knowledgeSyncStatus(row.lastSyncStatus);
        return (
          <Space direction="vertical" size={0}>
            <Tag color={syncStatus.color}>{syncStatus.label}</Tag>
            <Text type="secondary" className="text-xs">
              {row.lastSyncedAt ?? "-"}
            </Text>
          </Space>
        );
      },
    },
    {
      title: "操作",
      width: 220,
      render: (_, row) => (
        <Space size="small" wrap>
          <Button type="link" onClick={() => void startSourceSync(row)}>
            同步
          </Button>
          <Button type="link" onClick={() => openSourceModal(row)}>
            编辑
          </Button>
          <Popconfirm
            title="将软删除此来源，历史文档仍可追溯。继续吗？"
            onConfirm={() =>
              void knowledgeApi
                .deleteSource(row.id)
                .then(() => loadCatalog())
                .catch((error) => message.error(getErrorMessage(error)))
            }
          >
            <Button type="link" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const jobColumns: TableColumnsType<KnowledgeJob> = [
    {
      title: "任务",
      dataIndex: "jobType",
      render: (value, row) => (
        <Space direction="vertical" size={0}>
          <Text>{value}</Text>
          <Text type="secondary" className="text-xs">
            {row.jobKey}
          </Text>
        </Space>
      ),
    },
    {
      title: "进度",
      render: (_, row) => knowledgeJobProgressLabel(row),
    },
    {
      title: "状态",
      dataIndex: "status",
      render: (value) => {
        const status = knowledgeJobStatus(value);
        return <Tag color={status.color}>{status.label}</Tag>;
      },
    },
    { title: "信息", dataIndex: "message", ellipsis: true },
    {
      title: "操作",
      width: 140,
      render: (_, row) => (
        <Space size="small">
          {["queued", "running"].includes(row.status) && (
            <Button
              type="link"
              onClick={() =>
                void knowledgeApi
                  .cancelJob(row.jobKey)
                  .then(() => loadCatalog())
                  .catch((error) => message.error(getErrorMessage(error)))
              }
            >
              取消
            </Button>
          )}
          {["failed", "interrupted", "cancelled"].includes(row.status) && (
            <Button
              type="link"
              onClick={() =>
                void knowledgeApi
                  .retryJob(row.jobKey)
                  .then(() => loadCatalog())
                  .catch((error) => message.error(getErrorMessage(error)))
              }
            >
              重试
            </Button>
          )}
        </Space>
      ),
    },
  ];

  const profileColumns: TableColumnsType<KnowledgeEmbeddingProfile> = [
    {
      title: "向量化方案",
      dataIndex: "name",
      render: (value, row) => (
        <Space direction="vertical" size={0}>
          <Text>{value}</Text>
          <Text type="secondary" className="text-xs">
            {row.profileKey}
          </Text>
        </Space>
      ),
    },
    {
      title: "模式 / 模型",
      render: (_, row) => (
        <Space>
          <Tag color={row.mode === "local" ? "green" : "orange"}>
            {row.mode === "remote" ? "远程向量化" : "本地向量化"}
          </Tag>
          <Text>{row.model}</Text>
        </Space>
      ),
    },
    {
      title: "索引状态",
      render: (_, row) => (
        <Space>
          <Tag color={row.isActive ? "green" : "default"}>
            {row.isActive ? "当前活动" : row.status}
          </Tag>
          <Text type="secondary">{row.dimension} 维</Text>
        </Space>
      ),
    },
    {
      title: "操作",
      width: 300,
      render: (_, row) => (
        <Space size="small" wrap>
          <Button type="link" onClick={() => openProfileModal(row)}>
            编辑
          </Button>
          <Button
            type="primary"
            loading={
              embeddingWorkflowBusy && embeddingWorkflow?.profile.id === row.id
            }
            disabled={
              row.isActive ||
              row.status === "building" ||
              !embeddingWorkspaceAvailable ||
              embeddingWorkflowBusy ||
              Boolean(embeddingWorkflow)
            }
            onClick={() => void startEmbeddingWorkflow(row)}
          >
            {row.isActive ? "当前使用中" : "开始线性构建"}
          </Button>
          {!row.isActive && row.status === "ready" && (
            <Button
              type="link"
              onClick={() =>
                void knowledgeApi
                  .rollbackEmbeddingProfileRebuild(row.id)
                  .then(() => loadIntegrationViews())
                  .catch((error) => message.error(getErrorMessage(error)))
              }
            >
              回滚至此
            </Button>
          )}
        </Space>
      ),
    },
  ];

  const zentaoConnectionColumns: TableColumnsType<ZentaoConnection> = [
    {
      title: "连接",
      dataIndex: "name",
      render: (value, row) => (
        <Space direction="vertical" size={0}>
          <Text>{value}</Text>
          <Text type="secondary" className="text-xs">
            {row.baseUrl}
          </Text>
        </Space>
      ),
    },
    {
      title: "能力",
      render: (_, row) => (
        <Tag color={row.lastTestStatus === "success" ? "green" : "default"}>
          {row.lastTestStatus || "未探测"}
        </Tag>
      ),
    },
    {
      title: "操作",
      width: 280,
      render: (_, row) => (
        <Space size="small" wrap>
          <Button
            type="link"
            onClick={() => void probeZentaoConnection(row.id)}
          >
            探测
          </Button>
          <Button type="link" onClick={() => void discoverZentaoScopes(row.id)}>
            发现范围
          </Button>
          <Button type="link" onClick={() => openZentaoModal(row)}>
            编辑
          </Button>
          <Popconfirm
            title="将禁用连接和关联映射，保留历史事实。继续吗？"
            onConfirm={() =>
              void knowledgeApi
                .deleteZentaoConnection(row.id)
                .then(loadIntegrationViews)
                .catch((error) => message.error(getErrorMessage(error)))
            }
          >
            <Button type="link" danger>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const zentaoMappingColumns: TableColumnsType<ZentaoProjectMapping> = [
    {
      title: "远程项目",
      dataIndex: "remoteProjectId",
      render: (value, row) => (
        <Space direction="vertical" size={0}>
          <Text>{value}</Text>
          <Text type="secondary" className="text-xs">
            本地项目 #{row.knowledgeProjectId}
          </Text>
        </Space>
      ),
    },
    {
      title: "同步范围",
      render: (_, row) => (
        <Text ellipsis={{ tooltip: JSON.stringify(row.syncScope) }}>
          {JSON.stringify(row.syncScope)}
        </Text>
      ),
    },
    {
      title: "操作",
      width: 320,
      render: (_, row) => (
        <Space size="small" wrap>
          <Button
            type="link"
            onClick={() =>
              void knowledgeApi
                .syncZentaoMapping({ mappingId: row.id, entityTypes: [] })
                .then(() => message.success("禅道同步完成"))
                .then(() => loadIntegrationViews())
                .catch((error) => message.error(getErrorMessage(error)))
            }
          >
            同步
          </Button>
          <Button
            type="link"
            onClick={() =>
              void knowledgeApi
                .generateZentaoFactDocuments({ mappingId: row.id })
                .then((result) =>
                  message.success(
                    `已生成 ${result.generatedDocumentVersionIds.length} 个事实文档`,
                  ),
                )
                .then(() => loadCatalog())
                .catch((error) => message.error(getErrorMessage(error)))
            }
          >
            生成事实文档
          </Button>
          <Button
            type="link"
            disabled={!row.allowRemoteAi}
            onClick={() => openZentaoAiSummaryModal(row)}
          >
            AI 摘要
          </Button>
          <Button type="link" onClick={() => openMappingModal(row)}>
            编辑
          </Button>
        </Space>
      ),
    },
  ];

  const codeSourceColumns: TableColumnsType<KnowledgeCodeSource> = [
    {
      title: "源码来源",
      render: (_, row) => (
        <Space direction="vertical" size={0}>
          <Text>{row.source.displayName}</Text>
          <Text type="secondary" className="text-xs">
            {row.source.rootPath || row.source.gitWorkspaceKey}
          </Text>
        </Space>
      ),
    },
    {
      title: "语言 / 范围",
      render: (_, row) => (
        <Text>{row.settings.allowedLanguages.join(", ") || "自动识别"}</Text>
      ),
    },
    {
      title: "操作",
      width: 300,
      render: (_, row) => {
        const isAnalyzingThisSource =
          codeAnalysisState?.sourceId === row.source.id &&
          (codeAnalysisState.stage === "capturing" ||
            codeAnalysisState.stage === "analyzing");
        const isCodeAnalysisRunning =
          codeAnalysisState?.stage === "capturing" ||
          codeAnalysisState?.stage === "analyzing";
        return (
          <Space size="small" wrap>
            <Button
              type="link"
              onClick={() =>
                void knowledgeApi
                  .previewCodeSourceScope(row.source.id)
                  .then((preview) =>
                    message.info(
                      `有效范围 ${preview.includedFiles} 文件，跳过 ${preview.skippedEntries} 条`,
                    ),
                  )
                  .catch((error) => message.error(getErrorMessage(error)))
              }
            >
              预览
            </Button>
            <Button
              type="link"
              loading={isAnalyzingThisSource}
              disabled={isCodeAnalysisRunning}
              onClick={() => void captureAndAnalyzeCodeSource(row)}
            >
              {isAnalyzingThisSource
                ? codeAnalysisState.stage === "capturing"
                  ? "正在捕获"
                  : "正在分析"
                : "捕获并分析"}
            </Button>
            <Button type="link" onClick={() => openCodeSourceModal(row)}>
              编辑
            </Button>
          </Space>
        );
      },
    },
  ];

  const codeSnapshotColumns: TableColumnsType<KnowledgeCodeSnapshot> = [
    {
      title: "代码来源 / 快照",
      render: (_, row) => (
        <Space direction="vertical" size={0}>
          <Text>{codeSnapshotSourceLabel(row, codeSources)}</Text>
          <Text type="secondary" className="text-xs">
            {row.snapshotType} ·{" "}
            {row.refName || row.commitSha || row.capturedAt}
          </Text>
        </Space>
      ),
    },
    {
      title: "状态",
      render: (_, row) => {
        const status = knowledgeCodeSnapshotStatus(row.status);
        return (
          <Space>
            <Tag color={status.color}>{status.label}</Tag>
            <Text type="secondary">
              {row.symbolCount} 个代码元素 / {row.relationCount} 条关系
            </Text>
          </Space>
        );
      },
    },
    {
      title: "操作",
      width: 320,
      render: (_, row) => {
        const isAnalyzingThisSnapshot =
          codeAnalysisState?.snapshotId === row.id &&
          codeAnalysisState.stage === "analyzing";
        const isCodeAnalysisRunning =
          codeAnalysisState?.stage === "capturing" ||
          codeAnalysisState?.stage === "analyzing";
        return (
          <Space size="small" wrap>
            <Button
              type="link"
              loading={isAnalyzingThisSnapshot}
              disabled={isCodeAnalysisRunning}
              onClick={() => void analyzeExistingCodeSnapshot(row)}
            >
              {isAnalyzingThisSnapshot ? "正在分析" : "分析"}
            </Button>
            <Button
              type="link"
              onClick={() =>
                void knowledgeApi
                  .generateCodeDocuments({ snapshotId: row.id })
                  .then((result) =>
                    message.success(
                      `生成 ${result.generatedDocumentVersionIds.length} 份工程文档`,
                    ),
                  )
                  .catch((error) => message.error(getErrorMessage(error)))
              }
            >
              生成文档
            </Button>
            <Button
              type="link"
              disabled={row.status !== "analyzed"}
              onClick={() => void selectCodeSnapshot(row.id)}
            >
              代码元素与关系
            </Button>
          </Space>
        );
      },
    },
  ];

  return (
    <div className="w-full space-y-4 p-1">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <Title level={2} className="mb-1!">
            团队知识库
          </Title>
          <Text type="secondary">
            按项目和版本检索需求、设计、禅道事实与代码证据；回答只基于可追溯来源。
          </Text>
        </div>
        <Space wrap>
          <Button
            icon={<RefreshCw size={16} />}
            onClick={() => void refresh()}
            loading={loading}
          >
            刷新目录
          </Button>
        </Space>
      </div>

      <Card
        size="small"
        title={
          <Space>
            <Sparkles size={17} />
            知识问答
          </Space>
        }
      >
        <Space direction="vertical" size="middle" className="w-full">
          <Input.TextArea
            value={search.query}
            rows={3}
            maxLength={2000}
            placeholder="例如：我想了解某某项目 v1.6.0 当初的需求、具体实现方案和测试证据"
            onChange={(event) =>
              setSearch((current) => ({
                ...current,
                query: event.target.value,
              }))
            }
          />
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-4">
            <Select
              mode="multiple"
              allowClear
              placeholder="限定项目"
              options={projectOptions}
              value={search.projectIds}
              onChange={(value) => void changeProjects(value)}
            />
            <Select
              mode="multiple"
              allowClear
              disabled={search.projectIds.length !== 1}
              placeholder="限定发布版本"
              options={releaseOptions}
              value={search.releaseIds}
              onChange={(releaseIds) => {
                setStoredReleaseIds(releaseIds);
                setSearch((current) => ({ ...current, releaseIds }));
              }}
            />
            <Select
              allowClear
              aria-label="AI 服务商"
              showSearch
              optionFilterProp="label"
              placeholder="选择 AI 服务商"
              value={providerKey || undefined}
              options={knowledgeChatProviderOptions}
              onChange={changeKnowledgeChatProvider}
              notFoundContent="暂无可用于知识问答的 AI 服务商，请先在 AI Provider 中完成配置"
            />
            <Input
              placeholder="模型名称"
              value={model}
              onChange={(event) => setModel(event.target.value)}
            />
          </div>
          <Space wrap>
            <Button
              icon={<Eye size={16} />}
              onClick={() => void previewContext()}
              loading={asking}
            >
              预览证据上下文
            </Button>
            <Button
              type="primary"
              icon={<Send size={16} />}
              onClick={() => void ask()}
              loading={asking}
            >
              基于证据回答
            </Button>
            <Button onClick={() => void loadDocuments(search)}>
              按条件刷新文档
            </Button>
          </Space>
        </Space>
      </Card>

      {(preview || answer) && (
        <Card title={answer ? "问答结果" : "将发送给模型的证据上下文"}>
          <Space direction="vertical" size="large" className="w-full">
            {answer && (
              <Paragraph className="mb-0 whitespace-pre-wrap">
                {answer.answer}
              </Paragraph>
            )}
            {preview && !answer && (
              <Paragraph
                className="mb-0 whitespace-pre-wrap"
                ellipsis={{ rows: 14, expandable: "collapsible" }}
              >
                {preview.context}
              </Paragraph>
            )}
            <Descriptions
              size="small"
              column={3}
              title="召回通道"
              items={Object.entries(
                (
                  answer?.retrievalDiagnostics ??
                  preview?.retrievalDiagnostics ??
                  {}
                ).channels ?? {},
              ).map(([channel, value]) => ({
                key: channel,
                label: channel.toUpperCase(),
                children: `${String((value as Record<string, unknown>).status ?? "unknown")} · ${String((value as Record<string, unknown>).candidates ?? 0)} 条`,
              }))}
            />
            {(answer?.conflicts ?? preview?.conflicts ?? []).map((conflict) => (
              <Alert
                key={conflict}
                type="warning"
                showIcon
                message="来源存在冲突"
                description={conflict}
              />
            ))}
            {(answer?.evidenceGaps ?? preview?.evidenceGaps ?? []).map(
              (gap) => (
                <Alert
                  key={gap}
                  type="info"
                  showIcon
                  message="证据缺口"
                  description={gap}
                />
              ),
            )}
            <div>
              <Text strong>引用证据</Text>
              <div className="mt-2">
                <CitationList
                  citations={answer?.citations ?? preview?.citations ?? []}
                  onOpen={(citation) => void openCitation(citation)}
                />
              </div>
            </div>
          </Space>
        </Card>
      )}

      <Tabs
        activeKey={activeCatalogTab}
        onChange={setActiveCatalogTab}
        items={[
          {
            key: "documents",
            label: (
              <span>
                <FileText size={15} className="mr-1 inline" />
                文档（{documents.total}）
              </span>
            ),
            children: (
              <Spin spinning={loading}>
                {documentDirectoryTree.length > 0 ? (
                  <Tree
                    blockNode
                    defaultExpandedKeys={documentDirectoryTree.map(
                      (node) => node.key,
                    )}
                    showIcon
                    treeData={documentDirectoryTree}
                    aria-label="知识文档目录"
                    onSelect={(_, info) => {
                      const documentId = (info.node as DocumentDirectoryNode)
                        .documentId;
                      if (documentId != null) void openDocument(documentId);
                    }}
                  />
                ) : (
                  <Empty description="暂无符合条件的知识文档" />
                )}
              </Spin>
            ),
          },
          {
            key: "selected",
            label: (
              <span>
                <BookOpenCheck size={15} className="mr-1 inline" />
                文档详情
              </span>
            ),
            children: selectedDocument ? (
              <Space direction="vertical" className="w-full" size="middle">
                <Descriptions
                  bordered
                  size="small"
                  column={2}
                  items={[
                    {
                      key: "path",
                      label: "逻辑路径",
                      children: selectedDocument.document.logicalPath,
                    },
                    {
                      key: "sensitivity",
                      label: "敏感级别",
                      children: selectedDocument.document.sensitivity,
                    },
                  ]}
                />
                <Collapse
                  items={selectedDocument.versions.map((version) => ({
                    key: version.id,
                    label: `${version.versionLabel || "未版本化"} · ${version.commitSha || "本地内容"}`,
                    children: (
                      <DocumentContentPreview
                        document={selectedDocument.document}
                        version={version}
                      />
                    ),
                  }))}
                />
              </Space>
            ) : (
              <Empty description="从文档列表选择一个文档查看版本内容" />
            ),
          },
          {
            key: "catalog",
            label: (
              <span>
                <Layers3 size={15} className="mr-1 inline" />
                项目与版本
              </span>
            ),
            children: (
              <Space direction="vertical" size="middle" className="w-full">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <Select
                    allowClear
                    className="min-w-64"
                    placeholder="选择项目查看版本和来源"
                    options={projectOptions}
                    value={catalogProjectId}
                    onChange={(value) => void selectCatalogProject(value)}
                  />
                  <Space>
                    <Button
                      icon={<Plus size={15} />}
                      onClick={() => openProjectModal()}
                    >
                      新建项目
                    </Button>
                    <Button
                      type="primary"
                      icon={<Plus size={15} />}
                      disabled={!catalogProjectId}
                      onClick={() => openReleaseModal()}
                    >
                      新建版本
                    </Button>
                  </Space>
                </div>
                <Table
                  rowKey="id"
                  size="small"
                  columns={projectColumns}
                  dataSource={projects}
                  pagination={false}
                  locale={{ emptyText: "尚未登记知识项目" }}
                />
                <Card
                  size="small"
                  title={
                    catalogProjectId ? "已选项目的发布版本" : "请先选择一个项目"
                  }
                  extra={
                    catalogProjectId && (
                      <Button type="link" onClick={() => openReleaseModal()}>
                        登记版本
                      </Button>
                    )
                  }
                >
                  <Table
                    rowKey="id"
                    size="small"
                    columns={releaseColumns}
                    dataSource={catalogReleases}
                    pagination={false}
                    locale={{
                      emptyText:
                        "尚未登记发布版本；不能将未识别内容自动归入最新版本。",
                    }}
                  />
                </Card>
              </Space>
            ),
          },
          {
            key: "sources",
            label: (
              <span>
                <FolderSync size={15} className="mr-1 inline" />
                来源与同步
              </span>
            ),
            children: (
              <Space direction="vertical" size="middle" className="w-full">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <Text type="secondary">
                    来源仅能读取后端校验后的授权根目录；范围预览不会上传文件内容。
                  </Text>
                  <Space>
                    <Select
                      allowClear
                      className="min-w-56"
                      disabled={!catalogProjectId}
                      placeholder={
                        catalogProjectId ? "同步绑定项目版本" : "请先选择项目"
                      }
                      options={catalogReleases.map((release) => ({
                        value: release.id,
                        label: release.version || "未版本化",
                      }))}
                      value={catalogReleaseId}
                      onChange={setCatalogReleaseId}
                      notFoundContent="当前项目暂无发布版本"
                    />
                    <Button onClick={() => void loadCatalog()}>刷新任务</Button>
                    <Button
                      type="primary"
                      icon={<Plus size={15} />}
                      onClick={() => openSourceModal()}
                    >
                      添加来源
                    </Button>
                  </Space>
                </div>
                <Table
                  rowKey="id"
                  size="small"
                  columns={sourceColumns}
                  dataSource={sources}
                  pagination={false}
                  locale={{ emptyText: "当前项目尚未登记知识来源" }}
                />
                <Card
                  size="small"
                  title="同步任务"
                  extra={
                    <Button type="link" onClick={() => void loadCatalog()}>
                      刷新
                    </Button>
                  }
                >
                  <Table
                    rowKey="id"
                    size="small"
                    columns={jobColumns}
                    dataSource={jobs}
                    pagination={false}
                    locale={{ emptyText: "暂无知识同步或索引任务" }}
                  />
                </Card>
              </Space>
            ),
          },
          {
            key: "embedding",
            label: (
              <span>
                <Settings2 size={15} className="mr-1 inline" />
                向量索引
              </span>
            ),
            children: (
              <Space direction="vertical" size="middle" className="w-full">
                <Alert
                  type="info"
                  showIcon
                  title="当前设备的全局本地索引配置"
                  description="向量化方案及其构建或重建作用于当前设备的全局本地索引，不限于当前项目。项目问答与检索仍会按所选项目和版本过滤。"
                />
                <Alert
                  type="info"
                  showIcon
                  title="向量化方案切换采用蓝绿重建"
                  description="新方案先构建和校验，激活成功前始终由当前活动索引提供检索；本地失败不会自动切换到远程服务商。"
                />
                <Card
                  size="small"
                  title="本地模型运行时与离线缓存"
                  extra={
                    <Button
                      type="link"
                      onClick={() => void loadIntegrationViews()}
                    >
                      刷新状态
                    </Button>
                  }
                >
                  <Space direction="vertical" className="w-full" size="middle">
                    <Descriptions
                      size="small"
                      column={3}
                      items={[
                        {
                          key: "runtime",
                          label: "运行时",
                          children: localEmbeddingRuntime?.runtime ?? "加载中",
                        },
                        {
                          key: "available",
                          label: "可用",
                          children: localEmbeddingRuntime?.runtimeAvailable
                            ? "是"
                            : "否",
                        },
                        {
                          key: "download",
                          label: "自动下载",
                          children:
                            localEmbeddingRuntime?.automaticDownloadEnabled
                              ? "已开启"
                              : "不会自动下载",
                        },
                        {
                          key: "cache",
                          label: "缓存目录",
                          span: 3,
                          children: localEmbeddingRuntime?.cacheDir ?? "-",
                        },
                      ]}
                    />
                    {localEmbeddingRuntime?.warnings.map((warning) => (
                      <Alert
                        key={warning}
                        type="warning"
                        showIcon
                        message={warning}
                      />
                    ))}
                    <Form
                      form={localEmbeddingForm}
                      layout="inline"
                      className="flex flex-wrap gap-y-2"
                    >
                      <Form.Item
                        name="modelKey"
                        label="模型标识"
                        rules={[{ required: true, message: "请输入模型标识" }]}
                      >
                        <Input placeholder="例如 multilingual-e5-small-int8" />
                      </Form.Item>
                      <Form.Item name="sourcePath" label="离线模型路径">
                        <Input placeholder="离线包路径" />
                      </Form.Item>
                      <Form.Item
                        name="expectedSha256"
                        label="SHA-256 校验值（必填）"
                        rules={[
                          {
                            required: true,
                            message: "请输入模型发布方提供的 64 位 SHA-256",
                          },
                          {
                            len: 64,
                            message: "SHA-256 必须是 64 位十六进制字符",
                          },
                          {
                            pattern: /^[a-fA-F0-9]+$/,
                            message: "SHA-256 只能包含十六进制字符",
                          },
                        ]}
                      >
                        <Input placeholder="请输入 64 位十六进制校验值" />
                      </Form.Item>
                      <Space wrap>
                        <Button
                          loading={saving}
                          onClick={() => void importLocalEmbeddingModel()}
                        >
                          离线导入
                        </Button>
                        <Button
                          loading={saving}
                          onClick={() => void downloadLocalEmbeddingModel()}
                        >
                          从内部镜像下载
                        </Button>
                      </Space>
                    </Form>
                    {(localEmbeddingRuntime?.cachedModels.length ?? 0) > 0 && (
                      <Table
                        rowKey="modelKey"
                        size="small"
                        pagination={false}
                        dataSource={localEmbeddingRuntime?.cachedModels}
                        columns={[
                          { title: "模型", dataIndex: "modelKey" },
                          {
                            title: "大小",
                            dataIndex: "sizeBytes",
                            render: (value) => `${value} B`,
                          },
                          {
                            title: "校验值",
                            dataIndex: "sha256",
                            ellipsis: true,
                          },
                          {
                            title: "操作",
                            render: (_, row) => (
                              <Popconfirm
                                title={`删除本地缓存模型 ${row.modelKey}？`}
                                onConfirm={() =>
                                  void knowledgeApi
                                    .removeLocalEmbeddingModel(row.modelKey)
                                    .then(loadIntegrationViews)
                                    .catch((error) =>
                                      message.error(getErrorMessage(error)),
                                    )
                                }
                              >
                                <Button type="link" danger>
                                  清理缓存
                                </Button>
                              </Popconfirm>
                            ),
                          },
                        ]}
                      />
                    )}
                  </Space>
                </Card>
                <div className="flex justify-end">
                  <Button
                    type="primary"
                    icon={<Plus size={15} />}
                    disabled={!embeddingWorkspaceAvailable}
                    onClick={() => openProfileModal()}
                  >
                    新建向量化方案
                  </Button>
                </div>
                <Table
                  rowKey="id"
                  size="small"
                  columns={profileColumns}
                  dataSource={profiles}
                  pagination={false}
                  locale={{
                    emptyText: "尚未配置向量化方案；可先使用 FTS 检索。",
                  }}
                />
                {embeddingEstimate && (
                  <Descriptions
                    bordered
                    size="small"
                    title="重建估算"
                    column={3}
                    items={[
                      {
                        key: "chunks",
                        label: "待向量化片段",
                        children: embeddingEstimate.chunksToEmbed,
                      },
                      {
                        key: "disk",
                        label: "额外磁盘",
                        children: `${embeddingEstimate.additionalDiskBytes} B`,
                      },
                      {
                        key: "remote",
                        label: "远程确认",
                        children: embeddingEstimate.requiresRemoteConfirmation
                          ? "需要"
                          : "不需要",
                      },
                    ]}
                  />
                )}
              </Space>
            ),
          },
          {
            key: "zentao",
            label: (
              <span>
                <FolderSync size={15} className="mr-1 inline" />
                禅道同步
              </span>
            ),
            children: (
              <Space direction="vertical" size="middle" className="w-full">
                <Alert
                  type="warning"
                  showIcon
                  title="先探测能力，再选择实体同步"
                  description="不同禅道版本和权限支持的 API 不同。页面不会猜测未探测到的需求变更、工时、测试或发布接口。"
                />
                <div className="flex flex-wrap justify-end gap-2">
                  <Button onClick={() => openMappingModal()}>新建映射</Button>
                  <Button
                    type="primary"
                    icon={<Plus size={15} />}
                    onClick={() => openZentaoModal()}
                  >
                    新建连接
                  </Button>
                </div>
                <Card size="small" title="只读连接">
                  <Table
                    rowKey="id"
                    size="small"
                    columns={zentaoConnectionColumns}
                    dataSource={zentaoConnections}
                    pagination={false}
                    locale={{ emptyText: "尚未配置禅道连接" }}
                  />
                </Card>
                {selectedZentaoProbe && (
                  <Card size="small" title="最近能力矩阵">
                    <Descriptions
                      bordered
                      size="small"
                      column={2}
                      items={[
                        {
                          key: "profile",
                          label: "端点配置",
                          children: selectedZentaoProbe.endpointProfile,
                        },
                        {
                          key: "version",
                          label: "API 版本",
                          children: selectedZentaoProbe.apiVersion,
                        },
                        {
                          key: "auth",
                          label: "认证模式",
                          children: selectedZentaoProbe.authMode,
                        },
                        {
                          key: "status",
                          label: "状态",
                          children: selectedZentaoProbe.status,
                        },
                        {
                          key: "entities",
                          label: "可同步实体",
                          span: 2,
                          children: (
                            (selectedZentaoProbe.capabilities.entities as
                              string[] | undefined) ?? []
                          ).map((item) => <Tag key={item}>{item}</Tag>),
                        },
                      ]}
                    />
                  </Card>
                )}
                {zentaoScopes.length > 0 && (
                  <Card size="small" title="远程项目树（待人工映射）">
                    <Table
                      rowKey={(row) => `${row.entityType}:${row.externalId}`}
                      size="small"
                      pagination={false}
                      dataSource={zentaoScopes}
                      columns={[
                        { title: "类型", dataIndex: "entityType" },
                        { title: "远程 ID", dataIndex: "externalId" },
                        { title: "名称", dataIndex: "name" },
                        { title: "父级", dataIndex: "parentExternalId" },
                      ]}
                    />
                  </Card>
                )}
                <Card size="small" title="项目 / 版本映射">
                  <Table
                    rowKey="id"
                    size="small"
                    columns={zentaoMappingColumns}
                    dataSource={zentaoMappings}
                    pagination={false}
                    locale={{ emptyText: "请将远程项目显式映射到本地知识项目" }}
                  />
                </Card>
              </Space>
            ),
          },
          {
            key: "code",
            label: (
              <span>
                <GitBranch size={15} className="mr-1 inline" />
                源码知识
              </span>
            ),
            children: (
              <Space direction="vertical" size="middle" className="w-full">
                <Alert
                  type="info"
                  showIcon
                  title="源码捕获始终只读"
                  description="Git 历史通过对象读取，工作树和本地目录经授权路径与内容哈希校验；不会 checkout、stash、reset 或执行任何被分析代码。"
                />
                {codeAnalysisState && (
                  <Card size="small" title="本次捕获与分析状态">
                    <Space direction="vertical" size="small" className="w-full">
                      <Steps
                        size="small"
                        current={
                          codeAnalysisState.stage === "capturing"
                            ? 0
                            : codeAnalysisState.stage === "analyzing"
                              ? 1
                              : 2
                        }
                        status={
                          codeAnalysisState.stage === "failed"
                            ? "error"
                            : codeAnalysisState.stage === "completed"
                              ? "finish"
                              : "process"
                        }
                        items={[
                          { title: "捕获代码快照" },
                          { title: "分析源码结构" },
                          { title: "刷新结果" },
                        ]}
                      />
                      {codeAnalysisState.stage === "capturing" && (
                        <Text>
                          正在捕获“{codeAnalysisState.sourceName}
                          ”的代码快照，请稍候。
                        </Text>
                      )}
                      {codeAnalysisState.stage === "analyzing" && (
                        <Text>
                          快照 #{codeAnalysisState.snapshotId}{" "}
                          已捕获，正在分析源码结构。
                        </Text>
                      )}
                      {codeAnalysisState.stage === "completed" &&
                        codeAnalysisState.result && (
                          <Text type="success">
                            已完成：分析{" "}
                            {codeAnalysisState.result.analyzedFiles}{" "}
                            个文件，发现 {codeAnalysisState.result.symbols}{" "}
                            个代码元素，生成{" "}
                            {codeAnalysisState.result.documents} 份文档；跳过{" "}
                            {codeAnalysisState.result.skippedFiles} 个文件。
                          </Text>
                        )}
                      {codeAnalysisState.stage === "failed" && (
                        <Text type="danger">
                          捕获或分析失败：
                          {codeAnalysisState.error ?? "未知错误"}
                        </Text>
                      )}
                    </Space>
                  </Card>
                )}
                <div className="flex justify-end">
                  <Button
                    type="primary"
                    icon={<Plus size={15} />}
                    onClick={() => openCodeSourceModal()}
                  >
                    添加源码来源
                  </Button>
                </div>
                <Card size="small" title="代码来源">
                  <Table
                    rowKey={(row) => row.source.id}
                    size="small"
                    columns={codeSourceColumns}
                    dataSource={codeSources}
                    pagination={false}
                    locale={{
                      emptyText: "尚未配置授权的 Git 工作区或本地代码目录",
                    }}
                  />
                </Card>
                <Card size="small" title="快照、代码元素与工程文档">
                  <Table
                    rowKey="id"
                    size="small"
                    columns={codeSnapshotColumns}
                    dataSource={codeSnapshots}
                    pagination={false}
                    locale={{
                      emptyText: "捕获并分析代码来源后将在此显示历史快照",
                    }}
                  />
                </Card>
                <Card size="small" title="快照内代码元素、关系与影响分析">
                  <Space direction="vertical" size="middle" className="w-full">
                    <Alert
                      type="info"
                      showIcon
                      title="什么是代码元素？"
                      description="代码元素是代码中可识别的组成部分，例如类、接口、枚举、方法和函数。选择一个代码元素后，可以查看它与其他代码元素的关联及潜在影响。"
                    />
                    <div className="grid grid-cols-1 gap-2 lg:grid-cols-4">
                      <Select
                        allowClear
                        placeholder="选择已分析快照"
                        value={selectedCodeSnapshotId}
                        onChange={(value) => void selectCodeSnapshot(value)}
                        options={codeSnapshots
                          .filter((item) => item.status === "analyzed")
                          .map((item) => ({
                            value: item.id,
                            label: codeSnapshotOptionLabel(item, codeSources),
                          }))}
                      />
                      <Select
                        allowClear
                        showSearch
                        optionFilterProp="label"
                        placeholder="选择代码元素（类、接口、方法等）"
                        value={selectedCodeSymbolKey}
                        onChange={selectCodeSymbol}
                        options={codeSymbols.map((item) => ({
                          value: item.symbolKey,
                          label: `${item.qualifiedName} · L${item.startLine}`,
                        }))}
                      />
                      <Button
                        disabled={!selectedCodeSymbolKey}
                        onClick={() => void showCodeGraph(false)}
                      >
                        查看关联关系
                      </Button>
                      <Button
                        disabled={!selectedCodeSymbolKey}
                        onClick={() => void showCodeGraph(true)}
                      >
                        影响分析
                      </Button>
                    </div>
                    <Card size="small" title="仓库文件树与只读代码查看">
                      <Space
                        direction="vertical"
                        className="w-full"
                        size="small"
                      >
                        {selectedCodeSnapshotId != null &&
                          codeFileSummary.totalFiles > 0 && (
                            <Alert
                              type={
                                codeFileSummary.skippedFiles > 0
                                  ? "warning"
                                  : "success"
                              }
                              showIcon
                              title={`快照共 ${codeFileSummary.totalFiles} 个文件：可读取 ${codeFileSummary.readableFiles} 个，已跳过 ${codeFileSummary.skippedFiles} 个`}
                              description={
                                codeFileSummary.redactedFiles > 0
                                  ? `其中 ${codeFileSummary.redactedFiles} 个文件已移除敏感值后建立索引；跳过文件仍显示原因，但不能读取正文。`
                                  : "跳过文件仍显示原因，但不能读取正文。"
                              }
                            />
                          )}
                        <Select
                          allowClear
                          showSearch
                          optionFilterProp="label"
                          placeholder="从已分析快照选择文件"
                          value={selectedCodeFile?.file.id}
                          onChange={(value) => void openCodeFile(value)}
                          options={codeFiles.map((file) => ({
                            value: file.id,
                            disabled: !isKnowledgeCodeFileReadable(file),
                            label: `${file.relativePath} · ${file.language} · ${
                              isKnowledgeCodeFileReadable(file)
                                ? file.skipReason.startsWith(
                                    "redacted_sensitive_content:",
                                  )
                                  ? knowledgeCodeFileReasonLabel(
                                      file.skipReason,
                                    )
                                  : "可读取"
                                : `已跳过：${knowledgeCodeFileReasonLabel(file.skipReason)}`
                            }`,
                          }))}
                        />
                        {selectedCodeFile ? (
                          <>
                            <Text type="secondary">
                              {selectedCodeFile.file.relativePath} ·{" "}
                              {knowledgeCodeAnalysisLevel(
                                selectedCodeFile.file.analysisLevel,
                              )}
                              {selectedCodeFile.file.isTest
                                ? " · 测试文件"
                                : ""}
                            </Text>
                            <KnowledgeCodeFilePreview {...selectedCodeFile} />
                          </>
                        ) : (
                          <Text type="secondary">
                            可读取文件支持查看脱敏后的正文；受限、跳过或已失效文件只展示安全元数据与原因。
                          </Text>
                        )}
                      </Space>
                    </Card>
                    {(codeGraph || codeImpact) &&
                      (() => {
                        const graph = codeImpact ?? codeGraph;
                        if (!graph) return null;
                        const isImpact = codeImpact != null;
                        return (
                          <Card
                            size="small"
                            title={isImpact ? "影响分析结果" : "关系图"}
                          >
                            <Space
                              direction="vertical"
                              size="small"
                              className="w-full"
                            >
                              <Text type="secondary">
                                已展示 {graph.nodes.length} 个代码元素、
                                {graph.edges.length} 条关系
                                {graph.truncated ? "；结果已按上限截断" : ""}
                              </Text>
                              {graph.edges.length === 0 ? (
                                <Alert
                                  type="info"
                                  showIcon
                                  title="当前代码元素没有可展示的关系"
                                  description={
                                    isImpact
                                      ? "未发现该代码元素的上游影响关系。可改选其他代码元素后重试。"
                                      : "关联关系默认只显示已确认的出向关系；快照总关系数不代表每个代码元素都有出向关系。可改选其他代码元素，或使用“影响分析”查看上游关联。"
                                  }
                                />
                              ) : (
                                <Table
                                  rowKey="id"
                                  size="small"
                                  pagination={false}
                                  dataSource={graph.edges}
                                  columns={[
                                    {
                                      title: "来源代码元素",
                                      dataIndex: "fromSymbolKey",
                                      ellipsis: true,
                                    },
                                    {
                                      title: "关系",
                                      dataIndex: "relationType",
                                    },
                                    {
                                      title: "目标代码元素",
                                      dataIndex: "toSymbolKey",
                                      ellipsis: true,
                                    },
                                    {
                                      title: "置信度",
                                      dataIndex: "confidence",
                                    },
                                    {
                                      title: "状态",
                                      dataIndex: "confirmed",
                                      render: (value) => (
                                        <Tag
                                          color={value ? "green" : "default"}
                                        >
                                          {value ? "已确认" : "候选"}
                                        </Tag>
                                      ),
                                    },
                                  ]}
                                />
                              )}
                            </Space>
                          </Card>
                        );
                      })()}
                    <div className="grid grid-cols-1 gap-2 lg:grid-cols-3">
                      <Select
                        allowClear
                        placeholder="比较起点"
                        value={comparisonFromSnapshotId}
                        options={codeSnapshots.map((item) => ({
                          value: item.id,
                          label: codeSnapshotOptionLabel(item, codeSources),
                        }))}
                        onChange={setComparisonFromSnapshotId}
                      />
                      <Select
                        allowClear
                        placeholder="比较终点"
                        value={comparisonToSnapshotId}
                        options={codeSnapshots.map((item) => ({
                          value: item.id,
                          label: codeSnapshotOptionLabel(item, codeSources),
                        }))}
                        onChange={setComparisonToSnapshotId}
                      />
                      <Button
                        onClick={() =>
                          void compareSelectedCodeSnapshots(
                            comparisonFromSnapshotId,
                            comparisonToSnapshotId,
                          )
                        }
                      >
                        比较快照
                      </Button>
                    </div>
                    {codeComparison && (
                      <Alert
                        type="info"
                        showIcon
                        message={`快照差异：新增 ${codeComparison.addedSymbolKeys.length} 个代码元素，移除 ${codeComparison.removedSymbolKeys.length} 个代码元素，文件变更 ${codeComparison.fileChanges.length} 项`}
                      />
                    )}
                  </Space>
                </Card>
              </Space>
            ),
          },
        ]}
      />

      <Modal
        title="向量索引构建向导"
        open={Boolean(embeddingWorkflow)}
        closable={!embeddingWorkflowBusy}
        mask={{ closable: !embeddingWorkflowBusy }}
        onCancel={() => {
          if (!embeddingWorkflowBusy) setEmbeddingWorkflow(null);
        }}
        footer={
          embeddingWorkflow?.phase === "estimate"
            ? [
                <Button
                  key="cancel"
                  disabled={embeddingWorkflowBusy}
                  onClick={() => setEmbeddingWorkflow(null)}
                >
                  暂不构建
                </Button>,
                <Button
                  key="build"
                  type="primary"
                  loading={embeddingWorkflowBusy}
                  onClick={requestEmbeddingBuild}
                >
                  开始自动构建
                </Button>,
              ]
            : embeddingWorkflow?.phase === "activate"
              ? [
                  <Button
                    key="close"
                    disabled={embeddingWorkflowBusy}
                    onClick={() => setEmbeddingWorkflow(null)}
                  >
                    稍后激活
                  </Button>,
                  <Button
                    key="activate"
                    type="primary"
                    loading={embeddingWorkflowBusy}
                    disabled={embeddingWorkflowBusy}
                    onClick={() => void activateEmbeddingWorkflow()}
                  >
                    激活新的索引
                  </Button>,
                ]
              : [
                  <Button
                    key="close"
                    disabled={embeddingWorkflowBusy}
                    onClick={() => setEmbeddingWorkflow(null)}
                  >
                    关闭
                  </Button>,
                ]
        }
        destroyOnHidden
      >
        {embeddingWorkflow && (
          <Space orientation="vertical" size="middle" className="w-full">
            <Steps
              current={
                embeddingWorkflow.phase === "estimate"
                  ? 0
                  : embeddingWorkflow.phase === "building"
                    ? 1
                    : embeddingWorkflow.phase === "activate"
                      ? 2
                      : 3
              }
              items={[
                { title: "模型测试与估算" },
                { title: "自动构建与校验" },
                { title: "确认激活" },
                { title: "完成" },
              ]}
            />
            <Alert
              type={
                embeddingWorkflow.estimate.requiresRemoteConfirmation
                  ? "warning"
                  : "info"
              }
              showIcon
              title={
                embeddingWorkflow.estimate.requiresRemoteConfirmation
                  ? "本次构建会向远程服务发送已授权片段"
                  : "本次构建仅使用本地处理"
              }
              description={
                embeddingWorkflow.estimate.requiresRemoteConfirmation
                  ? "当前活动索引会持续可用；系统会自动按批次构建、校验完整性，并在成功后等待你确认激活。发现未授权片段后，本次批次会立即停止，该片段不会发送；完成授权后请重新构建。"
                  : "当前活动索引会持续可用；系统会自动按批次构建、校验完整性，并在成功后等待你确认激活。"
              }
            />
            <Descriptions
              bordered
              size="small"
              column={2}
              items={[
                {
                  key: "profile",
                  label: "目标方案",
                  children: embeddingWorkflow.estimate.targetProfileKey,
                },
                {
                  key: "test",
                  label: "模型测试",
                  children: `${embeddingWorkflow.testDimension} 维，已通过`,
                },
                {
                  key: "chunks",
                  label: "待处理片段",
                  children: embeddingWorkflow.estimate.chunksToEmbed,
                },
                {
                  key: "remote",
                  label: "远程字符数",
                  children: embeddingWorkflow.estimate.remoteCharacters,
                },
                {
                  key: "disk",
                  label: "额外磁盘估算",
                  children: `${embeddingWorkflow.estimate.additionalDiskBytes} B`,
                },
                {
                  key: "blocked",
                  label: "策略阻断片段",
                  children: embeddingWorkflow.estimate.remoteBlockedChunks,
                },
                ...(embeddingWorkflow.batch
                  ? [
                      {
                        key: "progress",
                        label: "构建进度",
                        children: `${embeddingWorkflow.batch.processedChunks}/${embeddingWorkflow.batch.totalChunks}`,
                      },
                    ]
                  : []),
                ...(embeddingWorkflow.validation
                  ? [
                      {
                        key: "validation",
                        label: "完整性校验",
                        children: embeddingWorkflow.validation.complete
                          ? "通过"
                          : "未通过",
                      },
                    ]
                  : []),
              ]}
            />
            {embeddingWorkflow.phase === "building" && (
              <Spin tip="正在自动构建向量索引…" />
            )}
            {embeddingWorkflow.phase === "activate" && (
              <Alert
                type="success"
                showIcon
                title="构建与校验已完成"
                description="确认激活后，新的向量化方案才会参与知识检索。"
              />
            )}
            {embeddingWorkflow.phase === "completed" && (
              <Alert type="success" showIcon title="新索引已激活" />
            )}
          </Space>
        )}
      </Modal>

      <Modal
        title="确认发送已授权内容"
        open={remoteEmbeddingConfirmationOpen}
        mask={{ closable: false }}
        confirmLoading={embeddingWorkflowBusy}
        okText="确认并开始构建"
        cancelText="取消"
        onCancel={() =>
          !embeddingWorkflowBusy && setRemoteEmbeddingConfirmationOpen(false)
        }
        onOk={() => void confirmEmbeddingBuild()}
        destroyOnHidden
      >
        <Paragraph>
          本次索引构建会把预计{" "}
          {embeddingWorkflow?.estimate.remoteCharacters ?? 0}{" "}
          个字符发送到已配置的远程向量服务。
          仅来源级授权且通过敏感内容检查的片段会被发送；发现未授权片段后，本次批次会停止，
          该片段不会发送，完成授权后请重新构建。
        </Paragraph>
        <Alert
          type="warning"
          showIcon
          title="请确认远程处理范围"
          description="确认后才会开始构建；后端仍会在每个批次发送前再次执行来源授权和敏感内容检查。"
        />
      </Modal>

      <Modal
        title="知识项目"
        open={projectModalOpen}
        onCancel={() => setProjectModalOpen(false)}
        onOk={() => void saveProject()}
        confirmLoading={saving}
        destroyOnHidden
      >
        <Form
          form={projectForm}
          layout="vertical"
          initialValues={{ aliases: [], enabled: true, defaultBranch: "main" }}
        >
          <Form.Item name="id" hidden>
            <Input />
          </Form.Item>
          <Form.Item
            name="projectKey"
            label="项目标识"
            rules={[{ required: true, message: "请输入稳定的项目标识" }]}
          >
            <Input placeholder="例如 tauri-ssh" />
          </Form.Item>
          <Form.Item
            name="name"
            label="项目名称"
            rules={[{ required: true, message: "请输入项目名称" }]}
          >
            <Input />
          </Form.Item>
          <Form.Item
            name="aliases"
            label="项目别名"
            tooltip="别名用于查询解析；发生歧义时系统会要求选择项目。"
          >
            <Select
              mode="tags"
              tokenSeparators={[",", "，"]}
              placeholder="输入后按回车"
            />
          </Form.Item>
          <Form.Item
            name="gitWorkspaceKeys"
            label="Git 工作区标识"
            tooltip="从已加载的 Git 工作区中搜索并可多选；保存后用于发现关联仓库的 Tag、分支和 Commit。"
          >
            <Select
              mode="multiple"
              showSearch
              optionFilterProp="label"
              loading={projectGitWorkspacesLoading}
              options={projectGitWorkspaceOptions}
              placeholder="搜索并选择已加载的 Git 工作区"
              notFoundContent={
                projectGitWorkspacesLoading
                  ? "正在加载 Git 工作区…"
                  : "没有已加载的 Git 工作区，请先在 Git 工作区页面登记项目"
              }
            />
          </Form.Item>
          <Form.Item name="defaultBranch" label="默认分支">
            <Input />
          </Form.Item>
          <Form.Item name="description" label="说明">
            <Input.TextArea rows={3} />
          </Form.Item>
          <Form.Item name="enabled" label="启用项目" valuePropName="checked">
            <Switch />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="发布版本"
        open={releaseModalOpen}
        onCancel={() => setReleaseModalOpen(false)}
        onOk={() => void saveRelease()}
        confirmLoading={saving}
        destroyOnHidden
      >
        <Form form={releaseForm} layout="vertical">
          <Form.Item name="id" hidden>
            <Input />
          </Form.Item>
          <Form.Item
            name="projectId"
            label="所属项目"
            rules={[{ required: true, message: "请选择所属项目" }]}
          >
            <Select options={projectOptions} />
          </Form.Item>
          <Form.Item
            name="version"
            label="版本号"
            rules={[{ required: true, message: "请输入版本号或 unversioned" }]}
          >
            <Input placeholder="例如 v1.6.0 或 unversioned" />
          </Form.Item>
          <Form.Item name="tagName" label="Git Tag">
            <Input placeholder="可选" />
          </Form.Item>
          <Form.Item name="branch" label="分支">
            <Input placeholder="可选" />
          </Form.Item>
          <Form.Item name="commitSha" label="基线 Commit">
            <Input placeholder="可选；unversioned 时将不保存 Tag/Commit" />
          </Form.Item>
          <Form.Item name="description" label="说明">
            <Input.TextArea rows={3} />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="知识来源"
        open={sourceModalOpen}
        onCancel={() => setSourceModalOpen(false)}
        onOk={() => void saveSource()}
        confirmLoading={saving}
        width={760}
        destroyOnHidden
      >
        <Form
          form={sourceForm}
          layout="vertical"
          initialValues={{
            sourceType: "local_directory",
            versionStrategy: "unversioned",
            syncMode: "incremental",
            enabled: true,
            allowRemoteEmbedding: false,
          }}
        >
          <Form.Item name="id" hidden>
            <Input />
          </Form.Item>
          <div className="grid grid-cols-1 gap-x-4 md:grid-cols-2">
            <Form.Item
              name="sourceKey"
              label="来源标识"
              rules={[{ required: true, message: "请输入稳定的来源标识" }]}
            >
              <Input placeholder="例如 tauri-ssh-docs" />
            </Form.Item>
            <Form.Item
              name="displayName"
              label="显示名称"
              rules={[{ required: true, message: "请输入显示名称" }]}
            >
              <Input />
            </Form.Item>
            <Form.Item name="projectId" label="所属项目">
              <Select allowClear options={projectOptions} />
            </Form.Item>
            <Form.Item
              name="sourceType"
              label="来源类型"
              rules={[{ required: true }]}
            >
              <Select options={KNOWLEDGE_SOURCE_TYPE_OPTIONS} />
            </Form.Item>
          </div>
          <Form.Item
            name="rootPath"
            label="授权根目录 / 文件路径"
            tooltip="后端将规范化路径、拒绝越界和符号链接逃逸。"
          >
            <Input placeholder="本地目录或单文件；Git 来源可留空并填写工作区标识" />
          </Form.Item>
          <Form.Item
            name="gitWorkspaceKeys"
            label="Git 工作区标识"
            tooltip="从已加载的 Git 工作区中搜索并可多选；多选后将分别创建独立来源，以保留各自的同步游标和 Commit 证据。"
          >
            <Select
              mode="multiple"
              showSearch
              optionFilterProp="label"
              loading={projectGitWorkspacesLoading}
              options={projectGitWorkspaceOptions}
              placeholder="搜索并选择已加载的 Git 工作区"
              notFoundContent={
                projectGitWorkspacesLoading
                  ? "正在加载 Git 工作区…"
                  : "没有已加载的 Git 工作区，请先在 Git 工作区页面登记项目"
              }
            />
          </Form.Item>
          <div className="grid grid-cols-1 gap-x-4 md:grid-cols-2">
            <Form.Item name="includeLines" label="包含规则（每行一条）">
              <Input.TextArea rows={3} placeholder="**/*.md" />
            </Form.Item>
            <Form.Item name="excludeLines" label="排除规则（每行一条）">
              <Input.TextArea rows={3} placeholder="node_modules/**" />
            </Form.Item>
          </div>
          <div className="grid grid-cols-1 gap-x-4 md:grid-cols-2">
            <Form.Item name="versionStrategy" label="版本策略">
              <Select options={KNOWLEDGE_VERSION_STRATEGY_OPTIONS} />
            </Form.Item>
            <Form.Item name="syncMode" label="同步模式">
              <Select options={KNOWLEDGE_SYNC_MODE_OPTIONS} />
            </Form.Item>
          </div>
          <Space size="large">
            <Form.Item name="enabled" label="启用来源" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item
              name="allowRemoteEmbedding"
              label="允许远程向量化"
              valuePropName="checked"
            >
              <Switch />
            </Form.Item>
          </Space>
          <Button onClick={() => void previewScope()}>预览有效读取范围</Button>
          {scopePreview && (
            <Alert
              className="mt-3"
              type="info"
              showIcon
              message={`将包含 ${scopePreview.includedFiles} 个文件（${scopePreview.includedBytes} 字节），跳过 ${scopePreview.skippedEntries} 个条目`}
              description={
                <Space direction="vertical">
                  <Text>授权根目录：{scopePreview.canonicalRoot}</Text>
                  {scopePreview.warnings.map((warning) => (
                    <Text key={warning} type="warning">
                      {warning}
                    </Text>
                  ))}
                </Space>
              }
            />
          )}
        </Form>
      </Modal>

      <Modal
        title="向量化方案"
        open={profileModalOpen}
        onCancel={() => setProfileModalOpen(false)}
        onOk={() => void saveProfile()}
        confirmLoading={saving}
        width={720}
        destroyOnHidden
      >
        <Form form={profileForm} layout="vertical">
          <Form.Item name="id" hidden>
            <Input />
          </Form.Item>
          <div className="grid grid-cols-1 gap-x-4 md:grid-cols-2">
            <Form.Item
              name="profileKey"
              label="向量化方案标识"
              rules={[{ required: true }]}
            >
              <Input placeholder="local-e5-v1" />
            </Form.Item>
            <Form.Item
              name="name"
              label="显示名称"
              rules={[{ required: true }]}
            >
              <Input />
            </Form.Item>
            <Form.Item name="mode" label="模式" rules={[{ required: true }]}>
              <Select
                onChange={handleEmbeddingModeChange}
                options={[
                  { value: "local", label: "本地" },
                  { value: "remote", label: "远程（需授权）" },
                ]}
              />
            </Form.Item>
            <Form.Item
              name="providerKey"
              label="服务商标识"
              rules={
                profileMode === "remote"
                  ? [
                      {
                        required: true,
                        message: "远程模式必须选择已配置的服务商标识",
                      },
                    ]
                  : []
              }
            >
              <Select
                allowClear={profileMode === "remote"}
                aria-label="服务商标识"
                disabled={profileMode !== "remote"}
                loading={aiProvidersLoading}
                notFoundContent={
                  aiProvidersLoading
                    ? "正在读取已配置服务商"
                    : "暂无已启用且已配置向量模型的服务商"
                }
                onChange={handleRemoteEmbeddingProviderChange}
                optionFilterProp="label"
                options={remoteEmbeddingProviderOptions}
                placeholder={
                  profileMode === "remote"
                    ? "请选择已配置的向量服务商"
                    : "本地模式不使用服务商"
                }
                showSearch
              />
            </Form.Item>
            <Form.Item
              name="model"
              label="模型"
              rules={[{ required: true, message: "请选择向量化模型" }]}
            >
              <Select
                aria-label="模型"
                disabled={
                  profileMode === "remote" && !selectedEmbeddingProvider
                }
                onChange={(model: string) => setProfileModel(model)}
                notFoundContent={
                  profileMode === "remote"
                    ? "请先选择服务商，或在 AI Provider 中配置向量模型"
                    : "暂无可用本地模型"
                }
                optionFilterProp="label"
                options={profileModelOptions}
                placeholder={
                  profileMode === "remote"
                    ? "请选择服务商提供的向量模型"
                    : "请选择本地模型"
                }
                showSearch
              />
            </Form.Item>
            <Form.Item name="modelRevision" label="模型修订">
              <Input />
            </Form.Item>
            <Form.Item
              name="dimension"
              label="维度"
              rules={[{ required: true }]}
            >
              <InputNumber className="w-full" min={1} precision={0} />
            </Form.Item>
            <Form.Item name="normalized" label="归一化" valuePropName="checked">
              <Switch />
            </Form.Item>
          </div>
          {profileMode === "remote" && (
            <Alert
              className="mb-4"
              type="warning"
              showIcon
              message="远程模式必须经过来源级授权与敏感内容检查"
              description="保存向量化方案不会发送正文；蓝绿重建前仍会显示远程字符量并要求确认。"
            />
          )}
          <Form.Item
            name="configText"
            label="安全配置 JSON"
            tooltip="可填写 prefix、endpointIdentity 和协议；密钥仍只通过 Provider 安全凭据引用管理。"
          >
            <Input.TextArea rows={7} />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="禅道只读连接"
        open={zentaoModalOpen}
        onCancel={() => setZentaoModalOpen(false)}
        onOk={() => void saveZentaoConnection()}
        confirmLoading={saving}
        width={720}
        destroyOnHidden
      >
        <Form
          form={zentaoForm}
          layout="vertical"
          onValuesChange={(changedValues) => {
            if (typeof changedValues.baseUrl === "string") {
              const usesInsecureHttp = changedValues.baseUrl
                .trim()
                .toLowerCase()
                .startsWith("http://");
              zentaoForm.setFieldsValue({
                tlsVerify: !usesInsecureHttp,
                allowInsecureHttp: usesInsecureHttp
                  ? zentaoForm.getFieldValue("allowInsecureHttp")
                  : false,
              });
            }
          }}
        >
          <Form.Item name="id" hidden>
            <Input />
          </Form.Item>
          <div className="grid grid-cols-1 gap-x-4 md:grid-cols-2">
            <Form.Item
              name="connectionKey"
              label="连接标识"
              rules={[{ required: true }]}
            >
              <Input />
            </Form.Item>
            <Form.Item name="name" label="名称" rules={[{ required: true }]}>
              <Input />
            </Form.Item>
            <Form.Item
              name="baseUrl"
              label="禅道地址"
              rules={[{ required: true, type: "url" }]}
            >
              <Input placeholder="https://zentao.example/ 或 http://内网禅道/" />
            </Form.Item>
            <Form.Item name="apiVersion" label="API 版本">
              <Input placeholder="auto" />
            </Form.Item>
            <Form.Item
              name="authMode"
              label="认证模式"
              rules={[{ required: true }]}
            >
              <Select
                options={[
                  { value: "bearer", label: "API Token（Bearer）" },
                  { value: "auto", label: "自动探测（按 API Token）" },
                ]}
              />
            </Form.Item>
            <Form.Item
              name="credentialKey"
              label="安全凭据引用"
              rules={[
                {
                  required: true,
                  whitespace: true,
                  message: "请输入安全凭据引用",
                },
              ]}
            >
              <Input.Password placeholder="仅引用键，不保存明文 Token" />
            </Form.Item>
            <Form.Item name="endpointProfile" label="端点配置">
              <Input placeholder="先探测后填写" />
            </Form.Item>
            <Form.Item name="pageSize" label="分页大小">
              <InputNumber className="w-full" min={1} precision={0} />
            </Form.Item>
            <Form.Item name="requestTimeoutSeconds" label="超时（秒）">
              <InputNumber className="w-full" min={1} precision={0} />
            </Form.Item>
            <Form.Item name="rateLimitPerSecond" label="请求速率">
              <InputNumber className="w-full" min={0.1} />
            </Form.Item>
          </div>
          {zentaoUsesInsecureHttp && (
            <Alert
              className="mb-4"
              type="error"
              showIcon
              message="明文 HTTP 仅限已受控的内网环境"
              description="Token/Cookie 可能被网络监听或篡改。请先在“安全 → 策略”将该主机精确加入 HTTP 域名白名单，再显式允许内网 HTTP 并完成保存确认。"
            />
          )}
          <Space size="large">
            <Form.Item
              name="tlsVerify"
              label="校验证书"
              valuePropName="checked"
            >
              <Switch disabled={zentaoUsesInsecureHttp} />
            </Form.Item>
            <Form.Item
              name="allowInsecureHttp"
              label="允许内网 HTTP（高风险）"
              valuePropName="checked"
            >
              <Switch disabled={!zentaoUsesInsecureHttp} />
            </Form.Item>
            <Form.Item name="enabled" label="启用连接" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Space>
        </Form>
      </Modal>

      <Modal
        title="禅道项目 / 版本映射"
        open={mappingModalOpen}
        onCancel={() => setMappingModalOpen(false)}
        onOk={() => void saveMapping()}
        confirmLoading={saving}
        width={720}
        destroyOnHidden
      >
        <Form form={mappingForm} layout="vertical">
          <Form.Item name="id" hidden>
            <Input />
          </Form.Item>
          <div className="grid grid-cols-1 gap-x-4 md:grid-cols-2">
            <Form.Item
              name="connectionId"
              label="禅道连接"
              rules={[{ required: true }]}
            >
              <Select
                options={zentaoConnections.map((item) => ({
                  value: item.id,
                  label: item.name,
                }))}
              />
            </Form.Item>
            <Form.Item
              name="knowledgeProjectId"
              label="知识项目"
              rules={[{ required: true }]}
            >
              <Select options={projectOptions} />
            </Form.Item>
            <Form.Item name="remoteProductId" label="远程产品 ID">
              <Input />
            </Form.Item>
            <Form.Item
              name="remoteProjectId"
              label="远程项目 ID"
              rules={[{ required: true }]}
            >
              <Input />
            </Form.Item>
          </div>
          <Form.Item name="executionLines" label="执行 ID（每行一条）">
            <Input.TextArea rows={2} />
          </Form.Item>
          <Form.Item
            name="releaseMappingText"
            label="发布版本映射 JSON"
            rules={[{ required: true }]}
          >
            <Input.TextArea rows={3} />
          </Form.Item>
          <Form.Item
            name="syncScopeText"
            label="同步范围 JSON"
            rules={[{ required: true }]}
          >
            <Input.TextArea rows={3} />
          </Form.Item>
          <Space size="large" wrap>
            <Form.Item
              name="includeComments"
              label="同步评论"
              valuePropName="checked"
            >
              <Switch />
            </Form.Item>
            <Form.Item
              name="includeWorklogs"
              label="同步工时（须能力支持）"
              valuePropName="checked"
            >
              <Switch />
            </Form.Item>
            <Form.Item
              name="includeAttachmentMetadata"
              label="附件仅元数据"
              valuePropName="checked"
            >
              <Switch />
            </Form.Item>
            <Form.Item
              name="allowRemoteEmbedding"
              label="允许远程向量化"
              valuePropName="checked"
            >
              <Switch />
            </Form.Item>
            <Form.Item
              name="allowRemoteAi"
              label="允许远程 AI"
              valuePropName="checked"
            >
              <Switch />
            </Form.Item>
            <Form.Item name="enabled" label="启用映射" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Space>
        </Form>
      </Modal>

      <Modal
        title="生成禅道 AI 摘要"
        open={aiSummaryModalOpen}
        onCancel={() => setAiSummaryModalOpen(false)}
        onOk={() => void generateZentaoAiSummary()}
        confirmLoading={saving}
        width={680}
        destroyOnHidden
      >
        <Alert
          type="warning"
          showIcon
          className="mb-4"
          title="摘要与事实文档分离"
          description="仅在映射显式允许远程 AI 时可生成；系统只发送通过项目、来源和敏感级别过滤的已生成事实片段，并拒绝没有引用的结论。"
        />
        <Form form={aiSummaryForm} layout="vertical">
          <Form.Item
            name="providerKey"
            label="AI Provider Key"
            rules={[{ required: true }]}
          >
            <Input />
          </Form.Item>
          <Form.Item name="model" label="模型" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item
            name="prompt"
            label="摘要重点"
            rules={[{ required: true }]}
          >
            <Input.TextArea rows={4} />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="源码知识来源"
        open={codeSourceModalOpen}
        onCancel={() => setCodeSourceModalOpen(false)}
        onOk={() => void saveCodeSource()}
        confirmLoading={saving}
        width={760}
        // 表单需要在打开前接收编辑回填值；保持挂载避免首次打开时出现未连接 Form 警告。
        forceRender
      >
        <Form form={codeSourceForm} layout="vertical">
          <div className="grid grid-cols-1 gap-x-4 md:grid-cols-2">
            <Form.Item
              name={["source", "sourceKey"]}
              label="来源标识"
              rules={[{ required: true }]}
            >
              <Input />
            </Form.Item>
            <Form.Item
              name={["source", "displayName"]}
              label="显示名称"
              rules={[{ required: true }]}
            >
              <Input />
            </Form.Item>
            <Form.Item name={["source", "projectId"]} label="知识项目">
              <Select allowClear options={projectOptions} />
            </Form.Item>
            <Form.Item
              name={["source", "sourceType"]}
              label="类型"
              rules={[{ required: true }]}
            >
              <Select
                options={KNOWLEDGE_SOURCE_TYPE_OPTIONS.filter(({ value }) =>
                  ["git_workspace", "local_directory"].includes(value),
                )}
              />
            </Form.Item>
          </div>
          <Form.Item name={["source", "rootPath"]} label="授权目录">
            <Input placeholder="本地目录；Git 来源可填写工作区标识" />
          </Form.Item>
          <Form.Item
            name={["source", "gitWorkspaceKey"]}
            label="Git 工作区标识"
            tooltip="从已加载的 Git 工作区中搜索并选择；保存时仅提交稳定的工作区标识。"
          >
            <Select
              showSearch
              optionFilterProp="label"
              loading={projectGitWorkspacesLoading}
              options={projectGitWorkspaceOptions}
              placeholder="搜索并选择已加载的 Git 工作区"
              onChange={(workspaceKey: string | undefined) => {
                // 选择已登记工作区代表以 Git 方式读取，自动切换类型，避免用户仍保留
                // “本地目录”而遗漏必填目录路径。
                codeSourceForm.setFieldValue(
                  ["source", "sourceType"],
                  workspaceKey ? "git_workspace" : "local_directory",
                );
              }}
              notFoundContent={
                projectGitWorkspacesLoading
                  ? "正在加载 Git 工作区…"
                  : "没有已加载的 Git 工作区，请先在 Git 工作区页面登记项目"
              }
            />
          </Form.Item>
          <div className="grid grid-cols-1 gap-x-4 md:grid-cols-2">
            <Form.Item name="includeLines" label="包含规则（每行）">
              <Input.TextArea rows={2} />
            </Form.Item>
            <Form.Item name="excludeLines" label="排除规则（每行）">
              <Input.TextArea rows={2} />
            </Form.Item>
          </div>
          <Form.Item
            name="allowedLanguages"
            label="需要解析的编程语言"
            tooltip="可搜索并多选。Markdown 始终参与联合分析，系统会优先提取所选语言的结构、代码元素和调用关系。"
            rules={[{ required: true, message: "请至少选择一种编程语言" }]}
          >
            <Select
              mode="multiple"
              showSearch
              optionFilterProp="label"
              options={CODE_SOURCE_LANGUAGE_OPTIONS}
              placeholder="搜索并选择需要解析的编程语言"
            />
          </Form.Item>
          <Space size="large">
            <Form.Item
              name="includeUntracked"
              label="纳入未跟踪文件"
              valuePropName="checked"
            >
              <Switch />
            </Form.Item>
            <Form.Item name="maxFileSizeBytes" label="最大文件字节数">
              <InputNumber min={1} precision={0} />
            </Form.Item>
            <Form.Item
              name={["source", "enabled"]}
              label="启用来源"
              valuePropName="checked"
            >
              <Switch />
            </Form.Item>
          </Space>
          <Alert
            type="warning"
            showIcon
            title="源码远程处理默认关闭"
            description="源码敏感检测、范围校验和本地读取由 Rust 后端执行，前端不会直接读取文件。"
          />
        </Form>
      </Modal>
    </div>
  );
}
