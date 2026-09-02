import { ConfigProvider } from "antd";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

const knowledgeApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listSources: vi.fn(),
  listReleases: vi.fn(),
  upsertSource: vi.fn(),
  startSourceSync: vi.fn(),
}));
const knowledgeCatalogApi = vi.hoisted(() => ({
  listRepositoryBindings: vi.fn(),
  getProjectVersionManifest: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
  knowledgeApi,
}));
vi.mock("@/lib/api/knowledge-domain", () => ({ knowledgeCatalogApi }));

import ProjectSourcesPage from "./ProjectSourcesPage";

const source = {
  id: 31,
  sourceKey: "project-1-workspace-orders",
  projectId: 1,
  sourceType: "git_workspace",
  displayName: "订单服务",
  rootPath: "",
  gitWorkspaceKey: "orders",
  includeGlobs: [],
  excludeGlobs: [],
  versionStrategy: "git_ref",
  syncMode: "manual",
  allowRemoteEmbedding: false,
  enabled: true,
  lastCommitSha: "",
  lastSyncStatus: "success",
  lastSyncedAt: "2026-08-29 10:00:00",
  lastError: null,
  createdAt: "2026-08-29 10:00:00",
  updatedAt: "2026-08-29 10:00:00",
  deletedAt: null,
};

function renderPage(initialEntry = "/knowledge/projects/1/sources") {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={[initialEntry]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/sources"
            element={<ProjectSourcesPage />}
          />
          <Route path="/knowledge/projects" element={<div>项目列表</div>} />
          <Route
            path="/knowledge/projects/:projectId/versions"
            element={<div>项目版本</div>}
          />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

describe("ProjectSourcesPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    knowledgeApi.listProjects.mockResolvedValue({
      items: [{ id: 1, name: "订单中心" }],
    });
    knowledgeApi.listSources.mockResolvedValue([source]);
    knowledgeApi.listReleases.mockResolvedValue([{ id: 7, version: "v1.0.0" }]);
    knowledgeApi.upsertSource.mockResolvedValue({
      ...source,
      allowRemoteEmbedding: true,
    });
    knowledgeApi.startSourceSync.mockResolvedValue({ jobKey: "sync-31" });
    knowledgeCatalogApi.listRepositoryBindings.mockResolvedValue([
      { id: 101, workspaceKey: "orders" },
    ]);
    knowledgeCatalogApi.getProjectVersionManifest.mockResolvedValue({
      repositories: [
        {
          repositoryBindingId: 101,
          inclusionStatus: "ready",
          resolvedCommitSha: "a".repeat(40),
        },
      ],
    });
  });

  afterEach(() => cleanup());

  it("只加载当前项目来源并在确认后逐仓库授权", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("订单服务")).toBeInTheDocument();
    expect(knowledgeApi.listSources).toHaveBeenCalledWith(1);
    expect(screen.getByText("未授权")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "授权" }));
    expect(await screen.findByText("授权远程向量化")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "确认授权" }));

    await waitFor(() =>
      expect(knowledgeApi.upsertSource).toHaveBeenCalledWith(
        expect.objectContaining({
          id: 31,
          projectId: 1,
          allowRemoteEmbedding: true,
        }),
      ),
    );
    expect(await screen.findByText("已授权")).toBeInTheDocument();
  });

  it("非法项目地址不加载全局来源", async () => {
    renderPage("/knowledge/projects/invalid/sources");

    expect(await screen.findByText("项目地址无效")).toBeInTheDocument();
    expect(knowledgeApi.listSources).not.toHaveBeenCalled();
  });

  it("要求显式选择版本，并使用版本清单冻结的 Commit 同步 Git 来源", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("订单服务");
    await user.click(screen.getByRole("button", { name: "同步" }));
    expect(knowledgeApi.startSourceSync).not.toHaveBeenCalled();

    await user.click(screen.getByRole("combobox", { name: "同步到版本" }));
    await user.click(await screen.findByText("v1.0.0"));
    await user.click(screen.getByRole("button", { name: "同步" }));

    await waitFor(() =>
      expect(knowledgeApi.startSourceSync).toHaveBeenCalledWith({
        sourceId: 31,
        releaseId: 7,
        gitRef: "a".repeat(40),
      }),
    );
  });
});
