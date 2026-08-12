import { beforeEach, describe, expect, it, vi } from "vitest";

const { devApiFetch, hasTauriRuntime, invoke } = vi.hoisted(() => ({
  invoke: vi.fn(),
  devApiFetch: vi.fn(),
  hasTauriRuntime: vi.fn(),
}));

vi.mock("../client", () => ({ devApiFetch, hasTauriRuntime, invoke }));

import { knowledgeGraphApi } from "./graph";

describe("knowledgeGraphApi", () => {
  beforeEach(() => vi.clearAllMocks());

  it("桌面端通过项目和版本范围调用图谱构建与查询 Command", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({});
    const input = {
      projectId: 7,
      projectVersionId: 12,
      includeUnconfirmed: false,
    };

    await knowledgeGraphApi.buildProjectGraph(input);
    await knowledgeGraphApi.queryProjectGraph({
      ...input,
      rootEntityKey: "order-api",
      depth: 2,
      nodeLimit: 80,
    });

    expect(invoke).toHaveBeenNthCalledWith(1, "build_knowledge_project_graph", {
      input,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "query_knowledge_project_graph", {
      input: {
        ...input,
        rootEntityKey: "order-api",
        depth: 2,
        nodeLimit: 80,
      },
    });
  });

  it("浏览器开发回退路径保留项目路径与完整 DTO", async () => {
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue({});
    await knowledgeGraphApi.buildProjectGraph({
      projectId: 7,
      projectVersionId: 12,
    });

    expect(devApiFetch).toHaveBeenCalledWith(
      "/knowledge/projects/7/graph/build",
      {
        method: "POST",
        body: JSON.stringify({ projectId: 7, projectVersionId: 12 }),
      },
    );
  });
});
