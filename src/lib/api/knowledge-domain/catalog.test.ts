import { beforeEach, describe, expect, it, vi } from "vitest";

const { devApiFetch, hasTauriRuntime, invoke } = vi.hoisted(() => ({
  devApiFetch: vi.fn(),
  hasTauriRuntime: vi.fn(),
  invoke: vi.fn(),
}));

vi.mock("../client", () => ({ devApiFetch, hasTauriRuntime, invoke }));
vi.mock("../knowledge", () => ({
  knowledgeApi: {
    listProjects: vi.fn(),
    upsertProject: vi.fn(),
    deleteProject: vi.fn(),
    listReleases: vi.fn(),
    upsertRelease: vi.fn(),
    deleteRelease: vi.fn(),
  },
}));

import { knowledgeCatalogApi } from "./catalog";

describe("knowledgeCatalogApi", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("在 Tauri 中使用注册的仓库关联 Command", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue([]);

    await knowledgeCatalogApi.listRepositoryBindings(7);
    await knowledgeCatalogApi.replaceRepositoryBindings({
      projectId: 7,
      repositories: [{ workspaceKey: "orders-api", versionStrategy: "tag" }],
    });
    await knowledgeCatalogApi.unlinkRepositoryBinding(13);
    await knowledgeCatalogApi.createProjectVersionManifest({
      projectId: 7,
      version: "v1.0.0",
      repositories: [
        { repositoryBindingId: 13, refType: "tag", refName: "v1.0.0" },
      ],
    });
    await knowledgeCatalogApi.getProjectVersionManifest(21);
    await knowledgeCatalogApi.getProjectVersionCompleteness(21);
    await knowledgeCatalogApi.startProjectVersionBackfill({ releaseId: 21 });

    expect(invoke).toHaveBeenNthCalledWith(
      1,
      "list_knowledge_project_repository_bindings",
      { projectId: 7 },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "replace_knowledge_project_repository_bindings",
      {
        input: {
          projectId: 7,
          repositories: [
            { workspaceKey: "orders-api", versionStrategy: "tag" },
          ],
        },
      },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      5,
      "get_knowledge_project_version_manifest",
      { releaseId: 21 },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      6,
      "get_knowledge_project_version_completeness",
      { releaseId: 21 },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      7,
      "start_knowledge_project_version_backfill",
      { input: { releaseId: 21 } },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      3,
      "unlink_knowledge_project_repository_binding",
      { repositoryBindingId: 13 },
    );
    expect(invoke).toHaveBeenNthCalledWith(
      4,
      "create_knowledge_project_version_manifest",
      {
        input: {
          projectId: 7,
          version: "v1.0.0",
          repositories: [
            { repositoryBindingId: 13, refType: "tag", refName: "v1.0.0" },
          ],
        },
      },
    );
  });

  it("在浏览器开发验收时调用同名的回环 API", async () => {
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue([]);

    await knowledgeCatalogApi.replaceRepositoryBindings({
      projectId: 9,
      repositories: [{ workspaceKey: "billing-api" }],
    });

    expect(devApiFetch).toHaveBeenCalledWith(
      "/knowledge/projects/9/repository-bindings",
      {
        method: "POST",
        body: JSON.stringify({
          projectId: 9,
          repositories: [{ workspaceKey: "billing-api" }],
        }),
      },
    );

    await knowledgeCatalogApi.inspectRepositoryBinding(18);
    expect(devApiFetch).toHaveBeenLastCalledWith(
      "/knowledge/repository-bindings/18/inspect",
      { method: "POST" },
    );

    await knowledgeCatalogApi.createProjectVersionManifest({
      projectId: 9,
      version: "v2.0.0",
      repositories: [
        { repositoryBindingId: 18, refType: "branch", refName: "main" },
      ],
    });
    expect(devApiFetch).toHaveBeenLastCalledWith(
      "/knowledge/projects/9/version-manifests",
      {
        method: "POST",
        body: JSON.stringify({
          projectId: 9,
          version: "v2.0.0",
          repositories: [
            { repositoryBindingId: 18, refType: "branch", refName: "main" },
          ],
        }),
      },
    );

    await knowledgeCatalogApi.getProjectVersionManifest(23);
    expect(devApiFetch).toHaveBeenLastCalledWith(
      "/knowledge/version-manifests/23",
    );

    await knowledgeCatalogApi.getProjectVersionCompleteness(23);
    expect(devApiFetch).toHaveBeenLastCalledWith(
      "/knowledge/version-manifests/23/completeness",
    );

    await knowledgeCatalogApi.startProjectVersionBackfill({ releaseId: 23 });
    expect(devApiFetch).toHaveBeenLastCalledWith(
      "/knowledge/version-manifests/23/backfill",
      { method: "POST" },
    );
  });
});
