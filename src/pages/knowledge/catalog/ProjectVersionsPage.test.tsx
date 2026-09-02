import { ConfigProvider } from "antd";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

const { listen, knowledgeCatalogApi, knowledgeJobsApi } = vi.hoisted(() => ({
  listen: vi.fn(),
  knowledgeCatalogApi: {
    listProjects: vi.fn(),
    listReleases: vi.fn(),
    getProjectVersionManifest: vi.fn(),
    getProjectVersionCompleteness: vi.fn(),
    startProjectVersionBackfill: vi.fn(),
  },
  knowledgeJobsApi: { get: vi.fn() },
}));

vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@/lib/api/client", () => ({
  getErrorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
  hasTauriRuntime: () => true,
}));
vi.mock("@/lib/api/knowledge-domain", () => ({
  knowledgeCatalogApi,
  knowledgeJobsApi,
}));

import ProjectVersionsPage from "./ProjectVersionsPage";

let progressListener: ((event: { payload: unknown }) => void) | undefined;

function renderPage() {
  return render(
    <ConfigProvider>
      <MemoryRouter initialEntries={["/knowledge/projects/1/versions"]}>
        <Routes>
          <Route
            path="/knowledge/projects/:projectId/versions"
            element={<ProjectVersionsPage />}
          />
          <Route
            path="/knowledge/projects/:projectId/sources"
            element={<div>来源授权工作台</div>}
          />
          <Route
            path="/knowledge/projects/:projectId/overview"
            element={<div />}
          />
          <Route
            path="/knowledge/projects/:projectId/setup"
            element={<div />}
          />
        </Routes>
      </MemoryRouter>
    </ConfigProvider>,
  );
}

describe("ProjectVersionsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    progressListener = undefined;
    listen.mockImplementation(async (_event: string, listener: unknown) => {
      progressListener = listener as (event: { payload: unknown }) => void;
      return vi.fn();
    });
    knowledgeCatalogApi.listProjects.mockResolvedValue({
      items: [{ id: 1, name: "全业务工单中心" }],
    });
    knowledgeCatalogApi.listReleases.mockResolvedValue([
      { id: 7, version: "v1.2.0" },
    ]);
    knowledgeCatalogApi.getProjectVersionManifest.mockResolvedValue({
      releaseId: 7,
      projectId: 1,
      version: "v1.2.0",
      status: "ready",
      repositories: [],
    });
    knowledgeCatalogApi.getProjectVersionCompleteness.mockResolvedValue({
      releaseId: 7,
      projectId: 1,
      version: "v1.2.0",
      status: "partial",
      stages: [
        {
          stage: "parsing",
          label: "文档解析",
          status: "partial",
          completedCount: 1,
          totalCount: 2,
          summary: "1/2 个文档版本已解析",
        },
      ],
    });
    knowledgeCatalogApi.startProjectVersionBackfill.mockResolvedValue({
      jobKey: "knowledge-project-version-backfill-7",
      status: "queued",
      progressCurrent: 0,
      progressTotal: 0,
      message: "项目版本历史处理回填已进入队列",
      checkpoint: { stage: "backfill" },
    });
    knowledgeJobsApi.get.mockImplementation(() => new Promise(() => undefined));
  });

  afterEach(() => cleanup());

  it("只显示当前回填任务的实时进度，并在完成后刷新完整度", async () => {
    const user = userEvent.setup();
    renderPage();

    await user.click(
      await screen.findByRole("button", { name: "补齐历史处理" }),
    );
    await user.click(screen.getByRole("button", { name: "开始回填" }));
    expect(await screen.findByText("历史处理回填执行中")).toBeInTheDocument();
    await waitFor(() => expect(progressListener).toBeDefined());

    progressListener?.({
      payload: {
        jobKey: "other-job",
        status: "running",
        stage: "backfill",
        current: 9,
        total: 9,
        message: "不应显示",
        canCancel: true,
      },
    });
    expect(screen.queryByText("不应显示")).not.toBeInTheDocument();

    progressListener?.({
      payload: {
        jobKey: "knowledge-project-version-backfill-7",
        status: "running",
        stage: "backfill",
        current: 10,
        total: 30,
        message: "已补齐 10/30 个文档版本",
        canCancel: true,
      },
    });
    expect(
      await screen.findByText("已补齐 10/30 个文档版本"),
    ).toBeInTheDocument();

    const completenessCallsBeforeCompletion =
      knowledgeCatalogApi.getProjectVersionCompleteness.mock.calls.length;
    progressListener?.({
      payload: {
        jobKey: "knowledge-project-version-backfill-7",
        status: "completed",
        stage: "backfill",
        current: 30,
        total: 30,
        message: "历史文档解析与全文索引已补齐",
        canCancel: false,
      },
    });
    expect(await screen.findByText("历史处理回填已完成")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        knowledgeCatalogApi.getProjectVersionCompleteness.mock.calls.length,
      ).toBeGreaterThan(completenessCallsBeforeCompletion),
    );
  });

  it("提供来源授权的可发现入口", async () => {
    const user = userEvent.setup();
    renderPage();

    await user.click(
      await screen.findByRole("button", { name: "管理来源授权" }),
    );
    expect(await screen.findByText("来源授权工作台")).toBeInTheDocument();
  });
});
