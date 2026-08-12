import { beforeEach, describe, expect, it, vi } from "vitest";

const { devApiFetch, hasTauriRuntime, invoke } = vi.hoisted(() => ({
  invoke: vi.fn(),
  devApiFetch: vi.fn(),
  hasTauriRuntime: vi.fn(),
}));
const knowledgeApi = vi.hoisted(() => ({ upsertCodeSource: vi.fn() }));

vi.mock("../client", () => ({ devApiFetch, hasTauriRuntime, invoke }));
vi.mock("../knowledge", () => ({ knowledgeApi }));

import { knowledgeAnalysisApi } from "./analysis";

describe("knowledgeAnalysisApi", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("在桌面端通过项目范围的领域 Command 读写静态分析链路", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({});

    await knowledgeAnalysisApi.listCodeSources(7);
    knowledgeAnalysisApi.upsertCodeSource({
      source: {
        sourceKey: "project-7-orders",
        projectId: 7,
        sourceType: "git_workspace",
        displayName: "订单服务",
        rootPath: "/tmp/orders",
        gitWorkspaceKey: "orders",
        includeGlobs: [],
        excludeGlobs: [],
        versionStrategy: "branch",
        syncMode: "manual",
        allowRemoteEmbedding: false,
        enabled: true,
      },
      includeUntracked: false,
      maxFileSizeBytes: 1_048_576,
      allowedLanguages: ["rust"],
      allowRemoteProcessing: false,
    });
    await knowledgeAnalysisApi.listCodeSnapshots({
      projectId: 7,
      sourceId: 12,
    });
    await knowledgeAnalysisApi.captureGitSnapshot({
      projectId: 7,
      sourceId: 12,
      gitRef: "v1.2.0",
      projectVersionId: 22,
    });
    await knowledgeAnalysisApi.analyzeSnapshot({
      projectId: 7,
      snapshotId: 31,
    });
    await knowledgeAnalysisApi.generateDocuments({
      projectId: 7,
      snapshotId: 31,
    });

    expect(invoke).toHaveBeenNthCalledWith(
      1,
      "list_knowledge_analysis_code_sources",
      { projectId: 7 },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "list_knowledge_analysis_code_snapshots",
      { projectId: 7, sourceId: 12 },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      3,
      "capture_knowledge_analysis_git_snapshot",
      {
        projectId: 7,
        input: { sourceId: 12, gitRef: "v1.2.0", releaseId: 22 },
      },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      4,
      "analyze_knowledge_analysis_snapshot",
      { projectId: 7, snapshotId: 31 },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      5,
      "generate_knowledge_analysis_documents",
      { projectId: 7, input: { snapshotId: 31 } },
    );
    expect(knowledgeApi.upsertCodeSource).toHaveBeenCalledWith(
      expect.objectContaining({
        source: expect.objectContaining({ projectId: 7 }),
      }),
    );
  });

  it("浏览器开发验收通过等价 Dev API 生成并确认 AI 草稿", async () => {
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue([]);

    await knowledgeAnalysisApi.listCodeSources(9);
    await knowledgeAnalysisApi.createAiDraft({
      projectId: 9,
      projectVersionId: 18,
      snapshotIds: [27],
      providerKey: "local-chat",
    });
    await knowledgeAnalysisApi.confirmAiDraft({
      draftId: 38,
      title: "订单实现分析",
      content: "正文 [code:27:file:12]",
      versionLabel: "AI 分析",
    });
    expect(devApiFetch).toHaveBeenCalledWith(
      "/knowledge/projects/9/analysis/code-sources",
    );
    expect(devApiFetch).toHaveBeenCalledWith(
      "/knowledge/projects/9/analysis/ai-drafts",
      expect.objectContaining({ method: "POST" }),
    );
    expect(devApiFetch).toHaveBeenCalledWith(
      "/knowledge/analysis/ai-drafts/38/confirm",
      expect.objectContaining({ method: "POST" }),
    );
  });
});
