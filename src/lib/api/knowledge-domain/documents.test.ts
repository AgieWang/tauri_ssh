import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, devApiFetch, hasTauriRuntime } = vi.hoisted(() => ({
  invoke: vi.fn(),
  devApiFetch: vi.fn(),
  hasTauriRuntime: vi.fn(),
}));

vi.mock("../client", () => ({
  invoke,
  devApiFetch,
  hasTauriRuntime,
}));

import { knowledgeDocumentsApi } from "./documents";

describe("knowledgeDocumentsApi", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("在桌面端以历史版本和修订号创建恢复草稿", async () => {
    hasTauriRuntime.mockReturnValue(true);
    invoke.mockResolvedValue({ conflict: false });
    const input = {
      sourceVersionId: 7,
      draftId: 12,
      revision: 3,
      editorLabel: "本地用户",
    };

    await knowledgeDocumentsApi.restoreVersionToDraft(input);

    expect(invoke).toHaveBeenCalledWith(
      "restore_knowledge_document_version_to_draft",
      { input },
    );
    expect(devApiFetch).not.toHaveBeenCalled();
  });

  it("在浏览器验收环境调用相同恢复语义的开发接口", async () => {
    hasTauriRuntime.mockReturnValue(false);
    devApiFetch.mockResolvedValue({ conflict: true });
    const input = { sourceVersionId: 7 };

    await knowledgeDocumentsApi.restoreVersionToDraft(input);

    expect(devApiFetch).toHaveBeenCalledWith(
      "/knowledge/document-versions/restore-draft",
      { method: "POST", body: JSON.stringify(input) },
    );
    expect(invoke).not.toHaveBeenCalled();
  });
});
