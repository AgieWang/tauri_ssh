import { devApiBaseUrl, devApiFetch, hasTauriRuntime, invoke } from "./client";
import type {
  TerminalCommandInput,
  TerminalCommandResult,
  TerminalSessionCloseInput,
  TerminalSessionResizeInput,
  TerminalSessionStartInput,
  TerminalSessionStartResult,
  TerminalSessionWriteInput,
} from "@/types";

const DEV_TERMINAL_WS_BASE_URL = `${devApiBaseUrl.replace(/^http/, "ws")}/terminal/ws`;

export const terminalApi = {
  execute: (input: TerminalCommandInput) =>
    hasTauriRuntime()
      ? invoke<TerminalCommandResult>("execute_terminal_command", { input })
      : devApiFetch<TerminalCommandResult>("/terminal/execute", {
          method: "POST",
          body: JSON.stringify(input),
        }),
  startSession: (input: TerminalSessionStartInput) =>
    invoke<TerminalSessionStartResult>("start_terminal_session", { input }),
  writeSession: (input: TerminalSessionWriteInput) =>
    invoke<void>("write_terminal_session", { input }),
  resizeSession: (input: TerminalSessionResizeInput) =>
    invoke<void>("resize_terminal_session", { input }),
  closeSession: (input: TerminalSessionCloseInput) =>
    invoke<void>("close_terminal_session", { input }),
  devWebSocketUrl: (input: TerminalSessionStartInput) => {
    const params = new URLSearchParams({
      serverAlias: input.serverAlias,
      cols: String(input.cols ?? 100),
      rows: String(input.rows ?? 30),
    });
    return `${DEV_TERMINAL_WS_BASE_URL}?${params.toString()}`;
  },
};
