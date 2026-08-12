import { devApiFetch, hasTauriRuntime, invoke } from "../client";
import { knowledgeApi } from "../knowledge";
import type {
  KnowledgeProjectVersionManifestInput,
  KnowledgeProjectVersionCompleteness,
  KnowledgeProjectVersionManifestResult,
  KnowledgeRepositoryBinding,
  KnowledgeRepositoryBindingInput,
  KnowledgeRepositoryAvailability,
} from "@/types/knowledge-domain/catalog";

/** 目录领域先收敛旧项目/发布 API；仓库关联的新 Command 在目录 Service 就绪后接入。 */
export const knowledgeCatalogApi = {
  listProjects: knowledgeApi.listProjects,
  upsertProject: knowledgeApi.upsertProject,
  deleteProject: knowledgeApi.deleteProject,
  listReleases: knowledgeApi.listReleases,
  upsertRelease: knowledgeApi.upsertRelease,
  deleteRelease: knowledgeApi.deleteRelease,
  listRepositoryBindings: (projectId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeRepositoryBinding[]>(
          "list_knowledge_project_repository_bindings",
          { projectId },
        )
      : devApiFetch<KnowledgeRepositoryBinding[]>(
          `/knowledge/projects/${projectId}/repository-bindings`,
        ),
  replaceRepositoryBindings: (input: KnowledgeRepositoryBindingInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeRepositoryBinding[]>(
          "replace_knowledge_project_repository_bindings",
          { input },
        )
      : devApiFetch<KnowledgeRepositoryBinding[]>(
          `/knowledge/projects/${input.projectId}/repository-bindings`,
          { method: "POST", body: JSON.stringify(input) },
        ),
  inspectRepositoryBinding: (repositoryBindingId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeRepositoryAvailability>(
          "inspect_knowledge_project_repository_binding",
          { repositoryBindingId },
        )
      : devApiFetch<KnowledgeRepositoryAvailability>(
          `/knowledge/repository-bindings/${repositoryBindingId}/inspect`,
          { method: "POST" },
        ),
  unlinkRepositoryBinding: (repositoryBindingId: number) =>
    hasTauriRuntime()
      ? invoke<void>("unlink_knowledge_project_repository_binding", {
          repositoryBindingId,
        })
      : devApiFetch<void>(
          `/knowledge/repository-bindings/${repositoryBindingId}`,
          { method: "DELETE" },
        ),
  createProjectVersionManifest: (
    input: KnowledgeProjectVersionManifestInput,
  ) =>
    hasTauriRuntime()
      ? invoke<KnowledgeProjectVersionManifestResult>(
          "create_knowledge_project_version_manifest",
          { input },
        )
      : devApiFetch<KnowledgeProjectVersionManifestResult>(
          `/knowledge/projects/${input.projectId}/version-manifests`,
          { method: "POST", body: JSON.stringify(input) },
        ),
  getProjectVersionManifest: (releaseId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeProjectVersionManifestResult>(
          "get_knowledge_project_version_manifest",
          { releaseId },
        )
      : devApiFetch<KnowledgeProjectVersionManifestResult>(
          `/knowledge/version-manifests/${releaseId}`,
        ),
  getProjectVersionCompleteness: (releaseId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeProjectVersionCompleteness>(
          "get_knowledge_project_version_completeness",
          { releaseId },
        )
      : devApiFetch<KnowledgeProjectVersionCompleteness>(
          `/knowledge/version-manifests/${releaseId}/completeness`,
        ),
};
