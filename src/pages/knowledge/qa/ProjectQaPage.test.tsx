import { ConfigProvider } from "antd";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import type { KnowledgeAskResult } from "@/types";

const knowledgeCatalogApi = vi.hoisted(() => ({
  listProjects: vi.fn(),
  listReleases: vi.fn(),
}));
const knowledgeQaApi = vi.hoisted(() => ({
  askScopedQuestion: vi.fn(),
  listSessions: vi.fn(),
  getSession: vi.fn(),
  persistRound: vi.fn(),
  deleteSession: vi.fn(),
  saveMarkdown: vi.fn(),
}));
const aiProviderApi = vi.hoisted(() => ({ list: vi.fn() }));

vi.mock("@/lib/api", () => ({
  aiProviderApi,
  getErrorCode: (error: unknown) =>
    typeof error === "object" && error !== null && "code" in error
      ? String(error.code)
      : "UNKNOWN",
  getErrorMessage: (error: unknown) => {
    if (error instanceof Error) return error.message;
    if (typeof error === "object" && error !== null && "message" in error) {
      return String(error.message);
    }
    return String(error);
  },
}));
vi.mock("@/lib/api/knowledge-domain", () => ({
  knowledgeCatalogApi,
  knowledgeQaApi,
}));

import ProjectQaPage from "./ProjectQaPage";

function renderPage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects/11/qa"]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/qa"
            element={<ProjectQaPage />}
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

describe("ProjectQaPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [{ id: 11, name: "订单中心", projectKey: "order-center" }],
    });
    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 21, version: "v1.0.0" },
    ]);
    aiProviderApi.list.mockResolvedValue([
      {
        key: "chat",
        name: "项目助手",
        defaultModel: "chat-model",
        embeddingModel: "",
        status: "configured",
        enabled: true,
        capabilities: ["chat"],
      },
    ]);
    knowledgeQaApi.askScopedQuestion.mockResolvedValue({
      answer:
        "## 库存规则\n\n库存不足时**拒绝创建订单**。[citation:chunk:9]；同证据 [document:7:version:8:chunk:9]",
      citationValidation: "verified",
      citations: [
        {
          citationKey: "document:7:version:8:chunk:9",
          chunkId: 9,
          title: "库存规则",
          logicalPath: "docs/inventory.md",
          headingPath: "创建订单",
          startLine: 8,
          endLine: 12,
          excerpt: "库存不足时拒绝创建订单。",
        },
      ],
      conflicts: [],
      evidenceGaps: [],
      retrievalDiagnostics: {},
    });
    const persistedMessages: Array<Record<string, unknown>> = [];
    knowledgeQaApi.listSessions.mockResolvedValue([]);
    knowledgeQaApi.persistRound.mockImplementation(async (input) => {
      const sequence = persistedMessages.length;
      persistedMessages.push(
        {
          id: sequence + 1,
          sessionId: 31,
          role: "user",
          content: input.question,
          evidenceOnly: input.evidenceOnly,
          createdAt: "2026-08-11 10:00:00",
        },
        {
          id: sequence + 2,
          sessionId: 31,
          role: "assistant",
          content: input.answer.answer,
          evidenceOnly: input.evidenceOnly,
          answer: input.answer,
          createdAt: "2026-08-11 10:00:01",
        },
      );
      return {
        session: {
          id: 31,
          projectId: 11,
          projectVersionId: 21,
          releaseCommitSha: "",
          providerKey: input.providerKey,
          model: input.model,
          title: String(persistedMessages[0]?.content ?? "新对话"),
          messageCount: persistedMessages.length,
          createdAt: "2026-08-11 10:00:00",
          updatedAt: "2026-08-11 10:00:01",
        },
        messages: [...persistedMessages],
      };
    });
    knowledgeQaApi.deleteSession.mockResolvedValue(undefined);
    knowledgeQaApi.saveMarkdown.mockResolvedValue("saved");
  });

  afterEach(() => cleanup());

  it("将返回入口保持在内容左侧并导航到项目概览", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");

    const backButton = screen.getByRole("button", { name: "返回项目概览" });
    expect(backButton).toHaveClass("!self-start");
    await user.click(backButton);

    expect(await screen.findByText("项目概览")).toBeInTheDocument();
  });

  it("将问题严格提交到路由项目的选定版本并展开引用证据", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "库存不足怎么办？");
    await user.click(screen.getByRole("button", { name: "查看本地证据" }));
    await waitFor(() =>
      expect(knowledgeQaApi.askScopedQuestion).toHaveBeenCalledWith({
        projectId: 11,
        projectVersionId: 21,
        question: "库存不足怎么办？",
        evidenceOnly: true,
        providerKey: undefined,
        model: undefined,
        conversation: [],
      }),
    );
    expect(
      await screen.findByText("库存规则 · 创建订单 · 第 8-12 行"),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole("heading", { name: "库存规则" }),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("project-qa-answer-markdown-preview"),
    ).toHaveTextContent("拒绝创建订单");
    expect(screen.getByText("拒绝创建订单").tagName).toBe("STRONG");
    await user.click(screen.getByText("库存规则 · 创建订单 · 第 8-12 行"));
    expect(
      await screen.findByText("库存不足时拒绝创建订单。"),
    ).toBeInTheDocument();
  });

  it("一键生成当前版本需求覆盖问题并允许直接提交", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.click(
      screen.getByRole("button", { name: "分析当前版本需求实现情况" }),
    );

    expect(screen.getByLabelText("项目问题")).toHaveValue(
      "请逐条分析 v1.0.0 的需求：哪些已确认实现，哪些只找到代码候选，哪些尚未找到实现证据？",
    );
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));
    await waitFor(() =>
      expect(knowledgeQaApi.askScopedQuestion).toHaveBeenCalledWith(
        expect.objectContaining({
          projectVersionId: 21,
          question:
            "请逐条分析 v1.0.0 的需求：哪些已确认实现，哪些只找到代码候选，哪些尚未找到实现证据？",
        }),
      ),
    );
  });

  it("清楚展示版本需求覆盖模式、候选数量和证据角色", async () => {
    knowledgeQaApi.askScopedQuestion.mockResolvedValue({
      answer: "| 需求 | 判断 |\n|---|---|\n| 一键删除 | 待确认 |",
      citationValidation: "verified",
      citations: [
        {
          citationKey: "document:1:version:1:chunk:1",
          chunkId: 1,
          sourceType: "knowledge_document",
          title: "v1.0.0 需求文档",
          logicalPath: "docs/v1.0.0-requirements.md",
          headingPath: "需求清单",
          excerpt: "新增一键删除功能。",
          commitSha: "",
          externalKey: "",
        },
        {
          citationKey: "code:snapshot:1:chunk:2",
          chunkId: 2,
          sourceType: "code_snapshot",
          title: "DeleteService.java",
          logicalPath: "src/DeleteService.java",
          headingPath: "batchDelete",
          excerpt: "实现批量删除。",
          commitSha: "abc1234",
          externalKey: "",
        },
        {
          citationKey: "code:snapshot:1:chunk:3",
          chunkId: 3,
          sourceType: "code_snapshot",
          title: "DeleteServiceTest.java",
          logicalPath: "src/test/java/DeleteServiceTest.java",
          headingPath: "batchDelete",
          excerpt: "测试批量删除。",
          commitSha: "abc1234",
          externalKey: "",
        },
      ],
      conflicts: [],
      evidenceGaps: ["需求与代码尚未建立显式关系"],
      retrievalDiagnostics: {
        queryMode: "releaseRequirementCoverage",
        coverage: {
          requirementCandidateCount: 4,
          implementationCandidateCount: 6,
          testCandidateCount: 1,
          verifiedRelationCount: 0,
          explicitRelationCount: 0,
        },
      },
    });
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "实现了哪些需求？");
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));

    expect(
      await screen.findByText("已按版本需求覆盖模式分析"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        /已识别 4 条需求候选，并找到 6 条代码候选。另找到 1 条测试源码候选，但源码存在不代表已经执行通过/,
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("需求基线")).toBeInTheDocument();
    expect(screen.getByText("代码候选")).toBeInTheDocument();
    expect(screen.getByText("测试源码候选")).toBeInTheDocument();
  });

  it("没有项目版本时引导用户先创建版本", async () => {
    knowledgeCatalogApi.listReleases.mockResolvedValue([]);
    renderPage();
    expect(await screen.findByText("请先创建一个项目版本")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "管理项目版本" }),
    ).toBeInTheDocument();
  });

  it("引用未核验时仍展示模型原始回答并提示用户复核", async () => {
    knowledgeQaApi.askScopedQuestion.mockResolvedValue({
      answer: "模型根据上下文给出的原始回答。",
      citationValidation: "unverified",
      citations: [],
      conflicts: [],
      evidenceGaps: [],
      retrievalDiagnostics: {},
    });
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "如何创建订单？");
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));

    expect(
      await screen.findByText("模型回答的引用未通过校验"),
    ).toBeInTheDocument();
    expect(
      await screen.findByText("模型根据上下文给出的原始回答。"),
    ).toBeInTheDocument();
  });

  it("展示 Git Agent 的冻结版本口径、工具证据和部分失败摘要", async () => {
    knowledgeQaApi.askScopedQuestion.mockResolvedValue({
      answer:
        "已按冻结版本统计。\n\n合计 **249 次**。[tool:git_commit_count:release:21:repository:7]",
      citationValidation: "notApplicable",
      citations: [
        {
          citationKey: "tool:git_commit_count:release:21:repository:7",
          sourceType: "git_statistics",
          title: "订单服务 Git 提交统计",
          logicalPath: "git/orders-api",
          headingPath: "提交统计",
          commitSha: "d03ccf7b4d4d9d837e2330255c2460a039c926c9",
          externalKey: "orders-api",
          symbolKey: "git.commit_count",
          excerpt:
            "截至所选版本冻结提交 d03ccf7，可达提交 249 次，包含合并提交。",
        },
      ],
      conflicts: [],
      evidenceGaps: ["仓库“orders-ui”：冻结提交在本地仓库中不可达。"],
      retrievalDiagnostics: {
        queryMode: "gitAgent",
        agent: {
          intent: "git.commit_count",
          status: "partial",
          repositoryCount: 2,
          succeededCount: 1,
          failedCount: 1,
          scope: "selectedRelease",
          includeMerges: true,
          totalCommitCount: 249,
        },
      },
    });
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(
      screen.getByLabelText("项目问题"),
      "当前版本有多少次 git 提交？",
    );
    await user.click(screen.getByRole("button", { name: "查看本地证据" }));

    expect(
      await screen.findByText("Git Agent 已返回部分结果"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/冻结提交查询 2 个关联仓库，成功 1 个，失败 1 个/),
    ).toBeInTheDocument();
    expect(screen.getAllByText("Git 实时证据")).toHaveLength(2);
    expect(screen.getByText(/仓库“orders-ui”/)).toBeInTheDocument();
    await user.click(screen.getByText("订单服务 Git 提交统计 · 提交统计"));
    expect(
      await screen.findByText(
        "冻结提交 d03ccf7b4d4d9d837e2330255c2460a039c926c9",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Users\/bin/)).not.toBeInTheDocument();
    expect(
      screen.queryByText("[tool:git_commit_count:release:21:repository:7]"),
    ).not.toBeInTheDocument();
    expect(knowledgeQaApi.persistRound).toHaveBeenCalledWith(
      expect.objectContaining({
        providerKey: "",
        model: "",
        evidenceOnly: true,
      }),
    );
  });

  it("连续追问会携带上一轮用户问题和助手回答", async () => {
    const user = userEvent.setup();
    knowledgeQaApi.askScopedQuestion
      .mockResolvedValueOnce({
        answer: "第一轮结论 [document:1:version:1:chunk:1]",
        citationValidation: "verified",
        citations: [],
        conflicts: [],
        evidenceGaps: [],
        retrievalDiagnostics: {},
      })
      .mockResolvedValueOnce({
        answer: "第二轮补充",
        citationValidation: "unverified",
        citations: [],
        conflicts: [],
        evidenceGaps: [],
        retrievalDiagnostics: {},
      });
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "第一轮问题");
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));
    await waitFor(() =>
      expect(knowledgeQaApi.askScopedQuestion).toHaveBeenCalledTimes(1),
    );
    await user.type(
      screen.getByLabelText("项目问题"),
      "上面方法的限制是什么？",
    );
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));

    await waitFor(() =>
      expect(knowledgeQaApi.askScopedQuestion).toHaveBeenLastCalledWith({
        projectId: 11,
        projectVersionId: 21,
        question: "上面方法的限制是什么？",
        evidenceOnly: false,
        providerKey: "chat",
        model: "chat-model",
        conversation: [
          { role: "user", content: "第一轮问题" },
          {
            role: "assistant",
            content: "第一轮结论 [document:1:version:1:chunk:1]",
          },
        ],
      }),
    );
    expect(screen.getByText("第二轮补充")).toBeInTheDocument();
    expect(screen.getByText("已进行 2 轮")).toBeInTheDocument();
  });

  it("可以手动保存当前对话及其证据链", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "库存不足怎么办？");
    await user.click(screen.getByRole("button", { name: "查看本地证据" }));
    await waitFor(() =>
      expect(knowledgeQaApi.askScopedQuestion).toHaveBeenCalledTimes(1),
    );
    await user.click(
      screen.getByRole("button", { name: "保存当前对话（Markdown）" }),
    );
    await waitFor(() => expect(knowledgeQaApi.saveMarkdown).toHaveBeenCalled());
    const input = knowledgeQaApi.saveMarkdown.mock.calls[0][0];
    expect(input.defaultFileName).toContain("v1.0.0");
    expect(input.content).toContain("# 项目知识问答记录");
    expect(input.content).toContain("AI Provider：未调用（本地证据模式）");
    expect(input.content).toContain("库存不足时拒绝创建订单。");
    expect(input.content).toContain(
      "【证据：库存规则 · 创建订单 · 第 8-12 行】",
    );
    expect(input.content).not.toContain("[citation:chunk:9]");
    expect(input.content).not.toContain(
      "同证据 [document:7:version:8:chunk:9]",
    );
    expect(input.content).toContain("document:7:version:8:chunk:9");
  });

  it("新建对话后会丢弃尚未返回的旧问答响应", async () => {
    let resolveSecond: ((value: KnowledgeAskResult) => void) | undefined;
    knowledgeQaApi.askScopedQuestion
      .mockResolvedValueOnce({
        answer: "第一轮结论",
        citationValidation: "unverified",
        citations: [],
        conflicts: [],
        evidenceGaps: [],
        retrievalDiagnostics: {},
      })
      .mockImplementationOnce(
        () =>
          new Promise<KnowledgeAskResult>((resolve) => {
            resolveSecond = resolve;
          }),
      );
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "第一轮问题");
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));
    expect(await screen.findByText("第一轮结论")).toBeInTheDocument();

    await user.type(screen.getByLabelText("项目问题"), "第二轮问题");
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));
    await waitFor(() => expect(resolveSecond).toBeTypeOf("function"));
    await user.click(screen.getByRole("button", { name: "新建对话" }));
    expect(screen.queryByText("第一轮结论")).not.toBeInTheDocument();
    expect(screen.queryByText("已进行 1 轮")).not.toBeInTheDocument();

    resolveSecond?.({
      answer: "过期回答不应显示",
      citationValidation: "unverified",
      citations: [],
      conflicts: [],
      evidenceGaps: [],
      retrievalDiagnostics: {},
    });
    await waitFor(() =>
      expect(screen.queryByText("过期回答不应显示")).not.toBeInTheDocument(),
    );
  });

  it("刷新后当前版本被替换时会清空旧会话", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "库存不足怎么办？");
    await user.click(screen.getByRole("button", { name: "查看本地证据" }));
    expect(await screen.findByText("已进行 1 轮")).toBeInTheDocument();

    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 22, version: "v2.0.0" },
    ]);
    await user.click(screen.getByRole("button", { name: "刷新" }));

    await waitFor(() =>
      expect(screen.queryByText("已进行 1 轮")).not.toBeInTheDocument(),
    );
    expect(
      screen.queryByRole("button", { name: "保存当前对话（Markdown）" }),
    ).not.toBeInTheDocument();
  });

  it("刷新后当前版本的提交发生变化时会清空旧会话", async () => {
    const user = userEvent.setup();
    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 21, version: "v1.0.0", commitSha: "old-sha" },
    ]);
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "库存不足怎么办？");
    await user.click(screen.getByRole("button", { name: "查看本地证据" }));
    expect(await screen.findByText("已进行 1 轮")).toBeInTheDocument();

    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 21, version: "v1.0.0", commitSha: "new-sha" },
    ]);
    await user.click(screen.getByRole("button", { name: "刷新" }));

    await waitFor(() =>
      expect(screen.queryByText("已进行 1 轮")).not.toBeInTheDocument(),
    );
  });

  it("刷新后当前聊天模型发生变化时会清空旧会话", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "库存不足怎么办？");
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));
    expect(await screen.findByText("已进行 1 轮")).toBeInTheDocument();

    aiProviderApi.list.mockResolvedValue([
      {
        key: "chat",
        name: "项目助手",
        defaultModel: "chat-model-v2",
        embeddingModel: "",
        status: "configured",
        enabled: true,
        capabilities: ["chat"],
      },
    ]);
    await user.click(screen.getByRole("button", { name: "刷新" }));

    await waitFor(() =>
      expect(screen.queryByText("已进行 1 轮")).not.toBeInTheDocument(),
    );
  });

  it("打开页面时会恢复最近一次兼容的已保存对话", async () => {
    knowledgeQaApi.listSessions.mockResolvedValue([
      {
        id: 41,
        projectId: 11,
        projectVersionId: 21,
        releaseCommitSha: "",
        providerKey: "chat",
        model: "chat-model",
        title: "已保存的问题",
        messageCount: 2,
        createdAt: "2026-08-11 10:00:00",
        updatedAt: "2026-08-11 10:00:01",
      },
    ]);
    knowledgeQaApi.getSession.mockResolvedValue({
      session: (await knowledgeQaApi.listSessions())[0],
      messages: [
        {
          id: 1,
          sessionId: 41,
          role: "user",
          content: "已保存的问题",
          evidenceOnly: false,
          createdAt: "2026-08-11 10:00:00",
        },
        {
          id: 2,
          sessionId: 41,
          role: "assistant",
          content: "已恢复的回答",
          evidenceOnly: false,
          answer: {
            answer: "已恢复的回答",
            citationValidation: "unverified",
            citations: [],
            conflicts: [],
            evidenceGaps: [],
            retrievalDiagnostics: {},
          },
          createdAt: "2026-08-11 10:00:01",
        },
      ],
    });

    renderPage();
    expect(await screen.findByText("已恢复的回答")).toBeInTheDocument();
    expect(screen.getByText("已进行 1 轮")).toBeInTheDocument();
  });

  it("会话保存失败时不保留只存在内存中的缺轮回答", async () => {
    knowledgeQaApi.persistRound.mockRejectedValueOnce(
      new Error("磁盘写入失败"),
    );
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "不能丢失的问题");
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));

    expect(
      await screen.findByText("回答已生成，但会话保存失败：磁盘写入失败"),
    ).toBeInTheDocument();
    expect(screen.queryByText("库存规则")).not.toBeInTheDocument();
    expect(screen.getByLabelText("项目问题")).toHaveValue("不能丢失的问题");
  });

  it("Provider 临时中断时保留问题并提供一键重试", async () => {
    knowledgeQaApi.askScopedQuestion
      .mockRejectedValueOnce({
        code: "PROVIDER_TRANSIENT",
        message: "Provider 回答超时，请重试",
      })
      .mockResolvedValueOnce({
        answer: "重试后的回答",
        citationValidation: "unverified",
        citations: [],
        conflicts: [],
        evidenceGaps: [],
        retrievalDiagnostics: {},
      });
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("问答 订单中心");
    await user.type(screen.getByLabelText("项目问题"), "分析当前版本需求");
    await user.click(screen.getByRole("button", { name: "基于证据回答" }));

    expect(
      await screen.findByText("Provider 回答超时，请重试"),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("项目问题")).toHaveValue("分析当前版本需求");
    await user.click(screen.getByRole("button", { name: "重试回答" }));

    expect(await screen.findByText("重试后的回答")).toBeInTheDocument();
    expect(knowledgeQaApi.askScopedQuestion).toHaveBeenCalledTimes(2);
  });

  it("已停用的聊天 Provider 会话只允许查看，不能继续追加", async () => {
    const session = {
      id: 51,
      projectId: 11,
      projectVersionId: 21,
      releaseCommitSha: "",
      providerKey: "chat",
      model: "chat-model",
      title: "停用模型的历史会话",
      messageCount: 2,
      createdAt: "2026-08-11 10:00:00",
      updatedAt: "2026-08-11 10:00:01",
    };
    knowledgeQaApi.listSessions.mockResolvedValue([session]);
    aiProviderApi.list.mockResolvedValue([
      {
        key: "chat",
        name: "项目助手",
        defaultModel: "chat-model",
        status: "configured",
        enabled: false,
        capabilities: ["chat"],
      },
    ]);
    knowledgeQaApi.getSession.mockResolvedValue({
      session,
      messages: [],
    });
    const user = userEvent.setup();
    renderPage();
    await screen.findByText("停用模型的历史会话");
    expect(knowledgeQaApi.getSession).not.toHaveBeenCalled();
    await user.click(
      screen.getByRole("button", { name: /^停用模型的历史会话/ }),
    );
    expect(
      await screen.findByText("此历史对话的版本或模型已变化，仅供查看"),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("项目问题")).toBeDisabled();
  });
});
