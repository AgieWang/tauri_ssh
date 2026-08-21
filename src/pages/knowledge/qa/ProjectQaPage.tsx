import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Collapse,
  Empty,
  Input,
  message,
  Popconfirm,
  Select,
  Skeleton,
  Space,
  Tag,
  Typography,
} from "antd";
import {
  ArrowLeft,
  BookOpenCheck,
  Bot,
  Eye,
  FileDown,
  Plus,
  RefreshCw,
  Send,
  Trash2,
  User,
} from "lucide-react";
import { MarkdownPreview } from "@/components/ui/MarkdownPreview";
import { aiProviderApi, getErrorCode, getErrorMessage } from "@/lib/api";
import {
  knowledgeCatalogApi,
  knowledgeQaApi,
} from "@/lib/api/knowledge-domain";
import type {
  AiProvider,
  KnowledgeAskResult,
  KnowledgeConversationMessage,
  KnowledgeProject,
  KnowledgeRelease,
} from "@/types";
import type {
  KnowledgeQaSession,
  KnowledgeQaSessionDetail,
} from "@/types/knowledge-domain/qa";

const { Paragraph, Text, Title } = Typography;

type QaTurn = {
  id: number;
  question: string;
  answer: KnowledgeAskResult;
  evidenceOnly: boolean;
};

type RequirementCoverageDiagnostics = {
  requirementCandidateCount: number;
  implementationCandidateCount: number;
  testCandidateCount: number;
  verifiedRelationCount: number;
  explicitRelationCount: number;
};

type GitAgentDiagnostics = {
  status: "completed" | "partial" | "failed";
  repositoryCount: number;
  succeededCount: number;
  failedCount: number;
  totalCommitCount?: number;
};

function isChatProvider(provider: AiProvider) {
  const capabilities = provider.capabilities.map((value) =>
    value.trim().toLowerCase(),
  );
  const hasExplicitMode = capabilities.some((value) =>
    ["chat", "embedding"].includes(value),
  );
  return (
    provider.enabled &&
    provider.status === "configured" &&
    (hasExplicitMode
      ? capabilities.includes("chat")
      : Boolean(provider.defaultModel.trim()))
  );
}

function citationLabel(citation: KnowledgeAskResult["citations"][number]) {
  const lineRange =
    citation.startLine == null
      ? ""
      : ` · 第 ${citation.startLine}${citation.endLine != null ? `-${citation.endLine}` : ""} 行`;
  return `${citation.title || "未命名文档"}${citation.headingPath ? ` · ${citation.headingPath}` : ""}${lineRange}`;
}

function requirementCoverageDiagnostics(
  answer: KnowledgeAskResult,
): RequirementCoverageDiagnostics | null {
  if (answer.retrievalDiagnostics.queryMode !== "releaseRequirementCoverage") {
    return null;
  }
  const coverage = answer.retrievalDiagnostics.coverage;
  if (!coverage || typeof coverage !== "object" || Array.isArray(coverage)) {
    return {
      requirementCandidateCount: 0,
      implementationCandidateCount: 0,
      testCandidateCount: 0,
      verifiedRelationCount: 0,
      explicitRelationCount: 0,
    };
  }
  const values = coverage as Record<string, unknown>;
  const numberValue = (key: string) => {
    const value = values[key];
    return typeof value === "number" ? value : 0;
  };
  return {
    requirementCandidateCount: numberValue("requirementCandidateCount"),
    implementationCandidateCount: numberValue("implementationCandidateCount"),
    testCandidateCount: numberValue("testCandidateCount"),
    verifiedRelationCount: numberValue("verifiedRelationCount"),
    explicitRelationCount: numberValue("explicitRelationCount"),
  };
}

function gitAgentDiagnostics(
  answer: KnowledgeAskResult,
): GitAgentDiagnostics | null {
  if (answer.retrievalDiagnostics.queryMode !== "gitAgent") return null;
  const agent = answer.retrievalDiagnostics.agent;
  if (!agent || typeof agent !== "object" || Array.isArray(agent)) return null;
  const values = agent as Record<string, unknown>;
  const numberValue = (key: string) =>
    typeof values[key] === "number" ? (values[key] as number) : 0;
  const status = ["completed", "partial", "failed"].includes(
    String(values.status),
  )
    ? (String(values.status) as GitAgentDiagnostics["status"])
    : "failed";
  return {
    status,
    repositoryCount: numberValue("repositoryCount"),
    succeededCount: numberValue("succeededCount"),
    failedCount: numberValue("failedCount"),
    totalCommitCount:
      typeof values.totalCommitCount === "number"
        ? values.totalCommitCount
        : undefined,
  };
}

function isTestCitationPath(logicalPath: string) {
  const normalized = logicalPath.replace(/\\/g, "/").toLowerCase();
  const fileName = normalized.split("/").pop() ?? normalized;
  return (
    normalized.includes("/src/test/") ||
    normalized.includes("/tests/") ||
    normalized.startsWith("tests/") ||
    /(?:test|tests)\.[^.]+$/.test(fileName) ||
    fileName.includes(".test.") ||
    fileName.includes(".spec.") ||
    normalized.endsWith("code-reports/test-map.md")
  );
}

function coverageCitationRole(
  citation: KnowledgeAskResult["citations"][number],
) {
  if (citation.logicalPath.startsWith("code-reports/")) return "版本信息";
  if (isTestCitationPath(citation.logicalPath)) return "测试源码候选";
  if (
    citation.sourceType === "code_snapshot" ||
    /\.(java|rs|ts|tsx|js|jsx|vue|xml|sql|py|go|cs)$/i.test(
      citation.logicalPath,
    )
  ) {
    return "代码候选";
  }
  return "需求基线";
}

function citationRole(
  citation: KnowledgeAskResult["citations"][number],
  coverage: RequirementCoverageDiagnostics | null,
) {
  if (citation.sourceType === "git_statistics") return "Git 实时证据";
  return coverage ? coverageCitationRole(citation) : null;
}

function conversationMessages(turns: QaTurn[]): KnowledgeConversationMessage[] {
  return turns.flatMap((turn) => [
    { role: "user" as const, content: turn.question },
    { role: "assistant" as const, content: turn.answer.answer },
  ]);
}

function turnsFromSession(detail: KnowledgeQaSessionDetail): QaTurn[] {
  const turns: QaTurn[] = [];
  let question = "";
  for (const item of detail.messages) {
    if (item.role === "user") {
      question = item.content;
    } else if (question && item.answer) {
      turns.push({
        id: item.id,
        question,
        answer: item.answer,
        evidenceOnly: item.evidenceOnly,
      });
      question = "";
    }
  }
  return turns;
}

function isSessionCompatible(
  session: KnowledgeQaSession,
  releases: KnowledgeRelease[],
  providers: AiProvider[],
) {
  const release = releases.find((item) => item.id === session.projectVersionId);
  if (!release || (release.commitSha ?? "") !== session.releaseCommitSha) {
    return false;
  }
  if (!session.providerKey) return !session.model;
  const provider = providers.find((item) => item.key === session.providerKey);
  return Boolean(
    provider &&
    isChatProvider(provider) &&
    provider.defaultModel === session.model,
  );
}

function releaseScopeIdentity(release: KnowledgeRelease | undefined) {
  return release
    ? `${release.id}:${release.version}:${release.commitSha ?? ""}`
    : "";
}

function safeFenceContent(value: string) {
  return value.replace(/```/g, "``\u200b`");
}

function citationMarkdown(citation: KnowledgeAskResult["citations"][number]) {
  const location = citationLabel(citation);
  const path = citation.logicalPath || "未记录逻辑路径";
  return [
    `##### \`${citation.citationKey}\` · ${location}`,
    `- 文件路径：\`${path}\``,
    citation.commitSha ? `- 提交：\`${citation.commitSha}\`` : "",
    `\n\`\`\`text\n${safeFenceContent(citation.excerpt || "（无摘录）")}\n\`\`\``,
  ]
    .filter(Boolean)
    .join("\n");
}

function formatAnswerForExport(
  answer: string,
  citations: KnowledgeAskResult["citations"],
) {
  const byKey = new Map(
    citations.map((citation) => [citation.citationKey, citation]),
  );
  const byChunkId = new Map(
    citations
      .filter((citation) => citation.chunkId != null)
      .map((citation) => [citation.chunkId as number, citation]),
  );
  return answer.replace(
    /\[((?:code|citation|tool):[^\]\r\n]+|(?:[A-Za-z0-9_-]+:)+chunk:\d+)\]/g,
    (token, rawKey: string) => {
      const normalized = rawKey.startsWith("citation:")
        ? rawKey.slice("citation:".length)
        : rawKey;
      const chunkMatch = normalized.match(/(?:^|:)chunk:(\d+)$/);
      const chunkId = chunkMatch
        ? Number(chunkMatch[1])
        : /^\d+$/.test(normalized)
          ? Number(normalized)
          : undefined;
      const citation =
        byKey.get(normalized) ??
        (chunkId == null ? undefined : byChunkId.get(chunkId));
      return citation ? `【证据：${citationLabel(citation)}】` : token;
    },
  );
}

function buildConversationMarkdown(
  project: KnowledgeProject,
  release: KnowledgeRelease,
  provider: AiProvider | undefined,
  turns: QaTurn[],
  session?: KnowledgeQaSession,
) {
  const providerLabel = turns.some((turn) => !turn.evidenceOnly)
    ? session?.model
      ? `${session.providerKey || "未记录"}（${session.model}）`
      : provider
        ? `${provider.name}（${provider.defaultModel}）`
        : "未记录"
    : "未调用（本地证据模式）";
  const exportedAt = new Date().toLocaleString("zh-CN", {
    hour12: false,
  });
  const sections = turns.map((turn, index) => {
    const answer = turn.answer;
    const citations = answer.citations.length
      ? answer.citations.map(citationMarkdown).join("\n\n")
      : "（本轮没有可引用证据）";
    const gaps = answer.evidenceGaps.length
      ? answer.evidenceGaps.map((gap) => `- ${gap}`).join("\n")
      : "- 无";
    const conflicts = answer.conflicts.length
      ? answer.conflicts.map((conflict) => `- ${conflict}`).join("\n")
      : "- 无";
    return [
      `### 第 ${index + 1} 轮 · ${turn.evidenceOnly ? "本地证据" : "AI 回答"}`,
      "",
      "#### 用户问题",
      "",
      turn.question,
      "",
      "#### Markdown 回答",
      "",
      formatAnswerForExport(answer.answer.trim(), answer.citations) ||
        "（空回答）",
      "",
      `#### 引用证据（${answer.citations.length} 条）`,
      "",
      citations,
      "",
      "#### 证据缺口",
      "",
      gaps,
      "",
      "#### 来源冲突",
      "",
      conflicts,
      "",
      `#### 引用校验状态：${answer.citationValidation}`,
    ].join("\n");
  });
  return [
    "# 项目知识问答记录",
    "",
    "> 本文由用户手动从项目问答会话导出，回答和引用证据均保留原始 Markdown 结构，便于人工复核与后续评测。",
    "",
    "## 会话范围",
    "",
    `- 项目：${project.name}（${project.projectKey}）`,
    `- 项目版本：${release.version}`,
    session?.releaseCommitSha || release.commitSha
      ? `- 版本提交：\`${session?.releaseCommitSha || release.commitSha}\``
      : "",
    `- AI Provider：${providerLabel}`,
    `- 导出时间：${exportedAt}`,
    `- 对话轮数：${turns.length}`,
    "",
    "## 对话与证据链",
    "",
    sections.join("\n\n---\n\n"),
    "",
  ]
    .filter(Boolean)
    .join("\n");
}

function renderTurnAnswer(turn: QaTurn, index: number) {
  const answer = turn.answer;
  const coverage = requirementCoverageDiagnostics(answer);
  const gitAgent = gitAgentDiagnostics(answer);
  return (
    <Space orientation="vertical" size="middle" className="w-full">
      {coverage ? (
        <Alert
          type="info"
          showIcon
          title="已按版本需求覆盖模式分析"
          description={`已识别 ${coverage.requirementCandidateCount} 条需求候选，并找到 ${coverage.implementationCandidateCount} 条代码候选。${
            coverage.testCandidateCount > 0
              ? `另找到 ${coverage.testCandidateCount} 条测试源码候选，但源码存在不代表已经执行通过。`
              : "未找到测试源码候选。"
          }${
            coverage.explicitRelationCount > 0
              ? `当前版本存在 ${coverage.explicitRelationCount} 条已确认的实现或验证关系，其中 ${coverage.verifiedRelationCount} 条为验证关系。`
              : "尚无显式需求—代码关系，系统会保留“待确认”状态，不会把“没搜到”误判成“未实现”。"
          }`}
        />
      ) : null}
      {gitAgent ? (
        <Alert
          type={gitAgent.status === "completed" ? "success" : "warning"}
          showIcon
          title={
            gitAgent.status === "completed"
              ? "Git Agent 已完成只读统计"
              : gitAgent.status === "partial"
                ? "Git Agent 已返回部分结果"
                : "Git Agent 未取得可用证据"
          }
          description={`按所选版本的冻结提交查询 ${gitAgent.repositoryCount} 个关联仓库，成功 ${gitAgent.succeededCount} 个，失败 ${gitAgent.failedCount} 个。统计包含合并提交，逐仓库计算后相加${
            gitAgent.totalCommitCount == null
              ? "。"
              : `，合计 ${gitAgent.totalCommitCount} 次。`
          }`}
        />
      ) : null}
      {answer.citationValidation === "unverified" ? (
        <Alert
          type="warning"
          showIcon
          title="模型回答的引用未通过校验"
          description="以下内容为 AI Provider 原始响应，至少有一个事实段落未附带本次检索证据中的有效引用。请结合下方“引用证据”核对后再使用。"
        />
      ) : null}
      <MarkdownPreview
        content={answer.answer}
        testId={
          index === 0
            ? "project-qa-answer-markdown-preview"
            : `project-qa-answer-markdown-preview-${turn.id}`
        }
      />
      {answer.conflicts.map((value) => (
        <Alert
          key={value}
          type="warning"
          showIcon
          title="来源存在冲突"
          description={value}
        />
      ))}
      {answer.evidenceGaps.map((value) => (
        <Alert
          key={value}
          type="info"
          showIcon
          title="证据缺口"
          description={value}
        />
      ))}
      <div>
        <Text strong>引用证据</Text>
        {answer.citations.length ? (
          <Collapse
            className="mt-2"
            items={answer.citations.map((citation) => ({
              key: citation.citationKey,
              label: citationRole(citation, coverage) ? (
                <Space size={6} wrap>
                  <span>{citationLabel(citation)}</span>
                  <Tag className="!mr-0" color="blue">
                    {citationRole(citation, coverage)}
                  </Tag>
                </Space>
              ) : (
                citationLabel(citation)
              ),
              children: (
                <Space orientation="vertical" size={4}>
                  <Text type="secondary">
                    {citation.logicalPath || "未记录逻辑路径"}
                  </Text>
                  <Paragraph className="!mb-0 whitespace-pre-wrap">
                    {citation.excerpt}
                  </Paragraph>
                  {citation.commitSha ? (
                    <Text code>冻结提交 {citation.commitSha}</Text>
                  ) : null}
                  <Tag>{citation.citationKey}</Tag>
                </Space>
              ),
            }))}
          />
        ) : (
          <Text type="secondary" className="ml-2">
            本轮未返回可引用证据
          </Text>
        )}
      </div>
    </Space>
  );
}

/**
 * 项目问答页面维护当前路由范围内的会话上下文。普通问题只检索当前问题；版本需求
 * 覆盖追问会由后端复用最近的用户需求范围，但历史助手回答仍不能充当事实证据。
 * 切换版本或 Provider 时主动清空，避免跨范围继续对话。
 */
export default function ProjectQaPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [providers, setProviders] = useState<AiProvider[]>([]);
  const [releaseId, setReleaseId] = useState<number | null>(null);
  const [question, setQuestion] = useState("");
  const [providerKey, setProviderKey] = useState("");
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const [saving, setSaving] = useState(false);
  const [sessions, setSessions] = useState<KnowledgeQaSession[]>([]);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [sessionLoading, setSessionLoading] = useState(false);
  const [sessionReadOnly, setSessionReadOnly] = useState(false);
  const [turns, setTurns] = useState<QaTurn[]>([]);
  const [askError, setAskError] = useState<string | null>(null);
  const [askErrorRetryable, setAskErrorRetryable] = useState(false);
  const requestId = useRef(0);
  const askRequestId = useRef(0);
  const sessionRequestId = useRef(0);
  const [messageApi, messageContextHolder] = message.useMessage();

  const chatProviders = useMemo(
    () => providers.filter(isChatProvider),
    [providers],
  );
  const selectedProvider = useMemo(
    () => chatProviders.find((provider) => provider.key === providerKey),
    [chatProviders, providerKey],
  );
  const selectedRelease = useMemo(
    () => releases.find((release) => release.id === releaseId),
    [releaseId, releases],
  );
  // 刷新是异步的，不能把 releaseId/providerKey 放进 load 的依赖，否则每次选择
  // 版本或服务商都会触发整页重载。用 refs 保留当前会话作用域，刷新返回新列表时
  // 可以判断旧回答是否仍属于同一个版本和聊天模型。
  const releaseIdRef = useRef<number | null>(null);
  const releaseIdentityRef = useRef("");
  const providerKeyRef = useRef("");
  const providerModelRef = useRef("");
  const sessionIdRef = useRef<number | null>(null);
  releaseIdRef.current = releaseId;
  releaseIdentityRef.current = releaseScopeIdentity(selectedRelease);
  providerKeyRef.current = providerKey;
  providerModelRef.current = selectedProvider?.defaultModel ?? "";
  sessionIdRef.current = sessionId;

  const invalidateAskRequest = useCallback(() => {
    askRequestId.current += 1;
    setAsking(false);
  }, []);

  const clearConversation = useCallback(() => {
    // 清空会话也要使尚未返回的请求失效，避免旧版本/旧 Provider 的响应污染新会话。
    invalidateAskRequest();
    setTurns([]);
    setSessionId(null);
    setSessionReadOnly(false);
    sessionRequestId.current += 1;
    setQuestion("");
    setAskError(null);
    setAskErrorRetryable(false);
  }, [invalidateAskRequest]);

  const load = useCallback(async () => {
    // 刷新可能同时改变版本或 Provider 列表，先让当前请求进入不可提交状态。
    invalidateAskRequest();
    const currentRequest = ++requestId.current;
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      setLoadError("项目地址无效");
      setLoading(false);
      return;
    }
    setLoading(true);
    setLoadError(null);
    try {
      const projects = await knowledgeCatalogApi.listProjects({
        projectId: numericProjectId,
        offset: 0,
        limit: 1,
      });
      const nextProject =
        projects.items.find((item) => item.id === numericProjectId) ?? null;
      const [nextReleases, nextProviders, nextSessions] = nextProject
        ? await Promise.all([
            knowledgeCatalogApi.listReleases(nextProject.id),
            aiProviderApi.list(),
            knowledgeQaApi.listSessions(nextProject.id),
          ])
        : [[], await aiProviderApi.list(), []];
      if (requestId.current !== currentRequest) return;
      const nextChatProviders = nextProviders.filter(isChatProvider);
      const nextSelectedProvider = nextChatProviders.find(
        (provider) => provider.key === providerKeyRef.current,
      );
      const nextSelectedRelease = nextReleases.find(
        (release) => release.id === releaseIdRef.current,
      );
      const releaseScopeChanged =
        releaseIdentityRef.current !== "" &&
        releaseScopeIdentity(nextSelectedRelease) !==
          releaseIdentityRef.current;
      const providerScopeChanged =
        providerKeyRef.current !== "" &&
        (!nextSelectedProvider ||
          nextSelectedProvider.defaultModel !== providerModelRef.current);
      setProject(nextProject);
      setReleases(nextReleases);
      setProviders(nextProviders);
      setSessions(nextSessions);
      const compatibleSessions = nextSessions.filter((item) =>
        isSessionCompatible(item, nextReleases, nextProviders),
      );
      const targetSession =
        releaseScopeChanged || providerScopeChanged
          ? undefined
          : (compatibleSessions.find(
              (item) => item.id === sessionIdRef.current,
            ) ?? compatibleSessions[0]);
      const detail = targetSession
        ? await knowledgeQaApi.getSession(nextProject!.id, targetSession.id)
        : null;
      if (requestId.current !== currentRequest) return;
      if (detail) {
        setSessionId(detail.session.id);
        setSessionReadOnly(false);
        setTurns(turnsFromSession(detail));
        setReleaseId(detail.session.projectVersionId);
        setProviderKey(detail.session.providerKey);
        return;
      }
      setSessionId(null);
      setSessionReadOnly(false);
      setTurns([]);
      setReleaseId((current) =>
        current != null &&
        nextReleases.some((release) => release.id === current)
          ? current
          : (nextReleases[0]?.id ?? null),
      );
      setProviderKey((current) => {
        const available = nextChatProviders;
        return available.some((provider) => provider.key === current)
          ? current
          : (available[0]?.key ?? "");
      });
    } catch (error) {
      if (requestId.current === currentRequest)
        setLoadError(getErrorMessage(error));
    } finally {
      if (requestId.current === currentRequest) setLoading(false);
    }
  }, [invalidateAskRequest, numericProjectId]);

  useEffect(() => {
    setProject(null);
    setReleases([]);
    setReleaseId(null);
    clearConversation();
    setAskError(null);
    void load();
    return () => {
      requestId.current += 1;
      askRequestId.current += 1;
      sessionRequestId.current += 1;
    };
  }, [clearConversation, load]);

  useEffect(() => {
    if (turns.length && releaseId != null && !selectedRelease) {
      clearConversation();
    }
  }, [clearConversation, releaseId, selectedRelease, turns.length]);

  async function ask(evidenceOnly: boolean) {
    if (!project || releaseId == null || !question.trim()) return;
    setAskErrorRetryable(false);
    if (sessionReadOnly) {
      setAskError("当前历史对话的版本或模型已经变化，请新建对话后继续提问");
      return;
    }
    if (!evidenceOnly && !selectedProvider?.defaultModel.trim()) {
      setAskError("请选择已配置默认聊天模型的 AI 服务商，或先查看本地证据");
      return;
    }
    const askRequest = ++askRequestId.current;
    const questionText = question.trim();
    const history = conversationMessages(turns);
    setAsking(true);
    setAskError(null);
    try {
      const result = await knowledgeQaApi.askScopedQuestion({
        projectId: project.id,
        projectVersionId: releaseId,
        question: questionText,
        evidenceOnly,
        providerKey: evidenceOnly ? undefined : selectedProvider?.key,
        model: evidenceOnly ? undefined : selectedProvider?.defaultModel,
        conversation: history,
      });
      if (askRequest !== askRequestId.current) return;
      const handledByLocalAgent =
        result.retrievalDiagnostics.queryMode === "gitAgent";
      const persistedEvidenceOnly = evidenceOnly || handledByLocalAgent;
      const persistedProviderKey = persistedEvidenceOnly
        ? ""
        : (selectedProvider?.key ?? "");
      const persistedModel = persistedEvidenceOnly
        ? ""
        : (selectedProvider?.defaultModel ?? "");
      const currentSession = sessions.find((item) => item.id === sessionId);
      const compatibleSessionId =
        currentSession?.providerKey === persistedProviderKey &&
        currentSession.model === persistedModel
          ? sessionId
          : null;
      try {
        const detail = await knowledgeQaApi.persistRound({
          sessionId: compatibleSessionId ?? undefined,
          projectId: project.id,
          projectVersionId: releaseId,
          providerKey: persistedProviderKey,
          model: persistedModel,
          question: questionText,
          answer: result,
          evidenceOnly: persistedEvidenceOnly,
        });
        if (askRequest !== askRequestId.current) return;
        setSessionId(detail.session.id);
        setSessionReadOnly(false);
        setTurns(turnsFromSession(detail));
        setQuestion("");
        setSessions((current) => [
          detail.session,
          ...current.filter((item) => item.id !== detail.session.id),
        ]);
        void knowledgeQaApi
          .listSessions(project.id)
          .then(setSessions)
          .catch(() => undefined);
      } catch (error) {
        if (askRequest === askRequestId.current) {
          setAskError(`回答已生成，但会话保存失败：${getErrorMessage(error)}`);
        }
      }
    } catch (error) {
      if (askRequest === askRequestId.current) {
        setAskError(getErrorMessage(error));
        setAskErrorRetryable(getErrorCode(error) === "PROVIDER_TRANSIENT");
      }
    } finally {
      if (askRequest === askRequestId.current) {
        setAsking(false);
      }
    }
  }

  async function openSession(nextSessionId: number) {
    if (!project || nextSessionId === sessionId) return;
    invalidateAskRequest();
    const sessionRequest = ++sessionRequestId.current;
    setSessionLoading(true);
    setAskError(null);
    setAskErrorRetryable(false);
    try {
      const detail = await knowledgeQaApi.getSession(project.id, nextSessionId);
      if (sessionRequest !== sessionRequestId.current) return;
      setSessionId(detail.session.id);
      setReleaseId(detail.session.projectVersionId);
      setProviderKey(detail.session.providerKey);
      setTurns(turnsFromSession(detail));
      setSessionReadOnly(
        !isSessionCompatible(detail.session, releases, providers),
      );
      setQuestion("");
    } catch (error) {
      if (sessionRequest === sessionRequestId.current) {
        setAskError(getErrorMessage(error));
      }
    } finally {
      if (sessionRequest === sessionRequestId.current) {
        setSessionLoading(false);
      }
    }
  }

  async function deleteSession(nextSessionId: number) {
    if (!project) return;
    try {
      await knowledgeQaApi.deleteSession(project.id, nextSessionId);
      const nextSessions = await knowledgeQaApi.listSessions(project.id);
      setSessions(nextSessions);
      if (nextSessionId === sessionId) {
        clearConversation();
        const first = nextSessions[0];
        if (first) await openSession(first.id);
      }
      messageApi.success("对话已删除");
    } catch (error) {
      messageApi.error(getErrorMessage(error));
    }
  }

  async function saveConversation() {
    if (!project || !selectedRelease || turns.length === 0) return;
    setSaving(true);
    try {
      const content = buildConversationMarkdown(
        project,
        selectedRelease,
        selectedProvider,
        turns,
        sessions.find((item) => item.id === sessionId),
      );
      const result = await knowledgeQaApi.saveMarkdown({
        content,
        defaultFileName: `${project.projectKey}-${selectedRelease.version}-qa.md`,
      });
      if (result === "saved") {
        messageApi.success("问答结果与证据链已保存为 Markdown 文档");
      } else if (result === "downloaded") {
        messageApi.success("问答结果与证据链已下载为 Markdown 文档");
      }
    } catch (error) {
      messageApi.error(getErrorMessage(error));
    } finally {
      setSaving(false);
    }
  }

  if (loading) return <Skeleton active className="mt-8 w-full px-6" />;
  if (loadError) {
    return (
      <main className="mt-8 w-full px-6">
        <Alert
          type="error"
          showIcon
          title="无法打开项目问答"
          description={loadError}
          action={<Button onClick={() => void load()}>重试</Button>}
        />
      </main>
    );
  }
  if (!project) {
    return (
      <main className="mt-8 w-full px-6">
        <Empty description="没有找到这个项目">
          <Button
            type="primary"
            onClick={() => navigate("/knowledge/projects")}
          >
            返回项目列表
          </Button>
        </Empty>
      </main>
    );
  }
  if (releases.length === 0) {
    return (
      <main className="mt-8 w-full px-6">
        <Alert
          type="info"
          showIcon
          title="请先创建一个项目版本"
          description="项目问答必须绑定到一个版本，确保引用的文档与代码证据不会串用。"
          action={
            <Button
              type="primary"
              onClick={() =>
                navigate(`/knowledge/projects/${project.id}/versions`)
              }
            >
              管理项目版本
            </Button>
          }
        />
      </main>
    );
  }

  return (
    <>
      {messageContextHolder}
      <main className="flex h-[calc(100vh-64px)] w-full flex-col px-4 py-4 sm:px-6">
        <Button
          type="link"
          className="!mb-4 !self-start !px-0"
          icon={<ArrowLeft size={16} />}
          onClick={() => navigate(`/knowledge/projects/${project.id}/overview`)}
        >
          返回项目概览
        </Button>
        <div className="mb-4 flex flex-wrap items-start justify-between gap-4">
          <div>
            <Title level={2} className="!mb-1">
              问答 {project.name}
            </Title>
            <Paragraph type="secondary" className="!mb-0">
              像 Codex 一样连续提问；对话与引用证据会自动保存到本地。
            </Paragraph>
          </div>
          <Space wrap>
            {turns.length ? (
              <Button
                icon={<FileDown size={16} />}
                loading={saving}
                onClick={() => void saveConversation()}
              >
                保存当前对话（Markdown）
              </Button>
            ) : null}
            <Button icon={<RefreshCw size={16} />} onClick={() => void load()}>
              刷新
            </Button>
          </Space>
        </div>
        <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-[280px_minmax(0,1fr)]">
          <Card
            className="min-h-0 overflow-hidden"
            styles={{ body: { padding: 12, height: "100%" } }}
          >
            <Button
              type="primary"
              block
              icon={<Plus size={16} />}
              onClick={clearConversation}
            >
              新建对话
            </Button>
            <div className="mt-4 flex items-center gap-2 px-2">
              <BookOpenCheck size={16} />
              <Text strong>历史对话</Text>
            </div>
            <div className="mt-2 max-h-[calc(100vh-260px)] overflow-y-auto">
              {sessionLoading ? (
                <Skeleton active paragraph={{ rows: 3 }} title={false} />
              ) : sessions.length ? (
                sessions.map((session) => (
                  <div
                    key={session.id}
                    className={`flex w-full items-center rounded-lg pr-1 ${
                      session.id === sessionId
                        ? "bg-[var(--bg-secondary)]"
                        : "hover:bg-[var(--bg-secondary)]"
                    }`}
                  >
                    <Button
                      type="text"
                      block
                      className="!h-auto !min-w-0 !justify-start !px-2 !py-2 !text-left"
                      onClick={() => void openSession(session.id)}
                    >
                      <span className="min-w-0">
                        <span className="block truncate">{session.title}</span>
                        <Text type="secondary" className="text-xs">
                          {Math.floor(session.messageCount / 2)} 轮
                        </Text>
                        {!isSessionCompatible(session, releases, providers) ? (
                          <Tag className="ml-1" color="default">
                            只读
                          </Tag>
                        ) : null}
                      </span>
                    </Button>
                    <Popconfirm
                      title="删除这个对话？"
                      description="删除后不会影响知识文档。"
                      okText="删除"
                      cancelText="取消"
                      onConfirm={() => void deleteSession(session.id)}
                    >
                      <Button
                        type="text"
                        danger
                        aria-label={`删除对话 ${session.title}`}
                        icon={<Trash2 size={15} />}
                      />
                    </Popconfirm>
                  </div>
                ))
              ) : (
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description="还没有保存的对话"
                />
              )}
            </div>
          </Card>

          <Card
            className="min-h-0 overflow-hidden"
            styles={{ body: { padding: 0, height: "100%" } }}
          >
            <div className="flex h-full min-h-0 flex-col">
              <div className="grid gap-2 border-b border-[var(--border)] p-3 md:grid-cols-2">
                <Select
                  aria-label="项目版本"
                  value={releaseId ?? undefined}
                  options={releases.map((release) => ({
                    value: release.id,
                    label: release.version,
                  }))}
                  onChange={(value) => {
                    if (value !== releaseId) clearConversation();
                    setReleaseId(value);
                  }}
                />
                <Select
                  aria-label="AI 服务商"
                  allowClear
                  showSearch
                  optionFilterProp="label"
                  placeholder="仅查看证据时无需选择"
                  value={providerKey || undefined}
                  options={chatProviders.map((provider) => ({
                    value: provider.key,
                    label: `${provider.name}（${provider.defaultModel}）`,
                  }))}
                  onChange={(value) => {
                    const next = value ?? "";
                    if (next !== providerKey) clearConversation();
                    setProviderKey(next);
                  }}
                />
              </div>

              <div className="min-h-0 flex-1 overflow-y-auto px-4 py-5 sm:px-8">
                {turns.length ? (
                  <div className="w-full space-y-7">
                    <div className="flex justify-center">
                      <Tag color="blue">已进行 {turns.length} 轮</Tag>
                    </div>
                    {turns.map((turn, index) => (
                      <div key={turn.id} className="space-y-5">
                        <div className="flex justify-end gap-3">
                          <div className="max-w-[82%] rounded-2xl rounded-tr-sm bg-[var(--accent)] px-4 py-3 text-white">
                            <Paragraph className="!mb-0 whitespace-pre-wrap !text-inherit">
                              {turn.question}
                            </Paragraph>
                          </div>
                          <div className="mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--bg-secondary)]">
                            <User size={16} />
                          </div>
                        </div>
                        <div className="flex items-start gap-3">
                          <div className="mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--bg-tertiary)] text-[var(--accent)]">
                            <Bot size={17} />
                          </div>
                          <div className="min-w-0 flex-1">
                            <Text
                              type="secondary"
                              className="mb-2 block text-xs"
                            >
                              {turn.evidenceOnly ? "本地证据" : "知识助手"}
                            </Text>
                            {renderTurnAnswer(turn, index)}
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="flex h-full min-h-64 flex-col items-center justify-center text-center">
                    <Bot size={40} className="mb-3 text-[var(--accent)]" />
                    <Title level={4} className="!mb-1">
                      开始项目知识对话
                    </Title>
                    <Text type="secondary">
                      提问后会自动保存，可在左侧随时继续以前的对话。
                    </Text>
                    <Button
                      className="!mt-4"
                      icon={<BookOpenCheck size={16} />}
                      disabled={asking || sessionReadOnly || !selectedRelease}
                      onClick={() =>
                        setQuestion(
                          `请逐条分析 ${selectedRelease?.version ?? "当前版本"} 的需求：哪些已确认实现，哪些只找到代码候选，哪些尚未找到实现证据？`,
                        )
                      }
                    >
                      分析当前版本需求实现情况
                    </Button>
                  </div>
                )}
              </div>

              <div className="border-t border-[var(--border)] p-3 sm:p-4">
                <div className="w-full">
                  {askError ? (
                    <Alert
                      className="!mb-3"
                      type="error"
                      showIcon
                      title={askError}
                      action={
                        askErrorRetryable ? (
                          <Button
                            size="small"
                            disabled={!question.trim() || sessionReadOnly}
                            loading={asking}
                            onClick={() => void ask(false)}
                          >
                            重试回答
                          </Button>
                        ) : undefined
                      }
                      closable
                      onClose={() => {
                        setAskError(null);
                        setAskErrorRetryable(false);
                      }}
                    />
                  ) : null}
                  <Input.TextArea
                    aria-label="项目问题"
                    autoSize={{ minRows: 2, maxRows: 6 }}
                    maxLength={2000}
                    disabled={asking || sessionReadOnly}
                    value={question}
                    placeholder="向项目知识库提问…（Enter 发送，Shift + Enter 换行）"
                    onChange={(event) => setQuestion(event.target.value)}
                    onPressEnter={(event) => {
                      if (!event.shiftKey) {
                        event.preventDefault();
                        if (question.trim()) void ask(false);
                      }
                    }}
                  />
                  <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
                    <Text type="secondary" className="text-xs">
                      {sessionReadOnly
                        ? "此历史对话的版本或模型已变化，仅供查看"
                        : sessionId
                          ? "本轮回答将自动保存到当前对话"
                          : "首次回答后将自动创建并保存对话"}
                    </Text>
                    <Space wrap>
                      <Button
                        icon={<Eye size={16} />}
                        disabled={
                          sessionReadOnly ||
                          releaseId == null ||
                          !question.trim()
                        }
                        loading={asking}
                        onClick={() => void ask(true)}
                      >
                        查看本地证据
                      </Button>
                      <Button
                        type="primary"
                        icon={<Send size={16} />}
                        disabled={
                          sessionReadOnly ||
                          releaseId == null ||
                          !question.trim()
                        }
                        loading={asking}
                        onClick={() => void ask(false)}
                      >
                        基于证据回答
                      </Button>
                    </Space>
                  </div>
                </div>
              </div>
            </div>
          </Card>
        </div>
      </main>
    </>
  );
}
