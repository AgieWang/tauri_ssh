export interface TerminalCommandInput {
  serverAlias: string;
  command: string;
  timeoutSecs?: number | null;
}

export interface TerminalCommandResult {
  serverAlias: string;
  command: string;
  exitStatus: number;
  stdout: string;
  stderr: string;
  durationMs: number;
  blocked: boolean;
  message: string;
}

export interface TerminalSessionStartInput {
  serverAlias: string;
  cols?: number | null;
  rows?: number | null;
}

export interface TerminalSessionStartResult {
  sessionId: string;
}

export interface TerminalSessionWriteInput {
  sessionId: string;
  data: string;
}

export interface TerminalSessionResizeInput {
  sessionId: string;
  cols: number;
  rows: number;
}

export interface TerminalSessionCloseInput {
  sessionId: string;
}

export interface TerminalSessionEvent {
  sessionId: string;
  kind: "data" | "status" | "error" | "exit";
  data?: string | null;
  message?: string | null;
}
