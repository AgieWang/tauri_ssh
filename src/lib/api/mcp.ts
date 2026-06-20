import { devApiFetch, hasTauriRuntime, invoke } from "./client";
import type { ConfigureMcpClientInput, ConfigureMcpClientResult, McpOverview } from "@/types";

export const mcpApi = {
  overview: () =>
    hasTauriRuntime()
      ? invoke<McpOverview>("get_mcp_overview")
      : devApiFetch<McpOverview>("/mcp/overview"),
  configureClient: (input: ConfigureMcpClientInput) =>
    hasTauriRuntime()
      ? invoke<ConfigureMcpClientResult>("configure_mcp_client", { input })
      : devApiFetch<ConfigureMcpClientResult>("/mcp/configure", {
          method: "POST",
          body: JSON.stringify(input),
        }),
};
