export interface McpServerStatus {
  serverName: string;
  streamableHttpUrl: string;
  stdioCommand: string;
  stdioArgs: string[];
  enabled: boolean;
  localOnly: boolean;
  httpReachable: boolean;
  platform: string;
  notes: string[];
}

export interface McpClientConfig {
  key: string;
  name: string;
  vendor: string;
  description: string;
  configPath: string;
  scope: string;
  transport: string;
  status: "configured" | "available" | "not_found" | string;
  installed: boolean;
  configured: boolean;
  lastConfiguredAt: string | null;
  notes: string[];
}

export interface McpToolPermission {
  tool: string;
  policy: string;
  audit: string;
}

export interface McpManualSnippet {
  title: string;
  transport: string;
  content: string;
}

export interface McpOverview {
  status: McpServerStatus;
  clients: McpClientConfig[];
  tools: McpToolPermission[];
  snippets: McpManualSnippet[];
}

export interface ConfigureMcpClientInput {
  clientKey: string;
  transport?: string | null;
}

export interface ConfigureMcpClientResult {
  client: McpClientConfig;
  configPath: string;
  backupPath: string | null;
  message: string;
  snippet: string;
}
