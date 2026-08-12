import type { KnowledgeProject, UpsertKnowledgeProjectInput } from "@/types";
import type { GitWorkspace } from "@/types/gitWorkspace";

/**
 * 生成仅供系统使用的项目键。普通用户只需要填写项目名称，避免把内部标识带入主流程。
 */
export function createProjectKey(name: string) {
  const normalized = name
    .trim()
    .toLowerCase()
    // 后端键只接受 ASCII；项目名称仍完整保留中文，内部键由系统自动生成。
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 36);
  return `${normalized || "project"}-${Date.now().toString(36)}`;
}

export function projectInput(
  project: Pick<
    KnowledgeProject,
    | "id"
    | "projectKey"
    | "name"
    | "aliases"
    | "description"
    | "gitWorkspaceKeys"
    | "gitWorkspaceKey"
    | "defaultBranch"
    | "enabled"
  >,
  changes: Pick<
    UpsertKnowledgeProjectInput,
    "name" | "description" | "enabled"
  >,
): UpsertKnowledgeProjectInput {
  return {
    id: project.id,
    projectKey: project.projectKey,
    name: changes.name.trim(),
    aliases: project.aliases,
    description: changes.description.trim(),
    gitWorkspaceKeys: project.gitWorkspaceKeys,
    gitWorkspaceKey: project.gitWorkspaceKey,
    defaultBranch: project.defaultBranch,
    enabled: changes.enabled,
  };
}

export function workspaceDefaultBranch(workspace: GitWorkspace) {
  return workspace.branch.trim() || "main";
}
