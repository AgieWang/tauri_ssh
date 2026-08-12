import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { knowledgeApi } from "../knowledge";
import type { KnowledgeAskResult } from "@/types";
import type {
  KnowledgeQaSession,
  KnowledgeQaSessionDetail,
  KnowledgeScopedQuestionInput,
  PersistKnowledgeQaRoundInput,
} from "@/types/knowledge-domain/qa";
import { devApiFetch, hasTauriRuntime, invoke } from "../client";

export type KnowledgeQaSaveResult = "saved" | "downloaded" | "cancelled";

export const knowledgeQaApi = {
  previewContext: knowledgeApi.previewRagContext,
  ask: knowledgeApi.ask,
  runEvaluation: knowledgeApi.runFixedRetrievalEvaluation,
  askScopedQuestion: (input: KnowledgeScopedQuestionInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeAskResult>("ask_knowledge_scoped_question", { input })
      : devApiFetch<KnowledgeAskResult>(
          `/knowledge/projects/${input.projectId}/qa/ask`,
          { method: "POST", body: JSON.stringify(input) },
        ),
  listSessions: (projectId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeQaSession[]>("list_knowledge_qa_sessions", {
          projectId,
        })
      : devApiFetch<KnowledgeQaSession[]>(
          `/knowledge/projects/${projectId}/qa/sessions`,
        ),
  getSession: (projectId: number, sessionId: number) =>
    hasTauriRuntime()
      ? invoke<KnowledgeQaSessionDetail>("get_knowledge_qa_session", {
          projectId,
          sessionId,
        })
      : devApiFetch<KnowledgeQaSessionDetail>(
          `/knowledge/projects/${projectId}/qa/sessions/${sessionId}`,
        ),
  persistRound: (input: PersistKnowledgeQaRoundInput) =>
    hasTauriRuntime()
      ? invoke<KnowledgeQaSessionDetail>("persist_knowledge_qa_round", {
          input,
        })
      : devApiFetch<KnowledgeQaSessionDetail>(
          `/knowledge/projects/${input.projectId}/qa/rounds`,
          { method: "POST", body: JSON.stringify(input) },
        ),
  deleteSession: (projectId: number, sessionId: number) =>
    hasTauriRuntime()
      ? invoke<void>("delete_knowledge_qa_session", {
          projectId,
          sessionId,
        })
      : devApiFetch<void>(
          `/knowledge/projects/${projectId}/qa/sessions/${sessionId}`,
          { method: "DELETE" },
        ),
  saveMarkdown: async (input: {
    content: string;
    defaultFileName: string;
  }): Promise<KnowledgeQaSaveResult> => {
    const safeFileName = input.defaultFileName.replace(/[\\/:*?"<>|]/g, "_");
    if (hasTauriRuntime()) {
      const path = await saveFileDialog({
        defaultPath: safeFileName.endsWith(".md")
          ? safeFileName
          : `${safeFileName}.md`,
        filters: [{ name: "Markdown", extensions: ["md", "markdown"] }],
      });
      if (!path) return "cancelled";
      await invoke<string>("save_knowledge_qa_markdown", {
        input: { path, content: input.content },
      });
      return "saved";
    }

    // 浏览器 Dev API 没有本地文件插件时仍提供手动下载，便于验收和导出草稿。
    const blob = new Blob([input.content], {
      type: "text/markdown;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = safeFileName.endsWith(".md")
      ? safeFileName
      : `${safeFileName}.md`;
    link.rel = "noopener";
    link.style.display = "none";
    document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 0);
    return "downloaded";
  },
};
