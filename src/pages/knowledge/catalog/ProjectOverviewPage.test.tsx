import { ConfigProvider } from "antd";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));
vi.mock("@/lib/api/knowledge-domain", () => ({ knowledgeCatalogApi }));

import ProjectOverviewPage from "./ProjectOverviewPage";

const project = {
  id: 11,
  projectKey: "customer-platform",
  name: "客户服务平台",
  aliases: [],
  description: "服务客户的统一平台",
  gitWorkspaceKeys: ["gateway"],
  gitWorkspaceKey: "gateway",
  defaultBranch: "main",
  enabled: true,
  createdAt: "2026-08-01T00:00:00Z",
  updatedAt: "2026-08-01T00:00:00Z",
};

function renderPage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects/11/overview"]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<ProjectOverviewPage />}
          />
          <Route
            path="/knowledge/projects/:projectId/embedding"
            element={<div>向量索引工作台</div>}
          />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

describe("ProjectOverviewPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [project],
      total: 1,
      offset: 0,
      limit: 1,
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("提供全局索引配置入口，并说明项目与版本过滤仍然生效", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("向量化与索引")).toBeVisible();
    expect(
      screen.getByText(
        "配置当前设备的全局本地索引方案；构建或重建不会只处理当前项目。",
      ),
    ).toBeVisible();
    expect(
      screen.getByText("项目问答与检索仍会按所选项目和版本过滤。"),
    ).toBeVisible();

    await user.click(screen.getByRole("button", { name: "配置向量化与索引" }));
    expect(await screen.findByText("向量索引工作台")).toBeVisible();
  });
});
