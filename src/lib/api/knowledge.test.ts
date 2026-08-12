import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, devApiFetch, hasTauriRuntime } = vi.hoisted(() => ({
  invoke: vi.fn(),
  devApiFetch: vi.fn(),
  hasTauriRuntime: vi.fn(),
}));

vi.mock("./client", () => ({
  invoke,
  devApiFetch,
  hasTauriRuntime,
}));

import { knowledgeApi } from "./knowledge";

describe("knowledgeApi", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("在桌面和浏览器验收环境均可显式重建全文索引", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue(18);
    await knowledgeApi.rebuildFts();
    expect(invoke).toHaveBeenCalledWith("rebuild_knowledge_fts");

    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue(18);
    await knowledgeApi.rebuildFts();
    expect(devApiFetch).toHaveBeenCalledWith("/knowledge/search/fts/rebuild", {
      method: "POST",
    });
  });

  it("在桌面和浏览器验收环境使用同一全文搜索契约", async () => {
    const input = {
      query: "订单创建",
      projectIds: [11],
      releaseIds: [21],
      sourceIds: [],
      documentTypes: [],
      sensitivities: [],
      limit: 30,
      includeContext: true,
    };

    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue([]);
    await knowledgeApi.searchFts(input);
    expect(invoke).toHaveBeenCalledWith("search_knowledge_fts", { input });

    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue([]);
    await knowledgeApi.searchFts(input);
    expect(devApiFetch).toHaveBeenCalledWith("/knowledge/search/fts", {
      method: "POST",
      body: JSON.stringify(input),
    });
  });

  it("在桌面运行时时调用禅道 AI 摘要 Command", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({ documentVersionId: 7, citationCount: 2 });

    await knowledgeApi.generateZentaoAiSummary({
      mappingId: 1,
      providerKey: "provider-a",
      model: "model-a",
      prompt: "总结风险",
    });

    expect(invoke).toHaveBeenCalledWith("generate_zentao_ai_summary", {
      input: {
        mappingId: 1,
        providerKey: "provider-a",
        model: "model-a",
        prompt: "总结风险",
      },
    });
    expect(devApiFetch).not.toHaveBeenCalled();
  });

  it("在浏览器验收环境走同一摘要 Dev API", async () => {
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue({ documentVersionId: 8, citationCount: 1 });

    await knowledgeApi.generateZentaoAiSummary({
      mappingId: 2,
      providerKey: "provider-b",
      model: "model-b",
      prompt: "总结实现",
    });

    expect(devApiFetch).toHaveBeenCalledWith(
      "/knowledge/zentao/ai-summary/generate",
      {
        method: "POST",
        body: JSON.stringify({
          mappingId: 2,
          providerKey: "provider-b",
          model: "model-b",
          prompt: "总结实现",
        }),
      },
    );
    expect(invoke).not.toHaveBeenCalled();
  });

  it("保持旧项目和版本 API 的 Command、参数与浏览器回环外观", async () => {
    const projectInput = {
      projectKey: "legacy-catalog",
      name: "兼容项目",
      aliases: [],
      description: "",
      gitWorkspaceKey: "",
      gitWorkspaceKeys: [],
      defaultBranch: "main",
      enabled: true,
    };
    const releaseInput = {
      projectId: 9,
      version: "v1.0.0",
      tagName: "v1.0.0",
      branch: "main",
      commitSha: "",
      description: "兼容版本",
      releasedAt: null,
    };
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({});
    await knowledgeApi.listProjects({ keyword: "兼容" });
    await knowledgeApi.upsertProject(projectInput);
    await knowledgeApi.listReleases(9);
    await knowledgeApi.upsertRelease(releaseInput);
    expect(invoke).toHaveBeenNthCalledWith(1, "list_knowledge_projects", {
      input: { keyword: "兼容" },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "upsert_knowledge_project", {
      input: projectInput,
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "list_knowledge_releases", {
      projectId: 9,
    });
    expect(invoke).toHaveBeenNthCalledWith(4, "upsert_knowledge_release", {
      input: releaseInput,
    });

    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue({});
    await knowledgeApi.upsertProject(projectInput);
    await knowledgeApi.listReleases(9);
    await knowledgeApi.upsertRelease(releaseInput);
    expect(devApiFetch).toHaveBeenNthCalledWith(1, "/knowledge/projects", {
      method: "POST",
      body: JSON.stringify(projectInput),
    });
    expect(devApiFetch).toHaveBeenNthCalledWith(
      2,
      "/knowledge/projects/9/releases",
    );
    expect(devApiFetch).toHaveBeenNthCalledWith(3, "/knowledge/releases", {
      method: "POST",
      body: JSON.stringify(releaseInput),
    });
  });

  it("在桌面运行时将蓝绿重建与激活操作映射到受控 Command", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({});

    await knowledgeApi.beginEmbeddingProfileRebuild(7);
    await knowledgeApi.completeEmbeddingProfileRebuild(7);
    await knowledgeApi.activateEmbeddingProfileRebuild(7);
    await knowledgeApi.rollbackEmbeddingProfileRebuild(3);

    expect(invoke).toHaveBeenNthCalledWith(
      1,
      "begin_knowledge_embedding_profile_rebuild",
      {
        profileId: 7,
      },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "complete_knowledge_embedding_profile_rebuild",
      {
        profileId: 7,
      },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      3,
      "activate_knowledge_embedding_profile_rebuild",
      {
        profileId: 7,
      },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      4,
      "rollback_knowledge_embedding_profile_rebuild",
      {
        previousProfileId: 3,
      },
    );
  });

  it("在浏览器验收环境保持源码比较和影响分析的 Dev API 对等入口", async () => {
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue({});

    await knowledgeApi.compareCodeSnapshots({
      fromSnapshotId: 4,
      toSnapshotId: 8,
    });
    await knowledgeApi.analyzeCodeImpact({
      snapshotId: 8,
      symbolKeys: ["src/lib.rs:OrderService::submit:12"],
      maxDepth: 2,
    });

    expect(devApiFetch).toHaveBeenNthCalledWith(
      1,
      "/knowledge/code-snapshots/compare",
      {
        method: "POST",
        body: JSON.stringify({ fromSnapshotId: 4, toSnapshotId: 8 }),
      },
    );
    expect(devApiFetch).toHaveBeenNthCalledWith(
      2,
      "/knowledge/code-snapshots/impact",
      {
        method: "POST",
        body: JSON.stringify({
          snapshotId: 8,
          symbolKeys: ["src/lib.rs:OrderService::submit:12"],
          maxDepth: 2,
        }),
      },
    );
    expect(invoke).not.toHaveBeenCalled();
  });

  it("按运行环境分派远程 Embedding 探测和批次构建", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({});

    await knowledgeApi.testRemoteEmbeddingProfile(6);
    await knowledgeApi.buildRemoteEmbeddingBatch({
      profileId: 6,
      batchSize: 8,
    });

    expect(invoke).toHaveBeenNthCalledWith(
      1,
      "test_knowledge_remote_embedding_profile",
      { profileId: 6 },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "build_knowledge_remote_embedding_batch",
      { input: { profileId: 6, batchSize: 8 } },
    );

    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue({});
    await knowledgeApi.testRemoteEmbeddingProfile(6);
    await knowledgeApi.buildRemoteEmbeddingBatch({ profileId: 6 });
    expect(devApiFetch).toHaveBeenNthCalledWith(
      1,
      "/knowledge/embedding/profiles/6/test-remote",
      { method: "POST" },
    );
    expect(devApiFetch).toHaveBeenNthCalledWith(
      2,
      "/knowledge/embedding/remote-batch",
      { method: "POST", body: JSON.stringify({ profileId: 6 }) },
    );
  });

  it("在两个运行时读取始终开启的远程向量化能力", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue(true);

    await knowledgeApi.getRemoteEmbeddingEnabled();
    expect(invoke).toHaveBeenNthCalledWith(
      1,
      "get_knowledge_remote_embedding_enabled",
    );

    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue(true);
    await knowledgeApi.getRemoteEmbeddingEnabled();
    expect(devApiFetch).toHaveBeenCalledWith(
      "/knowledge/embedding/remote-enabled",
    );
  });

  it("对等暴露已分析快照的只读文件树和代码正文", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({});

    await knowledgeApi.listCodeFiles(8);
    await knowledgeApi.getCodeFileContent(8, 12);

    expect(invoke).toHaveBeenNthCalledWith(1, "list_knowledge_code_files", {
      snapshotId: 8,
    });
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "get_knowledge_code_file_content",
      { snapshotId: 8, fileId: 12 },
    );

    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue({});
    await knowledgeApi.listCodeFiles(8);
    await knowledgeApi.getCodeFileContent(8, 12);
    expect(devApiFetch).toHaveBeenNthCalledWith(
      1,
      "/knowledge/code-snapshots/8/files",
    );
    expect(devApiFetch).toHaveBeenNthCalledWith(
      2,
      "/knowledge/code-snapshots/8/files/12/content",
    );
  });

  it("为失败任务在桌面与浏览器运行时分派同一重试语义", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({});
    await knowledgeApi.retryJob("sync-001");
    expect(invoke).toHaveBeenCalledWith("retry_knowledge_job", {
      jobKey: "sync-001",
    });

    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue({});
    await knowledgeApi.retryJob("sync-001");
    expect(devApiFetch).toHaveBeenCalledWith("/knowledge/jobs/sync-001/retry", {
      method: "POST",
    });
  });
});
