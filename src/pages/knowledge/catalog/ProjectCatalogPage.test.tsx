import { ConfigProvider, Modal } from "antd";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes, useParams } from "react-router-dom";

const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  upsertProject: vi.fn(),
  deleteProject: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));
vi.mock("@/lib/api/knowledge-domain", () => ({ knowledgeCatalogApi }));

import ProjectCatalogPage from "./ProjectCatalogPage";

const project = {
  id: 11,
  projectKey: "customer-platform",
  name: "客户服务平台",
  aliases: [],
  description: "服务客户的统一平台",
  gitWorkspaceKeys: ["gateway", "orders"],
  gitWorkspaceKey: "gateway",
  defaultBranch: "main",
  enabled: true,
  createdAt: "2026-08-01T00:00:00Z",
  updatedAt: "2026-08-01T00:00:00Z",
};

function ProjectQaRouteProbe() {
  const { projectId } = useParams();
  return <div>项目问答页面 {projectId}</div>;
}

function renderPage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects"]}>
        <Routes>
          <Route path="/knowledge/projects" element={<ProjectCatalogPage />} />
          <Route path="/knowledge/projects/new" element={<div>创建页面</div>} />
          <Route path="/knowledge" element={<div>知识库设置</div>} />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div>项目概览</div>}
          />
          <Route
            path="/knowledge/projects/:projectId/qa"
            element={<ProjectQaRouteProbe />}
          />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

describe("ProjectCatalogPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [project],
      total: 1,
      offset: 0,
      limit: 100,
    });
    knowledgeCatalogApi.upsertProject.mockResolvedValue(project);
    knowledgeCatalogApi.deleteProject.mockResolvedValue(undefined);
  });

  afterEach(() => {
    Modal.destroyAll();
    cleanup();
  });

  it("显示项目、支持编辑并进入概览", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("客户服务平台")).toBeInTheDocument();
    expect(screen.getByText("已关联 2 个代码仓库")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "编辑客户服务平台" }));
    await user.clear(screen.getByLabelText("项目名称"));
    await user.type(screen.getByLabelText("项目名称"), "客户服务中心");
    const editDrawer = screen.getByRole("dialog", { name: "编辑项目" });
    await user.click(within(editDrawer).getByRole("button", { name: "保 存" }));

    await waitFor(() =>
      expect(knowledgeCatalogApi.upsertProject).toHaveBeenCalledWith(
        expect.objectContaining({ name: "客户服务中心" }),
      ),
    );

    await user.click(screen.getByRole("button", { name: "进入项目" }));
    expect(await screen.findByText("项目概览")).toBeInTheDocument();
  });

  it("空状态提供创建项目主操作", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [],
      total: 0,
      offset: 0,
      limit: 100,
    });
    renderPage();

    expect(await screen.findByText("还没有项目")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "创建第一个项目" }));
    expect(await screen.findByText("创建页面")).toBeInTheDocument();
  });

  it("从项目卡片直接进入当前项目问答", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("客户服务平台");
    await user.click(
      screen.getByRole("button", { name: "进入客户服务平台项目问答" }),
    );

    expect(await screen.findByText("项目问答页面 11")).toBeInTheDocument();
  });

  it("读取失败时只提供重试，不显示知识库启停设置", async () => {
    knowledgeCatalogApi.listProjects.mockRejectedValue(
      new Error("项目目录读取失败"),
    );
    renderPage();

    expect(await screen.findByText("项目暂时无法读取")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /重\s*试/ })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "前往知识库设置" }),
    ).not.toBeInTheDocument();
  });

  it("暂停与删除均需要明确确认", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("客户服务平台");

    await user.click(screen.getByRole("button", { name: "暂停客户服务平台" }));
    await user.click(screen.getByRole("button", { name: "暂停项目" }));
    await waitFor(() =>
      expect(knowledgeCatalogApi.upsertProject).toHaveBeenCalledWith(
        expect.objectContaining({ enabled: false }),
      ),
    );

    await user.click(screen.getByRole("button", { name: "删除客户服务平台" }));
    await user.click(screen.getByRole("button", { name: "删除项目" }));
    await waitFor(() =>
      expect(knowledgeCatalogApi.deleteProject).toHaveBeenCalledWith(11),
    );
  });
});
