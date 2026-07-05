import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Alert,
  Button,
  Card,
  Descriptions,
  Divider,
  Drawer,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Select,
  Space,
  Switch,
  Table,
  Tabs,
  Tag,
  Typography,
  message,
} from "antd";
import type { ColumnsType } from "antd/es/table";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Copy, Download, Edit, FileText, Folder, PackageCheck, PlayCircle, Plus, RefreshCw, RotateCcw, Square, Star, Trash2 } from "lucide-react";
import { getErrorMessage, gitWorkspaceApi, hasTauriRuntime, jenkinsApi, secureCredentialApi, sshServerApi } from "@/lib/api";
import type {
  GitWorkspace,
  GitWorkspaceStatusResult,
  JenkinsArtifact,
  JenkinsBuild,
  JenkinsBuildAnalysis,
  JenkinsBuildLogResult,
  JenkinsBuildStatusEvent,
  JenkinsConnection,
  DeploymentCandidate,
  DeploymentPlan,
  DeploymentPlanStage,
  JenkinsFileParameterMetadata,
  JenkinsJob,
  JenkinsParameterDefinition,
  JenkinsParameterDefinitionsResult,
  JenkinsParameterTemplate,
  JenkinsQueueItem,
  JenkinsRecentParameterValue,
  JenkinsSensitiveParameterReference,
  SecureCredential,
  SshServer,
  UpsertJenkinsConnectionInput,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

type RiskLevel = "L2" | "L3" | "blocked";
type EnvironmentRisk = "auto" | RiskLevel;

interface JenkinsRiskJobRuleFormValue {
  pattern?: string;
  risk?: RiskLevel;
  enabled?: boolean;
}

interface JenkinsRiskParameterRuleFormValue {
  name?: string;
  value?: string;
  risk?: RiskLevel;
  enabled?: boolean;
}

interface JenkinsRiskRuleFormValues {
  riskFallbackRisk?: RiskLevel;
  riskFileParameterRisk?: RiskLevel;
  riskEnvironmentRisk?: EnvironmentRisk;
  riskAllowConcurrentBuilds?: boolean;
  riskAllowConcurrentPatternsText?: string;
  riskJobRules?: JenkinsRiskJobRuleFormValue[];
  riskParameterRules?: JenkinsRiskParameterRuleFormValue[];
}

type ConnectionFormValues = UpsertJenkinsConnectionInput & JenkinsRiskRuleFormValues;
type ParameterFormValue =
  | string
  | boolean
  | JenkinsFileParameterMetadata
  | JenkinsSensitiveParameterReference
  | undefined;
type ParameterFormValues = Record<string, ParameterFormValue>;

const logErrorPatterns = [
  "ERROR",
  "FAILURE",
  "Exception",
  "Traceback",
  "BUILD FAILED",
  "npm ERR!",
  "MavenCompilationFailureException",
];

const statusMeta: Record<string, { label: string; color: string }> = {
  draft: { label: "草稿", color: "default" },
  ok: { label: "可用", color: "green" },
  success: { label: "成功", color: "green" },
  failed: { label: "失败", color: "red" },
  failure: { label: "失败", color: "red" },
  unstable: { label: "不稳定", color: "orange" },
  aborted: { label: "已中止", color: "default" },
  not_built: { label: "未构建", color: "default" },
  building: { label: "构建中", color: "blue" },
  queued: { label: "排队中", color: "blue" },
  waiting: { label: "等待中", color: "blue" },
  blocked: { label: "已阻断", color: "red" },
  credential_missing: { label: "缺少凭证", color: "orange" },
  credential_failed: { label: "凭证失败", color: "red" },
  pending_integration: { label: "待接入", color: "blue" },
};

const environmentOptions = [
  { label: "开发", value: "dev" },
  { label: "测试", value: "test" },
  { label: "预发", value: "staging" },
  { label: "生产", value: "prod" },
];

const riskLevelOptions = [
  { label: "L2（需审批）", value: "L2" },
  { label: "L3（高风险审批）", value: "L3" },
  { label: "blocked（禁止）", value: "blocked" },
];

const environmentRiskOptions = [{ label: "自动：生产环境 L3，其余 L2", value: "auto" }, ...riskLevelOptions];
const unfinishedBuildStatuses = new Set([
  "queued",
  "waiting",
  "blocked",
  "stuck",
  "triggered",
  "building",
  "tracking_timeout",
]);

const defaultRiskRuleFormValues: Required<JenkinsRiskRuleFormValues> = {
  riskFallbackRisk: "L2",
  riskFileParameterRisk: "L3",
  riskEnvironmentRisk: "auto",
  riskAllowConcurrentBuilds: false,
  riskAllowConcurrentPatternsText: "",
  riskJobRules: [
    { pattern: ".*(prod|production|release).*", risk: "L3", enabled: true },
    { pattern: ".*(dev|test).*", risk: "L2", enabled: true },
  ],
  riskParameterRules: [
    { name: "ENV", value: "prod", risk: "L3", enabled: true },
    { name: "DEPLOY", value: "true", risk: "L3", enabled: true },
  ],
};

function statusTag(status: string) {
  const meta = statusMeta[status] ?? { label: status || "未知", color: "default" };
  return <Tag color={meta.color}>{meta.label}</Tag>;
}

function jobStatusTag(record: JenkinsJob) {
  if (record.jobType === "folder") {
    return <Text type="secondary">-</Text>;
  }
  if (record.normalizedStatus === "unknown" && !record.lastBuildNumber) {
    return statusTag("not_built");
  }
  return statusTag(record.normalizedStatus);
}

function formatJobType(value: string) {
  const labels: Record<string, string> = {
    folder: "目录",
    freestyle: "自由风格",
    pipeline: "流水线",
    multibranch: "多分支",
    organization: "组织目录",
    job: "任务",
  };
  return labels[value] ?? (value || "未知");
}

function deploymentRiskTag(risk: string) {
  const meta: Record<string, { label: string; color: string }> = {
    high: { label: "高风险", color: "red" },
    review: { label: "需复核", color: "orange" },
    readonly: { label: "只读", color: "blue" },
  };
  const item = meta[risk] ?? { label: risk || "未知", color: "default" };
  return <Tag color={item.color}>{item.label}</Tag>;
}

type LogHighlightKind = "search" | "error";

interface LogHighlightMatch {
  start: number;
  end: number;
  kind: LogHighlightKind;
}

interface FailureLogSummary {
  matchedLines: number;
  startLine: number;
  endLine: number;
  text: string;
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function countMatches(text: string, needle: string) {
  const trimmed = needle.trim();
  if (!trimmed) {
    return 0;
  }
  const matches = text.match(new RegExp(escapeRegExp(trimmed), "gi"));
  return matches?.length ?? 0;
}

function collectLogHighlightMatches(text: string, searchTerm: string): LogHighlightMatch[] {
  const matches: LogHighlightMatch[] = [];
  const pushMatches = (pattern: string, kind: LogHighlightKind) => {
    if (!pattern.trim()) {
      return;
    }
    const regex = new RegExp(escapeRegExp(pattern), "gi");
    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      if (match.index === regex.lastIndex) {
        regex.lastIndex += 1;
      }
      matches.push({
        start: match.index,
        end: match.index + match[0].length,
        kind,
      });
    }
  };

  logErrorPatterns.forEach((pattern) => pushMatches(pattern, "error"));
  pushMatches(searchTerm.trim(), "search");

  return matches
    .filter((match) => match.end > match.start)
    .sort((left, right) => left.start - right.start || right.end - left.end)
    .reduce<LogHighlightMatch[]>((merged, match) => {
      const previous = merged[merged.length - 1];
      if (!previous || match.start >= previous.end) {
        merged.push(match);
        return merged;
      }
      previous.end = Math.max(previous.end, match.end);
      if (match.kind === "search") {
        previous.kind = "search";
      }
      return merged;
    }, []);
}

function renderHighlightedLogText(text: string, searchTerm: string): ReactNode[] {
  const matches = collectLogHighlightMatches(text, searchTerm);
  if (!matches.length) {
    return [text];
  }

  const nodes: ReactNode[] = [];
  let cursor = 0;
  matches.forEach((match, index) => {
    if (match.start > cursor) {
      nodes.push(text.slice(cursor, match.start));
    }
    nodes.push(
      <mark
        key={`${match.start}-${match.end}-${index}`}
        style={{
          background: match.kind === "search" ? "#fff1b8" : "#ffd6d6",
          color: "inherit",
          padding: 0,
        }}
      >
        {text.slice(match.start, match.end)}
      </mark>,
    );
    cursor = match.end;
  });
  if (cursor < text.length) {
    nodes.push(text.slice(cursor));
  }
  return nodes;
}

function extractFailureLogSummary(text: string): FailureLogSummary | null {
  const lines = text.split(/\r?\n/);
  const matchedIndexes = lines
    .map((line, index) => ({
      index,
      matched: logErrorPatterns.some((pattern) => line.toLowerCase().includes(pattern.toLowerCase())),
    }))
    .filter((item) => item.matched)
    .map((item) => item.index);

  if (!matchedIndexes.length) {
    return null;
  }

  const selectedIndexes = new Set<number>();
  matchedIndexes.forEach((index) => {
    const start = Math.max(0, index - 2);
    const end = Math.min(lines.length - 1, index + 3);
    for (let lineIndex = start; lineIndex <= end; lineIndex += 1) {
      selectedIndexes.add(lineIndex);
    }
  });

  const sortedIndexes = Array.from(selectedIndexes).sort((left, right) => left - right);
  const clippedIndexes = sortedIndexes.slice(0, 120);
  return {
    matchedLines: matchedIndexes.length,
    startLine: clippedIndexes[0] + 1,
    endLine: clippedIndexes[clippedIndexes.length - 1] + 1,
    text: clippedIndexes.map((index) => `${String(index + 1).padStart(5, " ")} | ${lines[index]}`).join("\n"),
  };
}

function parseRiskRuleFormValues(riskRulesJson?: string): JenkinsRiskRuleFormValues {
  if (!riskRulesJson || riskRulesJson.trim() === "{}" || riskRulesJson.trim() === "[]") {
    return defaultRiskRuleFormValues;
  }
  try {
    const value = JSON.parse(riskRulesJson) as {
      fallbackRisk?: RiskLevel;
      fileParameterRisk?: RiskLevel;
      environmentRisk?: EnvironmentRisk;
      concurrency?: {
        allowConcurrentBuilds?: boolean;
        allowConcurrentPatterns?: string[];
      };
      jobRules?: JenkinsRiskJobRuleFormValue[];
      parameterRules?: JenkinsRiskParameterRuleFormValue[];
    };
    return {
      riskFallbackRisk: value.fallbackRisk ?? defaultRiskRuleFormValues.riskFallbackRisk,
      riskFileParameterRisk: value.fileParameterRisk ?? defaultRiskRuleFormValues.riskFileParameterRisk,
      riskEnvironmentRisk: value.environmentRisk ?? defaultRiskRuleFormValues.riskEnvironmentRisk,
      riskAllowConcurrentBuilds:
        value.concurrency?.allowConcurrentBuilds ?? defaultRiskRuleFormValues.riskAllowConcurrentBuilds,
      riskAllowConcurrentPatternsText: (value.concurrency?.allowConcurrentPatterns ?? []).join("\n"),
      riskJobRules: value.jobRules?.length ? value.jobRules : defaultRiskRuleFormValues.riskJobRules,
      riskParameterRules: value.parameterRules?.length
        ? value.parameterRules
        : defaultRiskRuleFormValues.riskParameterRules,
    };
  } catch {
    return defaultRiskRuleFormValues;
  }
}

function buildRiskRulesJson(values: JenkinsRiskRuleFormValues) {
  return JSON.stringify({
    version: 1,
    fallbackRisk: values.riskFallbackRisk ?? defaultRiskRuleFormValues.riskFallbackRisk,
    fileParameterRisk: values.riskFileParameterRisk ?? defaultRiskRuleFormValues.riskFileParameterRisk,
    environmentRisk: values.riskEnvironmentRisk ?? defaultRiskRuleFormValues.riskEnvironmentRisk,
    concurrency: {
      allowConcurrentBuilds: values.riskAllowConcurrentBuilds ?? false,
      allowConcurrentPatterns: splitLines(values.riskAllowConcurrentPatternsText),
    },
    jobRules: normalizeJobRiskRules(values.riskJobRules),
    parameterRules: normalizeParameterRiskRules(values.riskParameterRules),
  });
}

function splitLines(value?: string) {
  return (value ?? "")
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function normalizeJobRiskRules(rules?: JenkinsRiskJobRuleFormValue[]) {
  return (rules ?? [])
    .map((rule) => ({
      pattern: rule.pattern?.trim() ?? "",
      risk: rule.risk ?? "L2",
      enabled: rule.enabled ?? true,
    }))
    .filter((rule) => rule.pattern);
}

function normalizeParameterRiskRules(rules?: JenkinsRiskParameterRuleFormValue[]) {
  return (rules ?? [])
    .map((rule) => ({
      name: rule.name?.trim() ?? "",
      value: rule.value?.trim() ?? "",
      risk: rule.risk ?? "L2",
      enabled: rule.enabled ?? true,
    }))
    .filter((rule) => rule.name);
}

function buildConnectionInput(values: ConnectionFormValues): UpsertJenkinsConnectionInput {
  const {
    riskFallbackRisk: _riskFallbackRisk,
    riskFileParameterRisk: _riskFileParameterRisk,
    riskEnvironmentRisk: _riskEnvironmentRisk,
    riskAllowConcurrentBuilds: _riskAllowConcurrentBuilds,
    riskAllowConcurrentPatternsText: _riskAllowConcurrentPatternsText,
    riskJobRules: _riskJobRules,
    riskParameterRules: _riskParameterRules,
    ...input
  } = values;
  return {
    ...input,
    riskRulesJson: buildRiskRulesJson(values),
  };
}

export default function JenkinsPage() {
  const [connections, setConnections] = useState<JenkinsConnection[]>([]);
  const [jobs, setJobs] = useState<JenkinsJob[]>([]);
  const [builds, setBuilds] = useState<JenkinsBuild[]>([]);
  const [queueItems, setQueueItems] = useState<JenkinsQueueItem[]>([]);
  const [secureCredentials, setSecureCredentials] = useState<SecureCredential[]>([]);
  const [sshServers, setSshServers] = useState<SshServer[]>([]);
  const [activeDetailTab, setActiveDetailTab] = useState("jobs");
  const [buildSyncingJob, setBuildSyncingJob] = useState("");
  const [selectedKey, setSelectedKey] = useState("");
  const [keyword, setKeyword] = useState("");
  const [includeDeleted, setIncludeDeleted] = useState(false);
  const [loading, setLoading] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [referenceOptionsLoading, setReferenceOptionsLoading] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [drawerTesting, setDrawerTesting] = useState(false);
  const [buildDrawerOpen, setBuildDrawerOpen] = useState(false);
  const [parameterDrawerOpen, setParameterDrawerOpen] = useState(false);
  const [buildDetailLoading, setBuildDetailLoading] = useState(false);
  const [buildLogLoading, setBuildLogLoading] = useState(false);
  const [analysisLoading, setAnalysisLoading] = useState(false);
  const [parameterLoading, setParameterLoading] = useState(false);
  const [templateLoading, setTemplateLoading] = useState(false);
  const [approvalCreating, setApprovalCreating] = useState(false);
  const [stopApprovalCreating, setStopApprovalCreating] = useState(false);
  const [artifactLoading, setArtifactLoading] = useState(false);
  const [artifactDownloading, setArtifactDownloading] = useState("");
  const [artifactCleaning, setArtifactCleaning] = useState("");
  const [artifactCandidateCreating, setArtifactCandidateCreating] = useState("");
  const [deploymentDryRunLoading, setDeploymentDryRunLoading] = useState(false);
  const [selectedJob, setSelectedJob] = useState<JenkinsJob | null>(null);
  const [selectedBuild, setSelectedBuild] = useState<JenkinsBuild | null>(null);
  const [selectedBuildDetail, setSelectedBuildDetail] = useState<JenkinsBuild | null>(null);
  const [buildLog, setBuildLog] = useState<JenkinsBuildLogResult | null>(null);
  const [logSearchTerm, setLogSearchTerm] = useState("");
  const [failureLogSummary, setFailureLogSummary] = useState<FailureLogSummary | null>(null);
  const [buildAnalysis, setBuildAnalysis] = useState<JenkinsBuildAnalysis | null>(null);
  const [artifacts, setArtifacts] = useState<JenkinsArtifact[]>([]);
  const [deploymentCandidate, setDeploymentCandidate] = useState<DeploymentCandidate | null>(null);
  const [deploymentDryRunPlan, setDeploymentDryRunPlan] = useState<DeploymentPlan | null>(null);
  const [deploymentDryRunServerAlias, setDeploymentDryRunServerAlias] = useState("");
  const [deploymentDryRunDeployRoot, setDeploymentDryRunDeployRoot] = useState("");
  const [deploymentDryRunPort, setDeploymentDryRunPort] = useState<number | null>(null);
  const [parameterResult, setParameterResult] = useState<JenkinsParameterDefinitionsResult | null>(null);
  const [parameterValues, setParameterValues] = useState<ParameterFormValues>({});
  const [parameterTemplates, setParameterTemplates] = useState<JenkinsParameterTemplate[]>([]);
  const [selectedTemplateKey, setSelectedTemplateKey] = useState("");
  const [templateName, setTemplateName] = useState("");
  const [recentParameterValues, setRecentParameterValues] = useState<JenkinsRecentParameterValue[]>([]);
  const [gitWorkspaces, setGitWorkspaces] = useState<GitWorkspace[]>([]);
  const [selectedGitWorkspaceKey, setSelectedGitWorkspaceKey] = useState("");
  const [gitWorkspaceStatus, setGitWorkspaceStatus] = useState<GitWorkspaceStatusResult | null>(null);
  const [gitWorkspaceLoading, setGitWorkspaceLoading] = useState(false);
  const [approvalReason, setApprovalReason] = useState("");
  const [stopReason, setStopReason] = useState("");
  const [editing, setEditing] = useState<JenkinsConnection | null>(null);
  const [jobFolderStack, setJobFolderStack] = useState<string[]>([]);
  const deploymentPromptedBuildKeys = useRef(new Set<string>());
  const parameterLoadSeq = useRef(0);
  const buildLogRef = useRef<JenkinsBuildLogResult | null>(null);
  const buildLogLoadingRef = useRef(false);
  const [form] = Form.useForm<ConnectionFormValues>();
  const [parameterForm] = Form.useForm<ParameterFormValues>();
  const tlsVerifyValue = Form.useWatch("tlsVerify", form);

  const selectedConnection = useMemo(
    () => connections.find((item) => item.connectionKey === selectedKey) ?? null,
    [connections, selectedKey],
  );
  const currentJobFolder = findJobInTree(jobs, jobFolderStack[jobFolderStack.length - 1]) ?? null;
  const visibleJobs = currentJobFolder?.children ?? jobs;
  const jobTableData = useMemo(() => normalizeJobTableData(visibleJobs), [visibleJobs]);
  const buildTableData = useMemo(() => sortJenkinsBuilds(builds), [builds]);
  const hasUnfinishedBuilds = useMemo(
    () =>
      builds.some((build) => unfinishedBuildStatuses.has(build.status)) ||
      queueItems.length > 0,
    [builds, queueItems],
  );

  function clearConnectionScopedState() {
    setJobs([]);
    setJobFolderStack([]);
    setBuilds([]);
    setQueueItems([]);
    setSelectedJob(null);
    setSelectedBuild(null);
    setSelectedBuildDetail(null);
    setActiveDetailTab("jobs");
    setBuildLog(null);
    setBuildAnalysis(null);
    setArtifacts([]);
    setParameterResult(null);
    setRecentParameterValues([]);
    setApprovalReason("");
    setStopReason("");
    parameterForm.resetFields();
    setParameterValues({});
  }

  function switchJenkinsConnection(connectionKey: string) {
    if (!connectionKey || connectionKey === selectedKey) {
      return;
    }
    clearConnectionScopedState();
    setSelectedKey(connectionKey);
  }
  const secureCredentialOptions = useMemo(
    () =>
      secureCredentials.map((credential) => {
        const disabled = !credential.enabled || credential.status !== "active" || !credential.hasSecret;
        const reason = !credential.enabled
          ? "已禁用"
          : credential.status !== "active"
            ? credential.status
            : !credential.hasSecret
              ? "无密钥"
              : "";
        const label = `${credential.displayName || credential.credentialKey} (${credential.credentialKey})${
          reason ? ` - ${reason}` : ""
        }`;
        return {
          label,
          value: credential.credentialKey,
          disabled,
          title: `${credential.provider} / ${credential.credentialType} / ${credential.accountName || "-"}`,
        };
      }),
    [secureCredentials],
  );
  const sshServerOptions = useMemo(
    () =>
      sshServers.map((server) => {
        const label = `${server.alias} (${server.username}@${server.host}:${server.port})${
          server.enabled ? "" : " - 已禁用"
        }`;
        return {
          label,
          value: server.alias,
          disabled: !server.enabled,
          title: `${server.groupName || "-"} / ${server.status}`,
        };
      }),
    [sshServers],
  );
  const loadedLogText = buildLog?.text ?? "";
  const logSearchCount = useMemo(() => countMatches(loadedLogText, logSearchTerm), [loadedLogText, logSearchTerm]);
  const logErrorHighlightCount = useMemo(
    () => logErrorPatterns.reduce((sum, pattern) => sum + countMatches(loadedLogText, pattern), 0),
    [loadedLogText],
  );
  const highlightedLogNodes = useMemo(
    () => renderHighlightedLogText(loadedLogText || "当前偏移未返回日志内容", logSearchTerm),
    [loadedLogText, logSearchTerm],
  );
  const recentParameterValueMap = useMemo(
    () => new Map(recentParameterValues.map((item) => [item.parameterName, item])),
    [recentParameterValues],
  );

  const loadConnections = useCallback(async () => {
    setLoading(true);
    try {
      const list = await jenkinsApi.listConnections({ includeDeleted, keyword });
      setConnections(list);
      if (!selectedKey && list.length > 0) {
        clearConnectionScopedState();
        setSelectedKey(list[0].connectionKey);
      }
      if (selectedKey && !list.some((item) => item.connectionKey === selectedKey)) {
        clearConnectionScopedState();
        setSelectedKey(list[0]?.connectionKey ?? "");
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [includeDeleted, keyword, selectedKey]);

  const loadReferenceOptions = useCallback(async () => {
    setReferenceOptionsLoading(true);
    try {
      const [credentialList, serverList] = await Promise.all([
        secureCredentialApi.list({}),
        sshServerApi.list(),
      ]);
      setSecureCredentials(credentialList);
      setSshServers(serverList);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setReferenceOptionsLoading(false);
    }
  }, []);

  const refreshBuildsAndQueue = useCallback(async (connectionKey: string, jobFullNameOverride?: string | null) => {
    if (!connectionKey) {
      return;
    }
    const jobFullName =
      jobFullNameOverride === undefined
        ? selectedJob?.jobFullName
        : jobFullNameOverride?.trim() || undefined;
    const [rows, queueList] = await Promise.all([
      jenkinsApi.listBuilds({ connectionKey, jobFullName, limit: 30 }),
      jenkinsApi.listQueue(connectionKey),
    ]);
    setBuilds(rows);
    setQueueItems(queueList);
  }, [selectedJob?.jobFullName]);

  const loadConnectionDetail = useCallback(async (connectionKey: string) => {
    if (!connectionKey) {
      clearConnectionScopedState();
      return;
    }
    setDetailLoading(true);
    try {
      const connection = connections.find((item) => item.connectionKey === connectionKey);
      const defaultView = connection?.defaultView?.trim();
      const defaultFolder = connection?.defaultFolder?.trim();
      const [jobList, buildList, queueList] = await Promise.all([
        jenkinsApi.listJobs({
          connectionKey,
          viewName: defaultView || undefined,
          folder: defaultFolder || undefined,
          depth: 3,
        }),
        jenkinsApi.listBuilds({ connectionKey, limit: 30, offset: 0 }),
        jenkinsApi.listQueue(connectionKey),
      ]);
      setJobs(jobList);
      setJobFolderStack([]);
      setBuilds(buildList);
      setQueueItems(queueList);
      setSelectedJob(null);
      setSelectedBuild(null);
      setSelectedBuildDetail(null);
      setActiveDetailTab("jobs");
      setBuildLog(null);
      setArtifacts([]);
      setParameterResult(null);
      setRecentParameterValues([]);
      setApprovalReason("");
      parameterForm.resetFields();
      setParameterValues({});
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setDetailLoading(false);
    }
  }, [connections, parameterForm]);

  useEffect(() => {
    void loadConnections();
  }, [loadConnections]);

  useEffect(() => {
    void loadConnectionDetail(selectedKey);
  }, [loadConnectionDetail, selectedKey]);

  useEffect(() => {
    if (!hasTauriRuntime() || !selectedKey) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<JenkinsBuildStatusEvent>("jenkins-build-status", async (event) => {
          if (disposed || event.payload.connectionKey !== selectedKey) {
            return;
          }
          try {
            if (!disposed) {
              await refreshBuildsAndQueue(selectedKey);
            }
            if (isSuccessfulBuildLike(event.payload) && event.payload.buildNumber) {
              const promptKey = `${event.payload.connectionKey}:${event.payload.jobFullName}:${event.payload.buildNumber}`;
              if (deploymentPromptedBuildKeys.current.has(promptKey)) {
                return;
              }
              const list = await jenkinsApi.listArtifacts({
                connectionKey: event.payload.connectionKey,
                jobFullName: event.payload.jobFullName,
                buildNumber: event.payload.buildNumber,
              });
              if (!disposed && list.length > 0) {
                deploymentPromptedBuildKeys.current.add(promptKey);
                const availableCount = list.filter(isAvailableArtifact).length;
                message.success(
                  availableCount > 0
                    ? `Jenkins 构建成功，已有 ${availableCount} 个可部署 artifact，可打开详情生成部署候选`
                    : `Jenkins 构建成功，发现 ${list.length} 个 artifact，可打开详情下载后准备部署`,
                );
              }
            } else if (isDeploymentBlockedBuildLike(event.payload) && event.payload.buildNumber) {
              const promptKey = `${event.payload.connectionKey}:${event.payload.jobFullName}:${event.payload.buildNumber}:blocked`;
              if (!deploymentPromptedBuildKeys.current.has(promptKey)) {
                deploymentPromptedBuildKeys.current.add(promptKey);
                message.warning("Jenkins 构建未成功，已阻断该构建的部署候选和 Dry-run 入口");
              }
            }
          } catch (error) {
            if (!disposed) {
              message.error(getErrorMessage(error));
            }
          }
        }),
      )
      .then((handler) => {
        unlisten = handler;
      })
      .catch((error) => {
        console.warn("Jenkins build status event listener failed", error);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshBuildsAndQueue, selectedKey]);

  useEffect(() => {
    if (!selectedKey || !hasUnfinishedBuilds) {
      return;
    }
    let disposed = false;
    let running = false;
    const sync = async () => {
      if (disposed || running) {
        return;
      }
      running = true;
      try {
        await jenkinsApi.syncUnfinishedRuns(selectedKey);
        if (!disposed) {
          await refreshBuildsAndQueue(selectedKey);
        }
      } catch (error) {
        if (!disposed) {
          console.warn("Jenkins 未完成构建同步失败", error);
        }
      } finally {
        running = false;
      }
    };
    void sync();
    const timer = window.setInterval(() => void sync(), 5000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [hasUnfinishedBuilds, refreshBuildsAndQueue, selectedKey]);

  useEffect(() => {
    buildLogRef.current = buildLog;
  }, [buildLog]);

  useEffect(() => {
    buildLogLoadingRef.current = buildLogLoading;
  }, [buildLogLoading]);

  function openCreateDrawer() {
    void loadReferenceOptions();
    setEditing(null);
    form.resetFields();
    form.setFieldsValue({
      connectionKey: undefined,
      name: "",
      baseUrl: "",
      credentialKey: "",
      credentialDisplayName: "",
      usernameMasked: "",
      sshServerAlias: "",
      environment: "dev",
      environmentLabel: "",
      tlsVerify: true,
      defaultView: "",
      defaultFolder: "",
      allowMcpRead: true,
      allowMcpWrite: false,
      approvalPolicy: "manual",
      parameterPrefillEnabled: true,
      riskRulesJson: "{}",
      ...defaultRiskRuleFormValues,
      notifyOnSuccess: false,
      notifyOnFailure: true,
      notifyOnUnstable: true,
      notifyOnAborted: true,
      description: "",
      enabled: false,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(record: JenkinsConnection) {
    void loadReferenceOptions();
    setEditing(record);
    form.setFieldsValue({
      connectionKey: record.connectionKey,
      name: record.name,
      baseUrl: record.baseUrl,
      credentialKey: record.credentialKey,
      credentialDisplayName: record.credentialDisplayName,
      usernameMasked: record.usernameMasked,
      sshServerAlias: record.sshServerAlias,
      environment: record.environment,
      environmentLabel: record.environmentLabel,
      tlsVerify: record.tlsVerify,
      defaultView: record.defaultView,
      defaultFolder: record.defaultFolder,
      allowMcpRead: record.allowMcpRead,
      allowMcpWrite: record.allowMcpWrite,
      approvalPolicy: record.approvalPolicy,
      parameterPrefillEnabled: record.parameterPrefillEnabled,
      riskRulesJson: record.riskRulesJson,
      ...parseRiskRuleFormValues(record.riskRulesJson),
      notifyOnSuccess: record.notifyOnSuccess,
      notifyOnFailure: record.notifyOnFailure,
      notifyOnUnstable: record.notifyOnUnstable,
      notifyOnAborted: record.notifyOnAborted,
      description: record.description,
      enabled: record.enabled,
    });
    setDrawerOpen(true);
  }

  function selectCredentialKey(credentialKey: string | undefined) {
    if (!credentialKey) {
      form.setFieldsValue({
        credentialKey: "",
        credentialDisplayName: "",
        usernameMasked: "",
      });
      return;
    }
    const credential = secureCredentials.find((item) => item.credentialKey === credentialKey);
    form.setFieldsValue({
      credentialKey,
      credentialDisplayName: credential?.displayName ?? "",
      usernameMasked: credential?.accountName || credential?.secretMasked || "",
    });
  }

  async function submitConnection() {
    try {
      const values = await form.validateFields();
      if (values.enabled && !values.connectionKey) {
        const saved = await jenkinsApi.upsertConnection(
          buildConnectionInput({
            ...values,
            enabled: false,
          }),
        );
        form.setFieldsValue({
          connectionKey: saved.connectionKey,
          enabled: false,
          credentialDisplayName: saved.credentialDisplayName,
          usernameMasked: saved.usernameMasked,
        });
        setEditing(saved);
        setSelectedKey(saved.connectionKey);
        await loadConnections();
        message.warning("新建 Jenkins 连接已保存为未启用草稿，请先测试连接成功后再启用");
        return;
      }
      if (values.enabled && (values.environment === "prod" || values.allowMcpWrite)) {
        await new Promise<void>((resolve, reject) => {
          Modal.confirm({
            title: "确认启用 Jenkins 写入连接？",
            content: "生产环境或 MCP 写入开启后，构建触发、停止构建等写入操作将进入审批链路。",
            okText: "确认启用",
            cancelText: "取消",
            onOk: () => resolve(),
            onCancel: () => reject(new Error("cancelled")),
          });
        });
      }
      const saved = await jenkinsApi.upsertConnection(buildConnectionInput(values));
      message.success("Jenkins 连接已保存");
      setDrawerOpen(false);
      setSelectedKey(saved.connectionKey);
      await loadConnections();
    } catch (error) {
      if (error instanceof Error && error.message === "cancelled") {
        return;
      }
      message.error(getErrorMessage(error));
    }
  }

  async function saveConnectionForTest(values: ConnectionFormValues) {
    return jenkinsApi.upsertConnection(
      buildConnectionInput({
        ...values,
        enabled: false,
      }),
    );
  }

  async function testDrawerConnection() {
    setDrawerTesting(true);
    try {
      const values = await form.validateFields();
      const saved = await saveConnectionForTest(values);
      form.setFieldsValue({
        connectionKey: saved.connectionKey,
        credentialDisplayName: saved.credentialDisplayName,
        usernameMasked: saved.usernameMasked,
      });
      setEditing(saved);
      setSelectedKey(saved.connectionKey);
      const result = await jenkinsApi.testConnection(saved.connectionKey);
      if (result.ok) {
        setEditing((current) =>
          current
            ? {
                ...current,
                status: result.status,
                version: result.version,
                lastErrorCode: "",
                lastErrorMessage: "",
              }
            : current,
        );
        message.success("Jenkins 连接测试成功，当前连接可以启用");
      } else {
        message.warning(result.message);
      }
      await loadConnections();
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setDrawerTesting(false);
    }
  }

  async function handleTest(record: JenkinsConnection) {
    try {
      const result = await jenkinsApi.testConnection(record.connectionKey);
      if (result.ok) {
        message.success(result.message);
      } else {
        message.warning(result.message);
      }
      await loadConnections();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function handleDelete(record: JenkinsConnection) {
    try {
      await jenkinsApi.deleteConnection(record.connectionKey);
      message.success("已软删除 Jenkins 连接");
      await loadConnections();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function handleRestore(record: JenkinsConnection) {
    try {
      const restored = await jenkinsApi.restoreConnection(record.connectionKey);
      message.success("已恢复 Jenkins 连接");
      setSelectedKey(restored.connectionKey);
      await loadConnections();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function handleDuplicate(record: JenkinsConnection) {
    try {
      const copied = await jenkinsApi.duplicateConnection(record.connectionKey);
      message.success("已复制 Jenkins 连接配置");
      setSelectedKey(copied.connectionKey);
      await loadConnections();
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function toggleJobFavorite(record: JenkinsJob) {
    const connectionKey = selectedConnection?.connectionKey;
    if (!connectionKey) {
      message.warning("请先选择 Jenkins 连接");
      return;
    }
    const nextFavorite = !record.favorite;
    setJobs((current) =>
      updateJobInTree(current, record.jobFullName, (item) => ({ ...item, favorite: nextFavorite })),
    );
    try {
      await jenkinsApi.setJobFavorite({
        connectionKey,
        jobFullName: record.jobFullName,
        favorite: nextFavorite,
        requester: "local-user",
      });
      message.success(nextFavorite ? "已收藏 Job" : "已取消收藏");
    } catch (error) {
      setJobs((current) =>
        updateJobInTree(current, record.jobFullName, (item) => ({ ...item, favorite: record.favorite })),
      );
      message.error(getErrorMessage(error));
    }
  }

  async function openBuildDetail(record: JenkinsBuild) {
    if (!record.buildNumber) {
      message.warning("该构建没有可读取的构建号");
      return;
    }
    setSelectedBuild(record);
    setSelectedBuildDetail(null);
    setBuildLog(null);
    setFailureLogSummary(null);
    setBuildAnalysis(null);
    setArtifacts([]);
    setStopReason(`停止 Jenkins 构建 ${record.jobFullName} #${record.buildNumber}`);
    setBuildDrawerOpen(true);
    setBuildDetailLoading(true);
    try {
      const detailInput = {
        connectionKey: record.connectionKey,
        jobFullName: record.jobFullName,
        buildNumber: record.buildNumber,
      };
      const [detail, latestAnalysis] = await Promise.all([
        jenkinsApi.getBuildDetail(detailInput),
        jenkinsApi.getLatestBuildAnalysis(detailInput),
        loadArtifacts(record),
      ]);
      setSelectedBuildDetail(detail);
      setBuildAnalysis(latestAnalysis);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setBuildDetailLoading(false);
    }
  }

  async function syncJobBuildRecords(record: JenkinsJob) {
    if (!selectedConnection) {
      message.warning("请先选择 Jenkins 连接");
      return;
    }
    setBuildSyncingJob(record.jobFullName);
    try {
      const rows = await jenkinsApi.listBuilds({
        connectionKey: selectedConnection.connectionKey,
        jobFullName: record.jobFullName,
        limit: 30,
      });
      setSelectedJob(record);
      setBuilds(rows);
      setActiveDetailTab("builds");
      message.success(`已同步 ${rows.length} 条构建运行记录`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setBuildSyncingJob("");
    }
  }

  async function showAllBuildRecords() {
    if (!selectedConnection) {
      message.warning("请先选择 Jenkins 连接");
      return;
    }
    setSelectedJob(null);
    await refreshBuildsAndQueue(selectedConnection.connectionKey, null);
    setActiveDetailTab("builds");
  }

  async function showLatestBuildRecordsAfterTrigger(connectionKey: string) {
    setSelectedJob(null);
    await refreshBuildsAndQueue(connectionKey, null);
    setActiveDetailTab("builds");
  }

  function handleDetailTabChange(key: string) {
    setActiveDetailTab(key);
    if (key === "builds" && selectedJob && selectedConnection) {
      void showAllBuildRecords();
    }
  }

  async function loadArtifacts(record: JenkinsBuild) {
    if (!record.buildNumber) {
      setArtifacts([]);
      return;
    }
    setArtifactLoading(true);
    try {
      const list = await jenkinsApi.listArtifacts({
        connectionKey: record.connectionKey,
        jobFullName: record.jobFullName,
        buildNumber: record.buildNumber,
      });
      setArtifacts(list);
      setDeploymentCandidate(null);
      setDeploymentDryRunPlan(null);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setArtifactLoading(false);
    }
  }

  async function downloadArtifact(record: JenkinsArtifact) {
    setArtifactDownloading(record.relativePath);
    try {
      const saved = await jenkinsApi.downloadArtifact({
        connectionKey: record.connectionKey,
        jobFullName: record.jobFullName,
        buildNumber: record.buildNumber,
        relativePath: record.relativePath,
      });
      message.success("artifact 已下载到应用托管目录");
      setArtifacts((current) =>
        current.map((item) => (item.relativePath === saved.relativePath ? { ...item, ...saved } : item)),
      );
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setArtifactDownloading("");
    }
  }

  async function cleanupArtifact(record: JenkinsArtifact) {
    setArtifactCleaning(record.artifactKey);
    try {
      const cleaned = await jenkinsApi.cleanupArtifactLocalFile({
        artifactKey: record.artifactKey,
      });
      message.success(cleaned.status === "file_missing" ? "本地文件不存在，已更新状态" : "本地 artifact 文件已清理");
      setArtifacts((current) =>
        current.map((item) => (item.artifactKey === cleaned.artifactKey ? { ...item, ...cleaned } : item)),
      );
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setArtifactCleaning("");
    }
  }

  async function createArtifactDeploymentCandidate(record: JenkinsArtifact) {
    if (isDeploymentBlockedBuildLike(selectedBuildDetail ?? selectedBuild)) {
      message.warning("该 Jenkins 构建未成功，不能创建部署候选");
      return;
    }
    setArtifactCandidateCreating(record.artifactKey);
    try {
      const candidate = await jenkinsApi.createArtifactDeploymentCandidate({
        artifactKey: record.artifactKey,
      });
      setDeploymentCandidate(candidate);
      setDeploymentDryRunPlan(null);
      setDeploymentDryRunServerAlias(selectedConnection?.sshServerAlias || "");
      setDeploymentDryRunDeployRoot(`/opt/tauri-ssh/stacks/${candidate.key}`);
      setDeploymentDryRunPort(candidate.exposedPorts[0] ?? null);
      message.success("已生成部署候选");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setArtifactCandidateCreating("");
    }
  }

  async function createBuildDeploymentDryRun() {
    if (!deploymentCandidate) {
      message.warning("请先生成部署候选");
      return;
    }
    if (!deploymentDryRunServerAlias.trim()) {
      message.warning("请填写目标服务器别名");
      return;
    }
    setDeploymentDryRunLoading(true);
    setDeploymentDryRunPlan(null);
    try {
      const plan = await jenkinsApi.createBuildDeploymentDryRun({
        artifactKey: getDeploymentCandidateArtifactKey(deploymentCandidate),
        serverAlias: deploymentDryRunServerAlias.trim(),
        deployRoot: deploymentDryRunDeployRoot.trim() || undefined,
        port: deploymentDryRunPort,
      });
      setDeploymentDryRunPlan(plan);
      message.success("部署 Dry-run 已生成");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setDeploymentDryRunLoading(false);
    }
  }

  async function loadBuildLog(record: JenkinsBuild, start = 0, options: { quiet?: boolean } = {}) {
    if (!record.buildNumber) {
      if (!options.quiet) {
        message.warning("该构建没有可读取的构建号");
      }
      return;
    }
    if (start === 0) {
      setFailureLogSummary(null);
      setBuildAnalysis(null);
    }
    buildLogLoadingRef.current = true;
    setBuildLogLoading(true);
    try {
      const result = await jenkinsApi.readBuildLog({
        connectionKey: record.connectionKey,
        jobFullName: record.jobFullName,
        buildNumber: record.buildNumber,
        start,
        requestId: start > 0 ? buildLogRef.current?.requestId : undefined,
      });
      setBuildLog((current) =>
        start > 0 && current
          ? {
              ...result,
              start: current.start,
              text: `${current.text}${result.text}`,
            }
          : result,
      );
    } catch (error) {
      if (options.quiet) {
        console.warn("Jenkins 构建日志自动读取失败", error);
      } else {
        message.error(getErrorMessage(error));
      }
    } finally {
      buildLogLoadingRef.current = false;
      setBuildLogLoading(false);
    }
  }

  useEffect(() => {
    if (!buildDrawerOpen || !selectedBuild?.buildNumber) {
      return;
    }
    let disposed = false;
    const readNextLogChunk = async () => {
      if (disposed || buildLogLoadingRef.current) {
        return;
      }
      const currentLog = buildLogRef.current;
      const currentBuild = selectedBuildDetail ?? selectedBuild;
      if (currentLog && !currentLog.hasMore && !unfinishedBuildStatuses.has(currentBuild.status)) {
        return;
      }
      await loadBuildLog(selectedBuild, currentLog?.nextStart ?? 0, { quiet: true });
    };
    void readNextLogChunk();
    const timer = window.setInterval(() => void readNextLogChunk(), 5000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [
    buildDrawerOpen,
    selectedBuild?.connectionKey,
    selectedBuild?.jobFullName,
    selectedBuild?.buildNumber,
    selectedBuild?.status,
    selectedBuildDetail?.status,
  ]);

  function extractLoadedFailureLogSummary() {
    if (!loadedLogText.trim()) {
      message.warning("请先读取构建日志");
      return;
    }
    const summary = extractFailureLogSummary(loadedLogText);
    if (!summary) {
      setFailureLogSummary(null);
      message.info("已加载日志中没有命中失败关键字");
      return;
    }
    setFailureLogSummary(summary);
    message.success(`已提取 ${summary.matchedLines} 行失败关键字上下文`);
  }

  async function generateFailureAnalysis() {
    if (!selectedBuild || !selectedBuild.buildNumber || !failureLogSummary) {
      message.warning("请先读取日志并提取失败片段");
      return;
    }
    setAnalysisLoading(true);
    try {
      const analysis = await jenkinsApi.generateFailureAnalysis({
        connectionKey: selectedBuild.connectionKey,
        jobFullName: selectedBuild.jobFullName,
        buildNumber: selectedBuild.buildNumber,
        runKey: selectedBuildDetail?.runKey || selectedBuild.runKey,
        requestId: buildLog?.requestId || selectedBuild.requestId,
        logSnippet: failureLogSummary.text,
        snippetStartLine: failureLogSummary.startLine,
        snippetEndLine: failureLogSummary.endLine,
        matchedLines: failureLogSummary.matchedLines,
        requester: "local-user",
      });
      setBuildAnalysis(analysis);
      message.success("已生成并保存本地 AI 失败总结");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setAnalysisLoading(false);
    }
  }

  async function openBuildLog(record: JenkinsBuild) {
    await openBuildDetail(record);
  }

  async function copyLoadedBuildLog() {
    if (!selectedBuild || !selectedBuild.buildNumber || !buildLog?.text) {
      message.warning("当前没有可复制的日志内容");
      return;
    }
    try {
      await navigator.clipboard.writeText(buildLog.text);
      await jenkinsApi.recordLogCopyAudit({
        connectionKey: selectedBuild.connectionKey,
        jobFullName: selectedBuild.jobFullName,
        buildNumber: selectedBuild.buildNumber,
        requestId: buildLog.requestId,
        startOffset: buildLog.start,
        endOffset: buildLog.nextStart,
        bytes: new TextEncoder().encode(buildLog.text).length,
        redacted: buildLog.redacted,
        rawLogAccess: false,
        confirmationSource: "ui-loaded-log",
      });
      message.success("已复制已加载日志，并记录复制审计");
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function openParameterForm(record: JenkinsJob, refresh = false) {
    if (!selectedConnection) {
      message.warning("请先选择 Jenkins 连接");
      return;
    }
    const loadSeq = parameterLoadSeq.current + 1;
    parameterLoadSeq.current = loadSeq;
    setSelectedJob(record);
    setParameterDrawerOpen(true);
    setApprovalReason("");
    setSelectedTemplateKey("");
    setTemplateName("");
    setSelectedGitWorkspaceKey("");
    setGitWorkspaceStatus(null);
    setParameterResult(null);
    setRecentParameterValues([]);
    setParameterTemplates([]);
    setParameterValues({});
    parameterForm.resetFields();
    setParameterLoading(true);
    try {
      const [result, recentValues, templates, workspaces] = await Promise.all([
        jenkinsApi.listParameters({
          connectionKey: selectedConnection.connectionKey,
          jobFullName: record.jobFullName,
          refresh,
        }),
        jenkinsApi.listRecentParameterValues({
          connectionKey: selectedConnection.connectionKey,
          jobFullName: record.jobFullName,
          requester: "local-user",
        }),
        jenkinsApi.listParameterTemplates({
          connectionKey: selectedConnection.connectionKey,
          jobFullName: record.jobFullName,
          requester: "local-user",
        }),
        gitWorkspaceApi.list({}),
      ]);
      if (parameterLoadSeq.current !== loadSeq) {
        return;
      }
      setParameterResult(result);
      setRecentParameterValues(recentValues);
      setParameterTemplates(templates);
      setGitWorkspaces(workspaces);
      const initialValues = buildInitialParameterValues(result.parameters, recentValues);
      parameterForm.setFieldsValue(initialValues);
      setParameterValues(initialValues);
    } catch (error) {
      if (parameterLoadSeq.current !== loadSeq) {
        return;
      }
      message.error(getErrorMessage(error));
      setParameterResult(null);
      setRecentParameterValues([]);
      setParameterTemplates([]);
      setGitWorkspaces([]);
      parameterForm.resetFields();
      setParameterValues({});
    } finally {
      if (parameterLoadSeq.current === loadSeq) {
        setParameterLoading(false);
      }
    }
  }

  async function refreshParameterForm() {
    if (!selectedJob) {
      return;
    }
    await openParameterForm(selectedJob, true);
  }

  async function copyParameterSummary() {
    if (!parameterResult) {
      return;
    }
    const summary = buildSafeParameterSummary(parameterResult.parameters, parameterValues);
    try {
      await navigator.clipboard.writeText(JSON.stringify(summary, null, 2));
      message.success("已复制参数摘要");
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function saveParameterTemplate() {
    if (!selectedConnection || !selectedJob || !parameterResult) {
      message.warning("请先选择连接和 Job，并读取参数定义");
      return;
    }
    const name = templateName.trim();
    if (!name) {
      message.warning("请输入模板名称");
      return;
    }
    const summary = buildSafeParameterSummary(parameterResult.parameters, parameterValues);
    setTemplateLoading(true);
    try {
      const saved = await jenkinsApi.upsertParameterTemplate({
        connectionKey: selectedConnection.connectionKey,
        jobFullName: selectedJob.jobFullName,
        name,
        parametersJson: summary,
        parameterDefinitionHash: parameterResult.parameterDefinitionHash,
        requester: "local-user",
      });
      setParameterTemplates((current) => [saved, ...current.filter((item) => item.templateKey !== saved.templateKey)]);
      setSelectedTemplateKey(saved.templateKey);
      message.success(`已保存参数模板：${saved.name}`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setTemplateLoading(false);
    }
  }

  function applyParameterTemplate() {
    if (!parameterResult) {
      return;
    }
    const template = parameterTemplates.find((item) => item.templateKey === selectedTemplateKey);
    if (!template) {
      message.warning("请选择参数模板");
      return;
    }
    const values = templateSummaryToFormValues(parameterResult.parameters, template.parametersJson);
    parameterForm.setFieldsValue(values);
    setParameterValues((current) => ({ ...current, ...values }));
    setTemplateName(template.name);
    message.success(`已套用参数模板：${template.name}`);
  }

  async function selectGitWorkspaceForParameters(workspaceKey: string) {
    setSelectedGitWorkspaceKey(workspaceKey);
    setGitWorkspaceStatus(null);
    if (!workspaceKey) {
      return;
    }
    setGitWorkspaceLoading(true);
    try {
      const status = await gitWorkspaceApi.status(workspaceKey);
      setGitWorkspaceStatus(status);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setGitWorkspaceLoading(false);
    }
  }

  async function refreshSelectedGitWorkspaceStatus() {
    if (!selectedGitWorkspaceKey) {
      message.warning("请先选择 Git 工作区");
      return;
    }
    await selectGitWorkspaceForParameters(selectedGitWorkspaceKey);
  }

  function applyGitWorkspaceParameters() {
    if (!parameterResult || !gitWorkspaceStatus) {
      message.warning("请先选择 Git 工作区并读取状态");
      return;
    }
    const values = buildGitWorkspaceParameterValues(parameterResult.parameters, gitWorkspaceStatus);
    if (Object.keys(values).length === 0) {
      message.warning("未找到可注入的 branch/commit 类参数");
      return;
    }
    parameterForm.setFieldsValue(values);
    setParameterValues((current) => ({ ...current, ...values }));
    message.success(`已注入 ${Object.keys(values).length} 个 Git 参数`);
  }

  async function deleteParameterTemplate() {
    const template = parameterTemplates.find((item) => item.templateKey === selectedTemplateKey);
    if (!template) {
      message.warning("请选择参数模板");
      return;
    }
    setTemplateLoading(true);
    try {
      const deleted = await jenkinsApi.deleteParameterTemplate({
        templateKey: template.templateKey,
        requester: "local-user",
      });
      setParameterTemplates((current) => current.filter((item) => item.templateKey !== template.templateKey));
      setSelectedTemplateKey("");
      if (templateName.trim() === template.name) {
        setTemplateName("");
      }
      message.success(deleted ? "已删除参数模板" : "该参数模板不存在或无权删除");
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setTemplateLoading(false);
    }
  }

  async function createBuildTriggerApproval() {
    if (!selectedConnection || !selectedJob || !parameterResult) {
      message.warning("请先选择连接和 Job，并读取参数定义");
      return;
    }
    const noApproval = selectedConnection.approvalPolicy === "none";
    const reason = approvalReason.trim();
    if (!noApproval && !reason) {
      message.warning("请输入构建审批理由");
      return;
    }
    const summary = buildSafeParameterSummary(parameterResult.parameters, parameterValues);
    setApprovalCreating(true);
    try {
      if (noApproval) {
        const result = await jenkinsApi.triggerWithoutApproval({
          connectionKey: selectedConnection.connectionKey,
          jobFullName: selectedJob.jobFullName,
          parameterDefinitionHash: parameterResult.parameterDefinitionHash,
          parametersJson: summary,
          requester: "local-user",
          reason,
        });
        message.success(
          result.buildNumber
            ? `已触发 Jenkins 构建 #${result.buildNumber}`
            : `已触发 Jenkins 构建，队列 ID：${result.queueId || "-"}`,
        );
        setParameterDrawerOpen(false);
        await showLatestBuildRecordsAfterTrigger(selectedConnection.connectionKey);
        return;
      }
      const approval = await jenkinsApi.createTriggerApproval({
        connectionKey: selectedConnection.connectionKey,
        jobFullName: selectedJob.jobFullName,
        parameterDefinitionHash: parameterResult.parameterDefinitionHash,
        parametersJson: summary,
        requester: "local-user",
        reason,
      });
      if (approval.status === "approved") {
        const result = await jenkinsApi.executeTriggerApproved({
          approvalId: approval.id,
          requestHash: approval.command,
        });
        message.success(
          result.buildNumber
            ? `已触发 Jenkins 构建 #${result.buildNumber}`
            : `已触发 Jenkins 构建，队列 ID：${result.queueId || "-"}`,
        );
        setParameterDrawerOpen(false);
        await showLatestBuildRecordsAfterTrigger(selectedConnection.connectionKey);
      } else {
        message.success(`已创建构建审批 #${approval.id}，批准后将触发 Jenkins 构建`);
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setApprovalCreating(false);
    }
  }

  async function createBuildStopApproval() {
    if (!selectedBuild?.buildNumber) {
      message.warning("请选择带构建号的构建记录");
      return;
    }
    if (!isStoppableBuildLike(selectedBuildDetail ?? selectedBuild)) {
      message.warning("该构建已结束，不能停止");
      return;
    }
    const connection = connections.find((item) => item.connectionKey === selectedBuild.connectionKey) ?? selectedConnection;
    const noApproval = connection?.approvalPolicy === "none";
    const reason = stopReason.trim();
    if (!noApproval && !reason) {
      message.warning("请输入停止构建审批理由");
      return;
    }
    setStopApprovalCreating(true);
    try {
      if (noApproval) {
        await jenkinsApi.stopWithoutApproval({
          connectionKey: selectedBuild.connectionKey,
          jobFullName: selectedBuild.jobFullName,
          buildNumber: selectedBuild.buildNumber,
          requester: "local-user",
          reason,
        });
        message.success("已发送停止 Jenkins 构建请求");
        await refreshBuildsAndQueue(selectedBuild.connectionKey);
        return;
      }
      const approval = await jenkinsApi.createStopApproval({
        connectionKey: selectedBuild.connectionKey,
        jobFullName: selectedBuild.jobFullName,
        buildNumber: selectedBuild.buildNumber,
        requester: "local-user",
        reason,
      });
      message.success(`已创建停止构建审批 #${approval.id}`);
    } catch (error) {
      message.error(getErrorMessage(error));
    } finally {
      setStopApprovalCreating(false);
    }
  }

  async function forgetRecentParameterValue(parameterName: string) {
    if (!selectedConnection || !selectedJob) {
      return;
    }
    try {
      const deleted = await jenkinsApi.forgetRecentParameterValue({
        connectionKey: selectedConnection.connectionKey,
        jobFullName: selectedJob.jobFullName,
        parameterName,
        requester: "local-user",
      });
      setRecentParameterValues((current) => current.filter((item) => item.parameterName !== parameterName));
      message.success(deleted ? "已忘记该参数最近值" : "该参数没有可忘记的最近值");
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function inspectFileParameterPath(parameterName: string, localPath: string) {
    const trimmed = localPath.trim();
    if (!trimmed) {
      message.warning("请先选择或输入本地文件路径");
      return;
    }
    try {
      const metadata = await jenkinsApi.inspectFileParameter({
        parameterName,
        localPath: trimmed,
      });
      parameterForm.setFieldValue(parameterName, metadata);
      setParameterValues((current) => ({
        ...current,
        [parameterName]: metadata,
      }));
      message.success("已读取文件参数元数据");
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  async function chooseFileParameter(parameterName: string) {
    if (!hasTauriRuntime()) {
      message.info("浏览器预览模式下请手工输入本地绝对路径后读取元数据");
      return;
    }
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        title: "选择 Jenkins File Parameter 文件",
      });
      if (typeof selected === "string") {
        await inspectFileParameterPath(parameterName, selected);
      }
    } catch (error) {
      message.error(getErrorMessage(error));
    }
  }

  const connectionColumns: ColumnsType<JenkinsConnection> = [
    {
      title: "连接",
      dataIndex: "name",
      width: 220,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Button type="link" className="p-0 h-auto" onClick={() => switchJenkinsConnection(record.connectionKey)}>
            {record.name}
          </Button>
          <Text type="secondary" className="text-xs">
            {record.connectionKey}
          </Text>
        </Space>
      ),
    },
    {
      title: "当前",
      dataIndex: "connectionKey",
      width: 100,
      render: (_, record) =>
        record.connectionKey === selectedKey ? (
          <Tag color="blue">当前</Tag>
        ) : (
          <Button size="small" onClick={() => switchJenkinsConnection(record.connectionKey)}>
            切换
          </Button>
        ),
    },
    { title: "环境", dataIndex: "environment", width: 90 },
    { title: "状态", dataIndex: "status", width: 110, render: (value) => statusTag(String(value)) },
    {
      title: "Base URL",
      dataIndex: "baseUrl",
      ellipsis: true,
    },
    {
      title: "操作",
      width: 270,
      fixed: "right",
      render: (_, record) => (
        <Space size={4} wrap>
          <Button size="small" icon={<Edit size={14} />} onClick={() => openEditDrawer(record)} />
          <Button size="small" icon={<PlayCircle size={14} />} onClick={() => handleTest(record)} />
          <Button size="small" icon={<Copy size={14} />} onClick={() => handleDuplicate(record)} />
          {record.deletedAt ? (
            <Button size="small" icon={<RotateCcw size={14} />} onClick={() => handleRestore(record)} />
          ) : (
            <Popconfirm title="确认软删除该 Jenkins 连接？" onConfirm={() => handleDelete(record)}>
              <Button size="small" danger icon={<Trash2 size={14} />} />
            </Popconfirm>
          )}
        </Space>
      ),
    },
  ];

  const jobColumns: ColumnsType<JenkinsJob> = [
    {
      title: "Job",
      dataIndex: "displayName",
      ellipsis: true,
      render: (_, record) => (
        <Space size={8}>
          {record.jobType === "folder" ? <Folder size={16} /> : <FileText size={16} />}
          <Text strong={record.jobType === "folder"}>{record.displayName || record.jobFullName}</Text>
        </Space>
      ),
    },
    { title: "类型", dataIndex: "jobType", width: 120, render: (value) => formatJobType(String(value)) },
    { title: "状态", dataIndex: "normalizedStatus", width: 120, render: (_, record) => jobStatusTag(record) },
    { title: "最近构建", dataIndex: "lastBuildNumber", width: 120, render: (value) => value ?? "-" },
    {
      title: "操作",
      width: 240,
      fixed: "right",
      render: (_, record) => (
        <Space size={4}>
          <Button
            size="small"
            type={record.favorite ? "primary" : "default"}
            icon={<Star size={14} fill={record.favorite ? "currentColor" : "none"} />}
            onClick={() => toggleJobFavorite(record)}
          />
          {record.jobType === "folder" ? null : (
            <>
              <Button
                size="small"
                icon={<FileText size={14} />}
                loading={buildSyncingJob === record.jobFullName}
                onClick={() => syncJobBuildRecords(record)}
              >
                历史
              </Button>
              <Button size="small" icon={<PlayCircle size={14} />} onClick={() => openParameterForm(record)}>
                构建
              </Button>
            </>
          )}
        </Space>
      ),
    },
  ];

  const buildColumns: ColumnsType<JenkinsBuild> = [
    { title: "Job", dataIndex: "jobFullName", ellipsis: true },
    { title: "构建号", dataIndex: "buildNumber", width: 100, render: (value) => value ?? "-" },
    { title: "状态", dataIndex: "status", width: 130, render: (value) => statusTag(String(value)) },
    { title: "来源", dataIndex: "statusSource", width: 100 },
    {
      title: "开始时间",
      dataIndex: "startedAt",
      width: 170,
      render: (value) => formatJenkinsBuildTime(value),
    },
    {
      title: "操作",
      width: 150,
      fixed: "right",
      render: (_, record) => (
        <Space size={4}>
          <Button size="small" icon={<FileText size={14} />} onClick={() => openBuildDetail(record)}>
            详情
          </Button>
          <Button size="small" onClick={() => openBuildLog(record)}>
            日志
          </Button>
        </Space>
      ),
    },
  ];

  const artifactColumns: ColumnsType<JenkinsArtifact> = [
    {
      title: "Artifact",
      dataIndex: "relativePath",
      ellipsis: true,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text>{record.fileName || record.relativePath}</Text>
          <Text type="secondary" className="text-xs">
            {record.relativePath}
          </Text>
        </Space>
      ),
    },
    {
      title: "风险",
      dataIndex: "riskFlags",
      width: 120,
      render: (flags: string[]) =>
        flags?.length ? <Tag color="orange">高风险</Tag> : <Tag color="green">普通</Tag>,
    },
    {
      title: "状态",
      dataIndex: "status",
      width: 110,
      render: (value) => <Tag>{String(value || "remote")}</Tag>,
    },
    {
      title: "大小",
      dataIndex: "sizeBytes",
      width: 110,
      render: (value) => formatBytes(value),
    },
    {
      title: "操作",
      width: 280,
      render: (_, record) => (
        <Space size={4}>
          <Button
            size="small"
            icon={<Download size={14} />}
            loading={artifactDownloading === record.relativePath}
            disabled={record.status === "available"}
            onClick={() => downloadArtifact(record)}
          >
            下载
          </Button>
          {record.localPath ? (
            <Button
              size="small"
              icon={<PackageCheck size={14} />}
              loading={artifactCandidateCreating === record.artifactKey}
              disabled={record.status !== "available" || isDeploymentBlockedBuildLike(selectedBuildDetail ?? selectedBuild)}
              onClick={() => createArtifactDeploymentCandidate(record)}
            >
              部署候选
            </Button>
          ) : null}
          {record.localPath ? (
            <Popconfirm title="确认清理该 artifact 的本地文件？记录、审批和审计日志会保留。" onConfirm={() => cleanupArtifact(record)}>
              <Button
                size="small"
                danger
                icon={<Trash2 size={14} />}
                loading={artifactCleaning === record.artifactKey}
                disabled={record.status === "local_deleted"}
              >
                清理
              </Button>
            </Popconfirm>
          ) : null}
        </Space>
      ),
    },
  ];

  const deploymentDryRunStageColumns: ColumnsType<DeploymentPlanStage> = [
    {
      title: "阶段",
      dataIndex: "title",
      width: 150,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.title}</Text>
          <Text type="secondary" className="text-xs">
            {record.key}
          </Text>
        </Space>
      ),
    },
    {
      title: "风险",
      dataIndex: "risk",
      width: 100,
      render: (value) => deploymentRiskTag(String(value)),
    },
    {
      title: "审批",
      dataIndex: "approvalRequired",
      width: 100,
      render: (value) => <Tag color={value ? "red" : "green"}>{value ? "需要" : "不需要"}</Tag>,
    },
    { title: "命令预览", dataIndex: "commandPreview", ellipsis: true },
    { title: "说明", dataIndex: "summary", ellipsis: true },
  ];

  return (
    <div className="prototype-page">
      <div className="prototype-page-header">
        <div>
          <Title level={3}>Jenkins 构建运维工作台</Title>
          <Paragraph type="secondary">
            首版聚焦连接配置、Job/构建读取、详情日志查看、受控构建触发和停止审批。
          </Paragraph>
        </div>
        <Space>
          <Button icon={<RefreshCw size={16} />} onClick={loadConnections}>
            刷新
          </Button>
          <Button type="primary" icon={<Plus size={16} />} onClick={openCreateDrawer}>
            新增连接
          </Button>
        </Space>
      </div>

      <Card>
        <Space className="mb-4" wrap>
          <Input.Search
            allowClear
            placeholder="搜索名称、URL 或连接 Key"
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            onSearch={() => loadConnections()}
            style={{ width: 320 }}
          />
          <Switch checked={includeDeleted} onChange={setIncludeDeleted} />
          <Text>显示已删除</Text>
        </Space>
        <Table
          rowKey="connectionKey"
          loading={loading}
          columns={connectionColumns}
          dataSource={connections}
          rowSelection={{
            type: "radio",
            selectedRowKeys: selectedKey ? [selectedKey] : [],
            onChange: (keys) => switchJenkinsConnection(String(keys[0] ?? "")),
          }}
          scroll={{ x: 1080 }}
          rowClassName={(record) => (record.connectionKey === selectedKey ? "ant-table-row-selected" : "")}
          pagination={{ pageSize: 8 }}
        />
      </Card>

      <Card className="mt-4">
        {selectedConnection ? (
          <>
            <Descriptions size="small" column={3} bordered>
              <Descriptions.Item label="当前连接">{selectedConnection.name}</Descriptions.Item>
              <Descriptions.Item label="状态">{statusTag(selectedConnection.status)}</Descriptions.Item>
              <Descriptions.Item label="配置版本">v{selectedConnection.configVersion}</Descriptions.Item>
              <Descriptions.Item label="URL" span={2}>
                {selectedConnection.baseUrl}
              </Descriptions.Item>
              <Descriptions.Item label="凭证">
                {selectedConnection.credentialDisplayName || selectedConnection.credentialKey || "未配置"}
              </Descriptions.Item>
              <Descriptions.Item label="MCP 只读">{selectedConnection.allowMcpRead ? "允许" : "禁止"}</Descriptions.Item>
              <Descriptions.Item label="MCP 写入">{selectedConnection.allowMcpWrite ? "允许" : "禁止"}</Descriptions.Item>
              <Descriptions.Item label="TLS 校验">
                {selectedConnection.tlsVerify ? "启用" : <Tag color="red">已关闭</Tag>}
              </Descriptions.Item>
              <Descriptions.Item label="最近测试">{selectedConnection.lastTestedAt || "-"}</Descriptions.Item>
            </Descriptions>
            <Tabs
              className="mt-4"
              activeKey={activeDetailTab}
              onChange={handleDetailTabChange}
              items={[
                {
                  key: "jobs",
                  label: "Job",
                  children: (
                    <Space direction="vertical" size={12} className="w-full">
                      <Space wrap>
                        <Button
                          size="small"
                          disabled={jobFolderStack.length === 0}
                          onClick={() => setJobFolderStack((current) => current.slice(0, -1))}
                        >
                          返回上级
                        </Button>
                        <Button
                          size="small"
                          disabled={jobFolderStack.length === 0}
                          onClick={() => setJobFolderStack([])}
                        >
                          根目录
                        </Button>
                        <Text type="secondary">
                          {jobFolderStack.length > 0
                            ? jobFolderStack
                                .map((jobFullName) => {
                                  const folder = findJobInTree(jobs, jobFullName);
                                  return folder?.displayName || jobFullName;
                                })
                                .join(" / ")
                            : "根目录"}
                        </Text>
                      </Space>
                      <Table
                        rowKey="jobFullName"
                        loading={detailLoading}
                        columns={jobColumns}
                        dataSource={jobTableData}
                        onRow={(record) => ({
                          onDoubleClick: () => {
                            if (record.jobType === "folder") {
                              setJobFolderStack((current) => [...current, record.jobFullName]);
                            }
                          },
                        })}
                        rowClassName={(record) => (record.jobType === "folder" ? "cursor-pointer" : "")}
                        scroll={{ x: 760 }}
                      />
                    </Space>
                  ),
                },
                {
                  key: "builds",
                  label: "构建",
                  children: (
                    <Space direction="vertical" size={12} className="w-full">
                      <Space wrap>
                        {selectedJob ? (
                          <>
                            <Tag color="blue">当前 Job：{selectedJob.jobFullName}</Tag>
                            <Button size="small" onClick={() => void showAllBuildRecords()}>
                              全部历史
                            </Button>
                          </>
                        ) : (
                          <Tag>本机全部构建历史</Tag>
                        )}
                      </Space>
                      <Table
                        rowKey="runKey"
                        loading={detailLoading}
                        columns={buildColumns}
                        dataSource={buildTableData}
                        scroll={{ x: 900 }}
                      />
                    </Space>
                  ),
                },
                {
                  key: "queue",
                  label: "队列",
                  children: (
                    <Table
                      rowKey="queueId"
                      loading={detailLoading}
                      dataSource={queueItems}
                      columns={[
                        { title: "Queue ID", dataIndex: "queueId" },
                        { title: "Job", dataIndex: "jobFullName" },
                        { title: "状态", dataIndex: "status" },
                        { title: "说明", dataIndex: "message" },
                      ]}
                    />
                  ),
                },
              ]}
            />
          </>
        ) : (
          <Alert type="info" showIcon title="请先新增或选择一个 Jenkins 连接" />
        )}
      </Card>

      <Drawer
        title={editing ? "编辑 Jenkins 连接" : "新增 Jenkins 连接"}
        size="large"
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        extra={
          <Space>
            <Button icon={<PlayCircle size={14} />} loading={drawerTesting} onClick={testDrawerConnection}>
              测试连接
            </Button>
            <Button onClick={() => setDrawerOpen(false)}>取消</Button>
            <Button type="primary" onClick={submitConnection}>
              保存
            </Button>
          </Space>
        }
      >
        <Form form={form} layout="vertical">
          <Form.Item name="connectionKey" hidden>
            <Input />
          </Form.Item>
          <Form.Item name="name" label="连接名称" rules={[{ required: true, message: "请输入连接名称" }]}>
            <Input placeholder="例如：公司 Jenkins" />
          </Form.Item>
          <Form.Item
            name="baseUrl"
            label="Jenkins Base URL"
            rules={[{ required: true, message: "请输入 Jenkins Base URL" }]}
          >
            <Input placeholder="https://ci.example.com/jenkins" />
          </Form.Item>
          <Form.Item name="credentialKey" label="安全凭证 Key">
            <Select
              allowClear
              showSearch
              loading={referenceOptionsLoading}
              options={secureCredentialOptions}
              optionFilterProp="label"
              placeholder="选择安全凭证模块中的 Jenkins API Token"
              onChange={selectCredentialKey}
            />
          </Form.Item>
          <Form.Item name="credentialDisplayName" label="凭证显示名">
            <Input placeholder="例如：jenkins-user / api-token" />
          </Form.Item>
          <Form.Item name="usernameMasked" label="脱敏账号">
            <Input placeholder="例如：admin 或 j***s" />
          </Form.Item>
          <Form.Item name="sshServerAlias" label="SSH 隧道服务器">
            <Select
              allowClear
              showSearch
              loading={referenceOptionsLoading}
              options={sshServerOptions}
              optionFilterProp="label"
              placeholder="可选，选择内网 Jenkins 访问入口"
            />
          </Form.Item>
          <Space size={16} className="w-full" align="start">
            <Form.Item name="environment" label="环境" className="flex-1">
              <Select options={environmentOptions} />
            </Form.Item>
            <Form.Item name="environmentLabel" label="环境标签" className="flex-1">
              <Input placeholder="可选" />
            </Form.Item>
          </Space>
          <Space size={16} className="w-full" align="start">
            <Form.Item name="defaultView" label="默认 View" className="flex-1">
              <Input placeholder="可选" />
            </Form.Item>
            <Form.Item name="defaultFolder" label="默认 Folder" className="flex-1">
              <Input placeholder="可选" />
            </Form.Item>
          </Space>
          <Form.Item name="approvalPolicy" label="审批策略">
            <Select
              options={[
                { label: "手动审批", value: "manual" },
                { label: "按风险策略", value: "risk_based" },
                { label: "无需审批", value: "none" },
                { label: "禁止写入", value: "readonly" },
              ]}
            />
          </Form.Item>
          <Divider>风险规则</Divider>
          <Space size={16} className="w-full" align="start">
            <Form.Item name="riskFallbackRisk" label="未匹配默认风险" className="flex-1">
              <Select options={riskLevelOptions} />
            </Form.Item>
            <Form.Item name="riskEnvironmentRisk" label="环境风险" className="flex-1">
              <Select options={environmentRiskOptions} />
            </Form.Item>
          </Space>
          <Space size={16} className="w-full" align="start">
            <Form.Item name="riskFileParameterRisk" label="File Parameter 风险" className="flex-1">
              <Select options={riskLevelOptions} />
            </Form.Item>
            <Form.Item name="riskAllowConcurrentBuilds" label="允许同 Job 并发" valuePropName="checked" className="flex-1">
              <Switch />
            </Form.Item>
          </Space>
          <Form.Item name="riskAllowConcurrentPatternsText" label="并发白名单 Job 正则">
            <Input.TextArea rows={2} placeholder="每行一个正则；默认空，表示即使开启并发也不会放行任何 Job" />
          </Form.Item>
          <Form.List name="riskJobRules">
            {(fields, { add, remove }) => (
              <Space direction="vertical" size={8} className="w-full">
                <div className="flex items-center justify-between">
                  <Text strong>Job 风险规则</Text>
                  <Button size="small" onClick={() => add({ pattern: "", risk: "L2", enabled: true })}>
                    新增 Job 规则
                  </Button>
                </div>
                {fields.map((field) => (
                  <Space key={field.key} size={8} className="w-full" align="start">
                    <Form.Item
                      name={[field.name, "pattern"]}
                      className="flex-1"
                      rules={[{ required: true, message: "请输入 Job 正则" }]}
                    >
                      <Input placeholder="Job 正则，例如 .*(prod|release).*" />
                    </Form.Item>
                    <Form.Item name={[field.name, "risk"]} className="w-40">
                      <Select options={riskLevelOptions} />
                    </Form.Item>
                    <Form.Item name={[field.name, "enabled"]} valuePropName="checked" className="w-20">
                      <Switch />
                    </Form.Item>
                    <Button danger onClick={() => remove(field.name)}>
                      删除
                    </Button>
                  </Space>
                ))}
              </Space>
            )}
          </Form.List>
          <Form.List name="riskParameterRules">
            {(fields, { add, remove }) => (
              <Space direction="vertical" size={8} className="w-full mt-4">
                <div className="flex items-center justify-between">
                  <Text strong>参数风险规则</Text>
                  <Button size="small" onClick={() => add({ name: "", value: "", risk: "L2", enabled: true })}>
                    新增参数规则
                  </Button>
                </div>
                {fields.map((field) => (
                  <Space key={field.key} size={8} className="w-full" align="start">
                    <Form.Item
                      name={[field.name, "name"]}
                      className="w-40"
                      rules={[{ required: true, message: "请输入参数名" }]}
                    >
                      <Input placeholder="参数名" />
                    </Form.Item>
                    <Form.Item name={[field.name, "value"]} className="flex-1">
                      <Input placeholder="参数值或精确匹配值" />
                    </Form.Item>
                    <Form.Item name={[field.name, "risk"]} className="w-40">
                      <Select options={riskLevelOptions} />
                    </Form.Item>
                    <Form.Item name={[field.name, "enabled"]} valuePropName="checked" className="w-20">
                      <Switch />
                    </Form.Item>
                    <Button danger onClick={() => remove(field.name)}>
                      删除
                    </Button>
                  </Space>
                ))}
              </Space>
            )}
          </Form.List>
          <Form.Item hidden name="riskRulesJson">
            <Input />
          </Form.Item>
          <Form.Item shouldUpdate noStyle>
            {() => (
              <Form.Item label="风险规则 JSON 预览">
                <Input.TextArea rows={5} readOnly value={buildRiskRulesJson(form.getFieldsValue())} />
              </Form.Item>
            )}
          </Form.Item>
          <Form.Item name="description" label="说明">
            <Input.TextArea rows={3} />
          </Form.Item>
          <Space size={24} wrap>
            <Form.Item name="tlsVerify" label="TLS 校验" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="enabled" label="启用" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="allowMcpRead" label="MCP 只读" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="allowMcpWrite" label="MCP 写入" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="parameterPrefillEnabled" label="参数回填" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="notifyOnSuccess" label="成功通知" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="notifyOnFailure" label="失败通知" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="notifyOnUnstable" label="不稳定通知" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Form.Item name="notifyOnAborted" label="终止通知" valuePropName="checked">
              <Switch />
            </Form.Item>
          </Space>
          {tlsVerifyValue === false ? (
            <Alert
              className="mt-2"
              type="warning"
              showIcon
              message="TLS 校验已关闭"
              description="仅在内网自签证书或临时排障场景使用。关闭后无法验证 Jenkins 服务证书身份，连接测试、Job、构建和日志读取都会沿用该风险配置。"
            />
          ) : null}
        </Form>
      </Drawer>

      <Drawer
        title="触发 Jenkins 构建"
        size="large"
        open={parameterDrawerOpen}
        onClose={() => setParameterDrawerOpen(false)}
        extra={
          <Space>
            <Button loading={parameterLoading} onClick={refreshParameterForm}>
              刷新定义
            </Button>
            <Button disabled={!parameterResult} onClick={copyParameterSummary}>
              复制摘要
            </Button>
            <Button
              type="primary"
              disabled={!parameterResult}
              loading={approvalCreating}
              onClick={createBuildTriggerApproval}
            >
              {selectedConnection?.approvalPolicy === "none" ? "开始构建" : "创建构建审批"}
            </Button>
          </Space>
        }
      >
        {selectedJob ? (
          <Space direction="vertical" size={16} className="w-full">
            <Descriptions size="small" column={2} bordered>
              <Descriptions.Item label="Job" span={2}>
                {selectedJob.jobFullName}
              </Descriptions.Item>
              <Descriptions.Item label="参数定义 Hash" span={2}>
                <Text code copyable>
                  {parameterResult?.parameterDefinitionHash || "-"}
                </Text>
              </Descriptions.Item>
              <Descriptions.Item label="缓存来源">
                {parameterResult?.fromCache ? <Tag color="blue">缓存</Tag> : <Tag color="green">Jenkins</Tag>}
              </Descriptions.Item>
              <Descriptions.Item label="过期时间">{parameterResult?.expiresAt || "-"}</Descriptions.Item>
            </Descriptions>
            {parameterResult?.parameters.some((parameter) => parameter.dynamicParameter) ? (
              <Alert
                type="warning"
                showIcon
                message="动态参数已降级为手动输入"
                description={`以下参数来自 Jenkins 动态参数插件，Tauri SSH 不执行 Jenkins 页面脚本联动：${parameterResult.parameters
                  .filter((parameter) => parameter.dynamicParameter)
                  .map((parameter) => parameter.name)
                  .join("、")}。请按 Jenkins 页面实际可选值填写，提交审批前会保留 dynamicParameter 标记。`}
              />
            ) : null}
            {parameterResult ? (
              <Space direction="vertical" size={8} className="w-full">
                <Text strong>参数模板</Text>
                <Space wrap>
                  <Select
                    allowClear
                    placeholder="选择模板"
                    value={selectedTemplateKey || undefined}
                    loading={templateLoading}
                    style={{ width: 260 }}
                    options={parameterTemplates.map((template) => ({
                      label: template.name,
                      value: template.templateKey,
                    }))}
                    onChange={(value) => {
                      const nextKey = value ?? "";
                      const template = parameterTemplates.find((item) => item.templateKey === nextKey);
                      setSelectedTemplateKey(nextKey);
                      if (template) {
                        setTemplateName(template.name);
                      }
                    }}
                  />
                  <Button disabled={!selectedTemplateKey} onClick={applyParameterTemplate}>
                    套用模板
                  </Button>
                  <Input
                    value={templateName}
                    onChange={(event) => setTemplateName(event.target.value)}
                    placeholder="模板名称"
                    style={{ width: 220 }}
                  />
                  <Button loading={templateLoading} onClick={saveParameterTemplate}>
                    保存模板
                  </Button>
                  <Button danger disabled={!selectedTemplateKey} loading={templateLoading} onClick={deleteParameterTemplate}>
                    删除模板
                  </Button>
                </Space>
                <Text type="secondary">
                  模板仅保存脱敏参数摘要；敏感参数留空时不传给 Jenkins，填写后必须保存为 secretRef。
                </Text>
              </Space>
            ) : null}
            {parameterResult ? (
              <Space direction="vertical" size={8} className="w-full">
                <Text strong>Git 工作区参数</Text>
                <Space wrap>
                  <Select
                    allowClear
                    showSearch
                    placeholder="选择 Git 工作区"
                    value={selectedGitWorkspaceKey || undefined}
                    loading={gitWorkspaceLoading}
                    style={{ width: 320 }}
                    options={gitWorkspaces.map((workspace) => ({
                      label: `${workspace.name} (${workspace.branch || "HEAD"})`,
                      value: workspace.workspaceKey,
                    }))}
                    optionFilterProp="label"
                    onChange={(value) => void selectGitWorkspaceForParameters(value ?? "")}
                  />
                  <Button disabled={!selectedGitWorkspaceKey} loading={gitWorkspaceLoading} onClick={refreshSelectedGitWorkspaceStatus}>
                    刷新 Git 状态
                  </Button>
                  <Button type="primary" disabled={!gitWorkspaceStatus} onClick={applyGitWorkspaceParameters}>
                    注入 branch/commit 参数
                  </Button>
                </Space>
                {gitWorkspaceStatus ? (
                  <Descriptions size="small" bordered column={3}>
                    <Descriptions.Item label="分支">{gitWorkspaceStatus.workspace.branch || "-"}</Descriptions.Item>
                    <Descriptions.Item label="HEAD">
                      <Text code>{gitWorkspaceStatus.headCommit || "-"}</Text>
                    </Descriptions.Item>
                    <Descriptions.Item label="状态">
                      <Tag color={gitWorkspaceStatus.workspace.changedFiles > 0 ? "orange" : "green"}>
                        {gitWorkspaceStatus.workspace.status}
                      </Tag>
                    </Descriptions.Item>
                    <Descriptions.Item label="变更文件">{gitWorkspaceStatus.workspace.changedFiles}</Descriptions.Item>
                    <Descriptions.Item label="Ahead">{gitWorkspaceStatus.workspace.ahead}</Descriptions.Item>
                    <Descriptions.Item label="Behind">{gitWorkspaceStatus.workspace.behind}</Descriptions.Item>
                  </Descriptions>
                ) : (
                  <Alert
                    type="info"
                    showIcon
                    message="选择 Git 工作区后，可将当前分支和 HEAD commit 注入到名称包含 branch/ref/commit/revision/sha 的构建参数。"
                  />
                )}
              </Space>
            ) : null}
            {parameterResult?.parameters.length ? (
              <Form
                form={parameterForm}
                layout="vertical"
                onValuesChange={(_, values) => setParameterValues(values)}
              >
                {parameterResult.parameters.map((parameter) => (
                  <Form.Item
                    key={parameter.name}
                    name={parameter.name}
                    label={
                      <Space size={6} wrap>
                        <span>{parameter.name}</span>
                        <Tag>{parameter.parameterType}</Tag>
                        {parameter.sensitive ? <Tag color="red">敏感</Tag> : null}
                        {parameter.fileParameter ? <Tag color="orange">文件</Tag> : null}
                        {parameter.dynamicParameter ? <Tag color="purple">动态</Tag> : null}
                        {parameter.unsupported ? <Tag color="red">不支持</Tag> : null}
                        {recentParameterValueMap.has(parameter.name) ? (
                          <>
                            <Tag color="blue">最近值</Tag>
                            <Button
                              size="small"
                              type="link"
                              className="px-0"
                              onClick={() => forgetRecentParameterValue(parameter.name)}
                            >
                              忘记
                            </Button>
                          </>
                        ) : null}
                      </Space>
                    }
                    tooltip={parameter.description || undefined}
                    valuePropName={parameter.parameterType === "boolean" ? "checked" : "value"}
                  >
                    {renderParameterInput(parameter, chooseFileParameter, inspectFileParameterPath)}
                  </Form.Item>
                ))}
              </Form>
            ) : parameterLoading ? (
              <Alert type="info" showIcon message="正在读取 Jenkins 参数定义" />
            ) : (
              <Alert type="success" showIcon message="该 Job 未声明构建参数" />
            )}
            {parameterResult ? (
              <Input.TextArea
                rows={3}
                value={approvalReason}
                onChange={(event) => setApprovalReason(event.target.value)}
                placeholder={
                  selectedConnection?.approvalPolicy === "none"
                    ? "可选，填写触发该 Jenkins 构建的操作理由"
                    : "填写触发该 Jenkins 构建的理由；创建审批后不会立即触发 Jenkins"
                }
              />
            ) : null}
            {parameterResult ? (
              <Input.TextArea
                readOnly
                rows={8}
                value={JSON.stringify(
                  buildSafeParameterSummary(parameterResult.parameters, parameterValues),
                  null,
                  2,
                )}
                className="font-mono text-xs"
              />
            ) : null}
          </Space>
        ) : (
          <Alert type="info" showIcon message="请选择一个 Job" />
        )}
      </Drawer>

      <Drawer
        title="构建详情"
        size="large"
        open={buildDrawerOpen}
        onClose={() => setBuildDrawerOpen(false)}
      >
        {selectedBuild ? (
          <Space direction="vertical" size={16} className="w-full">
            {isSuccessfulBuildLike(selectedBuildDetail ?? selectedBuild) && artifacts.length > 0 ? (
              <Alert
                type="success"
                showIcon
                message="构建成功，可进入部署准备"
                description={
                  getAvailableDeploymentArtifacts(artifacts).length > 0
                    ? `已存在 ${getAvailableDeploymentArtifacts(artifacts).length} 个可部署 artifact，可直接生成部署候选并继续 Dry-run。`
                    : "当前 artifact 仍未下载到应用托管目录，请先下载需要部署的 artifact。"
                }
                action={
                  getAvailableDeploymentArtifacts(artifacts)[0] ? (
                    <Button
                      size="small"
                      type="primary"
                      icon={<PackageCheck size={14} />}
                      loading={artifactCandidateCreating === getAvailableDeploymentArtifacts(artifacts)[0].artifactKey}
                      onClick={() => createArtifactDeploymentCandidate(getAvailableDeploymentArtifacts(artifacts)[0])}
                    >
                      生成部署候选
                    </Button>
                  ) : null
                }
              />
            ) : null}
            {isDeploymentBlockedBuildLike(selectedBuildDetail ?? selectedBuild) ? (
              <Alert
                type="error"
                showIcon
                message="构建未成功，部署已阻断"
                description="该构建不能生成部署候选，也不能进入部署 Dry-run。请先查看日志、提取失败片段或生成 AI 失败总结，修复后重新构建。"
              />
            ) : null}
            <Descriptions size="small" column={2} bordered>
              <Descriptions.Item label="Job" span={2}>
                {selectedBuild.jobFullName}
              </Descriptions.Item>
              <Descriptions.Item label="构建号">{selectedBuild.buildNumber ?? "-"}</Descriptions.Item>
              <Descriptions.Item label="状态">
                {statusTag(selectedBuildDetail?.status ?? selectedBuild.status)}
              </Descriptions.Item>
              <Descriptions.Item label="结果">{selectedBuildDetail?.result || selectedBuild.result || "-"}</Descriptions.Item>
              <Descriptions.Item label="来源">
                {selectedBuildDetail?.statusSource || selectedBuild.statusSource || "-"}
              </Descriptions.Item>
              <Descriptions.Item label="触发人">
                {selectedBuildDetail?.createdBy || selectedBuild.createdBy || "-"}
              </Descriptions.Item>
              <Descriptions.Item label="触发原因" span={2}>
                {selectedBuildDetail?.cause || selectedBuild.cause || "-"}
              </Descriptions.Item>
              <Descriptions.Item label="开始时间">
                {selectedBuildDetail?.startedAt || selectedBuild.startedAt || "-"}
              </Descriptions.Item>
              <Descriptions.Item label="结束时间">
                {selectedBuildDetail?.finishedAt || selectedBuild.finishedAt || "-"}
              </Descriptions.Item>
            </Descriptions>
            <Input.TextArea
              rows={2}
              value={stopReason}
              onChange={(event) => setStopReason(event.target.value)}
              placeholder={
                selectedConnection?.approvalPolicy === "none"
                  ? "可选，填写停止该 Jenkins 构建的操作理由"
                  : "填写停止该 Jenkins 构建的理由；创建审批后不会立即停止 Jenkins"
              }
            />
            <Space wrap size={[8, 8]} className="w-full">
              <Button
                icon={<RefreshCw size={14} />}
                loading={buildDetailLoading}
                onClick={() => openBuildDetail(selectedBuild)}
              >
                刷新详情
              </Button>
              <Button
                icon={<RefreshCw size={14} />}
                loading={artifactLoading}
                onClick={() => loadArtifacts(selectedBuild)}
              >
                刷新 Artifacts
              </Button>
              <Button disabled={!buildLog?.text} onClick={copyLoadedBuildLog}>
                复制已加载日志
              </Button>
              <Button disabled={!buildLog?.text} onClick={extractLoadedFailureLogSummary}>
                提取失败片段
              </Button>
              <Button loading={analysisLoading} disabled={!failureLogSummary} onClick={generateFailureAnalysis}>
                AI 失败总结
              </Button>
              <Button
                danger
                icon={<Square size={14} />}
                loading={stopApprovalCreating}
                disabled={!selectedBuild.buildNumber || !isStoppableBuildLike(selectedBuildDetail ?? selectedBuild)}
                onClick={createBuildStopApproval}
              >
                {selectedConnection?.approvalPolicy === "none" ? "停止构建" : "创建停止审批"}
              </Button>
            </Space>
            {buildLog ? (
              <>
                <Alert
                  type="info"
                  showIcon
                  message={buildLog.message}
                  description={`偏移 ${buildLog.start} -> ${buildLog.nextStart}${
                    buildLog.hasMore ? "，仍有更多日志" : "，已到当前末尾"
                  }`}
                />
                <Space wrap>
                  <Input.Search
                    allowClear
                    placeholder="在已加载日志中搜索"
                    value={logSearchTerm}
                    onChange={(event) => setLogSearchTerm(event.target.value)}
                    style={{ width: 260 }}
                  />
                  <Tag color={logSearchTerm.trim() ? "blue" : "default"}>搜索命中 {logSearchCount}</Tag>
                  <Tag color={logErrorHighlightCount > 0 ? "red" : "default"}>
                    错误高亮 {logErrorHighlightCount}
                  </Tag>
                </Space>
                <pre
                  className="font-mono text-xs"
                  style={{
                    minHeight: 360,
                    maxHeight: 520,
                    overflow: "auto",
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-word",
                    margin: 0,
                    padding: 12,
                    border: "1px solid var(--ant-color-border)",
                    borderRadius: 6,
                    background: "var(--ant-color-fill-quaternary)",
                  }}
                >
                  {highlightedLogNodes}
                </pre>
                {failureLogSummary ? (
                  <Alert
                    type="warning"
                    showIcon
                    message={`失败片段：命中 ${failureLogSummary.matchedLines} 行，范围 ${failureLogSummary.startLine}-${failureLogSummary.endLine}`}
                    description={
                      <pre
                        className="font-mono text-xs"
                        style={{
                          maxHeight: 260,
                          overflow: "auto",
                          whiteSpace: "pre-wrap",
                          wordBreak: "break-word",
                          margin: 0,
                        }}
                      >
                        {failureLogSummary.text}
                      </pre>
                    }
                  />
                ) : null}
                {buildAnalysis ? (
                  <Alert
                    type="success"
                    showIcon
                    message={`AI 失败总结已保存：${buildAnalysis.providerName} / ${buildAnalysis.model}`}
                    description={
                      <Space direction="vertical" size={8} className="w-full">
                        <Text type="secondary">
                          范围 {buildAnalysis.snippetStartLine}-{buildAnalysis.snippetEndLine}，命中{" "}
                          {buildAnalysis.matchedLines} 行，片段哈希 {buildAnalysis.snippetSha256.slice(0, 12)}
                        </Text>
                        <JenkinsMarkdownSummary content={buildAnalysis.summaryMarkdown} />
                      </Space>
                    }
                  />
                ) : null}
              </>
            ) : (
              <Alert type="info" showIcon message="正在自动读取脱敏后的 Jenkins 控制台输出片段，每 5 秒刷新一次。" />
            )}
            <Table
              rowKey="relativePath"
              size="small"
              loading={artifactLoading}
              columns={artifactColumns}
              dataSource={artifacts}
              pagination={false}
              scroll={{ x: 980 }}
            />
            {deploymentCandidate ? (
              <Card size="small" title="部署候选与 Dry-run">
                <Space direction="vertical" className="w-full" size="middle">
                  <Descriptions size="small" column={1}>
                    <Descriptions.Item label="名称">{deploymentCandidate.name}</Descriptions.Item>
                    <Descriptions.Item label="配方">{deploymentCandidate.recipe}</Descriptions.Item>
                    <Descriptions.Item label="来源">{deploymentCandidate.sourceType}</Descriptions.Item>
                    <Descriptions.Item label="制品">{deploymentCandidate.artifactDir}</Descriptions.Item>
                  </Descriptions>
                  <Space size={12} wrap>
                    <Input
                      style={{ width: 220 }}
                      placeholder="目标服务器别名"
                      value={deploymentDryRunServerAlias}
                      onChange={(event) => setDeploymentDryRunServerAlias(event.target.value)}
                    />
                    <Input
                      style={{ width: 320 }}
                      placeholder="部署根目录"
                      value={deploymentDryRunDeployRoot}
                      onChange={(event) => setDeploymentDryRunDeployRoot(event.target.value)}
                    />
                    <InputNumber
                      style={{ width: 120 }}
                      min={1}
                      max={65535}
                      placeholder="端口"
                      value={deploymentDryRunPort}
                      onChange={(value) => setDeploymentDryRunPort(typeof value === "number" ? value : null)}
                    />
                    <Button type="primary" loading={deploymentDryRunLoading} onClick={() => void createBuildDeploymentDryRun()}>
                      生成 Dry-run
                    </Button>
                  </Space>
                  {deploymentDryRunPlan ? (
                    <Space direction="vertical" className="w-full" size="middle">
                      <Alert
                        type={deploymentDryRunPlan.approvalRequired ? "warning" : "info"}
                        showIcon
                        message={deploymentDryRunPlan.title}
                        description="Dry-run 只生成计划和风险预览，不会执行部署命令。"
                      />
                      <Descriptions size="small" bordered column={3}>
                        <Descriptions.Item label="Plan ID">{deploymentDryRunPlan.planId}</Descriptions.Item>
                        <Descriptions.Item label="目标">{deploymentDryRunPlan.targetKey}</Descriptions.Item>
                        <Descriptions.Item label="服务器">{deploymentDryRunPlan.serverAlias}</Descriptions.Item>
                        <Descriptions.Item label="配方">{deploymentDryRunPlan.recipe}</Descriptions.Item>
                        <Descriptions.Item label="风险">{deploymentRiskTag(deploymentDryRunPlan.risk)}</Descriptions.Item>
                        <Descriptions.Item label="审批">
                          <Tag color={deploymentDryRunPlan.approvalRequired ? "red" : "green"}>
                            {deploymentDryRunPlan.approvalRequired ? "需要" : "不需要"}
                          </Tag>
                        </Descriptions.Item>
                      </Descriptions>
                      {deploymentDryRunPlan.warnings.length ? (
                        <Alert
                          type="warning"
                          showIcon
                          message="风险提示"
                          description={
                            <ul className="m-0 pl-5">
                              {deploymentDryRunPlan.warnings.map((warning) => (
                                <li key={warning}>{warning}</li>
                              ))}
                            </ul>
                          }
                        />
                      ) : null}
                      <Table
                        rowKey="key"
                        size="small"
                        columns={deploymentDryRunStageColumns}
                        dataSource={deploymentDryRunPlan.stages}
                        pagination={false}
                        scroll={{ x: 980 }}
                      />
                    </Space>
                  ) : null}
                </Space>
              </Card>
            ) : null}
          </Space>
        ) : (
          <Alert type="info" showIcon message="请选择一条构建记录" />
        )}
      </Drawer>
    </div>
  );
}

function formatBytes(value?: number | null) {
  if (value == null || Number.isNaN(value)) {
    return "-";
  }
  if (value < 1024) {
    return `${value} B`;
  }
  const units = ["KB", "MB", "GB"];
  let size = value / 1024;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  return `${size.toFixed(size >= 10 ? 1 : 2)} ${units[index]}`;
}

function getDeploymentCandidateArtifactKey(candidate: DeploymentCandidate) {
  try {
    const value = JSON.parse(candidate.configJson || "{}") as { artifactKey?: string };
    return value.artifactKey || "";
  } catch {
    return "";
  }
}

function isSuccessfulBuildLike(build: Pick<JenkinsBuild, "status" | "result"> | Pick<JenkinsBuildStatusEvent, "status" | "result">) {
  return build.status?.toLowerCase() === "success" || build.result?.toUpperCase() === "SUCCESS";
}

function isStoppableBuildLike(
  build:
    | Pick<JenkinsBuild, "status" | "result">
    | Pick<JenkinsBuildStatusEvent, "status" | "result">
    | null
    | undefined,
) {
  if (!build) {
    return false;
  }
  if (build.result?.trim()) {
    return false;
  }
  return unfinishedBuildStatuses.has(build.status?.toLowerCase() ?? "");
}

function isDeploymentBlockedBuildLike(
  build:
    | Pick<JenkinsBuild, "status" | "result">
    | Pick<JenkinsBuildStatusEvent, "status" | "result">
    | null
    | undefined,
) {
  if (!build) {
    return false;
  }
  if (isSuccessfulBuildLike(build)) {
    return false;
  }
  const result = build.result?.trim().toUpperCase();
  if (result) {
    return true;
  }
  return ["failure", "failed", "unstable", "aborted", "not_built", "sync_failed"].includes(
    build.status?.toLowerCase() ?? "",
  );
}

function isAvailableArtifact(record: JenkinsArtifact) {
  return record.status === "available" && Boolean(record.localPath);
}

function getAvailableDeploymentArtifacts(records: JenkinsArtifact[]) {
  return records.filter(isAvailableArtifact);
}

function buildGitWorkspaceParameterValues(
  parameters: JenkinsParameterDefinition[],
  status: GitWorkspaceStatusResult,
): ParameterFormValues {
  const branch = status.workspace.branch || "";
  const commit = status.headCommit || "";
  return parameters.reduce<ParameterFormValues>((values, parameter) => {
    if (parameter.sensitive || parameter.fileParameter || parameter.unsupported) {
      return values;
    }
    const normalized = parameter.name.toLowerCase().replace(/[^a-z0-9]+/g, "_");
    if (commit && isCommitParameterName(normalized)) {
      values[parameter.name] = commit;
      return values;
    }
    if (branch && isBranchParameterName(normalized)) {
      values[parameter.name] = branch;
    }
    return values;
  }, {});
}

function isBranchParameterName(name: string) {
  return (
    name === "branch" ||
    name === "git_branch" ||
    name === "source_branch" ||
    name === "target_branch" ||
    name === "ref" ||
    name === "git_ref" ||
    name.endsWith("_branch") ||
    name.endsWith("_ref")
  );
}

function isCommitParameterName(name: string) {
  return (
    name === "commit" ||
    name === "git_commit" ||
    name === "commit_hash" ||
    name === "git_commit_hash" ||
    name === "revision" ||
    name === "git_revision" ||
    name === "sha" ||
    name === "git_sha" ||
    name === "head" ||
    name === "git_head" ||
    name.endsWith("_commit") ||
    name.endsWith("_commit_hash") ||
    name.endsWith("_revision") ||
    name.endsWith("_sha")
  );
}

function buildInitialParameterValues(
  parameters: JenkinsParameterDefinition[],
  recentValues: JenkinsRecentParameterValue[] = [],
) {
  const recentValueMap = new Map(recentValues.map((value) => [value.parameterName, value]));
  return parameters.reduce<ParameterFormValues>((values, parameter) => {
    if (parameter.fileParameter || parameter.unsupported) {
      values[parameter.name] = undefined;
      return values;
    }
    const recentValue = recentValueMap.get(parameter.name);
    const recentFormValue = recentValueToFormValue(parameter, recentValue);
    if (recentFormValue !== undefined) {
      values[parameter.name] = recentFormValue;
      return values;
    }
    if (parameter.sensitive) {
      values[parameter.name] = { valueKind: "secret_ref", secretRef: "" };
      return values;
    }
    if (parameter.parameterType === "boolean") {
      values[parameter.name] = typeof parameter.defaultValue === "boolean" ? parameter.defaultValue : false;
      return values;
    }
    if (parameter.parameterType === "choice") {
      const defaultValue = typeof parameter.defaultValue === "string" ? parameter.defaultValue : "";
      values[parameter.name] = parameter.choices.includes(defaultValue) ? defaultValue : parameter.choices[0];
      return values;
    }
    values[parameter.name] = scalarToString(parameter.defaultValue);
    return values;
  }, {});
}

function recentValueToFormValue(
  parameter: JenkinsParameterDefinition,
  recentValue?: JenkinsRecentParameterValue,
): ParameterFormValue {
  if (!recentValue || parameter.fileParameter || parameter.unsupported) {
    return undefined;
  }
  if (parameter.sensitive) {
    const secretRef = recentSecretRef(recentValue.valueJson);
    return secretRef ? { valueKind: "secret_ref", secretRef } : undefined;
  }
  if (recentValue.valueKind !== "plain") {
    return undefined;
  }
  if (parameter.parameterType === "boolean") {
    return typeof recentValue.valueJson === "boolean" ? recentValue.valueJson : undefined;
  }
  if (typeof recentValue.valueJson === "string" || typeof recentValue.valueJson === "number") {
    return String(recentValue.valueJson);
  }
  return undefined;
}

function recentSecretRef(value: unknown) {
  if (!value || typeof value !== "object" || !("secretRef" in value)) {
    return "";
  }
  const secretRef = (value as { secretRef?: unknown }).secretRef;
  return typeof secretRef === "string" ? secretRef : "";
}

function templateSummaryToFormValues(parameters: JenkinsParameterDefinition[], summary: unknown) {
  if (!summary || typeof summary !== "object") {
    return {};
  }
  const entries = Array.isArray((summary as { parameters?: unknown }).parameters)
    ? ((summary as { parameters: unknown[] }).parameters)
    : [];
  const entryMap = new Map(
    entries
      .filter((entry): entry is { name: string; value?: unknown } => {
        return Boolean(entry && typeof entry === "object" && typeof (entry as { name?: unknown }).name === "string");
      })
      .map((entry) => [entry.name, entry.value]),
  );
  return parameters.reduce<ParameterFormValues>((values, parameter) => {
    if (!entryMap.has(parameter.name) || parameter.fileParameter || parameter.unsupported) {
      return values;
    }
    const value = entryMap.get(parameter.name);
    if (parameter.sensitive) {
      const secretRef = recentSecretRef(value);
      if (secretRef) {
        values[parameter.name] = { valueKind: "secret_ref", secretRef };
      }
      return values;
    }
    if (parameter.parameterType === "boolean") {
      values[parameter.name] = typeof value === "boolean" ? value : value === "true";
      return values;
    }
    if (typeof value === "string" || typeof value === "number") {
      values[parameter.name] = String(value);
    }
    return values;
  }, {});
}

function renderParameterInput(
  parameter: JenkinsParameterDefinition,
  onChooseFile: (parameterName: string) => void,
  onInspectFilePath: (parameterName: string, localPath: string) => void,
) {
  if (parameter.fileParameter) {
    return (
      <FileParameterInput
        parameter={parameter}
        onChooseFile={onChooseFile}
        onInspectFilePath={onInspectFilePath}
      />
    );
  }
  if (parameter.parameterType === "password" || parameter.sensitive) {
    return <SensitiveParameterInput parameter={parameter} />;
  }
  if (parameter.dynamicParameter) {
    return <Input placeholder="动态参数已降级为手动输入，请按 Jenkins 页面实际值填写" />;
  }
  if (parameter.unsupported) {
    return <Input disabled placeholder="当前参数类型暂不支持" />;
  }
  if (parameter.parameterType === "boolean") {
    return <Switch />;
  }
  if (parameter.parameterType === "choice") {
    return <Select options={parameter.choices.map((choice) => ({ label: choice, value: choice }))} />;
  }
  return <Input placeholder={parameter.description || "请输入参数值"} />;
}

function buildSafeParameterSummary(parameters: JenkinsParameterDefinition[], values: ParameterFormValues) {
  return {
    parameters: parameters
      .map((parameter) => {
        const value = safeParameterValue(parameter, values[parameter.name]);
        if (value === undefined) {
          return null;
        }
        return {
          name: parameter.name,
          type: parameter.parameterType,
          sensitive: parameter.sensitive,
          fileParameter: parameter.fileParameter,
          dynamicParameter: parameter.dynamicParameter,
          unsupported: parameter.unsupported,
          value,
        };
      })
      .filter((parameter): parameter is NonNullable<typeof parameter> => parameter !== null),
  };
}

interface FileParameterInputProps {
  parameter: JenkinsParameterDefinition;
  value?: ParameterFormValue;
  onChange?: (value: ParameterFormValue) => void;
  onChooseFile: (parameterName: string) => void;
  onInspectFilePath: (parameterName: string, localPath: string) => void;
}

function FileParameterInput({
  parameter,
  value,
  onChooseFile,
  onInspectFilePath,
}: FileParameterInputProps) {
  const metadata = isFileParameterMetadata(value) ? value : null;
  const [manualPath, setManualPath] = useState(metadata?.localPath ?? "");

  useEffect(() => {
    if (metadata?.localPath) {
      setManualPath(metadata.localPath);
    }
  }, [metadata?.localPath]);

  return (
    <Space direction="vertical" size={8} className="w-full">
      <Input.Search
        value={manualPath}
        placeholder="选择文件，或输入本地绝对路径后读取元数据"
        enterButton="读取元数据"
        onChange={(event) => setManualPath(event.target.value)}
        onSearch={(path) => onInspectFilePath(parameter.name, path)}
      />
      <Space wrap>
        <Button onClick={() => onChooseFile(parameter.name)}>选择文件</Button>
        {metadata ? (
          <>
            <Tag color="blue">{metadata.fileName}</Tag>
            <Tag>{formatBytes(metadata.sizeBytes)}</Tag>
            <Text code copyable className="text-xs">
              {metadata.sha256}
            </Text>
          </>
        ) : (
          <Text type="secondary">尚未读取文件元数据</Text>
        )}
      </Space>
    </Space>
  );
}

function safeParameterValue(parameter: JenkinsParameterDefinition, value: ParameterFormValue) {
  if (parameter.sensitive) {
    if (isSensitiveParameterReference(value) && value.secretRef.trim()) {
      return {
        valueKind: "secret_ref",
        secretRef: value.secretRef.trim(),
      };
    }
    return undefined;
  }
  if (parameter.fileParameter) {
    if (isFileParameterMetadata(value)) {
      return {
        fileName: value.fileName,
        sizeBytes: value.sizeBytes,
        sha256: value.sha256,
        modifiedAt: value.modifiedAt ?? null,
      };
    }
    return "file_parameter_pending";
  }
  if (parameter.dynamicParameter) {
    return value ?? "";
  }
  if (parameter.unsupported) {
    return "unsupported_parameter";
  }
  return value ?? null;
}

interface SensitiveParameterInputProps {
  parameter: JenkinsParameterDefinition;
  value?: ParameterFormValue;
  onChange?: (value: JenkinsSensitiveParameterReference) => void;
}

function SensitiveParameterInput({ parameter, value, onChange }: SensitiveParameterInputProps) {
  const secretRef = isSensitiveParameterReference(value) ? value.secretRef : "";

  return (
    <Input
      value={secretRef}
      addonBefore="secretRef"
      placeholder={`留空则不传 ${parameter.name}，使用 Jenkins 现有配置`}
      onChange={(event) =>
        onChange?.({
          valueKind: "secret_ref",
          secretRef: event.target.value,
        })
      }
    />
  );
}

function JenkinsMarkdownSummary({ content }: { content: string }) {
  return <div className="jenkins-ai-markdown">{renderJenkinsMarkdownBlocks(content)}</div>;
}

function normalizeJenkinsMarkdown(markdown: string) {
  return markdown
    .replace(/\r\n/g, "\n")
    .replace(/\s+(#{1,6}\s+)/g, "\n\n$1")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function renderJenkinsMarkdownBlocks(markdown: string) {
  const lines = normalizeJenkinsMarkdown(markdown).split("\n");
  const blocks: ReactNode[] = [];
  let index = 0;

  while (index < lines.length) {
    const trimmed = lines[index].trim();
    if (!trimmed) {
      index += 1;
      continue;
    }

    const heading = trimmed.match(/^(#{1,6})\s+(.+)$/);
    if (heading) {
      const HeadingTag = (heading[1].length <= 3 ? "h3" : "h4") as "h3" | "h4";
      blocks.push(<HeadingTag key={`heading-${index}`}>{renderJenkinsInlineMarkdown(heading[2])}</HeadingTag>);
      index += 1;
      continue;
    }

    if (/^[-*]\s+/.test(trimmed)) {
      const items: string[] = [];
      while (index < lines.length && /^[-*]\s+/.test(lines[index].trim())) {
        items.push(lines[index].trim().replace(/^[-*]\s+/, ""));
        index += 1;
      }
      blocks.push(
        <ul key={`ul-${index}`}>
          {items.map((item, itemIndex) => (
            <li key={`${item}-${itemIndex}`}>{renderJenkinsInlineMarkdown(item)}</li>
          ))}
        </ul>,
      );
      continue;
    }

    if (/^\d+[.)]\s+/.test(trimmed)) {
      const items: string[] = [];
      while (index < lines.length && /^\d+[.)]\s+/.test(lines[index].trim())) {
        items.push(lines[index].trim().replace(/^\d+[.)]\s+/, ""));
        index += 1;
      }
      blocks.push(
        <ol key={`ol-${index}`}>
          {items.map((item, itemIndex) => (
            <li key={`${item}-${itemIndex}`}>{renderJenkinsInlineMarkdown(item)}</li>
          ))}
        </ol>,
      );
      continue;
    }

    const paragraphLines = [trimmed];
    index += 1;
    while (index < lines.length) {
      const next = lines[index].trim();
      if (!next || /^(#{1,6})\s+/.test(next) || /^[-*]\s+/.test(next) || /^\d+[.)]\s+/.test(next)) {
        break;
      }
      paragraphLines.push(next);
      index += 1;
    }
    blocks.push(<p key={`p-${index}`}>{renderJenkinsInlineMarkdown(paragraphLines.join(" "))}</p>);
  }

  return blocks;
}

function renderJenkinsInlineMarkdown(value: string) {
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

function scalarToString(value: unknown) {
  if (value == null) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return "";
}

function normalizeJobTableData(items: JenkinsJob[]): JenkinsJob[] {
  return items.map((item) => {
    const rest = { ...item };
    delete rest.children;
    return rest;
  });
}

function sortJenkinsBuilds(items: JenkinsBuild[]): JenkinsBuild[] {
  return [...items].sort((left, right) => {
    const leftTime = parseJenkinsBuildTime(left.startedAt || left.updatedAt || left.createdAt);
    const rightTime = parseJenkinsBuildTime(right.startedAt || right.updatedAt || right.createdAt);
    if (Number.isFinite(leftTime) && Number.isFinite(rightTime) && leftTime !== rightTime) {
      return rightTime - leftTime;
    }
    const leftBuildNumber = left.buildNumber ?? Number.NEGATIVE_INFINITY;
    const rightBuildNumber = right.buildNumber ?? Number.NEGATIVE_INFINITY;
    if (leftBuildNumber !== rightBuildNumber) {
      return rightBuildNumber - leftBuildNumber;
    }
    return right.runKey.localeCompare(left.runKey);
  });
}

function parseJenkinsBuildTime(value?: string | null): number {
  if (!value) {
    return Number.NaN;
  }
  const normalized = value.includes("T") ? value : value.replace(" ", "T");
  return Date.parse(normalized);
}

function formatJenkinsBuildTime(value?: string | null): string {
  const millis = parseJenkinsBuildTime(value);
  if (!Number.isFinite(millis)) {
    return value || "-";
  }
  const date = new Date(millis);
  const pad = (item: number) => String(item).padStart(2, "0");
  return [
    date.getFullYear(),
    pad(date.getMonth() + 1),
    pad(date.getDate()),
  ].join("-") + ` ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function findJobInTree(items: JenkinsJob[], jobFullName?: string): JenkinsJob | null {
  if (!jobFullName) {
    return null;
  }
  for (const item of items) {
    if (item.jobFullName === jobFullName) {
      return item;
    }
    const child = findJobInTree(item.children ?? [], jobFullName);
    if (child) {
      return child;
    }
  }
  return null;
}

function updateJobInTree(
  items: JenkinsJob[],
  jobFullName: string,
  updater: (item: JenkinsJob) => JenkinsJob,
): JenkinsJob[] {
  return items.map((item) => {
    if (item.jobFullName === jobFullName) {
      return updater(item);
    }
    if (!item.children?.length) {
      return item;
    }
    return {
      ...item,
      children: updateJobInTree(item.children, jobFullName, updater),
    };
  });
}

function isFileParameterMetadata(value: ParameterFormValue): value is JenkinsFileParameterMetadata {
  return (
    typeof value === "object" &&
    value !== null &&
    !("valueKind" in value) &&
    "fileName" in value &&
    "sha256" in value &&
    "sizeBytes" in value
  );
}

function isSensitiveParameterReference(value: ParameterFormValue): value is JenkinsSensitiveParameterReference {
  return (
    typeof value === "object" &&
    value !== null &&
    "valueKind" in value &&
    value.valueKind === "secret_ref" &&
    "secretRef" in value &&
    typeof value.secretRef === "string"
  );
}
