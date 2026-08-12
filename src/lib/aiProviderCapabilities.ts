import type { AiProvider, AiProviderCapabilityMode } from "@/types";

const CHAT_CAPABILITY = "chat";
const EMBEDDING_CAPABILITY = "embedding";

function hasCapability(capabilities: string[], expected: string) {
  return capabilities.some(
    (capability) => capability.trim().toLowerCase() === expected,
  );
}

export function getAiProviderCapabilityMode(
  provider: Pick<AiProvider, "capabilities" | "defaultModel"> &
    Partial<Pick<AiProvider, "embeddingModel">>,
): AiProviderCapabilityMode {
  const capabilities = provider.capabilities ?? [];
  const hasExplicitMode = capabilities.some((capability) =>
    [CHAT_CAPABILITY, EMBEDDING_CAPABILITY].includes(
      capability.trim().toLowerCase(),
    ),
  );
  const supportsChat = hasExplicitMode
    ? hasCapability(capabilities, CHAT_CAPABILITY)
    : Boolean(provider.defaultModel?.trim());
  const supportsEmbedding = hasExplicitMode
    ? hasCapability(capabilities, EMBEDDING_CAPABILITY)
    : Boolean(provider.embeddingModel?.trim());

  if (supportsChat && supportsEmbedding) return "chat_and_embedding";
  if (supportsEmbedding) return "embedding";
  return "chat";
}

export function applyAiProviderCapabilityMode(
  capabilities: string[],
  mode: AiProviderCapabilityMode,
) {
  const retained = capabilities.filter(
    (capability) =>
      ![CHAT_CAPABILITY, EMBEDDING_CAPABILITY].includes(
        capability.trim().toLowerCase(),
      ),
  );
  if (mode !== "embedding") retained.push(CHAT_CAPABILITY);
  if (mode !== "chat") retained.push(EMBEDDING_CAPABILITY);
  return Array.from(new Set(retained));
}

export function providerModeSupportsChat(mode: AiProviderCapabilityMode) {
  return mode !== "embedding";
}

export function providerModeSupportsEmbedding(mode: AiProviderCapabilityMode) {
  return mode !== "chat";
}
