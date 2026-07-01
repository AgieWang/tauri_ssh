import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  GitWorkspace,
  GitWorkspaceBranch,
  GitWorkspaceDetail,
  GitWorkspaceScanJobStatus,
  GitWorkspaceScanStartResult,
  AiCommitGitWorkspaceInput,
  AiCommitGitWorkspaceResult,
  ListGitWorkspacesInput,
  MergeGitWorkspaceBranchInput,
  ScanGitWorkspaceRootInput,
  ScanGitWorkspaceRootResult,
  SwitchGitWorkspaceBranchInput,
  UpsertGitWorkspaceInput,
} from "@/types";

function requireTauriRuntime(): never {
  throw new Error("Git 工作区需要读取本地仓库目录，请在 Tauri 桌面端使用该功能。");
}

export const gitWorkspaceApi = {
  list: (input?: ListGitWorkspacesInput) =>
    hasTauriRuntime()
      ? invoke<GitWorkspace[]>("list_git_workspaces", { input })
      : devApiFetch<GitWorkspace[]>("/git-workspaces/list", {
          method: "POST",
          body: JSON.stringify(input ?? null),
        }).catch(() => []),
  upsert: (input: UpsertGitWorkspaceInput) =>
    hasTauriRuntime()
      ? invoke<GitWorkspace>("upsert_git_workspace", { input })
      : Promise.resolve(requireTauriRuntime()),
  delete: (workspaceKey: string) =>
    hasTauriRuntime()
      ? invoke<void>("delete_git_workspace", { workspaceKey })
      : Promise.resolve(requireTauriRuntime()),
  refresh: (workspaceKey: string) =>
    hasTauriRuntime()
      ? invoke<GitWorkspace>("refresh_git_workspace", { workspaceKey })
      : Promise.resolve(requireTauriRuntime()),
  detail: (workspaceKey: string) =>
    hasTauriRuntime()
      ? invoke<GitWorkspaceDetail>("get_git_workspace_detail", { workspaceKey })
      : Promise.resolve(requireTauriRuntime()),
  scanRoot: (input: ScanGitWorkspaceRootInput) =>
    hasTauriRuntime()
      ? invoke<ScanGitWorkspaceRootResult>("scan_git_workspace_root", { input })
      : Promise.resolve(requireTauriRuntime()),
  startScanRoot: (input: ScanGitWorkspaceRootInput) =>
    hasTauriRuntime()
      ? invoke<GitWorkspaceScanStartResult>("start_git_workspace_root_scan", { input })
      : Promise.resolve(requireTauriRuntime()),
  getScanStatus: (jobId: string) =>
    hasTauriRuntime()
      ? invoke<GitWorkspaceScanJobStatus>("get_git_workspace_scan_status", { jobId })
      : Promise.resolve(requireTauriRuntime()),
  aiCommit: (input: AiCommitGitWorkspaceInput) =>
    hasTauriRuntime()
      ? invoke<AiCommitGitWorkspaceResult>("ai_commit_git_workspace", { input })
      : Promise.resolve(requireTauriRuntime()),
  pull: (workspaceKey: string) =>
    hasTauriRuntime()
      ? invoke<GitWorkspace>("pull_git_workspace", { workspaceKey })
      : Promise.resolve(requireTauriRuntime()),
  push: (workspaceKey: string) =>
    hasTauriRuntime()
      ? invoke<GitWorkspace>("push_git_workspace", { workspaceKey })
      : Promise.resolve(requireTauriRuntime()),
  branches: (workspaceKey: string) =>
    hasTauriRuntime()
      ? invoke<GitWorkspaceBranch[]>("list_git_workspace_branches", { workspaceKey })
      : Promise.resolve(requireTauriRuntime()),
  switchBranch: (input: SwitchGitWorkspaceBranchInput) =>
    hasTauriRuntime()
      ? invoke<GitWorkspace>("switch_git_workspace_branch", { input })
      : Promise.resolve(requireTauriRuntime()),
  mergeBranch: (input: MergeGitWorkspaceBranchInput) =>
    hasTauriRuntime()
      ? invoke<GitWorkspace>("merge_git_workspace_branch", { input })
      : Promise.resolve(requireTauriRuntime()),
};
