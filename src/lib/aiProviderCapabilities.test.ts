import { describe, expect, it } from "vitest";

import {
  applyAiProviderCapabilityMode,
  getAiProviderCapabilityMode,
  providerModeSupportsChat,
  providerModeSupportsEmbedding,
} from "@/lib/aiProviderCapabilities";

describe("AI Provider 能力模式", () => {
  it("兼容旧聊天 Provider 并识别 Embedding-only", () => {
    expect(
      getAiProviderCapabilityMode({
        capabilities: ["streaming"],
        defaultModel: "chat-model",
        embeddingModel: "",
      }),
    ).toBe("chat");
    expect(
      getAiProviderCapabilityMode({
        capabilities: ["embedding"],
        defaultModel: "",
        embeddingModel: "multilingual-e5-small-int8",
      }),
    ).toBe("embedding");
    expect(
      getAiProviderCapabilityMode({
        capabilities: ["streaming"],
        defaultModel: "legacy-chat-model",
      }),
    ).toBe("chat");
  });

  it("切换能力时保留无关能力并同步聊天和向量能力", () => {
    expect(
      applyAiProviderCapabilityMode(
        ["streaming", "chat", "embedding"],
        "embedding",
      ),
    ).toEqual(["streaming", "embedding"]);
    expect(
      applyAiProviderCapabilityMode(["streaming"], "chat_and_embedding"),
    ).toEqual(["streaming", "chat", "embedding"]);
  });

  it("按模式决定表单字段", () => {
    expect(providerModeSupportsChat("embedding")).toBe(false);
    expect(providerModeSupportsChat("chat_and_embedding")).toBe(true);
    expect(providerModeSupportsEmbedding("chat")).toBe(false);
    expect(providerModeSupportsEmbedding("chat_and_embedding")).toBe(true);
  });
});
