import { ConfigProvider } from "antd";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listReleases: vi.fn(),
}));
const knowledgeAnalysisApi = vi.hoisted(() => ({
  upsertCodeSource: vi.fn(),
  listCodeSources: vi.fn(),
  listCodeSnapshots: vi.fn(),
  captureGitSnapshot: vi.fn(),
  analyzeSnapshot: vi.fn(),
  generateDocuments: vi.fn(),
  createAiDraft: vi.fn(),
  confirmAiDraft: vi.fn(),
}));
const aiProviderApi = vi.hoisted(() => ({ list: vi.fn() }));

vi.mock("@/lib/api", () => ({
  aiProviderApi,
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));
vi.mock("@/lib/api/knowledge-domain", () => ({
  knowledgeCatalogApi,
  knowledgeAnalysisApi,
}));

import ProjectAnalysisPage from "./ProjectAnalysisPage";

const project = { id: 11, name: "订单中心" };
const source = {
  source: {
    id: 31,
    projectId: 11,
    displayName: "订单服务",
    gitWorkspaceKey: "orders",
  },
  settings: {
    sourceId: 31,
    allowedLanguages: ["rust"],
    includeUntracked: false,
    maxFileSizeBytes: 1024,
    allowRemoteProcessing: false,
  },
};
const snapshot = {
  id: 41,
  sourceId: 31,
  projectId: 11,
  releaseId: 21,
  refName: "main",
  commitSha: "a".repeat(40),
  status: "captured",
};

function renderPage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects/11/analysis"]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/analysis"
            element={<ProjectAnalysisPage />}
          />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div>项目概览</div>}
          />
          <Route
            path="/knowledge/projects/:projectId/setup"
            element={<div>项目设置</div>}
          />
          <Route path="/knowledge/projects" element={<div>项目列表</div>} />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

describe("ProjectAnalysisPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    knowledgeCatalogApi.listProjects.mockResolvedValue({ items: [project] });
    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 21, version: "v1.0.0" },
    ]);
    knowledgeAnalysisApi.listCodeSources.mockResolvedValue([source]);
    knowledgeAnalysisApi.listCodeSnapshots.mockResolvedValue([snapshot]);
    knowledgeAnalysisApi.captureGitSnapshot.mockResolvedValue(snapshot);
    knowledgeAnalysisApi.analyzeSnapshot.mockResolvedValue({
      snapshot: { ...snapshot, status: "analyzed" },
      analyzedFiles: 8,
      skippedFiles: 1,
      symbols: 12,
      documents: 2,
      warnings: [],
    });
    knowledgeAnalysisApi.generateDocuments.mockResolvedValue({
      snapshotId: 41,
      sourceId: 31,
      generatedDocumentVersionIds: [],
      fileCount: 8,
      symbolCount: 12,
      relationCount: 4,
    });
    aiProviderApi.list.mockResolvedValue([]);
    knowledgeAnalysisApi.createAiDraft.mockResolvedValue({
      id: 91,
      analysisRunId: 81,
      projectId: 11,
      projectVersionId: 21,
      snapshotIds: [41],
      providerKey: "local-chat",
      model: "model-v1",
      templateKey: "project-implementation-analysis-v1",
      content: "# 项目分析\n\n订单服务提供订单能力。 [code:41:file:201]",
      claimRefs: ["code:41:file:201"],
      status: "draft",
      confirmedDocumentVersionId: null,
    });
  });

  afterEach(() => cleanup());

  it("按固定顺序捕获、分析并生成静态报告，且不将其标注为 AI 草稿", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("订单服务")).toBeInTheDocument();
    await waitFor(() =>
      expect(knowledgeAnalysisApi.listCodeSnapshots).toHaveBeenCalledWith({
        projectId: 11,
        sourceId: 31,
      }),
    );
    await user.click(screen.getByRole("button", { name: "捕获只读快照" }));
    await waitFor(() =>
      expect(knowledgeAnalysisApi.captureGitSnapshot).toHaveBeenCalledWith({
        projectId: 11,
        sourceId: 31,
        gitRef: "HEAD",
        projectVersionId: null,
      }),
    );

    await user.click(screen.getByRole("button", { name: "运行静态分析" }));
    await waitFor(() =>
      expect(knowledgeAnalysisApi.analyzeSnapshot).toHaveBeenCalledWith({
        projectId: 11,
        snapshotId: 41,
      }),
    );
    expect(await screen.findByText(/已分析 8 个文件/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "生成项目分析文档" }));
    await waitFor(() =>
      expect(knowledgeAnalysisApi.generateDocuments).toHaveBeenCalledWith({
        projectId: 11,
        snapshotId: 41,
      }),
    );
    expect(
      await screen.findByTestId("analysis-generated-this-run"),
    ).toHaveTextContent(/本次补生成 0\s*份文档/);
    expect(
      await screen.findByTestId("analysis-generation-result"),
    ).toHaveTextContent(/快照包含 8 个文件/);
  });

  it("已分析快照直接生成项目文档后会显示完成结果", async () => {
    const user = userEvent.setup();
    knowledgeAnalysisApi.listCodeSnapshots.mockResolvedValue([
      { ...snapshot, status: "analyzed" },
    ]);
    knowledgeAnalysisApi.generateDocuments.mockResolvedValue({
      snapshotId: 41,
      sourceId: 31,
      generatedDocumentVersionIds: [501],
      fileCount: 58,
      symbolCount: 227,
      relationCount: 68,
    });

    renderPage();
    await screen.findByText("订单服务");
    await user.click(screen.getByRole("button", { name: "生成项目分析文档" }));

    expect(
      await screen.findByTestId("analysis-generation-result"),
    ).toHaveTextContent(/新增\s*1\s*份文档/);
    expect(
      await screen.findByText("项目分析文档生成完成：新增 1 份文档。"),
    ).toBeInTheDocument();
  });

  it("可联合选择同一项目版本的多个服务快照生成一份 AI 草稿", async () => {
    const user = userEvent.setup();
    const inventorySource = {
      ...source,
      source: {
        ...source.source,
        id: 32,
        displayName: "库存服务",
        gitWorkspaceKey: "inventory",
      },
      settings: { ...source.settings, sourceId: 32 },
    };
    const analyzedOrderSnapshot = { ...snapshot, status: "analyzed" };
    const analyzedInventorySnapshot = {
      ...snapshot,
      id: 42,
      sourceId: 32,
      refName: "release/1.0",
      commitSha: "b".repeat(40),
      status: "analyzed",
    };
    knowledgeAnalysisApi.listCodeSources.mockResolvedValue([
      source,
      inventorySource,
    ]);
    knowledgeAnalysisApi.listCodeSnapshots.mockImplementation(
      ({ sourceId }: { sourceId: number | null }) =>
        Promise.resolve(
          sourceId == null
            ? [analyzedOrderSnapshot, analyzedInventorySnapshot]
            : sourceId === 31
              ? [analyzedOrderSnapshot]
              : [analyzedInventorySnapshot],
        ),
    );
    aiProviderApi.list.mockResolvedValue([
      {
        key: "local-chat",
        name: "本地聊天服务",
        defaultModel: "model-v1",
        capabilities: ["chat"],
        enabled: true,
        status: "configured",
      },
    ]);

    renderPage();
    const jointSelect = await screen.findByLabelText("联合分析快照");
    await user.click(jointSelect);
    await user.click(await screen.findByText(/库存服务 · release\/1\.0/));
    expect(
      await screen.findByText("将联合分析 2 个服务快照"),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "生成 AI 分析草稿" }));
    await waitFor(() =>
      expect(knowledgeAnalysisApi.createAiDraft).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: 21,
        snapshotIds: [41, 42],
        providerKey: "local-chat",
      }),
    );
  });

  it("静态分析运行期间显示阶段进度并禁止重复点击", async () => {
    const user = userEvent.setup();
    let releaseAnalysis: (() => void) | undefined;
    const analysisPending = new Promise<void>((resolve) => {
      releaseAnalysis = resolve;
    });
    knowledgeAnalysisApi.analyzeSnapshot.mockImplementation(async () => {
      await analysisPending;
      return {
        snapshot: { ...snapshot, status: "analyzed" },
        analyzedFiles: 8,
        skippedFiles: 1,
        symbols: 12,
        documents: 2,
        warnings: [],
      };
    });

    renderPage();
    await screen.findByText("订单服务");
    const analyzeButton = screen.getByRole("button", {
      name: "运行静态分析",
    });
    await user.click(analyzeButton);

    expect(
      await screen.findByTestId("analysis-operation-progress"),
    ).toHaveTextContent("正在运行静态分析");
    expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");
    expect(analyzeButton).toBeDisabled();

    releaseAnalysis?.();
    await waitFor(() =>
      expect(
        screen.queryByTestId("analysis-operation-progress"),
      ).not.toBeInTheDocument(),
    );
  });

  it("分析完成会保留快于全项目列表响应的新联合候选快照", async () => {
    const user = userEvent.setup();
    let resolveJointSnapshots!: (snapshots: (typeof snapshot)[]) => void;
    const delayedJointSnapshots = new Promise<(typeof snapshot)[]>(
      (resolve) => {
        resolveJointSnapshots = resolve;
      },
    );
    knowledgeAnalysisApi.listCodeSnapshots.mockImplementation(
      ({ sourceId }: { sourceId: number | null }) =>
        sourceId == null ? delayedJointSnapshots : Promise.resolve([snapshot]),
    );
    knowledgeAnalysisApi.analyzeSnapshot.mockResolvedValue({
      snapshot: { ...snapshot, status: "analyzed" },
      analyzedFiles: 8,
      skippedFiles: 1,
      symbols: 12,
      documents: 2,
      warnings: [],
    });

    renderPage();
    await screen.findByText("订单服务");
    await user.click(screen.getByLabelText("项目版本（可选）"));
    await user.click(await screen.findByText("v1.0.0"));
    await user.click(screen.getByRole("button", { name: "运行静态分析" }));
    await screen.findByText(/已分析 8 个文件/);
    resolveJointSnapshots([]);

    const jointSelect = await screen.findByLabelText("联合分析快照");
    await user.click(jointSelect);
    expect(
      await screen.findByRole("option", { name: /订单服务 · main/ }),
    ).toBeInTheDocument();
  });

  it("生成项目文档期间显示阶段进度并禁止重复点击", async () => {
    const user = userEvent.setup();
    let releaseGeneration: (() => void) | undefined;
    const generationPending = new Promise<void>((resolve) => {
      releaseGeneration = resolve;
    });
    knowledgeAnalysisApi.listCodeSnapshots.mockResolvedValue([
      { ...snapshot, status: "analyzed" },
    ]);
    knowledgeAnalysisApi.generateDocuments.mockImplementation(async () => {
      await generationPending;
      return {
        snapshotId: 41,
        sourceId: 31,
        generatedDocumentVersionIds: [],
        fileCount: 8,
        symbolCount: 12,
        relationCount: 4,
      };
    });

    renderPage();
    await screen.findByText("订单服务");
    const generateButton = screen.getByRole("button", {
      name: "生成项目分析文档",
    });
    await user.click(generateButton);

    expect(
      await screen.findByTestId("analysis-operation-progress"),
    ).toHaveTextContent("正在生成项目分析文档");
    expect(screen.getByRole("status")).toHaveAttribute("aria-live", "polite");
    expect(generateButton).toBeDisabled();

    releaseGeneration?.();
    await waitFor(() =>
      expect(
        screen.queryByTestId("analysis-operation-progress"),
      ).not.toBeInTheDocument(),
    );
  });

  it("为绑定版本的已分析快照生成 Markdown 预览草稿", async () => {
    const user = userEvent.setup();
    knowledgeAnalysisApi.listCodeSnapshots.mockResolvedValue([
      { ...snapshot, status: "analyzed" },
    ]);
    aiProviderApi.list.mockResolvedValue([
      {
        key: "local-chat",
        name: "本地聊天服务",
        defaultModel: "model-v1",
        capabilities: ["chat"],
        enabled: true,
        status: "configured",
      },
    ]);
    renderPage();

    await screen.findByText("订单服务");
    await user.click(screen.getByRole("button", { name: "生成 AI 分析草稿" }));

    await waitFor(() =>
      expect(knowledgeAnalysisApi.createAiDraft).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: 21,
        snapshotIds: [41],
        providerKey: "local-chat",
      }),
    );
    expect(await screen.findByText("请复核 AI 分析草稿")).toBeInTheDocument();
    expect(
      screen.getByTestId("knowledge-analysis-draft-markdown-preview"),
    ).toHaveTextContent("项目分析");
    expect(
      screen.queryByRole("textbox", { name: "分析文档 Markdown 源码" }),
    ).toBeNull();

    await user.click(screen.getByRole("button", { name: "编辑草稿" }));
    expect(
      screen.getByRole("textbox", { name: "分析文档 Markdown 源码" }),
    ).toHaveValue("# 项目分析\n\n订单服务提供订单能力。 [code:41:file:201]");
  });

  it("将有默认聊天模型但缺少显式 chat 能力的 Provider 加入下拉框", async () => {
    knowledgeAnalysisApi.listCodeSnapshots.mockResolvedValue([
      { ...snapshot, status: "analyzed" },
    ]);
    aiProviderApi.list.mockResolvedValue([
      {
        key: "deepseek",
        name: "DeepSeek",
        defaultModel: "deepseek-v4-flash",
        capabilities: ["streaming", "openai_compatible"],
        enabled: true,
        status: "configured",
      },
    ]);

    renderPage();

    expect(
      await screen.findByText("DeepSeek · 聊天模型：deepseek-v4-flash"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "生成 AI 分析草稿" }),
    ).toBeEnabled();
  });

  it("切换代码来源时会清理旧的 AI 草稿，避免跨快照确认", async () => {
    const user = userEvent.setup();
    const inventorySource = {
      ...source,
      source: {
        ...source.source,
        id: 32,
        displayName: "库存服务",
        gitWorkspaceKey: "inventory",
      },
      settings: { ...source.settings, sourceId: 32 },
    };
    knowledgeAnalysisApi.listCodeSources.mockResolvedValue([
      source,
      inventorySource,
    ]);
    knowledgeAnalysisApi.listCodeSnapshots.mockImplementation(
      ({ sourceId }: { sourceId: number }) =>
        Promise.resolve([
          {
            ...snapshot,
            id: sourceId === 31 ? 41 : 42,
            sourceId,
            status: "analyzed",
          },
        ]),
    );
    aiProviderApi.list.mockResolvedValue([
      {
        key: "local-chat",
        name: "本地聊天服务",
        defaultModel: "model-v1",
        capabilities: ["chat"],
        enabled: true,
        status: "configured",
      },
    ]);

    renderPage();
    await screen.findByText("订单服务");
    await user.click(screen.getByRole("button", { name: "生成 AI 分析草稿" }));
    expect(await screen.findByText("请复核 AI 分析草稿")).toBeInTheDocument();

    await user.click(screen.getByLabelText("代码仓库"));
    await user.click(await screen.findByText("库存服务"));
    expect(screen.queryByText("请复核 AI 分析草稿")).not.toBeInTheDocument();
  });

  it("没有代码来源时引导用户回到项目设置", async () => {
    knowledgeAnalysisApi.listCodeSources.mockResolvedValue([]);
    renderPage();
    expect(
      await screen.findByText("当前项目还没有可分析的代码来源"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "管理代码仓库" }),
    ).toBeInTheDocument();
  });

  it("切换多仓库来源时只请求该来源的快照", async () => {
    const user = userEvent.setup();
    const inventorySource = {
      ...source,
      source: {
        ...source.source,
        id: 32,
        displayName: "库存服务",
        gitWorkspaceKey: "inventory",
      },
      settings: { ...source.settings, sourceId: 32 },
    };
    knowledgeAnalysisApi.listCodeSources.mockResolvedValue([
      source,
      inventorySource,
    ]);
    knowledgeAnalysisApi.listCodeSnapshots.mockImplementation(
      ({ sourceId }: { sourceId: number }) =>
        Promise.resolve([
          {
            ...snapshot,
            id: sourceId === 31 ? 41 : 42,
            sourceId,
            refName: sourceId === 31 ? "main" : "release/1.0",
          },
        ]),
    );
    renderPage();

    await waitFor(() =>
      expect(knowledgeAnalysisApi.listCodeSnapshots).toHaveBeenCalledWith({
        projectId: 11,
        sourceId: 31,
      }),
    );
    await user.click(screen.getByLabelText("代码仓库"));
    await user.click(await screen.findByText("库存服务"));
    await waitFor(() =>
      expect(knowledgeAnalysisApi.listCodeSnapshots).toHaveBeenLastCalledWith({
        projectId: 11,
        sourceId: 32,
      }),
    );
    expect(
      await screen.findByText(/库存服务 · release\/1.0/),
    ).toBeInTheDocument();
  });
});
