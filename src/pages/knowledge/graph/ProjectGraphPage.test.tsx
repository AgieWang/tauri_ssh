import { ConfigProvider } from "antd";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listReleases: vi.fn(),
}));
const knowledgeGraphApi = vi.hoisted(() => ({
  buildProjectGraph: vi.fn(),
  queryProjectGraph: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));
vi.mock("@/lib/api/knowledge-domain", () => ({
  knowledgeCatalogApi,
  knowledgeGraphApi,
}));

import ProjectGraphPage from "./ProjectGraphPage";

const projection = {
  buildId: 81,
  buildKey: "graph:11:21:test",
  projectId: 11,
  projectVersionId: 21,
  truncated: false,
  nodes: [
    { id: 1, entityType: "document", entityKey: "1", label: "订单接口说明" },
    { id: 2, entityType: "api", entityKey: "order-api", label: "创建订单接口" },
  ],
  edges: [
    {
      id: 3,
      fromNodeId: 1,
      relationType: "describes",
      toNodeId: 2,
      evidence: { documentVersionId: 7 },
      confidence: 1,
      confirmed: true,
      sourceRelationRef: "relation:3",
    },
  ],
};

function renderPage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects/11/graph"]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/graph"
            element={<ProjectGraphPage />}
          />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div>项目概览</div>}
          />
          <Route
            path="/knowledge/projects/:projectId/versions"
            element={<div>项目版本</div>}
          />
          <Route path="/knowledge/projects" element={<div>项目列表</div>} />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

describe("ProjectGraphPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [{ id: 11, name: "订单中心" }],
    });
    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 21, version: "v1.0.0" },
    ]);
    knowledgeGraphApi.buildProjectGraph.mockResolvedValue({
      buildId: 81,
      buildKey: "graph:11:21:test",
      projectId: 11,
      projectVersionId: 21,
      nodeCount: 2,
      edgeCount: 1,
      reused: false,
    });
    knowledgeGraphApi.queryProjectGraph.mockResolvedValue(projection);
  });

  afterEach(() => cleanup());

  it("按项目版本生成本地图谱并展示可追溯关系", async () => {
    const user = userEvent.setup();
    renderPage();
    await waitFor(() =>
      expect(knowledgeCatalogApi.listReleases).toHaveBeenCalledWith(11),
    );

    await user.click(screen.getByRole("button", { name: "生成知识图谱" }));
    await waitFor(() =>
      expect(knowledgeGraphApi.buildProjectGraph).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: 21,
        includeUnconfirmed: false,
      }),
    );
    expect(
      await screen.findByText("关系清单（起点 → 关系 → 终点）"),
    ).toBeInTheDocument();
    expect(screen.getByText("文档版本 #7")).toBeInTheDocument();
    expect(screen.getByText("文档 · 订单接口说明")).toBeInTheDocument();
    expect(screen.getAllByText("说明").length).toBeGreaterThan(0);
  });

  it("没有项目版本时引导用户先完成版本管理", async () => {
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);
    renderPage();
    expect(await screen.findByText("请先创建一个项目版本")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "管理项目版本" }),
    ).toBeInTheDocument();
  });

  it("重新生成或查询失败时保留已经显示的图谱", async () => {
    const user = userEvent.setup();
    renderPage();
    await waitFor(() =>
      expect(knowledgeCatalogApi.listReleases).toHaveBeenCalledWith(11),
    );
    await user.click(screen.getByRole("button", { name: "生成知识图谱" }));
    expect(await screen.findByText("文档版本 #7")).toBeInTheDocument();

    knowledgeGraphApi.queryProjectGraph.mockRejectedValueOnce(
      new Error("查询暂时失败"),
    );
    await user.click(screen.getByRole("button", { name: "更新视图" }));
    expect(await screen.findByText("查询暂时失败")).toBeInTheDocument();
    expect(screen.getByText("文档版本 #7")).toBeInTheDocument();
  });

  it("将代码和版本实体转换为业务名称，并收敛过多节点的画布", async () => {
    const user = userEvent.setup();
    const crowdedProjection = {
      ...projection,
      nodes: [
        {
          id: 1,
          entityType: "git_commit",
          entityKey: "a".repeat(40),
          label: `git_commit: ${"a".repeat(40)}`,
        },
        ...Array.from({ length: 16 }, (_, index) => ({
          id: index + 2,
          entityType: "code_symbol",
          entityKey: `symbol-${String(index + 1)}`,
          label: `code_symbol: src/main/java/com/example/OrderService${String(index + 1)}.java`,
        })),
      ],
      edges: Array.from({ length: 16 }, (_, index) => ({
        ...projection.edges[0],
        id: index + 10,
        fromNodeId: 1,
        toNodeId: index + 2,
      })),
    };
    knowledgeGraphApi.queryProjectGraph.mockResolvedValue(crowdedProjection);
    renderPage();
    await waitFor(() =>
      expect(knowledgeCatalogApi.listReleases).toHaveBeenCalledWith(11),
    );

    await user.click(screen.getByRole("button", { name: "生成知识图谱" }));
    expect(
      await screen.findByText(/为保证可读性，图中仅展示 15 个关键实体/),
    ).toBeInTheDocument();
    expect(screen.getAllByText("代码元素").length).toBeGreaterThan(0);
    expect(screen.getByText("提交 aaaaaaaaaaaa")).toBeInTheDocument();
    const codeElement = screen.getByText("代码元素 · OrderService1.java");
    expect(codeElement).toBeInTheDocument();
    expect(codeElement).toHaveAttribute("tabindex", "0");
    expect(codeElement).toHaveAttribute(
      "aria-label",
      "代码元素完整路径：src/main/java/com/example/OrderService1.java",
    );
    await user.hover(codeElement);
    expect(
      await screen.findByText("src/main/java/com/example/OrderService1.java"),
    ).toBeInTheDocument();
  });
});
