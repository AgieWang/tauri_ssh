import { ConfigProvider } from "antd";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

const knowledgeCatalogApi = vi.hoisted(() => ({ listProjects: vi.fn() }));
const aiProviderApi = vi.hoisted(() => ({ list: vi.fn() }));
const knowledgeApi = vi.hoisted(() => ({
  listEmbeddingProfiles: vi.fn(),
  getLocalEmbeddingRuntimeStatus: vi.fn(),
  testLocalEmbeddingProfile: vi.fn(),
  testRemoteEmbeddingProfile: vi.fn(),
  estimateEmbeddingRebuild: vi.fn(),
  beginEmbeddingProfileRebuild: vi.fn(),
  buildLocalEmbeddingBatch: vi.fn(),
  buildRemoteEmbeddingBatch: vi.fn(),
  validateEmbeddingProfileRebuild: vi.fn(),
  completeEmbeddingProfileRebuild: vi.fn(),
  activateEmbeddingProfileRebuild: vi.fn(),
  retireEmbeddingProfileRebuild: vi.fn(),
  calculateEmbeddingFingerprint: vi.fn(),
  upsertEmbeddingProfile: vi.fn(),
  importLocalEmbeddingModel: vi.fn(),
  downloadLocalEmbeddingModel: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));
vi.mock("@/lib/api/aiProvider", () => ({ aiProviderApi }));
vi.mock("@/lib/api/knowledge", () => ({ knowledgeApi }));
vi.mock("@/lib/api/knowledge-domain", () => ({ knowledgeCatalogApi }));

import ProjectEmbeddingPage from "./ProjectEmbeddingPage";

const project = {
  id: 11,
  projectKey: "customer-platform",
  name: "客户服务平台",
  aliases: [],
  description: "",
  gitWorkspaceKeys: ["gateway"],
  gitWorkspaceKey: "gateway",
  defaultBranch: "main",
  enabled: true,
  createdAt: "2026-08-01T00:00:00Z",
  updatedAt: "2026-08-01T00:00:00Z",
};

const profile = {
  id: 3,
  profileKey: "local-e5",
  name: "推荐本地方案",
  mode: "local" as const,
  providerKey: "",
  model: "multilingual-e5-small-int8",
  modelRevision: "",
  dimension: 384,
  normalized: true,
  config: {},
  fingerprint: "fingerprint",
  status: "draft",
  isActive: false,
  createdAt: "2026-08-01T00:00:00Z",
  updatedAt: "2026-08-01T00:00:00Z",
};

const remoteProvider = {
  key: "deepseek",
  name: "DeepSeek",
  region: "global" as const,
  protocol: "openai_compatible",
  defaultModel: "deepseek-chat",
  embeddingModel: "text-embedding-3-small",
  status: "configured" as const,
  endpoint: "https://provider.example/v1",
  authType: "Bearer API Key",
  apiKeyMasked: "sk-••••",
  hasApiKey: true,
  latencyMs: 80,
  costLevel: "中" as const,
  capabilities: ["chat", "embedding"],
  models: ["deepseek-chat", "text-embedding-3-small"],
  scenarioFit: [],
  fallback: "",
  enabled: true,
  updatedAt: "2026-08-01T00:00:00Z",
};

function renderPage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects/11/embedding"]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/embedding"
            element={<ProjectEmbeddingPage />}
          />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div>项目概览</div>}
          />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

describe("ProjectEmbeddingPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [project],
      total: 1,
      offset: 0,
      limit: 1,
    });
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([profile]);
    aiProviderApi.list.mockResolvedValue([]);
    knowledgeApi.getLocalEmbeddingRuntimeStatus.mockResolvedValue({
      runtime: "fastembed",
      fastembedFeatureEnabled: true,
      runtimeAvailable: true,
      automaticDownloadEnabled: false,
      cacheDir: "/tmp/knowledge-models",
      cachedModels: [
        {
          modelKey: "multilingual-e5-small-int8",
          sizeBytes: 1024,
          sha256: "hash",
          importedAt: "2026-08-01",
        },
      ],
      warnings: [],
    });
    knowledgeApi.testLocalEmbeddingProfile.mockResolvedValue({
      profile,
      dimension: 384,
      probeText: "test",
    });
    knowledgeApi.estimateEmbeddingRebuild.mockResolvedValue({
      targetProfileId: 3,
      targetProfileKey: "local-e5",
      targetMode: "local",
      targetDimension: 384,
      affectedDocuments: 2,
      affectedChunks: 5,
      reusableChunks: 0,
      chunksToEmbed: 5,
      localWorkChunks: 5,
      remoteEligibleChunks: 0,
      remoteCharacters: 0,
      remoteBlockedChunks: 0,
      estimatedIndexBytes: 2048,
      additionalDiskBytes: 1024,
      requiresRemoteConfirmation: false,
      remoteSources: [],
      currentIndex: null,
    });
  });

  afterEach(() => cleanup());

  it("使用独立的线性流程检查方案并展示构建估算", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(
      await screen.findByRole("heading", { name: "配置向量化与索引" }),
    ).toBeVisible();
    expect(screen.getByText("远程模型优先，本地模型随时可切换")).toBeVisible();
    expect(
      screen.getByRole("button", { name: /推荐本地方案 尚未启用/ }),
    ).toBeVisible();

    await user.click(screen.getByRole("button", { name: "检查并估算" }));
    await waitFor(() =>
      expect(knowledgeApi.testLocalEmbeddingProfile).toHaveBeenCalledWith(3),
    );
    expect(await screen.findByText("需要处理")).toBeVisible();
    expect(screen.getByText("5 个内容片段")).toBeVisible();
    expect(screen.getByRole("button", { name: "构建并校验" })).toBeVisible();
  });

  it("不会要求用户重建当前正在使用的索引", async () => {
    const user = userEvent.setup();
    const activeProfile = { ...profile, isActive: true, status: "active" };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([activeProfile]);
    knowledgeApi.testLocalEmbeddingProfile.mockResolvedValue({
      profile: activeProfile,
      dimension: 384,
      probeText: "test",
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "检查并估算" }));
    expect(await screen.findByText("当前方案正在使用中")).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "构建并校验" }),
    ).not.toBeInTheDocument();
  });

  it("默认推荐已配置的远程模型，并允许切换到本地模型", async () => {
    const user = userEvent.setup();
    aiProviderApi.list.mockResolvedValue([remoteProvider]);
    renderPage();

    await user.click(await screen.findByRole("button", { name: "新建方案" }));
    const remoteMode = screen.getByRole("radio", {
      name: "远程模型（推荐）",
    });
    expect(remoteMode).toBeChecked();
    expect(screen.getByText("推荐使用远程模型")).toBeVisible();
    expect(screen.getByText("text-embedding-3-small")).toBeVisible();

    const localMode = screen.getByRole("radio", { name: "本地已安装模型" });
    await user.click(screen.getByText("本地已安装模型", { exact: true }));
    expect(localMode).toBeChecked();
    expect(screen.getByText("本地模型适合离线和隐私场景")).toBeVisible();
    expect(screen.getByText("multilingual-e5-small-int8")).toBeVisible();
  });

  it("远程索引构建必须先确认预计发送的内容范围", async () => {
    const user = userEvent.setup();
    const remoteDraftProfile = {
      ...profile,
      mode: "remote" as const,
      providerKey: "deepseek",
      model: "text-embedding-3-small",
      dimension: 1536,
      status: "draft",
    };
    aiProviderApi.list.mockResolvedValue([remoteProvider]);
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([remoteDraftProfile]);
    knowledgeApi.testRemoteEmbeddingProfile.mockResolvedValue({
      profile: remoteDraftProfile,
      dimension: 1536,
      probeText: "test",
    });
    knowledgeApi.estimateEmbeddingRebuild.mockResolvedValue({
      targetProfileId: 3,
      targetProfileKey: "remote-e5",
      targetMode: "remote",
      targetDimension: 1536,
      affectedDocuments: 1,
      affectedChunks: 2,
      reusableChunks: 0,
      chunksToEmbed: 2,
      localWorkChunks: 0,
      remoteEligibleChunks: 2,
      remoteCharacters: 120,
      remoteBlockedChunks: 0,
      estimatedIndexBytes: 4096,
      additionalDiskBytes: 4096,
      requiresRemoteConfirmation: true,
      remoteSources: [],
      currentIndex: null,
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "启用" }));
    await waitFor(() =>
      expect(knowledgeApi.testRemoteEmbeddingProfile).toHaveBeenCalledWith(3),
    );
    await user.click(await screen.findByRole("button", { name: "构建并校验" }));

    const confirmationDialog = await screen.findByRole("dialog", {
      name: "确认发送已授权内容",
    });
    expect(confirmationDialog).toBeInTheDocument();
    expect(knowledgeApi.buildRemoteEmbeddingBatch).not.toHaveBeenCalled();
    await user.click(
      within(confirmationDialog).getByRole("button", { name: /取\s*消/ }),
    );
    expect(knowledgeApi.buildRemoteEmbeddingBatch).not.toHaveBeenCalled();
  });

  it("保存远程方案时沿用 Provider 引用而不收集密钥", async () => {
    const user = userEvent.setup();
    aiProviderApi.list.mockResolvedValue([remoteProvider]);
    knowledgeApi.calculateEmbeddingFingerprint.mockResolvedValue(
      "remote-fingerprint",
    );
    knowledgeApi.upsertEmbeddingProfile.mockResolvedValue({
      ...profile,
      id: 9,
      profileKey: "remote-test",
      name: "远程语义检索",
      mode: "remote",
      providerKey: "deepseek",
      model: "text-embedding-3-small",
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "新建方案" }));
    await user.click(screen.getByRole("button", { name: "保存并继续" }));

    await waitFor(() =>
      expect(knowledgeApi.upsertEmbeddingProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          mode: "remote",
          providerKey: "deepseek",
          model: "text-embedding-3-small",
          fingerprint: "remote-fingerprint",
        }),
      ),
    );
    const payload = knowledgeApi.upsertEmbeddingProfile.mock.calls[0][0];
    expect(payload).not.toHaveProperty("apiKey");
  });

  it("从方案卡片开始启用流程", async () => {
    const user = userEvent.setup();
    const draftProfile = { ...profile, status: "draft" };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([draftProfile]);
    knowledgeApi.testLocalEmbeddingProfile.mockResolvedValue({
      profile: draftProfile,
      dimension: 384,
      probeText: "test",
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "启用" }));

    await waitFor(() =>
      expect(knowledgeApi.testLocalEmbeddingProfile).toHaveBeenCalledWith(3),
    );
    expect(
      await screen.findByRole("button", { name: "构建并校验" }),
    ).toBeVisible();
  });

  it("允许编辑草稿方案并保留原 Profile 标识", async () => {
    const user = userEvent.setup();
    const draftProfile = { ...profile, status: "draft" };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([draftProfile]);
    knowledgeApi.calculateEmbeddingFingerprint.mockResolvedValue(
      "updated-fingerprint",
    );
    knowledgeApi.upsertEmbeddingProfile.mockResolvedValue({
      ...draftProfile,
      name: "更新后的本地方案",
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "编辑" }));
    expect(
      await screen.findByRole("dialog", { name: "编辑索引方案" }),
    ).toBeVisible();
    const nameInput = screen.getByRole("textbox", { name: "方案名称" });
    await user.clear(nameInput);
    await user.type(nameInput, "更新后的本地方案");
    await user.click(screen.getByRole("button", { name: "保存并继续" }));

    await waitFor(() =>
      expect(knowledgeApi.upsertEmbeddingProfile).toHaveBeenCalledWith(
        expect.objectContaining({
          id: 3,
          profileKey: "local-e5",
          name: "更新后的本地方案",
        }),
      ),
    );
  });

  it("复制已构建方案时拒绝未改变向量指纹的重复配置", async () => {
    const user = userEvent.setup();
    const activeProfile = { ...profile, status: "active", isActive: true };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([activeProfile]);
    knowledgeApi.calculateEmbeddingFingerprint.mockResolvedValue(
      activeProfile.fingerprint,
    );
    renderPage();

    await user.click(await screen.findByRole("button", { name: "编辑" }));
    await user.click(screen.getByRole("button", { name: "保存并继续" }));

    expect(
      await screen.findByText(
        "复制已构建方案时请至少修改模型、维度或前缀配置。",
      ),
    ).toBeVisible();
    expect(knowledgeApi.upsertEmbeddingProfile).not.toHaveBeenCalled();
  });

  it("删除非活动方案前要求确认并从列表移除", async () => {
    const user = userEvent.setup();
    const draftProfile = { ...profile, status: "draft" };
    knowledgeApi.listEmbeddingProfiles.mockResolvedValue([draftProfile]);
    knowledgeApi.retireEmbeddingProfileRebuild.mockResolvedValue({
      profile: { ...draftProfile, status: "retired" },
      validation: {
        profileId: 3,
        profileKey: "local-e5",
        expectedChunks: 0,
        indexedChunks: 0,
        staleChunks: 0,
        dimensionMismatchChunks: 0,
        invalidVectorChunks: 0,
        complete: true,
      },
    });
    renderPage();

    await user.click(await screen.findByRole("button", { name: "删除" }));
    await user.click(await screen.findByRole("button", { name: "删 除" }));

    await waitFor(() =>
      expect(knowledgeApi.retireEmbeddingProfileRebuild).toHaveBeenCalledWith(
        3,
      ),
    );
    expect(
      screen.queryByRole("button", { name: "推荐本地方案 尚未启用" }),
    ).not.toBeInTheDocument();
  });
});
