import { ConfigProvider } from "antd";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes, useNavigate } from "react-router-dom";

const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listReleases: vi.fn(),
}));
const knowledgeSearchApi = vi.hoisted(() => ({
  searchCatalog: vi.fn(),
  rebuildFts: vi.fn(),
}));
const knowledgeDocumentsApi = vi.hoisted(() => ({ citationDetail: vi.fn() }));
const knowledgeTerminologyApi = vi.hoisted(() => ({
  listProjectTerms: vi.fn(),
  upsertProjectTerm: vi.fn(),
  deleteProjectTerm: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error
      ? error.message
      : typeof error === "object" && error !== null && "message" in error
        ? String(error.message)
        : String(error),
  getErrorCode: (error: unknown) =>
    typeof error === "object" && error !== null && "code" in error
      ? String(error.code)
      : "UNKNOWN",
}));
vi.mock("@/lib/api/knowledge-domain", () => ({
  knowledgeCatalogApi,
  knowledgeDocumentsApi,
  knowledgeSearchApi,
  knowledgeTerminologyApi,
}));

import ProjectSearchPage from "./ProjectSearchPage";

function RouteSwitchButton() {
  const navigate = useNavigate();
  return (
    <button onClick={() => navigate("/knowledge/projects/12/search")}>
      切换项目
    </button>
  );
}

function renderPage(initialEntry = "/knowledge/projects/11/search") {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={[initialEntry]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/search"
            element={
              <>
                <ProjectSearchPage />
                <RouteSwitchButton />
              </>
            }
          />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div>项目概览</div>}
          />
          <Route path="/knowledge/projects" element={<div>项目列表</div>} />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe("ProjectSearchPage", () => {
  beforeEach(() => {
    // 每个用例都可能为同一 API 排入一次性响应；只清除调用记录会遗留未消费的
    // mockResolvedValueOnce，从而让后续用例读取前一个用例的数据。
    knowledgeCatalogApi.listProjects.mockReset();
    knowledgeCatalogApi.listReleases.mockReset();
    knowledgeSearchApi.searchCatalog.mockReset();
    knowledgeSearchApi.rebuildFts.mockReset();
    knowledgeDocumentsApi.citationDetail.mockReset();
    knowledgeTerminologyApi.listProjectTerms.mockReset();
    knowledgeTerminologyApi.upsertProjectTerm.mockReset();
    knowledgeTerminologyApi.deleteProjectTerm.mockReset();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [
        {
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
        },
      ],
      total: 1,
      offset: 0,
      limit: 100,
    });
    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 21, projectId: 11, version: "v1.0.0" },
    ]);
    knowledgeSearchApi.searchCatalog.mockResolvedValue({
      items: [
        {
          score: 1,
          channels: ["title", "fts"],
          citation: {
            citationKey: "chunk:31",
            sourceType: "manual",
            documentId: 31,
            chunkId: 31,
            title: "订单创建说明",
            logicalPath: "docs/orders.md",
            headingPath: "创建订单",
            excerpt: "创建订单需要校验客户状态。",
            releaseId: 21,
          },
          content: "创建订单需要校验客户状态。",
          diagnostics: {},
        },
      ],
      nextCursor: null,
      resultSnapshot: "initial",
      snapshotChanged: false,
    });
    knowledgeSearchApi.rebuildFts.mockResolvedValue(8);
    knowledgeDocumentsApi.citationDetail.mockResolvedValue({
      citation: {
        citationKey: "chunk:31",
        documentId: 31,
        title: "订单创建说明",
        logicalPath: "docs/orders.md",
        startLine: 3,
        endLine: 4,
      },
      document: { id: 31, title: "订单创建说明" },
      version: { id: 41 },
      chunk: {
        id: 31,
        headingPath: "创建订单",
        content: "创建订单需要校验客户状态。",
      },
    });
    knowledgeTerminologyApi.listProjectTerms.mockResolvedValue([]);
    knowledgeTerminologyApi.upsertProjectTerm.mockResolvedValue({
      id: 91,
      projectId: 11,
      term: "工单",
      aliases: ["WorkOrder", "work_order"],
      confirmationNote: "项目负责人已确认。",
      createdBy: "本地用户",
      createdAt: "2026-08-04T00:00:00Z",
      updatedAt: "2026-08-04T00:00:00Z",
    });
    knowledgeTerminologyApi.deleteProjectTerm.mockResolvedValue(undefined);
  });

  afterEach(cleanup);

  it("锁定当前项目、按版本筛选并显示可读引用", async () => {
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("搜索 客户服务平台")).toBeInTheDocument();
    await user.click(screen.getByText("高级筛选（可选）"));
    await user.click(screen.getByLabelText("项目版本"));
    await user.click(await screen.findByText("v1.0.0"));
    await user.type(screen.getByLabelText("搜索关键词"), "创建订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));

    await waitFor(() =>
      expect(knowledgeSearchApi.searchCatalog).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: 21,
        query: "创建订单",
        documentTypes: [],
        cursor: null,
        limit: 20,
      }),
    );
    expect(knowledgeCatalogApi.listProjects).toHaveBeenCalledWith({
      projectId: 11,
      limit: 1,
      offset: 0,
    });
    expect(
      await screen.findByRole("heading", { name: "订单创建说明" }),
    ).toBeInTheDocument();
    expect(screen.getByText("docs/orders.md · 创建订单")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "搜索结果" })).toHaveTextContent(
      "创建订单需要校验客户状态。",
    );
    expect(screen.getByText("标题匹配")).toBeInTheDocument();
    expect(screen.getByText("全文匹配")).toBeInTheDocument();
  });

  it("展示可重试的搜索错误", async () => {
    const user = userEvent.setup();
    knowledgeSearchApi.searchCatalog.mockRejectedValue(
      new Error("全文索引暂不可用"),
    );
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));

    expect(await screen.findByText("全文索引暂不可用")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /重\s*试/ })).toBeInTheDocument();
  });

  it("在全文索引失配时提供重建并重新搜索的恢复动作", async () => {
    const user = userEvent.setup();
    knowledgeSearchApi.searchCatalog.mockRejectedValueOnce({
      code: "KNOWLEDGE_FTS_REBUILD_REQUIRED",
      message: "全文索引尚未准备完成，请在知识库中重建全文索引后重试",
    });
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));

    const rebuildButton = await screen.findByRole("button", {
      name: "重建全文索引",
    });
    await user.click(rebuildButton);

    await waitFor(() =>
      expect(knowledgeSearchApi.rebuildFts).toHaveBeenCalledTimes(1),
    );
    await waitFor(() =>
      expect(knowledgeSearchApi.searchCatalog).toHaveBeenCalledTimes(2),
    );
    expect(
      await screen.findByRole("heading", { name: "订单创建说明" }),
    ).toBeInTheDocument();
  });

  it("重建全文索引期间切换项目不会让旧搜索重新覆盖新范围", async () => {
    const user = userEvent.setup();
    const rebuilding = deferred<number>();
    knowledgeSearchApi.searchCatalog.mockRejectedValueOnce({
      code: "KNOWLEDGE_FTS_REBUILD_REQUIRED",
      message: "全文索引尚未准备完成，请在知识库中重建全文索引后重试",
    });
    knowledgeSearchApi.rebuildFts.mockReturnValueOnce(rebuilding.promise);
    knowledgeCatalogApi.listProjects.mockImplementation(({ projectId }) =>
      Promise.resolve({
        items: [
          {
            id: projectId,
            projectKey: `project-${projectId}`,
            name: projectId === 12 ? "项目十二" : "客户服务平台",
            aliases: [],
            description: "",
            gitWorkspaceKeys: [],
            gitWorkspaceKey: "",
            defaultBranch: "main",
            enabled: true,
            createdAt: "2026-08-01T00:00:00Z",
            updatedAt: "2026-08-01T00:00:00Z",
          },
        ],
        total: 1,
        offset: 0,
        limit: 1,
      }),
    );
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    await user.click(
      await screen.findByRole("button", { name: "重建全文索引" }),
    );
    await user.click(screen.getByRole("button", { name: "切换项目" }));
    await screen.findByText("搜索 项目十二");

    rebuilding.resolve(8);
    await waitFor(() =>
      expect(knowledgeSearchApi.rebuildFts).toHaveBeenCalledTimes(1),
    );
    await Promise.resolve();
    await Promise.resolve();
    expect(knowledgeSearchApi.searchCatalog).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/全文索引已重建/)).not.toBeInTheDocument();
  });

  it("管理已确认项目术语，并解释本次搜索的扩展范围", async () => {
    const user = userEvent.setup();
    knowledgeSearchApi.searchCatalog.mockResolvedValueOnce({
      items: [],
      nextCursor: null,
      resultSnapshot: "term-search",
      snapshotChanged: false,
      appliedTerms: [{ term: "工单", aliases: ["WorkOrder", "work_order"] }],
    });
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.click(screen.getByRole("button", { name: "管理项目术语" }));
    await waitFor(() =>
      expect(knowledgeTerminologyApi.listProjectTerms).toHaveBeenCalledWith(11),
    );
    expect(await screen.findByText("暂无已确认术语")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "添加术语" }));
    await user.type(screen.getByLabelText("用户术语"), "工单");
    await user.type(
      screen.getByLabelText("代码或业务别名"),
      "WorkOrder, work_order",
    );
    await user.type(screen.getByLabelText("确认说明"), "项目负责人已确认。");
    await user.click(screen.getByRole("button", { name: /保\s*存/ }));
    await waitFor(() =>
      expect(knowledgeTerminologyApi.upsertProjectTerm).toHaveBeenCalledWith({
        id: undefined,
        projectId: 11,
        term: "工单",
        aliases: ["WorkOrder", "work_order"],
        confirmationNote: "项目负责人已确认。",
      }),
    );
    expect(await screen.findByText("WorkOrder")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    await user.type(screen.getByLabelText("搜索关键词"), "工单");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    expect(await screen.findByText("已按项目术语扩展检索")).toBeInTheDocument();
    expect(
      screen.getByText(/工单 → WorkOrder、work_order/),
    ).toBeInTheDocument();
  });

  it("用可读排序理由、安全高亮和引用详情解释命中", async () => {
    const user = userEvent.setup();
    const { container } = renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));

    expect(
      await screen.findByText("标题和正文均匹配，按标题优先显示"),
    ).toBeInTheDocument();
    expect(container.querySelector("mark")?.textContent).toBe("订单");
    await user.click(screen.getByRole("button", { name: "查看引用详情" }));

    await waitFor(() =>
      expect(knowledgeDocumentsApi.citationDetail).toHaveBeenCalledWith(31),
    );
    expect(await screen.findByText("来源文档")).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toHaveTextContent(
      "创建订单需要校验客户状态。",
    );
  });

  it("在输入新关键词但未搜索时，保留已提交搜索的高亮依据", async () => {
    const user = userEvent.setup();
    const { container } = renderPage();

    await screen.findByText("搜索 客户服务平台");
    const input = screen.getByLabelText("搜索关键词");
    await user.type(input, "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    expect((await screen.findAllByRole("mark"))[0]).toHaveTextContent("订单");

    await user.clear(input);
    await user.type(input, "退款");
    expect(container.querySelector("mark")?.textContent).toBe("订单");
  });

  it("引用详情与搜索结果文档不一致时显示受控错误", async () => {
    const user = userEvent.setup();
    knowledgeDocumentsApi.citationDetail.mockResolvedValueOnce({
      citation: { citationKey: "chunk:31", documentId: 99 },
      document: { id: 99, title: "其他文档" },
      version: { id: 41 },
      chunk: { id: 31, headingPath: "", content: "不应显示为当前引用" },
    });
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    await user.click(screen.getByRole("button", { name: "查看引用详情" }));

    expect(
      await screen.findByText("引用与搜索结果不一致，请重新搜索"),
    ).toBeInTheDocument();
    expect(screen.queryByText("不应显示为当前引用")).not.toBeInTheDocument();
  });

  it("引用详情失败后可重试并展示后续成功结果", async () => {
    const user = userEvent.setup();
    knowledgeDocumentsApi.citationDetail.mockRejectedValueOnce(
      new Error("引用服务暂不可用"),
    );
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    await user.click(screen.getByRole("button", { name: "查看引用详情" }));
    expect(await screen.findByText("引用服务暂不可用")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /重\s*试/ }));
    expect(await screen.findByText("来源文档")).toBeInTheDocument();
    expect(knowledgeDocumentsApi.citationDetail).toHaveBeenCalledTimes(2);
  });

  it("路由切换后忽略旧项目的迟到响应", async () => {
    const user = userEvent.setup();
    let resolveFirstProject:
      | ((value: {
          items: Array<{ id: number; name: string }>;
          total: number;
          offset: number;
          limit: number;
        }) => void)
      | undefined;
    knowledgeCatalogApi.listProjects
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirstProject = resolve;
          }),
      )
      .mockResolvedValueOnce({
        items: [{ id: 12, name: "退款服务" }],
        total: 1,
        offset: 0,
        limit: 1,
      });
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);
    renderPage();

    await user.click(screen.getByRole("button", { name: "切换项目" }));
    expect(await screen.findByText("搜索 退款服务")).toBeInTheDocument();
    resolveFirstProject?.({
      items: [{ id: 11, name: "订单服务" }],
      total: 1,
      offset: 0,
      limit: 1,
    });

    await waitFor(() =>
      expect(screen.getByText("搜索 退款服务")).toBeInTheDocument(),
    );
    expect(screen.queryByText("搜索 订单服务")).not.toBeInTheDocument();
  });

  it("将文档文本按纯文本高亮，不解析其中的标签", async () => {
    const user = userEvent.setup();
    knowledgeSearchApi.searchCatalog.mockResolvedValueOnce({
      items: [
        {
          score: 1,
          channels: ["title"],
          citation: {
            citationKey: "document:32:version:42",
            sourceType: "manual",
            documentId: 32,
            title: "<img data-search-xss src=x>订单",
            logicalPath: "docs/orders.md",
            headingPath: "",
            excerpt: "<img data-search-xss src=x>订单",
          },
          content: "",
          diagnostics: {},
        },
      ],
      nextCursor: null,
      resultSnapshot: "initial",
      snapshotChanged: false,
    });
    const { container } = renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));

    expect(
      await screen.findByRole("heading", {
        name: "<img data-search-xss src=x>订单",
      }),
    ).toBeInTheDocument();
    expect(container.querySelector("img[data-search-xss]")).toBeNull();
  });

  it("按上传文档的真实类型筛选图片", async () => {
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.click(screen.getByText("高级筛选（可选）"));
    await user.click(screen.getByLabelText("文档类型"));
    await user.type(screen.getByLabelText("文档类型"), "图片");
    await user.click(await screen.findByText("图片"));
    await user.type(screen.getByLabelText("搜索关键词"), "架构图");
    await user.click(screen.getByRole("button", { name: "搜索" }));

    await waitFor(() =>
      expect(knowledgeSearchApi.searchCatalog).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: null,
        query: "架构图",
        documentTypes: ["image"],
        cursor: null,
        limit: 20,
      }),
    );
  });

  it("分页期间索引更新时保留当前结果并提示刷新", async () => {
    const user = userEvent.setup();
    knowledgeSearchApi.searchCatalog
      .mockResolvedValueOnce({
        items: [
          {
            score: 1,
            channels: ["fts"],
            citation: {
              citationKey: "chunk:31",
              sourceType: "manual",
              documentId: 31,
              chunkId: 31,
              title: "订单创建说明",
              logicalPath: "docs/orders.md",
              headingPath: "创建订单",
              excerpt: "创建订单需要校验客户状态。",
            },
            content: "创建订单需要校验客户状态。",
            diagnostics: {},
          },
        ],
        nextCursor: "next-page",
        resultSnapshot: "before-update",
        snapshotChanged: false,
      })
      .mockResolvedValueOnce({
        items: [],
        nextCursor: null,
        resultSnapshot: "after-update",
        snapshotChanged: true,
      });
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    await user.click(await screen.findByRole("button", { name: "加载更多" }));

    expect(await screen.findByText("搜索结果已有更新")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "订单创建说明" }),
    ).toBeInTheDocument();
    expect(knowledgeSearchApi.searchCatalog).toHaveBeenLastCalledWith({
      projectId: 11,
      projectVersionId: null,
      query: "订单",
      documentTypes: [],
      cursor: "next-page",
      limit: 20,
    });
  });

  it("修改筛选后要求重新搜索，避免用旧游标加载新范围", async () => {
    const user = userEvent.setup();
    knowledgeSearchApi.searchCatalog.mockResolvedValueOnce({
      items: [
        {
          score: 1,
          channels: ["fts"],
          citation: {
            citationKey: "chunk:31",
            sourceType: "manual",
            documentId: 31,
            chunkId: 31,
            title: "订单创建说明",
            logicalPath: "docs/orders.md",
            headingPath: "创建订单",
            excerpt: "创建订单需要校验客户状态。",
          },
          content: "创建订单需要校验客户状态。",
          diagnostics: {},
        },
      ],
      nextCursor: "next-page",
      resultSnapshot: "initial",
      snapshotChanged: false,
    });
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    await user.click(screen.getByText("高级筛选（可选）"));
    await user.click(screen.getByLabelText("项目版本"));
    await user.click(await screen.findByText("v1.0.0"));

    expect(await screen.findByText("筛选条件已修改")).toBeInTheDocument();
    const loadMore = screen.getByRole("button", { name: "加载更多" });
    expect(loadMore).toBeDisabled();
    await user.click(loadMore);
    expect(knowledgeSearchApi.searchCatalog).toHaveBeenCalledTimes(1);
  });

  it("引用详情晚到的旧响应不会覆盖后来打开的引用", async () => {
    const user = userEvent.setup();
    let resolveFirstDetail: ((value: unknown) => void) | undefined;
    let resolveSecondDetail: ((value: unknown) => void) | undefined;
    knowledgeSearchApi.searchCatalog.mockResolvedValueOnce({
      items: [
        {
          score: 1,
          channels: ["fts"],
          citation: {
            citationKey: "chunk:31",
            sourceType: "manual",
            documentId: 31,
            chunkId: 31,
            title: "第一份文档",
            logicalPath: "docs/first.md",
            headingPath: "",
            excerpt: "第一段摘要",
          },
          content: "第一段摘要",
          diagnostics: {},
        },
        {
          score: 1,
          channels: ["fts"],
          citation: {
            citationKey: "chunk:32",
            sourceType: "manual",
            documentId: 32,
            chunkId: 32,
            title: "第二份文档",
            logicalPath: "docs/second.md",
            headingPath: "",
            excerpt: "第二段摘要",
          },
          content: "第二段摘要",
          diagnostics: {},
        },
      ],
      nextCursor: null,
      resultSnapshot: "initial",
      snapshotChanged: false,
    });
    knowledgeDocumentsApi.citationDetail
      .mockImplementationOnce(
        () =>
          new Promise<unknown>((resolve) => {
            resolveFirstDetail = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise<unknown>((resolve) => {
            resolveSecondDetail = resolve;
          }),
      );
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "文档");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    const citationButtons = await screen.findAllByRole("button", {
      name: "查看引用详情",
    });
    await user.click(citationButtons[0]);
    await user.keyboard("{Escape}");
    await user.click(citationButtons[1]);

    resolveSecondDetail?.({
      citation: { citationKey: "chunk:32", documentId: 32 },
      document: { id: 32, title: "第二份文档" },
      version: { id: 42 },
      chunk: { id: 32, headingPath: "", content: "第二段详细内容" },
    });
    expect(await screen.findByText("第二段详细内容")).toBeInTheDocument();

    resolveFirstDetail?.({
      citation: { citationKey: "chunk:31", documentId: 31 },
      document: { id: 31, title: "第一份文档" },
      version: { id: 41 },
      chunk: { id: 31, headingPath: "", content: "第一段过期内容" },
    });
    await waitFor(() =>
      expect(screen.getByRole("dialog")).toHaveTextContent("第二段详细内容"),
    );
    expect(screen.queryByText("第一段过期内容")).not.toBeInTheDocument();
  });

  it("同一页正在加载时不会重复追加结果", async () => {
    const user = userEvent.setup();
    let resolveNextPage: ((value: unknown) => void) | undefined;
    knowledgeSearchApi.searchCatalog
      .mockResolvedValueOnce({
        items: [
          {
            score: 1,
            channels: ["fts"],
            citation: {
              citationKey: "chunk:31",
              sourceType: "manual",
              title: "订单创建说明",
              logicalPath: "docs/orders.md",
              headingPath: "创建订单",
              excerpt: "创建订单需要校验客户状态。",
            },
            content: "创建订单需要校验客户状态。",
            diagnostics: {},
          },
        ],
        nextCursor: "next-page",
        resultSnapshot: "initial",
        snapshotChanged: false,
      })
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveNextPage = resolve;
          }),
      );
    renderPage();

    await screen.findByText("搜索 客户服务平台");
    await user.type(screen.getByLabelText("搜索关键词"), "订单");
    await user.click(screen.getByRole("button", { name: "搜索" }));
    const loadMore = await screen.findByRole("button", { name: "加载更多" });
    await user.click(loadMore);
    await user.click(loadMore);

    expect(knowledgeSearchApi.searchCatalog).toHaveBeenCalledTimes(2);
    resolveNextPage?.({
      items: [],
      nextCursor: null,
      resultSnapshot: "initial",
      snapshotChanged: false,
    });
  });
});
