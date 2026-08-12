import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Empty,
  Input,
  Result,
  Select,
  Skeleton,
  Spin,
  Space,
  Progress,
  Steps,
  Tag,
  Typography,
} from "antd";
import {
  ArrowLeft,
  Bot,
  FileOutput,
  GitBranch,
  RefreshCw,
  ScanSearch,
} from "lucide-react";
import { MarkdownPreview } from "@/components/ui/MarkdownPreview";
import { aiProviderApi, getErrorMessage } from "@/lib/api";
import {
  knowledgeAnalysisApi,
  knowledgeCatalogApi,
} from "@/lib/api/knowledge-domain";
import type {
  KnowledgeCodeAnalysisResult,
  KnowledgeCodeSnapshot,
  KnowledgeCodeSource,
  KnowledgeAnalysisDraft,
  GenerateKnowledgeCodeDocumentsResult,
  KnowledgeProject,
  KnowledgeRelease,
  AiProvider,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

type AnalysisStage =
  "idle" | "capturing" | "analyzing" | "generating" | "drafting" | "confirming";

const operationStageInfo: Record<
  Exclude<AnalysisStage, "idle">,
  { label: string; percent: number }
> = {
  capturing: { label: "正在捕获固定 Git Commit", percent: 20 },
  analyzing: { label: "正在运行静态分析", percent: 50 },
  generating: { label: "正在生成项目分析文档", percent: 70 },
  drafting: { label: "正在生成 AI 分析草稿", percent: 85 },
  confirming: { label: "正在确认并写入知识库", percent: 95 },
};

function isChatProvider(provider: AiProvider) {
  const capabilities = (provider.capabilities ?? []).map((value) =>
    value.trim().toLowerCase(),
  );
  const hasExplicitMode = capabilities.some((value) =>
    ["chat", "embedding"].includes(value),
  );
  const supportsChat = hasExplicitMode
    ? capabilities.includes("chat")
    : Boolean(provider.defaultModel.trim());
  return (
    provider.enabled &&
    provider.status === "configured" &&
    supportsChat &&
    Boolean(provider.defaultModel.trim())
  );
}

function snapshotStatus(snapshot: KnowledgeCodeSnapshot) {
  const labels: Record<string, { label: string; color: string }> = {
    captured: { label: "等待分析", color: "gold" },
    analyzing: { label: "正在分析", color: "blue" },
    analyzed: { label: "已分析", color: "green" },
    failed: { label: "分析失败", color: "red" },
  };
  return (
    labels[snapshot.status] ?? {
      label: snapshot.status || "未知",
      color: "default",
    }
  );
}

function sourceLabel(source: KnowledgeCodeSource) {
  return (
    source.source.displayName ||
    source.source.gitWorkspaceKey ||
    `源码来源 #${source.source.id}`
  );
}

function snapshotSourceLabel(
  snapshot: KnowledgeCodeSnapshot,
  sources: KnowledgeCodeSource[],
) {
  const source = sources.find((item) => item.source.id === snapshot.sourceId);
  return source ? sourceLabel(source) : `源码来源 #${snapshot.sourceId}`;
}

/**
 * 源码工作台将一次操作固定为“选择来源和版本 → 捕获 Commit → 静态分析 → 生成报告”。
 * 所有写入仍由后端根据不可变 Commit 处理，页面不会访问或执行用户工作树。
 */
export default function ProjectAnalysisPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [releases, setReleases] = useState<KnowledgeRelease[]>([]);
  const [sources, setSources] = useState<KnowledgeCodeSource[]>([]);
  const [providers, setProviders] = useState<AiProvider[]>([]);
  const [snapshots, setSnapshots] = useState<KnowledgeCodeSnapshot[]>([]);
  // 单仓库操作沿用 snapshots；联合 AI 分析单独保留项目内全部来源的快照，避免切换
  // 当前捕获来源时丢失已经选定的其他服务 Commit。
  const [jointSnapshots, setJointSnapshots] = useState<KnowledgeCodeSnapshot[]>(
    [],
  );
  const [selectedSourceId, setSelectedSourceId] = useState<number | null>(null);
  const [selectedReleaseId, setSelectedReleaseId] = useState<number | null>(
    null,
  );
  const [selectedSnapshotId, setSelectedSnapshotId] = useState<number | null>(
    null,
  );
  const [selectedJointSnapshotIds, setSelectedJointSnapshotIds] = useState<
    number[]
  >([]);
  const [gitRef, setGitRef] = useState("HEAD");
  const [analysisResult, setAnalysisResult] =
    useState<KnowledgeCodeAnalysisResult | null>(null);
  const [generationResult, setGenerationResult] =
    useState<GenerateKnowledgeCodeDocumentsResult | null>(null);
  const [generatedThisRun, setGeneratedThisRun] = useState<number | null>(null);
  const [selectedProviderKey, setSelectedProviderKey] = useState<string>();
  const [aiDraft, setAiDraft] = useState<KnowledgeAnalysisDraft | null>(null);
  const [draftTitle, setDraftTitle] = useState("");
  const [draftContent, setDraftContent] = useState("");
  const [editingDraft, setEditingDraft] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [operationNotice, setOperationNotice] = useState<string | null>(null);
  const [stage, setStage] = useState<AnalysisStage>("idle");
  const [snapshotsLoading, setSnapshotsLoading] = useState(false);
  const [jointSnapshotsLoading, setJointSnapshotsLoading] = useState(false);
  const loadRequestId = useRef(0);
  const snapshotRequestId = useRef(0);
  const jointSnapshotRequestId = useRef(0);
  const operationRequestId = useRef(0);
  const operationRunning = useRef(false);

  const load = useCallback(async () => {
    const requestId = ++loadRequestId.current;
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      setError("项目地址无效");
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    setOperationError(null);
    setOperationNotice(null);
    setAnalysisResult(null);
    setGenerationResult(null);
    setGeneratedThisRun(null);
    setAiDraft(null);
    setDraftTitle("");
    setDraftContent("");
    setEditingDraft(false);
    try {
      const projects = await knowledgeCatalogApi.listProjects({
        projectId: numericProjectId,
        limit: 1,
        offset: 0,
      });
      const selectedProject =
        projects.items.find((item) => item.id === numericProjectId) ?? null;
      const [projectReleases, projectSources, configuredProviders] =
        selectedProject
          ? await Promise.all([
              knowledgeCatalogApi.listReleases(selectedProject.id),
              knowledgeAnalysisApi.listCodeSources(selectedProject.id),
              aiProviderApi.list(),
            ])
          : [[], [] as KnowledgeCodeSource[], [] as AiProvider[]];
      if (requestId !== loadRequestId.current) return;
      setProject(selectedProject);
      setReleases(projectReleases);
      setSources(projectSources);
      const chatProviders = configuredProviders.filter(isChatProvider);
      setProviders(chatProviders);
      setSnapshots([]);
      setJointSnapshots([]);
      setSelectedSourceId((current) =>
        current != null &&
        projectSources.some((item) => item.source.id === current)
          ? current
          : (projectSources[0]?.source.id ?? null),
      );
      setSelectedReleaseId((current) =>
        current != null && projectReleases.some((item) => item.id === current)
          ? current
          : null,
      );
      setSelectedSnapshotId(null);
      setSelectedJointSnapshotIds([]);
      setSelectedProviderKey((current) =>
        current && chatProviders.some((provider) => provider.key === current)
          ? current
          : chatProviders[0]?.key,
      );
    } catch (cause) {
      if (requestId === loadRequestId.current) setError(getErrorMessage(cause));
    } finally {
      if (requestId === loadRequestId.current) setLoading(false);
    }
  }, [numericProjectId]);

  useEffect(() => {
    setProject(null);
    setReleases([]);
    setSources([]);
    setProviders([]);
    setSnapshots([]);
    setJointSnapshots([]);
    setSelectedSourceId(null);
    setSelectedReleaseId(null);
    setSelectedSnapshotId(null);
    setSelectedJointSnapshotIds([]);
    setSelectedProviderKey(undefined);
    setAnalysisResult(null);
    setGenerationResult(null);
    setGeneratedThisRun(null);
    setAiDraft(null);
    setDraftTitle("");
    setDraftContent("");
    setOperationError(null);
    setOperationNotice(null);
    setStage("idle");
    operationRunning.current = false;
    void load();
    return () => {
      loadRequestId.current += 1;
      operationRequestId.current += 1;
      operationRunning.current = false;
    };
  }, [load]);

  const loadSnapshots = useCallback(async () => {
    const requestId = ++snapshotRequestId.current;
    setSnapshotsLoading(true);
    if (!project || selectedSourceId == null) {
      setSnapshots([]);
      setSelectedSnapshotId(null);
      setSnapshotsLoading(false);
      return;
    }
    try {
      const projectSnapshots = await knowledgeAnalysisApi.listCodeSnapshots({
        projectId: project.id,
        sourceId: selectedSourceId,
      });
      if (requestId !== snapshotRequestId.current) return;
      setSnapshots(projectSnapshots);
      setSelectedSnapshotId((current) =>
        current != null && projectSnapshots.some((item) => item.id === current)
          ? current
          : (projectSnapshots[0]?.id ?? null),
      );
    } catch (cause) {
      if (requestId !== snapshotRequestId.current) return;
      setSnapshots([]);
      setSelectedSnapshotId(null);
      setOperationError(getErrorMessage(cause));
    } finally {
      if (requestId === snapshotRequestId.current) setSnapshotsLoading(false);
    }
  }, [project, selectedSourceId]);

  useEffect(() => {
    void loadSnapshots();
    return () => {
      snapshotRequestId.current += 1;
    };
  }, [loadSnapshots]);

  const loadJointSnapshots = useCallback(async () => {
    const requestId = ++jointSnapshotRequestId.current;
    if (!project) {
      setJointSnapshots([]);
      return;
    }
    setJointSnapshotsLoading(true);
    try {
      const projectSnapshots = await knowledgeAnalysisApi.listCodeSnapshots({
        projectId: project.id,
        sourceId: null,
      });
      if (requestId !== jointSnapshotRequestId.current) return;
      setJointSnapshots(projectSnapshots);
      setSelectedJointSnapshotIds((current) =>
        current.length > 0
          ? current.filter((snapshotId) =>
              projectSnapshots.some((snapshot) => snapshot.id === snapshotId),
            )
          : projectSnapshots
              .filter(
                (snapshot) =>
                  snapshot.status === "analyzed" && snapshot.releaseId != null,
              )
              .slice(0, 1)
              .map((snapshot) => snapshot.id),
      );
    } catch (cause) {
      if (requestId !== jointSnapshotRequestId.current) return;
      setJointSnapshots([]);
      setOperationError(getErrorMessage(cause));
    } finally {
      if (requestId === jointSnapshotRequestId.current) {
        setJointSnapshotsLoading(false);
      }
    }
  }, [project]);

  useEffect(() => {
    void loadJointSnapshots();
    return () => {
      jointSnapshotRequestId.current += 1;
    };
  }, [loadJointSnapshots]);

  const selectedSnapshot =
    snapshots.find((item) => item.id === selectedSnapshotId) ?? null;
  const selectedJointSnapshots = jointSnapshots.filter((snapshot) =>
    selectedJointSnapshotIds.includes(snapshot.id),
  );
  const jointProjectVersionId =
    selectedReleaseId ?? selectedJointSnapshots[0]?.releaseId ?? null;
  const hasValidJointSelection =
    jointProjectVersionId != null &&
    selectedJointSnapshots.length > 0 &&
    selectedJointSnapshots.every(
      (snapshot) =>
        snapshot.releaseId === jointProjectVersionId &&
        snapshot.status === "analyzed",
    );
  const jointSnapshotOptions = jointSnapshots
    .filter(
      (snapshot) =>
        snapshot.status === "analyzed" &&
        (jointProjectVersionId == null ||
          snapshot.releaseId === jointProjectVersionId),
    )
    .map((snapshot) => ({
      value: snapshot.id,
      label: `${snapshotSourceLabel(snapshot, sources)} · ${snapshot.refName || snapshot.commitSha} · ${snapshot.commitSha.slice(0, 12)}`,
    }));

  async function runOperation(
    nextStage: Exclude<AnalysisStage, "idle">,
    action: (isCurrent: () => boolean) => Promise<void>,
  ) {
    if (operationRunning.current) return;
    const operationId = ++operationRequestId.current;
    const isCurrent = () => operationRequestId.current === operationId;
    operationRunning.current = true;
    setStage(nextStage);
    setOperationError(null);
    setOperationNotice(null);
    if (nextStage === "capturing" || nextStage === "analyzing") {
      setAnalysisResult(null);
      setGenerationResult(null);
      setGeneratedThisRun(null);
      setAiDraft(null);
      setDraftTitle("");
      setDraftContent("");
    } else if (nextStage === "generating") {
      setGenerationResult(null);
      setGeneratedThisRun(null);
    } else if (nextStage === "drafting") {
      setAiDraft(null);
      setDraftTitle("");
      setDraftContent("");
    }
    try {
      await action(isCurrent);
    } catch (cause) {
      if (isCurrent()) setOperationError(getErrorMessage(cause));
    } finally {
      if (isCurrent()) {
        operationRunning.current = false;
        setStage("idle");
      }
    }
  }

  function captureSnapshot() {
    if (
      !project ||
      selectedSourceId == null ||
      !gitRef.trim() ||
      snapshotsLoading
    )
      return;
    snapshotRequestId.current += 1;
    // 失效仍在返回中的全项目快照请求，避免它以捕获前的列表覆盖刚写入的联合候选。
    jointSnapshotRequestId.current += 1;
    setSnapshotsLoading(false);
    void runOperation("capturing", async (isCurrent) => {
      const captured = await knowledgeAnalysisApi.captureGitSnapshot({
        projectId: project.id,
        sourceId: selectedSourceId,
        gitRef: gitRef.trim(),
        projectVersionId: selectedReleaseId,
      });
      if (!isCurrent()) return;
      setSnapshots((current) => [
        captured,
        ...current.filter((item) => item.id !== captured.id),
      ]);
      setJointSnapshots((current) => [
        captured,
        ...current.filter((item) => item.id !== captured.id),
      ]);
      setSelectedSnapshotId(captured.id);
      setAnalysisResult(null);
      setGenerationResult(null);
      setGeneratedThisRun(null);
      setAiDraft(null);
      setOperationNotice("已捕获固定 Git Commit 快照，可以开始静态分析。");
    });
  }

  function analyzeSnapshot() {
    if (!project || !selectedSnapshot) return;
    // 分析完成会改变联合候选资格；旧列表不能覆盖本地更新后的 analyzed 状态。
    jointSnapshotRequestId.current += 1;
    void runOperation("analyzing", async (isCurrent) => {
      const result = await knowledgeAnalysisApi.analyzeSnapshot({
        projectId: project.id,
        snapshotId: selectedSnapshot.id,
      });
      if (!isCurrent()) return;
      setAnalysisResult(result);
      setGenerationResult(null);
      setGeneratedThisRun(null);
      setAiDraft(null);
      setSnapshots((current) =>
        current.map((item) =>
          item.id === result.snapshot.id ? result.snapshot : item,
        ),
      );
      setJointSnapshots((current) => [
        result.snapshot,
        ...current.filter((item) => item.id !== result.snapshot.id),
      ]);
      setOperationNotice(
        `静态分析已完成：分析 ${result.analyzedFiles} 个文件，识别 ${result.symbols} 个代码元素。`,
      );
    });
  }

  function generateDocuments() {
    if (!project || !selectedSnapshot) return;
    void runOperation("generating", async (isCurrent) => {
      const result = await knowledgeAnalysisApi.generateDocuments({
        projectId: project.id,
        snapshotId: selectedSnapshot.id,
      });
      if (!isCurrent()) return;
      // 分析阶段已经持久化固定模板报告；这里是幂等补偿入口，空列表表示所有报告
      // 已存在，不能覆盖首次分析返回的总报告数量。
      setGenerationResult(result);
      setGeneratedThisRun(result.generatedDocumentVersionIds.length);
      setOperationNotice(
        result.generatedDocumentVersionIds.length > 0
          ? `项目分析文档生成完成：新增 ${result.generatedDocumentVersionIds.length} 份文档。`
          : "项目分析文档生成完成：固定报告已存在，本次没有新增文档。",
      );
    });
  }

  function createAiDraft() {
    if (!project || !hasValidJointSelection || jointProjectVersionId == null)
      return;
    void runOperation("drafting", async (isCurrent) => {
      const draft = await knowledgeAnalysisApi.createAiDraft({
        projectId: project.id,
        projectVersionId: jointProjectVersionId,
        snapshotIds: selectedJointSnapshotIds,
        providerKey: selectedProviderKey,
      });
      if (!isCurrent()) return;
      setAiDraft(draft);
      setDraftContent(draft.content);
      setEditingDraft(false);
      const version = releases.find(
        (release) => release.id === jointProjectVersionId,
      );
      setDraftTitle(
        `${project.name}${version ? ` ${version.version}` : ""}项目实现分析`,
      );
      setOperationNotice("AI 分析草稿已生成，请复核并编辑后再确认入库。");
    });
  }

  function confirmAiDraft() {
    if (!aiDraft || !draftTitle.trim() || !draftContent.trim()) return;
    void runOperation("confirming", async (isCurrent) => {
      const result = await knowledgeAnalysisApi.confirmAiDraft({
        draftId: aiDraft.id,
        title: draftTitle.trim(),
        content: draftContent,
        versionLabel: `AI 分析 · ${new Date().toISOString().slice(0, 10)}`,
      });
      if (!isCurrent()) return;
      setAiDraft(result.draft);
      setDraftContent(result.draft.content);
      setOperationNotice("AI 分析文档已确认并写入知识库。");
    });
  }

  if (loading) return <Skeleton active className="mt-8 w-full px-6" />;

  if (!project) {
    return (
      <main className="mt-8 w-full px-6">
        <Result
          status="warning"
          title="无法打开源码分析"
          subTitle={error || "没有找到这个项目"}
          extra={
            <Button onClick={() => navigate("/knowledge/projects")}>
              返回项目列表
            </Button>
          }
        />
      </main>
    );
  }

  const isBusy = stage !== "idle";
  const currentOperation = stage === "idle" ? null : operationStageInfo[stage];
  return (
    <main className="w-full px-4 py-6 sm:px-6">
      <Button
        type="link"
        className="!mb-4 !px-0"
        icon={<ArrowLeft size={16} />}
        onClick={() => navigate(`/knowledge/projects/${project.id}/overview`)}
      >
        返回项目
      </Button>
      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <Title level={2} className="!mb-1">
            源码分析
          </Title>
          <Paragraph type="secondary" className="!mb-0">
            选择代码仓库、项目版本和固定 Commit；AI
            分析可联合多个服务仓库，系统不会修改工作区或执行代码。
          </Paragraph>
        </div>
        <Button
          icon={<RefreshCw size={16} />}
          disabled={isBusy}
          onClick={() => void load()}
        >
          刷新
        </Button>
      </div>

      <Alert
        className="mb-4"
        type="info"
        showIcon
        title="本阶段生成静态分析报告"
        description="报告基于代码结构和已捕获的 Commit 生成；生成后可在项目文档中查看和继续维护。"
      />
      {error ? (
        <Alert
          className="mb-4"
          type="error"
          showIcon
          title="无法刷新分析信息"
          description={error}
          action={<Button onClick={() => void load()}>重试</Button>}
        />
      ) : null}
      {operationError ? (
        <Alert
          className="mb-4"
          type="error"
          showIcon
          title="操作未完成"
          description={operationError}
        />
      ) : null}
      {operationNotice ? (
        <Alert
          className="mb-4"
          type="success"
          showIcon
          title="操作已完成"
          description={operationNotice}
        />
      ) : null}

      {!sources.length ? (
        <Empty description="当前项目还没有可分析的代码来源">
          <Button
            type="primary"
            icon={<GitBranch size={16} />}
            onClick={() => navigate(`/knowledge/projects/${project.id}/setup`)}
          >
            管理代码仓库
          </Button>
        </Empty>
      ) : (
        <>
          <Card className="mb-4" title="1. 选择代码与版本">
            <div className="grid gap-4 md:grid-cols-2">
              <label className="block" htmlFor="knowledge-analysis-source">
                <Text strong>代码仓库</Text>
                <Select
                  id="knowledge-analysis-source"
                  className="mt-2 w-full"
                  aria-label="代码仓库"
                  value={selectedSourceId}
                  disabled={isBusy}
                  onChange={(value: number) => {
                    setSelectedSourceId(value);
                    setSelectedSnapshotId(null);
                    setAnalysisResult(null);
                    setGenerationResult(null);
                    setGeneratedThisRun(null);
                    setAiDraft(null);
                    setDraftTitle("");
                    setDraftContent("");
                    setOperationError(null);
                    setOperationNotice(null);
                  }}
                  options={sources.map((source) => ({
                    value: source.source.id,
                    label: sourceLabel(source),
                  }))}
                />
              </label>
              <label className="block" htmlFor="knowledge-analysis-version">
                <Text strong>项目版本（可选）</Text>
                <Select
                  id="knowledge-analysis-version"
                  className="mt-2 w-full"
                  aria-label="项目版本（可选）"
                  allowClear
                  placeholder="不绑定项目版本"
                  value={selectedReleaseId}
                  disabled={isBusy}
                  onChange={(value: number | undefined) => {
                    const releaseId = value ?? null;
                    setSelectedReleaseId(releaseId);
                    setSelectedJointSnapshotIds((current) =>
                      current.filter(
                        (snapshotId) =>
                          jointSnapshots.find(
                            (snapshot) => snapshot.id === snapshotId,
                          )?.releaseId === releaseId,
                      ),
                    );
                    setAiDraft(null);
                    setDraftTitle("");
                    setDraftContent("");
                  }}
                  options={releases.map((release) => ({
                    value: release.id,
                    label: release.version,
                  }))}
                />
              </label>
            </div>
            <label className="mt-4 block" htmlFor="knowledge-analysis-git-ref">
              <Text strong>Git 分支、Tag 或 Commit</Text>
              <Input
                id="knowledge-analysis-git-ref"
                className="mt-2"
                aria-label="Git 分支、Tag 或 Commit"
                value={gitRef}
                disabled={isBusy}
                onChange={(event) => setGitRef(event.target.value)}
                placeholder="例如 main、v1.2.0 或 Commit SHA"
              />
            </label>
            <Button
              className="mt-4"
              type="primary"
              icon={<GitBranch size={16} />}
              loading={stage === "capturing"}
              disabled={
                isBusy ||
                snapshotsLoading ||
                selectedSourceId == null ||
                !gitRef.trim()
              }
              onClick={captureSnapshot}
            >
              捕获只读快照
            </Button>
          </Card>

          <Card className="mb-4" title="2. 分析已捕获的快照">
            <label className="block" htmlFor="knowledge-analysis-snapshot">
              <Text strong>代码快照</Text>
              <Select
                id="knowledge-analysis-snapshot"
                className="mt-2 w-full"
                aria-label="代码快照"
                value={selectedSnapshotId}
                placeholder="请先捕获一个快照"
                disabled={isBusy || !snapshots.length}
                loading={snapshotsLoading}
                onChange={(value: number) => {
                  setSelectedSnapshotId(value);
                  setAnalysisResult(null);
                  setGenerationResult(null);
                  setGeneratedThisRun(null);
                  setAiDraft(null);
                  setDraftTitle("");
                  setDraftContent("");
                  setOperationError(null);
                  setOperationNotice(null);
                }}
                options={snapshots.map((snapshot) => ({
                  value: snapshot.id,
                  label: `${snapshotSourceLabel(snapshot, sources)} · ${snapshot.refName || snapshot.commitSha} · ${snapshot.commitSha.slice(0, 12)} · ${snapshotStatus(snapshot).label}`,
                }))}
              />
            </label>
            {selectedSnapshot ? (
              <Space className="mt-3" wrap>
                <Tag color={snapshotStatus(selectedSnapshot).color}>
                  {snapshotStatus(selectedSnapshot).label}
                </Tag>
                <Text type="secondary" className="font-mono text-xs">
                  {selectedSnapshot.commitSha}
                </Text>
                {selectedSnapshot.releaseId ? (
                  <Tag>已绑定项目版本</Tag>
                ) : (
                  <Tag>未绑定项目版本</Tag>
                )}
              </Space>
            ) : null}
            <Space className="mt-4" wrap>
              <Button
                icon={<ScanSearch size={16} />}
                loading={stage === "analyzing"}
                disabled={isBusy || !selectedSnapshot}
                onClick={analyzeSnapshot}
              >
                运行静态分析
              </Button>
              <Button
                icon={<FileOutput size={16} />}
                loading={stage === "generating"}
                disabled={isBusy || selectedSnapshot?.status !== "analyzed"}
                onClick={generateDocuments}
              >
                生成项目分析文档
              </Button>
            </Space>
          </Card>

          <Card
            className="mb-4"
            title="3. 生成并确认 AI 分析文档"
            extra={<Bot size={18} aria-label="AI 分析" />}
          >
            {jointProjectVersionId == null ? (
              <Alert
                type="info"
                showIcon
                title="先绑定项目版本"
                description="联合 AI 分析必须引用同一个固定项目版本。请在捕获各服务快照时选择项目版本，再完成静态分析。"
              />
            ) : (
              <>
                <Paragraph type="secondary">
                  可联合选择同一项目版本下多个服务的已分析
                  Commit。每个服务的来源和 Commit 会保留在证据引用中；AI
                  草稿可先编辑，确认后才会存入知识库。
                </Paragraph>
                <label
                  className="block"
                  htmlFor="knowledge-analysis-joint-snapshots"
                >
                  <Text strong>联合分析快照</Text>
                  <Select
                    id="knowledge-analysis-joint-snapshots"
                    className="mt-2 w-full"
                    aria-label="联合分析快照"
                    mode="multiple"
                    allowClear
                    maxTagCount="responsive"
                    placeholder="选择一个或多个已分析服务快照"
                    value={selectedJointSnapshotIds}
                    disabled={isBusy || aiDraft?.status === "confirmed"}
                    loading={jointSnapshotsLoading}
                    onChange={(values: number[]) => {
                      setSelectedJointSnapshotIds(values);
                      setAiDraft(null);
                      setDraftTitle("");
                      setDraftContent("");
                    }}
                    options={jointSnapshotOptions}
                  />
                </label>
                {selectedJointSnapshots.length > 1 ? (
                  <Alert
                    className="mt-3"
                    type="info"
                    showIcon
                    title={`将联合分析 ${selectedJointSnapshots.length} 个服务快照`}
                    description="仅会使用所选固定 Commit 的已分析证据；服务之间不会因相同分支名而混淆。"
                  />
                ) : null}
                <label className="block" htmlFor="knowledge-analysis-provider">
                  <Text strong>AI 服务（可选）</Text>
                  <Select
                    id="knowledge-analysis-provider"
                    className="mt-2 w-full"
                    aria-label="AI 服务（可选）"
                    allowClear
                    disabled={isBusy || aiDraft?.status === "confirmed"}
                    value={selectedProviderKey}
                    placeholder="请选择 AI Provider 中配置的聊天模型"
                    onChange={(value: string | undefined) =>
                      setSelectedProviderKey(value)
                    }
                    options={providers.map((provider) => ({
                      value: provider.key,
                      label: `${provider.name} · 聊天模型：${provider.defaultModel}`,
                    }))}
                  />
                </label>
                {!providers.length ? (
                  <Alert
                    className="mt-3"
                    type="warning"
                    showIcon
                    title="尚未配置可用的聊天服务"
                    description="可先继续使用静态分析报告；需要 AI 分析时，请在 AI Provider 中启用并测试一个带默认聊天模型的服务。"
                  />
                ) : null}
                <Button
                  className="mt-4"
                  icon={<Bot size={16} />}
                  loading={stage === "drafting"}
                  disabled={
                    isBusy ||
                    !hasValidJointSelection ||
                    aiDraft?.status === "confirmed" ||
                    !providers.length
                  }
                  onClick={createAiDraft}
                >
                  生成 AI 分析草稿
                </Button>
                {aiDraft ? (
                  <div className="mt-5 space-y-4">
                    <Alert
                      type={aiDraft.status === "confirmed" ? "success" : "info"}
                      showIcon
                      title={
                        aiDraft.status === "confirmed"
                          ? "AI 分析文档已存入知识库"
                          : "请复核 AI 分析草稿"
                      }
                      description={
                        aiDraft.status === "confirmed"
                          ? `已创建文档版本 #${aiDraft.confirmedDocumentVersionId}，可在项目文档中查看。`
                          : `本草稿包含 ${aiDraft.claimRefs.length} 个可验证代码引用。`
                      }
                      action={
                        aiDraft.status === "confirmed" ? (
                          <Button
                            onClick={() =>
                              navigate(
                                `/knowledge/projects/${project.id}/documents`,
                              )
                            }
                          >
                            查看项目文档
                          </Button>
                        ) : undefined
                      }
                    />
                    <label
                      className="block"
                      htmlFor="knowledge-analysis-draft-title"
                    >
                      <Text strong>文档标题</Text>
                      <Input
                        id="knowledge-analysis-draft-title"
                        className="mt-2"
                        value={draftTitle}
                        disabled={isBusy || aiDraft.status === "confirmed"}
                        onChange={(event) => setDraftTitle(event.target.value)}
                      />
                    </label>
                    <section aria-labelledby="knowledge-analysis-draft-content">
                      <Space className="w-full justify-between" wrap>
                        <Text id="knowledge-analysis-draft-content" strong>
                          分析文档
                        </Text>
                        {aiDraft.status !== "confirmed" ? (
                          <Button
                            size="small"
                            onClick={() => setEditingDraft((value) => !value)}
                          >
                            {editingDraft ? "完成编辑" : "编辑草稿"}
                          </Button>
                        ) : null}
                      </Space>
                      {editingDraft ? (
                        <Input.TextArea
                          className="mt-2"
                          aria-label="分析文档 Markdown 源码"
                          autoSize={{ minRows: 12, maxRows: 28 }}
                          value={draftContent}
                          disabled={isBusy || aiDraft.status === "confirmed"}
                          onChange={(event) =>
                            setDraftContent(event.target.value)
                          }
                        />
                      ) : (
                        <div className="mt-2 max-h-[42rem] overflow-auto rounded-lg border border-slate-200 bg-white p-4">
                          <MarkdownPreview
                            content={draftContent}
                            testId="knowledge-analysis-draft-markdown-preview"
                          />
                        </div>
                      )}
                    </section>
                    {aiDraft.status !== "confirmed" ? (
                      <Button
                        type="primary"
                        loading={stage === "confirming"}
                        disabled={
                          isBusy || !draftTitle.trim() || !draftContent.trim()
                        }
                        onClick={confirmAiDraft}
                      >
                        确认存入知识库
                      </Button>
                    ) : null}
                  </div>
                ) : null}
              </>
            )}
          </Card>

          {isBusy ? (
            <Card className="mb-4" title="正在处理">
              {currentOperation ? (
                <div
                  className="mb-4"
                  data-testid="analysis-operation-progress"
                  role="status"
                  aria-live="polite"
                  aria-atomic="true"
                  aria-busy="true"
                >
                  <Space align="center" className="mb-2">
                    <Spin size="small" />
                    <Text>{currentOperation.label}，请稍候…</Text>
                  </Space>
                  <Progress
                    percent={currentOperation.percent}
                    status="active"
                    showInfo={false}
                    aria-label={`${currentOperation.label}阶段进度提示`}
                  />
                  <Text type="secondary" className="text-xs">
                    阶段进度仅用于反馈当前处理状态，完成度以后台返回的实际统计为准。
                  </Text>
                </div>
              ) : null}
              <Steps
                size="small"
                current={
                  stage === "capturing"
                    ? 0
                    : stage === "analyzing"
                      ? 1
                      : stage === "generating"
                        ? 2
                        : stage === "drafting"
                          ? 3
                          : 4
                }
                items={[
                  { title: "捕获 Commit" },
                  { title: "分析代码" },
                  { title: "生成报告" },
                  { title: "生成 AI 草稿" },
                  { title: "确认入库" },
                ]}
              />
            </Card>
          ) : null}

          {analysisResult || generationResult ? (
            <Card title="本次静态分析结果">
              <div className="space-y-2 text-sm">
                {analysisResult ? (
                  <>
                    <div>
                      已分析 {analysisResult.analyzedFiles} 个文件，跳过{" "}
                      {analysisResult.skippedFiles} 个文件
                    </div>
                    <div>
                      识别 {analysisResult.symbols} 个代码元素，生成{" "}
                      {analysisResult.documents} 份文档
                    </div>
                  </>
                ) : null}
                {generationResult ? (
                  <div>
                    <Text data-testid="analysis-generation-result">
                      项目分析文档生成完成：快照包含{" "}
                      {generationResult.fileCount} 个文件、{" "}
                      {generationResult.symbolCount} 个代码元素和{" "}
                      {generationResult.relationCount} 条关系；本次新增{" "}
                      {generationResult.generatedDocumentVersionIds.length}{" "}
                      份文档。
                      {generationResult.generatedDocumentVersionIds.length === 0
                        ? "固定报告已存在，无需重复创建。"
                        : ""}
                    </Text>
                  </div>
                ) : null}
                {generatedThisRun != null ? (
                  <div>
                    <Text data-testid="analysis-generated-this-run">
                      本次补生成 {generatedThisRun}
                      份文档；0 份表示该快照的固定报告已存在。
                    </Text>
                  </div>
                ) : null}
                {analysisResult?.warnings.length ? (
                  <div>提示：{analysisResult.warnings.join("；")}</div>
                ) : null}
              </div>
            </Card>
          ) : null}
        </>
      )}
    </main>
  );
}
