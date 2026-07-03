import { hasTauriRuntime, invoke } from "./client";
import type {
  CodeReviewBatchParseResult,
  CodeReviewTask,
  CreateCodeReviewBatchTasksInput,
  CreateCodeReviewTaskInput,
  ListCodeReviewTasksInput,
  ParseCodeReviewBatchInput,
  RunCodeReviewAiInput,
} from "@/types";

function requireTauriRuntime(): never {
  throw new Error("代码审核需要读取和操作本地 Git 工作区，请在 Tauri 桌面端使用该功能。");
}

export const codeReviewApi = {
  list: (input?: ListCodeReviewTasksInput) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask[]>("list_code_review_tasks", { input })
      : Promise.resolve([]),
  get: (taskKey: string) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask>("get_code_review_task", { taskKey })
      : Promise.resolve(requireTauriRuntime()),
  create: (input: CreateCodeReviewTaskInput) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask>("create_code_review_task", { input })
      : Promise.resolve(requireTauriRuntime()),
  createBatchTasks: (input: CreateCodeReviewBatchTasksInput) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask[]>("create_code_review_batch_tasks", { input })
      : Promise.resolve(requireTauriRuntime()),
  prepareDiff: (taskKey: string) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask>("prepare_code_review_diff", { taskKey })
      : Promise.resolve(requireTauriRuntime()),
  runAi: (input: RunCodeReviewAiInput) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask>("run_code_review_ai", { input })
      : Promise.resolve(requireTauriRuntime()),
  merge: (taskKey: string) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask>("merge_code_review_task", { taskKey })
      : Promise.resolve(requireTauriRuntime()),
  push: (taskKey: string) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask>("push_code_review_task", { taskKey })
      : Promise.resolve(requireTauriRuntime()),
  abortMerge: (taskKey: string) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask>("abort_code_review_merge", { taskKey })
      : Promise.resolve(requireTauriRuntime()),
  cancel: (taskKey: string) =>
    hasTauriRuntime()
      ? invoke<CodeReviewTask>("cancel_code_review_task", { taskKey })
      : Promise.resolve(requireTauriRuntime()),
  parseBatch: (input: ParseCodeReviewBatchInput) =>
    hasTauriRuntime()
      ? invoke<CodeReviewBatchParseResult>("parse_code_review_batch", { input })
      : Promise.resolve(requireTauriRuntime()),
};
