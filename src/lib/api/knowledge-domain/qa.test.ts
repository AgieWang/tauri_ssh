import { beforeEach, describe, expect, it, vi } from "vitest";

const saveFileDialog = vi.hoisted(() => vi.fn());
const invoke = vi.hoisted(() => vi.fn());
const hasTauriRuntime = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: saveFileDialog }));
vi.mock("../client", () => ({
  devApiFetch: vi.fn(),
  hasTauriRuntime,
  invoke,
}));
vi.mock("../knowledge", () => ({
  knowledgeApi: {
    previewRagContext: vi.fn(),
    ask: vi.fn(),
    runFixedRetrievalEvaluation: vi.fn(),
  },
}));

import { knowledgeQaApi } from "./qa";

describe("knowledgeQaApi", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    hasTauriRuntime.mockReturnValue(true);
    saveFileDialog.mockResolvedValue("/tmp/project-qa.md");
    invoke.mockResolvedValue("/tmp/project-qa.md");
  });

  it("通过保存对话框和 Tauri Command 写入 Markdown 文档", async () => {
    const result = await knowledgeQaApi.saveMarkdown({
      content: "# 问答\n\n## 引用证据",
      defaultFileName: "订单中心:v1.0.0-qa.md",
    });

    expect(result).toBe("saved");
    expect(saveFileDialog).toHaveBeenCalledWith({
      defaultPath: "订单中心_v1.0.0-qa.md",
      filters: [{ name: "Markdown", extensions: ["md", "markdown"] }],
    });
    expect(invoke).toHaveBeenCalledWith("save_knowledge_qa_markdown", {
      input: {
        path: "/tmp/project-qa.md",
        content: "# 问答\n\n## 引用证据",
      },
    });
  });

  it("用户取消保存时不调用写入 Command", async () => {
    saveFileDialog.mockResolvedValue(null);
    const result = await knowledgeQaApi.saveMarkdown({
      content: "# 问答",
      defaultFileName: "qa.md",
    });

    expect(result).toBe("cancelled");
    expect(invoke).not.toHaveBeenCalled();
  });
});
