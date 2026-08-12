export type AiProviderRegion = "global" | "china" | "gateway" | "local";
export type AiProviderCapabilityMode =
  "chat" | "embedding" | "chat_and_embedding";
export type AiProviderStatus =
  "configured" | "testing" | "unconfigured" | "reserved";

export interface AiProvider {
  key: string;
  name: string;
  region: AiProviderRegion;
  protocol: string;
  defaultModel: string;
  embeddingModel: string;
  status: AiProviderStatus;
  endpoint: string;
  authType: string;
  apiKeyMasked: string | null;
  hasApiKey: boolean;
  latencyMs: number | null;
  costLevel: "低" | "中" | "高" | "企业";
  capabilities: string[];
  models: string[];
  scenarioFit: string[];
  fallback: string;
  enabled: boolean;
  updatedAt: string;
}

export interface UpsertAiProviderInput {
  key: string;
  name: string;
  region: AiProviderRegion;
  protocol: string;
  defaultModel: string;
  embeddingModel?: string;
  status: AiProviderStatus;
  endpoint: string;
  authType: string;
  apiKey?: string | null;
  clearApiKey?: boolean;
  costLevel: "低" | "中" | "高" | "企业";
  capabilities: string[];
  models: string[];
  scenarioFit: string[];
  fallback: string;
  enabled: boolean;
}

export interface AiProviderModelListInput {
  key: string;
  protocol: string;
  endpoint: string;
  authType: string;
  apiKey?: string | null;
}

export interface AiProviderModelListResult {
  providerKey: string;
  models: string[];
  source: string;
}

export interface AiProviderRoute {
  scenario: string;
  primaryProviderKey: string;
  fallbackProviderKey: string;
  requirement: string;
  updatedAt: string;
}

export interface UpsertAiProviderRouteInput {
  scenario: string;
  primaryProviderKey: string;
  fallbackProviderKey: string;
  requirement: string;
}

export interface AiProviderTestResult {
  ok: boolean;
  providerKey: string;
  providerName: string;
  model: string;
  endpoint: string;
  latencyMs: number;
  statusCode: number | null;
  message: string;
}

export interface AiProviderAskInput {
  prompt: string;
  providerKey?: string | null;
  systemPrompt?: string | null;
  skillScope?: string | null;
  useSkillTrigger?: boolean | null;
}

export interface AiProviderAskResult {
  providerKey: string;
  providerName: string;
  model: string;
  answer: string;
  latencyMs: number;
}
