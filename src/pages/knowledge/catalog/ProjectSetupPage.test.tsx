import { ConfigProvider } from "antd";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

const gitWorkspaceApi = vi.hoisted(() => ({ list: vi.fn() }));
const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listRepositoryBindings: vi.fn(),
  listReleases: vi.fn(),
  upsertProject: vi.fn(),
  replaceRepositoryBindings: vi.fn(),
  createProjectVersionManifest: vi.fn(),
}));
const knowledgeIngestionApi = vi.hoisted(() => ({
  upsertSource: vi.fn(),
  upsertSourcesAtomically: vi.fn(),
  startSourceSync: vi.fn(),
}));
const knowledgeAnalysisApi = vi.hoisted(() => ({ upsertCodeSource: vi.fn() }));

vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
  gitWorkspaceApi,
}));
vi.mock("@/lib/api/knowledge-domain", () => ({
  knowledgeCatalogApi,
  knowledgeAnalysisApi,
  knowledgeIngestionApi,
}));

import ProjectSetupPage from "./ProjectSetupPage";

function renderPage(initialEntry = "/knowledge/projects/new") {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={[initialEntry]}>
        <Routes>
          <Route
            path="/knowledge/projects/new"
            element={<ProjectSetupPage />}
          />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div>项目已打开</div>}
          />
          <Route
            path="/knowledge/projects/:projectId/setup"
            element={<ProjectSetupPage />}
          />
          <Route path="/knowledge/projects" element={<div>项目列表</div>} />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

describe("ProjectSetupPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sessionStorage.clear();
    gitWorkspaceApi.list.mockResolvedValue([
      {
        id: 1,
        workspaceKey: "orders",
        name: "订单服务",
        repoPath: "/tmp/orders",
        credentialKey: "",
        branch: "main",
        remoteUrl: "",
        status: "clean",
        changedFiles: 0,
        ahead: 0,
        behind: 0,
        description: "",
        lastScannedAt: null,
        createdAt: "2026-08-01T00:00:00Z",
        updatedAt: "2026-08-01T00:00:00Z",
      },
    ]);
    knowledgeCatalogApi.upsertProject.mockResolvedValue({ id: 21 });
    knowledgeCatalogApi.listRepositoryBindings.mockResolvedValue([]);
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);
    knowledgeCatalogApi.replaceRepositoryBindings.mockResolvedValue([
      {
        id: 31,
        workspaceKey: "orders",
        defaultBranch: "main",
      },
    ]);
    knowledgeCatalogApi.createProjectVersionManifest.mockResolvedValue({
      releaseId: 41,
    });
    knowledgeIngestionApi.upsertSource.mockResolvedValue({
      id: 51,
      gitWorkspaceKey: "orders",
    });
    knowledgeIngestionApi.startSourceSync.mockResolvedValue({ id: 61 });
    knowledgeAnalysisApi.upsertCodeSource.mockResolvedValue({ id: 71 });
  });

  afterEach(() => cleanup());

  it("按四步保存项目、仓库和初始版本，并开始首次同步", async () => {
    const user = userEvent.setup();
    renderPage();

    await user.type(screen.getByLabelText("项目名称"), "订单中心");
    await user.click(screen.getByRole("button", { name: "选择代码仓库" }));
    expect(await screen.findByText("选择代码仓库")).toBeInTheDocument();

    const repositorySelect = screen.getByRole("combobox");
    await user.click(repositorySelect);
    await user.click(await screen.findByText("订单服务 · main"));
    await user.click(screen.getByRole("button", { name: "选择初始版本" }));
    expect(await screen.findByText("选择初始版本")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "查看同步内容" }));
    expect(await screen.findByText("确认同步内容")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "创建项目并开始同步" }),
    );
    await waitFor(() =>
      expect(knowledgeCatalogApi.upsertProject).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "订单中心",
          projectKey: expect.stringMatching(/^project-[a-z0-9]+$/),
          gitWorkspaceKeys: ["orders"],
          defaultBranch: "main",
        }),
      ),
    );
    expect(knowledgeCatalogApi.replaceRepositoryBindings).toHaveBeenCalledWith(
      expect.objectContaining({
        projectId: 21,
        repositories: [expect.objectContaining({ workspaceKey: "orders" })],
      }),
    );
    expect(
      knowledgeCatalogApi.createProjectVersionManifest,
    ).toHaveBeenCalledWith({
      projectId: 21,
      version: "初始版本",
      repositories: [
        { repositoryBindingId: 31, refType: "branch", refName: "main" },
      ],
    });
    expect(knowledgeIngestionApi.upsertSource).toHaveBeenCalledWith(
      expect.objectContaining({
        projectId: 21,
        sourceType: "git_workspace",
        gitWorkspaceKey: "orders",
        versionStrategy: "git_ref",
      }),
    );
    expect(
      knowledgeIngestionApi.upsertSourcesAtomically,
    ).not.toHaveBeenCalled();
    expect(knowledgeIngestionApi.startSourceSync).toHaveBeenCalledWith({
      sourceId: 51,
      releaseId: 41,
      gitRef: "main",
    });
    expect(knowledgeAnalysisApi.upsertCodeSource).toHaveBeenCalledWith(
      expect.objectContaining({
        source: expect.objectContaining({
          projectId: 21,
          gitWorkspaceKey: "orders",
          sourceType: "git_workspace",
        }),
        allowRemoteProcessing: true,
      }),
    );
    expect(await screen.findByText("项目已准备好")).toBeInTheDocument();
  });

  it("仓库已保存但版本登记失败后允许从设置页继续", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [
        {
          id: 21,
          projectKey: "project-orders",
          name: "订单中心",
          aliases: [],
          description: "订单知识库",
          gitWorkspaceKeys: ["orders"],
          gitWorkspaceKey: "orders",
          defaultBranch: "main",
          enabled: true,
        },
      ],
    });
    knowledgeCatalogApi.listRepositoryBindings.mockResolvedValue([
      {
        id: 31,
        workspaceKey: "orders",
        alias: "订单服务",
        repositoryRole: "service",
        defaultBranch: "main",
      },
    ]);
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);

    renderPage("/knowledge/projects/21/setup");

    expect(await screen.findByText("继续项目设置")).toBeInTheDocument();
    expect(screen.queryByText("暂时无法继续设置")).not.toBeInTheDocument();
    expect(await screen.findByText("订单服务 · main")).toBeInTheDocument();
    expect(screen.getByRole("combobox")).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "选择初始版本" }));
    expect(await screen.findByText("选择初始版本")).toBeInTheDocument();
    expect(knowledgeCatalogApi.listRepositoryBindings).toHaveBeenCalledWith(21);
    expect(knowledgeCatalogApi.listReleases).toHaveBeenCalledWith(21);
  });

  it("恢复设置仅登记缺失版本，不重复保存项目或仓库绑定", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [
        {
          id: 21,
          projectKey: "project-orders",
          name: "订单中心",
          aliases: [],
          description: "订单知识库",
          gitWorkspaceKeys: ["orders"],
          gitWorkspaceKey: "orders",
          defaultBranch: "main",
          enabled: true,
        },
      ],
    });
    knowledgeCatalogApi.listRepositoryBindings.mockResolvedValue([
      {
        id: 31,
        projectId: 21,
        workspaceKey: "orders",
        alias: "订单服务",
        repositoryRole: "service",
        defaultBranch: "main",
        versionStrategy: "branch",
        enabled: true,
        deletedAt: null,
      },
    ]);
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);

    renderPage("/knowledge/projects/21/setup");

    await screen.findByText("继续项目设置");
    await user.click(screen.getByRole("button", { name: "选择初始版本" }));
    await user.click(screen.getByRole("button", { name: "查看同步内容" }));
    await user.click(
      await screen.findByRole("button", { name: "登记版本并开始同步" }),
    );

    await waitFor(() =>
      expect(
        knowledgeCatalogApi.createProjectVersionManifest,
      ).toHaveBeenCalledWith({
        projectId: 21,
        version: "初始版本",
        repositories: [
          { repositoryBindingId: 31, refType: "branch", refName: "main" },
        ],
      }),
    );
    expect(knowledgeCatalogApi.upsertProject).not.toHaveBeenCalled();
    expect(
      knowledgeCatalogApi.replaceRepositoryBindings,
    ).not.toHaveBeenCalled();
  });

  it("已有版本的项目仍可登记新的版本清单", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [
        {
          id: 21,
          projectKey: "project-orders",
          name: "订单中心",
          aliases: [],
          description: "订单知识库",
          gitWorkspaceKeys: ["orders"],
          gitWorkspaceKey: "orders",
          defaultBranch: "main",
          enabled: true,
        },
      ],
    });
    knowledgeCatalogApi.listRepositoryBindings.mockResolvedValue([
      {
        id: 31,
        projectId: 21,
        workspaceKey: "orders",
        alias: "订单服务",
        repositoryRole: "service",
        defaultBranch: "main",
        versionStrategy: "branch",
        enabled: true,
        deletedAt: null,
      },
    ]);
    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 40, version: "v1.0.0" },
    ]);

    renderPage("/knowledge/projects/21/setup");

    await screen.findByText("继续项目设置");
    await user.click(screen.getByRole("button", { name: "选择初始版本" }));
    await user.type(screen.getByLabelText("版本名称"), "v1.1.0");
    await user.click(screen.getByRole("button", { name: "查看同步内容" }));
    await user.click(
      await screen.findByRole("button", { name: "登记版本并开始同步" }),
    );

    await waitFor(() =>
      expect(
        knowledgeCatalogApi.createProjectVersionManifest,
      ).toHaveBeenCalledWith({
        projectId: 21,
        version: "v1.1.0",
        repositories: [
          { repositoryBindingId: 31, refType: "branch", refName: "main" },
        ],
      }),
    );
    expect(knowledgeCatalogApi.upsertProject).not.toHaveBeenCalled();
    expect(
      knowledgeCatalogApi.replaceRepositoryBindings,
    ).not.toHaveBeenCalled();
  });

  it("首次响应丢失后重试复用已有版本并继续来源登记，且不重复项目或绑定", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.createProjectVersionManifest
      // 后端已落库但网络在响应返回前中断；第二次返回同一不可变清单。
      .mockRejectedValueOnce(new Error("版本清单保存响应丢失"))
      .mockResolvedValueOnce({ releaseId: 41 });
    renderPage();

    await user.type(screen.getByLabelText("项目名称"), "订单中心");
    await user.click(screen.getByRole("button", { name: "选择代码仓库" }));
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByText("订单服务 · main"));
    await user.click(screen.getByRole("button", { name: "选择初始版本" }));
    await user.click(screen.getByRole("button", { name: "查看同步内容" }));
    await user.click(
      screen.getByRole("button", { name: "创建项目并开始同步" }),
    );

    expect(await screen.findByText("版本登记暂未完成")).toBeInTheDocument();
    expect(screen.getAllByText("版本清单保存响应丢失").length).toBeGreaterThan(
      0,
    );
    expect(knowledgeCatalogApi.upsertProject).toHaveBeenCalledTimes(1);
    expect(knowledgeCatalogApi.replaceRepositoryBindings).toHaveBeenCalledTimes(
      1,
    );

    await user.click(screen.getByRole("button", { name: "重试登记" }));
    await waitFor(() =>
      expect(
        knowledgeCatalogApi.createProjectVersionManifest,
      ).toHaveBeenCalledTimes(2),
    );
    expect(knowledgeCatalogApi.upsertProject).toHaveBeenCalledTimes(1);
    expect(knowledgeCatalogApi.replaceRepositoryBindings).toHaveBeenCalledTimes(
      1,
    );
    expect(knowledgeIngestionApi.upsertSource).toHaveBeenCalledTimes(1);
    expect(knowledgeIngestionApi.startSourceSync).toHaveBeenCalledWith({
      sourceId: 51,
      releaseId: 41,
      gitRef: "main",
    });
    expect(await screen.findByText("项目已准备好")).toBeInTheDocument();
  });

  it("项目写入失败后重试沿用同一个项目键", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.upsertProject
      .mockRejectedValueOnce(new Error("项目保存失败"))
      .mockResolvedValueOnce({ id: 21 });
    renderPage();

    await user.type(screen.getByLabelText("项目名称"), "订单中心");
    await user.click(screen.getByRole("button", { name: "选择代码仓库" }));
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByText("订单服务 · main"));
    await user.click(screen.getByRole("button", { name: "选择初始版本" }));
    await user.click(screen.getByRole("button", { name: "查看同步内容" }));
    await user.click(
      screen.getByRole("button", { name: "创建项目并开始同步" }),
    );

    expect(await screen.findByText("版本登记暂未完成")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试登记" }));

    await waitFor(() =>
      expect(knowledgeCatalogApi.upsertProject).toHaveBeenCalledTimes(2),
    );
    expect(knowledgeCatalogApi.upsertProject.mock.calls[0][0].projectKey).toBe(
      knowledgeCatalogApi.upsertProject.mock.calls[1][0].projectKey,
    );
  });

  it("多仓库继续使用原子批量来源登记并逐个开始同步", async () => {
    const user = userEvent.setup();
    gitWorkspaceApi.list.mockResolvedValue([
      {
        id: 1,
        workspaceKey: "orders",
        name: "订单服务",
        repoPath: "/tmp/orders",
        credentialKey: "",
        branch: "main",
        remoteUrl: "",
        status: "clean",
        changedFiles: 0,
        ahead: 0,
        behind: 0,
        description: "",
        lastScannedAt: null,
        createdAt: "2026-08-01T00:00:00Z",
        updatedAt: "2026-08-01T00:00:00Z",
      },
      {
        id: 2,
        workspaceKey: "payments",
        name: "支付服务",
        repoPath: "/tmp/payments",
        credentialKey: "",
        branch: "release",
        remoteUrl: "",
        status: "clean",
        changedFiles: 0,
        ahead: 0,
        behind: 0,
        description: "",
        lastScannedAt: null,
        createdAt: "2026-08-01T00:00:00Z",
        updatedAt: "2026-08-01T00:00:00Z",
      },
    ]);
    knowledgeCatalogApi.replaceRepositoryBindings.mockResolvedValue([
      { id: 31, workspaceKey: "orders", defaultBranch: "main" },
      { id: 32, workspaceKey: "payments", defaultBranch: "release" },
    ]);
    knowledgeIngestionApi.upsertSourcesAtomically.mockResolvedValue([
      { id: 51, gitWorkspaceKey: "orders" },
      { id: 52, gitWorkspaceKey: "payments" },
    ]);
    renderPage();

    await user.type(screen.getByLabelText("项目名称"), "订单中心");
    await user.click(screen.getByRole("button", { name: "选择代码仓库" }));
    const repositorySelect = screen.getByRole("combobox");
    await user.click(repositorySelect);
    await user.click(await screen.findByText("订单服务 · main"));
    await user.click(repositorySelect);
    await user.click(await screen.findByText("支付服务 · release"));
    await user.click(screen.getByRole("button", { name: "选择初始版本" }));
    await user.click(screen.getByRole("button", { name: "查看同步内容" }));
    await user.click(
      screen.getByRole("button", { name: "创建项目并开始同步" }),
    );

    await waitFor(() =>
      expect(
        knowledgeIngestionApi.upsertSourcesAtomically,
      ).toHaveBeenCalledWith([
        expect.objectContaining({ gitWorkspaceKey: "orders" }),
        expect.objectContaining({ gitWorkspaceKey: "payments" }),
      ]),
    );
    expect(knowledgeIngestionApi.upsertSource).not.toHaveBeenCalled();
    expect(knowledgeIngestionApi.startSourceSync).toHaveBeenCalledWith({
      sourceId: 51,
      releaseId: 41,
      gitRef: "main",
    });
    expect(knowledgeIngestionApi.startSourceSync).toHaveBeenCalledWith({
      sourceId: 52,
      releaseId: 41,
      gitRef: "release",
    });
  });

  it("恢复设置时不会因暂时不可用的工作区静默移除已有仓库", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [
        {
          id: 21,
          projectKey: "project-orders",
          name: "订单中心",
          aliases: [],
          description: "订单知识库",
          gitWorkspaceKeys: ["orders", "payments"],
          gitWorkspaceKey: "orders",
          defaultBranch: "main",
          enabled: true,
        },
      ],
    });
    knowledgeCatalogApi.listRepositoryBindings.mockResolvedValue([
      {
        id: 31,
        workspaceKey: "orders",
        alias: "订单服务",
        repositoryRole: "service",
        defaultBranch: "main",
      },
      {
        id: 32,
        workspaceKey: "payments",
        alias: "支付服务",
        repositoryRole: "service",
        defaultBranch: "main",
      },
    ]);
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);

    renderPage("/knowledge/projects/21/setup");

    await screen.findByText("继续项目设置");
    expect(
      await screen.findByText("部分已保存的代码仓库暂时无法读取"),
    ).toBeInTheDocument();
    expect(screen.getAllByText(/payments/).length).toBeGreaterThan(0);
    await user.click(screen.getByRole("button", { name: "选择初始版本" }));
    await user.click(
      await screen.findByRole("button", { name: "查看同步内容" }),
    );
    await user.click(
      await screen.findByRole("button", { name: "登记版本并开始同步" }),
    );
    await waitFor(() =>
      expect(
        knowledgeCatalogApi.replaceRepositoryBindings,
      ).not.toHaveBeenCalled(),
    );
  });
});
