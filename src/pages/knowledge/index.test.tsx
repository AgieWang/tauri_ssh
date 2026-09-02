import { ConfigProvider, Modal } from "antd";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const knowledgeApi = vi.hoisted(() => ({
  getRemoteEmbeddingEnabled: vi.fn(),
  listProjects: vi.fn(),
  listDocuments: vi.fn(),
  listReleases: vi.fn(),
  listSources: vi.fn(),
  listJobs: vi.fn(),
  startSourceSync: vi.fn(),
  listEmbeddingProfiles: vi.fn(),
  calculateEmbeddingFingerprint: vi.fn(),
  upsertEmbeddingProfile: vi.fn(),
  estimateEmbeddingRebuild: vi.fn(),
  testRemoteEmbeddingProfile: vi.fn(),
  testLocalEmbeddingProfile: vi.fn(),
  beginEmbeddingProfileRebuild: vi.fn(),
  buildRemoteEmbeddingBatch: vi.fn(),
  buildLocalEmbeddingBatch: vi.fn(),
  validateEmbeddingProfileRebuild: vi.fn(),
  completeEmbeddingProfileRebuild: vi.fn(),
  activateEmbeddingProfileRebuild: vi.fn(),
  rollbackEmbeddingProfileRebuild: vi.fn(),
  getLocalEmbeddingRuntimeStatus: vi.fn(),
  listZentaoConnections: vi.fn(),
  listZentaoProjectMappings: vi.fn(),
  listCodeSources: vi.fn(),
  listCodeSnapshots: vi.fn(),
  upsertCodeSource: vi.fn(),
  upsertProject: vi.fn(),
  upsertSource: vi.fn(),
  upsertSourcesAtomically: vi.fn(),
  upsertZentaoConnection: vi.fn(),
  previewRagContext: vi.fn(),
  getCitationDetail: vi.fn(),
  getDocumentDetail: vi.fn(),
}));
const gitWorkspaceApi = vi.hoisted(() => ({
  list: vi.fn(),
}));
const aiProviderApi = vi.hoisted(() => ({
  list: vi.fn(),
}));
const getErrorMessage = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  getErrorMessage,
  knowledgeApi,
  gitWorkspaceApi,
  aiProviderApi,
}));

import KnowledgePage, {
  codeSnapshotOptionLabel,
  documentCodeLanguage,
  DocumentContentPreview,
  hasAvailableRemoteEmbeddingProvider,
  isMarkdownPath,
  isMarkdownDocument,
  KnowledgeCodeFilePreview,
  isKnowledgeCodeFileReadable,
  knowledgeCodeFileReasonLabel,
  knowledgeCodeSnapshotStatus,
  isAvailableKnowledgeChatProvider,
  normalizedEmbeddingProfileProviderKey,
  summarizeKnowledgeCodeFiles,
} from "./index";
import { useKnowledgeStore } from "@/store";
import type { KnowledgeDocument, KnowledgeProject } from "@/types";

const EMBEDDING_ACTIVATION_ERROR =
  "向量索引尚未激活，请刷新索引状态后重试；如仍未激活，请重新完成构建和校验。";

function renderPage(initialCatalogTab?: "documents" | "embedding") {
  return render(
    <ConfigProvider>
      <KnowledgePage initialCatalogTab={initialCatalogTab} />
    </ConfigProvider>,
  );
}

function getEmbeddingWorkflowButton(name: string) {
  const labels = screen.getAllByText("目标方案");
  const modal = labels[labels.length - 1]?.closest(".ant-modal");
  if (!(modal instanceof HTMLElement)) {
    throw new Error("未找到向量索引构建向导");
  }
  const button = within(modal)
    .getAllByText(name)
    .map((element) => element.closest("button"))
    .find(
      (element): element is HTMLButtonElement =>
        element instanceof HTMLButtonElement,
    );
  if (!button) {
    throw new Error(`向量索引构建向导中未找到“${name}”按钮`);
  }
  expect(modal).not.toHaveClass("ant-modal-hidden");
  expect(modal.parentElement).not.toHaveAttribute("aria-hidden", "true");
  return button;
}

function findButtonByText(name: string) {
  const button = queryButtonByText(name);
  if (!button) {
    throw new Error(`未找到“${name}”按钮`);
  }
  return button;
}

function queryButtonByText(name: string) {
  return screen
    .getAllByText(name)
    .map((element) => element.closest("button"))
    .find(
      (element): element is HTMLButtonElement =>
        element instanceof HTMLButtonElement,
    );
}

function expectNewEmbeddingActivationError(previousCount: number) {
  const messages = screen.getAllByText(EMBEDDING_ACTIVATION_ERROR);
  expect(messages).toHaveLength(previousCount + 1);
  expect(messages[messages.length - 1]).toBeVisible();
}

async function confirmRemoteEmbeddingBuild() {
  const title = await screen.findByText("确认发送已授权内容", { exact: true });
  const dialog = title.closest(".ant-modal");
  expect(dialog).toBeInstanceOf(HTMLElement);
  fireEvent.click(
    within(dialog as HTMLElement).getByRole("button", {
      name: "确认并开始构建",
    }),
  );
}

describe("KnowledgePage", () => {
  afterEach(() => {
    Modal.destroyAll();
    vi.restoreAllMocks();
    cleanup();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    getErrorMessage.mockImplementation((error: unknown) =>
      error instanceof Error ? "已脱敏错误" : String(error),
    );
    useKnowledgeStore.setState({ projectIds: [], releaseIds: [] });
    knowledgeApi.getRemoteEmbeddingEnabled.mockResolvedValue(true);
    knowledgeApi.listProjects.mockResolvedValue({
      items: [],
      total: 0,
      offset: 0,
      limit: 100,
    });
    knowledgeApi.listDocuments.mockResolvedValue({
      items: [],
      total: 0,
      offset: 0,
      limit: 20,
    });
    knowledgeApi.listReleases.mockResolvedValue([]);
    knowledgeApi.listSources.mockResolvedValue([]);
    knowledgeApi.listJobs.mockResolvedValue([]);
    knowledgeApi.startSourceSync.mockResolvedValue({});
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([]);
    knowledgeApi.calculateEmbeddingFingerprint.mockResolvedValue("fingerprint");
    knowledgeApi.upsertEmbeddingProfile.mockImplementation((input) =>
      Promise.resolve({
        id: 100,
        ...input,
        status: "draft",
        isActive: false,
        createdAt: "2026-08-01T00:00:00Z",
        updatedAt: "2026-08-01T00:00:00Z",
      }),
    );
    knowledgeApi.estimateEmbeddingRebuild.mockResolvedValue({});
    knowledgeApi.testRemoteEmbeddingProfile.mockResolvedValue({});
    knowledgeApi.testLocalEmbeddingProfile.mockResolvedValue({});
    knowledgeApi.beginEmbeddingProfileRebuild.mockResolvedValue({});
    knowledgeApi.buildRemoteEmbeddingBatch.mockResolvedValue({
      completed: true,
    });
    knowledgeApi.buildLocalEmbeddingBatch.mockResolvedValue({
      completed: true,
    });
    knowledgeApi.validateEmbeddingProfileRebuild.mockResolvedValue({
      complete: true,
    });
    knowledgeApi.completeEmbeddingProfileRebuild.mockResolvedValue({});
    knowledgeApi.activateEmbeddingProfileRebuild.mockResolvedValue({});
    knowledgeApi.rollbackEmbeddingProfileRebuild.mockResolvedValue({});
    knowledgeApi.getLocalEmbeddingRuntimeStatus.mockResolvedValue({
      runtime: "not_installed",
      runtimeAvailable: false,
      automaticDownloadEnabled: false,
      cacheDir: "/tmp/knowledge-models",
      cachedModels: [],
      warnings: [],
    });
    knowledgeApi.listZentaoConnections.mockResolvedValue([]);
    knowledgeApi.listZentaoProjectMappings.mockResolvedValue([]);
    knowledgeApi.listCodeSources.mockResolvedValue([]);
    knowledgeApi.listCodeSnapshots.mockResolvedValue([]);
    knowledgeApi.upsertCodeSource.mockResolvedValue({});
    knowledgeApi.upsertProject.mockResolvedValue({});
    knowledgeApi.upsertSource.mockResolvedValue({});
    knowledgeApi.upsertSourcesAtomically.mockResolvedValue([]);
    gitWorkspaceApi.list.mockResolvedValue([]);
    aiProviderApi.list.mockResolvedValue([
      {
        key: "embedding-192-162-11-71",
        name: "内网向量服务",
        region: "china",
        protocol: "OpenAI-compatible",
        defaultModel: "chat-model",
        embeddingModel: "multilingual-e5-small-int8",
        status: "configured",
        endpoint: "http://192.162.11.71:18080/v1",
        authType: "Bearer API Key",
        apiKeyMasked: null,
        hasApiKey: true,
        latencyMs: null,
        costLevel: "企业",
        capabilities: ["embedding"],
        models: ["multilingual-e5-small-int8", "bge-m3"],
        scenarioFit: [],
        fallback: "",
        enabled: true,
        updatedAt: "2026-08-01T00:00:00Z",
      },
    ]);
    knowledgeApi.previewRagContext.mockResolvedValue({
      context: "已检索到可引用证据",
      citations: [],
      conflicts: [],
      evidenceGaps: [],
      retrievalDiagnostics: { channels: {} },
    });
  });

  it("加载目录和集成视图，并优先打开远程默认的向量化方案表单", async () => {
    const user = userEvent.setup();
    renderPage();

    await waitFor(() => {
      expect(knowledgeApi.listProjects).toHaveBeenCalledOnce();
      expect(knowledgeApi.listEmbeddingProfiles).toHaveBeenCalledOnce();
    });

    await user.click(await screen.findByRole("tab", { name: /向量索引/ }));
    await user.click(screen.getByRole("button", { name: "新建向量化方案" }));

    expect(screen.getByLabelText("向量化方案标识")).toHaveValue("");
    expect(screen.getByLabelText("服务商标识")).toBeEnabled();
    expect(screen.getByLabelText("模型")).toBeEnabled();
    await user.click(screen.getByLabelText("模型"));
    expect(
      await screen.findByRole("option", { name: "multilingual-e5-small-int8" }),
    ).toBeInTheDocument();
  });

  it("可从项目工作台直接以向量索引标签打开全局索引配置", async () => {
    renderPage("embedding");

    const embeddingTab = await screen.findByRole("tab", {
      name: /向量索引/,
    });
    expect(embeddingTab).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByText("当前设备的全局本地索引配置")).toBeVisible();
    expect(
      screen.getByText(
        "向量化方案及其构建或重建作用于当前设备的全局本地索引，不限于当前项目。项目问答与检索仍会按所选项目和版本过滤。",
      ),
    ).toBeVisible();
  });

  it("工作台默认开放全部知识能力，不显示发布阶段或本地启停设置", async () => {
    renderPage();

    await screen.findByText("团队知识库");
    expect(screen.getByRole("tab", { name: /向量索引/ })).toBeVisible();
    expect(screen.getByRole("tab", { name: "禅道同步" })).toBeVisible();
    expect(screen.getByRole("tab", { name: "源码知识" })).toBeVisible();
    expect(screen.queryByLabelText("知识库发布阶段")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("启用本地向量化")).not.toBeInTheDocument();
  });

  it("知识问答仅将已配置的对话服务商提供给下拉选择", () => {
    const baseProvider = {
      key: "chat-provider",
      name: "内部对话模型",
      region: "china" as const,
      protocol: "OpenAI-compatible",
      defaultModel: "deepseek-chat",
      embeddingModel: "",
      endpoint: "http://127.0.0.1:8080/v1",
      authType: "Bearer API Key",
      apiKeyMasked: null,
      hasApiKey: true,
      latencyMs: null,
      costLevel: "企业" as const,
      models: ["deepseek-chat"],
      scenarioFit: [],
      fallback: "",
      updatedAt: "2026-08-02T00:00:00Z",
    };

    expect(
      isAvailableKnowledgeChatProvider({
        ...baseProvider,
        capabilities: ["chat"],
        status: "configured",
        enabled: true,
      }),
    ).toBe(true);
    expect(
      isAvailableKnowledgeChatProvider({
        ...baseProvider,
        capabilities: ["embedding"],
        defaultModel: "embedding-default-model",
        status: "configured",
        enabled: true,
      }),
    ).toBe(false);
    expect(
      isAvailableKnowledgeChatProvider({
        ...baseProvider,
        capabilities: ["streaming", "reasoning"],
        status: "configured",
        enabled: true,
      }),
    ).toBe(true);
    expect(
      isAvailableKnowledgeChatProvider({
        ...baseProvider,
        capabilities: ["chat"],
        status: "unconfigured",
        enabled: true,
      }),
    ).toBe(false);
  });

  it("远程向量化方案通过线性向导自动构建并在确认后激活", async () => {
    const profile = {
      id: 71,
      profileKey: "local-e5-71",
      name: "内网服务器 E5",
      mode: "remote",
      providerKey: "embedding-192-162-11-71",
      model: "multilingual-e5-small-int8",
      modelRevision: "main",
      dimension: 384,
      normalized: true,
      config: {},
      fingerprint: "e5-71-fingerprint",
      status: "draft",
      isActive: false,
      createdAt: "2026-08-01T00:00:00Z",
      updatedAt: "2026-08-01T00:00:00Z",
    };
    const validation = {
      profileId: 71,
      profileKey: "local-e5-71",
      expectedChunks: 4,
      indexedChunks: 4,
      staleChunks: 0,
      dimensionMismatchChunks: 0,
      invalidVectorChunks: 0,
      complete: true,
    };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([profile]);
    knowledgeApi.testRemoteEmbeddingProfile.mockResolvedValue({
      profile,
      dimension: 384,
      probeText: "知识库远程向量化短文本探测",
    });
    knowledgeApi.estimateEmbeddingRebuild.mockResolvedValue({
      targetProfileKey: "local-e5-71",
      chunksToEmbed: 4,
      localWorkChunks: 0,
      remoteCharacters: 512,
      additionalDiskBytes: 6144,
      remoteBlockedChunks: 0,
      requiresRemoteConfirmation: true,
    });
    knowledgeApi.buildRemoteEmbeddingBatch
      .mockResolvedValueOnce({
        profileId: 71,
        jobKey: "knowledge-embedding-71",
        totalChunks: 4,
        processedChunks: 2,
        embeddedChunks: 2,
        skippedChunks: 0,
        blockedChunks: 0,
        completed: false,
        checkpoint: {},
      })
      .mockResolvedValueOnce({
        profileId: 71,
        jobKey: "knowledge-embedding-71",
        totalChunks: 4,
        processedChunks: 4,
        embeddedChunks: 4,
        skippedChunks: 0,
        blockedChunks: 0,
        completed: true,
        checkpoint: {},
      });
    knowledgeApi.validateEmbeddingProfileRebuild.mockResolvedValue(validation);
    knowledgeApi.completeEmbeddingProfileRebuild.mockResolvedValue({
      profile: { ...profile, status: "ready" },
      validation,
    });
    knowledgeApi.activateEmbeddingProfileRebuild.mockResolvedValue({
      profile: { ...profile, status: "active", isActive: true },
      validation,
    });
    renderPage();

    fireEvent.click(await screen.findByRole("tab", { name: /向量索引/ }));
    expect(await screen.findByText("内网服务器 E5")).toBeVisible();
    const startButton = findButtonByText("开始线性构建");
    expect(startButton).toBeVisible();
    expect(startButton).toBeEnabled();
    fireEvent.click(startButton);

    await waitFor(() => {
      expect(knowledgeApi.testRemoteEmbeddingProfile).toHaveBeenCalledWith(71);
      expect(knowledgeApi.estimateEmbeddingRebuild).toHaveBeenCalledWith({
        profileId: 71,
      });
    });
    expect(startButton).toBeDisabled();
    const buildButton = await waitFor(() => {
      const button = getEmbeddingWorkflowButton("开始自动构建");
      expect(button).toBeInTheDocument();
      expect(button).toBeEnabled();
      return button;
    });
    expect(queryButtonByText("模型测试")).toBeUndefined();
    expect(queryButtonByText("重建估算")).toBeUndefined();

    fireEvent.click(buildButton);
    await confirmRemoteEmbeddingBuild();
    await waitFor(() => {
      expect(knowledgeApi.beginEmbeddingProfileRebuild).toHaveBeenCalledWith(
        71,
      );
      expect(knowledgeApi.buildRemoteEmbeddingBatch).toHaveBeenNthCalledWith(
        1,
        {
          profileId: 71,
          jobKey: undefined,
        },
      );
      expect(knowledgeApi.buildRemoteEmbeddingBatch).toHaveBeenNthCalledWith(
        2,
        {
          profileId: 71,
          jobKey: "knowledge-embedding-71",
        },
      );
      expect(knowledgeApi.validateEmbeddingProfileRebuild).toHaveBeenCalledWith(
        71,
      );
      expect(knowledgeApi.completeEmbeddingProfileRebuild).toHaveBeenCalledWith(
        71,
      );
    });
    const activateButton = getEmbeddingWorkflowButton("激活新的索引");
    expect(activateButton).toBeInTheDocument();
    expect(activateButton).toBeEnabled();
    fireEvent.click(activateButton);
    await waitFor(() => {
      expect(knowledgeApi.activateEmbeddingProfileRebuild).toHaveBeenCalledWith(
        71,
      );
    });
    const workflowCallOrder: Array<[string, number]> = [
      [
        "模型测试",
        Number(
          knowledgeApi.testRemoteEmbeddingProfile.mock.invocationCallOrder[0],
        ),
      ],
      [
        "重建估算",
        Number(
          knowledgeApi.estimateEmbeddingRebuild.mock.invocationCallOrder[0],
        ),
      ],
      [
        "开始重建",
        Number(
          knowledgeApi.beginEmbeddingProfileRebuild.mock.invocationCallOrder[0],
        ),
      ],
      [
        "首批构建",
        Number(
          knowledgeApi.buildRemoteEmbeddingBatch.mock.invocationCallOrder[0],
        ),
      ],
      [
        "末批构建",
        Number(
          knowledgeApi.buildRemoteEmbeddingBatch.mock.invocationCallOrder[1],
        ),
      ],
      [
        "完整性校验",
        Number(
          knowledgeApi.validateEmbeddingProfileRebuild.mock
            .invocationCallOrder[0],
        ),
      ],
      [
        "完成重建",
        Number(
          knowledgeApi.completeEmbeddingProfileRebuild.mock
            .invocationCallOrder[0],
        ),
      ],
      [
        "激活索引",
        Number(
          knowledgeApi.activateEmbeddingProfileRebuild.mock
            .invocationCallOrder[0],
        ),
      ],
    ];
    expect(workflowCallOrder).toEqual(
      [...workflowCallOrder].sort(([, left], [, right]) => left - right),
    );
    const completionMessages = screen.getAllByText("新索引已激活");
    expect(
      completionMessages[completionMessages.length - 1],
    ).toBeInTheDocument();
  });

  it("激活接口未返回活动索引时不显示成功并提示用户", async () => {
    const profile = {
      id: 72,
      profileKey: "local-e5-72",
      name: "未激活的内网服务器 E5",
      mode: "remote",
      providerKey: "embedding-192-162-11-71",
      model: "multilingual-e5-small-int8",
      modelRevision: "main",
      dimension: 384,
      normalized: true,
      config: {},
      fingerprint: "e5-72-fingerprint",
      status: "draft",
      isActive: false,
      createdAt: "2026-08-01T00:00:00Z",
      updatedAt: "2026-08-01T00:00:00Z",
    };
    const validation = {
      profileId: 72,
      profileKey: "local-e5-72",
      expectedChunks: 1,
      indexedChunks: 1,
      staleChunks: 0,
      dimensionMismatchChunks: 0,
      invalidVectorChunks: 0,
      complete: true,
    };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([profile]);
    knowledgeApi.testRemoteEmbeddingProfile.mockResolvedValue({
      profile,
      dimension: 384,
      probeText: "知识库远程向量化短文本探测",
    });
    knowledgeApi.estimateEmbeddingRebuild.mockResolvedValue({
      targetProfileKey: profile.profileKey,
      chunksToEmbed: 1,
      localWorkChunks: 0,
      remoteCharacters: 128,
      additionalDiskBytes: 1536,
      remoteBlockedChunks: 0,
      requiresRemoteConfirmation: true,
    });
    knowledgeApi.buildRemoteEmbeddingBatch.mockResolvedValue({
      profileId: 72,
      jobKey: "knowledge-embedding-72",
      totalChunks: 1,
      processedChunks: 1,
      embeddedChunks: 1,
      skippedChunks: 0,
      blockedChunks: 0,
      completed: true,
      checkpoint: {},
    });
    knowledgeApi.validateEmbeddingProfileRebuild.mockResolvedValue(validation);
    knowledgeApi.completeEmbeddingProfileRebuild.mockResolvedValue({
      profile: { ...profile, status: "ready" },
      validation,
    });
    knowledgeApi.activateEmbeddingProfileRebuild.mockResolvedValue({
      profile: { ...profile, status: "ready", isActive: false },
      validation,
    });
    getErrorMessage.mockImplementation((error: unknown) =>
      error instanceof Error ? error.message : String(error),
    );
    renderPage();

    fireEvent.click(await screen.findByRole("tab", { name: /向量索引/ }));
    expect(await screen.findByText(profile.name)).toBeVisible();
    const startButton = findButtonByText("开始线性构建");
    expect(startButton).toBeEnabled();
    fireEvent.click(startButton);

    const buildButton = await waitFor(() => {
      const button = getEmbeddingWorkflowButton("开始自动构建");
      expect(button).toBeEnabled();
      return button;
    });
    fireEvent.click(buildButton);
    await confirmRemoteEmbeddingBuild();
    const activateButton = await waitFor(() => {
      const button = getEmbeddingWorkflowButton("激活新的索引");
      expect(button).toBeEnabled();
      return button;
    });
    const activationErrorCount = screen.queryAllByText(
      EMBEDDING_ACTIVATION_ERROR,
    ).length;
    fireEvent.click(activateButton);

    await waitFor(() => {
      expect(knowledgeApi.activateEmbeddingProfileRebuild).toHaveBeenCalledWith(
        72,
      );
      expectNewEmbeddingActivationError(activationErrorCount);
    });
    expect(screen.queryByText("新索引已激活")).not.toBeInTheDocument();
  });

  it("激活接口缺失方案结果时不显示成功且解除构建忙碌状态", async () => {
    const profile = {
      id: 73,
      profileKey: "local-e5-73",
      name: "缺失激活结果的内网服务器 E5",
      mode: "remote",
      providerKey: "embedding-192-162-11-71",
      model: "multilingual-e5-small-int8",
      modelRevision: "main",
      dimension: 384,
      normalized: true,
      config: {},
      fingerprint: "e5-73-fingerprint",
      status: "draft",
      isActive: false,
      createdAt: "2026-08-01T00:00:00Z",
      updatedAt: "2026-08-01T00:00:00Z",
    };
    const validation = {
      profileId: 73,
      profileKey: "local-e5-73",
      expectedChunks: 1,
      indexedChunks: 1,
      staleChunks: 0,
      dimensionMismatchChunks: 0,
      invalidVectorChunks: 0,
      complete: true,
    };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([profile]);
    knowledgeApi.testRemoteEmbeddingProfile.mockResolvedValue({
      profile,
      dimension: 384,
      probeText: "知识库远程向量化短文本探测",
    });
    knowledgeApi.estimateEmbeddingRebuild.mockResolvedValue({
      targetProfileKey: profile.profileKey,
      chunksToEmbed: 1,
      localWorkChunks: 0,
      remoteCharacters: 128,
      additionalDiskBytes: 1536,
      remoteBlockedChunks: 0,
      requiresRemoteConfirmation: true,
    });
    knowledgeApi.buildRemoteEmbeddingBatch.mockResolvedValue({
      profileId: 73,
      jobKey: "knowledge-embedding-73",
      totalChunks: 1,
      processedChunks: 1,
      embeddedChunks: 1,
      skippedChunks: 0,
      blockedChunks: 0,
      completed: true,
      checkpoint: {},
    });
    knowledgeApi.validateEmbeddingProfileRebuild.mockResolvedValue(validation);
    knowledgeApi.completeEmbeddingProfileRebuild.mockResolvedValue({
      profile: { ...profile, status: "ready" },
      validation,
    });
    let resolveActivation: ((result: undefined) => void) | undefined;
    knowledgeApi.activateEmbeddingProfileRebuild.mockImplementation(
      () =>
        new Promise<undefined>((resolve) => {
          resolveActivation = resolve;
        }),
    );
    getErrorMessage.mockImplementation((error: unknown) =>
      error instanceof Error ? error.message : String(error),
    );
    renderPage();

    fireEvent.click(await screen.findByRole("tab", { name: /向量索引/ }));
    expect(await screen.findByText(profile.name)).toBeVisible();
    const startButton = findButtonByText("开始线性构建");
    expect(startButton).toBeEnabled();
    fireEvent.click(startButton);

    const buildButton = await waitFor(() => {
      const button = getEmbeddingWorkflowButton("开始自动构建");
      expect(button).toBeEnabled();
      return button;
    });
    fireEvent.click(buildButton);
    await confirmRemoteEmbeddingBuild();
    const activateButton = await waitFor(() => {
      const button = getEmbeddingWorkflowButton("激活新的索引");
      expect(button).toBeEnabled();
      return button;
    });
    const activationErrorCount = screen.queryAllByText(
      EMBEDDING_ACTIVATION_ERROR,
    ).length;
    fireEvent.click(activateButton);

    const pendingActivateButton = await waitFor(() => {
      expect(knowledgeApi.activateEmbeddingProfileRebuild).toHaveBeenCalledWith(
        73,
      );
      const button = getEmbeddingWorkflowButton("激活新的索引");
      expect(button).toHaveClass("ant-btn-loading");
      expect(button).toBeDisabled();
      return button;
    });
    fireEvent.click(pendingActivateButton);
    expect(knowledgeApi.activateEmbeddingProfileRebuild).toHaveBeenCalledTimes(
      1,
    );
    resolveActivation?.(undefined);
    await waitFor(() => {
      expectNewEmbeddingActivationError(activationErrorCount);
      expect(getEmbeddingWorkflowButton("激活新的索引")).toBeEnabled();
    });
    expect(screen.queryByText("新索引已激活")).not.toBeInTheDocument();
  });

  it("激活接口仅返回校验结果时不显示成功并提示用户", async () => {
    const profile = {
      id: 74,
      profileKey: "local-e5-74",
      name: "缺失方案字段的内网服务器 E5",
      mode: "remote",
      providerKey: "embedding-192-162-11-71",
      model: "multilingual-e5-small-int8",
      modelRevision: "main",
      dimension: 384,
      normalized: true,
      config: {},
      fingerprint: "e5-74-fingerprint",
      status: "draft",
      isActive: false,
      createdAt: "2026-08-01T00:00:00Z",
      updatedAt: "2026-08-01T00:00:00Z",
    };
    const validation = {
      profileId: 74,
      profileKey: "local-e5-74",
      expectedChunks: 1,
      indexedChunks: 1,
      staleChunks: 0,
      dimensionMismatchChunks: 0,
      invalidVectorChunks: 0,
      complete: true,
    };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([profile]);
    knowledgeApi.testRemoteEmbeddingProfile.mockResolvedValue({
      profile,
      dimension: 384,
      probeText: "知识库远程向量化短文本探测",
    });
    knowledgeApi.estimateEmbeddingRebuild.mockResolvedValue({
      targetProfileKey: profile.profileKey,
      chunksToEmbed: 1,
      localWorkChunks: 0,
      remoteCharacters: 128,
      additionalDiskBytes: 1536,
      remoteBlockedChunks: 0,
      requiresRemoteConfirmation: true,
    });
    knowledgeApi.buildRemoteEmbeddingBatch.mockResolvedValue({
      profileId: 74,
      jobKey: "knowledge-embedding-74",
      totalChunks: 1,
      processedChunks: 1,
      embeddedChunks: 1,
      skippedChunks: 0,
      blockedChunks: 0,
      completed: true,
      checkpoint: {},
    });
    knowledgeApi.validateEmbeddingProfileRebuild.mockResolvedValue(validation);
    knowledgeApi.completeEmbeddingProfileRebuild.mockResolvedValue({
      profile: { ...profile, status: "ready" },
      validation,
    });
    knowledgeApi.activateEmbeddingProfileRebuild.mockResolvedValue({
      validation,
    });
    getErrorMessage.mockImplementation((error: unknown) =>
      error instanceof Error ? error.message : String(error),
    );
    renderPage();

    fireEvent.click(await screen.findByRole("tab", { name: /向量索引/ }));
    expect(await screen.findByText(profile.name)).toBeVisible();
    const startButton = findButtonByText("开始线性构建");
    expect(startButton).toBeEnabled();
    fireEvent.click(startButton);

    const buildButton = await waitFor(() => {
      const button = getEmbeddingWorkflowButton("开始自动构建");
      expect(button).toBeEnabled();
      return button;
    });
    fireEvent.click(buildButton);
    await confirmRemoteEmbeddingBuild();
    const activateButton = await waitFor(() => {
      const button = getEmbeddingWorkflowButton("激活新的索引");
      expect(button).toBeEnabled();
      return button;
    });
    const activationErrorCount = screen.queryAllByText(
      EMBEDDING_ACTIVATION_ERROR,
    ).length;
    fireEvent.click(activateButton);

    await waitFor(() => {
      expect(knowledgeApi.activateEmbeddingProfileRebuild).toHaveBeenCalledWith(
        74,
      );
      expectNewEmbeddingActivationError(activationErrorCount);
      expect(getEmbeddingWorkflowButton("激活新的索引")).toBeEnabled();
    });
    expect(screen.queryByText("新索引已激活")).not.toBeInTheDocument();
  });

  it("远程向量化默认可用且不展示全局开关", async () => {
    renderPage();

    await screen.findByText("团队知识库");
    expect(
      screen.queryByRole("switch", { name: "启用远程向量化" }),
    ).not.toBeInTheDocument();
    expect(knowledgeApi.getRemoteEmbeddingEnabled).not.toHaveBeenCalled();
  });

  it("知识项目可搜索并多选已加载的 Git 工作区", async () => {
    const user = userEvent.setup();
    gitWorkspaceApi.list.mockResolvedValue([
      {
        id: 1,
        workspaceKey: "fj-workorder",
        name: "企业业务工单中心",
        repoPath: "/workspace/fj-workorder",
        branch: "main",
      },
      {
        id: 2,
        workspaceKey: "tauri-ssh",
        name: "Tauri SSH",
        repoPath: "/workspace/tauri-ssh",
        branch: "master",
      },
    ]);
    renderPage();

    await screen.findByText("团队知识库");
    await user.click(screen.getByRole("tab", { name: "项目与版本" }));
    await user.click(screen.getByRole("button", { name: "新建项目" }));

    const dialog = screen.getByRole("dialog");
    const workspaceSelect = within(dialog).getByLabelText("Git 工作区标识");
    await user.click(workspaceSelect);
    await user.click(await screen.findByText(/企业业务工单中心/));
    await user.click(workspaceSelect);
    await user.click(await screen.findByText(/Tauri SSH/));

    fireEvent.change(within(dialog).getByLabelText("项目标识"), {
      target: { value: "knowledge-project" },
    });
    fireEvent.change(within(dialog).getByLabelText("项目名称"), {
      target: { value: "知识项目" },
    });
    await user.click(within(dialog).getByRole("button", { name: "OK" }));

    expect(gitWorkspaceApi.list).toHaveBeenCalledWith({});
    await waitFor(() => {
      expect(knowledgeApi.upsertProject).toHaveBeenCalledWith(
        expect.objectContaining({
          gitWorkspaceKeys: ["fj-workorder", "tauri-ssh"],
          gitWorkspaceKey: "fj-workorder",
        }),
      );
    });
  });

  it("源码知识来源从已加载的 Git 工作区下拉选择标识", async () => {
    const user = userEvent.setup();
    gitWorkspaceApi.list.mockResolvedValue([
      {
        id: 1,
        workspaceKey: "fj-workorder",
        name: "企业业务工单中心",
        repoPath: "/workspace/fj-workorder",
        branch: "main",
      },
    ]);
    renderPage();

    await screen.findByText("团队知识库");
    await user.click(screen.getByRole("tab", { name: "源码知识" }));
    await user.click(screen.getByRole("button", { name: "添加源码来源" }));
    const dialog = screen.getByRole("dialog");
    const workspaceSelect = within(dialog).getByLabelText("Git 工作区标识");

    await user.click(workspaceSelect);
    await user.click(await screen.findByText(/企业业务工单中心/));

    expect(gitWorkspaceApi.list).toHaveBeenCalledWith({});
    expect(
      within(dialog).getByTitle(
        "企业业务工单中心（fj-workorder · /workspace/fj-workorder）",
      ),
    ).toBeInTheDocument();

    await user.type(within(dialog).getByLabelText("来源标识"), "fj-workorder");
    await user.type(
      within(dialog).getByLabelText("显示名称"),
      "企业业务工单主服务",
    );
    await user.click(within(dialog).getByRole("button", { name: "OK" }));

    await waitFor(() => {
      expect(knowledgeApi.upsertCodeSource).toHaveBeenCalledWith(
        expect.objectContaining({
          source: expect.objectContaining({
            sourceType: "git_workspace",
            gitWorkspaceKey: "fj-workorder",
          }),
        }),
      );
    });
  });

  it("源码知识来源以可搜索多选框配置需要解析的编程语言", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("团队知识库");
    await user.click(screen.getByRole("tab", { name: "源码知识" }));
    await user.click(screen.getByRole("button", { name: "添加源码来源" }));
    const dialog = screen.getByRole("dialog");
    const languageSelect = within(dialog).getByLabelText("需要解析的编程语言");

    expect(
      within(dialog).queryByText("P0 语言（每行）"),
    ).not.toBeInTheDocument();
    await user.click(languageSelect);
    expect(
      await screen.findByRole("option", { name: "Rust" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: "TypeScript" }),
    ).toBeInTheDocument();
    await user.type(languageSelect, "SQL");
    expect(
      await screen.findByRole("option", { name: "SQL" }),
    ).toBeInTheDocument();
  });

  it("源码知识来源保存所选的编程语言", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("团队知识库");
    await user.click(screen.getByRole("tab", { name: "源码知识" }));
    await user.click(screen.getByRole("button", { name: "添加源码来源" }));
    const dialog = screen.getByRole("dialog");
    const languageSelect = within(dialog).getByLabelText("需要解析的编程语言");

    await user.click(languageSelect);
    await user.type(within(dialog).getByLabelText("来源标识"), "demo-source");
    await user.type(within(dialog).getByLabelText("显示名称"), "示例源码");
    await user.click(within(dialog).getByRole("button", { name: "OK" }));

    await waitFor(() => {
      expect(knowledgeApi.upsertCodeSource).toHaveBeenCalledWith(
        expect.objectContaining({
          allowRemoteProcessing: true,
          allowedLanguages: [
            "rust",
            "typescript",
            "javascript",
            "vue",
            "java",
            "sql",
            "markdown",
          ],
          source: expect.objectContaining({
            sourceType: "local_directory",
            versionStrategy: "unversioned",
            syncMode: "manual",
            allowRemoteEmbedding: false,
          }),
        }),
      );
    });
  });

  it("文档目录初始展开项目层级，目录层级仍保持折叠", async () => {
    const project: KnowledgeProject = {
      id: 9,
      projectKey: "order-center",
      name: "订单中心",
      aliases: [],
      description: "",
      gitWorkspaceKeys: [],
      gitWorkspaceKey: "",
      defaultBranch: "main",
      enabled: true,
      createdAt: "2026-08-01 10:00:00",
      updatedAt: "2026-08-01 10:00:00",
      deletedAt: null,
    };
    const document: KnowledgeDocument = {
      id: 31,
      documentKey: "order-refund-requirement",
      projectId: project.id,
      sourceId: 5,
      docType: "markdown",
      title: "退款审批需求",
      logicalPath: "需求说明/退款/refund.md",
      status: "active",
      sensitivity: "internal",
      tags: [],
      latestVersionId: 42,
      allowAi: true,
      allowMcp: true,
      createdAt: "2026-08-01 10:00:00",
      updatedAt: "2026-08-01 10:00:00",
      deletedAt: null,
    };
    knowledgeApi.listProjects.mockResolvedValue({
      items: [project],
      total: 1,
      offset: 0,
      limit: 100,
    });
    knowledgeApi.listDocuments.mockResolvedValue({
      items: [document],
      total: 1,
      offset: 0,
      limit: 100,
    });
    renderPage();

    expect(await screen.findByText("订单中心")).toBeVisible();
    expect(screen.getByRole("treeitem", { name: /订单中心/ })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getByText("需求说明")).toBeVisible();
    expect(screen.getByRole("treeitem", { name: /需求说明/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(screen.queryByText("退款")).not.toBeInTheDocument();
  });

  it("Markdown 文档会解析标题、列表、表格和代码块", () => {
    render(
      <DocumentContentPreview
        document={{
          docType: "markdown",
          logicalPath: "需求/退款.md",
          title: "退款需求",
        }}
        version={{
          sourcePath: "需求/退款.md",
          mimeType: "text/markdown",
          content:
            "# 退款需求\n\n- 支持原路退款\n\n| 阶段 | 状态 |\n| --- | --- |\n| 审批 | 已完成 |\n\n```java\nreturn approved;\n```",
        }}
      />,
    );

    expect(screen.getByTestId("knowledge-markdown-preview")).toBeVisible();
    expect(screen.getByRole("heading", { name: "退款需求" })).toBeVisible();
    expect(screen.getByText("支持原路退款")).toBeVisible();
    expect(screen.getByRole("table")).toBeVisible();
    expect(screen.getByText("return approved;")).toBeVisible();
  });

  it("代码及配置文件按路径识别高亮语言，Markdown 不进入代码预览", () => {
    expect(documentCodeLanguage("pom.xml")).toBe("xml");
    expect(documentCodeLanguage("src/main/App.tsx")).toBe("typescript");
    expect(documentCodeLanguage("README.txt")).toBe("plain");
    expect(isMarkdownPath("docs/设计说明.md")).toBe(true);
    expect(isMarkdownPath("docs/设计说明.markdown")).toBe(true);
    expect(isMarkdownPath("docs/设计说明.mdown")).toBe(true);
    expect(isMarkdownPath("docs/设计说明.mkdn")).toBe(true);
    expect(isMarkdownPath("docs/组件.mdx")).toBe(true);
    expect(isMarkdownPath("src/main/App.tsx")).toBe(false);
    expect(
      isMarkdownDocument(
        { docType: "markdown", logicalPath: "README.md", title: "README" },
        { sourcePath: "README.md", mimeType: "text/markdown" },
      ),
    ).toBe(true);
    expect(
      isMarkdownDocument(
        { docType: "source_code", logicalPath: "pom.xml", title: "pom.xml" },
        { sourcePath: "pom.xml", mimeType: "application/xml" },
      ),
    ).toBe(false);
    expect(knowledgeCodeSnapshotStatus("analyzed")).toMatchObject({
      label: "已分析",
      color: "green",
    });
    expect(knowledgeCodeSnapshotStatus("failed").label).toBe("分析失败");
  });

  it("源码快照标签包含代码来源，避免多仓库 HEAD 混淆", () => {
    const label = codeSnapshotOptionLabel(
      {
        sourceId: 8,
        refName: "HEAD",
        snapshotKey: "git:order-app:sha",
        commitSha: "5e22387a5f53ce67dbd73edb8b99f489aec70178",
        capturedAt: "2026-08-02T09:00:00Z",
      },
      [
        {
          source: {
            id: 8,
            displayName: "工单前端",
            sourceKey: "fj-workorder-app",
          },
        },
      ],
    );

    expect(label).toBe("工单前端（fj-workorder-app） · HEAD · 5e22387a5f53");
  });

  it("源码快照文件按路径渲染 Markdown 或代码高亮", () => {
    const baseFile = {
      id: 1,
      snapshotId: 2,
      documentVersionId: 3,
      language: "java",
      fileSize: 10,
      contentHash: "hash",
      analysisLevel: "text_only" as const,
      isGenerated: false,
      isTest: false,
      sensitivity: "internal",
      status: "active",
      skipReason: "",
      createdAt: "2026-08-02T00:00:00Z",
    };
    const { rerender } = render(
      <KnowledgeCodeFilePreview
        file={{ ...baseFile, relativePath: "docs/设计说明.md" }}
        content={"# 设计说明\n\n支持 Markdown 展示"}
      />,
    );
    expect(
      screen.getByTestId("knowledge-code-file-markdown-preview"),
    ).toBeVisible();
    expect(screen.getByRole("heading", { name: "设计说明" })).toBeVisible();

    rerender(
      <KnowledgeCodeFilePreview
        file={{ ...baseFile, relativePath: "src/Demo.java" }}
        content="public class Demo {}"
      />,
    );
    expect(screen.getByTestId("knowledge-code-file-preview")).toHaveAttribute(
      "data-language",
      "java",
    );
  });

  it("源码文件统计区分可读、脱敏和安全跳过文件", () => {
    const baseFile = {
      id: 1,
      snapshotId: 6,
      documentVersionId: 10,
      relativePath: "src/index.vue",
      language: "vue",
      fileSize: 100,
      contentHash: "hash",
      analysisLevel: "structured_fallback" as const,
      isGenerated: false,
      isTest: false,
      sensitivity: "internal",
      status: "active",
      skipReason: "",
      createdAt: "2026-08-12T00:00:00Z",
    };
    const files = [
      baseFile,
      {
        ...baseFile,
        id: 2,
        relativePath: "src/redacted.ts",
        skipReason:
          "redacted_sensitive_content:credential_or_connection_string",
      },
      {
        ...baseFile,
        id: 3,
        documentVersionId: null,
        relativePath: "src/private-key.ts",
        analysisLevel: "skipped" as const,
        sensitivity: "restricted",
        status: "skipped",
        skipReason: "sensitive_content:private_key",
      },
    ];

    expect(summarizeKnowledgeCodeFiles(files)).toEqual({
      totalFiles: 3,
      readableFiles: 2,
      redactedFiles: 1,
      skippedFiles: 1,
    });
    expect(isKnowledgeCodeFileReadable(files[2])).toBe(false);
    expect(knowledgeCodeFileReasonLabel(files[2].skipReason)).toBe(
      "包含私钥，已安全阻断",
    );
  });

  it("文档目录会继续分页加载超过单页上限的文件", async () => {
    const documents: KnowledgeDocument[] = Array.from(
      { length: 501 },
      (_, index) => ({
        id: index + 1,
        documentKey: `document-${index + 1}`,
        projectId: null,
        sourceId: null,
        docType: "markdown",
        title: `文档 ${index + 1}`,
        logicalPath: `目录/document-${index + 1}.md`,
        status: "active",
        sensitivity: "internal",
        tags: [],
        latestVersionId: null,
        allowAi: true,
        allowMcp: true,
        createdAt: "2026-08-01 10:00:00",
        updatedAt: "2026-08-01 10:00:00",
        deletedAt: null,
      }),
    );
    knowledgeApi.listDocuments.mockImplementation(
      ({ offset = 0 }: { offset?: number }) =>
        Promise.resolve({
          items: documents.slice(offset, offset + 500),
          total: documents.length,
          offset,
          limit: 500,
        }),
    );
    renderPage();

    await waitFor(() => {
      expect(knowledgeApi.listDocuments).toHaveBeenCalledWith(
        expect.objectContaining({ limit: 500, offset: 0 }),
      );
      expect(knowledgeApi.listDocuments).toHaveBeenCalledWith(
        expect.objectContaining({ limit: 500, offset: 500 }),
      );
    });
    expect(screen.getByText("未归属项目")).toBeVisible();
    expect(screen.queryByText("document-501.md")).not.toBeInTheDocument();
  });

  it("在加载失败时只展示脱敏错误，不能把底层异常正文带到页面", async () => {
    knowledgeApi.listProjects.mockRejectedValue(
      new Error("token=secret-value"),
    );
    renderPage();

    await waitFor(() => {
      expect(screen.getByText("已脱敏错误")).toBeVisible();
    });
    expect(screen.queryByText(/secret-value/)).not.toBeInTheDocument();
  });

  it("禅道与源码页可切换，且源码范围保持只读提示", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("团队知识库");

    await user.click(screen.getByRole("tab", { name: "禅道同步" }));
    expect(screen.getByText("先探测能力，再选择实体同步")).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "源码知识" }));
    expect(screen.getByText("源码捕获始终只读")).toBeVisible();

    expect(
      screen.getByText(/不会 checkout、stash、reset 或执行任何被分析代码/),
    ).toBeVisible();
  });

  it("HTTP 禅道地址展示明文风险，未显式授权时拒绝保存", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("团队知识库");
    await user.click(screen.getByRole("tab", { name: "禅道同步" }));
    await user.click(screen.getByRole("button", { name: "新建连接" }));
    const dialog = screen.getByRole("dialog");

    fireEvent.change(within(dialog).getByLabelText("连接标识"), {
      target: { value: "zentao-http" },
    });
    fireEvent.change(within(dialog).getByLabelText("名称"), {
      target: { value: "内网禅道" },
    });
    fireEvent.change(within(dialog).getByLabelText("安全凭据引用"), {
      target: { value: "zentao-readonly-ref" },
    });

    fireEvent.change(within(dialog).getByLabelText("禅道地址"), {
      target: { value: "http://192.162.11.133:9090/zentao/" },
    });

    await waitFor(() => {
      expect(within(dialog).getByLabelText("校验证书")).toBeDisabled();
    });

    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    expect(
      await screen.findByText("HTTP 连接必须显式允许内网 HTTP，并关闭证书校验"),
    ).toBeVisible();
    expect(knowledgeApi.upsertZentaoConnection).not.toHaveBeenCalled();
  });

  it("禅道连接仅提供 API Token 认证并始终要求安全凭据引用", async () => {
    const user = userEvent.setup();
    knowledgeApi.upsertZentaoConnection.mockResolvedValue({});
    renderPage();

    await screen.findByText("团队知识库");
    await user.click(screen.getByRole("tab", { name: "禅道同步" }));
    await user.click(screen.getByRole("button", { name: "新建连接" }));
    const dialog = screen.getByRole("dialog");

    expect(screen.queryByText("账号密码会话")).not.toBeInTheDocument();
    expect(within(dialog).getByLabelText("安全凭据引用")).toBeInTheDocument();
  });

  it("远程 Profile 从已配置服务商选择模型，并展示授权警告", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("团队知识库");

    await user.click(screen.getByRole("tab", { name: /向量索引/ }));
    await user.click(screen.getByRole("button", { name: "新建向量化方案" }));
    await user.click(screen.getByLabelText("模式"));
    await user.click(screen.getByRole("option", { name: "远程（需授权）" }));

    const providerSelect = screen.getByLabelText("服务商标识");
    await user.click(providerSelect);
    await user.click(
      await screen.findByRole("option", {
        name: "内网向量服务（embedding-192-162-11-71）",
      }),
    );

    await user.click(screen.getByLabelText("模型"));
    expect(
      await screen.findByRole("option", { name: "multilingual-e5-small-int8" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: "bge-m3" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("远程模式必须经过来源级授权与敏感内容检查"),
    ).toBeInTheDocument();
  });

  it("保存向量化方案时拒绝失效服务商，并在本地模式清空服务商标识", () => {
    expect(hasAvailableRemoteEmbeddingProvider("retired-provider", [])).toBe(
      false,
    );
    expect(
      normalizedEmbeddingProfileProviderKey("local", "retired-provider"),
    ).toBe("");
    expect(
      normalizedEmbeddingProfileProviderKey(
        "remote",
        " embedding-192-162-11-71 ",
      ),
    ).toBe("embedding-192-162-11-71");
  });

  it("已构建方案可以直接进入启用流程而不重复探测模型", async () => {
    const profile = {
      id: 75,
      profileKey: "ready-embedding-75",
      name: "已构建远程方案",
      mode: "remote",
      providerKey: "embedding-192-162-11-71",
      model: "multilingual-e5-small-int8",
      modelRevision: "main",
      dimension: 384,
      normalized: true,
      config: {},
      fingerprint: "ready-embedding-75-fingerprint",
      status: "ready",
      isActive: false,
      createdAt: "2026-08-01T00:00:00Z",
      updatedAt: "2026-08-01T00:00:00Z",
    };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([profile]);
    knowledgeApi.estimateEmbeddingRebuild.mockResolvedValue({
      targetProfileKey: profile.profileKey,
      chunksToEmbed: 0,
      targetDimension: profile.dimension,
    });
    renderPage();

    fireEvent.click(await screen.findByRole("tab", { name: /向量索引/ }));
    fireEvent.click(findButtonByText("开始线性构建"));

    await waitFor(() => {
      expect(knowledgeApi.estimateEmbeddingRebuild).toHaveBeenCalledWith({
        profileId: profile.id,
      });
    });
    expect(knowledgeApi.testRemoteEmbeddingProfile).not.toHaveBeenCalled();
  });

  it("引用只能经后端详情打开原文", async () => {
    knowledgeApi.previewRagContext.mockResolvedValue({
      context: "已检索到可引用证据",
      citations: [
        {
          citationKey: "citation-7",
          sourceType: "document",
          documentId: 7,
          documentVersionId: 11,
          chunkId: 17,
          title: "退款审批需求",
          logicalPath: "requirements/refund.md",
          headingPath: "范围",
          commitSha: "abc123",
          externalKey: "REQ-1042",
          symbolKey: "",
          excerpt: "退款审批事实",
        },
      ],
      conflicts: [],
      evidenceGaps: [],
      retrievalDiagnostics: { channels: {} },
    });
    knowledgeApi.getCitationDetail.mockResolvedValue({ document: { id: 7 } });
    knowledgeApi.getDocumentDetail.mockResolvedValue({
      document: {
        id: 7,
        logicalPath: "requirements/refund.md",
        sensitivity: "internal",
      },
      versions: [],
    });
    renderPage();

    await screen.findByText("团队知识库");

    fireEvent.change(screen.getByPlaceholderText(/我想了解某某项目/), {
      target: { value: "退款审批需求" },
    });
    fireEvent.click(screen.getByRole("button", { name: "预览证据上下文" }));
    await waitFor(() => {
      expect(
        screen.getByText(/退款审批需求（requirements\/refund\.md/),
      ).toBeVisible();
    });
    fireEvent.click(screen.getByText(/退款审批需求（requirements\/refund\.md/));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "打开原文" })).toBeVisible();
    });
    fireEvent.click(screen.getByRole("button", { name: "打开原文" }));
    expect(knowledgeApi.getCitationDetail).toHaveBeenCalledWith(17);
    await waitFor(() => {
      expect(knowledgeApi.getDocumentDetail).toHaveBeenCalledWith(7);
    });
  });
});
