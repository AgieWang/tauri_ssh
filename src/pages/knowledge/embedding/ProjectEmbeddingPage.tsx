import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  Alert,
  Button,
  Card,
  Collapse,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Progress,
  Radio,
  Select,
  Skeleton,
  Space,
  Steps,
  Tag,
  Typography,
  message,
} from "antd";
import {
  ArrowLeft,
  CheckCircle2,
  CircleGauge,
  DatabaseZap,
  Download,
  HardDriveDownload,
  Pencil,
  Play,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { getErrorMessage } from "@/lib/api";
import { aiProviderApi } from "@/lib/api/aiProvider";
import {
  getAiProviderCapabilityMode,
  providerModeSupportsEmbedding,
} from "@/lib/aiProviderCapabilities";
import { knowledgeApi } from "@/lib/api/knowledge";
import { knowledgeCatalogApi } from "@/lib/api/knowledge-domain";
import type {
  KnowledgeEmbeddingBatchResult,
  KnowledgeEmbeddingProfile,
  KnowledgeEmbeddingRebuildEstimate,
  KnowledgeLocalEmbeddingRuntimeStatus,
  KnowledgeProject,
  AiProvider,
} from "@/types";

const { Paragraph, Text, Title } = Typography;

const DEFAULT_LOCAL_MODEL = "multilingual-e5-small-int8";
const DEFAULT_REMOTE_DIMENSION = 1536;
type EmbeddingMode = "local" | "remote";

type WorkflowStage =
  "checking" | "ready" | "building" | "activate" | "completed";

interface EmbeddingWorkflow {
  profile: KnowledgeEmbeddingProfile;
  stage: WorkflowStage;
  estimate: KnowledgeEmbeddingRebuildEstimate;
  testDimension: number;
  batch?: KnowledgeEmbeddingBatchResult;
}

interface ProfileFormValues {
  mode: EmbeddingMode;
  name: string;
  providerKey: string;
  model: string;
  modelRevision: string;
  dimension: number;
}

interface OfflineModelFormValues {
  modelKey: string;
  sourcePath?: string;
  expectedSha256: string;
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024) {
    return `${(value / 1024 / 1024).toFixed(1)} MB`;
  }
  return `${(value / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function profileStatus(profile: KnowledgeEmbeddingProfile) {
  if (profile.isActive) return { color: "green", label: "当前使用中" };
  if (profile.status === "ready")
    return { color: "blue", label: "已构建，待启用" };
  if (profile.status === "building")
    return { color: "processing", label: "构建中" };
  if (profile.status === "failed")
    return { color: "red", label: "构建失败，可重试" };
  return { color: "default", label: "尚未启用" };
}

function providerEmbeddingModels(provider: AiProvider | undefined) {
  const model = provider?.embeddingModel?.trim() ?? "";
  return model ? [model] : [];
}

function availableRemoteEmbeddingProvider(provider: AiProvider) {
  return (
    provider.enabled &&
    provider.status === "configured" &&
    providerModeSupportsEmbedding(getAiProviderCapabilityMode(provider)) &&
    providerEmbeddingModels(provider).length > 0
  );
}

/**
 * 索引是设备级资源，但入口保留项目上下文，方便用户理解后续搜索仍会按项目和版本过滤。
 * 这里刻意不复用旧工作台的标签页，避免把后台配置和日常知识操作混在一起。
 */
export default function ProjectEmbeddingPage() {
  const navigate = useNavigate();
  const { projectId } = useParams();
  const numericProjectId = Number(projectId);
  const [project, setProject] = useState<KnowledgeProject | null>(null);
  const [profiles, setProfiles] = useState<KnowledgeEmbeddingProfile[]>([]);
  const [aiProviders, setAiProviders] = useState<AiProvider[]>([]);
  const [runtime, setRuntime] =
    useState<KnowledgeLocalEmbeddingRuntimeStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selectedProfileId, setSelectedProfileId] = useState<number | null>(
    null,
  );
  const [workflow, setWorkflow] = useState<EmbeddingWorkflow | null>(null);
  const [busy, setBusy] = useState(false);
  const [profileDrawerOpen, setProfileDrawerOpen] = useState(false);
  const [editingProfile, setEditingProfile] =
    useState<KnowledgeEmbeddingProfile | null>(null);
  const [offlineDrawerOpen, setOfflineDrawerOpen] = useState(false);
  const [remoteConfirmationOpen, setRemoteConfirmationOpen] = useState(false);
  const [profileMode, setProfileMode] = useState<EmbeddingMode>("remote");
  const [profileProviderKey, setProfileProviderKey] = useState("");
  const [profileForm] = Form.useForm<ProfileFormValues>();
  const [offlineModelForm] = Form.useForm<OfflineModelFormValues>();
  const requestId = useRef(0);
  const [messageApi, messageContextHolder] = message.useMessage();

  const selectedProfile = useMemo(
    () => profiles.find((profile) => profile.id === selectedProfileId) ?? null,
    [profiles, selectedProfileId],
  );
  const localModelOptions = useMemo(
    () =>
      Array.from(
        new Set([
          DEFAULT_LOCAL_MODEL,
          ...(runtime?.cachedModels.map((model) => model.modelKey) ?? []),
        ]),
      ).map((model) => ({ label: model, value: model })),
    [runtime?.cachedModels],
  );
  const remoteEmbeddingProviders = useMemo(
    () => aiProviders.filter(availableRemoteEmbeddingProvider),
    [aiProviders],
  );
  const remoteProviderOptions = useMemo(
    () =>
      remoteEmbeddingProviders.map((provider) => ({
        label: `${provider.name}（${provider.key}）`,
        value: provider.key,
      })),
    [remoteEmbeddingProviders],
  );
  const selectedRemoteProvider = useMemo(
    () =>
      remoteEmbeddingProviders.find(
        (provider) => provider.key === profileProviderKey,
      ),
    [profileProviderKey, remoteEmbeddingProviders],
  );
  const remoteModelOptions = useMemo(
    () =>
      providerEmbeddingModels(selectedRemoteProvider).map((model) => ({
        label: model,
        value: model,
      })),
    [selectedRemoteProvider],
  );

  const loadPage = useCallback(async () => {
    const currentRequestId = ++requestId.current;
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId < 1) {
      setLoadError("项目地址无效");
      setLoading(false);
      return;
    }
    setLoading(true);
    setLoadError(null);
    try {
      const [projectResult, nextProfiles, nextRuntime, nextProviders] =
        await Promise.all([
          knowledgeCatalogApi.listProjects({
            projectId: numericProjectId,
            limit: 1,
            offset: 0,
          }),
          knowledgeApi.listEmbeddingProfiles(),
          // 本地缓存状态是辅助信息；缓存目录损坏或不可读时仍允许用户配置远程方案。
          knowledgeApi
            .getLocalEmbeddingRuntimeStatus()
            .catch(() => null as KnowledgeLocalEmbeddingRuntimeStatus | null),
          aiProviderApi.list().catch(() => [] as AiProvider[]),
        ]);
      if (currentRequestId !== requestId.current) return;
      const nextProject = projectResult.items[0] ?? null;
      setProject(nextProject);
      const visibleProfiles = nextProfiles.filter(
        (profile) => profile.status !== "retired",
      );
      setProfiles(visibleProfiles);
      setRuntime(nextRuntime);
      setAiProviders(nextProviders);
      setSelectedProfileId((current) => {
        if (
          current != null &&
          visibleProfiles.some((profile) => profile.id === current)
        ) {
          return current;
        }
        return (
          visibleProfiles.find((profile) => profile.isActive)?.id ??
          visibleProfiles.find((profile) => profile.mode === "remote")?.id ??
          visibleProfiles[0]?.id ??
          null
        );
      });
    } catch (error) {
      if (currentRequestId === requestId.current)
        setLoadError(getErrorMessage(error));
    } finally {
      if (currentRequestId === requestId.current) setLoading(false);
    }
  }, [numericProjectId]);

  useEffect(() => {
    void loadPage();
    return () => {
      requestId.current += 1;
    };
  }, [loadPage]);

  function selectProfile(profile: KnowledgeEmbeddingProfile) {
    if (busy) return;
    setSelectedProfileId(profile.id);
    setWorkflow(null);
  }

  function openProfileDrawer(profile?: KnowledgeEmbeddingProfile) {
    const isEditingDraft = profile?.status === "draft";
    const defaultMode: EmbeddingMode =
      profile?.mode ?? (remoteEmbeddingProviders.length ? "remote" : "local");
    const defaultProvider = remoteEmbeddingProviders[0];
    const providerKey = profile?.providerKey ?? defaultProvider?.key ?? "";
    const defaultModel =
      profile?.model ??
      (defaultProvider
        ? (providerEmbeddingModels(defaultProvider)[0] ?? "")
        : DEFAULT_LOCAL_MODEL);
    setEditingProfile(profile ?? null);
    setProfileMode(defaultMode);
    setProfileProviderKey(providerKey);
    profileForm.setFieldsValue({
      mode: defaultMode,
      name:
        profile && !isEditingDraft
          ? `${profile.name}（副本）`
          : (profile?.name ??
            (defaultMode === "remote" ? "远程语义检索" : "本地语义检索")),
      providerKey,
      model: defaultModel,
      modelRevision: profile?.modelRevision ?? "",
      dimension:
        profile?.dimension ??
        (defaultMode === "remote" ? DEFAULT_REMOTE_DIMENSION : 384),
    });
    setProfileDrawerOpen(true);
  }

  function closeProfileDrawer() {
    if (busy) return;
    setProfileDrawerOpen(false);
    setEditingProfile(null);
  }

  function changeProfileMode(mode: EmbeddingMode) {
    const defaultProvider = remoteEmbeddingProviders[0];
    const nextProviderKey =
      mode === "remote" ? (defaultProvider?.key ?? "") : "";
    const nextModel =
      mode === "remote"
        ? (providerEmbeddingModels(defaultProvider)[0] ?? "")
        : DEFAULT_LOCAL_MODEL;
    setProfileMode(mode);
    setProfileProviderKey(nextProviderKey);
    profileForm.setFieldsValue({
      mode,
      providerKey: nextProviderKey,
      model: nextModel,
      dimension: mode === "remote" ? DEFAULT_REMOTE_DIMENSION : 384,
    });
  }

  function changeRemoteProvider(providerKey: string) {
    const provider = remoteEmbeddingProviders.find(
      (item) => item.key === providerKey,
    );
    const model = providerEmbeddingModels(provider)[0] ?? "";
    setProfileProviderKey(providerKey);
    profileForm.setFieldsValue({ providerKey, model });
  }

  async function saveEmbeddingProfile() {
    try {
      const values = await profileForm.validateFields();
      const provider =
        values.mode === "remote"
          ? remoteEmbeddingProviders.find(
              (item) => item.key === values.providerKey,
            )
          : undefined;
      if (values.mode === "remote" && (!provider || !values.model.trim())) {
        throw new Error("请选择已配置的远程服务商和向量模型。");
      }
      setBusy(true);
      const isEditingDraft = editingProfile?.status === "draft";
      const existingConfig = editingProfile?.config ?? {};
      const config = {
        providerProtocol:
          values.mode === "remote"
            ? provider?.protocol ||
              String(existingConfig.providerProtocol ?? "openai_compatible")
            : "local",
        endpointIdentity:
          values.mode === "remote"
            ? provider?.endpoint ||
              String(existingConfig.endpointIdentity ?? "")
            : "",
        queryPrefix: "query: ",
        documentPrefix: "passage: ",
        chunkStrategyId: "knowledge-structure-v1",
        normalizationVersion: "v1",
      };
      const fingerprint = await knowledgeApi.calculateEmbeddingFingerprint({
        mode: values.mode,
        providerProtocol: config.providerProtocol,
        endpointIdentity: config.endpointIdentity,
        providerKey: values.mode === "remote" ? values.providerKey : "",
        model: values.model,
        modelRevision: values.modelRevision,
        dimension: values.dimension,
        normalized: true,
        queryPrefix: config.queryPrefix,
        documentPrefix: config.documentPrefix,
        chunkStrategyId: config.chunkStrategyId,
        normalizationVersion: config.normalizationVersion,
      });
      if (
        editingProfile &&
        editingProfile.status !== "draft" &&
        fingerprint === editingProfile.fingerprint
      ) {
        throw new Error("复制已构建方案时请至少修改模型、维度或前缀配置。");
      }
      const profile = await knowledgeApi.upsertEmbeddingProfile({
        id: isEditingDraft ? editingProfile?.id : undefined,
        profileKey:
          isEditingDraft && editingProfile
            ? editingProfile.profileKey
            : `${values.mode}-${Date.now().toString(36)}`,
        name: values.name.trim(),
        mode: values.mode,
        providerKey: values.mode === "remote" ? values.providerKey : "",
        model: values.model,
        modelRevision: values.modelRevision,
        dimension: values.dimension,
        normalized: true,
        config,
        fingerprint,
      });
      setProfiles((current) => {
        if (isEditingDraft) {
          return current.map((item) =>
            item.id === profile.id ? profile : item,
          );
        }
        return [profile, ...current];
      });
      setSelectedProfileId(profile.id);
      setWorkflow(null);
      setProfileDrawerOpen(false);
      setEditingProfile(null);
      messageApi.success(
        `${isEditingDraft ? "方案配置已更新" : `${values.mode === "remote" ? "远程" : "本地"}方案已创建`}，下一步请启用索引。`,
      );
    } catch (error) {
      if (error && typeof error === "object" && "errorFields" in error) return;
      messageApi.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function checkAndEstimate(profileToCheck = selectedProfile) {
    if (!profileToCheck) return;
    if (profileToCheck.status === "building") {
      messageApi.info("这个方案正在构建，请等待任务完成或到任务中心恢复。");
      return;
    }
    setBusy(true);
    setWorkflow(null);
    try {
      let checkedProfile = profileToCheck;
      let testDimension = profileToCheck.dimension;
      if (profileToCheck.status === "draft") {
        const test =
          profileToCheck.mode === "remote"
            ? await knowledgeApi.testRemoteEmbeddingProfile(profileToCheck.id)
            : await knowledgeApi.testLocalEmbeddingProfile(profileToCheck.id);
        checkedProfile = test.profile;
        testDimension = test.dimension;
        setProfiles((current) =>
          current.map((profile) =>
            profile.id === checkedProfile.id ? checkedProfile : profile,
          ),
        );
      }
      const estimate = await knowledgeApi.estimateEmbeddingRebuild({
        profileId: profileToCheck.id,
      });
      let stage: WorkflowStage = "ready";
      let readyValidationPassed = false;
      if (profileToCheck.status === "ready") {
        const validation = await knowledgeApi.validateEmbeddingProfileRebuild(
          profileToCheck.id,
        );
        readyValidationPassed = validation.complete;
        stage = readyValidationPassed ? "activate" : "ready";
      }
      setWorkflow({
        profile: checkedProfile,
        stage,
        testDimension,
        estimate,
      });
      messageApi.success(
        profileToCheck.status === "active"
          ? "当前方案正在使用中。"
          : readyValidationPassed
            ? "检查通过，索引已就绪，可以启用。"
            : profileToCheck.status === "ready"
              ? "索引完整性需要重新构建，已生成构建估算。"
              : "检查通过，已生成构建估算。",
      );
    } catch (error) {
      messageApi.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  function enableProfile(profile: KnowledgeEmbeddingProfile) {
    if (busy || profile.isActive || profile.status === "building") return;
    setSelectedProfileId(profile.id);
    setWorkflow(null);
    void checkAndEstimate(profile);
  }

  async function retireProfile(profile: KnowledgeEmbeddingProfile) {
    if (busy || profile.isActive || profile.status === "building") return;
    setBusy(true);
    try {
      await knowledgeApi.retireEmbeddingProfileRebuild(profile.id);
      const nextProfiles = profiles.filter((item) => item.id !== profile.id);
      setProfiles(nextProfiles);
      setSelectedProfileId((current) =>
        current === profile.id
          ? (nextProfiles.find((item) => item.isActive)?.id ??
            nextProfiles[0]?.id ??
            null)
          : current,
      );
      setWorkflow((current) =>
        current?.profile.id === profile.id ? null : current,
      );
      messageApi.success("向量模型配置已删除。");
    } catch (error) {
      messageApi.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function buildAndValidate() {
    if (!workflow) return;
    const { profile, estimate, testDimension } = workflow;
    if (profile.isActive) {
      messageApi.warning("当前使用中的索引不能直接重建，请先新建一个方案。");
      return;
    }
    setBusy(true);
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
          throw new Error("向量构建未推进，请稍后重试或检查本机运行环境。");
        }
        batch = nextBatch;
        previousProcessedChunks = batch.processedChunks;
        setWorkflow({
          profile,
          stage: "building",
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
            `索引校验未通过，且失败收尾未完成：${cleanupFailure}。请刷新状态后重试。`,
          );
        }
        throw new Error("索引校验未通过，方案已标记为失败，可重新构建。");
      }
      const completed = await knowledgeApi.completeEmbeddingProfileRebuild(
        profile.id,
      );
      setWorkflow({
        profile: completed.profile,
        stage: "activate",
        testDimension,
        estimate,
        batch,
      });
      messageApi.success("构建和校验已完成，可以启用新索引。");
    } catch (error) {
      messageApi.error(getErrorMessage(error));
      void loadPage();
      setWorkflow((current) =>
        current && current.profile.id === profile.id
          ? { ...current, stage: "ready" }
          : current,
      );
    } finally {
      setBusy(false);
    }
  }

  function requestBuild() {
    if (!workflow) return;
    if (workflow.estimate.requiresRemoteConfirmation) {
      setRemoteConfirmationOpen(true);
      return;
    }
    void buildAndValidate();
  }

  async function confirmRemoteBuild() {
    setRemoteConfirmationOpen(false);
    await buildAndValidate();
  }

  async function activateIndex() {
    if (!workflow) return;
    setBusy(true);
    try {
      const result = await knowledgeApi.activateEmbeddingProfileRebuild(
        workflow.profile.id,
      );
      if (!result.profile.isActive)
        throw new Error("新索引尚未启用，请刷新后重试。");
      setProfiles((current) =>
        current.map((profile) =>
          profile.id === result.profile.id
            ? result.profile
            : {
                ...profile,
                isActive: false,
                status: profile.status === "active" ? "ready" : profile.status,
              },
        ),
      );
      setWorkflow((current) =>
        current ? { ...current, stage: "completed" } : current,
      );
      messageApi.success("新索引已启用，项目检索会继续按项目和版本过滤。");
    } catch (error) {
      messageApi.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function importOfflineModel() {
    try {
      const values = await offlineModelForm.validateFields();
      setBusy(true);
      await knowledgeApi.importLocalEmbeddingModel({
        modelKey: values.modelKey,
        sourcePath: values.sourcePath?.trim() ?? "",
        expectedSha256: values.expectedSha256.trim(),
      });
      setOfflineDrawerOpen(false);
      messageApi.success("离线模型已校验并加入本机缓存。");
      await loadPage();
    } catch (error) {
      if (error && typeof error === "object" && "errorFields" in error) return;
      messageApi.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function downloadOfflineModel() {
    try {
      const { modelKey } = await offlineModelForm.validateFields(["modelKey"]);
      setBusy(true);
      await knowledgeApi.downloadLocalEmbeddingModel({ modelKey });
      setOfflineDrawerOpen(false);
      messageApi.success("模型已从内部镜像下载并校验。");
      await loadPage();
    } catch (error) {
      if (error && typeof error === "object" && "errorFields" in error) return;
      messageApi.error(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  if (loading) return <Skeleton active className="mt-8 w-full px-6" />;

  if (loadError) {
    return (
      <main className="mt-8 w-full px-6">
        <Alert
          type="error"
          showIcon
          title="无法打开向量化与索引"
          description={loadError}
          action={<Button onClick={() => void loadPage()}>重试</Button>}
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

  const activeStep = workflow
    ? workflow.stage === "ready"
      ? 1
      : workflow.stage === "building"
        ? 2
        : 3
    : 0;
  const hasLocalRuntimeIssue = runtime != null && !runtime.runtimeAvailable;

  return (
    <main className="w-full px-4 py-6 sm:px-6">
      {messageContextHolder}
      <Button
        type="link"
        className="!mb-4 !px-0"
        icon={<ArrowLeft size={16} />}
        onClick={() => navigate(`/knowledge/projects/${project.id}/overview`)}
      >
        返回项目概览
      </Button>

      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <Space size={8} wrap>
            <Title level={2} className="!mb-0">
              配置向量化与索引
            </Title>
            <Tag color="blue">当前设备</Tag>
          </Space>
          <Paragraph type="secondary" className="!mb-0 !mt-2">
            为“{project.name}
            ”选择索引方案。索引构建使用当前设备的共享资源；搜索和问答仍只读取所选项目及版本的内容。
          </Paragraph>
        </div>
        <Button
          icon={<RefreshCw size={16} />}
          loading={loading}
          onClick={() => void loadPage()}
        >
          刷新状态
        </Button>
      </div>

      <Alert
        className="mb-6"
        type="info"
        showIcon
        title="远程模型优先，本地模型随时可切换"
        description="检测到可用远程服务商时，创建方案默认使用远程模型；如果你需要离线处理、降低外发风险或本机已有模型，也可以切换到本地安装模型。"
      />

      <Steps
        className="mb-6"
        current={activeStep}
        items={[
          { title: "选择方案" },
          { title: "检查与估算" },
          { title: "构建并校验" },
          { title: "启用索引" },
        ]}
      />

      <div className="space-y-4">
        <Card
          title={
            <Space>
              <Settings2 size={18} />
              1. 选择方案
            </Space>
          }
          extra={
            <Button
              type="link"
              disabled={busy}
              onClick={() => openProfileDrawer()}
            >
              新建方案
            </Button>
          }
        >
          {!remoteEmbeddingProviders.length ? (
            <Alert
              className="mb-4"
              type="warning"
              showIcon
              title="还没有可用的远程向量模型"
              description="先在 AI Provider 中配置一个已启用、已测试并填写向量模型的服务商；完成后回到这里即可优先使用远程方案。"
              action={
                <Button onClick={() => navigate("/providers")}>
                  配置远程模型
                </Button>
              }
            />
          ) : null}
          {profiles.length ? (
            <div className="grid gap-3 md:grid-cols-2">
              {profiles.map((profile) => {
                const status = profileStatus(profile);
                const selected = profile.id === selectedProfileId;
                return (
                  <div
                    key={profile.id}
                    className={`rounded-xl border transition ${
                      selected
                        ? "border-[var(--primary)] bg-[var(--fill-tertiary)]"
                        : "border-[var(--border)] hover:border-[var(--primary)]"
                    }`}
                  >
                    <button
                      type="button"
                      aria-pressed={selected}
                      aria-label={`${profile.name} ${status.label}`}
                      disabled={busy}
                      className="block w-full rounded-xl p-4 text-left"
                      onClick={() => selectProfile(profile)}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <Text strong>{profile.name}</Text>
                        <Tag color={status.color}>{status.label}</Tag>
                      </div>
                      <Text type="secondary" className="mt-2 block text-sm">
                        {profile.mode === "local" ? "本机处理" : "远程服务"} ·{" "}
                        {profile.model} · {profile.dimension} 维
                      </Text>
                      {profile.mode === "remote" ? (
                        <Text type="warning" className="mt-2 block text-xs">
                          构建前会再次确认可发送的已授权内容。
                        </Text>
                      ) : null}
                    </button>
                    <div className="flex flex-wrap items-center justify-end gap-1 border-t border-[var(--border)] px-3 py-2">
                      {!profile.isActive ? (
                        <Button
                          type="link"
                          size="small"
                          disabled={busy || profile.status === "building"}
                          onClick={() => enableProfile(profile)}
                        >
                          启用
                        </Button>
                      ) : null}
                      <Button
                        type="link"
                        size="small"
                        icon={<Pencil size={14} />}
                        disabled={busy || profile.status === "building"}
                        onClick={() => openProfileDrawer(profile)}
                      >
                        编辑
                      </Button>
                      {profile.isActive || profile.status === "building" ? (
                        <Button
                          type="link"
                          size="small"
                          danger
                          disabled
                          title={
                            profile.isActive
                              ? "活动方案需要先切换到其他方案后才能删除"
                              : "正在构建的方案暂时不能删除"
                          }
                        >
                          删除
                        </Button>
                      ) : (
                        <Popconfirm
                          title="删除这个向量模型配置？"
                          description="删除会清理该方案的向量数据，活动方案不会被删除。"
                          okText="删除"
                          cancelText="取消"
                          onConfirm={() => void retireProfile(profile)}
                        >
                          <Button
                            type="link"
                            size="small"
                            danger
                            icon={<Trash2 size={14} />}
                            disabled={busy}
                          >
                            删除
                          </Button>
                        </Popconfirm>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description="还没有索引方案"
            >
              <Button
                type="primary"
                disabled={busy}
                onClick={() => openProfileDrawer()}
              >
                {remoteEmbeddingProviders.length
                  ? "创建推荐远程方案"
                  : "创建本地方案"}
              </Button>
            </Empty>
          )}
        </Card>

        <Card
          title={
            <Space>
              <CircleGauge size={18} />
              2. 检查本机环境并生成估算
            </Space>
          }
        >
          <Descriptions
            size="small"
            column={{ xs: 1, sm: 2 }}
            items={[
              {
                key: "runtime",
                label: "本机运行时",
                children: runtime?.runtimeAvailable ? "已就绪" : "尚未就绪",
              },
              {
                key: "cache",
                label: "已缓存模型",
                children: `${runtime?.cachedModels.length ?? 0} 个`,
              },
            ]}
          />
          {hasLocalRuntimeIssue ? (
            <Alert
              className="mt-4"
              type="warning"
              showIcon
              title="本机向量运行环境尚未就绪"
              description="可先导入已准备好的离线模型，或从受控内部镜像下载。完成后刷新状态再继续。"
            />
          ) : null}
          {runtime?.warnings.map((warning) => (
            <Alert
              key={warning}
              className="mt-3"
              type="warning"
              showIcon
              title={warning}
            />
          ))}
          <Space className="mt-4" wrap>
            <Button
              type="primary"
              icon={<ShieldCheck size={16} />}
              disabled={!selectedProfile || busy}
              loading={busy && workflow == null}
              onClick={() => void checkAndEstimate()}
            >
              检查并估算
            </Button>
            <Button
              icon={<HardDriveDownload size={16} />}
              disabled={busy}
              onClick={() => setOfflineDrawerOpen(true)}
            >
              准备离线模型
            </Button>
          </Space>
        </Card>

        {workflow ? (
          <Card
            title={
              <Space>
                <DatabaseZap size={18} />
                3. 构建并校验索引
              </Space>
            }
          >
            <Descriptions
              size="small"
              column={{ xs: 1, sm: 2 }}
              items={[
                {
                  key: "profile",
                  label: "当前方案",
                  children: workflow.profile.name,
                },
                {
                  key: "dimension",
                  label: "检查结果",
                  children: `${workflow.testDimension} 维，连接正常`,
                },
                {
                  key: "chunks",
                  label: "需要处理",
                  children: `${workflow.estimate.chunksToEmbed} 个内容片段`,
                },
                {
                  key: "disk",
                  label: "预计额外空间",
                  children: formatBytes(workflow.estimate.additionalDiskBytes),
                },
              ]}
            />
            {workflow.estimate.requiresRemoteConfirmation ? (
              <Alert
                className="mt-4"
                type="warning"
                showIcon
                title="本次构建会使用远程服务"
                description={`仅会处理已授权的内容；预计发送 ${workflow.estimate.remoteCharacters} 个字符。发现未授权片段后，本次批次会立即停止，预计 ${workflow.estimate.remoteBlockedChunks} 个片段不会发送；完成授权后请重新构建。`}
              />
            ) : null}
            {workflow.batch ? (
              <Progress
                className="mt-5"
                percent={
                  workflow.batch.totalChunks
                    ? Math.round(
                        (workflow.batch.processedChunks /
                          workflow.batch.totalChunks) *
                          100,
                      )
                    : 100
                }
                status={workflow.stage === "building" ? "active" : "success"}
                format={() =>
                  `${workflow.batch?.processedChunks}/${workflow.batch?.totalChunks}`
                }
              />
            ) : null}
            {workflow.profile.isActive ? (
              <Alert
                className="mt-4"
                type="info"
                showIcon
                title="当前方案正在使用中"
                description="它可以继续提供检索。若要更新模型或索引，请新建一个本地方案，再构建、校验并启用新索引。"
                action={
                  <Button onClick={() => openProfileDrawer()}>新建方案</Button>
                }
              />
            ) : workflow.stage === "ready" || workflow.stage === "building" ? (
              <Button
                className="mt-4"
                type="primary"
                icon={<Play size={16} />}
                loading={busy}
                onClick={requestBuild}
              >
                构建并校验
              </Button>
            ) : null}
            {workflow.stage === "activate" || workflow.stage === "completed" ? (
              <Alert
                className="mt-4"
                type={workflow.stage === "completed" ? "success" : "info"}
                showIcon
                icon={<CheckCircle2 size={18} />}
                title={
                  workflow.stage === "completed"
                    ? "新索引已启用"
                    : "构建和校验已完成"
                }
                description={
                  workflow.stage === "completed"
                    ? "现在可以返回项目继续搜索和问答。"
                    : "确认启用后，项目检索会开始使用这个新索引。"
                }
                action={
                  workflow.stage === "activate" ? (
                    <Button
                      type="primary"
                      loading={busy}
                      onClick={() => void activateIndex()}
                    >
                      启用新索引
                    </Button>
                  ) : undefined
                }
              />
            ) : null}
          </Card>
        ) : null}

        <Collapse
          items={[
            {
              key: "details",
              label: "了解本机模型与索引范围",
              children: (
                <Space orientation="vertical" className="w-full" size="middle">
                  <Text type="secondary">
                    缓存目录：{runtime?.cacheDir ?? "暂未读取"}
                    。本机模型不会自动下载，避免占用网络和磁盘。
                  </Text>
                  {runtime?.cachedModels.length ? (
                    <div className="space-y-2">
                      {runtime.cachedModels.map((model) => (
                        <div
                          key={model.modelKey}
                          className="flex flex-wrap justify-between gap-2 rounded-lg bg-[var(--fill-tertiary)] px-3 py-2"
                        >
                          <Text>{model.modelKey}</Text>
                          <Text type="secondary">
                            {formatBytes(model.sizeBytes)}
                          </Text>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <Text type="secondary">暂无已校验的本机模型缓存。</Text>
                  )}
                </Space>
              ),
            },
          ]}
        />
      </div>

      <Drawer
        title={
          editingProfile
            ? editingProfile.status === "draft"
              ? "编辑索引方案"
              : "复制并编辑索引方案"
            : "创建索引方案"
        }
        open={profileDrawerOpen}
        size="large"
        mask={{ closable: false }}
        onClose={closeProfileDrawer}
        extra={
          <Button
            type="primary"
            loading={busy}
            onClick={() => void saveEmbeddingProfile()}
          >
            保存并继续
          </Button>
        }
      >
        <Alert
          className="mb-5"
          type="info"
          showIcon
          title={
            profileMode === "remote"
              ? "推荐使用远程模型"
              : "本地模型适合离线和隐私场景"
          }
          description={
            editingProfile && editingProfile.status !== "draft"
              ? "这个方案已经构建过，保存会创建一个新的草稿，不会修改正在使用的向量空间。"
              : profileMode === "remote"
                ? "沿用 AI Provider 中已保存的连接和凭据，不在这里重复填写密钥。保存配置不会立即发送正文或开始构建。"
                : "文档会在这台设备上处理，不需要网络。请先确保模型已经安装并出现在本机缓存中。"
          }
        />
        <Form form={profileForm} layout="vertical">
          <Form.Item name="mode" label="处理方式">
            <Radio.Group
              optionType="button"
              buttonStyle="solid"
              options={[
                {
                  label: "远程模型（推荐）",
                  value: "remote",
                  disabled: !remoteEmbeddingProviders.length,
                },
                { label: "本地已安装模型", value: "local" },
              ]}
              value={profileMode}
              onChange={(event) =>
                changeProfileMode(event.target.value as EmbeddingMode)
              }
            />
          </Form.Item>
          <Form.Item
            name="name"
            label="方案名称"
            rules={[
              { required: true, whitespace: true, message: "请输入方案名称" },
            ]}
          >
            <Input autoFocus placeholder="例如：远程语义检索" />
          </Form.Item>
          {profileMode === "remote" ? (
            <>
              <Form.Item
                name="providerKey"
                label="远程服务商"
                rules={[{ required: true, message: "请选择远程服务商" }]}
              >
                <Select
                  showSearch
                  optionFilterProp="label"
                  options={remoteProviderOptions}
                  onChange={changeRemoteProvider}
                  placeholder="选择已配置的 AI Provider"
                  notFoundContent="暂无可用远程服务商"
                />
              </Form.Item>
              <Alert
                className="mb-4"
                type="warning"
                showIcon
                title="远程模型只使用已授权内容"
                description="构建前会显示预计发送的字符量，并继续执行来源授权和敏感内容检查。"
              />
            </>
          ) : null}
          <Form.Item
            name="model"
            label="向量模型"
            rules={[{ required: true, message: "请选择向量模型" }]}
          >
            <Select
              options={
                profileMode === "remote"
                  ? remoteModelOptions
                  : localModelOptions
              }
              disabled={profileMode === "remote" && !selectedRemoteProvider}
              placeholder={
                profileMode === "remote"
                  ? "先选择远程服务商"
                  : "选择已安装的本地模型"
              }
              notFoundContent={
                profileMode === "remote"
                  ? "该服务商尚未配置向量模型"
                  : "暂无已安装的本地模型"
              }
            />
          </Form.Item>
          <Form.Item name="modelRevision" label="模型修订（可选）">
            <Input placeholder="未填写时按当前模型版本处理" />
          </Form.Item>
          <Form.Item
            name="dimension"
            label={
              profileMode === "remote"
                ? "向量维度（按模型文档填写）"
                : "向量维度"
            }
            rules={[{ required: true, message: "请输入向量维度" }]}
          >
            <InputNumber min={1} precision={0} className="w-full" />
          </Form.Item>
        </Form>
      </Drawer>

      <Drawer
        title="准备离线模型"
        open={offlineDrawerOpen}
        size="large"
        mask={{ closable: false }}
        onClose={() => !busy && setOfflineDrawerOpen(false)}
      >
        <Paragraph type="secondary">
          只有本机环境缺少模型时才需要这一步。可导入已下载的模型文件，或从企业受控内部镜像下载。
        </Paragraph>
        <Form
          form={offlineModelForm}
          layout="vertical"
          initialValues={{ modelKey: DEFAULT_LOCAL_MODEL }}
        >
          <Form.Item
            name="modelKey"
            label="模型标识"
            rules={[
              { required: true, whitespace: true, message: "请输入模型标识" },
            ]}
          >
            <Input />
          </Form.Item>
          <Form.Item name="sourcePath" label="离线模型路径">
            <Input placeholder="选择或填写已准备的离线包路径" />
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
              type="primary"
              icon={<Download size={16} />}
              loading={busy}
              onClick={() => void importOfflineModel()}
            >
              导入并校验
            </Button>
            <Button loading={busy} onClick={() => void downloadOfflineModel()}>
              从内部镜像下载
            </Button>
          </Space>
        </Form>
      </Drawer>

      <Modal
        title="确认发送已授权内容"
        open={remoteConfirmationOpen}
        mask={{ closable: false }}
        confirmLoading={busy}
        okText="确认并开始构建"
        cancelText="取消"
        onCancel={() => !busy && setRemoteConfirmationOpen(false)}
        onOk={() => void confirmRemoteBuild()}
        destroyOnHidden
      >
        <Paragraph>
          本次索引构建会把预计 {workflow?.estimate.remoteCharacters ?? 0}{" "}
          个字符发送到已配置的远程向量服务。
          仅来源级授权且通过敏感内容检查的片段会被发送，未授权片段会被阻断。
        </Paragraph>
        <Alert
          type="warning"
          showIcon
          title="请确认远程处理范围"
          description="确认后才会开始构建；后端仍会在每个批次发送前再次执行来源授权和敏感内容检查。发现未授权片段后，本次批次会停止，该片段不会发送；完成授权后请重试。"
        />
      </Modal>
    </main>
  );
}
